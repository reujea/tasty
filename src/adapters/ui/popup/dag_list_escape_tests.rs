//! 상세 화면의 Esc. 창이 키 주인 팝업의 포커스를 풀어도(바깥 클릭 Unfocus 선언) 같은 Esc 는 입력 시점의 키
//! 주인 본문이 받아 상세에서 목록으로 돌아간다.

use super::{DAG_LIST_POPUP_ID, DagListState, DrillDownView};
use crate::adapters::ui::popup::{PopupPayload, defs};

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
    drop(ctx.run(raw, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, engine, &[], term, 1.0);
    }));
}

fn view(state: &crate::state::MainViewState) -> Option<DrillDownView> {
    state
        .popups
        .instances_of(DAG_LIST_POPUP_ID)
        .find_map(|p| match &p.payload {
            PopupPayload::DagList(dag) => Some(dag.view),
            _ => None,
        })
}

/// Given 상세 화면을 보는 키 주인 DAG 목록 팝업, When 창이 Esc 를 처리하면(포커스 해제, 닫지 않음), Then 같은
/// Esc 로 본문이 목록으로 돌아가고 팝업은 열려 있다.
#[test]
fn one_escape_on_the_detail_view_returns_to_the_list() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state.popups.open_at_focused(
        DAG_LIST_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Vec::new());
    for p in state.popups.entries_of_mut(DAG_LIST_POPUP_ID) {
        p.payload = PopupPayload::DagList(Box::new(DagListState {
            view: DrillDownView::Detail,
            ..Default::default()
        }));
    }
    assert_eq!(view(&state), Some(DrillDownView::Detail));

    // 창의 Esc 판정(포커스 해제 갈래). 바깥 클릭 Unfocus 선언이라 닫지 않는다.
    let mut events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    assert!(state.window_escape(&mut events));
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    // 창은 그 Esc 를 egui 입력에서 뺐다. 팝업 층이 묶인 키 주인 본문에 넘긴다.
    assert!(events.is_empty());
    frame(&ctx, &mut state, &read, events);
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    assert_eq!(view(&state), Some(DrillDownView::List));
}
