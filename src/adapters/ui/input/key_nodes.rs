//! 메인 창 키 경로의 노드 목록과 단계 4 수신자 판정. 사건 시점 상태(포커스 노드, 팝업 선언·표시 상태, 노드
//! 키맵, surface 내용)로 출발 노드에서 engine 앞까지의 목록을 만들고 `route_key`에 넘긴다. 메인 창 view(단계 4),
//! App plugin 판정 게이트, 시험 대역이 같은 함수를 부른다. 규칙: `docs/architecture/input-layer.md`의 "키 경로와
//! 수신자".

use winit::keyboard::{Key, ModifiersState};

use super::edit_key::EditBindings;
use super::key_class::{KeyClass, KeyEventValue, classify_key};
use super::key_level::{FieldLevel, KEYBINDING_LEVELS, KeyLevel, NumericAxis, numeric_axis_level};
use super::key_route::{
    AttachedPopup, KeyNode, KeyReceiver, PopupFold, PopupKeyNode, RouteKey, Routed, ScopeContent,
    ScopeKeyNode, route_key, start_node_ends_key, text_edit_builtin,
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

/// 키 사건 하나에 한 번 정한 판정. 메인 창이 단계 3~5 에서 정해 보관하고(`MainView::settle_key_event`), 창 view 의
/// 단계는 `decision` 을, App plugin 단계는 `plugin` 을 읽는다. 편집 보류 뒤 재생하는 사건은 재생 시점에 다시 정한다
/// (보류 때 값을 쓰지 않는다. 재생은 그 입구 처음부터 지난다).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SettledKey {
    pub(crate) decision: KeyDecision,
    pub(crate) plugin: PluginKeyStep,
}

/// App plugin 단계가 읽는 사건 시점 값: 게이트가 열렸는지와 plugin 명령 후보를 거를 경로 level(`None` 이면 거르지
/// 않는다, `key_path::focus_path_levels`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PluginKeyStep {
    pub(crate) gate_open: bool,
    pub(crate) levels: Option<super::key_level::PathLevels>,
}

/// 메인 창 키 사건 하나의 단계 4 와 App plugin 게이트. `replay_edit`은 재생하는 원래 키에 보류 때 붙은 편집 표시가
/// 있는지, `egui_surface`는 포커스 surface 가 egui 로 입력을 받는 host 위젯인지다(egui 적재 게이트와 같은 값,
/// [`focused_surface_takes_egui`]). 노드 목록은 한 번만 만든다.
///
/// plugin 게이트: 설정 열기 요청 래치 중 누름이면 닫힌다(래치 노드가 끝낸다). 경로 키면 포커스 노드에서 시작한 경로가
/// 출발 노드에서 그 키를 끝내지 않을 때 열고, 지금 경로로 남는 사건이면 지금 게이트(`window_keys_to_overlay`)다.
/// bare Esc 는 그 경로의 수신자(`route_escape`)가 단축키 단계·받은 surface·버림이면 연다(출발 노드 안 수신자면
/// 닫힌다). plugin 명령 level 은 매처가 거른다.
pub(crate) fn settle_key(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
    replay_edit: bool,
    egui_surface: bool,
) -> SettledKey {
    let levels = crate::adapters::ui::input::key_path::focus_path_levels(state, engine);
    let settled = |decision, gate_open| SettledKey {
        decision,
        plugin: PluginKeyStep { gate_open, levels },
    };
    if settings_latch_ends(state, engine, value) {
        return settled(settings_latch_decision(), false);
    }
    let Some((key, path, lookup)) =
        window_path_key(state, engine, value, replay_edit, egui_surface)
    else {
        return settled(KeyDecision::Current, !state.window_keys_to_overlay());
    };
    let edits = EditBindings::from_settings(&engine.settings.keybindings).with_judged(replay_edit);
    let decision = route_decision(state, engine, key, &path, &lookup, value.mods, &edits);
    let gate_open = if key.bare_escape() {
        matches!(
            decision,
            KeyDecision::Path(Routed {
                receiver: KeyReceiver::Shortcut { .. }
                    | KeyReceiver::Surface(_)
                    | KeyReceiver::Drop,
                ..
            })
        )
    } else {
        !start_node_ends_key(&path, key, &edits)
    };
    settled(decision, gate_open)
}

/// 메인 창 키 사건 하나의 단계 4([`settle_key`]의 수신자). 제품은 [`settle_key`] 하나를 쓰고, 시험이 수신자만 읽는다.
#[cfg(test)]
pub(crate) fn window_key_decision(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
    replay_edit: bool,
    egui_surface: bool,
) -> KeyDecision {
    settle_key(state, engine, value, replay_edit, egui_surface).decision
}

/// 이 누름이 설정 열기 요청 래치 노드(출발 노드 `FocusNode::SettingsRequest`)에서 끝나는지. 뗌은 지금 경로다.
fn settings_latch_ends(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
) -> bool {
    value.pressed
        && crate::adapters::ui::input::key_path::key_start_node(state, engine)
            == FocusNode::SettingsRequest
}

/// 경로 키의 단계 4 수신자. 지금 경로로 남는 사건은 `None` 이다. 단계 6 편집 대상과 단계 8 붙여넣기 대상이 읽는다.
pub(crate) fn path_receiver(decision: KeyDecision) -> Option<KeyReceiver> {
    match decision {
        KeyDecision::Path(routed) => Some(routed.receiver),
        KeyDecision::Current => None,
    }
}

/// 단계 4 수신자가 설정 열기 요청 래치 노드인지. 단계 6(편집 판정)·단계 11(래치 끝, Esc 적용)이 같은 판정 값으로
/// 읽는다.
pub(crate) fn settings_latch_receives(decision: KeyDecision) -> bool {
    matches!(
        decision,
        KeyDecision::Path(Routed {
            receiver: KeyReceiver::SettingsRequest,
            ..
        })
    )
}

/// 설정 열기 요청 래치 중 누름의 단계 4. 출발 노드(`FocusNode::SettingsRequest`)가 그 키를 끝낸다. 경로에 다른
/// 노드가 없어 level 은 engine 뿐이지만 단축키 단계로 가지 않으므로 후보를 보지 않는다.
fn settings_latch_decision() -> KeyDecision {
    KeyDecision::Path(Routed {
        receiver: KeyReceiver::SettingsRequest,
        levels: super::key_level::PathLevels::engine_only(),
        escape: false,
    })
}

/// egui 사건으로 들어온 bare Esc 누름(debug 주입)의 단계 4. winit 키와 같은 순서로 래치 중이면 래치 노드가 받고,
/// 아니면 포커스 노드에서 시작한 경로의 Esc 수신자다(`route_escape`). 부르는 곳이 debug 주입과 시험뿐이다.
#[cfg(any(debug_assertions, test))]
pub(crate) fn bare_escape_decision(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    egui_surface: bool,
) -> KeyDecision {
    if crate::adapters::ui::input::key_path::key_start_node(state, engine)
        == FocusNode::SettingsRequest
    {
        return settings_latch_decision();
    }
    let key = RouteKey {
        key: Some(egui::Key::Escape),
        modifiers: egui::Modifiers::NONE,
        super_held: false,
    };
    let lookup = Key::Named(winit::keyboard::NamedKey::Escape);
    let mods = ModifiersState::empty();
    let path = window_key_path(state, engine, key, &lookup, mods, egui_surface, false);
    let edits = EditBindings::from_settings(&engine.settings.keybindings);
    route_decision(state, engine, key, &path, &lookup, mods, &edits)
}

/// 경로 키 하나와 그 노드 목록의 수신자.
fn route_decision(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    key: RouteKey,
    path: &[KeyNode],
    lookup: &Key,
    mods: ModifiersState,
    edits: &EditBindings,
) -> KeyDecision {
    let kb = &engine.settings.keybindings;
    let matching = matching_levels(
        kb,
        explorer_key_use(state, engine, lookup, mods),
        engine.settings.general.workspace_categories_enabled,
        lookup,
        mods,
    );
    KeyDecision::Path(route_key(path, key, edits, &matching))
}

/// 시험: 창이 지금 상태에서 정한 App plugin 게이트([`settle_key`]).
#[cfg(test)]
pub(crate) fn plugin_key_gate_open(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    value: &KeyEventValue<'_>,
) -> bool {
    let egui_surface = focused_surface_takes_egui(state, engine);
    settle_key(state, engine, value, false, egui_surface)
        .plugin
        .gate_open
}

/// 포커스 surface 가 egui 로 입력을 받는 host 위젯(registry `consumes_egui_input`)인지.
pub(crate) fn focused_surface_takes_egui(state: &MainViewState, engine: &EngineRead<'_>) -> bool {
    state
        .focused_surface_type(engine)
        .kind_capability(engine, |d| d.consumes_egui_input)
}

/// native WebView 가 넘긴 키의 분류. 전달 키는 조회 키와 수식키만 가지므로 그 조회 키를 논리 키로 넣어 winit 입구와
/// 같은 판정(`classify_key`)을 한다. 경로 키가 아닌 것(수식키 단독 누름)은 지금 게이트를 쓴다. 클립보드 조합과
/// 편집 바인딩 키는 경로 키다. bare Esc 는 경로 키라 받은 surface 덮임 게이트를 쓴다. 설정 열기 요청 래치는 창 전체 차단이 먼저 본다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardedKey {
    Path,
    Current,
}

/// 전달 키 하나를 한 번 분류한다. 바깥 게이트와 plugin 게이트가 이 값을 함께 쓴다.
pub(crate) fn forwarded_key_class(key: &Key, mods: ModifiersState) -> ForwardedKey {
    let value = KeyEventValue {
        logical: key,
        physical: winit::keyboard::PhysicalKey::Unidentified(
            winit::keyboard::NativeKeyCode::Unidentified,
        ),
        pressed: true,
        mods,
    };
    match classify_key(&value) {
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
    let lookup = tasty_key_match::shortcut_lookup_key(value.logical, &value.physical, value.mods);
    match classify_key(value) {
        KeyClass::Current(_) => None,
        KeyClass::Path(key) => {
            let path = window_key_path(
                state,
                engine,
                key,
                &lookup,
                value.mods,
                egui_surface,
                replay_edit,
            );
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
    replay_edit: bool,
) -> Vec<KeyNode> {
    use crate::adapters::ui::input::key_path::{key_start_node, start_scope_levels};
    let kb = &engine.settings.keybindings;
    let notice = crate::adapters::ui::restore_notice::focused_notice(&state.popups);
    let start = key_start_node(state, engine);
    let mut scopes = start_scope_levels(state, engine, start)
        .into_iter()
        .map(|level| {
            KeyNode::Scope(ScopeKeyNode {
                level,
                content: ScopeContent::None,
                attached: attached_popups(state, engine, level),
            })
        })
        .collect::<Vec<_>>();
    let first = match (start, notice) {
        (FocusNode::SettingsRequest, _) => return Vec::new(),
        (FocusNode::Stage, _) => KeyNode::Stage {
            exit: crate::shortcuts::matches_any_binding(&kb.fullscreen_stage_exit, lookup, mods),
        },
        (FocusNode::Popup(node), _) => match popup_key_node(state, node, key, kb) {
            Some(popup) => KeyNode::Popup(popup),
            None => return Vec::new(),
        },
        (_, Some(iid)) => KeyNode::Notice(iid),
        (FocusNode::Tutorial, None) => KeyNode::Tutorial,
        (FocusNode::HtmlAddress(sid), None) => KeyNode::HtmlAddress {
            sid,
            keymap: crate::view::main::address_keymap_receives(kb, lookup, mods),
        },
        (FocusNode::Surface(Some(sid)), None) => {
            if let Some(KeyNode::Scope(surface)) = scopes.first_mut() {
                surface.content =
                    surface_content(state, engine, sid, key, egui_surface, replay_edit);
            }
            return scopes;
        }
        (FocusNode::Surface(None), None) => return scopes,
    };
    scopes.insert(0, first);
    scopes
}

/// 이 level 범위 노드에 붙은 팝업(z 최상단 먼저): 그 level 범위에 놓인 보이는 비덮음 host 인스턴스 가운데 범위가
/// 명령 대상의 같은 level 노드인 것. 경로의 범위 노드가 명령 대상의 조상이므로(`key_path::popup_scope_levels`) 같은
/// 대상으로 고른다.
fn attached_popups(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    level: KeyLevel,
) -> Vec<AttachedPopup> {
    use crate::adapters::ui::input::key_path::scope_level;
    use crate::adapters::ui::popup::route::{path_of_command_target, path_of_scope};
    let target = path_of_command_target(state, engine);
    let mut found = state
        .popups
        .attached_nodes()
        .filter(|(_, scope, _, _)| {
            scope_level(scope) == level && path_of_scope(engine, scope).same_at(&target, level)
        })
        .map(|(iid, _, z, escape)| (z, iid, escape))
        .collect::<Vec<_>>();
    found.sort_by_key(|(z, _, _)| std::cmp::Reverse(*z));
    found
        .into_iter()
        .map(|(_, iid, escape)| AttachedPopup {
            node: PopupNode::Host(iid),
            escape,
        })
        .collect()
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
            Some(host_popup_key_node(
                p,
                key,
                Some(kb),
                place.expanded,
                place.covers,
            ))
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

/// host 팝업 인스턴스의 경로 노드. `expanded`·`covers` 는 사건 시점 표시 상태다. `kb` 가 없는 창(별도 창)에는 find
/// 키맵을 쓰는 팝업(검색 띠)이 없다.
fn host_popup_key_node(
    p: &crate::adapters::ui::popup::PopupState,
    key: RouteKey,
    kb: Option<&KeybindingSettings>,
    expanded: bool,
    covers: bool,
) -> PopupKeyNode {
    let fold = if expanded {
        PopupFold::Expanded
    } else if p.collapsed() {
        PopupFold::Collapsed
    } else {
        PopupFold::Fits
    };
    PopupKeyNode {
        node: PopupNode::Host(p.instance),
        keyboard: p.keyboard(),
        escape: p.escape_decl(),
        fold,
        keymap: host_keymap_receives(p, key, kb),
        covers,
    }
}

/// 별도 창(설정·Preset·Plugins) 키 하나의 단계 4 수신자. 노드 목록은 사건 시점 노드가 팝업이면 그 팝업(`popup`), 그
/// 위에 창 범위 노드(창 위젯과 창 자체 처리)다. 메인 창과 같은 `route_key` 가 정한다: 팝업이 쓰지 않는 키와 Esc 선언이
/// 넘긴 Esc 는 창 노드가 받는다. `own` 은 이 키가 창 자체 처리의 바인딩(설정 창의 toggle_settings, engine level)에
/// 맞는지다. 별도 창은 편집 바인딩 없이 팝업을 그린다(`EditBindings::default`).
pub(crate) fn separate_window_receiver(
    popup: Option<&crate::adapters::ui::popup::PopupState>,
    key: RouteKey,
    own: bool,
) -> KeyReceiver {
    let mut path = Vec::with_capacity(2);
    path.extend(
        popup.map(|p| KeyNode::Popup(host_popup_key_node(p, key, None, p.expanded(), p.covers()))),
    );
    path.push(KeyNode::Scope(ScopeKeyNode {
        level: KeyLevel::Window,
        content: ScopeContent::None,
        attached: Vec::new(),
    }));
    let matching: &[KeyLevel] = if own { &[KeyLevel::Engine] } else { &[] };
    route_key(&path, key, &EditBindings::default(), matching).receiver
}

/// host 팝업의 활성 노드 키맵에 이 키가 맞는지. 사건 시점에 정해지는 조건(폴더 모드, find 바인딩)만 본다.
fn host_keymap_receives(
    p: &crate::adapters::ui::popup::PopupState,
    key: RouteKey,
    kb: Option<&KeybindingSettings>,
) -> bool {
    let Some(egui_key) = key.key else {
        return false;
    };
    if p.id == crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID {
        return kb.is_some_and(|kb| {
            crate::adapters::ui::search_bar::find_keymap_receives(
                &kb.find,
                egui_key,
                key.modifiers,
                key.super_held,
            )
        });
    }
    if p.id == crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID {
        let folder = crate::adapters::ui::popup::file_picker::instances::picker_of(&p.payload)
            .is_some_and(|data| data.folder);
        return folder
            && crate::adapters::ui::popup::file_picker::folder::folder_keymap_receives(
                key.modifiers,
                egui_key,
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
    replay_edit: bool,
) -> ScopeContent {
    if !egui {
        return ScopeContent::Yielding(sid);
    }
    let typing = crate::shortcuts::focused_explorer_typing(state, engine);
    let edits = EditBindings::from_settings(&engine.settings.keybindings).with_judged(replay_edit);
    let takes = typing
        && (key.consumed_by(PopupKeyboard::Text, &edits)
            || key.key.is_some_and(|k| text_edit_builtin(k, key.modifiers)));
    ScopeContent::Egui { sid, takes }
}

/// 사건 시점 탐색기 키 사용: 탐색기가 포커스인지, 이 키로 주소 확인을 버리는지, 선택 해제 바인딩을 쓰는지. 판정은
/// 실행부가 거절하는 조건과 같은 함수다(`shortcuts::explorer_cancels_address_check`·`explorer_takes_clear_selection`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExplorerKeyUse {
    pub(crate) focused: bool,
    pub(crate) cancels_address_check: bool,
    pub(crate) clears_selection: bool,
}

fn explorer_key_use(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    key: &Key,
    mods: ModifiersState,
) -> ExplorerKeyUse {
    if crate::shortcuts::focused_explorer_surface_id(state, engine).is_none() {
        return ExplorerKeyUse::default();
    }
    ExplorerKeyUse {
        focused: true,
        cancels_address_check: crate::shortcuts::explorer_cancels_address_check(
            state, engine, key, mods,
        ),
        clears_selection: crate::shortcuts::explorer_takes_clear_selection(state, engine),
    }
}

/// 이 키에 맞는 host 단축키 후보의 level(경로 필터 전). 메인 창 단축키 단계가 부르지 않는 필드(설정 창 코드
/// 영역 키, 무대 종료 키는 무대 노드가 본다)와, 탐색기가 포커스가 아닐 때의 탐색기 키는 후보가 아니다. 탐색기가 쓰지
/// 않는 선택 해제는 후보가 아니고, 주소 확인을 버리는 Esc(바인딩이 아닌 고정 키)는 surface level 후보다.
pub(crate) fn matching_levels(
    kb: &KeybindingSettings,
    explorer: ExplorerKeyUse,
    categories: bool,
    key: &Key,
    mods: ModifiersState,
) -> Vec<KeyLevel> {
    let mut out = Vec::new();
    if explorer.cancels_address_check {
        out.push(KeyLevel::Surface);
    }
    for (field, level) in KEYBINDING_LEVELS {
        let FieldLevel::Level(level) = level else {
            continue;
        };
        if matches!(
            *field,
            "code_area_apply" | "code_area_cancel" | "fullscreen_stage_exit"
        ) || (field.starts_with("explorer_") && !explorer.focused)
            || (*field == "explorer_clear_selection" && !explorer.clears_selection)
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

#[cfg(test)]
#[path = "explorer_escape_tests.rs"]
mod explorer_escape_tests;
