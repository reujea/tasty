//! 메시지 큐 도착을 받는 surface의 notify 로그에 알릴지 정한다. 파일 I/O는 하지 않는다.
//!
//! 큐가 비어 있다가 메시지가 들어오면 바로 알린다. 안 읽은 메시지가 남아 있으면 그 surface에
//! 마지막으로 알린 뒤 [`QUEUE_NOTICE_REPEAT`]가 지났을 때만 다시 알린다. 소비 읽기로 큐가
//! 비거나, 큐를 지우거나, surface가 닫히면 기록을 지워 다음 메시지가 다시 바로 알리게 한다.
//! 계약: docs/dev-guide/external-interaction.md의 notify 로그 절.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 안 읽은 메시지가 남아 있을 때 같은 surface에 다시 알리는 최소 간격.
/// 여러 lane이 한 conductor에 연달아 보고해도 받는 Claude에 프롬프트가 쌓이지 않게 하면서,
/// 받는 쪽이 첫 줄을 놓쳤을 때(세션 시작 전에 온 줄 등) 1분 안에는 다시 깨우도록 정한 값이다.
pub(crate) const QUEUE_NOTICE_REPEAT: Duration = Duration::from_secs(60);

/// surface별 마지막 알림 시각. 받는 surface를 소유한 엔진이 가진다.
#[derive(Debug, Default)]
pub(crate) struct QueueNoticeTimes(HashMap<u32, Instant>);

impl QueueNoticeTimes {
    /// 메시지 하나가 큐에 들어간 뒤 호출한다. `pending_before`는 넣기 전 대기 수다.
    /// 알려야 하면 시각을 기록하고 true를 돌려준다.
    pub(crate) fn should_notify(
        &mut self,
        surface: u32,
        pending_before: usize,
        now: Instant,
    ) -> bool {
        let due = pending_before == 0
            || self
                .0
                .get(&surface)
                .is_none_or(|last| now.saturating_duration_since(*last) >= QUEUE_NOTICE_REPEAT);
        if due {
            self.0.insert(surface, now);
        }
        due
    }

    /// 큐가 비었거나 surface가 사라졌을 때 기록을 지운다.
    pub(crate) fn forget(&mut self, surface: u32) {
        self.0.remove(&surface);
    }

    #[cfg(test)]
    pub(crate) fn has_record(&self, surface: u32) -> bool {
        self.0.contains_key(&surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_message_into_an_empty_queue_notifies_at_once() {
        let mut t = QueueNoticeTimes::default();
        let now = Instant::now();
        assert!(t.should_notify(4, 0, now));
        assert!(t.has_record(4));
    }

    #[test]
    fn more_messages_before_the_repeat_interval_stay_quiet() {
        let mut t = QueueNoticeTimes::default();
        let t0 = Instant::now();
        assert!(t.should_notify(4, 0, t0));
        assert!(!t.should_notify(4, 1, t0 + Duration::from_secs(1)));
        assert!(!t.should_notify(4, 2, t0 + QUEUE_NOTICE_REPEAT - Duration::from_millis(1)));
    }

    #[test]
    fn a_message_after_the_repeat_interval_notifies_again_and_restarts_it() {
        let mut t = QueueNoticeTimes::default();
        let t0 = Instant::now();
        assert!(t.should_notify(4, 0, t0));
        let t1 = t0 + QUEUE_NOTICE_REPEAT;
        assert!(t.should_notify(4, 1, t1));
        assert!(!t.should_notify(4, 2, t1 + Duration::from_secs(1)));
    }

    #[test]
    fn an_emptied_queue_notifies_the_next_message_regardless_of_the_interval() {
        let mut t = QueueNoticeTimes::default();
        let t0 = Instant::now();
        assert!(t.should_notify(4, 0, t0));
        t.forget(4);
        assert!(t.should_notify(4, 0, t0 + Duration::from_secs(1)));
    }

    #[test]
    fn surfaces_are_judged_apart() {
        let mut t = QueueNoticeTimes::default();
        let t0 = Instant::now();
        assert!(t.should_notify(4, 0, t0));
        assert!(t.should_notify(5, 0, t0));
        assert!(!t.should_notify(4, 1, t0));
    }

    #[test]
    fn a_waiting_queue_without_a_record_notifies() {
        let mut t = QueueNoticeTimes::default();
        assert!(t.should_notify(4, 3, Instant::now()));
    }
}
