//! 원격 mirror의 전경 프로세스 이름. 로컬 PTY가 없어 서버 push로 받는다.
//! 1Hz 로컬 폴링이 `live.foreground_names`를 통째로 덮어쓰므로 저장소를 따로 둔다.

use crate::runtime::engine_access::EngineMut;
use std::collections::HashMap;

/// mirror surface별 원격 전경 이름과 그 세대. 세대 규칙은 로컬 폴링과 같다.
#[derive(Default)]
pub(crate) struct MirrorForegrounds {
    names: HashMap<u32, String>,
    generations: HashMap<u32, u64>,
}

impl MirrorForegrounds {
    pub(crate) fn name(&self, surface_id: u32) -> Option<&str> {
        self.names.get(&surface_id).map(String::as_str)
    }

    #[cfg(any(feature = "gui", test))]
    pub(crate) fn generation(&self, surface_id: u32) -> Option<u64> {
        self.generations.get(&surface_id).copied()
    }

    /// 이름이 바뀌면 세대를 올린다. None은 로컬 폴링이 이름을 잃을 때처럼 세대까지 지운다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn set(&mut self, surface_id: u32, name: Option<String>) {
        match name {
            Some(name) => {
                if self.names.get(&surface_id) != Some(&name) {
                    *self.generations.entry(surface_id).or_insert(0) += 1;
                }
                self.names.insert(surface_id, name);
            }
            None => self.forget(surface_id),
        }
    }

    pub(crate) fn forget(&mut self, surface_id: u32) {
        self.names.remove(&surface_id);
        self.generations.remove(&surface_id);
    }

    pub(crate) fn names(&self) -> impl Iterator<Item = (u32, &str)> {
        self.names.iter().map(|(&sid, name)| (sid, name.as_str()))
    }
}

impl EngineMut<'_> {
    /// 원격 push 값을 적용한다. 호출자가 mirror ID를 넘기는지 여기서는 검증하지 않는다.
    #[cfg(any(feature = "gui", test))]
    pub fn set_mirror_surface_foreground(&mut self, surface_id: u32, name: Option<String>) {
        self.remote.mirror_foreground.set(surface_id, name);
    }

    #[cfg(any(feature = "gui", test))]
    pub fn forget_mirror_surface_foreground(&mut self, surface_id: u32) {
        self.remote.mirror_foreground.forget(surface_id);
    }

    #[cfg(test)]
    pub fn mirror_surface_foreground(&self, surface_id: u32) -> Option<&str> {
        self.remote.mirror_foreground.name(surface_id)
    }

    /// 준비된 하드 점유 surface의 (holder, ID, 이름) 전송 후보. 최초 None도 포함한다.
    /// 준비 전 점유는 후보와 캐시에서 빼서 descriptor 뒤 첫 호출에 최초 값을 낸다.
    /// holder와 값으로 중복을 구분하고 점유가 끝난 캐시는 지운다.
    /// 전송 전에 캐시를 갱신하므로 실패 후 같은 값의 재전송은 보장하지 않는다.
    pub fn surface_foreground_forwards(
        &mut self,
    ) -> Vec<(crate::core::attach::AttachClientId, u32, Option<String>)> {
        let locks = self.live.occupancy.ready_locks_snapshot();
        let occupied: std::collections::HashSet<u32> = locks.iter().map(|&(sid, _)| sid).collect();
        self.remote
            .last_forwarded_foreground
            .retain(|sid, _| occupied.contains(sid));
        let mut out = Vec::new();
        for (sid, lock) in locks {
            let name = self.as_ref().foreground_name(sid).map(str::to_owned);
            let record = (lock.holder, name);
            if self.remote.last_forwarded_foreground.get(&sid) != Some(&record) {
                out.push((record.0, sid, record.1.clone()));
                self.remote.last_forwarded_foreground.insert(sid, record);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    fn engine() -> crate::runtime::engine_session::EngineSession {
        crate::state::tests::test_state().1
    }

    fn first_surface(e: &crate::runtime::engine_access::EngineMut<'_>) -> u32 {
        e.workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0]
    }

    #[test]
    fn foreground_forwards_only_on_change_and_clears_with_none() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let sid = first_surface(&e);
        e.live.occupancy.acquire(sid, 7).expect("lock");
        e.live.foreground_names.insert(sid, "claude".into());

        let first = e.surface_foreground_forwards();
        assert_eq!(first, vec![(7, sid, Some("claude".to_string()))], "최초");
        assert!(e.surface_foreground_forwards().is_empty(), "같은 값");

        e.live.foreground_names.insert(sid, "vim".into());
        assert_eq!(
            e.surface_foreground_forwards(),
            vec![(7, sid, Some("vim".to_string()))]
        );

        e.live.foreground_names.remove(&sid);
        assert_eq!(
            e.surface_foreground_forwards(),
            vec![(7, sid, None)],
            "이름을 잃으면 None을 보낸다"
        );
    }

    #[test]
    fn foreground_forwards_waits_for_a_ready_lock() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let sid = first_surface(&e);
        let grant = e.live.occupancy.acquire(sid, 7).expect("lock").granted_seq;
        e.live.occupancy.set_attachment_ready(7, grant, false);
        assert!(e.surface_foreground_forwards().is_empty());
        assert!(e.remote.last_forwarded_foreground.is_empty());

        e.live.occupancy.set_attachment_ready(7, grant, true);
        assert_eq!(e.surface_foreground_forwards(), vec![(7, sid, None)]);
        assert!(e.surface_foreground_forwards().is_empty());
    }

    #[test]
    fn holder_swap_within_one_tick_pushes_to_the_new_holder() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let sid = first_surface(&e);
        e.live.foreground_names.insert(sid, "claude".into());
        e.live.occupancy.acquire(sid, 7).expect("lock");
        assert_eq!(e.surface_foreground_forwards().len(), 1);

        e.live.occupancy.release(sid, 7).expect("release");
        e.live.occupancy.acquire(sid, 8).expect("lock by another");
        assert_eq!(
            e.surface_foreground_forwards(),
            vec![(8, sid, Some("claude".to_string()))]
        );
    }

    #[test]
    fn released_then_reacquired_pushes_baseline_again() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let sid = first_surface(&e);
        e.live.occupancy.acquire(sid, 7).expect("lock");
        assert_eq!(e.surface_foreground_forwards().len(), 1);
        e.live.occupancy.release(sid, 7).expect("release");
        assert!(e.surface_foreground_forwards().is_empty());
        assert!(e.remote.last_forwarded_foreground.is_empty());
        e.live.occupancy.acquire(sid, 7).expect("re-lock");
        assert_eq!(e.surface_foreground_forwards().len(), 1);
    }

    /// 1Hz 로컬 폴링은 mirror 값을 지우지 않는다.
    #[test]
    fn refresh_busy_surfaces_does_not_clobber_the_mirror_name() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.set_mirror_surface_foreground(42, Some("claude".into()));
        e.refresh_busy_surfaces();
        assert_eq!(e.mirror_surface_foreground(42), Some("claude"));
        assert_eq!(e.as_ref().foreground_name(42), Some("claude"));
        #[cfg(feature = "gui")]
        assert_eq!(e.read().foreground_name(42), Some("claude"));
    }

    #[test]
    fn a_mirror_name_change_bumps_the_generation_and_none_resets_it() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        assert_eq!(e.foreground_generation(42), 0);
        e.set_mirror_surface_foreground(42, Some("vim".into()));
        assert_eq!(e.foreground_generation(42), 1);
        e.set_mirror_surface_foreground(42, Some("vim".into()));
        assert_eq!(e.foreground_generation(42), 1, "같은 이름");
        e.set_mirror_surface_foreground(42, Some("htop".into()));
        assert_eq!(e.foreground_generation(42), 2);
        #[cfg(feature = "gui")]
        assert_eq!(
            e.read().foreground_generation(42),
            2,
            "View 읽기도 같은 세대"
        );
        e.set_mirror_surface_foreground(42, None);
        assert_eq!(e.as_ref().foreground_name(42), None);
        assert_eq!(e.foreground_generation(42), 0);
    }

    #[test]
    fn forget_drops_the_name_and_generation() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.set_mirror_surface_foreground(42, Some("claude".into()));
        e.forget_mirror_surface_foreground(42);
        assert_eq!(e.mirror_surface_foreground(42), None);
        assert_eq!(e.foreground_generation(42), 0);
    }

    /// 마우스 캡처 제외 목록은 mirror의 원격 이름에도 적용되고 폴링이 그 판정을 지우지 않는다.
    #[test]
    fn the_capture_exemption_covers_a_mirror_name() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.runtime.settings.general.mouse_capture_blacklist = vec!["vim".to_string()];
        e.set_mirror_surface_foreground(42, Some("vim".into()));
        e.set_mirror_surface_foreground(43, Some("htop".into()));
        e.refresh_busy_surfaces();
        assert!(e.is_surface_mouse_capture_disabled(42));
        assert!(!e.is_surface_mouse_capture_disabled(43));
        e.set_mirror_surface_foreground(42, None);
        e.refresh_busy_surfaces();
        assert!(!e.is_surface_mouse_capture_disabled(42));
    }
}
