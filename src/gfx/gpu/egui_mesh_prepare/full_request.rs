//! 전체 텍스처 재전송 요청의 간격.
//!
//! 텍스처 delta 연결이 끊기면 전체 프레임을 다시 받아야 한다. 받는 쪽은 요청을 받으면
//! "전체 프레임을 보내야 함" 표시만 세우므로 같은 요청을 반복해도 응답이 빨라지지 않는다.
//! 매 프레임 다시 보내면 상대가 바쁠수록 요청이 더 쌓인다. 그래서 첫 요청은 바로 보내고,
//! 응답이 없을 때만 간격을 늘려 가며 다시 보낸다. 재요청은 이전 요청을 취소하지 않는다.

use std::time::{Duration, Instant};

/// 첫 재요청까지의 간격.
const FIRST_RETRY: Duration = Duration::from_millis(500);
/// 재요청 간격의 상한.
const MAX_RETRY: Duration = Duration::from_secs(5);

/// mesh 대상 하나의 재전송 요청 상태. 전체 프레임을 받으면 [`Self::reset`]한다.
#[derive(Debug)]
pub(super) struct FullRequestPacer {
    next_at: Option<Instant>,
    interval: Duration,
}

impl Default for FullRequestPacer {
    fn default() -> Self {
        Self {
            next_at: None,
            interval: FIRST_RETRY,
        }
    }
}

impl FullRequestPacer {
    /// 지금 요청을 보내도 되면 `Ok`, 아니면 다음 요청까지 남은 시간을 `Err`로 돌려준다.
    pub(super) fn admit(&mut self, now: Instant) -> Result<(), Duration> {
        if let Some(at) = self.next_at
            && now < at
        {
            return Err(at - now);
        }
        self.next_at = Some(now + self.interval);
        self.interval = (self.interval * 2).min(MAX_RETRY);
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_request_goes_out_immediately_and_retries_back_off() {
        let mut p = FullRequestPacer::default();
        let t0 = Instant::now();
        assert_eq!(p.admit(t0), Ok(()));
        assert_eq!(
            p.admit(t0 + Duration::from_millis(100)),
            Err(Duration::from_millis(400))
        );
        let mut at = t0 + FIRST_RETRY;
        let mut gaps = Vec::new();
        for _ in 0..6 {
            assert_eq!(p.admit(at), Ok(()));
            let Err(wait) = p.admit(at) else {
                panic!("a second request at the same instant")
            };
            gaps.push(wait.as_millis());
            at += wait;
        }
        assert_eq!(gaps, vec![1000, 2000, 4000, 5000, 5000, 5000]);
    }

    #[test]
    fn reset_lets_the_next_break_request_at_once() {
        let mut p = FullRequestPacer::default();
        let t0 = Instant::now();
        assert_eq!(p.admit(t0), Ok(()));
        p.reset();
        assert_eq!(p.admit(t0 + Duration::from_millis(1)), Ok(()));
    }
}
