//! Barrier primitive — N개 신호가 모일 때까지 기다리는 동기화 게이트.
//!
//! 본 단계(Phase 5.2)에서는 **poll-based** 모델만 제공한다. `barrier_await` 은
//! 즉시 현 상태를 반환하고, 호출자가 다시 호출해 폴링한다. 실제 long-poll/wakeup
//! 은 scheduler 도입 후 추가.
//!
//! 영속: `tasty.agent.barrier.<name>` (workspace scope).
//!
//! 상태 전이:
//! - `Open`: 신호 누적 중. `signal` 으로 count_signaled++.
//! - `Open → Closed`: count_signaled >= count_required 도달.
//! - `Open → TimedOut`: timeout_ms 가 있고 (created_at + timeout_ms) < now 인 채로
//!   `signal`/`await`/`state` 가 호출되는 시점.
//! - `Closed/TimedOut`: 종착 상태. signal 거부.

use serde::{Deserialize, Serialize};
use tasty_memory::{ListOpts, MemoryStorage, MemoryValue, PutOpts, Scope};
use tasty_utils::id::WorkspaceId;

use crate::{AgentError, Result};

pub const BARRIER_KEY_PREFIX: &str = "tasty.agent.barrier.";

fn barrier_key(name: &str) -> Result<String> {
    crate::component_key(BARRIER_KEY_PREFIX, "barrier name", name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarrierState {
    Open,
    Closed,
    TimedOut,
}

impl BarrierState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, BarrierState::Closed | BarrierState::TimedOut)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Barrier {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub count_required: u32,
    pub count_signaled: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    pub state: BarrierState,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
}

impl Barrier {
    /// 만료 여부를 시각 비교로 판단. (`now_ms >= created_at + timeout_ms`).
    pub fn is_expired(&self, now_ms: u64) -> bool {
        match self.timeout_ms {
            Some(t) => now_ms >= self.created_at.saturating_add(t),
            None => false,
        }
    }

    /// Open 상태인데 timeout 이 지났으면 TimedOut 으로 도장 찍는다. 변경 여부 반환.
    fn maybe_timeout(&mut self, now_ms: u64) -> bool {
        if matches!(self.state, BarrierState::Open) && self.is_expired(now_ms) {
            self.state = BarrierState::TimedOut;
            self.finished_at = Some(now_ms);
            true
        } else {
            false
        }
    }
}

pub struct BarrierStore<'a> {
    mem: &'a mut dyn MemoryStorage,
    owner: String,
}

impl<'a> BarrierStore<'a> {
    pub fn new(mem: &'a mut dyn MemoryStorage, owner: impl Into<String>) -> Self {
        Self {
            mem,
            owner: owner.into(),
        }
    }

    fn put(&mut self, b: &Barrier) -> Result<()> {
        let scope = Scope::Workspace(b.workspace_id);
        let value = MemoryValue::Json(serde_json::to_value(b)?);
        self.mem.put(
            &self.owner,
            &scope,
            &barrier_key(&b.name)?,
            &value,
            &PutOpts::default(),
        )?;
        Ok(())
    }

    pub fn get(&self, workspace_id: WorkspaceId, name: &str) -> Result<Option<Barrier>> {
        let scope = Scope::Workspace(workspace_id);
        let entry = self.mem.get(&scope, &barrier_key(name)?)?;
        match entry {
            Some(e) => match e.value {
                MemoryValue::Json(v) => Ok(Some(serde_json::from_value(v)?)),
                _ => Err(AgentError::InvalidArgument(format!(
                    "barrier entry is not json: {name}"
                ))),
            },
            None => Ok(None),
        }
    }

    /// 워크스페이스의 모든 barrier 목록. `now_ms` 가 있으면 조회 시점에 timeout
    /// 도장도 함께 찍어 영속한다.
    pub fn list(&mut self, workspace_id: WorkspaceId, now_ms: Option<u64>) -> Result<Vec<Barrier>> {
        let scope = Scope::Workspace(workspace_id);
        let opts = ListOpts {
            prefix: Some(BARRIER_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        let entries = self.mem.list(&scope, &opts)?;
        let mut out: Vec<Barrier> = Vec::with_capacity(entries.len());
        for e in entries {
            if let MemoryValue::Json(v) = e.value {
                out.push(serde_json::from_value(v)?);
            }
        }
        if let Some(now) = now_ms {
            for b in out.iter_mut() {
                if b.maybe_timeout(now) {
                    self.put(b)?;
                }
            }
        }
        Ok(out)
    }

    /// 새 barrier 생성. 같은 이름이 이미 존재하면 거부 (`InvalidArgument`).
    pub fn create(
        &mut self,
        workspace_id: WorkspaceId,
        name: impl Into<String>,
        count_required: u32,
        timeout_ms: Option<u64>,
        now_ms: u64,
    ) -> Result<Barrier> {
        let name = name.into();
        if count_required == 0 {
            return Err(AgentError::InvalidArgument(
                "barrier.count_required must be >= 1".into(),
            ));
        }
        if let Some(existing) = self.get(workspace_id, &name)? {
            return Err(AgentError::InvalidArgument(format!(
                "barrier '{name}' already exists (state={:?})",
                existing.state
            )));
        }
        let b = Barrier {
            workspace_id,
            name,
            count_required,
            count_signaled: 0,
            timeout_ms,
            state: BarrierState::Open,
            created_at: now_ms,
            finished_at: None,
        };
        self.put(&b)?;
        Ok(b)
    }

    /// 신호 1회 누적. count_signaled 이 count_required 에 도달하면 `Closed`.
    /// timeout 이 지난 상태라면 먼저 `TimedOut` 으로 전이하고 signal 은 거부.
    pub fn signal(
        &mut self,
        workspace_id: WorkspaceId,
        name: &str,
        now_ms: u64,
    ) -> Result<Barrier> {
        let mut b = self
            .get(workspace_id, name)?
            .ok_or_else(|| AgentError::InvalidArgument(format!("barrier not found: {name}")))?;
        if b.maybe_timeout(now_ms) {
            self.put(&b)?;
            return Err(AgentError::InvalidArgument(format!(
                "barrier '{name}' timed out"
            )));
        }
        if b.state.is_terminal() {
            return Err(AgentError::AlreadyTerminal(
                serde_json::to_string(&b.state).unwrap_or_default(),
            ));
        }
        b.count_signaled = b.count_signaled.saturating_add(1);
        if b.count_signaled >= b.count_required {
            b.state = BarrierState::Closed;
            b.finished_at = Some(now_ms);
        }
        self.put(&b)?;
        Ok(b)
    }

    /// 현 상태 조회 (timeout 도장 포함).
    pub fn state(&mut self, workspace_id: WorkspaceId, name: &str, now_ms: u64) -> Result<Barrier> {
        let mut b = self
            .get(workspace_id, name)?
            .ok_or_else(|| AgentError::InvalidArgument(format!("barrier not found: {name}")))?;
        if b.maybe_timeout(now_ms) {
            self.put(&b)?;
        }
        Ok(b)
    }

    /// 삭제 (드물게 사용).
    pub fn delete(&mut self, workspace_id: WorkspaceId, name: &str) -> Result<()> {
        let scope = Scope::Workspace(workspace_id);
        self.mem
            .delete(&self.owner, &scope, &barrier_key(name)?, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_memory::MemoryStore;
    use tempfile::TempDir;

    fn fresh() -> (TempDir, MemoryStore) {
        let td = tempfile::tempdir().unwrap();
        let mem = MemoryStore::open(&td.path().join("mem.db")).unwrap();
        (td, mem)
    }

    #[test]
    fn create_then_signals_close_barrier() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        let b = store.create(1, "b1", 3, None, 1000).unwrap();
        assert_eq!(b.state, BarrierState::Open);
        store.signal(1, "b1", 1100).unwrap();
        let b2 = store.signal(1, "b1", 1200).unwrap();
        assert_eq!(b2.state, BarrierState::Open);
        let b3 = store.signal(1, "b1", 1300).unwrap();
        assert_eq!(b3.state, BarrierState::Closed);
        assert_eq!(b3.count_signaled, 3);
        assert_eq!(b3.finished_at, Some(1300));
    }

    #[test]
    fn duplicate_create_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        store.create(1, "b1", 2, None, 1000).unwrap();
        let err = store.create(1, "b1", 2, None, 1001).unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
    }

    #[test]
    fn timeout_transitions_on_state_query() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        store.create(1, "b1", 5, Some(500), 1000).unwrap();
        let b = store.state(1, "b1", 1499).unwrap();
        assert_eq!(b.state, BarrierState::Open);
        let b = store.state(1, "b1", 1500).unwrap();
        assert_eq!(b.state, BarrierState::TimedOut);
        assert_eq!(b.finished_at, Some(1500));
    }

    #[test]
    fn signal_rejected_after_timeout() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        store.create(1, "b1", 3, Some(500), 1000).unwrap();
        let err = store.signal(1, "b1", 2000).unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
        let b = store.state(1, "b1", 2001).unwrap();
        assert_eq!(b.state, BarrierState::TimedOut);
    }

    #[test]
    fn signal_after_close_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        store.create(1, "b1", 1, None, 1000).unwrap();
        store.signal(1, "b1", 1100).unwrap();
        let err = store.signal(1, "b1", 1200).unwrap_err();
        assert!(matches!(err, AgentError::AlreadyTerminal(_)));
    }

    #[test]
    fn count_required_zero_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        let err = store.create(1, "b1", 0, None, 1000).unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
    }

    #[test]
    fn list_returns_workspace_scoped() {
        let (_td, mut mem) = fresh();
        let mut store = BarrierStore::new(&mut mem, "_host");
        store.create(1, "b1", 2, None, 1000).unwrap();
        store.create(1, "b2", 3, None, 1001).unwrap();
        store.create(2, "b3", 1, None, 1002).unwrap();
        assert_eq!(store.list(1, None).unwrap().len(), 2);
        assert_eq!(store.list(2, None).unwrap().len(), 1);
    }
}
