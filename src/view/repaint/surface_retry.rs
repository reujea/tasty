//! One bounded retry reservation per View. No timer/thread or retained error payload.
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct SurfaceRetry {
    failures: u32,
    not_before: Option<Instant>,
}

impl SurfaceRetry {
    pub(super) fn ready(&self, now: Instant) -> bool {
        self.not_before.is_none_or(|at| now >= at)
    }

    /// 실패한 뒤 아직 성공·취소하지 않았다. 만기 뒤에도 성공할 때까지 유지된다.
    pub(super) fn active(&self) -> bool {
        self.not_before.is_some()
    }

    pub(super) fn failed(&mut self, now: Instant) -> (Instant, bool) {
        self.failures = self.failures.saturating_add(1);
        let delay = Duration::from_millis((16u64 << (self.failures - 1).min(6)).min(1000));
        let at = now + delay;
        self.not_before = Some(at);
        // A persistent failure emits at 1, 2, 4, 8, ...; successes reset the series.
        (at, self.failures.is_power_of_two())
    }

    pub(super) fn recovered(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_is_not_immediate_and_needs_no_external_event() {
        let now = Instant::now();
        let mut retry = SurfaceRetry::default();
        let (due, log) = retry.failed(now);
        assert!(log);
        assert_eq!(due - now, Duration::from_millis(16));
        assert!(!retry.ready(now));
        assert!(!retry.ready(due - Duration::from_nanos(1)));
        assert!(retry.ready(due));
        assert!(retry.active());
    }

    #[test]
    fn repeated_failures_bound_rate_storage_and_logs() {
        let mut now = Instant::now();
        let mut retry = SurfaceRetry::default();
        let mut logged = 0;
        for n in 1..=10000 {
            let (due, log) = retry.failed(now);
            logged += u32::from(log);
            assert!(due - now >= Duration::from_millis(16));
            assert!(due - now <= Duration::from_secs(1));
            if n >= 7 {
                assert_eq!(due - now, Duration::from_secs(1));
            }
            now = due;
        }
        assert_eq!(logged, 14);
    }

    #[test]
    fn actual_success_resets_backoff_and_reservation() {
        let now = Instant::now();
        let mut retry = SurfaceRetry::default();
        retry.failed(now);
        retry.failed(now);
        retry.recovered();
        assert!(retry.ready(now));
        assert!(!retry.active());
        assert_eq!(retry.failed(now), (now + Duration::from_millis(16), true));
    }
}
