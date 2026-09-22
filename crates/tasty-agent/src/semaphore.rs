//! Semaphore primitive — N개 permit 까지 동시 점유 허용.
//!
//! 본 단계(Phase 5.2)에서는 **poll-based acquire** 만 제공한다. 가용 permit 이
//! 없으면 `acquire` 가 `acquired=false` 로 즉시 응답하고, 호출자가 다시 시도한다.
//! 실제 blocking + queue/fairness 는 scheduler 도입 후 추가.
//!
//! 영속: `tasty.agent.semaphore.<name>` (workspace scope).
//!
//! 점유 규칙:
//! - 같은 `holder` 가 이미 점유 중이면 acquire 는 idempotent 하게 성공 응답
//!   (재시도 안전). 동일 holder 가 permits 를 두 개 잡고 싶다면 다른 holder id
//!   를 써야 한다. 재acquire 는 `acquired_at`/`expires_at` 를 갱신한다.
//! - `release` 는 holders 에서 해당 id 제거 + permits_available 1 회복. holder
//!   가 점유 중이 아니면 no-op (idempotent).
//!
//! ## 만료 (opt-in)
//!
//! [`Lease`](crate::lease) 와 **같은 메커니즘**을 쓴다 — 두 primitive 가 각자
//! 다른 만료 개념을 갖지 않게 의도적으로 맞췄다: `acquire` 에 `ttl_ms` 를 주면
//! `expires_at = now + ttl_ms` 가 기록되고, 만료된 홀더는 다음 `acquire`/`list`
//! 시점에 lazy 하게 evict 된다. 같은 holder 의 재acquire 가 갱신(heartbeat)이다.
//!
//! **기본값은 만료 없음**이다(`ttl_ms: None`). 만료를 기본으로 켜면 오래 걸리는
//! 정당한 작업의 permit 이 도중에 회수되어 두 홀더가 동시에 임계구역에 들어가는데,
//! 그건 교착보다 나쁘다 — 근거·대안·재검토 조건은
//! [ADR-0119](../../../docs/adr/0119-agent-semaphore-resize-and-holder-expiry.md).
//!
//! ## 리사이즈 (`set_permits`)
//!
//! `permits_total` 은 운용 중에 바뀌는 값이라 [`SemaphoreStore::set_permits`] 로
//! 원자적으로 조정한다(delete → create 우회는 세마포어가 존재하지 않는 틈을
//! 만든다). **축소는 drain 이다** — 이미 점유 중인 홀더를 강제 회수하지 않는다.
//! `holders.len() > permits_total` 인 초과 상태를 그대로 두고, 새 acquire 는
//! 홀더 수가 새 한도 아래로 내려갈 때까지 거절된다.

use serde::{Deserialize, Serialize};
use tasty_memory::{ListOpts, MemoryStorage, MemoryValue, PutOpts, Scope};
use tasty_utils::id::WorkspaceId;

use crate::{AgentError, Result};

pub const SEMAPHORE_KEY_PREFIX: &str = "tasty.agent.semaphore.";

fn semaphore_key(name: &str) -> Result<String> {
    crate::component_key(SEMAPHORE_KEY_PREFIX, "semaphore name", name)
}

/// permit 하나를 점유 중인 홀더.
///
/// `acquired_at` 이 `Option` 인 이유는 **모르는 것을 0 으로 뭉개지 않기 위해서**다
/// — holders 가 문자열 배열이던 시절에 잡힌 permit 은 획득 시각이 기록되지 않았고,
/// 그걸 epoch 로 적으면 "56년째 점유 중" 이라는 거짓말이 된다. `None` 은 "이
/// 홀더는 시각 기록 이전에 잡았다" 는 뜻이다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemaphoreHolder {
    pub id: String,
    /// 획득(또는 마지막 갱신) 시각 ms. 구 형식에서 올라온 홀더는 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquired_at: Option<u64>,
    /// 만료 시각 ms. `None` 이면 만료되지 않는다(기본값).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

impl SemaphoreHolder {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        match self.expires_at {
            Some(t) => now_ms >= t,
            None => false,
        }
    }
}

/// 구 형식(`holders: ["h1", "h2"]`)으로 영속된 세마포어를 그대로 읽는다. 이미
/// 실행 중인 인스턴스의 memory db 에 그 형식이 남아 있으므로 새 코드가 그것을
/// 못 읽으면 부팅 시 세마포어가 통째로 사라진다.
impl<'de> Deserialize<'de> for SemaphoreHolder {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Legacy(String),
            Full {
                id: String,
                #[serde(default)]
                acquired_at: Option<u64>,
                #[serde(default)]
                expires_at: Option<u64>,
            },
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Legacy(id) => SemaphoreHolder {
                id,
                acquired_at: None,
                expires_at: None,
            },
            Repr::Full {
                id,
                acquired_at,
                expires_at,
            } => SemaphoreHolder {
                id,
                acquired_at,
                expires_at,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Semaphore {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub permits_total: u32,
    /// `permits_total - holders.len()` 의 파생값 — 저장은 되지만 읽기·쓰기
    /// 시점마다 다시 계산해 홀더 목록과 어긋나지 않게 한다. 축소(drain)로
    /// 홀더가 한도를 넘긴 상태에서는 0 이다(음수로 내려가지 않는다).
    pub permits_available: u32,
    pub holders: Vec<SemaphoreHolder>,
    pub created_at: u64,
}

impl Semaphore {
    /// 홀더 수가 한도를 넘긴 상태 — `set_permits` 축소 직후에만 생긴다.
    /// 이 동안 새 acquire 는 거절되고, 기존 홀더는 강제 회수되지 않는다.
    pub fn is_over_subscribed(&self) -> bool {
        self.holders.len() as u64 > u64::from(self.permits_total)
    }

    fn recompute_available(&mut self) {
        let held = u32::try_from(self.holders.len()).unwrap_or(u32::MAX);
        self.permits_available = self.permits_total.saturating_sub(held);
    }

    /// 만료된 홀더 제거. 제거가 일어났으면 true.
    fn evict_expired(&mut self, now_ms: u64) -> bool {
        let before = self.holders.len();
        self.holders.retain(|h| !h.is_expired(now_ms));
        self.holders.len() != before
    }

    fn has_free_permit(&self) -> bool {
        (self.holders.len() as u64) < u64::from(self.permits_total)
    }
}

/// `acquire` 응답.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquireOutcome {
    pub acquired: bool,
    pub semaphore: Semaphore,
}

/// `release` 응답.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseOutcome {
    /// 실제로 release 되어 permits 가 회복되었는지 (점유 중이 아니었으면 false).
    pub released: bool,
    pub semaphore: Semaphore,
}

pub struct SemaphoreStore<'a> {
    mem: &'a mut dyn MemoryStorage,
    owner: String,
}

impl<'a> SemaphoreStore<'a> {
    pub fn new(mem: &'a mut dyn MemoryStorage, owner: impl Into<String>) -> Self {
        Self {
            mem,
            owner: owner.into(),
        }
    }

    fn put(&mut self, s: &Semaphore) -> Result<()> {
        let scope = Scope::Workspace(s.workspace_id);
        let value = MemoryValue::Json(serde_json::to_value(s)?);
        self.mem.put(
            &self.owner,
            &scope,
            &semaphore_key(&s.name)?,
            &value,
            &PutOpts::default(),
        )?;
        Ok(())
    }

    pub fn get(&self, workspace_id: WorkspaceId, name: &str) -> Result<Option<Semaphore>> {
        let scope = Scope::Workspace(workspace_id);
        let entry = self.mem.get(&scope, &semaphore_key(name)?)?;
        match entry {
            Some(e) => match e.value {
                MemoryValue::Json(v) => {
                    let mut s: Semaphore = serde_json::from_value(v)?;
                    s.recompute_available();
                    Ok(Some(s))
                }
                _ => Err(AgentError::InvalidArgument(format!(
                    "semaphore entry is not json: {name}"
                ))),
            },
            None => Ok(None),
        }
    }

    fn require(&self, workspace_id: WorkspaceId, name: &str) -> Result<Semaphore> {
        self.get(workspace_id, name)?
            .ok_or_else(|| AgentError::InvalidArgument(format!("semaphore not found: {name}")))
    }

    /// 워크스페이스의 모든 semaphore. `now_ms` 가 있으면 만료된 홀더를 evict 한
    /// 뒤(영속 포함) 결과를 돌려준다 — [`crate::lease::LeaseStore::list`] 와 같은
    /// 규약이다.
    pub fn list(
        &mut self,
        workspace_id: WorkspaceId,
        now_ms: Option<u64>,
    ) -> Result<Vec<Semaphore>> {
        let scope = Scope::Workspace(workspace_id);
        let opts = ListOpts {
            prefix: Some(SEMAPHORE_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        let entries = self.mem.list(&scope, &opts)?;
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if let MemoryValue::Json(v) = e.value {
                let mut s: Semaphore = serde_json::from_value(v)?;
                let evicted = match now_ms {
                    Some(now) => s.evict_expired(now),
                    None => false,
                };
                s.recompute_available();
                if evicted {
                    self.put(&s)?;
                }
                out.push(s);
            }
        }
        Ok(out)
    }

    pub fn create(
        &mut self,
        workspace_id: WorkspaceId,
        name: impl Into<String>,
        permits: u32,
        now_ms: u64,
    ) -> Result<Semaphore> {
        let name = name.into();
        if permits == 0 {
            return Err(AgentError::InvalidArgument(
                "semaphore.permits must be >= 1".into(),
            ));
        }
        if let Some(existing) = self.get(workspace_id, &name)? {
            return Err(AgentError::InvalidArgument(format!(
                "semaphore '{name}' already exists (permits_total={}) — use set_permits to change it",
                existing.permits_total
            )));
        }
        let s = Semaphore {
            workspace_id,
            name,
            permits_total: permits,
            permits_available: permits,
            holders: Vec::new(),
            created_at: now_ms,
        };
        self.put(&s)?;
        Ok(s)
    }

    /// 한도를 원자적으로 조정한다. 확대는 즉시 반영되고, **축소는 drain** —
    /// 초과 홀더를 강제 회수하지 않고 그대로 두며 새 acquire 만 거절한다.
    pub fn set_permits(
        &mut self,
        workspace_id: WorkspaceId,
        name: &str,
        permits: u32,
        now_ms: u64,
    ) -> Result<Semaphore> {
        if permits == 0 {
            return Err(AgentError::InvalidArgument(
                "semaphore.permits must be >= 1".into(),
            ));
        }
        let mut s = self.require(workspace_id, name)?;
        s.evict_expired(now_ms);
        s.permits_total = permits;
        s.recompute_available();
        self.put(&s)?;
        Ok(s)
    }

    /// permit 1개 획득 시도. 이미 동일 holder 가 점유 중이면 idempotent 성공이며
    /// 그 홀더의 `acquired_at`/`expires_at` 를 갱신한다(heartbeat).
    ///
    /// `ttl_ms` 를 주면 그 홀더는 `now_ms + ttl_ms` 에 만료되어 다음 `acquire`
    /// 또는 `list(Some(now))` 에서 회수된다. 생략하면 만료되지 않는다.
    pub fn acquire(
        &mut self,
        workspace_id: WorkspaceId,
        name: &str,
        holder: &str,
        ttl_ms: Option<u64>,
        now_ms: u64,
    ) -> Result<AcquireOutcome> {
        if holder.is_empty() {
            return Err(AgentError::InvalidArgument(
                "holder must be non-empty".into(),
            ));
        }
        let mut s = self.require(workspace_id, name)?;
        let evicted = s.evict_expired(now_ms);
        let expires_at = ttl_ms.map(|t| now_ms.saturating_add(t));

        if let Some(h) = s.holders.iter_mut().find(|h| h.id == holder) {
            h.acquired_at = Some(now_ms);
            h.expires_at = expires_at;
            s.recompute_available();
            self.put(&s)?;
            return Ok(AcquireOutcome {
                acquired: true,
                semaphore: s,
            });
        }

        if !s.has_free_permit() {
            s.recompute_available();
            if evicted {
                self.put(&s)?;
            }
            return Ok(AcquireOutcome {
                acquired: false,
                semaphore: s,
            });
        }

        s.holders.push(SemaphoreHolder {
            id: holder.to_string(),
            acquired_at: Some(now_ms),
            expires_at,
        });
        s.recompute_available();
        self.put(&s)?;
        Ok(AcquireOutcome {
            acquired: true,
            semaphore: s,
        })
    }

    /// permit 반환. holder 가 점유 중이 아니면 no-op.
    pub fn release(
        &mut self,
        workspace_id: WorkspaceId,
        name: &str,
        holder: &str,
    ) -> Result<ReleaseOutcome> {
        let mut s = self.require(workspace_id, name)?;
        let before = s.holders.len();
        s.holders.retain(|h| h.id != holder);
        let released = s.holders.len() != before;
        s.recompute_available();
        if released {
            self.put(&s)?;
        }
        Ok(ReleaseOutcome {
            released,
            semaphore: s,
        })
    }

    pub fn delete(&mut self, workspace_id: WorkspaceId, name: &str) -> Result<()> {
        let scope = Scope::Workspace(workspace_id);
        self.mem
            .delete(&self.owner, &scope, &semaphore_key(name)?, None)?;
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

    fn ids(s: &Semaphore) -> Vec<&str> {
        s.holders.iter().map(|h| h.id.as_str()).collect()
    }

    #[test]
    fn create_then_two_acquires_third_fails() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        let r1 = store.acquire(1, "s1", "h1", None, 1000).unwrap();
        assert!(r1.acquired);
        assert_eq!(r1.semaphore.permits_available, 1);
        let r2 = store.acquire(1, "s1", "h2", None, 1000).unwrap();
        assert!(r2.acquired);
        assert_eq!(r2.semaphore.permits_available, 0);
        let r3 = store.acquire(1, "s1", "h3", None, 1000).unwrap();
        assert!(!r3.acquired);
        assert_eq!(r3.semaphore.permits_available, 0);
        assert_eq!(ids(&r3.semaphore), vec!["h1", "h2"]);
    }

    #[test]
    fn acquire_idempotent_for_same_holder() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 1, 1000).unwrap();
        let r1 = store.acquire(1, "s1", "h1", None, 1000).unwrap();
        assert!(r1.acquired);
        let r2 = store.acquire(1, "s1", "h1", None, 1100).unwrap();
        assert!(r2.acquired);
        assert_eq!(r2.semaphore.permits_available, 0);
        assert_eq!(ids(&r2.semaphore), vec!["h1"]);
    }

    #[test]
    fn acquire_records_when_the_permit_was_taken() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 1, 1000).unwrap();
        let r = store.acquire(1, "s1", "h1", None, 4242).unwrap();
        assert_eq!(r.semaphore.holders[0].acquired_at, Some(4242));
        assert_eq!(r.semaphore.holders[0].expires_at, None);
    }

    #[test]
    fn release_restores_permit() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        store.acquire(1, "s1", "h1", None, 1000).unwrap();
        let rel = store.release(1, "s1", "h1").unwrap();
        assert!(rel.released);
        assert_eq!(rel.semaphore.permits_available, 2);
        assert!(rel.semaphore.holders.is_empty());
    }

    #[test]
    fn release_unknown_holder_is_noop() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        store.acquire(1, "s1", "h1", None, 1000).unwrap();
        let rel = store.release(1, "s1", "ghost").unwrap();
        assert!(!rel.released);
        assert_eq!(rel.semaphore.permits_available, 1);
    }

    #[test]
    fn permits_zero_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        let err = store.create(1, "s", 0, 1000).unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
    }

    #[test]
    fn duplicate_create_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        let err = store.create(1, "s1", 3, 1001).unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
    }

    // ── 만료 ────────────────────────────────────────────────────────────

    /// 2026-09-04 실측 사고의 재현. permits 1 짜리 세마포어를 잡은 홀더가 반납
    /// 없이 사라지면(모델 사용 한도로 응답 불능) 대기자는 시간이 아무리 지나도
    /// 들어가지 못한다. `ttl_ms` 를 주지 않은 permit 이 **영구히** 묶인다는 사실
    /// 자체를 고정하는 음성 방향 테스트다 — 나중에 누가 "정리 좀 하자" 며 전역
    /// 기본 만료를 넣으면 여기서 잡힌다. 근거는 ADR-0119.
    #[test]
    fn a_permit_taken_without_a_ttl_is_never_reclaimed() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "gui-verify", 1, 1000).unwrap();
        assert!(
            store
                .acquire(1, "gui-verify", "h1", None, 1000)
                .unwrap()
                .acquired
        );

        // 홀더가 사라진다 — release 는 오지 않는다.
        const A_YEAR_MS: u64 = 365 * 24 * 60 * 60 * 1000;
        for waiter in ["w1", "w2", "w3"] {
            let r = store
                .acquire(1, "gui-verify", waiter, None, 1000 + A_YEAR_MS)
                .unwrap();
            assert!(!r.acquired, "{waiter} must stay blocked");
        }
        let list = store.list(1, Some(1000 + A_YEAR_MS)).unwrap();
        assert_eq!(ids(&list[0]), vec!["h1"]);
        assert_eq!(list[0].permits_available, 0);
    }

    /// 같은 시나리오에서 홀더가 `ttl_ms` 를 주고 잡았다면 만료 후 회수된다.
    #[test]
    fn a_ttl_reclaims_the_permit_of_a_holder_that_vanished() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "gui-verify", 1, 1000).unwrap();
        assert!(
            store
                .acquire(1, "gui-verify", "h1", Some(5_000), 1000)
                .unwrap()
                .acquired
        );

        // 만료 직전 — 대기자는 아직 막힌다.
        let early = store.acquire(1, "gui-verify", "w1", None, 5_999).unwrap();
        assert!(!early.acquired);
        assert_eq!(ids(&early.semaphore), vec!["h1"]);

        // 만료 시각 이후 — 죽은 홀더가 evict 되고 대기자가 들어간다.
        let late = store.acquire(1, "gui-verify", "w1", None, 6_000).unwrap();
        assert!(late.acquired);
        assert_eq!(ids(&late.semaphore), vec!["w1"]);
    }

    /// 살아 있는 홀더는 재acquire 로 만료를 미룬다(heartbeat).
    #[test]
    fn re_acquiring_renews_the_expiry() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 1, 1000).unwrap();
        let first = store.acquire(1, "s1", "h1", Some(5_000), 1000).unwrap();
        assert_eq!(first.semaphore.holders[0].expires_at, Some(6_000));

        let renewed = store.acquire(1, "s1", "h1", Some(5_000), 5_500).unwrap();
        assert!(renewed.acquired);
        assert_eq!(renewed.semaphore.holders[0].acquired_at, Some(5_500));
        assert_eq!(renewed.semaphore.holders[0].expires_at, Some(10_500));

        // 원래 만료 시각을 지나도 갱신 덕분에 여전히 점유 중이다.
        let blocked = store.acquire(1, "s1", "w1", None, 7_000).unwrap();
        assert!(!blocked.acquired);
        assert_eq!(ids(&blocked.semaphore), vec!["h1"]);
    }

    #[test]
    fn list_evicts_expired_holders_and_persists_it() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        store.acquire(1, "s1", "h1", Some(1_000), 1000).unwrap();
        store.acquire(1, "s1", "h2", None, 1000).unwrap();

        let listed = store.list(1, Some(9_000)).unwrap();
        assert_eq!(ids(&listed[0]), vec!["h2"]);
        // evict 가 영속됐는지 — 별도 조회로 확인.
        let after = store.get(1, "s1").unwrap().unwrap();
        assert_eq!(ids(&after), vec!["h2"]);
        assert_eq!(after.permits_available, 1);
    }

    #[test]
    fn list_without_a_clock_does_not_evict() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 1, 1000).unwrap();
        store.acquire(1, "s1", "h1", Some(1_000), 1000).unwrap();
        let listed = store.list(1, None).unwrap();
        assert_eq!(ids(&listed[0]), vec!["h1"]);
    }

    // ── 리사이즈 ────────────────────────────────────────────────────────

    #[test]
    fn growing_permits_keeps_existing_holders() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 1, 1000).unwrap();
        store.acquire(1, "s1", "h1", None, 1000).unwrap();
        assert!(!store.acquire(1, "s1", "h2", None, 1000).unwrap().acquired);

        let grown = store.set_permits(1, "s1", 2, 1100).unwrap();
        assert_eq!(grown.permits_total, 2);
        assert_eq!(grown.permits_available, 1);
        assert_eq!(ids(&grown), vec!["h1"]);

        let r = store.acquire(1, "s1", "h2", None, 1100).unwrap();
        assert!(r.acquired);
        assert_eq!(ids(&r.semaphore), vec!["h1", "h2"]);
    }

    /// 축소는 drain 이다 — 이미 임계구역에 들어간 홀더를 강제로 끌어내지 않는다.
    /// 홀더 수가 새 한도로 수렴할 때까지 새 대기자는 계속 거절된다.
    #[test]
    fn shrinking_drains_instead_of_revoking() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        store.acquire(1, "s1", "h1", None, 1000).unwrap();
        store.acquire(1, "s1", "h2", None, 1000).unwrap();

        let shrunk = store.set_permits(1, "s1", 1, 1100).unwrap();
        assert_eq!(shrunk.permits_total, 1);
        assert_eq!(
            ids(&shrunk),
            vec!["h1", "h2"],
            "홀더를 강제 회수하지 않는다"
        );
        assert!(shrunk.is_over_subscribed());
        assert_eq!(shrunk.permits_available, 0, "음수로 내려가지 않는다");

        assert!(!store.acquire(1, "s1", "w1", None, 1200).unwrap().acquired);

        // 한 명이 반납해도 아직 한도를 다 쓰고 있으므로 대기자는 못 들어온다.
        store.release(1, "s1", "h1").unwrap();
        let still_blocked = store.acquire(1, "s1", "w1", None, 1300).unwrap();
        assert!(!still_blocked.acquired);
        assert!(!still_blocked.semaphore.is_over_subscribed());
        assert_eq!(ids(&still_blocked.semaphore), vec!["h2"]);

        // 마지막 홀더가 반납하면 그때 자리가 난다.
        store.release(1, "s1", "h2").unwrap();
        assert!(store.acquire(1, "s1", "w1", None, 1400).unwrap().acquired);
    }

    #[test]
    fn set_permits_rejects_zero_and_missing_semaphores() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        assert!(matches!(
            store.set_permits(1, "s1", 0, 1000).unwrap_err(),
            AgentError::InvalidArgument(_)
        ));
        assert!(matches!(
            store.set_permits(1, "nope", 2, 1000).unwrap_err(),
            AgentError::InvalidArgument(_)
        ));
    }

    #[test]
    fn set_permits_also_evicts_expired_holders() {
        let (_td, mut mem) = fresh();
        let mut store = SemaphoreStore::new(&mut mem, "_host");
        store.create(1, "s1", 2, 1000).unwrap();
        store.acquire(1, "s1", "h1", Some(500), 1000).unwrap();
        let resized = store.set_permits(1, "s1", 3, 9_000).unwrap();
        assert!(resized.holders.is_empty());
        assert_eq!(resized.permits_available, 3);
    }

    // ── 구 형식 호환 ────────────────────────────────────────────────────

    /// `holders` 가 문자열 배열이던 시절의 레코드를 그대로 읽는다. 못 읽으면
    /// 실행 중 인스턴스의 세마포어가 부팅 시 통째로 사라진다.
    #[test]
    fn legacy_string_holders_still_load() {
        let (_td, mut mem) = fresh();
        let legacy = serde_json::json!({
            "workspace_id": 1u32,
            "name": "s1",
            "permits_total": 2u32,
            "permits_available": 0u32,
            "holders": ["h1", "h2"],
            "created_at": 1000u64,
        });
        mem.put(
            "_host",
            &Scope::Workspace(1),
            "tasty.agent.semaphore.s1",
            &MemoryValue::Json(legacy),
            &PutOpts::default(),
        )
        .unwrap();

        let mut store = SemaphoreStore::new(&mut mem, "_host");
        let s = store.get(1, "s1").unwrap().unwrap();
        assert_eq!(ids(&s), vec!["h1", "h2"]);
        assert_eq!(
            s.holders[0].acquired_at, None,
            "모르는 시각은 0 이 아니라 None"
        );
        assert_eq!(s.holders[0].expires_at, None);
        assert_eq!(s.permits_available, 0);

        // 구 홀더는 만료가 없으므로 evict 되지 않는다.
        let listed = store.list(1, Some(u64::MAX)).unwrap();
        assert_eq!(ids(&listed[0]), vec!["h1", "h2"]);

        // release 로 정상 회수된다.
        let rel = store.release(1, "s1", "h1").unwrap();
        assert!(rel.released);
        assert_eq!(ids(&rel.semaphore), vec!["h2"]);
    }
}
