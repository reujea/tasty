//! 플러그인 매니저를 모달·단축키·화면 표시 상태와 연결한다.

mod file_install;
mod file_reapprove;
mod file_remove;
mod file_reserved_remove;
mod file_upgrade;
pub(crate) mod install_trust;
pub(crate) mod lifecycle;
pub(crate) mod palette_commands;
pub(crate) mod shortcut;
pub(crate) mod snapshot;
pub(crate) mod tool_registry;
pub(crate) mod window_actions;
