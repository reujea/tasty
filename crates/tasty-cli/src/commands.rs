//! Subcommand enum 도메인 분할.

pub mod agent;
pub mod approval;
pub mod clipboard;
pub mod completion_strategy;
pub mod debug;
pub mod events;
pub mod file_handler;
pub mod hook_handler;
pub mod list_set;
pub mod memory;
pub mod move_send;
pub mod new_close;
pub mod notification;
pub mod output;
pub mod passkey;
pub mod plugin_cmd;
pub mod preset;
pub mod pty;
pub mod read_unset;
pub mod remote;
pub mod remote_profile;
pub mod session;
pub mod settings;
pub mod surface;
pub mod surface_meta;
pub mod telemetry;
pub mod terminal;
pub mod webhook;
pub mod workspace_category;

pub use agent::{AgentCommands, ReportCommands};
pub use approval::{ApprovalCommands, ApprovalSummaryCommands};
pub use clipboard::ClipboardCommands;
pub use completion_strategy::CompletionStrategyCommands;
#[cfg(debug_assertions)]
pub use debug::{
    BannerDebugCommands, DebugCommands, EventBusCommands, ExtensionDebugCommands,
    FullscreenDebugCommands, HostPopupDebugCommands, InjectDebugCommands, LuaDebugCommands,
    ModalDebugCommands, ModifierHintDebugCommands, PluginBannerDebugCommands, PopupDebugCommands,
    RemoteDownloadDebugCommands, SettingsDebugCommands, ToolDebugCommands,
};
pub use events::EventsCommands;
pub use file_handler::FileHandlerCommands;
pub use hook_handler::HookHandlerCommands;
pub use list_set::{ListCommands, SetCommands};
pub use memory::{
    MemoryBbCommands, MemoryCacheCommands, MemoryCommands, MemoryGoalCommands, MemoryPlanCommands,
    MemorySecretCommands,
};
pub use move_send::{MoveCommands, SendCommands};
pub use new_close::{CloseCommands, NewCommands};
pub use notification::NotificationCommands;
pub use output::{OutputCommands, OutputObserveCommands};
pub use plugin_cmd::{ExtensionCommands, PluginCommands, ToolCommands};
pub use preset::PresetCommands;
pub use pty::PtyCommands;
pub use read_unset::{ReadCommands, UnsetCommands};
pub use remote::RemoteCommands;
pub use remote_profile::RemoteProfileCommands;
pub use session::SessionCommands;
pub use settings::SettingsCommands;
pub use surface::{SurfaceAttentionCommands, SurfaceCommands};
pub use surface_meta::SurfaceMetaCommands;
pub use telemetry::{TelemetryAnomalyCommands, TelemetryCapCommands, TelemetryCommands};
pub use terminal::TerminalCommands;
pub use webhook::WebhookCommands;
pub use workspace_category::WorkspaceCategoryCommands;
