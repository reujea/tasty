//! 부모 surface에 연결된 soft 점유. 표시용이며 로컬 쓰기를 막지 않는다.
//! 임의 점유용 IPC 메서드 없이 child-terminal 처리에서 내부 호출한다.

use crate::core::attach::OccupancyError;
use crate::core::child_parent::ParentKey;

impl crate::runtime::engine_access::EngineMut<'_> {
    /// 같은 parent는 라벨을 갱신하고 다른 점유자가 있으면 오류다. 대상 ID는 호출자가 지정한다.
    pub fn occupy_soft(
        &mut self,
        surface_id: u32,
        parent: impl Into<ParentKey>,
        label: Option<String>,
    ) -> Result<(), OccupancyError> {
        self.live
            .occupancy
            .acquire_soft(surface_id, parent.into(), label)
    }

    /// 기록된 parent만 해제할 수 있다. 점유 부재와 parent 불일치는 오류로 구분한다.
    pub fn release_soft_occupancy(
        &mut self,
        surface_id: u32,
        parent: impl Into<ParentKey>,
    ) -> Result<(), OccupancyError> {
        self.live.occupancy.release_soft(surface_id, &parent.into())
    }

    /// 로컬 사용자 강제 해제. hard 점유는 IPC `attach.force_detach`와 같은 `force_detach`로 푼다
    /// (workspace 멤버면 workspace 점유 전체). soft 점유는 stream client가 아니므로 통지 없이 지운다.
    /// 사용자 조작 전용이며 에이전트 경로(terminal.kill 등)는 hard 점유를 풀지 않는다.
    #[cfg(any(feature = "gui", test))]
    pub fn release_occupancy(&mut self, surface_id: u32) -> bool {
        if self.force_detach(surface_id).is_some() {
            return true;
        }
        self.live.occupancy.clear_soft(surface_id)
    }

    /// 사용자 포커스 때 parent surface가 트리에 없는 soft 점유를 정리한다.
    /// 연결 EOF로 수명을 알 수 없는 점유이므로 이 시점에 확인한다.
    /// External 부모는 surface가 아니므로 트리 부재로 정리하지 않는다. 자식 관계 해제가 정리한다.
    #[cfg(any(feature = "gui", test))]
    pub fn reconcile_soft_occupancy_on_focus(&mut self, surface_id: u32) {
        let Some(occ) = self.live.occupancy.occupancy_of(surface_id) else {
            return;
        };
        if occ.tier != crate::core::attach::OccupancyTier::Soft {
            return; // hard 는 연결 EOF/force-detach 수명 — 이 경로 무관.
        }
        if let Some(ParentKey::Surface(parent)) = occ.parent
            && self.find_surface_by_id(parent).is_none()
        {
            self.live.occupancy.clear_soft(surface_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::child_parent::ParentKey;

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    #[test]
    fn occupy_soft_records_parent_without_hard_predicate() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.occupy_soft(5000, 99, Some("agent".into())).unwrap();
        assert!(!e.live.occupancy.is_hard_occupied(5000));
        let occ = e.live.occupancy.occupancy_of(5000).unwrap();
        assert_eq!(occ.parent, Some(ParentKey::Surface(99)));
    }

    #[test]
    fn self_release_by_subject_and_reject_non_subject() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.occupy_soft(5000, 99, None).unwrap();
        assert!(e.release_soft_occupancy(5000, 77).is_err());
        assert!(e.live.occupancy.occupancy_of(5000).is_some());
        e.release_soft_occupancy(5000, 99).unwrap();
        assert!(e.live.occupancy.occupancy_of(5000).is_none());
    }

    #[test]
    fn release_occupancy_clears_soft() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.occupy_soft(5000, 99, None).unwrap();
        assert!(e.release_occupancy(5000)); // 로컬 force-detach tier 공용
        assert!(e.live.occupancy.occupancy_of(5000).is_none());
    }

    #[test]
    fn release_occupancy_clears_hard() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.live.occupancy.acquire(5000, 1).unwrap();
        assert!(e.release_occupancy(5000));
        assert!(!e.live.occupancy.is_hard_occupied(5000));
    }

    #[test]
    fn focus_cleanup_releases_soft_when_parent_gone() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.occupy_soft(5000, 99999, None).unwrap();
        e.reconcile_soft_occupancy_on_focus(5000);
        assert!(e.live.occupancy.occupancy_of(5000).is_none()); // 지연 청소.
    }

    #[test]
    fn focus_cleanup_keeps_soft_when_parent_alive() {
        let mut e_session = crate::state::tests::test_state().1;
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0]; // 기본 워크스페이스 live surface
        e.occupy_soft(5000, parent, None).unwrap();
        e.reconcile_soft_occupancy_on_focus(5000);
        assert!(e.live.occupancy.occupancy_of(5000).is_some()); // parent 생존 → 유지.
    }

    #[test]
    fn focus_cleanup_ignores_hard_occupancy() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.live.occupancy.acquire(5000, 1).unwrap();
        e.reconcile_soft_occupancy_on_focus(5000);
        assert!(e.live.occupancy.is_hard_occupied(5000)); // hard 는 이 경로 무관 — 유지.
    }

    #[test]
    fn focus_cleanup_keeps_an_external_parent_that_has_no_surface() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let parent = ParentKey::External("codex:thread-a".into());
        e.occupy_soft(5000, parent.clone(), Some("worker".into()))
            .unwrap();
        e.reconcile_soft_occupancy_on_focus(5000);
        let occ = e
            .live
            .occupancy
            .occupancy_of(5000)
            .expect("external 부모 점유 유지");
        assert_eq!(occ.parent, Some(parent.clone()));
        assert!(e.release_soft_occupancy(5000, 7).is_err());
        assert!(
            e.release_soft_occupancy(5000, ParentKey::External("codex:other".into()))
                .is_err()
        );
        e.release_soft_occupancy(5000, parent).unwrap();
        assert!(e.live.occupancy.occupancy_of(5000).is_none());
    }
}
