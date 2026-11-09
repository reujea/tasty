//! 분당 한도(429)로 거절된 턴을 다시 보낸다(차단 · 탐침 규칙은 `docs/surfaces/agent-session/index.md` § 목적·지원 범위).
//!
//! 429 는 API 가 요청을 **처리하기 전에** 거절한 것이라, 같은 말을 다시 보내도 일이 두 번 되지
//! 않는다. 막힌 말은 사용자가 보낸 말과 같은 길(`send_queue::take_or_queue` → `send_turn`)로 다시
//! 보낸다. 언제 보낼지는 그 탭의 agent 프로필 차단기(`rate_block`)가 정한다 — 같은 프로필의 탭들은
//! 한 계정의 한도를 함께 쓰므로, 차단기가 허락한 탐침 하나만 나가고 나머지는 그 결과를 기다린다.
//! 차단이 [`crate::domain::rate_block::LONG_BLOCK`] 을 넘으면 분당 한도가 아닌 긴 차단으로 보고 멈춘 뒤
//! 사용자에게 넘긴다.
//!
//! 다시 보내지 않는 것: 중지한 턴 · 현행화 턴(compact 순서가 그 턴에 걸려 있다) · 429 가 아닌 실패.
//! 상한을 넘긴 턴(`overdue`)은 실패가 아니라 열린 턴이라 여기 오지 않는다.

use std::time::{Duration, Instant};

use crate::domain::i18n;

/// 다시 보내기까지 기다리는 최소 시간 — 분당 한도의 버킷이 다시 차는 폭. 그 뒤는 차단기가 정한다.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);

/// 실패 사유에서 분당 한도 거절을 알아보는 표지 — Claude Code 가 내는 오류 문면이다.
const MARKER: &str = "Request rejected (429)";

/// 현행화 턴의 꼬리표 머리(`sync_compact::sync_input`) — 그 턴은 다시 보내지 않는다.
const SYNC_TAG: &str = "[sync-compact:";

/// 탭 하나의 재전송 상태. 저장하지 않는다 — plugin 이 다시 뜨면 예약도 사라진다.
#[derive(Debug, Default)]
pub struct State {
    /// 다시 보낼 말과 그 시각.
    pub pending: Option<Pending>,
    /// 지금 말을 몇 번 다시 보냈나 — 턴이 잘 끝나면 0 으로 돌아간다.
    pub tries: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub text: String,
    pub due: Instant,
}

/// 턴 하나가 끝난 뒤 할 일.
#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    Ignore,
    /// 다시 보낸다 — 몇 번째인가.
    Retry(u8),
    /// 프로필 차단이 길어 멈춘다.
    GiveUp,
}

/// 분당 한도 거절인가.
pub fn is_rate_limited(error: &str) -> bool {
    error.contains(MARKER)
}

/// 턴 끝을 반영한다 — 다시 보낼지 여기서 정한다. `long` 은 그 프로필의 차단이 긴 차단이 됐는가.
pub fn after_turn(
    s: &mut State,
    sent: Option<&str>,
    got: &Result<String, String>,
    stopped: bool,
    syncing: bool,
    long: bool,
    now: Instant,
) -> Plan {
    let error = match got {
        Ok(_) => {
            s.tries = 0;
            return Plan::Ignore;
        }
        Err(e) => e,
    };
    if stopped || syncing || !is_rate_limited(error) {
        return Plan::Ignore;
    }
    let Some(text) = sent.filter(|t| !t.trim().is_empty() && !t.contains(SYNC_TAG)) else {
        return Plan::Ignore;
    };
    if long {
        s.tries = 0;
        s.pending = None;
        return Plan::GiveUp;
    }
    s.tries += 1;
    s.pending = Some(Pending {
        text: text.to_string(),
        due: now + RETRY_AFTER,
    });
    Plan::Retry(s.tries)
}

/// 다시 보낼 때가 됐나.
pub fn due(s: &State, now: Instant) -> bool {
    s.pending.as_ref().is_some_and(|p| now >= p.due)
}

/// 때가 된 재전송 하나를 지금 어떻게 할까.
#[derive(Debug, PartialEq, Eq)]
pub enum Due {
    /// 보낸다(차단 중이었으면 이것이 탐침이다).
    Send,
    /// 그 프로필이 막혀 있거나 다른 탭의 탐침 결과를 기다린다 — 예약을 그대로 둔다.
    Wait,
    /// 긴 차단 — 예약을 버리고 사용자에게 넘긴다.
    GiveUp,
}

/// 때가 된 재전송을 그 탭의 프로필 차단기에 묻는다. `Send` 면 탐침으로 적힌다.
pub fn on_due(blocks: &mut crate::domain::rate_block::Blocks, profile: &str, now: Instant) -> Due {
    if blocks.is_long(profile, now) {
        Due::GiveUp
    } else if blocks.admit(profile, now) {
        Due::Send
    } else {
        Due::Wait
    }
}

/// 알림 문구에 쓰는 프로필 이름 — 모르면 `?`.
fn shown(profile: &str) -> &str {
    if profile.is_empty() { "?" } else { profile }
}

/// 대기 중 알림 문구 — 다음 확인까지 남은 초.
pub(crate) fn waiting_note(profile: &str, wait: Duration) -> String {
    i18n::tn(
        "sup.retry.waiting",
        &[shown(profile), &wait.as_secs().max(1).to_string()],
    )
}

/// 긴 차단으로 멈출 때의 문구 — 막힌 지 몇 분째인가.
pub(crate) fn gave_up_note(profile: &str, blocked: Duration) -> String {
    i18n::tn(
        "sup.retry.gave_up",
        &[shown(profile), &(blocked.as_secs() / 60).to_string()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convo::Convo;

    const REJECTED: &str = "턴이 실패로 끝났다: success (출력: 먼저 경로를 모읍니다.\n\nAPI Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later.)";

    fn at(secs: u64) -> Instant {
        thread_local!(static BASE: Instant = Instant::now());
        BASE.with(|b| *b + Duration::from_secs(secs))
    }

    #[test]
    fn only_a_rate_limit_rejection_counts() {
        assert!(is_rate_limited(REJECTED));
        assert!(!is_rate_limited("socket closed"));
        assert!(!is_rate_limited("API Error: 500 Internal Server Error"));
    }

    #[test]
    fn a_rejected_turn_is_retried_until_the_block_turns_long() {
        let mut s = State::default();
        let err = Err(REJECTED.to_string());
        assert_eq!(
            after_turn(&mut s, Some("일"), &err, false, false, false, at(0)),
            Plan::Retry(1)
        );
        assert_eq!(
            s.pending,
            Some(Pending {
                text: "일".into(),
                due: at(0) + RETRY_AFTER
            })
        );
        assert_eq!(
            after_turn(&mut s, Some("일"), &err, false, false, false, at(61)),
            Plan::Retry(2)
        );
        assert_eq!(
            after_turn(&mut s, Some("일"), &err, false, false, false, at(182)),
            Plan::Retry(3),
            "횟수로 멈췄다 — 차단이 길어질 때만 멈춘다"
        );
        assert_eq!(
            after_turn(&mut s, Some("일"), &err, false, false, true, at(900)),
            Plan::GiveUp
        );
        assert_eq!(s.pending, None);
        assert_eq!(s.tries, 0);
    }

    #[test]
    fn a_turn_that_ends_well_resets_the_count() {
        let mut s = State::default();
        let err = Err(REJECTED.to_string());
        after_turn(&mut s, Some("일"), &err, false, false, false, at(0));
        assert_eq!(s.tries, 1);
        after_turn(
            &mut s,
            Some("일"),
            &Ok("답".into()),
            false,
            false,
            false,
            at(61),
        );
        assert_eq!(s.tries, 0);
    }

    #[test]
    fn stopped_sync_and_other_failures_are_not_retried() {
        let err = Err(REJECTED.to_string());
        let mut s = State::default();
        assert_eq!(
            after_turn(&mut s, Some("일"), &err, true, false, false, at(0)),
            Plan::Ignore
        );
        assert_eq!(
            after_turn(&mut s, Some("일"), &err, false, true, false, at(0)),
            Plan::Ignore
        );
        let tagged = "프로젝트 현행화\n\n[sync-compact:idle] …";
        assert_eq!(
            after_turn(&mut s, Some(tagged), &err, false, false, false, at(0)),
            Plan::Ignore
        );
        let other = Err("socket closed".to_string());
        assert_eq!(
            after_turn(&mut s, Some("일"), &other, false, false, false, at(0)),
            Plan::Ignore
        );
        assert_eq!(
            after_turn(&mut s, None, &err, false, false, false, at(0)),
            Plan::Ignore
        );
        assert_eq!(s.pending, None);
    }

    #[test]
    fn the_retry_waits_the_full_minute() {
        let mut s = State::default();
        after_turn(
            &mut s,
            Some("일"),
            &Err(REJECTED.into()),
            false,
            false,
            false,
            at(0),
        );
        assert!(!due(&s, at(59)));
        assert!(due(&s, at(60)));
    }

    /// 사람이 새 말을 보내면 예약을 버린다 — 직접 다시 보냈을 수 있다.
    #[test]
    fn a_new_message_cancels_the_pending_retry() {
        let mut c = Convo::default();
        after_turn(
            &mut c.retry,
            Some("일"),
            &Err(REJECTED.into()),
            false,
            false,
            false,
            at(0),
        );
        crate::domain::convo::begin_turn(&mut c, "다시 쓴 말");
        assert_eq!(c.retry.pending, None);
    }

    /// 같은 프로필의 재전송은 차단 중 하나만 탐침으로 나가고, 풀리면 모두 나가며, 길어지면 멈춘다.
    #[test]
    fn due_retries_go_one_probe_at_a_time_per_profile() {
        use crate::domain::rate_block::Blocks;
        let mut b = Blocks::default();
        assert_eq!(
            on_due(&mut b, "a", at(0)),
            Due::Send,
            "막히지 않았는데 붙든다"
        );
        b.note_rejected("a", at(0));
        assert_eq!(on_due(&mut b, "a", at(30)), Due::Wait);
        assert_eq!(
            on_due(&mut b, "b", at(30)),
            Due::Send,
            "다른 프로필까지 붙든다"
        );
        assert_eq!(on_due(&mut b, "a", at(60)), Due::Send);
        assert_eq!(on_due(&mut b, "a", at(60)), Due::Wait, "탐침을 둘 보냈다");
        b.note_reached("a", at(61));
        assert_eq!(on_due(&mut b, "a", at(61)), Due::Send);
        assert_eq!(
            on_due(&mut b, "a", at(61)),
            Due::Send,
            "풀렸는데 하나만 보낸다"
        );
        b.note_rejected("a", at(100));
        assert_eq!(on_due(&mut b, "a", at(100 + 900)), Due::GiveUp);
    }

    /// 턴 끝이 예약을 부르고, 폴링이 사용자의 말과 같은 길로 보낸다.
    #[test]
    fn the_turn_end_and_the_poll_are_wired() {
        let main = crate::test_support::wiring_source();
        let prod = main.split("#[cfg(test)]").next().unwrap_or(main);
        assert!(prod.contains("self.note_turn_for_retry(sid, sent.as_deref(), &got, stopped);"));
        assert!(prod.contains("self.run_pending_retries();"));
        // 차단기는 현행화 턴의 결과까지 본다 — 그 턴을 거르는 판정(`syncing`)보다 앞에서 적는다.
        let src = include_str!("../application/retry_resend.rs");
        let note = src
            .split("fn note_turn_for_retry(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .expect("note_turn_for_retry 가 없다");
        let stamp = note
            .find("self.blocks.note_rejected(&profile, now)")
            .expect("429 를 차단기에 안 알린다");
        assert!(
            stamp < note.find("let syncing").unwrap_or(0),
            "현행화 턴의 429 를 뺀다"
        );
        assert!(
            note.contains("self.blocks.note_reached(&profile, now)"),
            "성공해도 차단을 안 푼다"
        );
        let src = include_str!("../application/retry_resend.rs");
        let body = src
            .split("fn run_pending_retries(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .expect("run_pending_retries 가 없다");
        assert!(
            body.contains("send_queue::take_or_queue("),
            "대기열 길을 안 거친다"
        );
        assert!(body.contains("self.send_turn(&host, sid, &text, None);"));
        assert!(body.contains("!c.sync.running()"), "현행화 중에 끼어든다");
        let ask = body
            .find("on_due(&mut self.blocks, &profile, now)")
            .expect("차단기를 안 거친다");
        assert!(
            ask < body.find("c.retry.pending.take()").unwrap_or(0),
            "막힌 프로필의 말을 꺼낸 뒤에 묻는다"
        );
        assert!(
            body.contains("Due::Wait => continue,"),
            "기다리라는데 보낸다"
        );
    }
}
