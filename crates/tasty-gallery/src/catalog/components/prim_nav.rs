//! 공용 TreeRow와 MenuItem의 상태별 예제.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    MenuItemVariant, menu_item, menu_option, menu_separator, select_or_placeholder, tree_row,
};

use super::glyph;
use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};

thread_local! {
    static SEL: RefCell<usize> = const { RefCell::new(0) };
    static TREE_SEL: RefCell<usize> = const { RefCell::new(2) };
}

/// MenuItem — icon · label · shortcut · active · disabled · danger · separator.
pub fn draw_menu_item(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        // 디자인: surface-raised border radius padding 6 width 280.
        egui::Frame::new()
            .fill(egui::Color32::from(theme.surface_raised()))
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                egui::Color32::from(theme.border_default()),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
            .show(ui, |ui| {
                ui.set_width(theme.measure_sm.value() - theme.spacing_lg.value());
                ui.spacing_mut().item_spacing.y = 0.0;
                SEL.with(|s| {
                    let mut sel = s.borrow_mut();
                    let items: [(glyph::MockGlyph, &str, Option<&str>); 3] = [
                        (glyph::TERMINAL, "New tab", Some("Ctrl+T")),
                        (glyph::SPLIT, "Split pane", Some("Ctrl+D")),
                        (glyph::COPY, "Copy path", None),
                    ];
                    for (i, (g, label, sc)) in items.iter().enumerate() {
                        let gg = *g;
                        let r = menu_item(
                            ui,
                            theme,
                            Some(&|ui, rect, c| gg.image(rect.height(), c).paint_at(ui, rect)),
                            label,
                            *sc,
                            MenuItemVariant::Normal,
                            i == *sel,
                            true,
                        );
                        if r.clicked() {
                            *sel = i;
                        }
                    }
                    menu_separator(ui, theme);
                    menu_item(
                        ui,
                        theme,
                        Some(&|ui, rect, c| {
                            glyph::TRASH.image(rect.height(), c).paint_at(ui, rect)
                        }),
                        "Move to Trash",
                        None,
                        MenuItemVariant::Danger,
                        false,
                        true,
                    );
                });
            });
    });

    meta(
        ui,
        theme,
        &[
            ("height", "28 control-height"),
            ("active", "surface-active"),
            ("danger", "accent-danger label"),
        ],
        &[
            TokenChip::new(
                "surface-active",
                "active row",
                egui::Color32::from(theme.surface_active()),
            ),
            TokenChip::new(
                "accent-danger",
                "danger label",
                egui::Color32::from(theme.accent_danger()),
            ),
            TokenChip::new(
                "text-muted",
                "shortcut",
                egui::Color32::from(theme.text_muted_raised()),
            ),
        ],
    );
}

/// MenuItem — selected option. 열린 선택 목록의 현재 값은 selected 글자 + 오른쪽 체크이고 채움이 없다.
/// 채움은 호버(menu-item-bg-hover)와 키보드 active(surface-active)에만 쓴다(Mocha·Latte).
pub fn draw_menu_item_selected(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            selected_option_panel(ui, &mocha, "Mocha");
            selected_option_panel(ui, &latte, "Latte");
            no_value_panel(ui, &mocha);
        });
    });

    meta(
        ui,
        theme,
        &[
            ("selected ink", "menu-item-selected-fg"),
            (
                "check",
                "check icon · menu-item-check-size 14 · menu-item-check-fg · trailing, after any shortcut",
            ),
            ("fill", "none — hover / keyboard fills only"),
            (
                "no value",
                "the placeholder is NOT a row — the list holds real options only, none checked, keyboard-active starts on row 1; the trigger alone shows the placeholder (text-placeholder). A clearable Select gets a real option for it (e.g. “None”), drawn like any other row.",
            ),
        ],
        &[
            TokenChip::new(
                "menu-item-selected-fg",
                "selected label → text-primary",
                egui::Color32::from(theme.menu_item_selected_fg()),
            ),
            TokenChip::new(
                "menu-item-check-fg",
                "check glyph",
                egui::Color32::from(theme.menu_item_check_fg()),
            ),
            TokenChip::new(
                "text-secondary-raised",
                "resting label role (Latte → n1100)",
                egui::Color32::from(theme.text_secondary_raised()),
            ),
        ],
    );
}

/// 테마 하나의 패널 — 캡션과 네 행(쉼 · 선택 · 쉼 · 키보드 active) 메뉴.
fn selected_option_panel(ui: &mut egui::Ui, th: &Theme, name: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from(th.bg_panel()))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                ui.label(
                    egui::RichText::new(format!(
                        "{name} — rest · selected · rest · keyboard-active"
                    ))
                    .size(th.font_size_caption.value())
                    .color(egui::Color32::from(th.text_muted_raised())),
                );
                let bw = th.border_width.value();
                egui::Frame::new()
                    .fill(egui::Color32::from(th.menu_bg()))
                    .stroke(egui::Stroke::new(bw, egui::Color32::from(th.menu_border())))
                    .corner_radius(th.menu_radius().value())
                    .shadow(th.shadow_popover().to_egui())
                    .inner_margin(egui::Margin::same(th.spacing_xs.value() as i8))
                    .show(ui, |ui| {
                        // 시안 폭 field-width-lg 는 테두리까지 포함한 바깥 폭이다.
                        ui.set_width(
                            th.field_width_lg.value() - (th.spacing_xs.value() + bw) * 2.0,
                        );
                        ui.spacing_mut().item_spacing.y = 0.0;
                        menu_option(ui, th, "Ask", false);
                        menu_option(
                            ui,
                            th,
                            crate::i18n::t("settings.general.close_behavior_minimize"),
                            true,
                        );
                        menu_option(ui, th, "Quit", false);
                        menu_item(
                            ui,
                            th,
                            None,
                            "Quit and save layout",
                            None,
                            MenuItemVariant::Normal,
                            true,
                            true,
                        );
                    });
            });
        });
}

/// 아직 값이 없는 Select — 트리거만 placeholder 색이고, 열린 목록은 실제 옵션만 담는다(체크 없음,
/// 키보드 active 는 첫 행).
fn no_value_panel(ui: &mut egui::Ui, th: &Theme) {
    egui::Frame::new()
        .fill(egui::Color32::from(th.bg_panel()))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                ui.label(
                    egui::RichText::new(
                        "No value yet — trigger in placeholder ink, list = real options only",
                    )
                    .size(th.font_size_caption.value())
                    .color(egui::Color32::from(th.text_muted_raised())),
                );
                let options = no_value_options();
                let mut none = None;
                select_or_placeholder(
                    ui,
                    th,
                    "gallery_no_value_select",
                    &mut none,
                    &options,
                    "Choose an action",
                    th.field_width_lg.value(),
                    true,
                );
                let bw = th.border_width.value();
                egui::Frame::new()
                    .fill(egui::Color32::from(th.menu_bg()))
                    .stroke(egui::Stroke::new(bw, egui::Color32::from(th.menu_border())))
                    .corner_radius(th.menu_radius().value())
                    .shadow(th.shadow_popover().to_egui())
                    .inner_margin(egui::Margin::same(th.spacing_xs.value() as i8))
                    .show(ui, |ui| {
                        ui.set_width(
                            th.field_width_lg.value() - (th.spacing_xs.value() + bw) * 2.0,
                        );
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, label) in options.iter().enumerate() {
                            if i == 0 {
                                menu_item(
                                    ui,
                                    th,
                                    None,
                                    label,
                                    None,
                                    MenuItemVariant::Normal,
                                    true,
                                    true,
                                );
                            } else {
                                menu_option(ui, th, label, false);
                            }
                        }
                    });
            });
        });
}

/// "no value" 패널의 옵션. 시안과 같은 세 행이며 가운데 행은 본체 문구를 읽는다.
fn no_value_options() -> [&'static str; 3] {
    [
        "Ask",
        crate::i18n::t("settings.general.close_behavior_minimize"),
        "Quit",
    ]
}

/// TreeRow — depth · chevron · icon(selected accent) · meta.
pub fn draw_tree_row(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        // 디자인: bg-sidebar padding 6px 4px width 320.
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_sidebar()))
            .inner_margin(egui::Margin {
                left: theme.spacing_xs.value() as i8,
                right: theme.spacing_xs.value() as i8,
                top: theme.spacing_sm.value() as i8,
                bottom: theme.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                ui.set_width(theme.measure_md.value());
                ui.spacing_mut().item_spacing.y = 0.0;
                TREE_SEL.with(|s| {
                    let mut sel = s.borrow_mut();
                    // (depth, has_children, open, glyph, label, meta)
                    let rows: [(u16, bool, bool, glyph::MockGlyph, &str, &str); 4] = [
                        (0, true, true, glyph::FOLDER, "tasty", "34"),
                        (1, true, false, glyph::FOLDER, "crates", "39"),
                        (1, false, false, glyph::FILE, "Cargo.toml", ""),
                        (1, false, false, glyph::FILE, "README.md", "4.7k"),
                    ];
                    for (i, (depth, hc, open, g, label, meta)) in rows.iter().enumerate() {
                        let gg = *g;
                        let r = tree_row(
                            ui,
                            theme,
                            *depth,
                            *hc,
                            *open,
                            Some(&|ui, rect, c| gg.image(rect.height(), c).paint_at(ui, rect)),
                            label,
                            (!meta.is_empty()).then_some(*meta),
                            i == *sel,
                        );
                        if r.clicked() {
                            *sel = i;
                        }
                    }
                });
            });
    });

    meta(
        ui,
        theme,
        &[
            ("height", "22 control-height-tree"),
            ("indent", "space-md / level"),
            ("selected", "surface-active"),
            ("disabled", "none — no such state (2026-09-29)"),
        ],
        &[
            TokenChip::new(
                "surface-active",
                "selected row",
                egui::Color32::from(theme.surface_active()),
            ),
            TokenChip::without_color("overlay-hover", "hover"),
            TokenChip::new(
                "text-muted",
                "meta value",
                egui::Color32::from(theme.text_muted()),
            ),
        ],
    );
    note(
        ui,
        theme,
        "No disabled state (2026-09-29). A tree row names something that exists. A folder without \
         read permission or a favourite on a dropped remote stays a normal row: selectable, \
         expandable, and opening it reports the reason (CenterState error in the explorer body). \
         Greying it would hide the one row the user needs to act on. The product's enabled \
         argument is removed. A pending cut is a dimmed item (cut-pending-opacity), not \
         disabled.",
    );
}
