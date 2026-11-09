//! 입력 아래 오류 줄과 그 상태. 줄은 컨트롤 열에서 시작하고 caption 크기 `accent-danger-raised` 로
//! `measure-md` (남은 폭이 더 좁으면 그 폭)에서 줄을 바꾼다. 위 컨트롤과의 간격은 `settings-row-caption-gap`.
//!
//! 칸의 높이는 입력이 활성이 아니고 포인터 버튼이 눌려 있지 않은 프레임에만 바뀐다. 누른 자리의 컨트롤이
//! 뗄 때까지 그 자리에 있어야 클릭이 성립하기 때문이다. 빨강은 눌림과 상관없이 판정을 바로 따른다.

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
    /// 칸도 빨강도 없다.
    #[default]
    Clear,
    /// 칸에 줄을 그리고 빨강을 그린다.
    Shown,
    /// 칸만 있고 빨강이 없다.
    Held,
    /// 거절됐지만 포인터가 눌려 있어 칸이 아직 없다. 빨강 테두리만 그린다.
    Due,
}

impl FieldErrorState {
    /// 지난 프레임 상태에서 이번 프레임 상태를 정한다. `active` 는 입력의 포커스, 열린 목록, 판정 대기다.
    pub fn step(self, active: bool, pointer_held: bool, rejected: bool) -> Self {
        let had_box = matches!(self, Self::Held | Self::Shown);
        let has_box = if active || pointer_held {
            had_box
        } else {
            rejected
        };
        match (has_box, !active && rejected) {
            (false, false) => Self::Clear,
            (false, true) => Self::Due,
            (true, false) => Self::Held,
            (true, true) => Self::Shown,
        }
    }

    pub fn line(self, text: &str) -> Option<FieldErrorLine<'_>> {
        match self {
            Self::Clear | Self::Due => None,
            Self::Shown => Some(FieldErrorLine { text, ink: true }),
            Self::Held => Some(FieldErrorLine { text, ink: false }),
        }
    }

    /// 빨간 테두리를 그리는 상태인지.
    pub fn invalid(self) -> bool {
        matches!(self, Self::Shown | Self::Due)
    }
}

/// 지금 누름이 창이 포커스를 잃기 전에 시작돼 뗌을 받지 못한 것인지 정한다. egui 는 `Event::PointerButton`
/// 으로만 눌림을 지우므로 창 밖에서 뗀 버튼은 눌린 채로 남는다.
fn press_is_stale(was_stale: bool, focused: bool, down: bool, pressed: bool) -> bool {
    if !down || (focused && pressed) {
        false
    } else {
        was_stale || !focused
    }
}

/// 어느 버튼이든 누른 프레임부터 그 뗌을 처리하는 프레임까지 참이다. 창이 포커스를 잃으면 거짓이다.
fn pointer_held(ctx: &egui::Context) -> bool {
    let key = egui::Id::new("field_error_stale_press");
    let was_stale = ctx.data(|d| d.get_temp(key)).unwrap_or(false);
    let (focused, down, pressed, released) = ctx.input(|i| {
        (
            i.focused,
            i.pointer.any_down(),
            i.pointer.any_pressed(),
            i.pointer.any_released(),
        )
    });
    let stale = press_is_stale(was_stale, focused, down, pressed);
    ctx.data_mut(|d| d.insert_temp(key, stale));
    focused && !stale && (down || released)
}

/// `id` 로 지난 상태를 기억해 이번 프레임 상태를 정한다. 뗌을 처리한 프레임에는 이벤트가 있어 egui 가 바로
/// 다음 프레임을 요청하므로(`InputState::wants_repaint_after`) 미룬 칸이 다음 입력까지 남지 않는다.
pub fn field_error_state(
    ctx: &egui::Context,
    id: egui::Id,
    active: bool,
    rejected: bool,
) -> FieldErrorState {
    let key = id.with("field_error_state");
    let last: FieldErrorState = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let next = last.step(active, pointer_held(ctx), rejected);
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
#[path = "field_error_tests.rs"]
mod tests;
