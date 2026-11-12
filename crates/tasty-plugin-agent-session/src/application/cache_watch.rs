//! 대화 탭의 **프롬프트 캐시 재작성 감지**(설정 `cache_watch`, `docs/surfaces/agent-session/index.md` § 설정).
//!
//! 턴 첫 호출이 대화 전체를 캐시에 다시 쓰면, 그 턴은 캐시 읽기(×0.1) 대신 캐시 쓰기(×1.25 · ×2)
//! 를 낸다. 분당 입력 한도(ITPM)도 그 쓰기를 센다. 원인은 셋으로 갈린다.
//!
//! | 분류 | 조건 | 내보내는 것 |
//! |---|---|---|
//! | `Ttl` | 대량 재작성 && 쉰 시간 > 직전 캐시 수명 | 지표 `cache_rewrite_ttl` 만 — 사용자는 쉰 것을 안다 |
//! | `Prefix` | 대량 재작성 && 쉰 시간 ≤ 캐시 수명 | 지표 `cache_rewrite_prefix` + 상태줄 토큰 + 알림 줄 |
//! | `TierDown` | 직전 1시간 캐시 → 이번 첫 호출이 5분 캐시만 | 지표 `cache_tier_down` + 알림 줄(세션당 1회) |
//!
//! 대량 재작성 = 첫 호출의 캐시 쓰기가 [`MIN_REWRITE`] 이상이고 직전 턴 컨텍스트의
//! [`MIN_SHARE`] 이상이며, 그 호출의 캐시 읽기는 직전 컨텍스트의 [`MIN_SHARE`] 미만. 읽기가 크면
//! 대화를 다시 쓴 것이 아니라 큰 새 내용(붙여 넣은 글 등)을 더한 것이다 — 실측 대량 재작성 102건은
//! 모두 읽기가 직전 컨텍스트의 31% 이하였다. 기준값은 dev 프로필 4일 실측(7,086 호출: 대량 재작성 100 = TTL 97 ·
//! 접두부 3, 1h→5m 강등 13 — 그중 6건이 인증 변수가 빠진 데몬 재기동 직후)에서 왔다.
//!
//! 판정하지 않는 턴: 탭 첫 턴 · plugin 재기동 직후(직전 값이 없다) · compact 뒤 첫 턴 ·
//! 비우기 뒤(새 대화). 지속 측정의 정본은 세션 대화 기록이다 — 텔레메트리는 개수 상한에서
//! 밀린다. 이 모듈은 실시간 고지만 맡는다.

use std::time::Duration;

use crate::ports::host::HostCall;
use crate::ports::host::SendHost;

use crate::domain::i18n;

/// 대량 재작성으로 볼 첫 호출 캐시 쓰기의 하한(토큰).
pub const MIN_REWRITE: u64 = 50_000;
/// 직전 턴 컨텍스트 대비 비율의 하한.
pub const MIN_SHARE: f64 = 0.5;
/// 설정 키 — 끄면 알림만 끄고 지표는 남긴다.
pub const SETTING_KEY: &str = "cache_watch";
pub const DEFAULT_ON: bool = true;

const TTL_5M: Duration = Duration::from_secs(300);
const TTL_1H: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ttl,
    Prefix,
    TierDown,
}

impl Kind {
    pub fn metric(self) -> &'static str {
        match self {
            Self::Ttl => "cache_rewrite_ttl",
            Self::Prefix => "cache_rewrite_prefix",
            Self::TierDown => "cache_tier_down",
        }
    }
}

/// 접두부가 깨진 까닭으로 짐작되는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    Model,
    Command(String),
    Unknown,
}

/// 직전 턴의 값 — 판정은 이번 턴과 이것을 견준다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prev {
    pub context: Option<u64>,
    pub one_hour: Option<bool>,
    pub model: Option<String>,
}

/// 이번 턴의 값.
#[derive(Debug, Clone, Default)]
pub struct Now<'a> {
    /// 이번 턴 끝의 컨텍스트 — 다음 턴의 기준이 된다.
    pub context: Option<u64>,
    pub first_write: Option<u64>,
    pub first_read: Option<u64>,
    pub first_1h: Option<bool>,
    pub model: Option<&'a str>,
    /// 직전 턴 끝에서 이번 턴 첫 호출까지. 모르면 `None` — 그 턴은 판정하지 않는다.
    pub gap: Option<Duration>,
    /// 이번 턴을 연 말이 슬래시 명령이면 그 이름(`/model` 등).
    pub command: Option<&'a str>,
}

/// 탭 하나의 감지 상태.
#[derive(Debug, Default)]
pub struct State {
    pub prev: Prev,
    /// 다음 턴은 판정하지 않는다(compact · 비우기 뒤 — 대화가 바뀌었다).
    pub skip_next: bool,
    /// 상태줄에 보일 직전 접두부 재작성(다음 판정에서 지운다).
    pub shown: Option<u64>,
    /// 이번 턴을 연 슬래시 명령(`begin_turn` 이 적고 판정이 쓴다).
    pub command: Option<String>,
}

/// 첫 호출의 쓰기 판정 — 대량 재작성이면 `Ttl`·`Prefix` 중 하나.
pub fn classify(prev: &Prev, now: &Now) -> Option<Kind> {
    let (Some(ctx), Some(write), Some(gap)) = (prev.context, now.first_write, now.gap) else {
        return None;
    };
    let read = now.first_read.unwrap_or(0);
    if write < MIN_REWRITE
        || (write as f64) < MIN_SHARE * ctx as f64
        || (read as f64) >= MIN_SHARE * ctx as f64
    {
        return None;
    }
    let ttl = if prev.one_hour == Some(true) {
        TTL_1H
    } else {
        TTL_5M
    };
    Some(if gap > ttl { Kind::Ttl } else { Kind::Prefix })
}

/// 1시간 캐시를 쓰던 세션이 5분 캐시로 내려갔나.
pub fn tier_down(prev: &Prev, now: &Now) -> bool {
    prev.one_hour == Some(true) && now.first_1h == Some(false)
}

pub fn cause(prev: &Prev, now: &Now) -> Cause {
    if let (Some(a), Some(b)) = (prev.model.as_deref(), now.model)
        && a != b
    {
        return Cause::Model;
    }
    match now.command {
        Some(c)
            if matches!(
                c,
                "/model" | "/reload-plugins" | "/reload-skills" | "/add-dir"
            ) =>
        {
            Cause::Command(c.to_string())
        }
        _ => Cause::Unknown,
    }
}

/// 턴 하나를 판정하고 상태를 옮긴다. 돌려주는 것은 이번 턴에 난 일(지표·알림 대상).
pub fn observe(s: &mut State, now: &Now) -> Vec<(Kind, u64)> {
    let mut out = Vec::new();
    s.shown = None;
    if std::mem::take(&mut s.skip_next) {
        // 대화가 바뀌었다 — 직전 값을 버리고 이번 턴부터 다시 잰다.
        s.prev = Prev::default();
    } else {
        if let Some(k) = classify(&s.prev, now) {
            let w = now.first_write.unwrap_or(0);
            if k == Kind::Prefix {
                s.shown = Some(w);
            }
            out.push((k, w));
        }
        // 한 번 알리면 직전 수명이 5분이 되어, 1시간으로 돌아왔다 다시 내려가기 전에는 또 걸리지 않는다.
        if tier_down(&s.prev, now) {
            out.push((Kind::TierDown, now.first_write.unwrap_or(0)));
        }
    }
    // 다음 턴의 기준. 캐시를 새로 안 쓴 턴은 수명 판단을 바꾸지 않는다 — 수명을 관측하지
    // 못한 턴이라 직전 관측을 둔다.
    s.prev.context = now.context.or(s.prev.context);
    if now.first_1h.is_some() {
        s.prev.one_hour = now.first_1h;
    }
    if let Some(m) = now.model {
        s.prev.model = Some(m.to_string());
    }
    s.command = None;
    out
}

/// 알림 줄 문구.
fn note_text(kind: Kind, tokens: u64, cause: &Cause) -> String {
    let k = (tokens / 1000).to_string();
    match kind {
        Kind::TierDown => i18n::t1("sup.cache.tier_down", &k),
        _ => {
            let why = match cause {
                Cause::Model => i18n::t("sup.cache.cause_model"),
                Cause::Command(c) => i18n::t1("sup.cache.cause_command", c),
                Cause::Unknown => i18n::t("sup.cache.cause_unknown"),
            };
            i18n::tn("sup.cache.prefix_break", &[&k, &why])
        }
    }
}

/// 직전 턴 끝부터 이번 턴 첫 호출까지. 직전 턴 끝을 모르면(탭 첫 턴 · plugin 재기동 직후) `None`.
pub fn gap_since(
    last_result: Option<std::time::Instant>,
    now: std::time::Instant,
    turn_ms: Option<u64>,
) -> Option<Duration> {
    last_result.map(|l| {
        now.duration_since(l)
            .saturating_sub(Duration::from_millis(turn_ms.unwrap_or(0)))
    })
}

/// 알림 줄에 낼 문구 — 강등·접두부 깨짐만 알린다(사용자 결정 2026-10-26). TTL 만료는 쉰 것을
/// 사용자가 알므로 지표만 남긴다. 한 턴에 둘이 겹치면 뒤의 것(강등)을 낸다.
pub fn told(events: &[(Kind, u64)], why: &Cause) -> Option<String> {
    events
        .iter()
        .rev()
        .find(|(k, _)| *k != Kind::Ttl)
        .map(|&(k, t)| note_text(k, t, why))
}

/// 이번 슬래시 명령의 이름 — 첫 낱말.
pub fn command_of(text: &str) -> Option<String> {
    let t = text.trim_start();
    t.starts_with('/')
        .then(|| t.split_whitespace().next().unwrap_or(t).to_string())
}

fn notify_on<H: HostCall>(host: &H) -> bool {
    host.call(
        "settings.get_plugin_setting",
        serde_json::json!({ "storage_key": SETTING_KEY }),
    )
    .ok()
    .and_then(|v| v.get("value").and_then(serde_json::Value::as_bool))
    .unwrap_or(DEFAULT_ON)
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 턴 끝(`judge_after_result`) — 그 턴의 첫 호출을 판정하고 지표·알림을 낸다. `gap` 은 직전
    /// 턴 끝부터 이번 턴 첫 호출까지(모르면 `None`).
    pub(crate) fn watch_cache(&mut self, host: &impl SendHost, sid: u32, gap: Option<Duration>) {
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let command = c.cache.command.clone();
        let model = c.usage.model.clone();
        let now = Now {
            context: c.usage.context_used,
            first_write: c.usage.first_write,
            first_read: c.usage.first_read,
            first_1h: c.usage.first_1h,
            model: model.as_deref(),
            gap,
            command: command.as_deref(),
        };
        let why = cause(&c.cache.prev, &now);
        let was_shown = c.cache.shown.is_some();
        let events = observe(&mut c.cache, &now);
        let h = host;
        if events.is_empty() {
            if was_shown {
                self.activity.refresh_status(&h, sid, c);
            }
            return;
        }
        for &(kind, tokens) in &events {
            if let Err(e) = crate::application::surface_link::record_metric(
                &h,
                sid,
                kind.metric(),
                tokens as f64,
            ) {
                tracing::debug!("[agent_session] surface={sid}: 캐시 지표 실패: {e:?}");
            }
            tracing::info!(
                "[agent_session] surface={sid}: 캐시 재작성 {kind:?} {tokens} 토큰 ({why:?})"
            );
        }
        let told = told(&events, &why);
        let on = notify_on(&h);
        if !on {
            c.cache.shown = None;
        }
        if let Some(note) = told
            && on
        {
            crate::application::session_surface::patch(
                &h,
                sid,
                &crate::application::clear::note_msg(Some(&note)),
            );
        }
        if was_shown || c.cache.shown.is_some() {
            self.activity.refresh_status(&h, sid, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    fn prev(ctx: u64, one_hour: Option<bool>) -> Prev {
        Prev {
            context: Some(ctx),
            one_hour,
            model: Some("m".into()),
        }
    }

    fn now(write: u64, gap: Duration) -> Now<'static> {
        Now {
            context: Some(write + 10_000),
            first_write: Some(write),
            first_read: Some(0),
            first_1h: None,
            model: Some("m"),
            gap: Some(gap),
            command: None,
        }
    }

    #[test]
    fn a_big_rewrite_after_the_cache_lifetime_is_ttl_and_within_it_is_a_prefix_break() {
        // U1 — 5분 캐시, 10분 쉼
        assert_eq!(
            classify(&prev(600_000, Some(false)), &now(550_000, 10 * MIN)),
            Some(Kind::Ttl)
        );
        // U2 — 30초 만에 대량 재작성
        assert_eq!(
            classify(
                &prev(600_000, Some(false)),
                &now(550_000, Duration::from_secs(30))
            ),
            Some(Kind::Prefix)
        );
        // 1시간 캐시는 59분 쉼도 수명 안이다
        assert_eq!(
            classify(&prev(600_000, Some(true)), &now(550_000, 59 * MIN)),
            Some(Kind::Prefix)
        );
        assert_eq!(
            classify(&prev(600_000, Some(true)), &now(550_000, 61 * MIN)),
            Some(Kind::Ttl)
        );
        // 모르는 수명은 5분으로 본다
        assert_eq!(
            classify(&prev(600_000, None), &now(550_000, 6 * MIN)),
            Some(Kind::Ttl)
        );
    }

    #[test]
    fn small_or_partial_writes_are_not_rewrites() {
        // U5 — 하한 바로 아래 둘
        assert_eq!(classify(&prev(80_000, None), &now(49_999, MIN)), None);
        assert_eq!(classify(&prev(200_000, None), &now(98_000, MIN)), None);
        // 경계는 포함한다
        assert_eq!(
            classify(&prev(100_000, None), &now(MIN_REWRITE, MIN)),
            Some(Kind::Prefix)
        );
        // 직전 값이나 쉰 시간을 모르면 판정하지 않는다
        let mut n = now(550_000, MIN);
        n.gap = None;
        assert_eq!(classify(&prev(600_000, None), &n), None);
        assert_eq!(classify(&Prev::default(), &now(550_000, MIN)), None);
        // 직전 대화를 그대로 읽고 큰 새 내용을 더한 턴(붙여 넣은 글)은 재작성이 아니다
        let mut paste = now(120_000, MIN);
        paste.first_read = Some(1_000);
        assert_eq!(classify(&prev(1_200, None), &paste), None);
        paste.first_read = Some(599);
        assert_eq!(classify(&prev(1_200, None), &paste), Some(Kind::Prefix));
    }

    #[test]
    fn the_cause_names_a_model_switch_or_the_command_that_opened_the_turn() {
        // U3
        let mut n = now(550_000, MIN);
        assert_eq!(cause(&prev(600_000, None), &n), Cause::Unknown);
        n.command = Some("/model");
        assert_eq!(
            cause(&prev(600_000, None), &n),
            Cause::Command("/model".into())
        );
        n.command = Some("/cost");
        assert_eq!(cause(&prev(600_000, None), &n), Cause::Unknown);
        n.model = Some("other");
        assert_eq!(cause(&prev(600_000, None), &n), Cause::Model);
        assert_eq!(
            command_of("  /reload-skills now"),
            Some("/reload-skills".into())
        );
        assert_eq!(command_of("hello /model"), None);
    }

    #[test]
    fn a_tier_drop_is_told_once_until_the_session_is_back_on_one_hour() {
        // U4
        let mut s = State {
            prev: prev(100_000, Some(true)),
            ..State::default()
        };
        let mut n = now(1_000, MIN);
        n.first_1h = Some(false);
        assert_eq!(observe(&mut s, &n), vec![(Kind::TierDown, 1_000)]);
        // 계속 5분 — 다시 안 알린다(이전이 이제 5분이라 강등도 아니다)
        assert!(observe(&mut s, &n).is_empty());
        // 1시간으로 돌아왔다가 다시 내려가면 다시 알린다
        let mut up = now(1_000, MIN);
        up.first_1h = Some(true);
        assert!(observe(&mut s, &up).is_empty());
        assert_eq!(observe(&mut s, &n), vec![(Kind::TierDown, 1_000)]);
        // 캐시를 안 쓴 턴은 수명 판단을 안 바꾼다
        s.prev.one_hour = Some(true);
        let mut quiet = now(0, MIN);
        quiet.first_1h = None;
        observe(&mut s, &quiet);
        assert_eq!(s.prev.one_hour, Some(true));
    }

    #[test]
    fn the_turn_after_a_compact_or_clear_is_not_judged() {
        // U6
        let mut s = State {
            prev: prev(800_000, Some(false)),
            skip_next: true,
            ..State::default()
        };
        assert!(observe(&mut s, &now(700_000, Duration::from_secs(5))).is_empty());
        assert!(!s.skip_next);
        // 그 다음 턴부터는 새 기준으로 잰다
        assert_eq!(
            observe(&mut s, &now(700_000, Duration::from_secs(5))),
            vec![(Kind::Prefix, 700_000)]
        );
        assert_eq!(s.shown, Some(700_000));
        // 조용한 턴은 상태줄 표시를 지운다
        assert!(observe(&mut s, &now(10, Duration::from_secs(5))).is_empty());
        assert_eq!(s.shown, None);
    }

    /// 재생 수용 — 2026-10-23 05:12~05:45Z 의 인증 변수 상실 구간을 숫자만 옮긴 것. 세션 여섯이
    /// 1시간 캐시로 돌다가 데몬 재기동 뒤 첫 호출부터 5분 캐시를 썼다.
    #[test]
    fn the_recorded_auth_loss_window_is_told_as_six_tier_drops() {
        let sessions = [
            (612_000_u64, 31_000_u64),
            (380_000, 12_000),
            (701_000, 40_000),
            (95_000, 2_000),
            (455_000, 9_000),
            (220_000, 4_000),
        ];
        let mut told = 0;
        for (ctx, write) in sessions {
            let mut s = State {
                prev: prev(ctx, Some(true)),
                ..State::default()
            };
            let mut n = now(write, 2 * MIN);
            n.first_1h = Some(false);
            told += observe(&mut s, &n)
                .iter()
                .filter(|(k, _)| *k == Kind::TierDown)
                .count();
        }
        assert_eq!(told, 6);
    }

    #[test]
    fn only_tier_drops_and_prefix_breaks_are_told() {
        assert_eq!(told(&[(Kind::Ttl, 400_000)], &Cause::Unknown), None);
        assert!(told(&[(Kind::Prefix, 400_000)], &Cause::Model).is_some());
        assert!(
            told(
                &[(Kind::Ttl, 400_000), (Kind::TierDown, 1)],
                &Cause::Unknown
            )
            .is_some()
        );
        assert_eq!(told(&[], &Cause::Unknown), None);
    }

    /// 배선 — 턴 끝 판정이 감지를 부르고, compact 뒤·비우기 턴을 건너뛰게 하고, 탭이 연 말의
    /// 명령을 적고, 설정이 꺼지면 상태줄 표시를 지운다. 소스 본문으로 본다(호스트 없이 도는 자리라).
    #[test]
    fn the_detector_is_wired_into_the_turn_end_and_the_turn_start() {
        // 메서드는 들여쓴 `}` 로, 최상위 함수는 첫 칸 `}` 로 끝난다.
        let body = |src: &str, f: &str| {
            let i = src.find(f).unwrap_or_else(|| panic!("{f} 없음"));
            let end = if f.starts_with("pub fn") {
                "\n}\n"
            } else {
                "\n    }\n"
            };
            src[i..i + src[i..].find(end).unwrap_or(src.len() - i)].to_string()
        };
        let judge = body(include_str!("sync_compact.rs"), "fn judge_after_result(");
        assert!(judge.contains("self.watch_cache(host, sid, gap);"));
        assert!(judge.contains("if c.sync.verify || c.clearing {"));
        // 쉰 시간은 직전 턴 끝을 덮어쓰기 전에 잰다.
        assert!(
            judge.find("gap_since(").unwrap()
                < judge.find("c.sync.last_result = Some(now);").unwrap()
        );
        let begin = body(include_str!("../domain/convo.rs"), "pub fn begin_turn(");
        assert!(
            begin.contains(
                "convo.cache.command = crate::application::cache_watch::command_of(text);"
            )
        );
        let watch = body(include_str!("cache_watch.rs"), "pub(crate) fn watch_cache(");
        assert!(watch.contains("c.cache.shown = None;"));
        assert!(watch.contains("&& on"));
        let line = include_str!("../adapters/driving/statusline.rs");
        assert!(line.contains("let cache = convo.cache.shown.map(|_| {"));
        assert!(line.contains("l.extend(sync_and_cache_tokens(convo));"));
    }

    #[test]
    fn the_gap_is_the_rest_after_the_turn_itself() {
        let t0 = std::time::Instant::now();
        let later = t0 + Duration::from_secs(600);
        // 턴 끝이 600초 전, 이번 턴이 그중 30초 걸렸다 → 쉼 570초
        assert_eq!(
            gap_since(Some(t0), later, Some(30_000)),
            Some(Duration::from_secs(570))
        );
        assert_eq!(gap_since(None, later, Some(1)), None);
        assert_eq!(
            gap_since(Some(later), later, Some(5_000)),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn manifest_default_matches_the_code() {
        let m = include_str!("../../tasty-plugin.toml");
        let block = m
            .split("[[contributes.settings_pages.items]]")
            .find(|b| b.contains(&format!("storage_key = \"{SETTING_KEY}\"")))
            .expect("cache_watch setting");
        assert!(block.contains("kind = \"toggle\""), "기타 묶음의 토글이다");
        assert!(block.contains("group = \"etc\""), "기타 묶음에 든다");
        assert!(block.contains(&format!("default = {DEFAULT_ON}")));
    }
}
