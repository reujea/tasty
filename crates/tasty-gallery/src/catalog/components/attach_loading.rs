//! Shared attach loading parts from the design loading gallery.

mod matrix;
mod surface;

use crate::catalog::spec::{self, StageVariant, stage};
use crate::i18n::{t, t_fmt};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{LoadingPlacement, PendingIndicator, paint_loading_overlay};

/// 시안 Loading overlay의 일곱 예제와 응답 대기 표시.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    matrix::placements(ui, theme);
    matrix::tabs(ui, theme);
    matrix::lnb(ui, theme);
    matrix::rail(ui, theme);
    surface::surfaces(ui, theme);
    surface::together(ui, theme);
    matrix::labels(ui, theme);
    pending(ui, theme);
}

/// 응답 대기 표시: 한 건·여러 건·좁은 폭·모션 감소.
fn pending(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(ui, theme, "Response wait indicator", None);
    stage(ui, theme, StageVariant::Column, |ui| {
        PendingIndicator {
            label: t("attach.pending.one"),
            time: "0s",
            compact: false,
            details: &format!("{} · 0s", t("attach.pending.close_tab")),

            open: false,
        }
        .show(ui, theme);
        PendingIndicator {
            label: &t_fmt("attach.pending.many", "3"),
            time: "1m 04s",
            compact: false,
            details: &format!(
                "{} · 1m 04s\n{} · 12s\n{} · 3s",
                t("attach.pending.close_tab"),
                t("attach.pending.move_tab"),
                t("attach.pending.input"),
            ),
            open: false,
        }
        .show(ui, theme);
        PendingIndicator {
            label: &t_fmt("attach.pending.many", "3"),
            time: "1m 04s",
            compact: true,
            details: &format!(
                "{} · 1m 04s\n{} · 12s\n{} · 3s",
                t("attach.pending.close_tab"),
                t("attach.pending.move_tab"),
                t("attach.pending.input"),
            ),
            open: false,
        }
        .show(ui, theme);
        let mut still = theme.clone();
        still.reduced_motion = true;
        PendingIndicator {
            label: t("attach.pending.one"),
            time: "1h 02m",
            compact: false,
            details: &format!("{} · 1h 02m", t("attach.pending.close_tab")),

            open: false,
        }
        .show(ui, &still);
    });
}

// Design storyboard frames (layouts-loading-prelayout.jsx W=560/H=180), not product dimensions.
const FRAME_WIDTH: tasty_type_geometry::length::LogicalPx =
    tasty_type_geometry::length::LogicalPx(560.0);
const FRAME_HEIGHT: tasty_type_geometry::length::LogicalPx =
    tasty_type_geometry::length::LogicalPx(180.0);

pub fn draw_prelayout(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        for (name, kind) in [
            ("New tab", 0),
            ("Surface split · columns", 1),
            ("Surface split · rows", 2),
            ("Pane split · columns", 3),
            ("Pane split · rows", 4),
        ] {
            ui.label(name);
            for loading in [false, true] {
                prelayout(ui, theme, kind, loading);
            }
        }
    });
}
fn prelayout(ui: &mut egui::Ui, theme: &Theme, kind: usize, loading: bool) {
    let (frame, _) = ui.allocate_exact_size(
        egui::vec2(
            FRAME_WIDTH.min(LogicalPx(ui.available_width())).value(),
            FRAME_HEIGHT.value(),
        ),
        egui::Sense::hover(),
    );
    let border = theme.border_width;
    ui.painter()
        .rect_filled(frame, 0.0, theme.separator.to_egui_premultiplied());
    let horizontal = kind == 2 || kind == 4;
    let split = |rect: egui::Rect| {
        if horizontal {
            let y = LogicalPx(rect.top()) + (LogicalPx(rect.height()) - border) * 0.5;
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), y.value())),
                egui::Rect::from_min_max(egui::pos2(rect.left(), (y + border).value()), rect.max),
            )
        } else {
            let x = LogicalPx(rect.left()) + (LogicalPx(rect.width()) - border) * 0.5;
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(x.value(), rect.bottom())),
                egui::Rect::from_min_max(egui::pos2((x + border).value(), rect.top()), rect.max),
            )
        }
    };
    if kind >= 3 {
        let (old, new) = split(frame);
        let old_content = preview_tabs(ui, theme, old, false, false);
        preview_content(ui, theme, old_content, false, false);
        let new_content = preview_tabs(ui, theme, new, true, loading);
        preview_content(ui, theme, new_content, true, loading);
    } else {
        let content = preview_tabs(ui, theme, frame, kind == 0, loading);
        if kind == 0 {
            let (old_header, new_header) = split(frame);
            preview_tabs(ui, theme, old_header, false, false);
            preview_tabs(ui, theme, new_header, true, loading);
            preview_content(ui, theme, content, true, loading);
        } else {
            let (old, new) = split(content);
            preview_content(ui, theme, old, false, false);
            preview_content(ui, theme, new, true, loading);
        }
    }
}
fn preview_tabs(
    ui: &mut egui::Ui,
    theme: &Theme,
    frame: egui::Rect,
    new: bool,
    loading: bool,
) -> egui::Rect {
    let header = egui::Rect::from_min_size(
        frame.min,
        egui::vec2(frame.width(), theme.tab_bar_height.value()),
    );
    ui.painter()
        .rect_filled(header, 0.0, theme.surface_raised());
    let inset = theme.spacing_sm;
    let slot = egui::Rect::from_min_size(
        header.min
            + egui::vec2(
                inset.value(),
                ((LogicalPx(header.height()) - theme.loading_overlay_tab_size()) * 0.5).value(),
            ),
        egui::Vec2::splat(theme.loading_overlay_tab_size().value()),
    );
    tasty_icons::TERMINAL
        .image(slot.width(), theme.text_primary().to_egui())
        .paint_at(ui, slot);
    if loading {
        paint_loading_overlay(
            ui,
            theme,
            slot,
            LoadingPlacement::Tab,
            theme.surface_raised().to_egui(),
        );
    }
    ui.painter().text(
        egui::pos2(
            (LogicalPx(slot.right()) + theme.spacing_xs).value(),
            header.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        if new {
            t("attach.pending.new_tab")
        } else {
            "server"
        },
        egui::FontId::proportional(theme.tab_bar_label_font_size.value()),
        theme.text_primary().to_egui(),
    );
    egui::Rect::from_min_max(egui::pos2(frame.left(), header.bottom()), frame.max)
}
fn preview_content(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, blank: bool, loading: bool) {
    ui.painter().rect_filled(rect, 0.0, theme.bg_panel());
    if !blank {
        ui.painter().text(
            rect.min + egui::Vec2::splat(theme.spacing_sm.value()),
            egui::Align2::LEFT_TOP,
            "$ build",
            egui::FontId::monospace(theme.font_size_caption.value()),
            theme.text_secondary().to_egui(),
        );
    }
    if loading {
        paint_loading_overlay(
            ui,
            theme,
            rect,
            LoadingPlacement::Surface,
            theme.bg_panel().to_egui(),
        );
    }
}
