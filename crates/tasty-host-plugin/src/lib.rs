//! 호스트의 plugin 설치·프로세스 관리·IPC·이벤트 전달.
//! 본체 기능은 host_port trait을 통해 사용한다.

// 이유: 테스트의 let _는 사유 검사 대상이 아니다. 제품 코드에는 이 면제를 적용하지 않는다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

pub mod builtin;
pub mod bundle_sig;
pub mod command_registry;
pub mod discovery;
pub mod event_bus;
pub mod extension_registry;
pub mod file_persistence;
pub mod handle_channel;
pub mod host_actions;
pub mod host_cmd;
pub mod keybinding_bundle;
pub mod known_plugins;
pub mod listener;
pub mod manager;
pub mod official;
pub mod process;
pub mod protocol;
pub mod reaper;
pub mod registry_state;
pub mod reserved_removal;
pub mod settings_registry;
#[cfg(test)]
mod test_fake_plugin;
#[cfg(test)]
mod test_support;
pub mod tool_registry;
pub mod trust_verdict;
// wasm-poc feature에서만 사용하는 실험 코드.
pub mod wasm_poc;

pub use builtin::{
    BuiltinUpgradeAction, BuiltinUpgradeItem, BuiltinUpgradeReport, bundle_root,
    install_builtins_if_needed, is_builtin_plugin, mark_builtin_removed, upgrade_builtins,
};
pub use discovery::{discover, plugin_root};
pub use listener::HostListener;
pub use manager::{
    EguiMeshFrame, PluginCopiedText, PluginCopyTarget, PluginManager, PopupInstance,
    next_popup_z_seq,
};
pub use process::PluginProcess;
pub use registry_state::PluginsConfig;
pub use settings_registry::{SettingsPageEntry, SettingsPageRegistry};
pub use tasty_plugin_manifest::PluginPackage;
