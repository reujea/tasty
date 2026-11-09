//! Badge, Tag, Kbd를 각각 보여주는 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    BadgeVariant, TagVariant, badge, badge_disabled, badge_dot, disabled_chip_scope, kbd, tag,
    tag_disabled, tag_mono,
};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, stage};

/// Badge — count pill + dot.
pub fn draw_badge(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        cluster(ui, theme, "counts", |ui| {
            badge(ui, theme, "3", BadgeVariant::Danger);
            badge(ui, theme, "99+", BadgeVariant::Danger);
            badge(ui, theme, "12", BadgeVariant::Primary);
            badge(ui, theme, "2", BadgeVariant::Warning);
            badge(ui, theme, "new", BadgeVariant::Agent);
            badge(ui, theme, "ok", BadgeVariant::Success);
        });
        // 시안 BadgeGroup — NeedsInput이 앞, Completion이 뒤, 사이는 badge-group-gap.
        cluster(ui, theme, "attention kinds", |ui| {
            for (needs_input, completion) in [("2", "5"), ("99+", "99+")] {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.badge_group_gap().value();
                    badge(ui, theme, needs_input, BadgeVariant::Warning);
                    badge(ui, theme, completion, BadgeVariant::Primary);
                });
            }
        });
        cluster(ui, theme, "dot", |ui| {
            badge_dot(ui, theme, BadgeVariant::Danger);
            badge_dot(ui, theme, BadgeVariant::Warning);
            badge_dot(ui, theme, BadgeVariant::Primary);
            badge_dot(ui, theme, BadgeVariant::Agent);
            badge_dot(ui, theme, BadgeVariant::Success);
        });
        // 시안 Badge `disabled` — 모든 variant가 중립 채움과 disabled ink 한 벌이다.
        cluster(ui, theme, "disabled", |ui| {
            badge_disabled(ui, theme, "3");
            badge_disabled(ui, theme, "12");
            badge_disabled(ui, theme, "ok");
            disabled_chip_scope(ui, tasty_ui_widgets::Ground::Panel, |ui| {
                badge_dot(ui, theme, BadgeVariant::Danger);
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("radius", "pill (full)"),
            ("font", "caption 11px"),
            ("dot", "status-dot-size"),
            ("group gap", "badge-group-gap"),
            ("overflow", "99+"),
        ],
        &[
            TokenChip::new(
                "badge-danger-bg",
                "default",
                egui::Color32::from(theme.badge_danger_bg()),
            ),
            TokenChip::new(
                "badge-warning-bg",
                "NeedsInput count",
                egui::Color32::from(theme.badge_warning_bg()),
            ),
            TokenChip::new(
                "badge-primary-bg",
                "Completion count",
                egui::Color32::from(theme.badge_primary_bg()),
            ),
            TokenChip::new(
                "badge-agent-bg",
                "agent",
                egui::Color32::from(theme.badge_agent_bg()),
            ),
            TokenChip::new(
                "badge-disabled-bg",
                "disabled fill",
                egui::Color32::from(theme.badge_disabled_bg()),
            ),
            TokenChip::without_color("font-size-caption", "11px"),
        ],
    );
}

/// Tag — outlined chip + state dot.
pub fn draw_tag(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        cluster(ui, theme, "variants", |ui| {
            tag_mono(ui, theme, "terminal", TagVariant::Default, false);
            tag_mono(ui, theme, "markdown", TagVariant::Accent, false);
            tag(ui, theme, "plugin", TagVariant::Agent, false);
            tag(ui, theme, "main", TagVariant::Info, false);
            tag(ui, theme, "running", TagVariant::Success, true);
            tag(ui, theme, "readonly", TagVariant::Warning, true);
            tag(
                ui,
                theme,
                crate::i18n::t("remote_attach.in_use"),
                TagVariant::Attached,
                false,
            );
            tag(ui, theme, "error", TagVariant::Danger, true);
        });
        // 시안 Tag `mono` — ID·버전·크기·권한은 모든 언어에서 고정폭이다.
        cluster(ui, theme, "mono", |ui| {
            tag_mono(ui, theme, "fs:read", TagVariant::Default, false);
            tag_mono(ui, theme, "v1.2.0", TagVariant::Default, false);
            tag_mono(ui, theme, "s_05MD", TagVariant::Default, false);
            tag_mono(ui, theme, "3.2 MB", TagVariant::Warning, false);
        });
        // 시안 Tag `disabled` — accent 채움과 tint 테두리가 빠지고 점도 같은 ink다.
        cluster(ui, theme, "disabled edge on a ground", |ui| {
            for (ground, name) in crate::catalog::spec::GROUND_CELLS {
                crate::catalog::spec::on_ground(ui, theme, ground, |ui| {
                    tag_disabled(ui, theme, name, false, ground);
                });
            }
        });
        cluster(ui, theme, "disabled", |ui| {
            tag_disabled(
                ui,
                theme,
                "terminal",
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            tag_disabled(
                ui,
                theme,
                "markdown",
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            tag_disabled(ui, theme, "running", true, tasty_ui_widgets::Ground::Panel);
        });
    });

    meta(
        ui,
        theme,
        &[
            ("font", "font-label"),
            ("font, mono", "font-mono"),
            ("radius", "radius-sm 2"),
            ("dot", "leading status-dot"),
        ],
        &[
            TokenChip::without_color("font-label", "label"),
            TokenChip::without_color("font-mono", "label, mono"),
            TokenChip::without_color("border-default", "outline"),
            TokenChip::new(
                "accent-info",
                "info tone",
                egui::Color32::from(theme.accent_info()),
            ),
            TokenChip::new(
                "tag-disabled-fg",
                "disabled ink",
                egui::Color32::from(theme.tag_disabled_fg()),
            ),
        ],
    );
}

/// Kbd — keycaps.
pub fn draw_kbd(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        cluster(ui, theme, "plus on a ground", |ui| {
            for (ground, name) in crate::catalog::spec::GROUND_CELLS {
                crate::catalog::spec::on_ground(ui, theme, ground, |ui| {
                    ui.label(
                        egui::RichText::new(name)
                            .size(theme.font_size_caption.value())
                            .color(if ground == tasty_ui_widgets::Ground::Active {
                                theme.text_on_active().to_egui()
                            } else {
                                theme.text_muted().to_egui()
                            }),
                    );
                    kbd(ui, theme, "Ctrl+K", ground);
                });
            }
        });
        cluster(ui, theme, "shortcuts", |ui| {
            kbd(ui, theme, "Ctrl+K", tasty_ui_widgets::Ground::Panel);
            kbd(ui, theme, "Ctrl+Shift+N", tasty_ui_widgets::Ground::Panel);
            kbd(ui, theme, "⌘,", tasty_ui_widgets::Ground::Panel);
            kbd(ui, theme, "Esc", tasty_ui_widgets::Ground::Panel);
        });
    });

    meta(
        ui,
        theme,
        &[
            ("font", "mono"),
            ("radius", "radius-sm 2"),
            ("fill", "surface-raised"),
        ],
        &[
            TokenChip::without_color("font-mono", "keycap"),
            TokenChip::new(
                "surface-raised",
                "keycap fill",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::without_color("border-default", "cap border"),
        ],
    );
}
