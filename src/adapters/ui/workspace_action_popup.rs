//! workspace 하나를 다루는 확인 팝업을 여는 한 지점. 강제 분리 확인과 원격 닫기 확인이 여기서 연다.
//! workspace 이름·부제 바꾸기는 같은 규칙으로 `RenameTarget::popup_scope`가 창 범위를 정한다.
//! workspace 자체를 다루는 팝업이라 그 workspace가 보이는 자리인 창 범위로 연다.
//! 대상이 활성 workspace가 아니어도 지금 창에 바로 보이고, workspace를 전환하지 않는다.
//! 새 요청은 같은 창 범위에서 열린 팝업의 내용과 대상만 바꾼다. 앞 요청의 대상은 각 팝업이 요청 상태를 통째로 바꿔 남기지 않는다.

use crate::adapters::ui::popup::PopupId;
use crate::intent::{DispatchedIntent, OpenPopupMode, UiIntent};
use crate::model::popup_kind::PopupScope;
use crate::state::MainViewState;

/// 창 범위로 팝업 `id`를 연다. 대상 workspace는 호출자가 요청 상태에 먼저 넣는다. `origin`은 요청 출처를 붙인다.
pub(crate) fn open(
    state: &mut MainViewState,
    id: PopupId,
    origin: impl FnOnce(UiIntent) -> DispatchedIntent,
) {
    state.dispatch_intent(origin(UiIntent::ReopenPopup {
        id,
        mode: OpenPopupMode::CenteredFocused(PopupScope::Window),
    }));
}

#[cfg(test)]
mod tests {
    use crate::adapters::ui::LayoutContext;
    use crate::adapters::ui::popup::PopupId;
    use crate::app::attach_client::workspace_close::SavedRowClose;
    use crate::app::command::DomainIntent;
    use crate::state::RenameTarget;

    /// workspace 1이 활성인 창에서 맨 위에 보이는 팝업.
    fn visible_in_workspace_1(state: &crate::state::MainViewState) -> Option<PopupId> {
        let ctx = LayoutContext {
            active_workspace_id: 1,
            ..LayoutContext::window_only()
        };
        state
            .popups
            .topmost_visible_open(Some(&ctx))
            .map(|(id, _)| id)
    }

    /// 비활성 workspace를 다루는 확인·이름 바꾸기 팝업은 workspace를 전환하지 않고 지금 창에 보인다.
    #[test]
    fn a_popup_for_an_inactive_workspace_shows_in_the_current_window() {
        use crate::adapters::ui::popup::confirm_close_remote::CONFIRM_CLOSE_REMOTE_POPUP_ID;
        use crate::adapters::ui::popup::confirm_force_detach_workspace::CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID;
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        crate::adapters::ui::popup::confirm_force_detach_workspace::open(&mut state, 7);
        crate::intent::popup::apply_pending(&mut state, &mut session);
        assert_eq!(
            visible_in_workspace_1(&state),
            Some(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
        );

        crate::adapters::ui::popup::confirm_close_remote::open(
            &mut state,
            SavedRowClose {
                row: 9,
                last: false,
            },
            DomainIntent::CloseWorkspace { workspace_id: 9 },
        );
        crate::intent::popup::apply_pending(&mut state, &mut session);
        assert_eq!(
            visible_in_workspace_1(&state),
            Some(CONFIRM_CLOSE_REMOTE_POPUP_ID)
        );

        {
            let mut engine = session.borrow_mut();
            let other = crate::state::tests::add_mirror_test_workspace(&mut state, &mut engine);
            state.set_active_workspace_index(&engine.read(), 0);
            assert!(crate::adapters::ui::rename_popup::open(
                &mut state,
                &engine.read(),
                RenameTarget::WorkspaceName {
                    workspace_id: other
                },
                String::new(),
                crate::intent::UiIntent::from_user_context_menu
            ));
        }
        crate::intent::popup::apply_pending(&mut state, &mut session);
        assert_eq!(visible_in_workspace_1(&state), Some("rename"));
        assert_eq!(state.active_workspace(&session.read()).id, 1);
    }
}
