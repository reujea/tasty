//! 이름 바꾸기 입력 팝업을 여는 한 지점. 범위는 바꿀 대상이 보이는 자리에서 나온다(`RenameTarget::popup_scope`).
//! 대상을 찾지 못하면 창 범위로 바꾸지 않고 열지 않는다. 새 요청이 범위를 바꾸면 그 범위로 다시 열고,
//! 앞 요청의 원격 대기 상태는 넘기지 않는다.

use crate::intent::{DispatchedIntent, OpenPopupMode, UiIntent};
use crate::state::{MainViewState, RenameTarget};

pub(crate) const RENAME_POPUP_ID: crate::adapters::ui::popup::PopupId = "rename";

/// `target`의 범위로 이름 바꾸기를 연다. 대상이 없으면 경고 로그만 남기고 false다.
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
    // 다시 열면 닫는 훅이 돌지 않는다. 앞 요청의 원격 대기와 높이가 새 요청에 남지 않게 여기서 비운다.
    state.dialogs.rename_waiting = None;
    state.dialogs.rename_content_height = None;
    state.dispatch_intent(origin(UiIntent::ReopenPopup {
        id: RENAME_POPUP_ID,
        mode: OpenPopupMode::CenteredFocused(scope),
    }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::popup_kind::PopupScope;

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
                PopupScope::Window,
            ),
            (
                RenameTarget::WorkspaceSubtitle { workspace_id: 1 },
                PopupScope::Window,
            ),
            (RenameTarget::TabName { tab_id: 102 }, PopupScope::Pane(10)),
            (RenameTarget::TabName { tab_id: 201 }, PopupScope::Pane(20)),
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

    /// 한 pane 에서 연 탭 이름 바꾸기에 다른 pane 탭의 요청이 오면 범위와 대상이 모두 새 탭의 pane 이다.
    #[test]
    fn a_second_request_for_another_target_moves_the_popup_there() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        for tab_id in [101, 201] {
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
            Some(&PopupScope::Pane(20))
        );
        assert_eq!(
            state.dialogs.rename.as_ref().map(|(t, _)| t.clone()),
            Some(RenameTarget::TabName { tab_id: 201 })
        );
        assert!(state.popups.take_closed_ids().is_empty());
    }

    /// 같은 pane 의 다른 탭 요청은 범위를 그대로 두고 대상만 바꾼다.
    #[test]
    fn a_second_request_for_a_tab_in_the_same_pane_retargets_the_popup() {
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
            Some(&PopupScope::Pane(10))
        );
        assert_eq!(
            state.dialogs.rename.as_ref().map(|(t, _)| t.clone()),
            Some(RenameTarget::TabName { tab_id: 102 })
        );
        assert!(state.popups.take_closed_ids().is_empty());
    }

    /// 원격 이름 바꾸기를 기다리는 동안 다른 대상의 이름 바꾸기를 열면 대기 표시가 없고,
    /// 앞 요청의 늦은 성공 응답이 새 팝업을 닫지 않는다.
    #[test]
    fn a_second_request_drops_the_remote_wait_of_the_first() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        state.dialogs.rename_waiting = Some(crate::state::RenameWaiting {
            request: 5,
            host: "host".into(),
            done: false,
        });
        state.dialogs.rename_content_height = Some(tasty_type_geometry::length::LogicalPx(40.0));
        let read = session.read();
        assert!(open(
            &mut state,
            &read,
            RenameTarget::TabName { tab_id: 102 },
            String::new(),
            UiIntent::from_user_context_menu
        ));
        drop(read);
        crate::intent::popup::apply_pending(&mut state, &mut session);
        assert!(state.dialogs.rename_waiting.is_none());
        assert!(state.dialogs.rename_content_height.is_none());
        assert!(!crate::app::explorer_files::mark_rename_done(&mut state, 5));
        assert!(state.dialogs.rename_waiting.is_none());
        assert_eq!(
            state.dialogs.rename.as_ref().map(|(t, _)| t.clone()),
            Some(RenameTarget::TabName { tab_id: 102 })
        );
    }
}
