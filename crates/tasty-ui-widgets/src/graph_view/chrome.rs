//! 캔버스 위의 줌·맞춤·방향 버튼, 미니맵, LOD 칩.
//! 폭이 좁으면 줌 숫자·미니맵부터 숨긴다. 문구는 호출자가 props로 넘긴다.

use tasty_dag_layout::{GraphLayout, Orientation};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::canvas::GraphPainter;
use super::model::GraphData;
use super::state::{GraphView, Lod, ZOOM_MAX, ZOOM_MIN};

/// 캔버스 조작 버튼에서 나온 동작.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GraphChromeAction {
    ToggleDirection,
    Fit,
    /// 줌 단계 변경. 앵커는 **캔버스 중심**이어야 하므로 여기서 적용하지 않고
    /// 캔버스 크기를 아는 호출자에게 넘긴다.
    Zoom(f32),
}

/// 캔버스 조작 버튼 툴팁과 LOD 칩 문구.
pub struct GraphChromeLabels<'a> {
    pub zoom_in: &'a str,
    pub zoom_out: &'a str,
    pub fit: &'a str,
    pub direction: &'a str,
    /// 카드가 글리프와 이름만 보일 때 칩 문구.
    pub lod_compact: &'a str,
    /// 카드가 색만 보일 때 칩 문구.
    pub lod_block: &'a str,
}

/// 줌 퍼센트 판독창 폭. 대응 디자인 토큰은 없다.
const ZOOM_READOUT_WIDTH: LogicalPx = LogicalPx(46.0);
/// 이 폭보다 좁으면 줌 숫자를 숨긴다. 대응 디자인 토큰은 없다.
const NARROW_ZOOM_LABEL: LogicalPx = LogicalPx(400.0);
/// 상세를 우측 패널로 둘 수 있는 최소 폭. 그 아래는 하단 시트다. 대응 디자인 토큰은 없다.
pub const GRAPH_NARROW_DETAIL_SHEET: LogicalPx = LogicalPx(640.0);

/// 줌 클러스터 전체 크기 — 버튼 4 개 + (판독창) + 1px 구분선.
pub fn zoom_cluster_size(theme: &Theme, compact: bool) -> egui::Vec2 {
    let h = theme.dag_chrome_height().value();
    let mut w = h * 4.0 + theme.border_width.value();
    if !compact {
        w += ZOOM_READOUT_WIDTH.value();
    }
    egui::vec2(w, h)
}

/// 캔버스 안에서 줌 클러스터가 차지하는 자리. 캔버스 인터랙션이 이 영역을 비켜가야
/// 하므로 그리기 전에 먼저 알아야 한다.
pub fn zoom_cluster_rect(theme: &Theme, canvas: egui::Rect) -> egui::Rect {
    let inset = theme.dag_chrome_inset().value();
    let size = zoom_cluster_size(theme, canvas.width() < NARROW_ZOOM_LABEL.value());
    egui::Rect::from_min_size(
        egui::pos2(canvas.max.x - inset - size.x, canvas.max.y - inset - size.y),
        size,
    )
}

/// 클러스터 셀 하나. 눌렸으면 `true`.
fn cluster_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    salt: &'static str,
    icon: tasty_icons::Icon,
    enabled: bool,
    tooltip: &str,
) -> bool {
    let resp = ui.interact(
        rect,
        ui.id().with(("dag_zoom_cell", salt)),
        egui::Sense::click(),
    );
    if enabled && resp.hovered() {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.overlay_hover().to_egui_premultiplied(),
        );
    }
    let color = if enabled {
        theme.dag_chrome_fg()
    } else {
        theme.text_disabled()
    };
    let side = theme.icon_glyph_size_sm.value();
    icon.image(side, color.to_egui()).paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(side, side)),
    );
    enabled && resp.on_hover_text(tooltip).clicked()
}

/// 캔버스 위의 줌·맞춤·방향 버튼.
pub fn draw_zoom_cluster(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    view: &GraphView,
    orientation: Orientation,
    labels: &GraphChromeLabels<'_>,
) -> Option<GraphChromeAction> {
    let compact = rect.width() < zoom_cluster_size(theme, false).x;
    let radius = theme.corner_radius.value();
    let border = theme.dag_chrome_border().to_egui();
    ui.painter()
        .rect_filled(rect, radius, theme.dag_chrome_bg().to_egui());
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), border),
        egui::StrokeKind::Inside,
    );

    let side = theme.dag_chrome_height();
    let mut x = rect.min.x;
    // LogicalPx를 받아 egui 계산에서만 숫자로 꺼낸다.
    let mut cell = |w: LogicalPx| {
        let r = egui::Rect::from_min_size(
            egui::pos2(x, rect.min.y),
            egui::vec2(w.value(), side.value()),
        );
        x += w.value();
        r
    };

    let mut action = None;
    let minus = cell(side);
    if cluster_cell(
        ui,
        theme,
        minus,
        "out",
        tasty_icons::MINUS,
        view.zoom > ZOOM_MIN,
        labels.zoom_out,
    ) {
        action = Some(GraphChromeAction::Zoom(-1.0));
    }
    if !compact {
        let readout = cell(ZOOM_READOUT_WIDTH);
        ui.painter().text(
            readout.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}%", (view.zoom * 100.0).round() as i32),
            egui::FontId::monospace(theme.font_size_caption.value()),
            theme.dag_chrome_fg().to_egui(),
        );
    }
    let plus = cell(side);
    if cluster_cell(
        ui,
        theme,
        plus,
        "in",
        tasty_icons::PLUS,
        view.zoom < ZOOM_MAX,
        labels.zoom_in,
    ) {
        action = Some(GraphChromeAction::Zoom(1.0));
    }

    let sep = cell(theme.border_width);
    ui.painter().rect_filled(sep, 0.0, border);

    if cluster_cell(
        ui,
        theme,
        cell(side),
        "fit",
        tasty_icons::MOVE,
        true,
        labels.fit,
    ) {
        action = Some(GraphChromeAction::Fit);
    }
    let dir_icon = match orientation {
        Orientation::LeftRight => tasty_icons::ARROW_RIGHT,
        Orientation::TopDown => tasty_icons::ARROW_DOWN,
    };
    if cluster_cell(
        ui,
        theme,
        cell(side),
        "dir",
        dir_icon,
        true,
        labels.direction,
    ) {
        action = Some(GraphChromeAction::ToggleDirection);
    }
    action
}

/// 캔버스의 미니맵·줌 버튼·축약 표시.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_canvas_chrome<G: GraphData, P: GraphPainter<G>>(
    ui: &mut egui::Ui,
    theme: &Theme,
    canvas: egui::Rect,
    // 팝업이 back bar에 줌 버튼을 두면 미니맵은 캔버스 하단에 놓는다.
    cluster: Option<egui::Rect>,
    view: &GraphView,
    graph: &G,
    painter_impl: &P,
    layout: &GraphLayout,
    node_size: egui::Vec2,
    orientation: Orientation,
    lod: Lod,
    labels: &GraphChromeLabels<'_>,
) -> Option<GraphChromeAction> {
    let stack_bottom = match cluster {
        Some(c) => c.min.y,
        None => canvas.max.y - theme.dag_chrome_inset().value(),
    };
    // 좁은 화면에서는 숨긴다. 줌 버튼 위에 배치한다.
    if canvas.width() >= theme.dag_minimap_min_surface().value() {
        let size = egui::vec2(
            theme.dag_minimap_width().value(),
            theme.dag_minimap_height().value(),
        );
        let map = egui::Rect::from_min_size(
            egui::pos2(
                canvas.max.x - size.x - theme.dag_chrome_inset().value(),
                stack_bottom - theme.spacing_sm.value() - size.y,
            ),
            size,
        );
        let plot = GraphMinimap {
            graph,
            painter_impl,
            layout,
            node_size,
        };
        paint_graph_minimap(
            &ui.painter_at(canvas),
            theme,
            map,
            canvas.size(),
            view,
            &plot,
        );
    }
    let action = cluster.and_then(|c| draw_zoom_cluster(ui, theme, c, view, orientation, labels));
    paint_lod_chip(&ui.painter_at(canvas), theme, canvas, lod, labels);
    action
}

/// 좌하단 LOD 칩 — 지금 카드가 왜 축약돼 보이는지 알린다.
pub fn paint_lod_chip(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    lod: Lod,
    labels: &GraphChromeLabels<'_>,
) {
    if lod == Lod::Full {
        return;
    }
    let text = match lod {
        Lod::Compact => labels.lod_compact,
        _ => labels.lod_block,
    };
    let pad = theme.spacing_xs.value();
    let font = egui::FontId::proportional(theme.font_size_micro.value());
    let galley = painter.layout_no_wrap(text.to_string(), font, theme.dag_chrome_fg().to_egui());
    let size = galley.size() + egui::vec2(pad * 2.0, pad);
    let chip = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + pad, rect.max.y - size.y - pad),
        size,
    );
    painter.rect_filled(
        chip,
        theme.dag_runner_radius().value(),
        theme.dag_chrome_bg().to_egui(),
    );
    painter.galley(
        chip.min + egui::vec2(pad, pad / 2.0),
        galley,
        theme.dag_chrome_fg().to_egui(),
    );
}

/// 미니맵이 그릴 그래프. `layout.nodes` 는 노드 목록과 순서가 같다.
pub struct GraphMinimap<'a, G, P> {
    pub graph: &'a G,
    pub painter_impl: &'a P,
    pub layout: &'a GraphLayout,
    /// 레이아웃에 넘긴 카드 크기(배율 1).
    pub node_size: egui::Vec2,
}

/// `map` 자리에 그래프 축소판과 지금 캔버스(`canvas_size`)가 보는 영역을 그린다.
pub fn paint_graph_minimap<G: GraphData, P: GraphPainter<G>>(
    painter: &egui::Painter,
    theme: &Theme,
    map: egui::Rect,
    canvas_size: egui::Vec2,
    view: &GraphView,
    plot: &GraphMinimap<'_, G, P>,
) {
    let GraphMinimap {
        graph,
        painter_impl,
        layout,
        node_size,
    } = *plot;
    let graph_size = egui::vec2(layout.width.value(), layout.height.value());
    if graph_size.x <= 0.0 || graph_size.y <= 0.0 {
        return;
    }
    let size = map.size();
    painter.rect_filled(
        map,
        theme.dag_node_radius().value(),
        theme.dag_minimap_bg().to_egui(),
    );

    let k = (size.x / graph_size.x).min(size.y / graph_size.y);
    let base = map.min + (size - graph_size * k) / 2.0;
    let node = theme.dag_minimap_node().to_egui();
    let (nw, nh) = (node_size.x * k, node_size.y * k);
    let node_dim = theme.dag_minimap_node_dim().to_egui();
    for (n, data) in layout.nodes.iter().zip(graph.nodes()) {
        painter.rect_filled(
            egui::Rect::from_min_size(
                base + egui::vec2(n.x.value() * k, n.y.value() * k),
                egui::vec2(nw.max(1.0), nh.max(1.0)),
            ),
            0.0,
            if painter_impl.minimap_dimmed(data) {
                node_dim
            } else {
                node
            },
        );
    }

    // 현재 뷰포트가 그래프의 어디를 보고 있는지. 화면 좌표 → 그래프 좌표 역변환.
    let vp = egui::Rect::from_min_size(
        base + egui::vec2(
            -view.offset.x / view.zoom * k,
            -view.offset.y / view.zoom * k,
        ),
        egui::vec2(canvas_size.x / view.zoom * k, canvas_size.y / view.zoom * k),
    );
    painter.rect_stroke(
        vp.intersect(map),
        0.0,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.dag_minimap_viewport().to_egui(),
        ),
        egui::StrokeKind::Inside,
    );
}
