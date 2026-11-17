//! surface 하나를 대상으로 하는 내장 액션. 대상 surface 는 호출자가 입력 시점에 정해 넘기며 여기서
//! 포커스를 다시 읽지 않는다. 입력 대상이 생성 대기 초안이면 액션을 그 초안에 걸어 두고, 생성된 surface 에서
//! 같은 함수로 실행한다.

use crate::intent::UiIntent;
use crate::runtime::engine_read::EngineRead;
use crate::runtime::surface_registry::KindCapabilities;
use crate::state::MainViewState;
use crate::state::attach_pending::deferred::{CommandSurfaceTarget, DeferredCommand};

/// `sid` 의 검색을 연다. 탐색기는 자기 Find 바, 터미널은 검색창이다. 다른 kind 는 처리하지 않고 false 다.
pub(crate) fn find_on(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    sid: u32,
    source: &'static str,
) -> bool {
    match engine.find_surface_by_id(sid).map(|s| s.kind()) {
        Some("explorer") => state.open_explorer_find(sid),
        Some("terminal") => {
            crate::adapters::ui::search_bar::open_or_focus_for(state, Some(sid), source)
        }
        _ => return false,
    }
    true
}

/// `sid` 를 변환할 kind 를 고르는 popup 을 연다.
pub(crate) fn open_convert_popup(state: &mut MainViewState, sid: u32, source: &'static str) {
    state.dialogs.convert_popup = Some(sid);
    state.dialogs.convert_popup_selected = None;
    state.dispatch_intent(
        UiIntent::OpenPopup(
            crate::intent::OpenRequest::new(
                "convert_surface",
                crate::adapters::ui::popup::PopupScope::Surface(sid),
            )
            .centered()
            .focused(),
        )
        .from_user_shortcut(source),
    );
}

/// `sid` 를 탐색기로 바꾸도록 요청한다.
pub(crate) fn convert_to_explorer(state: &mut MainViewState, sid: u32, source: &'static str) {
    state.dispatch_intent(
        crate::intent::Intent::ConvertSurface {
            surface_id: sid,
            target: crate::intent::ConvertTarget::Kind {
                cwd: None,
                kind: "explorer".to_string(),
                params: serde_json::json!({}),
            },
        }
        .from_user_shortcut(source),
    );
}

/// 탐색기 `sid` 의 미리보기·숨김 파일 토글과 Properties. `sid` 가 탐색기가 아니면 아무것도 하지 않는다.
pub(crate) fn explorer_view_on(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    sid: u32,
    action: &str,
) {
    let Some(panel) = engine.find_surface_by_id(sid).and_then(|s| s.explorer()) else {
        return;
    };
    let cwd = panel.current_root().to_path_buf();
    if action == "explorer_toggle_preview" {
        if let Some(view) = state.explorer_views.get_mut(sid) {
            view.preview.toggle();
        }
        return;
    }
    if action == "explorer_toggle_hidden" {
        if let Some(view) = state.explorer_views.get_mut(sid) {
            view.toggle_hidden();
        }
        return;
    }
    crate::adapters::ui::popup::explorer_properties::open_for_shortcut(
        state,
        engine.is_mirror_surface(sid),
        sid,
        &cwd,
    );
}

/// html `sid` 의 주소 띠에 포커스를 요청한다. 띠를 숨기는 mirror workspace 의 html leaf 는 처리하지 않고 false 다.
/// 포커스는 다음에 띠를 그릴 때 준다.
pub(crate) fn html_focus_address_on(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    sid: u32,
) -> bool {
    let html = engine
        .find_surface_by_id(sid)
        .and_then(|s| s.remote_webview())
        .is_some_and(|w| w.capabilities().address_band);
    if !html || engine.is_mirror_surface(sid) {
        return false;
    }
    state.html_address.request_focus(sid);
    true
}

/// 액션을 받는 surface kind. host 내장 kind 는 이름으로, plugin kind 는 선언한 능력으로 고른다.
#[derive(Clone, Copy)]
enum Takes {
    Any,
    Only(&'static [&'static str]),
    /// 탐색기의 주소창, 또는 주소 띠(`address_band`)를 선언한 kind 의 띠.
    Address,
}

impl Takes {
    fn accepts(self, kind: &str, capabilities: KindCapabilities) -> bool {
        match self {
            Self::Any => true,
            Self::Only(kinds) => kinds.contains(&kind),
            Self::Address => kind == "explorer" || capabilities.address_band,
        }
    }
}

/// `sid` 가 실은 kind 능력. WebView surface 가 아니면 모두 거짓이다.
fn surface_capabilities(engine: &EngineRead<'_>, sid: u32) -> KindCapabilities {
    engine
        .find_surface_by_id(sid)
        .and_then(|s| s.remote_webview())
        .map(|w| w.capabilities())
        .unwrap_or_default()
}

/// 등록된 `kind` 의 능력. 초안처럼 아직 surface 가 없을 때 쓴다. 등록되지 않았으면 모두 거짓이다.
fn kind_capabilities(engine: &EngineRead<'_>, kind: &str) -> KindCapabilities {
    engine
        .surface_registry
        .get(kind)
        .map(|meta| meta.capabilities)
        .unwrap_or_default()
}

const EXPLORER: Takes = Takes::Only(&["explorer"]);

/// surface 하나를 대상으로 하는 내장 액션과 그 액션을 받는 kind. 대상이 생성 대기 초안이면 걸어 두었다가
/// 생성된 surface 에서 실행한다. 복사·잘라내기·붙여넣기는 입력 성격이라 넣지 않는다.
const SURFACE_ACTIONS: &[(&str, Takes)] = &[
    ("find", Takes::Only(&["terminal", "explorer"])),
    ("convert_surface", Takes::Any),
    ("convert_to_markdown", Takes::Any),
    ("convert_to_explorer", Takes::Any),
    ("refresh", Takes::Any),
    ("enter_copy_mode", Takes::Only(&["terminal"])),
    ("select_all", EXPLORER),
    ("copy_path", EXPLORER),
    ("explorer_refresh", EXPLORER),
    ("explorer_go_up", EXPLORER),
    ("explorer_back", EXPLORER),
    ("explorer_forward", EXPLORER),
    ("explorer_focus_address", Takes::Address),
    ("explorer_toggle_preview", EXPLORER),
    ("explorer_toggle_hidden", EXPLORER),
    ("explorer_properties", EXPLORER),
    ("explorer_new_folder", EXPLORER),
    ("explorer_new_file", EXPLORER),
    ("explorer_rename", EXPLORER),
    ("explorer_trash", EXPLORER),
    ("explorer_undo", EXPLORER),
    ("explorer_redo", EXPLORER),
];

fn surface_action(action_id: &str) -> Option<(&'static str, Takes)> {
    SURFACE_ACTIONS
        .iter()
        .find(|(id, _)| *id == action_id)
        .copied()
}

/// 입력 대상이 생성 대기 초안일 때 surface 액션을 처리한다.
///
/// - `None`: 대상이 초안이 아니거나 surface 를 대상으로 하는 액션이 아니다. 호출자가 지금처럼 처리한다.
/// - `Some(true)`: 초안에 걸었다. 생성된 surface 에서 실행한다.
/// - `Some(false)`: 초안 kind 가 받지 않는 액션이거나 초안이 이미 끝났다. 로그만 남기며 다른 surface 로
///   넘기지 않는다.
pub(crate) fn hold_for_draft(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    action_id: &str,
    target: Option<CommandSurfaceTarget>,
    trigger: crate::intent::RefreshTrigger,
) -> Option<bool> {
    let Some(CommandSurfaceTarget::Pending(pending)) = target else {
        return None;
    };
    let (id, takes) = surface_action(action_id)?;
    let Some(entry) = state.attach_pending.waiting_entry(engine, pending) else {
        tracing::info!(
            "action '{id}' for pending {} dropped: the draft is no longer waiting",
            pending.0
        );
        return Some(false);
    };
    if !takes.accepts(&entry.kind, kind_capabilities(engine, &entry.kind)) {
        tracing::info!(
            "action '{id}' for pending {} ignored: a '{}' surface does not take it",
            pending.0,
            entry.kind
        );
        return Some(false);
    }
    let kind = entry.kind.clone();
    state
        .attach_pending
        .deferred
        .push(pending, DeferredCommand::Action { id, kind, trigger });
    Some(true)
}

/// 키 입력 경로: 지금 입력 대상이 초안이고 그 초안에 액션을 걸었으면 true 다.
pub(crate) fn held_for_draft(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    action_id: &str,
) -> bool {
    let target = state.command_surface_target(engine);
    hold_for_draft(
        state,
        engine,
        action_id,
        target,
        crate::intent::RefreshTrigger::Shortcut,
    ) == Some(true)
}

/// 걸어 둔 액션을 생성된 `sid` 에서 실행한다. 접수 때와 kind 가 다르면 실행하지 않고 버린다. 창이 가진
/// 것(클립보드·복사 모드)이 필요한 액션은 그 창이 다음에 그릴 때 실행하도록 넘긴다.
pub(crate) fn run_held_action(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    id: &'static str,
    kind: &str,
    trigger: crate::intent::RefreshTrigger,
    sid: u32,
) {
    let now = engine.find_surface_by_id(sid).map(|s| s.kind().to_string());
    if now.as_deref() != Some(kind) {
        tracing::info!(
            "deferred action '{id}' dropped: surface {sid} is {now:?}, not the accepted '{kind}'"
        );
        return;
    }
    ensure_explorer_view(state, engine, sid);
    match run_surface_action(state, engine, id, sid, trigger) {
        Ran::Done => {}
        Ran::Nothing => tracing::info!("deferred action '{id}' did nothing on surface {sid}"),
        Ran::NeedsView => state.attach_pending.deferred.push_view(id, sid),
    }
}

/// 생성 직후 탐색기는 아직 그려지지 않아 View 상태가 없다. 그릴 때와 같은 값으로 만든다.
fn ensure_explorer_view(state: &mut MainViewState, engine: &EngineRead<'_>, sid: u32) {
    let Some(panel) = engine.find_surface_by_id(sid).and_then(|s| s.explorer()) else {
        return;
    };
    let mirror_ws = engine
        .find_workspace_index_for_surface(sid)
        .and_then(|(index, _)| engine.workspace_at(index))
        .and_then(|ws| ws.mirror.then_some(ws.id));
    state.explorer_views.get_or_init(panel, mirror_ws);
}

/// surface 액션 하나를 실행한 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ran {
    Done,
    /// 실행할 상태가 없어 아무것도 하지 않았다.
    Nothing,
    /// 클립보드·복사 모드처럼 창이 가진 것이 필요해 창이 실행해야 한다.
    NeedsView,
}

/// surface 액션을 `sid` 에서 실행한다. 호출자가 `sid` 의 kind 가 액션을 받는지 먼저 확인한다.
pub(crate) fn run_surface_action(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    id: &'static str,
    sid: u32,
    trigger: crate::intent::RefreshTrigger,
) -> Ran {
    use crate::explorer_ui::ExplorerAction;
    let ran = |done: bool| if done { Ran::Done } else { Ran::Nothing };
    let explorer = |state: &mut MainViewState, action| {
        crate::adapters::ui::egui_panels::apply_explorer_action(state, engine, sid, action);
        Ran::Done
    };
    match id {
        "find" => ran(find_on(state, engine, sid, id)),
        "convert_surface" => {
            open_convert_popup(state, sid, id);
            Ran::Done
        }
        "convert_to_markdown" => {
            ran(state.enqueue_convert_input_popup(&engine.as_ref(), "markdown", Some(sid)))
        }
        "convert_to_explorer" => {
            convert_to_explorer(state, sid, id);
            Ran::Done
        }
        "refresh" => ran(super::request_surface_refresh(state, engine, sid, trigger)),
        "select_all" => ran(state
            .explorer_views
            .get_mut(sid)
            .map(|view| view.select_all())
            .is_some()),
        "explorer_refresh" => explorer(state, ExplorerAction::Refresh),
        "explorer_go_up" => explorer(state, ExplorerAction::GoUp),
        "explorer_back" => explorer(state, ExplorerAction::GoBack),
        "explorer_forward" => explorer(state, ExplorerAction::GoForward),
        "explorer_focus_address" if surface_capabilities(engine, sid).address_band => {
            ran(html_focus_address_on(state, engine, sid))
        }
        "explorer_focus_address" => ran(state
            .explorer_views
            .get_mut(sid)
            .map(|view| view.focus_address = true)
            .is_some()),
        "explorer_toggle_preview" | "explorer_toggle_hidden" | "explorer_properties" => {
            explorer_view_on(state, engine, sid, id);
            Ran::Done
        }
        "explorer_new_folder" | "explorer_new_file" => {
            state.start_explorer_create(engine, sid, None, id == "explorer_new_folder");
            Ran::Done
        }
        "explorer_undo" | "explorer_redo" => {
            let origin = crate::intent::IntentOrigin::User {
                source: crate::intent::UserSource::Shortcut(id),
            };
            state.explorer_history_step(engine, sid, id == "explorer_redo", origin, false);
            Ran::Done
        }
        "copy_path" | "enter_copy_mode" | "explorer_rename" | "explorer_trash" => Ran::NeedsView,
        _ => Ran::Nothing,
    }
}

/// 팔레트 호스트 명령을 처리한 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteRun {
    /// surface 액션이 아니다. 호출자가 액션 ID 로 실행한다.
    Dispatch,
    Done,
    /// 창이 `sid` 에서 실행해야 하는 액션이다.
    View(&'static str, u32),
}

/// 팔레트에서 고른 호스트 명령을 Enter 때 정한 대상으로 처리한다. 초안이면 걸거나 로그만 남기고, surface 면
/// 그 surface 에서 실행한다.
pub(crate) fn run_palette_action(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    action_id: &str,
    target: Option<CommandSurfaceTarget>,
) -> PaletteRun {
    let trigger = crate::intent::RefreshTrigger::Palette;
    if hold_for_draft(state, engine, action_id, target, trigger).is_some() {
        return PaletteRun::Done;
    }
    let (Some(CommandSurfaceTarget::Surface(sid)), Some((id, takes))) =
        (target, surface_action(action_id))
    else {
        return PaletteRun::Dispatch;
    };
    let Some(kind) = engine.find_surface_by_id(sid).map(|s| s.kind()) else {
        tracing::info!("palette action '{id}' dropped: its surface {sid} is gone");
        return PaletteRun::Done;
    };
    if !takes.accepts(kind, surface_capabilities(engine, sid)) {
        // 액션 ID 처리로 넘기면 실행할 때의 포커스를 다시 읽는다. 그 surface 에서 뜻이 있는 둘만 창이
        // 처리한다: 페이지의 전체 선택과, 터미널이 아니라고 알리는 복사 모드.
        if matches!(id, "select_all" | "enter_copy_mode") {
            return PaletteRun::View(id, sid);
        }
        tracing::info!(
            "palette action '{id}' ignored: surface {sid} is a '{kind}' that does not take it"
        );
        return PaletteRun::Done;
    }
    match run_surface_action(state, engine, id, sid, trigger) {
        Ran::Done => PaletteRun::Done,
        Ran::Nothing => {
            tracing::info!("palette action '{id}' did nothing on surface {sid}");
            PaletteRun::Done
        }
        Ran::NeedsView => PaletteRun::View(id, sid),
    }
}

/// native WebView 가 넘긴 키를 surface 액션으로 처리한 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Received {
    /// 키를 받은 surface 에서 실행할 surface 액션 바인딩이 아니다. 호출자가 지금처럼 처리한다.
    Unbound,
    /// surface 액션 바인딩이지만 키를 받은 surface 가 받지 않는다. 모델 포커스가 다른 surface 라 그쪽으로
    /// 보내지 않고 처리하지 못한 키로 넘긴다.
    Pass,
    /// surface 액션 바인딩이지만 키를 받은 surface 가 받지 않고, 같은 키가 surface 하나를 대상으로 하지 않는
    /// 전역 단축키이기도 하다. 호출자는 전역 단축키만 처리한다. 모델 포커스 surface 의 목록 범위 키(탐색기
    /// 이름 바꾸기 등)는 고르지 않는다.
    Global,
    Done,
    /// 창이 키를 받은 surface 에서 실행해야 하는 액션이다.
    View(&'static str),
}

/// native WebView 가 넘긴 키가 surface 액션 바인딩이면 키를 받은 `sid` 에서 실행한다. 페이지 편집 키는
/// 페이지 편집 경로가, 새로고침은 앞 단계가 받은 surface 로 처리하므로 여기서 고르지 않는다.
pub(crate) fn run_received_key(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    kb: &crate::settings::KeybindingSettings,
    key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
    sid: u32,
) -> Received {
    let Some(kind) = engine.find_surface_by_id(sid).map(|s| s.kind()) else {
        return Received::Unbound;
    };
    let mut bound = false;
    for &(id, takes) in SURFACE_ACTIONS {
        if id == "refresh" || crate::webview::WebViewEditAction::from_field_id(id).is_some() {
            continue;
        }
        if !kb
            .get_bindings(id)
            .is_some_and(|combos| super::matches_any_binding(combos, key, mods))
        {
            continue;
        }
        bound = true;
        if !takes.accepts(kind, surface_capabilities(engine, sid)) {
            continue;
        }
        match run_surface_action(
            state,
            engine,
            id,
            sid,
            crate::intent::RefreshTrigger::Shortcut,
        ) {
            Ran::Done => return Received::Done,
            Ran::NeedsView => return Received::View(id),
            Ran::Nothing => {}
        }
    }
    // 모델 포커스가 키를 받은 surface 면 지금 경로가 같은 surface 를 대상으로 처리한다. 같은 키에
    // surface 액션이 아닌 전역 단축키(F2 의 `rename_tab` 등)가 있으면 그 단축키만 처리한다.
    if !bound || state.focused_surface_id(engine) == Some(sid) {
        Received::Unbound
    } else if binds_host_command(kb, key, mods) {
        Received::Global
    } else {
        Received::Pass
    }
}

/// 키가 surface 하나를 대상으로 하지 않는 전역 단축키에 묶여 있다.
fn binds_host_command(
    kb: &crate::settings::KeybindingSettings,
    key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
) -> bool {
    crate::settings::KeybindingSettings::GENERAL_BINDING_FIELDS
        .iter()
        .map(|&(id, _)| id)
        .filter(|id| {
            surface_action(id).is_none()
                && crate::webview::WebViewEditAction::from_field_id(id).is_none()
        })
        .any(|id| {
            kb.get_bindings(id)
                .is_some_and(|combos| super::matches_any_binding(combos, key, mods))
        })
}

#[cfg(test)]
#[path = "surface_action_tests.rs"]
mod tests;
