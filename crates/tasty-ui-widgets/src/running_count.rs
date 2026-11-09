//! 워크스페이스 행의 실행 중 surface 개수. 본체 사이드바와 갤러리가 같은 함수로 칸과 숫자를 정해
//! 이름 시작 위치와 표기가 갈라지지 않게 한다. 순서는 점 칸, 간격, 숫자 칸, 점 간격, 몸통이다.
//! 값이 0이면 숫자만 그리지 않고 칸은 비워 둔다. 값이 0과 1을 오갈 때 이름이 움직이지 않는다.

use egui::FontId;
use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

use crate::attention_count_label;

/// 칸 폭의 기준 글자. 0~99에서 이름이 움직이지 않도록 두 자리 폭을 언제나 예약한다.
const RESERVED_DIGITS: &str = "00";

/// 그릴 숫자. 0은 그리지 않고 99를 넘으면 배지와 같은 `99+`다.
pub fn running_count_label(count: usize) -> Option<String> {
    (count > 0).then(|| attention_count_label(count))
}

fn numeral_font(theme: &Theme) -> FontId {
    FontId::monospace(theme.workspace_running_count_font_size().value())
}

fn numeral_width(ui: &egui::Ui, theme: &Theme, text: &str) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(
            text.to_owned(),
            numeral_font(theme),
            egui::Color32::PLACEHOLDER,
        )
        .size()
        .x
    })
}

/// 숫자 칸 폭. 토큰과 실제 두 자리 폭 중 큰 값이고 `99+`인 행만 그 글자만큼 늘어난다.
/// 공칭 advance는 실제 폭과 달라 글자를 레이아웃해서 잰다.
pub fn running_count_column_width(ui: &egui::Ui, theme: &Theme, count: usize) -> f32 {
    let base = theme
        .workspace_running_count_min_width()
        .value()
        .max(numeral_width(ui, theme, RESERVED_DIGITS));
    match running_count_label(count) {
        Some(label) => base.max(numeral_width(ui, theme, &label)),
        None => base,
    }
}

/// 왼쪽 여백 뒤에서 몸통까지의 거리. 이름, mirror 줄, 부제, 설명이 모두 이 x에서 시작한다.
pub fn workspace_row_body_offset(ui: &egui::Ui, theme: &Theme, count: usize) -> f32 {
    theme.workspace_dot_slot().value()
        + theme.workspace_running_count_gap().value()
        + running_count_column_width(ui, theme, count)
        + theme.workspace_dot_gap().value()
}

/// 칸 왼쪽 끝에 숫자를 그린다. 세로는 칸 가운데다. 숫자를 그렸으면 true를 돌려준다.
pub fn paint_running_count(
    painter: &egui::Painter,
    theme: &Theme,
    column: egui::Rect,
    count: usize,
    ink: HexColor,
) -> bool {
    let Some(label) = running_count_label(count) else {
        return false;
    };
    painter.text(
        column.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        numeral_font(theme),
        ink.to_egui(),
    );
    true
}

/// 가로 레이아웃에 숫자 칸을 잡고 그린다. 점 칸을 `workspace-running-count-gap` 간격으로 잡은 뒤,
/// `workspace-dot-gap` 간격으로 부른다. 툴팁을 등록할 칸을 돌려주며 숫자가 없으면 None이다.
pub fn running_count_column(
    ui: &mut egui::Ui,
    theme: &Theme,
    count: usize,
    height: f32,
    ink: HexColor,
) -> Option<egui::Rect> {
    let width = running_count_column_width(ui, theme, count);
    let (column, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    paint_running_count(ui.painter(), theme, column, count, ink).then_some(column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RowParent, RowState};
    use tasty_type_geometry::length::LogicalPx;

    fn in_frame<R>(theme_zoom: f32, body: impl FnOnce(&mut egui::Ui, &Theme) -> R) -> R {
        let base = tasty_themes::mocha_fallback();
        let theme = Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme_zoom);
        let ctx = egui::Context::default();
        let mut result = None;
        let mut body = Some(body);
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(body) = body.take() {
                    result = Some(body(ui, &theme));
                }
            });
        }));
        result.expect("frame ran")
    }

    fn numeral_shapes(output: &egui::FullOutput) -> Vec<String> {
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            if let egui::Shape::Text(text) = &clipped.shape {
                texts.push(text.galley.text().to_owned());
            }
        }
        texts
    }

    #[test]
    fn zero_draws_nothing_and_values_share_the_badge_notation() {
        assert_eq!(running_count_label(0), None);
        assert_eq!(running_count_label(1).as_deref(), Some("1"));
        assert_eq!(running_count_label(12).as_deref(), Some("12"));
        assert_eq!(running_count_label(99).as_deref(), Some("99"));
        assert_eq!(running_count_label(100).as_deref(), Some("99+"));
    }

    #[test]
    fn the_overflow_notation_follows_the_design_maximum() {
        let max = tasty_design_tokens::generated::component::workspace::RUNNING_COUNT_MAX as usize;
        assert_eq!(running_count_label(max), Some(max.to_string()));
        assert_eq!(running_count_label(max + 1).as_deref(), Some("99+"));
    }

    #[test]
    fn the_column_holds_two_digits_and_the_token_at_every_scale() {
        for zoom in [0.85, 1.0, 1.2] {
            in_frame(zoom, |ui, theme| {
                let width = running_count_column_width(ui, theme, 12);
                assert!(width >= theme.workspace_running_count_min_width().value());
                assert!(width >= numeral_width(ui, theme, "00"), "zoom {zoom}");
            });
        }
    }

    #[test]
    fn only_a_row_past_the_maximum_moves_the_body() {
        in_frame(1.0, |ui, theme| {
            let at_one = workspace_row_body_offset(ui, theme, 1);
            assert_eq!(at_one, workspace_row_body_offset(ui, theme, 12));
            assert_eq!(at_one, workspace_row_body_offset(ui, theme, 99));
            let over = workspace_row_body_offset(ui, theme, 100);
            assert!(over > at_one, "{over} <= {at_one}");
        });
    }

    #[test]
    fn a_zero_row_keeps_the_column_so_the_name_does_not_move() {
        for zoom in [0.85, 1.0, 1.2] {
            in_frame(zoom, |ui, theme| {
                assert_eq!(
                    workspace_row_body_offset(ui, theme, 0),
                    workspace_row_body_offset(ui, theme, 1),
                    "zoom {zoom}"
                );
            });
        }
    }

    #[test]
    fn the_body_offset_is_the_sum_of_the_row_columns() {
        in_frame(1.0, |ui, theme| {
            let width = running_count_column_width(ui, theme, 1);
            assert_eq!(
                workspace_row_body_offset(ui, theme, 1),
                theme.workspace_dot_slot().value()
                    + theme.workspace_running_count_gap().value()
                    + width
                    + theme.workspace_dot_gap().value()
            );
        });
    }

    #[test]
    fn a_zero_row_paints_no_numeral_and_reports_no_tooltip_zone() {
        let ctx = egui::Context::default();
        let theme = tasty_themes::mocha_fallback();
        let mut zones = Vec::new();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let ink = theme.text_muted();
                zones.push(running_count_column(
                    ui,
                    &theme,
                    0,
                    LogicalPx(20.0).value(),
                    ink,
                ));
                zones.push(running_count_column(
                    ui,
                    &theme,
                    3,
                    LogicalPx(20.0).value(),
                    ink,
                ));
            });
        });
        assert!(zones[0].is_none());
        assert!(zones[1].is_some());
        assert_eq!(numeral_shapes(&output), ["3"]);
    }

    /// 대비를 잰 여섯 칸의 토큰. 디자인 회신의 표와 같다.
    #[test]
    fn the_numeral_ink_follows_the_row_state_table() {
        let theme = tasty_themes::mocha_fallback();
        let meta = |state: RowState| state.meta(&theme, RowParent::Mantle);
        assert_eq!(meta(RowState::Rest), theme.text_muted());
        assert_eq!(meta(RowState::Hover), theme.text_muted_raised());
        assert_eq!(meta(RowState::Press), theme.text_muted_raised());
        assert_eq!(meta(RowState::Selected), theme.text_on_active());
        assert_eq!(meta(RowState::SelectedHover), theme.text_on_active_hover());
        assert_eq!(meta(RowState::SelectedPress), theme.text_on_active_hover());
    }
}
