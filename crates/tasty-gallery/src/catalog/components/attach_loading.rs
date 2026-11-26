//! Shared attach loading parts from the design loading gallery.

mod matrix;
#[cfg(test)]
mod move_glyph_tests;
mod surface;

use crate::catalog::spec::{self, StageVariant, stage};
use crate::catalog::toast_card::{self, ToastKind};
use crate::i18n::{t, t_fmt, t_lang};
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
    copy_wait(ui, theme);
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

/// Languages of the design copy-wait table (attach_copy_wait_copy.js).
const COPY_WAIT_LANGS: [&str; 4] = ["en", "ko", "ja", "zh-Hans"];

/// `key` in `lang` with its first `{}` replaced by `arg`.
fn lang_fmt(lang: &str, key: &'static str, arg: &str) -> String {
    tasty_i18n::fill_args(t_lang(lang, key), &[arg])
}

/// Design views pending-copy-only, pending-copy-mixed and pending-copy-result (loading-wait spec).
fn copy_wait(ui: &mut egui::Ui, theme: &Theme) {
    let mut still = theme.clone();
    still.reduced_motion = true;
    spec::spec(ui, theme, "Clipboard copy wait", None);
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for lang in COPY_WAIT_LANGS {
            let label = t_lang(lang, "attach.pending.copy");
            let time = lang_fmt(lang, "attach.pending.seconds", "2");
            PendingIndicator {
                label,
                time: &time,
                compact: false,
                details: &format!("{label} · {time}"),
                open: false,
            }
            .show(ui, &still);
        }
        let label = t_lang("en", "attach.pending.copy");
        let time = lang_fmt("en", "attach.pending.seconds", "2");
        // The compact row spans the column (its widest chip) and centres the chip, so the
        // bubble stays on the card.
        let column = egui::vec2(ui.min_rect().width(), 0.0);
        ui.allocate_ui_with_layout(column, egui::Layout::top_down(egui::Align::Center), |ui| {
            open_detail(
                ui,
                &still,
                &PendingIndicator {
                    label,
                    time: &time,
                    compact: true,
                    details: &format!("{label} · {time}"),
                    open: true,
                },
            );
        });
    });
    stage(ui, theme, StageVariant::Column, |ui| {
        let oldest = lang_fmt("en", "attach.pending.seconds", "20");
        let copy = lang_fmt("en", "attach.pending.seconds", "3");
        open_detail(
            ui,
            &still,
            &PendingIndicator {
                label: &lang_fmt("en", "attach.pending.many", "2"),
                time: &oldest,
                compact: false,
                details: &format!(
                    "{} · {oldest}\n{} · {copy}",
                    t_lang("en", "attach.pending.close_tab"),
                    t_lang("en", "attach.pending.copy"),
                ),
                open: true,
            },
        );
    });
    stage(ui, theme, StageVariant::Column, |ui| {
        copy_wait_in_window(ui, &still);
    });
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for (kind, key) in [
            (ToastKind::Info, "toast.copied_osc52"),
            (ToastKind::Error, "toast.clipboard_failed"),
        ] {
            for lang in COPY_WAIT_LANGS {
                toast_card::draw_single_card(ui, theme, kind, t_lang(lang, key), &[], 1.0);
            }
        }
    });
    let show = format!(
        "at once, no delay or minimum time · response waits keep the {}ms delay",
        theme.pending_indicator_delay().0
    );
    spec::meta(
        ui,
        theme,
        &[
            (
                "label",
                "attach.pending.copy · mixed with other waits: attach.pending.one / many",
            ),
            ("show", &show),
            (
                "tooltip",
                "detail lines in the design Tooltip below the chip, one per kind",
            ),
            (
                "input",
                "not a pointer target · clicks, drags and the wheel reach the terminal",
            ),
            (
                "result",
                "toast.copied_osc52 info · toast.clipboard_failed danger · once per write",
            ),
        ],
        &[],
    );
}

/// The chip with its detail Tooltip drawn open below it, with room left for the bubble.
fn open_detail(ui: &mut egui::Ui, theme: &Theme, chip: &PendingIndicator<'_>) {
    chip.show(ui, theme);
    let bubble = LogicalPx(chip.detail_tooltip().bubble_size(ui.ctx(), theme).y);
    ui.add_space((theme.spacing_xs + bubble).value());
}

/// Design view pending-copy-in-window: the copy chip in the notice column of a window frame.
fn copy_wait_in_window(ui: &mut egui::Ui, theme: &Theme) {
    let label = t_lang("en", "attach.pending.copy");
    let time = lang_fmt("en", "attach.pending.seconds", "2");
    surface::notice_window(
        ui,
        theme,
        [FRAME_WIDTH, FRAME_HEIGHT],
        &PendingIndicator {
            label,
            time: &time,
            compact: false,
            details: &format!("{label} · {time}"),
            open: false,
        },
    );
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
