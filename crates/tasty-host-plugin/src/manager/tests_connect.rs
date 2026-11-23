//! 연결 대기 중에도 요청을 큐에 넣고, 연결 실패는 기동 실패로 집계하는지 확인한다.
//! 가짜 플러그인은 bash /dev/tcp를 사용하므로 Unix에서만 실행한다.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{PluginManager, RESTART_FAILURE_LIMIT};
use crate::protocol::PluginRequest;
use crate::test_support::HomeEnvGuard;
use tasty_terminal::waker_factory::NoopWakerFactory;
use tasty_utils::test_wait::holds_by_every;

const ID: &str = "com.example.connect";

const MANIFEST: &str = r#"
manifest_version = 1
id = "com.example.connect"
name = "Connect"
version = "1.0.0"
api_version = "1"
[entry]
type = "process"
command = "entry.sh"
"#;

/// go 파일이 생겨야 연결하고, 받은 첫 줄을 first-line에 쓴다.
const GATED: &str = r#"#!/bin/bash
here="$(dirname "$0")"
while [ ! -e "$here/go" ]; do sleep 0.02; done
exec 3<>/dev/tcp/127.0.0.1/"$TASTY_HOST_IPC_PORT"
printf '{"plugin_id":"%s","token":"%s"}\n' "$TASTY_PLUGIN_ID" "$TASTY_PLUGIN_TOKEN" >&3
IFS= read -r ack <&3
IFS= read -r line <&3
printf '%s\n' "$line" > "$here/first-line.tmp"
mv "$here/first-line.tmp" "$here/first-line"
exec cat <&3 >/dev/null
"#;

/// 끝내 연결하지 않는다.
const SILENT: &str = "#!/bin/bash\nexec sleep 30\n";

fn write_plugin(dir: &Path, entry: &str) -> tasty_plugin_manifest::PluginPackage {
    std::fs::create_dir_all(dir).expect("plugin dir");
    std::fs::write(dir.join("tasty-plugin.toml"), MANIFEST).expect("manifest");
    let path = dir.join("entry.sh");
    tasty_test_support::write_executable(&path, entry).expect("entry");
    tasty_plugin_manifest::PluginPackage {
        dir: dir.to_path_buf(),
        manifest: toml::from_str(MANIFEST).expect("fixture manifest"),
    }
}

fn manager(pkg: &tasty_plugin_manifest::PluginPackage, handshake: Duration) -> PluginManager {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![pkg.clone()]);
    mgr.ensure_listener();
    mgr.listener
        .as_mut()
        .expect("listener binds")
        .set_handshake_timeout(handshake);
    mgr
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
    // holds_by 로 바꾸지 않는다. poll_shutdown_all 은 내부 기한이 지난 살아 있는 자식을 kill 하고 기다리므로
    // 기한 뒤 읽기로 쓰면 관측이 아니라 새 종료가 된다. 끝난 자식만 거두는 관측 경로가 없다.
    while !mgr.poll_shutdown_all() {
        assert!(Instant::now() < give_up, "종료가 10 초 안에 안 끝났다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 연결 전에 기동 함수가 반환하고 큐에 넣은 요청은 연결 뒤 전달돼야 한다.
/// go 파일을 쓰기 전에는 연결할 수 없어, 연결을 기다리는 구현은 첫 단언에서 실패한다.
#[test]
fn a_start_returns_before_the_plugin_connects_and_queued_requests_reach_it() {
    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    let pkg = write_plugin(&dir, GATED);
    let mut mgr = manager(&pkg, Duration::from_secs(3));

    mgr.start_plugin_internal(&pkg);
    assert!(
        mgr.is_running(ID),
        "연결 전에도 플러그인 프로세스는 실행 중이어야 한다"
    );
    assert!(
        mgr.is_connecting(ID),
        "연결 전 플러그인은 연결 대기 상태여야 한다"
    );
    let request = PluginRequest::new("probe.echo", serde_json::json!({}), 7);
    mgr.processes[ID]
        .try_send_request(request)
        .expect("연결 전에도 요청은 큐에 들어간다");

    std::fs::write(dir.join("go"), "").expect("release the plugin");
    let first = dir.join("first-line");
    pump_until(&mut mgr, "plugin 이 첫 요청을 받는다", |_| {
        first.exists()
    });
    let line = std::fs::read_to_string(&first).expect("first line");
    assert!(
        line.contains("probe.echo"),
        "연결 전에 보낸 요청이 연결 뒤 첫 줄로 닿아야 한다: {line}"
    );
    // 연결 완료를 처리해야 해당 플러그인의 요청 만료 시간을 계산할 수 있다.
    pump_until(&mut mgr, "연결 완료를 처리한다", |m| {
        !m.is_connecting(ID)
    });
    assert!(mgr.is_running(ID), "연결한 plugin 이 내려갔다");
    shut_down(&mut mgr);
}

/// 연결 실패를 기동 실패로 누적한다. 성공적인 연결 전에 기록을 지우면
/// 매번 연결에 실패하는 플러그인이 자동 비활성화되지 않는다.
#[test]
fn a_plugin_that_never_connects_is_a_spawn_failure_and_repeats_auto_disable_it() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), SILENT);
    let mut mgr = manager(&pkg, Duration::from_millis(200));

    for attempt in 1..=RESTART_FAILURE_LIMIT {
        mgr.start_plugin_internal(&pkg);
        assert!(mgr.is_running(ID), "{attempt}: 기동이 연결을 기다렸다");
        pump_until(&mut mgr, "연결 실패로 내려간다", |m| {
            !m.is_running(ID)
        });
        pump_until(&mut mgr, "버린 프로세스가 회수된다", |m| {
            m.retiring_count() == 0
        });
        if attempt < RESTART_FAILURE_LIMIT {
            assert_eq!(
                mgr.spawn_failures.get(ID).map(Vec::len),
                Some(attempt),
                "연결 실패가 기동 실패로 누적돼야 한다"
            );
        }
    }
    assert!(
        mgr.is_auto_disabled(ID),
        "연결 실패 {RESTART_FAILURE_LIMIT} 회면 자동 비활성돼야 한다"
    );
}

/// 연결이 성사되면 연속 실패 기록이 지워진다.
#[test]
fn a_connection_clears_the_failure_record() {
    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    let pkg = write_plugin(&dir, GATED);
    std::fs::write(dir.join("go"), "").expect("let the plugin connect at once");
    let mut mgr = manager(&pkg, Duration::from_secs(3));
    mgr.spawn_failures
        .insert(ID.to_string(), vec![Instant::now()]);

    mgr.start_plugin_internal(&pkg);
    pump_until(&mut mgr, "연결이 실패 기록을 지운다", |m| {
        !m.spawn_failures.contains_key(ID)
    });
    assert!(mgr.is_running(ID));
    shut_down(&mut mgr);
}

/// 연결 인증 정보를 등록한 뒤 자식을 실행해야 한다.
/// 실행 직후 호스트 처리를 1초 늦춰도 플러그인이 인증을 통과해야 한다.
#[test]
fn the_connection_is_registered_before_the_child_starts() {
    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    let pkg = write_plugin(&dir, GATED);
    std::fs::write(dir.join("go"), "").expect("let the plugin connect at once");
    let mut mgr = manager(&pkg, Duration::from_secs(3));
    mgr.spawn_failures
        .insert(ID.to_string(), vec![Instant::now()]);

    crate::process::AFTER_CHILD_SPAWN_DELAY.with(|d| d.set(Duration::from_secs(1)));
    mgr.start_plugin_internal(&pkg);
    crate::process::AFTER_CHILD_SPAWN_DELAY.with(|d| d.set(Duration::ZERO));

    pump_until(
        &mut mgr,
        "연결 성공 또는 실패를 처리한다",
        |m| !m.spawn_failures.contains_key(ID) || !m.is_running(ID),
    );
    assert!(
        mgr.is_running(ID) && !mgr.spawn_failures.contains_key(ID),
        "곧바로 연결한 plugin 이 등록 전에 인증해 거절됐다"
    );
    shut_down(&mut mgr);
}

/// 전체 기동은 연결 결과까지 기다려 이후 hello 대기 시간을 연결에 쓰지 않는다.
#[test]
fn discover_and_start_settles_connections_before_it_returns() {
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugins").join(ID), SILENT);
    let mut mgr = manager(&pkg, Duration::from_millis(200));

    mgr.discover_and_start();
    assert!(
        mgr.packages().iter().any(|p| p.manifest.id == ID),
        "전제: 디스크의 가짜 plugin 이 발견된다"
    );
    assert!(
        !mgr.is_running(ID),
        "전체 기동은 연결 결과를 처리한 뒤 반환해야 한다"
    );
    assert_eq!(mgr.spawn_failures.get(ID).map(Vec::len), Some(1));
    pump_until(&mut mgr, "버린 프로세스가 회수된다", |m| {
        m.retiring_count() == 0
    });
}

/// 연결하지 못한 기동은 그 세대의 실패로 남고, 실패 창 안의 실패는 보고 한 건으로 묶이며
/// 자동 비활성화는 따로 한 건 보고된다.
#[test]
fn connect_failures_are_kept_per_start_and_reported_once_then_auto_disable() {
    use super::{FailureKind, FailureReport, StartState};
    let home = HomeEnvGuard::tasty_home();
    let pkg = write_plugin(&home.path().join("plugin"), SILENT);
    let mut mgr = manager(&pkg, Duration::from_millis(200));

    for attempt in 1..=RESTART_FAILURE_LIMIT {
        mgr.start_plugin_internal(&pkg);
        let generation = mgr.start_status(ID).expect("start recorded").generation;
        assert_eq!(
            mgr.start_status(ID).map(|s| s.state),
            Some(StartState::Starting)
        );
        pump_until(&mut mgr, "연결 실패로 내려간다", |m| {
            !m.is_running(ID)
        });
        pump_until(&mut mgr, "버린 프로세스가 회수된다", |m| {
            m.retiring_count() == 0
        });
        let status = mgr.start_status(ID).expect("start recorded");
        assert_eq!(status.generation, generation, "{attempt}");
        assert_eq!(status.state, StartState::Failed);
        let failure = status.failure.as_ref().expect("failure kept");
        assert_eq!(failure.kind, FailureKind::ConnectFailed);
        assert!(!failure.message.is_empty());
    }
    let reports = mgr.take_failure_reports();
    assert_eq!(reports.len(), 2, "{reports:?}");
    assert!(matches!(&reports[0], FailureReport::Failed { plugin_id, .. } if plugin_id == ID));
    assert!(matches!(
        &reports[1],
        FailureReport::AutoDisabled { failures, .. } if *failures == RESTART_FAILURE_LIMIT
    ));
    assert!(mgr.is_auto_disabled(ID));
    assert!(mgr.last_failure(ID).is_some());
}

/// 실행할 수 없는 진입점은 띄우기 실패로 그 기동에 남고 보고된다.
#[test]
fn an_entry_that_cannot_run_is_a_spawn_failure_of_that_start() {
    use super::{FailureKind, FailureReport, StartState};
    use std::os::unix::fs::PermissionsExt;
    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    let pkg = write_plugin(&dir, SILENT);
    std::fs::set_permissions(dir.join("entry.sh"), std::fs::Permissions::from_mode(0o644))
        .expect("drop the execute bit");
    let mut mgr = manager(&pkg, Duration::from_millis(200));

    mgr.start_plugin_internal(&pkg);
    pump_until(&mut mgr, "실패 결과가 난다", |m| {
        m.start_status(ID)
            .is_some_and(|s| s.state == StartState::Failed)
    });
    let status = mgr.start_status(ID).expect("start recorded").clone();
    let failure = status.failure.as_ref().expect("failure kept");
    // 실행 비트가 없으면 spawn 자체가 실패하거나, 셸이 진입점을 못 열어 연결 전에 끝난다.
    assert!(
        matches!(
            failure.kind,
            FailureKind::SpawnFailed | FailureKind::ConnectFailed
        ),
        "{failure:?}"
    );
    let reports = mgr.take_failure_reports();
    assert!(
        matches!(reports.as_slice(), [FailureReport::Failed { failure: f, .. }] if f.generation == status.generation),
        "{reports:?}"
    );
}

/// 연결에 성공한 기동은 성공으로 남고 보고가 없다. 결과 전에 끄면 취소로 끝난다.
#[test]
fn a_connected_start_reports_nothing_and_a_disable_while_starting_cancels() {
    use super::StartState;
    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    let pkg = write_plugin(&dir, GATED);
    let mut mgr = manager(&pkg, Duration::from_secs(3));

    mgr.start_plugin_internal(&pkg);
    let first = mgr.start_status(ID).expect("start").generation;
    mgr.disable(ID).expect("disable while starting");
    assert_eq!(
        mgr.start_status(ID).map(|s| s.state),
        Some(StartState::Cancelled)
    );
    pump_until(&mut mgr, "끈 프로세스가 회수된다", |m| {
        m.retiring_count() == 0
    });

    std::fs::write(dir.join("go"), "").expect("let the plugin connect at once");
    mgr.enable(ID).expect("enable");
    let second = mgr.start_status(ID).expect("start").generation;
    assert_ne!(first, second);
    pump_until(&mut mgr, "연결한다", |m| {
        m.start_status(ID)
            .is_some_and(|s| s.state == StartState::Connected)
    });
    assert!(mgr.take_failure_reports().is_empty());
    shut_down(&mut mgr);
}
