//! Rate limit primitive — token bucket 기반 시간당 비율 제한.
//!
//! `telemetry.cap` 과 구분되는 점:
//!
//! | 시스템 | 의미 |
//! |---|---|
//! | telemetry.cap | 누적 임계 (예: input_tokens 총합 ≥ 100000 → 차단) |
//! | agent.rate_limit | 시간당 비율 (예: ipc_calls 100/분 → 101번째 throttle) |
//!
//! 영속: `tasty.agent.rate_limit.<id>` (Global scope — agent/workspace 무관).
//!
//! CRUD + `try_consume` 을 제공하며, IPC dispatcher 미들웨어(`handler.rs` 의
//! `should_rate_limit` + `rate_limit_try_consume`)가 매 호출마다 자동 평가한다.
//! 호출자가 직접 `try_consume` 으로 토큰 소비를 검사할 수도 있다.
//!
//! 토큰 버킷:
//! - 용량 = `burst` (기본 = `limit`)
//! - 보충 속도 = `limit / per_ms` 토큰/ms
//! - `try_consume(now_ms, cost)`: 마지막 refill 이후 경과 시간만큼 토큰 보충 → cost
//!   만큼 차감. 부족하면 `allowed=false`, 차감 안 함.

use serde::{Deserialize, Serialize};
use tasty_memory::{ListOpts, MemoryStorage, MemoryValue, PutOpts, Scope};

use crate::{AgentError, Result};

pub const RATE_LIMIT_KEY_PREFIX: &str = "tasty.agent.rate_limit.";

fn rate_limit_key(id: &str) -> Result<String> {
    crate::component_key(RATE_LIMIT_KEY_PREFIX, "rate_limit id", id)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimit {
    pub id: String,
    pub agent: String,
    pub metric: String,
    /// `per_ms` 당 허용 토큰 수 (= 보충량 / window).
    pub limit: u32,
    /// 비율 윈도우 길이 (ms). 예: 60_000 = 1분.
    pub per_ms: u64,
    /// 버킷 용량 (burst). 기본 = `limit` (즉시 limit 까지 누적 가능).
    pub burst: u32,
    /// 현재 잔량 토큰 (소수점 — 부분 refill 추적).
    pub tokens: f64,
    /// 마지막 보충 시각 (ms).
    pub last_refill_ms: u64,
    /// 누적 throttle 카운트 (try_consume 이 false 를 돌려준 횟수).
    #[serde(default)]
    pub throttled_count: u64,
}

impl RateLimit {
    /// 현재 시각 기준 토큰 보충 (mutation). `now_ms` 가 과거면 no-op.
    fn refill(&mut self, now_ms: u64) {
        if now_ms <= self.last_refill_ms {
            return;
        }
        let elapsed = (now_ms - self.last_refill_ms) as f64;
        let rate = self.limit as f64 / self.per_ms as f64;
        self.tokens = (self.tokens + elapsed * rate).min(self.burst as f64);
        self.last_refill_ms = now_ms;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsumeOutcome {
    pub allowed: bool,
    /// 소비 직후 잔량 (allowed=false 면 차감 안 됨).
    pub tokens_left: f64,
}

pub struct RateLimitStore<'a> {
    mem: &'a mut dyn MemoryStorage,
    owner: String,
}

impl<'a> RateLimitStore<'a> {
    pub fn new(mem: &'a mut dyn MemoryStorage, owner: impl Into<String>) -> Self {
        Self {
            mem,
            owner: owner.into(),
        }
    }

    fn put(&mut self, rl: &RateLimit) -> Result<()> {
        let scope = Scope::Global;
        let value = MemoryValue::Json(serde_json::to_value(rl)?);
        self.mem.put(
            &self.owner,
            &scope,
            &rate_limit_key(&rl.id)?,
            &value,
            &PutOpts::default(),
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<RateLimit>> {
        let scope = Scope::Global;
        let entry = self.mem.get(&scope, &rate_limit_key(id)?)?;
        match entry {
            Some(e) => match e.value {
                MemoryValue::Json(v) => Ok(Some(serde_json::from_value(v)?)),
                _ => Err(AgentError::InvalidArgument(format!(
                    "rate_limit entry is not json: {id}"
                ))),
            },
            None => Ok(None),
        }
    }

    pub fn list(&self) -> Result<Vec<RateLimit>> {
        let scope = Scope::Global;
        let opts = ListOpts {
            prefix: Some(RATE_LIMIT_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        let entries = self.mem.list(&scope, &opts)?;
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if let MemoryValue::Json(v) = e.value {
                out.push(serde_json::from_value(v)?);
            }
        }
        Ok(out)
    }

    /// (agent, metric) pair 로 찾아본다 — `set` 의 upsert 매칭에 사용.
    pub fn find_by_agent_metric(&self, agent: &str, metric: &str) -> Result<Option<RateLimit>> {
        let all = self.list()?;
        Ok(all
            .into_iter()
            .find(|r| r.agent == agent && r.metric == metric))
    }

    /// (agent, metric) 키로 upsert. 이미 존재하면 limit/per/burst 만 갱신하고
    /// 버킷은 새 burst 로 reset.
    pub fn set(
        &mut self,
        agent: impl Into<String>,
        metric: impl Into<String>,
        limit: u32,
        per_ms: u64,
        burst: Option<u32>,
        now_ms: u64,
    ) -> Result<RateLimit> {
        let agent = agent.into();
        let metric = metric.into();
        if agent.is_empty() {
            return Err(AgentError::InvalidArgument(
                "rate_limit.agent must be non-empty".into(),
            ));
        }
        if limit == 0 {
            return Err(AgentError::InvalidArgument(
                "rate_limit.limit must be >= 1".into(),
            ));
        }
        if per_ms == 0 {
            return Err(AgentError::InvalidArgument(
                "rate_limit.per_ms must be >= 1".into(),
            ));
        }
        if metric.is_empty() {
            return Err(AgentError::InvalidArgument(
                "rate_limit.metric must be non-empty".into(),
            ));
        }
        let burst = burst.unwrap_or(limit);
        if burst == 0 {
            return Err(AgentError::InvalidArgument(
                "rate_limit.burst must be >= 1".into(),
            ));
        }

        if let Some(mut existing) = self.find_by_agent_metric(&agent, &metric)? {
            existing.limit = limit;
            existing.per_ms = per_ms;
            existing.burst = burst;
            existing.tokens = burst as f64;
            existing.last_refill_ms = now_ms;
            self.put(&existing)?;
            return Ok(existing);
        }

        let id = format!("rl-{now_ms}-{}", sanitize(&agent));
        let rl = RateLimit {
            id,
            agent,
            metric,
            limit,
            per_ms,
            burst,
            tokens: burst as f64,
            last_refill_ms: now_ms,
            throttled_count: 0,
        };
        self.put(&rl)?;
        Ok(rl)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        let scope = Scope::Global;
        self.mem
            .delete(&self.owner, &scope, &rate_limit_key(id)?, None)?;
        Ok(())
    }

    /// 토큰 소비 시도. 부족하면 `allowed=false`, 차감 안 함. 부수 효과로 `tokens` /
    /// `last_refill_ms` / `throttled_count` 가 영속에 반영된다.
    pub fn try_consume(
        &mut self,
        agent: &str,
        metric: &str,
        cost: u32,
        now_ms: u64,
    ) -> Result<ConsumeOutcome> {
        let Some(mut rl) = self.find_by_agent_metric(agent, metric)? else {
            // 등록 안 된 (agent, metric) 은 throttle 대상 아님 → 항상 허용.
            return Ok(ConsumeOutcome {
                allowed: true,
                tokens_left: f64::INFINITY,
            });
        };
        rl.refill(now_ms);
        if rl.tokens < cost as f64 {
            rl.throttled_count = rl.throttled_count.saturating_add(1);
            let left = rl.tokens;
            self.put(&rl)?;
            return Ok(ConsumeOutcome {
                allowed: false,
                tokens_left: left,
            });
        }
        rl.tokens -= cost as f64;
        let left = rl.tokens;
        self.put(&rl)?;
        Ok(ConsumeOutcome {
            allowed: true,
            tokens_left: left,
        })
    }

    /// 모든 rate_limit 의 현 상태를 시각 기준 refill 한 뒤 반환.
    pub fn status(&mut self, now_ms: u64) -> Result<Vec<RateLimit>> {
        let mut all = self.list()?;
        for rl in all.iter_mut() {
            rl.refill(now_ms);
            self.put(rl)?;
        }
        Ok(all)
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
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
    fn set_then_consume_until_empty() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        store
            .set("a1", "ipc_calls", 3, 60_000, None, 1_000_000)
            .unwrap();
        for i in 0..3 {
            let r = store.try_consume("a1", "ipc_calls", 1, 1_000_000).unwrap();
            assert!(r.allowed, "consume #{i} should be allowed");
        }
        let r = store.try_consume("a1", "ipc_calls", 1, 1_000_000).unwrap();
        assert!(!r.allowed, "4th consume should be throttled");
        assert_eq!(r.tokens_left, 0.0);
    }

    #[test]
    fn refill_over_window_restores_tokens() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        store.set("a", "m", 10, 1_000, None, 0).unwrap();
        for _ in 0..10 {
            assert!(store.try_consume("a", "m", 1, 0).unwrap().allowed);
        }
        assert!(!store.try_consume("a", "m", 1, 0).unwrap().allowed);
        let r = store.try_consume("a", "m", 5, 500).unwrap();
        assert!(r.allowed);
        let r = store.try_consume("a", "m", 1, 500).unwrap();
        assert!(!r.allowed);
    }

    #[test]
    fn unknown_agent_metric_pair_always_allowed() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        let r = store.try_consume("nobody", "m", 1, 1000).unwrap();
        assert!(r.allowed);
        assert!(r.tokens_left.is_infinite());
    }

    #[test]
    fn set_upsert_replaces_bucket() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        let first = store.set("a", "m", 5, 1_000, None, 0).unwrap();
        for _ in 0..5 {
            store.try_consume("a", "m", 1, 0).unwrap();
        }
        let updated = store.set("a", "m", 20, 60_000, Some(30), 100).unwrap();
        assert_eq!(updated.id, first.id);
        assert_eq!(updated.limit, 20);
        assert_eq!(updated.burst, 30);
        assert_eq!(updated.tokens, 30.0);
    }

    #[test]
    fn list_returns_all_registered() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        store.set("a", "m1", 1, 1_000, None, 0).unwrap();
        store.set("a", "m2", 1, 1_000, None, 1).unwrap();
        store.set("b", "m1", 1, 1_000, None, 2).unwrap();
        assert_eq!(store.list().unwrap().len(), 3);
    }

    #[test]
    fn invalid_limit_or_per_rejected() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        assert!(matches!(
            store.set("a", "m", 0, 1, None, 0).unwrap_err(),
            AgentError::InvalidArgument(_)
        ));
        assert!(matches!(
            store.set("a", "m", 1, 0, None, 0).unwrap_err(),
            AgentError::InvalidArgument(_)
        ));
        assert!(matches!(
            store.set("a", "", 1, 1, None, 0).unwrap_err(),
            AgentError::InvalidArgument(_)
        ));
        assert!(matches!(
            store.set("", "m", 1, 1, None, 0).unwrap_err(),
            AgentError::InvalidArgument(_)
        ));
    }

    #[test]
    fn remove_deletes_record() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        let rl = store.set("a", "m", 1, 1_000, None, 0).unwrap();
        store.remove(&rl.id).unwrap();
        assert!(store.get(&rl.id).unwrap().is_none());
    }

    #[test]
    fn throttled_count_increments_on_deny() {
        let (_td, mut mem) = fresh();
        let mut store = RateLimitStore::new(&mut mem, "_host");
        store.set("a", "m", 1, 60_000, None, 0).unwrap();
        store.try_consume("a", "m", 1, 0).unwrap();
        store.try_consume("a", "m", 1, 0).unwrap();
        store.try_consume("a", "m", 1, 0).unwrap();
        let rl = store.find_by_agent_metric("a", "m").unwrap().unwrap();
        assert_eq!(rl.throttled_count, 2);
    }
}
