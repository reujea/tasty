//! 설정 윈도우 탭별 draw 함수 모음. 각 sub-module 은 한 탭의 `pub fn draw_*_tab`
//! + 그에 종속된 helper 만 보관한다.

mod accessibility;
mod anomaly_detection;
mod appearance;
mod choice;
mod font_rows;
mod general;
#[cfg(target_os = "macos")]
mod general_display;
#[cfg(target_os = "macos")]
mod macos_permissions;
mod misc;
mod notifications;
mod number;
mod overlay;
mod performance;
mod plugin;
mod remote_attach;
mod remote_transfer;
mod task_pipeline;
mod terminal;
mod terminal_input;

pub use accessibility::draw_accessibility_tab;
pub use anomaly_detection::draw_anomaly_detection_tab;
pub use appearance::draw_appearance_tab;
pub use font_rows::PreviewFonts;
pub use general::draw_general_tab;
#[cfg(target_os = "macos")]
pub use general_display::draw_general_display_tab;
#[cfg(target_os = "macos")]
pub use macos_permissions::draw_macos_permissions_tab;
pub use misc::{ScriptsUiState, draw_scripts_subtab};
pub use notifications::draw_notifications_tab;
pub use overlay::draw_overlay_tab;
pub use performance::draw_performance_tab;
pub use plugin::draw_plugin_tab;
pub use remote_attach::draw_remote_attach_tab;
pub use remote_transfer::draw_remote_transfer_tab;
pub use task_pipeline::draw_task_pipeline_tab;
pub use terminal::{draw_terminal_mouse_capture_tab, draw_terminal_tab, draw_terminal_tui_tab};
pub use terminal_input::draw_terminal_input_tab;
