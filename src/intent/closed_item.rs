//! 닫힌 항목 (closed_items) 복원 Intent 핸들러.
//!
//! 정책:
//! - **RestoreClosedItem**: Ctrl+Shift+T 등 사용자 단축키 전용. target_pane_id
//!   는 호출 시점의 focused pane (있으면). closed 스택 top 이 Surface/Tab
//!   인 상태에서 workspaces 가 비어있으면 사전에 `ensure_workspace_exists`
//!   처리해 add_workspace 부수효과를 정상화.
//! - **mirror 워크스페이스면 원격으로 forward 된다**: `Core::apply` 의 mirror 게이트가
//!   로컬 실행을 막고 `StructuralOp::RestoreClosedItem` 을 forward 큐에 넣는다. 복원은
//!   새 PTY spawn 이고 스냅샷의 스크롤백은 서버 디스크 참조라 서버만 실행할 수 있다
//!   (`docs/adr/0264-mirror-restore-closed-item-runs-on-the-remote.md`). 이 핸들러는
//!   그 갈래에서 로컬 스택을 **전혀 건드리지 않는다** — pop 이 게이트 뒤에 있다.

use super::{DispatchedIntent, Intent};
use crate::core::Core;
use crate::core::CoreState;
use crate::state::AppState;

/// Surface / Tab / Pane 복원에 대비해 workspace 확보 (closed top peek). `list()`
/// 는 newest-first 이므로 next() 가 stack top (pop 대상). Workspace 복원은 새
/// workspace 를 자체적으로 만들므로 이 사전 확보가 불필요.
fn ensure_workspace_for_restore(core: &mut Core, state: &mut AppState, engine: &mut CoreState) {
    use crate::model::closed_item::ClosedItem;
    let top_needs_workspace = matches!(
        engine.closed_items.list().next(),
        Some(ClosedItem::Surface { .. } | ClosedItem::Tab(_) | ClosedItem::Pane { .. })
    );
    if !top_needs_workspace || !engine.workspaces.is_empty() {
        return;
    }
    match core.create_default_workspace(engine) {
        Ok(idx) => state.active_workspace = idx,
        Err(e) => {
            tracing::warn!("RestoreClosedItem precondition workspace failed: {e}");
        }
    }
}

pub fn handle(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    intent: &DispatchedIntent,
) {
    if !matches!(&intent.body, Intent::RestoreClosedItem) {
        return;
    }
    ensure_workspace_for_restore(core, state, engine);
    let target_pane_id = state.focused_pane(engine).map(|p| p.id);
    let domain_intent = crate::core::intent::DomainIntent::RestoreClosedItem {
        target_pane_id,
        // 이 핸들러는 이 인스턴스 앞의 사용자 단축키 전용이다(모듈 doc 참조).
        scope: crate::core::intent::RestoreScope::Local,
    };
    let events = match core.apply(engine, domain_intent) {
        Ok(e) => e,
        Err(e) => {
            // mirror 워크스페이스면 `Core::apply` 가 로컬 실행을 막고 복원 op 를
            // forward 큐에 넣은 뒤 이 에러를 돌려준다 — 로컬 스택은 손대지 않는다.
            // 그 op 를 "사용자 유래" 로 뒤집어야 복원된 탭으로 focus 가 옮겨간다 —
            // 사용자 유래 op 만 결과 delta 의 새 surface 로 focus 를 따라가게 한다.
            // 이 핸들러는 단축키 전용이라 origin 은 항상 사용자다.
            crate::core::mark_last_forward_user_triggered(engine, &e, &intent.origin);
            // forward 불가/그 밖의 실패는 warn 만 남기고 사용자에게 아무 신호가 없었다.
            // 공통 처리로 태워 차단 toast 가 나가게 한다.
            crate::intent::report_apply_error(
                state,
                engine,
                &intent.origin,
                "RestoreClosedItem",
                &e,
            );
            return;
        }
    };
    for ev in events {
        if let crate::core::intent::CoreEvent::ClosedItemRestored { restored, kind } = ev
            && restored
        {
            crate::app::dispatch_domain::cascade_closed_item_restored(state, engine, kind);
        }
    }
}
