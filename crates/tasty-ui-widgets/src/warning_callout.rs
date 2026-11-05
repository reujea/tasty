//! 아이콘과 줄바꿈 본문을 경고색 상자로 묶는다. 아이콘은 호출자가 전달한다.
//! 배경은 공통 tint_fill_alpha, 테두리는 이 위젯의 비율을 egui 색에 곱한다.

use tasty_type_appearance::theme::Theme;

use crate::icon_button::IconPainter;

/// 경고 테두리의 비율. 배경 채움만 공통 tint_fill_alpha를 사용한다.
const BORDER_MIX: f32 = 0.4;

/// bordered warning callout — leading 경고 삼각 아이콘 + wrapping caption 본문.
///
/// `paint_icon` 은 위젯이 계산한 정사각 `rect` 와 warning 색을 받아 글리프를 그린다
/// (본체: `|ui, rect, c| icons::ALERT_TRIANGLE.image(rect.height(), c).paint_at(ui, rect)`).
pub fn warning_callout(
    ui: &mut egui::Ui,
    theme: &Theme,
    text: &str,
    paint_icon: IconPainter<'_>,
) -> egui::Response {
    let warning = theme.accent_warning().to_egui();
    let bg = warning.gamma_multiply(theme.tint_fill_alpha());
    let border = warning.gamma_multiply(BORDER_MIX);
    let glyph = theme.icon_glyph_size_sm.value();
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(theme.border_width.value(), border))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_md.value() as i8,
            theme.spacing_sm.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(glyph, glyph), egui::Sense::hover());
                paint_icon(ui, rect, theme.accent_warning_glyph().to_egui());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(theme.font_size_caption.value())
                            .color(theme.text_primary().to_egui()),
                    )
                    .wrap(),
                );
            });
        })
        .response
}
