//! View-only anomaly settings layout. Callers own translations, drafts and numeric commit policy.
use tasty_type_appearance::theme::Theme;

use crate::{SettingsRow, settings_apply_note, settings_label_column};

pub struct AnomalySection<'a> {
    pub title: &'a str,
    pub caption: &'a str,
    pub detector: &'a str,
    pub rows: [SettingsRow<'a>; 3],
}

/// Render the shared notice, three sections and nine responsive rows.
/// Controls stay editable independently of notification switches.
pub fn settings_anomaly(
    ui: &mut egui::Ui,
    theme: &Theme,
    notice: &str,
    detector_label: &str,
    sections: &[AnomalySection<'_>; 3],
    mut control: impl FnMut(&mut egui::Ui, usize, usize),
) {
    let col = settings_label_column(ui, theme, sections.iter().flat_map(|s| &s.rows));
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xl.value();
        settings_apply_note(ui, theme, notice);
        for (index, section) in sections.iter().enumerate() {
            ui.push_id(section.detector, |ui| {
                ui.spacing_mut().item_spacing.y = theme.settings_row_gap().value();
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(section.title.to_uppercase())
                            .family(crate::label_font_family())
                            .size(theme.font_size_micro.value())
                            .color(theme.text_muted().to_egui()),
                    )
                    .wrap(),
                );
                for (row_index, row) in section.rows.iter().enumerate() {
                    if row_index > 0 {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), theme.border_width.value()),
                            egui::Sense::hover(),
                        );
                        ui.painter().hline(
                            rect.x_range(),
                            rect.center().y,
                            egui::Stroke::new(
                                theme.border_width.value(),
                                theme.separator.to_egui_premultiplied(),
                            ),
                        );
                    }
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = theme.settings_row_caption_gap().value();
                        row.show(ui, theme, col, |ui| control(ui, index, row_index));
                        if row_index == 0 {
                            let mut job = egui::text::LayoutJob::default();
                            let caption = egui::TextFormat {
                                font_id: egui::FontId::proportional(
                                    theme.font_size_caption.value(),
                                ),
                                color: theme.text_muted().to_egui(),
                                ..Default::default()
                            };
                            job.append(
                                &format!("{} {}: ", section.caption, detector_label),
                                0.0,
                                caption.clone(),
                            );
                            job.append(
                                section.detector,
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::monospace(
                                        theme.font_size_caption.value(),
                                    ),
                                    color: theme.text_secondary().to_egui(),
                                    ..caption.clone()
                                },
                            );
                            job.append(".", 0.0, caption);
                            // Label::wrap resolves width from its Ui, replacing the job's wrap width.
                            // Constrain the actual layout region, as settings_row_caption does.
                            ui.scope(|ui| {
                                ui.set_max_width(
                                    theme.measure_md.value().min(ui.available_width()),
                                );
                                ui.add(egui::Label::new(job).wrap());
                            });
                        }
                    });
                }
            });
        }
    });
}

#[cfg(test)]
mod tests;
