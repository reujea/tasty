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

/// 탭·pane 프리셋 적용 팝업이 작용하는 대상. 열 때 정하고 적용할 때 그대로 쓴다.
/// 팝업이 열린 채 포커스가 다른 pane 으로 옮겨져도 적용 대상은 바뀌지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresetApplyTarget {
    /// 탭 프리셋. 그 탭이 속한 pane 에 적용한다.
    Tab(u32),
    /// pane 프리셋.
    Pane(u32),
}

impl PresetApplyTarget {
    fn popup_id(self) -> crate::adapters::ui::popup::PopupId {
        match self {
            Self::Tab(_) => APPLY_TAB_POPUP_ID,
            Self::Pane(_) => APPLY_PANE_POPUP_ID,
        }
    }

    /// 대상 자체를 통째로 바꾸므로 대상이 보이는 상위 자리가 범위다. 탭은 그 탭의 pane, pane 은 그 pane 의 workspace.
    fn scope(
        self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) -> Option<crate::adapters::ui::popup::PopupScope> {
        use crate::adapters::ui::popup::PopupScope;
        match self {
            Self::Tab(_) => self.pane(engine).map(PopupScope::Pane),
            Self::Pane(pane) => engine
                .find_workspace_index_for_pane(pane)
                .and_then(|index| engine.workspace_at(index))
                .map(|ws| PopupScope::Workspace(ws.id)),
        }
    }

    /// 적용할 pane. 대상이 사라졌으면 None이다.
    fn pane(self, engine: &crate::runtime::engine_read::EngineRead<'_>) -> Option<u32> {
        match self {
            Self::Tab(tab) => engine.find_pane_for_tab(tab),
            Self::Pane(pane) => engine.has_pane(pane).then_some(pane),
        }
    }

    /// 이 대상을 쓰는 프리셋 종류.
    fn kind(self) -> PresetKind {
        match self {
            Self::Tab(_) => PresetKind::Tab,
            Self::Pane(_) => PresetKind::Pane,
        }
    }
}

/// 탭 팝업과 pane 팝업의 대상. 두 팝업이 함께 열릴 수 있어 팝업마다 따로 둔다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PresetApplyTargets {
    tab: Option<PresetApplyTarget>,
    pane: Option<PresetApplyTarget>,
}

impl PresetApplyTargets {
    /// 그 종류의 팝업이 열 때 정한 대상.
    pub(crate) fn get(&self, kind: PresetKind) -> Option<PresetApplyTarget> {
        match kind {
            PresetKind::Tab => self.tab,
            PresetKind::Pane => self.pane,
            PresetKind::Workspace => None,
        }
    }

    fn set(&mut self, target: PresetApplyTarget) {
        match target.kind() {
            PresetKind::Tab => self.tab = Some(target),
            _ => self.pane = Some(target),
        }
    }

    fn clear(&mut self, kind: PresetKind) {
        match kind {
            PresetKind::Tab => self.tab = None,
            PresetKind::Pane => self.pane = None,
            PresetKind::Workspace => {}
        }
    }
}

/// 프리셋 적용 팝업마다 고른 프리셋 이름. 탭 팝업과 pane 팝업이 함께 열릴 수 있어 종류별로 둔다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PresetPickerSelection {
    workspace: Option<String>,
    tab: Option<String>,
    pane: Option<String>,
}

impl PresetPickerSelection {
    pub(crate) fn get(&self, kind: PresetKind) -> Option<&str> {
        self.slot(kind).as_deref()
    }

    pub(crate) fn set(&mut self, kind: PresetKind, name: Option<String>) {
        *self.slot_mut(kind) = name;
    }

    pub(crate) fn clear(&mut self, kind: PresetKind) {
        self.set(kind, None);
    }

    /// 그 종류 팝업의 선택만 자기 목록에 맞춘다. 없거나 목록에 없으면 첫 항목을 고른다.
    /// 함께 열린 다른 종류 팝업의 선택은 건드리지 않는다.
    fn fit_to_list(&mut self, kind: PresetKind, names: &[String]) {
        if self
            .get(kind)
            .is_none_or(|sel| !names.iter().any(|n| n == sel))
        {
            self.set(kind, names.first().cloned());
        }
    }

    fn slot(&self, kind: PresetKind) -> &Option<String> {
        match kind {
            PresetKind::Workspace => &self.workspace,
            PresetKind::Tab => &self.tab,
            PresetKind::Pane => &self.pane,
        }
    }

    fn slot_mut(&mut self, kind: PresetKind) -> &mut Option<String> {
        match kind {
            PresetKind::Workspace => &mut self.workspace,
            PresetKind::Tab => &mut self.tab,
            PresetKind::Pane => &mut self.pane,
        }
    }
}

/// 적용할 pane. 워크스페이스 프리셋은 대상 pane 이 없다(None).
/// 탭·pane 프리셋은 열 때 정한 대상에 적용한다. 대상이 사라졌거나 팝업 종류와 대상 종류가 다르면
/// Err 로 적용하지 않는다.
fn apply_target_pane(
    kind: PresetKind,
    target: Option<PresetApplyTarget>,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Result<Option<u32>, ()> {
    match kind {
        PresetKind::Workspace => Ok(None),
        PresetKind::Tab | PresetKind::Pane => target
            .filter(|target| target.kind() == kind)
            .and_then(|target| target.pane(engine))
            .map(Some)
            .ok_or(()),
    }
}

/// 탭 프리셋 팝업의 대상 탭이 아직 있는지. 범위(그 탭의 pane)가 남아도 탭이 닫히면 거짓이다.
/// 팝업 정리 단계가 그리기와 무관하게 매 프레임 부른다.
pub(crate) fn tab_popup_target_alive(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    let target = state.dialogs.preset_apply_targets.get(PresetKind::Tab);
    apply_target_pane(PresetKind::Tab, target, engine).is_ok()
}

/// pane 프리셋 팝업의 대상 pane 이 아직 있는지. 범위(그 pane 의 workspace)가 남아도 pane 이 닫히면 거짓이다.
pub(crate) fn pane_popup_target_alive(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    let target = state.dialogs.preset_apply_targets.get(PresetKind::Pane);
    apply_target_pane(PresetKind::Pane, target, engine).is_ok()
}

/// pane 의 탭 또는 pane 프리셋 적용 팝업을 대상이 보이는 상위 범위로 연다. 탭 프리셋은 그 pane 에서 선택된 탭이
/// 대상이고 범위는 그 pane, pane 프리셋은 범위가 그 pane 의 workspace 다. 대상을 찾지 못하면 열지 않고 warn 을 남긴다.
pub(crate) fn open_for_pane(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    kind: PresetKind,
    pane_id: u32,
    dispatch: impl FnOnce(crate::intent::UiIntent) -> crate::intent::DispatchedIntent,
) -> bool {
    let target = match kind {
        PresetKind::Pane => engine
            .has_pane(pane_id)
            .then_some(PresetApplyTarget::Pane(pane_id)),
        PresetKind::Tab => engine
            .find_pane_by_id(pane_id)
            .and_then(|pane| pane.tabs.get(state.navigation.tab_index(pane)))
            .map(|tab| PresetApplyTarget::Tab(tab.id)),
        PresetKind::Workspace => None,
    };
    let Some((target, scope)) = target.and_then(|t| t.scope(engine).map(|scope| (t, scope))) else {
        tracing::warn!("{kind:?} preset popup not opened: pane {pane_id} has no target");
        return false;
    };
    state.dialogs.preset_picker_selection.clear(kind);
    state.dialogs.preset_apply_targets.set(target);
    // 새 요청이 대상을 바꾸므로 다른 범위에 열려 있으면 새 범위로 다시 연다. 범위가 같으면 대상만 바뀐다.
    state.dispatch_intent(dispatch(crate::intent::UiIntent::ReopenPopup(
        crate::intent::OpenRequest::new(target.popup_id(), scope)
            .centered()
            .focused(),
    )));
    true
}

/// 워크스페이스 프리셋 팝업이 닫힐 때 선택과 대상 카테고리를 비운다.
pub fn on_close_apply_workspace_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    state
        .dialogs
        .preset_picker_selection
        .clear(PresetKind::Workspace);
    state.dialogs.preset_apply_target_category = None;
}

/// 탭 프리셋 팝업이 닫힐 때 그 팝업의 선택과 대상만 비운다. pane 팝업의 선택과 대상은 그대로 둔다.
pub fn on_close_apply_tab_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    state.dialogs.preset_picker_selection.clear(PresetKind::Tab);
    state.dialogs.preset_apply_targets.clear(PresetKind::Tab);
}

/// pane 프리셋 팝업이 닫힐 때 그 팝업의 선택과 대상만 비운다. 탭 팝업의 선택과 대상은 그대로 둔다.
pub fn on_close_apply_pane_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    state
        .dialogs
        .preset_picker_selection
        .clear(PresetKind::Pane);
    state.dialogs.preset_apply_targets.clear(PresetKind::Pane);
}

pub fn draw_apply_workspace_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    draw_apply_popup(ui, state, engine, PresetKind::Workspace)
}

pub fn draw_apply_tab_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    draw_apply_popup(ui, state, engine, PresetKind::Tab)
}

pub fn draw_apply_pane_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    draw_apply_popup(ui, state, engine, PresetKind::Pane)
}

fn draw_apply_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    kind: PresetKind,
) -> PopupAction {
    let th = theme::theme();
    let names = state.preset_store.list(kind);

    state
        .dialogs
        .preset_picker_selection
        .fit_to_list(kind, &names);

    let empty_label = t("preset.popup.empty");
    let apply_label = t("preset.popup.apply_button");
    let cancel_label = t("preset.popup.cancel_button");

    let props = ApplyPresetProps {
        theme: &th,
        empty_label,
        apply_button_label: apply_label,
        cancel_button_label: cancel_label,
        names: &names,
        selected: state.dialogs.preset_picker_selection.get(kind),
    };

    let action = draw_apply_preset_view(ui, &props);

    match action {
        ApplyPresetAction::None => PopupAction::None,
        ApplyPresetAction::Cancel => PopupAction::Close,
        ApplyPresetAction::Select(name) => {
            state.dialogs.preset_picker_selection.set(kind, Some(name));
            PopupAction::None
        }
        ApplyPresetAction::Apply(name) => {
            let category = state.dialogs.preset_apply_target_category.take();
            let Ok(target_pane_id) =
                apply_target_pane(kind, state.dialogs.preset_apply_targets.get(kind), engine)
            else {
                tracing::warn!("{kind:?} preset '{name}' not applied: its target is gone");
                return PopupAction::Close;
            };
            state.dispatch_intent(
                Intent::ApplyPreset {
                    kind,
                    name: name.clone(),
                    category,
                    target_pane_id,
                }
                .from_user_menu("preset_apply_popup"),
            );
            state.dialogs.preset_picker_selection.clear(kind);
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
    use crate::adapters::ui::popup::{PopupId, PopupScope};

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

    /// 두 pane fixture: pane 10 에 탭 101(선택)·102, pane 20 에 탭 201. 포커스는 pane 20.
    fn open_on_left_pane(
        kind: PresetKind,
    ) -> (
        crate::state::RequestContext,
        crate::runtime::engine_session::EngineSession,
        crate::intent::DispatchedIntent,
    ) {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let opened = open_for_pane(
            &mut state,
            &session.read(),
            kind,
            10,
            crate::intent::UiIntent::from_user_context_menu,
        );
        assert!(opened);
        let mut intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1);
        let intent = intents.remove(0);
        (state, session, intent)
    }

    fn opened_popup(intent: &crate::intent::DispatchedIntent) -> (PopupId, PopupScope) {
        match &intent.body {
            Intent::Ui(crate::intent::UiIntent::ReopenPopup(crate::intent::OpenRequest {
                id,
                scope,
                placement: crate::intent::PopupPlacement::ScopeCenter,
                focus: true,
                ..
            })) => (*id, scope.clone()),
            other => panic!("expected a centered popup open, got {other:?}"),
        }
    }

    #[test]
    fn the_tab_preset_popup_opens_in_the_pane_of_the_selected_tab() {
        let (state, _session, intent) = open_on_left_pane(PresetKind::Tab);
        assert_eq!(
            opened_popup(&intent),
            (APPLY_TAB_POPUP_ID, PopupScope::Pane(10))
        );
        assert_eq!(
            state.dialogs.preset_apply_targets.get(PresetKind::Tab),
            Some(PresetApplyTarget::Tab(101))
        );
    }

    #[test]
    fn the_pane_preset_popup_opens_in_the_workspace_of_its_pane() {
        let (state, _session, intent) = open_on_left_pane(PresetKind::Pane);
        assert_eq!(
            opened_popup(&intent),
            (APPLY_PANE_POPUP_ID, PopupScope::Workspace(1))
        );
        assert_eq!(
            state.dialogs.preset_apply_targets.get(PresetKind::Pane),
            Some(PresetApplyTarget::Pane(10))
        );
    }

    #[test]
    fn a_pane_without_a_target_does_not_open() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        for kind in [PresetKind::Tab, PresetKind::Pane] {
            let opened = open_for_pane(
                &mut state,
                &session.read(),
                kind,
                999,
                crate::intent::UiIntent::from_user_context_menu,
            );
            assert!(!opened, "{kind:?}");
        }
        assert!(state.take_pending_intents().is_empty());
        assert_eq!(
            state.dialogs.preset_apply_targets,
            PresetApplyTargets::default()
        );
    }

    /// 팝업을 연 뒤 포커스가 다른 pane 에 있어도 열 때 정한 pane 에 적용한다.
    #[test]
    fn apply_targets_the_pane_chosen_at_open_not_the_focused_pane() {
        for kind in [PresetKind::Tab, PresetKind::Pane] {
            let (mut state, session, _) = open_on_left_pane(kind);
            let read = session.read();
            assert_eq!(state.focused_pane_id(&read), 20);
            let target_pane_id =
                apply_target_pane(kind, state.dialogs.preset_apply_targets.get(kind), &read)
                    .unwrap();
            assert_eq!(target_pane_id, Some(10), "{kind:?}");

            let dispatched = Intent::ApplyPreset {
                kind,
                name: "p".into(),
                category: None,
                target_pane_id,
            }
            .from_user_menu("preset_apply_popup");
            drop(read);
            let resolved = crate::app::creation_intent::resolve(
                &state,
                &session.as_ref(),
                &dispatched.body,
                &dispatched.origin,
            )
            .unwrap();
            match resolved {
                Some(crate::app::command::DomainIntent::ApplyPreset { target_pane_id, .. }) => {
                    assert_eq!(target_pane_id, Some(10), "{kind:?}")
                }
                other => panic!("{other:?}"),
            }
            state.dialogs.preset_apply_targets.clear(kind);
            assert!(apply_target_pane(kind, None, &session.read()).is_err());
        }
    }

    #[test]
    fn a_gone_target_is_not_applied() {
        let (_state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        assert!(
            apply_target_pane(PresetKind::Tab, Some(PresetApplyTarget::Tab(999)), &read).is_err()
        );
        assert!(
            apply_target_pane(PresetKind::Pane, Some(PresetApplyTarget::Pane(999)), &read).is_err()
        );
        assert_eq!(
            apply_target_pane(PresetKind::Workspace, None, &read),
            Ok(None)
        );
    }

    /// 열린 팝업에 다른 대상의 요청이 오면 범위와 대상이 모두 새 요청을 따른다.
    /// 탭 팝업은 다른 pane 범위로 옮기고, pane 팝업은 같은 workspace 범위에서 대상만 바꾼다.
    #[test]
    fn a_second_request_for_another_target_moves_scope_and_target_together() {
        for (kind, popup, second_pane, scope, target) in [
            (
                PresetKind::Tab,
                APPLY_TAB_POPUP_ID,
                20,
                PopupScope::Pane(20),
                PresetApplyTarget::Tab(201),
            ),
            (
                PresetKind::Pane,
                APPLY_PANE_POPUP_ID,
                20,
                PopupScope::Workspace(1),
                PresetApplyTarget::Pane(20),
            ),
        ] {
            let (mut state, mut session) = crate::state::tests::two_pane_fixture();
            for pane in [10, second_pane] {
                let read = session.read();
                assert!(open_for_pane(
                    &mut state,
                    &read,
                    kind,
                    pane,
                    crate::intent::UiIntent::from_user_context_menu,
                ));
                drop(read);
                crate::intent::popup::apply_pending(&mut state, &mut session);
            }
            assert_eq!(state.popups.open_scope(popup), Some(&scope), "{kind:?}");
            assert_eq!(
                state.dialogs.preset_apply_targets.get(kind),
                Some(target),
                "{kind:?}"
            );
            assert_eq!(
                apply_target_pane(
                    kind,
                    state.dialogs.preset_apply_targets.get(kind),
                    &session.read()
                ),
                Ok(Some(20)),
                "{kind:?}"
            );
            assert!(state.popups.take_closed_ids().is_empty(), "{kind:?}");
        }
    }

    /// 탭 팝업이 열린 채 pane 팝업을 열고 닫아도 탭 팝업의 대상은 그대로다.
    #[test]
    fn closing_the_pane_popup_keeps_the_tab_popup_target() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        assert!(open_for_pane(
            &mut state,
            &read,
            PresetKind::Tab,
            10,
            crate::intent::UiIntent::from_user_context_menu,
        ));
        assert!(open_for_pane(
            &mut state,
            &read,
            PresetKind::Pane,
            20,
            crate::intent::UiIntent::from_user_context_menu,
        ));
        on_close_apply_pane_popup(
            &egui::Context::default(),
            &mut state,
            &read,
            &crate::adapters::ui::popup::ClosedPopup::test("test"),
        );
        assert_eq!(
            state.dialogs.preset_apply_targets.get(PresetKind::Tab),
            Some(PresetApplyTarget::Tab(101))
        );
        assert_eq!(
            state.dialogs.preset_apply_targets.get(PresetKind::Pane),
            None
        );
        assert_eq!(
            apply_target_pane(
                PresetKind::Tab,
                state.dialogs.preset_apply_targets.get(PresetKind::Tab),
                &read
            ),
            Ok(Some(10))
        );
    }

    /// 탭 팝업과 pane 팝업이 함께 그려져도 탭 팝업의 선택은 pane 목록에 맞춰지지 않는다.
    #[test]
    fn drawing_the_pane_popup_keeps_the_tab_popup_selection() {
        let mut selection = PresetPickerSelection::default();
        let tabs = vec!["alpha".to_string(), "beta".to_string()];
        let panes = vec!["solo".to_string()];
        selection.set(PresetKind::Tab, Some("beta".into()));
        for _ in 0..2 {
            selection.fit_to_list(PresetKind::Tab, &tabs);
            selection.fit_to_list(PresetKind::Pane, &panes);
        }
        assert_eq!(selection.get(PresetKind::Tab), Some("beta"));
        assert_eq!(selection.get(PresetKind::Pane), Some("solo"));
    }

    /// pane 팝업을 열고 닫아도 탭 팝업의 선택은 그대로다.
    #[test]
    fn opening_and_closing_the_pane_popup_keeps_the_tab_popup_selection() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        assert!(open_for_pane(
            &mut state,
            &read,
            PresetKind::Tab,
            10,
            crate::intent::UiIntent::from_user_context_menu,
        ));
        state
            .dialogs
            .preset_picker_selection
            .set(PresetKind::Tab, Some("beta".into()));
        assert!(open_for_pane(
            &mut state,
            &read,
            PresetKind::Pane,
            20,
            crate::intent::UiIntent::from_user_context_menu,
        ));
        on_close_apply_pane_popup(
            &egui::Context::default(),
            &mut state,
            &read,
            &crate::adapters::ui::popup::ClosedPopup::test("test"),
        );
        assert_eq!(
            state.dialogs.preset_picker_selection.get(PresetKind::Tab),
            Some("beta")
        );
    }

    /// 팝업 종류와 대상 종류가 다르면 적용하지 않는다.
    #[test]
    fn a_target_of_the_other_kind_is_not_applied() {
        let (_state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        assert!(
            apply_target_pane(PresetKind::Tab, Some(PresetApplyTarget::Pane(20)), &read).is_err()
        );
        assert!(
            apply_target_pane(PresetKind::Pane, Some(PresetApplyTarget::Tab(101)), &read).is_err()
        );
    }

    #[test]
    fn view_renders_with_names_without_panic() {
        let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let action = run_with_input(egui::RawInput::default(), &names, Some("a"));
        assert_eq!(action, ApplyPresetAction::None);
    }
}
