//! DAG 노드를 공통 DAG 카드(`tasty_ui_widgets::paint_dag_card`)로 그린다.
//! 상태 라벨·소요 시간 같은 문구는 여기서 번역해 넘긴다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{DagCardView, GraphLod, GraphNodeVisual, dag_card_colors, paint_dag_card};

pub use tasty_ui_widgets::dag_status_colors as status_colors;

use super::model::{DagNodeData, node_duration};

/// 노드 한 장의 (바 색, 배경, 라벨 색). 입력 대기는 상태 대신 needs-input 색 전체를 쓴다.
pub fn node_colors(theme: &Theme, node: &DagNodeData) -> (HexColor, HexColor, HexColor) {
    dag_card_colors(theme, node.status, node.awaiting().is_some())
}

/// 화면 좌표의 카드에 내용을 그린다. Ui는 아이콘 텍스처에, painter는 나머지에 사용한다.
/// `now_ms`는 duration 계산 기준 시각(epoch ms), `in_cycle`은 사이클을 이루는 노드인지다.
// 이유: 공통 카드 상태에 DAG만 쓰는 시각·사이클 여부를 더해 받는다.
#[allow(clippy::too_many_arguments)]
pub fn paint_node(
    ui: &egui::Ui,
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    node: &DagNodeData,
    vis: &GraphNodeVisual,
    now_ms: u64,
    in_cycle: bool,
) {
    // 상태 문구와 시간은 가장 자세한 단계에서만 카드에 실린다.
    let full = vis.lod == GraphLod::Full;
    let status_label = if full {
        node.status_label()
    } else {
        String::new()
    };
    let duration = if full {
        node_duration(node, now_ms)
    } else {
        None
    };
    let card = DagCardView {
        name: &node.name,
        status: node.status,
        awaiting: node.awaiting().is_some(),
        command_kind: node.command_kind,
        glyph: node.glyph(),
        status_label: &status_label,
        duration: duration.as_deref(),
        in_cycle,
    };
    paint_dag_card(ui, painter, theme, rect, &card, vis);
}
