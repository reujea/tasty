//! 단축키 프리셋 목록과 선택한 프리셋의 비교 화면.
//! Apply는 설정 초안에만 반영하며, 하단 Save가 파일에 저장한다.
//! selected_preset이 없으면 목록을, 있으면 해당 프리셋의 비교 표를 보여준다.

use std::cell::Cell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, DrillDown, DrillDownView, ListCtrl, ListCtrlItem,
    TagVariant, tag,
};

use crate::i18n::{t, t_count, t_fmt};
use crate::settings::{GeneralSettings, KeybindingSettings};

pub(super) fn draw_preset_subtab(
    ui: &mut egui::Ui,
    keybindings: &mut KeybindingSettings,
    general: &GeneralSettings,
    selected_preset: &mut Option<String>,
) {
    let th = crate::theme::theme();
    let names = KeybindingSettings::preset_names();

    // 사용 중(Active) 프리셋 — 현재 draft 와 모든 일반 바인딩이 일치하는 프리셋.
    let active_idx = names.iter().position(|n| preset_matches(keybindings, n));

    // 선택한 프리셋이 없어졌으면 목록을 표시한다.
    let detail = selected_preset
        .as_deref()
        .and_then(|n| KeybindingSettings::preset_by_name(n).map(|p| (n.to_string(), p)));
    let view = if detail.is_some() {
        DrillDownView::Detail
    } else {
        DrillDownView::List
    };

    let title = detail
        .as_ref()
        .map(|(n, _)| t_fmt("settings.keybindings.preset_detail_title", n))
        .unwrap_or_default();
    let is_active_sel = detail
        .as_ref()
        .is_some_and(|(n, _)| active_idx.is_some_and(|i| names[i] == n));

    // DrillDown 클로저는 &dyn Fn(불변) — 클릭 신호는 Cell 로 꺼내 show 후 반영한다.
    let clicked_row = Cell::new(None::<usize>);
    let apply_clicked = Cell::new(false);

    let active_tag = |ui: &mut egui::Ui, th: &Theme| {
        tag(
            ui,
            th,
            t("settings.keybindings.preset_active_tag"),
            TagVariant::Success,
            true,
        );
    };
    let actions = |ui: &mut egui::Ui, th: &Theme| {
        let label = if is_active_sel {
            t("settings.keybindings.preset_applied_button")
        } else {
            t("settings.keybindings.apply_button")
        };
        if Button::new(label)
            .variant(ButtonVariant::Primary)
            .size(ControlSize::Sm)
            .enabled(!is_active_sel)
            .show(ui, th)
            .clicked()
        {
            apply_clicked.set(true);
        }
    };

    let out = DrillDown::new("settings_kb_preset")
        .view(view)
        .title(&title)
        .back_label(t("settings.keybindings.preset_back"))
        .show(
            ui,
            &th,
            |ui, th| {
                // jsx list wrapper: padding space-md(상하)/space-lg(좌우), gap space-sm.
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(
                        th.spacing_lg.value() as i8,
                        th.spacing_md.value() as i8,
                    ))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                        intro_note(ui, th, t("settings.keybindings.select_preset_label"));
                        let items: Vec<ListCtrlItem<'_>> = names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| {
                                let mut item =
                                    ListCtrlItem::new(name).description(preset_desc(name));
                                if active_idx == Some(i) {
                                    item = item.trailing(&active_tag);
                                }
                                item
                            })
                            .collect();
                        let out = ListCtrl::new().show(ui, th, &items, active_idx);
                        if let Some(i) = out.clicked {
                            clicked_row.set(Some(i));
                        }
                    });
            },
            |ui, th| {
                let Some((name, preset)) = detail.as_ref() else {
                    return;
                };
                // jsx detail wrapper: padding space-lg, column gap space-md.
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(th.spacing_lg.value() as i8))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                        let (changed, total) = diff_counts(keybindings, preset);
                        let note = if is_active_sel {
                            t("settings.keybindings.preset_note_active").to_string()
                        } else {
                            t_count(
                                "settings.keybindings.preset_note_diff",
                                total as u64,
                                &[&changed.to_string(), &total.to_string()],
                            )
                        };
                        intro_note(ui, th, &note);
                        draw_preset_diff_table(ui, th, keybindings, preset, name, general);
                    });
            },
            Some(&actions),
        );

    if let Some(i) = clicked_row.get() {
        *selected_preset = Some(names[i].to_string());
    }
    if apply_clicked.get()
        && let Some((name, _)) = detail.as_ref()
        && !keybindings.apply_preset(name)
    {
        tracing::warn!("preset subtab: apply_preset failed for unknown preset {name:?}");
    }
    if out.back_clicked {
        *selected_preset = None;
    }
}

/// 안내문 — jsx `<p>` 전사: caption(muted), line-height ui, max-width measure-md.
fn intro_note(ui: &mut egui::Ui, th: &Theme, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(th.measure_md.value());
        ui.label(
            egui::RichText::new(text)
                .size(th.font_size_caption.value())
                .color(th.text_muted()),
        );
    });
}

/// 프리셋 한 줄 설명 (리스트 행 description).
fn preset_desc(name: &str) -> &str {
    match name {
        "Tasty" => t("settings.keybindings.preset_desc_tasty"),
        "Mac" => t("settings.keybindings.preset_desc_mac"),
        "Windows" => t("settings.keybindings.preset_desc_windows"),
        "Linux" => t("settings.keybindings.preset_desc_linux"),
        _ => "",
    }
}

/// 현재 draft 와 프리셋의 모든 일반 바인딩이 일치하면 true (= 사용 중).
fn preset_matches(current: &KeybindingSettings, name: &str) -> bool {
    KeybindingSettings::preset_by_name(name).is_some_and(|p| {
        KeybindingSettings::binding_fields()
            .all(|(id, _)| current.get_bindings(id) == p.get_bindings(id))
    })
}

/// (변경 행 수, 전체 행 수).
fn diff_counts(current: &KeybindingSettings, preset: &KeybindingSettings) -> (usize, usize) {
    let changed = KeybindingSettings::binding_fields()
        .filter(|(id, _)| current.get_bindings(id) != preset.get_bindings(id))
        .count();
    (changed, KeybindingSettings::binding_fields().count())
}

/// 동작·현재 키·프리셋 키의 비교 표. 달라진 프리셋 값은 색으로 강조한다.
fn draw_preset_diff_table(
    ui: &mut egui::Ui,
    th: &Theme,
    current: &KeybindingSettings,
    preset: &KeybindingSettings,
    preset_name: &str,
    general: &GeneralSettings,
) {
    let w = ui.available_width();
    // grid-template-columns: minmax(0,1.6fr) 1fr 1fr → 폭 비율 1.6/1/1.
    let col1 = w * 1.6 / 3.6;
    let col = w / 3.6;
    let x_off = [0.0, col1, col1 + col];
    let col_w = [col1, col, col];
    let pad_x = th.spacing_md.value();
    let pad_y = th.spacing_sm.value();
    let hairline = egui::Stroke::new(
        th.border_width.value(),
        th.separator.to_egui_premultiplied(),
    );

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        // ── 헤더 행 (padding: 0 space-md space-sm) ──
        let head_font = tasty_ui_widgets::label_font(th.font_size_micro.value());
        let head_h = ui.fonts(|f| f.row_height(&head_font)) + pad_y;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, head_h), egui::Sense::hover());
        let headers = [
            t("settings.keybindings.preset_col_action").to_uppercase(),
            t("settings.keybindings.preset_col_before").to_uppercase(),
            preset_name.to_uppercase(),
        ];
        for (i, text) in headers.iter().enumerate() {
            let galley = truncated(
                ui,
                text,
                head_font.clone(),
                th.text_muted().to_egui(),
                (col_w[i] - pad_x * 2.0).max(0.0),
            );
            ui.painter().galley(
                egui::pos2(rect.left() + x_off[i] + pad_x, rect.top()),
                galley,
                egui::Color32::PLACEHOLDER,
            );
        }
        let y = rect.bottom() - th.border_width.value() * 0.5;
        ui.painter().hline(rect.x_range(), y, hairline);

        // ── 데이터 행 (padding: space-sm space-md, 하단 헤어라인) ──
        let action_font = egui::FontId::proportional(th.font_size_body.value());
        let mono_font = egui::FontId::monospace(th.font_size_term_sm.value());
        // 현재 조합 열은 muted caption, 바뀔 조합 열은 12 다.
        let cur_font = egui::FontId::monospace(th.font_size_caption.value());
        let content_h = ui.fonts(|f| f.row_height(&action_font).max(f.row_height(&mono_font)));
        let row_h = content_h + pad_y * 2.0;

        for (field_id, label_key) in KeybindingSettings::binding_fields() {
            let cur_raw = current.get_bindings(field_id).unwrap_or(&[]);
            let next_raw = preset.get_bindings(field_id).unwrap_or(&[]);
            let changed = cur_raw != next_raw;
            let action = t(label_key).trim_end_matches(':').trim().to_string();

            let next_fg = if changed {
                th.accent_primary_raised().to_egui()
            } else {
                th.text_muted().to_egui()
            };
            let cells: [(String, egui::FontId, egui::Color32); 3] = [
                (action, action_font.clone(), th.text_secondary().to_egui()),
                (
                    fmt_bindings(cur_raw, general),
                    cur_font.clone(),
                    th.text_muted().to_egui(),
                ),
                (fmt_bindings(next_raw, general), mono_font.clone(), next_fg),
            ];

            let (rect, _) = ui.allocate_exact_size(egui::vec2(w, row_h), egui::Sense::hover());
            for (i, (text, font, fg)) in cells.iter().enumerate() {
                let galley = truncated(
                    ui,
                    text,
                    font.clone(),
                    *fg,
                    (col_w[i] - pad_x * 2.0).max(0.0),
                );
                let pos = egui::pos2(
                    rect.left() + x_off[i] + pad_x,
                    rect.center().y - galley.rect.height() * 0.5,
                );
                ui.painter().galley(pos, galley, egui::Color32::PLACEHOLDER);
            }
            let y = rect.bottom() - th.border_width.value() * 0.5;
            ui.painter().hline(rect.x_range(), y, hairline);
        }
    });
}

/// 바인딩 목록 표시 문자열 — 비어 있으면 "None"(i18n).
fn fmt_bindings(v: &[String], general: &GeneralSettings) -> String {
    if v.is_empty() {
        t("settings.keybindings.hint_none").to_string()
    } else {
        v.iter()
            .map(|b| KeybindingSettings::format_display(b, general))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// 한 줄 말줄임 galley (디자인 ellipsis — listctrl 관례).
fn truncated(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    ui.fonts(|f| f.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_matches_는_동일_프리셋만_true() {
        let current = KeybindingSettings::preset_tasty();
        assert!(preset_matches(&current, "Tasty"));
        assert!(!preset_matches(&current, "Mac"));
        assert!(!preset_matches(&current, "존재하지 않는 프리셋"));
    }

    #[test]
    fn diff_counts_동일하면_변경_0() {
        let p = KeybindingSettings::preset_tasty();
        let (changed, total) = diff_counts(&p, &p);
        assert_eq!(changed, 0);
        assert_eq!(total, KeybindingSettings::binding_fields().count());
    }

    #[test]
    fn diff_counts_다른_프리셋은_변경_행이_있다() {
        let cur = KeybindingSettings::preset_tasty();
        let mac = KeybindingSettings::preset_mac();
        let (changed, total) = diff_counts(&cur, &mac);
        assert!(changed > 0);
        assert!(changed <= total);
    }
}
