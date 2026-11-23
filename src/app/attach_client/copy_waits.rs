//! 원격 프로그램의 OSC 52 쓰기 진행을 정산하고, 실패를 그 surface가 보이는 창에 알린다.
use crate::app::App;
use crate::state::attach_pending::waits::{Wait, WaitKind};
use std::collections::HashMap;

impl App {
    /// 사건 처리가 끝난 mirror 터미널의 남은 결과 대기 건을 실패로 닫는다. 연결이 끝났거나(끊김·서버
    /// 닫기·재연결 대기) 재동기화 중이면 본문을 받던 건도 실패다. 사용자가 mirror를 닫았거나 구조 변경으로
    /// surface가 사라졌으면 알리지 않고 버린다.
    pub(super) fn settle_attach_copies(&mut self) {
        for surface in self.attach_copies.surfaces() {
            let pending = self.engines().all().find_map(|engine| {
                engine
                    .find_terminal_by_id(surface)
                    .map(|terminal| terminal.has_pending_events())
            });
            let Some(pending) = pending else {
                self.attach_copies.forget(surface);
                continue;
            };
            let streaming = self.remote.sessions.iter().any(|session| {
                session
                    .state
                    .remote_to_local
                    .values()
                    .any(|&local| local == surface)
                    && session.transport.frame_tx.epoch().is_active()
                    && session.state.resync_pending.is_none()
            });
            if !streaming {
                self.attach_copies.break_stream(surface);
            }
            if !pending {
                self.attach_copies.fail_unanswered(surface);
            }
        }
        self.announce_copy_failures();
    }

    /// 실패한 복사를 그 surface가 있는 창(parked 포함)에 한 건마다 알린다.
    fn announce_copy_failures(&mut self) {
        use crate::view::ui::View as _;
        let failures = self.attach_copies.take_failures();
        if failures.is_empty() {
            return;
        }
        let toast =
            |state: &mut crate::state::MainViewState, core: &crate::core::CoreState, surface| {
                state.toasts.push(
                    crate::i18n::t("toast.clipboard_failed"),
                    crate::adapters::ui::ToastKind::Error,
                    Self::copy_result_scope(true, state, core, surface),
                );
            };
        for (_, main, engine) in self.engines_mut().window_pairs() {
            for &surface in &failures {
                if engine.find_terminal_by_id(surface).is_some() {
                    toast(&mut main.state, engine.core, surface);
                    main.mark_dirty();
                }
            }
        }
        for (_, state, engine) in self.engines_mut().parked_sessions_with_ids() {
            for &surface in &failures {
                if engine.find_terminal_by_id(surface).is_some() {
                    toast(state, engine.core, surface);
                }
            }
        }
    }

    /// 복사 결과 토스트의 범위. attach 복사의 결과는 창이 지금 그 surface를 그리면 Surface, 다른 탭이나
    /// workspace에 있으면 창 범위다(Surface 범위 토스트는 그 surface가 화면에 없으면 바로 사라진다).
    pub(crate) fn copy_result_scope(
        attach: bool,
        state: &crate::state::MainViewState,
        core: &crate::core::CoreState,
        surface: u32,
    ) -> crate::adapters::ui::ToastScope {
        let layout = state.active_workspace(core).pane_layout();
        let shown = layout.all_pane_ids().iter().any(|&pane_id| {
            layout
                .find_pane(pane_id)
                .and_then(|pane| pane.tabs.get(state.navigation.tab_index(pane)))
                .is_some_and(|tab| tab.layout().find_surface(surface).is_some())
        });
        if attach && !shown {
            crate::adapters::ui::ToastScope::Window
        } else {
            crate::adapters::ui::ToastScope::Surface(surface)
        }
    }

    /// 창 engine에 있는 진행 중 복사를 workspace별 대기로 모은다.
    pub(super) fn attach_copy_waits(
        copies: &crate::remote::copy_progress::CopyProgress,
        engine: &crate::runtime::engine_access::EngineMut<'_>,
    ) -> HashMap<u32, Vec<Wait>> {
        let mut waits: HashMap<u32, Vec<Wait>> = HashMap::new();
        for (surface, since) in copies.waits() {
            if let Some((index, _)) = engine.find_workspace_index_for_surface(surface)
                && let Some(workspace) = engine.workspace_at(index)
            {
                waits
                    .entry(workspace.id)
                    .or_default()
                    .push(Wait::new(WaitKind::Copy, since));
            }
        }
        for times in waits.values_mut() {
            times.sort_by_key(|wait| wait.admitted);
        }
        waits
    }
}

#[cfg(test)]
mod tests {
    use crate::state::attach_pending::waits::WaitKind;

    /// Given mirror surface에서 원격 OSC 52 쓰기 본문을 받는 중, When 창의 대기를 모은다, Then 그 surface의
    /// workspace에 Copy 대기 한 건이 생긴다. 끝난 뒤 결과가 처리되면 사라진다.
    #[test]
    fn a_remote_copy_in_progress_is_a_copy_wait_of_its_workspace() {
        let (_state, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let ws = engine.workspace_at(0).expect("workspace").id;
        let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let mut copies = crate::remote::copy_progress::CopyProgress::default();
        copies.feed(sid, b"\x1b]52;c;aGk", std::time::Instant::now());
        let waits = crate::app::App::attach_copy_waits(&copies, &engine);
        assert_eq!(waits.len(), 1);
        assert_eq!(waits[&ws].len(), 1);
        assert_eq!(waits[&ws][0].kind, WaitKind::Copy);
        copies.feed(sid, b"=\x07", std::time::Instant::now());
        assert!(copies.settle_write(sid));
        assert!(crate::app::App::attach_copy_waits(&copies, &engine).is_empty());
    }

    /// Given 창이 지금 그리는 surface와 그리지 않는 surface, When 복사 결과 토스트의 범위를 정한다, Then
    /// attach 복사의 결과만 그리지 않는 surface에서 창 범위로 바뀐다.
    #[test]
    fn an_attach_copy_result_for_a_surface_off_screen_goes_to_the_window() {
        use crate::adapters::ui::ToastScope;
        let (state, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let shown = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let hidden = shown + 10_000;
        let scope = |attach, surface| {
            crate::app::App::copy_result_scope(attach, &state, &*engine.core, surface)
        };
        assert_eq!(scope(true, shown), ToastScope::Surface(shown));
        assert_eq!(scope(true, hidden), ToastScope::Window);
        assert_eq!(scope(false, hidden), ToastScope::Surface(hidden));
    }
}
