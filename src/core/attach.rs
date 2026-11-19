//! surface·workspace의 hard attach 점유와 표시용 soft 점유를 관리한다.
//! 상태는 저장하지 않으며 재시작하면 비어 있다. 인증은 이 레지스트리 밖에서 처리한다.
//! 값 변경은 전송을 수행하지 않는다. App/Remote 실행 경계가 반환된 holder에게 통지한다.

mod publication;

use std::collections::HashMap;

use crate::core::child_parent::ParentKey;
use crate::model::{SurfaceId, WorkspaceId};

/// StreamClientId와 같은 연결 식별자.
pub type AttachClientId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachLock {
    pub ready: bool,
    pub holder: AttachClientId,
    pub granted_seq: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AttachError {
    AlreadyAttached { holder: AttachClientId },
    NotHolder { holder: AttachClientId },
    NotAttached,
}

/// soft는 표시만 하고 hard는 attach의 입력 제한에 사용한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OccupancyTier {
    Soft,
    Hard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Holder {
    StreamClient(AttachClientId),
    Subject { label: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occupancy {
    pub tier: OccupancyTier,
    pub holder: Holder,
    /// soft 주체의 부모. hard는 연결 수명으로 정리하므로 None이다.
    pub parent: Option<ParentKey>,
    pub granted_seq: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OccupancyError {
    AlreadyOccupied { parent: ParentKey },
    NotHolder { parent: ParentKey },
    NotOccupied,
}

/// hard와 별도로 저장해 soft 표시가 입력 제한·연결 종료 처리에 섞이지 않게 한다.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SoftEntry {
    parent: ParentKey,
    label: Option<String>,
    granted_seq: u64,
}

#[derive(Default)]
pub struct OccupancyRegistry {
    surface_locks: HashMap<SurfaceId, AttachLock>,
    /// workspace 터미널은 surface_locks에도 등록해 단일 surface 입력 검사를 공유한다.
    workspace_locks: HashMap<WorkspaceId, AttachLock>,
    /// 비터미널까지 포함한 멤버의 역매핑. holder 조회와 workspace 단위 정리에 사용한다.
    surface_to_workspace: HashMap<SurfaceId, WorkspaceId>,
    next_seq: u64,
    workspace_publications: HashMap<WorkspaceId, std::sync::Arc<()>>,
    member_publications: HashMap<SurfaceId, std::sync::Arc<()>>,
    /// 이번 배치에서 끊김을 확인한 client. 점유 정리는 뒤에서 하되 새 acquire가 이를 구별할 수 있게 한다.
    dead_clients: std::collections::HashSet<AttachClientId>,
    soft: HashMap<SurfaceId, SoftEntry>,
}

impl OccupancyRegistry {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// hard 점유만 확인한다. soft 표시는 입력 차단으로 해석하지 않는다.
    pub fn is_hard_occupied(&self, surface_id: SurfaceId) -> bool {
        self.surface_locks.contains_key(&surface_id)
    }

    /// hard가 있으면 그것을 먼저 반환하고 없으면 soft를 조회한다.
    pub fn occupancy_of(&self, surface_id: SurfaceId) -> Option<Occupancy> {
        if let Some(lock) = self.surface_locks.get(&surface_id) {
            return Some(Occupancy {
                tier: OccupancyTier::Hard,
                holder: Holder::StreamClient(lock.holder),
                parent: None,
                granted_seq: lock.granted_seq,
            });
        }
        self.soft.get(&surface_id).map(|e| Occupancy {
            tier: OccupancyTier::Soft,
            holder: Holder::Subject {
                label: e.label.clone(),
            },
            parent: Some(e.parent.clone()),
            granted_seq: e.granted_seq,
        })
    }

    /// 다른 parent의 soft 점유는 거절한다. 같은 parent도 라벨과 부여 순번을 새로 기록한다.
    pub fn acquire_soft(
        &mut self,
        surface_id: SurfaceId,
        parent: ParentKey,
        label: Option<String>,
    ) -> Result<(), OccupancyError> {
        if let Some(existing) = self.soft.get(&surface_id)
            && existing.parent != parent
        {
            return Err(OccupancyError::AlreadyOccupied {
                parent: existing.parent.clone(),
            });
        }
        self.next_seq += 1;
        self.soft.insert(
            surface_id,
            SoftEntry {
                parent,
                label,
                granted_seq: self.next_seq,
            },
        );
        Ok(())
    }

    /// parent가 일치할 때만 soft 점유를 해제한다.
    pub fn release_soft(
        &mut self,
        surface_id: SurfaceId,
        parent: &ParentKey,
    ) -> Result<(), OccupancyError> {
        match self.soft.get(&surface_id) {
            None => Err(OccupancyError::NotOccupied),
            Some(e) if &e.parent != parent => Err(OccupancyError::NotHolder {
                parent: e.parent.clone(),
            }),
            Some(_) => {
                self.soft.remove(&surface_id);
                Ok(())
            }
        }
    }

    /// 주체 확인 없이 soft 표시만 지운다. 연결 통지는 보내지 않는다.
    pub fn clear_soft(&mut self, surface_id: SurfaceId) -> bool {
        self.soft.remove(&surface_id).is_some()
    }

    /// 터미널 surface 의 hard lock holder. 원본 터미널 읽기(`crate::remote::terminal_read`)의 대상 검사다.
    pub fn holder(&self, surface_id: SurfaceId) -> Option<AttachClientId> {
        self.surface_locks.get(&surface_id).map(|l| l.holder)
    }

    /// 같은 client면 기존 lock을 반환한다. 다른 holder가 있으면 끊김 표시를 확인해 회수하거나 거절한다.
    pub fn acquire(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
    ) -> Result<AttachLock, AttachError> {
        if let Some(existing) = self.surface_locks.get(&surface_id).copied() {
            if existing.holder == client_id {
                return Ok(existing);
            }
            if !self.evict_if_dead(existing.holder) {
                return Err(AttachError::AlreadyAttached {
                    holder: existing.holder,
                });
            }
        }
        self.next_seq += 1;
        let lock = AttachLock {
            ready: true,
            holder: client_id,
            granted_seq: self.next_seq,
        };
        self.surface_locks.insert(surface_id, lock);
        Ok(lock)
    }

    /// 실제 holder만 해제할 수 있다.
    pub fn release(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
    ) -> Result<(), AttachError> {
        match self.surface_locks.get(&surface_id) {
            None => Err(AttachError::NotAttached),
            Some(l) if l.holder != client_id => Err(AttachError::NotHolder { holder: l.holder }),
            Some(_) => {
                self.surface_locks.remove(&surface_id);
                Ok(())
            }
        }
    }

    /// holder 확인 없이 surface 점유를 해제하고 통지 대상 holder를 반환한다.
    /// workspace 멤버 여부는 호출자(`EngineMut::force_detach`)가 먼저 가린다.
    pub fn force_detach(&mut self, surface_id: SurfaceId) -> Option<AttachClientId> {
        let lock = self.surface_locks.remove(&surface_id)?;
        Some(lock.holder)
    }

    /// 배치 끝까지 잔여 입력을 처리하되, 끊긴 holder가 같은 배치의 재attach를 막지 않도록 표시한다.
    /// 경쟁 acquire가 없으면 점유를 유지하고 release_all_for_client에서 표시까지 지운다.
    pub fn mark_clients_disconnected(&mut self, clients: &[AttachClientId]) {
        self.dead_clients.extend(clients.iter().copied());
        self.retire_dead_publications(clients);
    }

    fn is_dead(&self, client_id: AttachClientId) -> bool {
        self.dead_clients.contains(&client_id)
    }

    /// 새 acquire와 충돌할 때만 끊긴 holder를 회수한다. 경쟁이 없으면 잔여 입력 뒤 정리한다.
    fn evict_if_dead(&mut self, holder: AttachClientId) -> bool {
        if !self.is_dead(holder) {
            return false;
        }
        self.release_all_for_client(holder);
        true
    }

    /// 연결의 workspace·surface 점유와 멤버 역매핑을 지운다. 이미 끊긴 연결에는 통지하지 않는다.
    pub fn release_all_for_client(&mut self, client_id: AttachClientId) -> Vec<SurfaceId> {
        let mut released: Vec<SurfaceId> = Vec::new();
        let ws_held: Vec<WorkspaceId> = self
            .workspace_locks
            .iter()
            .filter(|(_, l)| l.holder == client_id)
            .map(|(&w, _)| w)
            .collect();
        for ws in &ws_held {
            self.workspace_locks.remove(ws);
            self.workspace_publications.remove(ws);
            let members: Vec<SurfaceId> = self
                .surface_to_workspace
                .iter()
                .filter(|(_, w)| **w == *ws)
                .map(|(&s, _)| s)
                .collect();
            for s in &members {
                self.surface_locks.remove(s);
                self.surface_to_workspace.remove(s);
                self.member_publications.remove(s);
                released.push(*s);
            }
        }
        let surf_held: Vec<SurfaceId> = self
            .surface_locks
            .iter()
            .filter(|(_, l)| l.holder == client_id)
            .map(|(&sid, _)| sid)
            .collect();
        for sid in &surf_held {
            self.surface_locks.remove(sid);
            released.push(*sid);
        }
        // ID가 재사용됐을 때 새 연결을 끊긴 것으로 오인하지 않도록 표시도 지운다.
        self.dead_clients.remove(&client_id);
        released
    }

    /// 사라진 surface만 지우고 같은 workspace의 다른 멤버 점유는 유지한다.
    /// holder는 구조 delta로 닫힘을 알게 하며 workspace 전체가 분리된 것처럼 통지하지 않는다.
    pub fn forget_closed_surface(&mut self, surface_id: SurfaceId) -> bool {
        let had_lock = self.surface_locks.remove(&surface_id).is_some();
        let member_of = self.surface_to_workspace.remove(&surface_id);
        self.member_publications.remove(&surface_id);
        let had_member = member_of.is_some();
        let had_soft = self.clear_soft(surface_id);
        had_lock || had_member || had_soft
    }

    /// client가 점유한 workspace 하나를 반환한다. 여러 항목이면 HashMap에서 먼저 찾은 항목이다.
    pub(crate) fn workspace_held_by(&self, client_id: AttachClientId) -> Option<WorkspaceId> {
        self.workspace_locks
            .iter()
            .find(|(_, l)| l.holder == client_id)
            .map(|(&w, _)| w)
    }

    pub fn locks_snapshot(&self) -> Vec<(SurfaceId, AttachLock)> {
        self.surface_locks.iter().map(|(&s, &l)| (s, l)).collect()
    }

    /// 준비된 surface lock만. 준비 전 점유의 client는 attach descriptor를 아직 받지 않아서
    /// 그보다 먼저 상태 Control을 받으면 attach가 실패한다.
    pub(crate) fn ready_locks_snapshot(&self) -> Vec<(SurfaceId, AttachLock)> {
        self.surface_locks
            .iter()
            .filter(|(_, l)| l.ready)
            .map(|(&s, &l)| (s, l))
            .collect()
    }

    /// client가 점유한 surface 하나를 반환한다. 여러 항목이면 순서는 보장하지 않는다.
    pub fn surface_held_by(&self, client_id: AttachClientId) -> Option<SurfaceId> {
        self.surface_locks
            .iter()
            .find(|(_, l)| l.holder == client_id)
            .map(|(&s, _)| s)
    }

    /// workspace와 멤버 터미널의 다른 holder를 확인한 뒤 점유를 등록한다.
    /// 끊긴 holder 회수는 검사 도중에도 일어날 수 있다. 같은 client의 재호출은 기존 lock을 반환한다.
    /// 비터미널을 포함한 members는 역매핑에, terminals는 surface lock에도 등록한다.
    pub fn acquire_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        terminals: &[SurfaceId],
        members: &[SurfaceId],
        client_id: AttachClientId,
    ) -> Result<AttachLock, AttachError> {
        if let Some(existing) = self.workspace_locks.get(&workspace_id).copied() {
            if existing.holder == client_id {
                return Ok(existing);
            }
            if !self.evict_if_dead(existing.holder) {
                return Err(AttachError::AlreadyAttached {
                    holder: existing.holder,
                });
            }
        }
        for s in terminals {
            if let Some(l) = self.surface_locks.get(s).copied()
                && l.holder != client_id
                && !self.evict_if_dead(l.holder)
            {
                return Err(AttachError::AlreadyAttached { holder: l.holder });
            }
        }
        self.next_seq += 1;
        let lock = AttachLock {
            ready: true,
            holder: client_id,
            granted_seq: self.next_seq,
        };
        self.workspace_locks.insert(workspace_id, lock);
        self.workspace_publications
            .insert(workspace_id, std::sync::Arc::new(()));
        for s in terminals {
            self.surface_locks.entry(*s).or_insert(lock);
        }
        for s in members {
            self.surface_to_workspace.insert(*s, workspace_id);
            self.member_publications.insert(*s, std::sync::Arc::new(()));
        }
        Ok(lock)
    }

    /// 점유한 workspace의 새 멤버를 등록한다. workspace가 없으면 false다.
    /// 터미널 lock이 이미 있으면 덮지 않고 멤버 역매핑은 전달한 workspace로 갱신한다.
    pub fn add_workspace_member(
        &mut self,
        workspace_id: WorkspaceId,
        surface_id: SurfaceId,
        is_terminal: bool,
    ) -> bool {
        let Some(&lock) = self.workspace_locks.get(&workspace_id) else {
            return false;
        };
        if is_terminal {
            self.surface_locks.entry(surface_id).or_insert(lock);
        }
        if self.surface_to_workspace.get(&surface_id) != Some(&workspace_id) {
            self.member_publications
                .insert(surface_id, std::sync::Arc::new(()));
        }
        self.surface_to_workspace.insert(surface_id, workspace_id);
        true
    }

    /// hard surface lock 또는 점유 workspace의 멤버인지 확인하는 렌더용 판정.
    #[cfg(any(feature = "gui", test))]
    pub fn is_content_hidden(&self, surface_id: SurfaceId) -> bool {
        self.surface_locks.contains_key(&surface_id)
            || self.surface_to_workspace.contains_key(&surface_id)
    }

    /// Publication readiness belongs to the original grant, independently from connection IDs.
    pub(crate) fn set_attachment_ready(&mut self, client: AttachClientId, grant: u64, ready: bool) {
        self.set_publication_ready(client, grant, ready);
        for lock in self
            .surface_locks
            .values_mut()
            .chain(self.workspace_locks.values_mut())
        {
            if lock.holder == client && lock.granted_seq == grant {
                lock.ready = ready;
            }
        }
    }
    pub(crate) fn workspace_attachment_ready(&self, workspace: WorkspaceId) -> bool {
        self.workspace_locks
            .get(&workspace)
            .is_some_and(|lock| lock.ready)
    }
    pub(crate) fn surface_attachment_ready(&self, surface: SurfaceId) -> bool {
        self.surface_locks
            .get(&surface)
            .is_some_and(|lock| lock.ready)
    }

    pub fn workspace_holder(&self, workspace_id: WorkspaceId) -> Option<AttachClientId> {
        self.workspace_locks.get(&workspace_id).map(|l| l.holder)
    }

    pub fn workspace_of_surface(&self, surface_id: SurfaceId) -> Option<WorkspaceId> {
        self.surface_to_workspace.get(&surface_id).copied()
    }

    /// 비터미널 멤버는 surface lock이 없어 workspace 역매핑으로 holder를 찾는다.
    #[cfg(test)]
    pub fn workspace_holder_of(&self, surface_id: SurfaceId) -> Option<AttachClientId> {
        self.workspace_of_surface(surface_id)
            .and_then(|ws| self.workspace_holder(ws))
    }

    pub fn client_holds_workspace(&self, client_id: AttachClientId) -> bool {
        self.workspace_locks.values().any(|l| l.holder == client_id)
    }

    /// workspace 점유 client를 중복 없이 정렬한다. 인가와 변경 통지가 같은 점유 표를 사용한다.
    #[cfg(feature = "gui")]
    pub fn workspace_holders(&self) -> Vec<AttachClientId> {
        let mut holders: Vec<AttachClientId> =
            self.workspace_locks.values().map(|l| l.holder).collect();
        holders.sort_unstable();
        holders.dedup();
        holders
    }

    /// workspace와 멤버 점유를 지우고 통지 대상 holder를 반환한다.
    pub fn force_detach_workspace(&mut self, workspace_id: WorkspaceId) -> Option<AttachClientId> {
        let lock = self.workspace_locks.remove(&workspace_id)?;
        self.workspace_publications.remove(&workspace_id);
        self.clear_workspace_members(workspace_id);
        tracing::debug!(
            "attach: force_detach_workspace workspace={workspace_id:?} holder={:?}",
            lock.holder
        );
        Some(lock.holder)
    }

    pub fn workspaces_snapshot(&self) -> Vec<(WorkspaceId, AttachLock)> {
        self.workspace_locks.iter().map(|(&w, &l)| (w, l)).collect()
    }

    fn clear_workspace_members(&mut self, workspace_id: WorkspaceId) {
        let members: Vec<SurfaceId> = self
            .surface_to_workspace
            .iter()
            .filter(|(_, w)| **w == workspace_id)
            .map(|(&s, _)| s)
            .collect();
        for s in &members {
            self.surface_locks.remove(s);
            self.surface_to_workspace.remove(s);
            self.member_publications.remove(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_free_succeeds() {
        let mut reg = OccupancyRegistry::new();
        let lock = reg.acquire(10, 1).unwrap();
        assert_eq!(lock.holder, 1);
        assert_eq!(lock.granted_seq, 1);
        assert!(reg.is_hard_occupied(10));
        assert_eq!(reg.holder(10), Some(1));
    }

    #[test]
    fn acquire_already_attached_rejected() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 1).unwrap();
        let err = reg.acquire(10, 2).unwrap_err();
        assert_eq!(err, AttachError::AlreadyAttached { holder: 1 });
        assert_eq!(reg.holder(10), Some(1));
    }

    #[test]
    fn acquire_same_client_idempotent() {
        let mut reg = OccupancyRegistry::new();
        let a = reg.acquire(10, 1).unwrap();
        let b = reg.acquire(10, 1).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn release_by_holder() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 1).unwrap();
        reg.release(10, 1).unwrap();
        assert!(!reg.is_hard_occupied(10));
    }

    #[test]
    fn release_non_holder_rejected() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 1).unwrap();
        let err = reg.release(10, 2).unwrap_err();
        assert_eq!(err, AttachError::NotHolder { holder: 1 });
        assert!(reg.is_hard_occupied(10));
    }

    #[test]
    fn release_not_attached() {
        let mut reg = OccupancyRegistry::new();
        assert_eq!(reg.release(10, 1).unwrap_err(), AttachError::NotAttached);
    }

    #[test]
    fn force_detach_frees_and_returns_holder() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 7).unwrap();
        assert_eq!(reg.force_detach(10), Some(7));
        assert!(!reg.is_hard_occupied(10));
    }

    #[test]
    fn force_detach_idempotent_when_free() {
        let mut reg = OccupancyRegistry::new();
        assert_eq!(reg.force_detach(10), None);
    }

    #[test]
    fn release_all_for_client() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 1).unwrap();
        reg.acquire(11, 1).unwrap();
        reg.acquire(12, 2).unwrap();
        let mut released = reg.release_all_for_client(1);
        released.sort_unstable();
        assert_eq!(released, vec![10, 11]);
        assert!(!reg.is_hard_occupied(10));
        assert!(!reg.is_hard_occupied(11));
        assert!(reg.is_hard_occupied(12));
    }

    #[test]
    fn granted_seq_monotonic() {
        let mut reg = OccupancyRegistry::new();
        let a = reg.acquire(10, 1).unwrap();
        let b = reg.acquire(11, 2).unwrap();
        assert!(b.granted_seq > a.granted_seq);
    }

    #[test]
    fn acquire_workspace_locks_terminals_and_members() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10, 11], &[10, 11, 12], 1)
            .unwrap();
        assert!(reg.is_hard_occupied(10));
        assert!(reg.is_hard_occupied(11));
        assert!(!reg.is_hard_occupied(12));
        assert!(reg.is_content_hidden(10));
        assert!(reg.is_content_hidden(12));
        assert_eq!(reg.workspace_holder(100), Some(1));
        assert_eq!(reg.workspace_of_surface(12), Some(100));
        assert_eq!(reg.workspace_holder_of(12), Some(1));
        assert!(reg.client_holds_workspace(1));
    }

    #[test]
    fn acquire_workspace_already_attached_rejected() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10], &[10], 1).unwrap();
        let err = reg.acquire_workspace(100, &[10], &[10], 2).unwrap_err();
        assert_eq!(err, AttachError::AlreadyAttached { holder: 1 });
    }

    #[test]
    fn acquire_workspace_idempotent_same_client() {
        let mut reg = OccupancyRegistry::new();
        let a = reg.acquire_workspace(100, &[10], &[10], 1).unwrap();
        let b = reg.acquire_workspace(100, &[10], &[10], 1).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn acquire_workspace_partial_conflict_rejected() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(11, 2).unwrap();
        let err = reg
            .acquire_workspace(100, &[10, 11], &[10, 11], 1)
            .unwrap_err();
        assert_eq!(err, AttachError::AlreadyAttached { holder: 2 });
        assert!(!reg.workspace_locks.contains_key(&100));
        assert!(!reg.is_hard_occupied(10));
    }

    #[test]
    fn force_detach_workspace_clears_members() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10, 11], &[10, 11, 12], 7)
            .unwrap();
        assert_eq!(reg.force_detach_workspace(100), Some(7));
        assert!(!reg.is_hard_occupied(10));
        assert!(!reg.is_hard_occupied(11));
        assert!(!reg.is_content_hidden(12));
        assert_eq!(reg.workspace_holder(100), None);
        assert!(reg.workspaces_snapshot().is_empty());
    }

    #[test]
    fn force_detach_workspace_idempotent_when_free() {
        let mut reg = OccupancyRegistry::new();
        assert_eq!(reg.force_detach_workspace(100), None);
    }

    #[test]
    fn release_all_for_client_clears_workspace_and_surface() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10, 11], &[10, 11, 12], 1)
            .unwrap();
        reg.acquire(20, 1).unwrap();
        reg.acquire_workspace(200, &[30], &[30], 2).unwrap();
        let mut released = reg.release_all_for_client(1);
        released.sort_unstable();
        assert_eq!(released, vec![10, 11, 12, 20]);
        assert!(!reg.is_content_hidden(10));
        assert!(!reg.is_content_hidden(12));
        assert!(!reg.is_hard_occupied(20));
        assert_eq!(reg.workspace_holder(200), Some(2));
        assert!(reg.is_hard_occupied(30));
    }

    #[test]
    fn soft_occupancy_does_not_set_hard_predicate() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, /*parent*/ 99.into(), None).unwrap();
        assert!(!reg.is_hard_occupied(10));
        assert_eq!(
            reg.occupancy_of(10).map(|o| o.tier),
            Some(OccupancyTier::Soft)
        );
        reg.acquire(11, 1).unwrap();
        assert!(reg.is_hard_occupied(11));
    }

    #[test]
    fn soft_occupancy_records_parent_and_label() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), Some("agent".into()))
            .unwrap();
        let occ = reg.occupancy_of(10).unwrap();
        assert_eq!(occ.tier, OccupancyTier::Soft);
        assert_eq!(occ.parent, Some(99.into()));
        assert_eq!(
            occ.holder,
            Holder::Subject {
                label: Some("agent".into())
            }
        );
    }

    #[test]
    fn soft_acquire_same_parent_idempotent_updates_label() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), None).unwrap();
        reg.acquire_soft(10, 99.into(), Some("x".into())).unwrap();
        assert_eq!(
            reg.occupancy_of(10).unwrap().holder,
            Holder::Subject {
                label: Some("x".into())
            }
        );
    }

    #[test]
    fn soft_acquire_other_subject_rejected() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), None).unwrap();
        let err = reg.acquire_soft(10, 77.into(), None).unwrap_err();
        assert_eq!(err, OccupancyError::AlreadyOccupied { parent: 99.into() });
    }

    #[test]
    fn soft_release_by_subject() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), None).unwrap();
        reg.release_soft(10, &99.into()).unwrap();
        assert!(reg.occupancy_of(10).is_none());
    }

    #[test]
    fn soft_release_non_holder_rejected() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), None).unwrap();
        assert_eq!(
            reg.release_soft(10, &77.into()).unwrap_err(),
            OccupancyError::NotHolder { parent: 99.into() }
        );
        assert!(reg.occupancy_of(10).is_some());
    }

    #[test]
    fn soft_release_not_occupied() {
        let mut reg = OccupancyRegistry::new();
        assert_eq!(
            reg.release_soft(10, &99.into()).unwrap_err(),
            OccupancyError::NotOccupied
        );
    }

    #[test]
    fn clear_soft_removes_unconditionally() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), None).unwrap();
        assert!(reg.clear_soft(10));
        assert!(reg.occupancy_of(10).is_none());
        assert!(!reg.clear_soft(10));
    }

    #[test]
    fn hard_dominates_soft_in_occupancy_of() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_soft(10, 99.into(), None).unwrap();
        reg.acquire(10, 1).unwrap();
        assert!(reg.is_hard_occupied(10));
        assert_eq!(
            reg.occupancy_of(10).map(|o| o.tier),
            Some(OccupancyTier::Hard)
        );
    }

    #[test]
    fn force_detach_pushes_to_notifier() {
        use tasty_ipc::stream::StreamTag;
        let hub = tasty_ipc::stream_hub::StreamHub::new();
        let holder = hub.alloc_id();
        let rx = hub.register(holder);
        let mut session = crate::state::tests::test_state().1;
        let mut engine = session.borrow_mut();
        engine.remote.set_notifier(hub);
        engine.live.occupancy.acquire(10, holder).unwrap();
        assert_eq!(engine.force_detach(10).map(|d| d.holder), Some(holder));
        let f1 = rx.recv().unwrap();
        assert_eq!(f1.tag, StreamTag::Control);
        assert!(String::from_utf8_lossy(&f1.payload).contains("force_detached"));
        let f2 = rx.recv().unwrap();
        assert_eq!(f2.tag, StreamTag::Detach);
    }

    /// GUI 점유 테두리 버튼과 IPC가 같이 쓰는 `force_detach`는 workspace 멤버면 workspace 점유 전체를 푼다.
    #[test]
    fn force_detach_on_a_workspace_member_releases_the_whole_workspace() {
        let hub = tasty_ipc::stream_hub::StreamHub::new();
        let holder = hub.alloc_id();
        let rx = hub.register(holder);
        let mut session = crate::state::tests::test_state().1;
        let mut engine = session.borrow_mut();
        engine.remote.set_notifier(hub);
        engine
            .live
            .occupancy
            .acquire_workspace(100, &[10, 11], &[10, 11, 12], holder)
            .unwrap();
        let detached = engine.force_detach(11).expect("member is held");
        assert_eq!(detached.holder, holder);
        assert_eq!(detached.workspace_id, Some(100));
        assert!(detached.notified);
        assert_eq!(engine.live.occupancy.workspace_holder(100), None);
        for s in [10, 11, 12] {
            assert_eq!(engine.live.occupancy.workspace_of_surface(s), None);
            assert!(!engine.live.occupancy.is_hard_occupied(s));
        }
        let notice = rx.recv().unwrap();
        assert!(String::from_utf8_lossy(&notice.payload).contains("force_detach_workspace"));
    }

    /// soft 점유는 stream client가 아니라 IPC 강제 끊기의 대상이 아니다.
    #[test]
    fn force_detach_leaves_a_soft_occupancy_alone() {
        let mut session = crate::state::tests::test_state().1;
        let mut engine = session.borrow_mut();
        engine.occupy_soft(5000, 99, Some("agent".into())).unwrap();
        assert_eq!(engine.force_detach(5000), None);
        assert!(engine.live.occupancy.occupancy_of(5000).is_some());
    }

    /// 큐가 차서 통지를 넣지 못하면 연결을 닫고 `notified: false`로 알린다.
    /// 통지는 넣었지만 Detach가 밀렸으면 통지 뒤 연결을 닫는다.
    /// 이 시험은 허브의 큐와 채널 종료만 본다. 실제 writer 의 binding 검사와 소켓 종료는
    /// `tcp_ipc_server/stream_bytes_tests.rs` 의 강제 끊김 시험이 잰다.
    #[test]
    fn a_full_queue_closes_the_connection_and_reports_whether_the_notice_went_in() {
        use tasty_ipc::stream::{StreamFrame, StreamTag};
        use tasty_ipc::stream_hub::PushResult;
        for (fill_to_leave, notified) in [(0usize, false), (1, true)] {
            let hub = tasty_ipc::stream_hub::StreamHub::new();
            let holder = hub.alloc_id();
            let rx = hub.register(holder);
            let mut filled = 0usize;
            while hub.push(holder, StreamFrame::new(StreamTag::Data, vec![0])) == PushResult::Sent {
                filled += 1;
            }
            // 가득 찬 큐에서 하나를 꺼내 자리를 하나 남기는 경우를 만든다.
            for _ in 0..fill_to_leave {
                rx.recv().unwrap();
                filled -= 1;
            }
            let mut session = crate::state::tests::test_state().1;
            let mut engine = session.borrow_mut();
            engine.remote.set_notifier(hub);
            engine.live.occupancy.acquire(10, holder).unwrap();
            let detached = engine.force_detach(10).expect("held");
            assert_eq!(detached.notified, notified);
            let mut tags = Vec::new();
            while let Ok(frame) = rx.recv() {
                tags.push(frame.tag);
            }
            // sink를 지웠으므로 남은 프레임을 다 꺼내면 채널이 끊긴다.
            let controls = tags.iter().filter(|t| **t == StreamTag::Control).count();
            assert_eq!(tags.len(), filled + controls);
            assert_eq!(controls == 1, notified);
            assert!(!tags.contains(&StreamTag::Detach));
        }
    }
    // 소켓 없이 같은 배치의 끊김 표시 → 재attach → 정리 순서를 재현한다.

    #[test]
    fn a_dead_holder_does_not_block_a_workspace_reattach_in_the_same_batch() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10, 11], &[10, 11, 12], 1)
            .unwrap();

        reg.mark_clients_disconnected(&[1]);

        let lock = reg
            .acquire_workspace(100, &[10, 11], &[10, 11, 12], 2)
            .expect("끊긴 holder 는 재attach 를 막지 못한다");
        assert_eq!(lock.holder, 2);
        assert_eq!(
            reg.holder(10),
            Some(2),
            "surface 10 의 holder 도 넘어와야 한다"
        );
        assert_eq!(
            reg.holder(11),
            Some(2),
            "surface 11 의 holder 도 넘어와야 한다"
        );
        assert_eq!(reg.workspace_holder_of(12), Some(2));
    }

    #[test]
    fn a_dead_holder_does_not_block_a_surface_reattach_in_the_same_batch() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 1).unwrap();
        reg.mark_clients_disconnected(&[1]);
        assert_eq!(
            reg.acquire(10, 2)
                .expect("끊긴 holder 는 막지 못한다")
                .holder,
            2
        );
    }

    #[test]
    fn a_live_holder_still_blocks_a_reattach() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10], &[10], 1).unwrap();
        reg.acquire(20, 3).unwrap();
        reg.mark_clients_disconnected(&[9]);
        assert_eq!(
            reg.acquire_workspace(100, &[10], &[10], 2).unwrap_err(),
            AttachError::AlreadyAttached { holder: 1 }
        );
        assert_eq!(
            reg.acquire(20, 4).unwrap_err(),
            AttachError::AlreadyAttached { holder: 3 }
        );
    }

    #[test]
    fn the_batch_end_cleanup_does_not_evict_the_new_holder() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(100, &[10], &[10, 12], 1).unwrap();
        reg.mark_clients_disconnected(&[1]);
        reg.acquire_workspace(100, &[10], &[10, 12], 2).unwrap();

        reg.release_all_for_client(1);

        assert_eq!(reg.holder(10), Some(2), "새 holder 가 살아 있어야 한다");
        assert_eq!(reg.workspace_holder_of(12), Some(2));
    }

    #[test]
    fn the_dead_mark_does_not_outlive_its_batch() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire(10, 1).unwrap();
        reg.mark_clients_disconnected(&[1]);
        reg.release_all_for_client(1);

        reg.acquire(10, 1).unwrap();
        assert_eq!(
            reg.acquire(10, 2).unwrap_err(),
            AttachError::AlreadyAttached { holder: 1 },
            "표시가 남아 있으면 살아 있는 holder 가 죽은 것으로 취급된다"
        );
    }
    /// 두 원문에서 적용 함수명 뒤의 표시·attach 문자열 순서를 확인한다.
    /// 함수 끝을 잘라내거나 주석·리터럴을 제외하지 않으며 실제 호출·실행 순서를 분석하지 않는다.
    #[test]
    fn both_pumps_mark_disconnects_before_applying_attach_requests() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let sites = [
            (
                "src/boot/headless_stream.rs",
                "fn apply(",
                "mark_clients_disconnected(",
                "apply_attach_requests(",
            ),
            (
                "src/app/event_handler.rs",
                "fn apply_stream_outcome(",
                "mark_disconnected_clients(",
                "apply_attach_requests_batch(",
            ),
        ];
        let mut checked = 0usize;
        for (rel, func, mark, attach) in sites {
            let src = std::fs::read_to_string(root.join(rel))
                .unwrap_or_else(|e| panic!("{rel} 원문을 읽지 못했다: {e}"));
            let start = src
                .find(func)
                .unwrap_or_else(|| panic!("{rel}에서 함수명 문자열 `{func}`를 찾지 못했다"));
            let body = &src[start..];
            let m = body.find(mark).unwrap_or_else(|| {
                panic!("{rel}::{func} 뒤에서 표시 문자열 `{mark}`를 찾지 못했다")
            });
            let a = body.find(attach).unwrap_or_else(|| {
                panic!("{rel}::{func} 뒤에서 attach 문자열 `{attach}`를 찾지 못했다")
            });
            assert!(
                m < a,
                "{rel}::{func}: 원문에서 끊김 표시가 attach보다 앞서야 한다 (표시 {m}, attach {a})"
            );
            checked += 1;
        }
        assert_eq!(checked, 2, "GUI와 헤드리스 원문을 모두 검사해야 한다");
    }
}
