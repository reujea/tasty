//! 기본 간격 토큰을 배지·버튼·카드·컬럼·구역에 적용한 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{BadgeVariant, Button, ButtonVariant, badge};

use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        use_row(ui, theme, "space-xs", "4 · chip gap", |ui, theme| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
            badge(ui, theme, "3", BadgeVariant::Danger);
            badge(ui, theme, "new", BadgeVariant::Agent);
            badge(ui, theme, "ok", BadgeVariant::Success);
        });
        use_row(ui, theme, "space-sm", "8 · button pair", |ui, theme| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            Button::new("Cancel")
                .variant(ButtonVariant::Secondary)
                .show(ui, theme);
            Button::new("Confirm").show(ui, theme);
        });
        use_row(ui, theme, "space-md", "12 · card padding", |ui, theme| {
            egui::Frame::new()
                .fill(ec(theme.surface_raised()))
                .stroke(egui::Stroke::new(
                    theme.border_width.value(),
                    ec(theme.border_default()),
                ))
                .corner_radius(theme.corner_radius.value())
                .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Card body")
                            .size(theme.font_size_body.value())
                            .color(ec(theme.text_secondary_raised())),
                    );
                });
        });
        use_row(ui, theme, "space-lg", "16 · column gap", |ui, theme| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            for label in ["Column A", "Column B"] {
                col_block(ui, theme, label);
            }
        });
        use_row(ui, theme, "space-xl", "24 · region gap", |ui, theme| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
            for label in ["Region 1", "Region 2"] {
                region_block(ui, theme, label);
            }
        });
    });

    meta(
        ui,
        theme,
        &[
            ("grid", "4 / 8 / 12 / 16 / 24"),
            ("rule", "spacing tokens use 4px steps"),
            ("heights", "control heights use their own tokens"),
        ],
        &[
            TokenChip::without_color("space-xs", "4 · icon gaps"),
            TokenChip::without_color("space-sm", "8 · control gaps"),
            TokenChip::without_color("space-md", "12 · card pad"),
            TokenChip::without_color("space-lg", "16 · sections"),
            TokenChip::without_color("space-xl", "24 · regions"),
        ],
    );
    note(
        ui,
        theme,
        "기본 간격은 4·8·12·16·24px 토큰을 사용한다. 컨트롤 높이는 별도 토큰을 따르므로 트리 행의 22px처럼 4의 배수가 아닌 값도 있다.",
    );
}

/// SpaceUse 한 행 — [150px 라벨(토큰 + 용도)] [실사용 데모].
fn use_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    tok: &str,
    role: &str,
    demo: impl FnOnce(&mut egui::Ui, &Theme),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        ui.allocate_ui(
            egui::vec2(
                theme.tab_width.value(),
                theme.item_height_interactive.value(),
            ),
            |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tok)
                            .monospace()
                            .size(theme.font_size_micro.value())
                            .color(ec(theme.text_primary())),
                    );
                    ui.label(
                        egui::RichText::new(role)
                            .size(theme.font_size_micro.value())
                            .color(ec(theme.text_muted())),
                    );
                });
            },
        );
        ui.horizontal(|ui| demo(ui, theme));
    });
}

fn col_block(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    egui::Frame::new()
        .fill(ec(theme.bg_panel()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.separator.to_egui_premultiplied()),
        ))
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(theme.font_size_caption.value())
                    .color(ec(theme.text_secondary())),
            );
        });
}

fn region_block(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    egui::Frame::new()
        .fill(ec(theme.surface_raised()))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(theme.font_size_caption.value())
                    .color(ec(theme.text_muted_raised())),
            );
        });
}
