//! 결과 전달 경로는 피커 결과를 호출자에게 넘길 뿐이다. 플러그인 요청에는 wire 이벤트 내용만 만든다.

use super::{deliver_outcome, take_outcome};
use crate::adapters::ipc::handler::file_picker::{handle_trigger, result_payload};
use crate::state::FilePickerResult;

fn plugin_caller() -> tasty_ipc::caller::CallerContext {
    tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: "com.tasty.markdown".to_string(),
    }
}

fn trigger(
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> u64 {
    let resp = handle_trigger(
        state,
        engine,
        &plugin_caller(),
        serde_json::json!(1),
        // 범위는 출발 surface 에서 나온다. 없으면 피커가 열리지 않는다.
        &serde_json::json!({
            "filters": ["md"],
            "origin_surface_id": state.focused_surface_id(&engine.read()),
        }),
    );
    resp.result.expect("trigger succeeds")["request_id"]
        .as_u64()
        .expect("request_id")
}

/// 플러그인이 연 피커를 확정해도 호스트는 파일을 열지 않고 결과 이벤트 내용만 만든다.
#[test]
fn plugin_confirm_yields_only_the_result_event() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let wire_id = trigger(&mut state, &mut engine);
    crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state)
        .expect("open")
        .result = Some(FilePickerResult::Confirmed {
        paths: vec!["/x/a.md".to_string()],
    });

    let (outcome, requester) = take_outcome(&mut state).expect("result");
    let requester = requester.expect("plugin requester");
    assert_eq!(requester.request_id, wire_id);
    assert_eq!(outcome.request_id.0, wire_id);
    deliver_outcome(&mut state, &engine, None, Some(&requester), &outcome);

    assert!(state.take_pending_intents().is_empty());
    assert!(state.toasts.len() == 0);
    assert_eq!(
        result_payload(&requester, &outcome),
        serde_json::json!({ "request_id": wire_id, "paths": ["/x/a.md"], "cancelled": false })
    );
}

#[test]
fn plugin_cancel_yields_cancelled_with_no_paths() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let wire_id = trigger(&mut state, &mut engine);
    crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state)
        .expect("open")
        .result = Some(FilePickerResult::Cancelled);

    let (outcome, requester) = take_outcome(&mut state).expect("result");
    let requester = requester.expect("plugin requester");
    deliver_outcome(&mut state, &engine, None, Some(&requester), &outcome);
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(
        result_payload(&requester, &outcome),
        serde_json::json!({ "request_id": wire_id, "paths": [], "cancelled": true })
    );
}

/// 결과가 없는 피커는 그대로 두고, 결과는 한 번만 꺼낸다.
#[test]
fn take_outcome_waits_for_a_result_and_takes_it_once() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    trigger(&mut state, &mut engine);
    assert!(take_outcome(&mut state).is_none());
    assert!(crate::adapters::ui::popup::file_picker::instances::current(&mut state).is_some());
    crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state)
        .expect("open")
        .result = Some(FilePickerResult::Cancelled);
    assert!(take_outcome(&mut state).is_some());
    assert!(take_outcome(&mut state).is_none());
    assert!(crate::adapters::ui::popup::file_picker::instances::current(&mut state).is_none());
}
