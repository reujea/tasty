//! 구조 명령(분할·새 탭·닫은 항목 복원)을 입력 시점 대상으로 보낸다. intent 해석은 이 대상만 쓰고 포커스를
//! 다시 읽지 않는다. 대상은 키·더블탭이면 키 처리 때의 포커스, 팔레트면 Enter 때 담은 값이다.
//!
//! 대상이 생성 대기 초안이면 명령이 다룰 단위가 이미 있는지 본다. pane 단위 명령은 새 탭·surface 분할
//! 초안이면 그 초안이 기댄 기존 pane 이 대상이라 바로 보낸다. 새 pane 초안이면 만들 pane 이 대상이라 걸어
//! 두고, surface 분할은 늘 만들 surface 가 대상이라 걸어 둔다. 걸어 둔 명령은 생성이 끝나면 생성된
//! surface(또는 그 pane)를 명시해 실행하고, 생성이 실패하면 버린다.

use crate::intent::{Intent, StructureTarget};
use crate::model::SplitDirection;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::state::attach_pending::Creation;
use crate::state::attach_pending::deferred::{DeferredCommand, HeldStructure};
use crate::state::command_target::CommandTarget;

/// 명령이 다루는 단위.
#[derive(Clone, Copy)]
enum Unit {
    Pane,
    Surface,
}

/// 입력 때 대상이 지금 있으면 true 다. 초안의 만들 단위가 대상이면 명령을 걸어 두고 false 를 돌려준다.
fn bound_now(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    target: &CommandTarget,
    unit: Unit,
    command: HeldStructure,
    source: &'static str,
) -> bool {
    let Some(pending) = target.draft else {
        return true;
    };
    let Some(entry) = state.attach_pending.waiting_entry(engine, pending) else {
        tracing::info!(
            "{source} for pending {} dropped: the draft is no longer waiting",
            pending.0
        );
        return false;
    };
    let creates_its_unit = match unit {
        Unit::Surface => true,
        Unit::Pane => matches!(entry.creation, Some(Creation::Pane(_))),
    };
    if !creates_its_unit {
        return true;
    }
    state
        .attach_pending
        .deferred
        .push(pending, DeferredCommand::Structure { command, source });
    false
}

/// 대상 pane 분할.
pub(crate) fn split_pane(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    target: &CommandTarget,
    direction: SplitDirection,
    source: &'static str,
) {
    if bound_now(
        state,
        engine,
        target,
        Unit::Pane,
        HeldStructure::SplitPane(direction),
        source,
    ) {
        state.dispatch_intent(
            Intent::SplitPane {
                direction,
                target: target.structure_target(),
            }
            .from_user_shortcut(source),
        );
    }
}

/// 대상 surface 분할.
pub(crate) fn split_surface(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    target: &CommandTarget,
    direction: SplitDirection,
    source: &'static str,
) {
    if bound_now(
        state,
        engine,
        target,
        Unit::Surface,
        HeldStructure::SplitSurface(direction),
        source,
    ) {
        state.dispatch_intent(
            Intent::SplitSurface {
                direction,
                surface: target.surface_id(),
            }
            .from_user_shortcut(source),
        );
    }
}

/// 대상 pane 에 `kind` 새 탭.
pub(crate) fn new_tab(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    target: &CommandTarget,
    kind: Option<String>,
    params: serde_json::Value,
    source: &'static str,
) {
    let held = HeldStructure::NewTab {
        kind: kind.clone(),
        params: params.clone(),
    };
    if bound_now(state, engine, target, Unit::Pane, held, source) {
        state.dispatch_intent(
            Intent::NewTab {
                kind,
                params,
                target: target.structure_target(),
            }
            .from_user_shortcut(source),
        );
    }
}

/// 대상 pane 에 터미널 새 탭. 입력 때는 cwd 원본 surface id 만 정하고 cwd 는 처리할 때 그 surface 에서 읽는다.
pub(crate) fn new_terminal_tab(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    target: &CommandTarget,
    source: &'static str,
) {
    new_tab(state, engine, target, None, serde_json::Value::Null, source);
}

/// 대상 pane 에 닫은 항목 복원.
pub(crate) fn restore_closed(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    target: &CommandTarget,
    source: &'static str,
) {
    if bound_now(
        state,
        engine,
        target,
        Unit::Pane,
        HeldStructure::Restore,
        source,
    ) {
        state.dispatch_intent(
            Intent::RestoreClosedItem { pane: target.pane }.from_user_shortcut(source),
        );
    }
}

/// `pane` 을 대상으로 한 분할. pane 을 직접 가리키는 입력(탭 바 분할 버튼)이 쓴다.
pub(crate) fn split_pane_at(
    pane: u32,
    cwd_source: Option<u32>,
    direction: SplitDirection,
) -> Intent {
    Intent::SplitPane {
        direction,
        target: StructureTarget {
            pane: Some(pane),
            cwd_source,
        },
    }
}

/// 걸어 둔 구조 명령을 생성된 `local` surface 를 대상으로 보낸다. pane 단위 명령은 그 surface 의 pane 이
/// 대상이고 cwd 원본은 그 surface 다.
pub(crate) fn run_held(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    command: HeldStructure,
    source: &'static str,
    local: u32,
) {
    let pane = engine.find_pane_for_surface(local);
    let at = StructureTarget {
        pane,
        cwd_source: Some(local),
    };
    let intent = match command {
        HeldStructure::SplitPane(direction) => Intent::SplitPane {
            direction,
            target: at,
        },
        HeldStructure::SplitSurface(direction) => Intent::SplitSurface {
            direction,
            surface: Some(local),
        },
        HeldStructure::NewTab { kind, params } => Intent::NewTab {
            kind,
            params,
            target: at,
        },
        HeldStructure::Restore => Intent::RestoreClosedItem { pane },
    };
    state.dispatch_intent(intent.from_user_shortcut(source));
}
