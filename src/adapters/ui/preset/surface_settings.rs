//! 프리셋 surface 설정. 오른쪽 상세 영역에 헤더·필드·확인/취소 버튼을 표시한다.
//! 값은 LeafDraft에만 기록하고 저장·복귀는 호출부에서 처리한다.
//! 변경이 없으면 확인을 막는다. Escape 취소와 한 줄 입력의 Enter 확인은 고정 대화상자 키다.

mod browse;
#[cfg(test)]
mod browse_tests;
pub(crate) use browse::FieldBrowseRequest;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::tokens::STRUCT_GAP_3;
use tasty_ui_widgets::{Button, ButtonVariant, Input, select};

use crate::adapters::ui::icons;
use crate::i18n::{t, t_fmt};
use crate::runtime::surface_registry::{PresetFieldInput, PresetFieldSpec, PresetFieldTarget};

use super::demo_layout::{KindCatalog, LeafDraft, LeafLocation, kind_accent};

/// 대상 leaf, 초안과 변경 비교 기준. 다른 프리셋으로 바뀌면 적용하지 않고 버린다.
#[derive(Clone, Debug)]
pub struct SurfaceCfg {
    generation: u64,
    browse_request: Option<FieldBrowseRequest>,
    preset_key: String,
    leaf_id: usize,
    draft: LeafDraft,
    orig: LeafDraft,
}

impl SurfaceCfg {
    pub(crate) fn remap_preset(&mut self, kind: tasty_presets::PresetKind, from: &str, to: &str) {
        if self.preset_key == format!("{}:{from}", kind.as_str()) {
            self.preset_key = format!("{}:{to}", kind.as_str());
            self.generation = browse::next_generation();
            self.browse_request = None;
        }
    }
    pub(crate) fn belongs_to(&self, kind: tasty_presets::PresetKind, name: &str) -> bool {
        self.preset_key == format!("{}:{name}", kind.as_str())
    }

    pub(super) fn open(preset_key: String, leaf_id: usize, orig: LeafDraft) -> Self {
        Self {
            generation: browse::next_generation(),
            browse_request: None,
            preset_key,
            leaf_id,
            draft: orig.clone(),
            orig,
        }
    }

    pub(super) fn preset_key(&self) -> &str {
        &self.preset_key
    }

    pub(super) fn leaf_id(&self) -> usize {
        self.leaf_id
    }

    pub(super) fn draft(&self) -> &LeafDraft {
        &self.draft
    }
}

/// 이번 프레임에 사용자가 고른 것.
pub(super) enum CfgOutcome {
    None,
    /// 확인 — draft 를 적용·저장해 달라(변경이 있을 때만 나온다).
    Confirm,
    /// 취소 — draft 를 버려 달라.
    Cancel,
}

/// 헤더 breadcrumb 조각: `preset › pane N › tab › surface k`. `pane N` 은 Workspace
/// scope 에서만, 탭 이름은 Tab scope 가 아닐 때만 있다.
pub(super) fn breadcrumb(preset: &str, loc: &LeafLocation) -> Vec<String> {
    let mut v = vec![preset.to_string()];
    if let Some(n) = loc.pane {
        v.push(t_fmt("preset.settings.pane", &n.to_string()));
    }
    if let Some(tab) = &loc.tab {
        v.push(tab.clone());
    }
    v.push(t_fmt("preset.settings.surface", &loc.surface.to_string()));
    v
}

/// 설정 화면을 `rect`(detail 컬럼 전체)에 그린다.
pub(super) fn draw_surface_settings(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    cfg: &mut SurfaceCfg,
    catalog: &KindCatalog,
    path: &[String],
) -> CfgOutcome {
    // 키는 위젯보다 먼저 읽는다 — Kind 드롭다운이 열려 있을 때의 Esc 는 드롭다운을
    // 닫는 키이지 화면을 닫는 키가 아니고, 그 popup 은 이번 프레임 안에서 닫힌다.
    let chooser_open = crate::view::file_chooser::owns_input(ui.ctx());
    let (esc, enter) = ui.input(|i| {
        (
            !chooser_open && ui.is_enabled() && i.key_pressed(egui::Key::Escape),
            !chooser_open && ui.is_enabled() && i.key_pressed(egui::Key::Enter),
        )
    });
    let popup_open = ui.memory(|m| m.any_popup_open());

    let bw = theme.border_width.value();
    let header_h = theme.preset_cfg_header_height().value();
    let footer_h = theme.preset_cfg_footer_height().value();
    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), header_h));
    let footer = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, (rect.max.y - footer_h).max(header.max.y)),
        rect.max,
    );
    let body = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, header.max.y),
        egui::pos2(rect.max.x, footer.min.y),
    );

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let sep = egui::Stroke::new(bw, theme.separator.to_egui_premultiplied());
    painter.hline(header.x_range(), header.max.y, sep);
    painter.hline(footer.x_range(), footer.min.y, sep);

    let mut outcome = CfgOutcome::None;
    let mut enter_in_input = false;

    draw_body(ui, theme, body, cfg, catalog, enter, &mut enter_in_input);
    let dirty = cfg.draft.is_dirty(&cfg.orig, catalog);
    draw_header(ui, theme, header, &cfg.draft, catalog, path, dirty);
    let (ok, cancel) = draw_footer(ui, theme, footer, dirty);

    if ok || (enter_in_input && dirty) {
        outcome = CfgOutcome::Confirm;
    }
    if cancel || (esc && !popup_open) {
        outcome = CfgOutcome::Cancel;
    }
    outcome
}

fn draw_header(
    ui: &mut egui::Ui,
    theme: &Theme,
    header: egui::Rect,
    draft: &LeafDraft,
    catalog: &KindCatalog,
    path: &[String],
    dirty: bool,
) {
    let inner = header.shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
    let mut hui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    hui.set_clip_rect(header);
    hui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let muted = theme.text_muted().to_egui();
    let caption = theme.font_size_caption.value();

    if dirty {
        let resp = hui
            .scope(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                ui.label(
                    egui::RichText::new(t("preset.settings.unsaved"))
                        .size(caption)
                        .color(muted),
                );
                let d = theme.status_dot_size_compact().value();
                let (dot, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
                ui.painter().circle_filled(
                    dot.center(),
                    d * 0.5,
                    theme.preset_cfg_draft_fg().to_egui(),
                );
            })
            .response;
        resp.on_hover_text(t("preset.settings.unsaved_tooltip"));
    }

    hui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let kind = draft.kind();
        let size = theme.icon_glyph_size_md.value();
        let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        catalog
            .kind_icon(kind)
            .image(size, kind_accent(theme, kind))
            .paint_at(ui, icon_rect);
        ui.label(
            egui::RichText::new(catalog.label(kind))
                .size(theme.font_size_max.value())
                .strong()
                .color(theme.text_primary().to_egui()),
        );
        if !path.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(path.join(" › "))
                        .monospace()
                        .size(caption)
                        .color(muted),
                )
                .truncate(),
            );
        }
    });
}

fn draw_body(
    ui: &mut egui::Ui,
    theme: &Theme,
    body: egui::Rect,
    cfg: &mut SurfaceCfg,
    catalog: &KindCatalog,
    enter: bool,
    enter_in_input: &mut bool,
) {
    if body.height() <= 0.0 {
        return;
    }
    let mut bui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    bui.set_clip_rect(body);
    let pad = theme.preset_cfg_form_padding().value();
    egui::ScrollArea::vertical()
        .id_salt("preset_cfg_body")
        .auto_shrink([false; 2])
        .drag_to_scroll(false)
        .show(&mut bui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(pad as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let w = ui
                        .available_width()
                        .min(theme.preset_cfg_form_max_width().value());
                    draw_form(ui, theme, w, cfg, catalog, enter, enter_in_input);
                });
        });
}

fn draw_form(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: f32,
    cfg: &mut SurfaceCfg,
    catalog: &KindCatalog,
    enter: bool,
    enter_in_input: &mut bool,
) {
    let gap = theme.preset_cfg_field_gap().value();

    // Kind. 후보는 저장본 kind 기준으로 만든다 — plugin 이 꺼져 catalog 에 없는 원래
    // kind 로도 되돌아갈 수 있어야 한다.
    field_label(ui, theme, t("preset.edit.kind"));
    let mut candidates = catalog.candidates(cfg.orig.kind());
    if !candidates.iter().any(|k| k == cfg.draft.kind()) {
        candidates.push(cfg.draft.kind().to_string());
    }
    let labels: Vec<String> = candidates.iter().map(|k| catalog.label(k)).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut idx = candidates
        .iter()
        .position(|k| k == cfg.draft.kind())
        .unwrap_or(0);
    if select(ui, theme, "preset_cfg_kind", &mut idx, &label_refs, w, true) {
        cfg.switch_kind(&candidates[idx], catalog);
    }

    for field in catalog.fields(cfg.draft.kind()) {
        ui.add_space(gap);
        field_label(ui, theme, &field_label_text(&field));
        let salt = (
            "preset_cfg_field",
            cfg.draft.kind().to_string(),
            field.id.clone(),
        );
        ui.push_id(salt, |ui| {
            let mut buf = cfg.draft.value(&field.target);
            let placeholder = field.placeholder_key.as_deref().map(t).unwrap_or("");
            let browse = matches!(
                field.input,
                PresetFieldInput::FilePath | PresetFieldInput::Dir
            );
            let (resp, picked) = if browse {
                ui.allocate_ui_with_layout(
                    egui::vec2(w, theme.input_height().value()),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                        let clicked = Button::new(t("preset.field.browse"))
                            .variant(ButtonVariant::Secondary)
                            .leading_icon(&|ui, rect, c| {
                                icons::FOLDER_OPEN.image(rect.width(), c).paint_at(ui, rect);
                            })
                            .show(ui, theme)
                            .clicked();
                        let resp = Input::new()
                            .mono(true)
                            .width(ui.available_width())
                            .placeholder(placeholder)
                            .show(ui, theme, &mut buf);
                        (resp, clicked)
                    },
                )
                .inner
            } else {
                let resp = Input::new()
                    .mono(true)
                    .width(w)
                    .placeholder(placeholder)
                    .show(ui, theme, &mut buf);
                (resp, false)
            };
            if resp.changed() {
                cfg.draft.set_value(&field.target, buf);
            }
            if picked {
                cfg.request_browse(&field);
            }
            if resp.lost_focus() && enter {
                *enter_in_input = true;
            }
        });
    }
}

fn draw_footer(ui: &mut egui::Ui, theme: &Theme, footer: egui::Rect, dirty: bool) -> (bool, bool) {
    let inner = footer.shrink2(egui::vec2(theme.preset_cfg_footer_padding_x().value(), 0.0));
    let mut fui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    fui.set_clip_rect(footer);
    fui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let ok = Button::new(t("button.ok"))
        .variant(ButtonVariant::Primary)
        .enabled(dirty)
        .show(&mut fui, theme)
        .clicked();
    let cancel = Button::new(t("button.cancel"))
        .variant(ButtonVariant::Ghost)
        .show(&mut fui, theme)
        .clicked();
    (ok && dirty, cancel)
}

/// 필드 라벨 텍스트 — label_key 를 번역하되 미번역(키 그대로)이면 param_key/id 로
/// 안전한 대체 표기(플러그인 lang 미로드 방어).
fn field_label_text(field: &PresetFieldSpec) -> String {
    let tr = t(&field.label_key);
    if tr != field.label_key {
        return tr.to_string();
    }
    match &field.target {
        PresetFieldTarget::Params(k) => k.clone(),
        _ => field.id.clone(),
    }
}

/// 필드 라벨 — font-label micro, uppercase, muted. 입력과의 간격은 디자인 `Field` 의 3px.
fn field_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
    ui.add_space(STRUCT_GAP_3.value());
}
