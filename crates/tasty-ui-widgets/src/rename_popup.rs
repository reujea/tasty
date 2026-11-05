//! 이름 변경 popup 콘텐츠. 본체 rename popup 과 갤러리 예제가 이 함수를 함께 부른다. 문자열은 호출자가
//! 주입하고 popup 높이 측정은 호출자가 맡는다. 콘텐츠 여백은 이 함수가 넣으므로 호출자는 여백 없는
//! 영역을 준다. 치수는 대상에 따라 두 시안 중 하나를 따른다([`RenamePopupLayout`]).

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::ControlSize;
use crate::button::{Button, ButtonVariant};
use crate::input::Input;
use crate::spacing::margin_sym;

/// 대상별 시안. 치수만 다르고 입력·버튼·키 동작은 같다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenamePopupLayout {
    /// 워크스페이스·부제·탭·카테고리. 시안 `RenameFrame`·`CategoryEditFrame`: 네 변과 입력 → 버튼
    /// 사이가 size-14, 버튼 md.
    Dialog,
    /// 탐색기 항목·즐겨찾기 추가. 시안 `RenamePending`: 위아래 space-md · 좌우 size-14, 행 사이 space-sm,
    /// 버튼 sm.
    Explorer,
}

/// 콘텐츠 입력값. 본체는 번역 문자열을, 갤러리는 예제 문자열을 넣는다.
pub struct RenamePopupView<'a> {
    pub layout: RenamePopupLayout,
    pub save: &'a str,
    pub cancel: &'a str,
    /// 입력 아래 accent-danger 검증 문구. `Some` 이면 입력 테두리도 invalid 다.
    pub error: Option<&'a str>,
    /// false 이면 Save 와 Enter 확정을 모두 막는다.
    pub save_enabled: bool,
    /// 원격의 답을 기다리는 동안의 문구("Renaming on {host}…"). `Some` 이면 입력을 읽기 전용으로
    /// 두고 Spinner 줄을 보이며, 두 버튼과 Enter·Esc 를 막는다.
    pub waiting: Option<&'a str>,
    /// 입력에 포커스를 주고 처음 포커스를 얻을 때 전체를 선택한다. 예제가 여럿인 갤러리는 끈다.
    pub autofocus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenamePopupAction {
    None,
    Cancel,
    Confirm(String),
}

/// 시안 `size-14` 안쪽 여백. 디자인 토큰 `explorer-props-padding-x` 가 이 popup 계열(Properties ·
/// 충돌 · rename)의 안쪽 여백으로 정의한 값이다.
pub fn rename_pad_x(theme: &Theme) -> LogicalPx {
    theme.explorer_props_padding_x()
}

struct Metrics {
    pad_y: LogicalPx,
    /// 입력(또는 오류·대기 줄)과 버튼 행 사이.
    footer_gap: LogicalPx,
    /// 입력과 오류·대기 줄 사이.
    line_gap: LogicalPx,
    button: ControlSize,
}

fn metrics(theme: &Theme, layout: RenamePopupLayout) -> Metrics {
    match layout {
        // 오류 줄 위 간격은 시안 `CategoryEditFrame` 의 6px 에 대응하는 역할 토큰이 없어 space-xs 다.
        RenamePopupLayout::Dialog => Metrics {
            pad_y: rename_pad_x(theme),
            footer_gap: rename_pad_x(theme),
            line_gap: theme.spacing_xs,
            button: ControlSize::Md,
        },
        RenamePopupLayout::Explorer => Metrics {
            pad_y: theme.spacing_md,
            footer_gap: theme.spacing_sm,
            line_gap: theme.spacing_sm,
            button: ControlSize::Sm,
        },
    }
}

/// 오류·대기 줄이 없을 때의 콘텐츠 높이. 아직 재지 않은 첫 프레임의 popup 높이에 쓴다.
pub fn rename_popup_base_height(theme: &Theme, layout: RenamePopupLayout) -> LogicalPx {
    let m = metrics(theme, layout);
    m.pad_y + theme.input_height() + m.footer_gap + LogicalPx(m.button.height(theme)) + m.pad_y
}

/// 입력 → (오류 또는 대기 줄) → 우측 정렬 버튼 행을 그린다.
/// Enter/Save 는 확정, Escape/Cancel 은 취소다. Enter 는 입력 포커스가 필요하다.
/// 같은 프레임에 둘 다 발생하면 확정을 우선한다.
pub fn rename_popup(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &RenamePopupView<'_>,
    buffer: &mut String,
) -> RenamePopupAction {
    let m = metrics(theme, view.layout);
    egui::Frame::NONE
        .inner_margin(margin_sym(rename_pad_x(theme), m.pad_y))
        .show(ui, |ui| content(ui, theme, &m, view, buffer))
        .inner
}

fn content(
    ui: &mut egui::Ui,
    theme: &Theme,
    m: &Metrics,
    view: &RenamePopupView<'_>,
    buffer: &mut String,
) -> RenamePopupAction {
    let ctx = ui.ctx().clone();
    let waiting = view.waiting.is_some();
    if !waiting && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return RenamePopupAction::Cancel;
    }
    ui.spacing_mut().item_spacing = egui::vec2(theme.spacing_sm.value(), 0.0);

    // 기다리는 동안 보낸 이름은 Input readOnly 모양으로 보이고 고치지 못한다.
    let resp = Input::new()
        .read_only(waiting)
        .invalid(view.error.is_some())
        .show(ui, theme, buffer);

    if view.autofocus && !resp.has_focus() {
        resp.request_focus();
    }
    if resp.gained_focus()
        && let Some(mut state) = egui::TextEdit::load_state(&ctx, resp.id)
    {
        let len = buffer.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(len),
            )));
        state.store(&ctx, resp.id);
    }

    let mut confirm = !waiting
        && view.save_enabled
        && resp.has_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut cancel = false;

    if let Some(text) = view.waiting {
        ui.add_space(m.line_gap.value());
        waiting_line(ui, theme, text);
    } else if let Some(err) = view.error {
        ui.add_space(m.line_gap.value());
        ui.add(
            egui::Label::new(
                egui::RichText::new(err)
                    .size(theme.font_size_caption.value())
                    .color(theme.accent_danger_raised().to_egui()),
            )
            .wrap(),
        );
    }

    ui.add_space(m.footer_gap.value());
    // 행 높이를 버튼 높이로 고정해 측정한 콘텐츠 높이에 버튼 전체가 들어가게 한다.
    let row = egui::vec2(ui.available_width(), m.button.height(theme));
    ui.allocate_ui_with_layout(
        row,
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if Button::new(view.save)
                .variant(ButtonVariant::Primary)
                .size(m.button)
                .enabled(view.save_enabled && !waiting)
                .show(ui, theme)
                .clicked()
            {
                confirm = true;
            }
            if Button::new(view.cancel)
                .variant(ButtonVariant::Ghost)
                .size(m.button)
                .enabled(!waiting)
                .show(ui, theme)
                .clicked()
            {
                cancel = true;
            }
        },
    );

    if confirm {
        return RenamePopupAction::Confirm(buffer.clone());
    }
    if cancel {
        return RenamePopupAction::Cancel;
    }
    RenamePopupAction::None
}

/// 대기 줄: `icon-size-sm` Spinner 와 caption text-muted 문구를 `space-sm` 간격으로 둔다. 문구는
/// 남은 폭에서 줄바꿈한다.
fn waiting_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let icon = theme.icon_glyph_size_sm.value();
    ui.horizontal_top(|ui| {
        ui.set_min_height(icon);
        crate::Spinner::new().size(icon).show(ui, theme);
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            )
            .wrap(),
        );
    });
    ui.ctx().request_repaint();
}

#[cfg(test)]
#[path = "rename_popup_tests.rs"]
mod tests;
