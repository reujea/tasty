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

/// 호출자가 피커를 여는 범위. 같은 범위의 두 번째 요청만 거절된다.
fn scope_of(caller: Caller) -> PopupScope {
    match caller {
        Caller::Tools | Caller::Plugin => PopupScope::Window,
        Caller::NewHtml => PopupScope::Pane(20),
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
    // 두 번째 요청이 여는 범위에 첫 피커를 둔다. 범위와 사용자 입력 초안이 새 호출로 초기화되면 안 된다.
    let scope = scope_of(second);
    state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope = scope.clone();
    let data = crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state).unwrap();
    data.current_dir = "/x".into();
    data.selected = vec!["kept.md".into()];
    data.result = settled.clone();
    let id = data.request_id;
    let filters = data.filters.clone();
    let requester = requester_key(data.requester.as_ref());
    let tools = state.dialogs.file_picker_callers.open_file.clone();
    let new_tab = state.dialogs.file_picker_callers.new_tab.clone();
    if settled.is_some() {
        // 닫혔지만 결과를 아직 넘기지 않은 피커. 닫기 훅이 결과를 출력함에 남긴다.
        state.popups.close(FILE_PICKER_POPUP_ID);
        for closed in state.popups.take_closed_queue() {
            crate::adapters::ui::popup::file_picker::instances::finish(&mut state, &closed);
        }
    }

    let reply = enter(second, &mut state, &mut engine);
    let kept = Kept {
        label: format!("{first:?} -> {second:?}"),
        id,
        filters,
        requester,
        scope,
    };
    assert_first_entry_kept(&state, &kept, settled.as_ref());
    assert_eq!(state.dialogs.file_picker_callers.open_file, tools);
    assert_eq!(state.dialogs.file_picker_callers.new_tab, new_tab);
    assert!(
        state.take_pending_intents().is_empty(),
        "rejected entry queued an intent"
    );
    assert_busy_feedback(&state, reply);

    assert_original_result_is_delivered_once(&mut state, &engine, first, id, settled);
}

type RequesterKey = Option<(String, u64, Option<u64>)>;

/// 거절된 두 번째 요청 뒤에도 남아 있어야 하는 첫 요청의 값.
struct Kept {
    label: String,
    id: crate::state::FilePickerRequestId,
    filters: Vec<String>,
    requester: RequesterKey,
    scope: PopupScope,
}

fn requester_key(r: Option<&crate::state::FilePickerRequester>) -> RequesterKey {
    r.map(|r| (r.plugin_id.clone(), r.request_id, r.owner_popup_instance))
}

/// 열린 첫 피커는 데이터와 범위를 그대로 두고, 닫혀 결과를 기다리던 첫 피커는 출력함에 혼자 남는다.
fn assert_first_entry_kept(
    state: &RequestContext,
    kept: &Kept,
    settled: Option<&FilePickerResult>,
) {
    let label = &kept.label;
    match settled {
        None => {
            let data = crate::adapters::ui::popup::file_picker::instances::latest(state).unwrap();
            assert_eq!(data.request_id, kept.id, "{label}: replaced request");
            assert_eq!(data.current_dir, "/x");
            assert_eq!(data.selected, ["kept.md"]);
            assert_eq!(data.filters, kept.filters);
            assert_eq!(data.result, None);
            assert_eq!(requester_key(data.requester.as_ref()), kept.requester);
            assert_eq!(
                state.popups.open_scope(FILE_PICKER_POPUP_ID),
                Some(&kept.scope)
            );
        }
        Some(result) => {
            let [done] = &state.dialogs.finished_file_pickers[..] else {
                panic!("{label}: the settled result was not kept alone");
            };
            assert_eq!(done.request_id, kept.id, "{label}: replaced request");
            assert_eq!(done.result, *result);
            assert_eq!(done.scope, kept.scope);
            assert_eq!(requester_key(done.requester.as_ref()), kept.requester);
            assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
        }
    }
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
    let result = settled.clone().unwrap_or(FilePickerResult::Cancelled);
    if settled.is_none() {
        crate::adapters::ui::popup::file_picker::instances::current_mut(state)
            .unwrap()
            .result = Some(result.clone());
    }
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
