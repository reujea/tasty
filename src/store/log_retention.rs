//! memory.db의 audit·telemetry event·telemetry anomaly 보존 정책.
//! 부팅 시 ALL을 순회한다. 실행 중에는 기록을 넣은 경로와 Tick::LogPrune이 `claim_prune`으로 주기를 가져간 뒤
//! `prune_all`을 기다리지 않는 writer 요청으로 넣는다.
//!
//! 개수와 보존 기간을 기준으로 삭제를 시도한다. 주기 사이에는 keep을 초과할 수 있고,
//! 스케줄 지연이나 저장소 오류가 있으면 정리가 더 늦어진다. 엄격한 메모리·디스크 상한은 아니다.
//! 결정 근거는 [저장소 시스템 (Storage System)](../../docs/design/systems/storage.md)를 참고한다.
//!
//! 파일 로그에는 이 정책을 적용하지 않는다. notify 로그(완료 줄과 큐 줄)는 tasty_utils::notify,
//! hook 실패는 crates/tasty-cli/src/hook_failure.rs, 호스트 로그는 crash_report,
//! 플러그인 로그는 tasty_host_plugin::process에서 관리한다.
//! 플러그인의 완료 줄은 TASTY_PARENT_HOME을 우선하며 호스트의 큐 줄과 나머지는 tasty_home()을 사용한다.

use std::sync::atomic::{AtomicU64, Ordering};

use tasty_memory::MemoryStorage;

/// 로그 한 종류의 보존 정책.
pub struct LogRetention {
    /// `{prefix}{ts:013}...` 형식의 키 접두사. 문자열 정렬이 시간순과 같아야 한다.
    pub prefix: &'static str,
    /// 남길 최대 행 수.
    pub keep: u64,
    /// 이보다 오래된 행은 개수 상한과 무관하게 삭제. `None` = 개수 상한만.
    pub ttl_ms: Option<u64>,
}

impl LogRetention {
    /// 삭제에 성공한 행 수를 합산한다. 실패한 삭제는 경고를 남기고 다음 처리를 계속한다.
    pub fn enforce(&self, mem: &mut dyn MemoryStorage, now_ms: u64) -> u64 {
        let mut removed = 0u64;
        if let Some(ttl) = self.ttl_ms {
            let cutoff = now_ms.saturating_sub(ttl);
            match mem.prune_prefix_older_than(self.prefix, cutoff) {
                Ok(n) => removed += n,
                Err(e) => tracing::warn!("log retention: ttl prune {} failed: {e}", self.prefix),
            }
        }
        match mem.prune_prefix_keep_recent(self.prefix, self.keep) {
            Ok(n) => removed += n,
            Err(e) => tracing::warn!("log retention: count prune {} failed: {e}", self.prefix),
        }
        removed
    }
}

/// audit와 anomaly의 공통 보존 기간: 50시간.
pub const LOG_TTL_MS: u64 = 50 * 60 * 60 * 1_000;

pub const AUDIT: LogRetention = LogRetention {
    prefix: crate::store::audit::AUDIT_KEY_PREFIX,
    keep: 50_000,
    ttl_ms: Some(LOG_TTL_MS),
};

/// 원본 이벤트를 최근 20,000개로 정리한다. 별도 시간 제한은 없으며,
/// 조회는 남아 있는 원본 이벤트로 집계한다.
pub const TELEMETRY_EVENT: LogRetention = LogRetention {
    prefix: tasty_telemetry::EVENT_KEY_PREFIX,
    keep: 20_000,
    ttl_ms: None,
};

pub const TELEMETRY_ANOMALY: LogRetention = LogRetention {
    prefix: tasty_telemetry::ANOMALY_KEY_PREFIX,
    keep: 5_000,
    ttl_ms: Some(LOG_TTL_MS),
};

/// 부팅·런타임 공통 목록. 새 관측 로그의 정책도 여기에 등록한다.
pub const ALL: [LogRetention; 3] = [AUDIT, TELEMETRY_EVENT, TELEMETRY_ANOMALY];

/// 매 요청마다 정리하지 않도록 1시간 간격으로 제한한다.
pub const PRUNE_INTERVAL_MS: u64 = 60 * 60 * 1_000;

/// 마지막 정리 시도 시각. 프로세스 전체가 공유한다.
static LAST_PRUNE_MS: AtomicU64 = AtomicU64::new(0);

/// 이번 주기의 정리를 가져온다. 간격이 지나지 않았거나 다른 호출자가 가져갔으면 false다.
pub fn claim_prune(now_ms: u64) -> bool {
    let last = LAST_PRUNE_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < PRUNE_INTERVAL_MS {
        return false;
    }
    LAST_PRUNE_MS
        .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

/// 모든 보존 정책을 집행한다. 같은 시각으로 다시 실행해도 결과가 같다.
pub fn prune_all(mem: &mut dyn MemoryStorage, now_ms: u64) {
    let mut removed = 0u64;
    for policy in &ALL {
        removed += policy.enforce(mem, now_ms);
    }
    if removed > 0 {
        tracing::debug!("log retention: pruned {removed} rows");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_memory::{MemoryValue, PutOpts, Scope, testing::InMemoryStorage};

    fn put_log(mem: &mut InMemoryStorage, prefix: &str, ts: u64, seq: u64) {
        mem.put(
            tasty_memory::HOST_OWNER,
            &Scope::Global,
            &format!("{prefix}{ts:013}.{seq:04}"),
            &MemoryValue::Text("x".into()),
            &PutOpts::default(),
        )
        .unwrap();
    }

    fn rows(mem: &InMemoryStorage, prefix: &str) -> usize {
        mem.list(
            &Scope::Global,
            &tasty_memory::ListOpts {
                prefix: Some(prefix.to_string()),
                ..Default::default()
            },
        )
        .unwrap()
        .len()
    }

    #[test]
    fn every_ipc_log_prefix_has_a_policy() {
        let covered: Vec<&str> = ALL.iter().map(|p| p.prefix).collect();
        for expected in [
            crate::store::audit::AUDIT_KEY_PREFIX,
            tasty_telemetry::EVENT_KEY_PREFIX,
            tasty_telemetry::ANOMALY_KEY_PREFIX,
        ] {
            assert!(
                covered.contains(&expected),
                "'{expected}'의 보존 정책이 목록에 없다"
            );
        }
    }

    #[test]
    fn count_cap_trims_only_its_own_prefix() {
        let mut mem = InMemoryStorage::new();
        for i in 0..10u64 {
            put_log(&mut mem, "tasty.audit.", 1_000 + i, 0);
            put_log(&mut mem, "tasty.telemetry.event.", 1_000 + i, 0);
        }
        let policy = LogRetention {
            prefix: "tasty.audit.",
            keep: 3,
            ttl_ms: None,
        };
        assert_eq!(policy.enforce(&mut mem, 10_000), 7);
        assert_eq!(rows(&mem, "tasty.audit."), 3);
        assert_eq!(rows(&mem, "tasty.telemetry.event."), 10);

        // 원본 이벤트로 조회를 집계하므로 최신 행을 남겨야 한다.
        let survivors: Vec<String> = mem
            .list(
                &Scope::Global,
                &tasty_memory::ListOpts {
                    prefix: Some("tasty.audit.".to_string()),
                    ..Default::default()
                },
            )
            .unwrap()
            .into_iter()
            .map(|e| e.key)
            .collect();
        for ts in [1_007u64, 1_008, 1_009] {
            assert!(
                survivors.contains(&format!("tasty.audit.{ts:013}.0000")),
                "최신 {ts} 가 지워졌다"
            );
        }
    }

    #[test]
    fn ttl_removes_rows_that_the_count_cap_would_keep() {
        let mut mem = InMemoryStorage::new();
        put_log(&mut mem, "tasty.audit.", 1_000, 0);
        put_log(&mut mem, "tasty.audit.", 9_000, 0);
        let policy = LogRetention {
            prefix: "tasty.audit.",
            keep: 1_000, // 개수로는 하나도 안 걸린다
            ttl_ms: Some(2_000),
        };
        assert_eq!(policy.enforce(&mut mem, 10_000), 1);
        assert_eq!(rows(&mem, "tasty.audit."), 1);
    }
}
