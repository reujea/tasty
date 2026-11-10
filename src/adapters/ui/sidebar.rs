//! 사이드바 UI — collapsed / full 두 모드 + 도구 메뉴 핸들러.

mod collapsed;
mod full;
pub(crate) mod remote_row;
pub(crate) mod tools;
pub(crate) mod view;

pub use collapsed::draw_collapsed_sidebar;
pub use full::draw_full_sidebar;
