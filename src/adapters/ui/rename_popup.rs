//! 이름 바꾸기 입력 팝업을 여는 한 지점. 범위는 바꿀 대상에서 나온다.
//! 대상을 찾지 못하면 창 범위로 바꾸지 않고 열지 않는다. 새 요청이 대상을 바꾸면 범위도 그 대상으로 다시 연다.

use crate::intent::{DispatchedIntent, OpenPopupMode, UiIntent};
use crate::model::popup_kind::PopupScope;
use crate::state::{MainViewState, RenameTarget};

const RENAME_POPUP_ID: crate::adapters::ui::popup::PopupId = "rename";

/// `target`의 범위로 이름 바꾸기를 연다. 대상이 없으면 경고 로그만 남기고 false다.
/// workspace 대상은 workspace 에 작용하는 팝업의 공용 열기 지점을 거친다.
pub(crate) fn open(
    state: &mut MainViewState,
    engine: &crate::core::CoreState,
    target: RenameTarget,
    current_value: String,
    origin: impl FnOnce(UiIntent) -> DispatchedIntent,
) -> bool {
    let Some(scope) = target.popup_scope(engine) else {
        tracing::warn!("rename popup not opened: its target {target:?} is gone");
        return false;
    };
    state.dialogs.rename = Some((target, current_value));
    if let PopupScope::Workspace(workspace_id) = scope {
        crate::adapters::ui::workspace_action_popup::open(
            state,
            workspace_id,
            RENAME_POPUP_ID,
            origin,
        );
        return true;
    }
    state.dispatch_intent(origin(UiIntent::ReopenPopup {
        id: RENAME_POPUP_ID,
        mode: OpenPopupMode::CenteredFocused(scope),
    }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened_scope(state: &mut MainViewState) -> Option<PopupScope> {
        let intents = state.take_pending_intents();
        match &intents[..] {
            [] => None,
            [
                DispatchedIntent {
                    body:
                        crate::intent::Intent::Ui(UiIntent::ReopenPopup {
                            id: RENAME_POPUP_ID,
                            mode: OpenPopupMode::CenteredFocused(scope),
                        }),
                    ..
                },
            ] => Some(scope.clone()),
            other => panic!("unexpected intents {other:?}"),
        }
    }

    #[test]
    fn each_target_opens_in_its_own_scope() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        for (target, scope) in [
            (
                RenameTarget::WorkspaceName { workspace_id: 1 },
                PopupScope::Workspace(1),
            ),
            (
                RenameTarget::WorkspaceSubtitle { workspace_id: 1 },
                PopupScope::Workspace(1),
            ),
            (RenameTarget::TabName { tab_id: 102 }, PopupScope::Tab(102)),
            (RenameTarget::NewCategory, PopupScope::Window),
        ] {
            assert!(open(
                &mut state,
                &read,
                target.clone(),
                String::new(),
                UiIntent::from_user_context_menu
            ));
            assert_eq!(opened_scope(&mut state), Some(scope), "{target:?}");
        }
    }

    /// workspace·탭 대상이 없으면 창 범위로 열지 않고 상태도 남기지 않는다.
    #[test]
    fn a_gone_target_does_not_open() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        for target in [
            RenameTarget::WorkspaceName { workspace_id: 99 },
            RenameTarget::WorkspaceSubtitle { workspace_id: 99 },
            RenameTarget::TabName { tab_id: 999 },
        ] {
            assert!(!open(
                &mut state,
                &read,
                target.clone(),
                String::new(),
                UiIntent::from_user_context_menu
            ));
            assert_eq!(opened_scope(&mut state), None, "{target:?}");
            assert!(state.dialogs.rename.is_none(), "{target:?}");
        }
    }

    /// 한 대상에서 연 이름 바꾸기에 다른 대상의 요청이 오면 범위와 대상이 모두 새 대상이다.
    #[test]
    fn a_second_request_for_another_target_moves_the_popup_there() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        for tab_id in [101, 102] {
            let read = session.read();
            assert!(open(
                &mut state,
                &read,
                RenameTarget::TabName { tab_id },
                String::new(),
                UiIntent::from_user_context_menu
            ));
            drop(read);
            crate::intent::popup::apply_pending(&mut state, &mut session);
        }
        assert_eq!(
            state.popups.open_scope(RENAME_POPUP_ID),
            Some(&PopupScope::Tab(102))
        );
        assert_eq!(
            state.dialogs.rename.as_ref().map(|(t, _)| t.clone()),
            Some(RenameTarget::TabName { tab_id: 102 })
        );
        assert!(state.popups.take_closed_queue().is_empty());
    }
}
