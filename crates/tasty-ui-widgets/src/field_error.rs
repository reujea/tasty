//! 입력 아래 오류 줄과 그 상태. 줄은 컨트롤 열에서 시작하고 caption 크기 `accent-danger-raised` 로
//! `measure-md` (남은 폭이 더 좁으면 그 폭)에서 줄을 바꾼다. 위 컨트롤과의 간격은 `settings-row-caption-gap`.
//!
//! 입력에 포커스가 있는 동안에는 빨강을 그리지 않지만, 보이던 줄의 칸은 남긴다. 포커스를 넣고 뺄
//! 때마다 아래 행이 움직이면 그 행을 누르는 클릭이 빗나갈 수 있어서다. 판정 결과가 바뀔 때만 한 번
//! 움직인다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 행 아래에 그릴 오류 줄. `ink` 가 거짓이면 글자 없이 칸만 차지한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldErrorLine<'a> {
    pub text: &'a str,
    pub ink: bool,
}

/// 오류 줄의 표시 상태.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldErrorState {
    /// 줄이 없다.
    #[default]
    Clear,
    /// 줄과 빨간 테두리를 그린다.
    Shown,
    /// 입력 중이라 글자는 숨기고 칸만 유지한다.
    Held,
}

impl FieldErrorState {
    /// 지난 프레임 상태에서 이번 프레임 상태를 정한다. `active` 는 입력의 포커스나 열린 목록이다.
    pub fn step(self, active: bool, rejected: bool) -> Self {
        match (active, rejected) {
            (true, _) if self == Self::Clear => Self::Clear,
            (true, _) => Self::Held,
            (false, true) => Self::Shown,
            (false, false) => Self::Clear,
        }
    }

    pub fn line(self, text: &str) -> Option<FieldErrorLine<'_>> {
        match self {
            Self::Clear => None,
            Self::Shown => Some(FieldErrorLine { text, ink: true }),
            Self::Held => Some(FieldErrorLine { text, ink: false }),
        }
    }

    /// 빨간 테두리를 그리는 상태인지.
    pub fn invalid(self) -> bool {
        self == Self::Shown
    }
}

/// `id` 로 지난 상태를 기억해 이번 프레임 상태를 정한다.
pub fn field_error_state(
    ctx: &egui::Context,
    id: egui::Id,
    active: bool,
    rejected: bool,
) -> FieldErrorState {
    let key = id.with("field_error_state");
    let last: FieldErrorState = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let next = last.step(active, rejected);
    ctx.data_mut(|d| d.insert_temp(key, next));
    next
}

/// 왼쪽을 `indent` 만큼 띄우고 줄을 그린다.
pub(crate) fn paint_field_error_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    indent: LogicalPx,
    line: FieldErrorLine<'_>,
) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(indent.value());
        let width = theme.measure_md.value().min(ui.available_width()).max(1.0);
        let caption = theme.font_size_caption.value();
        let color = theme.accent_danger_raised().to_egui();
        let galley = ui.fonts(|f| {
            let mut job = egui::text::LayoutJob::single_section(
                line.text.to_owned(),
                egui::TextFormat {
                    font_id: egui::FontId::proportional(caption),
                    color,
                    line_height: Some(caption * theme.line_height_ui),
                    ..Default::default()
                },
            );
            job.wrap.max_width = width;
            f.layout_job(job)
        });
        let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
        if line.ink {
            ui.painter().galley(rect.min, galley, color);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::FieldErrorState::{self, Clear, Held, Shown};

    #[test]
    fn taking_focus_keeps_only_the_box_of_a_visible_line() {
        assert_eq!(Shown.step(true, true), Held);
        assert_eq!(Shown.step(true, false), Held);
        assert_eq!(Held.step(true, false), Held);
    }

    #[test]
    fn taking_focus_on_a_good_value_opens_no_box_while_typing() {
        assert_eq!(Clear.step(true, true), Clear);
        assert_eq!(Clear.step(true, false), Clear);
    }

    #[test]
    fn leaving_follows_the_verdict_at_that_moment() {
        assert_eq!(Held.step(false, false), Clear);
        assert_eq!(Held.step(false, true), Shown);
        assert_eq!(Clear.step(false, true), Shown);
    }

    #[test]
    fn only_the_shown_state_is_invalid_or_has_ink() {
        assert!(Shown.invalid());
        assert!(!Held.invalid());
        assert!(Shown.line("x").is_some_and(|l| l.ink));
        assert!(Held.line("x").is_some_and(|l| !l.ink));
        assert_eq!(FieldErrorState::Clear.line("x"), None);
    }
}
