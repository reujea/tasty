//! 단축키 가져오기·내보내기 진입 화면과 완료 안내 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, checkbox};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::toast_card::{self, ToastKind};
use crate::catalog::widgets::dialog as kit;

use super::paint::{caption, glyph_at, intro};
use super::{ENTRY_W, IE_FILE, STATE};

pub fn draw_entry(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
            STATE.with(|s| {
                let st = &mut *s.borrow_mut();
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    l2_tail(ui, theme, st.l2_filter_active);
                    checkbox(
                        ui,
                        theme,
                        &mut st.l2_filter_active,
                        "filter active (separator hidden)",
                        true,
                    );
                });
            });
            ui.vertical(|ui| {
                ui.set_width(ENTRY_W.value());
                ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
                entry(ui, theme);
                kit::hsep(ui, theme);
                caption(
                    ui,
                    theme,
                    "export feedback — the settings window's own toast",
                );
                export_toast(ui, theme);
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("L2 items", "one — “Import / Export”"),
            ("position", "last, after Plugins"),
            ("separator", "1px above the row (new axis)"),
            ("filtering", "separator hidden while filtering"),
            ("entry", "2 action rows, not a ListCtrl"),
            ("weights", "Import primary · Export secondary"),
            (
                "export feedback",
                "the window's own toast, carrying the path",
            ),
            ("file picker", "popup on the window's PopupManager"),
        ],
        &[
            TokenChip::without_color("separator", "L2 separator + row rules"),
            TokenChip::new(
                "surface-raised",
                "action row bed",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "border-default",
                "action row edge",
                theme.border_default().to_egui(),
            ),
            TokenChip::without_color("settings-sidebar-width", "200 — L2 column"),
        ],
    );
    spec::note(
        ui,
        theme,
        "The comparison preview uses DrillDown. Import and Export share a separate file picker popup for choosing an input file or save target, managed by the settings window’s PopupManager.",
    );
    spec::dont(
        ui,
        theme,
        "Don't give export its own result screen. There is nothing to do after a write, so the \
         confirmation is a toast with the resolved path and the user stays where they were.",
    );
}

/// jsx `IeL2Tail` + `settings_window.jsx` L2 행 — Keybindings L2 의 끝 네 행 · separator ·
/// 선택된 Import / Export.
fn l2_tail(ui: &mut egui::Ui, theme: &Theme, filter_active: bool) {
    egui::Frame::new()
        .fill(theme.bg_sidebar().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(tasty_ui_widgets::margin_all(theme.spacing_sm))
        .show(ui, |ui| {
            ui.set_width((theme.settings_sidebar_width() - theme.spacing_sm.scaled(2.0)).value());
            ui.spacing_mut().item_spacing.y = 0.0;
            for label in ["Explorer", "Scripts", "Preset", "Plugins"] {
                l2_row(ui, theme, label, false);
            }
            if !filter_active {
                l2_separator(ui, theme);
            }
            l2_row(ui, theme, "Import / Export", true);
        });
}

/// L2 separator — jsx `height: border-width · background: separator · margin: space-sm space-sm`.
fn l2_separator(ui: &mut egui::Ui, theme: &Theme) {
    let m = theme.spacing_sm.value();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value() + m * 2.0),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        (rect.left() + m)..=(rect.right() - m),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// L2 한 행 — 본체 `sidebar_row` 와 같은 치수(padding space-xs/space-sm, radius-sm).
fn l2_row(ui: &mut egui::Ui, theme: &Theme, label: &str, active: bool) {
    let h = theme.font_size_body.value() + theme.spacing_xs.value() * 2.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
    if active {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.surface_active().to_egui(),
        );
    }
    let fg = if active {
        theme.text_on_active()
    } else {
        theme.text_muted()
    };
    ui.painter().text(
        egui::pos2(rect.left() + theme.spacing_sm.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme.font_size_body.value()),
        fg.to_egui(),
    );
}

/// jsx `KbImportExportSubtab` 의 list 위치 — 안내문 + 액션 행 둘(Export secondary ·
/// Import primary).
fn entry(ui: &mut egui::Ui, theme: &Theme) {
    ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
    intro(
        ui,
        theme,
        theme.measure_md,
        crate::i18n::t("settings.keybindings.ie_entry_intro"),
    );
    action_row(
        ui,
        theme,
        icons::DOWNLOAD,
        "Export",
        crate::i18n::t("settings.keybindings.ie_export_desc"),
        "Export…",
        ButtonVariant::Secondary,
        true,
        None,
    );
    action_row(
        ui,
        theme,
        icons::FILE,
        "Import",
        crate::i18n::t("settings.keybindings.ie_import_desc"),
        "Import…",
        ButtonVariant::Primary,
        true,
        None,
    );
}

/// jsx `IeActionRow` — glyph · 제목(13 primary) + 설명(12 muted, measure-md) · trailing 버튼,
/// 그 아래 `notice` 자리. `enabled == false` 면 trailing 버튼이 꺼진다(notice 가 재시도를 든 동안).
// reason: jsx `IeActionRow` 의 prop 을 그대로 받는다 — 본체 짝과 같은 모양이다.
#[allow(clippy::too_many_arguments)]
pub(super) fn action_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: MockGlyph,
    title: &str,
    desc: &str,
    button: &str,
    variant: ButtonVariant,
    enabled: bool,
    notice: Option<&mut dyn FnMut(&mut egui::Ui)>,
) {
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(tasty_ui_widgets::margin_all(theme.spacing_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                glyph_at(
                    ui,
                    glyph,
                    theme.icon_glyph_size_md,
                    theme.text_muted().to_egui(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    Button::new(button)
                        .variant(variant)
                        .size(ControlSize::Sm)
                        .enabled(enabled)
                        .show(ui, theme);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y =
                            tasty_ui_widgets::tokens::STRUCT_GAP_2.value();
                        ui.label(
                            egui::RichText::new(title)
                                .size(theme.font_size_body.value())
                                .color(theme.text_primary().to_egui()),
                        );
                        ui.scope(|ui| {
                            ui.set_max_width(theme.measure_md.value());
                            ui.label(
                                egui::RichText::new(desc)
                                    .size(theme.font_size_term_sm.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        });
                    });
                });
            });
            if let Some(notice) = notice {
                notice(ui);
            }
        });
}

/// 본체와 같은 draw_single_card 함수로 내보내기 완료 토스트를 그린다.
fn export_toast(ui: &mut egui::Ui, theme: &Theme) {
    let text = format!("Exported to ~/tasty/{IE_FILE}");
    toast_card::draw_single_card(ui, theme, ToastKind::Success, &text, &[], 1.0);
}
