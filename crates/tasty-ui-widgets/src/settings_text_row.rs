//! 설정 행 중 읽기 전용 글 — plugin 이 쓰는 지시문 같은 긴 값을 고칠 수 없게 보여 준다.
//!
//! 글은 컨트롤 열 폭에서 줄을 바꾸고, 여러 줄이 되므로 라벨을 위에 맞춘다. 선택·복사는 되고 편집 칸이
//! 아니므로 테두리·배경이 없다. 색은 행 라벨과 같은 `text-secondary`, 크기는 `font-size-body` 다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::settings_row_stack::{StackRow, settings_stack_row};

/// 라벨과 읽기 전용 글 한 행을 그린다. 앞 여백은 호출자의 행 규칙을 따른다.
pub fn settings_text_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label_col: LogicalPx,
    label: &str,
    text: &str,
) -> egui::Response {
    let row = StackRow {
        id: ui.id().with(("settings_text_row", label)),
        label_col,
        row_h: theme.settings_row_min_height(),
        label,
        hint: None,
        align_top: true,
    };
    settings_stack_row(ui, theme, row, |ui| {
        ui.set_max_width(ui.available_width());
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(theme.font_size_body.value())
                    .color(theme.text_secondary().to_egui()),
            )
            .wrap_mode(egui::TextWrapMode::Wrap)
            .selectable(true),
        );
    })
}
