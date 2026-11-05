//! 서피스 오른쪽 위 검색줄 예제. 배경을 어둡게 하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{IconButton, IconButtonVariant};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

pub(crate) const WIDTH: LogicalPx = LogicalPx(360.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        bar_state(ui, theme, "2/3", false, false);
        bar_state(ui, theme, "2/3", false, true);
        bar_state(ui, theme, "0/0", true, true);
        bar_state(ui, theme, "0/0", true, false);
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "frame",
                "360px body width · 38px painted shell · 28px Input",
            ),
            ("anchor", "focused surface top-right · sticky"),
            ("scrim", "none"),
            ("parts", "input · 2/3 counter · ▲▼ · Aa .* ab · close"),
            ("no-match", "counter → accent-danger"),
        ],
        &[
            TokenChip::new("surface-raised", "bar", theme.surface_raised().to_egui()),
            TokenChip::new(
                "search-bar-border",
                "edge → menu-border",
                theme.search_bar_border().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "0 matches",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::without_color("shadow-popover", "lift"),
        ],
    );

    spec::note(
        ui,
        theme,
        "An in-place find bar, not a modal — it sticks to the top-right of the \
         focused surface, leaves the rest interactive, and Esc closes it.",
    );
}

pub(crate) fn bar(ui: &mut egui::Ui, theme: &Theme, count: &str, no_match: bool) {
    bar_state(ui, theme, count, no_match, false);
}

fn bar_state(ui: &mut egui::Ui, theme: &Theme, count: &str, invalid: bool, focused: bool) {
    kit::frame_card_popover_edge(
        ui,
        theme,
        WIDTH,
        kit::raised_fill(theme),
        theme.search_bar_border().to_egui(),
        |ui| {
            kit::region_sym(ui, theme.spacing_sm, theme.spacing_xs, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                    let counter_w = theme.field_width_xs.scaled(0.5);
                    let trailing = counter_w
                        + theme.item_height_interactive.scaled(4.0)
                        + theme.spacing_md.scaled(4.0);
                    let input_w =
                        (WIDTH - theme.spacing_sm.scaled(2.0) - trailing).max(LogicalPx(80.0));
                    let mut query = if invalid { "(tas" } else { "tasty" }.to_owned();
                    tasty_ui_widgets::Input::new()
                        .ground(tasty_ui_widgets::Ground::Raised)
                        .width(input_w.value())
                        .invalid(invalid)
                        .focus_look(focused)
                        .show(ui, theme, &mut query);
                    let counter_color = if invalid {
                        theme.accent_danger()
                    } else {
                        theme.text_muted()
                    };
                    ui.label(
                        egui::RichText::new(count)
                            .monospace()
                            .size(theme.font_size_caption.value())
                            .color(counter_color.to_egui()),
                    );
                    icon_btn(ui, theme, icons::CHEVRON_DOWN, false);
                    icon_btn(ui, theme, icons::CHEVRON_RIGHT, false);
                    toggle_chip(ui, theme, "Aa", false);
                    toggle_chip(ui, theme, ".*", invalid);
                    toggle_chip(ui, theme, "ab", !invalid);
                    let h = theme.item_height_interactive.value() * 0.6;
                    let (r, _) = ui.allocate_exact_size(
                        egui::vec2(theme.border_width.value(), h),
                        egui::Sense::hover(),
                    );
                    ui.painter().vline(
                        r.center().x,
                        r.y_range(),
                        egui::Stroke::new(
                            theme.border_width.value(),
                            theme.separator.to_egui_premultiplied(),
                        ),
                    );
                    icon_btn(ui, theme, icons::CLOSE, false);
                });
            });
        },
    );
}

fn icon_btn(ui: &mut egui::Ui, theme: &Theme, glyph: icons::MockGlyph, active: bool) {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .active(active)
        .show(ui, theme, &|ui, rect, c| {
            glyph.image(rect.height(), c).paint_at(ui, rect)
        });
}

fn toggle_chip(ui: &mut egui::Ui, theme: &Theme, label: &str, active: bool) {
    let (fill, fg) = if active {
        (
            theme.surface_active().to_egui(),
            theme.text_on_active().to_egui(),
        )
    } else {
        (egui::Color32::TRANSPARENT, theme.text_muted().to_egui())
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::monospace(theme.font_size_caption.value()),
        egui::Color32::PLACEHOLDER,
    );
    let s = theme.item_height_interactive.value() * 0.78;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
    if fill != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, theme.corner_radius_sm.value(), fill);
    }
    ui.painter()
        .galley(rect.center() - galley.rect.size() * 0.5, galley, fg);
}
