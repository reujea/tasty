//! 펼친 파일 선택기의 Esc. 경로 메뉴가 닫혀 있어도 펼친 묶음 안의 Esc 는 피커를 닫지 않고 다시 접는다.

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
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let surface = state.focused_surface_id(&read).expect("포커스 surface");
    let iid = state
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
        .expect("파일 선택기");
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

    let mut events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    assert!(
        state.window_escape(&mut events, Some(0), false),
        "창이 Esc 를 처리한다"
    );
    frame(&ctx, &mut state, &read, events);
    frame(&ctx, &mut state, &read, Vec::new());
    let p = state.popups.instance(iid);
    assert!(p.is_some_and(|p| p.open), "피커는 닫히지 않는다");
    assert!(p.is_some_and(|p| p.collapsed()), "피커가 다시 접힌다");
    assert!(
        state.dialogs.finished_file_pickers.is_empty(),
        "요청은 취소되지 않는다"
    );
}
