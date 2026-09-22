//! Preset 도메인 Intent 핸들러 + 공유 mutation 함수.
//!
//! 본 모듈은 두 가지 표면을 제공한다:
//!
//! 1. **Intent 핸들러** (`handle`): `dispatch_pending_intents` 가 호출.
//!    inner 함수를 호출해 mutation 후 toast/cascade 처리.
//!
//! 2. **공유 mutation 함수** (`apply_inner`, `save_inner`, `delete_inner`,
//!    `rename_inner`, `capture_inner`): IPC 핸들러가 sync 결과를 돌려주기 위해
//!    직접 호출. 외부 caller 와 동일한 코드 경로를 보장한다.
//!
//! 정책 (action-dispatch.md 참조):
//! - **ApplyPreset focus**: origin 으로 자동 분기 (User → focus=true, Agent → false).
//! - **SavePreset naming**: `explicit_name` 우선, 없으면 `base_name` 으로 store.unique_name 자동.
//! - **SavePreset cascade**: User origin 일 때만 save 후 PresetView 자동 오픈 + select.
//!   Agent origin 은 cascade 미수행 (focus 독립성 원칙). `state.dialogs.pending_open_preset_window`
//!   + `pending_preset_window_selection` 으로 main loop 에 신호.
//! - **List/Get**: read-only — Intent 큐 안 거치고 IPC handler 가 직접 처리.

use super::{DispatchedIntent, Intent};

/// preset Intent 가 운반하는 캡처된 preset payload.
/// 호출자 (우클릭 / IPC) 가 capture 를 수행한 뒤 핸들러에 그대로 넘긴다 — CapturePreset 을
/// 별도 Intent 로 두지 않기로 한 결정 반영.
#[derive(Debug, Clone)]
pub enum ClonedPreset {
    Workspace(tasty_presets::WorkspacePreset),
    Tab(tasty_presets::TabPreset),
    Pane(tasty_presets::PanePreset),
}

impl ClonedPreset {
    pub fn kind(&self) -> tasty_presets::PresetKind {
        match self {
            ClonedPreset::Workspace(_) => tasty_presets::PresetKind::Workspace,
            ClonedPreset::Tab(_) => tasty_presets::PresetKind::Tab,
            ClonedPreset::Pane(_) => tasty_presets::PresetKind::Pane,
        }
    }
}

use crate::intent::preset_capture::{
    capture_pane_preset, capture_tab_preset, capture_workspace_preset,
};
use crate::state::AppState;
use crate::state::preset_apply::{ApplyError, ApplyOptions};
use tasty_presets::{PresetError, PresetKind};

// ───────────────────────────────── Intent dispatcher ─────────────────────────────────

/// preset 도메인 분기 핸들러. `dispatch_pending_intents` 에서 호출.
pub fn handle(
    core: &crate::core::Core,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    intent: &DispatchedIntent,
) {
    match &intent.body {
        Intent::ApplyPreset {
            kind,
            name,
            category,
        } => apply(
            core,
            state,
            engine,
            intent,
            PresetApplyTarget {
                kind: *kind,
                name,
                target_pane_id: None,
                target_workspace_id: None,
                category: *category,
            },
        ),
        Intent::SavePreset {
            base_name,
            explicit_name,
            overwrite,
            preset,
        } => save(
            core,
            state,
            engine,
            intent,
            PresetSaveRequest {
                base_name,
                explicit_name: explicit_name.as_deref(),
                overwrite: *overwrite,
                preset,
            },
        ),
        _ => {}
    }
}

fn apply(
    core: &crate::core::Core,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    intent: &DispatchedIntent,
    target: PresetApplyTarget,
) {
    // P1: focus 정책은 origin 으로 자동 분기.
    let focus = intent.origin.is_user();
    let options = ApplyOptions { focus };

    if let Err(e) = apply_inner(core, state, engine, target, options) {
        tracing::warn!("preset apply failed: {e}");
        // 실패 toast 는 사용자 발화에서만 — 에이전트 발화의 실패는 로그로 끝난다(identity 원칙 1).
        #[cfg(feature = "gui")]
        if intent.origin.is_user() {
            state.toasts.push(
                crate::i18n::t("preset.toast.apply_failed"),
                crate::model::toast_kind::ToastKind::Error,
                crate::model::toast_kind::ToastScope::Window,
            );
        }
    }
}

fn save(
    core: &crate::core::Core,
    state: &mut AppState,
    _engine: &mut crate::core::CoreState,
    intent: &DispatchedIntent,
    request: PresetSaveRequest,
) {
    let kind = request.preset.kind();
    let save_result = save_inner(
        core,
        request.base_name,
        request.explicit_name,
        request.overwrite,
        request.preset.clone(),
    );

    let toast_key = match (&save_result, kind) {
        (Ok(_), PresetKind::Workspace) => "preset.toast.saved_workspace",
        (Ok(_), PresetKind::Tab) => "preset.toast.saved_tab",
        (Ok(_), PresetKind::Pane) => "preset.toast.saved_pane",
        (Err(_), _) => "preset.toast.save_failed",
    };
    // 실패 toast 는 사용자 발화에서만 — 에이전트 발화의 실패는 아래 `warn` 로그로 끝난다
    // (identity 원칙 1). 성공 toast 는 origin 과 무관하게 종전 그대로다.
    let show_toast = save_result.is_ok() || intent.origin.is_user();
    #[cfg(feature = "gui")]
    if show_toast {
        let toast_kind = if save_result.is_ok() {
            crate::model::toast_kind::ToastKind::Info
        } else {
            crate::model::toast_kind::ToastKind::Error
        };
        state.toasts.push(
            crate::i18n::t(toast_key),
            toast_kind,
            crate::model::toast_kind::ToastScope::Window,
        );
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = (toast_key, show_toast); // headless: toast 소비자 없음, silent drop.
    }

    let saved_name = match save_result {
        Ok(SaveOutcome::Saved(n)) => n,
        Ok(SaveOutcome::SkippedExists) => return,
        Err(e) => {
            tracing::warn!("preset save failed: {e}");
            return;
        }
    };

    // User origin cascade: save 후 PresetView 자동 오픈 + select.
    // Agent origin 은 cascade 미수행 (focus 독립성). headless 에는 열 창도 이 요청을 비울
    // 소비자(`process_pending_open_preset_window`)도 없어 요청 자체를 안 만든다.
    #[cfg(feature = "gui")]
    if intent.origin.is_user() {
        state.dialogs.pending_open_preset_window = true;
        state.dialogs.pending_preset_window_selection = Some((kind, saved_name));
    }
    // headless: 위 요청을 만들지 않고 토스트도 없으므로 창 상태와 세 값을 읽는 쪽이 없다.
    #[cfg(not(feature = "gui"))]
    let _ = (state, intent, kind, saved_name);
}

// ───────────────────────────────── Shared mutation API ─────────────────────────────────

/// `apply_*_preset` 결과를 IPC 가 그대로 응답에 실을 수 있도록 enum 으로 캡슐화.
#[derive(Debug, Clone)]
pub enum ApplyOutcome {
    Workspace { workspace_id: u32 },
    Tab { tab_id: u32 },
    Pane { pane_id: u32 },
}

/// `save_inner` 결과 — 충돌 시 skip 된 경우를 명시.
#[derive(Debug, Clone)]
pub enum SaveOutcome {
    Saved(String),
    /// `overwrite=false` + 이미 존재 → 저장 skip.
    SkippedExists,
}

/// 공유 mutation API: 발생 가능한 실패를 모두 enum 으로 노출.
#[derive(Debug)]
pub enum PresetMutationError {
    NotFound { kind: PresetKind, name: String },
    Apply(ApplyError),
    Store(PresetError),
}

impl std::fmt::Display for PresetMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { kind, name } => {
                write!(f, "preset not found: {}/{name}", kind.as_str())
            }
            Self::Apply(e) => write!(f, "{e}"),
            Self::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PresetMutationError {}

/// preset_store 잠금 + clone. lock 안 ↔ apply 본체를 분리해 critical section 을 짧게 유지.
fn clone_preset_from_store(
    core: &crate::core::Core,
    kind: PresetKind,
    name: &str,
) -> Result<Option<ClonedPreset>, PresetMutationError> {
    let guard = crate::poison::recover_mutex(
        core.preset_store.lock(),
        crate::core::PRESET_STORE_WHAT,
        &crate::core::PRESET_STORE_POISONED,
    );
    let cloned = match kind {
        PresetKind::Workspace => guard
            .get_workspace(name)
            .cloned()
            .map(ClonedPreset::Workspace),
        PresetKind::Tab => guard.get_tab(name).cloned().map(ClonedPreset::Tab),
        PresetKind::Pane => guard.get_pane(name).cloned().map(ClonedPreset::Pane),
    };
    Ok(cloned)
}

/// preset apply 요청 — 무엇을(`kind`/`name`) 어디에(`target_pane_id`/
/// `target_workspace_id`/`category`) 적용할지의 개념적 단위. `target_pane_id` /
/// `target_workspace_id` 는 tab/pane apply 시에만, `category` 는 workspace apply
/// 시에만 의미가 있다(다른 kind 에는 무시된다).
pub struct PresetApplyTarget<'a> {
    pub kind: PresetKind,
    pub name: &'a str,
    pub target_pane_id: Option<u32>,
    pub target_workspace_id: Option<u32>,
    pub category: Option<crate::model::WorkspaceCategoryId>,
}

/// preset save 요청 — 저장할 preset 과 naming/overwrite 정책의 개념적 단위.
pub struct PresetSaveRequest<'a> {
    pub base_name: &'a str,
    pub explicit_name: Option<&'a str>,
    pub overwrite: bool,
    pub preset: &'a ClonedPreset,
}

/// Preset 적용. store 에서 clone 후 lock 해제하고 본체를 호출한다.
pub fn apply_inner(
    core: &crate::core::Core,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    target: PresetApplyTarget,
    options: ApplyOptions,
) -> Result<ApplyOutcome, PresetMutationError> {
    let cloned = clone_preset_from_store(core, target.kind, target.name)?.ok_or_else(|| {
        PresetMutationError::NotFound {
            kind: target.kind,
            name: target.name.to_string(),
        }
    })?;

    match cloned {
        ClonedPreset::Workspace(p) => {
            let idx = state
                .apply_workspace_preset(engine, &p, target.category, options)
                .map_err(PresetMutationError::Apply)?;
            let workspace_id = engine.workspaces[idx].id;
            Ok(ApplyOutcome::Workspace { workspace_id })
        }
        ClonedPreset::Tab(p) => {
            let tab_id = state
                .apply_tab_preset(engine, &p, target.target_pane_id, options)
                .map_err(PresetMutationError::Apply)?;
            Ok(ApplyOutcome::Tab { tab_id })
        }
        ClonedPreset::Pane(p) => {
            let pane_id = state
                .apply_pane_preset(engine, &p, target.target_workspace_id, options)
                .map_err(PresetMutationError::Apply)?;
            Ok(ApplyOutcome::Pane { pane_id })
        }
    }
}

/// `explicit_name`이 있으면 사용(overwrite=false면 충돌 시 `None` = skip 신호),
/// 없으면 `base_name` 기반 unique_name 자동 부여.
fn resolve_save_name(
    store: &tasty_presets::PresetStore,
    kind: PresetKind,
    base_name: &str,
    explicit_name: Option<&str>,
    overwrite: bool,
) -> Option<String> {
    match explicit_name {
        Some(n) => {
            if !overwrite {
                let exists = match kind {
                    PresetKind::Workspace => store.get_workspace(n).is_some(),
                    PresetKind::Tab => store.get_tab(n).is_some(),
                    PresetKind::Pane => store.get_pane(n).is_some(),
                };
                if exists {
                    tracing::warn!("SavePreset: name '{n}' exists, overwrite=false → skip");
                    return None;
                }
            }
            Some(n.to_string())
        }
        None => Some(store.unique_name(kind, base_name)),
    }
}

/// preset kind 별로 `name` 을 적용해 store 에 반영(overwrite 여부에 따라 대응 메서드 분기).
fn store_preset(
    store: &mut tasty_presets::PresetStore,
    preset: ClonedPreset,
    name: String,
    overwrite: bool,
) -> Result<(), PresetError> {
    match preset {
        ClonedPreset::Workspace(mut p) => {
            p.name = name;
            if overwrite {
                store.save_workspace_overwrite(p)
            } else {
                store.save_workspace(p)
            }
        }
        ClonedPreset::Tab(mut p) => {
            p.name = name;
            if overwrite {
                store.save_tab_overwrite(p)
            } else {
                store.save_tab(p)
            }
        }
        ClonedPreset::Pane(mut p) => {
            p.name = name;
            if overwrite {
                store.save_pane_overwrite(p)
            } else {
                store.save_pane(p)
            }
        }
    }
}

/// Preset 저장. `explicit_name` 이 있으면 사용 (overwrite=false 면 충돌 시 skip),
/// 없으면 `base_name` 기반 unique_name 자동 부여.
pub fn save_inner(
    core: &crate::core::Core,
    base_name: &str,
    explicit_name: Option<&str>,
    overwrite: bool,
    preset: ClonedPreset,
) -> Result<SaveOutcome, PresetMutationError> {
    let mut store = crate::poison::recover_mutex(
        core.preset_store.lock(),
        crate::core::PRESET_STORE_WHAT,
        &crate::core::PRESET_STORE_POISONED,
    );

    let kind = preset.kind();
    let Some(name) = resolve_save_name(&store, kind, base_name, explicit_name, overwrite) else {
        return Ok(SaveOutcome::SkippedExists);
    };

    store_preset(&mut store, preset, name.clone(), overwrite)
        .map(|_| SaveOutcome::Saved(name))
        .map_err(PresetMutationError::Store)
}

pub fn delete_inner(
    core: &crate::core::Core,
    kind: PresetKind,
    name: &str,
) -> Result<(), PresetMutationError> {
    let mut store = crate::poison::recover_mutex(
        core.preset_store.lock(),
        crate::core::PRESET_STORE_WHAT,
        &crate::core::PRESET_STORE_POISONED,
    );
    store.delete(kind, name).map_err(PresetMutationError::Store)
}

pub fn rename_inner(
    core: &crate::core::Core,
    kind: PresetKind,
    from: &str,
    to: &str,
) -> Result<(), PresetMutationError> {
    let mut store = crate::poison::recover_mutex(
        core.preset_store.lock(),
        crate::core::PRESET_STORE_WHAT,
        &crate::core::PRESET_STORE_POISONED,
    );
    store
        .rename(kind, from, to)
        .map_err(PresetMutationError::Store)
}

/// Surface 식별자로 워크스페이스/탭/페인을 캡처해 ClonedPreset 으로 변환.
/// IPC `preset.capture` 가 사용. UI 우클릭 경로는 자신이 직접 capture 한 뒤
/// `Intent::SavePreset { preset: ClonedPreset, .. }` 로 발화하므로 별도 경로.
pub fn capture_inner(
    engine: &crate::core::CoreState,
    kind: PresetKind,
    source_id: u32,
) -> Result<(ClonedPreset, String), String> {
    let registry = engine.surface_registry.clone();

    match kind {
        PresetKind::Workspace => {
            let ws = engine
                .workspaces
                .iter()
                .find(|w| w.id == source_id)
                .ok_or_else(|| format!("Workspace id {source_id} not found"))?;
            let base = if ws.name.is_empty() {
                "workspace".to_string()
            } else {
                ws.name.clone()
            };
            let preset = capture_workspace_preset(engine, ws, None, &registry)
                .ok_or_else(|| "workspace capture failed".to_string())?;
            Ok((ClonedPreset::Workspace(preset), base))
        }
        PresetKind::Tab => {
            let pane_id = engine
                .find_pane_for_tab(source_id)
                .ok_or_else(|| format!("Tab id {source_id} not found"))?;
            for ws in &engine.workspaces {
                if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                    for tab in &pane.tabs {
                        if tab.id == source_id {
                            let base = tab
                                .explicit_name
                                .clone()
                                .unwrap_or_else(|| tab.name.clone());
                            let base = if base.is_empty() {
                                "tab".to_string()
                            } else {
                                base
                            };
                            let preset = capture_tab_preset(engine, tab, None, &registry)
                                .ok_or_else(|| "tab capture failed".to_string())?;
                            return Ok((ClonedPreset::Tab(preset), base));
                        }
                    }
                }
            }
            Err(format!("Tab id {source_id} not found"))
        }
        PresetKind::Pane => {
            for ws in &engine.workspaces {
                if let Some(pane) = ws.pane_layout().find_pane(source_id) {
                    let preset = capture_pane_preset(engine, pane, None, &registry)
                        .ok_or_else(|| "pane capture failed".to_string())?;
                    return Ok((ClonedPreset::Pane(preset), "pane".to_string()));
                }
            }
            Err(format!("Pane id {source_id} not found"))
        }
    }
}
