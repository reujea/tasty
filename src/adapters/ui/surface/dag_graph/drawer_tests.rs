//! 상세(Drawer)에서 나온 조작이 그래프 상태와 반환값에 닿는지 headless egui로 확인한다.
//! 의존성 행을 누르면 선택이 그 노드로 옮겨 가고, 입력 대기 세션 열기는 그 surface id를 돌려준다.

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use tasty_model::DagDirection;

use super::model::{
    DagData, DagEdgeData, DagGraphData, DagNodeData, DagRelation, DagStatus, NodePhase,
    RunnerBadgeData,
};
use super::view::DagGraphView;
use super::{DagChrome, DagTarget, draw_dag_graph};

const SESSION_SURFACE: u32 = 42;

fn node(id: &str, status: DagStatus) -> DagNodeData {
    DagNodeData {
        id: id.to_string(),
        name: id.to_string(),
        status,
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
    }
}

/// `upstream` → `waiter` 두 노드. `waiter`는 agent 세션의 입력을 기다린다.
fn view_with_waiter_selected() -> DagGraphView {
    let mut waiter = node("waiter", DagStatus::Running);
    waiter.incoming = vec![(0, DagRelation::DependsOn)];
    waiter.phase = Some(NodePhase::AwaitingInput {
        provider: "claude".to_string(),
        surface_id: SESSION_SURFACE,
        since: 0,
    });
    let graph = DagGraphData {
        id: "d:drawer".to_string(),
        name: "drawer".to_string(),
        nodes: vec![node("upstream", DagStatus::Succeeded), waiter],
        edges: vec![DagEdgeData {
            from: 0,
            to: 1,
            relation: DagRelation::DependsOn,
            selection: None,
        }],
        cycle: None,
        done: 1,
    };
    let mut view = DagGraphView::default();
    view.data = Some(DagData {
        workspace_id: 1,
        dags: Vec::new(),
        current: Some(graph),
        runner: RunnerBadgeData::default(),
        target_missing: false,
    });
    view.selected = Some("waiter".to_string());
    view
}

fn raw(events: Vec<Event>) -> RawInput {
    RawInput {
        // 상세가 오른쪽 패널로 놓이는 넓은 화면.
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
        focused: true,
        events,
        ..Default::default()
    }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}

/// 한 프레임을 그리고 (세션 열기 결과, 그려진 글자와 그 위치)를 돌려준다.
fn frame(
    ctx: &egui::Context,
    view: &mut DagGraphView,
    events: Vec<Event>,
) -> (Option<u32>, Vec<(String, Rect)>) {
    let mut dag_id = None;
    let mut direction = DagDirection::LeftRight;
    let mut opened = None;
    let out = ctx.run(raw(events), |c| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(c, |ui| {
                let target = DagTarget {
                    dag_id: &mut dag_id,
                    direction: &mut direction,
                };
                opened = draw_dag_graph(ui, target, view, DagChrome::Own { bed_focused: true });
            });
    });
    let texts = out
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect();
    (opened, texts)
}

/// 화면에 그려진 글자 중 `text`와 같은 것의 가운데.
fn center_of(texts: &[(String, Rect)], text: &str) -> Pos2 {
    texts
        .iter()
        .find(|(t, _)| t == text)
        .unwrap_or_else(|| panic!("{text:?} 가 그려지지 않았다: {texts:?}"))
        .1
        .center()
}

/// 누르고 떼는 세 프레임. 마지막 프레임의 세션 열기 결과를 돌려준다.
fn click(ctx: &egui::Context, view: &mut DagGraphView, at: Pos2) -> Option<u32> {
    frame(ctx, view, vec![Event::PointerMoved(at)]);
    frame(ctx, view, vec![button(at, true)]);
    frame(ctx, view, vec![button(at, false)]).0
}

#[test]
fn clicking_a_dependency_row_moves_the_selection_to_that_node() {
    crate::i18n::init("en");
    let ctx = egui::Context::default();
    let mut view = view_with_waiter_selected();
    let (_, texts) = frame(&ctx, &mut view, Vec::new());
    let row = format!("{}  upstream", DagStatus::Succeeded.glyph());
    let at = center_of(&texts, &row);

    let opened = click(&ctx, &mut view, at);

    assert_eq!(view.selected.as_deref(), Some("upstream"));
    assert_eq!(opened, None, "의존성 행은 세션을 열지 않는다");
}

#[test]
fn opening_the_awaiting_session_returns_its_surface_id() {
    crate::i18n::init("en");
    let ctx = egui::Context::default();
    let mut view = view_with_waiter_selected();
    let (_, texts) = frame(&ctx, &mut view, Vec::new());
    let at = center_of(&texts, crate::i18n::t("dag.detail.open_session"));

    let opened = click(&ctx, &mut view, at);

    assert_eq!(opened, Some(SESSION_SURFACE));
    assert_eq!(
        view.selected.as_deref(),
        Some("waiter"),
        "세션 열기는 선택을 바꾸지 않는다"
    );
}
