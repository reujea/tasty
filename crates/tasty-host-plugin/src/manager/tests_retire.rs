//! 별도 스레드에서 회수하고, 이전 프로세스의 회수가 끝난 뒤 재시작하는지 확인한다.
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::PluginManager;
use super::file_config::ConfigChange;
use crate::process::PluginProcess;
use crate::test_support::HomeEnvGuard;
use tasty_terminal::waker_factory::NoopWakerFactory;

const ID: &str = "com.example.test";

/// shutdown 요청을 못 받는 자식 — deadline 까지 살아 있다가 kill 된다.
fn slow_child() -> Child {
    #[cfg(unix)]
    {
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("test child spawn")
    }
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 > nul"])
            .spawn()
            .expect("test child spawn")
    }
}

fn installed(home: &HomeEnvGuard) -> PluginManager {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![tasty_plugin_manifest::PluginPackage {
        dir: home.path().join("plugin"),
        manifest: toml::from_str(
            r#"
manifest_version = 1
id = "com.example.test"
name = "Test"
version = "1.0.0"
api_version = "1"
[entry]
type = "process"
command = "unused"
"#,
        )
        .expect("fixture manifest"),
    }]);
    mgr
}

/// 회수가 끝날 때까지 pump 를 돌리고, 가장 오래 걸린 pump 한 번의 시간을 돌려준다.
fn pump_until_retired(mgr: &mut PluginManager) -> Duration {
    let give_up = Instant::now() + Duration::from_secs(10);
    let mut longest = Duration::ZERO;
    while mgr.retiring_count() > 0 {
        assert!(Instant::now() < give_up, "회수가 10 초 안에 안 끝났다");
        let t = Instant::now();
        mgr.pump(Instant::now());
        longest = longest.max(t.elapsed());
        std::thread::sleep(Duration::from_millis(20));
    }
    longest
}

/// disable 적용과 이후 pump는 별도 스레드의 회수를 기다리지 않아야 한다.
///
/// 제품에서 설정 파일 확정은 writer worker가 하고 메인 루프는 그 결과만 적용한다. 그래서
/// 확정을 시작 시각 앞에 둔다. 확정은 fsync 때문에 다른 프로세스의 디스크 사용에 좌우된다.
/// 벽시계 상한은 회수를 일부만 기다리는 회귀를, 상태 단정은 회수를 끝내고 돌아오는 회귀를 잡는다.
#[test]
fn disable_returns_before_a_stalled_child_exits() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    mgr.processes
        .insert(ID.into(), PluginProcess::stub_with_child(ID, slow_child()));
    let change = ConfigChange::Enabled {
        id: ID.into(),
        enabled: false,
    };
    let config = mgr
        .prepare_config_change(&change)
        .expect("prepare")
        .execute()
        .expect("config receipt");

    let t = Instant::now();
    mgr.apply_config_change(config, &change).expect("apply");
    let took = t.elapsed();
    assert!(
        took < Duration::from_millis(500),
        "disable 적용이 회수를 기다렸다: {took:?}"
    );
    assert_eq!(
        mgr.retiring_finished(ID),
        Some(false),
        "disable 적용이 회수를 끝내고 돌아왔다"
    );
    assert_eq!(mgr.retiring_count(), 1, "회수 항목이 하나 남아 있어야 한다");
    assert_eq!(mgr.retiring_respawn(ID), Some(false));

    let longest = pump_until_retired(&mut mgr);
    assert!(
        longest < Duration::from_millis(500),
        "회수 완료를 확인하는 pump가 오래 걸렸다: {longest:?}"
    );
    assert!(
        t.elapsed() >= Duration::from_millis(1500),
        "shutdown 을 못 받는 자식은 deadline(2 초)까지 기회를 받아야 한다: {:?}",
        t.elapsed()
    );
    assert!(!mgr.is_running(ID));
}

/// 재시작으로 회수 중에 온 disable 은 회수 뒤의 재기동 예약을 거둔다.
#[test]
fn a_disable_during_a_restart_cancels_the_restart() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    let proc = PluginProcess::stub_with_child(ID, slow_child());
    proc.backdate_pong_for_test(super::HEALTHCHECK_TIMEOUT + Duration::from_secs(1));
    mgr.processes.insert(ID.into(), proc);
    mgr.restart_unresponsive_plugins();
    assert_eq!(mgr.retiring_respawn(ID), Some(true));

    mgr.disable(ID).expect("disable");
    assert_eq!(mgr.retiring_respawn(ID), Some(false));
    pump_until_retired(&mut mgr);
    assert!(!mgr.is_running(ID), "disable 로 거둔 예약이 기동됐다");
}

/// 명시적 enable은 이전 프로세스의 회수를 기다린 뒤 새 프로세스를 실행한다.
#[cfg(unix)]
#[test]
fn an_explicit_enable_during_retirement_waits_and_starts_in_place() {
    use crate::test_fake_plugin as fake;

    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    fake::write(&dir);
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![fake::package(&dir)]);
    mgr.processes
        .insert(ID.into(), PluginProcess::stub_with_child(ID, slow_child()));
    mgr.disable(ID).expect("disable");
    assert_eq!(mgr.retiring_count(), 1);

    let t = Instant::now();
    mgr.enable(ID).expect("enable");
    assert!(
        mgr.is_running(ID),
        "enable 이 돌아왔는데 plugin 이 떠 있지 않다"
    );
    assert_eq!(mgr.retiring_count(), 0, "옛 프로세스를 두고 새 것이 떴다");
    assert!(
        t.elapsed() >= Duration::from_millis(1500),
        "shutdown 을 못 받는 옛 프로세스를 기다리지 않았다: {:?}",
        t.elapsed()
    );

    mgr.begin_shutdown_all();
    let give_up = Instant::now() + Duration::from_secs(10);
    while !mgr.poll_shutdown_all() {
        assert!(Instant::now() < give_up, "종료가 10 초 안에 안 끝났다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 회수 중의 기동 요청은 이전 프로세스와 겹치지 않도록 미뤄야 한다.
#[cfg(unix)]
#[test]
fn the_start_gate_defers_a_plugin_that_is_still_retiring() {
    use crate::test_fake_plugin as fake;

    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    fake::write(&dir);
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![fake::package(&dir)]);
    let proc = PluginProcess::stub_with_child(ID, slow_child());
    proc.backdate_pong_for_test(super::HEALTHCHECK_TIMEOUT + Duration::from_secs(1));
    mgr.processes.insert(ID.into(), proc);
    mgr.restart_unresponsive_plugins();

    mgr.ensure_listener();
    mgr.start_plugin_internal(&fake::package(&dir));
    assert!(
        !mgr.is_running(ID),
        "옛 프로세스가 빠지기 전에 새 프로세스가 떴다"
    );
    assert_eq!(mgr.retiring_respawn(ID), Some(true));

    pump_until_retired(&mut mgr);
    assert!(mgr.is_running(ID), "회수 뒤에 미뤄 둔 기동이 안 됐다");
    mgr.begin_shutdown_all();
    let give_up = Instant::now() + Duration::from_secs(10);
    while !mgr.poll_shutdown_all() {
        assert!(Instant::now() < give_up, "종료가 10 초 안에 안 끝났다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 무응답 재시작은 옛 프로세스를 뒤에서 회수하고, 새 프로세스를 **그 자리에서** 띄우지
/// 않는다 — 회수가 끝나는 tick 에 띄운다.
#[test]
fn an_unresponsive_restart_waits_for_the_old_process_before_starting() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    let proc = PluginProcess::stub_with_child(ID, slow_child());
    proc.backdate_pong_for_test(super::HEALTHCHECK_TIMEOUT + Duration::from_secs(1));
    mgr.processes.insert(ID.into(), proc);

    let t = Instant::now();
    mgr.restart_unresponsive_plugins();
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "재시작이 회수를 기다렸다: {:?}",
        t.elapsed()
    );
    assert!(
        !mgr.is_running(ID),
        "옛 프로세스가 빠지기 전에 새 프로세스가 떴다"
    );
    assert_eq!(mgr.retiring_respawn(ID), Some(true));
    // 회수가 끝난 뒤의 기동 자체는 이 시험의 몫이 아니다(실제 실행 파일이 없다).
    pump_until_retired(&mut mgr);
}

/// 호스트 종료는 단건 경로로 내려가던 plugin 도 끝날 때까지 기다리고, 다시 띄우지 않는다.
#[test]
fn exit_waits_for_a_retiring_plugin_and_does_not_respawn_it() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    let proc = PluginProcess::stub_with_child(ID, slow_child());
    proc.backdate_pong_for_test(super::HEALTHCHECK_TIMEOUT + Duration::from_secs(1));
    mgr.processes.insert(ID.into(), proc);
    mgr.restart_unresponsive_plugins();

    mgr.begin_shutdown_all();
    assert!(
        !mgr.poll_shutdown_all(),
        "회수 중인 plugin 을 두고 종료가 끝났다"
    );
    assert_eq!(mgr.retiring_respawn(ID), Some(false));
    let give_up = Instant::now() + Duration::from_secs(10);
    while !mgr.poll_shutdown_all() {
        assert!(Instant::now() < give_up, "종료가 10 초 안에 안 끝났다");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(mgr.retiring_count(), 0);
    assert!(!mgr.is_running(ID));
}

/// 디렉토리를 지우거나 덮어쓰는 호출자는 옛 프로세스가 사라질 때까지 기다린다.
#[test]
fn wait_retired_blocks_until_the_child_is_gone() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    mgr.processes
        .insert(ID.into(), PluginProcess::stub_with_child(ID, slow_child()));
    mgr.disable(ID).expect("disable");
    assert!(
        !mgr.wait_retired(ID),
        "disable 로 내려간 회수는 재기동 예약을 넘기지 않는다"
    );
    assert_eq!(mgr.retiring_count(), 0);
}
