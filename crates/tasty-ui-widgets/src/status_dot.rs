//! 상태 점과 선택적인 라벨. 색상과 점 크기는 Theme의 상태 점 토큰을 사용한다.
//! pulse가 켜지고 reduced_motion이 꺼진 경우에만 확장·페이드 링을 그린다.

use tasty_type_appearance::theme::Theme;

use crate::control::Ground;

const RING_INSET: f32 = 3.0; // CSS inset:-3px → base 반경 dot/2 + 3
const PULSE_SCALE_MIN: f32 = 0.6;
const PULSE_SCALE_RANGE: f32 = 1.2;
const PULSE_OPACITY: f32 = 0.5;

/// 디자인 StatusDot status. NeedsInput·Completion은 실행 상태가 아닌 attention 종류이며
/// 같은 점 자리를 순위(needs-input > completion > 실행 상태)로 차지한다.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Running,
    Idle,
    Agent,
    Waiting,
    Error,
    NeedsInput,
    Completion,
}

impl StatusKind {
    /// 바탕 `ground` 위의 점 색. raised에서는 유휴와 대기 점만 그 칸의 잉크로 바뀐다.
    fn color(self, theme: &Theme, ground: Ground) -> egui::Color32 {
        let color = match (self, ground) {
            (StatusKind::Running, Ground::Active) => theme.status_dot_success_active(),
            (StatusKind::Running, _) => theme.status_dot_success(),
            (StatusKind::Idle, Ground::Active) => theme.status_dot_idle_active(),
            (StatusKind::Idle, Ground::Raised) => theme.glyph_dim_raised(),
            (StatusKind::Idle, Ground::Panel) => theme.status_dot_idle(),
            (StatusKind::Agent, Ground::Active) => theme.status_dot_agent_active(),
            (StatusKind::Agent, _) => theme.status_dot_agent(),
            (StatusKind::Waiting, Ground::Active) => theme.status_dot_warning_active(),
            (StatusKind::Waiting, Ground::Raised) => theme.accent_warning_raised(),
            (StatusKind::Waiting, Ground::Panel) => theme.status_dot_warning(),
            (StatusKind::Error, Ground::Active) => theme.status_dot_danger_active(),
            (StatusKind::Error, _) => theme.status_dot_danger(),
            (StatusKind::NeedsInput, Ground::Active) => theme.status_dot_needs_input_active(),
            (StatusKind::NeedsInput, _) => theme.status_dot_needs_input(),
            (StatusKind::Completion, Ground::Active) => theme.status_dot_completion_active(),
            (StatusKind::Completion, _) => theme.status_dot_completion(),
        };
        color.to_egui()
    }
}

/// 상태 점과 라벨을 그린다. 라벨이 비면 간격 없이 점 하나의 폭만 사용한다.
/// `ground`는 점이 놓인 바탕이다. `Active`면 surface-active 위의 `status-dot-*-active` 색을 쓴다.
/// 공용 표의 선택 행은 셀 내용이 행 상태를 모르므로 표 문맥도 `Active`로 읽는다.
pub fn status_dot(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: StatusKind,
    label: &str,
    pulse: bool,
    reduced_motion: bool,
    ground: Ground,
) -> egui::Response {
    let ground = if crate::in_selected_table_row(ui) {
        Ground::Active
    } else {
        ground
    };
    let selected = ground == Ground::Active;
    let dot = theme.status_dot_size().value();
    // 디자인 `.tasty-statusdot`의 점과 라벨 사이 `space-xs`.
    let gap = theme.spacing_xs.value();
    let caption = theme.font_size_caption.value();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(caption),
        egui::Color32::PLACEHOLDER,
    );
    let h = dot.max(galley.rect.height());
    let w = dot
        + if label.is_empty() {
            0.0
        } else {
            gap + galley.rect.width()
        };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());

    let dot_center = egui::pos2(rect.left() + dot * 0.5, rect.center().y);
    let color = kind.color(theme, ground);

    if pulse && !reduced_motion {
        let t = ui.ctx().input(|i| i.time);
        let period = theme.status_dot_pulse_duration().to_secs_f64();
        let phase = (t / period).rem_euclid(1.0) as f32;
        let eased = 1.0 - (1.0 - phase).powi(3); // ease-out cubic
        let radius = (dot * 0.5 + RING_INSET) * (PULSE_SCALE_MIN + PULSE_SCALE_RANGE * eased);
        let ring = color.gamma_multiply(PULSE_OPACITY * (1.0 - eased));
        ui.painter().circle_filled(dot_center, radius, ring);
        ui.ctx().request_repaint();
    }
    if selected && kind == StatusKind::Idle {
        // 선택 행에서 유휴 점과 완료 점은 색 차이가 작아 모양으로 구분한다.
        let width = theme.status_dot_ring_width().value();
        ui.painter().circle_stroke(
            dot_center,
            dot * 0.5 - width * 0.5,
            egui::Stroke::new(width, color),
        );
    } else {
        ui.painter().circle_filled(dot_center, dot * 0.5, color);
    }

    if !label.is_empty() {
        let pos = egui::pos2(
            rect.left() + dot + gap,
            rect.center().y - galley.rect.height() * 0.5,
        );
        let ink = if selected {
            theme.text_on_active().to_egui()
        } else {
            crate::table_cell_ink(ui, theme, theme.text_secondary())
        };
        ui.painter().galley(pos, galley, ink);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 배율 `zoom` 에서 라벨 `label` 로 그린 상태 점의 폭과, 같은 글꼴로 잰 라벨 폭을 돌려준다.
    fn draw(zoom: f32, label: &str) -> (f32, f32) {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
        let ctx = egui::Context::default();
        let mut out = None;
        for _ in 0..2 {
            drop(ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let resp = status_dot(
                        ui,
                        &theme,
                        StatusKind::Idle,
                        label,
                        false,
                        true,
                        Ground::Panel,
                    );
                    let label_w = ui
                        .painter()
                        .layout_no_wrap(
                            label.to_owned(),
                            egui::FontId::proportional(theme.font_size_caption.value()),
                            egui::Color32::PLACEHOLDER,
                        )
                        .rect
                        .width();
                    out = Some((resp.rect.width(), label_w));
                });
            }));
        }
        out.expect("drawn")
    }

    /// 선택 행의 모든 상태가 자기 `-active` 토큰을 읽는다. idle은 행 글자색이다.
    #[test]
    fn a_selected_row_reads_the_active_dot_tokens() {
        let theme = tasty_themes::mocha_fallback();
        for (kind, rest, active) in [
            (
                StatusKind::Running,
                theme.status_dot_success(),
                theme.status_dot_success_active(),
            ),
            (
                StatusKind::Idle,
                theme.status_dot_idle(),
                theme.text_on_active(),
            ),
            (
                StatusKind::Agent,
                theme.status_dot_agent(),
                theme.status_dot_agent_active(),
            ),
            (
                StatusKind::Waiting,
                theme.status_dot_warning(),
                theme.status_dot_warning_active(),
            ),
            (
                StatusKind::Error,
                theme.status_dot_danger(),
                theme.status_dot_danger_active(),
            ),
            (
                StatusKind::NeedsInput,
                theme.status_dot_needs_input(),
                theme.status_dot_needs_input_active(),
            ),
            (
                StatusKind::Completion,
                theme.status_dot_completion(),
                theme.status_dot_completion_active(),
            ),
        ] {
            assert_eq!(kind.color(&theme, Ground::Panel), rest.to_egui());
            assert_eq!(kind.color(&theme, Ground::Active), active.to_egui());
        }
    }

    /// raised에서는 유휴와 대기 점만 그 칸의 잉크로 바뀐다.
    #[test]
    fn only_idle_and_waiting_change_on_raised() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        assert_eq!(
            StatusKind::Idle.color(&theme, Ground::Raised),
            theme.glyph_dim_raised().to_egui()
        );
        assert_eq!(
            StatusKind::Waiting.color(&theme, Ground::Raised),
            theme.accent_warning_raised().to_egui()
        );
        assert_eq!(
            StatusKind::Running.color(&theme, Ground::Raised),
            StatusKind::Running.color(&theme, Ground::Panel)
        );
    }

    #[test]
    fn a_labelled_dot_is_dot_plus_space_xs_plus_label_wide() {
        for zoom in [1.0, 1.2] {
            let theme =
                Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
            let (width, label_w) = draw(zoom, "LISTEN");
            assert_eq!(
                width,
                theme.status_dot_size().value() + theme.spacing_xs.value() + label_w,
                "zoom {zoom}"
            );
        }
    }

    #[test]
    fn a_dot_without_a_label_is_only_the_dot_wide() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        assert_eq!(draw(1.0, "").0, theme.status_dot_size().value());
    }
}
