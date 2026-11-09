//! **이 탭이 연 턴**의 표 — 구독(`stream`)이 본 턴이 이 탭의 것인지 가른다.
//!
//! # 왜 표인가
//!
//! 종전에는 구독의 `started` 가 올 때 "이 탭에 기다리는 턴이 있나" 를 보고 판정했다. 그런데
//! 구독 이벤트와 탭의 턴 끝 알림은 **서로 다른 워커**가 plugin 루프로 되던지므로 처리 순서가
//! 보장되지 않는다. 턴이 짧으면 탭이 그 턴을 이미 닫은 **뒤에** `started` 가 처리되고, 그 순간
//! 기다리는 턴이 없어 남의 턴으로 판정된다 — 그 답이 "에이전트가 이어서" 턴으로 **한 번 더**
//! 붙었다(실측 2026-10-19, macOS 가상머신 기능 시험 F37: `echo: keep-0` 이 두 번).
//!
//! 데몬의 진행 이벤트에는 턴을 가리키는 값이 없다(`started`·`output`·`finished` 뿐). 그래서
//! **순서 대신 개수**로 가른다: 탭이 턴을 열 때 표를 하나 쌓고, 구독의 `started` 는 아직 짝이
//! 없는 가장 오래된 표를 가져가면 이 탭의 턴이다. 에이전트는 입력을 하나씩 처리하므로 표와
//! 턴의 순서가 같다.
//!
//! # 표가 남는 경우
//!
//! 구독이 그 턴을 아예 못 보면(구독이 붙기 전에 돈 턴) 표가 남아 다음의 **진짜** 남의 턴을
//! 삼킨다. 그래서 **닫힌 지 [`GRACE`] 가 지난 표는 버린다** — 구독의 지연은 그보다 훨씬 짧고,
//! 에이전트가 스스로 이어 가는 턴은 그보다 늦게 온다.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 닫힌 표를 이만큼 더 기다린다 — 늦게 처리되는 `started` 가 짝을 찾을 시간.
pub const GRACE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default)]
struct Ticket {
    claimed: bool,
    closed_at: Option<Instant>,
}

/// 이 탭이 연 턴의 표(오래된 순).
#[derive(Debug, Clone, Default)]
pub struct OwnTurns {
    tickets: VecDeque<Ticket>,
}

impl OwnTurns {
    /// 탭이 턴을 열었다(보낸 말 · 맥락 턴).
    pub fn opened(&mut self) {
        self.tickets.push_back(Ticket::default());
    }

    /// 탭이 턴 하나를 닫았다 — 아직 안 닫힌 가장 오래된 표를 닫는다.
    pub fn closed(&mut self, now: Instant) {
        if let Some(t) = self.tickets.iter_mut().find(|t| t.closed_at.is_none()) {
            t.closed_at = Some(now);
        }
        self.purge(now);
    }

    /// 구독이 턴의 시작을 봤다 — 짝이 없는 가장 오래된 표를 가져가면 `true`(이 탭의 턴).
    pub fn claim(&mut self, now: Instant) -> bool {
        self.purge(now);
        match self.tickets.iter_mut().find(|t| !t.claimed) {
            Some(t) => {
                t.claimed = true;
                self.purge(now);
                true
            }
            None => false,
        }
    }

    /// 끝난 표(짝을 찾고 닫힘)와 기한이 지난 표(닫힌 지 [`GRACE`] 가 지났는데 짝이 없음)를 버린다.
    fn purge(&mut self, now: Instant) {
        self.tickets.retain(|t| match t.closed_at {
            None => true,
            Some(at) => !t.claimed && now.duration_since(at) < GRACE,
        });
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.tickets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 정상 순서 — 시작이 먼저, 닫힘이 나중. 한 턴에 표 하나가 쓰이고 사라진다.
    #[test]
    fn a_turn_seen_before_it_closes_is_ours() {
        let now = Instant::now();
        let mut o = OwnTurns::default();
        o.opened();
        assert!(o.claim(now));
        o.closed(now);
        assert_eq!(o.len(), 0);
        assert!(!o.claim(now), "다음 남의 턴까지 우리 것으로 본다");
    }

    /// 경합 — 탭이 턴을 닫은 **뒤에** 시작이 처리돼도 이 탭의 턴이다(F37 이 드러낸 순서).
    #[test]
    fn a_start_processed_after_the_tab_closed_the_turn_is_still_ours() {
        let now = Instant::now();
        let mut o = OwnTurns::default();
        o.opened();
        o.closed(now);
        assert!(o.claim(now + Duration::from_millis(50)));
        assert!(
            !o.claim(now + Duration::from_millis(60)),
            "표 하나로 턴 둘을 가져갔다"
        );
    }

    /// 구독이 못 본 턴의 표는 기한이 지나면 버린다 — 진짜 남의 턴을 삼키지 않는다.
    #[test]
    fn a_ticket_nobody_claimed_expires() {
        let now = Instant::now();
        let mut o = OwnTurns::default();
        o.opened();
        o.closed(now);
        assert!(!o.claim(now + GRACE + Duration::from_millis(1)));
    }

    /// 탭이 연 턴이 없으면 시작은 남의 턴이다.
    #[test]
    fn with_no_open_turn_a_start_is_someone_elses() {
        assert!(!OwnTurns::default().claim(Instant::now()));
    }

    /// 연달아 연 턴은 순서대로 짝을 찾는다 — 닫힘이 시작보다 앞서도.
    #[test]
    fn several_turns_pair_in_order() {
        let now = Instant::now();
        let mut o = OwnTurns::default();
        o.opened();
        o.opened();
        o.closed(now);
        assert!(o.claim(now));
        o.closed(now);
        assert!(o.claim(now));
        assert!(!o.claim(now));
        assert_eq!(o.len(), 0);
    }
}
