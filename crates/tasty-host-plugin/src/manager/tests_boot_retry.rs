//! 부팅 기동의 실패는 자동 재시작과 같은 백오프·한도로 다시 띄우고, 사용자가 켠 기동의 실패는
//! 재시도 없이 바로 알리는지 확인한다. 재시도 시각은 pump에 넘기는 now로 건너뛴다. 실제 기동은
//! bash /dev/tcp 가짜 플러그인을 쓰므로 Unix에서만 실행한다.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tasty_terminal::waker_factory::NoopWakerFactory;
use tasty_utils::test_wait::holds_by_every;

use super::{
    FailureKind, FailureReport, PluginManager, PluginTick, RESTART_FAILURE_LIMIT,
    RESTART_FAILURE_WINDOW,
};
use crate::test_support::HomeEnvGuard;

const ID: &str = "com.example.boot-retry";

const MANIFEST: &str = r#"
manifest_version = 1
id = "com.example.boot-retry"
name = "Boot retry"
version = "1.0.0"
api_version = "1"
[entry]
type = "process"
command = "entry.sh"
"#;

/// 바로 연결하고 입력을 버린다.
const CONNECTS: &str = r#"#!/bin/bash
exec 3<>/dev/tcp/127.0.0.1/"$TASTY_HOST_IPC_PORT"
printf '{"plugin_id":"%s","token":"%s"}\n' "$TASTY_PLUGIN_ID" "$TASTY_PLUGIN_TOKEN" >&3
exec cat <&3 >/dev/null
"#;

/// 끝내 연결하지 않는다.
const SILENT: &str = "#!/bin/bash\nexec sleep 30\n";

/// 연결 대기 한도. 연결 실패를 빨리 내기 위해 줄인다.
const HANDSHAKE: Duration = Duration::from_millis(300);

/// 재시도 시각 계산이 실제 시계로 몇 ms 밀리는 것을 허용하는 폭.
const SLACK: Duration = Duration::from_secs(2);

/// 패키지를 둔다. `entry` 가 None 이면 실행 파일이 없어 프로세스 띄우기가 곧바로 실패한다.
fn write_plugin(dir: &Path, entry: Option<&str>) -> tasty_plugin_manifest::PluginPackage {
    std::fs::create_dir_all(dir).expect("plugin dir");
    std::fs::write(dir.join("tasty-plugin.toml"), MANIFEST).expect("manifest");
    if let Some(entry) = entry {
        tasty_test_support::write_executable(&dir.join("entry.sh"), entry).expect("entry");
    }
    tasty_plugin_manifest::PluginPackage {
        dir: dir.to_path_buf(),
        manifest: toml::from_str(MANIFEST).expect("fixture manifest"),
    }
}

fn manager(pkg: &tasty_plugin_manifest::PluginPackage) -> PluginManager {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![pkg.clone()]);
    mgr.ensure_listener();
    mgr.listener
        .as_mut()
        .expect("listener binds")
        .set_handshake_timeout(HANDSHAKE);
    mgr
}

fn boot(mgr: &mut PluginManager) {
    mgr.start_at_boot(&[ID.to_string()]);
}

fn due(mgr: &PluginManager) -> Option<Instant> {
    mgr.respawn_streaks.get(ID).and_then(|s| s.due)
}

fn pump_until(mgr: &mut PluginManager, what: &str, mut done: impl FnMut(&PluginManager) -> bool) {
    let give_up = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    assert!(
        holds_by_every(give_up, Duration::ZERO, |last| {
            if !last && std::mem::replace(&mut started, true) {
                mgr.pump(Instant::now());
                std::thread::sleep(Duration::from_millis(20));
            }
            done(mgr)
        }),
        "10 초 안에 안 됐다: {what}"
    );
}

fn shut_down(mgr: &mut PluginManager) {
    mgr.begin_shutdown_all();
    let give_up = Instant::now() + Duration::from_secs(10);
    // poll 은 끝난 자식을 거두고 원래도 받던 기한 지난 강제 종료만 해 새 시도가 없다. 마지막 읽기로 쓴다.
    assert!(
        holds_by_every(give_up, Duration::from_millis(20), |_| mgr
            .poll_shutdown_all()),
        "종료가 10 초 안에 안 끝났다"
    );
}

/// 실패 하나 뒤 다음 재시도까지의 기다림을 잰다. 재시도는 실패 창 뒤라 10 초 창 집계와 겹치지
/// 않으므로, 창이 지난 것처럼 창 기록을 비운다.
fn wait_after_failure(mgr: &mut PluginManager, failed_at: Instant, n: usize) -> Instant {
    mgr.spawn_failures.clear();
    assert!(
        !mgr.is_auto_disabled(ID),
        "{n}번째 실패에서 자동 비활성화했다"
    );
    let at = due(mgr).unwrap_or_else(|| panic!("부팅 기동의 {n}번째 실패 뒤 재시도가 없다"));
    let wait = at.saturating_duration_since(failed_at);
    let expected = RESTART_FAILURE_WINDOW * (1u32 << (n - 1));
    assert!(
        expected <= wait + SLACK && wait <= expected + SLACK,
        "{n}번째 실패 뒤 {expected:?} 가 아니라 {wait:?} 뒤 재시도한다"
    );
    assert!(mgr.timers.is_registered(PluginTick::Respawn));
    at
}

fn assert_auto_disabled(mgr: &mut PluginManager, kind: FailureKind) {
    assert!(
        mgr.is_auto_disabled(ID),
        "부팅 기동이 {RESTART_FAILURE_LIMIT}번 실패했는데 자동 비활성화하지 않았다"
    );
    assert!(!mgr.respawn_streaks.contains_key(ID));
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    assert!(
        mgr.last_failure(ID)
            .is_some_and(|f| f.kind == kind && !f.message.is_empty()),
        "자동 비활성화 사유가 남지 않았다: {:?}",
        mgr.last_failure(ID)
    );
    assert!(
        mgr.take_failure_reports().iter().any(|r| matches!(
            r,
            FailureReport::AutoDisabled { failures, .. } if *failures == RESTART_FAILURE_LIMIT
        )),
        "자동 비활성화 보고가 없다"
    );
}

/// Given 앱 부팅, When 프로세스를 띄우지 못하면, Then 10초·20초 뒤 다시 띄우고 세 번째 실패에서 자동 비활성화한다.
#[test]
fn a_boot_spawn_failure_is_retried_until_the_limit() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), None);
    let mut mgr = manager(&pkg);
    let mut failed_at = Instant::now();
    boot(&mut mgr);
    assert!(!mgr.is_running(ID));
    for n in 1..RESTART_FAILURE_LIMIT {
        let at = wait_after_failure(&mut mgr, failed_at, n);
        mgr.pump(at - Duration::from_millis(1));
        assert!(due(&mgr) == Some(at), "재시도 시각 전에 다시 띄웠다 ({n})");
        failed_at = Instant::now();
        mgr.pump(at);
    }
    assert_auto_disabled(&mut mgr, FailureKind::SpawnFailed);
}

/// Given 앱 부팅, When connect 실패, Then 띄우지 못한 경우와 같은 재시도·한도가 적용된다.
#[test]
fn a_boot_connect_failure_is_retried_until_the_limit() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), Some(SILENT));
    let mut mgr = manager(&pkg);
    let mut failed_at = Instant::now();
    boot(&mut mgr);
    for n in 1..RESTART_FAILURE_LIMIT {
        let at = wait_after_failure(&mut mgr, failed_at, n);
        // 연결하지 못한 옛 프로세스의 회수를 실제 시각으로 끝낸 뒤 재시도 시각으로 건너뛴다.
        // 미래 시각의 pump 가 회수 tick 을 그 시각에 걸면 실제 시각으로는 회수가 끝나지 않는다.
        pump_until(
            &mut mgr,
            "연결하지 못한 프로세스의 회수",
            |m| !m.is_retiring(ID),
        );
        mgr.pump(at);
        pump_until(
            &mut mgr,
            "재시도 기동이 연결을 기다린다",
            |m| m.is_connecting(ID),
        );
        pump_until(&mut mgr, "재시도 기동의 연결 실패", |m| {
            !m.is_running(ID)
        });
        failed_at = Instant::now() - HANDSHAKE;
    }
    assert_auto_disabled(&mut mgr, FailureKind::ConnectFailed);
    shut_down(&mut mgr);
}

/// Given 부팅 재시도 대기 중, When 사용자가 disable하거나 제거, Then 다시 띄우지 않는다.
#[test]
fn a_boot_retry_is_dropped_when_the_plugin_is_disabled_or_removed() {
    for removed in [false, true] {
        let home = HomeEnvGuard::tasty_home();
        let pkg = write_plugin(&home.path().join("plugin"), None);
        let mut mgr = manager(&pkg);
        boot(&mut mgr);
        let at = due(&mgr).expect("부팅 기동 실패 뒤 재시도 예약");
        // 파일을 채워 두어, 다시 띄우면 실제로 뜨게 한다.
        tasty_test_support::write_executable(&pkg.dir.join("entry.sh"), CONNECTS).expect("entry");
        if removed {
            mgr.set_packages_for_tests(Vec::new());
        } else {
            mgr.disable(ID).expect("disable");
            assert!(
                !mgr.respawn_streaks.contains_key(ID),
                "끈 뒤에도 부팅 재시도가 남았다"
            );
        }
        mgr.pump(at);
        assert!(!mgr.is_running(ID), "removed={removed}: 다시 띄웠다");
        assert!(
            !mgr.respawn_streaks.contains_key(ID),
            "removed={removed}: 연속 실패 기록이 남았다"
        );
        assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    }
}

/// Given 사용자가 enable, When 띄우거나 연결하지 못하면, Then 재시도 타이머 없이 실패를 바로 알린다.
#[test]
fn a_user_enable_failure_is_reported_without_a_retry() {
    for entry in [None, Some(SILENT)] {
        let home = HomeEnvGuard::tasty_home();
        let pkg = write_plugin(&home.path().join("plugin"), entry);
        let mut mgr = manager(&pkg);
        mgr.enable(ID).expect("enable");
        pump_until(&mut mgr, "사용자 기동의 실패", |m| {
            !m.is_running(ID) && m.last_failure(ID).is_some()
        });
        assert!(
            !mgr.respawn_streaks.contains_key(ID),
            "entry={entry:?}: 사용자 기동의 실패에 재시도를 예약했다"
        );
        assert!(!mgr.timers.is_registered(PluginTick::Respawn));
        assert!(!mgr.is_auto_disabled(ID));
        assert!(
            mgr.take_failure_reports()
                .iter()
                .any(|r| matches!(r, FailureReport::Failed { plugin_id, .. } if plugin_id == ID)),
            "entry={entry:?}: 사용자 기동의 실패를 알리지 않았다"
        );
        mgr.pump(Instant::now() + RESTART_FAILURE_WINDOW * 8);
        assert!(
            !mgr.is_running(ID),
            "entry={entry:?}: 사용자 기동을 다시 띄웠다"
        );
        shut_down(&mut mgr);
    }
}

/// Given 부팅 재시도 중, When 연결에 성공, Then 연속 실패 기록이 지워진다.
#[test]
fn a_successful_boot_retry_clears_the_streak() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), None);
    let mut mgr = manager(&pkg);
    boot(&mut mgr);
    let at = due(&mgr).expect("부팅 기동 실패 뒤 재시도 예약");
    tasty_test_support::write_executable(&pkg.dir.join("entry.sh"), CONNECTS).expect("entry");
    mgr.pump(at);
    assert!(mgr.is_running(ID), "재시도 시각에 다시 띄우지 않았다");
    pump_until(&mut mgr, "재시도 기동의 연결", |m| {
        !m.is_connecting(ID)
    });
    assert!(mgr.is_running(ID), "재시도한 plugin 이 연결 뒤 내려갔다");
    assert!(
        !mgr.respawn_streaks.contains_key(ID),
        "연결에 성공했는데 연속 실패 기록이 남았다"
    );
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    shut_down(&mut mgr);
}

/// Given 사용자 enable 로 띄워 연결을 기다리는 plugin, When 부팅 기동이 지나가고 그 연결이 실패,
/// Then 부팅 기동으로 세지 않아 재시도 타이머 없이 사용자 기동의 실패로 바로 알린다.
#[test]
fn a_boot_pass_leaves_an_already_started_plugin_to_its_own_path() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), Some(SILENT));
    let mut mgr = manager(&pkg);
    mgr.enable(ID).expect("enable");
    assert!(
        mgr.is_connecting(ID),
        "사용자 기동이 연결을 기다리지 않는다"
    );
    boot(&mut mgr);
    pump_until(&mut mgr, "사용자 기동의 연결 실패", |m| {
        !m.is_running(ID)
    });
    assert!(
        !mgr.respawn_streaks.contains_key(ID),
        "부팅 기동이 이미 떠 있던 사용자 기동에 연속 실패 기록을 붙였다"
    );
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    assert!(
        mgr.take_failure_reports()
            .iter()
            .any(|r| matches!(r, FailureReport::Failed { plugin_id, .. } if plugin_id == ID)),
        "사용자 기동의 실패를 알리지 않았다"
    );
    shut_down(&mut mgr);
}

/// Given 호스트 listener 를 열 수 없는 부팅, When 부팅 기동, Then 띄우지 못한 실패로 세어 재시도를
/// 예약하고, 재시도 때 listener 를 다시 열어 띄운다. 사용자 enable 은 같은 실패에 재시도하지 않는다.
#[test]
fn a_boot_start_without_a_listener_is_retried() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), Some(CONNECTS));
    crate::listener::REFUSE_BIND.with(|r| r.set(true));

    let mut user = PluginManager::new(Arc::new(NoopWakerFactory));
    user.set_packages_for_tests(vec![pkg.clone()]);
    user.enable(ID).expect("enable");
    assert!(!user.is_running(ID));
    assert!(
        !user.respawn_streaks.contains_key(ID),
        "사용자 기동의 listener 실패에 재시도를 예약했다"
    );

    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![pkg.clone()]);
    let failed_at = Instant::now();
    boot(&mut mgr);
    assert!(!mgr.is_running(ID));
    assert!(
        mgr.last_failure(ID)
            .is_some_and(|f| f.kind == FailureKind::SpawnFailed && f.message.contains("listener")),
        "listener 실패가 기록되지 않았다: {:?}",
        mgr.last_failure(ID)
    );
    let at = wait_after_failure(&mut mgr, failed_at, 1);

    crate::listener::REFUSE_BIND.with(|r| r.set(false));
    mgr.pump(at);
    assert!(
        mgr.is_running(ID),
        "재시도 때 listener 를 다시 열어 띄우지 않았다"
    );
    pump_until(&mut mgr, "재시도 기동의 연결", |m| {
        !m.is_connecting(ID)
    });
    assert!(!mgr.respawn_streaks.contains_key(ID));
    shut_down(&mut mgr);
}
