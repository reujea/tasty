//! `surface.*` IPC 핸들러 도메인 sub-module 모음.

mod attention;
mod busy;
mod commands;
mod completion;
pub(crate) mod cwd;
#[cfg(feature = "gui")]
mod html_history;
#[cfg(feature = "gui")]
mod html_remote_allow;
#[cfg(feature = "gui")]
mod html_script;
#[cfg(feature = "gui")]
mod html_script_allow;
mod kinds;
mod list;
mod mark;
pub(crate) mod query;
pub(crate) mod remote_read;
mod send;

pub(crate) use attention::{handle_attention_clear, handle_attention_get};
pub(crate) use busy::handle_set_busy;
pub(crate) use commands::{handle_command_at, handle_commands, handle_last_command};
pub(crate) use completion::handle_completion;
pub(crate) use cwd::handle_set_cwd;
#[cfg(feature = "gui")]
pub(crate) use html_history::handle_html_history;
#[cfg(feature = "gui")]
pub(crate) use html_remote_allow::handle_html_remote_allow;
#[cfg(feature = "gui")]
pub(crate) use html_script::{handle_html_nav, handle_html_script};
#[cfg(feature = "gui")]
pub(crate) use html_script_allow::handle_html_script_allow;
pub(crate) use kinds::handle_surface_kinds;
pub(crate) use list::handle_surface_list;
pub(crate) use mark::{
    handle_parse_since_mark, handle_read_since_mark, handle_read_since_scan_mark, handle_set_mark,
    read_output_response,
};
pub(crate) use query::{
    handle_cursor_position, handle_foreground_process, handle_mouse_tracking, handle_screen_text,
    handle_surface_locate,
};
pub(crate) use send::{
    decode_input_header, handle_surface_send, handle_surface_send_combo, handle_surface_send_key,
    handle_surface_send_to, send_fail_message,
};

pub(super) use super::require_surface_id;
