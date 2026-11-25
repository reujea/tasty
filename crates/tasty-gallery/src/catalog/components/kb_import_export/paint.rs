//! 단축키 가져오기·내보내기 예제의 텍스트·아이콘 그리기 헬퍼.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::icons::MockGlyph;

/// 안내문 — 12 muted, line-height ui, max-width `measure`.
pub(super) fn intro(ui: &mut egui::Ui, theme: &Theme, measure: LogicalPx, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(measure.value());
        ui.label(
            egui::RichText::new(text)
                .size(theme.font_size_term_sm.value())
                .color(theme.text_muted().to_egui()),
        );
    });
}

pub(super) fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}

pub(super) fn glyph_at(ui: &mut egui::Ui, glyph: MockGlyph, size: LogicalPx, tint: egui::Color32) {
    let s = size.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
    glyph.image(s, tint).paint_at(ui, rect);
}

pub(super) fn truncated(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    truncated_tracked(ui, text, font, color, max_width, LogicalPx(0.0))
}

/// [`truncated`] 에 자간을 더한다. 대문자 mono 머리글의 `letter-spacing-caps` 용.
pub(super) fn truncated_tracked(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_width: f32,
    tracking: LogicalPx,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    for section in &mut job.sections {
        section.format.extra_letter_spacing = tracking.value();
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.fonts(|f| f.layout_job(job))
}
