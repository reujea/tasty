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

/// `read` 가 참이 되기를 `deadline` 까지 `interval` 간격으로 기다린다. 정규 읽기는 `read(false)` 다.
/// 정규 읽기가 거짓이면 그때 기한을 보고, 기한에 닿았으면 `read(true)` 를 한 번만 불러 그 값을 돌려준다.
/// 기한 전에 읽은 거짓 뒤 검사 스레드가 밀려도 기한 뒤 읽기 하나가 남으므로 그 거짓으로 실패하지 않는다.
///
/// `read(false)` 는 처음 한 번과, 거짓 읽기 뒤 기한 검사를 통과한 다음에만 불린다. 그래서 그 안의 진행
/// 작업(재전송, 새 시도, 러너 전진)은 기한 검사를 원래 루프의 단언 자리에 둔 것과 같은 범위에서만 돈다.
/// `holds_by` 는 기한 뒤에도 본문을 그대로 다시 부르지만, 이 도우미의 `read(true)` 에서는 진행을 빼고
/// 이미 관측된 상태만 읽는다. 진행과 읽기가 같은 `&mut` 상태를 빌리는 자리를 위해 하나의 클로저로 받는다.
pub fn holds_by_every(
    deadline: Instant,
    interval: Duration,
    mut read: impl FnMut(bool) -> bool,
) -> bool {
    loop {
        if read(false) {
            return true;
        }
        if Instant::now() >= deadline {
            return read(true);
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

    /// 이미 지난 기한이어도 정규 읽기를 먼저 하고, 그것이 참이면 `last` 읽기 없이 참이다.
    #[test]
    fn a_true_regular_read_holds_without_a_last_read() {
        let mut calls = Vec::new();
        assert!(holds_by_every(
            Instant::now(),
            Duration::from_millis(1),
            |last| {
                calls.push(last);
                true
            }
        ));
        assert_eq!(calls, vec![false]);
    }

    /// 정규 읽기가 거짓인 사이 기한이 지나면 `last` 읽기 한 번을 하고 그 값을 돌려준다.
    #[test]
    fn a_false_regular_read_past_the_deadline_is_followed_by_one_last_read() {
        for value in [true, false] {
            let mut calls = Vec::new();
            let held = holds_by_every(Instant::now(), Duration::from_millis(1), |last| {
                calls.push(last);
                last && value
            });
            assert_eq!(held, value);
            assert_eq!(calls, vec![false, true]);
        }
    }

    /// 정규 읽기 안의 진행 중에 기한이 지나 거짓이면 정규 읽기를 다시 하지 않고 `last` 읽기로 끝난다.
    #[test]
    fn a_deadline_passed_during_a_regular_read_ends_with_one_last_read() {
        let deadline = Instant::now() + Duration::from_millis(30);
        let mut calls = Vec::new();
        assert!(!holds_by_every(
            deadline,
            Duration::from_millis(1),
            |last| {
                if !last {
                    std::thread::sleep(Duration::from_millis(40));
                }
                calls.push(last);
                false
            }
        ));
        assert_eq!(calls, vec![false, true]);
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
        assert_eq!(calls.iter().filter(|last| **last).count(), 1);
        assert!(calls.last() == Some(&true));
        assert!(calls.len() >= 3, "read {} times", calls.len());
    }

    /// 읽기 사이 간격은 `interval` 이상이다.
    #[test]
    fn reads_are_at_least_interval_apart() {
        let interval = Duration::from_millis(15);
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut stamps = Vec::new();
        assert!(!holds_by_every(deadline, interval, |_| {
            stamps.push(Instant::now());
            false
        }));
        assert!(stamps.len() >= 3, "read {} times", stamps.len());
        for pair in stamps[..stamps.len() - 1].windows(2) {
            assert!(pair[1] - pair[0] >= interval, "{:?}", pair[1] - pair[0]);
        }
    }
}
