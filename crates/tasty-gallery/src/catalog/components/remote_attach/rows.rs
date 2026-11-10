//! Profile/workspace rows and their shared selection and dot-slot geometry.

use super::{PROFILE_ROW_H, Prof, WS_ROW_H, Ws};
use crate::catalog::icons;
use crate::catalog::widgets::dialog as kit;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::tokens::STRUCT_GAP_2;
use tasty_ui_widgets::{StatusKind, TagVariant, status_dot, tag, tag_with_icon};

pub(super) fn profile_row(ui: &mut egui::Ui, theme: &Theme, p: &Prof, selected: bool) {
    let w = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w, PROFILE_ROW_H.value()), egui::Sense::hover());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        selected_bar(ui, theme, rect);
    }
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + theme.spacing_md.value(),
            rect.top() + theme.spacing_sm.value(),
        ),
        egui::pos2(
            rect.right() - theme.spacing_md.value(),
            rect.bottom() - theme.spacing_sm.value(),
        ),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
    let meta_c = if selected {
        theme.text_on_active()
    } else {
        theme.text_muted()
    }
    .to_egui();
    child.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let name_c = if selected {
            theme.text_on_active()
        } else {
            theme.text_secondary()
        };
        ui.label(
            egui::RichText::new(p.name)
                .size(theme.font_size_body.value())
                .strong()
                .color(name_c.to_egui()),
        );
        if !p.label.is_empty() {
            ui.label(
                egui::RichText::new(format!("({})", p.label))
                    .size(theme.font_size_body.value())
                    .color(meta_c),
            );
        }
        if p.inactive {
            tag_with_icon(
                ui,
                theme,
                "inactive",
                TagVariant::Warning,
                icons::ALERT_TRIANGLE,
            );
        }
    });
    child.label(
        egui::RichText::new(p.target)
            .monospace()
            .size(theme.font_size_caption.value())
            .color(meta_c),
    );
}

/// 연결됐지만 워크스페이스가 없을 때의 안내. 이름 열을 다른 행과 맞춘다.
pub(super) fn empty_line(ui: &mut egui::Ui, theme: &Theme, profile: &str) {
    let width = ui.available_width();
    let h = theme.spacing_xs.value() * 2.0 + theme.font_size_caption.value() * theme.line_height_ui;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let x = rect.left()
        + theme.spacing_md.value()
        + theme.status_dot_size().value()
        + theme.spacing_sm.value();
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{profile} is reachable but has no workspaces yet."),
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
}

/// 모든 행에 같은 점 슬롯을 확보한다. 더 큰 새 행 아이콘은 슬롯 중심에 놓는다.
pub(super) fn dot_slot(ui: &mut egui::Ui, theme: &Theme) -> egui::Rect {
    let (slot, _) = ui.allocate_exact_size(
        egui::vec2(
            theme.status_dot_size().value(),
            theme.icon_glyph_size_sm.value(),
        ),
        egui::Sense::hover(),
    );
    slot
}

/// StatusDot이 추가하는 라벨 간격이 열 정렬을 밀지 않도록 고정 슬롯 안에 그린다.
fn dot_slot_status(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: StatusKind,
    pulse: bool,
    selected: bool,
) {
    let slot = dot_slot(ui, theme);
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(slot)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ground = if selected {
        tasty_ui_widgets::Ground::Active
    } else {
        tasty_ui_widgets::Ground::Panel
    };
    status_dot(&mut c, theme, kind, "", pulse, false, ground);
}

/// 선택 행의 inset accent 좌측바. 한쪽 변 선택 띠라 hairline selection_edge_width를 쓴다.
pub(super) fn selected_bar(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let bar = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(theme.selection_edge_width.value(), rect.height()),
    );
    ui.painter()
        .rect_filled(bar, 0.0, theme.accent_primary().to_egui());
}

pub(super) fn ws_row(ui: &mut egui::Ui, theme: &Theme, w: &Ws, selected: bool, muted: bool) {
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, WS_ROW_H.value()), egui::Sense::hover());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        selected_bar(ui, theme, rect);
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
    let busy = w.busy && !w.chain;
    let kind = if busy {
        StatusKind::Running
    } else {
        StatusKind::Idle
    };
    dot_slot_status(&mut child, theme, kind, busy, selected);
    let name_c = if w.attached || w.chain || muted {
        theme.text_muted()
    } else if selected {
        theme.text_on_active()
    } else {
        theme.text_secondary()
    };
    let meta_c = if selected {
        theme.text_on_active()
    } else {
        theme.text_muted()
    }
    .to_egui();
    child.label(
        egui::RichText::new(w.name)
            .size(theme.font_size_body.value())
            .color(name_c.to_egui()),
    );
    kit::icon(&mut child, icons::SPLIT, theme.font_size_caption, meta_c);
    child.label(
        egui::RichText::new(w.panes.to_string())
            .size(theme.font_size_caption.value())
            .color(meta_c),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if w.chain {
            tag_with_icon(
                ui,
                theme,
                crate::i18n::t("remote_attach.chain_badge"),
                TagVariant::Default,
                icons::REMOTE,
            )
            .on_hover_text(crate::i18n::t_fmt("remote.row.reason.chain", w.name));
        } else if w.attached {
            tag(ui, theme, "in use", TagVariant::Attached, false);
        } else if w.busy {
            ui.label(
                egui::RichText::new("busy")
                    .size(theme.font_size_caption.value())
                    .color(meta_c),
            );
        }
    });
}
