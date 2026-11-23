//! 자동 재시작의 기동이 실패하면 한도까지 다시 띄우고, 한도에서 자동 비활성화하는지 확인한다.
//! 재시도 시각은 pump에 넘기는 now로 건너뛴다. 실제 기동은 bash /dev/tcp 가짜 플러그인을 쓰므로
//! Unix에서만 실행한다.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tasty_terminal::waker_factory::NoopWakerFactory;
use tasty_utils::test_wait::holds_by_every;

use super::{
    FailureKind, PluginManager, PluginTick, RESTART_FAILURE_LIMIT, RESTART_FAILURE_WINDOW,
    RemotePublication,
};
use crate::host_cmd::{HostCmd, SurfaceHandles};
use crate::process::PluginProcess;
use crate::test_support::HomeEnvGuard;

const ID: &str = "com.example.respawn";

const MANIFEST: &str = r#"
manifest_version = 1
id = "com.example.respawn"
name = "Respawn"
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

fn write_plugin(dir: &Path, entry: &str) -> tasty_plugin_manifest::PluginPackage {
    std::fs::create_dir_all(dir).expect("plugin dir");
    std::fs::write(dir.join("tasty-plugin.toml"), MANIFEST).expect("manifest");
    tasty_test_support::write_executable(&dir.join("entry.sh"), entry).expect("entry");
    tasty_plugin_manifest::PluginPackage {
        dir: dir.to_path_buf(),
        manifest: toml::from_str(MANIFEST).expect("fixture manifest"),
    }
}

fn manager(pkg: &tasty_plugin_manifest::PluginPackage) -> PluginManager {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![pkg.clone()]);
    mgr.ensure_listener();
    mgr
}

fn fail(mgr: &mut PluginManager) {
    mgr.on_plugin_spawn_failure(
        ID,
        FailureKind::ConnectFailed,
        anyhow::anyhow!("did not connect within 10s"),
    );
}

fn due(mgr: &PluginManager) -> Option<Instant> {
    mgr.respawn_streaks.get(ID).and_then(|s| s.due)
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

/// 자동 재시작의 연결 실패 뒤 실패 창만큼 지나면 다시 띄우고, 연결하면 남은 surface를 새 프로세스에 게시한다.
#[test]
fn a_failed_automatic_restart_is_retried_and_reattaches_its_surface() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.host_cmd_tx
        .send(HostCmd::RemoteSurfaceCreated {
            surface_id: 7,
            plugin_id: ID.into(),
            kind: "document".into(),
            cwd: None,
            params: json!({"file": "/tmp/a.md"}),
            handles: SurfaceHandles {
                display_name: Arc::default(),
                snapshot_cache: Arc::default(),
            },
        })
        .expect("host cmd");
    mgr.drain_host_cmds();

    let before = Instant::now();
    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    assert!(!mgr.is_running(ID));
    assert!(!mgr.is_auto_disabled(ID));
    let at = due(&mgr).expect("자동 재시작의 기동 실패 뒤 재시도를 예약하지 않았다");
    assert!(at >= before + RESTART_FAILURE_WINDOW);
    assert!(mgr.timers.is_registered(PluginTick::Respawn));

    mgr.pump(at - Duration::from_millis(1));
    assert!(!mgr.is_running(ID), "재시도 시각 전에 다시 띄웠다");
    mgr.pump(at);
    assert!(mgr.is_running(ID), "재시도 시각에 다시 띄우지 않았다");

    let give_up = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    assert!(
        holds_by_every(give_up, Duration::ZERO, |last| {
            if !last && std::mem::replace(&mut started, true) {
                mgr.pump(Instant::now());
                std::thread::sleep(Duration::from_millis(20));
            }
            !mgr.is_connecting(ID)
        }),
        "재시도한 plugin 이 연결하지 않았다"
    );
    assert!(mgr.is_running(ID), "재시도한 plugin 이 연결 뒤 내려갔다");
    assert!(
        !mgr.respawn_streaks.contains_key(ID),
        "연결에 성공했는데 연속 실패 기록이 남았다"
    );
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    let current = mgr.processes[ID].reply_binding();
    assert!(
        matches!(&mgr.surfaces[&7].publication, RemotePublication::Sent(b) if b.ptr_eq(&current)),
        "남은 surface 를 재시도한 프로세스에 게시하지 않았다"
    );
    shut_down(&mut mgr);
}

/// 실패 창 밖으로 흩어진 자동 재시작 실패도 한도에서 자동 비활성화한다. 그 전에는 매번 재시도를 예약한다.
#[test]
fn consecutive_restart_failures_auto_disable_at_the_limit() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.begin_respawn_streak(ID);
    let mut waits = Vec::new();
    for n in 1..RESTART_FAILURE_LIMIT {
        let t = Instant::now();
        fail(&mut mgr);
        // 재시도는 실패 창 뒤라 10 초 창 집계에는 겹치지 않는다. 창이 지난 것처럼 창 기록을 비운다.
        mgr.spawn_failures.clear();
        assert!(
            !mgr.is_auto_disabled(ID),
            "{n}번째 실패에서 자동 비활성화했다"
        );
        let at = due(&mgr).unwrap_or_else(|| panic!("{n}번째 실패 뒤 재시도가 없다"));
        waits.push(at.saturating_duration_since(t));
    }
    assert!(
        waits.windows(2).all(|w| w[1] > w[0]),
        "재시도 간격이 늘지 않았다: {waits:?}"
    );

    fail(&mut mgr);
    assert!(
        mgr.is_auto_disabled(ID),
        "한도에 닿았는데 자동 비활성화하지 않았다"
    );
    assert!(!mgr.respawn_streaks.contains_key(ID));
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    let failure = mgr.last_failure(ID).expect("마지막 실패 사유");
    assert_eq!(failure.message, "did not connect within 10s");
    assert!(
        mgr.take_failure_reports().iter().any(|r| matches!(
            r,
            super::FailureReport::AutoDisabled { failures, .. }
                if *failures == RESTART_FAILURE_LIMIT
        )),
        "자동 비활성화 보고가 없다"
    );
}

/// 사용자가 끈 plugin 은 자동 재시작의 실패 기록이 있어도 다시 띄우지 않는다.
#[test]
fn a_user_disabled_plugin_is_not_retried() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);

    // 재시도를 기다리는 중에 끈다.
    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    assert!(due(&mgr).is_some());
    mgr.disable(ID).expect("disable");
    assert!(
        !mgr.respawn_streaks.contains_key(ID),
        "끈 뒤에도 재시도가 남았다"
    );
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));

    // 꺼진 상태에서 실패를 받는다.
    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    assert!(due(&mgr).is_none(), "꺼진 plugin 의 재시도를 예약했다");
    assert!(!mgr.is_auto_disabled(ID));
    mgr.pump(Instant::now() + RESTART_FAILURE_WINDOW * 8);
    assert!(!mgr.is_running(ID), "꺼진 plugin 을 다시 띄웠다");
}

/// 제거된 plugin 은 재시도하지 않는다.
#[test]
fn a_removed_plugin_is_not_retried() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    let at = due(&mgr).expect("재시도 예약");
    mgr.set_packages_for_tests(Vec::new());
    mgr.pump(at);
    assert!(!mgr.is_running(ID));
    assert!(
        !mgr.respawn_streaks.contains_key(ID),
        "제거된 plugin 의 연속 실패 기록이 남았다"
    );

    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    assert!(due(&mgr).is_none(), "제거된 plugin 의 재시도를 예약했다");
}

/// 사용자가 켠 기동의 실패는 지금처럼 재시도하지 않는다.
#[test]
fn a_failure_outside_an_automatic_restart_is_not_retried() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    fail(&mut mgr);
    assert!(due(&mgr).is_none());
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
}

/// 무응답 재시작이 그 plugin 의 연속 실패 집계를 시작한다.
#[test]
fn an_unresponsive_restart_starts_the_streak() {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let base = Instant::now() - Duration::from_secs(200);
    let mut process = PluginProcess::stub_for_test(ID);
    process.set_ping_owed_since(Some(base));
    process.backdate_pong_for_test(Duration::from_secs(300));
    mgr.processes.insert(ID.into(), process);
    mgr.restart_unresponsive_plugins(base + Duration::from_secs(61));
    assert!(!mgr.is_running(ID), "무응답 plugin 을 재시작하지 않았다");
    assert!(
        mgr.respawn_streaks.contains_key(ID),
        "무응답 재시작이 연속 실패 집계를 시작하지 않았다"
    );
}

/// 사용자가 다시 켜면 기다리던 재시도를 거두고 그 기동을 사용자 기동으로 다룬다.
#[test]
fn a_user_enable_takes_over_a_pending_retry() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    assert!(due(&mgr).is_some());
    mgr.enable(ID).expect("enable");
    assert!(
        !mgr.respawn_streaks.contains_key(ID),
        "사용자가 켠 뒤에도 자동 재시작 기록이 남았다"
    );
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));
    shut_down(&mut mgr);
}

/// 실행하면 연결하지 않고 끝난다. 교체 전 파일로 띄우면 연결을 기다리는 시험이 실패한다.
const EXITS: &str = "#!/bin/bash\nexit 1\n";

/// 비동기 파일 교체가 기다리던 재시도를 넘겨받는다. 쓰는 동안 재시도 시각이 지나도 옛 파일로
/// 띄우지 않고, 교체를 마치면 새 파일로 띄운다.
#[test]
fn a_file_swap_takes_over_a_pending_retry() {
    let home = HomeEnvGuard::tasty_home();
    let old = write_plugin(&home.path().join("old"), EXITS);
    let new = write_plugin(&home.path().join("new"), CONNECTS);
    let mut mgr = manager(&old);
    mgr.begin_respawn_streak(ID);
    fail(&mut mgr);
    let at = due(&mgr).expect("재시도 예약");

    let swap = mgr.begin_file_swap(ID, true);
    assert!(
        !swap.swapping && swap.restart && swap.retry.is_some(),
        "교체가 기다리던 재시도를 넘겨받지 않았다"
    );
    assert!(!mgr.respawn_streaks.contains_key(ID));
    assert!(!mgr.timers.is_registered(PluginTick::Respawn));

    // 파일을 쓰는 동안 재시도 시각이 지난다.
    mgr.pump(at + RESTART_FAILURE_WINDOW);
    assert!(!mgr.is_running(ID), "교체 중에 옛 파일로 띄웠다");

    mgr.replace_committed_package(ID, Some(new));
    assert!(
        mgr.finish_file_swap(ID).expect("finish"),
        "교체를 마치고 띄우지 않았다"
    );
    let give_up = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    assert!(
        holds_by_every(give_up, Duration::ZERO, |last| {
            if !last && std::mem::replace(&mut started, true) {
                mgr.pump(Instant::now());
                std::thread::sleep(Duration::from_millis(20));
            }
            !mgr.is_connecting(ID)
        }),
        "교체 뒤 띄운 plugin 이 연결하지 않았다"
    );
    assert!(
        mgr.is_running(ID),
        "새 파일로 띄운 plugin 이 연결하지 않았다"
    );
    shut_down(&mut mgr);
}

/// 재시도가 두 번 실패한 뒤 교체한 기동이 실패해도 자동 재시작의 연속 실패로 세지 않는다.
#[test]
fn a_failure_after_a_file_swap_is_not_an_automatic_restart_failure() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.fail_automatic_restarts_for_test(ID, RESTART_FAILURE_LIMIT - 1);
    assert!(due(&mgr).is_some());

    let swap = mgr.begin_file_swap(ID, true);
    assert!(swap.restart, "교체가 기다리던 재시도를 넘겨받지 않았다");
    assert!(mgr.finish_file_swap(ID).expect("finish"));
    mgr.fail_start_for_test(ID);
    assert!(
        !mgr.is_auto_disabled(ID),
        "교체 뒤 기동 실패를 자동 재시작의 연속 실패로 셌다"
    );
    assert!(due(&mgr).is_none(), "교체 뒤 기동 실패를 재시도했다");
    shut_down(&mut mgr);
}

/// 연결을 기다리는 자동 재시작(재시도 예약 없음)은 그 프로세스를 그대로 두는 교체가 넘겨받지
/// 않는다. 기록이 남아 그 연결이 실패하면 재시도한다.
#[test]
fn a_swap_that_keeps_a_connecting_restart_leaves_its_streak() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.fail_automatic_restarts_for_test(ID, 0);
    assert!(due(&mgr).is_none());

    let swap = mgr.begin_file_swap(ID, false);
    assert!(
        !swap.restart && swap.retry.is_none(),
        "교체가 연결 중인 자동 재시작을 넘겨받았다"
    );
    assert!(
        mgr.respawn_streaks.contains_key(ID),
        "넘겨받지 않는 교체가 연속 기록을 지웠다"
    );
    mgr.fail_start_for_test(ID);
    assert!(due(&mgr).is_some(), "연결 실패를 재시도하지 않았다");
    shut_down(&mut mgr);
}

/// 교체가 넘겨받은 재시도는 실패 수와 시각 그대로 되돌릴 수 있고, 그 사이 꺼졌으면 되돌리지 않는다.
#[test]
fn a_taken_retry_is_restored_only_while_the_plugin_is_still_wanted() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    for disabled in [false, true] {
        let mut mgr = manager(&pkg);
        mgr.fail_automatic_restarts_for_test(ID, 2);
        let before = mgr.respawn_streaks[ID].clone();
        let retry = mgr
            .begin_file_swap(ID, true)
            .retry
            .expect("넘겨받은 재시도");
        assert!(!mgr.respawn_streaks.contains_key(ID));
        if disabled {
            assert!(mgr.config.disable(ID));
        }
        mgr.restore_respawn_retry(ID, retry);
        match mgr.respawn_streaks.get(ID) {
            Some(s) => {
                assert!(!disabled, "꺼진 plugin 의 재시도를 되돌렸다");
                assert_eq!((s.failures, s.due), (before.failures, before.due));
                assert!(mgr.timers.is_registered(PluginTick::Respawn));
            }
            None => assert!(disabled, "넘겨받은 재시도를 되돌리지 않았다"),
        }
        shut_down(&mut mgr);
    }
}

/// 연결을 기다리는 자동 재시작(재시도 예약 없음)을 멈추고 다시 띄우는 교체는 그 기록을 넘겨받는다.
/// 넘겨받은 재시도는 실패 수를 담고 예약 시각은 지금이라, 되돌리면 다음 tick에서 바로 띄운다.
#[test]
fn a_swap_that_stops_a_connecting_restart_takes_its_failure_count() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    let mut mgr = manager(&pkg);
    mgr.fail_automatic_restarts_for_test(ID, 1);
    let at = due(&mgr).expect("재시도 예약");
    mgr.pump(at);
    assert!(
        mgr.is_connecting(ID),
        "재시도가 띄운 기동이 연결을 기다리지 않는다"
    );
    assert!(due(&mgr).is_none());

    let before = Instant::now();
    let swap = mgr.begin_file_swap(ID, true);
    assert!(
        swap.swapping && swap.restart,
        "교체가 연결 중인 재시작을 멈추지 않았다"
    );
    let retry = swap.retry.expect("멈춘 자동 재시작을 넘겨받지 않았다");
    assert!(!mgr.respawn_streaks.contains_key(ID));
    assert!(
        !mgr.wait_retired(ID),
        "교체가 멈춘 프로세스에 회수 뒤 기동이 걸렸다"
    );

    mgr.restore_respawn_retry(ID, retry);
    let s = mgr
        .respawn_streaks
        .get(ID)
        .expect("넘겨받은 재시도를 되돌리지 않았다");
    assert_eq!(s.failures, 1, "넘겨받은 재시도가 실패 수를 잃었다");
    let restored = s.due.expect("되돌린 재시도의 예약");
    assert!(
        before <= restored && restored <= Instant::now(),
        "연결 대기 중 넘겨받은 재시도의 예약이 지금이 아니다"
    );
    mgr.pump(Instant::now());
    assert!(
        mgr.is_running(ID),
        "되돌린 재시도를 다음 tick에 띄우지 않았다"
    );
    shut_down(&mut mgr);
}

/// 무응답 재시작의 옛 프로세스를 회수하는 동안 시작한 교체는 회수 뒤 기동과 연속 기록을 함께
/// 넘겨받는다. 설치본을 바꾸지 않고 실패하면, 아직 회수 중이면 회수 뒤 기동으로, 회수가 끝났으면
/// 다음 tick 의 재시도로 되돌려 결국 다시 띄운다.
#[test]
fn a_swap_during_an_unresponsive_retire_restores_the_restart_on_an_unchanged_failure() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), CONNECTS);
    for restore_while_retiring in [true, false] {
        let mut mgr = manager(&pkg);
        let base = Instant::now() - Duration::from_secs(200);
        let mut process = PluginProcess::stub_for_test(ID);
        process.set_ping_owed_since(Some(base));
        process.backdate_pong_for_test(Duration::from_secs(300));
        mgr.processes.insert(ID.into(), process);
        mgr.restart_unresponsive_plugins(base + Duration::from_secs(61));
        assert!(mgr.is_retiring(ID));

        let swap = mgr.begin_file_swap(ID, true);
        assert!(!swap.swapping && swap.restart);
        let retry = swap
            .retry
            .expect("회수 뒤 기동을 가져간 교체가 자동 재시작을 넘겨받지 않았다");
        assert!(!mgr.respawn_streaks.contains_key(ID));
        let give_up = Instant::now() + Duration::from_secs(10);
        if !restore_while_retiring {
            // App 경로: 회수를 기다린 뒤 쓰기 결과로 되돌린다.
            // poll 은 끝난 회수를 거두기만 한다. 마지막 읽기로 쓴다.
            assert!(
                holds_by_every(give_up, Duration::from_millis(20), |_| mgr
                    .poll_file_retirement(ID)),
                "회수가 10 초 안에 안 끝났다"
            );
        }
        mgr.restore_respawn_retry(ID, retry);
        let s = mgr
            .respawn_streaks
            .get(ID)
            .expect("넘겨받은 재시작을 되돌리지 않았다");
        assert_eq!(s.failures, 0);
        assert_eq!(
            s.due.is_none(),
            restore_while_retiring,
            "회수 중이면 회수 뒤 기동으로, 아니면 재시도 예약으로 되돌려야 한다"
        );

        let mut started = false;
        assert!(
            holds_by_every(give_up, Duration::ZERO, |last| {
                if !last && std::mem::replace(&mut started, true) {
                    mgr.pump(Instant::now());
                    std::thread::sleep(Duration::from_millis(20));
                }
                mgr.is_running(ID)
            }),
            "미변경 실패 뒤 다시 띄우지 않았다 (회수 중 되돌림={restore_while_retiring})"
        );
        assert!(
            mgr.respawn_streaks.contains_key(ID),
            "되돌린 기동이 자동 재시작의 연속 기록 밖에서 떴다"
        );
        shut_down(&mut mgr);
    }
}
