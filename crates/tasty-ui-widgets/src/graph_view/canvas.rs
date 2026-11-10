//! 그래프 노드·간선을 painter로 그리고 클릭·호버를 직접 판별한다.
//! 그래프 좌표는 canvas.min + offset + graph * zoom으로 화면 좌표에 옮긴다.
//! 카드 내용·간선 잉크·흐림 판정은 [`GraphPainter`]가 정한다.

use tasty_dag_layout::{EdgeRoute, GraphLayout, Orientation};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::chrome::{GraphChromeAction, GraphChromeLabels, draw_canvas_chrome, zoom_cluster_rect};
use super::model::GraphData;
use super::state::{GraphView, Lod};

/// 그래프 좌표 ↔ 화면 좌표.
#[derive(Clone, Copy)]
pub struct Transform {
    pub origin: egui::Pos2,
    pub zoom: f32,
}

impl Transform {
    pub fn to_screen(self, x: LogicalPx, y: LogicalPx) -> egui::Pos2 {
        egui::pos2(
            self.origin.x + (x * self.zoom).value(),
            self.origin.y + (y * self.zoom).value(),
        )
    }

    pub fn rect(self, x: LogicalPx, y: LogicalPx, w: LogicalPx, h: LogicalPx) -> egui::Rect {
        egui::Rect::from_min_size(
            self.to_screen(x, y),
            egui::vec2((w * self.zoom).value(), (h * self.zoom).value()),
        )
    }
}

/// 카드 한 장의 그리기 상태.
pub struct NodeVisual {
    /// 이 배율에서 카드 안에 무엇을 넣을지.
    pub lod: Lod,
    /// 화면 배율 — 폰트/패딩만 따라 커진다. 박스 크기는 이것과 무관하게 고정이다.
    pub zoom: f32,
    pub selected: bool,
    pub hovered: bool,
    /// [`GraphPainter::dimmed_nodes`]가 흐리게 고른 노드.
    pub dimmed: bool,
}

/// 간선 한 줄의 잉크.
pub struct EdgeInk {
    pub color: egui::Color32,
    pub width: LogicalPx,
    /// 선·틈 길이를 번갈아 적은 파선 패턴(배율 1 기준). `None` 이면 실선.
    pub dash: Option<&'static [f32]>,
}

/// 데이터 소스마다 다른 카드 내용·간선 잉크·흐림 판정.
pub trait GraphPainter<G: GraphData> {
    /// 흐리게 그릴 노드. 길이는 노드 수와 같다.
    fn dimmed_nodes(&self, graph: &G) -> Vec<bool>;

    /// 노드 흐림과 별개로 간선 자체를 흐리게 그릴지.
    fn edge_dimmed(&self, _graph: &G, _edge: &G::Edge) -> bool {
        false
    }

    /// `dim`은 끝 노드가 흐리거나 [`Self::edge_dimmed`]일 때, `highlight`는 끝 노드가 선택됐을 때다.
    fn edge_ink(&self, theme: &Theme, edge: &G::Edge, dim: bool, highlight: bool) -> EdgeInk;

    /// 화면 좌표의 카드에 내용을 그린다. Ui는 아이콘 텍스처에, painter는 나머지에 사용한다.
    fn paint_node(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        theme: &Theme,
        rect: egui::Rect,
        node: &G::Node,
        vis: &NodeVisual,
    );

    /// 카드에 다 쓰지 못하는 이름·상태·이유를 보이는 호버 문구.
    fn hover_text(&self, node: &G::Node) -> String;

    /// 미니맵에서 흐리게 칠할 노드.
    fn minimap_dimmed(&self, node: &G::Node) -> bool;
}

/// 캔버스 한 장의 입력.
pub struct GraphCanvasProps<'a> {
    pub canvas_bg: egui::Color32,
    pub layout: &'a GraphLayout,
    pub orientation: Orientation,
    /// 레이아웃에 넘긴 카드 크기(배율 1).
    pub node_size: egui::Vec2,
    /// 캔버스 위 줌 클러스터를 그릴지. 팝업에서는 back bar가 줌 버튼을 그리므로
    /// 캔버스에 버튼 영역을 남기지 않는다.
    pub own_zoom_cluster: bool,
    pub labels: &'a GraphChromeLabels<'a>,
}

/// 캔버스와 조작 버튼을 그린다. 전역 단축키와 충돌하지 않도록 자체 키 조합은 등록하지 않는다.
pub fn draw_graph_canvas<G: GraphData, P: GraphPainter<G>>(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &mut GraphView,
    graph: &G,
    painter_impl: &P,
    props: &GraphCanvasProps<'_>,
) -> Option<GraphChromeAction> {
    let layout = props.layout;
    let orientation = props.orientation;
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, props.canvas_bg);

    let graph_size = egui::vec2(layout.width.value(), layout.height.value());
    // 같은 그래프·방향·뷰포트에서는 자동 맞춤을 반복하지 않아 사용자의 이동·배율을 유지한다.
    if !graph.nodes().is_empty() && view.take_fit(graph.graph_key(), orientation, rect.size()) {
        view.fit(graph_size, rect.size(), theme.dag_canvas_padding());
    }

    // 줌 버튼 영역을 미리 확보해 캔버스 이동·선택에서 제외한다.
    let cluster = props
        .own_zoom_cluster
        .then(|| zoom_cluster_rect(theme, rect));
    interact(
        ui,
        &response,
        rect,
        cluster.unwrap_or(egui::Rect::NOTHING),
        view,
        graph,
        layout,
        props.node_size,
    );

    let tr = Transform {
        origin: rect.min + view.offset,
        zoom: view.zoom,
    };
    let lod = Lod::of(view.zoom);

    paint_dot_grid(&painter, theme, rect, &tr);

    let dead = painter_impl.dimmed_nodes(graph);
    let selected_idx = view.selected.as_deref().and_then(|id| graph.index_of(id));

    for (edge, route) in graph.edges().iter().zip(layout.edges.iter()) {
        let dim = dead[route.from] || dead[route.to] || painter_impl.edge_dimmed(graph, edge);
        let highlight = selected_idx == Some(route.from) || selected_idx == Some(route.to);
        let ink = painter_impl.edge_ink(theme, edge, dim, highlight);
        paint_edge(&painter, theme, &tr, route, &ink, orientation);
    }

    let hovered = response
        .hover_pos()
        .filter(|p| !cluster.is_some_and(|c| c.contains(*p)))
        .and_then(|p| node_at(layout, &tr, props.node_size, p));

    for (i, pos) in layout.nodes.iter().enumerate() {
        let Some(node) = graph.nodes().get(i) else {
            continue;
        };
        let r = tr.rect(
            pos.x,
            pos.y,
            LogicalPx(props.node_size.x),
            LogicalPx(props.node_size.y),
        );
        if !rect.intersects(r.expand(theme.dag_node_selected_ring_width().value())) {
            continue;
        }
        let vis = NodeVisual {
            lod,
            zoom: view.zoom,
            selected: selected_idx == Some(i),
            hovered: hovered == Some(i),
            dimmed: dead[i],
        };
        painter_impl.paint_node(ui, &painter, theme, r, node, &vis);
        if vis.selected {
            paint_selection_ring(&painter, theme, r, view.zoom);
        }
    }

    // 이름·상태와 이유는 카드에 다 쓰지 못하므로 호버 툴팁으로 보인다.
    if let Some(node) = hovered.and_then(|i| graph.nodes().get(i)) {
        response
            .clone()
            .on_hover_text(painter_impl.hover_text(node));
    }

    draw_canvas_chrome(
        ui,
        theme,
        rect,
        cluster,
        view,
        graph,
        painter_impl,
        layout,
        props.node_size,
        orientation,
        lod,
        props.labels,
    )
}

/// pan / zoom / 선택.
#[allow(clippy::too_many_arguments)]
fn interact<G: GraphData>(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    cluster: egui::Rect,
    view: &mut GraphView,
    graph: &G,
    layout: &GraphLayout,
    node_size: egui::Vec2,
) {
    // 중간 버튼은 노드 위에서도 이동하고 왼쪽 버튼은 빈 배경에서만 이동한다.
    let tr = Transform {
        origin: rect.min + view.offset,
        zoom: view.zoom,
    };
    let on_chrome = response.hover_pos().is_some_and(|p| cluster.contains(p));
    let on_node = response
        .hover_pos()
        .filter(|_| !on_chrome)
        .and_then(|p| node_at(layout, &tr, node_size, p));
    let panning = !on_chrome
        && (response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged_by(egui::PointerButton::Primary) && on_node.is_none()));
    if panning {
        view.offset += response.drag_delta();
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    if response.clicked() && !on_chrome {
        view.selected = on_node
            .and_then(|i| graph.nodes().get(i))
            .map(|n| super::model::GraphNodeData::node_id(n).to_string());
    }

    if !response.hovered() || on_chrome {
        return;
    }
    let (scroll, modifiers) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers));
    if scroll != egui::Vec2::ZERO {
        if modifiers.command {
            let anchor = response
                .hover_pos()
                .map(|p| p - rect.min)
                .unwrap_or(rect.size() / 2.0);
            view.zoom_by(scroll.y.signum(), anchor);
        } else if modifiers.shift {
            view.offset.x += scroll.y + scroll.x;
        } else {
            view.offset += scroll;
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        view.selected = None;
    }
}

/// 점 아래의 노드 인덱스. 뒤에서부터 찾아 위에 그려진 카드가 이긴다.
fn node_at(
    layout: &GraphLayout,
    tr: &Transform,
    node_size: egui::Vec2,
    p: egui::Pos2,
) -> Option<usize> {
    layout
        .nodes
        .iter()
        .enumerate()
        .rev()
        .find(|(_, n)| {
            tr.rect(n.x, n.y, LogicalPx(node_size.x), LogicalPx(node_size.y))
                .contains(p)
        })
        .map(|(i, _)| i)
}

/// 그래프 좌표에 고정해 화면 이동·배율을 따르는 배경 격자.
fn paint_dot_grid(painter: &egui::Painter, theme: &Theme, rect: egui::Rect, tr: &Transform) {
    let gap = theme.dag_canvas_dot_gap().value() * tr.zoom;
    if gap < 6.0 {
        return;
    }
    let size = (theme.dag_canvas_dot_size().value() * tr.zoom).max(1.0) / 2.0;
    let color = theme.dag_canvas_dot().to_egui();
    let start_x = rect.min.x + (tr.origin.x - rect.min.x).rem_euclid(gap);
    let start_y = rect.min.y + (tr.origin.y - rect.min.y).rem_euclid(gap);
    let mut y = start_y - gap;
    while y < rect.max.y {
        let mut x = start_x - gap;
        while x < rect.max.x {
            painter.circle_filled(egui::pos2(x, y), size, color);
            x += gap;
        }
        y += gap;
    }
}

/// 간선 하나 — 직교 세그먼트 + 라운드 코너 + 화살촉.
fn paint_edge(
    painter: &egui::Painter,
    theme: &Theme,
    tr: &Transform,
    route: &EdgeRoute,
    ink: &EdgeInk,
    orientation: Orientation,
) {
    let raw: Vec<egui::Pos2> = route
        .points
        .iter()
        .map(|(x, y)| tr.to_screen(*x, *y))
        .collect();
    if raw.len() < 2 {
        return;
    }
    let ortho = orthogonalize(&raw, orientation);
    let rounded = round_corners(&ortho, theme.dag_edge_corner_radius() * tr.zoom);
    paint_edge_line(painter, &rounded, ink, tr.zoom);
    paint_arrow(
        painter,
        theme.dag_edge_arrow_size() * tr.zoom,
        ink.color,
        &ortho,
    );
}

/// 이미 화면 좌표로 옮긴 경로를 실선 또는 배율을 적용한 파선으로 긋는다.
pub fn paint_edge_line(painter: &egui::Painter, path: &[egui::Pos2], ink: &EdgeInk, zoom: f32) {
    let stroke = egui::Stroke::new(ink.width.value().max(1.0), ink.color);
    match ink.dash {
        Some(pattern) => {
            let on: Vec<f32> = pattern.iter().step_by(2).map(|v| v * zoom).collect();
            let off: Vec<f32> = pattern
                .iter()
                .skip(1)
                .step_by(2)
                .map(|v| v * zoom)
                .collect();
            painter.add(egui::Shape::dashed_line_with_offset(
                path, stroke, &on, &off, 0.0,
            ))
        }
        None => painter.add(egui::Shape::line(path.to_vec(), stroke)),
    };
}

/// 대각 세그먼트를 레이어 축 중간에서 꺾어 직교로 편다.
pub fn orthogonalize(points: &[egui::Pos2], orientation: Orientation) -> Vec<egui::Pos2> {
    let mut out = vec![points[0]];
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let diag = (a.x - b.x).abs() > 0.5 && (a.y - b.y).abs() > 0.5;
        if diag {
            match orientation {
                Orientation::LeftRight => {
                    let mid = (a.x + b.x) / 2.0;
                    out.push(egui::pos2(mid, a.y));
                    out.push(egui::pos2(mid, b.y));
                }
                Orientation::TopDown => {
                    let mid = (a.y + b.y) / 2.0;
                    out.push(egui::pos2(a.x, mid));
                    out.push(egui::pos2(b.x, mid));
                }
            }
        }
        out.push(b);
    }
    out
}

/// 각 꺾임점을 반경 `r` 짜리 사분원으로 깎는다.
pub fn round_corners(points: &[egui::Pos2], r: LogicalPx) -> Vec<egui::Pos2> {
    if points.len() < 3 || r <= LogicalPx(0.5) {
        return points.to_vec();
    }
    const ARC_STEPS: usize = 4;
    let mut out = vec![points[0]];
    for i in 1..points.len() - 1 {
        let (prev, cur, next) = (points[i - 1], points[i], points[i + 1]);
        let din = (cur - prev).normalized();
        let dout = (next - cur).normalized();
        let rin = r.min(LogicalPx((cur - prev).length() / 2.0)).value();
        let rout = r.min(LogicalPx((next - cur).length() / 2.0)).value();
        let start = cur - din * rin;
        let end = cur + dout * rout;
        out.push(start);
        // 제어점을 모서리로 두는 2 차 베지어 — 직각에서 사분원과 시각적으로 같다.
        for s in 1..ARC_STEPS {
            let t = s as f32 / ARC_STEPS as f32;
            let u = 1.0 - t;
            out.push(egui::pos2(
                u * u * start.x + 2.0 * u * t * cur.x + t * t * end.x,
                u * u * start.y + 2.0 * u * t * cur.y + t * t * end.y,
            ));
        }
        out.push(end);
    }
    out.push(points[points.len() - 1]);
    out
}

/// 끝점 화살촉. 마지막 세그먼트 방향을 따른다.
pub fn paint_arrow(
    painter: &egui::Painter,
    size: LogicalPx,
    color: egui::Color32,
    points: &[egui::Pos2],
) {
    let size = size.value();
    if size < 2.0 || points.len() < 2 {
        return;
    }
    let tip = points[points.len() - 1];
    let dir = (tip - points[points.len() - 2]).normalized();
    if !dir.x.is_finite() || !dir.y.is_finite() {
        return;
    }
    let normal = egui::vec2(-dir.y, dir.x);
    let back = tip - dir * size;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, back + normal * size * 0.4, back - normal * size * 0.4],
        color,
        egui::Stroke::NONE,
    ));
}

/// 선택 링은 카드 밖에 간격을 두어 상태 테두리와 구분한다.
pub fn paint_selection_ring(painter: &egui::Painter, theme: &Theme, rect: egui::Rect, zoom: f32) {
    let w = (theme.dag_node_selected_ring_width().value() * zoom).max(1.0);
    let offset = theme.border_width.value() * zoom;
    painter.rect_stroke(
        rect.expand(offset + w / 2.0),
        (theme.dag_node_radius().value() * zoom + offset + w).round(),
        egui::Stroke::new(w, theme.dag_node_selected_ring().to_egui()),
        egui::StrokeKind::Middle,
    );
}

/// 한 줄 글자 폭.
pub fn text_width(painter: &egui::Painter, text: &str, font: &egui::FontId) -> LogicalPx {
    LogicalPx(painter.ctx().fonts(|f| {
        f.layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    }))
}

/// 글자 경계를 지키며 폭에 맞춰 줄인다.
pub fn ellipsize(
    painter: &egui::Painter,
    text: &str,
    font: &egui::FontId,
    max_w: LogicalPx,
) -> String {
    if max_w <= LogicalPx(0.0) {
        return String::new();
    }
    if text_width(painter, text, font) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = lo.midpoint(hi + 1).min(chars.len());
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if text_width(painter, &cand, font) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
        if mid == lo && lo == hi {
            break;
        }
    }
    if lo == 0 {
        return "…".to_string();
    }
    chars[..lo].iter().collect::<String>() + "…"
}
