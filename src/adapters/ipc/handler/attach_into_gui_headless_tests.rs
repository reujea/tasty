//! 헤드리스는 처리하지 않을 `attach.into_gui`를 접수하지 않는다.
//! 외부 IPC와 플러그인 host-call이 같은 engine 처리기에 닿으므로 두 호출자를 함께 확인한다.

use serde_json::json;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

const OWN_PORT: u16 = 47_001;

fn call(caller: &CallerContext, port: u16) -> JsonRpcResponse {
    let mut core = super::cli_entry_tests::test_core_builder()
        .build()
        .expect("core");
    core.record_ipc_port(OWN_PORT);
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let req = JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        session_token: None,
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "attach.into_gui".into(),
        params: json!({ "port": port, "workspace": 1 }),
    };
    super::handle_with_caller(&mut core, &mut state, &mut engine, &req, caller)
}

fn plugin_caller() -> CallerContext {
    CallerContext::Plugin {
        plugin_id: "com.example.attach".to_string(),
    }
}

#[test]
fn every_caller_and_port_is_refused_as_unsupported_by_name() {
    for (who, caller) in [
        ("local", CallerContext::local()),
        ("plugin", plugin_caller()),
    ] {
        for port in [OWN_PORT, OWN_PORT + 1] {
            let resp = call(&caller, port);
            assert!(
                resp.result.is_none(),
                "{who}/{port}: 처리하지 않을 요청을 접수했다: {:?}",
                resp.result
            );
            let error = resp
                .error
                .unwrap_or_else(|| panic!("{who}/{port}: 오류가 없다"));
            assert_eq!(error.code, -32017, "{who}/{port}: {}", error.message);
            assert!(
                error.message.contains("attach.into_gui"),
                "{who}/{port}: 거절 문구에 메서드 이름이 없다: {}",
                error.message
            );
        }
    }
}
