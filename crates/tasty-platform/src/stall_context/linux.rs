//! Linux 수집: 프로세스 값은 getrusage, 메인 스레드 값은 /proc/self/task/<tid>.

use super::{Counters, CpuTimes, ThreadState};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

pub(super) const STACK_SAMPLE_SUPPORTED: bool = false;

static MAIN_TID: OnceLock<u32> = OnceLock::new();

pub(super) fn remember_main_thread() {
    // /proc/thread-self 는 "<pid>/task/<tid>" 를 가리킨다.
    let tid = std::fs::read_link("/proc/thread-self")
        .map_err(|e| e.to_string())
        .and_then(|link| {
            link.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.parse::<u32>().ok())
                .ok_or_else(|| format!("unexpected link {}", link.display()))
        });
    match tid {
        Ok(tid) => {
            MAIN_TID.get_or_init(|| tid);
        }
        Err(error) => tracing::warn!(%error, "stall context: main thread id not recorded"),
    }
}

fn main_thread_file(name: &str) -> Result<String, String> {
    thread_file(MAIN_TID.get().copied(), name)
}

fn thread_file(tid: Option<u32>, name: &str) -> Result<String, String> {
    let tid = tid.ok_or("main thread not recorded")?;
    let path = format!("/proc/self/task/{tid}/{name}");
    std::fs::read_to_string(&path).map_err(|e| format!("read {path}: {e}"))
}

/// `/proc/.../stat` 에서 명령 이름 뒤의 필드. 반환 벡터의 `i` 번째가 man proc 의 `i + 3` 번 필드다.
fn stat_fields(stat: &str) -> Result<Vec<&str>, String> {
    let close = stat.rfind(')').ok_or("malformed stat")?;
    Ok(stat[close + 1..].split_whitespace().collect())
}

fn stat_field<'a>(fields: &[&'a str], number: usize) -> Result<&'a str, String> {
    fields
        .get(number - 3)
        .copied()
        .ok_or_else(|| format!("stat field {number} missing"))
}

fn clock_ticks_per_second() -> Result<u64, String> {
    // SAFETY: sysconf 는 인자만 읽고 전역 상태를 바꾸지 않는다.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u64::try_from(ticks)
        .ok()
        .filter(|t| *t > 0)
        .ok_or_else(|| "sysconf(_SC_CLK_TCK) failed".to_string())
}

fn rusage() -> Result<libc::rusage, String> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: usage 는 rusage 크기의 쓰기 가능한 메모리이고 getrusage 는 그 안에만 쓴다.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return Err(format!("getrusage: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: 0 으로 채운 rusage 도 유효한 값이고 getrusage 가 성공해 값을 채웠다.
    Ok(unsafe { usage.assume_init() })
}

fn timeval(tv: libc::timeval) -> Duration {
    Duration::from_secs(tv.tv_sec.max(0) as u64) + Duration::from_micros(tv.tv_usec.max(0) as u64)
}

/// 스레드를 기록하지 않았으면 "main thread not recorded", 읽지 못했으면 그 이유를 돌려준다.
fn thread_cpu(tid: Option<u32>) -> Result<Duration, String> {
    let stat = thread_file(tid, "stat")?;
    let fields = stat_fields(&stat)?;
    let ticks = |n| {
        stat_field(&fields, n)?
            .parse::<u64>()
            .map_err(|e| format!("stat field {n}: {e}"))
    };
    let total = ticks(14)? + ticks(15)?;
    Ok(Duration::from_millis(
        total * 1000 / clock_ticks_per_second()?,
    ))
}

pub(super) fn cpu_times() -> Result<CpuTimes, String> {
    cpu_times_for(MAIN_TID.get().copied())
}

fn cpu_times_for(main_tid: Option<u32>) -> Result<CpuTimes, String> {
    let usage = rusage()?;
    Ok(CpuTimes {
        user: timeval(usage.ru_utime),
        system: timeval(usage.ru_stime),
        main_thread: thread_cpu(main_tid),
    })
}

pub(super) fn load_average() -> Result<[f64; 3], String> {
    let text =
        std::fs::read_to_string("/proc/loadavg").map_err(|e| format!("read /proc/loadavg: {e}"))?;
    let mut values = text.split_whitespace().map(str::parse::<f64>);
    let mut next = || {
        values
            .next()
            .ok_or("short /proc/loadavg")?
            .map_err(|e| format!("/proc/loadavg: {e}"))
    };
    Ok([next()?, next()?, next()?])
}

pub(super) fn counters() -> Result<Counters, String> {
    let usage = rusage()?;
    Ok(Counters {
        involuntary_context_switches: usage.ru_nivcsw.max(0) as u64,
        major_page_faults: usage.ru_majflt.max(0) as u64,
    })
}

pub(super) fn main_thread_state() -> Result<ThreadState, String> {
    let stat = main_thread_file("stat")?;
    let state = stat_field(&stat_fields(&stat)?, 3)?.to_string();
    // 커널이 주소를 감추거나 실행 중이면 "0" 이나 빈 값이다.
    let wait_channel = main_thread_file("wchan")
        .ok()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty() && w != "0");
    Ok(ThreadState {
        state,
        wait_channel,
    })
}

pub(super) fn scheduling() -> Result<String, String> {
    let stat = main_thread_file("stat")?;
    let fields = stat_fields(&stat)?;
    Ok(format!(
        "main thread priority {}, nice {}, policy {}",
        stat_field(&fields, 18)?,
        stat_field(&fields, 19)?,
        stat_field(&fields, 41)?
    ))
}

pub(super) fn start_stack_sample(_out: &Path) -> Result<String, String> {
    Err("not supported on this OS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_fields_skip_a_command_name_with_spaces_and_parens() {
        let stat = "123 (a (b) c) S 1 2 3";
        let fields = stat_fields(stat).expect("fields");
        assert_eq!(stat_field(&fields, 3), Ok("S"));
        assert_eq!(stat_field(&fields, 6), Ok("3"));
        assert!(stat_field(&fields, 7).is_err());
    }

    #[test]
    fn the_calling_thread_can_be_recorded_and_read() {
        remember_main_thread();
        let state = main_thread_state().expect("state");
        assert!(!state.state.is_empty());
        assert!(load_average().is_ok());
        assert!(cpu_times().is_ok_and(|c| c.main_thread.is_ok()));
        assert!(scheduling().is_ok_and(|s| s.contains("nice")));
    }

    #[test]
    fn cpu_times_carry_the_main_thread_read_failure() {
        let cpu = cpu_times_for(Some(u32::MAX)).expect("process cpu");
        let why = cpu.main_thread.expect_err("no such thread");
        assert!(why.starts_with("read /proc/self/task/"), "{why}");
    }

    #[test]
    fn a_main_thread_cpu_read_failure_keeps_its_reason() {
        assert_eq!(
            thread_cpu(None),
            Err("main thread not recorded".to_string())
        );
        let missing = thread_cpu(Some(u32::MAX)).expect_err("no such thread");
        assert!(
            missing.starts_with(&format!("read /proc/self/task/{}/stat: ", u32::MAX)),
            "{missing}"
        );
    }
}
