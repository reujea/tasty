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
    read: impl FnMut(bool) -> bool,
) -> bool {
    wait_with(deadline, interval, read, Instant::now, pause)
}

/// 간격이 0 이면 그 회차에 스레드를 양보만 하고, 아니면 그만큼 잔다. 0 간격 대기가 바쁜 폴링이 되지 않게 한다.
fn pause(interval: Duration) {
    if interval.is_zero() {
        std::thread::yield_now();
    } else {
        std::thread::sleep(interval);
    }
}

/// `holds_by_every` 의 본체. 시험이 시각과 멈춤을 주입해 읽기·시각 검사·멈춤의 순서를 직접 가른다.
fn wait_with(
    deadline: Instant,
    interval: Duration,
    mut read: impl FnMut(bool) -> bool,
    mut now: impl FnMut() -> Instant,
    mut pause: impl FnMut(Duration),
) -> bool {
    loop {
        if read(false) {
            return true;
        }
        if now() >= deadline {
            return read(true);
        }
        pause(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::{holds_by, holds_by_every, wait_with};
    use std::cell::{Cell, RefCell};
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

    #[derive(Debug, PartialEq, Eq)]
    enum Step {
        Read { last: bool },
        Clock,
        Pause(Duration),
    }

    /// 주입한 시각과 멈춤으로 `wait_with` 를 돌려 단계 순서를 돌려준다. `read` 는 몇 번째 정규 읽기인지
    /// 받아 (그 읽기 동안 기한을 넘기는지, 정규 읽기 값) 을 정한다. 마지막 읽기는 `last_value` 를 돌려준다.
    fn run(
        interval: Duration,
        regular: impl Fn(usize) -> (bool, bool),
        last_value: bool,
    ) -> (bool, Vec<Step>) {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(1);
        let past = Cell::new(false);
        let steps = RefCell::new(Vec::new());
        let mut regular_reads = 0;
        let held = wait_with(
            deadline,
            interval,
            |last| {
                steps.borrow_mut().push(Step::Read { last });
                if last {
                    return last_value;
                }
                let (passes_deadline, value) = regular(regular_reads);
                regular_reads += 1;
                if passes_deadline {
                    past.set(true);
                }
                value
            },
            || {
                steps.borrow_mut().push(Step::Clock);
                if past.get() { deadline } else { start }
            },
            |paused| steps.borrow_mut().push(Step::Pause(paused)),
        );
        (held, steps.into_inner())
    }

    const IV: Duration = Duration::from_millis(7);

    /// 정규 읽기가 참이면 시각도 보지 않고 바로 참이다.
    #[test]
    fn a_true_regular_read_holds_without_a_clock_check_or_a_last_read() {
        let (held, steps) = run(IV, |_| (false, true), false);
        assert!(held);
        assert_eq!(steps, vec![Step::Read { last: false }]);
    }

    /// 정규 읽기 동안 기한이 지나 거짓이면, 멈추지 않고 곧바로 시각을 본 뒤 마지막 읽기 한 번으로 판정한다.
    /// 거짓 읽기 뒤 한 번 더 멈춘 다음 시각을 보는 구현은 여기서 Pause 가 끼어 실패한다.
    #[test]
    fn a_deadline_passed_during_a_regular_read_is_checked_before_any_pause() {
        for last_value in [true, false] {
            let (held, steps) = run(IV, |_| (true, false), last_value);
            assert_eq!(held, last_value);
            assert_eq!(
                steps,
                vec![
                    Step::Read { last: false },
                    Step::Clock,
                    Step::Read { last: true }
                ]
            );
        }
    }

    /// 기한 전 정규 읽기는 매번 시각 검사를 통과한 뒤 interval 만큼 멈추고 다음 정규 읽기로 간다.
    /// 마지막 읽기는 한 번뿐이고 그 뒤에는 아무것도 없다.
    #[test]
    fn regular_reads_run_only_after_a_passed_clock_check_and_one_pause() {
        let (held, steps) = run(IV, |n| (n == 2, false), false);
        assert!(!held);
        assert_eq!(
            steps,
            vec![
                Step::Read { last: false },
                Step::Clock,
                Step::Pause(IV),
                Step::Read { last: false },
                Step::Clock,
                Step::Pause(IV),
                Step::Read { last: false },
                Step::Clock,
                Step::Read { last: true },
            ]
        );
    }

    /// 간격이 0 이어도 기한 전 거짓 읽기 뒤에는 멈춤을 부른다. 멈춤이 무엇을 하는지는 보지 않는다.
    #[test]
    fn a_zero_interval_still_pauses_every_round() {
        let (_, steps) = run(Duration::ZERO, |n| (n == 1, false), false);
        assert_eq!(
            steps
                .iter()
                .filter(|step| **step == Step::Pause(Duration::ZERO))
                .count(),
            1
        );
        assert_eq!(steps[2], Step::Pause(Duration::ZERO));
    }

    /// 실제 시각으로도 끝까지 거짓이면 거짓이고, 마지막 읽기는 한 번뿐이며 맨 끝이다.
    #[test]
    fn with_the_real_clock_a_read_that_stays_false_ends_with_one_last_read() {
        let deadline = Instant::now() + Duration::from_millis(10);
        let mut calls = Vec::new();
        assert!(!holds_by_every(deadline, Duration::ZERO, |last| {
            calls.push(last);
            false
        }));
        assert_eq!(calls.iter().filter(|last| **last).count(), 1);
        assert_eq!(calls.last(), Some(&true));
    }
}
