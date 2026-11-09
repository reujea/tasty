//! 단축키 비교 표. 행 선택과 그룹 접힘 상태를 갱신한다.

use std::collections::BTreeSet;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::checkbox;

use crate::adapters::ui::icons;
use crate::i18n::{t, t_fmt2};

use super::model::{Group, RowKey};
use super::paint::{truncated, truncated_tracked};
use super::view_model::{GroupView, RowView, Sub, ViewModel};
use super::{GROUP_CHEVRON_GAP, PLUGIN_DOT_GAP};

/// jsx `IeDiffTable` — 4 열 grid(select · action · current · imported) + 그룹 헤더.
pub(super) fn diff_table(
    ui: &mut egui::Ui,
    th: &Theme,
    vm: &ViewModel,
    changed_only: bool,
    collapsed: &mut BTreeSet<Group>,
    deselected: &mut BTreeSet<RowKey>,
) {
    let w = ui.available_width();
    let select_w = th.kb_ie_select_column_width().value();
    let rest = (w - select_w).max(0.0);
    let action_w = rest * 1.6 / 3.6;
    let value_w = rest / 3.6;
    let x_off = [
        0.0,
        select_w,
        select_w + action_w,
        select_w + action_w + value_w,
    ];
    let col_w = [select_w, action_w, value_w, value_w];
    let pad_x = th.spacing_md.value();
    let pad_y = th.spacing_sm.value();
    let hairline = egui::Stroke::new(
        th.border_width.value(),
        th.separator.to_egui_premultiplied(),
    );

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        // ── 열 헤더 (padding: 0 space-md space-sm) ──
        let head_font = tasty_ui_widgets::label_font(th.font_size_micro.value());
        let head_h = ui.fonts(|f| f.row_height(&head_font)) + pad_y;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, head_h), egui::Sense::hover());
        let headers = [
            String::new(),
            t("settings.keybindings.preset_col_action").to_uppercase(),
            t("settings.keybindings.preset_col_before").to_uppercase(),
            t("settings.keybindings.ie_col_imported").to_uppercase(),
        ];
        let caps = th.letter_spacing_caps(th.font_size_micro);
        for (i, text) in headers.iter().enumerate() {
            let g = truncated_tracked(
                ui,
                text,
                head_font.clone(),
                th.text_muted().to_egui(),
                (col_w[i] - pad_x * 2.0).max(0.0),
                caps,
            );
            ui.painter().galley(
                egui::pos2(rect.left() + x_off[i] + pad_x, rect.top()),
                g,
                egui::Color32::PLACEHOLDER,
            );
        }
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - th.border_width.value() * 0.5,
            hairline,
        );

        for g in &vm.groups {
            let shown: Vec<&RowView> = g
                .rows
                .iter()
                .filter(|r| !changed_only || r.changed)
                .collect();
            let open = !collapsed.contains(&g.group);
            group_header(ui, th, w, g, &shown, open, collapsed, deselected);
            if !open {
                continue;
            }
            for r in shown {
                let row_h = diff_row_height(ui, th, r);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, row_h), egui::Sense::hover());
                // 선택 열 — `checkbox` 는 빈 라벨에도 박스 뒤 gap 을 차지하므로 박스 중심이
                // 열 중심에 오도록 시작점을 잡는다.
                let box_sz = th.checkbox_size().value();
                let sel_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + (col_w[0] - box_sz) * 0.5, rect.top()),
                    egui::vec2(col_w[0], row_h),
                );
                let mut on = !deselected.contains(&r.key);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(sel_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        if checkbox(ui, th, &mut on, "", true, tasty_ui_widgets::Ground::Panel)
                            .changed()
                        {
                            if on {
                                deselected.remove(&r.key);
                            } else {
                                deselected.insert(r.key.clone());
                            }
                        }
                    },
                );
                action_cell(ui, th, rect, x_off[1] + pad_x, col_w[1] - pad_x * 2.0, r);
                let mono = egui::FontId::monospace(th.font_size_term_sm.value());
                value_cell(
                    ui,
                    rect,
                    x_off[2] + pad_x,
                    col_w[2] - pad_x * 2.0,
                    &r.cur,
                    mono.clone(),
                    th.text_muted().to_egui(),
                );
                let fg = if r.changed {
                    th.accent_primary()
                } else {
                    th.text_muted()
                };
                value_cell(
                    ui,
                    rect,
                    x_off[3] + pad_x,
                    col_w[3] - pad_x * 2.0,
                    &r.next,
                    mono,
                    fg.to_egui(),
                );
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - th.border_width.value() * 0.5,
                    hairline,
                );
            }
        }
    });
}

/// 그룹 헤더 — 네 열을 가로지르는 한 행(surface-raised). select-all · chevron · 그룹명 ·
/// `N changed · M total`. 열 헤더를 반복하지 않는다.
// reason: 표시 값(g · shown)과 두 선택 상태(collapsed · deselected)의 가변 빌림이 따로 온다 —
// 묶으면 `diff_table` 의 행 순회와 빌림이 겹친다.
#[allow(clippy::too_many_arguments)]
fn group_header(
    ui: &mut egui::Ui,
    th: &Theme,
    w: f32,
    g: &GroupView,
    shown: &[&RowView],
    open: bool,
    collapsed: &mut BTreeSet<Group>,
    deselected: &mut BTreeSet<RowKey>,
) {
    let h =
        th.checkbox_size().value().max(th.font_size_micro.value()) + th.spacing_sm.value() * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, th.surface_raised().to_egui());
    let inner = rect.shrink2(egui::vec2(th.spacing_md.value(), 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            let mut all = !shown.is_empty() && shown.iter().all(|r| !deselected.contains(&r.key));
            if checkbox(ui, th, &mut all, "", true, tasty_ui_widgets::Ground::Raised).changed() {
                for r in &g.rows {
                    if all {
                        deselected.remove(&r.key);
                    } else {
                        deselected.insert(r.key.clone());
                    }
                }
            }
            // chevron + 그룹명 — 한 버튼(접힘 토글).
            let glyph = th.icon_glyph_size_sm.value();
            let galley = truncated_tracked(
                ui,
                &t(g.group.label_key()).to_uppercase(),
                tasty_ui_widgets::label_font(th.font_size_micro.value()),
                th.text_secondary_raised().to_egui(),
                f32::INFINITY,
                th.letter_spacing_caps(th.font_size_micro),
            );
            let bw = glyph + GROUP_CHEVRON_GAP.value() + galley.rect.width();
            let (br, resp) = ui.allocate_exact_size(egui::vec2(bw, h), egui::Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let chevron = if open {
                icons::CHEVRON_DOWN
            } else {
                icons::CHEVRON_RIGHT
            };
            let cr = egui::Rect::from_min_size(
                egui::pos2(br.left(), br.center().y - glyph * 0.5),
                egui::vec2(glyph, glyph),
            );
            chevron
                .image(glyph, th.text_muted_raised().to_egui())
                .paint_at(ui, cr);
            ui.painter().galley(
                egui::pos2(
                    br.left() + glyph + GROUP_CHEVRON_GAP.value(),
                    br.center().y - galley.rect.height() * 0.5,
                ),
                galley,
                egui::Color32::PLACEHOLDER,
            );
            if resp.clicked() {
                if open {
                    collapsed.insert(g.group);
                } else {
                    collapsed.remove(&g.group);
                }
            }
            let changed = g.rows.iter().filter(|r| r.changed).count();
            ui.label(
                egui::RichText::new(t_fmt2(
                    "settings.keybindings.ie_group_counts",
                    &changed.to_string(),
                    &g.rows.len().to_string(),
                ))
                .family(tasty_ui_widgets::label_font_family())
                .size(th.font_size_caption.value())
                .color(th.text_muted_raised()),
            );
        },
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - th.border_width.value() * 0.5,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

fn diff_row_height(ui: &egui::Ui, th: &Theme, r: &RowView) -> f32 {
    let body = ui.fonts(|f| f.row_height(&egui::FontId::proportional(th.font_size_body.value())));
    let sub = if r.sub.is_some() {
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(th.font_size_micro.value())))
            + tasty_ui_widgets::tokens::STRUCT_GAP_2.value()
    } else {
        0.0
    };
    body + sub + th.spacing_sm.value() * 2.0
}

fn action_cell(ui: &egui::Ui, th: &Theme, rect: egui::Rect, x: f32, max_w: f32, r: &RowView) {
    let body_font = egui::FontId::proportional(th.font_size_body.value());
    let title = truncated(
        ui,
        &r.action,
        body_font,
        th.text_secondary().to_egui(),
        max_w,
    );
    let gap = tasty_ui_widgets::tokens::STRUCT_GAP_2.value();
    let sub_h = if r.sub.is_some() {
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(th.font_size_micro.value()))) + gap
    } else {
        0.0
    };
    let top = rect.center().y - (title.rect.height() + sub_h) * 0.5;
    let title_h = title.rect.height();
    ui.painter().galley(
        egui::pos2(rect.left() + x, top),
        title,
        egui::Color32::PLACEHOLDER,
    );
    let sub_y = top + title_h + gap;
    match &r.sub {
        Some(Sub::Plugin(name)) => {
            let d = th.status_dot_size.value();
            let micro = egui::FontId::monospace(th.font_size_micro.value());
            let g = truncated(ui, name, micro, th.text_muted().to_egui(), max_w - d);
            let cy = sub_y + g.rect.height() * 0.5;
            ui.painter().circle_filled(
                egui::pos2(rect.left() + x + d * 0.5, cy),
                d * 0.5,
                th.accent_agent().to_egui(),
            );
            ui.painter().galley(
                egui::pos2(rect.left() + x + d + PLUGIN_DOT_GAP.value(), sub_y),
                g,
                egui::Color32::PLACEHOLDER,
            );
        }
        Some(Sub::Note(note)) => {
            let g = truncated(
                ui,
                note,
                egui::FontId::proportional(th.font_size_micro.value()),
                th.text_muted().to_egui(),
                max_w,
            );
            ui.painter().galley(
                egui::pos2(rect.left() + x, sub_y),
                g,
                egui::Color32::PLACEHOLDER,
            );
        }
        None => {}
    }
}

fn value_cell(
    ui: &egui::Ui,
    rect: egui::Rect,
    x: f32,
    max_w: f32,
    text: &str,
    font: egui::FontId,
    fg: egui::Color32,
) {
    let g = truncated(ui, text, font, fg, max_w);
    let pos = egui::pos2(rect.left() + x, rect.center().y - g.rect.height() * 0.5);
    ui.painter().galley(pos, g, egui::Color32::PLACEHOLDER);
}
