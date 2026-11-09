//! about_to_wait에서 대기 중인 요청과 이벤트 큐를 처리한다.

pub(crate) mod agent_events;
pub(crate) mod file_picker;
pub(crate) mod handler_ipc;
pub(crate) mod host_events;
pub(crate) mod info_modal;
pub(crate) mod intents;
pub(crate) mod list_global;
pub(crate) mod lua_commands;
pub(crate) mod palette_plugin_commands;
pub(crate) mod picker;
pub(crate) mod plugin_banner;
pub(crate) mod plugin_ipc;
pub(crate) mod plugin_popup_events;
pub(crate) mod plugin_webview_menu;
pub(crate) mod plugin_webview_open;
pub(crate) mod popup_opens;
pub(crate) mod surface_lifecycle;
pub(crate) mod surface_popups;
pub(crate) mod tool_events;
