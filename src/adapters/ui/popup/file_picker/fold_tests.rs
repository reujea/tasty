//! 파일 선택기의 Esc. 본문은 Esc 를 읽지 않고 선언대로 중앙 단계가 닫는다. 경로 메뉴가 닫혀 있어도 펼친 묶음 안의
//! Esc 는 피커를 닫지 않고 다시 접는다.

use super::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::{PopupFit, PopupPayload, defs};

fn data() -> crate::state::FilePickerData {
    crate::state::FilePickerData {
        save_name: None,
        folder: false,
        request_id: crate::state::FilePickerRequestId(1),
        mirror_ws_id: None,
        remote_host: None,
        current_dir: "/tmp".to_string(),
        load: crate::state::FpLoadState::Loaded,
        local_query: None,
        entries: vec![],
        selected: vec![],
        result: None,
        requester: None,
        filters: vec![],
    }
}

/// 팝업 층 한 프레임(제품 `draw_popups`, 실제 정의 본문과 닫기 훅 포함).
fn frame(
    ctx: &egui::Context,
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    events: Vec<egui::Event>,
) {
    let term = crate::model::PhysicalRect {
        x: crate::model::PhysicalPx(0.0),
        y: crate::model::PhysicalPx(0.0),
        width: crate::model::PhysicalPx(1600.0),
        height: crate::model::PhysicalPx(900.0),
    };
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    state.popups.begin_key_pass(false);
    drop(ctx.run(raw, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, engine, &[], term, 1.0);
    }));
    state.close_input_pass(ctx);
}

/// T-F1-1: Given 포커스 surface 에서 좁아 접힌 뒤 펼친 키 주인 파일 선택기(경로 메뉴 닫힘), When 창을 거친
/// 수식키 없는 Esc 가 오면, Then 피커는 열린 채 다시 접히고 결과(취소)는 생기지 않는다.
#[test]
fn escape_on_an_expanded_file_picker_folds_it_and_keeps_the_request() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_picker(&mut state, &read);
    if let Some(p) = state.popups.instance_mut(iid) {
        p.arrange.fit = PopupFit::Collapse;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(800.0, 1600.0));
    }
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Vec::new());
    assert!(
        state.popups.instance(iid).is_some_and(|p| p.collapsed()),
        "좁아 접힌다"
    );
    state.popups.expand(iid);
    frame(&ctx, &mut state, &read, Vec::new());
    assert!(
        state.popups.instance(iid).is_some_and(|p| p.expanded()),
        "펼쳤다"
    );

    escape_through_the_window(&ctx, &mut state, &read);
    let p = state.popups.instance(iid);
    assert!(p.is_some_and(|p| p.open), "피커는 닫히지 않는다");
    assert!(p.is_some_and(|p| p.collapsed()), "피커가 다시 접힌다");
    assert!(
        state.dialogs.finished_file_pickers.is_empty(),
        "요청은 취소되지 않는다"
    );
}

/// 정의를 등록하고 포커스 surface 범위의 키 주인 파일 선택기를 연다.
fn open_picker(
    state: &mut crate::state::MainViewState,
    read: &crate::runtime::engine_read::EngineRead<'_>,
) -> crate::adapters::ui::popup::PopupInstanceId {
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let surface = state.focused_surface_id(read).expect("포커스 surface");
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(
                FILE_PICKER_POPUP_ID,
                crate::model::popup_kind::PopupScope::Surface(surface),
            )
            .centered()
            .focused()
            .with_payload(PopupPayload::FilePicker(Box::new(data()))),
            true,
        )
        .expect("파일 선택기")
}

/// 창을 거친 수식키 없는 Esc 한 패스와 빈 패스 하나.
fn escape_through_the_window(
    ctx: &egui::Context,
    state: &mut crate::state::MainViewState,
    read: &crate::runtime::engine_read::EngineRead<'_>,
) {
    let mut events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    assert!(
        crate::adapters::ui::popup::window_input_test_support::window_escape_step(
            state,
            read,
            &mut events,
            false
        ),
        "창이 Esc 를 처리한다"
    );
    frame(ctx, state, read, events);
    frame(ctx, state, read, Vec::new());
}

/// M-P1-1 F: Given 펼치지 않은 키 주인 파일 선택기(경로 메뉴 닫힘), When 창을 거친 수식키 없는 Esc 가 오면, Then
/// 본문은 Esc 를 읽지 않고 선언(`PopupEscape::Close`)대로 중앙 단계가 닫으며 요청은 취소로 끝난다.
#[test]
fn escape_on_the_picker_closes_it_by_its_declaration() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_picker(&mut state, &read);
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Vec::new());
    frame(&ctx, &mut state, &read, Vec::new());
    assert!(
        state.popups.instance(iid).is_some_and(|p| p.open),
        "열려 있다"
    );
    escape_through_the_window(&ctx, &mut state, &read);
    assert!(
        state.popups.instance(iid).is_none_or(|p| !p.open),
        "피커가 닫힌다"
    );
    let results: Vec<_> = state
        .dialogs
        .finished_file_pickers
        .iter()
        .map(|f| matches!(f.result, crate::state::FilePickerResult::Cancelled))
        .collect();
    assert_eq!(results, vec![true], "요청은 취소로 한 번 끝난다");
}
