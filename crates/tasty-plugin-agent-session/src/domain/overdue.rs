//! 상한(`TASTY_SUPERVISOR_TURN_TIMEOUT_SECS`, 기본 600초)을 넘긴 턴을 **구독 없이도 닫는다**.
//!
//! # 왜 있나
//!
//! `send_input` 이 상한을 넘기면 그 턴은 `overdue` 로 남고, 닫는 길은 구독의 `finished` 하나뿐이었다
//! (`stream`). 그 이벤트를 놓치면 탭은 영원히 바쁨이고(`activity::is_busy` 가 대기 턴을 본다), 쌓인 말은
//! `flush_queues` 가 영영 안 보낸다. 실측 2026-10-22: surface 93 은 사용자 메시지가 14시간 갇혔다가 재개
//! 뒤 다른 턴의 끝으로 닫혀 중복 전송됐고, surface 97 은 닫힌 기록이 없이 `tell` 이 갇혔다.
//!
//! # 어떻게 닫나
//!
//! 활동 폴링이 그 세션을 **2 주기 연속** 턴 없음으로 보면(한 주기는 턴 시작 순간과 겹칠 수 있다) overdue
//! 턴을 빈 답으로 닫고, 대화 기록 꼬리(`catch_up`)로 답을 채우게 한 뒤 대기열을 보낸다. 회수된 세션도 턴이
//! 없으므로 같은 길로 바쁨이 풀린다 — 중지 단추가 듣지 않던 상태가 사라진다.

use crate::domain::convo::Convo;

/// 이 주기의 판정을 반영하고, 닫을 때가 됐으면 `true`.
///
/// `daemon_active` 는 데몬이 그 세션의 턴을 돈다고 했는가(`turn_active`)다.
pub fn note_poll(convo: &mut Convo, daemon_active: bool) -> bool {
    if convo.overdue.is_empty() || daemon_active {
        convo.overdue_idle_polls = 0;
        return false;
    }
    convo.overdue_idle_polls = convo.overdue_idle_polls.saturating_add(1);
    convo.overdue_idle_polls >= 2
}

/// overdue 턴을 모두 빈 답으로 닫고 기록 꼬리 읽기를 예약한다 — 닫은 자리를 돌려준다.
pub fn close(convo: &mut Convo) -> Vec<usize> {
    let ats = std::mem::take(&mut convo.overdue);
    for &at in &ats {
        convo.overdue.push(at);
        crate::domain::convo::finish_turn(convo, at, Ok(String::new()));
    }
    convo.overdue_idle_polls = 0;
    convo.catch_up_on_finish = true;
    // 턴 끝 판정을 연다 — 구독이 끝 이벤트를 놓친 턴이라 다른 길로는 판정이 안 돈다. 쉰 시간도 이
    // 자리부터 잰다.
    convo.sync.result_seen = true;
    ats
}

/// 다시 누름으로 치는 최소 간격 — 첫 누름의 중지가 데몬에서 도는 시간을 준다.
pub const SECOND_PRESS: std::time::Duration = std::time::Duration::from_secs(3);

/// 중지를 눌렀을 때 **탭에서 턴을 닫을지** — 데몬에 중지만 보내면 데몬이 모르는 턴(상한을 넘겨 끝 이벤트를
/// 놓친 턴, 데몬 재기동으로 사라진 턴)은 영영 안 닫힌다(실측 2026-10-23: surface 97 · 106).
///
/// 닫는다: 데몬에 도는 턴이 없다 · 기다리는 턴이 전부 상한을 넘긴 것이다 · 3초 넘어 다시 눌렀다.
pub fn should_force_stop(convo: &Convo, daemon_active: bool, now: std::time::Instant) -> bool {
    let pending: Vec<usize> = (0..convo.turns.len())
        .filter(|&i| convo.turns[i].pending)
        .collect();
    if pending.is_empty() {
        return false;
    }
    let all_overdue = pending.iter().all(|i| convo.overdue.contains(i));
    let again = convo
        .last_stop_at
        .is_some_and(|t| now.duration_since(t) >= SECOND_PRESS);
    !daemon_active || all_overdue || again
}

/// 기다리는 턴을 모두 "중지했다" 로 닫는다 — 닫은 자리를 돌려준다.
pub fn force_stop(convo: &mut Convo) -> Vec<usize> {
    let pending: Vec<usize> = (0..convo.turns.len())
        .filter(|&i| convo.turns[i].pending)
        .collect();
    for &at in &pending {
        // `finish_turn` 은 중지 표시가 있으면 실패를 "중지했다" 로 적는다 — 자리마다 다시 켠다.
        convo.stop_requested = true;
        crate::domain::convo::finish_turn(convo, at, Err(String::new()));
    }
    convo.overdue.clear();
    convo.overdue_idle_polls = 0;
    convo.stop_requested = false;
    pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convo::Turn;

    fn convo_overdue() -> Convo {
        let mut c = Convo {
            session_id: Some("s".into()),
            ..Default::default()
        };
        c.turns.push(Turn {
            sent: "긴 일".into(),
            got: String::new(),
            pending: true,
            at: None,
            error: None,
        });
        c.overdue.push(0);
        c
    }

    /// 상한을 넘겨 overdue 로 닫힌 턴도 턴 끝 판정을 연다 — 구독이 끝 이벤트를 놓친 턴이라 다른 길이 없다.
    #[test]
    fn closing_an_overdue_turn_opens_the_turn_end_judgement() {
        let mut c = convo_overdue();
        assert!(!note_poll(&mut c, false));
        assert!(note_poll(&mut c, false));
        close(&mut c);
        assert!(c.sync.result_seen);
    }

    #[test]
    fn one_idle_poll_is_not_enough_two_close_it() {
        // U-B37-1
        let mut c = convo_overdue();
        assert!(
            !note_poll(&mut c, false),
            "한 주기는 턴 시작과 겹칠 수 있다"
        );
        assert!(note_poll(&mut c, false));
        assert_eq!(close(&mut c), vec![0]);
        assert!(!c.turns[0].pending && c.overdue.is_empty() && c.catch_up_on_finish);
    }

    #[test]
    fn a_busy_poll_resets_the_count() {
        let mut c = convo_overdue();
        assert!(!note_poll(&mut c, false));
        assert!(!note_poll(&mut c, true));
        assert!(!note_poll(&mut c, false), "바쁨이 끼면 처음부터 다시 센다");
    }

    #[test]
    fn an_overdue_turn_does_not_make_later_turns_ours() {
        // U-B37-2 · U-B37-3
        let mut c = convo_overdue();
        assert!(
            !crate::domain::convo::waits_for_own_turn(&c),
            "overdue 턴만 남았으면 뒤에 시작하는 턴은 이 탭의 것이 아니다"
        );
        c.turns.push(Turn {
            sent: "다음 말".into(),
            got: String::new(),
            pending: true,
            at: None,
            error: None,
        });
        assert!(
            crate::domain::convo::waits_for_own_turn(&c),
            "overdue 가 아닌 대기 턴은 그대로 이 탭의 것"
        );
    }

    /// 함수만 시험하면 폴링 갈래에서 부르는 자리가 빠져도 초록이다 — 배선을 고정한다.
    #[test]
    fn the_poll_arm_closes_stale_overdue_turns() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        let arm = prod
            .split("fn activity_polled(")
            .nth(1)
            .expect("폴러 갈래가 없다")
            .split("\n    }\n")
            .next()
            .unwrap_or_default();
        assert!(
            arm.contains(".close_stale_overdue("),
            "폴러 갈래가 overdue 턴을 안 닫는다"
        );
    }

    #[test]
    fn stop_closes_the_tab_turn_when_the_daemon_has_none_or_on_a_second_press() {
        let now = std::time::Instant::now();
        let mut c = convo_overdue();
        assert!(
            should_force_stop(&c, true, now),
            "상한을 넘긴 턴만 남았으면 닫는다"
        );
        c.overdue.clear();
        assert!(
            should_force_stop(&c, false, now),
            "데몬에 도는 턴이 없으면 닫는다"
        );
        assert!(
            !should_force_stop(&c, true, now),
            "데몬이 돌고 첫 누름이면 중지가 끝내기를 기다린다"
        );
        c.last_stop_at = Some(now - std::time::Duration::from_secs(1));
        assert!(
            !should_force_stop(&c, true, now),
            "연타는 다시 누름이 아니다"
        );
        c.last_stop_at = Some(now - std::time::Duration::from_secs(5));
        assert!(
            should_force_stop(&c, true, now),
            "3초 넘어 다시 누르면 닫는다"
        );
        assert_eq!(force_stop(&mut c), vec![0]);
        assert!(
            !c.turns[0].pending && c.stopped.contains(&0) && c.turns[0].error.is_none(),
            "실패가 아니라 중지"
        );
        assert!(
            !should_force_stop(&c, false, now),
            "닫을 턴이 없으면 아무것도 안 한다"
        );
    }

    #[test]
    fn nothing_overdue_never_closes() {
        let mut c = Convo::default();
        assert!(!note_poll(&mut c, false));
        assert!(!note_poll(&mut c, false));
    }
}
