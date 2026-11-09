//! 상태줄의 두 유휴 시계 — 캐시 시계와 휴식 시계(왜 둘인지는 `docs/surfaces/agent-session/index.md` § 목적·지원 범위).
//!
//! - **캐시 시계**(`cache:`): API 를 부른 마지막 턴 끝(`SyncState::last_result`)에서 캐시 수명까지 남은
//!   시간. 0 이 지나면 다음 말은 대화 전체를 캐시에 다시 쓴다.
//! - **휴식 시계**(`rest:`): 사람이 마지막으로 손댄 때(`sync_compact::rest_since`)에서 회수까지 남은 시간.
//!   회수 설정이 꺼졌거나 이미 회수된 세션에는 없다.
//!
//! 둘 다 **만료 예정 시각**(`HH:MM:ss`)으로 보인다 — 편 줄의 셋째 줄 끝과 접은 줄 끝. 서버는 기한(유닉스
//! 초)만 싣고 페이지가 현지 시각으로 그린다(plugin 은 시간대를 모른다). 60초 전 · 지난 뒤의 색은 시간이
//! 흐르며 바뀌므로 페이지가 1초마다 다시 그린다. 턴이 도는 동안에는 싣지 않는다 — 턴 끝에서 두 시계가 다시
//! 감긴다.

use std::time::Instant;

use serde_json::{Value, json};

use crate::adapters::driven::store::agents::CacheTtl;
use crate::application::sync_compact::{Knobs, cache_ttl, idle_wait, rest_since};
use crate::domain::convo::Convo;
use crate::ports::host::SendHost;

/// 시계를 그리는 데 드는 설정과 마지막으로 그린 기한. 저장하지 않는다.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ClockState {
    /// 그 프로필의 `cache_ttl` — 관측값이 없을 때 수명을 정한다.
    pub ttl: Option<CacheTtl>,
    /// 유휴 회수가 켜졌나 — 꺼졌으면 휴식 시계가 없다.
    pub reclaim: bool,
    /// 마지막으로 상태줄에 실은 두 기한 — 바뀌면 다시 그린다([`note`]).
    shown: Option<Deadlines>,
}

/// (캐시 만료, 회수) 기한.
type Deadlines = (Option<Instant>, Option<Instant>);

/// 지금 값으로 본 두 기한.
pub fn deadlines(c: &Convo) -> Deadlines {
    let ttl = c.sync.clock.ttl;
    let cache = c.sync.last_result.map(|t| t + cache_ttl(&c.usage, ttl));
    let rest = rest_since(c)
        .filter(|_| c.sync.clock.reclaim && !c.sync.reclaimed)
        .map(|t| t + idle_wait(&c.usage, ttl));
    (cache, rest)
}

/// 설정을 싣고, 기한이 마지막으로 그린 것과 다르면 `true`(다시 그릴 때다).
///
/// 휴식 시계는 탭 조작 · 명령으로도 밀리는데 그 자리들은 상태줄을 다시 그리지 않는다 — 판정 고리가
/// 지날 때마다 여기서 견준다.
pub fn note(c: &mut Convo, ttl: Option<CacheTtl>, reclaim: bool) -> bool {
    c.sync.clock.ttl = ttl;
    c.sync.clock.reclaim = reclaim;
    let now = Some(deadlines(c));
    if c.sync.clock.shown == now {
        return false;
    }
    c.sync.clock.shown = now;
    true
}

/// `at` 을 유닉스 초로 — 지난 시각이면 `now_unix` 보다 작다.
fn unix_of(at: Instant, now: Instant, now_unix: i64) -> i64 {
    let ms = if at >= now {
        at.duration_since(now).as_millis() as i64
    } else {
        -(now.duration_since(at).as_millis() as i64)
    };
    now_unix + (ms as f64 / 1000.0).round() as i64
}

/// 상태줄 토큰 — 턴이 도는 동안은 없다.
pub fn tokens(c: &Convo, busy: bool, now: Instant, now_unix: i64) -> Vec<Value> {
    if busy {
        return Vec::new();
    }
    let (cache, rest) = deadlines(c);
    // `over` 는 기한이 지난 뒤의 색 — 캐시 만료는 다음 말의 비용이 오르므로 `bad`, 회수 대기는 `wait`.
    let tok =
        |at: Instant, label: &'static str, hint: &'static str, done: &'static str, over: &str| {
            json!({
                "t": "clock",
                "label": crate::domain::i18n::t(label),
                "ts": unix_of(at, now, now_unix),
                "title": crate::domain::i18n::t(hint),
                "done": crate::domain::i18n::t(done),
                "over": over,
            })
        };
    let mut l = Vec::new();
    if let Some(at) = cache {
        l.push(tok(
            at,
            "sup.line.cache",
            "sup.status.cache_clock_hint",
            "sup.status.cache_expired",
            "bad",
        ));
    }
    if let Some(at) = rest {
        l.push(tok(
            at,
            "sup.line.rest",
            "sup.status.rest_clock_hint",
            "sup.status.rest_due",
            "wait",
        ));
    }
    l
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 설정 묶음을 탭마다 싣고, 시계 기한이 바뀐 탭의 상태줄을 다시 그린다(`flush_queues`).
    pub(crate) fn note_clocks(&mut self, host: &impl SendHost, knobs: &Knobs) {
        for (&sid, c) in self.convos.iter_mut() {
            let ttl = knobs.ttl_of(c);
            if note(c, ttl, knobs.reclaim) {
                self.activity.refresh_status(host, sid, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> Instant {
        // 테스트 시계의 원점 — 모든 시각을 여기서 잰다.
        static ZERO: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *ZERO.get_or_init(Instant::now) + Duration::from_secs(secs)
    }

    /// 캐시 시계는 턴 끝에서, 휴식 시계는 손댄 때에서 잰다 — 손대면 휴식 시계만 밀린다.
    #[test]
    fn the_two_clocks_run_from_their_own_origins() {
        let mut c = Convo::default();
        c.sync.last_result = Some(at(0));
        c.sync.clock.reclaim = true;
        assert_eq!(deadlines(&c), (Some(at(300)), Some(at(240))));
        c.sync.touch(at(100));
        assert_eq!(deadlines(&c), (Some(at(300)), Some(at(340))));
        c.sync.clock.ttl = Some(CacheTtl::OneHour);
        assert_eq!(deadlines(&c), (Some(at(3600)), Some(at(100 + 3540))));
        c.usage.cache_1h = Some(false);
        assert_eq!(deadlines(&c).0, Some(at(300)), "관측값이 프로필을 이긴다");
    }

    #[test]
    fn the_rest_clock_needs_reclaim_on_and_a_live_session() {
        let mut c = Convo::default();
        c.sync.last_result = Some(at(0));
        assert_eq!(deadlines(&c).1, None, "회수가 꺼졌는데 휴식 시계가 있다");
        c.sync.clock.reclaim = true;
        c.sync.reclaimed = true;
        assert_eq!(deadlines(&c).1, None, "회수된 세션에 휴식 시계가 있다");
        assert_eq!(deadlines(&Convo::default()), (None, None));
    }

    /// 기한이 바뀔 때만 다시 그린다 — 판정 고리가 돌 때마다 그리지 않는다.
    #[test]
    fn note_redraws_only_when_a_deadline_moves() {
        let mut c = Convo::default();
        c.sync.last_result = Some(at(0));
        assert!(note(&mut c, None, true));
        assert!(!note(&mut c, None, true));
        c.sync.touch(at(50));
        assert!(note(&mut c, None, true), "손댄 것이 안 그려진다");
        assert!(note(&mut c, Some(CacheTtl::OneHour), true));
        assert!(note(&mut c, Some(CacheTtl::OneHour), false));
        assert!(!note(&mut c, Some(CacheTtl::OneHour), false));
    }

    #[test]
    fn tokens_carry_unix_deadlines_and_hide_while_busy() {
        let mut c = Convo::default();
        c.sync.last_result = Some(at(0));
        c.sync.clock.reclaim = true;
        let l = tokens(&c, false, at(60), 1_000);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0]["t"], "clock");
        assert_eq!(
            (l[0]["over"].as_str(), l[1]["over"].as_str()),
            (Some("bad"), Some("wait"))
        );
        assert_eq!(l[0]["ts"], 1_240, "캐시 만료 = 1000 + (300 - 60)");
        assert_eq!(l[1]["ts"], 1_180, "회수 = 1000 + (240 - 60)");
        assert!(tokens(&c, true, at(60), 1_000).is_empty());
        assert_eq!(tokens(&c, false, at(400), 1_000)[0]["ts"], 900, "지난 기한");
    }

    /// 판정 고리가 시계를 싣는다 — 안 부르면 설정이 바뀌어도 시계가 그대로다.
    #[test]
    fn the_flush_loop_notes_the_clocks() {
        let src = crate::test_support::wiring_source();
        let body = src
            .split("fn flush_queues(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .expect("flush_queues 가 없다");
        assert!(body.contains("self.note_clocks(&host, &knobs);"));
        let page = include_str!("../../assets/app.js");
        assert!(page.contains("case 'clock':"), "페이지가 시계를 못 그린다");
        assert!(page.contains("hasClock()"), "페이지가 시계를 안 센다");
    }
}
