//! Surface 도메인 Intent 핸들러.
//!
//! 정책:
//! - **SplitSurface**: `DomainIntent::SplitSurface` forward. focused
//!   surface_id 는 handler 안에서 결정. cascade 가 origin 보고 focus 이동.
//! - **ConvertSurface**: target 분기 — `Terminal` / `Kind { kind, params }`
//!   모두 `Core::apply_convert_surface` 본문 한 곳에서 처리.

use super::{ConvertTarget, DispatchedIntent, Intent, IntentOrigin};
use crate::core::Core;
use crate::core::CoreState;
use crate::model::SplitDirection;
use crate::state::AppState;

pub fn handle(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    intent: &DispatchedIntent,
) {
    match &intent.body {
        Intent::SplitSurface { direction } => {
            split(core, state, engine, *direction, &intent.origin)
        }
        Intent::ConvertSurface { surface_id, target } => {
            convert(core, state, engine, *surface_id, target, &intent.origin)
        }
        _ => {}
    }
}

fn split(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    direction: SplitDirection,
    origin: &IntentOrigin,
) {
    let Some(sid) = state.focused_surface_id(engine) else {
        tracing::warn!("SplitSurface: no focused surface");
        return;
    };
    let cwd = state.resolve_inherit_cwd_from_surface(engine, sid);
    let intent = crate::core::intent::DomainIntent::SplitSurface {
        target_surface_id: sid,
        direction,
        cwd,
        kind: "terminal".to_string(),
        surface_params: serde_json::json!({}),
    };
    let events = match core.apply(engine, intent) {
        Ok(e) => e,
        Err(e) => {
            crate::core::mark_last_forward_user_triggered(engine, &e, origin);
            super::report_apply_error(state, engine, origin, "SplitSurface", &e);
            return;
        }
    };
    for ev in events {
        if let crate::core::intent::CoreEvent::SurfaceSplit {
            workspace_index,
            pane_id,
            new_surface_id,
            ..
        } = ev
        {
            crate::core::structural_cascade::cascade_surface_split(
                state,
                engine,
                origin,
                workspace_index,
                pane_id,
                new_surface_id,
            );
        }
    }
}

fn convert(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    surface_id: u32,
    target: &ConvertTarget,
    origin: &IntentOrigin,
) {
    use crate::core::intent::ConvertSurfaceTarget;

    let domain_target = match target {
        ConvertTarget::Terminal => {
            let cwd = state.resolve_inherit_cwd(engine);
            ConvertSurfaceTarget::Terminal { cwd }
        }
        ConvertTarget::Kind { cwd, kind, params } => {
            // 옛 caller 호환: SurfaceKindDef 가 기대하는 canonical 키(예: markdown
            // 의 `file`)로 registry 의 param_aliases(예: `file_path`→`file`)를 적용해
            // generic 하게 정규화한다(kind 하드코딩 없음).
            let mut params = params.clone();
            if let Some(def) = engine.surface_registry.get(kind) {
                def.normalize_param_aliases(&mut params);
            }
            // Surface cwd invariant: 변환 시 cwd 손실 금지. 호출자가 명시 cwd 를
            // 넘기지 않은 경우 source surface 에서 carry — 호스트 시작 cwd 같은
            // 사용자 의도와 무관한 fallback 으로 흘러가지 않도록 한다.
            let resolved_cwd = cwd
                .clone()
                .or_else(|| state.resolve_inherit_cwd_from_surface(engine, surface_id));
            // 제자리 변환(주소창 navigate·convert 팝업 등)도 최근 목록 기록. `file` 키로
            // 통일된 뒤라 여기서 1회 기록 — file 없으면 no-op. kind 하드코딩 없이 매니페스트
            // `records_recent` 를 선언한 kind 만 기록(generic per-kind).
            if engine
                .surface_registry
                .get(kind)
                .is_some_and(|d| d.records_recent)
            {
                state.record_recent(kind, &params);
            }
            ConvertSurfaceTarget::Kind {
                cwd: resolved_cwd,
                kind: kind.clone(),
                params,
            }
        }
    };

    let intent = crate::core::intent::DomainIntent::ConvertSurface {
        surface_id,
        target: domain_target,
    };
    if let Err(e) = core.apply(engine, intent) {
        super::report_apply_error(state, engine, origin, "ConvertSurface", &e);
    }
}
