//! 설정 Terminal › Frame rate 섹션 본문. 출력·입력 다시 그리기 상한 두 행(HelpHint, Select)과 그
//! 아래 muted 캡션 한 줄이다.
//!
//! 문구와 선택지는 호출하는 쪽이 넘긴다. 이 함수는 번역 키와 모니터 조회를 모른다. 본체 설정 창과
//! 갤러리 견본이 이 함수를 함께 부른다.

use tasty_type_appearance::theme::Theme;

use crate::{SettingsRow, select, settings_label_column, settings_row_caption};

/// 섹션의 행 라벨·도움말·캡션 문구.
pub struct FrameRateCopy<'a> {
    pub output: &'a str,
    pub output_hint: &'a str,
    pub input: &'a str,
    pub input_hint: &'a str,
    pub caption: &'a str,
}

/// 한 행의 Select. `choices`는 (저장 값, 표기) 목록이고 `value`가 그 안의 한 값이어야 선택이 표시된다.
/// 다른 행을 고르면 `value`가 그 행의 값으로 바뀐다.
pub struct FpsField<'a> {
    pub salt: &'a str,
    pub choices: &'a [(i64, String)],
    pub value: &'a mut i64,
}

/// 두 행과 캡션을 그린다. 행 사이 간격은 `settings-row-gap`이다.
pub fn frame_rate_section(
    ui: &mut egui::Ui,
    theme: &Theme,
    copy: &FrameRateCopy,
    output: FpsField,
    input: FpsField,
) {
    ui.spacing_mut().item_spacing.y = theme.settings_row_gap().value();
    let rows = [
        SettingsRow::new(copy.output).hint(copy.output_hint),
        SettingsRow::new(copy.input).hint(copy.input_hint),
    ];
    let col = settings_label_column(ui, theme, &rows);
    let [output_row, input_row] = rows;
    output_row.show(ui, theme, col, |ui| fps_select(ui, theme, output));
    input_row.show(ui, theme, col, |ui| fps_select(ui, theme, input));
    settings_row_caption(ui, theme, copy.caption);
}

fn fps_select(ui: &mut egui::Ui, theme: &Theme, field: FpsField) {
    let mut idx = field
        .choices
        .iter()
        .position(|(v, _)| *v == *field.value)
        .unwrap_or(0);
    let labels: Vec<&str> = field.choices.iter().map(|(_, l)| l.as_str()).collect();
    if select(
        ui,
        theme,
        field.salt,
        &mut idx,
        &labels,
        theme.field_width_lg.value(),
        true,
    ) && let Some((v, _)) = field.choices.get(idx)
    {
        *field.value = *v;
    }
}
