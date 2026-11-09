//! 가져오기 비교 표의 그룹 헤더, 선택 열, 긴 목록 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::checkbox;

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

use super::paint::{intro, truncated, truncated_tracked};
use super::{
    GROUP_CHEVRON_GAP, GROUPS, Group, IE_FILE, PLUGIN_DOT_GAP, PREVIEW_H, Row, STATE, State,
    counts, detail_frame, selected_count,
};

pub fn draw_preview(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        STATE.with(|s| {
            let st = &mut *s.borrow_mut();
            detail_frame(
                ui,
                theme,
                "kb_ie_preview",
                PREVIEW_H,
                st,
                |ui, theme, st| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
                    let (changed, total) = counts();
                    intro(
                        ui,
                        theme,
                        theme.measure_lg,
                        &format!(
                            "{IE_FILE} — {changed} of {total} bindings change, {} selected. Apply \
                         writes the selected rows into the draft; nothing is saved until you \
                         press Save.",
                            selected_count(st)
                        ),
                    );
                    diff_table(ui, theme, st);
                },
            );
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "columns",
                "32px select · 1.6fr action · 1fr current · 1fr imported",
            ),
            ("group header", "spans all columns, on surface-raised"),
            (
                "group content",
                "select-all · name (font-label 10 caps) · counts · chevron",
            ),
            (
                "groups",
                "general · quick switch · scripts · plugin overrides",
            ),
            ("default view", "changed only"),
            ("toggle", "“Show all {n}” in the back bar"),
            ("quick switch", "1 row per axis (3), slot count as sub-line"),
            (
                "plugin row",
                "command name + agent-dot plugin name, two lines",
            ),
            ("changed cell", "accent-primary, colour only (no bold)"),
        ],
        &[
            TokenChip::new(
                "surface-raised",
                "group header bed",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "changed imported value",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new("accent-agent", "plugin dot", theme.accent_agent().to_egui()),
            TokenChip::without_color("separator", "cell rules"),
            TokenChip::without_color("letter-spacing-caps", "header + group name tracking"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Apply writes the draft, Save commits it — the same two-stage contract as Preset, and \
         the intro line says so. The two never share a row: Apply sits in the back bar, Save in \
         the window footer.",
    );
}

/// jsx `IeDiffTable` — 4 열 grid(select · action · current · imported), 그룹 헤더 축.
fn diff_table(ui: &mut egui::Ui, theme: &Theme, st: &mut State) {
    let w = ui.available_width();
    let select_w = theme.kb_ie_select_column_width().value();
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
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let hairline = egui::Stroke::new(
        theme.border_width.value(),
        theme.separator.to_egui_premultiplied(),
    );

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        let head_font = tasty_ui_widgets::label_font(theme.font_size_micro.value());
        let head_h = ui.fonts(|f| f.row_height(&head_font)) + pad_y;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, head_h), egui::Sense::hover());
        let caps = theme.letter_spacing_caps(theme.font_size_micro);
        for (i, text) in ["", "ACTION", "CURRENT", "IMPORTED"].iter().enumerate() {
            let g = truncated_tracked(
                ui,
                text,
                head_font.clone(),
                theme.text_muted().to_egui(),
                f32::INFINITY,
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
            rect.bottom() - theme.border_width.value() * 0.5,
            hairline,
        );

        for g in GROUPS {
            let rows: Vec<&Row> = g
                .rows
                .iter()
                .filter(|r| !st.changed_only || r.cur != r.next)
                .collect();
            let open = !st.collapsed.contains(g.id);
            group_header(ui, theme, w, g, &rows, open, st);
            if !open {
                continue;
            }
            for r in rows {
                let key = (g.id, r.action);
                let row_h = diff_row_height(ui, theme, r);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, row_h), egui::Sense::hover());
                // 라벨이 없어도 체크박스 뒤에 간격이 붙으므로 박스 중심을 기준으로 배치한다.
                let box_sz = theme.checkbox_size().value();
                let sel_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + (col_w[0] - box_sz) * 0.5, rect.top()),
                    egui::vec2(col_w[0], row_h),
                );
                let mut on = !st.deselected.contains(&key);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(sel_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        if checkbox(
                            ui,
                            theme,
                            &mut on,
                            "",
                            true,
                            tasty_ui_widgets::Ground::Panel,
                        )
                        .changed()
                        {
                            if on {
                                st.deselected.remove(&key);
                            } else {
                                st.deselected.insert(key);
                            }
                        }
                    },
                );
                action_cell(ui, theme, rect, x_off[1] + pad_x, col_w[1] - pad_x * 2.0, r);
                let mono = egui::FontId::monospace(theme.font_size_term_sm.value());
                value_cell(
                    ui,
                    rect,
                    x_off[2] + pad_x,
                    col_w[2] - pad_x * 2.0,
                    r.cur,
                    mono.clone(),
                    theme.text_muted().to_egui(),
                );
                let fg = if r.cur != r.next {
                    theme.accent_primary()
                } else {
                    theme.text_muted()
                };
                value_cell(
                    ui,
                    rect,
                    x_off[3] + pad_x,
                    col_w[3] - pad_x * 2.0,
                    r.next,
                    mono,
                    fg.to_egui(),
                );
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - theme.border_width.value() * 0.5,
                    hairline,
                );
            }
        }
    });
}

/// 그룹 헤더 — 네 열을 가로지르는 한 행(surface-raised). select-all · chevron · 그룹명 ·
/// `N changed · M total`. 열 헤더를 반복하지 않는다.
fn group_header(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: f32,
    g: &Group,
    shown: &[&Row],
    open: bool,
    st: &mut State,
) {
    let h = theme
        .checkbox_size()
        .value()
        .max(theme.font_size_micro.value())
        + theme.spacing_sm.value() * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.surface_raised().to_egui());
    let inner = rect.shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            let mut all = !shown.is_empty()
                && shown
                    .iter()
                    .all(|r| !st.deselected.contains(&(g.id, r.action)));
            if checkbox(
                ui,
                theme,
                &mut all,
                "",
                true,
                tasty_ui_widgets::Ground::Panel,
            )
            .changed()
            {
                for r in g.rows {
                    if all {
                        st.deselected.remove(&(g.id, r.action));
                    } else {
                        st.deselected.insert((g.id, r.action));
                    }
                }
            }
            let glyph = theme.icon_glyph_size_sm.value();
            let galley = truncated_tracked(
                ui,
                &g.label.to_uppercase(),
                tasty_ui_widgets::label_font(theme.font_size_micro.value()),
                theme.text_secondary_raised().to_egui(),
                f32::INFINITY,
                theme.letter_spacing_caps(theme.font_size_micro),
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
                .image(glyph, theme.text_muted_raised().to_egui())
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
                    st.collapsed.insert(g.id);
                } else {
                    st.collapsed.remove(g.id);
                }
            }
            let changed = g.rows.iter().filter(|r| r.cur != r.next).count();
            ui.label(
                egui::RichText::new(format!("{changed} changed · {} total", g.rows.len()))
                    .family(tasty_ui_widgets::label_font_family())
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted_raised().to_egui()),
            );
        },
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - theme.border_width.value() * 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

fn diff_row_height(ui: &egui::Ui, theme: &Theme, r: &Row) -> f32 {
    let body =
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(theme.font_size_body.value())));
    let sub = if r.plugin.is_some() || r.note.is_some() {
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(theme.font_size_micro.value())))
            + tasty_ui_widgets::tokens::STRUCT_GAP_2.value()
    } else {
        0.0
    };
    body + sub + theme.spacing_sm.value() * 2.0
}

fn action_cell(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, x: f32, max_w: f32, r: &Row) {
    let body_font = egui::FontId::proportional(theme.font_size_body.value());
    let title = truncated(
        ui,
        r.action,
        body_font,
        theme.text_secondary().to_egui(),
        max_w,
    );
    let has_sub = r.plugin.is_some() || r.note.is_some();
    let sub_h = if has_sub {
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(theme.font_size_micro.value())))
            + tasty_ui_widgets::tokens::STRUCT_GAP_2.value()
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
    let sub_y = top + title_h + tasty_ui_widgets::tokens::STRUCT_GAP_2.value();
    if let Some(plugin) = r.plugin {
        let d = theme.status_dot_size.value();
        let micro = egui::FontId::monospace(theme.font_size_micro.value());
        let g = truncated(ui, plugin, micro, theme.text_muted().to_egui(), max_w - d);
        let cy = sub_y + g.rect.height() * 0.5;
        ui.painter().circle_filled(
            egui::pos2(rect.left() + x + d * 0.5, cy),
            d * 0.5,
            theme.accent_agent().to_egui(),
        );
        ui.painter().galley(
            egui::pos2(rect.left() + x + d + PLUGIN_DOT_GAP.value(), sub_y),
            g,
            egui::Color32::PLACEHOLDER,
        );
    } else if let Some(note) = r.note {
        let g = truncated(
            ui,
            note,
            egui::FontId::proportional(theme.font_size_micro.value()),
            theme.text_muted().to_egui(),
            max_w,
        );
        ui.painter().galley(
            egui::pos2(rect.left() + x, sub_y),
            g,
            egui::Color32::PLACEHOLDER,
        );
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
