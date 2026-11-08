//! 별도 스레드에서 winit 콜백의 무응답 시간과 마지막 렌더 단계를 기록한다. 복구는 수행하지 않는다.
//! 오류 로그와 stall당 첫 hang 보고서를 남겨 호스트 재시작으로 공유 로그가 바뀌어도 기록을 보존한다.
//! native 모달처럼 사용자 응답을 기다리는 정상 구간은 without_stall_watch로 제외한다.
//! 단계 표시는 멈춘 위치를 좁히는 자료이며 GPU 드라이버 등 근본 원인을 확정하지는 않는다.

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, Instant};

/// 콜백이 반환하지 않았다고 처음 보고할 시간. 정상 블로킹 구간은 별도로 제외해야 한다.
const REPORT_AFTER: Duration = Duration::from_secs(5);

/// 같은 stall 이 계속될 때 재보고 간격 (로그 폭주 방지).
const REPEAT_EVERY: Duration = Duration::from_secs(30);

/// 워치독 스레드 폴링 주기.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// 콜백 시각을 나노초로 기록할 기준점.
static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);

/// 콜백 진입/이탈 시퀀스. **홀수 = 콜백 안, 짝수 = 바깥.**
static SEQ: AtomicU64 = AtomicU64::new(0);
/// 현재 콜백 진입 시각 (`ORIGIN` 기준 나노초).
static ENTERED_NS: AtomicU64 = AtomicU64::new(0);
/// 현재 콜백 종류 ([`Site`] 의 discriminant).
static SITE: AtomicU8 = AtomicU8::new(0);
/// 렌더 세부 단계 ([`Phase`] 의 discriminant).
static PHASE: AtomicU8 = AtomicU8::new(0);
/// 의도적 블로킹 구간의 중첩 깊이 ([`without_stall_watch`]). 0 보다 크면 보고하지 않는다.
static PAUSED: AtomicU32 = AtomicU32::new(0);
static REVISION: AtomicU64 = AtomicU64::new(0);
static DEPTH: AtomicU32 = AtomicU32::new(0);
static PROGRESS_NS: AtomicU64 = AtomicU64::new(0);
// Saved ancestor clocks must also honor the most recently completed intentional wait.
static PAUSE_RESET_NS: AtomicU64 = AtomicU64::new(0);
static LAST_PHASE: AtomicU8 = AtomicU8::new(0);
/// 마지막 `about_to_wait` 이후 처음 들어온 콜백의 시각 + 1. 0 이면 iteration 이 닫혀 있다.
/// 짧은 콜백이 끝없이 이어져 `about_to_wait` 에 닿지 못하는 기아는 콜백 하나의 시간으로는 보이지 않는다.
static ITERATION_OPEN_NS: AtomicU64 = AtomicU64::new(0);
static LAST_PHASE_NS: AtomicU64 = AtomicU64::new(0);

/// Start of a continuous nonempty main-loop queue-obligation period.
#[derive(Clone, Copy, Debug)]
pub struct PendingWork {
    pub count: u64,
    pub since: Instant,
    pub generation: u64,
}
static PENDING_PROBE: OnceLock<fn() -> Option<PendingWork>> = OnceLock::new();
/// Install a cached, bounded, I/O-free probe. No periodic event-loop wake is added.
pub fn install_pending_probe(probe: fn() -> Option<PendingWork>) {
    PENDING_PROBE.get_or_init(|| probe);
}
fn now_ns() -> u64 {
    ORIGIN.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

/// 관측 대상 winit 콜백.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Site {
    Resumed = 1,
    /// `RedrawRequested` 를 제외한 `window_event`.
    WindowEvent = 2,
    /// `WindowEvent::RedrawRequested` — GPU 렌더가 도는 경로.
    Redraw = 3,
    UserEvent = 4,
    AboutToWait = 5,
    DeviceEvent = 6,
    NewEvents = 7,
}

fn site_label(raw: u8) -> &'static str {
    match raw {
        1 => "resumed",
        2 => "window_event",
        3 => "redraw",
        4 => "user_event",
        5 => "about_to_wait",
        6 => "device_event",
        7 => "new_events",
        _ => "unknown",
    }
}

/// 렌더 안의 세부 단계. 어느 GPU 호출이 안 돌아왔는지 구분하는 것이 목적이다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Phase {
    /// GPU 호출 구간 밖 (또는 phase 를 계측하지 않는 렌더 경로).
    None = 0,
    /// `surface.get_current_texture()`.
    Acquire = 1,
    /// 렌더 패스 기록 + `queue.submit(...)`.
    Submit = 2,
    /// `SurfaceTexture::present()`.
    Present = 3,
    /// Surface configuration; reporting never cancels this native operation.
    Configure = 4,
}

fn phase_label(raw: u8) -> &'static str {
    match raw {
        0 => "none",
        1 => "acquire",
        2 => "submit",
        3 => "present",
        4 => "configure",
        _ => "unknown",
    }
}

/// 콜백 구간을 표시하는 RAII 가드. 콜백 최상단에서 만들고, 조기 return 을 포함한
/// 모든 이탈 경로를 `Drop` 이 덮는다.
pub struct Guard {
    previous: (u8, u64, u8),
}
impl Guard {
    pub fn enter(site: Site) -> Self {
        REVISION.fetch_add(1, Ordering::AcqRel);
        let previous = (
            SITE.load(Ordering::Relaxed),
            ENTERED_NS.load(Ordering::Relaxed),
            PHASE.load(Ordering::Relaxed),
        );
        let nested = DEPTH.fetch_add(1, Ordering::Relaxed) > 0;
        if !nested && site != Site::AboutToWait && ITERATION_OPEN_NS.load(Ordering::Relaxed) == 0 {
            ITERATION_OPEN_NS.store(now_ns().saturating_add(1), Ordering::Relaxed);
        }
        SITE.store(site as u8, Ordering::Relaxed);
        PHASE.store(Phase::None as u8, Ordering::Relaxed);
        ENTERED_NS.store(now_ns(), Ordering::Relaxed);
        PROGRESS_NS.store(now_ns(), Ordering::Relaxed);
        SEQ.fetch_add(if nested { 2 } else { 1 }, Ordering::Release);
        REVISION.fetch_add(1, Ordering::Release);
        Self { previous }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        REVISION.fetch_add(1, Ordering::AcqRel);
        let nested = DEPTH.fetch_sub(1, Ordering::Relaxed) > 1;
        if nested {
            SITE.store(self.previous.0, Ordering::Relaxed);
            ENTERED_NS.store(
                self.previous.1.max(PAUSE_RESET_NS.load(Ordering::Relaxed)),
                Ordering::Relaxed,
            );
            PHASE.store(self.previous.2, Ordering::Relaxed);
        } else {
            PROGRESS_NS.store(now_ns(), Ordering::Relaxed);
            PHASE.store(Phase::None as u8, Ordering::Relaxed);
            if SITE.load(Ordering::Relaxed) == Site::AboutToWait as u8 {
                ITERATION_OPEN_NS.store(0, Ordering::Relaxed);
            }
        }
        SEQ.fetch_add(if nested { 2 } else { 1 }, Ordering::Release);
        REVISION.fetch_add(1, Ordering::Release);
    }
}

/// 렌더 세부 단계 표시. 가드 안에서만 의미가 있고, 가드 `Drop` 이 자동으로 되돌린다.
#[inline]
pub fn set_phase(phase: Phase) {
    REVISION.fetch_add(1, Ordering::AcqRel);
    PHASE.store(phase as u8, Ordering::Relaxed);
    if phase != Phase::None {
        LAST_PHASE.store(phase as u8, Ordering::Relaxed);
        LAST_PHASE_NS.store(now_ns(), Ordering::Release);
    }
    REVISION.fetch_add(1, Ordering::Release);
}

/// 사용자 응답을 기다리는 native 모달 같은 정상 블로킹 구간의 보고를 중지한다.
/// 구간 종료 뒤 콜백 진입 시각을 다시 잡아 대기 시간이 stall에 포함되지 않게 한다.
/// 일반 파일·GPU 작업의 예상하지 못한 지연을 숨기는 용도로 사용하지 않는다.
pub fn without_stall_watch<T>(f: impl FnOnce() -> T) -> T {
    let _pause = PauseGuard::enter();
    f()
}

/// [`without_stall_watch`] 의 RAII 본체. `f` 가 panic 해도 `Drop` 이 구간을 닫는다.
struct PauseGuard;

impl PauseGuard {
    fn enter() -> Self {
        REVISION.fetch_add(1, Ordering::AcqRel);
        PAUSED.fetch_add(1, Ordering::Release);
        REVISION.fetch_add(1, Ordering::Release);
        PauseGuard
    }
}

impl Drop for PauseGuard {
    fn drop(&mut self) {
        REVISION.fetch_add(1, Ordering::AcqRel);
        // 블로킹에 쓴 시간은 stall 이 아니다 — 남은 콜백을 지금부터 다시 잰다.
        let now = now_ns();
        ENTERED_NS.store(now, Ordering::Relaxed);
        PROGRESS_NS.store(now, Ordering::Relaxed);
        PAUSE_RESET_NS.store(now, Ordering::Relaxed);
        // 대기한 시간은 iteration 기아도 아니다. 열린 iteration 은 지금부터 다시 잰다.
        if ITERATION_OPEN_NS.load(Ordering::Relaxed) != 0 {
            ITERATION_OPEN_NS.store(now.saturating_add(1), Ordering::Relaxed);
        }
        // seq 를 2 만큼 올린다: 홀짝(= 콜백 안)은 유지하면서 ① 진행 중인 표본을 무효화하고
        // ② 재보고 추적이 이 이후를 별개 구간으로 보게 한다.
        SEQ.fetch_add(2, Ordering::Release);
        // 시각을 갱신한 뒤에 푼다 — 반대 순서면 낡은 진입 시각이 잠깐 노출된다.
        PAUSED.fetch_sub(1, Ordering::Release);
        REVISION.fetch_add(1, Ordering::Release);
    }
}

/// 관측 스냅샷 — 보고 판정을 순수 함수로 떼어내기 위한 값 묶음.
#[derive(Clone, Copy, Debug, Default)]
struct Snapshot {
    pending: u64,
    work_generation: u64,
    site: u8,
    phase: u8,
    last_phase: u8,
    last_phase_age: Duration,
    progress_age: Duration,
    pending_age: Duration,
    seq: u64,
    stuck: Duration,
    /// 표본 시점에 의도적 블로킹 구간([`without_stall_watch`]) 안이었나.
    paused: bool,
    /// 열린 iteration 의 식별값(열린 시각). 0 이면 `about_to_wait` 뒤라 닫혀 있다.
    iteration_key: u64,
    /// iteration 이 열린 뒤 `about_to_wait` 에 닿지 못한 시간.
    iteration_age: Duration,
}

/// 직전 보고 상태.
#[derive(Clone, Copy, Debug, Default)]
struct Reported {
    seq: u64,
    work_generation: u64,
    at: Duration,
    /// iteration 기아로 보고했으면 그 iteration 의 식별값, 콜백 stall 이면 0.
    starved_iteration: u64,
}

/// 보고 판정 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Verdict {
    /// 이 stall 의 첫 보고인가.
    first: bool,
    /// 콜백 하나가 아니라 iteration 이 `about_to_wait` 에 닿지 못해 보고한다.
    starved: bool,
}

/// 이번 폴링에서 보고할지 판정한다. `None` 이면 보고하지 않는다.
fn should_report(snap: Snapshot, reported: Option<Reported>) -> Option<Verdict> {
    if snap.paused {
        return None; // 의도적 블로킹(native 모달 등) — 행이 아니다
    }
    let callback_idle = snap.seq.is_multiple_of(2) && snap.pending == 0;
    if callback_idle || snap.stuck < REPORT_AFTER {
        // 콜백은 짧게 끝나지만 iteration 이 about_to_wait 에 닿지 못하는 기아.
        // 콜백 사이에 표본을 잡으면 idle 처럼 보이므로 idle 판정보다 먼저 본다.
        if snap.iteration_key == 0 || snap.iteration_age < REPORT_AFTER {
            return None;
        }
        return match reported {
            Some(prev) if prev.starved_iteration == snap.iteration_key => {
                (snap.iteration_age.saturating_sub(prev.at) >= REPEAT_EVERY).then_some(Verdict {
                    first: false,
                    starved: true,
                })
            }
            _ => Some(Verdict {
                first: true,
                starved: true,
            }),
        };
    }
    match reported {
        Some(prev)
            if prev.starved_iteration == 0
                && prev.seq == snap.seq
                && prev.work_generation == snap.work_generation =>
        {
            // 같은 stall — 재보고 간격을 채웠을 때만.
            (snap.stuck.saturating_sub(prev.at) >= REPEAT_EVERY).then_some(Verdict {
                first: false,
                starved: false,
            })
        }
        _ => Some(Verdict {
            first: true,
            starved: false,
        }),
    }
}

/// 워치독 스레드를 띄운다. 이벤트 루프 진입 전에 한 번 호출한다.
pub fn spawn() {
    LazyLock::force(&ORIGIN);
    if let Err(e) = std::thread::Builder::new()
        .name("tasty-stall-watchdog".to_string())
        .spawn(watch_loop)
    {
        // 워치독이 없어도 앱 동작은 동일하다 — 진단이 빠질 뿐이라 warn 으로 남긴다.
        tracing::warn!("stall watchdog thread spawn failed: {e}");
    }
}

/// 이벤트 루프 상태를 한 번 읽는다. 읽는 도중 콜백이 교체되면 `None`(그 표본은 버린다).
fn sample() -> Option<Snapshot> {
    let revision = REVISION.load(Ordering::Acquire);
    if !revision.is_multiple_of(2) {
        return None;
    }
    let seq = SEQ.load(Ordering::Acquire);
    let paused = PAUSED.load(Ordering::Acquire) > 0;
    let pending = PENDING_PROBE.get().and_then(|probe| probe());
    let now = Instant::now();
    let progress = *ORIGIN + Duration::from_nanos(PROGRESS_NS.load(Ordering::Relaxed));
    let entered = *ORIGIN + Duration::from_nanos(ENTERED_NS.load(Ordering::Relaxed));
    let start = if seq.is_multiple_of(2) {
        pending.map_or(progress, |p| p.since.max(progress))
    } else {
        entered
    };
    let iteration_open = ITERATION_OPEN_NS.load(Ordering::Relaxed);
    let snap = Snapshot {
        seq,
        paused,
        iteration_key: iteration_open,
        iteration_age: if iteration_open == 0 {
            Duration::ZERO
        } else {
            now.saturating_duration_since(*ORIGIN + Duration::from_nanos(iteration_open - 1))
        },
        stuck: now.checked_duration_since(start)?,
        pending: pending.map_or(0, |p| p.count),
        work_generation: if seq.is_multiple_of(2) {
            pending.map_or(0, |p| p.generation)
        } else {
            0
        },
        site: SITE.load(Ordering::Relaxed),
        phase: if seq.is_multiple_of(2) {
            0
        } else {
            PHASE.load(Ordering::Relaxed)
        },
        last_phase: LAST_PHASE.load(Ordering::Relaxed),
        last_phase_age: now.saturating_duration_since(
            *ORIGIN + Duration::from_nanos(LAST_PHASE_NS.load(Ordering::Acquire)),
        ),
        progress_age: now.saturating_duration_since(progress),
        pending_age: pending.map_or(Duration::ZERO, |p| now.saturating_duration_since(p.since)),
    };
    (SEQ.load(Ordering::Acquire) == seq && REVISION.load(Ordering::Acquire) == revision)
        .then_some(snap)
}

/// stall 1 건 보고. `first` 면 파일 리포트까지 남긴다(재보고는 로그만 — 파일 폭주 방지).
fn report(snap: Snapshot, verdict: Verdict) {
    let first = verdict.first;
    let inside_callback = !snap.seq.is_multiple_of(2);
    let site = if verdict.starved {
        "starved_iteration"
    } else if inside_callback {
        site_label(snap.site)
    } else {
        "outside_callback"
    };
    let phase = phase_label(snap.phase);
    let stuck_ms = if verdict.starved {
        snap.iteration_age.as_millis() as u64
    } else {
        snap.stuck.as_millis() as u64
    };
    tracing::error!(target: "tasty::stall", site, phase, stuck_ms, first,
        inside_callback, pending = snap.pending,
        progress_age_ms = snap.progress_age.as_millis() as u64,
        pending_age_ms = snap.pending_age.as_millis() as u64,
        last_callback = site_label(snap.site), last_phase = phase_label(snap.last_phase),
        last_phase_age_ms = snap.last_phase_age.as_millis() as u64,
        "event loop stalled — no observed main-loop progress; recorded regions do not establish the cause");
    if first {
        write_report_file(site, phase, stuck_ms, snap);
    }
}

/// stall 리포트 파일 1 개를 남기고 결과를 로그로 확인한다.
fn write_report_file(site: &str, phase: &str, stuck_ms: u64, snap: Snapshot) {
    // 리포트를 못 써도 호출자의 error 로그는 이미 남았다 — 진단이 얕아질 뿐이다.
    let written = crate::crash_report::write_hang_report(site, phase, stuck_ms);
    if let Some(path) = &written {
        use std::io::Write;
        let result = std::fs::OpenOptions::new().append(true).open(path).and_then(|mut file| {
            writeln!(file, "Inside callback: {}\nPending main-loop work: {}\nLast progress age: {} ms\nPending period age: {} ms\nLast callback: {}\nLast recorded render phase: {} ({} ms ago)",
                !snap.seq.is_multiple_of(2), snap.pending, snap.progress_age.as_millis(), snap.pending_age.as_millis(), site_label(snap.site), phase_label(snap.last_phase), snap.last_phase_age.as_millis())
        });
        if let Err(error) = result {
            tracing::warn!(%error, "hang progress context append failed");
        }
    }
    tracing::error!(
        target: "tasty::stall",
        path = ?written,
        "hang report written (path = None means the write failed)"
    );
}

fn watch_loop() {
    let mut reported: Option<Reported> = None;
    loop {
        std::thread::sleep(POLL_INTERVAL);

        let Some(snap) = sample() else { continue };
        let Some(verdict) = should_report(snap, reported) else {
            continue;
        };
        reported = Some(Reported {
            seq: snap.seq,
            work_generation: snap.work_generation,
            at: if verdict.starved {
                snap.iteration_age
            } else {
                snap.stuck
            },
            starved_iteration: if verdict.starved {
                snap.iteration_key
            } else {
                0
            },
        });
        report(snap, verdict);
    }
}

// =============================================================================
// Debug-only: 결함 주입 (워치독 자체를 검증하기 위한 인위적 블로킹)
// =============================================================================

#[cfg(debug_assertions)]
static DEBUG_STALL_MS: AtomicU64 = AtomicU64::new(0);

/// debug에서 다음 present 직전에 한 번 블로킹하도록 예약한다.
/// 실제 GPU 드라이버 결함 대신 워치독의 감지·보고 경로를 시험한다.
#[cfg(debug_assertions)]
pub fn arm_debug_stall(ms: u64) {
    DEBUG_STALL_MS.store(ms, Ordering::Relaxed);
}

/// 예약된 결함 주입이 있으면 소비해 그 시간만큼 블로킹한다. release 는 no-op.
#[cfg(debug_assertions)]
pub fn take_debug_stall() {
    let ms = DEBUG_STALL_MS.swap(0, Ordering::Relaxed);
    if ms > 0 {
        tracing::warn!(target: "tasty::stall", ms, "debug stall injected before present");
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// release 빌드에는 결함 주입이 없다.
#[cfg(not(debug_assertions))]
#[inline(always)]
pub fn take_debug_stall() {}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// `SEQ` / `PAUSED` 는 프로세스 전역이라, 그것을 실제로 건드리는 테스트끼리는
    /// 직렬화해야 한다(테스트는 기본적으로 병렬 실행된다).
    static GLOBALS: Mutex<()> = Mutex::new(());

    /// 콜백 stall 판정만 보는 시험에서 첫 보고 여부만 꺼낸다.
    fn first_of(snap: Snapshot, reported: Option<Reported>) -> Option<bool> {
        should_report(snap, reported).map(|v| {
            assert!(!v.starved, "콜백 stall 시험이 iteration 기아로 판정됐다");
            v.first
        })
    }

    fn starving(seq: u64, iteration_secs: u64) -> Snapshot {
        Snapshot {
            iteration_key: 77,
            iteration_age: Duration::from_secs(iteration_secs),
            stuck: Duration::from_millis(3),
            ..snap(seq, 0)
        }
    }

    #[test]
    fn short_callbacks_that_never_reach_about_to_wait_report_starvation() {
        // 콜백 사이(짝수 seq, 대기 작업 없음)에 잡은 표본도 기아로 본다.
        let v = should_report(starving(8, 6), None).expect("starvation must report");
        assert!(v.first && v.starved);
        assert_eq!(
            should_report(starving(9, 4), None),
            None,
            "below the threshold"
        );
        let closed = Snapshot {
            iteration_key: 0,
            ..starving(8, 60)
        };
        assert_eq!(
            should_report(closed, None),
            None,
            "a closed iteration is idle"
        );
        let paused = Snapshot {
            paused: true,
            ..starving(8, 60)
        };
        assert_eq!(should_report(paused, None), None);
    }

    #[test]
    fn the_same_starved_iteration_reports_again_only_after_the_repeat_interval() {
        let prev = Reported {
            seq: 1,
            work_generation: 0,
            at: Duration::from_secs(6),
            starved_iteration: 77,
        };
        // seq 가 계속 바뀌어도 같은 iteration 이면 같은 stall 이다.
        assert_eq!(should_report(starving(500, 20), Some(prev)), None);
        let again = should_report(starving(900, 36), Some(prev)).expect("repeat");
        assert!(!again.first && again.starved);
        let next = Snapshot {
            iteration_key: 78,
            ..starving(901, 6)
        };
        assert!(should_report(next, Some(prev)).is_some_and(|v| v.first));
    }

    fn snap(seq: u64, secs: u64) -> Snapshot {
        Snapshot {
            pending: 0,
            seq,
            stuck: Duration::from_secs(secs),
            paused: false,
            ..Default::default()
        }
    }

    fn paused_snap(seq: u64, secs: u64) -> Snapshot {
        Snapshot {
            paused: true,
            ..snap(seq, secs)
        }
    }

    #[test]
    fn even_seq_means_outside_callback_never_reports() {
        assert_eq!(first_of(snap(4, 600), None), None);
    }

    #[test]
    fn outside_callback_pending_work_is_not_indistinguishable_from_idle() {
        let idle = snap(4, 6);
        assert_eq!(first_of(idle, None), None);
        let pending = Snapshot { pending: 1, ..idle };
        assert_eq!(pending.pending, 1);
        assert_eq!(first_of(pending, None), Some(true));
    }

    #[test]
    fn short_callback_does_not_report() {
        assert_eq!(first_of(snap(3, 1), None), None);
    }

    #[test]
    fn first_long_stall_reports_as_first() {
        assert_eq!(first_of(snap(3, 5), None), Some(true));
    }

    #[test]
    fn same_stall_waits_for_repeat_interval() {
        let prev = Reported {
            seq: 3,
            work_generation: 0,
            at: Duration::from_secs(5),
            starved_iteration: 0,
        };
        // 5s → 20s: 재보고 간격(30s) 미달.
        assert_eq!(first_of(snap(3, 20), Some(prev)), None);
        // 5s → 35s: 간격 충족, 첫 보고는 아니다.
        assert_eq!(first_of(snap(3, 35), Some(prev)), Some(false));
    }

    #[test]
    fn new_stall_after_previous_report_is_first_again() {
        let prev = Reported {
            seq: 3,
            work_generation: 0,
            at: Duration::from_secs(40),
            starved_iteration: 0,
        };
        assert_eq!(first_of(snap(7, 6), Some(prev)), Some(true));
    }

    #[test]
    fn intentional_block_is_not_reported() {
        // 콜백 안(홀수) + 임계 초과지만 의도적 블로킹 구간이면 보고하지 않는다.
        assert_eq!(first_of(paused_snap(3, 600), None), None);
    }

    #[test]
    fn pause_keeps_parity_and_invalidates_the_sample() {
        let _lock = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let _g = Guard::enter(Site::UserEvent);
        let inside = SEQ.load(Ordering::Acquire);
        assert!(!inside.is_multiple_of(2), "가드 안 = 홀수");

        without_stall_watch(|| {
            assert!(PAUSED.load(Ordering::Acquire) > 0, "구간 안 = paused");
        });

        assert_eq!(PAUSED.load(Ordering::Acquire), 0, "구간을 벗어나면 풀린다");
        let after = SEQ.load(Ordering::Acquire);
        assert_eq!(
            after,
            inside + 2,
            "seq 는 2 만큼 올라 홀짝(콜백 안)을 유지한다"
        );
        assert!(
            sample().is_none_or(|s| s.stuck < REPORT_AFTER),
            "구간이 끝나면 진입 시각이 다시 잡힌다"
        );
    }

    #[test]
    fn guard_toggles_seq_parity() {
        let _lock = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let before = SEQ.load(Ordering::Acquire);
        {
            let _g = Guard::enter(Site::Redraw);
            assert_eq!(
                SEQ.load(Ordering::Acquire),
                before + 1,
                "가드 안 = 홀수 전이"
            );
            set_phase(Phase::Present);
            assert_eq!(PHASE.load(Ordering::Relaxed), Phase::Present as u8);
        }
        assert_eq!(
            SEQ.load(Ordering::Acquire),
            before + 2,
            "가드 밖 = 짝수 전이"
        );
        assert_eq!(PHASE.load(Ordering::Relaxed), Phase::None as u8);
    }

    #[test]
    fn outside_work_period_and_progress_have_independent_report_identity() {
        let s = Snapshot {
            pending: 2,
            work_generation: 7,
            ..snap(4, 5)
        };
        assert_eq!(first_of(s, None), Some(true));
        let old = Reported {
            seq: 4,
            work_generation: 7,
            at: Duration::from_secs(5),
            starved_iteration: 0,
        };
        assert_eq!(first_of(s, Some(old)), None);
        assert_eq!(
            first_of(
                Snapshot {
                    stuck: Duration::from_secs(35),
                    ..s
                },
                Some(old)
            ),
            Some(false)
        );
        assert_eq!(
            first_of(
                Snapshot {
                    work_generation: 8,
                    ..s
                },
                Some(old)
            ),
            Some(true)
        );
        assert_eq!(first_of(Snapshot { paused: true, ..s }, None), None);
    }
    #[test]
    fn nested_callback_restores_outer_scope_and_pause_resets_outside_progress() {
        let _lock = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let outer = Guard::enter(Site::UserEvent);
        set_phase(Phase::Configure);
        {
            let _inner = Guard::enter(Site::WindowEvent);
            assert!(!SEQ.load(Ordering::Acquire).is_multiple_of(2));
        }
        let snap = sample().unwrap();
        assert!(!snap.seq.is_multiple_of(2));
        assert_eq!(snap.site, Site::UserEvent as u8);
        assert_eq!(snap.phase, Phase::Configure as u8);
        drop(outer);
        without_stall_watch(|| without_stall_watch(|| assert!(sample().unwrap().paused)));
        let snap = sample().unwrap();
        assert!(snap.seq.is_multiple_of(2));
        assert!(!snap.paused);
        assert!(snap.progress_age < REPORT_AFTER);
    }

    #[test]
    fn nested_pause_clock_does_not_resurrect_but_real_outer_work_still_reports() {
        let _lock = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        {
            let _outer = Guard::enter(Site::UserEvent);
            set_phase(Phase::Configure);
            {
                let _inner = Guard::enter(Site::WindowEvent);
                without_stall_watch(|| std::thread::sleep(Duration::from_millis(5200)));
                assert_eq!(first_of(sample().unwrap(), None), None);
            }
            let restored = sample().unwrap();
            assert_eq!(restored.site, Site::UserEvent as u8);
            assert_eq!(restored.phase, Phase::Configure as u8);
            assert_eq!(
                first_of(restored, None),
                None,
                "excluded nested wait must stay excluded after restoring the outer scope"
            );
        }
        let _outer = Guard::enter(Site::UserEvent);
        std::thread::sleep(Duration::from_millis(5200));
        {
            let _inner = Guard::enter(Site::WindowEvent);
        }
        assert_eq!(
            first_of(sample().unwrap(), None),
            Some(true),
            "a nested callback without a pause must not erase real outer work"
        );
    }

    #[test]
    fn labels_cover_every_variant() {
        assert_eq!(site_label(Site::Resumed as u8), "resumed");
        assert_eq!(site_label(Site::WindowEvent as u8), "window_event");
        assert_eq!(site_label(Site::Redraw as u8), "redraw");
        assert_eq!(site_label(Site::UserEvent as u8), "user_event");
        assert_eq!(site_label(Site::AboutToWait as u8), "about_to_wait");
        assert_eq!(site_label(Site::DeviceEvent as u8), "device_event");
        assert_eq!(site_label(Site::NewEvents as u8), "new_events");
        assert_eq!(site_label(0), "unknown");

        assert_eq!(phase_label(Phase::None as u8), "none");
        assert_eq!(phase_label(Phase::Acquire as u8), "acquire");
        assert_eq!(phase_label(Phase::Submit as u8), "submit");
        assert_eq!(phase_label(Phase::Present as u8), "present");
        assert_eq!(phase_label(Phase::Configure as u8), "configure");
        assert_eq!(phase_label(9), "unknown");
    }
}
