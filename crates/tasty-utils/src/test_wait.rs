//! 시험 전용 대기 도우미.

use std::time::{Duration, Instant};

/// `holds` 가 참이 되기를 `deadline` 까지 기다린다. 기한에 닿으면 한 번 더 읽어 그 값을 돌려준다.
/// 기한 전에 읽은 거짓 뒤에 검사 스레드가 밀려, 그 사이 기한 안에 참이 된 값을 놓치지 않기 위해서다.
/// 기한 뒤의 마지막 읽기에서도 `holds` 를 그대로 부르므로, 그 안의 진행 작업(결과 거두기 등)이 기한
/// 뒤에 한 번 더 돌아도 되는 자리에서만 쓴다.
pub fn holds_by(deadline: Instant, mut holds: impl FnMut() -> bool) -> bool {
    loop {
        if holds() {
            return true;
        }
        if Instant::now() >= deadline {
            return holds();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// `read` 가 참이 되기를 `deadline` 까지 `interval` 간격으로 기다린다. 읽기마다 먼저 기한을 본다. 기한 전
/// 읽기는 `last = false` 로, 기한에 닿은 뒤의 첫 읽기 하나만 `last = true` 로 부르고 그 값을 돌려준다.
/// 이미 지난 기한이면 `last = true` 읽기 한 번으로 끝난다. 기한 전 읽기가 거짓이면 다음 읽기가 반드시 있으므로
/// 기한 전에 읽은 거짓으로 실패하지 않는다.
///
/// `holds_by` 와 달리 마지막 읽기를 표시한다. 진행 작업(재전송, 새 시도, 러너 전진)이 기한 뒤에 돌면 안 되는
/// 자리는 `last` 일 때 진행을 빼고 이미 관측된 상태만 읽는다. 진행과 읽기가 같은 `&mut` 상태를 빌리는
/// 자리를 위해 하나의 클로저로 받는다.
pub fn holds_by_every(
    deadline: Instant,
    interval: Duration,
    mut read: impl FnMut(bool) -> bool,
) -> bool {
    loop {
        let last = Instant::now() >= deadline;
        if read(last) {
            return true;
        }
        if last {
            return false;
        }
        std::thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::{holds_by, holds_by_every};
    use std::time::{Duration, Instant};

    /// 기한이 지난 뒤 첫 재읽기에서 참이 되면 참이다.
    #[test]
    fn a_condition_that_turns_true_on_the_reread_after_the_deadline_holds() {
        let deadline = Instant::now();
        let mut reads = 0;
        assert!(holds_by(deadline, || {
            reads += 1;
            reads >= 2
        }));
        assert_eq!(reads, 2);
    }

    #[test]
    fn a_condition_that_stays_false_does_not_hold() {
        let deadline = Instant::now() + Duration::from_millis(10);
        let mut reads = 0;
        assert!(!holds_by(deadline, || {
            reads += 1;
            false
        }));
        assert!(reads >= 2, "read {reads} times");
    }

    /// 이미 지난 기한이면 `last` 읽기 한 번만 하고 그 값을 돌려준다.
    #[test]
    fn a_past_deadline_reads_once_as_last_and_returns_that_value() {
        for value in [true, false] {
            let mut calls = Vec::new();
            let held = holds_by_every(Instant::now(), Duration::from_millis(1), |last| {
                calls.push(last);
                value
            });
            assert_eq!(held, value);
            assert_eq!(calls, vec![true]);
        }
    }

    /// 기한 전 거짓 읽기 뒤 잠자는 사이 기한이 지나면 다음 읽기가 `last` 이고, 그 값이 참이면 참이다.
    #[test]
    fn a_false_read_before_the_deadline_is_followed_by_a_last_read() {
        let deadline = Instant::now() + Duration::from_millis(20);
        let mut calls = Vec::new();
        assert!(holds_by_every(deadline, Duration::from_millis(5), |last| {
            calls.push(last);
            last
        }));
        assert_eq!(calls.last(), Some(&true));
        assert!(
            calls[..calls.len() - 1].iter().all(|last| !last),
            "{calls:?}"
        );
        assert!(calls.len() >= 2, "{calls:?}");
    }

    /// 끝까지 거짓이면 거짓이고, `last` 읽기는 한 번뿐이며 마지막이다.
    #[test]
    fn a_read_that_stays_false_fails_after_exactly_one_last_read() {
        let deadline = Instant::now() + Duration::from_millis(10);
        let mut calls = Vec::new();
        assert!(!holds_by_every(
            deadline,
            Duration::from_millis(1),
            |last| {
                calls.push(last);
                false
            }
        ));
        assert_eq!(calls.iter().filter(|last| **last).count(), 1, "{calls:?}");
        assert_eq!(calls.last(), Some(&true));
    }

    /// 기한 전 읽기 사이 간격은 `interval` 이상이다.
    #[test]
    fn reads_before_the_deadline_are_at_least_interval_apart() {
        let interval = Duration::from_millis(15);
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut stamps = Vec::new();
        assert!(!holds_by_every(deadline, interval, |_| {
            stamps.push(Instant::now());
            false
        }));
        assert!(stamps.len() >= 2, "read {} times", stamps.len());
        for pair in stamps.windows(2) {
            assert!(pair[1] - pair[0] >= interval, "{:?}", pair[1] - pair[0]);
        }
    }
}
