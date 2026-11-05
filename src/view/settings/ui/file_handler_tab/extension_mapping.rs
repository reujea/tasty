use crate::runtime::file_catalog::FormatCatalog as FileFormatRegistry;
use std::collections::BTreeMap;

use crate::file::format::{DetectorId, DetectorInfo};
use crate::i18n::t;

use super::draw_intro_block;
use crate::adapters::ui::icons;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Input, TagVariant, Tooltip,
    tooltip_hover_delay_elapsed, vspace,
};

/// 등록된 확장자와 편집 중인 확장자를 표시하고 detector 우선순위를 조정한다.
pub(super) fn draw_extension_mapping(
    ui: &mut egui::Ui,
    draft: &mut Option<BTreeMap<String, Vec<DetectorId>>>,
    pending: &mut BTreeMap<String, Option<Vec<DetectorId>>>,
    new_ext_input: &mut String,
    file_format: &FileFormatRegistry,
) {
    let th = crate::theme::theme();
    // 초기 진입 시 registry 의 현재 priority 표를 draft 로 복사.
    if draft.is_none() {
        let mut map = BTreeMap::new();
        for ext in file_format.extension_priority_keys() {
            if let Some(order) = file_format.extension_priority_order(&ext) {
                map.insert(ext, order);
            }
        }
        *draft = Some(map);
    }
    let draft_map = draft.as_mut().expect("draft initialized above");

    draw_intro_block(
        ui,
        "settings.file_handler.extension_mapping.description",
        &[
            "settings.file_handler.extension_mapping.bullet_when",
            "settings.file_handler.extension_mapping.bullet_visibility",
            "settings.file_handler.extension_mapping.bullet_actions",
            "settings.file_handler.extension_mapping.bullet_unregistered",
        ],
    );

    // 새 확장자 수동 추가 — Input + Add(Button secondary sm). 입력이 비었거나 그 확장자를
    // 지원하는 detector가 없으면 Add는 disabled다.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let normalized = new_ext_input
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        let enabled =
            !normalized.is_empty() && !file_format.detectors_for_extension(&normalized).is_empty();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add = Button::new(t("button.add"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .enabled(enabled)
                .show(ui, &th);
            let placeholder = t("settings.file_handler.extension_mapping.add_placeholder");
            let input = Input::new()
                .mono(true)
                .placeholder(placeholder)
                .width(ui.available_width())
                .show(ui, &th, new_ext_input);
            let submitted = input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enabled && (add.clicked() || submitted) {
                let order = file_format.detectors_for_extension(&normalized);
                draft_map.entry(normalized.clone()).or_insert(order);
                new_ext_input.clear();
            }
        });
    });
    vspace(ui, th.spacing_md);

    // 초안의 확장자와 등록된 후보가 있는 확장자를 함께 표시한다.
    let all_exts = file_format.all_advertised_extensions();
    let mut visible: std::collections::BTreeSet<String> = draft_map.keys().cloned().collect();
    for ext in &all_exts {
        let candidates = file_format.detectors_for_extension(ext);
        if candidates.len() >= 2 {
            visible.insert(ext.clone());
        }
    }

    if visible.is_empty() {
        ui.label(
            egui::RichText::new(t("settings.file_handler.extension_mapping.no_conflicts"))
                .size(th.font_size_caption.value())
                .color(th.text_muted()),
        );
    } else {
        for ext in &visible {
            draw_extension_row(ui, ext, draft_map, pending, file_format);
            vspace(ui, th.spacing_md);
        }
    }
}

/// 한 확장자 묶음. 확장자 머리줄 아래에 detector 순서 목록을 그린다(먼저 맞는 detector가 이긴다).
fn draw_extension_row(
    ui: &mut egui::Ui,
    ext: &str,
    draft_map: &mut BTreeMap<String, Vec<DetectorId>>,
    pending: &mut BTreeMap<String, Option<Vec<DetectorId>>>,
    file_format: &FileFormatRegistry,
) {
    let th = crate::theme::theme();
    let candidates = file_format.detectors_for_extension(ext);
    let is_pending = pending.contains_key(ext);
    if candidates.is_empty() {
        // 미설치 확장자는 머리줄만 남는다 — detector 행 없이 아래 구분선 하나.
        let kind = if is_pending {
            GroupHeader::RemovePending
        } else {
            GroupHeader::NotInstalled
        };
        if group_header(ui, &th, ext, kind) {
            toggle_pending(ext, draft_map, pending);
        }
        return;
    }

    let order: Vec<DetectorId> = if let Some(d) = draft_map.get(ext) {
        let mut result: Vec<DetectorId> = d.clone();
        for c in &candidates {
            if !result.contains(c) {
                result.push(c.clone());
            }
        }
        result
    } else {
        candidates.clone()
    };

    let header = if is_pending {
        GroupHeader::ResetPending
    } else if draft_map.contains_key(ext) {
        GroupHeader::Custom
    } else {
        GroupHeader::Default
    };
    if group_header(ui, &th, ext, header) {
        toggle_pending(ext, draft_map, pending);
    }
    // 후보 = 켜져 있고 이 확장자를 지원하는 detector. 비후보(꺼짐·미설치) 행은 자리를 지키고
    // ▲▼를 모두 disabled로 둔다. ▼는 뒤에 후보가 더 없으면 disabled다.
    let last_candidate = last_candidate(&order, &candidates);
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, id) in order.iter().enumerate() {
            let candidate = candidates.contains(id);
            let (up, down) = detector_row(
                ui,
                &th,
                i,
                id.as_str(),
                candidate,
                arrows_enabled(i, candidate, last_candidate),
            );
            if up {
                move_up = Some(i);
            }
            if down {
                move_down = Some(i);
            }
        }
    });
    let swap = move_up
        .map(|i| (i - 1, i))
        .or(move_down.map(|i| (i, i + 1)));
    if let Some((a, b)) = swap {
        let mut new_order = order.clone();
        new_order.swap(a, b);
        draft_map.insert(ext.to_string(), new_order);
        // 순서를 다시 바꾸면 Reset 대기는 끝나고 새 사용자 순서가 된다.
        pending.remove(ext);
    }
}

/// Remove·Reset 은 draft 를 비우고 누르기 전 값을 `pending` 에 맡긴다. 같은 자리의 Undo 는
/// 맡긴 값으로 그 확장자 하나만 되돌린다.
fn toggle_pending(
    ext: &str,
    draft_map: &mut BTreeMap<String, Vec<DetectorId>>,
    pending: &mut BTreeMap<String, Option<Vec<DetectorId>>>,
) {
    match pending.remove(ext) {
        Some(Some(before)) => {
            draft_map.insert(ext.to_string(), before);
        }
        Some(None) => {
            draft_map.remove(ext);
        }
        None => {
            let before = draft_map.insert(ext.to_string(), Vec::new());
            pending.insert(ext.to_string(), before);
        }
    }
}

/// 확장자 머리줄의 갈래. 오른쪽 끝 버튼과 `.ext` 색이 이것으로 정해진다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupHeader {
    /// 설치 순서 그대로 — 버튼 없음.
    Default,
    /// 사용자가 순서를 바꿨다 — Reset.
    Custom,
    /// 켜진 detector 가 없다 — `.ext` 흐림 · Tag disabled · Remove · 아래 구분선.
    NotInstalled,
    /// Reset 을 눌러 Save 를 기다린다 — Tag disabled "reset on save" · Undo.
    ResetPending,
    /// Remove 를 눌러 Save 를 기다린다 — `.ext` 흐림·취소선 · Tag disabled "removed on save" · Undo.
    RemovePending,
}

impl GroupHeader {
    fn not_installed(self) -> bool {
        matches!(self, Self::NotInstalled | Self::RemovePending)
    }
}

/// 확장자 머리줄 — `.ext` · (미설치면 Tag disabled) · 빈 칸 · 오른쪽 끝 ghost Button sm.
/// 위·아래 space-xs 여백 안의 높이는 button-height-sm 이상이라 Reset 이 나타나도 아래 행이
/// 움직이지 않는다. Tag 와 버튼은 줄지 않는다. 버튼이 눌렸는지를 돌려준다.
fn group_header(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    ext: &str,
    kind: GroupHeader,
) -> bool {
    let mut clicked = false;
    let head_y = th.spacing_xs.value().round() as i8;
    let resp = egui::Frame::new()
        .inner_margin(egui::Margin {
            top: head_y,
            bottom: head_y,
            ..egui::Margin::ZERO
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(th.button_height_sm().value());
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let ext_fg = if kind.not_installed() {
                    th.text_muted()
                } else {
                    th.text_secondary()
                };
                let mut label = egui::RichText::new(format!(".{ext}"))
                    .monospace()
                    .size(th.font_size_caption.value())
                    .color(ext_fg.to_egui());
                if kind == GroupHeader::RemovePending {
                    label = label.strikethrough();
                }
                ui.label(label);
                let tag = match kind {
                    GroupHeader::NotInstalled => {
                        Some(t("settings.file_handler.extension_mapping.unregistered"))
                    }
                    GroupHeader::RemovePending => {
                        Some(t("settings.file_handler.extension_mapping.removed_on_save"))
                    }
                    GroupHeader::ResetPending => {
                        Some(t("settings.file_handler.extension_mapping.reset_on_save"))
                    }
                    GroupHeader::Default | GroupHeader::Custom => None,
                };
                if let Some(tag) = tag {
                    tasty_ui_widgets::tag(ui, th, tag, TagVariant::Default, false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, tooltip) = match kind {
                        GroupHeader::Default => return,
                        GroupHeader::Custom => (
                            t("settings.file_handler.extension_mapping.reset"),
                            Some(t("settings.file_handler.extension_mapping.reset_tooltip")),
                        ),
                        GroupHeader::NotInstalled => {
                            (t("settings.file_handler.extension_mapping.clear"), None)
                        }
                        GroupHeader::ResetPending | GroupHeader::RemovePending => {
                            (t("settings.file_handler.extension_mapping.undo"), None)
                        }
                    };
                    let resp = Button::new(label)
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, th);
                    if let Some(tip) = tooltip
                        && tooltip_hover_delay_elapsed(ui.ctx(), th, resp.id, resp.hovered())
                    {
                        Tooltip::new(tip).id_source(resp.id).show(ui, th, resp.rect);
                    }
                    clicked = resp.clicked();
                });
            });
        })
        .response;
    if kind.not_installed() {
        ui.painter().hline(
            resp.rect.x_range(),
            resp.rect.bottom(),
            egui::Stroke::new(
                th.border_width.value(),
                th.separator.to_egui_premultiplied(),
            ),
        );
    }
    clicked
}

/// 순서 목록에서 마지막 후보의 위치. 그 뒤에는 비후보 행만 온다.
fn last_candidate(order: &[DetectorId], candidates: &[DetectorId]) -> Option<usize> {
    order.iter().rposition(|id| candidates.contains(id))
}

/// (▲, ▼) 활성 여부. 맨 위 행의 ▲와 마지막 후보의 ▼는 disabled이고,
/// 비후보 행은 둘 다 disabled다. 숨기지 않으므로 행마다 자리가 같다.
fn arrows_enabled(index: usize, candidate: bool, last_candidate: Option<usize>) -> (bool, bool) {
    (
        candidate && index > 0,
        candidate && Some(index) != last_candidate,
    )
}

/// detector 한 행 — 순번 · 이름 · (비후보면 Tag disabled "off") · ▲ · ▼.
/// ▲·▼가 눌렸는지를 돌려준다. disabled 버튼은 클릭을 받지 않는다.
fn detector_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    index: usize,
    name: &str,
    candidate: bool,
    (up_enabled, down_enabled): (bool, bool),
) -> (bool, bool) {
    let mut up = false;
    let mut down = false;
    let resp = ui.horizontal(|ui| {
        ui.set_min_height(th.settings_row_min_height().value());
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let (num_rect, _) = ui.allocate_exact_size(
            egui::vec2(th.spacing_lg.value(), th.font_size_caption.value()),
            egui::Sense::hover(),
        );
        ui.painter().text(
            num_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            (index + 1).to_string(),
            egui::FontId::monospace(th.font_size_caption.value()),
            th.text_muted().to_egui(),
        );
        let name_fg = if candidate {
            th.text_secondary()
        } else {
            th.text_muted()
        };
        ui.label(
            egui::RichText::new(name)
                .size(th.font_size_body.value())
                .color(name_fg.to_egui()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            down = IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .enabled(down_enabled)
                .show(ui, th, &|ui, rect, c| {
                    icons::CHEVRON_DOWN
                        .image(rect.width(), c)
                        .paint_at(ui, rect)
                })
                .clicked();
            up = IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .enabled(up_enabled)
                .show(ui, th, &|ui, rect, c| {
                    icons::CHEVRON_UP.image(rect.width(), c).paint_at(ui, rect)
                })
                .clicked();
            if !candidate {
                tasty_ui_widgets::tag(
                    ui,
                    th,
                    t("settings.file_handler.extension_mapping.off"),
                    TagVariant::Default,
                    false,
                );
            }
        });
    });
    let rect = resp.response.rect;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    (up, down)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시안 규칙: ▲는 맨 위 행, ▼는 마지막 후보에서 disabled다. 비후보(꺼짐·미설치) 행은
    /// 위치와 관계없이 둘 다 disabled다. 뒤에 비후보만 남은 후보는 ▼를 쓸 수 없다.
    #[test]
    fn arrows_follow_the_top_row_last_candidate_and_non_candidate_rule() {
        let id = |s: &str| DetectorId::new(s);
        let order = [id("markdown"), id("editor"), id("html-preview")];
        let candidates = [id("markdown"), id("editor")];
        let last = last_candidate(&order, &candidates);
        assert_eq!(last, Some(1));
        let rows: Vec<(bool, bool)> = order
            .iter()
            .enumerate()
            .map(|(i, d)| arrows_enabled(i, candidates.contains(d), last))
            .collect();
        assert_eq!(rows, [(false, true), (true, false), (false, false)]);

        // 비후보가 가운데에 끼어도 마지막 후보만 ▼가 막힌다.
        let order = [id("a"), id("off"), id("b")];
        let candidates = [id("a"), id("b")];
        let last = last_candidate(&order, &candidates);
        let rows: Vec<(bool, bool)> = order
            .iter()
            .enumerate()
            .map(|(i, d)| arrows_enabled(i, candidates.contains(d), last))
            .collect();
        assert_eq!(rows, [(false, true), (false, false), (true, false)]);
    }

    /// Remove·Reset 은 draft 를 비우고 이전 값을 맡긴다. Undo 는 그 확장자 하나만 원래 값으로
    /// 되돌리고(draft 에 없던 것은 다시 없앤다), 다른 확장자의 대기는 그대로 둔다.
    #[test]
    fn undo_restores_only_that_extensions_draft_value() {
        let id = |s: &str| DetectorId::new(s);
        let mut draft = BTreeMap::from([
            ("md".to_string(), vec![id("editor"), id("markdown")]),
            ("ipynb".to_string(), vec![id("gone")]),
        ]);
        let mut pending = BTreeMap::new();

        toggle_pending("md", &mut draft, &mut pending);
        toggle_pending("ipynb", &mut draft, &mut pending);
        assert_eq!(draft["md"], Vec::<DetectorId>::new());
        assert_eq!(draft["ipynb"], Vec::<DetectorId>::new());
        assert_eq!(pending.len(), 2);

        toggle_pending("md", &mut draft, &mut pending);
        assert_eq!(draft["md"], vec![id("editor"), id("markdown")]);
        assert!(!pending.contains_key("md"));
        assert_eq!(draft["ipynb"], Vec::<DetectorId>::new());
        assert!(pending.contains_key("ipynb"));

        // draft 에 없던 확장자는 Undo 뒤에도 draft 에 남지 않는다.
        toggle_pending("log", &mut draft, &mut pending);
        assert!(draft.contains_key("log"));
        toggle_pending("log", &mut draft, &mut pending);
        assert!(!draft.contains_key("log"));
    }
}
