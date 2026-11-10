//! 닫힌 surface와 workspace의 도메인 자원을 회수한다.
//! 창 상태 없이 호출할 수 있어 GUI와 headless의 모든 닫기 경로가 같은 정리를 쓴다.

use crate::runtime::engine_access::EngineMut;

impl EngineMut<'_> {
    /// 닫힌 surface의 busy·cwd·전경 이름·mesh frame(mirror 전용)과 attention 레코드를 지운다.
    /// attention은 로컬 surface도 두 빌드에서 가지므로 로컬 레코드도 함께 지운다.
    /// client 쪽 정리는 workspace가 이미 없으면 건너뛰므로 사용자 닫기 경로도 여기서 회수한다.
    fn forget_mirror_surface_extras(&mut self, surface_id: u32) {
        self.remote.mirror_busy_surfaces.remove(&surface_id);
        self.remote.mirror_surface_cwd.remove(&surface_id);
        self.remote.mirror_foreground.forget(surface_id);
        self.forget_mirror_surface_attention(surface_id);
        #[cfg(feature = "gui")]
        self.remote.attach_mesh_frames.remove(surface_id);
    }
}

impl EngineMut<'_> {
    /// Non-View observation teardown for the captured owner in committed retirement.
    /// Call only for the captured old owner, while publication excludes replacement installation.
    pub(crate) fn cleanup_surface_observations(&mut self, surface_id: u32) {
        self.runtime.pending_scrollback_inject.remove(&surface_id);
        self.remote.pending_workspace_taps.remove(&surface_id);
        if self
            .runtime
            .child_terminals
            .unregister_child_by_surface(surface_id)
        {
            self.runtime.child_terminals.save();
        }
        #[cfg(feature = "gui")]
        {
            self.runtime.readonly_views.remove(&surface_id);
            self.runtime.readonly_sources.remove(&surface_id);
        }
        self.runtime
            .pending_submits
            .retain(|submit| submit.surface() != surface_id);
        self.live.surface_titles.remove(&surface_id);
        self.live.last_key_input.remove(&surface_id);
        self.live.busy_surfaces.remove(&surface_id);
        self.live
            .mouse_capture_disabled_surfaces
            .remove(&surface_id);
        self.live.foreground_names.remove(&surface_id);
        self.live.foreground_generation.remove(&surface_id);
        self.live.surface_messages.remove(&surface_id);
        self.forget_closed_surface(surface_id);
        self.forget_mirror_surface_extras(surface_id);
        self.remote.forget_surface_observations(surface_id);
        self.live.command_index.drop_surface(surface_id);
        self.observer_router.drop_surface(surface_id);
        self.hooks.forget_surface(surface_id);
        self.forget_shell_integration_hint(surface_id);
        self.forget_prompt_shown(surface_id);
        if let Some(factory) = self.runtime.waker_factory.as_ref() {
            factory.forget_surface(surface_id);
        }
    }
}

#[cfg(test)]
mod tests {
    /// 사용자가 mirror를 닫으면 client 쪽 정리를 건너뛰므로 이 경로가 전경 이름과 세대를 지운다.
    #[test]
    fn closing_a_surface_forgets_its_mirror_foreground() {
        let mut e_session = crate::state::tests::test_state().1;
        let mut e = e_session.borrow_mut();
        e.set_mirror_surface_foreground(4242, Some("claude".into()));
        e.cleanup_surface_observations(4242);
        assert_eq!(e.mirror_surface_foreground(4242), None);
        assert_eq!(e.foreground_generation(4242), 0);
    }
}
