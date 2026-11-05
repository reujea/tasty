//! 워크스페이스·탭·pane 프리셋 선택. 공용 화면에서 항목을 고르면 ApplyPreset을 보낸다.
//! 목록은 공유 PresetStore에서 읽으며 갤러리는 같은 화면 함수에 예제 데이터를 전달한다.

use tasty_presets::PresetKind;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant};

/// preset 행 라벨. DTCG primitive `font-size-12` 는 있으나 semantic role 이 없어
/// `Theme` 필드가 없다 — docs/design/systems/theme.md 대로 **이름에 primitive 임을 남긴다**.
const PRESET_ROW_LABEL_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

use crate::adapters::ui::popup::PopupAction;
use crate::i18n::t;
use crate::intent::Intent;
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;

pub const APPLY_WORKSPACE_POPUP_ID: &str = "apply_workspace_preset";
pub const APPLY_TAB_POPUP_ID: &str = "apply_tab_preset";
pub const APPLY_PANE_POPUP_ID: &str = "apply_pane_preset";

/// 프리셋 목록과 선택 상태. 목록이 비거나 선택이 없을 수 있다.
pub struct ApplyPresetProps<'a> {
    pub theme: &'a Theme,
    pub empty_label: &'a str,
    pub apply_button_label: &'a str,
    pub cancel_button_label: &'a str,
    pub names: &'a [String],
    pub selected: Option<&'a str>,
}

/// 선택·적용·취소 요청. 실제 상태 변경은 호출부에서 처리한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyPresetAction {
    None,
    Cancel,
    Select(String),
    Apply(String),
}

/// 팝업이 닫힐 때 선택과 대상 카테고리를 비운다. 세 종류 팝업이 공유한다.
pub fn on_close_apply_preset_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.dialogs.preset_picker_selected = None;
    state.dialogs.preset_apply_target_category = None;
}

pub fn draw_apply_workspace_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    draw_apply_popup(ui, state, engine, PresetKind::Workspace)
}

pub fn draw_apply_tab_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    draw_apply_popup(ui, state, engine, PresetKind::Tab)
}

pub fn draw_apply_pane_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    draw_apply_popup(ui, state, engine, PresetKind::Pane)
}

fn draw_apply_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    kind: PresetKind,
) -> PopupAction {
    let th = theme::theme();
    let names = state.preset_store.list(kind);

    if state.dialogs.preset_picker_selected.is_none() {
        if let Some(first) = names.first() {
            state.dialogs.preset_picker_selected = Some(first.clone());
        }
    } else if let Some(sel) = state.dialogs.preset_picker_selected.as_deref()
        && !names.iter().any(|n| n == sel)
    {
        state.dialogs.preset_picker_selected = names.first().cloned();
    }

    let empty_label = t("preset.popup.empty");
    let apply_label = t("preset.popup.apply_button");
    let cancel_label = t("preset.popup.cancel_button");

    let props = ApplyPresetProps {
        theme: &th,
        empty_label,
        apply_button_label: apply_label,
        cancel_button_label: cancel_label,
        names: &names,
        selected: state.dialogs.preset_picker_selected.as_deref(),
    };

    let action = draw_apply_preset_view(ui, &props);

    match action {
        ApplyPresetAction::None => PopupAction::None,
        ApplyPresetAction::Cancel => PopupAction::Close,
        ApplyPresetAction::Select(name) => {
            state.dialogs.preset_picker_selected = Some(name);
            PopupAction::None
        }
        ApplyPresetAction::Apply(name) => {
            let category = state.dialogs.preset_apply_target_category.take();
            state.dispatch_intent(
                Intent::ApplyPreset {
                    kind,
                    name: name.clone(),
                    category,
                }
                .from_user_menu("preset_apply_popup"),
            );
            state.dialogs.preset_picker_selected = None;
            PopupAction::Close
        }
    }
}

/// 앱 상태 없이 프리셋 목록과 적용·취소 버튼을 그린다.
pub fn draw_apply_preset_view(
    ui: &mut egui::Ui,
    props: &ApplyPresetProps<'_>,
) -> ApplyPresetAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return ApplyPresetAction::Cancel;
    }

    let th = props.theme;
    let names = props.names;

    let cur_index = props
        .selected
        .and_then(|s| names.iter().position(|n| n == s));
    let up = ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowUp));
    let down = ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowDown));
    let arrow_index = if (up || down) && !names.is_empty() {
        let cur = cur_index.unwrap_or(0);
        Some(if up {
            if cur == 0 { names.len() - 1 } else { cur - 1 }
        } else {
            (cur + 1) % names.len()
        })
    } else {
        None
    };
    let effective_index = arrow_index.or(cur_index);
    let effective_selected: Option<&str> =
        effective_index.and_then(|i| names.get(i).map(|s| s.as_str()));

    let enter_pressed = ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));

    let mut apply_clicked = false;
    let mut cancel_clicked = false;
    let mut clicked_name: Option<String> = None;
    let mut double_clicked_name: Option<String> = None;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();

        if names.is_empty() {
            ui.label(
                egui::RichText::new(props.empty_label)
                    .color(th.text_muted_raised())
                    .italics(),
            );
        } else {
            egui::ScrollArea::vertical()
                .max_height(th.autocomplete_max_height().value())
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    for name in names {
                        let is_selected = effective_selected == Some(name.as_str());
                        let full_width = ui.available_width();
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(full_width, 22.0),
                            egui::Sense::click(),
                        );
                        if is_selected {
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                th.active_overlay.to_egui_premultiplied(),
                            );
                        } else if resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                th.hover_overlay.to_egui_premultiplied(),
                            );
                        }
                        ui.painter().text(
                            egui::pos2(rect.min.x + th.spacing_sm.value(), rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            name,
                            egui::FontId::proportional(PRESET_ROW_LABEL_PRIMITIVE_12.value()),
                            if is_selected || resp.hovered() {
                                th.text_on_active().into()
                            } else {
                                th.text_muted_raised().into()
                            },
                        );
                        if resp.clicked() {
                            clicked_name = Some(name.clone());
                        }
                        if resp.double_clicked() {
                            double_clicked_name = Some(name.clone());
                        }
                    }
                });
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_apply = !names.is_empty() && effective_selected.is_some();
                if Button::new(props.apply_button_label)
                    .variant(ButtonVariant::Primary)
                    .enabled(can_apply)
                    .show(ui, th)
                    .clicked()
                {
                    apply_clicked = true;
                }
                if Button::new(props.cancel_button_label)
                    .variant(ButtonVariant::Ghost)
                    .show(ui, th)
                    .clicked()
                {
                    cancel_clicked = true;
                }
            });
        });
    });

    // Action 우선순위: Apply > Cancel > Select > None.
    if let Some(name) = double_clicked_name {
        return ApplyPresetAction::Apply(name);
    }
    if enter_pressed
        && !names.is_empty()
        && let Some(name) = effective_selected.map(|s| s.to_string())
    {
        return ApplyPresetAction::Apply(name);
    }
    if apply_clicked && let Some(name) = effective_selected.map(|s| s.to_string()) {
        return ApplyPresetAction::Apply(name);
    }
    if cancel_clicked {
        return ApplyPresetAction::Cancel;
    }
    if let Some(name) = clicked_name {
        return ApplyPresetAction::Select(name);
    }
    if let Some(i) = arrow_index
        && let Some(name) = names.get(i)
    {
        return ApplyPresetAction::Select(name.clone());
    }

    ApplyPresetAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    fn run_with_input(
        raw: egui::RawInput,
        names: &[String],
        selected: Option<&str>,
    ) -> ApplyPresetAction {
        let ctx = egui::Context::default();
        let mut out = ApplyPresetAction::None;
        let theme = test_theme();
        drop(ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let props = ApplyPresetProps {
                    theme: &theme,
                    empty_label: "No presets saved yet.",
                    apply_button_label: "Apply",
                    cancel_button_label: "Cancel",
                    names,
                    selected,
                };
                out = draw_apply_preset_view(ui, &props);
            });
        }));
        out
    }

    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn view_returns_none_when_empty_and_no_input() {
        let action = run_with_input(egui::RawInput::default(), &[], None);
        assert_eq!(action, ApplyPresetAction::None);
    }

    #[test]
    fn view_returns_cancel_on_escape() {
        let mut raw = egui::RawInput::default();
        raw.events.push(key_event(egui::Key::Escape));
        let action = run_with_input(raw, &[], None);
        assert_eq!(action, ApplyPresetAction::Cancel);
    }

    #[test]
    fn view_returns_apply_on_enter_when_selected() {
        let names = vec!["alpha".to_string(), "beta".to_string()];
        let mut raw = egui::RawInput::default();
        raw.events.push(key_event(egui::Key::Enter));
        let action = run_with_input(raw, &names, Some("alpha"));
        assert_eq!(action, ApplyPresetAction::Apply("alpha".to_string()));
    }

    #[test]
    fn view_arrow_down_moves_selection() {
        let names = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        let mut raw = egui::RawInput::default();
        raw.events.push(key_event(egui::Key::ArrowDown));
        let action = run_with_input(raw, &names, Some("alpha"));
        assert_eq!(action, ApplyPresetAction::Select("beta".to_string()));
    }

    #[test]
    fn view_arrow_up_wraps_to_last() {
        let names = vec!["alpha".to_string(), "beta".to_string()];
        let mut raw = egui::RawInput::default();
        raw.events.push(key_event(egui::Key::ArrowUp));
        let action = run_with_input(raw, &names, Some("alpha"));
        assert_eq!(action, ApplyPresetAction::Select("beta".to_string()));
    }

    #[test]
    fn view_arrow_then_enter_applies_new_selection() {
        let names = vec!["alpha".to_string(), "beta".to_string()];
        let mut raw = egui::RawInput::default();
        raw.events.push(key_event(egui::Key::ArrowDown));
        raw.events.push(key_event(egui::Key::Enter));
        let action = run_with_input(raw, &names, Some("alpha"));
        assert_eq!(action, ApplyPresetAction::Apply("beta".to_string()));
    }

    #[test]
    fn view_renders_with_names_without_panic() {
        let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let action = run_with_input(egui::RawInput::default(), &names, Some("a"));
        assert_eq!(action, ApplyPresetAction::None);
    }
}
