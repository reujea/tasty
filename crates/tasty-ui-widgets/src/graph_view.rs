//! 데이터 소스와 무관한 공통 그래프 뷰. DAG 그래프와 child 그래프가 같이 쓴다.
//!
//! 계층: 데이터([`GraphData`]) → 레이아웃([`GraphView::layout`], `tasty-dag-layout`) →
//! 캔버스([`draw_graph_canvas`]: 점 격자·꺾인선·화살촉·선택 링·미니맵·줌 클러스터·LOD 칩) →
//! 선택([`GraphView::selected`]) → 상세 자리([`split_graph_drawer`]) → 틀([`GraphFrame`]).
//! 카드 내용·간선 잉크는 [`GraphPainter`]로, 상세 내용은 클로저로 받는다.
//! DAG 카드([`paint_dag_card`])는 본체 DAG painter와 갤러리 DAG 표본이 같이 부른다.
//! 노드 의미·문구·조회 주기는 호출자가 가진다. 계약은 docs/architecture/ui-widgets-crate.md.

mod canvas;
mod chrome;
mod dag_card;
mod frame;
mod model;
mod state;

pub use canvas::{
    EdgeInk, GraphCanvasProps, GraphPainter, NodeVisual, Transform, draw_graph_canvas, ellipsize,
    orthogonalize, paint_arrow, paint_edge_line, paint_selection_ring, round_corners, text_width,
};
pub use chrome::{
    GRAPH_NARROW_DETAIL_SHEET, GraphChromeAction, GraphChromeLabels, GraphMinimap,
    draw_zoom_cluster, paint_graph_minimap, paint_lod_chip, zoom_cluster_rect, zoom_cluster_size,
};
pub use dag_card::{
    DagCardView, DagStatus, dag_card_colors, dag_kind_icon, dag_status_border, dag_status_colors,
    paint_dag_card,
};
pub use frame::{
    GraphDrawerDock, GraphDrawerSplit, GraphFrame, drawer_body, drawer_mono_row, drawer_row,
    drawer_section, drawer_text_block, paint_drawer_divider, split_graph_drawer,
};
pub use model::{
    EdgeLink, GraphData, GraphEdgeData, GraphNodeData, GraphNodeKind, machine_scoped_id,
};
pub use state::{
    GraphView, Lod, ZOOM_FIT_MAX, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP, orientation_key, shape_key,
};
