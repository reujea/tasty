//! 최상위 조립 — 헤더 + (사이클 배너) + 공통 그래프 캔버스 + 상세.
//!
//! 상세는 넓은 화면에서 우측 고정폭 패널, 좁은 화면에서 하단 시트다. 자리는 공통
//! 그래프 뷰가 나누고 **콘텐츠는 어느 쪽이든 [`super::detail::draw_detail`] 하나**다.

use tasty_ui_widgets::{
    GraphCanvasProps, GraphData as _, GraphFrame, draw_graph_canvas, split_graph_drawer,
};

use super::canvas::DagPainter;
use super::chrome::{self, ChromeAction, graph_chrome_labels};
use super::detail::{DetailAction, draw_detail};
use super::view::{DagGraphView, DagTarget, layout_config};
use crate::adapters::ui::surface::graph_source::GraphSource as _;

/// DAG 그래프를 담는 틀. surface는 자체 헤더·줌 버튼을 그리고 팝업은 back bar의 버튼 동작을 넘긴다.
pub type DagChrome = GraphFrame<ChromeAction>;

/// 공용 그래프 렌더링. 화면 폭에 따라 상세를 오른쪽 또는 아래에 배치한다.
/// 사용자가 상세에서 세션 열기를 누르면 그 세션의 surface id 를 돌려준다. 포커스는 호출부가 옮긴다.
pub fn draw_dag_graph(
    ui: &mut egui::Ui,
    target: DagTarget<'_>,
    view: &mut DagGraphView,
    frame: DagChrome,
) -> Option<u32> {
    let th = crate::theme::theme();
    let theme = &th;
    ui.set_min_size(ui.available_size());
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let canvas_bg = frame.canvas_bg(theme);
    let own_zoom_cluster = frame.owns_zoom_cluster();

    let Some(data) = view.source.data.clone() else {
        ui.painter().rect_filled(ui.max_rect(), 0.0, canvas_bg);
        return None;
    };

    let surface_width = ui.available_width();
    let now = now_ms();

    let chrome_action = match frame {
        GraphFrame::Surface { .. } => chrome::draw_header(ui, theme, &data, surface_width),
        GraphFrame::Popup(pending) => pending,
    };

    if chrome::is_empty(data.current.as_ref()) {
        chrome::draw_empty(ui, theme, canvas_bg, &data, target.dag_id.as_deref());
        apply_chrome(
            target,
            view,
            chrome_action,
            egui::Vec2::ZERO,
            egui::Vec2::ZERO,
            theme,
        );
        return None;
    }
    let graph = data
        .current
        .as_ref()
        .expect("non-empty graph checked above");

    if let Some(cycle) = &graph.cycle {
        chrome::draw_cycle_banner(ui, theme, cycle);
    }

    let cfg = layout_config(theme, *target.direction);
    let layout = view.canvas.layout(Some(graph), &cfg);
    let graph_size = egui::vec2(layout.width.value(), layout.height.value());

    let selected = view
        .canvas
        .selected
        .as_deref()
        .and_then(|id| graph.index_of(id))
        .and_then(|i| graph.nodes.get(i));

    let labels = graph_chrome_labels();
    let painter = DagPainter::new(graph, now);
    let props = GraphCanvasProps {
        canvas_bg,
        layout: &layout,
        orientation: cfg.orientation,
        node_size: egui::vec2(cfg.node_size.0.value(), cfg.node_size.1.value()),
        own_zoom_cluster,
        labels: &labels,
    };
    let canvas_view = &mut view.canvas;
    let split = split_graph_drawer(
        ui,
        theme,
        surface_width,
        selected.map(|node| move |ui: &mut egui::Ui| draw_detail(ui, theme, graph, node, now)),
        |ui| draw_graph_canvas(ui, theme, canvas_view, graph, &painter, &props),
    );
    let viewport = split.viewport;
    let canvas_action = split.canvas.map(ChromeAction::from);

    let mut open_session = None;
    match split.drawer.flatten() {
        Some(DetailAction::Select(id)) => view.canvas.selected = Some(id),
        Some(DetailAction::Close) => view.canvas.selected = None,
        Some(DetailAction::OpenSession(surface)) => open_session = Some(surface),
        None => {}
    }
    apply_chrome(
        target,
        view,
        canvas_action.or(chrome_action),
        graph_size,
        viewport,
        theme,
    );
    open_session
}

fn apply_chrome(
    target: DagTarget<'_>,
    view: &mut DagGraphView,
    action: Option<ChromeAction>,
    graph_size: egui::Vec2,
    viewport: egui::Vec2,
    theme: &tasty_type_appearance::theme::Theme,
) {
    match action {
        Some(ChromeAction::SelectDag(id)) => {
            // 사용자가 고른 DAG로 대상을 고정하고 다음 프레임에 즉시 조회한다.
            *target.dag_id = Some(id);
            view.canvas.selected = None;
            view.source.invalidate_poll();
        }
        Some(ChromeAction::ToggleDirection) => {
            // 방향이 바뀌어도 선택은 유지하고 자동 맞춤은 다음 프레임에 다시 계산한다.
            *target.direction = target.direction.toggled();
        }
        Some(ChromeAction::Fit) => {
            view.canvas
                .fit(graph_size, viewport, theme.dag_canvas_padding().value());
        }
        Some(ChromeAction::Zoom(steps)) => {
            view.canvas.zoom_by(steps, viewport / 2.0);
        }
        Some(ChromeAction::Refresh) => {
            view.source.invalidate_poll();
        }
        None => {}
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
