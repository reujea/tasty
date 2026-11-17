//! ping 무응답 판정. 시각을 모두 인자로 받는 순수 함수라 sleep 없이 시험한다.
//!
//! 기준은 "답하지 않은 가장 오래된 ping을 보낸 뒤의 경과"다. 마지막 수신 뒤의 실제 경과를
//! 세면 메인 루프가 멈춰 ping을 보내지 못한 시간까지 무응답으로 센다. 메인 루프가 멈춘 동안
//! 호스트 응답을 기다리느라 ping에 답하지 못한 plugin도 있으므로, 정지가 풀린 pump는 미응답
//! 기한을 그 시각으로 다시 잡고 판정하지 않는다. 설계는 `docs/dev-guide/timer-hub.md`의
//! "plugin 주기 작업과 healthcheck 검출 상한" 절에 있다.

use std::time::{Duration, Instant};

use super::super::{MAIN_LOOP_STALL_GAP, PluginManager};

impl PluginManager {
    /// 직전 pump와 이번 pump의 간격이 [`MAIN_LOOP_STALL_GAP`] 이상이면 메인 루프가 멈췄던
    /// 것으로 보고 모든 plugin의 미응답 기한을 `now`로 다시 잡는다. 정지였으면 true.
    pub(in crate::manager) fn reanchor_after_main_loop_stall(&mut self, now: Instant) -> bool {
        let prev = self
            .last_pump_at
            .replace(self.last_pump_at.map_or(now, |p| p.max(now)));
        if !is_main_loop_stall(prev, now, MAIN_LOOP_STALL_GAP) {
            return false;
        }
        let gap = prev.map_or(Duration::ZERO, |p| now.saturating_duration_since(p));
        tracing::warn!(
            gap_ms = gap.as_millis() as u64,
            "plugin pump resumed after a main loop stall — ping deadlines restart from now"
        );
        for proc in self.processes.values_mut() {
            let owed = owed_since_after_stall(proc.ping_owed_since(), proc.last_pong_at(), now);
            proc.set_ping_owed_since(owed);
        }
        true
    }
}

/// 직전 pump 시각(`prev`)부터 `now`까지 `gap` 이상 벌어졌는가. 첫 pump는 정지가 아니다.
pub(super) fn is_main_loop_stall(prev: Option<Instant>, now: Instant, gap: Duration) -> bool {
    prev.is_some_and(|p| now.saturating_duration_since(p) >= gap)
}

/// 정지가 풀린 시각의 미응답 ping 송신 시각. 답하지 않은 ping이 있으면 기한을 `now`부터
/// 다시 센다. 이미 답했거나 기다리는 ping이 없으면 그대로 둔다.
pub(super) fn owed_since_after_stall(
    owed_since: Option<Instant>,
    last_pong: Instant,
    now: Instant,
) -> Option<Instant> {
    match owed_since {
        Some(sent) if last_pong < sent => Some(sent.max(now)),
        other => other,
    }
}

/// 방금 발화한 반복 타이머의 격자 시각. 발화 뒤 다음 예정 시각(`next_due`)에서 한 주기를 뺀다.
///
/// 루프가 깨어난 시각(`now`)은 격자보다 몇 ms 늦고 그 흔들림은 tick마다 다르다. 송신 시각과
/// 판정 시각을 `now`로 잡으면 송신 쪽이 더 늦게 깨어났을 때 60초 비교가 한 tick 밀린다.
/// 격자 시각은 `now`를 넘지 않게 자른다.
pub(super) fn grid_time(next_due: Instant, interval: Option<Duration>, now: Instant) -> Instant {
    interval
        .and_then(|i| next_due.checked_sub(i))
        .map_or(now, |t| t.min(now))
}

/// ping을 보낸 뒤의 미응답 ping 송신 시각.
///
/// 앞선 ping 뒤로 수신이 있었거나 기다리는 ping이 없으면 이번 송신 시각(`now`)으로 새로 잡는다.
/// 아직 답하지 않았으면 가장 오래된 송신 시각을 유지한다.
pub(super) fn owed_since_after_ping(
    owed_since: Option<Instant>,
    last_pong: Instant,
    now: Instant,
) -> Option<Instant> {
    match owed_since {
        Some(sent) if last_pong < sent => Some(sent),
        _ => Some(now),
    }
}

/// 미응답 ping이 있고, 그 송신 뒤 수신이 없으며, 송신부터 `timeout` 이상 지났는가.
///
/// 첫 ping 전(`None`)은 판정하지 않는다. 연결 단계의 실패는 핸드셰이크 기한이 따로 처리한다.
pub(super) fn is_unresponsive(
    owed_since: Option<Instant>,
    last_pong: Instant,
    now: Instant,
    timeout: Duration,
) -> bool {
    match owed_since {
        Some(sent) => last_pong < sent && now.saturating_duration_since(sent) >= timeout,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(60);

    fn base() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_ping_after_an_answer_restarts_the_owed_clock() {
        let b = base();
        let s = b + Duration::from_secs(15);
        let pong = b + Duration::from_secs(16);
        let resume = b + Duration::from_secs(225);
        assert_eq!(owed_since_after_ping(Some(s), pong, resume), Some(resume));
        assert_eq!(owed_since_after_ping(None, b, s), Some(s));
    }

    #[test]
    fn an_unanswered_ping_keeps_the_oldest_send_time() {
        let b = base();
        let s = b + Duration::from_secs(30);
        let pong = b + Duration::from_secs(16);
        assert_eq!(
            owed_since_after_ping(Some(s), pong, b + Duration::from_secs(45)),
            Some(s)
        );
    }

    #[test]
    fn the_stall_resume_tick_is_not_a_timeout() {
        let b = base();
        let pong = b + Duration::from_secs(16);
        let resume = b + Duration::from_secs(225);
        let owed = owed_since_after_ping(Some(b + Duration::from_secs(15)), pong, resume);
        assert!(!is_unresponsive(owed, pong, resume, T));
    }

    #[test]
    fn the_timeout_is_inclusive_and_counted_from_the_send() {
        let b = base();
        let pong = b;
        let s = b + Duration::from_secs(1);
        assert!(!is_unresponsive(
            Some(s),
            pong,
            s + T - Duration::from_millis(1),
            T
        ));
        assert!(is_unresponsive(Some(s), pong, s + T, T));
    }

    #[test]
    fn an_answer_after_the_send_clears_the_timeout() {
        let b = base();
        let s = b + Duration::from_secs(1);
        assert!(!is_unresponsive(Some(s), s, s + T * 2, T));
    }

    #[test]
    fn the_grid_time_is_one_interval_before_the_next_due() {
        let b = base();
        let i = Duration::from_secs(15);
        let woke = b + Duration::from_millis(40);
        assert_eq!(grid_time(b + i, Some(i), woke), b);
        assert_eq!(grid_time(b + i, None, woke), woke);
        // 격자 시각은 now를 넘지 않는다.
        assert_eq!(grid_time(b + i * 3, Some(i), b), b);
    }

    #[test]
    fn only_a_gap_of_the_stall_length_is_a_stall() {
        let b = base();
        let g = Duration::from_secs(30);
        assert!(!is_main_loop_stall(None, b + g * 10, g));
        assert!(!is_main_loop_stall(
            Some(b),
            b + g - Duration::from_millis(1),
            g
        ));
        assert!(is_main_loop_stall(Some(b), b + g, g));
        // 시각이 거꾸로 와도 정지로 보지 않는다.
        assert!(!is_main_loop_stall(Some(b + g), b, g));
    }

    #[test]
    fn a_stall_moves_only_an_unanswered_ping_to_now() {
        let b = base();
        let s = b + Duration::from_secs(30);
        let resume = b + Duration::from_secs(120);
        assert_eq!(
            owed_since_after_stall(Some(s), b, resume),
            Some(resume),
            "답하지 않은 ping의 기한을 정지 뒤로 옮기지 않았다"
        );
        assert_eq!(owed_since_after_stall(Some(s), s, resume), Some(s));
        assert_eq!(owed_since_after_stall(None, b, resume), None);
    }

    #[test]
    fn no_ping_yet_is_never_a_timeout() {
        let b = base();
        assert!(!is_unresponsive(None, b, b + T * 10, T));
    }
}
