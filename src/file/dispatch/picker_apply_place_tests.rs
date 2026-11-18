//! 파일 열기 결과 탭은 사용자 입력이 정한 자리(`FileResultPlace`)에 열린다. 처리 전에 포커스가 옮겨져도
//! 그 자리는 바뀌지 않고, 자리가 사라졌으면 다른 pane 에 열지 않는다.

use super::tests::build_test_core;
use crate::app::command::DomainIntent;
use crate::app::creation_intent::{Resolved, resolve};
use crate::file::dispatch::{FileDispatchOrigin, FileDispatchPolicy, FileResultPlace};
use crate::intent::{DispatchedIntent, Intent};

fn user(result_place: FileResultPlace) -> FileDispatchPolicy {
    FileDispatchPolicy {
        dispatch_origin: FileDispatchOrigin::User,
        ignore_size_limit: false,
        run: None,
        result_place,
    }
}

fn resolved_pane(
    state: &crate::state::RequestContext,
    engine: &crate::runtime::engine_session::EngineSession,
    intent: &DispatchedIntent,
) -> Option<u32> {
    match resolve(state, &engine.as_ref(), &intent.body, &intent.origin).expect("resolve") {
        Resolved::Domain(domain) => match *domain {
            DomainIntent::CreateTab { pane_id, .. } => Some(pane_id),
            other => panic!("expected a new tab, got {other:?}"),
        },
        Resolved::Dropped => None,
        Resolved::Unchanged => panic!("a new tab must resolve"),
    }
}

/// 자리가 정해진 결과 탭은 처리 때의 포커스(pane 20)가 아니라 그 pane(10)에 열린다.
#[test]
fn a_result_tab_opens_in_the_pane_chosen_at_input() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    {
        let mut engine = engine_session.borrow_mut();
        assert_eq!(state.focused_pane_id(&engine.read()), 20);
        assert!(super::super::open_surface_tab(
            &mut core,
            &mut state,
            &mut engine,
            "empty",
            serde_json::json!({}),
            None,
            user(FileResultPlace::Pane(10)),
        ));
    }
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert_eq!(
        resolved_pane(&state, &engine_session, &intents[0]),
        Some(10)
    );
}

/// 자리가 사라졌으면 포커스 pane 에 열지 않고 버린다.
#[test]
fn a_result_tab_whose_pane_is_gone_is_dropped() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    {
        let mut engine = engine_session.borrow_mut();
        assert!(super::super::open_surface_tab(
            &mut core,
            &mut state,
            &mut engine,
            "empty",
            serde_json::json!({}),
            None,
            user(FileResultPlace::Pane(999)),
        ));
    }
    let intents = state.take_pending_intents();
    assert_eq!(resolved_pane(&state, &engine_session, &intents[0]), None);
}

/// 파일 열기 피커는 연 때의 pane 을 보관하고, 고른 로컬 파일을 그 pane 자리로 보낸다.
#[test]
fn the_file_picker_sends_its_local_result_to_the_pane_it_was_opened_from() {
    let (mut state, engine_session) = crate::state::tests::two_pane_fixture();
    state.select_pane(&engine_session.core_state, 10);
    crate::adapters::ui::file_open_tool::open(&mut state, &engine_session.read());
    state.take_pending_intents();
    let request = state
        .dialogs
        .file_picker_callers
        .open_file
        .clone()
        .expect("the picker request is kept");
    assert_eq!(request.pane, Some(10));
    state.select_pane(&engine_session.core_state, 20);
    crate::adapters::ui::file_open_tool::apply_outcome(
        &mut state,
        &engine_session.core_state,
        &crate::state::FilePickerOutcome {
            request_id: request.request_id,
            paths: vec!["/tmp/a.md".into()],
            cancelled: false,
        },
    );
    let intents = state.take_pending_intents();
    assert!(matches!(
        &intents[0].body,
        Intent::Domain(DomainIntent::DispatchFile {
            origin_surface_id: None,
            result_place: FileResultPlace::Pane(10),
            ..
        })
    ));
}

/// 처리기 선택창은 원래 요청의 결과 자리를 이어받는다.
#[test]
fn the_handler_picker_keeps_the_result_place() {
    let (mut state, engine_session) = crate::state::tests::two_pane_fixture();
    let read = engine_session.read();
    crate::file::dispatch::open_picker_from_read(
        &mut state,
        &read,
        crate::file::format::FileTarget::new("/tmp/a.unknown").into(),
        None,
        read.file_handler.all_handlers(),
        true,
        crate::file::dispatch::PickerOrigin::ShownIn(101),
        user(FileResultPlace::Pane(10)),
    );
    let picker = crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state)
        .expect("picker opens");
    assert_eq!(picker.result_place, FileResultPlace::Pane(10));
    assert_eq!(picker.origin_surface_id, None);
}

/// 선택창 결과를 실행하는 App 경로는 보관한 결과 자리를 정책에 다시 싣는다.
/// 이 경로는 App(창 묶음) 메서드라 시험 상태에서 부를 수 없어 소스로 확인한다.
#[test]
fn the_finished_picker_passes_its_result_place_on() {
    let src = include_str!("../../app/dispatch/picker.rs");
    assert!(src.contains("let result_place = data.result_place;"));
    assert!(src.contains("                    result_place,\n                },"));
}

/// 터미널의 로컬 파일 링크는 링크를 누른 surface 의 pane 을 결과 자리로 싣고 origin 은 비운다.
#[test]
fn a_local_terminal_link_names_its_pane_as_the_result_place() {
    let request = super::super::remote::terminal_link_file_intent(
        crate::file::format::FileTarget::new("/tmp/a.md"),
        101,
        Some(10),
        false,
    );
    assert!(matches!(
        request.body,
        Intent::Domain(DomainIntent::DispatchFile {
            origin_surface_id: None,
            result_place: FileResultPlace::Pane(10),
            ..
        })
    ));
}

/// 창에서만 도는 입력 경로(끌어 놓기, 터미널 링크 클릭, 받은 파일 완료, 단축키 파일 열기)가 싣는 결과 자리.
/// 모두 창(MainView·App) 메서드라 시험 상태에서 부를 수 없어 소스로 확인한다.
#[test]
fn window_input_paths_name_the_result_place() {
    let drop = include_str!("../../view/main/file_drop.rs");
    assert!(drop.contains(
        "drop_result_place(\n            hit.and_then(|sid| engine.find_pane_for_surface(sid)),\n            pane_at,\n        );"
    ));
    assert!(drop.contains(".pane_at_position(engine, x, y, terminal_rect, scale_factor);"));
    assert!(drop.contains("                    origin_surface_id: None,\n"));
    assert!(drop.contains("                    result_place,\n"));
    let mouse = include_str!("../../view/main/mouse.rs");
    assert!(mouse.contains(
        "                            hovered.surface_id,\n                            engine.find_pane_for_surface(hovered.surface_id),"
    ));
    let download = include_str!("../../app/attach_client/file_download.rs");
    assert!(
        download
            .contains("result_place: crate::file::dispatch::FileResultPlace::NewLocalWorkspace,")
    );
    let key = include_str!("../../adapters/ui/input/shortcuts/keybinding.rs");
    let open_at = &key[key.find("file_open_tool::open_at(").expect("open_at call")..];
    let args = &open_at[..open_at.find(");").expect("call end")];
    assert!(args.contains("t.surface_id(),") && args.contains("t.pane,"));
}
