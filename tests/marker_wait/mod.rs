//! 훅의 마커 파일을 기다리고 만료 시 경과·예산·실제 확인 횟수·부하를 기록한다.
//! 확인 횟수가 적다는 사실만으로 마커 생성 여부나 지연 원인을 단정하지 않는다.

// 이 모듈을 포함하는 타깃마다 사용하는 함수가 달라 미사용 함수도 제공한다.
#![allow(dead_code)]

use std::path::Path;
use std::time::{Duration, Instant};

pub const POLL: Duration = Duration::from_millis(50);

pub fn wait_file_content(path: &Path, budget: Duration) -> String {
    wait_file_content_with_evidence(path, budget, String::new)
}

/// 추가 진단을 만드는 evidence는 만료할 때만 호출한다.
pub fn wait_file_content_with_evidence(
    path: &Path,
    budget: Duration,
    evidence: impl Fn() -> String,
) -> String {
    let start = Instant::now();
    let mut polls = 0usize;
    let mut content = String::new();
    // 확인 횟수 진단이 POLL 간격을 전제하므로 그 간격으로 읽는다. 기한 뒤 읽기도 파일 읽기뿐이다.
    let found = tasty_utils::test_wait::holds_by_every(start + budget, POLL, |_| {
        polls += 1;
        content = std::fs::read_to_string(path)
            .map(|c| c.trim().to_string())
            .unwrap_or_default();
        !content.is_empty()
    });
    if !found {
        panic!(
            "{}",
            timeout_message(path, budget, start.elapsed(), polls, &evidence())
        );
    }
    content
}

fn timeout_message(
    path: &Path,
    budget: Duration,
    elapsed: Duration,
    polls: usize,
    evidence: &str,
) -> String {
    let expected = budget.as_secs_f64() / POLL.as_secs_f64();
    let mut msg = format!(
        "marker file {} content not observed within {budget:?}\n경과 {elapsed:?} · 확인 {polls} 회 (예산/간격 = {expected:.0} 회 기대)",
        path.display()
    );
    if (polls as f64) < expected * 0.5 {
        msg.push_str(
            "\n확인 횟수가 기대의 절반보다 적다. 폴링 지연과 마커 생성·읽기 상태를 함께 확인한다. 이 수만으로 원인을 단정하지 않는다.",
        );
    }
    if let Some(load) = load_average_1m() {
        msg.push_str(&format!("\n부하(1 분 평균) {load}"));
    }
    if !evidence.is_empty() {
        msg.push_str(&format!("\n호출자 증거: {evidence}"));
    }
    msg
}

/// 리눅스의 1분 평균 부하를 진단에 포함한다. 이 값만으로 지연 원인을 판단하지 않는다.
fn load_average_1m() -> Option<String> {
    let raw = std::fs::read_to_string("/proc/loadavg").ok()?;
    raw.split_whitespace().next().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_starved_loop_is_called_out_but_a_busy_one_is_not() {
        let p = Path::new("/tmp/nope");
        let starved = timeout_message(p, Duration::from_secs(15), Duration::from_secs(16), 3, "");
        assert!(
            starved.contains("확인 횟수가 기대의 절반보다 적다"),
            "{starved}"
        );

        let busy = timeout_message(p, Duration::from_secs(15), Duration::from_secs(16), 300, "");
        assert!(!busy.contains("확인 횟수가 기대의 절반보다 적다"), "{busy}");
    }

    #[test]
    fn the_message_carries_what_the_next_reader_needs() {
        let msg = timeout_message(
            Path::new("/tmp/m"),
            Duration::from_secs(15),
            Duration::from_secs(15),
            300,
            "shell 출력에 트리거 텍스트 있음",
        );
        assert!(msg.contains("/tmp/m"));
        assert!(msg.contains("15s"));
        assert!(msg.contains("300 회"));
        assert!(msg.contains("shell 출력에 트리거 텍스트 있음"));
    }

    #[test]
    fn empty_evidence_adds_no_line() {
        let msg = timeout_message(
            Path::new("/tmp/m"),
            Duration::from_secs(1),
            Duration::from_secs(2),
            20,
            "",
        );
        assert!(!msg.contains("호출자 증거"), "{msg}");
    }
}
