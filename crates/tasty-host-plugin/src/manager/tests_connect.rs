//! 기동이 plugin 의 연결을 메인 스레드에서 기다리지 않는다는 것, 그리고 연결의 결과가
//! 예전 spawn 결과와 같은 갈래로 거둬진다는 것(`manager::connect`, ADR-0505).
//!
//! 가짜 plugin 은 bash `/dev/tcp` 로 연결하므로 unix 에서만 돈다.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{PluginManager, RESTART_FAILURE_LIMIT};
use crate::protocol::PluginRequest;
use crate::test_support::HomeEnvGuard;
use tasty_terminal::waker_factory::NoopWakerFactory;

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

/// 옆의 `go` 파일이 생길 때까지 연결하지 않는다. 연결하면 받은 첫 줄을 `first-line` 에 쓴다.
/// 연결을 시험이 쥐고 있으므로, 기동이 연결을 기다리면 `go` 가 안 생긴 채 시한까지 선다.
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
    std::fs::write(&path, entry).expect("entry");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
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
    while !done(mgr) {
        assert!(Instant::now() < give_up, "10 초 안에 안 됐다: {what}");
        mgr.pump(Instant::now());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn shut_down(mgr: &mut PluginManager) {
    mgr.begin_shutdown_all();
    let give_up = Instant::now() + Duration::from_secs(10);
    while !mgr.poll_shutdown_all() {
        assert!(Instant::now() < give_up, "종료가 10 초 안에 안 끝났다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 기동은 plugin 이 연결하기 전에 돌아오고, 그 사이 보낸 요청은 연결 뒤 plugin 에 닿는다.
///
/// 연결을 시험이 쥐고 있다 — `go` 를 만들기 전에는 plugin 이 연결할 수 없다. 기동이 연결을
/// 기다리는 구현이면 `start_plugin_internal` 이 연결 한도까지 서다가 실패로 끝나 아래 첫
/// 단정이 깨진다. 시계로 재지 않으므로 부하와 무관하다.
#[test]
fn a_start_returns_before_the_plugin_connects_and_queued_requests_reach_it() {
    let home = HomeEnvGuard::tasty_home();
    let dir = home.path().join("plugin");
    let pkg = write_plugin(&dir, GATED);
    let mut mgr = manager(&pkg, Duration::from_secs(3));

    mgr.start_plugin_internal(&pkg);
    assert!(
        mgr.is_running(ID),
        "기동이 연결을 기다렸거나 연결 전의 plugin 을 떠 있지 않은 것으로 쳤다"
    );
    assert!(mgr.is_connecting(ID), "연결 전의 plugin 을 연결됨으로 쳤다");
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
    // 연결을 거두면 연결 중이 아니다 — 이것이 안 풀리면 그 plugin 에 보낸 요청은 시한이
    // 영영 안 선다(`deadline_from_connection`).
    pump_until(&mut mgr, "연결 성사를 거둔다", |m| {
        !m.is_connecting(ID)
    });
    assert!(mgr.is_running(ID), "연결한 plugin 이 내려갔다");
    shut_down(&mut mgr);
}

/// 끝내 연결하지 않는 plugin 은 예전 기동 실패와 같은 갈래로 간다 — 내려가고, 연속 실패로
/// 세이고, 한도만큼 되풀이되면 자동 비활성된다.
///
/// 누적을 재는 이유: 실패 기록을 기동 성공(자식을 띄운 것) 자리에서 지우면 연결 실패가 매번
/// 0 으로 돌아가 자동 비활성이 영영 안 걸린다. 지우는 자리는 연결 성사여야 한다.
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

/// 연결을 받을 자리는 자식을 띄우기 **전에** 열린다 — 곧바로 연결하는 plugin 이 등록보다 먼저
/// 인증해 토큰 없음으로 거절되지 않는다.
///
/// 옛 코드는 자식을 띄운 뒤 등록했다. 부하로 호출 스레드가 밀리면 가짜 plugin 이 그 사이 인증해
/// 거절됐고, 호스트는 연결 한도까지 기다린 뒤 기동 실패로 쳤다(같은 시험 12 개 병렬 + 바쁜 루프
/// 40 개에서 36 번 중 21~23 번). 그 밀림을 hook 으로 1 s 만들어 결정적으로 세운다 — 올바른
/// 구현은 hook 과 무관하게 연결된다.
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
        "연결이 성사되거나 실패로 거둬진다",
        |m| !m.spawn_failures.contains_key(ID) || !m.is_running(ID),
    );
    assert!(
        mgr.is_running(ID) && !mgr.spawn_failures.contains_key(ID),
        "곧바로 연결한 plugin 이 등록 전에 인증해 거절됐다"
    );
    shut_down(&mut mgr);
}

/// 전체 기동은 연결의 결과까지 기다린다 — 부팅은 이 뒤 hello 를 짧은 시한으로 기다리므로
/// 연결이 그 시한을 갉아먹으면 안 된다. 돌아온 시점에 연결 실패가 이미 거둬져 있다.
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
        "전체 기동이 연결의 결과를 거두지 않고 돌아왔다"
    );
    assert_eq!(mgr.spawn_failures.get(ID).map(Vec::len), Some(1));
    pump_until(&mut mgr, "버린 프로세스가 회수된다", |m| {
        m.retiring_count() == 0
    });
}
