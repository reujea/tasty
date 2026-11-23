//! 메인 창 키 경로의 노드 목록과 단계 4 수신자 판정. 사건 시점 상태(포커스 노드, 팝업 선언·표시 상태, 노드
//! 키맵, surface 내용)로 출발 노드에서 engine 앞까지의 목록을 만들고 `route_key`에 넘긴다. 메인 창 view(단계 4),
//! App plugin 판정 게이트, 시험 대역이 같은 함수를 부른다. 규칙: `docs/architecture/input-layer.md`의 "키 경로와
//! 수신자".

use winit::keyboard::{Key, ModifiersState};

use super::edit_key::{EditBindings, edit_binding};
use super::key_class::{KeyClass, KeyEventValue, classify_key};
use super::key_level::{FieldLevel, KEYBINDING_LEVELS, KeyLevel, NumericAxis, numeric_axis_level};
use super::key_route::{
    KeyNode, PopupFold, PopupKeyNode, RouteKey, Routed, ScopeContent, ScopeKeyNode, route_key,
    start_node_ends_key, text_edit_builtin,
};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::{PopupEscape, PopupKeyboard};
use crate::runtime::engine_read::EngineRead;
use crate::settings::KeybindingSettings;
use crate::state::MainViewState;
use crate::state::focus_node::FocusNode;

/// 단계 4의 결과. 지금 경로로 남는 사건은 `Current`다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum KeyDecision {
    #[default]
    Current,
    Path(Routed),
}

/// 메인 창 키 사건 하나의 단계 4. `replay_edit`은 재생하는 원래 키에 보류 때 붙은 편집 표시가 있는지,
/// `egui_surface`는 포커스 surface 가 egui 로 입력을 받는 host 위젯인지다(egui 적재 게이트와 같은 값,
/// [`focused_surface_takes_egui`]).
pub(crate) fn window_key_decision(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
    replay_edit: bool,
    egui_surface: bool,
) -> KeyDecision {
    let Some((key, path, lookup)) =
        window_path_key(state, engine, value, replay_edit, egui_surface)
    else {
        return KeyDecision::Current;
    };
    let kb = &engine.settings.keybindings;
    let matching = matching_levels(
        kb,
        explorer_focused(state, engine),
        engine.settings.general.workspace_categories_enabled,
        &lookup,
        value.mods,
    );
    KeyDecision::Path(route_key(
        &path,
        key,
        &EditBindings::from_settings(kb),
        &matching,
    ))
}

/// App plugin 판정 게이트(winit 키). 경로 키면 포커스 노드에서 시작한 경로가 출발 노드에서 그 키를 끝내지 않을
/// 때 열고, 지금 경로로 남는 사건이면 지금 게이트(`window_keys_to_overlay`)다. plugin 명령 level 은 매처가 거른다.
pub(crate) fn plugin_key_gate_open(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
) -> bool {
    match window_path_key(
        state,
        engine,
        value,
        false,
        focused_surface_takes_egui(state, engine),
    ) {
        Some((key, path, _)) => !start_node_ends_key(
            &path,
            key,
            &EditBindings::from_settings(&engine.settings.keybindings),
        ),
        None => !state.window_keys_to_overlay(),
    }
}

/// 포커스 surface 가 egui 로 입력을 받는 host 위젯(registry `consumes_egui_input`)인지.
pub(crate) fn focused_surface_takes_egui(state: &MainViewState, engine: &EngineRead<'_>) -> bool {
    state
        .focused_surface_type(engine)
        .kind_capability(engine, |d| d.consumes_egui_input)
}

/// native WebView 가 넘긴 키의 분류. 전달 키는 조회 키와 수식키만 가지므로 그 조회 키를 논리 키로 넣어 winit 입구와
/// 같은 판정(`classify_key`)을 한다. 경로 키가 아닌 것(bare Esc, 클립보드 조합, 편집 바인딩 키, egui 키 없는 누름)은
/// 지금 게이트를 쓴다. 설정 열기 요청 래치는 창 전체 차단이 먼저 본다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardedKey {
    Path,
    Current,
}

/// 전달 키 하나를 한 번 분류한다. 바깥 게이트와 plugin 게이트가 이 값을 함께 쓴다.
pub(crate) fn forwarded_key_class(
    kb: &KeybindingSettings,
    key: &Key,
    mods: ModifiersState,
) -> ForwardedKey {
    let value = KeyEventValue {
        logical: key,
        physical: winit::keyboard::PhysicalKey::Unidentified(
            winit::keyboard::NativeKeyCode::Unidentified,
        ),
        pressed: true,
        mods,
        egui_mods: super::key_class::egui_modifiers(mods),
    };
    match classify_key(&value, false, edit_binding(kb, key, mods).is_some()) {
        KeyClass::Path(_) => ForwardedKey::Path,
        KeyClass::Current(_) => ForwardedKey::Current,
    }
}

/// 받은 surface 에서 시작한 경로가 그 키를 막는지(창 전체 차단은 빼고). 경로 키는 받은 surface 를 덮는 팝업이 있을
/// 때만 막힌다(다른 곳의 덮지 않는 팝업 포커스는 경로에 없다). 지금 경로로 남는 키는 지금 게이트
/// (`window_keys_to_overlay`)다.
pub(crate) fn received_key_blocked(
    state: &MainViewState,
    core: &crate::core::CoreState,
    class: ForwardedKey,
    sid: u32,
) -> bool {
    match class {
        ForwardedKey::Path => state.surface_is_covered(core, sid),
        ForwardedKey::Current => state.window_keys_to_overlay(),
    }
}

/// 경로 키면 그 키와 노드 목록, 단축키 조회 키.
fn window_path_key(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
    replay_edit: bool,
    egui_surface: bool,
) -> Option<(RouteKey, Vec<KeyNode>, Key)> {
    let kb = &engine.settings.keybindings;
    let lookup = tasty_key_match::shortcut_lookup_key(value.logical, &value.physical, value.mods);
    let edit = replay_edit || edit_binding(kb, &lookup, value.mods).is_some();
    match classify_key(value, state.has_settings_open_request(), edit) {
        KeyClass::Current(_) => None,
        KeyClass::Path(key) => {
            let path = window_key_path(state, engine, key, &lookup, value.mods, egui_surface);
            Some((key, path, lookup))
        }
    }
}

/// 포커스 노드에서 engine 앞까지의 노드. 범위 노드는 경로 level과 같은 값(`key_path::start_scope_levels`)으로
/// 만든다. 덮는 팝업은 덮은 범위의 부모로, 펼친 팝업과 무대는 engine 으로 잇는다.
pub(crate) fn window_key_path(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    key: RouteKey,
    lookup: &Key,
    mods: ModifiersState,
    egui_surface: bool,
) -> Vec<KeyNode> {
    use crate::adapters::ui::input::key_path::{key_start_node, start_scope_levels};
    let kb = &engine.settings.keybindings;
    let notice = crate::adapters::ui::restore_notice::has_keyboard_focus(&state.popups);
    let start = key_start_node(state, engine);
    let mut scopes = start_scope_levels(state, engine, start)
        .into_iter()
        .map(|level| {
            KeyNode::Scope(ScopeKeyNode {
                level,
                content: ScopeContent::None,
                attached: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    let first = match start {
        FocusNode::SettingsRequest => return Vec::new(),
        FocusNode::Stage => KeyNode::Stage {
            exit: crate::shortcuts::matches_any_binding(&kb.fullscreen_stage_exit, lookup, mods),
        },
        FocusNode::Popup(node) => match popup_key_node(state, node, key, kb) {
            Some(popup) => KeyNode::Popup(popup),
            None => return Vec::new(),
        },
        _ if notice => KeyNode::Notice,
        FocusNode::Tutorial => KeyNode::Tutorial,
        FocusNode::HtmlAddress(sid) => KeyNode::HtmlAddress {
            sid,
            keymap: crate::view::main::address_keymap_receives(kb, lookup, mods),
        },
        FocusNode::Surface(Some(sid)) => {
            if let Some(KeyNode::Scope(surface)) = scopes.first_mut() {
                surface.content = surface_content(state, engine, sid, key, egui_surface);
            }
            return scopes;
        }
        FocusNode::Surface(None) => return scopes,
    };
    scopes.insert(0, first);
    scopes
}

/// 팝업 노드의 선언·표시 상태와 노드 키맵 수신 판정.
fn popup_key_node(
    state: &MainViewState,
    node: PopupNode,
    key: RouteKey,
    kb: &KeybindingSettings,
) -> Option<PopupKeyNode> {
    let place = crate::adapters::ui::input::key_path::popup_place(state, node)?;
    match node {
        PopupNode::Host(iid) => {
            let p = state.popups.instance(iid)?;
            let fold = if place.expanded {
                PopupFold::Expanded
            } else if p.collapsed() {
                PopupFold::Collapsed
            } else {
                PopupFold::Fits
            };
            Some(PopupKeyNode {
                node,
                keyboard: p.keyboard(),
                escape: p.escape_decl(),
                fold,
                keymap: host_keymap_receives(p, key, kb),
                covers: place.covers,
            })
        }
        PopupNode::Plugin(id) => {
            let (keyboard, escape) = state
                .plugin_popup_key_decls
                .get(&id)
                .copied()
                .unwrap_or((PopupKeyboard::Text, PopupEscape::Close));
            Some(PopupKeyNode {
                node,
                keyboard,
                escape,
                fold: if place.expanded {
                    PopupFold::Expanded
                } else {
                    PopupFold::Fits
                },
                keymap: false,
                covers: place.covers,
            })
        }
    }
}

/// host 팝업의 활성 노드 키맵에 이 키가 맞는지. 사건 시점에 정해지는 조건(폴더 모드, find 바인딩)만 본다.
fn host_keymap_receives(
    p: &crate::adapters::ui::popup::PopupState,
    key: RouteKey,
    kb: &KeybindingSettings,
) -> bool {
    if p.id == crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID {
        return crate::adapters::ui::search_bar::find_keymap_receives(
            &kb.find,
            key.key,
            key.modifiers,
            key.super_held,
        );
    }
    if p.id == crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID {
        let folder = crate::adapters::ui::popup::file_picker::instances::picker_of(&p.payload)
            .is_some_and(|data| data.folder);
        return folder
            && crate::adapters::ui::popup::file_picker::folder::folder_keymap_receives(
                key.modifiers,
                key.key,
            );
    }
    false
}

/// 포커스 surface 내용. egui surface(explorer·dag graph)는 입력칸이 활성일 때 그 입력칸이 쓰는 키를 받는다.
/// 그 밖의 surface 는 조상 바인딩에 양보하고 남은 키를 받는다(터미널 PTY·mesh·WebView).
fn surface_content(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    sid: u32,
    key: RouteKey,
    egui: bool,
) -> ScopeContent {
    if !egui {
        return ScopeContent::Yielding(sid);
    }
    let typing = crate::shortcuts::focused_explorer_typing(state, engine);
    let edits = EditBindings::from_settings(&engine.settings.keybindings);
    let takes = typing
        && (crate::adapters::ui::popup::keys::consumes_key(
            PopupKeyboard::Text,
            key.key,
            key.modifiers,
            key.super_held,
            &edits,
        ) || text_edit_builtin(key.key, key.modifiers));
    ScopeContent::Egui { sid, takes }
}

fn explorer_focused(state: &MainViewState, engine: &EngineRead<'_>) -> bool {
    crate::shortcuts::focused_explorer_surface_id(state, engine).is_some()
}

/// 이 키에 맞는 host 단축키 후보의 level(경로 필터 전). 메인 창 단축키 단계가 부르지 않는 필드(설정 창 코드
/// 영역 키, 무대 종료 키는 무대 노드가 본다)와, 탐색기가 포커스가 아닐 때의 탐색기 키는 후보가 아니다.
pub(crate) fn matching_levels(
    kb: &KeybindingSettings,
    explorer: bool,
    categories: bool,
    key: &Key,
    mods: ModifiersState,
) -> Vec<KeyLevel> {
    let mut out = Vec::new();
    for (field, level) in KEYBINDING_LEVELS {
        let FieldLevel::Level(level) = level else {
            continue;
        };
        if matches!(
            *field,
            "code_area_apply" | "code_area_cancel" | "fullscreen_stage_exit"
        ) || (field.starts_with("explorer_") && !explorer)
        {
            continue;
        }
        let hit = if *field == "script_bindings" {
            kb.script_bindings
                .iter()
                .any(|b| tasty_key_match::matches_binding(&b.combo, key, mods))
        } else {
            kb.get_bindings(field)
                .is_some_and(|b| crate::shortcuts::matches_any_binding(b, key, mods))
        };
        if hit && !out.contains(level) {
            out.push(*level);
        }
    }
    if let Some(axis) = numeric_axis_for(kb, categories, key, mods) {
        let level = numeric_axis_level(axis);
        if !out.contains(&level) {
            out.push(level);
        }
    }
    out
}

/// 숫자 전환 단계(`handle_numeric_switch_shortcuts`)가 이 키로 전환할 축.
fn numeric_axis_for(
    kb: &KeybindingSettings,
    categories: bool,
    key: &Key,
    mods: ModifiersState,
) -> Option<NumericAxis> {
    use tasty_key_match::matches_binding;
    let individual = KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER;
    let hit = |next: &str, prev: &str, slots: &[String]| {
        matches_binding(next, key, mods)
            || matches_binding(prev, key, mods)
            || slots
                .iter()
                .any(|c| !c.is_empty() && matches_binding(c, key, mods))
    };
    if kb.tab_switch_modifier == individual
        && hit(
            kb.tab_next_key(),
            kb.tab_prev_key(),
            &kb.tab_switch_slot_keys,
        )
    {
        return Some(NumericAxis::Tab);
    }
    if kb.workspace_switch_modifier == individual
        && hit(
            kb.workspace_next_key(),
            kb.workspace_prev_key(),
            &kb.workspace_switch_slot_keys,
        )
    {
        return Some(NumericAxis::Workspace);
    }
    if kb.category_switch_modifier == individual
        && categories
        && hit(
            kb.category_next_key(),
            kb.category_prev_key(),
            &kb.category_switch_slot_keys,
        )
    {
        return Some(NumericAxis::Category);
    }
    let Key::Character(c) = key else {
        return None;
    };
    let ch = c.as_str();
    let (alt, option) = tasty_key_match::token_axes(mods);
    use crate::adapters::ui::switch_overlay::{SwitchTarget, switch_target_for};
    let (axis, next, prev, slots) =
        match switch_target_for(kb, mods.control_key(), mods.shift_key(), alt, option)? {
            SwitchTarget::Tab => (
                NumericAxis::Tab,
                kb.tab_next_key(),
                kb.tab_prev_key(),
                &kb.tab_switch_slot_keys.as_slice(),
            ),
            SwitchTarget::Workspace => (
                NumericAxis::Workspace,
                kb.workspace_next_key(),
                kb.workspace_prev_key(),
                &kb.workspace_switch_slot_keys.as_slice(),
            ),
            SwitchTarget::Category if categories => (
                NumericAxis::Category,
                kb.category_next_key(),
                kb.category_prev_key(),
                &kb.category_switch_slot_keys.as_slice(),
            ),
            SwitchTarget::Category => return None,
        };
    (ch == next || ch == prev || slots.iter().any(|k| k == ch)).then_some(axis)
}

#[cfg(test)]
#[path = "key_nodes_tests.rs"]
mod tests;
