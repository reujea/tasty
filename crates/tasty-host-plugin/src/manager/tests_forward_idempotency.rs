//! plugin namespace forward 는 plugin 고유 이름에 **정확히 한 번을 약속하지 않는다**(ADR-0361).
//!
//! `PluginManager` 의 forward 는 멱등 키를 싣지도, 같은 호출을 가려내지도, 앞선 답을 되돌려
//! 주지도 않는다. 같은 호출이 두 번 오면 target plugin 이 두 번 실행한다 — plugin 고유 이름에서
//! 그것을 막을 수 있는 자리는 target 자신뿐이다. 이 시험은 **이 크레이트의 forward 층**에서 그
//! 사실을 고정한다: `PluginManager` 에 조용한 중복 제거를 넣으면 여기서 빨개진다.
//!
//! 호스트 바이너리의 forward 앞 보존소(루트 크레이트의 `forward_keeping_the_key`)는 이 시험이
//! 못 본다 — 이 크레이트는 루트 크레이트에 의존하지 않아 그 함수가 이 시험 바이너리에 없다.
//! 그 층은 표가 아는 이름(`image.open` 등)의 forward 를 실제로 거르고 재생하며(ADR-0566),
//! plugin 고유 이름을 거기서 계약 밖으로 지키는지는 루트 크레이트의
//! `idempotency::tests::a_plugin_name_is_forwarded_every_time_with_its_request_untouched` 가 잰다.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use tasty_terminal::waker_factory::NoopWakerFactory;

use super::PluginManager;
use crate::PluginPackage;
use crate::process::PluginProcess;
use crate::protocol::PluginResponse;

const OWNER: &str = "com.example.owner";

fn mgr_owning(prefix: &str) -> PluginManager {
    let manifest: tasty_plugin_manifest::Manifest = toml::from_str(&format!(
        r#"
manifest_version = 1
id = "{OWNER}"
name = "Forward Fixture"
version = "0.0.1"
api_version = "1.0"

[entry]
type = "process"
command = "echo"
args = []

[[contributes.ipc_namespace]]
prefix = "{prefix}"
"#
    ))
    .expect("fixture manifest should parse");
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![PluginPackage {
        dir: PathBuf::from("/nonexistent/forward_fixture"),
        manifest,
    }]);
    mgr
}

/// 같은 요청 id · 같은 params 로 두 번 forward 하면 target 이 **두 번** 부름을 받고,
/// caller 는 두 답을 받으며, 어느 답에도 재생 표지가 없다.
#[test]
fn the_same_forward_twice_runs_the_target_twice_and_is_never_replayed() {
    let mut mgr = mgr_owning("fwd");
    let (owner, owner_rx) = PluginProcess::stub_with_request_rx(OWNER);
    mgr.processes.insert(OWNER.into(), owner);

    let (tx, caller_rx) = mpsc::sync_channel(4);
    let params = serde_json::json!({ "n": 1 });
    for _ in 0..2 {
        mgr.forward_namespace_call(
            "fwd.run",
            params.clone(),
            None,
            serde_json::json!(7),
            tx.clone(),
            None,
        );
    }

    let first = owner_rx
        .try_recv()
        .expect("첫 forward 가 target 에 안 갔다");
    let second = owner_rx
        .try_recv()
        .expect("두 번째 forward 가 target 에 안 갔다 — 호스트가 같은 호출을 걸렀다");
    for req in [&first, &second] {
        assert_eq!(req.method, tasty_plugin_protocol::METHOD_IPC_INVOKE);
    }
    assert_ne!(first.id, second.id, "두 실행이 한 요청 id 로 묶였다");
    assert!(owner_rx.try_recv().is_err());

    for req in [&first, &second] {
        mgr.handle_plugin_response(
            OWNER,
            PluginResponse {
                id: req.id,
                result: Some(serde_json::json!({ "ran": req.id })),
                error: None,
                error_code: None,
            },
        );
    }
    let answers: Vec<_> = caller_rx.try_iter().collect();
    assert_eq!(answers.len(), 2, "caller 가 실행마다 답을 받지 않았다");
    for a in &answers {
        assert_eq!(a.id, serde_json::json!(7));
        assert!(
            !a.idempotent_replay,
            "forward 답에 재생 표지가 붙었다 — 호스트는 forward 를 재생하지 않는다"
        );
    }
    assert_ne!(
        answers[0].result, answers[1].result,
        "두 번째 답이 첫 실행의 결과를 되돌려 줬다"
    );
}
