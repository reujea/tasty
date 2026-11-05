//! 공용 StatusDot의 실행·대기·오류 상태와 attention 종류 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{StatusKind, status_dot};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, stage};

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        for (ground, name) in crate::catalog::spec::GROUND_CELLS {
            cluster(ui, theme, name, |ui| {
                crate::catalog::spec::on_ground(ui, theme, ground, |ui| {
                    for (kind, label) in [
                        (StatusKind::Idle, "idle"),
                        (StatusKind::Running, "running"),
                        (StatusKind::Waiting, "waiting"),
                        (StatusKind::Error, "error"),
                        (StatusKind::Agent, "agent"),
                        (StatusKind::Completion, "completion"),
                    ] {
                        status_dot(ui, theme, kind, label, false, true, ground);
                    }
                });
            });
        }
        cluster(ui, theme, "states", |ui| {
            status_dot(
                ui,
                theme,
                StatusKind::Running,
                "running",
                true,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            status_dot(
                ui,
                theme,
                StatusKind::Agent,
                "agent",
                true,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            status_dot(
                ui,
                theme,
                StatusKind::Waiting,
                "waiting",
                false,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            status_dot(
                ui,
                theme,
                StatusKind::Idle,
                "idle",
                false,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            status_dot(
                ui,
                theme,
                StatusKind::Error,
                "error",
                false,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
        });
        cluster(ui, theme, "attention kinds", |ui| {
            status_dot(
                ui,
                theme,
                StatusKind::NeedsInput,
                "needs input",
                false,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
            status_dot(
                ui,
                theme,
                StatusKind::Completion,
                "completion",
                false,
                false,
                tasty_ui_widgets::Ground::Panel,
            );
        });
    });

    meta(
        ui,
        theme,
        &[
            ("dot", "status-dot-size"),
            ("pulse", "ring on running / agent"),
            ("agent", "accent-agent"),
            ("attention rank", "needs-input › completion › activity"),
        ],
        &[
            TokenChip::new(
                "accent-success",
                "running",
                egui::Color32::from(theme.accent_success()),
            ),
            TokenChip::new(
                "accent-agent",
                "agent",
                egui::Color32::from(theme.accent_agent()),
            ),
            TokenChip::new(
                "accent-warning",
                "waiting",
                egui::Color32::from(theme.accent_warning()),
            ),
            TokenChip::new(
                "accent-danger",
                "error",
                egui::Color32::from(theme.accent_danger()),
            ),
            TokenChip::new(
                "status-dot-needs-input",
                "needs input",
                egui::Color32::from(theme.status_dot_needs_input()),
            ),
            TokenChip::new(
                "status-dot-completion",
                "completion",
                egui::Color32::from(theme.status_dot_completion()),
            ),
            TokenChip::new(
                "status-dot-idle",
                "idle",
                egui::Color32::from(theme.status_dot_idle()),
            ),
        ],
    );
}
