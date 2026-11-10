//! Surface kind 별 egui 렌더링. terminal 은 GPU 렌더링이라 여기에 없고, 나머지
//! builtin surface (empty / explorer / dag_graph) 의 view 와, webview-kind surface 의
//! host chrome (webview_chrome) 을 모음. (image / markdown 은 plugin 이
//! egui-mesh 로 자가 렌더 — host view 없음.)

pub mod dag_graph;
pub mod empty;
pub mod explorer;
pub mod graph_source;
pub mod html_address;
pub(crate) mod html_receive;
pub mod html_script_banner;
pub mod webview_chrome;
