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
        let failures = self.attach_copies.take_failures();
        if failures.is_empty() {
            return;
        }
        for (_, main, engine) in self.engines_mut().window_pairs() {
            for &surface in &failures {
                if engine.find_terminal_by_id(surface).is_some() {
                    main.toasts.push(
                        crate::i18n::t("toast.clipboard_failed"),
                        crate::adapters::ui::ToastKind::Error,
                        crate::adapters::ui::ToastScope::Surface(surface),
                    );
                    main.mark_dirty();
                }
            }
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
}
