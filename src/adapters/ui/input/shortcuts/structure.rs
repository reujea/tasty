//! 구조 명령(분할·새 탭·닫은 항목 복원)을 입력 시점 대상으로 만든다. intent 해석은 이 대상만 쓰고 포커스를
//! 다시 읽지 않는다. 대상은 키·더블탭이면 키 처리 때의 포커스, 팔레트면 Enter 때 담은 값이다.

use crate::intent::{Intent, StructureTarget};
use crate::model::SplitDirection;
use crate::state::command_target::CommandTarget;

/// 입력 때 생성 대기 초안이 대상이던 구조 명령은 초안 자리에 적용할 수 없어 거절한다.
fn refused_for_draft(target: &CommandTarget, what: &str, refuse: bool) -> bool {
    if refuse && let Some(draft) = target.draft {
        tracing::info!(
            "{what} refused: its target is the pending draft {}",
            draft.0
        );
        return true;
    }
    false
}

/// 대상 pane 분할.
pub(crate) fn split_pane(target: &CommandTarget, direction: SplitDirection) -> Option<Intent> {
    if refused_for_draft(target, "split pane", true) {
        return None;
    }
    Some(Intent::SplitPane {
        direction,
        target: target.structure_target(),
    })
}

/// 대상 surface 분할.
pub(crate) fn split_surface(target: &CommandTarget, direction: SplitDirection) -> Option<Intent> {
    if refused_for_draft(target, "split surface", true) {
        return None;
    }
    Some(Intent::SplitSurface {
        direction,
        surface: target.surface_id(),
    })
}

/// 대상 pane 에 `kind` 새 탭. 새 탭을 만드는 초안이면 그 anchor pane 에 둔다.
pub(crate) fn new_tab(
    target: &CommandTarget,
    kind: Option<String>,
    params: serde_json::Value,
) -> Option<Intent> {
    let tab_draft = matches!(
        target.draft_creation,
        Some(crate::state::attach_pending::Creation::Tab)
    );
    if refused_for_draft(target, "new tab", !tab_draft) {
        return None;
    }
    Some(Intent::NewTab {
        kind,
        params,
        target: target.structure_target(),
    })
}

/// 대상 pane 에 닫은 항목 복원.
pub(crate) fn restore_closed(target: &CommandTarget) -> Intent {
    Intent::RestoreClosedItem { pane: target.pane }
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
