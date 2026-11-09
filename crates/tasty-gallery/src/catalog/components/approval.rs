//! 에이전트 명령과 권한을 보여주는 승인 팝업 예제.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    BadgeVariant, Button, ButtonVariant, TagVariant, approval_choice, badge_dot, tag, tag_mono,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

const WIDTH: LogicalPx = LogicalPx(440.0);

/// 시안 "Danger choice" 의 테마별 열 폭.
const DANGER_COLUMN_WIDTH: LogicalPx = LogicalPx(300.0);

/// 시안 예시 선택지. 마지막이 위험 선택지다.
const DANGER_CHOICES: [(&str, bool); 3] = [
    ("Allow once", false),
    ("Always allow in this workspace", false),
    ("Allow and skip future checks", true),
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_md, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    badge_dot(ui, theme, BadgeVariant::Agent);
                    kit::title(ui, theme, "Approve agent action");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        tag(ui, theme, "agent", TagVariant::Agent, false);
                    });
                });
            });
            kit::hsep(ui, theme);

            kit::region_sym(ui, theme.spacing_md, theme.spacing_md, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                kit::body(
                    ui,
                    theme,
                    "The agent ai-review wants to run a command in s_01HXK9:",
                );
                egui::Frame::new()
                    .fill(theme.bg_app().to_egui())
                    .corner_radius(theme.corner_radius_sm.value())
                    .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new("git push --force origin main")
                                .monospace()
                                .size(theme.font_size_term_sm.value())
                                .color(theme.text_primary().to_egui()),
                        );
                    });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    tag(ui, theme, "destructive", TagVariant::Danger, true);
                    tag_mono(ui, theme, "fs:write", TagVariant::Default, false);
                    tag_mono(ui, theme, "net", TagVariant::Default, false);
                });
            });
            kit::hsep(ui, theme);

            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        Button::new("Always allow")
                            .variant(ButtonVariant::Agent)
                            .show(ui, theme);
                        Button::new("Allow once")
                            .variant(ButtonVariant::Secondary)
                            .show(ui, theme);
                        Button::new("Deny")
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
            ("header", "agent dot · title · agent Tag"),
            ("body", "prose · pre on bg-app · permission Tags"),
            ("footer", "Deny · Allow once · Always"),
        ],
        &[
            TokenChip::new(
                "accent-agent",
                "agent identity",
                theme.accent_agent().to_egui(),
            ),
            TokenChip::new("bg-app", "command block", theme.bg_app().to_egui()),
            TokenChip::new(
                "accent-warning",
                "risky grant",
                theme.accent_warning().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Mauve always means agent. The exact command and the permissions it grants \
         are shown verbatim before anything runs — Allow once is the safe default.",
    );

    draw_danger_choice(ui, theme);
}

/// 위험 선택지: 본체 popup 과 같은 `approval_choice` 를 Mocha·Latte 의 surface-raised 위에 그린다.
fn draw_danger_choice(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Danger choice — tinted fill with readable ink",
        Some(
            "The destructive option of an approval request: an opaque fill (danger 12% mixed into \
             bg-panel), a 1px danger edge and text-primary label ink. Red ink is rejected — it is \
             3.94 : 1 on the Latte tint.",
        ),
    );
    let latte = Theme::with_colors_and_zoom(
        crate::host_shell::latte_theme().to_colors(),
        true,
        theme.ui_zoom,
    );
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, th) in [
            ("Mocha · popup surface-raised · label 9.18 : 1", theme),
            ("Latte · popup surface-raised · label 5.80 : 1", &latte),
        ] {
            spec::wrap_item(ui, |ui| {
                ui.push_id(label, |ui| danger_column(ui, th, label));
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "fill",
                "color-mix(in srgb, accent-danger 12%, bg-panel) — opaque · Mocha (56,43,61) · Latte (236,214,222)",
            ),
            ("label", "text-primary · Mocha 9.18 · Latte 5.80"),
            (
                "edge",
                "1px accent-danger · vs surface-raised Mocha 5.43 · Latte 3.52 (≥ 3 non-text)",
            ),
            ("hover", "the normal button hover overlay on top"),
        ],
        &[
            TokenChip::new(
                "approval-danger-bg",
                "fill",
                theme.approval_danger_bg().to_egui(),
            ),
            TokenChip::new(
                "approval-danger-border",
                "→ accent-danger",
                theme.approval_danger_border().to_egui(),
            ),
            TokenChip::new(
                "approval-danger-fg",
                "→ text-primary",
                theme.approval_danger_fg().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Contrast is WCAG 2.x relative luminance on the computed sRGB values. Choice labels \
         in the sample are illustrative.",
    );
}

fn danger_column(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    // egui 버튼은 전역 스타일로 그려지므로 열의 테마 색을 이 열에만 적용한다.
    *ui.visuals_mut() = tasty_egui_theme::theme_visuals(theme);
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.set_width(DANGER_COLUMN_WIDTH.value());
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new(label)
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted_raised().to_egui()),
            );
            for (choice, destructive) in DANGER_CHOICES {
                approval_choice(ui, theme, choice, destructive);
            }
        });
}
