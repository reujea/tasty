//! 데이터 루트의 plugins.toml: 플러그인 활성 상태·단축키를 저장한다.
//!
//! 형식:
//! ```toml
//! [disabled]
//! ids = ["com.example.broken"]
//!
//! [removed_builtins]
//! ids = ["com.tasty.explorer"]
//!
//! [keybindings."com.example.explorer"."explorer.refresh"]
//! mode = "key"
//! value = ["F6"]
//!
//! [keybindings."com.example.explorer"."explorer.copy_paths"]
//! mode = "inherit"
//! source = "clipboard.copy"
//! ```
//!
//! 옛 파일에 남은 `[grants."<id>"]` 표는 읽지 않는다. 저장은 바뀐 칸만 고쳐 쓰므로 그 표는
//! 지우지 않고 그대로 둔다. 플러그인 권한 허가는 더 이상 동작에 쓰이지 않는다.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "plugins.toml";

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct PluginsConfig {
    #[serde(skip)]
    pub(crate) origin: Option<crate::file_persistence::Origin>,
    #[serde(default)]
    pub disabled: PluginsDisabled,
    /// 사용자가 제거한 기본 제공 플러그인 id 목록. 다음 부팅 시 bundle에서
    /// 자동 재설치되지 않도록 차단한다.
    #[serde(default)]
    pub removed_builtins: PluginsDisabled,
    /// plugin id → command id → 사용자가 매니페스트 기본값을 덮어쓴 단축키 설정.
    /// 항목이 없으면 매니페스트의 `default_keybinding` + `binding_mode`가 그대로 적용.
    #[serde(default)]
    pub keybindings: BTreeMap<String, BTreeMap<String, ShortcutOverride>>,
    /// `keybindings` 가 마지막으로 바뀐 시점의 전역 epoch(직렬화 대상 아님).
    /// 파생 스냅샷 캐시의 무효화 키 — `shortcut_revision()` 참조.
    #[serde(skip)]
    shortcut_revision: u64,
}

/// 사용자가 plugin 단축키를 어떻게 덮어쓰는지 표현.
///
/// - `Key { value }`: plugin 매니페스트가 inherit를 선언했더라도 사용자가
///   독립 키로 떼어낸 경우 또는 매니페스트가 independent였던 경우.
/// - `Inherit { source }`: plugin이 inherit를 선언한 command를 사용자가
///   그대로 두거나, plugin이 inherit 가능한 host action으로 명시 변경한 경우.
/// - `None`: 사용자가 의도적으로 단축키를 비워둠. 매니페스트 기본값보다 우선.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ShortcutOverride {
    Key { value: Vec<String> },
    Inherit { source: String },
    None,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct PluginsDisabled {
    #[serde(default)]
    pub ids: Vec<String>,
}

impl PluginsConfig {
    pub(crate) fn path() -> Option<PathBuf> {
        tasty_utils::path::tasty_home().map(|d| d.join(FILE_NAME))
    }

    pub fn load() -> Self {
        let path = match Self::path() {
            Some(p) => p,
            None => return Self::default(),
        };
        Self::load_from(path)
    }

    pub fn load_from(path: PathBuf) -> Self {
        let loaded = crate::file_persistence::read(&path)
            .and_then(|table| crate::file_persistence::decode::<Self>(&table));
        let (mut config, baseline) = match loaded {
            Ok(config) => {
                let baseline = crate::file_persistence::encode(&config).map_err(|e| e.to_string());
                (config, baseline)
            }
            Err(error) => {
                tracing::warn!(file = "plugins.toml", cause = %error, "plugin settings unavailable; protecting original");
                (Self::default(), Err(error.to_string()))
            }
        };
        config.origin = Some(crate::file_persistence::Origin::new(path, baseline));
        config
    }

    pub fn save(&self) -> anyhow::Result<()> {
        crate::file_persistence::save_config(self)?;
        Ok(())
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.ids.iter().any(|x| x == id)
    }

    pub fn is_builtin_removed(&self, id: &str) -> bool {
        self.removed_builtins.ids.iter().any(|x| x == id)
    }

    pub fn mark_builtin_removed(&mut self, id: &str) -> bool {
        if self.is_builtin_removed(id) {
            false
        } else {
            self.removed_builtins.ids.push(id.to_string());
            true
        }
    }

    /// removed_builtins 마크를 해제. `mark_builtin_removed` 의 역동작.
    /// graceful swap 시 자동 호출되지 않고, 명시적 `upgrade-builtins --restore-removed`
    /// 경로에서만 호출된다.
    pub fn unmark_builtin_removed(&mut self, id: &str) -> bool {
        let before = self.removed_builtins.ids.len();
        self.removed_builtins.ids.retain(|x| x != id);
        before != self.removed_builtins.ids.len()
    }

    /// removed_builtins 전체 clear. `--restore-removed-all` 진입점.
    pub fn clear_removed_builtins(&mut self) -> bool {
        let before = self.removed_builtins.ids.len();
        self.removed_builtins.ids.clear();
        before != 0
    }

    pub fn enable(&mut self, id: &str) -> bool {
        let before = self.disabled.ids.len();
        self.disabled.ids.retain(|x| x != id);
        before != self.disabled.ids.len()
    }

    pub fn disable(&mut self, id: &str) -> bool {
        if self.is_disabled(id) {
            false
        } else {
            self.disabled.ids.push(id.to_string());
            true
        }
    }

    /// 사용자가 plugin command 단축키에 적용한 override를 조회. 없으면 None.
    pub fn shortcut_override(
        &self,
        plugin_id: &str,
        command_id: &str,
    ) -> Option<&ShortcutOverride> {
        self.keybindings
            .get(plugin_id)
            .and_then(|m| m.get(command_id))
    }

    /// 단축키 변경 시 갱신하는 번호. 파생 스냅샷의 캐시를 무효화하는 데 쓴다.
    /// 디스크에서 읽은 설정의 초기값은 0이다.
    pub fn shortcut_revision(&self) -> u64 {
        self.shortcut_revision
    }

    /// 내보내기에 사용할 전체 override. 등록된 명령만 있는 UI 스냅샷과 달리
    /// 비활성·미등록 플러그인의 설정도 포함한다.
    pub fn shortcut_overrides(&self) -> &BTreeMap<String, BTreeMap<String, ShortcutOverride>> {
        &self.keybindings
    }

    /// 가져온 override로 전체 목록을 교체하고 shortcut_revision을 갱신한다.
    pub fn replace_shortcut_overrides(
        &mut self,
        overrides: BTreeMap<String, BTreeMap<String, ShortcutOverride>>,
    ) {
        self.keybindings = overrides;
        self.shortcut_revision = crate::command_registry::next_shortcut_epoch();
    }

    /// override를 설정. 같은 키가 있으면 덮어씀.
    pub fn set_shortcut_override(
        &mut self,
        plugin_id: &str,
        command_id: &str,
        ov: ShortcutOverride,
    ) {
        self.keybindings
            .entry(plugin_id.to_string())
            .or_default()
            .insert(command_id.to_string(), ov);
        self.shortcut_revision = crate::command_registry::next_shortcut_epoch();
    }

    /// override를 제거 (매니페스트 기본값으로 되돌림). 실제 제거됐는지 반환.
    pub fn clear_shortcut_override(&mut self, plugin_id: &str, command_id: &str) -> bool {
        let Some(map) = self.keybindings.get_mut(plugin_id) else {
            return false;
        };
        let removed = map.remove(command_id).is_some();
        if map.is_empty() {
            self.keybindings.remove(plugin_id);
        }
        if removed {
            self.shortcut_revision = crate::command_registry::next_shortcut_epoch();
        }
        removed
    }
}

/// `ShortcutOverride` 를 `command.shortcut_changed` payload 용 단순 문자열로 변환.
/// `Key` 모드의 다중 키는 `, ` 로 join, `Inherit` 는 `@source` 로 표기, 비어 있거나
/// `None` 모드는 `None` 반환. 정확한 상태는 plugin이 IPC로 재조회한다.
pub fn shortcut_override_display(ov: Option<&ShortcutOverride>) -> Option<String> {
    match ov? {
        ShortcutOverride::Key { value } if !value.is_empty() => Some(value.join(", ")),
        ShortcutOverride::Key { .. } => None,
        ShortcutOverride::Inherit { source } => Some(format!("@{source}")),
        ShortcutOverride::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_disable_round_trip() {
        let mut cfg = PluginsConfig::default();
        assert!(!cfg.is_disabled("com.example.x"));
        assert!(cfg.disable("com.example.x"));
        assert!(cfg.is_disabled("com.example.x"));
        assert!(!cfg.disable("com.example.x")); // 두 번째 disable은 false
        assert!(cfg.enable("com.example.x"));
        assert!(!cfg.is_disabled("com.example.x"));
        assert!(!cfg.enable("com.example.x")); // 이미 enabled
    }

    #[test]
    fn unmark_builtin_removed_after_mark() {
        let mut cfg = PluginsConfig::default();
        assert!(!cfg.is_builtin_removed("com.tasty.codex"));
        assert!(cfg.mark_builtin_removed("com.tasty.codex"));
        assert!(cfg.is_builtin_removed("com.tasty.codex"));
        assert!(cfg.unmark_builtin_removed("com.tasty.codex"));
        assert!(!cfg.is_builtin_removed("com.tasty.codex"));
        // idempotent
        assert!(!cfg.unmark_builtin_removed("com.tasty.codex"));
    }

    #[test]
    fn clear_removed_builtins_empties_all() {
        let mut cfg = PluginsConfig::default();
        cfg.mark_builtin_removed("a");
        cfg.mark_builtin_removed("b");
        assert!(cfg.clear_removed_builtins());
        assert!(!cfg.is_builtin_removed("a"));
        assert!(!cfg.is_builtin_removed("b"));
        assert!(!cfg.clear_removed_builtins());
    }

    /// 옛 `[grants]` 표가 남아 있어도 읽기를 거절하지 않고 다른 칸은 그대로 읽는다.
    #[test]
    fn leftover_grants_section_is_ignored() {
        let s = r#"
            [disabled]
            ids = ["com.example.y"]

            [grants."com.example.x"]
            granted = ["fs.read", "surface.write"]
        "#;
        let cfg: PluginsConfig = toml::from_str(s).unwrap();
        assert!(cfg.is_disabled("com.example.y"));
        assert!(!cfg.is_disabled("com.example.x"));
    }

    #[test]
    fn parses_disabled_list() {
        let s = r#"
            [disabled]
            ids = ["com.example.broken", "com.foo.bar"]
        "#;
        let cfg: PluginsConfig = toml::from_str(s).unwrap();
        assert!(cfg.is_disabled("com.example.broken"));
        assert!(cfg.is_disabled("com.foo.bar"));
        assert!(!cfg.is_disabled("com.example.good"));
    }

    #[test]
    fn shortcut_override_round_trip() {
        let mut cfg = PluginsConfig::default();
        assert!(
            cfg.shortcut_override("com.example.x", "x.refresh")
                .is_none()
        );
        cfg.set_shortcut_override(
            "com.example.x",
            "x.refresh",
            ShortcutOverride::Key {
                value: vec!["F6".into()],
            },
        );
        match cfg.shortcut_override("com.example.x", "x.refresh") {
            Some(ShortcutOverride::Key { value }) => assert_eq!(value, &vec!["F6".to_string()]),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(cfg.clear_shortcut_override("com.example.x", "x.refresh"));
        assert!(
            cfg.shortcut_override("com.example.x", "x.refresh")
                .is_none()
        );
        // plugin 항목이 비면 자체 제거됨
        assert!(!cfg.keybindings.contains_key("com.example.x"));
    }

    #[test]
    fn shortcut_override_serialization() {
        let mut cfg = PluginsConfig::default();
        cfg.set_shortcut_override(
            "com.example.x",
            "x.refresh",
            ShortcutOverride::Key {
                value: vec!["F6".into()],
            },
        );
        cfg.set_shortcut_override(
            "com.example.x",
            "x.copy",
            ShortcutOverride::Inherit {
                source: "clipboard.copy".into(),
            },
        );
        cfg.set_shortcut_override("com.example.x", "x.paste", ShortcutOverride::None);
        let s = toml::to_string_pretty(&cfg).unwrap();
        let restored: PluginsConfig = toml::from_str(&s).unwrap();
        assert!(matches!(
            restored.shortcut_override("com.example.x", "x.refresh"),
            Some(ShortcutOverride::Key { .. })
        ));
        assert!(matches!(
            restored.shortcut_override("com.example.x", "x.copy"),
            Some(ShortcutOverride::Inherit { .. })
        ));
        assert!(matches!(
            restored.shortcut_override("com.example.x", "x.paste"),
            Some(ShortcutOverride::None)
        ));
    }
}
