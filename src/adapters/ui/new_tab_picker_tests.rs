//! "새 HTML..."은 자기 요청을 보관하고, 같은 식별자의 결과로 메뉴를 연 pane에 탭을 만든다.

use super::{apply_outcome, open};
use crate::app::command::DomainIntent;
use crate::intent::{DispatchedIntent, Intent};
use crate::state::{FilePickerOutcome, FilePickerRequestId, NewTabPickerRequest, RequestContext};

const HTML_EXTENSIONS: &[&str] = &["html", "htm", "svg"];

fn confirmed(request_id: FilePickerRequestId, paths: &[&str]) -> FilePickerOutcome {
    FilePickerOutcome {
        request_id,
        paths: paths.iter().map(|p| p.to_string()).collect(),
        cancelled: false,
    }
}

fn stored_request(state: &RequestContext) -> NewTabPickerRequest {
    state
        .dialogs
        .file_picker_callers
        .new_tab
        .clone()
        .expect("caller keeps its request")
}

fn create_tabs(intents: &[DispatchedIntent]) -> Vec<(u32, &str, &serde_json::Value)> {
    intents
        .iter()
        .map(|r| match &r.body {
            Intent::Domain(DomainIntent::CreateTab {
                pane_id,
                kind,
                surface_params,
                ..
            }) => (*pane_id, kind.as_str(), surface_params),
            other => panic!("expected CreateTab, got {other:?}"),
        })
        .collect()
}

#[test]
fn new_html_picker_filters_html_htm_svg_and_keeps_its_pane() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        20,
        "html",
        "url",
        HTML_EXTENSIONS,
    );

    let data = state.dialogs.file_picker.as_ref().expect("picker opened");
    assert_eq!(data.filters, vec!["html", "htm", "svg"]);
    assert!(data.requester.is_none());
    assert!(data.mirror_ws_id.is_none());
    let request = stored_request(&state);
    assert_eq!(request.request_id, data.request_id);
    assert_eq!(
        (
            request.pane_id,
            request.kind.as_str(),
            request.param_key.as_str()
        ),
        (20, "html", "url")
    );
}

#[test]
fn confirmed_outcome_creates_html_tab_in_that_pane() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        20,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let id = stored_request(&state).request_id;
    state.take_pending_intents();

    apply_outcome(&mut state, &engine, &confirmed(id, &["/tmp/x/index.html"]));
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1, "{intents:?}");
    let Intent::Domain(DomainIntent::CreateTab {
        pane_id,
        kind,
        surface_params,
        activate,
        cwd,
        name,
    }) = &intents[0].body
    else {
        panic!("expected CreateTab, got {:?}", intents[0].body);
    };
    assert_eq!(*pane_id, 20);
    assert_eq!(kind, "html");
    assert_eq!(
        surface_params,
        &serde_json::json!({"url": "/tmp/x/index.html"})
    );
    assert!(*activate);
    assert!(cwd.is_none());
    assert!(name.is_none());
    assert!(matches!(
        intents[0].origin,
        crate::intent::IntentOrigin::User {
            source: crate::core::origin::UserSource::ContextMenu
        }
    ));
    assert!(state.dialogs.file_picker_callers.new_tab.is_none());
}

/// 피커는 단일 선택이지만 결과는 목록이다. 경로마다 탭을 하나씩 만든다.
#[test]
fn two_paths_create_two_tabs() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        10,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let id = stored_request(&state).request_id;
    state.take_pending_intents();
    apply_outcome(&mut state, &engine, &confirmed(id, &["/a.html", "/b.svg"]));
    let intents = state.take_pending_intents();
    let urls: Vec<_> = create_tabs(&intents)
        .into_iter()
        .map(|(pane, _, params)| (pane, params["url"].as_str().map(str::to_string)))
        .collect();
    assert_eq!(
        urls,
        vec![
            (10, Some("/a.html".to_string())),
            (10, Some("/b.svg".to_string()))
        ]
    );
}

/// 메뉴를 연 pane이 활성이었어도 피커가 열린 동안 활성 pane이 바뀌면 탭은 메뉴를 연 pane에 생긴다.
#[test]
fn keeps_menu_pane_when_active_pane_changes_during_picker() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    assert_eq!(state.focused_pane_id(&engine), 20);
    open(
        &mut state,
        &engine.read(),
        20,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let id = stored_request(&state).request_id;
    state.select_pane(&engine, 10);
    assert_eq!(state.focused_pane_id(&engine), 10);
    state.take_pending_intents();
    apply_outcome(&mut state, &engine, &confirmed(id, &["/tmp/x/a.svg"]));
    let intents = state.take_pending_intents();
    assert_eq!(create_tabs(&intents)[0].0, 20);
}

#[test]
fn pane_closed_during_picker_creates_nothing() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    let request_id = FilePickerRequestId::next();
    state.dialogs.file_picker_callers.new_tab = Some(NewTabPickerRequest {
        mirror: None,
        request_id,
        pane_id: 999,
        kind: "html".into(),
        param_key: "url".into(),
    });
    assert!(!engine.has_pane(999));
    apply_outcome(&mut state, &engine, &confirmed(request_id, &["/a.html"]));
    assert!(state.take_pending_intents().is_empty());
    assert!(state.dialogs.file_picker_callers.new_tab.is_none());
}

#[test]
fn cancelled_outcome_creates_nothing_and_releases_the_request() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        20,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let request_id = stored_request(&state).request_id;
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
    assert!(state.dialogs.file_picker_callers.new_tab.is_none());
}

/// 이전 요청의 결과는 다시 연 요청에 적용하지 않는다.
#[test]
fn outcome_of_another_request_is_ignored() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    let engine = engine_session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        20,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let first = stored_request(&state).request_id;
    state.dialogs.file_picker.as_mut().unwrap().result =
        Some(crate::state::FilePickerResult::Cancelled);
    let (cancelled, _) = crate::app::dispatch::file_picker::take_outcome(&mut state).unwrap();
    apply_outcome(&mut state, &engine, &cancelled);
    open(
        &mut state,
        &engine.read(),
        10,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let second = stored_request(&state).request_id;
    assert_ne!(first, second);
    state.take_pending_intents();

    apply_outcome(&mut state, &engine, &confirmed(first, &["/late.html"]));
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(stored_request(&state).request_id, second);

    apply_outcome(&mut state, &engine, &confirmed(second, &["/now.html"]));
    let intents = state.take_pending_intents();
    assert_eq!(create_tabs(&intents)[0].0, 10);
}

/// mirror pane에서는 피커를 열지 않고 원격 열기 불가 토스트만 띄운다.
#[test]
fn mirror_pane_does_not_open_picker() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let ws = engine.workspace_at(0).expect("mirror workspace");
    assert!(ws.mirror);
    let pane_id = ws.pane_layout().all_pane_ids()[0];

    open(
        &mut state,
        &engine.read(),
        pane_id,
        "html",
        "url",
        HTML_EXTENSIONS,
    );

    assert!(state.dialogs.file_picker.is_none());
    assert!(state.dialogs.file_picker_callers.new_tab.is_none());
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.toasts.len(), 1);
}

#[path = "new_tab_picker_tests/remote.rs"]
mod remote;

/// 새 탭 메뉴의 피커는 메뉴를 연 pane 범위로 연다.
#[test]
fn the_new_tab_picker_opens_in_the_pane_of_its_menu() {
    let (mut state, mut engine_session) = crate::state::tests::two_pane_fixture();
    {
        let engine = engine_session.borrow_mut();
        open(
            &mut state,
            &engine.read(),
            20,
            "html",
            "url",
            HTML_EXTENSIONS,
        );
    }
    crate::intent::popup::apply_pending(&mut state, &mut engine_session);
    assert_eq!(
        state
            .popups
            .open_scope(crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID),
        Some(&crate::model::popup_kind::PopupScope::Pane(20))
    );
}
