//! 요청별 IntentOutbox를 창 큐로 옮겨도 순서가 유지되는지 확인한다.
//! 기존 항목 뒤에, 진입 검사와 핸들러가 만든 intent를 요청 순서대로 추가해야 한다.

use crate::runtime::engine_access::EngineMut;
use serde_json::json;
use std::sync::Arc;
use tasty_plugin_manifest::Permission;

use crate::app::command::DomainIntent;
use crate::intent::{DispatchedIntent, Intent};
use crate::ipc::caller::CallerContext;
use crate::ipc::protocol::JsonRpcRequest;

fn request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
    JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: method.into(),
        params,
        session_token: None,
    }
}

fn label(intent: &DispatchedIntent) -> String {
    match &intent.body {
        Intent::Domain(DomainIntent::PushNotification { source, .. }) => {
            format!("PushNotification:{source}")
        }
        Intent::Domain(DomainIntent::SetTerminalMark { .. }) => "SetTerminalMark".into(),
        Intent::RestoreClosedItem { .. } => "RestoreClosedItem".into(),
        other => format!("{:?}", std::mem::discriminant(other)),
    }
}

fn labels(state: &crate::state::RequestContext) -> Vec<String> {
    state.pending_intents.iter().map(label).collect()
}

#[test]
fn handler_intents_append_after_the_queue_in_request_order() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = super::cli_entry_tests::test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let sid = state.focused_surface_id(&engine).expect("focused surface");
    state.dispatch_intent(Intent::RestoreClosedItem { pane: None }.from_user_shortcut("sentinel"));

    for req in [
        request(
            "notification.create",
            json!({ "title": "t", "body": "b", "surface_id": sid }),
        ),
        request("surface.set_mark", json!({ "surface_id": sid })),
    ] {
        let resp = super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::local(),
        );
        assert!(
            resp.error.is_none(),
            "{} 실패: {:?}",
            req.method,
            resp.error
        );
    }

    assert_eq!(
        labels(&state),
        [
            "RestoreClosedItem",
            "PushNotification:host",
            "SetTerminalMark"
        ],
        "요청이 낸 intent 는 기존 큐 뒤에 요청 순서대로 쌓여야 한다"
    );
}

// 따로 모은 진입 검사와 핸들러의 intent도 발생 순서를 유지해야 한다.
#[test]
fn gate_intents_precede_the_handler_intents_of_the_same_request() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = super::cli_entry_tests::test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let sid = state.focused_surface_id(&engine).expect("focused surface");

    let set = super::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &request(
            "telemetry.cap.set",
            json!({
                "agent": "order-probe",
                "metric": "ipc_calls",
                "threshold": 1,
                "action": "notify",
            }),
        ),
        &CallerContext::local(),
    );
    assert!(set.error.is_none(), "cap 등록 실패: {:?}", set.error);
    assert!(state.pending_intents.is_empty());

    let agent = CallerContext::Agent {
        agent_id: "order-probe".into(),
        permissions: Arc::new([Permission::Notification].into_iter().collect()),
    };
    let resp = super::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &request(
            "notification.create",
            json!({ "title": "t", "body": "b", "surface_id": sid }),
        ),
        &agent,
    );
    assert!(resp.error.is_none(), "{:?}", resp.error);

    assert_eq!(
        labels(&state),
        ["PushNotification:telemetry.cap", "PushNotification:host"],
        "게이트의 상한 알림이 핸들러의 알림보다 먼저 쌓여야 한다"
    );
}

#[test]
fn a_rejected_request_leaves_the_queue_untouched() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = super::cli_entry_tests::test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    state.dispatch_intent(Intent::RestoreClosedItem { pane: None }.from_user_shortcut("sentinel"));

    let resp = super::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &request("surface.set_mark", json!({})),
        &CallerContext::local(),
    );
    assert!(resp.error.is_some(), "surface_id 없는 set_mark 는 거절된다");
    assert_eq!(labels(&state), ["RestoreClosedItem"]);
}

// 상한 승인 알림과 핸들러 알림은 source가 같으므로 제목도 구분한다.
fn detailed_labels(state: &crate::state::RequestContext) -> Vec<String> {
    state
        .pending_intents
        .iter()
        .map(|intent| match &intent.body {
            Intent::Domain(DomainIntent::PushNotification { source, title, .. })
                if source == "host" =>
            {
                if title.contains("승인 필요") {
                    format!("PushNotification:{source}:approval")
                } else {
                    format!("PushNotification:{source}:{title}")
                }
            }
            #[cfg(feature = "gui")]
            Intent::Ui(crate::intent::UiIntent::OpenPopup(_)) => "OpenPopup".into(),
            _ => label(intent),
        })
        .collect()
}

// 실제 SQLite 저장소로 상한 평가 순서를 확인한다.
fn core_with_ordered_memory() -> crate::app::services::AppServices {
    let store = tasty_memory::MemoryStore::open_in_memory().expect("in-memory sqlite");
    super::cli_entry_tests::test_core_builder()
        .with_memory(Arc::new(std::sync::Mutex::new(store)))
        .build()
        .expect("test AppServices")
}

fn set_notify_and_approval_caps(
    core: &mut crate::app::services::AppServices,
    state: &mut crate::state::RequestContext,
    engine: &mut EngineMut<'_>,
    agent: &str,
    metric: &str,
) {
    for action in ["notify", "require_approval"] {
        let resp = super::handle_with_caller(
            core,
            state,
            engine,
            &request(
                "telemetry.cap.set",
                json!({
                    "agent": agent,
                    "metric": metric,
                    "threshold": 1,
                    "action": action,
                }),
            ),
            &CallerContext::local(),
        );
        assert!(
            resp.error.is_none(),
            "cap({action}) 등록 실패: {:?}",
            resp.error
        );
    }
    assert!(state.pending_intents.is_empty());
}

// 먼저 만든 알림보다 창 큐에 직접 들어가는 승인 팝업이 앞서면 안 된다.
#[test]
fn a_notify_cap_precedes_the_approval_popup_fired_by_the_same_gate() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core_with_ordered_memory();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let sid = state.focused_surface_id(&engine).expect("focused surface");
    set_notify_and_approval_caps(
        &mut core,
        &mut state,
        &mut engine,
        "order-probe",
        "ipc_calls",
    );

    let agent = CallerContext::Agent {
        agent_id: "order-probe".into(),
        permissions: Arc::new([Permission::Notification].into_iter().collect()),
    };
    let resp = super::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &request(
            "notification.create",
            json!({ "title": "t", "body": "b", "surface_id": sid }),
        ),
        &agent,
    );
    assert!(resp.error.is_none(), "{:?}", resp.error);

    #[cfg(feature = "gui")]
    let expected = [
        "PushNotification:telemetry.cap",
        "OpenPopup",
        "PushNotification:host:approval",
        "PushNotification:host:t",
    ]
    .as_slice();
    #[cfg(not(feature = "gui"))]
    let expected = ["PushNotification:telemetry.cap", "PushNotification:host:t"].as_slice();
    assert_eq!(detailed_labels(&state), expected);
}

#[test]
fn a_notify_cap_precedes_the_approval_popup_fired_by_a_telemetry_record() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core_with_ordered_memory();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    set_notify_and_approval_caps(&mut core, &mut state, &mut engine, "order-probe", "tokens");

    let resp = super::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &request(
            "telemetry.record",
            json!({ "agent": "order-probe", "metric": "tokens", "value": 1 }),
        ),
        &CallerContext::local(),
    );
    assert!(resp.error.is_none(), "{:?}", resp.error);
    // 기록 응답 뒤 cap 액션은 메인 루프가 회차마다 실행하는 후속 처리다.
    super::telemetry_follow_up::run(&mut core, &mut state, &mut engine);

    #[cfg(feature = "gui")]
    let expected = [
        "PushNotification:telemetry.cap",
        "OpenPopup",
        "PushNotification:host:approval",
    ]
    .as_slice();
    #[cfg(not(feature = "gui"))]
    let expected = ["PushNotification:telemetry.cap"].as_slice();
    assert_eq!(detailed_labels(&state), expected);
}

// 이벤트마다 상한이 하나이므로 저장소 순서와 무관하게 배치 순서를 따라야 한다.
#[test]
fn intents_in_one_outbox_arrive_in_the_order_they_were_pushed() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = super::cli_entry_tests::test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    for metric in ["m_a", "m_b"] {
        let resp = super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &request(
                "telemetry.cap.set",
                json!({ "agent": "order-probe", "metric": metric, "threshold": 1, "action": "notify" }),
            ),
            &CallerContext::local(),
        );
        assert!(
            resp.error.is_none(),
            "cap({metric}) 등록 실패: {:?}",
            resp.error
        );
    }

    let resp = super::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &request(
            "telemetry.record_batch",
            json!({ "events": [
                { "agent": "order-probe", "metric": "m_b", "value": 1 },
                { "agent": "order-probe", "metric": "m_a", "value": 1 },
            ] }),
        ),
        &CallerContext::local(),
    );
    assert!(resp.error.is_none(), "{:?}", resp.error);
    super::telemetry_follow_up::run(&mut core, &mut state, &mut engine);

    let titles: Vec<&str> = state
        .pending_intents
        .iter()
        .map(|intent| match &intent.body {
            Intent::Domain(DomainIntent::PushNotification { title, .. }) => title.as_str(),
            _ => "other",
        })
        .collect();
    assert_eq!(
        titles,
        ["Cap 'm_b' 임계 도달", "Cap 'm_a' 임계 도달"],
        "같은 큐에 넣은 intent는 배치 순서를 유지해야 한다"
    );
}
