//! Pane 도메인 Intent 핸들러.
//!
//! 정책:
//! - **SplitPane**: `DomainIntent::SplitPane` forward. focused pane_id 는
//!   handler 안에서 결정. cascade 가 origin 보고 focus 이동 분기.
//! - ratio / focus 변경 API 는 S3=B 결정으로 마이그레이션 범위 외 — 사용자 단축키
//!   전용 cascade (`close_active_pane` 등) 도 그대로 직접 호출 유지.

use super::{DispatchedIntent, Intent, IntentOrigin};
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
    if let Intent::SplitPane { direction } = &intent.body {
        split(core, state, engine, *direction, &intent.origin);
    }
}

fn split(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    direction: SplitDirection,
    origin: &IntentOrigin,
) {
    let pane_id = state.active_workspace(engine).focused_pane;
    let cwd = state.resolve_inherit_cwd(engine);
    let intent = crate::core::intent::DomainIntent::SplitPane {
        target_pane_id: pane_id,
        direction,
        cwd,
        kind: "terminal".to_string(),
        surface_params: serde_json::json!({}),
    };
    let events = match core.apply(engine, intent) {
        Ok(e) => e,
        Err(e) => {
            crate::core::mark_last_forward_user_triggered(engine, &e, origin);
            super::report_apply_error(state, engine, origin, "SplitPane", &e);
            return;
        }
    };
    for ev in events {
        if let crate::core::intent::CoreEvent::PaneSplit {
            workspace_index,
            original_pane_id,
            new_pane_id,
            new_surface_id,
            direction,
        } = ev
        {
            crate::core::structural_cascade::cascade_pane_split(
                state,
                engine,
                origin,
                crate::core::structural_cascade::PaneSplitCascade {
                    workspace_index,
                    original_pane_id,
                    new_pane_id,
                    new_surface_id,
                    direction,
                },
            );
        }
    }
}
