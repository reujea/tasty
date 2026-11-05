//! New-workspace row states and their specimen strips.

use super::rows::{dot_slot, selected_bar, ws_row};
use super::{CREATE_ERROR, NewRow, STRIP_W, WORKSPACES, WS_ROW_H};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, Spinner};

/// "+ New workspace" 행 5상태 — 440px pane 폭 스트립(디자인 specimen 과 동일 폭).
pub fn draw_new_row(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (label, state) in [
            ("rest", NewRow::Rest),
            ("hover", NewRow::Hover),
            ("selected", NewRow::Selected),
            ("creating", NewRow::Creating),
            ("failed", NewRow::Failed),
        ] {
            spec::cluster(ui, theme, label, |ui| {
                new_row_strip(ui, theme, state);
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("box", "34px — the same row as a remote workspace"),
            (
                "glyph",
                "plus 14px centered in a status-dot-width slot (overflows both sides)",
            ),
            ("label", "13px / 500 — reads as a peer of the rows below"),
            (
                "rest · hover",
                "accent-primary label on panel / overlay-hover",
            ),
            (
                "selected",
                "text-primary on surface-active + 2px accent bar",
            ),
            (
                "creating",
                "Spinner + muted 'Creating workspace…'; list names in text-muted, no opacity",
            ),
            ("failed", "danger warn glyph + inline reason + Try again"),
            ("group", "1px separator below, xs margin above and below"),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "rest/hover glyph + label",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "overlay-hover",
                "hover fill",
                theme.overlay_hover().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "surface-active",
                "selected fill",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "failed glyph + reason",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new(
                "text-muted",
                "'on remote' caption / creating label",
                theme.text_muted().to_egui(),
            ),
        ],
    );

    spec::do_(
        ui,
        theme,
        "Keep the glyph inside a status-dot-width slot. The dot the workspace rows draw is \
         8px and the plus is 14px, so the plus overflows its slot symmetrically and the name \
         column starts on exactly the same pixel as every row beneath it.",
    );

    spec::note(
        ui,
        theme,
        "Selected drops the accent label for text-primary. Accent on the active surface \
         measures 3.17:1 — the row would be least readable at the moment it is chosen. The \
         row still reads as the odd one out through the glyph, the separator, and the accent \
         bar, so nothing is lost by letting the label go quiet.",
    );

    spec::note(
        ui,
        theme,
        "During creation the list stays visible but disabled. A failure appears below the new-workspace row so an existing workspace can still be chosen afterward. Long error text is limited to three lines, with the full text in a tooltip.",
    );
}

/// 새 행과 일반 워크스페이스 행의 정렬을 비교한다. 팝업 내부 목록이므로 그림자는 없다.
fn new_row_strip(ui: &mut egui::Ui, theme: &Theme, state: NewRow) {
    let peek = &WORKSPACES[0];
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.set_width(STRIP_W.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                ui.set_width(STRIP_W.value());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                new_ws_row(ui, theme, state);
                ws_row(ui, theme, peek, false, state.creating());
            });
        });
}

/// 새 워크스페이스 행은 아이콘·라벨·구분선으로 일반 행과 구별한다.
pub(super) fn new_ws_row(ui: &mut egui::Ui, theme: &Theme, state: NewRow) {
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, WS_ROW_H.value()), egui::Sense::hover());
    if state.selected() {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        selected_bar(ui, theme, rect);
    } else if state == NewRow::Hover {
        ui.painter()
            .rect_filled(rect, 0.0, theme.overlay_hover().to_egui_premultiplied());
    }
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + theme.spacing_md.value(), rect.top()),
        egui::pos2(rect.right() - theme.spacing_md.value(), rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let glyph_c = if state.selected() {
        if state.creating() {
            theme.text_on_active()
        } else if state.failed() {
            theme.accent_danger_on_active()
        } else {
            theme.accent_primary_on_active()
        }
    } else if state.creating() {
        theme.text_muted()
    } else if state.failed() {
        theme.accent_danger()
    } else {
        theme.accent_primary()
    };
    dot_slot_glyph(&mut child, theme, state, glyph_c.to_egui());
    // 선택 배경 위에서 읽기 쉽도록 라벨을 text-on-active로 바꾼다.
    let label_c = if state.selected() {
        theme.text_on_active()
    } else if state.creating() {
        theme.text_muted()
    } else {
        theme.accent_primary_raised()
    };
    child.label(
        egui::RichText::new(if state.creating() {
            "Creating workspace…"
        } else {
            "New workspace"
        })
        .size(theme.font_size_body.value())
        .strong()
        .color(label_c.to_egui()),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !state.creating() && !state.failed() {
            ui.label(
                egui::RichText::new("on remote")
                    .size(theme.font_size_caption.value())
                    .color(
                        if state.selected() {
                            theme.text_on_active()
                        } else {
                            theme.text_muted()
                        }
                        .to_egui(),
                    ),
            );
        }
    });
    if state.failed() {
        new_ws_error(ui, theme);
    }
    row_separator(ui, theme);
}

/// 새 행의 글리프 — 슬롯 중심에 놓인 14px `plus`(실패 시 `alertTriangle`,
/// 생성 중이면 Spinner).
fn dot_slot_glyph(ui: &mut egui::Ui, theme: &Theme, state: NewRow, color: egui::Color32) {
    let size = theme.icon_glyph_size_sm.value();
    let g = egui::Rect::from_center_size(dot_slot(ui, theme).center(), egui::vec2(size, size));
    if state.creating() {
        let mut c = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(g)
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        Spinner::new().size(size).color(color).show(&mut c, theme);
    } else {
        let glyph = if state.failed() {
            icons::ALERT_TRIANGLE
        } else {
            icons::PLUS
        };
        glyph.image(size, color).paint_at(ui, g);
    }
}

/// 생성 실패는 목록을 가리지 않고 행 아래에 표시한다.
fn new_ws_error(ui: &mut egui::Ui, theme: &Theme) {
    let width = ui.available_width();
    let cap_h = theme.font_size_caption.value() * theme.line_height_ui;
    let btn_h = ControlSize::Sm.height(theme);
    let h = theme.spacing_xs.value() * 2.0 + cap_h + btn_h + theme.spacing_sm.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left()
                + theme.spacing_md.value()
                + theme.status_dot_size().value()
                + theme.spacing_sm.value(),
            rect.top() + theme.spacing_xs.value(),
        ),
        egui::pos2(
            rect.right() - theme.spacing_md.value(),
            rect.bottom() - theme.spacing_sm.value(),
        ),
    );
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    col.spacing_mut().item_spacing.y = theme.spacing_xs.value();
    col.label(
        egui::RichText::new(CREATE_ERROR)
            .size(theme.font_size_caption.value())
            .color(theme.accent_danger_raised().to_egui()),
    );
    Button::new("Try again")
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Sm)
        .leading_icon(&|ui, rect, c| icons::REFRESH.image(rect.height(), c).paint_at(ui, rect))
        .show(&mut col, theme);
}

/// 행 아래 1px 구분선 + 위/아래 xs 마진.
fn row_separator(ui: &mut egui::Ui, theme: &Theme) {
    let width = ui.available_width();
    let m = theme.spacing_xs.value();
    let t = theme.border_width.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, m * 2.0 + t), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.top() + m + t * 0.5,
        egui::Stroke::new(t, theme.separator.to_egui_premultiplied()),
    );
}
