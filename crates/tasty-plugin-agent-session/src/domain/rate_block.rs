//! agent 프로필별 429 차단기 — 한 프로필이 429 를 받으면 그 프로필의 자동 전송을 함께 멈춘다.
//!
//! 한도는 탭이 아니라 그 프로필이 쓰는 계정(조직)에 걸린다. 그래서 한 탭이 막히면 같은 프로필의 다른
//! 탭도 막혀 있다. 탭마다 따로 다시 보내면 실패만 탭 수만큼 늘고, 짧게 끝나지 않는 차단(지출 상한 등은
//! 몇 시간 간다)에서도 같은 간격으로 계속 두드린다.
//!
//! 막히면 [`backoff`] 만큼 그 프로필의 자동 전송(429 재전송 · 유휴 · 한도 · 임계 자동 현행화)을 열지
//! 않는다. 시간이 지나면 자동 전송 **하나만** 탐침으로 내보낸다([`Gate::Probe`]). 그 턴이 성공하면 풀리고,
//! 또 429 면 기다림이 두 배가 된다. [`LONG_BLOCK`] 을 넘게 이어지면 긴 차단으로 보고 다시 보내기를
//! 그만둔다(`retry_429`). 사용자가 직접 보내는 말은 막지 않는다 — 그것도 결과를 여기 알린다.
//!
//! 저장하지 않는다 — plugin 이 다시 뜨면 빈 상태로 시작한다.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 첫 기다림 `[초기값]` — 분당 한도의 버킷이 다시 차는 폭.
pub const FIRST_WAIT: Duration = Duration::from_secs(60);

/// 기다림의 상한 `[초기값]`.
pub const MAX_WAIT: Duration = Duration::from_secs(15 * 60);

/// 다른 세션의 429 를 지금 막힌 신호로 보는 나이 — 그보다 오래된 429 뒤에 성공이 없을 뿐이면(그 프로필이
/// 그 뒤로 쉬었다) 차단을 열지 않는다 `[초기값]`.
pub const OBSERVED_FRESH: Duration = Duration::from_secs(120);

/// 이만큼 이어지면 분당 한도가 아니라 긴 차단(지출 상한 · 일 한도 · 조직 공유 소진)으로 본다 `[초기값]`.
pub const LONG_BLOCK: Duration = Duration::from_secs(15 * 60);

/// 연속 429 횟수에 따른 기다림 — 1분에서 두 배씩, [`MAX_WAIT`] 에서 멈춘다.
pub fn backoff(hits: u32) -> Duration {
    let doubled = FIRST_WAIT.saturating_mul(1u32 << hits.saturating_sub(1).min(16));
    doubled.min(MAX_WAIT)
}

/// 프로필 하나의 차단.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// 이번 차단의 첫 429.
    pub since: Instant,
    /// 연속 429 수 — 성공하면 차단째 지운다.
    pub hits: u32,
    /// 이때까지 자동 전송을 열지 않는다.
    pub until: Instant,
    /// 기다림이 끝난 뒤 탐침을 내보낸 시각 — 결과가 올 때까지 다른 자동 전송을 열지 않는다. 결과가
    /// [`MAX_WAIT`] 안에 안 오면(탭을 닫았거나 턴이 끝나지 않는다) 다시 탐침을 허락한다.
    pub probing: Option<Instant>,
}

/// 자동 전송을 지금 열어도 되나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// 막히지 않았다.
    Open,
    /// 기다림이 끝났다 — 하나만 탐침으로 보낸다([`Blocks::take_probe`]).
    Probe,
    /// 기다리는 중이거나 탐침 결과를 기다린다.
    Closed,
}

/// 프로필 이름 → 차단.
#[derive(Debug, Default)]
pub struct Blocks {
    by_profile: HashMap<String, Block>,
    /// 마지막으로 풀린 시각 — 그보다 앞선 429 관측으로 다시 닫지 않는다.
    cleared: HashMap<String, Instant>,
}

/// 다른 세션의 결과를 반영한 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    Unchanged,
    Opened,
    Released,
}

impl Blocks {
    /// 그 프로필의 턴이 429 로 끝났다 — 차단을 열거나 기다림을 늘린다.
    pub fn note_rejected(&mut self, profile: &str, now: Instant) -> &Block {
        let b = self
            .by_profile
            .entry(profile.to_string())
            .and_modify(|b| b.hits += 1)
            .or_insert(Block {
                since: now,
                hits: 1,
                until: now,
                probing: None,
            });
        b.until = now + backoff(b.hits);
        b.probing = None;
        b
    }

    /// 그 프로필의 턴이 API 에 닿았다(성공) — 차단을 푼다. 풀렸으면 그 차단을 돌려준다.
    pub fn note_reached(&mut self, profile: &str, now: Instant) -> Option<Block> {
        self.cleared.insert(profile.to_string(), now);
        self.by_profile.remove(profile)
    }

    /// 같은 프로필의 **다른 세션**(다른 탭 · 터미널의 claude · 하위 에이전트)이 낸 마지막 성공 · 429
    /// 시각을 반영한다(`rate_meter`). 이 탭의 턴만 보면 그 프로필이 이미 막힌 것을 모른 채 자동 전송을 연다.
    ///
    /// - 막혀 있을 때 차단 시작 뒤의 성공이 마지막 결과면 푼다.
    /// - 안 막혔을 때 마지막 결과가 [`OBSERVED_FRESH`] 안의 429 이고 마지막 해제 뒤의 것이면 연다. 측정은
    ///   주기마다 늦게 오므로, 해제 직전의 오래된 429 로 방금 푼 차단을 다시 닫지 않는다.
    pub fn note_observed(
        &mut self,
        profile: &str,
        last_rejected: Option<Instant>,
        last_ok: Option<Instant>,
        now: Instant,
    ) -> Observed {
        let ok_last = last_ok.is_some_and(|o| last_rejected.is_none_or(|r| o > r));
        if let Some(b) = self.by_profile.get(profile) {
            if ok_last && last_ok.is_some_and(|o| o > b.since) {
                self.note_reached(profile, now);
                return Observed::Released;
            }
            return Observed::Unchanged;
        }
        let Some(r) = last_rejected else {
            return Observed::Unchanged;
        };
        let fresh = now.saturating_duration_since(r) < OBSERVED_FRESH;
        let after_clear = self.cleared.get(profile).is_none_or(|c| r > *c);
        if !ok_last && fresh && after_clear {
            self.note_rejected(profile, now);
            return Observed::Opened;
        }
        Observed::Unchanged
    }

    pub fn gate(&self, profile: &str, now: Instant) -> Gate {
        match self.by_profile.get(profile) {
            None => Gate::Open,
            Some(b)
                if now >= b.until
                    && b.probing
                        .is_none_or(|t| now.saturating_duration_since(t) >= MAX_WAIT) =>
            {
                Gate::Probe
            }
            Some(_) => Gate::Closed,
        }
    }

    /// 지금 자동 전송을 하나 열어도 되나 — [`Gate::Probe`] 면 그것을 탐침으로 적는다.
    pub fn admit(&mut self, profile: &str, now: Instant) -> bool {
        match self.gate(profile, now) {
            Gate::Open => true,
            Gate::Probe => {
                self.take_probe(profile, now);
                true
            }
            Gate::Closed => false,
        }
    }

    /// 방금 연 자동 전송을 탐침으로 적는다 — 막히지 않은 프로필이면 아무것도 안 한다.
    pub fn take_probe(&mut self, profile: &str, now: Instant) {
        if let Some(b) = self.by_profile.get_mut(profile) {
            b.probing = Some(now);
        }
    }

    pub fn get(&self, profile: &str) -> Option<&Block> {
        self.by_profile.get(profile)
    }

    /// [`LONG_BLOCK`] 을 넘게 막혀 있다.
    pub fn is_long(&self, profile: &str, now: Instant) -> bool {
        self.by_profile
            .get(profile)
            .is_some_and(|b| now.saturating_duration_since(b.since) >= LONG_BLOCK)
    }
}

/// 탭이 쓰는 agent 프로필 — 아직 모르면 빈 이름 하나로 묶는다.
pub fn profile_of(c: &crate::domain::convo::Convo) -> &str {
    c.agent_name.as_deref().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> Instant {
        thread_local!(static BASE: Instant = Instant::now());
        BASE.with(|b| *b + Duration::from_secs(secs))
    }

    #[test]
    fn the_wait_doubles_from_a_minute_up_to_fifteen() {
        let mins: Vec<u64> = (1..=6).map(|h| backoff(h).as_secs() / 60).collect();
        assert_eq!(mins, vec![1, 2, 4, 8, 15, 15]);
        assert_eq!(backoff(u32::MAX), MAX_WAIT);
    }

    /// 막힌 프로필만 닫힌다 — 기다림이 끝나면 하나만 탐침으로 열리고, 그 결과가 올 때까지 닫힌다.
    #[test]
    fn a_rejection_closes_only_that_profile_and_lets_one_probe_through() {
        let mut b = Blocks::default();
        b.note_rejected("secloud", at(0));
        assert_eq!(b.gate("secloud", at(30)), Gate::Closed);
        assert_eq!(b.gate("dev", at(30)), Gate::Open, "다른 프로필까지 닫았다");
        assert!(
            b.admit("secloud", at(60)),
            "기다림이 끝났는데 탐침을 안 연다"
        );
        assert!(!b.admit("secloud", at(61)), "탐침을 둘 열었다");
        b.note_rejected("secloud", at(62));
        assert_eq!(b.get("secloud").map(|x| x.until), Some(at(62 + 120)));
        assert_eq!(b.gate("secloud", at(62 + 119)), Gate::Closed);
        assert_eq!(b.gate("secloud", at(62 + 120)), Gate::Probe);
        b.take_probe("secloud", at(182));
        assert_eq!(b.gate("secloud", at(182 + 899)), Gate::Closed);
        assert_eq!(
            b.gate("secloud", at(182 + 900)),
            Gate::Probe,
            "결과가 안 오는 탐침이 프로필을 영영 닫았다"
        );
        assert!(b.note_reached("secloud", at(1_100)).is_some());
        assert_eq!(b.gate("secloud", at(200)), Gate::Open, "닿았는데 안 풀렸다");
    }

    /// 다른 세션의 결과 — 최근 429 가 마지막이면 열고, 차단 뒤 성공이 보이면 푼다. 오래된 429 · 해제 앞의
    /// 429 로는 다시 닫지 않는다.
    #[test]
    fn other_sessions_open_and_release_a_block() {
        let mut b = Blocks::default();
        assert_eq!(
            b.note_observed("p", Some(at(10)), Some(at(5)), at(20)),
            Observed::Opened
        );
        assert_eq!(b.gate("p", at(21)), Gate::Closed);
        assert_eq!(
            b.note_observed("p", Some(at(10)), Some(at(5)), at(22)),
            Observed::Unchanged,
            "막힌 채인데 바꿨다"
        );
        assert_eq!(
            b.note_observed("p", Some(at(10)), Some(at(25)), at(30)),
            Observed::Released
        );
        assert_eq!(
            b.note_observed("p", Some(at(10)), Some(at(5)), at(31)),
            Observed::Unchanged,
            "늦게 온 측정의 옛 429 로 방금 푼 차단을 다시 닫았다"
        );
        let mut c = Blocks::default();
        assert_eq!(
            c.note_observed("q", Some(at(0)), None, at(120)),
            Observed::Unchanged,
            "2분 지난 429 로 닫았다"
        );
        assert_eq!(
            c.note_observed("q", Some(at(100)), Some(at(110)), at(115)),
            Observed::Unchanged,
            "성공이 마지막인데 닫았다"
        );
        c.note_rejected("q", at(200));
        assert_eq!(
            c.note_observed("q", Some(at(150)), Some(at(160)), at(210)),
            Observed::Unchanged,
            "차단보다 앞선 성공으로 풀었다"
        );
    }

    #[test]
    fn a_block_is_long_after_fifteen_minutes_from_its_first_rejection() {
        let mut b = Blocks::default();
        b.note_rejected("p", at(0));
        b.note_rejected("p", at(600));
        assert!(!b.is_long("p", at(899)));
        assert!(b.is_long("p", at(900)));
        assert!(!b.is_long("q", at(900)));
    }
}
