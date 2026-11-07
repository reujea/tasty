//! 피커는 어느 호출자가 열었든 경로 배열과 요청 식별자만 남기고 아무 동작도 하지 않는다.
//! 위치 결정은 호출자가 열기 전에 끝낸다.

use super::{FILE_PICKER_POPUP_ID, FilePickerAction, apply_action, locate};
use crate::core::origin::{AgentSource, IntentOrigin, UserSource};
use crate::intent::{DispatchedIntent, Intent, UiIntent};
use crate::state::{FilePickerLocation, FilePickerRequestId, FilePickerRequester, RequestContext};

fn open_popup_requests(intents: &[DispatchedIntent]) -> Vec<&DispatchedIntent> {
    intents
        .iter()
        .filter(|r| {
            matches!(
                r.body,
                Intent::Ui(UiIntent::OpenPopup {
                    id: FILE_PICKER_POPUP_ID,
                    ..
                })
            )
        })
        .collect()
}

/// 피커를 연 호출자의 진입점을 실행한다. 플러그인은 IPC 트리거로 연다.
fn open_as(
    caller: &str,
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> FilePickerRequestId {
    match caller {
        "tools" => crate::adapters::ui::file_open_tool::open(state, &engine.read()),
        "new_html" => {
            crate::adapters::ui::new_tab_picker::open(
                state,
                &engine.read(),
                20,
                "html",
                "url",
                &["html", "htm", "svg"],
            );
        }
        "plugin" => {
            let caller = tasty_ipc::caller::CallerContext::Plugin {
                plugin_id: "com.example.p".to_string(),
                permissions: std::sync::Arc::new(Default::default()),
            };
            let resp = crate::adapters::ipc::handler::file_picker::handle_trigger(
                state,
                engine,
                &caller,
                serde_json::json!(1),
                &serde_json::json!({}),
            );
            let id = resp.result.expect("trigger succeeds")["request_id"]
                .as_u64()
                .expect("request_id");
            assert_eq!(
                state.dialogs.file_picker.as_ref().map(|d| d.request_id),
                Some(FilePickerRequestId(id)),
                "wire request_id is the picker request id"
            );
        }
        other => panic!("unknown caller {other}"),
    }
    state
        .dialogs
        .file_picker
        .as_ref()
        .expect("picker opened")
        .request_id
}

#[test]
fn confirming_picker_only_records_paths_and_dispatches_nothing() {
    for caller in ["tools", "new_html", "plugin"] {
        let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
        let mut engine = engine_session.borrow_mut();
        let request_id = open_as(caller, &mut state, &mut engine);
        state.take_pending_intents();

        state
            .dialogs
            .file_picker
            .as_mut()
            .expect("open")
            .current_dir = "/x".to_string();
        let action = apply_action(
            &mut state,
            &engine.read(),
            FilePickerAction::ConfirmEntry("a.md".to_string()),
        );
        assert!(matches!(
            action,
            crate::adapters::ui::popup::PopupAction::Close
        ));
        let (outcome, _requester) =
            crate::app::dispatch::file_picker::take_outcome(&mut state).expect("result");

        let expected = std::path::Path::new("/x")
            .join("a.md")
            .to_string_lossy()
            .into_owned();
        assert_eq!(outcome.paths, vec![expected], "{caller}");
        assert_eq!(outcome.request_id, request_id, "{caller}");
        assert!(!outcome.cancelled, "{caller}");
        assert!(
            state.take_pending_intents().is_empty(),
            "{caller}: picker and result take queued an intent"
        );
        assert!(state.toasts.len() == 0, "{caller}: picker pushed a toast");
        assert!(state.dialogs.file_picker.is_none(), "{caller}");
        assert!(
            crate::app::dispatch::file_picker::take_outcome(&mut state).is_none(),
            "{caller}: result is taken once"
        );
    }
}

#[test]
fn cancelling_picker_leaves_an_empty_cancelled_outcome() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let mut engine = engine_session.borrow_mut();
    let request_id = open_as("tools", &mut state, &mut engine);
    apply_action(&mut state, &engine.read(), FilePickerAction::Cancel);
    let (outcome, requester) =
        crate::app::dispatch::file_picker::take_outcome(&mut state).expect("result");
    assert_eq!(outcome.request_id, request_id);
    assert!(outcome.cancelled);
    assert!(outcome.paths.is_empty());
    assert!(requester.is_none());
}

/// 호출자 이름과 그 호출자가 연 popup 의 기대 출처 판정.
type OriginCase = (&'static str, fn(&IntentOrigin) -> bool);

#[test]
fn each_caller_opens_with_its_own_origin() {
    let cases: [OriginCase; 3] = [
        ("tools", |o| {
            matches!(
                o,
                IntentOrigin::User {
                    source: UserSource::Menu("tools_menu")
                }
            )
        }),
        ("new_html", |o| {
            matches!(
                o,
                IntentOrigin::User {
                    source: UserSource::ContextMenu
                }
            )
        }),
        (
            "plugin",
            |o| matches!(o, IntentOrigin::Agent { source: AgentSource::Plugin(id) } if id == "com.example.p"),
        ),
    ];
    for (caller, is_expected) in cases {
        let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
        let mut engine = engine_session.borrow_mut();
        open_as(caller, &mut state, &mut engine);
        let intents = state.take_pending_intents();
        let opens = open_popup_requests(&intents);
        assert_eq!(opens.len(), 1, "{caller}");
        assert!(
            is_expected(&opens[0].origin),
            "{caller}: {:?}",
            opens[0].origin
        );
        let requester = state
            .dialogs
            .file_picker
            .as_ref()
            .and_then(|d| d.requester.as_ref())
            .map(|r: &FilePickerRequester| r.plugin_id.clone());
        assert_eq!(
            requester.is_some(),
            caller == "plugin",
            "{caller}: only the plugin request keeps a requester for popup scope"
        );
    }
}

/// 출발 surface가 없으면 호출자가 넘긴 workspace로 원격을 정하고, 결과를 열 surface도 그때 정한다.
#[test]
fn locate_without_origin_uses_the_callers_workspace_and_picks_its_surface() {
    let (state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let ws = engine.workspace_at(0).expect("mirror workspace");
    assert!(ws.mirror);
    let (ws_id, surfaces) = (ws.id, ws.all_surface_ids());

    let location = locate(&state, &engine.read(), None, 0, None);
    let FilePickerLocation::Remote {
        mirror_ws_id,
        surface_id: Some(sid),
        dir: None,
    } = location
    else {
        panic!("expected remote location with a surface, got {location:?}");
    };
    assert_eq!(mirror_ws_id, ws_id);
    assert!(surfaces.contains(&sid));
}

#[test]
fn locate_on_a_local_workspace_is_local() {
    let (state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    let location = locate(&state, &engine.read(), None, 0, Some("/srv".into()));
    assert_eq!(
        location,
        FilePickerLocation::Local {
            dir: Some("/srv".into())
        }
    );
}
