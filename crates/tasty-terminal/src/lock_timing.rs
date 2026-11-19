//! TerminalState 잠금의 대기·보유 시간. 출력 수신과 화면 읽기가 같은 잠금을 두고 얼마나 기다리는지 본다.
//! 호스트의 perf 계측(`tasty::gfx::perf` info)이 켜졌을 때만 시각을 잰다. 꺼져 있으면 호출마다
//! 켜짐 확인 한 번만 한다. 값은 프로세스 전체 누계이며 [`take`]가 읽고 비운다.
//!
//! 곳마다 횟수·합·최댓값을 작은 Mutex 하나에 묶어, 한 기록과 한 [`take`]가 서로를 나누지 않게 한다.
//! 그래서 읽은 횟수·합·최댓값은 늘 같은 표본 집합의 값이다. 이 Mutex는 측정 대상 잠금을 놓은 뒤에만
//! 잡으므로 기록 비용이 TerminalState 보유 시간에 들어가지 않는다.

use std::sync::Mutex;
use std::time::Instant;

/// 잠금을 잡는 곳.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockSite {
    /// PTY reader 스레드가 출력을 파서에 넣는 구간.
    Ingest,
    /// 렌더 등 소비자가 화면 한 컷을 읽는 구간(`Terminal::with_view`).
    View,
}

/// 한 곳의 누계.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockSiteStats {
    pub count: u64,
    pub wait_ns_sum: u64,
    pub wait_ns_max: u64,
    pub hold_ns_sum: u64,
    pub hold_ns_max: u64,
}

impl LockSiteStats {
    const ZERO: Self = Self {
        count: 0,
        wait_ns_sum: 0,
        wait_ns_max: 0,
        hold_ns_sum: 0,
        hold_ns_max: 0,
    };

    fn add(&mut self, wait_ns: u64, hold_ns: u64) {
        self.count = self.count.saturating_add(1);
        self.wait_ns_sum = self.wait_ns_sum.saturating_add(wait_ns);
        self.wait_ns_max = self.wait_ns_max.max(wait_ns);
        self.hold_ns_sum = self.hold_ns_sum.saturating_add(hold_ns);
        self.hold_ns_max = self.hold_ns_max.max(hold_ns);
    }
}

struct SiteCounters(Mutex<LockSiteStats>);

impl SiteCounters {
    const fn new() -> Self {
        Self(Mutex::new(LockSiteStats::ZERO))
    }

    fn with<R>(&self, f: impl FnOnce(&mut LockSiteStats) -> R) -> R {
        // 기록 도중 패닉한 스레드가 남긴 값도 계측값일 뿐이라 그대로 이어 쓴다.
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    fn add(&self, wait_ns: u64, hold_ns: u64) {
        self.with(|stats| stats.add(wait_ns, hold_ns));
    }

    /// 읽으면서 비운다. 한 번에 바꾸므로 동시에 기록 중인 한 건은 이번 창이나 다음 창 한쪽에만 들어간다.
    fn take(&self) -> LockSiteStats {
        self.with(std::mem::take)
    }
}

static INGEST: SiteCounters = SiteCounters::new();
static VIEW: SiteCounters = SiteCounters::new();

fn counters(site: LockSite) -> &'static SiteCounters {
    match site {
        LockSite::Ingest => &INGEST,
        LockSite::View => &VIEW,
    }
}

/// 잠금 요청 시각. 계측이 꺼져 있으면 `None`이라 이후 단계가 시각을 읽지 않는다.
pub(crate) fn requested() -> Option<Instant> {
    tracing::enabled!(target: "tasty::gfx::perf", tracing::Level::INFO).then(Instant::now)
}

/// 잠금을 잡은 시각. 요청 시각이 없으면 재지 않는다.
pub(crate) fn acquired(requested: Option<Instant>) -> Option<Instant> {
    requested.map(|_| Instant::now())
}

/// 잠금을 놓은 뒤 부른다.
pub(crate) fn released(site: LockSite, requested: Option<Instant>, acquired: Option<Instant>) {
    let (Some(requested), Some(acquired)) = (requested, acquired) else {
        return;
    };
    let wait = acquired.saturating_duration_since(requested);
    let hold = acquired.elapsed();
    counters(site).add(nanos(wait), nanos(hold));
}

fn nanos(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

/// 두 곳의 누계를 읽고 비운다.
pub fn take() -> [(LockSite, LockSiteStats); 2] {
    [
        (LockSite::Ingest, INGEST.take()),
        (LockSite::View, VIEW.take()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate_and_reset() {
        let c = SiteCounters::new();
        c.add(10, 100);
        c.add(30, 50);
        assert_eq!(
            c.take(),
            LockSiteStats {
                count: 2,
                wait_ns_sum: 40,
                wait_ns_max: 30,
                hold_ns_sum: 150,
                hold_ns_max: 100,
            }
        );
        assert_eq!(c.take(), LockSiteStats::default());
    }

    /// 기록과 비우기가 겹쳐도 한 창의 횟수·합·최댓값은 같은 표본 집합에서 나온다.
    #[test]
    fn take_reads_one_consistent_window_while_recording() {
        const WRITERS: u64 = 4;
        const RECORDS: u64 = 20_000;
        let c = std::sync::Arc::new(SiteCounters::new());
        let writers: Vec<_> = (0..WRITERS)
            .map(|_| {
                let c = std::sync::Arc::clone(&c);
                std::thread::spawn(move || {
                    for _ in 0..RECORDS {
                        c.add(1, 2);
                    }
                })
            })
            .collect();
        let mut seen = 0;
        let check = |w: LockSiteStats| {
            assert_eq!(w.wait_ns_sum, w.count, "{w:?}");
            assert_eq!(w.hold_ns_sum, 2 * w.count, "{w:?}");
            let expected_max = if w.count == 0 { (0, 0) } else { (1, 2) };
            assert_eq!((w.wait_ns_max, w.hold_ns_max), expected_max, "{w:?}");
            w.count
        };
        while writers.iter().any(|w| !w.is_finished()) {
            seen += check(c.take());
        }
        for w in writers {
            w.join().expect("writer");
        }
        seen += check(c.take());
        assert_eq!(seen, WRITERS * RECORDS);
    }

    #[test]
    fn missing_request_time_records_nothing() {
        released(LockSite::View, None, None);
        released(LockSite::View, None, Some(Instant::now()));
    }
}
