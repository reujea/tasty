//! Lease primitive — 임의 resource 에 대한 협조적 점유 마커.
//!
//! 본 단계(Phase 5.3)는 **poll-based + 협조적 (advisory)** 모델이다. OS 락이
//! 아니므로 lease 를 무시한 채 resource 를 만지는 행위 자체는 막지 못한다.
//! 다중 에이전트가 자발적으로 점유 상태를 조회하고, 충돌이면 우회하거나
//! 재시도하기로 약속할 때 의미가 있다.
//!
//! 영속: `tasty.agent.lease.<resource>` (workspace scope).
//!
//! 모드:
//! - `fail` (기본): 점유 충돌 시 `AgentError::LeaseConflict` 로 즉시 실패.
//! - `block`: 점유 충돌 시 `acquired=false` 로 반환 (호출자가 다시 시도).
//!
//! TTL:
//! - `ttl_ms` 가 있으면 `expires_at = acquired_at + ttl_ms`. 만료된 lease 는
//!   다음 `acquire` 호출 시점에 lazy 하게 evict (다른 holder 가 점유 가능).
//!
//! 점유 규칙:
//! - 같은 holder 가 다시 `acquire` 하면 idempotent 갱신 (acquired_at /
//!   expires_at 만 갱신, 점유는 유지).
//! - `release` 는 점유 holder 만 가능; 다른 holder 가 `release` 호출하면 no-op.
//!
//! ## Pool 모드 (`acquire_any`)
//!
//! `acquire`(단일 resource)와 별도로, "N개 후보 중 아무거나 하나"를 배정받는
//! `acquire_any` 를 제공한다. `resource: String` 하나만 쓰는 기존 `acquire` 는
//! `candidates: [resource]` 의 퇴화형과 관측적으로 동일하다 — 두 경로 모두
//! 같은 `lease_key(resource)` 위치에 쓰기 때문에 store 상에서 자연히 충돌
//! 판정이 일치한다. 별도 코드 경로 통합(rewrite)은 하지 않는다 — `acquire` 는
//! 기존 호출자(`agent.lease_acquire` IPC, purge 정화 루틴)가 기대하는
//! `LeaseConflict` 에러 형태를 그대로 유지해야 하므로 독립 구현을 보존한다.
//!
//! 두 서브모드:
//! - **fixed** (기본, `elastic` 생략): 주어진 `candidates` 안에서만 순회.
//!   전부 점유 중이면 `mode` 에 따라 실패(`Fail`) 또는 대기(`Block` →
//!   `acquired=false`).
//! - **elastic** (`elastic: Some(ElasticSpec)` — 명시적 opt-in): candidates
//!   가 모두 소진되면 `overflow_prefix + N` 형태의 새 후보 이름을 원자적으로
//!   합성해 즉시 점유한다. `max_candidates` 가 있으면 그 상한(고정 candidates
//!   개수 + 합성된 개수)까지만 증설하고, 넘으면 fixed 와 동일하게 대기/실패.
//!
//! 합성된 이름의 원자성: pool 별로 카운터를 하나 영속(`pool_counter_key`)
//! 해 두고, "현재 카운터 읽기 → 후보 스캔 → (소진 시) 카운터 +1 → 새 이름으로
//! acquire" 전체를 **`acquire_any` 한 호출 안에서, 같은 `&mut dyn
//! MemoryStorage` 로 순차 수행**한다. 호출자(`HostExecutor::try_acquire_lease`)
//! 가 이 호출 전체를 `RunnerContext::with_memory` 클로저 하나 안에서 실행하는
//! 한(기존 관례와 동일), 그 클로저가 프로세스 전역 `Mutex` 를 처음부터 끝까지
//! 쥐고 있으므로 다른 스레드(다른 workspace runner, IPC 핸들러 등)가 같은
//! pool 카운터 키를 동시에 읽고 쓸 수 없다 — 별도 CAS/락 primitive 없이 기존
//! `with_memory` 관례만으로 충분하다 (`crates/tasty-agent/src/lease.rs`
//! 밖의 근거: `RunnerRegistry` 가 workspace 당 runner thread 를 정확히 하나만
//! 허용해 워크스페이스 내부 경쟁도 없고, `Core::memory` 는 모든 workspace가
//! 공유하는 단일 `Arc<Mutex<dyn MemoryStorage>>` 라 워크스페이스 간 경쟁은 그
//! 전역 락 하나로 직렬화된다).
//!
//! 합성된 candidate 의 재사용: 카운터는 "지금까지 합성된 개수의 상한"일 뿐,
//! 현재 점유 개수가 아니다. `acquire_any` 는 매번 `candidates ++
//! (1..=counter 로 합성된 이름들)` 전체를 다시 스캔하므로, 합성됐다가
//! `release` 된 이름은 다음 `acquire_any` 호출에서 빈 자리로 재발견되어
//! 재사용된다 — 카운터가 증가하는 건 오직 "그 스캔에서도 빈 자리가 전혀
//! 없었을 때"뿐이다.

use serde::{Deserialize, Serialize};
use tasty_memory::{ListOpts, MemoryStorage, MemoryValue, PutOpts, Scope};
use tasty_utils::id::WorkspaceId;

use crate::{AgentError, Result};

pub const LEASE_KEY_PREFIX: &str = "tasty.agent.lease.";

/// pool 합성 카운터 영속 key prefix (workspace scope). key 자체는
/// [`pool_counter_key`] 가 pool 을 식별하는 `candidates` 배열로부터 만든다.
pub const LEASE_POOL_COUNTER_KEY_PREFIX: &str = "tasty.agent.lease_pool_counter.";

/// 임의 바이트열을 memory 키 허용 문자(`[a-z0-9._-]`)만 쓰는 stable 토큰으로
/// 변환. 디코딩이 필요 없으므로(원본은 JSON value 에 같이 저장됨) 안전한 단방향
/// 인코딩이면 충분. `_` 는 escape sentinel 이라 `__` 로 더블링하고, 그 외
/// 비허용 바이트는 `_<hex>` 로 치환한다 — 두 형식이 충돌하지 않음.
fn encode_key_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b == b'_' {
            out.push_str("__");
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' {
            out.push(b as char);
        } else {
            out.push('_');
            out.push_str(&format!("{:02x}", b));
        }
    }
    out
}

fn lease_key(resource: &str) -> String {
    format!("{LEASE_KEY_PREFIX}{}", encode_key_component(resource))
}

/// pool 카운터 key. `_-` 를 구분자로 각 candidate 를 이어붙여 pool 정체성을
/// 만든다 — memory key 허용 문자(`[a-z0-9._-]`)만 써야 해서 `~` 같은 별도
/// 구분자 문자를 못 쓴다. `_-` 는 [`encode_key_component`] 가 절대 만들어내지
/// 않는 2바이트 시퀀스라 구분자로 안전하다: 그 함수가 내는 이스케이프는
/// `__`(리터럴 `_`) 아니면 `_` + 소문자 hex 2자리뿐이고 `-` 는 hex 자릿수가
/// 아니므로 `_-` 는 이스케이프 출력에 결코 나타나지 않는다(raw `-` 는 애초에
/// 이스케이프 없이 그대로 통과한다). pool 정체성은 candidates 배열(순서 포함)
/// 그 자체다 — 같은 배열을 선언하는 모든 호출이 같은 pool 카운터를 공유한다.
const POOL_KEY_SEPARATOR: &str = "_-";

fn pool_counter_key(candidates: &[String]) -> String {
    let mut out = String::from(LEASE_POOL_COUNTER_KEY_PREFIX);
    for (i, c) in candidates.iter().enumerate() {
        if i > 0 {
            out.push_str(POOL_KEY_SEPARATOR);
        }
        out.push_str(&encode_key_component(c));
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LeaseMode {
    /// 충돌 시 즉시 `LeaseConflict` 에러 반환.
    #[default]
    Fail,
    /// 충돌 시 `acquired=false` 로 반환 — 호출자가 polling.
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub workspace_id: WorkspaceId,
    pub resource: String,
    pub holder: String,
    pub acquired_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

impl Lease {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        match self.expires_at {
            Some(t) => now_ms >= t,
            None => false,
        }
    }
}

/// `acquire` 응답. `mode=block` 에서 점유 충돌 시 `acquired=false` 와 함께
/// 현재 점유 중인 lease 가 들어 있다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquireOutcome {
    pub acquired: bool,
    pub lease: Lease,
}

/// `acquire_any` 의 elastic(자동 증설) 옵션. 필드가 있어도(`{}` 포함) 이 값이
/// `Some` 이라는 사실 자체가 elastic opt-in — fixed(기본)에서는 `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElasticSpec {
    /// 이 pool 이 가질 수 있는 candidate 총량(원본 candidates 개수 + 합성된
    /// 개수) 상한. `None` 이면 무제한.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_candidates: Option<u32>,
    /// 합성 이름의 prefix — 실제 이름은 `{overflow_prefix}{N}`(`N` 은 1부터).
    /// 생략하면 `{candidates.last()}-overflow-` 를 기본값으로 쓴다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_prefix: Option<String>,
}

/// `acquire_any` 응답. `acquired=true` 일 때 `resource`/`lease` 가 채워진다 —
/// 실제로 어느 candidate(신규 합성 포함)를 받았는지는 `resource` 로 드러난다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquireAnyOutcome {
    pub acquired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<Lease>,
    /// elastic 모드로 이번 호출에서 새로 합성된 candidate 였는지 (기존
    /// candidates 안에서 찾았거나 이미 합성돼 있던 이름을 재사용했으면 false).
    #[serde(default)]
    pub synthesized: bool,
}

/// `release` 응답.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseOutcome {
    /// 호출 holder 가 실제로 점유 중이었는지 (즉, 본 호출로 release 가 일어났는지).
    pub released: bool,
    /// release 직전의 lease (또는 다른 holder 점유 시 그 lease). resource 자체가
    /// 점유 중이 아니었으면 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<Lease>,
}

pub struct LeaseStore<'a> {
    mem: &'a mut dyn MemoryStorage,
    owner: String,
}

impl<'a> LeaseStore<'a> {
    pub fn new(mem: &'a mut dyn MemoryStorage, owner: impl Into<String>) -> Self {
        Self {
            mem,
            owner: owner.into(),
        }
    }

    fn put(&mut self, lease: &Lease) -> Result<()> {
        let scope = Scope::Workspace(lease.workspace_id);
        let value = MemoryValue::Json(serde_json::to_value(lease)?);
        self.mem.put(
            &self.owner,
            &scope,
            &lease_key(&lease.resource),
            &value,
            &PutOpts::default(),
        )?;
        Ok(())
    }

    fn delete_key(&mut self, workspace_id: WorkspaceId, resource: &str) -> Result<()> {
        let scope = Scope::Workspace(workspace_id);
        self.mem
            .delete(&self.owner, &scope, &lease_key(resource), None)?;
        Ok(())
    }

    pub fn get(&self, workspace_id: WorkspaceId, resource: &str) -> Result<Option<Lease>> {
        let scope = Scope::Workspace(workspace_id);
        let entry = self.mem.get(&scope, &lease_key(resource))?;
        match entry {
            Some(e) => match e.value {
                MemoryValue::Json(v) => Ok(Some(serde_json::from_value(v)?)),
                _ => Err(AgentError::InvalidArgument(format!(
                    "lease entry is not json: {resource}"
                ))),
            },
            None => Ok(None),
        }
    }

    /// 워크스페이스의 모든 lease. `now_ms` 가 있으면 만료된 lease 는 evict 한 뒤
    /// 결과에서도 제외한다 (영속도 제거).
    pub fn list(&mut self, workspace_id: WorkspaceId, now_ms: Option<u64>) -> Result<Vec<Lease>> {
        let scope = Scope::Workspace(workspace_id);
        let opts = ListOpts {
            prefix: Some(LEASE_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        let entries = self.mem.list(&scope, &opts)?;
        let mut alive: Vec<Lease> = Vec::with_capacity(entries.len());
        let mut expired: Vec<String> = Vec::new();
        for e in entries {
            if let MemoryValue::Json(v) = e.value {
                let lease: Lease = serde_json::from_value(v)?;
                if let Some(now) = now_ms
                    && lease.is_expired(now)
                {
                    expired.push(lease.resource.clone());
                    continue;
                }
                alive.push(lease);
            }
        }
        for r in expired {
            self.delete_key(workspace_id, &r)?;
        }
        Ok(alive)
    }

    /// resource 에 대해 lease 점유 시도. 같은 holder 면 idempotent 갱신 (TTL 도 재설정).
    /// 다른 holder 가 점유 중이고 만료 안 됐다면 mode 에 따라 분기한다.
    pub fn acquire(
        &mut self,
        workspace_id: WorkspaceId,
        resource: &str,
        holder: &str,
        ttl_ms: Option<u64>,
        mode: LeaseMode,
        now_ms: u64,
    ) -> Result<AcquireOutcome> {
        if resource.is_empty() {
            return Err(AgentError::InvalidArgument(
                "lease.resource must be non-empty".into(),
            ));
        }
        if holder.is_empty() {
            return Err(AgentError::InvalidArgument(
                "lease.holder must be non-empty".into(),
            ));
        }

        let existing = self.get(workspace_id, resource)?;
        if let Some(cur) = existing {
            if cur.holder == holder {
                // idempotent 재acquire — TTL 갱신.
                let lease = Lease {
                    workspace_id,
                    resource: resource.to_string(),
                    holder: holder.to_string(),
                    acquired_at: now_ms,
                    expires_at: ttl_ms.map(|t| now_ms.saturating_add(t)),
                };
                self.put(&lease)?;
                return Ok(AcquireOutcome {
                    acquired: true,
                    lease,
                });
            }
            if !cur.is_expired(now_ms) {
                return match mode {
                    LeaseMode::Fail => Err(AgentError::LeaseConflict {
                        resource: resource.to_string(),
                        holder: cur.holder.clone(),
                    }),
                    LeaseMode::Block => Ok(AcquireOutcome {
                        acquired: false,
                        lease: cur,
                    }),
                };
            }
            // 만료 — evict 후 새로 점유.
        }

        let lease = Lease {
            workspace_id,
            resource: resource.to_string(),
            holder: holder.to_string(),
            acquired_at: now_ms,
            expires_at: ttl_ms.map(|t| now_ms.saturating_add(t)),
        };
        self.put(&lease)?;
        Ok(AcquireOutcome {
            acquired: true,
            lease,
        })
    }

    fn get_pool_counter(&self, workspace_id: WorkspaceId, candidates: &[String]) -> Result<u32> {
        let scope = Scope::Workspace(workspace_id);
        let entry = self.mem.get(&scope, &pool_counter_key(candidates))?;
        match entry {
            Some(e) => match e.value {
                MemoryValue::Json(v) => Ok(serde_json::from_value(v).unwrap_or(0)),
                _ => Ok(0),
            },
            None => Ok(0),
        }
    }

    fn put_pool_counter(
        &mut self,
        workspace_id: WorkspaceId,
        candidates: &[String],
        value: u32,
    ) -> Result<()> {
        let scope = Scope::Workspace(workspace_id);
        let value = MemoryValue::Json(serde_json::to_value(value)?);
        self.mem.put(
            &self.owner,
            &scope,
            &pool_counter_key(candidates),
            &value,
            &PutOpts::default(),
        )?;
        Ok(())
    }

    /// pool 카운터를 읽고 +1 해서 즉시 다시 쓴다. 호출자(`acquire_any`)가 이미
    /// `RunnerContext::with_memory` 같은 단일 락 구간 안에서 호출하는 한 다른
    /// 스레드가 그 사이에 끼어들 수 없다 — 모듈 문서 "합성된 이름의 원자성" 참조.
    fn bump_pool_counter(
        &mut self,
        workspace_id: WorkspaceId,
        candidates: &[String],
    ) -> Result<u32> {
        let next = self
            .get_pool_counter(workspace_id, candidates)?
            .saturating_add(1);
        self.put_pool_counter(workspace_id, candidates, next)?;
        Ok(next)
    }

    /// `candidates` 중 하나를 점유. `elastic` 이 `Some` 이면 전부 소진 시 새
    /// candidate 를 합성해 즉시 점유(상한은 `ElasticSpec::max_candidates`).
    /// `elastic` 이 `None` 이면(기본, fixed) candidates 안에서만 순회한다.
    ///
    /// 판정 순서:
    /// 1. `holder` 가 이미 이 pool 의 어느 candidate(합성분 포함)를 쥐고 있으면
    ///    그 candidate 를 idempotent 갱신해 반환 — 두 개를 동시에 쥐는 걸
    ///    막기 위해 "빈 자리부터 새로 점유" 보다 항상 우선한다.
    /// 2. candidates ++ (이미 합성된 이름들)을 순서대로 스캔해 첫 빈 자리를
    ///    점유.
    /// 3. 전부 점유 중이고 elastic 이면(+ 상한 이내면) 새 이름을 합성해 점유.
    /// 4. 그래도 못 받으면 `mode` 에 따라 실패(`Fail`) 또는 `acquired=false`
    ///    (`Block`).
    // 7개 파라미터 전부 서로 독립적인 원시값(struct로 묶어도 호출부 가독성만
    // 나빠짐) — `acquire`(단일 resource, 6개)와 같은 스타일을 유지한다.
    #[allow(clippy::too_many_arguments)]
    pub fn acquire_any(
        &mut self,
        workspace_id: WorkspaceId,
        candidates: &[String],
        holder: &str,
        ttl_ms: Option<u64>,
        mode: LeaseMode,
        elastic: Option<&ElasticSpec>,
        now_ms: u64,
    ) -> Result<AcquireAnyOutcome> {
        if candidates.is_empty() {
            return Err(AgentError::InvalidArgument(
                "lease.candidates must be non-empty".into(),
            ));
        }
        if holder.is_empty() {
            return Err(AgentError::InvalidArgument(
                "lease.holder must be non-empty".into(),
            ));
        }

        let default_prefix = || format!("{}-overflow-", candidates.last().expect("non-empty"));
        let overflow_prefix = elastic
            .and_then(|e| e.overflow_prefix.clone())
            .unwrap_or_else(default_prefix);

        // fixed candidates ++ 이미 합성된 이름들 (elastic 일 때만).
        let mut scan_list: Vec<String> = candidates.to_vec();
        if elastic.is_some() {
            let synthesized_so_far = self.get_pool_counter(workspace_id, candidates)?;
            for i in 1..=synthesized_so_far {
                scan_list.push(format!("{overflow_prefix}{i}"));
            }
        }

        // 1. idempotent: holder 가 이미 이 pool 의 뭔가를 쥐고 있으면 그걸 갱신.
        for r in &scan_list {
            if let Some(cur) = self.get(workspace_id, r)?
                && cur.holder == holder
                && !cur.is_expired(now_ms)
            {
                let outcome =
                    self.acquire(workspace_id, r, holder, ttl_ms, LeaseMode::Block, now_ms)?;
                return Ok(AcquireAnyOutcome {
                    acquired: true,
                    resource: Some(r.clone()),
                    lease: Some(outcome.lease),
                    synthesized: false,
                });
            }
        }

        // 2. 빈 자리 스캔 (block 모드로 시도 — 충돌은 그냥 다음 candidate 로).
        for r in &scan_list {
            let outcome =
                self.acquire(workspace_id, r, holder, ttl_ms, LeaseMode::Block, now_ms)?;
            if outcome.acquired {
                return Ok(AcquireAnyOutcome {
                    acquired: true,
                    resource: Some(r.clone()),
                    lease: Some(outcome.lease),
                    synthesized: false,
                });
            }
        }

        // 3. elastic — 상한 이내면 새 이름을 합성해 즉시 점유.
        if let Some(spec) = elastic {
            let cap_ok = match spec.max_candidates {
                Some(max) => (scan_list.len() as u32) < max,
                None => true,
            };
            if cap_ok {
                let new_counter = self.bump_pool_counter(workspace_id, candidates)?;
                let new_resource = format!("{overflow_prefix}{new_counter}");
                let outcome = self.acquire(
                    workspace_id,
                    &new_resource,
                    holder,
                    ttl_ms,
                    LeaseMode::Block,
                    now_ms,
                )?;
                if outcome.acquired {
                    return Ok(AcquireAnyOutcome {
                        acquired: true,
                        resource: Some(new_resource),
                        lease: Some(outcome.lease),
                        synthesized: true,
                    });
                }
                // 극히 드문 이름 충돌(사용자가 정적 candidate 로 같은 이름을 이미
                // 씀) — 아래 소진 처리로 폴백.
            }
        }

        // 4. 소진.
        match mode {
            LeaseMode::Fail => Err(AgentError::LeasePoolExhausted {
                candidates: scan_list,
                holder: holder.to_string(),
            }),
            LeaseMode::Block => {
                let lease = self.get(workspace_id, &scan_list[0])?;
                Ok(AcquireAnyOutcome {
                    acquired: false,
                    resource: None,
                    lease,
                    synthesized: false,
                })
            }
        }
    }

    /// 점유 holder 만 release 가능. 다른 holder 가 호출하면 no-op (현 점유는 유지).
    pub fn release(
        &mut self,
        workspace_id: WorkspaceId,
        resource: &str,
        holder: &str,
    ) -> Result<ReleaseOutcome> {
        let existing = self.get(workspace_id, resource)?;
        let Some(cur) = existing else {
            return Ok(ReleaseOutcome {
                released: false,
                lease: None,
            });
        };
        if cur.holder != holder {
            return Ok(ReleaseOutcome {
                released: false,
                lease: Some(cur),
            });
        }
        self.delete_key(workspace_id, resource)?;
        Ok(ReleaseOutcome {
            released: true,
            lease: Some(cur),
        })
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
    fn acquire_then_conflict_fail_mode() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let r = store
            .acquire(1, "file:/a", "h1", None, LeaseMode::Fail, 1000)
            .unwrap();
        assert!(r.acquired);
        let err = store
            .acquire(1, "file:/a", "h2", None, LeaseMode::Fail, 1100)
            .unwrap_err();
        match err {
            AgentError::LeaseConflict { resource, holder } => {
                assert_eq!(resource, "file:/a");
                assert_eq!(holder, "h1");
            }
            _ => panic!("expected LeaseConflict, got {err:?}"),
        }
    }

    #[test]
    fn acquire_conflict_block_mode_returns_acquired_false() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        store
            .acquire(1, "file:/a", "h1", None, LeaseMode::Fail, 1000)
            .unwrap();
        let r = store
            .acquire(1, "file:/a", "h2", None, LeaseMode::Block, 1100)
            .unwrap();
        assert!(!r.acquired);
        assert_eq!(r.lease.holder, "h1");
    }

    #[test]
    fn same_holder_reacquire_is_idempotent_and_renews_ttl() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let r1 = store
            .acquire(1, "file:/a", "h1", Some(500), LeaseMode::Fail, 1000)
            .unwrap();
        assert_eq!(r1.lease.expires_at, Some(1500));
        let r2 = store
            .acquire(1, "file:/a", "h1", Some(800), LeaseMode::Fail, 1200)
            .unwrap();
        assert!(r2.acquired);
        assert_eq!(r2.lease.acquired_at, 1200);
        assert_eq!(r2.lease.expires_at, Some(2000));
    }

    #[test]
    fn expired_lease_can_be_taken_by_other_holder() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        store
            .acquire(1, "file:/a", "h1", Some(500), LeaseMode::Fail, 1000)
            .unwrap();
        // 만료 시점 이후 다른 holder 가 acquire.
        let r = store
            .acquire(1, "file:/a", "h2", None, LeaseMode::Fail, 2000)
            .unwrap();
        assert!(r.acquired);
        assert_eq!(r.lease.holder, "h2");
    }

    #[test]
    fn release_only_by_holder() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        store
            .acquire(1, "file:/a", "h1", None, LeaseMode::Fail, 1000)
            .unwrap();
        let r = store.release(1, "file:/a", "h2").unwrap();
        assert!(!r.released);
        // 여전히 h1 점유.
        let cur = store.get(1, "file:/a").unwrap().unwrap();
        assert_eq!(cur.holder, "h1");

        let r = store.release(1, "file:/a", "h1").unwrap();
        assert!(r.released);
        assert!(store.get(1, "file:/a").unwrap().is_none());
    }

    #[test]
    fn release_unheld_resource_is_noop() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let r = store.release(1, "file:/none", "h1").unwrap();
        assert!(!r.released);
        assert!(r.lease.is_none());
    }

    #[test]
    fn list_evicts_expired_when_now_supplied() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        store
            .acquire(1, "file:/a", "h1", Some(500), LeaseMode::Fail, 1000)
            .unwrap();
        store
            .acquire(1, "file:/b", "h2", None, LeaseMode::Fail, 1000)
            .unwrap();
        let alive = store.list(1, Some(2000)).unwrap();
        let resources: Vec<_> = alive.iter().map(|l| l.resource.clone()).collect();
        assert_eq!(resources, vec!["file:/b".to_string()]);
        // /a 가 영속에서도 제거되었는지 확인.
        assert!(store.get(1, "file:/a").unwrap().is_none());
    }

    #[test]
    fn empty_resource_or_holder_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let err = store
            .acquire(1, "", "h1", None, LeaseMode::Fail, 1000)
            .unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
        let err = store
            .acquire(1, "file:/a", "", None, LeaseMode::Fail, 1000)
            .unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
    }

    fn candidates(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // --- acquire_any: fixed (elastic 미지정) ---

    #[test]
    fn acquire_any_fixed_distributes_distinct_resources_and_blocks_when_exhausted() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let pool = candidates(&["wt-1", "wt-2", "wt-3"]);
        let mut got = Vec::new();
        for i in 0..3 {
            let o = store
                .acquire_any(
                    1,
                    &pool,
                    &format!("h{i}"),
                    None,
                    LeaseMode::Block,
                    None,
                    1000,
                )
                .unwrap();
            assert!(o.acquired, "holder h{i} should get a resource");
            assert!(!o.synthesized);
            got.push(o.resource.unwrap());
        }
        // 서로 다른 자원.
        let mut sorted = got.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            3,
            "expected 3 distinct resources, got {got:?}"
        );

        // 4번째는 대기(block) — 새 candidate 합성 안 됨.
        let o4 = store
            .acquire_any(1, &pool, "h3", None, LeaseMode::Block, None, 1000)
            .unwrap();
        assert!(!o4.acquired);
        assert!(!o4.synthesized);

        // 5번째도 Fail 모드면 에러.
        let err = store
            .acquire_any(1, &pool, "h4", None, LeaseMode::Fail, None, 1000)
            .unwrap_err();
        assert!(matches!(err, AgentError::LeasePoolExhausted { .. }));

        // 카운터가 절대 증가하지 않았는지(합성 이름이 전혀 없음) — pool 카운터
        // 키가 store 에 존재하지 않아야 한다.
        let counter = store.get_pool_counter(1, &pool).unwrap();
        assert_eq!(counter, 0, "fixed mode must never synthesize");

        // 하나 반환(release)하면 대기하던 holder 가 바로 이어받을 수 있어야
        // 한다 — 이건 dispatch 루프(RunnerLoop)가 재시도할 때 acquire_any 를
        // 다시 부르는 방식으로 구현되므로, 여기서는 release 후 재시도가
        // 성공하는지만 확인.
        store.release(1, &got[0], "h0").unwrap();
        let o4_retry = store
            .acquire_any(1, &pool, "h3", None, LeaseMode::Block, None, 1000)
            .unwrap();
        assert!(o4_retry.acquired);
        assert_eq!(o4_retry.resource.unwrap(), got[0]);
    }

    #[test]
    fn acquire_any_fixed_idempotent_same_holder_returns_same_resource() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let pool = candidates(&["wt-1", "wt-2"]);
        let o1 = store
            .acquire_any(1, &pool, "h0", Some(500), LeaseMode::Block, None, 1000)
            .unwrap();
        assert!(o1.acquired);
        let r1 = o1.resource.unwrap();
        // 같은 holder 가 다시 호출 — 이미 쥔 자원을 그대로 갱신해 반환해야
        // 하고, 다른 candidate 를 새로 점유하면 안 된다.
        let o2 = store
            .acquire_any(1, &pool, "h0", Some(900), LeaseMode::Block, None, 1200)
            .unwrap();
        assert!(o2.acquired);
        assert_eq!(o2.resource.unwrap(), r1);
        assert_eq!(o2.lease.unwrap().expires_at, Some(2100));
    }

    // --- acquire_any: elastic ---

    #[test]
    fn acquire_any_elastic_unbounded_synthesizes_beyond_fixed_candidates() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let pool = candidates(&["wt-1", "wt-2", "wt-3"]);
        let elastic = ElasticSpec::default(); // {} — 무제한.
        let mut got = Vec::new();
        for i in 0..5 {
            let o = store
                .acquire_any(
                    1,
                    &pool,
                    &format!("h{i}"),
                    None,
                    LeaseMode::Block,
                    Some(&elastic),
                    1000,
                )
                .unwrap();
            assert!(
                o.acquired,
                "holder h{i} must not wait under unbounded elastic"
            );
            got.push((o.resource.unwrap(), o.synthesized));
        }
        // 5개 모두 서로 다른 자원.
        let mut names: Vec<_> = got.iter().map(|(r, _)| r.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 5);
        // 원본 3개는 합성이 아니었고, 나머지 2개는 합성됐다.
        let synthesized_count = got.iter().filter(|(_, s)| *s).count();
        assert_eq!(synthesized_count, 2);
        let fixed_count = got.iter().filter(|(r, _)| pool.contains(r)).count();
        assert_eq!(fixed_count, 3);
    }

    #[test]
    fn acquire_any_elastic_max_candidates_caps_synthesis() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let pool = candidates(&["wt-1", "wt-2", "wt-3"]);
        let elastic = ElasticSpec {
            max_candidates: Some(4),
            overflow_prefix: None,
        };
        let mut acquired_count = 0;
        for i in 0..6 {
            let o = store
                .acquire_any(
                    1,
                    &pool,
                    &format!("h{i}"),
                    None,
                    LeaseMode::Block,
                    Some(&elastic),
                    1000,
                )
                .unwrap();
            if o.acquired {
                acquired_count += 1;
            }
        }
        // 3(fixed) + 1(합성 상한까지) = 4 개만 즉시 점유, 나머지 2개는 대기.
        assert_eq!(acquired_count, 4);
        let counter = store.get_pool_counter(1, &pool).unwrap();
        assert_eq!(
            counter, 1,
            "only one candidate should have been synthesized"
        );
    }

    #[test]
    fn acquire_any_elastic_reuses_released_synthesized_candidate() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let pool = candidates(&["wt-1"]);
        let elastic = ElasticSpec::default();
        // wt-1 을 채우고, 2번째 holder 가 합성 자원을 받는다.
        store
            .acquire_any(1, &pool, "h0", None, LeaseMode::Block, Some(&elastic), 1000)
            .unwrap();
        let o2 = store
            .acquire_any(1, &pool, "h1", None, LeaseMode::Block, Some(&elastic), 1000)
            .unwrap();
        assert!(o2.synthesized);
        let synthesized_name = o2.resource.unwrap();
        assert_eq!(store.get_pool_counter(1, &pool).unwrap(), 1);

        // 합성 자원 반환 후 3번째 holder 가 acquire — 카운터가 또 증가하지
        // 않고, 반환된 그 이름을 재사용해야 한다.
        store.release(1, &synthesized_name, "h1").unwrap();
        let o3 = store
            .acquire_any(1, &pool, "h2", None, LeaseMode::Block, Some(&elastic), 1000)
            .unwrap();
        assert!(o3.acquired);
        assert!(
            !o3.synthesized,
            "released synthesized name must be reused, not re-synthesized"
        );
        assert_eq!(o3.resource.unwrap(), synthesized_name);
        assert_eq!(store.get_pool_counter(1, &pool).unwrap(), 1);
    }

    // --- sugar 통합: resource(단일) == candidates([단일]) ---

    #[test]
    fn resource_sugar_and_single_candidate_pool_collide_on_same_lease_key() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        // 기존 단일-resource acquire 로 먼저 점유.
        store
            .acquire(1, "single-path", "h1", None, LeaseMode::Fail, 1000)
            .unwrap();
        // candidates:["single-path"] 로 다른 holder 가 pool acquire 시도 —
        // 같은 lease_key 를 보고 있어야 충돌(대기)한다.
        let pool = candidates(&["single-path"]);
        let o = store
            .acquire_any(1, &pool, "h2", None, LeaseMode::Block, None, 1000)
            .unwrap();
        assert!(
            !o.acquired,
            "must conflict with the plain acquire on the same resource"
        );

        // 반대 방향도 확인: pool 로 먼저 점유 후, 단일-resource acquire 가
        // 같은 키에서 충돌 판정.
        let (_td2, mut mem2) = fresh();
        let mut store2 = LeaseStore::new(&mut mem2, "_host");
        let o1 = store2
            .acquire_any(1, &pool, "h1", None, LeaseMode::Block, None, 1000)
            .unwrap();
        assert!(o1.acquired);
        assert_eq!(o1.resource.as_deref(), Some("single-path"));
        let err = store2
            .acquire(1, "single-path", "h2", None, LeaseMode::Fail, 1000)
            .unwrap_err();
        assert!(matches!(err, AgentError::LeaseConflict { .. }));
    }

    #[test]
    fn acquire_any_empty_candidates_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = LeaseStore::new(&mut mem, "_host");
        let err = store
            .acquire_any(1, &[], "h1", None, LeaseMode::Fail, None, 1000)
            .unwrap_err();
        assert!(matches!(err, AgentError::InvalidArgument(_)));
    }
}
