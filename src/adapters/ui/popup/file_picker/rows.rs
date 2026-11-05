//! Picker rows keep disabled file ink separate from opacity and hit testing.
use super::*;

/// 목록 한 행. 클릭 의도가 있으면 돌려준다.
pub(super) fn entry_row(
    ui: &mut egui::Ui,
    props: &FilePickerProps<'_>,
    entry: &FilePickerEntryView,
) -> Option<FilePickerAction> {
    let th = props.theme;
    let inert = matches!(props.mode, FilePickerMode::Folder { .. }) && !entry.is_dir;
    let selected = !inert && props.selected.iter().any(|s| s == &entry.name);
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_height(th).value()),
        if inert {
            egui::Sense::hover()
        } else {
            egui::Sense::click()
        },
    );
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, th.surface_active().to_egui());
        // 한쪽 선택 bar — kit FpRow `inset selection-edge-width 0 0 accent-primary`.
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(th.selection_edge_width.value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, th.accent_primary().to_egui());
    } else if resp.hovered() && !inert {
        ui.painter()
            .rect_filled(rect, 0.0, th.hover_overlay.to_egui_premultiplied());
    }
    if resp.hovered() && !inert {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let (glyph, mut glyph_color) = match (entry.is_dir, selected) {
        (true, true) => (icons::FOLDER, th.text_on_active()),
        (true, false) => (icons::FOLDER, th.accent_primary()),
        (false, true) => (icons::FILE, th.text_on_active()),
        (false, false) => (icons::FILE, th.text_muted()),
    };
    let c = cols(rect, th);
    let glyph_size = th.icon_glyph_size_md.value();
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(c.icon_left.value(), rect.center().y - glyph_size * 0.5),
        egui::vec2(glyph_size, glyph_size),
    );
    if inert {
        glyph_color = th.fp_folder_file_fg();
    }
    glyph
        .image(glyph_size, glyph_color.into())
        .paint_at(ui, icon_rect);
    let Cols {
        name_left,
        name_right,
        size_right,
        modified_right,
        ..
    } = c;
    let name_color = if inert {
        th.fp_folder_file_fg().to_egui()
    } else if selected {
        th.text_on_active().to_egui()
    } else {
        th.text_secondary().to_egui()
    };
    let mut job = egui::text::LayoutJob::simple_singleline(
        entry.name.clone(),
        egui::FontId::proportional(th.font_size_body.value()),
        name_color,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(
        (name_right - name_left).max(LogicalPx(0.0)).value(),
    );
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(
        egui::pos2(name_left.value(), rect.center().y - galley.size().y * 0.5),
        galley,
        name_color,
    );
    let mono = egui::FontId::monospace(th.font_size_caption.value());
    let meta_fg = if inert {
        th.fp_folder_file_fg()
    } else if selected {
        th.text_on_active()
    } else {
        th.text_muted()
    };
    ui.painter().text(
        egui::pos2(size_right.value(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        &entry.size_display,
        mono.clone(),
        meta_fg.into(),
    );
    ui.painter().text(
        egui::pos2(modified_right.value(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        &entry.modified_display,
        mono,
        meta_fg.into(),
    );
    if inert {
        if let FilePickerMode::Folder { file_tooltip, .. } = props.mode {
            resp.on_hover_text(file_tooltip);
        }
        return None;
    }
    if resp.double_clicked() {
        Some(if entry.is_dir {
            FilePickerAction::NavigateInto(entry.name.clone())
        } else {
            FilePickerAction::ConfirmEntry(entry.name.clone())
        })
    } else if resp.clicked() {
        Some(FilePickerAction::Select(entry.name.clone()))
    } else {
        None
    }
}
