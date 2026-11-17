//! Tools 메뉴·단축키의 파일 열기는 둘러보게 한 위치로 결과를 해석한다.
//! 로컬은 파일 핸들러로 열고, 원격은 그 mirror workspace의 surface를 출발점으로 원격 파일을 연다.

use super::{apply_outcome, open};
use crate::app::command::DomainIntent;
use crate::intent::{DispatchedIntent, Intent};
use crate::state::{
    FilePickerLocation, FilePickerOutcome, FilePickerRequestId, OpenFilePickerRequest,
    RequestContext,
};

fn confirmed(request_id: FilePickerRequestId, paths: &[&str]) -> FilePickerOutcome {
    FilePickerOutcome {
        request_id,
        paths: paths.iter().map(|p| p.to_string()).collect(),
        cancelled: false,
    }
}

fn stored(state: &RequestContext) -> OpenFilePickerRequest {
    state
        .dialogs
        .file_picker_callers
        .open_file
        .clone()
        .expect("caller keeps its request")
}

/// DispatchFile마다 (경로, 출발 surface)를 돌려준다. 다른 intent가 섞이면 실패한다.
fn dispatched_files(intents: &[DispatchedIntent]) -> Vec<(String, Option<u32>)> {
    intents
        .iter()
        .map(|r| {
            assert!(matches!(
                r.origin,
                crate::intent::IntentOrigin::User {
                    source: crate::core::origin::UserSource::Menu("file_picker_confirm")
                }
            ));
            match &r.body {
                Intent::Domain(DomainIntent::DispatchFile {
                    target,
                    depth,
                    origin_surface_id,
                    dispatch_origin,
                    ..
                }) => {
                    assert_eq!(*depth, crate::file::format::DetectDepth::Deep);
                    assert_eq!(
                        *dispatch_origin,
                        crate::file::dispatch::FileDispatchOrigin::User
                    );
                    (target.display().to_string(), *origin_surface_id)
                }
                other => panic!("expected DispatchFile, got {other:?}"),
            }
        })
        .collect()
}

#[test]
fn local_outcome_opens_the_file_with_the_file_handler() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let engine = engine_session.borrow_mut();
    open(&mut state, &engine.read());
    let request = stored(&state);
    assert!(matches!(request.location, FilePickerLocation::Local { .. }));
    assert_eq!(
        crate::adapters::ui::popup::file_picker::instances::current(&mut state)
            .map(|d| d.request_id),
        Some(request.request_id)
    );
    state.take_pending_intents();

    apply_outcome(
        &mut state,
        &engine,
        &confirmed(request.request_id, &["/x/a.md"]),
    );
    assert_eq!(
        dispatched_files(&state.take_pending_intents()),
        vec![("/x/a.md".to_string(), None)]
    );
    assert!(state.dialogs.file_picker_callers.open_file.is_none());
    assert!(state.toasts.len() == 0);
}

#[test]
fn remote_outcome_opens_the_remote_file_from_the_mirror_surface() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    open(&mut state, &engine.read());
    let request = stored(&state);
    let FilePickerLocation::Remote {
        mirror_ws_id,
        surface_id: Some(sid),
        ..
    } = request.location
    else {
        panic!("expected remote location, got {:?}", request.location);
    };
    assert_eq!(
        crate::adapters::ui::popup::file_picker::instances::current(&mut state)
            .and_then(|d| d.mirror_ws_id),
        Some(mirror_ws_id)
    );
    assert!(engine.is_mirror_surface(sid));
    assert!(crate::core::origin::require_origin_pane(&engine, sid).is_ok());
    state.take_pending_intents();

    apply_outcome(
        &mut state,
        &engine,
        &confirmed(request.request_id, &["/srv/notes.md"]),
    );
    assert_eq!(
        dispatched_files(&state.take_pending_intents()),
        vec![("/srv/notes.md".to_string(), Some(sid))]
    );
    assert!(state.toasts.len() == 0, "no copy toast");
}

/// 피커가 열린 동안 보관한 surface가 닫혔으면 같은 mirror workspace의 다른 surface로 연다.
#[test]
fn closed_origin_surface_falls_back_to_another_surface_of_the_mirror() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let ws = engine.workspace_at(0).expect("mirror workspace");
    let (ws_id, surfaces) = (ws.id, ws.all_surface_ids());
    let request_id = FilePickerRequestId::next();
    state.dialogs.file_picker_callers.open_file = Some(OpenFilePickerRequest {
        request_id,
        location: FilePickerLocation::Remote {
            mirror_ws_id: ws_id,
            surface_id: Some(987_654),
            dir: None,
        },
    });
    apply_outcome(&mut state, &engine, &confirmed(request_id, &["/srv/a.md"]));
    let files = dispatched_files(&state.take_pending_intents());
    assert_eq!(files.len(), 1);
    let sid = files[0].1.expect("origin surface");
    assert!(surfaces.contains(&sid));
    assert!(crate::core::origin::require_origin_pane(&engine, sid).is_ok());
}

/// mirror workspace까지 사라졌으면 원격 경로를 로컬로 오해해 열지 않고 무시한다.
#[test]
fn closed_mirror_workspace_opens_nothing() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let request_id = FilePickerRequestId::next();
    state.dialogs.file_picker_callers.open_file = Some(OpenFilePickerRequest {
        request_id,
        location: FilePickerLocation::Remote {
            mirror_ws_id: 123_456,
            surface_id: Some(987_654),
            dir: None,
        },
    });
    apply_outcome(&mut state, &engine, &confirmed(request_id, &["/srv/a.md"]));
    assert!(state.take_pending_intents().is_empty());
    assert!(state.dialogs.file_picker_callers.open_file.is_none());
}

#[test]
fn cancelled_outcome_opens_nothing_and_releases_the_request() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let engine = engine_session.borrow_mut();
    open(&mut state, &engine.read());
    let request_id = stored(&state).request_id;
    state.take_pending_intents();
    apply_outcome(
        &mut state,
        &engine,
        &FilePickerOutcome {
            request_id,
            paths: Vec::new(),
            cancelled: true,
        },
    );
    assert!(state.take_pending_intents().is_empty());
    assert!(state.dialogs.file_picker_callers.open_file.is_none());
}

/// 이전 요청의 결과가 늦게 와도 다시 연 요청에는 적용하지 않고, 결과는 한 번만 적용된다.
#[test]
fn late_outcome_of_a_previous_request_is_ignored() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let engine = engine_session.borrow_mut();
    open(&mut state, &engine.read());
    let first = stored(&state).request_id;
    crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state)
        .unwrap()
        .result = Some(crate::state::FilePickerResult::Cancelled);
    let (cancelled, _) = crate::app::dispatch::file_picker::take_outcome(&mut state).unwrap();
    apply_outcome(&mut state, &engine, &cancelled);
    open(&mut state, &engine.read());
    let second = stored(&state).request_id;
    assert_ne!(first, second);
    state.take_pending_intents();

    apply_outcome(&mut state, &engine, &confirmed(first, &["/late.md"]));
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(stored(&state).request_id, second);

    let now = confirmed(second, &["/now.md"]);
    apply_outcome(&mut state, &engine, &now);
    apply_outcome(&mut state, &engine, &now);
    assert_eq!(
        dispatched_files(&state.take_pending_intents()),
        vec![("/now.md".to_string(), None)]
    );
}

/// 다른 호출자의 요청 결과는 Tools 요청을 건드리지 않는다.
#[test]
fn outcome_for_another_caller_leaves_the_tools_request() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    crate::adapters::ui::new_tab_picker::open(&mut state, &engine.read(), 20, "html", "url", &[]);
    let new_tab = state
        .dialogs
        .file_picker_callers
        .new_tab
        .last()
        .unwrap()
        .request_id;
    crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state)
        .unwrap()
        .result = Some(crate::state::FilePickerResult::Cancelled);
    let (cancelled, _) = crate::app::dispatch::file_picker::take_outcome(&mut state).unwrap();
    crate::adapters::ui::new_tab_picker::apply_outcome(&mut state, &engine, &cancelled);
    open(&mut state, &engine.read());
    let tools = stored(&state).request_id;
    state.take_pending_intents();

    crate::app::dispatch::file_picker::deliver_outcome(
        &mut state,
        &engine,
        None,
        None,
        &confirmed(new_tab, &["/a.html"]),
    );
    let intents = state.take_pending_intents();
    assert!(
        intents.is_empty(),
        "취소된 다른 호출자의 늦은 결과는 실행하지 않는다"
    );
    assert_eq!(stored(&state).request_id, tools);
}

/// 도구 메뉴의 파일 열기 피커는 창 범위로 연다. 포커스 surface 는 시작 위치만 정한다.
#[test]
fn the_tools_menu_picker_opens_in_the_window() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    {
        let engine = engine_session.borrow_mut();
        open(&mut state, &engine.read());
    }
    crate::intent::popup::apply_pending(&mut state, &mut engine_session);
    assert_eq!(
        state
            .popups
            .open_scope(crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID),
        Some(&crate::model::popup_kind::PopupScope::Window)
    );
}
