//! DAG 관찰 화면. 노드 편집 없이 이동·배율·선택·대상 전환을 지원한다.
//! 캔버스·선택·상세 자리는 공통 그래프 뷰(`tasty_ui_widgets::draw_graph_canvas`)가 그리고
//! 여기에는 DAG 데이터 소스·카드·상세 내용·헤더가 있다.
//! 레이아웃은 노드 ID·의존 관계·방향·치수로 계산하며 상태 변화만으로 다시 배치하지 않는다.

pub mod canvas;
pub mod chrome;
pub mod detail;
pub mod model;
pub mod node;
pub mod render;
pub mod view;

#[cfg(test)]
mod drawer_tests;

pub use render::{DagChrome, draw_dag_graph};
pub use view::{DagGraphViewStore, DagPollRequest, DagTarget};
