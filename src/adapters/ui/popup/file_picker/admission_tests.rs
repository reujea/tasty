//! 모든 진입점의 중복 요청이 기존 요청·결과·부모 관계를 보존하는지 확인한다.

use super::FILE_PICKER_POPUP_ID;
use crate::adapters::ipc::handler::file_picker::{handle_trigger, result_payload};
use crate::adapters::ui::popup::{PopupScope, defs};
use crate::app::dispatch::file_picker::{deliver_outcome, take_outcome};
use crate::runtime::engine_access::EngineMut;
use crate::state::{FilePickerOutcome, FilePickerResult, RequestContext};

#[derive(Clone, Copy, Debug)]
enum Caller {
    Tools,
    NewHtml,
    Plugin,
}

fn enter(
    caller: Caller,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
) -> Option<tasty_ipc::protocol::JsonRpcResponse> {
    match caller {
        Caller::Tools => crate::adapters::ui::file_open_tool::open(state, &engine.read()),
        Caller::NewHtml => crate::adapters::ui::new_tab_picker::open(
            state,
            &engine.read(),
            20,
            "html",
            "url",
            &["html"],
        ),
        Caller::Plugin => {
            // owner 팝업은 그리기 전 단계가 기록한 결속 범위로 찾는다.
            state
                .plugin_popup_bound_scopes
                .insert(7, ("com.example.picker".into(), Some(PopupScope::Window)));
            return Some(handle_trigger(
                state,
                engine,
                &tasty_ipc::caller::CallerContext::Plugin {
                    plugin_id: "com.example.picker".into(),
                    permissions: std::sync::Arc::new(Default::default()),
                },
                serde_json::json!(1),
                &serde_json::json!({"owner_popup_instance": 7, "filters": ["md"]}),
            ));
        }
    }
    None
}

fn rejected_entries_preserve(first: Caller) {
    for second in [Caller::Tools, Caller::NewHtml, Caller::Plugin] {
        for settled in [
            None,
            Some(FilePickerResult::Cancelled),
            Some(FilePickerResult::Confirmed {
                paths: vec!["/x/kept.md".into()],
            }),
        ] {
            rejected_entry_preserves(first, second, settled);
        }
    }
}

fn rejected_entry_preserves(first: Caller, second: Caller, settled: Option<FilePickerResult>) {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    if let Some(reply) = enter(first, &mut state, &mut engine) {
        assert!(reply.result.is_some());
    }
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    // 부모에서 상속받은 범위와 사용자 입력 초안도 새 호출로 초기화되면 안 된다.
    let scope = PopupScope::Surface(state.focused_surface_id(&engine).unwrap());
    state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope = scope.clone();
    let data = state.dialogs.file_picker.as_mut().unwrap();
    data.current_dir = "/x".into();
    data.selected = vec!["kept.md".into()];
    data.result = settled.clone();
    let id = data.request_id;
    let filters = data.filters.clone();
    let requester = data
        .requester
        .as_ref()
        .map(|r| (r.plugin_id.clone(), r.request_id, r.owner_popup_instance));
    let tools = state.dialogs.file_picker_callers.open_file.clone();
    let new_tab = state.dialogs.file_picker_callers.new_tab.clone();
    if settled.is_some() {
        state.popups.close(FILE_PICKER_POPUP_ID);
    }

    let reply = enter(second, &mut state, &mut engine);
    let data = state.dialogs.file_picker.as_ref().unwrap();
    assert_eq!(
        data.request_id, id,
        "{first:?} -> {second:?}: replaced request"
    );
    assert_eq!(data.current_dir, "/x");
    assert_eq!(data.selected, ["kept.md"]);
    assert_eq!(data.filters, filters);
    assert_eq!(data.result, settled);
    assert_eq!(
        data.requester
            .as_ref()
            .map(|r| { (r.plugin_id.clone(), r.request_id, r.owner_popup_instance) }),
        requester
    );
    assert_eq!(state.dialogs.file_picker_callers.open_file, tools);
    assert_eq!(state.dialogs.file_picker_callers.new_tab, new_tab);
    assert_eq!(
        state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope,
        scope
    );
    assert!(
        state.take_pending_intents().is_empty(),
        "rejected entry queued an intent"
    );
    assert_busy_feedback(&state, reply);

    assert_original_result_is_delivered_once(&mut state, &engine, first, id, settled);
}

fn assert_busy_feedback(
    state: &RequestContext,
    reply: Option<tasty_ipc::protocol::JsonRpcResponse>,
) {
    match reply {
        Some(reply) => {
            assert_eq!(reply.error.unwrap().code, -32000);
            assert_eq!(state.toasts.len(), 0, "IPC busy must not notify the user");
        }
        None => assert_eq!(
            state.toasts.messages(),
            [crate::i18n::t("filepicker.already_open")],
            "a user entry needs visible feedback",
        ),
    }
}

fn assert_original_result_is_delivered_once(
    state: &mut RequestContext,
    engine: &crate::core::CoreState,
    first: Caller,
    id: crate::state::FilePickerRequestId,
    settled: Option<FilePickerResult>,
) {
    let result = settled.unwrap_or(FilePickerResult::Cancelled);
    state.dialogs.file_picker.as_mut().unwrap().result = Some(result.clone());
    let (outcome, requester) = take_outcome(state).unwrap();
    assert_eq!(outcome, FilePickerOutcome::new(id, result));
    assert!(take_outcome(state).is_none());
    if let Some(req) = &requester {
        assert_eq!(
            result_payload(req, &outcome),
            serde_json::json!({
                "request_id": id.0, "paths": outcome.paths, "cancelled": outcome.cancelled,
            })
        );
    }
    deliver_outcome(state, engine, None, requester.as_ref(), &outcome);
    let delivered = state.take_pending_intents();
    let expected = usize::from(!outcome.cancelled && !matches!(first, Caller::Plugin));
    assert_eq!(
        delivered.len(),
        expected,
        "original caller must receive its result"
    );
    deliver_outcome(state, engine, None, None, &outcome);
    assert!(
        state.take_pending_intents().is_empty(),
        "host result applied twice"
    );
}

#[test]
fn reentry_preserves_plugin_requester_scope_and_settled_result() {
    rejected_entries_preserve(Caller::Plugin);
}

#[test]
fn reentry_preserves_tools_request_and_settled_result() {
    rejected_entries_preserve(Caller::Tools);
}

#[test]
fn reentry_preserves_new_html_request_and_settled_result() {
    rejected_entries_preserve(Caller::NewHtml);
}
