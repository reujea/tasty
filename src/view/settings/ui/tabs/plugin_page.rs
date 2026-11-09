//! plugin 이 기여한 설정 페이지(`[[contributes.settings_pages]]`)를 공통 위젯으로 그린다.
//!
//! 항목은 매니페스트의 `kind` 별 컨트롤로 그린다. 묶음(`kind = "group"`)은 머리 줄을 그리고 그
//! 아래에 `group` 이 그 묶음인 항목을 모은다. 스위치 머리를 끄면 묶음의 항목을 그리지 않는다.
//! 스위치 값은 `toggle` 과 같은 `plugin_settings.<plugin_id>.<storage_key>` 칸에 bool 로 저장한다.
//! 읽기 전용 글(`kind = "text"`)은 저장값이나 기본 글을 보여 주기만 하고 쓰지 않는다.

use crate::i18n::t;
use crate::settings::{PluginSettingValue, Settings};
use tasty_plugin_manifest::{SettingsEntry, SettingsItemDecl, SettingsPageContribute};
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    SettingsRow, settings_group_header, settings_label_column, settings_text_row,
};

use super::appearance::{SurfaceFontTarget, draw_surface_font_section};
use super::font_rows::PreviewFonts;

/// 페이지에 그릴 한 줄.
#[derive(Debug, Clone, Copy)]
enum PageRow<'a> {
    /// 묶음 머리. `first` 면 페이지 맨 위라 위 구분선을 생략한다.
    Header {
        decl: &'a SettingsItemDecl,
        first: bool,
    },
    Item(&'a SettingsItemDecl),
}

/// 묶음 머리 스위치의 현재 값 — 스위치가 없으면 늘 펼친다. 저장값이 없으면 매니페스트 기본값이다.
fn group_open(settings: &Settings, plugin_id: &str, header: &SettingsItemDecl) -> bool {
    match header {
        SettingsItemDecl::Group {
            storage_key: Some(key),
            default,
            ..
        } => match settings.plugin_setting(plugin_id, key) {
            Some(PluginSettingValue::Bool(b)) => *b,
            _ => *default,
        },
        _ => true,
    }
}

/// 그릴 줄 목록. 꺼진 스위치 묶음의 항목은 빠진다. 머리를 이번 프레임에 누르면 다음 프레임에 반영된다.
fn page_rows<'a>(
    settings: &Settings,
    plugin_id: &str,
    page: &'a SettingsPageContribute,
) -> Vec<PageRow<'a>> {
    let mut rows = Vec::new();
    for (i, entry) in page.entries().into_iter().enumerate() {
        match entry {
            SettingsEntry::Item(item) => rows.push(PageRow::Item(item)),
            SettingsEntry::Group { header, members } => {
                rows.push(PageRow::Header {
                    decl: header,
                    first: i == 0,
                });
                if group_open(settings, plugin_id, header) {
                    rows.extend(members.into_iter().map(PageRow::Item));
                }
            }
        }
    }
    rows
}

/// plugin이 선언한 FontOverride·Toggle·Select·Number·Text 항목과 묶음 머리를 공통 위젯으로 그린다.
/// 글꼴은 plugin_font_overrides, 나머지는 plugin 설정 저장소의 storage_key를 사용한다.
pub(super) fn draw_plugin_settings_page(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
    plugin_id: &str,
    page: &SettingsPageContribute,
) {
    // 한 페이지의 행과 묶음 머리는 같은 라벨 열을 쓴다. 숨긴 행도 넣어 묶음을 접고 펴도 열이 안 움직인다.
    // 글꼴 override 는 자기 격자를 따로 그린다.
    let row_labels: Vec<SettingsRow<'_>> = page
        .items
        .iter()
        .filter_map(|item| match item {
            SettingsItemDecl::Toggle { label_key, .. }
            | SettingsItemDecl::Select { label_key, .. }
            | SettingsItemDecl::Number { label_key, .. }
            | SettingsItemDecl::Text { label_key, .. }
            | SettingsItemDecl::Group { label_key, .. } => Some(SettingsRow::new(t(label_key))),
            SettingsItemDecl::FontOverride { .. } => None,
        })
        .collect();
    let col = settings_label_column(ui, &crate::theme::theme(), &row_labels);
    for row in page_rows(settings, plugin_id, page) {
        match row {
            PageRow::Header { decl, first } => {
                draw_group_header(ui, settings, col, plugin_id, decl, first);
            }
            PageRow::Item(item) => draw_plugin_item(
                ui,
                settings,
                font_families,
                preview_fonts,
                plugin_id,
                col,
                item,
            ),
        }
    }
}

/// 묶음 머리 — 스위치가 있으면 그 값을 저장한다.
fn draw_group_header(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    col: LogicalPx,
    plugin_id: &str,
    decl: &SettingsItemDecl,
    first: bool,
) {
    let SettingsItemDecl::Group {
        label_key,
        storage_key,
        ..
    } = decl
    else {
        return;
    };
    let th = crate::theme::theme();
    let cur = group_open(settings, plugin_id, decl);
    let mut val = cur;
    let switch = storage_key.as_ref().map(|_| &mut val);
    settings_group_header(ui, &th, col, t(label_key), switch, first);
    if let Some(key) = storage_key
        && val != cur
    {
        settings.set_plugin_setting(plugin_id, key, PluginSettingValue::Bool(val));
    }
}

/// 묶음 머리가 아닌 항목 하나.
fn draw_plugin_item(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
    plugin_id: &str,
    col: LogicalPx,
    item: &SettingsItemDecl,
) {
    match item {
        SettingsItemDecl::FontOverride { storage_key, .. } => {
            draw_surface_font_section(
                ui,
                settings,
                font_families,
                preview_fonts,
                SurfaceFontTarget::Plugin {
                    storage_key: storage_key.as_str(),
                },
            );
        }
        SettingsItemDecl::Toggle {
            label_key,
            storage_key,
            default,
            ..
        } => {
            draw_plugin_toggle(
                ui,
                settings,
                col,
                plugin_id,
                label_key,
                storage_key,
                *default,
            );
        }
        SettingsItemDecl::Select {
            label_key,
            storage_key,
            options,
            default,
            ..
        } => {
            draw_plugin_select(
                ui,
                settings,
                col,
                plugin_id,
                label_key,
                storage_key,
                options,
                default,
            );
        }
        SettingsItemDecl::Number {
            label_key,
            storage_key,
            default,
            min,
            max,
            suffix_key,
            ..
        } => {
            draw_plugin_number(
                ui,
                settings,
                col,
                plugin_id,
                label_key,
                storage_key,
                *default,
                *min,
                *max,
                suffix_key.as_deref(),
            );
        }
        SettingsItemDecl::Text { label_key, .. } => {
            let th = crate::theme::theme();
            ui.add_space(th.spacing_sm.value());
            settings_text_row(
                ui,
                &th,
                col,
                t(label_key),
                &text_value(settings, plugin_id, item),
            );
        }
        SettingsItemDecl::Group { .. } => {}
    }
}

/// 읽기 전용 글에 보일 값 — 저장된 문자열(앞뒤 공백을 뺀 값, 공백뿐이면 없는 것), 없으면 번역한 기본 글, 그것도 없으면 `default`.
/// plugin 은 빈 값을 기본값으로 읽으므로 화면도 같은 규칙을 따른다.
fn text_value(settings: &Settings, plugin_id: &str, item: &SettingsItemDecl) -> String {
    let SettingsItemDecl::Text {
        storage_key,
        default,
        default_key,
        ..
    } = item
    else {
        return String::new();
    };
    let stored =
        storage_key
            .as_deref()
            .and_then(|key| match settings.plugin_setting(plugin_id, key) {
                Some(PluginSettingValue::Text(s)) if !s.trim().is_empty() => {
                    Some(s.trim().to_string())
                }
                _ => None,
            });
    stored.unwrap_or_else(|| match default_key.as_deref() {
        Some(key) if !key.is_empty() => t(key).to_string(),
        _ => default.clone(),
    })
}

/// plugin 설정 한 행. 페이지가 정한 라벨 열과 설정 행 격자를 쓴다.
fn plugin_setting_row(
    ui: &mut egui::Ui,
    col: LogicalPx,
    label: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    let th = crate::theme::theme();
    ui.add_space(th.spacing_sm.value());
    SettingsRow::new(label).show(ui, &th, col, control);
}

/// `Toggle` → 디자인 Switch (host `tasty_ui_widgets::switch`). bool read/write.
fn draw_plugin_toggle(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    col: LogicalPx,
    plugin_id: &str,
    label_key: &str,
    storage_key: &str,
    default: bool,
) {
    let cur = match settings.plugin_setting(plugin_id, storage_key) {
        Some(crate::settings::PluginSettingValue::Bool(b)) => *b,
        _ => default,
    };
    let mut val = cur;
    let th = crate::theme::theme();
    plugin_setting_row(ui, col, t(label_key), |ui| {
        tasty_ui_widgets::switch(ui, &th, &mut val, None, true);
    });
    if val != cur {
        settings.set_plugin_setting(
            plugin_id,
            storage_key,
            crate::settings::PluginSettingValue::Bool(val),
        );
    }
}

/// `Select` → 디자인 Select (host `tasty_ui_widgets::select`). options label_key 를
/// `t()` 로 표시하고 선택 `value`(String) read/write.
#[allow(clippy::too_many_arguments)] // 이유: 매니페스트 Select 선언의 필드 하나당 인자 하나 — 형제 draw_plugin_* 와 같은 모양
fn draw_plugin_select(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    col: LogicalPx,
    plugin_id: &str,
    label_key: &str,
    storage_key: &str,
    options: &[tasty_plugin_manifest::SelectOptionDecl],
    default: &str,
) {
    let cur = match settings.plugin_setting(plugin_id, storage_key) {
        Some(crate::settings::PluginSettingValue::Text(s)) => s.clone(),
        _ => default.to_string(),
    };
    let mut idx = options.iter().position(|o| o.value == cur).unwrap_or(0);
    let labels: Vec<String> = options
        .iter()
        .map(|o| t(&o.label_key).to_string())
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let th = crate::theme::theme();
    let salt = format!("plugin_select_{plugin_id}_{storage_key}");
    let mut changed = false;
    plugin_setting_row(ui, col, t(label_key), |ui| {
        changed = tasty_ui_widgets::select(
            ui,
            &th,
            &salt,
            &mut idx,
            &label_refs,
            th.field_width_md.value(),
            true,
        );
    });
    if changed && idx < options.len() {
        let new_val = options[idx].value.clone();
        if new_val != cur {
            settings.set_plugin_setting(
                plugin_id,
                storage_key,
                crate::settings::PluginSettingValue::Text(new_val),
            );
        }
    }
}

/// plugin 숫자 설정을 공통 숫자 입력으로 편집한다.
#[allow(clippy::too_many_arguments)]
fn draw_plugin_number(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    col: LogicalPx,
    plugin_id: &str,
    label_key: &str,
    storage_key: &str,
    default: f64,
    min: Option<f64>,
    max: Option<f64>,
    suffix_key: Option<&str>,
) {
    let cur = match settings.plugin_setting(plugin_id, storage_key) {
        Some(crate::settings::PluginSettingValue::Number(n)) => *n,
        _ => default,
    };
    let th = crate::theme::theme();
    let suffix = suffix_key.map(t);
    let spec = super::number::NumberSpec {
        min,
        max,
        step: None,
        // plugin 이 정수 범위를 선언했는지 알 길이 없으므로 현재 값의 모양을 따른다.
        decimals: usize::from(cur.fract() != 0.0),
        suffix,
        suffix_label: false,
        unit: None,
        no_limit: None,
        range_note: None,
        align: egui::Align::RIGHT,
        enabled: true,
    };

    let mut val = cur;
    let mut committed = false;
    plugin_setting_row(ui, col, t(label_key), |ui| {
        committed = super::number::number_field(
            ui,
            &th,
            ("plugin_number", plugin_id, storage_key),
            &spec,
            &mut val,
        );
    });

    if committed {
        settings.set_plugin_setting(
            plugin_id,
            storage_key,
            crate::settings::PluginSettingValue::Number(val),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{PageRow, page_rows, text_value};
    use crate::settings::{PluginSettingValue, Settings};
    use tasty_plugin_manifest::SettingsPageContribute;

    const PLUGIN: &str = "com.tasty.x";

    fn page(items: &str) -> SettingsPageContribute {
        let src = format!(
            r#"
            manifest_version = 1
            id = "{PLUGIN}"
            name = "X"
            version = "0.1.0"
            api_version = "1"
            permissions = ["ui.settings_page"]
            [entry]
            type = "process"
            command = "x"

            [[contributes.settings_pages]]
            id = "x"
            title_key = "x.title"
            category = "plugin"
{items}
            "#
        );
        let m: tasty_plugin_manifest::Manifest = toml::from_str(&src).expect("toml");
        m.validate().expect("valid");
        m.contributes
            .settings_pages
            .into_iter()
            .next()
            .expect("page")
    }

    const GROUPS: &str = r#"
            [[contributes.settings_pages.items]]
            kind = "group"
            id = "keep"
            label_key = "x.keep"

            [[contributes.settings_pages.items]]
            kind = "number"
            id = "turns"
            label_key = "x.turns"
            storage_key = "turns"
            group = "keep"

            [[contributes.settings_pages.items]]
            kind = "group"
            id = "sync"
            label_key = "x.sync"
            storage_key = "sync_on"

            [[contributes.settings_pages.items]]
            kind = "number"
            id = "tokens"
            label_key = "x.tokens"
            storage_key = "tokens"
            group = "sync"
    "#;

    fn shape(rows: &[PageRow<'_>]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                PageRow::Header { decl, first } => {
                    let SettingsItemDecl::Group { id, .. } = decl else {
                        panic!("header is not a group")
                    };
                    format!("[{id}{}]", if *first { " first" } else { "" })
                }
                PageRow::Item(item) => match item {
                    SettingsItemDecl::Number { id, .. } | SettingsItemDecl::Toggle { id, .. } => {
                        id.clone()
                    }
                    other => panic!("unexpected item {other:?}"),
                },
            })
            .collect()
    }
    use tasty_plugin_manifest::SettingsItemDecl;

    /// 스위치 묶음은 기본 켬이라 항목이 보이고, 끄면 그 묶음 항목만 빠진다. 스위치 없는 묶음은 늘 보인다.
    #[test]
    fn a_switched_off_group_hides_only_its_items() {
        let p = page(GROUPS);
        let mut s = Settings::default();
        assert_eq!(
            shape(&page_rows(&s, PLUGIN, &p)),
            ["[keep first]", "turns", "[sync]", "tokens"]
        );
        s.set_plugin_setting(PLUGIN, "sync_on", PluginSettingValue::Bool(false));
        assert_eq!(
            shape(&page_rows(&s, PLUGIN, &p)),
            ["[keep first]", "turns", "[sync]"]
        );
        s.set_plugin_setting(PLUGIN, "sync_on", PluginSettingValue::Bool(true));
        assert_eq!(page_rows(&s, PLUGIN, &p).len(), 4);
        // 다른 plugin 의 같은 키는 이 페이지와 무관하다.
        let mut other = Settings::default();
        other.set_plugin_setting("com.tasty.y", "sync_on", PluginSettingValue::Bool(false));
        assert_eq!(page_rows(&other, PLUGIN, &p).len(), 4);
    }

    /// 매니페스트 기본값이 꺼짐이면 저장값이 없을 때 숨긴다.
    #[test]
    fn a_group_default_off_starts_hidden() {
        let p = page(&GROUPS.replace(
            "storage_key = \"sync_on\"",
            "storage_key = \"sync_on\"\n            default = false",
        ));
        assert_eq!(
            shape(&page_rows(&Settings::default(), PLUGIN, &p)),
            ["[keep first]", "turns", "[sync]"]
        );
    }

    /// 묶음이 없는 페이지는 선언 순서대로 모두 그린다.
    #[test]
    fn a_page_without_groups_draws_every_item() {
        let p = page(
            r#"
            [[contributes.settings_pages.items]]
            kind = "toggle"
            id = "a"
            label_key = "x.a"
            storage_key = "a"

            [[contributes.settings_pages.items]]
            kind = "number"
            id = "b"
            label_key = "x.b"
            storage_key = "b"
            "#,
        );
        assert_eq!(
            shape(&page_rows(&Settings::default(), PLUGIN, &p)),
            ["a", "b"]
        );
    }

    /// 읽기 전용 글은 저장값(앞뒤 공백 제외)을 먼저 보이고, 없거나 공백뿐이면 기본 글을 보인다 — plugin 이
    /// 같은 규칙으로 값을 읽으므로 화면이 실제로 쓰는 문구와 갈리지 않는다.
    #[test]
    fn a_text_item_shows_the_stored_value_then_its_default() {
        let p = page(
            r#"
            [[contributes.settings_pages.items]]
            kind = "text"
            id = "sentinel"
            label_key = "x.sentinel"
            storage_key = "sentinel"
            default = "[[DONE]]"
            "#,
        );
        let item = &p.items[0];
        let mut s = Settings::default();
        assert_eq!(text_value(&s, PLUGIN, item), "[[DONE]]");
        s.set_plugin_setting(PLUGIN, "sentinel", PluginSettingValue::Text("  ".into()));
        assert_eq!(
            text_value(&s, PLUGIN, item),
            "[[DONE]]",
            "공백뿐인 값은 없는 것"
        );
        s.set_plugin_setting(
            PLUGIN,
            "sentinel",
            PluginSettingValue::Text(" [[OK]] ".into()),
        );
        assert_eq!(text_value(&s, PLUGIN, item), "[[OK]]");
        // 다른 종류의 저장값은 글로 읽지 않는다.
        s.set_plugin_setting(PLUGIN, "sentinel", PluginSettingValue::Bool(true));
        assert_eq!(text_value(&s, PLUGIN, item), "[[DONE]]");
    }

    /// 읽기 전용 글도 묶음 규칙을 따른다 — 스위치 없는 머리 아래 늘 보인다.
    #[test]
    fn a_text_item_sits_in_its_group() {
        let p = page(
            r#"
            [[contributes.settings_pages.items]]
            kind = "group"
            id = "gate"
            label_key = "x.gate"

            [[contributes.settings_pages.items]]
            kind = "text"
            id = "prompt"
            label_key = "x.prompt"
            default_key = "x.default_prompt"
            group = "gate"
            "#,
        );
        let rows = page_rows(&Settings::default(), PLUGIN, &p);
        assert_eq!(rows.len(), 2);
        assert!(matches!(
            rows[1],
            PageRow::Item(SettingsItemDecl::Text { .. })
        ));
    }
}
