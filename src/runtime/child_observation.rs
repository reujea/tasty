//! Collect runtime evidence; the child-state policy remains a pure function in Core.
use crate::core::state::child_liveness::{ChildLiveness, ChildObservation, derive_child_state};
use crate::runtime::engine_access::{EngineMut, EngineRef};
use std::collections::HashSet;

impl EngineRef<'_> {
    /// 라이브 집합과 전경 이름 캐시를 재사용해 자식마다 전체 트리·프로세스를 다시 조회하지 않는다.
    fn observe_child(&self, child_surface: u32, live: &HashSet<u32>) -> ChildObservation {
        ChildObservation {
            surface_live: live.contains(&child_surface),
            pty_ready: self.runtime.terminals.contains(child_surface),
            busy: self.is_surface_busy(child_surface),
            foreground_is_shell: self
                .foreground_name(child_surface)
                .map(tasty_terminal::foreground_process::is_known_shell_name),
            output_silence: self
                .find_terminal_by_id(child_surface)
                .map(|t| t.silence_since().elapsed()),
            hook_silence: self.runtime.child_terminals.hook_silence(
                child_surface,
                crate::runtime::child_terminal::now_epoch_ms(),
            ),
        }
    }

    /// 목록 조회와 단건 조회가 같은 상태 판정을 사용한다.
    pub fn child_liveness_with_live(
        &self,
        child_surface: u32,
        live: &HashSet<u32>,
    ) -> ChildLiveness {
        let obs = self.observe_child(child_surface, live);
        derive_child_state(self.runtime.child_terminals.state_of(child_surface), &obs)
    }

    pub fn child_liveness(&self, child_surface: u32) -> ChildLiveness {
        let live = self.live_surface_ids();
        self.child_liveness_with_live(child_surface, &live)
    }
}

impl EngineMut<'_> {
    /// 목록 조회와 단건 조회가 같은 상태 판정을 사용한다.
    pub fn child_liveness_with_live(
        &self,
        child_surface: u32,
        live: &HashSet<u32>,
    ) -> ChildLiveness {
        self.as_ref().child_liveness_with_live(child_surface, live)
    }

    pub fn child_liveness(&self, child_surface: u32) -> ChildLiveness {
        self.as_ref().child_liveness(child_surface)
    }
}
