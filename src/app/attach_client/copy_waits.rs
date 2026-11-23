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
        self.announce_copy_failures(false);
    }

    /// 끝나는 세션의 mirror surface에서 진행 중인 복사를 지금 실패로 알린다. 종료 처리가 surface를 없앤
    /// 뒤에는 정산이 그 surface를 찾지 못해 안내가 빠진다. 곧 없어질 surface라 창 범위로 알린다.
    pub(super) fn fail_copies_of_ending_sessions(&mut self, ending: &[usize]) {
        let locals: Vec<u32> = ending
            .iter()
            .filter_map(|&index| self.remote.sessions.get(index))
            .flat_map(|session| session.state.remote_to_local.values().copied())
            .collect();
        for local in locals {
            self.attach_copies.break_stream(local);
            self.attach_copies.fail_unanswered(local);
        }
        self.announce_copy_failures(true);
    }

    /// 실패한 복사를 그 surface가 있는 창(parked 포함)에 한 건마다 알린다. `ending`이면 surface가 곧
    /// 사라지므로 창 범위다.
    fn announce_copy_failures(&mut self, ending: bool) {
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
                    Self::copy_failure_scope(ending, state, core, surface),
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

    /// attach 복사 실패 토스트의 범위. 끝나는 세션의 surface는 곧 사라지므로 창 범위이고, 그 밖에는
    /// `copy_result_scope`를 따른다.
    pub(crate) fn copy_failure_scope(
        ending: bool,
        state: &crate::state::MainViewState,
        core: &crate::core::CoreState,
        surface: u32,
    ) -> crate::adapters::ui::ToastScope {
        if ending {
            crate::adapters::ui::ToastScope::Window
        } else {
            Self::copy_result_scope(true, state, core, surface)
        }
    }

    /// 복사 결과 토스트의 범위. attach 복사의 결과는 창이 지금 그 surface를 그리면 Surface, 다른 탭이나
    /// workspace에 있거나 새 탭 대기 화면이 덮으면 창 범위다(Surface 범위 토스트는 그 surface가 화면에
    /// 없으면 바로 사라진다).
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
                .is_some_and(|tab| {
                    tab.layout().find_surface(surface).is_some()
                        && !state.pending_tab_covers(core, tab)
                })
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

    /// Given 같은 pane에 탭 두 개가 있다, When 복사 결과 토스트의 범위를 정한다, Then attach 복사의 결과만
    /// 선택되지 않은 탭의 surface에서 창 범위로 바뀐다.
    #[test]
    fn an_attach_copy_result_for_a_surface_off_screen_goes_to_the_window() {
        use crate::adapters::ui::ToastScope;
        use tasty_core::{DomainEvent, SurfaceSpec};
        let tab = |id: u32, index| DomainEvent::TabCreated {
            id,
            pane: 70_001,
            index,
            name: "Terminal".into(),
            surface: SurfaceSpec {
                id,
                kind: "terminal".into(),
                data: None,
            },
        };
        let model = crate::state::tests::test_model(vec![
            DomainEvent::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            DomainEvent::WorkspaceCreated {
                id: 70_001,
                name: "Workspace 1".into(),
                category: 0,
                index: 0,
                pane: 70_001,
            },
            tab(70_001, 0),
            tab(70_002, 1),
        ]);
        let (state, mut session) = crate::state::tests::test_state_from_model(model);
        let engine = session.borrow_mut();
        let core = &*engine.core;
        let shown = state.focused_surface_id(core).expect("selected tab");
        let hidden = if shown == 70_001 { 70_002 } else { 70_001 };
        assert!(
            core.find_tab_for_surface(hidden).is_some(),
            "트리에 있는 surface"
        );
        let scope =
            |attach, surface| crate::app::App::copy_result_scope(attach, &state, core, surface);
        assert_eq!(scope(true, shown), ToastScope::Surface(shown));
        assert_eq!(scope(true, hidden), ToastScope::Window);
        assert_eq!(scope(false, hidden), ToastScope::Surface(hidden));
    }

    /// Given mirror surface를 보는 창, When 같은 pane에 원격 새 탭 대기 화면이 선택된다, Then 그 surface의
    /// 결과는 창 범위다.
    #[test]
    fn a_pending_new_tab_screen_hides_the_surface_of_its_pane() {
        use crate::adapters::ui::ToastScope;
        let (mut state, mut session) = crate::state::tests::test_mirror_state();
        let engine = session.borrow_mut();
        let read = engine.read();
        let anchor = state.focused_surface_id(&read).expect("anchor");
        let scope = |state: &crate::state::MainViewState| {
            crate::app::App::copy_result_scope(true, state, &read, anchor)
        };
        assert_eq!(scope(&state), ToastScope::Surface(anchor));
        let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
        assert_eq!(scope(&state), ToastScope::Window);
    }

    /// Given 지금 보이는 surface, When 끝나는 세션의 실패를 알린다, Then 곧 사라질 surface라 창 범위다.
    /// 끝나지 않는 세션이면 결과 범위 규칙을 따른다.
    #[test]
    fn a_copy_failure_of_an_ending_session_goes_to_the_window() {
        use crate::adapters::ui::ToastScope;
        let (state, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let core = &*engine.core;
        let shown = state.focused_surface_id(core).expect("selected tab");
        let scope = |ending| crate::app::App::copy_failure_scope(ending, &state, core, shown);
        assert_eq!(scope(true), ToastScope::Window);
        assert_eq!(scope(false), ToastScope::Surface(shown));
    }
}
