//! 설정 숫자 칸의 공용 조각. 칸 폭은 값에 맞춰 자라고, 단위와 범위 줄은 칸 밖에 둔다.
//! 확정·범위 판단은 호출자가 하고 여기서는 모양만 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 숫자 칸이 쓰는 글꼴. 공용 `Input` 의 mono 글꼴과 같다.
fn mono_caption(theme: &Theme) -> egui::FontId {
    egui::FontId::monospace(theme.font_size_caption.value())
}

/// 칸에 든 글자 `text` 를 모두 담는 폭. 글자 수 + 커서 한 칸을 칸 자신의 mono 글꼴 `0` 너비로 세고
/// 상자 padding·테두리를 더한다. 하한은 `number-field-min-width`, 상한은 남은 폭이다.
/// 칸이 상한에 닿으면 값 전체가 보이지 않을 수 있다.
pub fn number_field_width(ui: &egui::Ui, theme: &Theme, text: &str) -> LogicalPx {
    let cells = text.chars().count().max(1) + 1;
    // 글리프 advance 를 곱하지 않고 실제로 배치한 폭을 잰다. 배치는 글자마다 픽셀로 반올림하므로
    // advance × 글자 수보다 넓어질 수 있다.
    let zeros = ui.painter().layout_no_wrap(
        "0".repeat(cells),
        mono_caption(theme),
        egui::Color32::PLACEHOLDER,
    );
    let fit = LogicalPx(zeros.size().x) + theme.input_padding_x() * 2.0 + theme.border_width * 2.0;
    fit.max(theme.number_field_min_width())
        .min(LogicalPx(ui.available_width()))
}

/// 칸 뒤 단위(`font-label`) 또는 단위를 대신하는 값 설명(UI 글꼴, 예: -1 의 "no limit").
/// 칸과의 간격은 `number-field-unit-gap` 이다.
pub fn number_field_unit(ui: &mut egui::Ui, theme: &Theme, text: &str, unit: bool, enabled: bool) {
    let gap = theme.number_field_unit_gap().value();
    ui.add_space((gap - ui.spacing().item_spacing.x).max(0.0));
    let ink = if enabled {
        theme.number_field_unit_fg()
    } else {
        theme.state_disabled_fg()
    };
    let mut rich = egui::RichText::new(text)
        .size(theme.font_size_caption.value())
        .color(ink.to_egui());
    if unit {
        rich = rich.font(crate::label_font(theme.font_size_caption.value()));
    }
    ui.add(egui::Label::new(rich).extend());
}

/// 칸 아래 범위 줄. `measure-md`(남은 폭이 더 좁으면 그 폭)에서 줄을 바꾼다.
pub fn number_field_note(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(theme.measure_md.value().min(ui.available_width()));
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(theme.font_size_caption.value())
                    .color(theme.number_field_note_fg().to_egui()),
            )
            .wrap(),
        );
    });
}
