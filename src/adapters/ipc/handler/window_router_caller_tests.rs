//! gui 창 라우터(`route_window_handler`)의 팔마다 **누가 부를 수 있는가**를 명부로 적고 지킨다.
//!
//! 창 라우터에는 창 상태 자체가 대상인 핸들러가 산다 — popup 을 열고 입력 포커스를 옮길 수
//! 있는 자리다. 권한 게이트는 `CallerContext::Local` 을 무조건 통과시키고, 권한 표의
//! `plugin_callable` 은 agent 토큰도 막지 않는다. 그래서 이 문을 지나는 CLI·agent 호출이
//! 사용자 상태에 닿지 않는다는 것(원칙 2.1 ① · 2.3)은 핸들러가 직접 판정해야 하고, 판정을
//! 빠뜨려도 조용하다 — `file_picker.trigger` 가 그랬다
//! (`docs/adr/0498-the-file-picker-trigger-answers-only-a-plugin-caller.md`).
//!
//! 두 시험이 짝이다. 하나는 라우터의 팔과 명부가 같은 집합인지 본다 — 새 팔은 명부에 호출자
//! 정책을 적어야 들어온다. 다른 하나는 `PluginOnly` 인 팔을 CLI·agent 로 실제로 불러, `-32016`
//! 으로 끝나고 창에 아무것도 남기지 않는지 본다.

use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::ipc::caller::CallerContext;
use crate::ipc::protocol::JsonRpcRequest;

/// 창 라우터 팔의 호출자 정책.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowCallers {
    /// plugin 만 부른다. CLI·agent 호출은 `-32016` 으로 끝나고 창을 건드리지 않는다.
    PluginOnly,
}

/// `(메서드, 정책, 근거)`. 근거를 한 줄로 못 적을 거면 팔을 더하지 마라.
const WINDOW_ROUTER_CALLERS: &[(&str, WindowCallers, &str)] = &[(
    "file_picker.trigger",
    WindowCallers::PluginOnly,
    "고른 경로는 호출한 plugin 에만 push 된다 — CLI·agent 호출은 사용자 포커스만 가져간다(ADR-0498)",
)];

/// `route_window_handler` 본문에서 `"<메서드>" =>` 팔 이름을 뽑는다.
fn window_router_arms() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/ipc/handler.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()));
    let src = tasty_doc_guards::source_text::mask_comments(&src);
    let sig = "fn route_window_handler(";
    let start = src
        .find(sig)
        .unwrap_or_else(|| panic!("`{sig}` 를 찾지 못했다 — 라우터가 옮겨졌으면 이 시험도 옮겨라"));
    let open = start + src[start..].find('{').expect("함수 본문");
    let mut depth = 0usize;
    let mut end = src.len();
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &src[open..end];
    let mut arms = BTreeSet::new();
    for (i, _) in body.match_indices("\" =>") {
        let head = &body[..i];
        let q = head.rfind('"').expect("팔 이름의 여는 따옴표");
        arms.insert(head[q + 1..].to_string());
    }
    arms
}

#[test]
fn every_window_router_arm_declares_who_may_call_it() {
    let arms = window_router_arms();
    assert!(
        !arms.is_empty(),
        "창 라우터에서 팔을 하나도 못 찾았다 — 추출기가 깨졌다"
    );
    let declared: BTreeSet<String> = WINDOW_ROUTER_CALLERS
        .iter()
        .map(|(m, _, _)| (*m).to_string())
        .collect();
    let undeclared: Vec<&String> = arms.difference(&declared).collect();
    let stale: Vec<&String> = declared.difference(&arms).collect();
    assert!(
        undeclared.is_empty() && stale.is_empty(),
        "창 라우터 팔과 호출자 명부가 다르다.\n  명부에 없는 팔: {undeclared:?}\n  \
         팔이 없는 명부 항목: {stale:?}\n★ 창 라우터 팔은 사용자 상태(popup·포커스)에 닿는다. \
         새 팔이면 누가 불러도 되는지 정해 `WINDOW_ROUTER_CALLERS` 에 근거와 함께 적고, \
         CLI·agent 가 사용자 상태를 바꾸지 않게 핸들러에서 판정하라"
    );
}

fn request(method: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: method.into(),
        params: json!({}),
        session_token: None,
    }
}

#[test]
fn a_plugin_only_window_arm_refuses_cli_and_agent_without_touching_the_window() {
    let agent = CallerContext::Agent {
        agent_id: "child:1".into(),
        permissions: Arc::new(Default::default()),
    };
    let mut checked = 0usize;
    for (method, policy, _) in WINDOW_ROUTER_CALLERS {
        if *policy != WindowCallers::PluginOnly {
            continue;
        }
        for caller in [CallerContext::Local, agent.clone()] {
            let (mut state, mut engine) = crate::state::tests::test_state();
            let active_before = state.active_workspace;
            let resp = super::route_window_handler(
                &mut state,
                &mut engine,
                &caller,
                &request(method),
                json!(1),
            )
            .unwrap_or_else(|| panic!("`{method}` 이 창 라우터에서 라우팅되지 않았다"));
            let err = resp
                .error
                .unwrap_or_else(|| panic!("`{method}` 을 {caller:?} 가 불렀는데 성공했다"));
            assert_eq!(err.code, -32016, "`{method}` · {caller:?}");
            assert!(
                state.take_pending_intents().is_empty(),
                "`{method}` 을 {caller:?} 가 불렀는데 창 큐에 intent 가 남았다"
            );
            assert!(
                !state.popups.has_focused(),
                "`{method}` 을 {caller:?} 가 불렀는데 포커스를 가진 popup 이 있다"
            );
            assert_eq!(
                state.active_workspace, active_before,
                "`{method}` · {caller:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "PluginOnly 팔을 하나도 재지 않았다");
}
