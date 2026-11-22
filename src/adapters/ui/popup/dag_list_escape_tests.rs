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
    assert!(state.window_escape(&mut events, Some(0), false));
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    // 창은 그 Esc 를 egui 입력에서 뺐다. 팝업 층이 묶인 키 주인 본문에 넘긴다.
    assert!(events.is_empty());
    frame(&ctx, &mut state, &read, events);
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    assert_eq!(view(&state), Some(DrillDownView::List));
}

/// 상세 화면을 보는 키 주인 DAG 목록 팝업 장면. 한 번 그린 뒤 상세로 바꾼다.
fn detail_scene() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
    egui::Context,
) {
    crate::i18n::init("en");
    let (mut state, session) = crate::state::tests::test_state();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state.popups.open_at_focused(
        DAG_LIST_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    let ctx = egui::Context::default();
    {
        let mut session = session;
        let engine = session.borrow_mut();
        frame(&ctx, &mut state, &engine.read(), Vec::new());
        drop(engine);
        for p in state.popups.entries_of_mut(DAG_LIST_POPUP_ID) {
            p.payload = PopupPayload::DagList(Box::new(DagListState {
                view: DrillDownView::Detail,
                ..Default::default()
            }));
        }
        assert_eq!(view(&state), Some(DrillDownView::Detail));
        (state, session, ctx)
    }
}

/// 창 입력 큐의 다음 패스를 꺼내 그린다(`WindowInput::take_pass` → 팝업 층 → 패스 닫기).
fn window_frame(
    ctx: &egui::Context,
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    input: &mut crate::adapters::ui::popup::window_input_test_support::WindowInput,
) {
    let events = input.take_pass(state, ctx);
    frame(ctx, state, engine, events);
    state.close_input_pass(ctx);
}

fn dag_instance(
    state: &crate::state::MainViewState,
) -> crate::adapters::ui::popup::PopupInstanceId {
    state
        .popups
        .instances_of(DAG_LIST_POPUP_ID)
        .next()
        .expect("DAG 목록 팝업")
        .instance
}

/// T-4a3-K3a: Given 상세 화면, When 한 몫에 수식키 없는 Esc 가 오고 그 뒤 Super 를 눌러 그리기 시점에 Super 가 눌려
/// 있으면, Then 창이 넘긴 그 Esc 로 목록으로 돌아가고 팝업은 열려 있다. 일치와 소비 집합이 모두 창이 사건 시점에 정한
/// 표지 값을 쓴다.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_bare_escape_returns_from_the_detail_even_if_super_is_down_at_draw() {
    let (mut state, mut session, ctx) = detail_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let mut input = crate::adapters::ui::popup::window_input_test_support::WindowInput::default();
    assert!(
        input.escape(&ctx, &mut state, &read, true),
        "창이 그 Esc 를 넘긴다"
    );
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::SUPER);
    assert!(
        tasty_key_match::super_held(&ctx),
        "그리기 시점에 Super 가 눌려 있다"
    );
    window_frame(&ctx, &mut state, &read, &mut input);
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    assert_eq!(view(&state), Some(DrillDownView::List));
}

/// T-4a3-K3b: Given 상세 화면, When Super 를 누른 채 Esc 를 누르고(창 Esc 가 아니다) 그리기 전에 Super 를 떼면,
/// Then 그리기 시점 값으로는 수식키 없는 Esc 로 보여도 상세에 머물고 팝업은 열려 있다.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_super_escape_does_not_leave_the_detail_after_super_is_released() {
    let (mut state, mut session, ctx) = detail_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let mut input = crate::adapters::ui::popup::window_input_test_support::WindowInput::default();
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::SUPER);
    assert!(
        !input.escape(&ctx, &mut state, &read, true),
        "창이 처리하지 않는다"
    );
    assert!(input.escape_fed(), "그 사건이 egui 큐에 남았다");
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::empty());
    assert!(
        !tasty_key_match::super_held(&ctx),
        "그리기 시점에 Super 가 놓였다"
    );
    window_frame(&ctx, &mut state, &read, &mut input);
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    assert_eq!(view(&state), Some(DrillDownView::Detail));
}

/// T-4a3-9: Given 상세 화면, When Esc 로 목록으로 돌아오면, Then 키는 계속 DAG 목록이 받고(포커스 노드), 다음 Esc 에
/// 선언대로 닫힌다.
#[test]
fn the_list_keeps_the_keys_after_escape_leaves_the_detail() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session, ctx) = detail_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = dag_instance(&state);
    let mut input = crate::adapters::ui::popup::window_input_test_support::WindowInput::default();
    assert!(input.escape(&ctx, &mut state, &read, true));
    window_frame(&ctx, &mut state, &read, &mut input);
    assert_eq!(view(&state), Some(DrillDownView::List));
    assert_eq!(
        state.focus_node(&read),
        FocusNode::Popup(PopupNode::Host(iid)),
        "목록이 키를 받는다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    window_frame(&ctx, &mut state, &read, &mut input);
    assert!(
        !state.popups.is_open(DAG_LIST_POPUP_ID),
        "둘째 Esc 에 닫힌다"
    );
}

/// T-4a3-14: Given 상세 화면에서 그래프 노드가 선택돼 있고 포인터가 그래프 위에 있을 때, When Shift+Esc 를 누르면
/// (창 Esc 가 아니다), Then 상세에 머물고 그래프 hover 읽기가 선택만 푼다.
#[test]
fn shift_escape_over_the_graph_clears_only_the_selection() {
    use crate::adapters::ui::surface::dag_graph::model::{
        DagData, DagGraphData, DagNodeData, DagStatus, RunnerBadgeData,
    };
    let (mut state, mut session, ctx) = detail_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let node = |id: &str| DagNodeData {
        id: id.to_string(),
        name: id.to_string(),
        status: DagStatus::Running,
        command_kind: "agent",
        on_failure_kind: "abort",
        command_text: String::new(),
        started_at: None,
        finished_at: None,
        exit_code: None,
        error_tail: None,
        output_tail: None,
        incoming: Vec::new(),
        skip: None,
        phase: None,
        unknown_reason: None,
    };
    let data = DagData {
        workspace_id: 1,
        dags: Vec::new(),
        current: Some(DagGraphData {
            id: "d:esc".to_string(),
            name: "esc".to_string(),
            nodes: vec![node("only")],
            edges: Vec::new(),
            cycle: None,
            done: 0,
        }),
        runner: RunnerBadgeData::default(),
        target_missing: false,
    };
    let set_graph = |state: &mut crate::state::MainViewState, select: bool| {
        for p in state.popups.entries_of_mut(DAG_LIST_POPUP_ID) {
            if let PopupPayload::DagList(dag) = &mut p.payload {
                dag.open_workspace = Some(1);
                dag.open_dag = Some("d:esc".to_string());
                dag.graph.source.data = Some(data.clone());
                if select {
                    dag.graph.canvas.selected = Some("only".to_string());
                }
            }
        }
    };
    let selected = |state: &crate::state::MainViewState| {
        state
            .popups
            .instances_of(DAG_LIST_POPUP_ID)
            .find_map(|p| match &p.payload {
                PopupPayload::DagList(dag) => Some(dag.graph.canvas.selected.clone()),
                _ => None,
            })
            .flatten()
    };
    // 첫 그리기가 조회 대상을 정한다(그때 데이터를 비운다). 그 뒤 데이터와 선택을 둔다.
    set_graph(&mut state, false);
    frame(&ctx, &mut state, &read, Vec::new());
    set_graph(&mut state, true);
    let rect = state
        .popups
        .instances_of(DAG_LIST_POPUP_ID)
        .next()
        .unwrap()
        .popup_rect();
    // 좁은 팝업에서는 선택 노드의 상세가 아래에 놓인다. 포인터는 back bar 아래 그래프 왼쪽 위에 둔다.
    let over_graph = egui::pos2(rect.left() + 40.0, rect.top() + 110.0);
    frame(
        &ctx,
        &mut state,
        &read,
        vec![egui::Event::PointerMoved(over_graph)],
    );
    assert_eq!(selected(&state), Some("only".to_string()), "선택이 있다");
    let mut input = crate::adapters::ui::popup::window_input_test_support::WindowInput::default();
    assert!(
        !input.escape_with(&ctx, &mut state, &read, true, egui::Modifiers::SHIFT),
        "Shift+Esc 는 창 Esc 가 아니다"
    );
    window_frame(&ctx, &mut state, &read, &mut input);
    assert!(state.popups.is_open(DAG_LIST_POPUP_ID));
    assert_eq!(view(&state), Some(DrillDownView::Detail), "상세에 머문다");
    assert_eq!(selected(&state), None, "그래프 선택이 풀린다");
}
