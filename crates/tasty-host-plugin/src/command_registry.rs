//! 매니페스트의 plugin 명령을 모아 단축키 매칭과 설정 화면에 제공한다.
//! 등록·해제 시 목록을 갱신하고 기본 키와 사용자 override를 함께 해석한다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tasty_settings::KeybindingSettings;

use crate::host_actions;

use super::registry_state::ShortcutOverride;
use tasty_plugin_manifest::{BindingMode, CommandDecl, CommandScope, Manifest, ToolAction};

/// 한 plugin이 등록한 한 command의 메타데이터.
#[derive(Debug, Clone)]
pub struct PluginCommandEntry {
    pub plugin_id: String,
    pub command_id: String,
    pub title_i18n_key: String,
    /// 매니페스트 `default_keybinding`의 raw 문자열 (예: `"F5"`, `"ctrl+shift+r"`).
    /// 사용자가 별도 override를 지정하지 않았을 때의 시작값. None이면 기본 키 없음.
    pub manifest_default: Option<String>,
    pub binding_mode: BindingMode,
    pub scope: CommandScope,
    /// 선언적 액션(`[[contributes.tool]].action`과 동일한 `ToolAction`). `Some`이면
    /// 디스패치 시 호스트가 이 액션을 직접 실행하고 옛 `command.invoke` IPC
    /// (`handle_command`)는 생략한다 — 우선순위는 `CommandDecl::action` 문서 참조.
    pub action: Option<ToolAction>,
}

impl PluginCommandEntry {
    fn from_decl(plugin_id: &str, decl: &CommandDecl) -> Self {
        // inherit 대상이 화이트리스트에 없으면 Independent로 강등 (warn 로그)
        let mode = match &decl.binding_mode {
            BindingMode::InheritHost(action) if !host_actions::is_inheritable(action) => {
                tracing::warn!(
                    "plugin '{plugin_id}' command '{}' uses unknown inherit target '{action}' — \
                     downgrading to Independent",
                    decl.id
                );
                BindingMode::Independent
            }
            other => other.clone(),
        };
        Self {
            plugin_id: plugin_id.to_string(),
            command_id: decl.id.clone(),
            title_i18n_key: decl.title_i18n_key.clone(),
            manifest_default: decl.default_keybinding.clone(),
            binding_mode: mode,
            scope: decl.scope,
            action: decl.action.clone(),
        }
    }
}

/// 목록이 바뀔 때 발급하는 프로세스 전역 epoch.
/// 레지스트리를 다시 만들어도 이전 스냅샷 캐시와 구분할 수 있게 한다.
pub(crate) fn next_shortcut_epoch() -> u64 {
    static EPOCH: AtomicU64 = AtomicU64::new(0);
    EPOCH.fetch_add(1, Ordering::Relaxed) + 1
}

/// Plugin command 일람. PluginManager가 보관하며, plugin enable/disable에 따라
/// `register_plugin` / `unregister_plugin`으로 갱신된다.
#[derive(Debug, Default, Clone)]
pub struct PluginCommandRegistry {
    /// (plugin_id, command_id) 기준 유일. plugin_id가 같은 entry는 vec에 모아서
    /// 설정 UI 그룹핑이 단순해지도록 한다.
    by_plugin: HashMap<String, Vec<PluginCommandEntry>>,
    /// 마지막 변경 시점의 [`next_shortcut_epoch`] 값.
    revision: u64,
}

impl PluginCommandRegistry {
    pub fn new() -> Self {
        Self {
            by_plugin: HashMap::new(),
            revision: next_shortcut_epoch(),
        }
    }

    /// 등록 내용이 마지막으로 바뀐 시점의 전역 epoch. 값이 그대로면 command 목록도
    /// 그대로다 — 파생 스냅샷 캐시의 무효화 키로 쓴다.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// plugin의 모든 command를 (재)등록. 기존 항목은 모두 교체된다.
    /// 이미 같은 plugin이 등록된 상태라면 새 매니페스트로 통째로 갱신.
    pub fn register_plugin(&mut self, manifest: &Manifest) {
        let entries: Vec<PluginCommandEntry> = manifest
            .contributes
            .commands
            .iter()
            .map(|d| PluginCommandEntry::from_decl(&manifest.id, d))
            .collect();
        if entries.is_empty() {
            self.by_plugin.remove(&manifest.id);
        } else {
            self.by_plugin.insert(manifest.id.clone(), entries);
        }
        self.revision = next_shortcut_epoch();
    }

    /// plugin의 command를 모두 제거.
    pub fn unregister_plugin(&mut self, plugin_id: &str) {
        self.by_plugin.remove(plugin_id);
        self.revision = next_shortcut_epoch();
    }

    /// 특정 plugin의 command 목록.
    pub fn commands_for(&self, plugin_id: &str) -> &[PluginCommandEntry] {
        self.by_plugin
            .get(plugin_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.by_plugin.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_plugin.values().map(|v| v.len()).sum()
    }

    /// 모든 plugin 명령을 순회한다. plugin 사이의 순서는 보장하지 않는다.
    pub fn iter_all(&self) -> impl Iterator<Item = &PluginCommandEntry> {
        self.by_plugin.values().flat_map(|v| v.iter())
    }

    /// 등록된 **모든** plugin의 `CommandScope::Global` command만 순회.
    ///
    /// 포커스된 plugin surface가 없는 상태에서 단축키를 전체 plugin의 Global
    /// command와 매칭할 때 사용 (`key_dispatch::match_global_shortcut`).
    /// `Surface` scope command는 이 iterator에 나타나지 않는다 — 그 owner
    /// plugin의 surface가 포커스되어 있을 때만 `commands_for`로 매칭된다.
    pub fn iter_global(&self) -> impl Iterator<Item = &PluginCommandEntry> {
        self.iter_all().filter(|e| e.scope == CommandScope::Global)
    }

    /// `(plugin_id, command_id)` 로 단일 entry lookup.
    pub fn find(&self, plugin_id: &str, command_id: &str) -> Option<&PluginCommandEntry> {
        self.by_plugin
            .get(plugin_id)?
            .iter()
            .find(|e| e.command_id == command_id)
    }
}

/// 한 plugin command에 실제 적용되는 단축키.
///
/// 매니페스트 default + binding_mode + 사용자 override를 합성한 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveBinding {
    /// plugin 자체 키 (사용자 또는 매니페스트 default).
    Keys(Vec<String>),
    /// 호스트 액션 키를 따라간다. `keys`는 호스트 KeybindingSettings에서
    /// 즉시 해석한 결과 (편의용 cache — 호스트 설정이 바뀌면 다시 조회 필요).
    Inherit { source: String, keys: Vec<String> },
    /// 단축키 미할당 (매니페스트도 default 없음, 사용자도 None으로 설정).
    None,
}

/// command entry + 사용자 override + 호스트 keybindings를 합성해 실제로 매칭에
/// 사용할 키 목록을 결정.
pub fn effective_binding(
    entry: &PluginCommandEntry,
    user_override: Option<&ShortcutOverride>,
    host_kb: &KeybindingSettings,
) -> EffectiveBinding {
    // 1. 사용자 override가 우선
    if let Some(ov) = user_override {
        match ov {
            ShortcutOverride::Key { value } => {
                if value.is_empty() {
                    return EffectiveBinding::None;
                }
                return EffectiveBinding::Keys(value.clone());
            }
            ShortcutOverride::Inherit { source } => {
                if !host_actions::is_inheritable(source) {
                    tracing::warn!(
                        "user override inherit:{source} on '{}/{}' refers to non-inheritable action — ignoring",
                        entry.plugin_id,
                        entry.command_id
                    );
                    return manifest_default(entry, host_kb);
                }
                let keys = host_actions::host_action_for(host_kb, source)
                    .cloned()
                    .unwrap_or_default();
                return EffectiveBinding::Inherit {
                    source: source.clone(),
                    keys,
                };
            }
            ShortcutOverride::None => return EffectiveBinding::None,
        }
    }
    // 2. 매니페스트 default
    manifest_default(entry, host_kb)
}

fn manifest_default(entry: &PluginCommandEntry, host_kb: &KeybindingSettings) -> EffectiveBinding {
    match &entry.binding_mode {
        BindingMode::InheritHost(action) => {
            let keys = host_actions::host_action_for(host_kb, action)
                .cloned()
                .unwrap_or_default();
            EffectiveBinding::Inherit {
                source: action.clone(),
                keys,
            }
        }
        BindingMode::Independent => match &entry.manifest_default {
            Some(s) if !s.is_empty() => EffectiveBinding::Keys(vec![s.clone()]),
            _ => EffectiveBinding::None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_plugin_manifest::{Contributes, Entry};

    fn manifest_with_commands(id: &str, cmds: Vec<CommandDecl>) -> Manifest {
        Manifest {
            manifest_version: 1,
            id: id.to_string(),
            name: id.to_string(),
            version: "0.1".to_string(),
            authors: vec![],
            description: String::new(),
            homepage: String::new(),
            api_version: "1".to_string(),
            min_tasty_version: None,
            entry: Entry::Process {
                command: "x".to_string(),
                args: vec![],
            },
            surface_kinds: vec![],
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

    fn cmd(id: &str, mode: BindingMode) -> CommandDecl {
        cmd_with_scope(id, mode, CommandScope::default())
    }

    fn cmd_with_scope(id: &str, mode: BindingMode, scope: CommandScope) -> CommandDecl {
        CommandDecl {
            id: id.to_string(),
            title_i18n_key: format!("{id}.title"),
            default_keybinding: Some("F5".to_string()),
            binding_mode: mode,
            scope,
            action: None,
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = PluginCommandRegistry::new();
        let m = manifest_with_commands(
            "com.example.x",
            vec![
                cmd("x.refresh", BindingMode::Independent),
                cmd("x.copy", BindingMode::InheritHost("clipboard.copy".into())),
            ],
        );
        reg.register_plugin(&m);
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.commands_for("com.example.x").len(), 2);
        assert_eq!(
            reg.find("com.example.x", "x.refresh").unwrap().command_id,
            "x.refresh"
        );
    }

    #[test]
    fn unregister_removes_all() {
        let mut reg = PluginCommandRegistry::new();
        reg.register_plugin(&manifest_with_commands(
            "com.example.x",
            vec![cmd("x.a", BindingMode::Independent)],
        ));
        reg.unregister_plugin("com.example.x");
        assert!(reg.is_empty());
    }

    #[test]
    fn re_register_replaces_entries() {
        let mut reg = PluginCommandRegistry::new();
        reg.register_plugin(&manifest_with_commands(
            "com.example.x",
            vec![
                cmd("x.a", BindingMode::Independent),
                cmd("x.b", BindingMode::Independent),
            ],
        ));
        // 같은 plugin을 더 적은 command로 다시 등록 → 기존 entry 모두 교체
        reg.register_plugin(&manifest_with_commands(
            "com.example.x",
            vec![cmd("x.c", BindingMode::Independent)],
        ));
        assert_eq!(reg.commands_for("com.example.x").len(), 1);
        assert!(reg.find("com.example.x", "x.a").is_none());
        assert!(reg.find("com.example.x", "x.c").is_some());
    }

    #[test]
    fn unknown_inherit_target_downgraded_to_independent() {
        let mut reg = PluginCommandRegistry::new();
        reg.register_plugin(&manifest_with_commands(
            "com.example.x",
            vec![cmd("x.weird", BindingMode::InheritHost("tab.new".into()))],
        ));
        let entry = reg.find("com.example.x", "x.weird").unwrap();
        assert_eq!(entry.binding_mode, BindingMode::Independent);
    }

    #[test]
    fn iter_global_only_yields_global_scope_across_plugins() {
        let mut reg = PluginCommandRegistry::new();
        reg.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![
                cmd_with_scope("a.global", BindingMode::Independent, CommandScope::Global),
                cmd_with_scope("a.surface", BindingMode::Independent, CommandScope::Surface),
            ],
        ));
        reg.register_plugin(&manifest_with_commands(
            "com.example.b",
            vec![cmd_with_scope(
                "b.global",
                BindingMode::Independent,
                CommandScope::Global,
            )],
        ));
        let mut global_ids: Vec<&str> = reg.iter_global().map(|e| e.command_id.as_str()).collect();
        global_ids.sort_unstable();
        assert_eq!(global_ids, vec!["a.global", "b.global"]);
    }

    #[test]
    fn iter_global_empty_when_no_global_commands() {
        let mut reg = PluginCommandRegistry::new();
        reg.register_plugin(&manifest_with_commands(
            "com.example.a",
            vec![cmd_with_scope(
                "a.surface",
                BindingMode::Independent,
                CommandScope::Surface,
            )],
        ));
        assert_eq!(reg.iter_global().count(), 0);
    }

    #[test]
    fn action_field_propagates_from_manifest_decl() {
        let mut reg = PluginCommandRegistry::new();
        let mut decl = cmd("x.open", BindingMode::Independent);
        decl.action = Some(tasty_plugin_manifest::ToolAction::OpenPopup {
            popup_id: "com.example.x/main".to_string(),
        });
        reg.register_plugin(&manifest_with_commands("com.example.x", vec![decl]));
        let entry = reg.find("com.example.x", "x.open").unwrap();
        assert!(matches!(
            entry.action,
            Some(tasty_plugin_manifest::ToolAction::OpenPopup { .. })
        ));
    }

    #[test]
    fn empty_command_list_purges_plugin_entry() {
        let mut reg = PluginCommandRegistry::new();
        reg.register_plugin(&manifest_with_commands(
            "com.example.x",
            vec![cmd("x.a", BindingMode::Independent)],
        ));
        // 매니페스트가 commands를 비워서 다시 들어오면 plugin 항목 자체 제거
        reg.register_plugin(&manifest_with_commands("com.example.x", vec![]));
        assert!(reg.is_empty());
    }

    fn entry(
        plugin: &str,
        id: &str,
        mode: BindingMode,
        default: Option<&str>,
    ) -> PluginCommandEntry {
        PluginCommandEntry {
            plugin_id: plugin.to_string(),
            command_id: id.to_string(),
            title_i18n_key: format!("{id}.title"),
            manifest_default: default.map(|s| s.to_string()),
            binding_mode: mode,
            scope: CommandScope::default(),
            action: None,
        }
    }

    #[test]
    fn effective_user_key_override_wins() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, Some("F5"));
        let ov = ShortcutOverride::Key {
            value: vec!["F6".into()],
        };
        match effective_binding(&e, Some(&ov), &kb) {
            EffectiveBinding::Keys(v) => assert_eq!(v, vec!["F6".to_string()]),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn effective_user_key_override_empty_means_none() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, Some("F5"));
        let ov = ShortcutOverride::Key { value: vec![] };
        assert!(matches!(
            effective_binding(&e, Some(&ov), &kb),
            EffectiveBinding::None
        ));
    }

    #[test]
    fn effective_user_inherit_override_resolves_host_keys() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.copy", BindingMode::Independent, None);
        let ov = ShortcutOverride::Inherit {
            source: "clipboard.copy".into(),
        };
        match effective_binding(&e, Some(&ov), &kb) {
            EffectiveBinding::Inherit { source, keys } => {
                assert_eq!(source, "clipboard.copy");
                assert_eq!(keys, kb.copy);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn effective_user_inherit_override_non_inheritable_falls_back_to_manifest() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, Some("F5"));
        let ov = ShortcutOverride::Inherit {
            source: "tab.new".into(),
        };
        match effective_binding(&e, Some(&ov), &kb) {
            EffectiveBinding::Keys(v) => assert_eq!(v, vec!["F5".to_string()]),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn effective_user_none_override_means_none() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, Some("F5"));
        let ov = ShortcutOverride::None;
        assert!(matches!(
            effective_binding(&e, Some(&ov), &kb),
            EffectiveBinding::None
        ));
    }

    #[test]
    fn effective_no_override_manifest_inherit_resolves_host() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry(
            "p",
            "p.copy",
            BindingMode::InheritHost("clipboard.copy".into()),
            None,
        );
        match effective_binding(&e, None, &kb) {
            EffectiveBinding::Inherit { source, keys } => {
                assert_eq!(source, "clipboard.copy");
                assert_eq!(keys, kb.copy);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn effective_no_override_manifest_independent_with_default() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, Some("F5"));
        match effective_binding(&e, None, &kb) {
            EffectiveBinding::Keys(v) => assert_eq!(v, vec!["F5".to_string()]),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn effective_no_override_manifest_independent_without_default() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, None);
        assert!(matches!(
            effective_binding(&e, None, &kb),
            EffectiveBinding::None
        ));
    }

    #[test]
    fn effective_no_override_manifest_independent_empty_default() {
        let kb = KeybindingSettings::preset_tasty();
        let e = entry("p", "p.refresh", BindingMode::Independent, Some(""));
        assert!(matches!(
            effective_binding(&e, None, &kb),
            EffectiveBinding::None
        ));
    }
}
