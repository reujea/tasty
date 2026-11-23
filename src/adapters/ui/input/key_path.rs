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
pub(crate) fn scope_level(scope: &PopupScope) -> KeyLevel {
    match scope {
        PopupScope::Window => KeyLevel::Window,
        PopupScope::Workspace(_) => KeyLevel::Workspace,
        PopupScope::Pane(_) => KeyLevel::Pane,
        PopupScope::Tab(_) => KeyLevel::Tab,
        PopupScope::Surface(_) => KeyLevel::Surface,
    }
}

/// `level`과 그 위(engine 쪽) 범위 level을 경로 순서로. `inclusive`가 거짓이면 `level`은 뺀다.
pub(crate) fn levels_from(level: KeyLevel, inclusive: bool) -> Vec<KeyLevel> {
    let chain = [
        KeyLevel::Surface,
        KeyLevel::Tab,
        KeyLevel::Pane,
        KeyLevel::Workspace,
        KeyLevel::Window,
    ];
    let start = chain.iter().position(|l| *l == level).unwrap_or(0);
    chain[start..]
        .iter()
        .skip(usize::from(!inclusive))
        .copied()
        .collect()
}

/// `levels`와 engine의 level 집합.
fn path_of(levels: &[KeyLevel]) -> PathLevels {
    levels
        .iter()
        .fold(PathLevels::engine_only(), |path, l| path.with(*l))
}

#[cfg(test)]
fn from_level(level: KeyLevel, inclusive: bool) -> PathLevels {
    path_of(&levels_from(level, inclusive))
}

/// 팝업 노드가 놓인 자리: 붙은 범위, 그 범위를 덮는지(plugin 은 펼침도 덮음이다), 펼쳤는지.
pub(crate) struct PopupPlace {
    pub(crate) scope: PopupScope,
    pub(crate) covers: bool,
    pub(crate) expanded: bool,
}

/// 팝업 노드의 자리. host 인스턴스가 없으면 `None`.
pub(crate) fn popup_place(state: &MainViewState, node: PopupNode) -> Option<PopupPlace> {
    match node {
        PopupNode::Host(iid) => state.popups.instance(iid).map(|p| PopupPlace {
            scope: p.scope.clone(),
            covers: p.covers(),
            expanded: p.expanded(),
        }),
        PopupNode::Plugin(id) => {
            let scope = state
                .plugin_popup_bound_scopes
                .get(&id)
                .and_then(|(_, scope)| scope.clone())
                .unwrap_or(PopupScope::Window);
            let expanded = state.popups.plugin_expanded().contains(&id);
            let covers = expanded
                || state
                    .plugin_popup_key_candidates
                    .iter()
                    .find(|(pid, _)| *pid == id)
                    .is_some_and(|(_, z)| state.plugin_popup_covers.iter().any(|c| c.z_seq == *z));
            Some(PopupPlace {
                scope,
                covers,
                expanded,
            })
        }
    }
}

/// 팝업 노드 뒤 범위 노드의 level. 펼친 팝업은 없고(engine으로 잇는다), 덮는 팝업은 덮은 범위의 부모부터, 덮지 않는
/// 팝업은 붙은 범위부터다.
fn popup_scope_levels(state: &MainViewState, node: PopupNode) -> Vec<KeyLevel> {
    match popup_place(state, node) {
        Some(place) if !place.expanded => levels_from(scope_level(&place.scope), !place.covers),
        _ => Vec::new(),
    }
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

/// 출발 노드 `start` 뒤에 오는 범위 노드의 level(engine 쪽 순서). 경로 level 집합과 노드 목록
/// (`key_nodes::window_key_path`)이 이 값 하나로 범위 노드를 만든다.
pub(crate) fn start_scope_levels(
    state: &MainViewState,
    core: &CoreState,
    start: FocusNode,
) -> Vec<KeyLevel> {
    let notice = || crate::adapters::ui::restore_notice::has_keyboard_focus(&state.popups);
    match start {
        FocusNode::SettingsRequest | FocusNode::Stage => Vec::new(),
        FocusNode::Popup(node) => popup_scope_levels(state, node),
        _ if notice() => levels_from(KeyLevel::Window, true),
        FocusNode::Tutorial => levels_from(KeyLevel::Window, true),
        FocusNode::HtmlAddress(_) | FocusNode::Surface(Some(_)) => {
            levels_from(KeyLevel::Surface, true)
        }
        FocusNode::Surface(None) if core.workspaces().is_empty() => {
            levels_from(KeyLevel::Window, true)
        }
        // 선택된 초안은 키 입력 surface 가 없어도 명령 대상의 pane·탭·surface 가 있다.
        FocusNode::Surface(None) if state.draft_selected(core) => {
            levels_from(KeyLevel::Surface, true)
        }
        FocusNode::Surface(None) => levels_from(KeyLevel::Workspace, true),
    }
}

/// 출발 노드에서 시작한 키 경로의 level. 설정 열기 요청 래치 중이면 `None`(거르지 않는다).
pub(crate) fn focus_path_levels(state: &MainViewState, core: &CoreState) -> Option<PathLevels> {
    match key_start_node(state, core) {
        FocusNode::SettingsRequest => None,
        start => Some(path_of(&start_scope_levels(state, core, start))),
    }
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
