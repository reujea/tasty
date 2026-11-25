//! writer 누계 계측. 원자값을 Relaxed로 읽으므로 스냅샷 전체가 원자적이지는 않다.

use super::{measurement::DurationStats, pending::PendingStats};
use std::sync::Arc;
use std::time::Instant;

use std::sync::atomic::{AtomicU64, Ordering};

/// 묶음 크기 분포의 구간 상한(포함). 마지막 구간은 그 위 전부다.
pub const BATCH_SIZE_BUCKETS: [u64; 5] = [1, 4, 16, 64, 256];

#[derive(Debug, Default)]
pub(crate) struct WriterStats {
    admitted: AtomicU64,
    admitted_bytes: AtomicU64,
    batch_bytes: AtomicU64,
    batch_bytes_max: AtomicU64,
    pub(super) batch_wait: DurationStats,
    pub(super) batch_execution: DurationStats,
    pub(super) executor_execution: DurationStats,
    pub(super) pending: Arc<PendingStats>,
    rejected_saturated: AtomicU64,
    rejected_closed: AtomicU64,
    batches: AtomicU64,
    batch_requests: AtomicU64,
    batch_size_max: AtomicU64,
    batch_sizes: [AtomicU64; BATCH_SIZE_BUCKETS.len() + 1],
    superseded: AtomicU64,
    retries: AtomicU64,
    flushes: AtomicU64,
    peak_items: AtomicU64,
    peak_bytes: AtomicU64,
}

fn as_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

impl WriterStats {
    pub(crate) fn record_admitted(&self, held_items: usize, held_bytes: usize, bytes: usize) {
        self.admitted.fetch_add(1, Ordering::Relaxed);
        self.admitted_bytes
            .fetch_add(as_u64(bytes), Ordering::Relaxed);
        self.peak_items
            .fetch_max(as_u64(held_items), Ordering::Relaxed);
        self.peak_bytes
            .fetch_max(as_u64(held_bytes), Ordering::Relaxed);
    }

    pub(crate) fn record_rejected_saturated(&self) {
        self.rejected_saturated.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_rejected_closed(&self) {
        self.rejected_closed.fetch_add(1, Ordering::Relaxed);
    }

    /// 실행한 묶음 하나. 재시도는 같은 묶음이라 다시 세지 않는다.
    pub(crate) fn record_batch(&self, requests: usize, superseded: usize, bytes: usize) {
        let n = as_u64(requests);
        self.batches.fetch_add(1, Ordering::Relaxed);
        self.batch_bytes.fetch_add(as_u64(bytes), Ordering::Relaxed);
        self.batch_bytes_max
            .fetch_max(as_u64(bytes), Ordering::Relaxed);
        self.batch_requests.fetch_add(n, Ordering::Relaxed);
        self.batch_size_max.fetch_max(n, Ordering::Relaxed);
        let bucket = BATCH_SIZE_BUCKETS
            .iter()
            .position(|&upper| n <= upper)
            .unwrap_or(BATCH_SIZE_BUCKETS.len());
        self.batch_sizes[bucket].fetch_add(1, Ordering::Relaxed);
        self.superseded
            .fetch_add(as_u64(superseded), Ordering::Relaxed);
    }

    pub(crate) fn record_retry(&self) {
        self.retries.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_flush(&self) {
        self.flushes.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(
        &self,
        held_items: usize,
        held_bytes: usize,
        now: Instant,
    ) -> WriterStatsSnapshot {
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let pending = self.pending.snapshot(now);
        WriterStatsSnapshot {
            admitted_bytes: load(&self.admitted_bytes),
            batch_bytes: load(&self.batch_bytes),
            batch_bytes_max: load(&self.batch_bytes_max),
            batch_wait: self.batch_wait.snapshot(),
            batch_execution: self.batch_execution.snapshot(),
            executor_execution: self.executor_execution.snapshot(),
            completed_ok: pending.outcomes[0],
            completed_rejected: pending.outcomes[1],
            completed_failed: pending.outcomes[2],
            completed_unknown: pending.outcomes[3],
            completed_lost: pending.outcomes[4],
            pending_items: pending.items,
            pending_bytes: pending.bytes,
            queued_items: pending.queued_items,
            queued_bytes: pending.queued_bytes,
            oldest_pending_age_us: pending.oldest,
            admitted: load(&self.admitted),
            rejected_saturated: load(&self.rejected_saturated),
            rejected_closed: load(&self.rejected_closed),
            batches: load(&self.batches),
            batch_requests: load(&self.batch_requests),
            batch_size_max: load(&self.batch_size_max),
            batch_sizes: std::array::from_fn(|i| load(&self.batch_sizes[i])),
            superseded: load(&self.superseded),
            retries: load(&self.retries),
            flushes: load(&self.flushes),
            held_items: as_u64(held_items),
            held_bytes: as_u64(held_bytes),
            peak_items: load(&self.peak_items),
            peak_bytes: load(&self.peak_bytes),
        }
    }
}

/// writer 계측을 필드마다 따로 읽은 값. 필드 사이의 한 시점 일관성은 보장하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WriterStatsSnapshot {
    pub admitted_bytes: u64,
    /// Original input bytes before coalescing, not physical bytes written.
    pub batch_bytes: u64,
    pub batch_bytes_max: u64,
    pub batch_wait: super::DurationStatsSnapshot,
    /// Logical batch service, including busy retry backoff and settlement, not receipt consumption.
    pub batch_execution: super::DurationStatsSnapshot,
    /// Individual executor attempts, including caught panic, excluding retry backoff.
    pub executor_execution: super::DurationStatsSnapshot,
    pub completed_ok: u64,
    pub completed_rejected: u64,
    pub completed_failed: u64,
    pub completed_unknown: u64,
    pub completed_lost: u64,
    /// Unsettled accepted work; excludes ready unread results, unlike held_*.
    pub pending_items: u64,
    /// Accepted work not yet dispatched to run_batch; excludes in-flight, retry and unread results.
    pub queued_items: u64,
    pub queued_bytes: u64,
    pub pending_bytes: u64,
    /// None when no unsettled work. Generic requests include reads, so this is not domain dirty age.
    pub oldest_pending_age_us: Option<u64>,
    /// 접수한 요청 수(flush 제외).
    pub admitted: u64,
    /// 예산이 차서 거절한 수.
    pub rejected_saturated: u64,
    /// 접수를 멈춘 뒤 거절한 수.
    pub rejected_closed: u64,
    /// 실행한 묶음 수. barrier 요청 하나도 묶음 하나다.
    pub batches: u64,
    /// 묶음에 들어간 요청 수 합(대체된 요청 포함).
    pub batch_requests: u64,
    pub batch_size_max: u64,
    /// 묶음 크기 분포. i번째는 `BATCH_SIZE_BUCKETS[i-1]` 초과 `BATCH_SIZE_BUCKETS[i]` 이하, 마지막은 그 위.
    pub batch_sizes: [u64; BATCH_SIZE_BUCKETS.len() + 1],
    /// 같은 snapshot 키의 뒤 요청에 대체된 요청 수.
    pub superseded: u64,
    /// busy 재실행 횟수.
    pub retries: u64,
    pub flushes: u64,
    /// 지금 잡혀 있는 항목 수와 바이트.
    pub held_items: u64,
    pub held_bytes: u64,
    /// 접수 직후 기준 최댓값.
    pub peak_items: u64,
    pub peak_bytes: u64,
}
