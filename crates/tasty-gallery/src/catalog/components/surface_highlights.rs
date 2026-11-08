//! 터미널의 포커스·비포커스·에이전트 상태를 정적으로 비교한다.
//! 본체에서 실제 사용자 포커스를 받으면 Completion과 NeedsInput을 모두 해제한다.
//! 이 예제 자체는 포커스 전환이나 알림 상태 변경을 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::layouts_settled::terminal_pane;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 시안 무대의 높이(`height: 220`).
const STAGE_H: LogicalPx = LogicalPx(220.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        let (area, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), STAGE_H.value()),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(area, 0.0, theme.bg_panel().to_egui());
        let inner = area.shrink(theme.spacing_md.value());
        let gap = theme.spacing_sm.value();
        let half = (inner.width() - gap) * 0.5;
        let left = egui::Rect::from_min_size(inner.min, egui::vec2(half, inner.height()));
        let right = egui::Rect::from_min_size(
            egui::pos2(left.right() + gap, inner.top()),
            egui::vec2(half, inner.height()),
        );
        ui.push_id("agent-pane", |ui| {
            terminal_pane(ui, theme, left, "agent · ai-review", false, true)
        });
        ui.push_id("focused-pane", |ui| {
            terminal_pane(ui, theme, right, "you · zsh", true, false)
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("focused bg", "surface-terminal-focused-bg"),
            ("unfocused bg", "surface-terminal-unfocused-bg"),
            ("agent marker", "static accent-agent dot in this example"),
            (
                "attention on focus",
                "clear_attention — kind(Completion/NeedsInput) 무관 단일 해제 규칙",
            ),
        ],
        &[
            TokenChip::new(
                "surface-terminal-focused-bg",
                "focused bed",
                theme.surface("terminal").focused_bg.into(),
            ),
            TokenChip::new(
                "surface-terminal-unfocused-bg",
                "unfocused bed",
                theme.surface("terminal").unfocused_bg.into(),
            ),
            TokenChip::new("accent-agent", "agent dot", theme.accent_agent().into()),
            TokenChip::new(
                "accent-success",
                "your running dot",
                theme.accent_success().into(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "포커스 터미널은 Mocha 검정, Latte 흰색 바탕과 강한 글자를 쓰고 비포커스 터미널은 비포커스 바탕과 약한 글자를 쓴다. Tasty가 바탕을 칠하는 모든 surface가 같은 쌍을 따른다. 본체의 실제 사용자 포커스 전환은 해당 서피스의 완료·응답 대기 알림도 해제한다.",
    );
}
