//! 공용 Button의 종류·크기·아이콘·비활성 상태 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, Ground, IconButton, checkbox, switch};

use super::glyph;
use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, stage};

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        cluster(ui, theme, "variants — hover & click them", |ui| {
            Button::new("Save")
                .variant(ButtonVariant::Primary)
                .show(ui, theme);
            Button::new("Open folder")
                .variant(ButtonVariant::Secondary)
                .show(ui, theme);
            Button::new("Cancel")
                .variant(ButtonVariant::Ghost)
                .show(ui, theme);
            Button::new("Force detach")
                .variant(ButtonVariant::Danger)
                .show(ui, theme);
            Button::new("Run agent task")
                .variant(ButtonVariant::Agent)
                .show(ui, theme);
        });
        cluster(ui, theme, "sizes — sm 24 · md 28 · lg 32", |ui| {
            Button::new("Small")
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .show(ui, theme);
            Button::new("Medium")
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Md)
                .show(ui, theme);
            Button::new("Large")
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Lg)
                .show(ui, theme);
        });
        cluster(ui, theme, "with icons · disabled", |ui| {
            Button::new("New tab")
                .variant(ButtonVariant::Secondary)
                .leading_icon(&|ui, rect, c| glyph::PLUS.image(rect.height(), c).paint_at(ui, rect))
                .show(ui, theme);
            Button::new("Search")
                .variant(ButtonVariant::Primary)
                .trailing_icon(&|ui, rect, c| {
                    glyph::SEARCH.image(rect.height(), c).paint_at(ui, rect)
                })
                .show(ui, theme);
            Button::new("Disabled")
                .variant(ButtonVariant::Secondary)
                .enabled(false)
                .show(ui, theme);
        });
        ghost_on_raised(ui, theme);
    });

    meta(
        ui,
        theme,
        &[
            ("height", "28 · sm 24 · lg 32"),
            ("padding", "0 space-md"),
            ("radius", "4"),
            ("overlay", "hover 8% · active 12%"),
            ("focus", "2px ring"),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "primary fill",
                egui::Color32::from(theme.accent_primary()),
            ),
            TokenChip::new(
                "accent-danger",
                "danger fill",
                egui::Color32::from(theme.accent_danger()),
            ),
            TokenChip::new(
                "accent-agent",
                "agent fill",
                egui::Color32::from(theme.accent_agent()),
            ),
            TokenChip::without_color("overlay-hover", "8% hover"),
            TokenChip::without_color("text-on-accent", "filled label"),
        ],
    );
}

/// 시안 ghost ground="raised": surface-raised 위 Mocha·Latte 칸에 panel ghost와 raised ghost를 나란히 둔다.
fn ghost_on_raised(ui: &mut egui::Ui, theme: &Theme) {
    let themes = [
        (
            "Mocha",
            palette(&tasty_themes::mocha_fallback(), theme.ui_zoom),
        ),
        (
            "Latte",
            palette(&crate::host_shell::latte_theme(), theme.ui_zoom),
        ),
    ];
    cluster(
        ui,
        theme,
        "ghost ground=\"raised\" — on surface-raised the label takes button-ghost-raised-fg → text-secondary-raised",
        |ui| {
            for (name, th) in &themes {
                egui::Frame::new()
                    .fill(th.surface_raised().to_egui())
                    .corner_radius(th.corner_radius.value())
                    .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                            ui.label(
                                egui::RichText::new(*name)
                                    .size(th.font_size_caption.value())
                                    .color(th.text_secondary_raised().to_egui()),
                            );
                            Button::new("Cancel (panel)")
                                .variant(ButtonVariant::Ghost)
                                .size(ControlSize::Sm)
                                .show(ui, th);
                            Button::new("Cancel (raised)")
                                .variant(ButtonVariant::Ghost)
                                .size(ControlSize::Sm)
                                .ground(Ground::Raised)
                                .show(ui, th);
                        });
                    });
            }
        },
    );
}

/// 팔레트 하나를 현재 UI 배율로 다시 만든다. 한 페이지에 Mocha와 Latte를 나란히 놓기 위해 쓴다.
fn palette(base: &Theme, zoom: f32) -> Theme {
    Theme::with_colors_and_zoom(base.to_colors(), base.is_light, zoom)
}

/// 시안 열 머리·행 이름의 작은 mono 라벨.
fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .monospace()
            .size(theme.font_size_micro.value())
            .color(egui::Color32::from(theme.text_muted())),
    );
}

/// 한 팔레트의 enabled·disabled 비교 격자. disabled는 모든 variant가 같은 중립 상자와
/// disabled ink로 그려진다.
fn disabled_grid(ui: &mut egui::Ui, th: &Theme, name: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from(th.bg_panel()))
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            egui::Color32::from(th.border_default()),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
        .show(ui, |ui| {
            egui::Grid::new(("button_disabled_grid", name))
                .spacing(egui::vec2(th.spacing_md.value(), th.spacing_sm.value()))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(name)
                            .size(th.font_size_caption.value())
                            .color(egui::Color32::from(th.text_muted())),
                    );
                    caption(ui, th, "enabled");
                    caption(ui, th, "disabled");
                    ui.end_row();
                    for (label, variant) in [
                        ("primary", ButtonVariant::Primary),
                        ("agent", ButtonVariant::Agent),
                        ("danger", ButtonVariant::Danger),
                        ("secondary", ButtonVariant::Secondary),
                        ("ghost", ButtonVariant::Ghost),
                    ] {
                        caption(ui, th, label);
                        for enabled in [true, false] {
                            Button::new("New tab")
                                .variant(variant)
                                .enabled(enabled)
                                .leading_icon(&|ui, rect, c| {
                                    glyph::PLUS.image(rect.height(), c).paint_at(ui, rect)
                                })
                                .show(ui, th);
                        }
                        ui.end_row();
                    }
                    caption(ui, th, "controls");
                    for enabled in [true, false] {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                            IconButton::new()
                                .enabled(enabled)
                                .show(ui, th, &|ui, rect, c| {
                                    glyph::SEARCH.image(rect.height(), c).paint_at(ui, rect)
                                });
                            let mut wrap = true;
                            checkbox(ui, th, &mut wrap, "Wrap", enabled, Ground::Panel);
                            let mut on = true;
                            switch(ui, th, &mut on, None, enabled);
                        });
                    }
                    ui.end_row();
                });
        });
}

/// disabled 규칙 — opacity 없이 중립 상자와 disabled ink. 흐린 항목(state-dim)과 구분한다.
pub fn draw_disabled(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = palette(&tasty_themes::mocha_fallback(), theme.ui_zoom);
    let latte = palette(&crate::host_shell::latte_theme(), theme.ui_zoom);
    // 두 팔레트 패널을 위쪽에 맞춰 나란히 둔다(시안 flex-start, 간격 space-md).
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            disabled_grid(ui, &mocha, "Mocha");
            disabled_grid(ui, &latte, "Latte");
        });
    });

    meta(
        ui,
        theme,
        &[
            (
                "fill / edge",
                "surface-raised / border-default — every variant (ghost: none)",
            ),
            ("label + icons", "text-disabled (one ink)"),
            ("accent fill", "drops out when disabled"),
            ("opacity", "none on disabled controls"),
            ("hover / active", "not drawn"),
            ("cursor", "default"),
            (
                "dimmed items",
                "switched-off rows · pending cut · inert regions → state-dim-opacity (0.5), not this rule",
            ),
        ],
        &[
            TokenChip::new(
                "button-disabled-bg",
                "→ state-disabled-fill",
                egui::Color32::from(theme.button_disabled_bg()),
            ),
            TokenChip::new(
                "button-disabled-border",
                "→ state-disabled-border",
                egui::Color32::from(theme.button_disabled_border()),
            ),
            TokenChip::new(
                "button-disabled-fg",
                "→ state-disabled-fg",
                egui::Color32::from(theme.button_disabled_fg()),
            ),
            TokenChip::without_color("state-dim-opacity", "items only"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't multiply a disabled control by 0.5. Its label lands at a different step for every \
         variant, and a faded accent fill still reads as \"the primary action\".",
    );
}
