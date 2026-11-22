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

#[cfg(test)]
mod tests {
    use super::holds_by;
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
}
