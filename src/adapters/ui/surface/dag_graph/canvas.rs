//! DAG 카드·엣지를 공통 그래프 캔버스에 주입하는 painter.
//! 캔버스 자체(이동·배율·선택·꺾인선·미니맵)는 `tasty_ui_widgets::draw_graph_canvas`가 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{EdgeInk, GraphNodeVisual, GraphPainter};

use super::model::{DagEdgeData, DagGraphData, DagNodeData, DagRelation, EdgeSelection};
use super::node::paint_node;

/// 한 프레임의 DAG painter. 사이클 노드와 시각은 프레임마다 정한다.
pub struct DagPainter {
    /// duration 계산 기준 시각(epoch ms).
    now_ms: u64,
    cycle: std::collections::HashSet<String>,
}

impl DagPainter {
    pub fn new(graph: &DagGraphData, now_ms: u64) -> Self {
        Self {
            now_ms,
            cycle: cycle_set(graph),
        }
    }
}

impl GraphPainter<DagGraphData> for DagPainter {
    fn dimmed_nodes(&self, graph: &DagGraphData) -> Vec<bool> {
        dead_path(graph)
    }

    fn edge_dimmed(&self, _graph: &DagGraphData, edge: &DagEdgeData) -> bool {
        edge.selection.is_some_and(EdgeSelection::is_dimmed)
    }

    fn edge_ink(&self, theme: &Theme, edge: &DagEdgeData, dim: bool, highlight: bool) -> EdgeInk {
        // 선택된 전이 경로만 굵게 그린다.
        let width = if edge.selection == Some(EdgeSelection::Selected) {
            theme.dag_edge_selected_width()
        } else {
            theme.dag_edge_width()
        };
        EdgeInk {
            color: edge_ink(theme, edge.relation, dim, highlight).to_egui(),
            width,
            dash: edge.relation.dash(),
        }
    }

    fn paint_node(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        theme: &Theme,
        rect: egui::Rect,
        node: &DagNodeData,
        vis: &GraphNodeVisual,
    ) {
        let in_cycle = self.cycle.contains(&node.id);
        paint_node(ui, painter, theme, rect, node, vis, self.now_ms, in_cycle);
    }

    fn hover_text(&self, node: &DagNodeData) -> String {
        node.hover_text(self.now_ms)
    }

    fn minimap_dimmed(&self, node: &DagNodeData) -> bool {
        node.status.is_dimmed()
    }
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
