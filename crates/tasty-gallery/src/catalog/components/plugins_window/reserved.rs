//! Attention › 예약 id. 디자인 Spec `plugin-reserved-id` 의 행 · 상세 세 상태 · 좁은 폭 · 언어 길이 · 알림 표본.
//! 본체 `src/view/plugins/ui/attention.rs` 와 같은 공용 위젯을 부른다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::PLUGIN_LIST_ROW_HEIGHT;
use tasty_ui_widgets::{
    NotificationLabels, NotificationRow, NotificationTone, PluginAttentionBarAction,
    PluginAttentionBarView, PluginIdentityView, PluginMetaView, PluginUninstallConfirmView,
    notification_item_expanded, paint_plugin_attention_row, plugin_attention_bar,
    plugin_detail_desc_hidden, plugin_detail_identity, plugin_folder_path,
    plugin_reserved_id_banner, plugin_uninstall_confirm_bar, plugin_uninstall_confirm_bar_height,
};

use super::attention::{ENTRIES, Entry, RESERVED_LONG, RESERVED_SAMPLE, row_view};
use crate::catalog::spec;

/// 디자인 `--tasty-size-560`. 넓은 상세 표본의 폭.
const WIDE_W: LogicalPx = LogicalPx(560.0);
/// 디자인 `--tasty-size-440`. 언어 길이 표본과 알림 표본의 폭.
const LANG_W: LogicalPx = LogicalPx(440.0);
/// 디자인 `--tasty-size-320`. 좁은 폭 표본의 폭.
const NARROW_W: LogicalPx = LogicalPx(320.0);

/// 상세 표본의 바 상태. 디자인 `ReservedDetail` 의 `state`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bar {
    Rest,
    Confirm,
    Removing,
}

/// 언어 코드에 맞는 문자열. `en` 은 번역표 초기화가 다르다.
fn copy(lang: &str, key: &'static str) -> &'static str {
    match lang {
        "en" => crate::i18n::t(key),
        code => crate::i18n::t_lang(code, key),
    }
}

/// 문자열의 첫 `{}` 를 채운다. 언어별 표본은 앱의 현재 언어를 거치지 않는다.
fn fill(lang: &str, key: &'static str, arg: &str) -> String {
    copy(lang, key).replacen("{}", arg, 1)
}

fn panel(ui: &mut egui::Ui, theme: &Theme, width: f32, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            contents(ui);
        });
}

/// 디자인 `ReservedRow` — 목록 폭의 사이드바 바탕에 행 하나.
fn row(ui: &mut egui::Ui, theme: &Theme, entry: &Entry, selected: bool) {
    let width = theme.plugins_side_panel_width().value();
    let pad = theme.spacing_sm.value();
    let row_h = PLUGIN_LIST_ROW_HEIGHT.value();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, row_h + 2.0 * pad), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        theme.bg_sidebar().to_egui(),
    );
    paint_plugin_attention_row(
        &p,
        theme,
        rect.shrink(pad),
        &row_view(theme, entry, selected),
    );
}

/// 디자인 `ReservedDetail`. 본체처럼 본문과 바를 쌓되 무대 높이는 내용을 따른다.
fn detail(ui: &mut egui::Ui, theme: &Theme, entry: &Entry, lang: &str, bar: Bar, width: f32) {
    panel(ui, theme, width, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let body = theme.spacing_lg.value() as i8;
        egui::Frame::new()
            .inner_margin(egui::Margin::same(body))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                plugin_detail_identity(
                    ui,
                    theme,
                    &PluginIdentityView {
                        name: entry.name,
                        version: "",
                        builtin_tag: None,
                        meta: PluginMetaView {
                            authors: "",
                            id: entry.id,
                            homepage: "",
                            wrap_long: true,
                        },
                    },
                );
                plugin_detail_desc_hidden(
                    ui,
                    theme,
                    copy(lang, "plugins.attn_reserved_desc_hidden"),
                );
                plugin_reserved_id_banner(
                    ui,
                    theme,
                    copy(lang, "plugins.attn_reserved_id_label"),
                    copy(lang, "plugins.attn_reserved_id_blurb"),
                );
                plugin_folder_path(ui, theme, copy(lang, "plugins.attn_folder"), entry.dir);
            });
        action_bar(ui, theme, entry, lang, bar, width);
    });
}

fn action_bar(ui: &mut egui::Ui, theme: &Theme, entry: &Entry, lang: &str, bar: Bar, width: f32) {
    let title = fill(lang, "plugins.attn_remove_confirm_title", entry.name);
    if bar == Bar::Rest {
        plugin_attention_bar(
            ui,
            theme,
            &PluginAttentionBarView {
                status: copy(lang, "plugins.attn_not_registered"),
                dot: theme.accent_danger().to_egui(),
                ink: theme.accent_danger_raised().to_egui(),
                action: Some(PluginAttentionBarAction::Remove(copy(
                    lang,
                    "plugins.attn_remove",
                ))),
            },
        );
        return;
    }
    let view = PluginUninstallConfirmView {
        title: &title,
        note: copy(lang, "plugins.attn_remove_note"),
        cancel: copy(lang, "button.cancel"),
        uninstall: copy(lang, "plugins.attn_remove"),
        // 정적 예제라 포커스를 옮기지 않는다. 다른 카드의 포커스를 빼앗지 않도록 한다.
        focus_cancel: false,
        busy: (bar == Bar::Removing).then(|| copy(lang, "plugins.attn_removing")),
        wrap: true,
    };
    let height = plugin_uninstall_confirm_bar_height(ui, theme, &view, width);
    ui.allocate_ui(egui::vec2(width, height), |ui| {
        plugin_uninstall_confirm_bar(ui, theme, &view);
    });
}

/// 디자인 `ReservedNotice` — 경고 수준 알림 한 건.
fn notice(ui: &mut egui::Ui, theme: &Theme, lang: &str, id: &str) {
    let title = fill(lang, "plugin_notice.reserved_id_title", id);
    let row = NotificationRow {
        id: 1,
        tone: NotificationTone::Warning,
        unread: true,
        title: &title,
        body: copy(lang, "plugin_notice.reserved_id_body"),
        source: "",
        source_closed: false,
        time: "now",
    };
    let t = crate::i18n::t;
    let labels = NotificationLabels {
        levels: [
            t("notification_panel.errors"),
            t("notification_panel.warnings"),
            t("notification_panel.info"),
        ],
        no_unread: t("notification_panel.no_unread"),
        empty: t("notification_panel.empty_message"),
        mark_all: t("button.mark_all_read"),
        mark_read: t("notification_panel.mark_read"),
        jump: t("button.jump_to_workspace"),
        closed: t("notification_panel.closed"),
        source_closed: t("notification_panel.source_closed"),
        more: t("notification_panel.more"),
        less: t("notification_panel.less"),
    };
    panel(ui, theme, LANG_W.value(), |ui| {
        ui.push_id((lang, id), |ui| {
            // 정적 예제다. 표본 안의 버튼은 동작할 곳이 없다.
            if notification_item_expanded(ui, theme, &row, &labels).is_some() {
                tracing::debug!("reserved id notice specimen ignored an action");
            }
        });
    });
}

fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .monospace()
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 디자인 `Themed` 카드. 표본 사이 간격은 `space-md` 다.
fn themed(ui: &mut egui::Ui, outer: &Theme, th: &Theme, contents: impl FnOnce(&mut egui::Ui)) {
    let margin = outer.spacing_md.value() as i8;
    egui::Frame::new()
        .fill(th.bg_app().to_egui())
        .inner_margin(egui::Margin::same(margin))
        .show(ui, |ui| {
            // cluster 는 가로 줄바꿈 배치다. 표본은 위아래로 쌓는다.
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = outer.spacing_md.value();
                contents(ui);
            });
        });
}

pub(super) fn draw(ui: &mut egui::Ui, theme: &Theme, mocha: &Theme, latte: &Theme) {
    let sample = &ENTRIES[RESERVED_SAMPLE];
    let long = &ENTRIES[RESERVED_LONG];
    let wide = WIDE_W.value();
    let narrow = NARROW_W.value();
    let lang_w = LANG_W.value();
    for (name, th) in [("Mocha", mocha), ("Latte", latte)] {
        spec::cluster(
            ui,
            theme,
            &format!("Attention › reserved id — {name}"),
            |ui| {
                themed(ui, theme, th, |ui| {
                    caption(ui, th, "list row · rest · selected");
                    row(ui, th, sample, false);
                    row(ui, th, sample, true);
                    caption(ui, th, "detail · rest — Remove not focused on selection");
                    detail(ui, th, sample, "en", Bar::Rest, wide);
                    caption(ui, th, "detail · remove confirm — bar replaced in place");
                    detail(ui, th, sample, "en", Bar::Confirm, wide);
                    caption(ui, th, "detail · removing — both disabled");
                    detail(ui, th, sample, "en", Bar::Removing, wide);
                    caption(
                        ui,
                        th,
                        "notification · warning · on arrival (once per folder)",
                    );
                    notice(ui, th, "en", sample.id);
                });
            },
        );
    }
    spec::cluster(
        ui,
        theme,
        "Attention › reserved id — Mocha · narrow 320 · long folder + id · confirm wraps",
        |ui| {
            themed(ui, theme, mocha, |ui| {
                detail(ui, mocha, long, "en", Bar::Rest, narrow);
                detail(ui, mocha, long, "en", Bar::Confirm, narrow);
            });
        },
    );
    let langs: Vec<&str> = crate::i18n::translated_languages().collect();
    spec::cluster(
        ui,
        theme,
        &format!(
            "Attention › reserved id — Mocha · {} length",
            langs.join(" / ")
        ),
        |ui| {
            themed(ui, theme, mocha, |ui| {
                for lang in langs.iter().copied() {
                    detail(ui, mocha, sample, lang, Bar::Rest, lang_w);
                    detail(ui, mocha, sample, lang, Bar::Confirm, lang_w);
                    notice(ui, mocha, lang, sample.id);
                }
            });
        },
    );
}
