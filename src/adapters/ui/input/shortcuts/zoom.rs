//! Zoom in / out / reset 단축키 — focused surface 의 font_size override 갱신.

use winit::keyboard::{Key, ModifiersState};

use crate::app::engine_action::{SettingsPatch, ZoomChange};
use crate::state::FocusedSurfaceType;
use crate::state::attach_pending::deferred::{CommandSurfaceTarget, DeferredCommand};
use crate::state::command_target::CommandTarget;
use crate::view::main::MainView;
use tasty_key_match::matches_any_binding;

/// 키 입력과 명령 팔레트가 공유하는 줌 동작.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZoomAction {
    In,
    Out,
    Reset,
}

impl MainView {
    pub(super) fn handle_zoom_shortcut(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
        target: &CommandTarget,
    ) -> bool {
        let kb = &engine.settings.keybindings;
        let action = if matches_any_binding(&kb.zoom_in, key, mods) {
            ZoomAction::In
        } else if matches_any_binding(&kb.zoom_out, key, mods) {
            ZoomAction::Out
        } else if matches_any_binding(&kb.zoom_reset, key, mods) {
            ZoomAction::Reset
        } else {
            return false;
        };
        Self::apply_zoom(state, engine, action, target)
    }

    /// 줌 실행. 명령 대상 surface 의 kind 가 줌 대상이 아니면 `false`. 대상이 생성 대기 초안이면 초안이 만들
    /// kind 로 판정해 걸어 두고, 생성된 surface 의 kind 글꼴에 적용한다.
    pub(crate) fn apply_zoom(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        action: ZoomAction,
        target: &CommandTarget,
    ) -> bool {
        let change = match action {
            ZoomAction::In => crate::app::engine_action::ZoomChange::In,
            ZoomAction::Out => crate::app::engine_action::ZoomChange::Out,
            ZoomAction::Reset => crate::app::engine_action::ZoomChange::Reset,
        };
        match target.surface {
            Some(CommandSurfaceTarget::Pending(pending)) => {
                hold_zoom_for_draft(state, engine, pending, change)
            }
            Some(CommandSurfaceTarget::Surface(sid)) => {
                let Some(kind) = engine.find_surface_by_id(sid).map(|s| s.kind().to_string())
                else {
                    return false;
                };
                zoom_surface_type(state, engine, &surface_type_of_kind(&kind), change)
            }
            None => false,
        }
    }
}

/// `kind` surface 의 종류. 아직 surface 가 없는 초안이나 생성 직후 surface 에 쓴다.
fn surface_type_of_kind(kind: &str) -> FocusedSurfaceType {
    match kind {
        "terminal" => FocusedSurfaceType::Terminal,
        other => FocusedSurfaceType::Kind(other.to_string()),
    }
}

/// `focus` 종류의 줌 설정 변경. 줌 대상이 아니면 없다.
fn zoom_patch(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    focus: &FocusedSurfaceType,
    change: ZoomChange,
) -> Option<SettingsPatch> {
    let kind_zoomable = focus.kind_capability(engine, |d| d.zoomable);
    // webview는 plugin_settings의 zoom을 sync_webviews에서 backend에 적용한다.
    // egui로 그리는 surface는 아래에서 font_size를 조정한다.
    if let FocusedSurfaceType::Kind(k) = focus
        && kind_zoomable
        && let Some(source) =
            crate::runtime::surface_registry::webview_kind::webview_settings_source(k)
    {
        return Some(SettingsPatch::PluginZoom {
            plugin: source.plugin_id,
            default_zoom: source.defaults.zoom_percent,
            change,
        });
    }
    let kind = match focus {
        FocusedSurfaceType::Terminal => None,
        FocusedSurfaceType::Kind(kind) if kind_zoomable => Some(kind.to_string()),
        _ => return None,
    };
    Some(SettingsPatch::FontSize { kind, change })
}

/// `focus` 종류의 줌을 요청한다. 대상을 지금 정하고, 실행 주체가 쌓인 변경을 순서대로 적용한다.
fn zoom_surface_type(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    focus: &FocusedSurfaceType,
    change: ZoomChange,
) -> bool {
    let Some(patch) = zoom_patch(engine, focus, change) else {
        return false;
    };
    state.dispatch_intent(crate::intent::Intent::PatchSettings(patch).from_user_shortcut("zoom"));
    true
}

/// 초안이 만들 kind 가 줌 대상이면 줌을 걸어 둔다. 대상이 아니면 걸지 않고 false 다.
fn hold_zoom_for_draft(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pending: crate::state::attach_pending::PendingId,
    change: ZoomChange,
) -> bool {
    let Some(entry) = state.attach_pending.waiting_entry(engine, pending) else {
        tracing::info!(
            "zoom for pending {} dropped: the draft is no longer waiting",
            pending.0
        );
        return false;
    };
    let kind = entry.kind.clone();
    if zoom_patch(engine, &surface_type_of_kind(&kind), change).is_none() {
        tracing::info!(
            "zoom for pending {} ignored: a '{kind}' surface does not zoom",
            pending.0
        );
        return false;
    }
    state
        .attach_pending
        .deferred
        .push(pending, DeferredCommand::Zoom { change, kind });
    true
}

/// 초안에 건 줌을 생성된 `sid` 의 kind 에 적용한다. 접수 때와 kind 가 다르면 적용하지 않고 버린다.
pub(crate) fn run_held_zoom(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    change: ZoomChange,
    kind: &str,
    sid: u32,
) {
    let now = engine.find_surface_by_id(sid).map(|s| s.kind().to_string());
    if now.as_deref() != Some(kind) {
        tracing::info!(
            "deferred zoom dropped: surface {sid} is {now:?}, not the accepted '{kind}'"
        );
        return;
    }
    if !zoom_surface_type(state, engine, &surface_type_of_kind(kind), change) {
        tracing::info!("deferred zoom did nothing on surface {sid}");
    }
}
