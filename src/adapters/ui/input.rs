//! 입력 처리 — 저수준 (raw mouse / modifier 검출) + 고수준 (shortcuts).

pub mod click_cursor;
pub mod double_tap;
pub(crate) mod edit_key;
#[cfg(target_os = "linux")]
pub(crate) mod explorer_ime;
pub(crate) mod key_level;
pub(crate) mod key_path;
pub(crate) mod key_route;
pub mod shortcuts;
pub mod synthetic;
