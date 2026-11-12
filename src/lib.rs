//! Tasty 라이브러리. src/main.rs가 boot::run으로 실행한다.
//! 공개 API는 cargo doc -p tasty --no-deps로 확인한다.

// reason: 시험 코드의 Result 무시는 사유 주석 대상에서 제외한다.
// 일반 라이브러리 빌드는 기존 lint 수준을 유지한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

mod adapters;
mod app;
pub mod boot;
// GUI 렌더러와 헤드리스 debug.glyph_color가 같은 색 계산을 사용한다.
#[cfg(any(feature = "gui", debug_assertions))]
mod cell_palette;
mod clipboard;
mod completion_strategy;
mod core;
mod db;
#[cfg(test)]
mod design_token_guard;
#[cfg(test)]
mod dpi_conversion_guard;
mod file;
mod file_command;
// GUI의 무대 정의와 debug 조회, 시험에서 사용하는 메타데이터다.
#[cfg(any(feature = "gui", debug_assertions, test))]
mod fullscreen_stages;
#[cfg(feature = "gui")]
mod gfx;
mod hook_handler;
mod hook_runtime;
mod host_api;
mod hub;
mod i18n;
mod intent;
mod model;
#[cfg(test)]
mod namespace_table_for_tests;
mod plugin_bridge;
mod ports;
mod remote;
mod request_runs;
mod runtime;
mod runtime_ports;
#[cfg(test)]
mod source_guards;
mod state;
mod store;
mod system_notice;
#[cfg(feature = "gui")]
mod view;
mod waker;
// 시험이 X11 EventLoop 를 만드는 유일한 입구(연결 실패 진단 포함).
mod webhook;
#[cfg(all(test, target_os = "linux", feature = "gui"))]
mod x11_test_loop;

pub(crate) use tasty_utils::poison;

#[cfg(test)]
pub(crate) use tasty_test_support as test_support;

pub use tasty_font as font;
pub(crate) use tasty_platform as platform;
pub use tasty_settings as settings;
#[cfg(feature = "gui")]
use tasty_terminal as terminal;
pub use tasty_themes as theme;
pub use tasty_utils::path as paths;

pub(crate) use crate::runtime::output_observer;
pub(crate) use crate::runtime::surface_registry::meta as surface_meta;
pub(crate) use adapters::cli;
pub(crate) use adapters::ipc;
pub(crate) use adapters::plugin;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::input::click_cursor;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::input::double_tap;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::input::shortcuts;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::preset as preset_ui;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::surface::empty as empty_ui;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::surface::explorer as explorer_ui;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::surface::webview_chrome as webview_chrome_ui;
#[cfg(feature = "gui")]
pub(crate) use adapters::ui::terminal_link;
#[cfg(feature = "gui")]
pub(crate) use app::App;
#[cfg(all(debug_assertions, feature = "gui"))]
pub(crate) use app::debug_info;
pub(crate) use app::event::AppEvent;
#[cfg(feature = "gui")]
pub(crate) use boot::waker as waker_factory_winit;
#[cfg(feature = "gui")]
pub(crate) use file::dispatch as file_dispatch;
#[cfg(feature = "gui")]
pub(crate) use file::identify_worker;
#[cfg(feature = "gui")]
pub(crate) use gfx::gpu;
#[cfg(feature = "gui")]
pub(crate) use gfx::renderer;
pub(crate) use hook_runtime::global as global_hooks;
pub(crate) use host_api::hooks;
#[cfg(feature = "gui")]
pub(crate) use host_api::webview;
#[cfg(feature = "gui")]
pub(crate) use platform::app_icon;
pub(crate) use platform::crash_report;
#[cfg(all(windows, feature = "gui"))]
pub(crate) use platform::jump_list;
#[cfg(all(target_os = "macos", feature = "gui"))]
pub(crate) use platform::macos_delegate;
#[cfg(feature = "gui")]
pub(crate) use platform::macos_permissions;
#[cfg(feature = "gui")]
pub(crate) use platform::stall_watchdog;
#[cfg(all(
    any(windows, target_os = "macos", target_os = "linux"),
    feature = "gui"
))]
pub(crate) use platform::system_tray;
#[cfg(feature = "gui")]
pub(crate) use state::search as search_state;
#[cfg(feature = "gui")]
pub(crate) use state::selection;
pub(crate) use store::notification;
pub(crate) use store::recent_files;
pub(crate) use store::scrollback as scrollback_store;
#[cfg(feature = "gui")]
pub(crate) use view as window;
#[cfg(feature = "gui")]
pub(crate) use view::plugins::ui as plugins_ui;
#[cfg(feature = "gui")]
pub(crate) use view::settings::ui as settings_ui;
