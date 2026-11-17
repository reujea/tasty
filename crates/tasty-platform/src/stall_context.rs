//! hang 보고서에 붙일 프로세스·시스템 상태. 모두 관측만 하고 메인 루프를 깨우거나 바꾸지 않는다.
//! 워치독 스레드에서 읽으므로 메인 스레드가 멈춰 있어도 수집할 수 있다.
//! 수집에 실패한 항목은 그 항목만 unavailable로 쓰고 보고를 막지 않는다.

#[cfg(target_os = "linux")]
#[path = "stall_context/linux.rs"]
mod os;

#[cfg(not(target_os = "linux"))]
mod os {
    //! 이 OS에서는 아직 수집하지 않는다.
    use super::{Counters, CpuTimes, ThreadState};
    use std::path::Path;

    const UNSUPPORTED: &str = "not supported on this OS";
    pub(super) const STACK_SAMPLE_SUPPORTED: bool = false;

    pub(super) fn remember_main_thread() {}
    pub(super) fn cpu_times() -> Result<CpuTimes, String> {
        Err(UNSUPPORTED.into())
    }
    pub(super) fn load_average() -> Result<[f64; 3], String> {
        Err(UNSUPPORTED.into())
    }
    pub(super) fn counters() -> Result<Counters, String> {
        Err(UNSUPPORTED.into())
    }
    pub(super) fn main_thread_state() -> Result<ThreadState, String> {
        Err(UNSUPPORTED.into())
    }
    pub(super) fn scheduling() -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }
    pub(super) fn start_stack_sample(_out: &Path) -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }
}

use std::collections::VecDeque;
use std::fmt::Display;
use std::path::Path;
use std::time::Duration;

/// 폴링 지연 최대값을 보는 구간.
pub(crate) const POLL_HISTORY: Duration = Duration::from_secs(60);
/// 스택 표본은 이 간격 안에 한 번만 만든다. 표본 도구가 수 초 동안 CPU를 쓰기 때문이다.
pub(crate) const STACK_SAMPLE_EVERY: Duration = Duration::from_secs(600);

/// 메인 스레드에서 한 번 불러 그 스레드를 이후 보고의 "Main thread"로 기록한다.
pub(crate) fn remember_main_thread() {
    os::remember_main_thread();
}

/// 프로세스 전체와 메인 스레드의 누적 CPU 시간.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CpuTimes {
    pub user: Duration,
    pub system: Duration,
    /// 메인 스레드를 기록하지 못했거나 읽지 못하면 그 이유.
    pub main_thread: Result<Duration, String>,
}

/// 매 폴링에서 읽는 CPU 시간. 보고 때 직전 폴링 값과의 차이를 낸다.
pub(crate) fn cpu_times() -> Result<CpuTimes, String> {
    os::cpu_times()
}

/// 프로세스 누적 카운터.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Counters {
    pub involuntary_context_switches: u64,
    pub major_page_faults: u64,
}

/// 메인 스레드의 스케줄러 상태.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ThreadState {
    /// Linux `/proc` 상태 문자(`R` 실행·대기, `S` 잠듦, `D` 끊을 수 없는 대기 등).
    pub state: String,
    /// 잠든 커널 함수. 커널이 감추면 `None`.
    pub wait_channel: Option<String>,
}

/// 마지막으로 받은 창 이벤트 값. `None`은 아직 받지 못했다는 뜻이다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowState {
    pub focused: Option<bool>,
    pub occluded: Option<bool>,
}

/// 워치독 폴링 간격 기록. 간격에는 직전 폴링의 수집·로그·보고서 쓰기 시간이 함께 들어가므로
/// 늦은 간격만으로 CPU 미할당을 단정하지 않는다. Process CPU 증가량·스레드 상태·write_ms와 함께 본다.
#[derive(Debug, Default)]
pub(crate) struct PollHistory {
    /// (폴링이 끝난 시각, 실제 간격). 시각은 워치독 기준점부터의 경과다.
    entries: VecDeque<(Duration, Duration)>,
}

/// 보고 시점의 폴링 간격 요약.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PollSummary {
    pub latest: Duration,
    pub max: Duration,
}

impl PollHistory {
    pub fn record(&mut self, at: Duration, interval: Duration) {
        self.entries.push_back((at, interval));
        while self
            .entries
            .front()
            .is_some_and(|(t, _)| at.saturating_sub(*t) > POLL_HISTORY)
        {
            self.entries.pop_front();
        }
    }

    pub fn summary(&self) -> PollSummary {
        PollSummary {
            latest: self.entries.back().map_or(Duration::ZERO, |(_, i)| *i),
            max: self
                .entries
                .iter()
                .map(|(_, i)| *i)
                .max()
                .unwrap_or_default(),
        }
    }
}

/// 스택 표본 빈도 제한. 첫 보고에서만, 그리고 [`STACK_SAMPLE_EVERY`]에 한 번만 허용한다.
#[derive(Debug, Default)]
pub(crate) struct SampleGate {
    last: Option<Duration>,
}

impl SampleGate {
    /// 허용하면 그 시각을 기록한다. `now`는 워치독 기준점부터의 경과다.
    pub fn admit(&mut self, now: Duration, first: bool) -> bool {
        let admitted = first
            && self
                .last
                .is_none_or(|last| now.saturating_sub(last) >= STACK_SAMPLE_EVERY);
        if admitted {
            self.last = Some(now);
        }
        admitted
    }
}

/// 이 OS에서 스택 표본을 만들 수 있는가.
pub(crate) const STACK_SAMPLE_SUPPORTED: bool = os::STACK_SAMPLE_SUPPORTED;

/// 메인 스레드 스택 표본을 `out`에 쓰는 자식 프로세스를 띄우고 기다리지 않는다.
/// 성공하면 보고서에 적을 설명을 돌려준다.
pub(crate) fn start_stack_sample(out: &Path) -> Result<String, String> {
    os::start_stack_sample(out)
}

/// hang 보고서 한 건에 붙일 관측값.
#[derive(Clone, Debug)]
pub(crate) struct Context {
    pub poll: PollSummary,
    pub poll_expected: Duration,
    pub load_average: Result<[f64; 3], String>,
    pub cpu: Result<CpuTimes, String>,
    /// 직전 폴링의 CPU 시간과 그때부터 지난 시간.
    pub cpu_previous: Option<(CpuTimes, Duration)>,
    pub counters: Result<Counters, String>,
    pub main_thread: Result<ThreadState, String>,
    pub scheduling: Result<String, String>,
    pub window: WindowState,
    /// 스택 표본 줄. 표본을 만들지 않으면 그 이유.
    pub stack_sample: Result<String, String>,
}

impl Context {
    /// 보고 시점에 OS 값을 읽어 모은다. CPU 시간은 이번 폴링에서 읽은 값을 받는다.
    pub fn collect(
        poll: PollSummary,
        poll_expected: Duration,
        cpu: Result<CpuTimes, String>,
        cpu_previous: Option<(CpuTimes, Duration)>,
        window: WindowState,
    ) -> Self {
        Self {
            poll,
            poll_expected,
            load_average: os::load_average(),
            cpu,
            cpu_previous,
            counters: os::counters(),
            main_thread: os::main_thread_state(),
            scheduling: os::scheduling(),
            window,
            stack_sample: Err(if STACK_SAMPLE_SUPPORTED {
                format!(
                    "not taken: first report only, one sample per {} s",
                    STACK_SAMPLE_EVERY.as_secs()
                )
            } else {
                "not supported on this OS".to_string()
            }),
        }
    }

    /// hang 보고서의 `=== Load ===` 절.
    pub fn describe(&self) -> String {
        let mut out = String::from("=== Load ===\n");
        let w = &mut out;
        line(
            w,
            "Watchdog poll interval",
            Ok::<_, String>(format!(
                "latest {} ms, max {} ms in last {} s (expected {} ms)",
                self.poll.latest.as_millis(),
                self.poll.max.as_millis(),
                POLL_HISTORY.as_secs(),
                self.poll_expected.as_millis()
            )),
        );
        line(
            w,
            "Load average",
            self.load_average
                .as_ref()
                .map_err(Clone::clone)
                .map(|[a, b, c]| format!("{a:.2} {b:.2} {c:.2}")),
        );
        line(
            w,
            "Process CPU",
            self.cpu
                .as_ref()
                .map(|c| self.process_cpu(c))
                .map_err(Clone::clone),
        );
        line(
            w,
            "Main thread CPU",
            self.cpu.as_ref().map_err(Clone::clone).and_then(|c| {
                c.main_thread
                    .as_ref()
                    .map(|now| self.main_thread_cpu(*now))
                    .map_err(Clone::clone)
            }),
        );
        line(
            w,
            "Main thread state",
            self.main_thread
                .as_ref()
                .map(|t| match &t.wait_channel {
                    Some(wchan) => format!("{} (wchan {wchan})", t.state),
                    None => t.state.clone(),
                })
                .map_err(Clone::clone),
        );
        line(
            w,
            "Involuntary context switches",
            self.counters
                .as_ref()
                .map(|c| c.involuntary_context_switches.to_string())
                .map_err(Clone::clone),
        );
        line(
            w,
            "Major page faults",
            self.counters
                .as_ref()
                .map(|c| c.major_page_faults.to_string())
                .map_err(Clone::clone),
        );
        line(w, "Scheduling", self.scheduling.clone());
        line(
            w,
            "Window",
            Ok::<_, String>(format!(
                "focused {}, occluded {}",
                tri(self.window.focused),
                tri(self.window.occluded)
            )),
        );
        line(w, "Stack sample", self.stack_sample.clone());
        out
    }

    fn process_cpu(&self, now: &CpuTimes) -> String {
        let mut text = format!(
            "user {} ms, system {} ms",
            now.user.as_millis(),
            now.system.as_millis()
        );
        if let Some((prev, ago)) = &self.cpu_previous {
            text.push_str(&format!(
                " (+{} ms user, +{} ms system over {} ms)",
                now.user.saturating_sub(prev.user).as_millis(),
                now.system.saturating_sub(prev.system).as_millis(),
                ago.as_millis()
            ));
        }
        text
    }

    fn main_thread_cpu(&self, now: Duration) -> String {
        let mut text = format!("{} ms", now.as_millis());
        if let Some((prev, ago)) = &self.cpu_previous
            && let Ok(prev) = &prev.main_thread
        {
            text.push_str(&format!(
                " (+{} ms over {} ms)",
                now.saturating_sub(*prev).as_millis(),
                ago.as_millis()
            ));
        }
        text
    }
}

fn line(out: &mut String, name: &str, value: Result<String, impl Display>) {
    out.push_str(&match value {
        Ok(v) => format!("{name}: {v}\n"),
        Err(why) => format!("{name}: unavailable ({why})\n"),
    });
}

fn tri(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        let cpu = CpuTimes {
            user: Duration::from_millis(1500),
            system: Duration::from_millis(300),
            main_thread: Ok(Duration::from_millis(1200)),
        };
        let prev = CpuTimes {
            user: Duration::from_millis(1490),
            system: Duration::from_millis(300),
            main_thread: Ok(Duration::from_millis(1200)),
        };
        Context {
            poll: PollSummary::default(),
            poll_expected: Duration::from_secs(1),
            load_average: Ok([2.38, 2.28, 2.54]),
            cpu: Ok(cpu),
            cpu_previous: Some((prev, Duration::from_millis(1001))),
            counters: Ok(Counters {
                involuntary_context_switches: 42,
                major_page_faults: 0,
            }),
            main_thread: Ok(ThreadState {
                state: "S".into(),
                wait_channel: Some("do_sys_poll".into()),
            }),
            scheduling: Ok("priority 20, nice 0".into()),
            window: WindowState {
                focused: Some(false),
                occluded: None,
            },
            stack_sample: Err("not supported on this OS".into()),
        }
    }

    #[test]
    fn a_seven_second_poll_shows_up_as_the_maximum_interval() {
        let mut history = PollHistory::default();
        let mut at = Duration::ZERO;
        for interval in [1000, 1001, 7003, 1000] {
            at += Duration::from_millis(interval);
            history.record(at, Duration::from_millis(interval));
        }
        let ctx = Context {
            poll: history.summary(),
            ..context()
        };
        let text = ctx.describe();
        assert!(
            text.contains(
                "Watchdog poll interval: latest 1000 ms, max 7003 ms in last 60 s (expected 1000 ms)"
            ),
            "{text}"
        );
    }

    #[test]
    fn poll_history_forgets_intervals_older_than_its_window() {
        let mut history = PollHistory::default();
        history.record(Duration::from_secs(10), Duration::from_secs(9));
        history.record(Duration::from_secs(71), Duration::from_secs(1));
        assert_eq!(history.summary().max, Duration::from_secs(1));
    }

    #[test]
    fn a_failed_item_is_unavailable_and_the_rest_stays() {
        let ctx = Context {
            load_average: Err("read /proc/loadavg: denied".into()),
            ..context()
        };
        let text = ctx.describe();
        assert!(
            text.contains("Load average: unavailable (read /proc/loadavg: denied)\n"),
            "{text}"
        );
        for expected in [
            "Process CPU: user 1500 ms, system 300 ms (+10 ms user, +0 ms system over 1001 ms)\n",
            "Main thread CPU: 1200 ms (+0 ms over 1001 ms)\n",
            "Main thread state: S (wchan do_sys_poll)\n",
            "Involuntary context switches: 42\n",
            "Major page faults: 0\n",
            "Scheduling: priority 20, nice 0\n",
            "Window: focused false, occluded unknown\n",
            "Stack sample: unavailable (not supported on this OS)\n",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in\n{text}");
        }
    }

    #[test]
    fn a_main_thread_read_failure_names_its_reason() {
        let mut cpu = context().cpu.expect("cpu");
        cpu.main_thread = Err("read /proc/self/task/7/stat: No such file or directory".into());
        let ctx = Context {
            cpu: Ok(cpu),
            ..context()
        };
        let text = ctx.describe();
        assert!(
            text.contains(
                "Main thread CPU: unavailable (read /proc/self/task/7/stat: No such file or directory)\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("Process CPU: user 1500 ms, system 300 ms (+10 ms user"),
            "{text}"
        );
    }

    #[test]
    fn stack_samples_once_per_event_and_once_per_ten_minutes() {
        let mut gate = SampleGate::default();
        let t0 = Duration::from_secs(100);
        assert!(gate.admit(t0, true), "the first report of the first event");
        assert!(
            !gate.admit(t0 + Duration::from_secs(30), false),
            "a repeat of the same event"
        );
        assert!(
            !gate.admit(t0 + Duration::from_secs(300), true),
            "another event within ten minutes"
        );
        assert!(
            gate.admit(t0 + STACK_SAMPLE_EVERY, true),
            "ten minutes later"
        );
        assert!(
            !gate.admit(t0 + STACK_SAMPLE_EVERY * 3, false),
            "a repeat report never samples"
        );
    }
}
