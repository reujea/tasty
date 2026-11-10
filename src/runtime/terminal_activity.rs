//! PTY 전경 이름으로 busy·마우스 캡처 설정을 계산하고 원격 busy 값은 따로 보관한다.
//! mirror의 캡처 설정은 서버가 보낸 원격 전경 이름으로 계산한다.

use crate::runtime::engine_access::EngineMut;

impl EngineMut<'_> {
    /// 마지막 폴링의 캡처 제외 설정. 클릭·드래그를 로컬 선택으로 처리하고 휠은 그대로 둔다.
    #[cfg(test)]
    pub fn is_surface_mouse_capture_disabled(&self, surface_id: u32) -> bool {
        self.live
            .mouse_capture_disabled_surfaces
            .contains(&surface_id)
    }

    /// 로컬 폴링 또는 원격 push 중 하나가 busy이면 true다.
    #[cfg(test)]
    pub fn is_surface_busy(&self, surface_id: u32) -> bool {
        self.is_locally_or_mirror_busy(surface_id)
    }

    /// 마지막 폴링의 전경 이름. 로컬 값이 없으면 원격 mirror가 받은 값이다.
    /// 아직 해석하지 못한 surface는 None이다.
    #[cfg(feature = "gui")]
    pub fn foreground_name(&self, surface_id: u32) -> Option<&str> {
        self.live
            .foreground_names
            .get(&surface_id)
            .map(String::as_str)
            .or_else(|| self.remote.mirror_foreground.name(surface_id))
    }

    /// 관측한 전경 이름이 바뀔 때 증가한다. 배너가 이전 프로그램의 것인지 판단하는 데 쓴다.
    /// 같은 이름의 프로그램이 폴링 사이에 재실행된 경우는 구분하지 못한다.
    #[cfg(any(feature = "gui", test))]
    pub fn foreground_generation(&self, surface_id: u32) -> u64 {
        self.live
            .foreground_generation
            .get(&surface_id)
            .copied()
            .or_else(|| self.remote.mirror_foreground.generation(surface_id))
            .unwrap_or(0)
    }

    /// 로컬 폴링 결과와 원격 push 결과의 합집합.
    #[cfg(test)]
    fn is_locally_or_mirror_busy(&self, surface_id: u32) -> bool {
        self.live.busy_surfaces.contains(&surface_id)
            || self.remote.mirror_busy_surfaces.contains(&surface_id)
    }

    #[cfg(test)]
    pub fn any_busy(&self, surface_ids: &[u32]) -> bool {
        surface_ids
            .iter()
            .any(|&sid| self.is_locally_or_mirror_busy(sid))
    }

    #[cfg(test)]
    pub fn busy_count(&self, surface_ids: &[u32]) -> usize {
        surface_ids
            .iter()
            .filter(|&&sid| self.is_locally_or_mirror_busy(sid))
            .count()
    }

    /// 원격 push 값을 적용한다. 호출자가 mirror ID를 넘기는지 여기서는 검증하지 않는다.
    #[cfg(any(feature = "gui", test))]
    pub fn set_mirror_surface_busy(&mut self, surface_id: u32, busy: bool) {
        if busy {
            self.remote.mirror_busy_surfaces.insert(surface_id);
        } else {
            self.remote.mirror_busy_surfaces.remove(&surface_id);
        }
    }

    #[cfg(any(feature = "gui", test))]
    pub fn forget_mirror_surface_busy(&mut self, surface_id: u32) {
        self.remote.mirror_busy_surfaces.remove(&surface_id);
    }

    /// 준비된 하드 점유 surface의 로컬 busy 전송 후보. 최초 값과 holder·busy 변경을 담는다.
    /// 준비 전 점유는 후보와 캐시에서 빼서 descriptor 뒤 첫 호출에 최초 값을 낸다.
    /// 점유가 끝난 캐시는 지우고 같은 조회 간격 안의 holder 변경도 구분한다.
    /// 후보 생성 시 캐시를 갱신하므로 전송에 실패해도 같은 값은 재시도하지 않는다.
    pub fn busy_activity_forwards(
        &mut self,
    ) -> Vec<(crate::core::attach::AttachClientId, u32, bool)> {
        let locks = self.live.occupancy.ready_locks_snapshot();
        let occupied: std::collections::HashSet<u32> = locks.iter().map(|&(sid, _)| sid).collect();
        self.remote
            .last_forwarded_busy
            .retain(|sid, _| occupied.contains(sid));
        let mut out = Vec::new();
        for (sid, lock) in locks {
            let record = (lock.holder, self.live.busy_surfaces.contains(&sid));
            if self.remote.last_forwarded_busy.get(&sid) != Some(&record) {
                self.remote.last_forwarded_busy.insert(sid, record);
                out.push((record.0, sid, record.1));
            }
        }
        out
    }
}

/// 전경 이름이 달라지면 번호를 올리고 더는 관측하지 못한 surface는 제거한다.
fn bump_foreground_generations(
    generations: &mut std::collections::HashMap<u32, u64>,
    old_names: &std::collections::HashMap<u32, String>,
    new_names: &std::collections::HashMap<u32, String>,
) {
    for (&sid, new_name) in new_names.iter() {
        if old_names.get(&sid) != Some(new_name) {
            *generations.entry(sid).or_insert(0) += 1;
        }
    }
    generations.retain(|sid, _| new_names.contains_key(sid));
}

impl EngineMut<'_> {
    /// 로컬 PTY를 폴링한다. 반환값은 busy 집합 변화만 나타내며 이름·캡처 설정 변화는 제외한다.
    pub fn refresh_busy_surfaces(&mut self) -> bool {
        // Windows에서 surface마다 전체 프로세스를 다시 조회하지 않도록 한 번에 해석한다.
        let mut sids: Vec<u32> = Vec::new();
        let mut shell_pids: Vec<u32> = Vec::new();
        for (sid, _) in self.runtime.terminals.iter() {
            if let Some(pid) = self
                .runtime
                .terminals
                .pty(sid)
                .and_then(|pty| pty.process_id())
            {
                sids.push(sid);
                shell_pids.push(pid);
            }
        }
        let foregrounds = tasty_terminal::foreground_process::resolve_foreground_many(&shell_pids);

        let mut busy: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut mouse_capture_disabled: std::collections::HashSet<u32> =
            std::collections::HashSet::new();
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        for ((&sid, &shell_pid), fg) in sids.iter().zip(shell_pids.iter()).zip(foregrounds.iter()) {
            let Some(terminal) = self.runtime.terminals.get_mut(sid) else {
                continue;
            };
            if terminal.busy_with_foreground(shell_pid, fg.as_ref()) {
                busy.insert(sid);
            }
            if let Some(f) = fg.as_ref() {
                if self
                    .runtime
                    .settings
                    .general
                    .mouse_capture_disabled_for(&f.name)
                {
                    mouse_capture_disabled.insert(sid);
                }
                names.insert(sid, f.name.clone());
            }
        }
        // mirror는 로컬 PTY가 없어 원격 이름으로 판정한다. 아래에서 집합을 교체하므로 함께 계산한다.
        for (sid, name) in self.remote.mirror_foreground.names() {
            if self
                .runtime
                .settings
                .general
                .mouse_capture_disabled_for(name)
            {
                mouse_capture_disabled.insert(sid);
            }
        }
        bump_foreground_generations(
            &mut self.live.foreground_generation,
            &self.live.foreground_names,
            &names,
        );

        self.live.mouse_capture_disabled_surfaces = mouse_capture_disabled;
        self.live.foreground_names = names;
        let changed = self.live.busy_surfaces != busy;
        self.live.busy_surfaces = busy;
        changed
    }
}

#[cfg(test)]
mod tests {

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    #[test]
    fn mirror_busy_surface_counts_without_local_pty() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        assert!(!e.is_surface_busy(42), "초기 상태는 idle");
        e.set_mirror_surface_busy(42, true);
        assert!(e.is_surface_busy(42));
        assert_eq!(e.busy_count(&[42]), 1);
        assert!(e.any_busy(&[42]));
        e.set_mirror_surface_busy(42, false);
        assert!(!e.is_surface_busy(42));
        assert_eq!(e.busy_count(&[42]), 0);
    }

    #[test]
    fn refresh_busy_surfaces_does_not_clobber_mirror_busy() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.set_mirror_surface_busy(42, true);
        e.refresh_busy_surfaces(); // 로컬 터미널이 없으니 busy_surfaces 는 빈 채로 재계산.
        assert!(
            e.is_surface_busy(42),
            "로컬 refresh 가 mirror 의 busy 상태를 지우면 안 된다"
        );
    }

    #[test]
    fn forget_mirror_surface_busy_clears_entry() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.set_mirror_surface_busy(42, true);
        e.forget_mirror_surface_busy(42);
        assert!(!e.is_surface_busy(42));
    }

    #[test]
    fn refresh_busy_surfaces_replaces_the_mouse_capture_cache() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.live.mouse_capture_disabled_surfaces.insert(99);
        e.refresh_busy_surfaces(); // 로컬 터미널이 없으니 빈 채로 재계산.
        assert!(!e.is_surface_mouse_capture_disabled(99));
    }

    #[test]
    fn busy_activity_forwards_only_on_change() {
        let mut e_session = crate::state::tests::test_state().1;
        let mut e = e_session.borrow_mut();
        let sid = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.live.occupancy.acquire(sid, 7).expect("lock 획득");

        let first = e.busy_activity_forwards();
        assert_eq!(first, vec![(7, sid, false)]);

        assert!(e.busy_activity_forwards().is_empty());

        e.live.busy_surfaces.insert(sid);
        assert_eq!(e.busy_activity_forwards(), vec![(7, sid, true)]);

        assert!(e.busy_activity_forwards().is_empty());
    }

    /// 준비 전 점유는 후보도 캐시도 만들지 않는다. 준비 뒤 첫 호출이 그 사이 바뀐 현재 값을
    /// 최초 값으로 한 번 낸다.
    #[test]
    fn busy_activity_forwards_waits_for_a_ready_lock() {
        let mut e_session = crate::state::tests::test_state().1;
        let mut e = e_session.borrow_mut();
        let sid = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let grant = e
            .live
            .occupancy
            .acquire(sid, 7)
            .expect("lock 획득")
            .granted_seq;
        e.live.occupancy.set_attachment_ready(7, grant, false);
        assert!(e.busy_activity_forwards().is_empty());
        e.live.busy_surfaces.insert(sid);
        assert!(e.busy_activity_forwards().is_empty());
        assert!(e.remote.last_forwarded_busy.is_empty());

        e.live.occupancy.set_attachment_ready(7, grant, true);
        assert_eq!(e.busy_activity_forwards(), vec![(7, sid, true)]);
        assert!(e.busy_activity_forwards().is_empty());
    }

    #[test]
    fn busy_activity_forwards_resets_on_reacquire() {
        let mut e_session = crate::state::tests::test_state().1;
        let mut e = e_session.borrow_mut();
        let sid = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.live.occupancy.acquire(sid, 7).expect("lock 획득");
        assert_eq!(e.busy_activity_forwards(), vec![(7, sid, false)]);
        assert!(e.busy_activity_forwards().is_empty());

        e.live.occupancy.release(sid, 7).expect("release");
        assert!(e.busy_activity_forwards().is_empty());

        e.live
            .occupancy
            .acquire(sid, 9)
            .expect("다른 client 재획득");
        assert_eq!(
            e.busy_activity_forwards(),
            vec![(9, sid, false)],
            "재획득 후에는 값이 이전과 같아도(false) 새 holder 에게 다시 push"
        );
    }

    #[test]
    fn busy_activity_forwards_holder_swap_within_one_tick_pushes_to_the_new_holder() {
        let mut e_session = crate::state::tests::test_state().1;
        let mut e = e_session.borrow_mut();
        let sid = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.live.busy_surfaces.insert(sid);
        e.live.occupancy.acquire(sid, 7).expect("lock 획득");
        assert_eq!(e.busy_activity_forwards(), vec![(7, sid, true)]);

        e.live.occupancy.release(sid, 7).expect("release");
        e.live
            .occupancy
            .acquire(sid, 9)
            .expect("같은 tick 창 안의 다른 client 획득");
        assert_eq!(
            e.busy_activity_forwards(),
            vec![(9, sid, true)],
            "값이 그대로여도 새 holder 는 초기 push 를 받아야 한다"
        );
        assert!(e.busy_activity_forwards().is_empty());
    }

    #[test]
    fn foreground_generation_bumps_on_name_change_and_holds_when_unchanged() {
        let mut gens = std::collections::HashMap::new();
        let mut old = std::collections::HashMap::new();
        let mut new = std::collections::HashMap::new();

        new.insert(7u32, "vim".to_string());
        super::bump_foreground_generations(&mut gens, &old, &new);
        assert_eq!(gens[&7], 1);

        old = new.clone();
        super::bump_foreground_generations(&mut gens, &old, &new);
        assert_eq!(gens[&7], 1);

        old = new.clone();
        new.insert(7, "bash".to_string());
        super::bump_foreground_generations(&mut gens, &old, &new);
        assert_eq!(gens[&7], 2);

        old = new.clone();
        new.insert(7, "htop".to_string());
        super::bump_foreground_generations(&mut gens, &old, &new);
        assert_eq!(gens[&7], 3);
    }

    /// 일부만 정리하는지 구분하려면 사라진 surface와 계속 관측되는 surface가 모두 필요하다.
    #[test]
    fn foreground_generation_prunes_entries_for_surfaces_no_longer_resolved() {
        let mut gens = std::collections::HashMap::new();
        gens.insert(8u32, 0u64);
        gens.insert(9u32, 3u64);
        let old = std::collections::HashMap::new();
        let new = std::collections::HashMap::from([(8u32, "sh".to_string())]);
        super::bump_foreground_generations(&mut gens, &old, &new);
        assert!(!gens.contains_key(&9));
        assert_eq!(gens.get(&8), Some(&1));
    }

    #[test]
    fn foreground_generation_accessor_reads_cache_and_defaults_to_zero() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        assert_eq!(e.foreground_generation(42), 0);
        e.live.foreground_generation.insert(42, 5);
        assert_eq!(e.foreground_generation(42), 5);
    }
}
