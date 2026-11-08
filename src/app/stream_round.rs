//! GUI 메인 루프의 스트림 수신 회차.
//!
//! winit은 사용자 이벤트를 큐가 빌 때까지 처리한 뒤에야 redraw와 `about_to_wait`로 넘어간다.
//! 스트림 회차가 끝날 때마다 같은 이벤트가 다시 오면 입력·렌더·IPC가 차례를 받지 못한다.
//! 그래서 IPC 회차와 같은 간격 규칙을 쓴다. 규칙과 이유는 docs/architecture/data-flows.md 의
//! 스트림 회차 절에 있다.

use std::time::Instant;

use crate::app::App;
use crate::app::ipc_round::ROUND_TIME_BUDGET;
use tasty_ipc::stream_hub::PUMP_MESSAGE_BUDGET;

/// 직전 스트림 회차가 끝난 시각을 기억해 사용자 이벤트가 회차를 너무 자주 열지 않게 한다.
#[derive(Default)]
pub(crate) struct StreamPacer {
    last_round_end: Option<Instant>,
}

impl StreamPacer {
    fn event_may_run_round(&self, now: Instant) -> bool {
        self.last_round_end
            .is_none_or(|end| now.saturating_duration_since(end) >= ROUND_TIME_BUDGET)
    }
}

/// 회차를 여는 자리.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StreamRoundSite {
    /// `StreamReady` 사용자 이벤트. 직전 회차 뒤 간격이 지났을 때만 연다.
    Event,
    /// iteration마다 한 번 오는 `about_to_wait`. 미룬 회차를 여기서 처리한다.
    AboutToWait,
}

/// 회차를 열지 판단한다. 처리 전인 깨움이 없으면 채널에도 처리할 메시지가 없다.
fn should_run(pending: bool, site: StreamRoundSite, pacer: &StreamPacer, now: Instant) -> bool {
    pending && (site == StreamRoundSite::AboutToWait || pacer.event_may_run_round(now))
}

impl App {
    /// 깨움 표시를 내린 뒤 예산만큼 수신 메시지를 처리한다.
    /// 간격 때문에 건너뛴 이벤트는 표시를 남겨 두므로 새 이벤트가 쌓이지 않고 `about_to_wait`가 처리한다.
    pub(crate) fn run_stream_round(&mut self, site: StreamRoundSite) {
        if !should_run(
            self.stream_wake.is_pending(),
            site,
            &self.stream_pacer,
            Instant::now(),
        ) {
            return;
        }
        self.stream_wake.open_round();
        let (outcome, cut) = self.pump_stream_inbound(PUMP_MESSAGE_BUDGET);
        self.apply_stream_outcome(outcome);
        self.stream_pacer.last_round_end = Some(Instant::now());
        // 남긴 메시지에는 새 깨움이 오지 않을 수 있어 표시를 다시 세운다.
        if cut {
            self.stream_wake.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn nothing_pending_never_runs() {
        let pacer = StreamPacer::default();
        let now = Instant::now();
        assert!(!should_run(false, StreamRoundSite::Event, &pacer, now));
        assert!(!should_run(
            false,
            StreamRoundSite::AboutToWait,
            &pacer,
            now
        ));
    }

    #[test]
    fn event_waits_for_the_gap_after_a_round() {
        let now = Instant::now();
        let pacer = StreamPacer {
            last_round_end: Some(now),
        };
        assert!(!should_run(true, StreamRoundSite::Event, &pacer, now));
        assert!(should_run(
            true,
            StreamRoundSite::Event,
            &pacer,
            now + ROUND_TIME_BUDGET
        ));
    }

    #[test]
    fn about_to_wait_runs_a_deferred_round_without_waiting() {
        let now = Instant::now();
        let pacer = StreamPacer {
            last_round_end: Some(now),
        };
        assert!(should_run(
            true,
            StreamRoundSite::AboutToWait,
            &pacer,
            now + Duration::from_millis(1)
        ));
    }
}
