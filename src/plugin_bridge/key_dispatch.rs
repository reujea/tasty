//! 플러그인 단축키 검색과 실행 요청.
//! 포커스된 플러그인 우선 검사와 전역 단축키 검사의 호출 순서는
//! src/app/plugin_glue/shortcut.rs의 App::try_plugin_shortcut이 정한다.

use winit::keyboard::{Key, ModifiersState};

use tasty_settings::KeybindingSettings;

use crate::plugin::PluginManager;
use crate::plugin::command_registry::{EffectiveBinding, effective_binding};
use crate::shortcuts::matches_any_binding;

pub fn focused_plugin_surface(
    state: &crate::state::RequestContext,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
) -> Option<(String, u32)> {
    let pane = state.focused_pane(engine)?;
    let tab = pane.tabs.get(state.navigation.tab_index(pane))?;
    let focused = state.navigation.surface_id(tab).unwrap_or(0);
    let surface = engine.find_surface_by_id(focused)?;
    let remote = surface
        .as_any()
        .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()?;
    Some((remote.plugin_id.clone(), remote.id))
}

/// 지정한 플러그인의 단축키를 사용자 설정·매니페스트·호스트 설정으로 계산한다.
/// 해당 플러그인의 surface에 포커스가 있을 때 호출하므로 Global과 Surface 모두 검사한다.
pub fn match_plugin_shortcut(
    mgr: &PluginManager,
    plugin_id: &str,
    key: &Key,
    mods: ModifiersState,
    host_kb: &KeybindingSettings,
) -> Option<String> {
    for entry in mgr.command_registry.commands_for(plugin_id) {
        let ov = mgr.config.shortcut_override(plugin_id, &entry.command_id);
        let bindings: Vec<String> = match effective_binding(entry, ov, host_kb) {
            EffectiveBinding::Keys(k) => k,
            EffectiveBinding::Inherit { keys, .. } => keys,
            EffectiveBinding::None => continue,
        };
        if bindings.is_empty() {
            continue;
        }
        if matches_any_binding(&bindings, key, mods) {
            return Some(entry.command_id.clone());
        }
    }
    None
}

/// 모든 플러그인의 Global 명령에서 단축키를 찾는다. Surface 명령은 제외한다.
pub fn match_global_shortcut(
    mgr: &PluginManager,
    key: &Key,
    mods: ModifiersState,
    host_kb: &KeybindingSettings,
) -> Option<(String, String)> {
    for entry in mgr.command_registry.iter_global() {
        let ov = mgr
            .config
            .shortcut_override(&entry.plugin_id, &entry.command_id);
        let bindings: Vec<String> = match effective_binding(entry, ov, host_kb) {
            EffectiveBinding::Keys(k) => k,
            EffectiveBinding::Inherit { keys, .. } => keys,
            EffectiveBinding::None => continue,
        };
        if bindings.is_empty() {
            continue;
        }
        if matches_any_binding(&bindings, key, mods) {
            return Some((entry.plugin_id.clone(), entry.command_id.clone()));
        }
    }
    None
}

/// WebView가 호스트로 전달할 plugin 단축키. 비활성 설정의 플러그인은 제외한다.
/// 실행 범위는 `App::dispatch_plugin_shortcut_key`와 같다: 키를 받은 surface가 plugin surface면 그 plugin의
/// 모든 명령, 아니면 모든 plugin의 Global 명령이다. 실제 실행할 명령은 호스트가 입력을 처리할 때 다시 선택한다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebViewCommandBindings {
    /// 모든 plugin의 Global 명령 조합.
    pub global: Vec<String>,
    /// plugin별 Global·Surface 명령 조합.
    pub owned: Vec<(String, Vec<String>)>,
}

pub fn webview_command_bindings(
    mgr: &PluginManager,
    host_kb: &KeybindingSettings,
) -> WebViewCommandBindings {
    let mut out = WebViewCommandBindings::default();
    for entry in mgr.command_registry.iter_all() {
        if mgr.config.is_disabled(&entry.plugin_id) {
            continue;
        }
        let ov = mgr
            .config
            .shortcut_override(&entry.plugin_id, &entry.command_id);
        let keys: Vec<String> = match effective_binding(entry, ov, host_kb) {
            EffectiveBinding::Keys(k) => k,
            EffectiveBinding::Inherit { keys, .. } => keys,
            EffectiveBinding::None => continue,
        }
        .into_iter()
        .filter(|k| !k.is_empty())
        .collect();
        if entry.scope == crate::plugin::manifest::CommandScope::Global {
            out.global.extend(keys.iter().cloned());
        }
        match out.owned.iter_mut().find(|(id, _)| *id == entry.plugin_id) {
            Some((_, owned)) => owned.extend(keys),
            None => out.owned.push((entry.plugin_id.clone(), keys)),
        }
    }
    out
}

/// command.invoked 전송을 명령 소유 플러그인에만 요청한다.
/// 다른 플러그인의 같은 이벤트 구독으로는 전달하지 않는다.
pub fn emit_command_invoked(
    mgr: &mut PluginManager,
    plugin_id: &str,
    command_id: &str,
    source_surface_id: Option<u32>,
) {
    let manifest_scope = mgr
        .command_registry
        .find(plugin_id, command_id)
        .map(|e| e.scope)
        .unwrap_or_default();
    use tasty_plugin_protocol::EventScope;
    use tasty_plugin_protocol::events::payloads::{CommandInvoked, CommandScope, CommandTrigger};
    let scope = match manifest_scope {
        crate::plugin::manifest::CommandScope::Global => CommandScope::Global,
        crate::plugin::manifest::CommandScope::Surface => CommandScope::Surface,
    };
    let payload = CommandInvoked {
        plugin_id: plugin_id.to_string(),
        command_id: command_id.to_string(),
        scope,
        source_surface_id,
        trigger: CommandTrigger::Shortcut,
    };
    mgr.emit_host_event_to_plugin(plugin_id, "command.invoked", &payload, EventScope::System);
}

/// action이 없는 명령의 실행을 플러그인에 요청한다.
/// command.invoked를 보내고, surface_id가 있을 때만 command.invoke도 요청한다.
/// action이 있는 명령은 호출자가 이벤트를 보낸 뒤 직접 처리한다.
pub fn dispatch_plugin_command(
    mgr: &mut PluginManager,
    plugin_id: &str,
    command_id: &str,
    surface_id: Option<u32>,
) {
    tracing::debug!(
        "plugin shortcut matched: plugin='{}' command='{}' surface={:?}",
        plugin_id,
        command_id,
        surface_id
    );
    emit_command_invoked(mgr, plugin_id, command_id, surface_id);
    if let Some(surface_id) = surface_id {
        mgr.send_command_invoke(plugin_id, surface_id, command_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_plugin_manifest::{CommandDecl, Contributes, Entry};
    use winit::keyboard::{NamedKey, SmolStr};

    struct StubFormat;
    impl tasty_plugin_protocol::host_port::FileFormatRegistryPort for StubFormat {
        fn install_plugin_detectors(&self, _: &str, _: &[serde_json::Value]) {}
        fn uninstall_plugin(&self, _: &str) {}
    }
    struct StubHandler;
    impl tasty_plugin_protocol::host_port::FileHandlerRegistryPort for StubHandler {
        fn install_plugin_handlers(&self, _: &str, _: &[serde_json::Value]) {}
        fn uninstall_plugin(&self, _: &str) {}
    }

    /// 매니저는 생성 중 plugins-logs를 만들므로 격리 홈을 먼저 설정한다.
    /// 반환한 홈 가드를 이름 있는 변수로 보관해 매니저보다 먼저 해제되지 않게 한다.
    fn mgr() -> (crate::test_support::IsolatedHome, PluginManager) {
        let home = crate::test_support::IsolatedHome::new();
        let m = PluginManager::with_registries(
            std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
            std::sync::Arc::new(StubFormat),
            std::sync::Arc::new(StubHandler),
        );
        (home, m)
    }

    fn k_char(s: &str) -> Key {
        Key::Character(SmolStr::new(s))
    }

    fn manifest_with_commands(id: &str, cmds: Vec<CommandDecl>) -> tasty_plugin_manifest::Manifest {
        tasty_plugin_manifest::Manifest {
            manifest_version: 1,
            id: id.to_string(),
            name: id.to_string(),
            version: "0.1".to_string(),
            authors: vec![],
            description: String::new(),
            homepage: String::new(),
            api_version: "1".to_string(),
            entry: Entry::Process {
                command: "x".to_string(),
                args: vec![],
            },
            surface_kinds: vec![],
            permissions: vec![],
            event_subscribe: vec![],
            event_publish: vec![],
            events_emitted: vec![],
            contributes: Contributes {
                commands: cmds,
                ..Default::default()
            },
            extends: None,
            lang_dir: "lang".to_string(),
            bundle: true,
        }
    }

    fn cmd(
        id: &str,
        default_keybinding: &str,
        scope: tasty_plugin_manifest::CommandScope,
    ) -> CommandDecl {
        CommandDecl {
            id: id.to_string(),
            title_i18n_key: format!("{id}.title"),
            default_keybinding: Some(default_keybinding.to_string()),
            binding_mode: tasty_plugin_manifest::BindingMode::Independent,
            scope,
            action: None,
        }
    }

    fn ctrl_shift_r() -> (Key, ModifiersState) {
        (k_char("r"), ModifiersState::CONTROL | ModifiersState::SHIFT)
    }

    #[test]
    fn match_global_shortcut_finds_across_multiple_plugins() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.noop",
                "ctrl+alt+z",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.b",
            vec![cmd(
                "b.open",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        let kb = KeybindingSettings::preset_tasty();
        let (key, mods) = ctrl_shift_r();
        let matched = match_global_shortcut(&m, &key, mods, &kb);
        assert_eq!(
            matched,
            Some(("com.example.b".to_string(), "b.open".to_string()))
        );
    }

    #[test]
    fn match_global_shortcut_ignores_surface_scope_commands() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.surface_only",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Surface,
            )],
        ));
        let kb = KeybindingSettings::preset_tasty();
        let (key, mods) = ctrl_shift_r();
        assert_eq!(match_global_shortcut(&m, &key, mods, &kb), None);
    }

    #[test]
    fn match_global_shortcut_no_match_returns_none() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.other",
                "ctrl+alt+z",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        let kb = KeybindingSettings::preset_tasty();
        let (key, mods) = ctrl_shift_r();
        assert_eq!(match_global_shortcut(&m, &key, mods, &kb), None);
    }

    #[test]
    fn match_global_shortcut_respects_user_override() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+alt+z",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        m.config.set_shortcut_override(
            "com.example.a",
            "a.open",
            crate::plugin::registry_state::ShortcutOverride::Key {
                value: vec!["ctrl+shift+r".to_string()],
            },
        );
        let kb = KeybindingSettings::preset_tasty();
        let (key, mods) = ctrl_shift_r();
        assert_eq!(
            match_global_shortcut(&m, &key, mods, &kb),
            Some(("com.example.a".to_string(), "a.open".to_string()))
        );
    }

    #[test]
    fn match_plugin_shortcut_matches_within_focused_plugin_only() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Surface,
            )],
        ));
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.b",
            vec![cmd(
                "b.open",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Surface,
            )],
        ));
        let kb = KeybindingSettings::preset_tasty();
        let (key, mods) = ctrl_shift_r();
        assert_eq!(
            match_plugin_shortcut(&m, "com.example.a", &key, mods, &kb),
            Some("a.open".to_string())
        );
        assert_eq!(
            match_plugin_shortcut(&m, "com.example.ghost", &key, mods, &kb),
            None
        );
    }

    #[test]
    fn modifier_only_press_never_matches() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        let kb = KeybindingSettings::preset_tasty();
        let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(
            match_global_shortcut(&m, &Key::Named(NamedKey::Control), mods, &kb),
            None
        );
    }

    #[test]
    fn dispatch_plugin_command_with_no_surface_does_not_panic() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        // 프로세스가 없는 경로에서 패닉하지 않는지만 확인한다.
        dispatch_plugin_command(&mut m, "com.example.a", "a.open", None);
    }

    #[test]
    fn dispatch_plugin_command_with_surface_does_not_panic() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+shift+r",
                tasty_plugin_manifest::CommandScope::Surface,
            )],
        ));
        dispatch_plugin_command(&mut m, "com.example.a", "a.open", Some(7));
    }

    #[test]
    fn emit_command_invoked_unknown_command_defaults_scope_without_panic() {
        let (_home, mut m) = mgr();
        emit_command_invoked(&mut m, "com.example.ghost", "ghost.cmd", None);
    }

    #[test]
    fn webview_command_bindings_collects_manifest_defaults_and_overrides() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![
                cmd(
                    "a.global",
                    "ctrl+alt+z",
                    tasty_plugin_manifest::CommandScope::Global,
                ),
                cmd(
                    "a.surface",
                    "ctrl+shift+r",
                    tasty_plugin_manifest::CommandScope::Surface,
                ),
            ],
        ));
        m.config.set_shortcut_override(
            "com.example.a",
            "a.global",
            crate::plugin::registry_state::ShortcutOverride::Key {
                value: vec!["ctrl+shift+h".to_string()],
            },
        );
        let kb = KeybindingSettings::preset_tasty();
        let got = webview_command_bindings(&m, &kb);
        assert_eq!(
            got.global,
            vec!["ctrl+shift+h".to_string()],
            "사용자 설정이 기본 단축키를 대체하고 Global에는 Surface scope가 없어야 한다"
        );
        let mut owned = got.owned.clone();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].0, "com.example.a");
        owned[0].1.sort();
        assert_eq!(
            owned[0].1,
            vec!["ctrl+shift+h".to_string(), "ctrl+shift+r".to_string()],
            "그 plugin의 surface에서는 Surface scope도 실행 범위다"
        );
    }

    #[test]
    fn webview_command_bindings_skips_cleared_bindings() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+alt+z",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        m.config.set_shortcut_override(
            "com.example.a",
            "a.open",
            crate::plugin::registry_state::ShortcutOverride::None,
        );
        let kb = KeybindingSettings::preset_tasty();
        let got = webview_command_bindings(&m, &kb);
        assert!(got.global.is_empty());
        assert!(got.owned.iter().all(|(_, keys)| keys.is_empty()));
    }

    #[test]
    fn webview_command_bindings_skips_disabled_plugins() {
        let (_home, mut m) = mgr();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+alt+z",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        let kb = KeybindingSettings::preset_tasty();
        assert_eq!(
            webview_command_bindings(&m, &kb).global,
            vec!["ctrl+alt+z".to_string()]
        );
        assert!(m.config.disable("com.example.a"));
        assert_eq!(
            webview_command_bindings(&m, &kb),
            WebViewCommandBindings::default()
        );
    }

    // registry와 사용자 설정 변경으로 WebView의 단축키 캐시를 갱신할 수 있어야 한다.
    #[test]
    fn shortcut_epochs_advance_on_every_binding_change() {
        let (_home, mut m) = mgr();
        let r0 = m.command_registry.revision();
        m.command_registry.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd(
                "a.open",
                "ctrl+alt+z",
                tasty_plugin_manifest::CommandScope::Global,
            )],
        ));
        let r1 = m.command_registry.revision();
        assert_ne!(
            r0, r1,
            "플러그인을 등록하면 registry revision이 바뀌어야 한다"
        );
        m.command_registry.unregister_plugin("com.example.a");
        assert_ne!(
            r1,
            m.command_registry.revision(),
            "플러그인을 해제하면 registry revision이 바뀌어야 한다"
        );
        // 새 registry도 이전 객체와 다른 revision을 가져야 한다.
        assert_ne!(
            m.command_registry.revision(),
            crate::plugin::command_registry::PluginCommandRegistry::new().revision()
        );

        let c0 = m.config.shortcut_revision();
        m.config.set_shortcut_override(
            "com.example.a",
            "a.open",
            crate::plugin::registry_state::ShortcutOverride::None,
        );
        let c1 = m.config.shortcut_revision();
        assert_ne!(
            c0, c1,
            "사용자 단축키를 설정하면 config revision이 바뀌어야 한다"
        );
        assert!(m.config.clear_shortcut_override("com.example.a", "a.open"));
        assert_ne!(
            c1,
            m.config.shortcut_revision(),
            "사용자 단축키를 지우면 config revision이 바뀌어야 한다"
        );
    }
}
