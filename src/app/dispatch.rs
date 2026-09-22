//! `about_to_wait` 에서 호출되는 각 dispatch_pending_* 메서드들.
//!
//! 한 프레임에 한 번씩 호출되어 직전 프레임에 쌓인 도메인별 큐를 drain → emit.

pub(crate) mod agent_events;
pub(crate) mod file_picker;
pub(crate) mod handler_ipc;
pub(crate) mod host_events;
pub(crate) mod intents;
pub(crate) mod list_global;
pub(crate) mod lua_commands;
pub(crate) mod memory_changes;
pub(crate) mod palette_plugin_commands;
pub(crate) mod picker;
pub(crate) mod plugin_banner;
pub(crate) mod plugin_ipc;
pub(crate) mod plugin_popup_events;
pub(crate) mod plugin_webview_open;
pub(crate) mod popup_opens;
pub(crate) mod surface_lifecycle;
pub(crate) mod tool_events;
