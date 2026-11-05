//! 프리셋 적용 팝업 예제.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::STRUCT_GAP_1;
use tasty_ui_widgets::{Button, ButtonVariant};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

const WIDTH: LogicalPx = LogicalPx(440.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                ui.horizontal(|ui| {
                    kit::title(ui, theme, "Apply preset");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        seg(ui, theme, &["Workspace", "Tab", "Pane"], 0);
                    });
                });
            });
            kit::hsep(ui, theme);

            kit::region_sym(ui, theme.spacing_sm, theme.spacing_sm, |ui| {
                preset(ui, theme, "Dev split", "2 panes · editor + shell", true);
                preset(ui, theme, "Logs grid", "4 panes · tail -f", false);
                preset(ui, theme, "Single shell", "1 pane · zsh", false);
            });
            kit::hsep(ui, theme);

            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        Button::new("Apply to workspace")
                            .variant(ButtonVariant::Primary)
                            .show(ui, theme);
                        Button::new("Cancel")
                            .variant(ButtonVariant::Ghost)
                            .show(ui, theme);
                    });
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "440px · bg-panel"),
            ("header", "title · Workspace/Tab/Pane seg · height 26"),
            ("row", "layers icon · name · meta mono"),
            ("selected", "2px accent inset"),
            ("footer", "Cancel · Apply to <scope>"),
        ],
        &[
            TokenChip::new(
                "surface-active",
                "selected row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::without_color("font-mono", "layout summary"),
            TokenChip::new(
                "accent-primary",
                "inset bar · seg",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "seg track",
                theme.surface_raised().to_egui(),
            ),
        ],
    );
}

fn preset(ui: &mut egui::Ui, theme: &Theme, name: &str, meta: &str, selected: bool) {
    let h = theme.item_height_interactive.value() + theme.spacing_md.value();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if selected {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.surface_active().to_egui(),
        );
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme.selection_edge_width.value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, theme.accent_primary().to_egui());
    }
    let (icon_fg, name_fg, meta_fg) = if selected {
        let ink = theme.text_on_active();
        (ink, ink, ink)
    } else {
        (
            theme.text_secondary(),
            theme.text_primary(),
            theme.text_muted(),
        )
    };
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(theme.spacing_md.value(), 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(
        &mut child,
        icons::SPLIT,
        theme.icon_glyph_size_md,
        icon_fg.to_egui(),
    );
    child.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = STRUCT_GAP_1.value();
        ui.label(
            egui::RichText::new(name)
                .size(theme.font_size_body.value())
                .color(name_fg.to_egui()),
        );
        ui.label(
            egui::RichText::new(meta)
                .monospace()
                .size(theme.font_size_caption.value())
                .color(meta_fg.to_egui()),
        );
    });
}

/// 인라인 세그먼트 컨트롤 (height 26) — active index 강조.
fn seg(ui: &mut egui::Ui, theme: &Theme, items: &[&str], active: usize) {
    let h = theme.item_height_interactive.value() - theme.spacing_xs.value() * 0.5;
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius_sm.value())
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                for (i, label) in items.iter().enumerate() {
                    if i > 0 {
                        let (r, _) = ui.allocate_exact_size(
                            egui::vec2(theme.border_width.value(), h),
                            egui::Sense::hover(),
                        );
                        ui.painter().vline(
                            r.center().x,
                            r.y_range(),
                            egui::Stroke::new(
                                theme.border_width.value(),
                                theme.border_default().to_egui(),
                            ),
                        );
                    }
                    let (fg, bg) = if i == active {
                        (
                            theme.text_on_active().to_egui(),
                            theme.surface_active().to_egui(),
                        )
                    } else {
                        (theme.text_muted().to_egui(), egui::Color32::TRANSPARENT)
                    };
                    let galley = ui.painter().layout_no_wrap(
                        (*label).to_owned(),
                        egui::FontId::proportional(theme.font_size_term_sm.value()),
                        egui::Color32::PLACEHOLDER,
                    );
                    let pad = theme.spacing_sm.value();
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(galley.rect.width() + pad * 2.0, h),
                        egui::Sense::hover(),
                    );
                    if bg != egui::Color32::TRANSPARENT {
                        ui.painter().rect_filled(rect, 0.0, bg);
                    }
                    ui.painter()
                        .galley(rect.center() - galley.rect.size() * 0.5, galley, fg);
                }
            });
        });
}
