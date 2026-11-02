//! 첫 출력 뒤 OSC 133 경계가 일정 시간 보이지 않으면 surface마다 한 번 안내를 요청한다.
//! 첫 출력 시각은 Terminal이 기록한 값을 주기 확인 때 읽는다. `OutputAppended`는 훅·observer가
//! 있을 때만 켜지므로 그 이벤트에 기대면 일반 surface에서는 판정이 시작되지 않는다.
//! 설치 여부를 직접 검사하지 않으며 배너 표시나 자동 설정 변경은 여기서 하지 않는다.
//! 창에 배너를 이미 보여줬는지는 창 상태가 기록한다.

use crate::runtime::engine_access::EngineMut;

/// 정상 셸의 첫 프롬프트를 기다리기 위한 최소 시간. 늦은 출력의 오탐까지 막지는 못한다.
#[cfg(any(feature = "gui", test))]
const SHELL_INTEGRATION_HINT_DELAY: std::time::Duration = std::time::Duration::from_secs(10);

impl EngineMut<'_> {
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn note_first_output(&mut self, surface_id: u32, at: std::time::Instant) {
        self.live
            .shell_integration_first_output_at
            .entry(surface_id)
            .or_insert(at);
    }

    /// 판정이 끝나지 않은 surface마다 Terminal의 첫 출력 시각을 읽고 마감이 지난 surface를 반환한다.
    /// 경계를 받았거나 이미 요청한 surface는 Terminal을 읽지 않는다. 판정 대상은 로컬 셸 surface뿐이다.
    /// - standalone PTY는 surface가 아니므로 제외한다.
    /// - 로컬 PTY가 없는 원격 attach mirror는 제외한다. 첫 내용인 스냅샷에 OSC 133이 없어 경계를
    ///   받지 못하고, 안내 문구가 권하는 로컬 셸 통합 설치도 원격 셸에는 맞지 않는다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn take_due_shell_integration_hints(
        &mut self,
    ) -> Vec<(u32, tasty_terminal::ResourceGeneration)> {
        let live = &*self.live;
        let terminals = &self.runtime.terminals;
        let pending: Vec<(u32, Option<std::time::Instant>)> = terminals
            .iter()
            .filter(|(sid, _)| {
                !live.shell_integration_boundary_seen.contains(sid)
                    && !live.shell_integration_hint_requested.contains(sid)
                    && terminals
                        .pty(*sid)
                        .is_some_and(|pty| pty.state().standalone().is_none())
            })
            .map(|(sid, t)| {
                let first = match live.shell_integration_first_output_at.get(&sid) {
                    Some(_) => None,
                    None => t.first_output_at(),
                };
                (sid, first)
            })
            .collect();
        let mut due = Vec::new();
        for (sid, first) in pending {
            if let Some(at) = first {
                self.note_first_output(sid, at);
            }
            if self.take_shell_integration_hint_due(sid)
                && let Some(generation) = self.runtime.terminals.generation(sid)
            {
                due.push((sid, generation));
            }
        }
        due
    }

    pub(crate) fn note_prompt_boundary_seen(&mut self, surface_id: u32) {
        self.live.shell_integration_boundary_seen.insert(surface_id);
    }

    /// 경계를 받지 못하고 지연 시간이 지나면 surface마다 한 번 true를 반환한다.
    /// 이벤트를 보내기 전에 표시를 남기므로 소비자가 처리하지 못해도 다시 요청하지 않는다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn take_shell_integration_hint_due(&mut self, surface_id: u32) -> bool {
        if self
            .live
            .shell_integration_boundary_seen
            .contains(&surface_id)
        {
            return false;
        }
        if self
            .live
            .shell_integration_hint_requested
            .contains(&surface_id)
        {
            return false;
        }
        let Some(&first_output) = self.live.shell_integration_first_output_at.get(&surface_id)
        else {
            return false;
        };
        if first_output.elapsed() < SHELL_INTEGRATION_HINT_DELAY {
            return false;
        }
        self.live
            .shell_integration_hint_requested
            .insert(surface_id);
        true
    }

    pub(crate) fn forget_shell_integration_hint(&mut self, surface_id: u32) {
        self.live
            .shell_integration_first_output_at
            .remove(&surface_id);
        self.live
            .shell_integration_boundary_seen
            .remove(&surface_id);
        self.live
            .shell_integration_hint_requested
            .remove(&surface_id);
    }
}

#[cfg(test)]
mod tests {

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    #[test]
    fn no_hint_before_first_output_recorded() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        assert!(!e.take_shell_integration_hint_due(1));
    }

    #[test]
    fn no_hint_immediately_after_first_output() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.note_first_output(1, std::time::Instant::now());
        assert!(!e.take_shell_integration_hint_due(1));
    }

    #[test]
    fn boundary_seen_suppresses_hint_permanently() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.note_first_output(1, std::time::Instant::now());
        e.note_prompt_boundary_seen(1);
        e.live.shell_integration_first_output_at.insert(
            1,
            std::time::Instant::now() - std::time::Duration::from_secs(999),
        );
        assert!(!e.take_shell_integration_hint_due(1));
    }

    /// 이후 출력이 계속 와도 같은 surface의 요청은 다시 나오지 않는다.
    #[test]
    fn hint_fires_once_after_delay_elapsed_without_boundary() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.live.shell_integration_first_output_at.insert(
            1,
            std::time::Instant::now() - std::time::Duration::from_secs(999),
        );
        assert!(e.take_shell_integration_hint_due(1));
        assert!(!e.take_shell_integration_hint_due(1));
        e.note_first_output(1, std::time::Instant::now());
        assert!(!e.take_shell_integration_hint_due(1));
    }

    #[test]
    fn different_surfaces_are_independent() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.live.shell_integration_first_output_at.insert(
            1,
            std::time::Instant::now() - std::time::Duration::from_secs(999),
        );
        e.note_prompt_boundary_seen(2);
        e.live.shell_integration_first_output_at.insert(
            2,
            std::time::Instant::now() - std::time::Duration::from_secs(999),
        );
        assert!(e.take_shell_integration_hint_due(1));
        assert!(!e.take_shell_integration_hint_due(2));
    }

    /// 다른 surface를 함께 두어 하나만 삭제하는지 확인한다.
    #[test]
    fn forget_shell_integration_hint_clears_all_three_caches() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        for sid in [1, 2] {
            e.note_first_output(sid, std::time::Instant::now());
            e.note_prompt_boundary_seen(sid);
            e.live.shell_integration_hint_requested.insert(sid);
        }
        e.forget_shell_integration_hint(1);
        assert!(!e.live.shell_integration_first_output_at.contains_key(&1));
        assert!(!e.live.shell_integration_boundary_seen.contains(&1));
        assert!(!e.live.shell_integration_hint_requested.contains(&1));
        assert!(e.live.shell_integration_first_output_at.contains_key(&2));
        assert!(e.live.shell_integration_boundary_seen.contains(&2));
        assert!(e.live.shell_integration_hint_requested.contains(&2));
    }
}
