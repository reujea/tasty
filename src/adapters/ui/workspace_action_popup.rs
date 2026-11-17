//! workspace 하나를 다루는 확인 팝업을 여는 한 지점. 강제 분리 확인과 원격 닫기 확인이 여기서 연다.
//! workspace 이름·부제 바꾸기는 같은 규칙으로 `RenameTarget::popup_scope`가 창 범위를 정한다.
//! workspace 자체를 다루는 팝업이라 그 workspace가 보이는 자리인 창 범위로 연다.
//! 대상이 활성 workspace가 아니어도 지금 창에 바로 보이고, workspace를 전환하지 않는다.
//! 새 요청은 같은 창 범위에서 열린 팝업의 내용과 대상만 바꾼다. 앞 요청의 대상은 각 팝업이 요청 상태를 통째로 바꿔 남기지 않는다.

use crate::adapters::ui::popup::PopupId;
use crate::intent::{DispatchedIntent, UiIntent};
use crate::model::popup_kind::PopupScope;
use crate::state::MainViewState;

/// 창 범위로 팝업 `id`를 연다. 대상 workspace는 호출자가 요청 상태에 먼저 넣는다. `origin`은 요청 출처를 붙인다.
pub(crate) fn open(
    state: &mut MainViewState,
    id: PopupId,
    origin: impl FnOnce(UiIntent) -> DispatchedIntent,
) {
    state.dispatch_intent(origin(UiIntent::ReopenPopup(
        crate::intent::OpenRequest::new(id, PopupScope::Window)
            .centered()
            .focused(),
    )));
}

#[cfg(test)]
mod tests {
    use crate::adapters::ui::LayoutContext;
    use crate::adapters::ui::popup::PopupId;
    use crate::adapters::ui::popup::confirm_close_remote::CONFIRM_CLOSE_REMOTE_POPUP_ID;
    use crate::adapters::ui::popup::confirm_force_detach_workspace::CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID;
    use crate::app::attach_client::workspace_close::SavedRowClose;
    use crate::app::command::DomainIntent;
    use crate::runtime::engine_session::EngineSession;
    use crate::state::{RenameTarget, RequestContext};
    use tasty_type_geometry::length::PhysicalPx;
    use tasty_type_geometry::rect::PhysicalRect;

    /// 저장된 원격 행 workspace ID.
    const ROW: u32 = 900_002;

    /// workspace 1이 활성인 창에서 맨 위에 보이는 팝업.
    fn visible_in_workspace_1(state: &crate::state::MainViewState) -> Option<PopupId> {
        let ctx = LayoutContext {
            active_workspace_id: 1,
            ..LayoutContext::window_only()
        };
        state
            .popups
            .topmost_visible_open(Some(&ctx))
            .map(|(id, _, _)| id)
    }

    /// 정리 단계와 그리기를 포함한 팝업 프레임 하나.
    fn frame(state: &mut RequestContext, session: &EngineSession) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1920.0, 1080.0),
            )),
            ..Default::default()
        };
        let rect = PhysicalRect {
            x: PhysicalPx(0.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(1920.0),
            height: PhysicalPx(1080.0),
        };
        drop(egui::Context::default().run(raw, |ctx| {
            crate::adapters::ui::draw_popups(ctx, state, &session.read(), &[], rect, 1.0);
        }));
    }

    /// 활성 workspace 1 옆에 실제로 있는 비활성 workspace 둘: 점유된 mirror workspace 와 저장된 원격 행 `ROW`.
    fn fixture() -> (RequestContext, EngineSession, u32) {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        let occupied = {
            let mut engine = session.borrow_mut();
            let id = crate::state::tests::add_mirror_test_workspace(&mut state, &mut engine);
            let members = engine
                .workspaces()
                .into_iter()
                .find(|ws| ws.id == id)
                .expect("mirror workspace")
                .all_surface_ids();
            engine
                .live
                .occupancy
                .acquire_workspace(id, &members, &members, 7)
                .expect("mirror workspace is free");
            let mut row =
                crate::model::Workspace::new_with_terminal_marker(ROW, "row".into(), ROW, ROW, ROW);
            row.mirror = true;
            row.remote_row = Some(crate::model::RemoteBinding {
                profile: "gx10".into(),
                remote_workspace: 4,
            });
            engine.core.push_mirror_workspace(row);
            state.set_active_workspace_index(&engine.read(), 0);
            id
        };
        assert_eq!(state.active_workspace(&session.read()).id, 1);
        (state, session, occupied)
    }

    fn open_close_remote(state: &mut RequestContext, row: u32) {
        crate::adapters::ui::popup::confirm_close_remote::open(
            state,
            SavedRowClose { row, last: false },
            DomainIntent::CloseWorkspace { workspace_id: row },
        );
    }

    /// 실제로 있는 비활성 workspace를 다루는 확인·이름 바꾸기 팝업은 프레임을 그린 뒤에도 열려 있고,
    /// workspace를 전환하지 않고 지금 창에 보인다.
    #[test]
    fn a_popup_for_an_inactive_workspace_shows_in_the_current_window() {
        let (mut state, mut session, occupied) = fixture();
        crate::adapters::ui::popup::confirm_force_detach_workspace::open(&mut state, occupied);
        crate::intent::popup::apply_pending(&mut state, &mut session);
        frame(&mut state, &session);
        assert_eq!(
            visible_in_workspace_1(&state),
            Some(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
        );
        assert_eq!(state.dialogs.pending_force_detach_workspace, Some(occupied));
        assert_eq!(state.active_workspace(&session.read()).id, 1);

        let (mut state, mut session, _) = fixture();
        open_close_remote(&mut state, ROW);
        crate::intent::popup::apply_pending(&mut state, &mut session);
        frame(&mut state, &session);
        assert_eq!(
            visible_in_workspace_1(&state),
            Some(CONFIRM_CLOSE_REMOTE_POPUP_ID)
        );
        assert!(
            state
                .dialogs
                .pending_close_remote
                .as_ref()
                .is_some_and(|p| p.close.row == ROW)
        );
        assert_eq!(state.active_workspace(&session.read()).id, 1);

        let (mut state, mut session, occupied) = fixture();
        assert!(crate::adapters::ui::rename_popup::open(
            &mut state,
            &session.read(),
            RenameTarget::WorkspaceName {
                workspace_id: occupied
            },
            String::new(),
            crate::intent::UiIntent::from_user_context_menu
        ));
        crate::intent::popup::apply_pending(&mut state, &mut session);
        frame(&mut state, &session);
        assert_eq!(visible_in_workspace_1(&state), Some("rename"));
        assert!(state.dialogs.rename.is_some());
        assert_eq!(state.active_workspace(&session.read()).id, 1);
    }

    /// 대상 workspace 가 없는 확인은 다음 프레임에 닫히고 대기 상태도 비운다.
    #[test]
    fn a_confirm_for_a_missing_workspace_closes_on_the_next_frame() {
        let (mut state, mut session, _) = fixture();
        crate::adapters::ui::popup::confirm_force_detach_workspace::open(&mut state, 7);
        open_close_remote(&mut state, 9);
        crate::intent::popup::apply_pending(&mut state, &mut session);
        assert!(
            state
                .popups
                .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
        );
        assert!(state.popups.is_open(CONFIRM_CLOSE_REMOTE_POPUP_ID));
        frame(&mut state, &session);
        assert!(
            !state
                .popups
                .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
        );
        assert!(!state.popups.is_open(CONFIRM_CLOSE_REMOTE_POPUP_ID));
        assert!(state.dialogs.pending_force_detach_workspace.is_none());
        assert!(state.dialogs.pending_close_remote.is_none());
    }
}
