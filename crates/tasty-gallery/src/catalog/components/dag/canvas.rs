//! 본체와 같은 공통 그래프 캔버스(`tasty_ui_widgets::draw_graph_canvas`)에 표본 그래프를 넣는 DAG 예제.
//! 표본은 매 프레임 새 뷰 상태로 그리므로 열릴 때처럼 그래프를 화면에 맞춘다. 선택만 프레임 사이에
//! 유지하고, 팬·줌 제스처와 줌 버튼 동작은 본체에서 확인한다.

use tasty_dag_layout::{GraphLayout, Orientation};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    EdgeInk, GraphCanvasProps, GraphChromeLabels, GraphNodeVisual, GraphPainter, GraphView,
    draw_graph_canvas, paint_dag_card,
};

use super::chrome;
use super::node;
use super::{Graph, Node, Plot, PlotEdge};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 표본 그래프의 카드·간선 잉크. 본체 DAG painter의 규칙을 표본 상태로 다시 쓴 것이다.
pub struct SamplePainter;

impl GraphPainter<Plot<'_>> for SamplePainter {
    fn dimmed_nodes(&self, plot: &Plot<'_>) -> Vec<bool> {
        plot.graph.nodes.iter().map(|n| n.status.is_dim()).collect()
    }

    fn edge_dimmed(&self, plot: &Plot<'_>, edge: &PlotEdge) -> bool {
        plot.graph.edge_is_dim(edge.from, edge.to)
    }

    fn edge_ink(&self, theme: &Theme, edge: &PlotEdge, dim: bool, highlight: bool) -> EdgeInk {
        let color = if highlight {
            theme.dag_edge_highlight()
        } else if dim {
            theme.dag_edge_dim()
        } else {
            edge.rel.color(theme)
        };
        EdgeInk {
            color: color.to_egui(),
            width: theme.dag_edge_width(),
            dash: edge.rel.dash(),
        }
    }

    fn paint_node(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        theme: &Theme,
        rect: egui::Rect,
        n: &Node,
        vis: &GraphNodeVisual,
    ) {
        node::with_card(n, |card| {
            paint_dag_card(ui, painter, theme, rect, card, vis)
        });
    }

    fn hover_text(&self, n: &Node) -> String {
        n.hover_text()
    }

    fn minimap_dimmed(&self, n: &Node) -> bool {
        n.status.is_dim()
    }
}

/// 본체와 같은 번역 키로 만든 캔버스 조작 문구.
pub fn chrome_labels() -> GraphChromeLabels<'static> {
    GraphChromeLabels {
        zoom_in: t("dag.zoom.in"),
        zoom_out: t("dag.zoom.out"),
        fit: t("dag.zoom.fit"),
        direction: t("dag.zoom.direction"),
        lod_compact: t("dag.lod.compact"),
        lod_block: t("dag.lod.block"),
    }
}

/// 레이아웃에 넘긴 카드 크기.
pub fn node_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.dag_node_width().value(),
        theme.dag_node_height().value(),
    )
}

/// 캔버스가 열릴 때 공통 뷰가 고르는 배율.
pub fn fit_zoom(viewport: egui::Vec2, layout: &GraphLayout, theme: &Theme) -> f32 {
    let mut view = GraphView::default();
    view.fit(
        egui::vec2(layout.width.value(), layout.height.value()),
        viewport,
        theme.dag_canvas_padding(),
    );
    view.zoom
}

/// 캔버스 한 장. `selected` 는 호출자가 소유한 선택 상태다.
// 이유: 본체 surface·팝업이 넘기는 틀 값(줌 버튼 소유·바탕)을 표본마다 인자로 고른다.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    graph: &Graph,
    dir: Orientation,
    selected: &mut Option<String>,
    // 팝업 상세에서는 뒤로 가기 바에 조작 도구가 있으므로 false로 둔다.
    zoom_cluster: bool,
    // 탭 surface는 포커스 바탕을 넘긴다. 워크스페이스 팝업은 surface가 아니라 None이다.
    bed: Option<egui::Color32>,
) {
    let mut view = GraphView::default();
    view.selected = selected.clone();
    let mut layer = canvas_child(ui, rect, &graph.id);
    paint_view(&mut layer, theme, graph, dir, &mut view, zoom_cluster, bed);
    *selected = view.selected;
}

/// `rect` 에 갇힌 캔버스용 Ui.
fn canvas_child(ui: &mut egui::Ui, rect: egui::Rect, salt: &str) -> egui::Ui {
    let mut layer = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("dag_canvas", salt))
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    layer.set_clip_rect(rect);
    layer
}

/// 남은 자리 전체에 공통 캔버스를 그린다. 뷰 상태는 호출자가 정한다.
fn paint_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    graph: &Graph,
    dir: Orientation,
    view: &mut GraphView,
    zoom_cluster: bool,
    bed: Option<egui::Color32>,
) {
    let rect = ui.available_rect_before_wrap();
    let plot = Plot::new(graph);
    let layout = view.layout(Some(&plot), &super::layout_config(theme, dir));
    let labels = chrome_labels();
    let props = GraphCanvasProps {
        canvas_bg: bed.unwrap_or_else(|| theme.dag_canvas_bg().to_egui()),
        layout: &layout,
        orientation: dir,
        node_size: node_size(theme),
        own_zoom_cluster: zoom_cluster,
        labels: &labels,
    };
    // 표본은 매 프레임 새 뷰 상태로 그리므로 줌 버튼 동작은 적용할 상태가 없다.
    if let Some(action) = draw_graph_canvas(ui, theme, view, &plot, &SamplePainter, &props) {
        tracing::trace!("gallery dag canvas ignores {action:?}");
    }
    if let Some(cycle) = &graph.cycle {
        chrome::paint_cycle_banner(ui, theme, rect, cycle);
    }
}

/// 상태별 표본 한 칸의 뷰. `zoom` 이 있으면 자동 맞춤 대신 그 배율로 고정한다.
fn specimen_view(
    graph: &Graph,
    dir: Orientation,
    viewport: egui::Vec2,
    zoom: Option<f32>,
    selected: Option<&str>,
    theme: &Theme,
) -> GraphView {
    let mut view = GraphView::default();
    view.selected = selected.map(str::to_owned);
    if let Some(z) = zoom {
        // 같은 키로 맞춤을 먼저 소비해 캔버스가 배율을 덮어쓰지 않게 한다.
        if view.take_fit(&graph.id, dir, viewport) {
            let pad = theme.dag_canvas_padding().value();
            view.zoom = z;
            view.offset = egui::vec2(pad, pad);
        }
    }
    view
}

/// 상태 표본 한 칸 — 캡션 아래에 공통 그래프 뷰 한 장. 선택이 있으면 상세 Drawer 를 붙인다.
fn state_tile(
    ui: &mut egui::Ui,
    theme: &Theme,
    caption: &str,
    graph: &Graph,
    size: egui::Vec2,
    zoom: Option<f32>,
    selected: Option<&str>,
) {
    spec::cluster(ui, theme, caption, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let mut tile = canvas_child(ui, rect, caption);
        let dir = Orientation::TopDown;
        let drawer = selected.map(|id| {
            move |ui: &mut egui::Ui| {
                tasty_ui_widgets::drawer_body(ui, theme, |ui| {
                    super::detail::draw_body(ui, theme, graph, id)
                })
            }
        });
        tasty_ui_widgets::split_graph_drawer(
            &mut tile,
            theme,
            LogicalPx(rect.width()),
            drawer,
            |ui| {
                let viewport = ui.available_size();
                let mut view = specimen_view(graph, dir, viewport, zoom, selected, theme);
                paint_view(ui, theme, graph, dir, &mut view, true, None);
            },
        );
    });
}

/// `graph-view` 섹션 Spec — 공통 그래프 뷰의 상태별 모습.
pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    let large = super::dense_dag();
    let mut small = super::build_dag();
    small.id = "small".into();
    small.nodes.truncate(3);
    let mut empty = super::build_dag();
    empty.id = "empty".into();
    empty.nodes.clear();
    let wide = theme.dag_popup_width().value();
    let h = theme.measure_sm.value();
    let narrow = super::surface::NARROW_STAGE_WIDTH.value();
    // 상세가 우측 패널로 놓이는 가장 좁은 폭.
    let side = tasty_ui_widgets::GRAPH_NARROW_DETAIL_SHEET.value();
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        let tile = egui::vec2(narrow, h);
        state_tile(ui, theme, "empty", &empty, tile, None, None);
        state_tile(ui, theme, "small graph", &small, tile, None, None);
        state_tile(
            ui,
            theme,
            "large graph",
            &large,
            egui::vec2(wide, h),
            None,
            None,
        );
        state_tile(
            ui,
            theme,
            "selection + detail",
            &super::build_dag(),
            egui::vec2(side, h),
            None,
            Some("unit"),
        );
        state_tile(
            ui,
            theme,
            "narrow + detail sheet",
            &super::build_dag(),
            egui::vec2(narrow, h),
            None,
            Some("unit"),
        );
        for (caption, zoom) in [
            ("lod full 100%", 1.0),
            ("lod compact 50%", 0.5),
            ("lod block 30%", 0.3),
        ] {
            state_tile(ui, theme, caption, &small, tile, Some(zoom), None);
        }
    });
    spec::note(
        ui,
        theme,
        "The host DAG surface, the DAG popup and these specimens all draw through the shared graph \
         view in tasty-ui-widgets. Only the data source, card content and drawer body differ.",
    );
}

/// Tight 예제 영역에 지정한 높이로 캔버스를 그린다.
pub fn stage(ui: &mut egui::Ui, theme: &Theme, graph: &Graph, dir: Orientation, height: f32) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        let key = ui.id().with(("dag_sel", &graph.id));
        let mut sel: Option<String> = ui.data(|d| d.get_temp(key)).unwrap_or(None);
        paint(ui, theme, rect, graph, dir, &mut sel, true, None);
        ui.data_mut(|d| d.insert_temp(key, sel));
    });
}

/// 캔버스 무대의 표준 높이 — 시안 460.
fn tall(theme: &Theme) -> f32 {
    theme.dag_popup_height().value()
}

/// `canvas` 섹션 Spec — 읽기 전용 관찰 캔버스.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::build_dag();
    stage(ui, theme, &graph, Orientation::TopDown, tall(theme));
    spec::meta(
        ui,
        theme,
        &[
            ("node box", "168 × 48 (fixed at every zoom)"),
            ("layer gap", "32 along flow · 24 across"),
            ("zoom", "20% – 150%, 10% steps"),
            ("fit", "auto on open, capped at 100%"),
            ("background", "dot grid · 16px"),
            ("poll", "0.5s — colour only, never re-layout"),
        ],
        &[
            TokenChip::new(
                "dag-canvas-bg",
                "canvas bed",
                theme.dag_canvas_bg().to_egui(),
            ),
            TokenChip::new(
                "dag-canvas-dot",
                "grid mark",
                theme.dag_canvas_dot().to_egui(),
            ),
            TokenChip::without_color("dag-layer-gap", "32 flow depth"),
            TokenChip::without_color("dag-sibling-gap", "24 across"),
            TokenChip::without_color("dag-edge-corner-radius", "elbow"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Position stability is the contract that makes a 0.5s poll survivable: layout is computed \
         from ids + dependency edges only — never from status, duration or counts — so a task \
         turning running → succeeded repaints one card and moves nothing.",
    );
    spec::dont(
        ui,
        theme,
        "Never add a connect handle, a port dot, or a drag-to-move affordance. Anything that looks \
         editable promises an edit the host cannot honour — DAGs are built by agents over CLI/IPC.",
    );
}

/// LOD Spec 안에 들어가는 55 노드 무대.
pub fn dense_stage(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::dense_dag();
    stage(
        ui,
        theme,
        &graph,
        Orientation::TopDown,
        theme.measure_sm.value(),
    );
}

/// 사이클 Spec 안에 들어가는 무대 — 배너가 떠도 그래프는 계속 그린다.
pub fn cycle_stage(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::cycle_dag();
    stage(
        ui,
        theme,
        &graph,
        Orientation::TopDown,
        theme.measure_sm.value(),
    );
}
