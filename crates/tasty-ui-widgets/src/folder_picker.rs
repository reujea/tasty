//! Shared folder-picker footer: path readout, wrapped hint and the only confirm action.
use crate::{Button, ButtonVariant};
use egui::{Color32, FontId, Rect, Ui};
use std::sync::Arc;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

pub struct FolderPickerFooter<'a> {
    pub label: &'a str,
    pub path: &'a str,
    /// Caller-owned localized template; {name} is painted in monospace.
    pub hint: &'a str,
    pub selected: Option<&'a str>,
    pub confirm: &'a str,
    pub cancel: &'a str,
    pub ready: bool,
}

impl FolderPickerFooter<'_> {
    fn hint_galley(&self, ui: &Ui, theme: &Theme, width: LogicalPx) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        let color = theme.fp_folder_hint_fg().to_egui();
        let line_height = theme.font_size_micro.scaled(theme.line_height_ui);
        let prose = egui::TextFormat {
            line_height: Some(line_height.value()),
            ..egui::TextFormat::simple(FontId::proportional(theme.font_size_micro.value()), color)
        };
        if let Some((before, after)) = self.hint.split_once("{name}") {
            job.append(before, 0.0, prose.clone());
            job.append(
                self.selected.unwrap_or_default(),
                0.0,
                egui::TextFormat {
                    line_height: Some(line_height.value()),
                    ..egui::TextFormat::simple(
                        FontId::monospace(theme.font_size_micro.value()),
                        color,
                    )
                },
            );
            job.append(after, 0.0, prose);
        } else {
            job.append(self.hint, 0.0, prose);
        }
        job.wrap.max_width = (width - theme.icon_glyph_size_sm - theme.spacing_xs)
            .max(LogicalPx(0.0))
            .value();
        job.wrap.break_anywhere = true;
        ui.fonts(|f| f.layout_job(job))
    }

    pub fn height(&self, ui: &Ui, theme: &Theme, width: LogicalPx) -> LogicalPx {
        let mut height = theme.input_height() + theme.fp_section_gap() + theme.button_height();
        if self.ready {
            height = height
                + LogicalPx(self.hint_galley(ui, theme, width).size().y)
                    .max(theme.icon_glyph_size_sm)
                + theme.fp_section_gap();
        }
        height
    }

    /// Returns (confirmed, cancelled); drawing never changes the path.
    pub fn show(
        &self,
        ui: &mut Ui,
        theme: &Theme,
        folder: &dyn Fn(&mut Ui, Rect, Color32),
    ) -> (bool, bool) {
        let width = LogicalPx(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        self.readout(ui, theme, width);
        if self.ready {
            ui.add_space(theme.fp_section_gap().value());
            let galley = self.hint_galley(ui, theme, width);
            let height = LogicalPx(galley.size().y).max(theme.icon_glyph_size_sm);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(width.value(), height.value()),
                egui::Sense::hover(),
            );
            let glyph = Rect::from_min_size(
                rect.min,
                egui::Vec2::splat(theme.icon_glyph_size_sm.value()),
            );
            folder(ui, glyph, theme.fp_folder_hint_fg().to_egui());
            ui.painter().galley(
                rect.min + egui::vec2((theme.icon_glyph_size_sm + theme.spacing_xs).value(), 0.0),
                galley,
                theme.fp_folder_hint_fg().to_egui(),
            );
        }
        ui.add_space(theme.fp_section_gap().value());
        ui.allocate_ui_with_layout(
            egui::vec2(width.value(), theme.button_height().value()),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                let confirm = Button::new(self.confirm)
                    .variant(ButtonVariant::Primary)
                    .enabled(self.ready)
                    .show(ui, theme)
                    .clicked();
                let cancel = Button::new(self.cancel)
                    .variant(ButtonVariant::Ghost)
                    .show(ui, theme)
                    .clicked();
                (confirm, cancel)
            },
        )
        .inner
    }

    fn readout(&self, ui: &mut Ui, theme: &Theme, width: LogicalPx) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(width.value(), theme.input_height().value()),
            egui::Sense::hover(),
        );
        let label_width = theme.fp_footer_label_width();
        let label_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.left() + label_width.value(), rect.bottom()),
        );
        let mut label = egui::text::LayoutJob::simple_singleline(
            self.label.to_owned(),
            FontId::proportional(theme.font_size_caption.value()),
            theme.text_muted().to_egui(),
        );
        label.wrap = egui::text::TextWrapping::truncate_at_width(label_width.value());
        let label = ui.fonts(|f| f.layout_job(label));
        ui.painter().with_clip_rect(label_rect).galley(
            egui::pos2(
                label_rect.left(),
                label_rect.center().y - label.size().y * 0.5,
            ),
            label,
            theme.text_muted().to_egui(),
        );
        let field = Rect::from_min_max(
            egui::pos2(
                rect.left() + (label_width + theme.fp_section_gap()).value(),
                rect.top(),
            ),
            rect.max,
        );
        ui.painter().rect(
            field,
            theme.corner_radius.value(),
            theme.input_readonly_bg().to_egui(),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.input_readonly_border().to_egui(),
            ),
            egui::StrokeKind::Inside,
        );
        let inner = field.shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
        let font = FontId::monospace(theme.font_size_caption.value());
        let color = if self.ready {
            theme.fp_folder_path_fg()
        } else {
            theme.text_muted()
        }
        .to_egui();
        let path = elide_front(ui, self.path, &font, LogicalPx(inner.width()));
        ui.painter().with_clip_rect(inner).text(
            inner.left_center(),
            egui::Align2::LEFT_CENTER,
            path,
            font,
            color,
        );
        ui.interact(field, ui.id().with("folder_readout"), egui::Sense::hover())
            .on_hover_text(self.path);
    }
}

/// Measure UTF-8 suffixes instead of estimating the character count from a pixel width.
fn elide_front(ui: &Ui, text: &str, font: &FontId, width: LogicalPx) -> String {
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
            <= width.value()
    };
    if fits(text) {
        return text.to_owned();
    }
    let offsets: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let (mut lo, mut hi) = (0, offsets.len() - 1);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if fits(&format!("…{}", &text[offsets[mid]..])) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    format!("…{}", &text[offsets[lo]..])
}

#[cfg(test)]
mod tests;
