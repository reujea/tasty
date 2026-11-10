mod anomaly_detection;
mod appearance;
/// 단축키 설정 — `KeybindingSettings` 와 그 값의 해석 규칙([`keybindings::parse`]).
pub mod keybindings;
pub mod merge;
mod persistence;
mod port;
mod port_impl;
mod remote_attach;
mod scripts;
pub mod shell_path;
mod task_pipeline;
pub mod terminal_input;
mod types;

pub mod general;
pub mod testing;
pub mod webhook_port_file;

use serde::{Deserialize, Serialize};

pub use anomaly_detection::*;
pub use persistence::FileObservation;
pub use remote_attach::*;

pub use appearance::{
    ActiveTabIndicator, AppearanceSettings, EffectiveFont, FontOverride, FontSettings, HexColor,
    PluginSettingValue, UI_SCALE_CHOICES,
};
pub use general::{DEFAULT_WHEEL_LINE_SCROLL, GeneralSettings, LinkModifier};
pub use keybindings::crud::{SwitchAxis, SwitchStep};
pub use keybindings::{KeybindingSettings, ScriptBinding};
pub use port::SettingsStorage;
pub use port_impl::FileSettingsStorage;
pub use scripts::{
    AUTO_TRIGGER_EVENTS, AutoTrigger, ScriptEntry, ScriptRegistry, hash_bytes, hash_file,
    is_auto_trigger_event,
};
pub use shell_path::{BootShell, ResolvedShell, ShellPathStatus};
pub use task_pipeline::{
    DEFAULT_REPORT_APPEND_BYTES, DEFAULT_REPORT_BLOCK_BYTES, REPORT_APPEND_BYTES_RANGE,
    REPORT_BLOCK_BYTES_RANGE, TaskPipelineSettings,
};
pub use types::{
    AccessibilitySettings, DEFAULT_SCROLLBACK_DISK_LINES, MemorySettings, ModifierHintSettings,
    NotificationSettings, OverlaySettings, PerformanceSettings, RemoteTransferSettings,
    UNLIMITED_SCROLLBACK_DISK_LINES, WebhookSettings, disk_scrollback_cap,
};

/// 로드 출처에 따른 저장 정책. 파싱 실패로 기본값을 쓸 때 원본을 바로 덮어쓰지 않는다.
/// 로드는 파일을 옮기지 않으며 실제 저장 전에 필요한 백업을 만든다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsOrigin {
    /// 파일을 정상적으로 읽었거나(부재 포함), 프로그램이 만든 값. 저장해도 잃을 것이 없다.
    #[default]
    Clean,
    /// 원본이 그 자리에 있는데 해석하지 못해 기본값으로 폴백했다. 저장 직전에 원본을
    /// `.bak` 으로 옮기고, 옮기지 못하면 저장을 거부한다.
    Unparsable,
    /// 원본을 **읽지도** 못했다(권한 · IO). 내용을 모르므로 옮기지도 않고, 저장만 막는다.
    ProtectedUnreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Settings {
    pub terminal_input: terminal_input::TerminalInputSettings,
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub notification: NotificationSettings,
    pub keybindings: KeybindingSettings,
    pub performance: PerformanceSettings,
    pub memory: MemorySettings,
    pub accessibility: AccessibilitySettings,
    /// 토스트 등 오버레이 표시 설정.
    pub overlay: OverlaySettings,
    /// Modifier 키 홀드 안내 오버레이의 표시 토글 + 위치·크기 영속 슬롯.
    /// `#[serde(default)]` 로 기존 config.toml 마이그레이션 안전(누락 시 enabled=true, pos/size=None).
    pub modifier_hint: ModifierHintSettings,
    /// 원격 전송(bulk 파일 전송 채널, docs/dev-guide/attach-behavior.md#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그) 수신측 저장 폴더 + 용량 상한.
    /// `#[serde(default)]` 로 기존 config.toml 마이그레이션 안전(누락 시 dir="", max_mb=500).
    pub remote_transfer: RemoteTransferSettings,
    /// 원격 attach 연결의 무응답 끊김 시간.
    pub remote_attach: RemoteAttachSettings,
    /// 웹훅 리스너의 외부 수신 허용. `#[serde(default)]`로 누락 시 loopback만 받는다.
    pub webhook: WebhookSettings,
    /// 작업 파이프라인의 report 상한. 누락 시 기본값이다.
    pub task_pipeline: TaskPipelineSettings,
    pub anomaly_detection: AnomalyDetectionSettings,
    /// Plugin-contributed settings page 의 generic 값 저장소.
    /// `plugin_settings[plugin_id][storage_key]` = `PluginSettingValue`.
    /// FontOverride(`appearance.plugin_font_overrides`)와 별개 네임스페이스.
    /// 키 부재 시 host 렌더러가 manifest item 의 default 로 fallback 한다.
    /// `#[serde(default)]` 로 기존 config.toml 마이그레이션 안전(누락 시 빈 맵).
    pub plugin_settings:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, PluginSettingValue>>,
    /// 사용자 등록 Lua 스크립트 목록 (docs/features/lua-hooks/index.md#실행-격리--안전-장치). 단축키 트리거·관리 창·TOFU 게이트의 기반.
    /// `#[serde(default)]` 로 기존 config.toml 마이그레이션 안전(누락 시 빈 목록).
    pub scripts: ScriptRegistry,
    /// 이 값의 출처 — 원본 파일을 덮어써도 되는지. 디스크에 나가지 않는 런타임 상태라
    /// `#[serde(skip)]`. clone 을 타고 전파되므로 어느 복사본으로 저장하든 판정이 같다.
    #[serde(skip)]
    pub origin: SettingsOrigin,
    /// Shared by snapshots from one load so own saves advance their external-edit observation.
    #[serde(skip)]
    pub file_observation: Option<std::sync::Arc<std::sync::Mutex<FileObservation>>>,
}

impl Settings {
    /// Admission과 live 갱신에서 쓰는 backend 설정 검증. 파일 로드는 normalize로 복구한다.
    pub fn validate_runtime(&self) -> Result<(), String> {
        self.task_pipeline.validate()?;
        self.anomaly_detection.validate()?;
        self.remote_attach.validate()
    }

    /// 전역 Theme 설치에 필요한 UI 배율·모션 설정을 함께 반환한다.
    pub fn theme_runtime(&self) -> tasty_themes::ThemeRuntime {
        tasty_themes::ThemeRuntime {
            ui_zoom: self.appearance.ui_scale_factor(),
            reduced_motion: self.accessibility.reduced_motion,
        }
    }

    /// Plugin settings 슬롯 조회. 부재 시 `None` — 호출자가 manifest default 로 fallback.
    pub fn plugin_setting(
        &self,
        plugin_id: &str,
        storage_key: &str,
    ) -> Option<&PluginSettingValue> {
        self.plugin_settings
            .get(plugin_id)
            .and_then(|m| m.get(storage_key))
    }

    /// Plugin settings 슬롯 write. 영속(save)은 기존 Settings 흐름이 담당.
    pub fn set_plugin_setting(
        &mut self,
        plugin_id: &str,
        storage_key: &str,
        value: PluginSettingValue,
    ) {
        self.plugin_settings
            .entry(plugin_id.to_string())
            .or_default()
            .insert(storage_key.to_string(), value);
    }
}

impl Settings {
    /// Validate enum-like string fields and replace invalid values with safe defaults.
    /// Returns a report so the caller can decide whether to save() and whether to
    /// surface a popup (theme only). Non-theme fields are fixed silently with a warn log.
    ///
    /// `general.language` is intentionally NOT normalized — users may install a
    /// language pack at `~/.tasty/lang/{code}/pack.toml` (a single `{code}.toml` only
    /// overrides a built-in language), so any code is potentially valid. Shape and
    /// discovery rules: `docs/features/language-packs/index.md`.
    pub fn normalize(&mut self) -> NormalizeReport {
        let mut report = NormalizeReport::default();

        // appearance.theme — legacy id 매핑.
        // 실제 valid 검증/fallback 은 부팅 흐름의 `tasty_themes::apply_theme()` 가 담당.
        // (settings 가 어떤 id 가 valid 인지 직접 알 필요 없음.)
        match self.appearance.theme.as_str() {
            "catppuccin-mocha" => {
                self.appearance.theme = "mocha".to_string();
                tracing::info!("migrated legacy theme id catppuccin-mocha → mocha");
                report.changed = true;
            }
            "catppuccin-latte" => {
                self.appearance.theme = "latte".to_string();
                tracing::info!("migrated legacy theme id catppuccin-latte → latte");
                report.changed = true;
            }
            _ => {}
        }

        // 배율 선택 목록을 공통 상수와 대조한다.
        normalize_choice(
            &mut self.appearance.ui_scale,
            UI_SCALE_CHOICES,
            "medium",
            "ui_scale",
            &mut report.changed,
        );

        // appearance.default_font.font_scale_mode
        normalize_choice(
            &mut self.appearance.default_font.font_scale_mode,
            &["auto", "fixed"],
            "auto",
            "default_font.font_scale_mode",
            &mut report.changed,
        );

        // appearance.{terminal,markdown,explorer}_font.font_scale_mode (Option<String>)
        // markdown_font / explorer_font are legacy fields kept for one-shot
        // migration; validating them here means a normalize() call made *before*
        // migration still catches bad values.
        for (label, opt) in [
            (
                "terminal_font.font_scale_mode",
                &mut self.appearance.terminal_font.font_scale_mode,
            ),
            (
                "markdown_font.font_scale_mode",
                &mut self.appearance.markdown_font.font_scale_mode,
            ),
            (
                "explorer_font.font_scale_mode",
                &mut self.appearance.explorer_font.font_scale_mode,
            ),
        ] {
            normalize_optional_font_scale_mode(opt, label, &mut report.changed);
        }

        // plugin_font_overrides.<kind>.font_scale_mode (Option<String>)
        for (kind, ov) in self.appearance.plugin_font_overrides.iter_mut() {
            let label = format!("plugin_font_overrides.{kind}.font_scale_mode");
            normalize_optional_font_scale_mode(
                &mut ov.font_scale_mode,
                &label,
                &mut report.changed,
            );
        }

        // general.close_behavior
        normalize_choice(
            &mut self.general.close_behavior,
            &["ask", "minimize", "quit"],
            "ask",
            "close_behavior",
            &mut report.changed,
        );

        // general.link_click_modifier
        normalize_choice(
            &mut self.general.link_click_modifier,
            &["ctrl", "alt", "none"],
            "ctrl",
            "link_click_modifier",
            &mut report.changed,
        );

        // general.explorer_view_mode
        normalize_choice(
            &mut self.general.explorer_view_mode,
            &["grid", "list", "detail"],
            "detail",
            "explorer_view_mode",
            &mut report.changed,
        );

        // general.alt_display_style / option_display_style / shift_display_style
        normalize_choice(
            &mut self.general.alt_display_style,
            &["alt", "cmd", "symbol"],
            "alt",
            "alt_display_style",
            &mut report.changed,
        );
        normalize_choice(
            &mut self.general.option_display_style,
            &["option", "symbol"],
            "option",
            "option_display_style",
            &mut report.changed,
        );
        normalize_choice(
            &mut self.general.shift_display_style,
            &["shift", "symbol"],
            "shift",
            "shift_display_style",
            &mut report.changed,
        );

        self.performance.normalize(&mut report.changed);
        self.task_pipeline.normalize(&mut report.changed);
        self.anomaly_detection.normalize(&mut report.changed);
        self.remote_attach.normalize(&mut report.changed);

        report
    }
}

/// Result of [`Settings::normalize`].
#[derive(Debug, Default)]
pub struct NormalizeReport {
    /// Original (invalid) theme name if it was replaced. Caller may surface a popup.
    pub invalid_theme_name: Option<String>,
    /// True if any field was changed by normalize. Caller should `save()` to persist.
    pub changed: bool,
}

fn normalize_choice(
    field: &mut String,
    allowed: &[&str],
    fallback: &str,
    label: &str,
    changed: &mut bool,
) {
    if !allowed.contains(&field.as_str()) {
        let invalid = std::mem::take(field);
        *field = fallback.to_string();
        tracing::warn!("invalid {label} \"{invalid}\" → {fallback}");
        *changed = true;
    }
}

/// `font_scale_mode` 류 `Option<String>` 필드가 `Some(x)` 인데 x 가 허용 값
/// (`auto`/`fixed`) 이 아니면 unset(`None`) 한다. `None` 은 그대로 유효(미지정).
fn normalize_optional_font_scale_mode(opt: &mut Option<String>, label: &str, changed: &mut bool) {
    let Some(mode) = opt.as_deref() else {
        return;
    };
    if matches!(mode, "auto" | "fixed") {
        return;
    }
    let invalid = opt.take().unwrap_or_default();
    tracing::warn!("invalid {label} \"{invalid}\" → unset");
    *changed = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_valid() {
        let settings = Settings::default();
        assert!(!settings.general.shell.is_empty());
        assert!(settings.appearance.default_font.font_size > 0.0);
        assert!(
            settings.appearance.scaled_sidebar_width()
                > tasty_type_geometry::length::LogicalPx(0.0)
        );
    }

    #[test]
    fn retired_sidebar_width_is_read_without_error_and_dropped_on_save() {
        let contents = "[appearance]\nsidebar_width = 260.0\nui_scale = \"medium\"\n";
        let settings = Settings::parse_with_migration(contents).expect("old config still parses");
        assert_eq!(
            settings.appearance.scaled_sidebar_width(),
            tasty_type_geometry::length::LogicalPx(180.0)
        );
        let raw: toml::Value = toml::from_str(contents).unwrap();
        assert_eq!(
            Settings::retired_keys_in(&raw),
            vec![(
                "appearance.sidebar_width".to_string(),
                "appearance.ui_scale"
            )]
        );
        let saved = toml::to_string_pretty(&settings).unwrap();
        assert!(!saved.contains("sidebar_width"), "got: {saved}");
    }

    /// 없어진 shell_mode 키는 경고·정규화 없이 무시되고 저장 때 남지 않는다.
    #[test]
    fn a_legacy_shell_mode_key_loads_without_a_report_and_is_dropped_on_save() {
        let contents = "[general]\nshell_mode = \"tasty\"\n";
        let mut settings =
            Settings::parse_with_migration(contents).expect("old config still parses");
        let raw: toml::Value = toml::from_str(contents).unwrap();
        assert!(Settings::retired_keys_in(&raw).is_empty());
        assert!(!settings.normalize().changed);
        let saved = toml::to_string_pretty(&settings).unwrap();
        assert!(!saved.contains("shell_mode"), "got: {saved}");
    }

    #[test]
    fn sidebar_width_follows_ui_scale_only() {
        for (scale, width) in [("small", 150.0), ("medium", 180.0), ("large", 220.0)] {
            let contents = format!("[appearance]\nui_scale = \"{scale}\"\n");
            let settings = Settings::parse_with_migration(&contents).unwrap();
            assert_eq!(
                settings.appearance.scaled_sidebar_width(),
                tasty_type_geometry::length::LogicalPx(width)
            );
            let raw: toml::Value = toml::from_str(&contents).unwrap();
            assert!(Settings::retired_keys_in(&raw).is_empty());
        }
    }

    #[test]
    fn settings_serialization_roundtrip() {
        let settings = Settings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            parsed.appearance.default_font.font_size,
            settings.appearance.default_font.font_size
        );
        assert_eq!(parsed.general.shell, settings.general.shell);
        assert_eq!(
            parsed.notification.coalesce_ms,
            settings.notification.coalesce_ms
        );
    }

    #[test]
    fn settings_partial_toml_uses_defaults() {
        let partial = r#"
[appearance]
ui_scale = "large"
"#;
        let parsed: Settings = toml::from_str(partial).unwrap();
        assert_eq!(parsed.appearance.ui_scale, "large");
        // Other fields fall back to defaults.
        assert_eq!(parsed.appearance.default_font.font_size, 14.0);
        assert!(parsed.notification.enabled);
        assert!(!parsed.general.shell.is_empty());
    }

    /// 여러 프로세스가 같은 원본을 읽을 수 있도록 로드만으로 파일을 옮기지 않는다.
    #[test]
    fn unparsable_settings_file_is_left_in_place_by_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not = valid toml [[[").unwrap();

        for _ in 0..2 {
            let loaded = Settings::load_from_path(&path);
            assert_eq!(
                loaded.origin,
                SettingsOrigin::Unparsable,
                "몇 번을 읽어도 같은 사실을 봐야 한다"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "this is not = valid toml [[[",
            "로드는 원본을 옮기지 않는다"
        );
        assert!(!tmp.path().join("config.toml.bak").exists());
    }

    /// 보존은 실제로 덮어쓰는 순간에 일어난다. 저장 뒤 원본은 `.bak` 에, 새 값은 원래
    /// 자리에 있어야 한다.
    #[test]
    fn saving_over_an_unparsable_file_preserves_it_first() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not = valid toml [[[").unwrap();

        let loaded = Settings::load_from_path(&path);
        loaded.save_to_path(&path).unwrap();

        let backup = tmp.path().join("config.toml.bak");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "this is not = valid toml [[[",
            "원본이 백업에 그대로 남아야 한다"
        );
        assert!(
            Settings::load_from_path(&path).origin == SettingsOrigin::Clean,
            "새로 쓴 파일은 정상이어야 한다"
        );
        // 두 번째 저장은 방금 쓴 정상 파일을 백업으로 옮기지 않는다.
        loaded.save_to_path(&path).unwrap();
        assert!(!tmp.path().join("config.toml.bak.2").exists());
    }

    /// 파일이 없는 것은 정상이다 — 첫 실행. 저장을 막을 이유가 없다.
    #[test]
    fn missing_settings_file_is_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = Settings::load_from_path(&tmp.path().join("config.toml"));
        assert_eq!(loaded.origin, SettingsOrigin::Clean);
    }

    /// 읽기 실패는 원본을 옮기지 않고 저장을 거절한다.
    #[cfg(unix)]
    #[test]
    fn unreadable_settings_file_blocks_save_and_is_left_in_place() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[general]\nshell = \"/bin/zsh\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let loaded = Settings::load_from_path(&path);
        assert_eq!(loaded.origin, SettingsOrigin::ProtectedUnreadable);
        assert!(
            loaded.save_to_path(&path).is_err(),
            "읽지 못한 파일 위에 기본값을 쓰면 안 된다"
        );

        // OS 쓰기 권한이 아닌 origin 검사로 거절하는지 보려고 쓰기 가능한 경로에도 저장한다.
        let writable = tmp.path().join("elsewhere.toml");
        assert!(
            loaded.save_to_path(&writable).is_err(),
            "쓸 수 있는 자리라도 거부해야 한다 — 거부의 근거는 파일 권한이 아니라 \
             '원본을 읽지 못했다' 는 판정이다"
        );
        assert!(!writable.exists(), "거부했으면 파일을 만들지도 않는다");
        // 정상 origin은 같은 경로에 저장할 수 있어야 한다.
        Settings::default()
            .save_to_path(&writable)
            .expect("Clean 인 설정은 쓸 수 있어야 한다");
        assert!(writable.exists());

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[general]\nshell = \"/bin/zsh\"\n",
            "원본이 그대로 있어야 한다"
        );
        assert!(
            !tmp.path().join("config.toml.bak").exists(),
            "백업도 만들지 않는다"
        );
    }

    #[test]
    fn valid_settings_file_stays_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[general]\nshell = \"/bin/zsh\"\n").unwrap();
        let loaded = Settings::load_from_path(&path);
        assert_eq!(loaded.origin, SettingsOrigin::Clean);
        assert_eq!(loaded.general.shell, "/bin/zsh");
    }

    #[test]
    fn settings_empty_toml_uses_all_defaults() {
        let parsed: Settings = toml::from_str("").unwrap();
        let defaults = Settings::default();
        assert_eq!(
            parsed.appearance.default_font.font_size,
            defaults.appearance.default_font.font_size
        );
        assert_eq!(
            parsed.notification.coalesce_ms,
            defaults.notification.coalesce_ms
        );
    }

    #[test]
    fn settings_font_family_default() {
        let settings = Settings::default();
        assert_eq!(settings.appearance.default_font.font_family, "");
    }

    #[test]
    fn settings_theme_default() {
        let settings = Settings::default();
        assert_eq!(settings.appearance.theme, "mocha");
    }

    #[test]
    fn settings_background_opacity_default() {
        let settings = Settings::default();
        assert_eq!(settings.appearance.background_opacity, 1.0);
    }

    #[test]
    fn settings_custom_appearance_roundtrip() {
        let mut settings = Settings::default();
        settings.appearance.default_font.font_family = "Fira Code".to_string();
        settings.appearance.default_font.font_size = 18.0;
        settings.appearance.terminal_font.font_size = Some(20.0);
        settings.appearance.theme = "light".to_string();
        settings.appearance.background_opacity = 0.8;
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.appearance.default_font.font_family, "Fira Code");
        assert_eq!(parsed.appearance.default_font.font_size, 18.0);
        assert_eq!(parsed.appearance.terminal_font.font_size, Some(20.0));
        assert_eq!(parsed.appearance.theme, "light");
        assert_eq!(parsed.appearance.background_opacity, 0.8);
    }

    #[test]
    fn settings_keybindings_default() {
        let settings = Settings::default();
        assert_eq!(
            settings.keybindings.get_field("new_workspace"),
            Some("alt+n")
        );
        assert_eq!(settings.keybindings.get_field("new_tab"), Some("alt+t"));
    }

    #[test]
    fn scrollback_disk_lines_keeps_minus_one_and_resets_other_negatives() {
        let parse =
            |line: &str| -> Settings { toml::from_str(&format!("[performance]\n{line}")).unwrap() };
        assert_eq!(
            Settings::default().performance.scrollback_disk_cap(),
            Some(10_000)
        );
        let mut unlimited = parse("scrollback_disk_lines = -1");
        assert!(!unlimited.normalize().changed);
        assert_eq!(unlimited.performance.scrollback_disk_cap(), None);
        let mut zero = parse("scrollback_disk_lines = 0");
        assert!(!zero.normalize().changed);
        assert_eq!(zero.performance.scrollback_disk_cap(), Some(0));
        let mut invalid = parse("scrollback_disk_lines = -5");
        assert!(invalid.normalize().changed);
        assert_eq!(invalid.performance.scrollback_disk_lines, 10_000);
        assert_eq!(disk_scrollback_cap(-5), Some(10_000));
    }

    #[test]
    fn normalize_valid_settings_is_noop() {
        let mut settings = Settings::default();
        let report = settings.normalize();
        assert!(!report.changed);
        assert!(report.invalid_theme_name.is_none());
    }

    #[test]
    fn normalize_migrates_legacy_theme_ids() {
        for (legacy, expected) in [("catppuccin-mocha", "mocha"), ("catppuccin-latte", "latte")] {
            let mut settings = Settings::default();
            settings.appearance.theme = legacy.to_string();
            let report = settings.normalize();
            assert!(report.changed, "{legacy} should be migrated");
            assert_eq!(settings.appearance.theme, expected);
        }
    }

    #[test]
    fn normalize_does_not_touch_unknown_theme_ids() {
        // valid 검증은 부팅 흐름의 apply_theme 에서 처리. settings 는 단순 매핑만 한다.
        let mut settings = Settings::default();
        settings.appearance.theme = "my-custom-theme".to_string();
        let report = settings.normalize();
        // 마이그레이션 대상이 아니라 변경 없음 (다른 enum 필드들도 valid 였다고 가정).
        assert!(!report.changed);
        assert_eq!(settings.appearance.theme, "my-custom-theme");
    }

    #[test]
    fn normalize_current_ids_pass_through() {
        for theme in ["mocha", "latte"] {
            let mut settings = Settings::default();
            settings.appearance.theme = theme.to_string();
            let report = settings.normalize();
            assert!(!report.changed, "theme {theme} should be accepted as-is");
            assert_eq!(settings.appearance.theme, theme);
        }
    }

    #[test]
    fn normalize_silent_enum_fields_get_fixed() {
        let mut settings = Settings::default();
        settings.appearance.ui_scale = "huge".to_string();
        settings.appearance.default_font.font_scale_mode = "bogus".to_string();
        settings.appearance.terminal_font.font_scale_mode = Some("nope".to_string());
        settings.general.close_behavior = "explode".to_string();
        settings.general.link_click_modifier = "meta".to_string();
        settings.general.alt_display_style = "bogus".to_string();
        settings.general.option_display_style = "bogus".to_string();
        settings.general.shift_display_style = "bogus".to_string();

        let report = settings.normalize();
        assert!(report.changed);
        assert!(report.invalid_theme_name.is_none());
        assert_eq!(settings.appearance.ui_scale, "medium");
        assert_eq!(settings.appearance.default_font.font_scale_mode, "auto");
        assert!(settings.appearance.terminal_font.font_scale_mode.is_none());
        assert_eq!(settings.general.close_behavior, "ask");
        assert_eq!(settings.general.link_click_modifier, "ctrl");
        assert_eq!(settings.general.alt_display_style, "alt");
        assert_eq!(settings.general.option_display_style, "option");
        assert_eq!(settings.general.shift_display_style, "shift");
    }

    #[test]
    fn normalize_does_not_touch_language() {
        let mut settings = Settings::default();
        settings.general.language = "fr".to_string();
        let report = settings.normalize();
        assert!(!report.changed);
        assert_eq!(settings.general.language, "fr");
    }

    #[test]
    fn default_targeted_pty_polling_is_on() {
        assert!(PerformanceSettings::default().targeted_pty_polling);
    }

    #[test]
    fn performance_missing_key_uses_new_default() {
        let parsed: Settings = toml::from_str("[performance]").unwrap();
        assert!(parsed.performance.targeted_pty_polling);
        let parsed: Settings = toml::from_str("").unwrap();
        assert!(parsed.performance.targeted_pty_polling);
    }

    #[test]
    fn performance_explicit_false_preserved() {
        let parsed: Settings =
            toml::from_str("[performance]\ntargeted_pty_polling = false").unwrap();
        assert!(!parsed.performance.targeted_pty_polling);
    }

    #[test]
    fn modifier_hint_default_enabled() {
        let settings = Settings::default();
        assert!(settings.modifier_hint.enabled);
        assert!(settings.modifier_hint.pos.is_none());
        assert!(settings.modifier_hint.size.is_none());
    }

    #[test]
    fn modifier_hint_missing_key_uses_defaults() {
        // 신규 키가 없는 구버전 config.toml 마이그레이션: enabled=true, pos/size=None.
        let parsed: Settings = toml::from_str("").unwrap();
        assert!(parsed.modifier_hint.enabled);
        assert!(parsed.modifier_hint.pos.is_none());
        assert!(parsed.modifier_hint.size.is_none());
        // 섹션은 있으나 키가 비어도 동일.
        let parsed: Settings = toml::from_str("[modifier_hint]").unwrap();
        assert!(parsed.modifier_hint.enabled);
    }

    #[test]
    fn remote_transfer_missing_key_uses_defaults() {
        // 신규 키가 없는 구버전 config.toml 마이그레이션: dir="", max_mb=500.
        let parsed: Settings = toml::from_str("").unwrap();
        assert_eq!(parsed.remote_transfer.dir, "");
        assert_eq!(parsed.remote_transfer.max_mb, 500);
        // 섹션은 있으나 키가 비어도 동일.
        let parsed: Settings = toml::from_str("[remote_transfer]").unwrap();
        assert_eq!(parsed.remote_transfer.dir, "");
        assert_eq!(parsed.remote_transfer.max_mb, 500);
    }

    #[test]
    fn remote_transfer_roundtrip() {
        let mut settings = Settings::default();
        settings.remote_transfer.dir = "/tmp/xfer".to_string();
        settings.remote_transfer.max_mb = 42;
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.remote_transfer.dir, "/tmp/xfer");
        assert_eq!(parsed.remote_transfer.max_mb, 42);
    }

    #[test]
    fn modifier_hint_enabled_false_preserved() {
        let parsed: Settings = toml::from_str("[modifier_hint]\nenabled = false").unwrap();
        assert!(!parsed.modifier_hint.enabled);
    }

    /// 전역 Theme에 전달할 설정을 함께 구성하는지 확인한다.
    #[test]
    fn theme_runtime_carries_every_settings_backed_value() {
        let mut settings = Settings::default();
        settings.accessibility.reduced_motion = true;
        let rt = settings.theme_runtime();
        assert!(rt.reduced_motion);
        assert_eq!(rt.ui_zoom, settings.appearance.ui_scale_factor());

        let off = Settings::default().theme_runtime();
        assert!(!off.reduced_motion);
    }

    #[test]
    fn modifier_hint_geometry_roundtrip() {
        use tasty_type_geometry::length::LogicalPx;
        let mut settings = Settings::default();
        settings.modifier_hint.enabled = false;
        settings.modifier_hint.pos = Some((LogicalPx(120.0), LogicalPx(340.0)));
        settings.modifier_hint.size = Some((LogicalPx(280.0), LogicalPx(160.0)));
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&toml_str).unwrap();
        assert!(!parsed.modifier_hint.enabled);
        assert_eq!(
            parsed.modifier_hint.pos,
            Some((LogicalPx(120.0), LogicalPx(340.0)))
        );
        assert_eq!(
            parsed.modifier_hint.size,
            Some((LogicalPx(280.0), LogicalPx(160.0)))
        );
    }
}
