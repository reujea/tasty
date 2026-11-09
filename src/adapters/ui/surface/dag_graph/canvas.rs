//! DAG 노드·엣지를 painter로 그리고 클릭·호버를 직접 판별한다.
//! 그래프 좌표는 canvas.min + offset + graph * zoom으로 화면 좌표에 옮긴다.

use tasty_dag_layout::GraphLayout;
use tasty_model::DagDirection;
use tasty_type_appearance::theme::Theme;

use super::chrome::ChromeAction;
use super::model::{DagEdgeData, DagGraphData, DagRelation, EdgeSelection};
use super::node::{NodeVisual, paint_node, paint_selection_ring};
use super::view::{DagGraphView, Lod};

/// 그래프 좌표 ↔ 화면 좌표.
#[derive(Clone, Copy)]
pub struct Transform {
    pub origin: egui::Pos2,
    pub zoom: f32,
}

impl Transform {
    pub fn to_screen(self, x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(self.origin.x + x * self.zoom, self.origin.y + y * self.zoom)
    }

    pub fn rect(self, x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(
            self.to_screen(x, y),
            egui::vec2(w * self.zoom, h * self.zoom),
        )
    }
}

/// 캔버스와 조작 버튼을 그린다. 전역 단축키와 충돌하지 않도록 자체 키 조합은 등록하지 않는다.
// 이유: DagChrome이 정하는 값과 프레임마다 계산하는 상태를 각각 전달한다.
#[allow(clippy::too_many_arguments)]
pub fn draw_canvas(
    ui: &mut egui::Ui,
    theme: &Theme,
    canvas_bg: egui::Color32,
    view: &mut DagGraphView,
    graph: &DagGraphData,
    layout: &GraphLayout,
    direction: DagDirection,
    now_ms: u64,
    // 팝업에서는 back bar가 줌 버튼을 그리므로 캔버스에 버튼 영역을 남기지 않는다.
    own_zoom_cluster: bool,
) -> Option<ChromeAction> {
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, canvas_bg);

    let graph_size = egui::vec2(layout.width.value(), layout.height.value());
    // 같은 DAG·방향·뷰포트에서는 자동 맞춤을 반복하지 않아 사용자의 이동·배율을 유지한다.
    if !graph.nodes.is_empty() && view.take_fit(&graph.id, direction, rect.size()) {
        view.fit(graph_size, rect.size(), theme.dag_canvas_padding().value());
    }

    // 줌 버튼 영역을 미리 확보해 캔버스 이동·선택에서 제외한다.
    let cluster = own_zoom_cluster.then(|| super::chrome::zoom_cluster_rect(theme, rect));
    interact(
        ui,
        theme,
        &response,
        rect,
        cluster.unwrap_or(egui::Rect::NOTHING),
        view,
        graph,
        layout,
    );

    let tr = Transform {
        origin: rect.min + view.offset,
        zoom: view.zoom,
    };
    let lod = Lod::of(view.zoom);

    paint_dot_grid(&painter, theme, rect, &tr);

    let dead = dead_path(graph);
    let cycle_nodes = cycle_set(graph);
    let selected_idx = view.selected.as_deref().and_then(|id| graph.index_of(id));

    for (edge, route) in graph.edges.iter().zip(layout.edges.iter()) {
        let dim = dead[route.from]
            || dead[route.to]
            || edge.selection.is_some_and(EdgeSelection::is_dimmed);
        let highlight = selected_idx == Some(route.from) || selected_idx == Some(route.to);
        paint_edge(&painter, theme, &tr, route, edge, direction, dim, highlight);
    }

    let hovered = response
        .hover_pos()
        .filter(|p| !cluster.is_some_and(|c| c.contains(*p)))
        .and_then(|p| node_at(layout, &tr, theme, p));

    for (i, pos) in layout.nodes.iter().enumerate() {
        let Some(node) = graph.nodes.get(i) else {
            continue;
        };
        let r = tr.rect(
            pos.x.value(),
            pos.y.value(),
            theme.dag_node_width().value(),
            theme.dag_node_height().value(),
        );
        if !rect.intersects(r.expand(theme.dag_node_selected_ring_width().value())) {
            continue;
        }
        let vis = NodeVisual {
            lod,
            zoom: view.zoom,
            now_ms,
            selected: selected_idx == Some(i),
            hovered: hovered == Some(i),
            dimmed: dead[i],
            in_cycle: cycle_nodes.contains(&node.id),
        };
        paint_node(ui, &painter, theme, r, node, &vis);
        if vis.selected {
            paint_selection_ring(&painter, theme, r, view.zoom);
        }
    }

    // 이름·상태와 건너뜀·알 수 없음·입력 대기의 이유는 카드에 다 쓰지 못하므로 호버 툴팁으로 보인다.
    if let Some(node) = hovered.and_then(|i| graph.nodes.get(i)) {
        response.clone().on_hover_text(node.hover_text(now_ms));
    }

    // 주기 조회는 egui 지연 repaint 가 아니라 타이머 허브에 보이는 뷰만 예약한다. 지연 repaint 는 그려질 때만 갱신된다.

    super::chrome::draw_canvas_chrome(
        ui, theme, rect, cluster, view, graph, layout, direction, lod,
    )
}

/// pan / zoom / 선택.
#[allow(clippy::too_many_arguments)]
fn interact(
    ui: &egui::Ui,
    theme: &Theme,
    response: &egui::Response,
    rect: egui::Rect,
    cluster: egui::Rect,
    view: &mut DagGraphView,
    graph: &DagGraphData,
    layout: &GraphLayout,
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
        .and_then(|p| node_at(layout, &tr, theme, p));
    let panning = !on_chrome
        && (response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged_by(egui::PointerButton::Primary) && on_node.is_none()));
    if panning {
        view.offset += response.drag_delta();
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    if response.clicked() && !on_chrome {
        view.selected = on_node
            .and_then(|i| graph.nodes.get(i))
            .map(|n| n.id.clone());
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
fn node_at(layout: &GraphLayout, tr: &Transform, theme: &Theme, p: egui::Pos2) -> Option<usize> {
    let (w, h) = (
        theme.dag_node_width().value(),
        theme.dag_node_height().value(),
    );
    layout
        .nodes
        .iter()
        .enumerate()
        .rev()
        .find(|(_, n)| tr.rect(n.x.value(), n.y.value(), w, h).contains(p))
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

/// 엣지 하나 — 직교 세그먼트 + 라운드 코너 + 화살촉.
#[allow(clippy::too_many_arguments)]
fn paint_edge(
    painter: &egui::Painter,
    theme: &Theme,
    tr: &Transform,
    route: &tasty_dag_layout::EdgeRoute,
    edge: &DagEdgeData,
    direction: DagDirection,
    dim: bool,
    highlight: bool,
) {
    let relation = edge.relation;
    let color = edge_ink(theme, relation, dim, highlight).to_egui();
    // 선택된 전이 경로만 굵게 그린다.
    let width = if edge.selection == Some(EdgeSelection::Selected) {
        theme.dag_edge_selected_width()
    } else {
        theme.dag_edge_width()
    };
    let stroke = egui::Stroke::new(width.value().max(1.0), color);

    let raw: Vec<egui::Pos2> = route
        .points
        .iter()
        .map(|(x, y)| tr.to_screen(x.value(), y.value()))
        .collect();
    if raw.len() < 2 {
        return;
    }
    let ortho = orthogonalize(&raw, direction);
    let rounded = round_corners(&ortho, theme.dag_edge_corner_radius().value() * tr.zoom);

    match relation.dash() {
        Some(pattern) => {
            let on: Vec<f32> = pattern.iter().step_by(2).map(|v| v * tr.zoom).collect();
            let off: Vec<f32> = pattern
                .iter()
                .skip(1)
                .step_by(2)
                .map(|v| v * tr.zoom)
                .collect();
            painter.add(egui::Shape::dashed_line_with_offset(
                &rounded, stroke, &on, &off, 0.0,
            ))
        }
        None => painter.add(egui::Shape::line(rounded.clone(), stroke)),
    };

    paint_arrow(
        painter,
        theme.dag_edge_arrow_size().value() * tr.zoom,
        color,
        &ortho,
    );
}

/// 대각 세그먼트를 레이어 축 중간에서 꺾어 직교로 편다.
fn orthogonalize(points: &[egui::Pos2], direction: DagDirection) -> Vec<egui::Pos2> {
    let mut out = vec![points[0]];
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let diag = (a.x - b.x).abs() > 0.5 && (a.y - b.y).abs() > 0.5;
        if diag {
            match direction {
                DagDirection::LeftRight => {
                    let mid = (a.x + b.x) / 2.0;
                    out.push(egui::pos2(mid, a.y));
                    out.push(egui::pos2(mid, b.y));
                }
                DagDirection::TopDown => {
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
fn round_corners(points: &[egui::Pos2], r: f32) -> Vec<egui::Pos2> {
    if points.len() < 3 || r <= 0.5 {
        return points.to_vec();
    }
    const ARC_STEPS: usize = 4;
    let mut out = vec![points[0]];
    for i in 1..points.len() - 1 {
        let (prev, cur, next) = (points[i - 1], points[i], points[i + 1]);
        let din = (cur - prev).normalized();
        let dout = (next - cur).normalized();
        let rin = r.min((cur - prev).length() / 2.0);
        let rout = r.min((next - cur).length() / 2.0);
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
fn paint_arrow(painter: &egui::Painter, size: f32, color: egui::Color32, points: &[egui::Pos2]) {
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

/// 엣지 잉크. 선택 노드의 강조색이 가장 앞서고, 쓸 수 없는 경로는 관계 색을 버린 `dag-edge-dim` 실선 잉크다.
/// 관계는 그대로 그리는 파선 무늬가 알린다.
fn edge_ink(
    theme: &Theme,
    relation: DagRelation,
    dim: bool,
    highlight: bool,
) -> tasty_type_appearance::color::HexColor {
    if highlight {
        return theme.dag_edge_highlight();
    }
    if dim {
        return theme.dag_edge_dim();
    }
    match relation {
        DagRelation::DependsOn => theme.dag_edge_depends(),
        DagRelation::Fallback => theme.dag_edge_fallback(),
        DagRelation::Reduce => theme.dag_edge_reduce(),
        DagRelation::Binding => theme.dag_edge_binding(),
        DagRelation::Transition => theme.dag_edge_transition(),
    }
}

/// 실패·취소·스킵 노드의 하류 중 흐리게 표시할 노드를 고른다.
/// fallback 연결과 종료된 노드는 제외하며, 실제 실행 가능 여부를 판정하는 함수는 아니다.
fn dead_path(graph: &DagGraphData) -> Vec<bool> {
    let mut dead = vec![false; graph.nodes.len()];
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.status.kills_outgoing() {
            dead[i] = true;
        }
    }
    // 반복 횟수를 노드 수로 제한해 순환 관계에서도 종료한다.
    for _ in 0..graph.nodes.len() {
        let mut changed = false;
        for e in &graph.edges {
            if e.relation == DagRelation::Fallback {
                continue;
            }
            if dead[e.from] && !dead[e.to] && !graph.nodes[e.to].status.is_terminal() {
                dead[e.to] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.status.is_terminal() {
            dead[i] = false;
        }
    }
    dead
}

fn cycle_set(graph: &DagGraphData) -> std::collections::HashSet<String> {
    graph
        .cycle
        .as_ref()
        .map(|ids| ids.iter().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selected_node_highlight_wins_over_the_dead_path_ink() {
        let th = tasty_themes::mocha_fallback();
        let t = DagRelation::Transition;
        assert_eq!(edge_ink(&th, t, true, true), th.dag_edge_highlight());
        assert_eq!(edge_ink(&th, t, true, false), th.dag_edge_dim());
        assert_eq!(edge_ink(&th, t, false, false), th.dag_edge_transition());
    }
}
