//! 사건 시점의 키 경로 level 집합과 그 집합으로 거른 단축키 후보. 단축키를 실행하는 갈래(창 단축키, 더블탭,
//! plugin 단축키)는 후보를 매칭하기 전에 이 값으로 고른다. 경로는 포커스 노드에서 engine까지다: 덮는 팝업은
//! 덮은 범위와 그 아래 level을 빼고, 펼친 팝업과 무대는 engine만 남긴다. 설정 열기 요청 래치 중 키 사건은
//! 거르지 않는다(지금 경로 그대로). 규칙: `docs/architecture/input-layer.md`의 "단축키 level".

use super::key_level::{
    FieldLevel, KEYBINDING_LEVELS, KeyLevel, NumericAxis, PathLevels, shortcut_allowed,
};
use crate::adapters::ui::popup::PopupScope;
use crate::adapters::ui::popup::focus::PopupNode;
use crate::core::CoreState;
use crate::settings::KeybindingSettings;
use crate::state::MainViewState;
use crate::state::focus_node::FocusNode;

/// 범위 노드의 level.
fn scope_level(scope: &PopupScope) -> KeyLevel {
    match scope {
        PopupScope::Window => KeyLevel::Window,
        PopupScope::Workspace(_) => KeyLevel::Workspace,
        PopupScope::Pane(_) => KeyLevel::Pane,
        PopupScope::Tab(_) => KeyLevel::Tab,
        PopupScope::Surface(_) => KeyLevel::Surface,
    }
}

/// `level`과 그 위(engine 쪽) level. `inclusive`가 거짓이면 `level`은 뺀다.
fn from_level(level: KeyLevel, inclusive: bool) -> PathLevels {
    let chain = [
        KeyLevel::Surface,
        KeyLevel::Tab,
        KeyLevel::Pane,
        KeyLevel::Workspace,
        KeyLevel::Window,
    ];
    let start = chain.iter().position(|l| *l == level).unwrap_or(0);
    let skip = usize::from(!inclusive);
    chain[start..]
        .iter()
        .skip(skip)
        .fold(PathLevels::engine_only(), |levels, l| levels.with(*l))
}

/// 팝업 노드 출발 경로의 level. 펼친 팝업은 engine만, 덮는 팝업은 덮은 범위의 부모부터, 덮지 않는 팝업은 붙은
/// 범위부터다.
fn popup_path_levels(state: &MainViewState, node: PopupNode) -> PathLevels {
    let (scope, covers, expanded) = match node {
        PopupNode::Host(iid) => match state.popups.instance(iid) {
            Some(p) => (p.scope.clone(), p.covers(), p.expanded()),
            None => return PathLevels::engine_only(),
        },
        PopupNode::Plugin(id) => {
            let scope = state
                .plugin_popup_bound_scopes
                .get(&id)
                .and_then(|(_, scope)| scope.clone())
                .unwrap_or(PopupScope::Window);
            let covers = state
                .plugin_popup_key_candidates
                .iter()
                .find(|(pid, _)| *pid == id)
                .is_some_and(|(_, z)| state.plugin_popup_covers.iter().any(|c| c.z_seq == *z));
            let expanded = state.popups.plugin_expanded().contains(&id);
            (scope, covers || expanded, expanded)
        }
    };
    if expanded {
        return PathLevels::engine_only();
    }
    from_level(scope_level(&scope), !covers)
}

/// 키 사건의 출발 노드. 포커스 노드이고, 창이 지금 받는 키를 팝업 키 주인에 묶었으면(입력 시점 결속) 그 팝업이다.
/// 결속은 그 주인의 포커스를 풀었거나 닫았어도 그 몫을 그리기 전까지 남는다(`keyboard_overlay_open_besides_html_address`
/// 와 같은 규칙). 설정 열기 요청 래치가 먼저다.
pub(crate) fn key_start_node(state: &MainViewState, core: &CoreState) -> FocusNode {
    let node = state.focus_node(core);
    if node == FocusNode::SettingsRequest {
        return node;
    }
    match state.popups.bound_input_key_owner() {
        Some(owner) => FocusNode::Popup(owner),
        None => node,
    }
}

/// 출발 노드에서 시작한 키 경로의 level. 설정 열기 요청 래치 중이면 `None`(거르지 않는다).
pub(crate) fn focus_path_levels(state: &MainViewState, core: &CoreState) -> Option<PathLevels> {
    let notice = || crate::adapters::ui::restore_notice::has_keyboard_focus(&state.popups);
    Some(match key_start_node(state, core) {
        FocusNode::SettingsRequest => return None,
        FocusNode::Stage => PathLevels::engine_only(),
        FocusNode::Popup(node) => popup_path_levels(state, node),
        _ if notice() => from_level(KeyLevel::Window, true),
        FocusNode::Tutorial => from_level(KeyLevel::Window, true),
        FocusNode::HtmlAddress(_) | FocusNode::Surface(Some(_)) => PathLevels::all(),
        FocusNode::Surface(None) if core.workspaces().is_empty() => {
            from_level(KeyLevel::Window, true)
        }
        FocusNode::Surface(None) => from_level(KeyLevel::Workspace, true),
    })
}

/// 이 level의 후보를 이 경로에서 판정해도 되는지. 경로가 없으면(래치 중) 거르지 않는다.
pub(crate) fn allows(path: Option<PathLevels>, level: KeyLevel) -> bool {
    path.is_none_or(|p| shortcut_allowed(p, level))
}

/// 경로에 없는 level의 후보를 비운 단축키 설정. 매칭은 이 값으로 하므로 걸러진 후보는 매칭되지 않는다.
/// 숫자 전환 축은 개별 키 모드로 두고 키를 비워 그 축의 두 모드를 함께 끈다.
pub(crate) fn path_keybindings(
    kb: &KeybindingSettings,
    path: Option<PathLevels>,
) -> KeybindingSettings {
    let mut out = kb.clone();
    let Some(path) = path else {
        return out;
    };
    for (field, level) in KEYBINDING_LEVELS {
        let Some(l) = level.level() else { continue };
        if shortcut_allowed(path, l) {
            continue;
        }
        match level {
            FieldLevel::Numeric(axis) => disable_numeric_axis(&mut out, *axis),
            _ if *field == "script_bindings" => out.script_bindings.clear(),
            _ => {
                out.clear_field(field);
            }
        }
    }
    out
}

fn disable_numeric_axis(kb: &mut KeybindingSettings, axis: NumericAxis) {
    let individual = KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER.to_string();
    let (modifier, slots, next, prev) = match axis {
        NumericAxis::Tab => (
            &mut kb.tab_switch_modifier,
            kb.tab_switch_slot_keys.as_mut_slice(),
            &mut kb.tab_switch_next_key,
            &mut kb.tab_switch_prev_key,
        ),
        NumericAxis::Workspace => (
            &mut kb.workspace_switch_modifier,
            kb.workspace_switch_slot_keys.as_mut_slice(),
            &mut kb.workspace_switch_next_key,
            &mut kb.workspace_switch_prev_key,
        ),
        NumericAxis::Category => (
            &mut kb.category_switch_modifier,
            kb.category_switch_slot_keys.as_mut_slice(),
            &mut kb.category_switch_next_key,
            &mut kb.category_switch_prev_key,
        ),
    };
    *modifier = individual;
    slots.iter_mut().for_each(String::clear);
    next.clear();
    prev.clear();
}

#[cfg(test)]
#[path = "key_path_tests.rs"]
mod tests;
