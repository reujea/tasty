//! 대화 탭의 **현행화 → compact** — 컨텍스트가 커졌을 때 세션을 정리한다(동작 계약은
//! `docs/surfaces/agent-session/index.md` § 목적·지원 범위 · § 설정).
//!
//! # 무엇을 하나
//!
//! 1. 현행화 문구(기본 `프로젝트 현행화`)를 **보이는 사용자 턴**으로 보낸다 — 에이전트의
//!    현행화 스킬이 상태 문서와 재개 지점을 갱신한다.
//! 2. 그 턴이 **성공으로 끝났을 때만** `/compact {고정 프롬프트}` 를 제어 턴으로 보낸다 —
//!    화면에 턴으로 안 내고(`/clear` 와 같은 길), 끝은 [`METHOD`] 로 온다.
//!
//! # 언제 도나
//!
//! - **수동**: IPC `agent_session.sync_compact` · CLI `tasty agent-session sync-compact`.
//! - **자동**: 설정 묶음 스위치 `sync_threshold_on` 이 켜져 있고 `sync_threshold_tokens` 가 0 이 아니고, 턴 끝의 컨텍스트가 그 토큰 수(창의 90% 를
//!   넘지 않게 끌어내린 값 — [`effective_threshold`])를 **넘는 순간**. 바로 시작하지 않고 그 탭이
//!   [`AUTO_QUIET`] 동안 조용할 때 시작한다 — 일하는 중에 끼어들지 않으려는 것이고, 기준을
//!   [`AUTO_URGENT_OVER`] 넘게 넘으면 다음 판정에서 바로 시작한다. 가드는 [`judge`] 에 모았다 —
//!   넘을 때 1회 · 쿨다운 · compact 뒤에도 넘으면 해제(바닥값: 시스템 프롬프트와 도구 정의는
//!   compact 로 안 준다) · 실패 뒤 재시도 안 함 · 내려가면 다시 장전.
//! - **유휴**(설정 이름 「세션 캐시 현행화」): 설정 묶음 스위치 `idle_sync_on` 이 켜져 있고 `idle_sync_tokens` 가 0 이 아니고, **캐시 시계**가 [`idle_wait`](캐시 수명 − 1분)를
//!   넘었고 점유가 그 토큰 수 이상일 때 — 그 세션의 프롬프트 캐시가 식기 전에 정리한다. 쉼 하나에 한
//!   번이고, 사람이 다음 말을 보내야 다시 열린다([`ready_for_idle`]).
//! - **유휴 회수**: 설정 「세션 최적화」(`release_hidden_webview`, 기본 켬)이면 **휴식 시계**가 같은 기한을 넘었을 때 세션의 claude 를
//!   내린다 — 기준 이상이면 위 현행화 → compact 가 끝난 뒤, 미만이거나 유휴 현행화가 꺼졌으면 바로([`idle_step`] ·
//!   `idle_reclaim`). 다음 입력이 `--resume` 으로 다시 띄운다.
//!
//! # 두 시계
//!
//! | 시계 | 칸 | 미는 것 | 보는 판정 |
//! |---|---|---|---|
//! | 캐시 | `last_result` | API 를 부른 턴의 끝(현행화 턴 포함) | 유휴 현행화 |
//! | 휴식 | `last_activity` | 사람이 연 턴의 시작·끝, 탭 조작, 세션을 겨눈 명령 | 유휴 회수 |
//!
//! 서버 캐시는 API 호출만 늘린다 — `/clear` · 폴더 추가 같은 로컬 명령으로 현행화 기한을 밀면 현행화가
//! 식은 캐시에 대화 전체를 다시 쓴다. 회수는 사람이 쉬는지만 보면 되므로 명령도 민다.
//! 캐시 수명은 관측값(턴의 캐시 쓰기) → 프로필 `cache_ttl` → 5분 순으로 고른다([`idle_wait`]).
//!
//! 판정이 끝나는 곳은 `flush_queues` 의 끝([`crate::application::plugin::SupervisorPlugin::run_pending_syncs`])이다 —
//! 턴 끝 · 원문 구독 · 2초 폴링이 모두 거기를 지나므로, 2분을 넘긴 현행화 턴(구독으로 끝이
//! 오는 턴)도 같은 자리에서 잡힌다.

use crate::domain::sync_rules::Mode;
use std::time::{Duration, Instant};

use crate::application::app_error::AppError;
use serde_json::{Value, json};

use crate::application::session_surface;
use crate::domain::convo::Convo;
use crate::domain::usage::Usage;
use crate::ports::host::HostCall;
use crate::ports::host::SendHost;
use crate::{application::clear, domain::i18n};

/// 워커가 compact 턴을 끝내고 되던지는 자기 호출.
pub const METHOD: &str = "agent_session.sync_compact_done";

/// 설정 키 — 매니페스트 `[[contributes.settings_pages.items]]` 의 `storage_key` 와 같아야 한다
/// (`manifest_defaults_match_the_code` 가 본다).
pub const THRESHOLD_KEY: &str = "sync_threshold_tokens";
pub const PROMPT_KEY: &str = "sync_prompt";
pub const COMPACT_KEY: &str = "compact_prompt";
pub const IDLE_KEY: &str = "idle_sync_tokens";
/// 유휴 회수 — 「세션 최적화」 스위치 하나가 회수와 가려진 탭의 화면 내리기(`view_state`)를 함께 켠다.
/// 저장 키는 호스트가 화면을 내릴 때 읽는 이름이다.
pub const RECLAIM_KEY: &str = crate::application::view_state::SAVER_KEY;
/// 설정 창 묶음 머리의 스위치 — 끄면 그 기준을 0(끔)으로 읽는다. 기준 칸의 0 도 계속 끔이다.
pub const THRESHOLD_ON_KEY: &str = "sync_threshold_on";
pub const IDLE_ON_KEY: &str = "idle_sync_on";
/// 컨텍스트 묶음 머리 스위치 — 끄면 두 현행화(작업 중 · 유휴)를 모두 끈다. 기능 스위치 값은 남는다.
pub const CONTEXT_ON_KEY: &str = "context_on";
pub const DEFAULT_CONTEXT_ON: bool = true;

/// 기본값 — 매니페스트의 `default` 와 같아야 한다(`manifest_defaults_match_the_code`).
pub const DEFAULT_THRESHOLD_TOKENS: f64 = 400_000.0;
pub const DEFAULT_IDLE_TOKENS: f64 = 150_000.0;
pub const DEFAULT_RECLAIM: bool = true;
pub const DEFAULT_THRESHOLD_ON: bool = true;
pub const DEFAULT_IDLE_ON: bool = true;

/// 유휴 기준의 위 끝 — 가장 큰 창(1M)의 두 배면 막을 일이 없다.
pub const IDLE_TOKENS_MAX: f64 = 2_000_000.0;
/// 기준 칸의 아래 끝. 끄는 것은 묶음 스위치뿐이라 1 미만(예전의 "0 = 끔")은 기본값으로 읽는다.
pub const IDLE_TOKENS_MIN: f64 = 1.0;
/// 기본 현행화 문구 — 받는 쪽 스킬의 트리거 문구라 로케일을 따른다.
pub fn default_sync_prompt() -> String {
    crate::domain::i18n::t("sup.default.sync_prompt")
}
/// 기본 compact 초점 지시문 — 모델에게 가는 문구라 로케일을 따른다.
pub fn default_compact_prompt() -> String {
    crate::domain::i18n::t("sup.default.compact_prompt")
}

/// 턴 끝 기준이 창의 이 비율을 넘지 않게 끌어내린다 — 창보다 큰 기준은 영영 안 넘고, 창에 붙이면
/// Claude Code 의 자체 compact 가 먼저 돈다.
pub const WINDOW_CAP: f64 = 0.9;

/// 턴 끝 기준을 넘은 탭은 이만큼 조용할 때 현행화를 연다 — 일하는 중에 끼어들지 않는다.
pub const AUTO_QUIET: Duration = Duration::from_secs(60);

/// 기준을 이만큼 넘게 넘으면 조용해지기를 기다리지 않는다 — 다음 판정에서 바로 연다.
pub const AUTO_URGENT_OVER: f64 = 200_000.0;

/// 유휴 현행화를 캐시 만료보다 이만큼 먼저 연다 — 현행화 턴이 아직 살아 있는 캐시를 읽게.
pub const IDLE_LEAD: Duration = Duration::from_secs(60);

/// 프롬프트 캐시 수명 — API 키 · 추가 사용량은 5분, 구독(플랜 포함 사용량)의 메인 대화는 1시간이다.
pub const CACHE_TTL_5M: Duration = Duration::from_secs(300);
pub const CACHE_TTL_1H: Duration = Duration::from_secs(3600);

/// 쉬는 탭이 이만큼 쉬면 회수 · 캐시 만료 전에 현행화한다 — 그 세션의 캐시 수명에서 [`IDLE_LEAD`]
/// 를 뺀 값(5분 캐시 → 4분, 1시간 캐시 → 59분).
///
/// 수명은 턴에서 관측한 값이 먼저다 — 프로필이 1시간이라 해도 인증 변수가 빠진 재기동은 5분으로 내려간다
/// (`cache_watch` 의 강등). 관측이 없을 때(비우기 직후 · plugin 재기동 직후)만 프로필 `cache_ttl` 을 쓰고,
/// 그것도 없으면 5분으로 본다.
pub fn idle_wait(
    u: &Usage,
    profile: Option<crate::adapters::driven::store::agents::CacheTtl>,
) -> Duration {
    cache_ttl(u, profile).saturating_sub(IDLE_LEAD)
}

/// 그 세션의 캐시 수명 — 관측값 > 프로필 > 5분([`idle_wait`] 와 같은 순서).
pub fn cache_ttl(
    u: &Usage,
    profile: Option<crate::adapters::driven::store::agents::CacheTtl>,
) -> Duration {
    let one_hour = u.cache_1h.unwrap_or_else(|| {
        profile.is_some_and(crate::adapters::driven::store::agents::CacheTtl::is_1h)
    });
    if one_hour { CACHE_TTL_1H } else { CACHE_TTL_5M }
}

/// 휴식 시계를 미는 IPC 메서드 — surface 를 겨눠 세션에 무언가를 시키는 명령. 읽기(`status` ·
/// `output` · `watch`)와 재우기(`sleep`)는 뺀다.
pub const TOUCHING_METHODS: &[&str] = &[
    "agent_session.tell",
    "agent_session.clear",
    "agent_session.sync_compact",
    "agent_session.add_dir",
    "agent_session.remove_dir",
    "agent_session.abort",
];

/// 탭이 낸 조작인가 — 조작은 `#` 꼬리표가 붙은 이동으로 온다.
pub fn is_tab_action(url: &str) -> bool {
    url.split_once('#').is_some_and(|(_, tag)| !tag.is_empty())
}

/// 같은 세션의 자동 수행 사이 최소 간격.
pub const COOLDOWN: Duration = Duration::from_secs(600);

/// 백그라운드 현행화를 미루는 사유 — 로그에만 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// 그 프로필이 429 로 막혀 기다리는 중이다(`rate_block`).
    RateLimited,
    /// 같은 프로필의 다른 탭이 유휴 현행화 중이다.
    OtherBackgroundSync,
    /// 같은 프로필의 어느 탭이 429 재전송을 기다린다.
    RetryPending,
    /// 이번 판정에서 그 프로필에 이미 하나를 열었다.
    OpenedThisPass,
}

/// 사용자가 시키지 않은 쉬는 탭의 현행화(유휴)를 그 agent 프로필에서 지금 열면 안 되는 사유 — 열어도 되면
/// `None`.
///
/// 한도는 프로필이 쓰는 계정 단위다. 쉰 탭의 현행화는 캐시가 식었으면 컨텍스트 전체를 다시 보내 그 한도를
/// 크게 쓴다. 그래서 같은 프로필의 여러 탭이 한꺼번에 열거나 429 재전송과 겹치면 서로를 429 로 막는다.
/// 이런 현행화는 프로필마다 하나씩만 열고, 그 프로필이 429 로 막혀 있거나 재전송을 기다리면 미룬다.
/// 차단기가 탐침을 허락하면([`crate::domain::rate_block::Gate::Probe`]) 막지 않는다 — 연 쪽이 탐침으로 적는다.
/// 사용자가 연 현행화(`Manual`)는 막지 않는다.
pub fn background_sync_hold<'a>(
    convos: impl IntoIterator<Item = &'a Convo>,
    profile: &str,
    blocks: &crate::domain::rate_block::Blocks,
    now: Instant,
) -> Option<Hold> {
    if blocks.gate(profile, now) == crate::domain::rate_block::Gate::Closed {
        return Some(Hold::RateLimited);
    }
    for c in convos
        .into_iter()
        .filter(|c| crate::domain::rate_block::profile_of(c) == profile)
    {
        if phase_mode(&c.sync.phase) == Some(Mode::Idle) {
            return Some(Hold::OtherBackgroundSync);
        }
        if c.retry.pending.is_some() {
            return Some(Hold::RetryPending);
        }
    }
    None
}

/// 진행 단계.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// 현행화 턴을 보냈다 — 그 턴의 보낸 말(`Turn::sent`)로 끝을 찾는다.
    Syncing { prompt: String, mode: Mode },
    /// compact 제어 턴이 도는 중 — 세션은 바쁘다(`activity::is_busy`).
    Compacting { mode: Mode },
}

/// 탭 하나의 현행화 상태. 저장하지 않는다 — 다시 뜬 탭은 다음 턴 끝에서 다시 판정한다.
#[derive(Debug)]
pub struct SyncState {
    pub phase: Phase,
    /// 마지막 턴 끝의 판정 — 넘었나.
    pub over: bool,
    /// 자동 수행을 쏠 수 있나. 실패·바닥값에서 꺼지고, 아래로 내려가면 다시 켜진다.
    pub armed: bool,
    /// 넘었고, 한가해지면 자동으로 시작한다.
    pub pending_auto: bool,
    /// compact 가 끝났다 — 다음 턴 끝에서 바닥값을 본다.
    pub verify: bool,
    /// 직전 자동 수행 시작 시각(쿨다운).
    pub last_auto: Option<Instant>,
    /// 직전 실패 사유 — 상태줄 툴팁 · 알림 줄.
    pub failed: Option<String>,
    /// 턴 끝 줄(`result`)을 원문 구독이 방금 반영했다 — 다음 [`crate::application::plugin::SupervisorPlugin::run_pending_syncs`] 가 판정하고 거둔다.
    pub result_seen: bool,
    /// 캐시 시계 — 마지막 턴 끝(`result`)을 본 시각. API 를 부른 턴만 민다. plugin 이 다시 뜨면 비어
    /// 있어 다음 턴 끝까지는 유휴 현행화를 안 한다.
    pub last_result: Option<Instant>,
    /// 휴식 시계 — 사람이 마지막으로 이 세션에 손댄 시각(턴 · 탭 조작 · 세션을 겨눈 명령). 자동 현행화
    /// 턴은 밀지 않는다. 비었으면 캐시 시계를 쓴다([`rest_since`]).
    pub last_activity: Option<Instant>,
    /// 이번 쉼에서 회수를 이미 시도했다 — 실패한 회수를 2초마다 되풀이하지 않는다. 휴식 시계가 밀리면 풀린다.
    pub release_spent: bool,
    /// 이번 쉼에서 유휴 현행화를 이미 시작했다 — 사용자가 다음 말을 보내야(`begin_turn`) 풀린다.
    /// compact 뒤에도 기준을 넘는 바닥값 세션이 4분마다 되풀이하지 않게 한다.
    pub idle_spent: bool,
    /// 기준을 [`AUTO_URGENT_OVER`] 넘게 넘었다 — [`AUTO_QUIET`] 를 기다리지 않는다.
    pub urgent: bool,
    /// 데몬이 이 세션을 회수했다(행의 `reclaimed` · `stopped`) — 현행화 · ping · 회수 예고를 하지 않는다.
    /// 활동 폴링이 주기마다 채운다(`activity::Activity::note_agent_sessions`).
    pub reclaimed: bool,
    /// 이번 턴은 원문 `result` 줄로 이미 판정했다 — 턴 끝(`finished`)이 한 번 더 판정하지 않게 한다.
    /// 턴 시작(`started`)에서 내린다.
    pub turn_judged: bool,
    /// 데몬 행이 실은 마지막 호출의 컨텍스트(`context_tokens`) — 탭이 못 잰 때 예고 판정이 본다.
    pub row_context: Option<u64>,
    /// 상태줄의 두 시계가 쓰는 설정 · 마지막으로 그린 기한(`idle_clock`).
    pub clock: crate::application::idle_clock::ClockState,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            over: false,
            armed: true,
            pending_auto: false,
            verify: false,
            last_auto: None,
            failed: None,
            result_seen: false,
            last_result: None,
            last_activity: None,
            release_spent: false,
            idle_spent: false,
            urgent: false,
            reclaimed: false,
            turn_judged: false,
            row_context: None,
            clock: crate::application::idle_clock::ClockState::default(),
        }
    }
}

impl SyncState {
    /// compact 제어 턴이 도는 중 — 탭이 연 턴이라 바쁨·자기 턴 판정에 들어간다.
    pub fn compacting(&self) -> bool {
        matches!(self.phase, Phase::Compacting { .. })
    }

    /// 현행화 또는 compact 가 진행 중.
    pub fn running(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// 휴식 시계를 민다 — 사람이 세션에 손댔다. 새 쉼이므로 회수 시도도 다시 연다.
    pub fn touch(&mut self, now: Instant) {
        self.last_activity = Some(now);
        self.release_spent = false;
    }
}

/// 설정 묶음 — 사용자가 안 건드린 칸(호스트가 `null`)·빈 칸은 기본값이다.
#[derive(Debug, Clone, PartialEq)]
pub struct Knobs {
    /// 턴 끝 기준(토큰) — 0 이면 끔.
    pub turn_tokens: f64,
    /// 유휴 현행화 기준(토큰) — 0 이면 끔.
    pub idle_tokens: f64,
    /// 쉰 세션의 claude 를 내린다(유휴 회수) — 「세션 최적화」 스위치로 [`Self::memory_saver`] 와 같은 값이다.
    pub reclaim: bool,
    /// 프로필별 `cache_ttl` — 탭이 캐시 종류를 아직 못 봤을 때 [`idle_wait`] 가 쓴다.
    pub cache_ttl:
        std::collections::BTreeMap<String, crate::adapters::driven::store::agents::CacheTtl>,
    /// 「세션 최적화」(`view_state`) — 가려진 탭의 WebView 를 호스트가 내린다.
    pub memory_saver: bool,
    pub prompt: String,
    pub compact: String,
}

fn setting<H: HostCall>(host: &H, key: &str) -> Option<Value> {
    host.call("settings.get_plugin_setting", json!({ "storage_key": key }))
        .ok()?
        .get("value")
        .cloned()
        .filter(|v| !v.is_null())
}

fn text_or<H: HostCall>(host: &H, key: &str, default: &str) -> String {
    setting(host, key)
        .and_then(|v| v.as_str().map(str::trim).map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// 묶음 스위치가 켜졌을 때만 기준(토큰)을 읽는다 — 꺼졌으면 0(끔).
fn tokens_if_on<H: HostCall>(
    host: &H,
    on_key: &str,
    on_default: bool,
    key: &str,
    default: f64,
) -> f64 {
    let on = setting(host, on_key)
        .and_then(|v| v.as_bool())
        .unwrap_or(on_default);
    if !on {
        return 0.0;
    }
    setting(host, key)
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n >= IDLE_TOKENS_MIN)
        .unwrap_or(default)
        .clamp(IDLE_TOKENS_MIN, IDLE_TOKENS_MAX)
}

/// 설정을 읽는다. 두 기준 모두 토큰이다. 스위치를 끈 기준만 0(끔)으로 읽는다 — 기준 칸의 0 은 끔이
/// 아니라 잘못된 값이라 기본값이 된다. 컨텍스트 묶음 머리 스위치를 끄면 두 현행화가 모두 0 이다.
pub fn read_knobs<H: HostCall>(host: &H) -> Knobs {
    let context_on = setting(host, CONTEXT_ON_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(DEFAULT_CONTEXT_ON);
    let gate = |v: f64| if context_on { v } else { 0.0 };
    let turn_tokens = tokens_if_on(
        host,
        THRESHOLD_ON_KEY,
        DEFAULT_THRESHOLD_ON,
        THRESHOLD_KEY,
        DEFAULT_THRESHOLD_TOKENS,
    );
    let idle_tokens = tokens_if_on(
        host,
        IDLE_ON_KEY,
        DEFAULT_IDLE_ON,
        IDLE_KEY,
        DEFAULT_IDLE_TOKENS,
    );
    // 「세션 최적화」 — 스위치 하나가 회수와 화면 내리기를 함께 켠다.
    let optimize = setting(host, RECLAIM_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(DEFAULT_RECLAIM);
    Knobs {
        turn_tokens: gate(turn_tokens),
        idle_tokens: gate(idle_tokens),
        reclaim: optimize,
        prompt: text_or(host, PROMPT_KEY, &default_sync_prompt()),
        compact: text_or(host, COMPACT_KEY, &default_compact_prompt()),
        cache_ttl: profile_cache_ttl(),
        memory_saver: optimize,
    }
}

/// 프로필 폴더에서 `cache_ttl` 을 밝힌 프로필만 모은다 — 설정과 같은 수명([`KNOBS_TTL`])으로 다시 읽는다.
fn profile_cache_ttl()
-> std::collections::BTreeMap<String, crate::adapters::driven::store::agents::CacheTtl> {
    let dir = crate::adapters::driven::store::agents::default_dir();
    crate::adapters::driven::store::agents::load(dir.as_deref())
        .into_iter()
        .filter_map(|p| p.cache_ttl.map(|t| (p.name, t)))
        .collect()
}

impl Knobs {
    /// 탭의 프로필이 밝힌 캐시 수명.
    pub fn ttl_of(&self, c: &Convo) -> Option<crate::adapters::driven::store::agents::CacheTtl> {
        self.cache_ttl
            .get(crate::domain::rate_block::profile_of(c))
            .copied()
    }
}

/// 설정을 다시 묻기까지 — 호스트는 설정 변경을 plugin 에 알리지 않으므로 바꾼 값은 이만큼
/// 늦게 반영된다. 현행화는 분 단위로 판정하므로 몇 초 늦는 것은 결과를 안 바꾼다.
pub const KNOBS_TTL: std::time::Duration = std::time::Duration::from_secs(10);

/// [`read_knobs`] 의 짧은 수명 캐시.
///
/// 판정 자리(`run_idle_syncs` · 턴 끝 · 현행화 시작)는 흐름 이벤트마다 돈다. 캐시 수명이 지난
/// 쉬는 탭이 기준 토큰 미만이면 쉼이 끝날 때까지 후보로 남아, 그동안 이벤트마다 설정 4칸을
/// 다시 읽었다 — 실측 2026-10-26: 10분에 `settings.get_plugin_setting` 8,499 회(supervisor 가
/// 부른 IPC 의 89%). 값은 [`KNOBS_TTL`] 안에서 한 번만 읽는다.
#[derive(Default)]
pub struct KnobCache {
    held: Option<(Instant, Knobs)>,
}

impl KnobCache {
    /// 수명 안이면 들고 있는 값, 지났으면 호스트에 다시 묻는다.
    pub fn get<H: HostCall>(&mut self, host: &H, now: Instant) -> Knobs {
        if let Some((at, k)) = &self.held
            && now.saturating_duration_since(*at) < KNOBS_TTL
        {
            return k.clone();
        }
        let k = read_knobs(host);
        self.held = Some((now, k.clone()));
        k
    }
}

/// 현행화 턴의 입력 — 문구 뒤에 **누가 시작했나**를 한 줄 붙인다.
///
/// 꼬리표는 `[sync-compact:auto]` · `[sync-compact:manual]` 고정 식별자로 시작한다(문구는
/// 로케일을 따른다). 대화 기록(세션 jsonl)만 보고 자동 · 수동 · 사람이 친 현행화를 가르려는
/// 것이다 — 텔레메트리는 최근 이벤트만 남아 지난 수행을 못 가른다.
/// 턴은 보낸 말 그대로 찾으므로([`sync_turn`]) 이 값을 [`Phase::Syncing`] 에 그대로 둔다.
///
/// `base` 는 자동이면 임계(%), 유휴면 기준 토큰 수다.
pub fn sync_input(prompt: &str, mode: Mode, usage: &Usage, base: f64) -> String {
    let tag = match mode {
        Mode::Auto => {
            let used = usage
                .context_used
                .map_or_else(|| "?".to_string(), |n| format!("{}k", n / 1000));
            let bar = effective_threshold(usage, base).unwrap_or(base);
            i18n::tn(
                "sup.sync.tag_auto",
                &[&used, &format!("{:.0}k", bar / 1000.0)],
            )
        }
        Mode::Manual => i18n::t("sup.sync.tag_manual"),
        Mode::Idle => {
            let used = usage
                .context_used
                .map_or_else(|| "?".to_string(), |n| format!("{}k", n / 1000));
            i18n::tn(
                "sup.sync.tag_idle",
                &[&used, &format!("{:.0}k", base / 1000.0)],
            )
        }
    };
    format!("{prompt}\n\n{tag}")
}

/// compact 제어 턴의 입력.
pub fn compact_input(prompt: &str) -> String {
    format!("/compact {prompt}")
}

/// 실제로 견줄 턴 끝 기준 — 창을 알면 그 [`WINDOW_CAP`] 을 넘지 않게 끌어내린다. 0 이면 끔(`None`).
pub fn effective_threshold(u: &Usage, tokens: f64) -> Option<f64> {
    if tokens <= 0.0 {
        return None;
    }
    Some(match u.context_window.filter(|w| *w > 0) {
        Some(w) => tokens.min(w as f64 * WINDOW_CAP),
        None => tokens,
    })
}

/// 컨텍스트를 턴 끝 기준에 견준 비 — 1.0 이상이면 넘었다. 컨텍스트를 모르거나 끈 기준이면 없다.
pub fn threshold_ratio(u: &Usage, tokens: f64) -> Option<f64> {
    let used = u.context_used?;
    Some(used as f64 / effective_threshold(u, tokens)?)
}

/// 기준을 [`AUTO_URGENT_OVER`] 넘게 넘었나.
pub fn urgent(u: &Usage, tokens: f64) -> bool {
    match (u.context_used, effective_threshold(u, tokens)) {
        (Some(used), Some(bar)) => used as f64 >= bar + AUTO_URGENT_OVER,
        _ => false,
    }
}

/// 턴 끝(`result`) 하나를 판정한다 — 자동 수행 가드는 전부 여기 있다.
///
/// `pct` 는 0..=95. 0 이면 판정·알림·자동이 모두 없다.
pub fn judge(s: &mut SyncState, ratio: Option<f64>, pct: f64, now: Instant) {
    if pct <= 0.0 {
        s.over = false;
        s.pending_auto = false;
        return;
    }
    let Some(r) = ratio else {
        return;
    };
    let over_now = r >= pct / 100.0;
    if s.verify {
        // compact 뒤 첫 턴 끝 — 여전히 넘으면 바닥값이다. 쏘아도 안 준다.
        s.verify = false;
        if over_now {
            s.armed = false;
            s.pending_auto = false;
            s.failed = Some(i18n::t("sup.sync.still_over"));
            s.over = true;
            return;
        }
    }
    if !over_now {
        s.armed = true;
        s.pending_auto = false;
    } else if !s.over && s.armed && !s.running() {
        let cooled = s
            .last_auto
            .is_none_or(|t| now.duration_since(t) >= COOLDOWN);
        if cooled {
            s.pending_auto = true;
        }
    }
    s.over = over_now;
}

/// 보낸 현행화 턴이 지금 어떤가.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncTurn {
    Pending,
    Ok,
    Failed(String),
    Stopped,
    /// 대화에서 그 턴을 못 찾았다(비우기 등).
    Missing,
}

/// 대화에서 가장 늦게 보낸 그 문구의 턴을 찾는다 — 보관으로 `older` 로 갔으면 거기서.
pub fn sync_turn(c: &Convo, prompt: &str) -> SyncTurn {
    if let Some(at) = c.turns.iter().rposition(|t| t.sent == prompt) {
        let t = &c.turns[at];
        if t.pending {
            return SyncTurn::Pending;
        }
        if c.stopped.contains(&at) {
            return SyncTurn::Stopped;
        }
        return match &t.error {
            Some(e) => SyncTurn::Failed(e.clone()),
            None => SyncTurn::Ok,
        };
    }
    match c.older.iter().rev().find(|t| t.sent == prompt) {
        Some(t) if t.pending => SyncTurn::Pending,
        Some(t) => match &t.error {
            Some(e) => SyncTurn::Failed(e.clone()),
            None => SyncTurn::Ok,
        },
        None => SyncTurn::Missing,
    }
}

/// 자동 시작이 지금 가능한가 — 탭이 연 턴·대기열·물음·비우기가 모두 없고, [`AUTO_QUIET`] 동안
/// 조용했을 때(기준을 크게 넘었으면 바로). 회수된 세션은 열지 않는다.
pub fn ready_for_auto(c: &Convo, busy: bool, now: Instant) -> bool {
    let quiet = c.sync.urgent
        || c.sync
            .last_result
            .is_some_and(|t| now.saturating_duration_since(t) >= AUTO_QUIET);
    c.sync.pending_auto
        && quiet
        && !c.sync.reclaimed
        && !c.sync.running()
        && !busy
        && !c.clearing
        && c.queued.is_empty()
        && c.needs_input.is_none()
        && c.session_id.is_some()
}

/// 쉬는 탭을 회수 전에 현행화할 때인가 — 한가하고, 캐시 시계가 [`idle_wait`] 를 넘었고, 이번 쉼에 아직
/// 안 했고, 컨텍스트가 기준(`min_tokens`, 0 = 끔) 이상일 때. 기준은 넘는 순간이 아니라 **지금 값**으로
/// 본다 — 쉬는 동안에는 점유가 안 바뀐다. 비율이 아니라 토큰 수인 것은 분당 입력 한도(429)가 절대
/// 토큰으로 걸리기 때문이다.
pub fn ready_for_idle(
    c: &Convo,
    busy: bool,
    now: Instant,
    min_tokens: f64,
    ttl: Option<crate::adapters::driven::store::agents::CacheTtl>,
) -> bool {
    min_tokens > 0.0
        && !c.sync.idle_spent
        && settled(c, busy)
        && elapsed(c.sync.last_result, now, idle_wait(&c.usage, ttl))
        && c.usage.context_used.is_some_and(|n| n as f64 >= min_tokens)
}

/// 휴식 시계의 기점 — 사람이 손댄 시각, 없으면(plugin 재기동 뒤 첫 턴 끝 전) 캐시 시계.
pub fn rest_since(c: &Convo) -> Option<Instant> {
    c.sync.last_activity.or(c.sync.last_result)
}

/// 회수할 때인가 — 한가하고, 휴식 시계가 [`idle_wait`] 를 넘었고, 이번 쉼에 아직 시도하지 않았다.
pub fn ready_for_release(
    c: &Convo,
    busy: bool,
    now: Instant,
    ttl: Option<crate::adapters::driven::store::agents::CacheTtl>,
) -> bool {
    !c.sync.release_spent
        && settled(c, busy)
        && elapsed(rest_since(c), now, idle_wait(&c.usage, ttl))
}

/// 기점에서 `wait` 이상 지났다 — 기점이 없으면 아니다.
fn elapsed(since: Option<Instant>, now: Instant, wait: Duration) -> bool {
    since.is_some_and(|t| now.saturating_duration_since(t) >= wait)
}

/// 판정 후보인가 — 설정을 읽기 전에 거른다. 프로필 수명을 안 보므로 [`idle_wait`] 의 아래 끝(관측값
/// 또는 5분)으로 본다 — 프로필은 기한을 늘리기만 한다.
fn candidate(c: &Convo, now: Instant) -> bool {
    let floor = idle_wait(&c.usage, None);
    (!c.sync.idle_spent && elapsed(c.sync.last_result, now, floor))
        || (!c.sync.release_spent && elapsed(rest_since(c), now, floor))
}

/// 손댈 것이 없다 — 턴 · 현행화 · 비우기 · 대기열 · 물음이 없고 아직 회수되지 않았다. 회수 직전에도
/// 다시 본다(`idle_reclaim`) — 현행화가 끝나는 사이 사용자가 말을 걸었을 수 있다.
pub fn settled(c: &Convo, busy: bool) -> bool {
    !c.sync.reclaimed
        && !c.sync.running()
        && !busy
        && !c.clearing
        && c.queued.is_empty()
        && c.needs_input.is_none()
        && c.session_id.is_some()
}

/// 쉰 탭에 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleStep {
    Nothing,
    /// 기준 이상 — 현행화 → compact(회수는 그 뒤).
    Sync,
    /// 기준 미만이거나 기준이 0 — 정리 없이 바로 내린다.
    Release,
}

/// 쉰 탭 하나의 할 일 — 현행화는 회수와 무관하게 캐시 시계로 열고, 회수는 설정이 켜졌을 때 휴식
/// 시계로 한다. 휴식 시계는 캐시 시계보다 늦거나 같으므로 회수할 때는 현행화 판정이 이미 났다.
pub fn idle_step(c: &Convo, busy: bool, now: Instant, k: &Knobs) -> IdleStep {
    let ttl = k.ttl_of(c);
    if ready_for_idle(c, busy, now, k.idle_tokens, ttl) {
        IdleStep::Sync
    } else if k.reclaim && !sync_owed(c, k.idle_tokens) && ready_for_release(c, busy, now, ttl) {
        IdleStep::Release
    } else {
        IdleStep::Nothing
    }
}

/// 이번 쉼에 아직 안 한 유휴 현행화가 남았다 — 그 전에는 내리지 않는다(캐시 시계가 아직이거나 429 로
/// 미뤄진 경우).
fn sync_owed(c: &Convo, min_tokens: f64) -> bool {
    min_tokens > 0.0
        && !c.sync.idle_spent
        && c.usage.context_used.is_some_and(|n| n as f64 >= min_tokens)
}

/// 지금 이 탭에서 할 다음 일.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// 현행화 턴이 잘 끝났다 — compact 를 보낸다.
    Compact(Mode),
    /// 현행화가 안 끝났다 — compact 없이 멈춘다(사유).
    Fail(String),
    /// 넘었고 한가하다 — 자동 현행화를 시작한다.
    StartAuto,
}

/// 탭 하나의 다음 일을 정한다 — **현행화 턴이 성공으로 끝났을 때만 compact** 가 여기서 정해진다.
pub fn next_step(c: &Convo, busy: bool, now: Instant) -> Step {
    match &c.sync.phase {
        Phase::Syncing { prompt, mode } => match sync_turn(c, prompt) {
            SyncTurn::Pending => Step::Nothing,
            // 답은 왔는데 데몬이 아직 턴을 돈다고 하면 다음 판정까지 기다린다.
            SyncTurn::Ok if busy => Step::Nothing,
            SyncTurn::Ok => Step::Compact(*mode),
            SyncTurn::Failed(why) => Step::Fail(i18n::t1("sup.sync.sync_failed", &why)),
            SyncTurn::Stopped => Step::Fail(i18n::t("sup.sync.stopped")),
            SyncTurn::Missing => Step::Fail(i18n::t("sup.sync.lost")),
        },
        Phase::Compacting { .. } => Step::Nothing,
        Phase::Idle if ready_for_auto(c, busy, now) => Step::StartAuto,
        Phase::Idle => Step::Nothing,
    }
}

/// 현행화 실패 표지를 거둘 때인가 — 실패가 남았고, 현행화가 돌지 않고, 방금 끝난 턴이 오류가 아니다.
///
/// 실패한 현행화 턴 자신은 아직 `Syncing` 이라 여기서 거두지 않는다(판정이 실패 처리보다 먼저 돈다).
pub fn recovered(s: &SyncState, turn_error: bool) -> bool {
    s.failed.is_some() && !s.running() && !turn_error
}

/// 상태줄 토큰 — (값 i18n 키, 색 class, 툴팁 i18n 키). 없으면 그리지 않는다.
pub fn status_label(s: &SyncState) -> Option<(&'static str, &'static str, &'static str)> {
    if s.running() {
        Some(("sup.sync.running", "wait", "sup.sync.running_hint"))
    } else if s.failed.is_some() {
        Some(("sup.sync.failed", "bad", "sup.sync.failed_hint"))
    } else if s.over {
        Some(("sup.sync.over", "wait", "sup.sync.over_hint"))
    } else {
        None
    }
}

/// 끝난 compact 의 결과 — `(surface, Ok | 사유)`.
pub fn parse_done(params: &Value) -> Option<(u32, Result<(), String>)> {
    let sid = params.get("surface_id")?.as_u64()? as u32;
    let ok = params.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let why = params
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((sid, if ok { Ok(()) } else { Err(why) }))
}

/// 진행 중인 현행화의 출처 — 쉬는 중이면 `None`.
fn phase_mode(p: &Phase) -> Option<Mode> {
    match p {
        Phase::Syncing { mode, .. } | Phase::Compacting { mode } => Some(*mode),
        _ => None,
    }
}

/// 탭 쪽 처리 — 대화 · 활동 · 보내기를 함께 만지므로 plugin 의 메서드로 둔다.
impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 현행화를 시작한다 — 거절이면 그 사유. 보내는 길은 사용자 턴과 같다(`send_turn`).
    pub(crate) fn start_sync(
        &mut self,
        host: &impl SendHost,
        sid: u32,
        mode: Mode,
    ) -> Result<(), String> {
        let knobs = self.knobs.get(host, Instant::now());
        let Some(c) = self.convos.get_mut(&sid) else {
            return Err(i18n::t("sup.sync.need_tab"));
        };
        let busy = self.activity.turn_busy(c);
        if busy || c.clearing || c.sync.running() || !c.queued.is_empty() {
            return Err(i18n::t("sup.sync.busy"));
        }
        if c.session_id.is_none() {
            return Err(i18n::t("sup.err.clear_no_session"));
        }
        let base = if mode == Mode::Idle {
            knobs.idle_tokens
        } else {
            knobs.turn_tokens
        };
        let sent = sync_input(&knobs.prompt, mode, &c.usage, base);
        c.sync.phase = Phase::Syncing {
            prompt: sent.clone(),
            mode,
        };
        c.sync.pending_auto = false;
        c.sync.failed = None;
        if mode == Mode::Auto {
            c.sync.last_auto = Some(Instant::now());
        }
        let metric = match mode {
            Mode::Auto => "sync_compact_auto",
            Mode::Manual => "sync_compact_manual",
            Mode::Idle => "sync_compact_idle",
        };
        let before = c.usage.context_used.unwrap_or(0) as f64;
        if let Err(e) = crate::application::surface_link::record_metric(host, sid, metric, before) {
            tracing::debug!("[agent_session] surface={sid}: 현행화 텔레메트리 실패: {e:?}");
        }
        tracing::info!("[agent_session] surface={sid}: 현행화를 보낸다({mode:?})");
        self.send_turn(host, sid, &sent, None);
        Ok(())
    }

    /// IPC · CLI — 거절 사유는 탭의 알림 줄에도 보인다.
    pub(crate) fn sync_from_tab(&mut self, host: &impl SendHost, sid: u32) -> Result<(), String> {
        let got = self.start_sync(host, sid, Mode::Manual);
        let note = got.as_ref().err().map(String::as_str);
        session_surface::patch(host, sid, &clear::note_msg(note));
        got
    }

    /// `agent_session.sync_compact` — 대화 탭에서만 돈다(compact 의 끝을 탭이 받아야 한다).
    pub(crate) fn sync_by_ipc(&mut self, params: &Value) -> Result<Value, AppError> {
        let host = self
            .host
            .clone()
            .ok_or_else(|| AppError::new(i18n::t("sup.err.worker_spawn")))?;
        let (sid, _session) = crate::application::request::session_for(&host, params)?;
        if !self.convos.contains_key(&sid) {
            return Err(AppError::new(i18n::t("sup.sync.need_tab")));
        }
        self.sync_from_tab(&host, sid).map_err(AppError::new)?;
        Ok(json!({ "surface_id": sid, "started": true }))
    }

    /// 턴 끝에서 임계치를 다시 판정한다 — 설정은 이때 읽는다(턴당 한 번).
    pub(crate) fn judge_after_result(&mut self, host: &impl SendHost, sid: u32) {
        let knobs = self.knobs.get(host, Instant::now());
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let now = Instant::now();
        // 캐시 재작성 감지(`cache_watch`) — 쉰 시간은 직전 턴 끝부터 이번 턴 시작까지. compact 뒤 첫
        // 턴과 비우기 턴은 대화가 바뀐 자리라 판정하지 않는다.
        let gap =
            crate::application::cache_watch::gap_since(c.sync.last_result, now, c.usage.turn_ms);
        if c.sync.verify || c.clearing {
            c.cache.skip_next = true;
        }
        c.sync.last_result = Some(now);
        // 자동 현행화 · compact 턴은 사람이 쉬는 동안 돈 것이다 — 휴식 시계는 그대로 둔다.
        if !c.sync.running() {
            c.sync.touch(now);
        }
        let on = if knobs.turn_tokens > 0.0 { 100.0 } else { 0.0 };
        judge(
            &mut c.sync,
            threshold_ratio(&c.usage, knobs.turn_tokens),
            on,
            now,
        );
        c.sync.urgent = urgent(&c.usage, knobs.turn_tokens);
        // 실패 표지는 다음 현행화까지 남기지 않는다 — 오류 없이 끝난 턴이 오면 통신이 돌아온 것이다.
        if recovered(&c.sync, c.hookless.turn_error) {
            c.sync.failed = None;
            session_surface::patch(host, sid, &clear::note_msg(None));
            self.activity.refresh_status(host, sid, c);
        }
        self.watch_cache(host, sid, gap);
    }

    /// `flush_queues` 의 끝 — 끝난 현행화 턴을 compact 로 잇고, 한가해진 탭의 자동 수행을 연다.
    pub(crate) fn run_pending_syncs(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let seen: Vec<u32> = self
            .convos
            .iter_mut()
            .filter_map(|(&sid, c)| std::mem::take(&mut c.sync.result_seen).then_some(sid))
            .collect();
        for sid in seen {
            self.judge_after_result(&host, sid);
        }
        let now = Instant::now();
        let mut compact = Vec::new();
        let mut fail = Vec::new();
        let mut auto = Vec::new();
        for (&sid, c) in &self.convos {
            match next_step(c, self.activity.turn_busy(c), now) {
                Step::Nothing => {}
                Step::Compact(mode) => compact.push((sid, mode)),
                Step::Fail(why) => fail.push((sid, why)),
                Step::StartAuto => auto.push(sid),
            }
        }
        for (sid, why) in fail {
            self.sync_failed(&host, sid, why);
        }
        for (sid, mode) in compact {
            self.start_compact(&host, sid, mode);
        }
        for sid in auto {
            // 임계 자동도 사용자가 시키지 않은 전송이다 — 그 프로필이 429 로 막혔으면 다음 판정까지 둔다.
            let Some(profile) = self.profile_of(sid) else {
                continue;
            };
            if self.blocks.gate(&profile, now) == crate::domain::rate_block::Gate::Closed {
                tracing::debug!(
                    "[agent_session] surface={sid}: 프로필 {profile:?} 이 막혀 자동 현행화를 미룬다"
                );
                continue;
            }
            match self.start_sync(&host, sid, Mode::Auto) {
                Ok(()) => self.blocks.take_probe(&profile, now),
                Err(why) => {
                    tracing::info!("[agent_session] surface={sid}: 자동 현행화를 미룬다: {why}")
                }
            }
        }
        self.run_idle_syncs(&host);
    }

    /// 쉬는 탭을 캐시가 식기 전에 현행화하거나 내린다 — 설정은 후보가 있을 때만 읽는다(2초마다 도는
    /// 자리라서).
    fn run_idle_syncs(&mut self, host: &impl SendHost) {
        let now = Instant::now();
        if !self.convos.values().any(|c| candidate(c, now)) {
            return;
        }
        let knobs = self.knobs.get(host, Instant::now());
        let due: Vec<(u32, IdleStep)> = self
            .convos
            .iter()
            .map(|(&sid, c)| (sid, idle_step(c, self.activity.is_busy(c), now, &knobs)))
            .filter(|(_, step)| *step != IdleStep::Nothing)
            .collect();
        let mut opened: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (sid, step) in due {
            let profile = self.profile_of(sid).unwrap_or_default();
            // 현행화할 탭은 막혔으면 표시 없이 넘긴다 — 다음 판정에서 다시 본다. 회수만 할 탭은 API 를
            // 안 쓰니 막지 않는다.
            let hold = if opened.contains(&profile) {
                Some(Hold::OpenedThisPass)
            } else {
                background_sync_hold(self.convos.values(), &profile, &self.blocks, now)
            };
            if step == IdleStep::Sync
                && let Some(why) = hold
            {
                tracing::debug!("[agent_session] surface={sid}: 유휴 현행화를 미룬다: {why:?}");
                continue;
            }
            // 시작이 거절돼도 이번 쉼에는 다시 안 쏜다 — 2초마다 같은 거절을 되풀이하지 않게. 회수 표지는
            // 아래에서 회수할 때 켠다 — 여기서 켜면 `release_due` 가 거짓이 되어 회수가 영영 안 돈다.
            if step == IdleStep::Sync
                && let Some(c) = self.convos.get_mut(&sid)
            {
                c.sync.idle_spent = true;
            }
            if step == IdleStep::Sync {
                match self.start_sync(host, sid, Mode::Idle) {
                    Ok(()) => {
                        self.blocks.take_probe(&profile, now);
                        opened.insert(profile);
                        continue;
                    }
                    Err(why) => {
                        tracing::info!(
                            "[agent_session] surface={sid}: 유휴 현행화를 못 열었다: {why}"
                        )
                    }
                }
            }
            // 회수로 골랐거나 현행화를 못 열었으면 휴식 시계를 본다 — 사람이 방금 손댄 세션은 아직 두고, 그때가
            // 되면 다음 판정이 회수로 고른다(현행화는 이번 쉼에 썼다).
            if knobs.reclaim && self.release_due(sid, &knobs, now) {
                if let Some(c) = self.convos.get_mut(&sid) {
                    c.sync.release_spent = true;
                }
                self.idle_reclaim(host, sid);
            }
        }
    }

    /// 유휴 현행화가 끝났다(성공 · 실패) — 회수가 켜졌으면 이어서 내린다. 실패해도 내리는 것은 그
    /// 세션이 이미 캐시 수명만큼 쉬었기 때문이다(데몬 시절에는 예고 마감에 데몬이 내렸다).
    ///
    /// 단 그 프로필이 429 로 막혀 있으면 내리지 않는다. 내린 세션은 다음 입력에 컨텍스트 전체를 다시 써서
    /// 차단이 풀리자마자 큰 요청을 낸다.
    fn after_idle_sync(&mut self, host: &impl SendHost, sid: u32, mode: Option<Mode>) {
        let now = Instant::now();
        let knobs = self.knobs.get(host, now);
        if mode != Some(Mode::Idle) || !knobs.reclaim {
            return;
        }
        // 현행화는 캐시 시계로 열렸다 — 그새 사람이 손댔으면(휴식 시계) 지금은 안 내리고 다음 판정에 맡긴다.
        if !self.release_due(sid, &knobs, now) {
            tracing::info!("[agent_session] surface={sid}: 최근 조작이 있어 회수를 미룬다");
            return;
        }
        let profile = self.profile_of(sid).unwrap_or_default();
        if self.blocks.gate(&profile, Instant::now()) != crate::domain::rate_block::Gate::Open {
            tracing::info!(
                "[agent_session] surface={sid}: 프로필 {profile:?} 이 429 로 막혀 회수를 미룬다"
            );
            return;
        }
        self.idle_reclaim(host, sid);
    }

    /// 휴식 시계로 회수할 때인가 — 현행화 판정과 무관하게 본다.
    fn release_due(&self, sid: u32, knobs: &Knobs, now: Instant) -> bool {
        self.convos
            .get(&sid)
            .is_some_and(|c| ready_for_release(c, self.activity.is_busy(c), now, knobs.ttl_of(c)))
    }

    /// 탭의 agent 프로필 이름 — 탭이 없으면 `None`.
    pub(crate) fn profile_of(&self, sid: u32) -> Option<String> {
        self.convos
            .get(&sid)
            .map(|c| crate::domain::rate_block::profile_of(c).to_string())
    }

    fn sync_failed(&mut self, host: &impl SendHost, sid: u32, why: String) {
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        tracing::warn!("[agent_session] surface={sid}: 현행화를 멈춘다: {why}");
        let mode = phase_mode(&c.sync.phase);
        if let Some(mode) = mode {
            crate::application::session_log::record(host, sid, c, mode, Some(&why));
        }
        c.sync.phase = Phase::Idle;
        c.sync.armed = false;
        c.sync.pending_auto = false;
        session_surface::patch(host, sid, &clear::note_msg(Some(&why)));
        c.sync.failed = Some(why);
        self.activity.refresh_status(host, sid, c);
        self.after_idle_sync(host, sid, mode);
    }

    /// compact 제어 턴 — 보내는 것은 워커가 하고 끝은 [`METHOD`] 로 온다(`clear::begin` 과 같은 길).
    fn start_compact(&mut self, host: &impl SendHost, sid: u32, mode: Mode) {
        let knobs = self.knobs.get(host, Instant::now());
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let Some(session) = c.session_id.clone() else {
            return;
        };
        c.sync.phase = Phase::Compacting { mode };
        // 제어 턴은 대화 턴으로 안 그린다 — 대신 어느 설정이 보냈는지 `[system]` 줄로 남긴다.
        let line = crate::domain::sys_note::open(c, crate::domain::sys_note::compact_text(mode));
        if let Some(js) = crate::domain::sys_note::message(c, line) {
            session_surface::patch(host, sid, &js);
        }
        // 이 탭이 연 턴이다 — 그 끝 줄이 "에이전트가 이어서" 로 붙지 않게(`own_turns`).
        c.own_turns.opened();
        let input = compact_input(&knobs.compact);
        let worker_host = host.clone();
        let sup = self.sup;
        let spawned = std::thread::Builder::new()
            .name(format!("agent-session-compact-{sid}"))
            .spawn(move || {
                let (ok, error) = match sup.send_input(&session, &input) {
                    Ok(_) => (true, String::new()),
                    Err(e) => (false, e.to_string()),
                };
                // 되던지기가 실패하면 plugin 이 내려가는 중이다 — 로그만 남긴다.
                if let Err(e) = worker_host.self_invoke(
                    METHOD,
                    json!({ "surface_id": sid, "ok": ok, "error": error }),
                ) {
                    tracing::debug!("[agent_session] surface={sid}: compact 끝을 못 알렸다: {e:?}");
                }
            });
        if spawned.is_err() {
            c.own_turns.closed(Instant::now());
            let why = i18n::t("sup.err.worker_spawn");
            self.sync_failed(host, sid, why);
            return;
        }
        tracing::info!("[agent_session] surface={sid}: compact 를 보냈다");
        if let Some(c) = self.convos.get(&sid) {
            self.activity.refresh_status(host, sid, c);
        }
    }

    /// compact 가 끝났다 — 대화는 그대로 두고, 다음 턴 끝에서 바닥값을 본다.
    pub(crate) fn sync_done(&mut self, sid: u32, got: Result<(), String>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        c.own_turns.closed(Instant::now());
        match got {
            Ok(()) => {
                tracing::info!("[agent_session] surface={sid}: compact 끝");
                let mode = phase_mode(&c.sync.phase);
                if let Some(mode) = mode {
                    crate::application::session_log::record(&host, sid, c, mode, None);
                }
                c.sync.phase = Phase::Idle;
                c.sync.verify = true;
                let line = crate::domain::sys_note::settle(c, crate::domain::sys_note::State::Done);
                if let Some(js) = line.and_then(|i| crate::domain::sys_note::message(c, i)) {
                    session_surface::patch(&host, sid, &js);
                }
                c.sync.failed = None;
                self.activity.refresh_status(&host, sid, c);
                self.after_idle_sync(&host, sid, mode);
            }
            Err(why) => {
                let line = crate::domain::sys_note::settle(
                    c,
                    crate::domain::sys_note::State::Failed(why.clone()),
                );
                if let Some(js) = line.and_then(|i| crate::domain::sys_note::message(c, i)) {
                    session_surface::patch(&host, sid, &js);
                }
                let why = i18n::t1("sup.sync.compact_failed", &why);
                self.sync_failed(&host, sid, why);
            }
        }
        self.flush_queues();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convo::Turn;

    fn at(secs: u64) -> Instant {
        // 기준 시각에서 secs 뒤 — 시험마다 같은 기준을 쓴다.
        thread_local!(static BASE: Instant = Instant::now());
        BASE.with(|b| *b + Duration::from_secs(secs))
    }

    fn usage(used: u64, window: u64) -> Usage {
        Usage {
            context_used: Some(used),
            context_window: Some(window),
            ..Default::default()
        }
    }

    #[test]
    fn the_token_threshold_is_inclusive_and_capped_by_the_window() {
        // U1: 기준 토큰 정확히는 넘은 것이다(1M 창이라 400k 그대로).
        assert_eq!(
            threshold_ratio(&usage(400_000, 1_000_000), 400_000.0),
            Some(1.0)
        );
        let mut s = SyncState::default();
        judge(
            &mut s,
            threshold_ratio(&usage(400_000, 1_000_000), 400_000.0),
            100.0,
            at(0),
        );
        assert!(s.over && s.pending_auto, "U1: 기준 정확히는 넘은 것이다");
        let mut s = SyncState::default();
        judge(
            &mut s,
            threshold_ratio(&usage(399_999, 1_000_000), 400_000.0),
            100.0,
            at(0),
        );
        assert!(!s.over && !s.pending_auto, "U2: 바로 아래는 아니다");
        // 창이 작으면 그 90% 로 끌어내린다 — 200k 창에서 400k 는 영영 안 넘으므로 180k.
        assert_eq!(
            effective_threshold(&usage(0, 200_000), 400_000.0),
            Some(180_000.0)
        );
        assert_eq!(
            threshold_ratio(&usage(180_000, 200_000), 400_000.0),
            Some(1.0)
        );
        // 창을 모르면 기준 그대로 · 0 은 끔 · 컨텍스트를 모르면 판정 안 함.
        let no_window = Usage {
            context_used: Some(400_000),
            ..Default::default()
        };
        assert_eq!(threshold_ratio(&no_window, 400_000.0), Some(1.0));
        assert_eq!(threshold_ratio(&usage(900_000, 1_000_000), 0.0), None);
        assert_eq!(threshold_ratio(&Usage::default(), 400_000.0), None);
        let mut s = SyncState::default();
        judge(
            &mut s,
            threshold_ratio(&usage(900_000, 1_000_000), 0.0),
            0.0,
            at(0),
        );
        assert!(!s.over && !s.pending_auto, "U3: 0 은 끔");
    }

    #[test]
    fn a_large_overshoot_is_urgent() {
        assert!(!urgent(&usage(599_999, 1_000_000), 400_000.0));
        assert!(urgent(&usage(600_000, 1_000_000), 400_000.0));
        assert!(
            !urgent(&usage(900_000, 1_000_000), 0.0),
            "끈 기준은 급하지 않다"
        );
    }

    #[test]
    fn auto_fires_once_when_the_ratio_crosses() {
        // U11: 69 → 71 → 73 → 75 — 넘는 순간 한 번만.
        let mut s = SyncState::default();
        let mut fired = 0;
        for (i, pct) in [69u64, 71, 73, 75].into_iter().enumerate() {
            judge(&mut s, Some(pct as f64 / 100.0), 70.0, at(i as u64));
            if s.pending_auto {
                fired += 1;
                // 시작했다고 치고 다음 판정으로.
                s.pending_auto = false;
                s.last_auto = Some(at(i as u64));
            }
        }
        assert_eq!(fired, 1);
    }

    #[test]
    fn still_over_after_compact_disarms_until_it_drops() {
        // U13: compact 뒤에도 71% — 해제, 73% 에서도 다시 안 쏜다. 내려갔다 넘으면 다시.
        let mut s = SyncState {
            over: true,
            verify: true,
            ..Default::default()
        };
        judge(&mut s, Some(0.71), 70.0, at(0));
        assert!(!s.armed && !s.pending_auto && s.failed.is_some());
        judge(&mut s, Some(0.73), 70.0, at(700));
        assert!(!s.pending_auto, "해제된 채로 계속 넘는 동안은 0");
        judge(&mut s, Some(0.50), 70.0, at(800));
        assert!(s.armed, "아래로 내려가면 다시 장전");
        judge(&mut s, Some(0.72), 70.0, at(900));
        assert!(s.pending_auto, "다시 넘으면 1회");
    }

    #[test]
    fn a_compact_that_worked_clears_the_verify_without_disarming() {
        let mut s = SyncState {
            over: true,
            verify: true,
            ..Default::default()
        };
        judge(&mut s, Some(0.12), 70.0, at(0));
        assert!(s.armed && !s.verify && !s.over && s.failed.is_none());
    }

    #[test]
    fn staying_over_does_not_fire_again_past_the_cooldown() {
        // G1: 넘는 **순간**에만 — 쿨다운이 지나도 계속 넘는 동안은 다시 안 쏜다.
        let mut s = SyncState::default();
        judge(&mut s, Some(0.71), 70.0, at(0));
        assert!(s.pending_auto);
        s.pending_auto = false;
        s.last_auto = Some(at(0));
        judge(&mut s, Some(0.73), 70.0, at(700));
        assert!(!s.pending_auto);
    }

    #[test]
    fn compact_follows_only_a_sync_turn_that_ended_well() {
        // U5 · U6 · U17 — 다음 일이 compact 인 것은 성공한 현행화 턴뿐이다.
        let p = &default_sync_prompt();
        let syncing = |turns: Vec<Turn>| {
            let mut c = convo_with(turns);
            c.sync.phase = Phase::Syncing {
                prompt: p.into(),
                mode: Mode::Auto,
            };
            c
        };
        assert_eq!(
            next_step(&syncing(vec![turn(p, true, None)]), false, at(0)),
            Step::Nothing
        );
        assert_eq!(
            next_step(&syncing(vec![turn(p, false, None)]), false, at(0)),
            Step::Compact(Mode::Auto)
        );
        assert_eq!(
            next_step(&syncing(vec![turn(p, false, None)]), true, at(0)),
            Step::Nothing,
            "데몬이 아직 바쁘면 기다린다"
        );
        assert!(matches!(
            next_step(&syncing(vec![turn(p, false, Some("boom"))]), false, at(0)),
            Step::Fail(_)
        ));
        let mut stopped = syncing(vec![turn(p, false, Some("aborted"))]);
        stopped.stopped.push(0);
        assert!(matches!(next_step(&stopped, false, at(0)), Step::Fail(_)));
        assert!(matches!(
            next_step(&syncing(vec![]), false, at(0)),
            Step::Fail(_)
        ));
        let mut compacting = convo_with(vec![]);
        compacting.sync.phase = Phase::Compacting { mode: Mode::Manual };
        assert_eq!(next_step(&compacting, false, at(0)), Step::Nothing);
    }

    #[test]
    fn an_idle_tab_starts_auto_only_when_it_was_asked_to() {
        // U8 · U15: 넘지 않았거나 끈 탭(pending_auto 가 안 섰다)은 아무 일도 없다.
        let mut c = convo_with(vec![]);
        c.sync.last_result = Some(at(0));
        assert_eq!(next_step(&c, false, at(600)), Step::Nothing);
        c.sync.pending_auto = true;
        // 일하는 중에 끼어들지 않는다 — 턴 끝에서 AUTO_QUIET 가 지나야 연다.
        assert_eq!(next_step(&c, false, at(59)), Step::Nothing);
        assert_eq!(next_step(&c, false, at(60)), Step::StartAuto);
        assert_eq!(next_step(&c, true, at(60)), Step::Nothing);
        // 기준을 크게 넘었으면 기다리지 않는다.
        c.sync.urgent = true;
        assert_eq!(next_step(&c, false, at(1)), Step::StartAuto);
        // 회수된 세션은 열지 않는다.
        c.sync.reclaimed = true;
        assert_eq!(next_step(&c, false, at(600)), Step::Nothing);
    }

    #[test]
    fn a_reclaimed_session_gets_no_idle_sync() {
        let mut c = idle_tab(400_000);
        c.sync.reclaimed = true;
        assert!(!ready_for_idle(&c, false, at(3600), BAR, None));
    }

    #[test]
    fn the_cooldown_holds_a_second_crossing() {
        // U14: 직전 자동 수행 5분 뒤 다시 넘음 → 0, 11분 뒤 넘음 → 1.
        let mut s = SyncState {
            last_auto: Some(at(0)),
            ..Default::default()
        };
        judge(&mut s, Some(0.75), 70.0, at(300));
        assert!(!s.pending_auto);
        judge(&mut s, Some(0.40), 70.0, at(400));
        judge(&mut s, Some(0.75), 70.0, at(660));
        assert!(s.pending_auto);
    }

    #[test]
    fn a_run_in_progress_does_not_queue_another() {
        let mut s = SyncState {
            phase: Phase::Compacting { mode: Mode::Auto },
            ..Default::default()
        };
        judge(&mut s, Some(0.9), 70.0, at(0));
        assert!(!s.pending_auto);
    }

    #[test]
    fn a_failure_disarms_until_the_next_crossing() {
        // U16: 실패 뒤 계속 넘는 동안 재시도 0, 내려갔다 넘으면 1.
        let mut s = SyncState {
            over: true,
            armed: false,
            failed: Some("x".into()),
            ..Default::default()
        };
        judge(&mut s, Some(0.8), 70.0, at(0));
        assert!(!s.pending_auto);
        judge(&mut s, Some(0.3), 70.0, at(1));
        judge(&mut s, Some(0.8), 70.0, at(2));
        assert!(s.pending_auto);
    }

    fn convo_with(turns: Vec<Turn>) -> Convo {
        Convo {
            session_id: Some("s".into()),
            turns,
            ..Default::default()
        }
    }

    fn turn(sent: &str, pending: bool, error: Option<&str>) -> Turn {
        Turn {
            sent: sent.into(),
            got: String::new(),
            pending,
            at: None,
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn the_sync_turn_is_found_by_what_was_sent() {
        let p = &default_sync_prompt();
        assert_eq!(
            sync_turn(&convo_with(vec![turn(p, true, None)]), p),
            SyncTurn::Pending
        );
        assert_eq!(
            sync_turn(&convo_with(vec![turn(p, false, None)]), p),
            SyncTurn::Ok
        );
        assert_eq!(
            sync_turn(&convo_with(vec![turn(p, false, Some("boom"))]), p),
            SyncTurn::Failed("boom".into())
        );
        let mut stopped = convo_with(vec![turn(p, false, Some("aborted"))]);
        stopped.stopped.push(0);
        assert_eq!(
            sync_turn(&stopped, p),
            SyncTurn::Stopped,
            "U17: 중단은 실패와 다르다"
        );
        // 같은 문구를 예전에 보낸 적이 있으면 가장 늦은 것을 본다.
        let two = convo_with(vec![
            turn(p, false, Some("old")),
            turn("x", false, None),
            turn(p, true, None),
        ]);
        assert_eq!(sync_turn(&two, p), SyncTurn::Pending);
        assert_eq!(sync_turn(&convo_with(vec![]), p), SyncTurn::Missing);
        let mut archived = convo_with(vec![]);
        archived.older.push(turn(p, false, None));
        assert_eq!(
            sync_turn(&archived, p),
            SyncTurn::Ok,
            "보관된 턴에서도 찾는다"
        );
    }

    #[test]
    fn auto_waits_for_an_idle_tab() {
        // U12: 넘은 순간 턴이 돌면 그 턴이 끝난 뒤.
        let mut c = convo_with(vec![]);
        c.sync.pending_auto = true;
        c.sync.last_result = Some(at(0));
        assert!(!ready_for_auto(&c, true, at(60)), "턴이 도는 중");
        c.queued.push("다음".into());
        assert!(!ready_for_auto(&c, false, at(60)), "대기열이 있다");
        c.queued.clear();
        c.clearing = true;
        assert!(!ready_for_auto(&c, false, at(60)), "비우는 중");
        c.clearing = false;
        assert!(ready_for_auto(&c, false, at(60)));
        c.session_id = None;
        assert!(!ready_for_auto(&c, false, at(60)), "세션 없음");
    }

    #[test]
    fn the_status_token_follows_the_state() {
        let mut s = SyncState::default();
        assert_eq!(status_label(&s), None);
        s.over = true;
        assert_eq!(status_label(&s).map(|t| t.0), Some("sup.sync.over"));
        s.failed = Some("x".into());
        assert_eq!(status_label(&s).map(|t| t.0), Some("sup.sync.failed"));
        s.phase = Phase::Compacting { mode: Mode::Manual };
        assert_eq!(status_label(&s).map(|t| t.0), Some("sup.sync.running"));
    }

    #[test]
    fn a_clean_turn_clears_the_sync_failure_but_an_error_turn_does_not() {
        let mut s = SyncState::default();
        assert!(!recovered(&s, false), "실패가 없으면 거둘 것도 없다");
        s.failed = Some("API Error: ENOTFOUND".into());
        assert!(!recovered(&s, true), "오류 턴은 통신이 돌아온 것이 아니다");
        assert!(recovered(&s, false));
        s.phase = Phase::Syncing {
            prompt: "p".into(),
            mode: Mode::Auto,
        };
        assert!(
            !recovered(&s, false),
            "현행화가 도는 중이면 그 결과를 기다린다"
        );
    }

    #[test]
    fn the_sync_input_tags_who_started_it() {
        let p = &default_sync_prompt();
        let u = usage(742_000, 1_000_000);
        let auto = sync_input(p, Mode::Auto, &u, 400_000.0);
        let manual = sync_input(p, Mode::Manual, &u, 400_000.0);
        // 문구가 앞에 그대로 있어야 현행화 스킬이 트리거된다.
        assert!(auto.starts_with(&format!("{p}\n\n")), "{auto}");
        assert!(manual.starts_with(&format!("{p}\n\n")), "{manual}");
        assert_ne!(auto, manual, "자동과 수동이 같은 말로 나간다");
        assert!(auto.contains("742k") && auto.contains("400k"), "{auto}");
        // 작은 창은 그 90% 로 끌어내린 기준을 적는다.
        let small = sync_input(p, Mode::Auto, &usage(190_000, 200_000), 400_000.0);
        assert!(small.contains("180k"), "{small}");
        assert!(sync_input(p, Mode::Auto, &Usage::default(), 400_000.0).contains('?'));
    }

    #[test]
    fn the_tagged_sync_turn_is_still_found() {
        let sent = sync_input(
            &default_sync_prompt(),
            Mode::Auto,
            &usage(710_000, 1_000_000),
            70.0,
        );
        let mut c = convo_with(vec![turn(&sent, false, None)]);
        c.sync.phase = Phase::Syncing {
            prompt: sent,
            mode: Mode::Auto,
        };
        assert_eq!(next_step(&c, false, at(0)), Step::Compact(Mode::Auto));
    }

    /// 보내는 말과 턴을 찾는 말이 같은 값이다 — 하나만 꼬리표 없는 문구로 돌아가면
    /// 현행화 턴을 못 찾아(`lost`) compact 가 영영 안 나간다.
    #[test]
    fn start_sync_sends_and_tracks_the_same_tagged_text() {
        let src = include_str!("sync_compact.rs");
        let body = src
            .split("fn start_sync(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .expect("start_sync 가 없다");
        assert!(
            body.contains("let sent = sync_input("),
            "꼬리표를 안 만든다"
        );
        assert!(
            body.contains("prompt: sent.clone(),"),
            "꼬리표 없는 말로 턴을 찾는다"
        );
        assert!(
            body.contains("self.send_turn(host, sid, &sent, None);"),
            "꼬리표 없는 말을 보낸다"
        );
    }

    /// 꼬리표는 로케일과 무관한 식별자로 시작한다 — 대화 기록을 그 식별자로 가른다.
    #[test]
    fn every_locale_starts_the_tag_with_the_fixed_marker() {
        for (name, src) in [
            ("en", include_str!("../../lang/en.toml")),
            ("ko", include_str!("../../lang/ko.toml")),
            ("ja", include_str!("../../lang/ja.toml")),
            ("zh-Hans", include_str!("../../lang/zh-Hans.toml")),
        ] {
            for (key, marker) in [
                ("tag_auto", "[sync-compact:auto] "),
                ("tag_manual", "[sync-compact:manual] "),
                ("tag_idle", "[sync-compact:idle] "),
            ] {
                let want = format!("{key} = \"{marker}");
                assert_eq!(src.matches(&want).count(), 1, "{name}: {want}");
            }
        }
    }

    /// 쉰 탭 — 마지막 턴 끝이 기준 시각, 점유 `used`/1M, 캐시 종류 모름(5분으로 본다).
    fn idle_tab(used: u64) -> Convo {
        let mut c = convo_with(vec![turn("앞의 일", false, None)]);
        c.usage = usage(used, 1_000_000);
        c.sync.last_result = Some(at(0));
        c
    }

    const BAR: f64 = 300_000.0;

    #[test]
    fn an_idle_tab_is_synced_before_reclaim_only_past_the_wait_and_the_bar() {
        let after = 240; // 5분 캐시 − 1분
        let c = idle_tab(300_000);
        // 기준 이상 · 4분 쉼 → 연다. 기준은 포함이다(토큰 수, 창 크기와 무관).
        assert!(ready_for_idle(&c, false, at(after), BAR, None));
        // 아직 덜 쉬었다.
        assert!(!ready_for_idle(&c, false, at(after - 1), BAR, None));
        // 기준 아래.
        assert!(!ready_for_idle(
            &idle_tab(299_999),
            false,
            at(after),
            BAR,
            None
        ));
        // 끔 · 바쁨.
        assert!(!ready_for_idle(&c, false, at(after), 0.0, None));
        assert!(!ready_for_idle(&c, true, at(after), BAR, None));
        // 이번 쉼에 이미 했다 — 바닥값 세션이 되풀이하지 않는다.
        let mut spent = idle_tab(900_000);
        spent.sync.idle_spent = true;
        assert!(!ready_for_idle(&spent, false, at(after * 3), BAR, None));
        // 턴 끝을 본 적이 없다(plugin 이 막 다시 떴다).
        let mut fresh = idle_tab(900_000);
        fresh.sync.last_result = None;
        assert!(!ready_for_idle(&fresh, false, at(after * 3), BAR, None));
        // 현행화가 이미 도는 중.
        let mut running = idle_tab(900_000);
        running.sync.phase = Phase::Compacting { mode: Mode::Idle };
        assert!(!ready_for_idle(&running, false, at(after), BAR, None));
    }

    /// 쉰 탭은 기준 이상이면 현행화(회수는 그 뒤), 미만이면 회수가 켜졌을 때만 바로 내린다. 덜 쉬었거나
    /// 할 일이 남은 탭 · 회수를 이미 시도한 탭 · 이미 회수된 탭은 건드리지 않는다.
    #[test]
    fn a_rested_tab_is_synced_over_the_bar_and_released_under_it() {
        let after = 240;
        let on = Knobs {
            turn_tokens: 0.0,
            idle_tokens: BAR,
            reclaim: true,
            prompt: String::new(),
            compact: String::new(),
            cache_ttl: Default::default(),
            memory_saver: true,
        };
        let off = Knobs {
            reclaim: false,
            ..on.clone()
        };
        let zero = Knobs {
            idle_tokens: 0.0,
            ..on.clone()
        };
        assert_eq!(
            idle_step(&idle_tab(300_000), false, at(after), &on),
            IdleStep::Sync
        );
        assert_eq!(
            idle_step(&idle_tab(300_000), false, at(after), &off),
            IdleStep::Sync
        );
        assert_eq!(
            idle_step(&idle_tab(299_999), false, at(after), &on),
            IdleStep::Release
        );
        assert_eq!(
            idle_step(&idle_tab(299_999), false, at(after), &off),
            IdleStep::Nothing
        );
        assert_eq!(
            idle_step(&idle_tab(900_000), false, at(after), &zero),
            IdleStep::Release
        );
        assert_eq!(
            idle_step(&idle_tab(10), false, at(after - 1), &on),
            IdleStep::Nothing
        );
        assert_eq!(
            idle_step(&idle_tab(10), true, at(after), &on),
            IdleStep::Nothing
        );
        let mut asking = idle_tab(10);
        asking.queued.push("next".into());
        assert_eq!(idle_step(&asking, false, at(after), &on), IdleStep::Nothing);
        let mut spent = idle_tab(10);
        spent.sync.release_spent = true;
        assert_eq!(idle_step(&spent, false, at(after), &on), IdleStep::Nothing);
        // 현행화를 쓴 쉼도 회수는 따로 한다 — 회수 표지는 `release_spent` 다.
        let mut synced = idle_tab(300_000);
        synced.sync.idle_spent = true;
        assert_eq!(idle_step(&synced, false, at(after), &on), IdleStep::Release);
        let mut gone = idle_tab(10);
        gone.sync.reclaimed = true;
        assert_eq!(idle_step(&gone, false, at(after), &on), IdleStep::Nothing);
    }

    /// 백그라운드 현행화는 프로필마다 하나씩 — 같은 프로필의 유휴·한도 현행화 · 429 재전송 대기 · 429 차단이
    /// 있으면 미룬다. 다른 프로필의 일은 막는 사유가 아니고, 차단기가 탐침을 허락하면 연다.
    #[test]
    fn a_background_sync_waits_for_its_own_profile_only() {
        use crate::domain::rate_block::Blocks;
        let tab = |profile: &str| {
            let mut c = idle_tab(10);
            c.agent_name = Some(profile.into());
            c
        };
        let now = at(1_000);
        let none = Blocks::default();
        let quiet = tab("a");
        assert_eq!(background_sync_hold([&quiet], "a", &none, now), None);

        let mut blocked = Blocks::default();
        blocked.note_rejected("a", at(1_000 - 30));
        assert_eq!(
            background_sync_hold([&quiet], "a", &blocked, now),
            Some(Hold::RateLimited)
        );
        assert_eq!(
            background_sync_hold([&quiet], "b", &blocked, now),
            None,
            "다른 프로필의 429 로 막았다"
        );
        assert_eq!(
            background_sync_hold([&quiet], "a", &blocked, at(1_000 + 30)),
            None,
            "기다림이 끝났는데 탐침을 안 연다"
        );

        for (mode, held) in [
            (Mode::Idle, true),
            (Mode::Auto, false),
            (Mode::Manual, false),
        ] {
            let mut other = tab("a");
            other.sync.phase = Phase::Syncing {
                prompt: "p".into(),
                mode,
            };
            assert_eq!(
                background_sync_hold([&quiet, &other], "a", &none, now).is_some(),
                held,
                "{mode:?}"
            );
            other.sync.phase = Phase::Compacting { mode };
            assert_eq!(
                background_sync_hold([&quiet, &other], "a", &none, now).is_some(),
                held,
                "compact 중 {mode:?}"
            );
            assert_eq!(
                background_sync_hold([&quiet, &other], "b", &none, now),
                None,
                "다른 프로필의 현행화로 막았다 {mode:?}"
            );
        }
        let mut waiting = tab("a");
        waiting.retry.pending = Some(crate::domain::retry_429::Pending {
            text: "일".into(),
            due: now,
        });
        assert_eq!(
            background_sync_hold([&quiet, &waiting], "a", &none, now),
            Some(Hold::RetryPending)
        );
        assert_eq!(
            background_sync_hold([&quiet, &waiting], "b", &none, now),
            None
        );
    }

    /// 1시간 캐시 세션은 59분을 기다린다 — 5분 쉼마다 끼어들지 않는다.
    #[test]
    fn a_one_hour_cache_waits_until_a_minute_before_it_expires() {
        let mut c = idle_tab(400_000);
        c.usage.cache_1h = Some(true);
        assert_eq!(idle_wait(&c.usage, None), Duration::from_secs(3540));
        assert!(!ready_for_idle(&c, false, at(240), BAR, None));
        assert!(!ready_for_idle(&c, false, at(3539), BAR, None));
        assert!(ready_for_idle(&c, false, at(3540), BAR, None));
        c.usage.cache_1h = Some(false);
        assert_eq!(idle_wait(&c.usage, None), Duration::from_secs(240));
        c.usage.cache_1h = None;
        assert_eq!(
            idle_wait(&c.usage, None),
            Duration::from_secs(240),
            "모르면 5분으로 본다"
        );
    }

    /// 관측이 없을 때만 프로필 수명을 쓴다 — 관측한 5분(강등)은 프로필 1시간보다 앞선다.
    #[test]
    fn the_profile_ttl_fills_in_only_when_the_tab_has_not_seen_the_cache() {
        use crate::adapters::driven::store::agents::CacheTtl;
        let mut c = idle_tab(10);
        c.usage.cache_1h = None;
        assert_eq!(
            idle_wait(&c.usage, Some(CacheTtl::OneHour)),
            Duration::from_secs(3540)
        );
        assert_eq!(
            idle_wait(&c.usage, Some(CacheTtl::FiveMin)),
            Duration::from_secs(240)
        );
        c.usage.cache_1h = Some(false);
        assert_eq!(
            idle_wait(&c.usage, Some(CacheTtl::OneHour)),
            Duration::from_secs(240),
            "관측한 강등을 프로필이 덮었다"
        );
        c.usage.cache_1h = Some(true);
        assert_eq!(
            idle_wait(&c.usage, Some(CacheTtl::FiveMin)),
            Duration::from_secs(3540)
        );
        // 판정도 탭의 프로필 이름으로 수명을 찾는다.
        let mut k = read_knobs_for_test();
        c.usage.cache_1h = None;
        c.agent_name = Some("dev".into());
        k.cache_ttl.insert("dev".into(), CacheTtl::OneHour);
        assert_eq!(idle_step(&c, false, at(240), &k), IdleStep::Nothing);
        assert_eq!(idle_step(&c, false, at(3540), &k), IdleStep::Release);
    }

    fn read_knobs_for_test() -> Knobs {
        Knobs {
            turn_tokens: 0.0,
            idle_tokens: BAR,
            reclaim: true,
            prompt: String::new(),
            compact: String::new(),
            cache_ttl: Default::default(),
            memory_saver: true,
        }
    }

    /// 명령은 휴식 시계만 민다 — 현행화는 캐시 시계대로 열리고, 회수는 마지막 조작에서 다시 잰다.
    #[test]
    fn a_command_delays_the_release_but_not_the_cache_sync() {
        let k = read_knobs_for_test();
        // 기준 이상: 캐시 시계(0초) 기준 240초에 현행화가 열린다 — 200초의 조작과 무관하다.
        let mut big = idle_tab(300_000);
        big.sync.touch(at(200));
        assert_eq!(idle_step(&big, false, at(240), &k), IdleStep::Sync);
        // 현행화를 썼다 — 회수는 휴식 시계(200초 + 240초)가 차야 한다.
        big.sync.idle_spent = true;
        assert_eq!(idle_step(&big, false, at(300), &k), IdleStep::Nothing);
        assert_eq!(idle_step(&big, false, at(440), &k), IdleStep::Release);
        // 기준 미만: 회수만 — 역시 휴식 시계를 본다.
        let mut small = idle_tab(10);
        small.sync.touch(at(200));
        assert_eq!(idle_step(&small, false, at(240), &k), IdleStep::Nothing);
        assert_eq!(idle_step(&small, false, at(440), &k), IdleStep::Release);
        // 시도한 회수는 되풀이하지 않고, 다음 조작이 다시 연다.
        small.sync.release_spent = true;
        assert_eq!(idle_step(&small, false, at(900), &k), IdleStep::Nothing);
        small.sync.touch(at(900));
        assert!(!small.sync.release_spent);
        assert_eq!(idle_step(&small, false, at(1140), &k), IdleStep::Release);
    }

    /// 현행화가 남은 세션은 회수보다 현행화가 먼저다 — 429 로 미뤄진 동안 내리면 정리 없이 내린다.
    #[test]
    fn an_owed_sync_holds_the_release() {
        let k = read_knobs_for_test();
        let c = idle_tab(300_000);
        assert!(sync_owed(&c, k.idle_tokens));
        assert_eq!(idle_step(&c, false, at(240), &k), IdleStep::Sync);
        assert!(!sync_owed(&c, 0.0));
    }

    /// 턴 끝은 사람이 연 턴만 휴식 시계를 민다 — 자동 현행화 턴은 캐시 시계만 민다.
    #[test]
    fn only_a_person_turn_end_moves_the_rest_clock() {
        let body = |name: &str| {
            let src = include_str!("sync_compact.rs");
            let at = src.find(name).expect("함수가 없다");
            src[at..]
                .split("\n    }\n")
                .next()
                .unwrap_or("")
                .to_string()
        };
        let judge = body("fn judge_after_result(");
        assert!(judge.contains("if !c.sync.running() {"));
        assert!(judge.contains("c.sync.touch(now);"));
        assert!(
            judge.find("c.sync.last_result = Some(now);") < judge.find("c.sync.touch(now);"),
            "캐시 시계를 안 민다"
        );
    }

    /// 비우기는 판정을 처음부터 다시 하되 휴식 시계는 민다 — 캐시 시계는 비어 유휴 현행화가 안 열린다.
    #[test]
    fn a_clear_restarts_the_judgement_and_moves_only_the_rest_clock() {
        let mut c = idle_tab(300_000);
        c.sync.idle_spent = true;
        crate::application::clear::reset(&mut c);
        assert!(c.sync.last_result.is_none());
        assert!(c.sync.last_activity.is_some());
        assert!(!c.sync.idle_spent);
        assert!(!c.sync.release_spent);
    }

    /// 회수 표지는 `release_due` 를 본 뒤에 켠다 — 먼저 켜면 회수 판정이 늘 거짓이라 회수가 안 돈다
    /// (VM 실측에서 그랬다).
    #[test]
    fn the_release_mark_follows_the_release_check() {
        let src = include_str!("sync_compact.rs");
        let at = src.find("fn run_idle_syncs(").expect("함수가 없다");
        let body = &src[at..at + src[at..].find("\n    }\n").expect("끝이 없다")];
        let check = body.find("self.release_due(").expect("회수 판정을 안 본다");
        let marks: Vec<usize> = body
            .match_indices("release_spent = true")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(marks.len(), 1, "회수 표지를 켜는 자리가 하나가 아니다");
        assert!(check < marks[0], "회수 판정 전에 표지를 켠다");
    }

    #[test]
    fn tab_actions_are_tagged_navigations() {
        assert!(is_tab_action("about:blank#agent-send=1&t=hi"));
        assert!(is_tab_action("about:blank#agent-copy-cwd=1"));
        assert!(!is_tab_action("about:blank"));
        assert!(!is_tab_action("about:blank#"));
        assert!(TOUCHING_METHODS.contains(&"agent_session.add_dir"));
        assert!(!TOUCHING_METHODS.contains(&"agent_session.sleep"));
    }

    /// 사람이 다음 말을 보내면 새 쉼이다 — 다시 열린다. 현행화 턴 자신은 풀지 않는다.
    #[test]
    fn a_new_message_reopens_the_idle_sync_but_the_sync_turn_does_not() {
        let mut c = idle_tab(900_000);
        c.sync.idle_spent = true;
        c.sync.phase = Phase::Syncing {
            prompt: "p".into(),
            mode: Mode::Idle,
        };
        crate::domain::convo::begin_turn(&mut c, "p");
        assert!(c.sync.idle_spent, "현행화 턴이 유휴 표시를 풀었다");
        c.sync.phase = Phase::Idle;
        crate::domain::convo::begin_turn(&mut c, "다음 일");
        assert!(
            !c.sync.idle_spent,
            "사람의 말이 유휴 현행화를 다시 열지 않았다"
        );
    }

    #[test]
    fn the_idle_sync_tag_names_the_bar() {
        let u = usage(384_500, 1_000_000);
        let t = sync_input(&default_sync_prompt(), Mode::Idle, &u, BAR);
        assert!(
            t.starts_with(&format!("{}\n\n", default_sync_prompt())),
            "{t}"
        );
        assert!(t.contains("384k") && t.contains("300k"), "{t}");
        assert_ne!(t, sync_input(&default_sync_prompt(), Mode::Auto, &u, 70.0));
    }

    /// 2초 폴링 자리가 유휴 현행화를 부르고, 시작 전에 이번 쉼을 쓴 것으로 적고, 턴 끝이 시각을 남긴다.
    #[test]
    fn the_poll_runs_idle_syncs_and_turn_ends_stamp_the_time() {
        let src = include_str!("sync_compact.rs");
        let body = |name: &str| {
            src.split(name)
                .nth(1)
                .and_then(|s| s.split("\n    }\n").next())
                .unwrap_or_else(|| panic!("{name} 가 없다"))
                .to_string()
        };
        assert!(body("fn run_pending_syncs(").contains("self.run_idle_syncs(&host);"));
        let idle = body("fn run_idle_syncs(");
        assert!(idle.contains("idle_step("), "판정을 안 거친다");
        assert!(
            idle.contains("self.idle_reclaim(host, sid);"),
            "기준 미만 탭을 안 내린다"
        );
        assert!(
            body("fn after_idle_sync(").contains("self.idle_reclaim(host, sid);"),
            "현행화 뒤에 안 내린다"
        );
        assert!(
            idle.contains("c.sync.idle_spent = true;"),
            "쉼을 쓴 것으로 안 적는다"
        );
        assert!(idle.contains("Mode::Idle"), "유휴로 안 연다");
        // 막혔으면 쉼을 쓴 것으로 적기 전에 넘긴다 — 적고 넘기면 그 쉼에는 다시 안 열린다.
        let hold_at = idle.find("let Some(why) = hold").expect("막는 판정이 없다");
        assert!(
            hold_at < idle.find("c.sync.idle_spent = true;").unwrap_or(0),
            "쉼을 쓴 뒤에 막는다"
        );
        assert!(
            idle.contains("background_sync_hold("),
            "유휴 현행화가 줄을 안 선다"
        );
        assert!(
            idle.contains("opened.insert(profile);") && idle.contains("Hold::OpenedThisPass"),
            "한 판정에서 같은 프로필에 여러 탭을 연다"
        );
        assert!(
            idle.contains("self.blocks.take_probe(&profile, now);"),
            "탐침으로 적지 않는다"
        );
        assert!(
            body("fn after_idle_sync(").contains("self.blocks.gate(&profile"),
            "막힌 프로필의 세션을 회수한다"
        );
        assert!(body("fn judge_after_result(").contains("c.sync.last_result = Some(now);"));
        // 유휴 현행화의 꼬리표는 유휴 기준을 적는다 — 임계 초과 기준(70%)과 섞이면 기록이 틀린다.
        assert!(
            body("fn start_sync(").contains("knobs.idle_tokens"),
            "유휴 기준을 안 쓴다"
        );
    }

    /// 껍데기 메서드를 가짜 호스트로 잰다 — 실패한 현행화는 멈추고, 사유를 탭에 띄우고, 실패 표지를 남긴다.
    /// 유휴 현행화가 아니므로 회수(데몬 호출)로 가지 않는다.
    #[test]
    fn a_failed_sync_stops_and_shows_its_reason_through_the_host() {
        let host = crate::application::surface_link::tests_support::SharedHost::default();
        let mut plugin = crate::test_support::TestPlugin::default();
        let mut c = Convo {
            session_id: Some("s1".into()),
            ..Default::default()
        };
        c.sync.phase = Phase::Compacting { mode: Mode::Manual };
        c.sync.armed = true;
        c.sync.pending_auto = true;
        plugin.convos.insert(7, c);

        plugin.sync_failed(&host, 7, "boom".into());

        let c = &plugin.convos[&7];
        assert!(matches!(c.sync.phase, Phase::Idle), "현행화가 안 멈췄다");
        assert!(!c.sync.armed && !c.sync.pending_auto, "자동 수행이 남았다");
        assert_eq!(c.sync.failed.as_deref(), Some("boom"));
        let posted = host.posted_to(7);
        assert!(
            posted.contains(&clear::note_msg(Some("boom"))),
            "사유를 탭에 안 띄웠다: {posted:?}"
        );
        assert!(
            posted.iter().any(|m| m["type"] == "status.set"),
            "상태줄을 다시 안 그렸다: {posted:?}"
        );
        assert!(host.invoked().is_empty(), "워커를 띄울 자리가 아니다");
    }

    /// 실패 표지가 있던 탭에 오류 없는 턴이 끝나면 표지를 지우고 안내를 걷는다 — 같은 가짜 호스트로 잰다.
    #[test]
    fn a_clean_turn_after_a_failure_clears_the_note_through_the_host() {
        let host = crate::application::surface_link::tests_support::SharedHost::default();
        let mut plugin = crate::test_support::TestPlugin::default();
        let mut c = Convo {
            session_id: Some("s1".into()),
            ..Default::default()
        };
        c.sync.failed = Some("earlier".into());
        plugin.convos.insert(7, c);

        plugin.judge_after_result(&host, 7);

        assert_eq!(plugin.convos[&7].sync.failed, None, "실패 표지가 남았다");
        let posted = host.posted_to(7);
        assert!(
            posted.contains(&clear::note_msg(None)),
            "안내를 안 걷었다: {posted:?}"
        );
        // 설정은 호스트에서 읽는다 — 가짜가 답한 null 은 기본값이 된다.
        assert!(
            host.calls()
                .iter()
                .any(|(m, _)| m == "settings.get_plugin_setting"),
            "설정을 안 읽었다"
        );
    }

    #[test]
    fn the_compact_input_carries_the_prompt() {
        assert_eq!(compact_input("요지만"), "/compact 요지만");
    }

    struct Settings(Vec<(&'static str, Value)>);
    impl HostCall for Settings {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            assert_eq!(method, "settings.get_plugin_setting");
            let key = params["storage_key"].as_str().unwrap_or_default();
            let v = self
                .0
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Null);
            Ok(json!({ "value": v }))
        }
    }

    /// 수명 안에서는 호스트에 다시 묻지 않고, 수명이 지나면 바뀐 값을 읽는다.
    #[test]
    fn the_knob_cache_asks_the_host_once_per_ttl() {
        struct Counting(std::cell::Cell<u32>, std::cell::Cell<f64>);
        impl HostCall for Counting {
            fn call(&self, _: &str, params: Value) -> Result<Value, String> {
                self.0.set(self.0.get() + 1);
                let v = if params["storage_key"] == IDLE_KEY {
                    json!(self.1.get())
                } else {
                    Value::Null
                };
                Ok(json!({ "value": v }))
            }
        }
        let host = Counting(std::cell::Cell::new(0), std::cell::Cell::new(100_000.0));
        let mut cache = KnobCache::default();
        let t0 = Instant::now();
        assert_eq!(cache.get(&host, t0).idle_tokens, 100_000.0);
        let per_read = host.0.get();
        assert!(per_read > 0);
        host.1.set(200_000.0);
        for i in 0..50 {
            cache.get(&host, t0 + std::time::Duration::from_millis(i * 100));
        }
        assert_eq!(host.0.get(), per_read, "수명 안에서 다시 물었다");
        assert_eq!(cache.get(&host, t0 + KNOBS_TTL).idle_tokens, 200_000.0);
        assert_eq!(host.0.get(), per_read * 2);
    }

    /// 탭 쪽 판정 자리는 모두 캐시를 거친다 — 하나라도 호스트에 바로 물으면 그 자리가 다시
    /// 흐름 이벤트마다 설정을 읽는다. 시험 모듈 앞까지만 본다.
    #[test]
    fn the_tab_side_reads_settings_through_the_cache() {
        let src = include_str!("sync_compact.rs");
        let shipped = &src[..src.find("#[cfg(test)]").unwrap()];
        let tab_side = &shipped[shipped
            .find("crate::application::plugin::SupervisorPlugin<H, S>")
            .unwrap()..];
        assert!(
            !tab_side.contains("read_knobs("),
            "판정 자리가 캐시를 건너뛴다"
        );
        assert!(tab_side.matches("self.knobs.get(").count() >= 4);
    }

    #[test]
    fn untouched_and_empty_settings_fall_back_to_defaults() {
        let k = read_knobs(&Settings(vec![]));
        assert_eq!(k.turn_tokens, DEFAULT_THRESHOLD_TOKENS);
        assert_eq!(k.idle_tokens, DEFAULT_IDLE_TOKENS);
        assert_eq!(k.prompt, default_sync_prompt());
        assert_eq!(k.compact, default_compact_prompt());
        let k = read_knobs(&Settings(vec![
            (PROMPT_KEY, json!("   ")),
            (THRESHOLD_KEY, json!(5_000_000.0)),
            (COMPACT_KEY, json!("요지만")),
        ]));
        assert_eq!(k.prompt, default_sync_prompt(), "빈 칸은 기본값");
        assert_eq!(k.turn_tokens, IDLE_TOKENS_MAX, "위 끝으로 끌어온다");
        assert_eq!(k.compact, "요지만");
    }

    /// 설정 창의 묶음 스위치를 끄면 그 기준을 0(끔)으로 읽는다 — 기준 칸에 남은 값과 상관없다. 다른 기준과
    /// 유휴 회수는 그대로다. 켜면 기준 칸의 값이 돌아온다.
    #[test]
    fn a_switched_off_group_turns_its_sync_off() {
        let on = |turn: bool, idle: bool| {
            read_knobs(&Settings(vec![
                (THRESHOLD_ON_KEY, json!(turn)),
                (THRESHOLD_KEY, json!(300_000.0)),
                (IDLE_ON_KEY, json!(idle)),
                (IDLE_KEY, json!(120_000.0)),
            ]))
        };
        let k = on(false, true);
        assert_eq!((k.turn_tokens, k.idle_tokens), (0.0, 120_000.0));
        assert!(k.reclaim, "유휴 회수는 따로 끈다");
        let k = on(true, false);
        assert_eq!((k.turn_tokens, k.idle_tokens), (300_000.0, 0.0));
        let k = on(true, true);
        assert_eq!((k.turn_tokens, k.idle_tokens), (300_000.0, 120_000.0));
        // 끄면 판정이 실제로 안 연다 — 0 이 끔이라는 기존 계약을 그대로 탄다.
        assert_eq!(
            effective_threshold(&Usage::default(), on(false, true).turn_tokens),
            None
        );
        assert!(!ready_for_idle(
            &idle_tab(900_000),
            false,
            at(240),
            on(true, false).idle_tokens,
            None
        ));
        assert!(ready_for_idle(
            &idle_tab(900_000),
            false,
            at(240),
            on(true, true).idle_tokens,
            None
        ));
    }

    #[test]
    fn the_done_call_carries_the_surface_and_the_reason() {
        assert_eq!(
            parse_done(&json!({ "surface_id": 7, "ok": true, "error": "" })),
            Some((7, Ok(())))
        );
        assert_eq!(
            parse_done(&json!({ "surface_id": 7, "ok": false, "error": "boom" })),
            Some((7, Err("boom".into())))
        );
    }

    /// 매니페스트 기본값 ↔ 코드 기본값 — 어긋나면 설정 창이 보이는 값과 실제로 쓰는 값이 갈린다.
    /// 칸 하나를 `[[contributes.settings_pages.items]]` 블록 단위로 잘라 읽는다.
    fn manifest_field(key: &str, field: &str) -> String {
        let m = include_str!("../../tasty-plugin.toml");
        let block = m
            .split("[[contributes.settings_pages.items]]")
            .find(|b| b.contains(&format!("storage_key = \"{key}\"")))
            .unwrap_or_else(|| panic!("매니페스트에 {key} 칸이 없다"));
        block
            .lines()
            .find_map(|l| l.trim().strip_prefix(&format!("{field} = ")))
            .unwrap_or_else(|| panic!("{key} 칸에 {field} 가 없다"))
            .trim()
            .trim_matches('"')
            .to_string()
    }

    /// 설정 페이지의 항목 덩어리 — 선언 순서 그대로.
    fn manifest_items() -> Vec<&'static str> {
        include_str!("../../tasty-plugin.toml")
            .split("[[contributes.settings_pages.items]]")
            .skip(1)
            .map(|b| b.split("\n[").next().unwrap_or(b))
            .collect()
    }

    fn item_field<'a>(block: &'a str, field: &str) -> Option<&'a str> {
        block
            .lines()
            .find_map(|l| l.trim().strip_prefix(&format!("{field} = ")))
            .map(|v| v.trim().trim_matches('"'))
    }

    /// 묶음 머리 `id` 에 머리 스위치(`storage_key`)가 있나.
    fn group_has_switch(id: &str) -> bool {
        manifest_items()
            .into_iter()
            .find(|b| item_field(b, "kind") == Some("group") && item_field(b, "id") == Some(id))
            .unwrap_or_else(|| panic!("{id} 묶음이 없다"))
            .contains("storage_key = ")
    }

    /// 저장 키로 찾은 항목의 선언 자리.
    fn item_index(storage_key: &str) -> usize {
        manifest_items()
            .iter()
            .position(|b| item_field(b, "storage_key") == Some(storage_key))
            .unwrap_or_else(|| panic!("{storage_key} 칸이 없다"))
    }

    /// 묶음은 대화 기록 보관 → 컨텍스트 비용 자동 관리 → 메모리 절약 → 기타 순서이고, 묶음마다 칸 수가
    /// 정해져 있다 — 칸이 다른 묶음으로 새거나 빠지면 걸린다. 머리 스위치는 컨텍스트 묶음에만 있다.
    /// 기능 설명은 따로 된 글 칸이 아니라 스위치 아래 설명(`caption_key`)이다.
    #[test]
    fn the_settings_page_keeps_its_groups_in_order() {
        let items = manifest_items();
        let groups: Vec<&str> = items
            .iter()
            .filter(|b| item_field(b, "kind") == Some("group"))
            .filter_map(|b| item_field(b, "id"))
            .collect();
        assert_eq!(groups, ["history", "context", "memory", "etc"]);
        let count = |g: &str| {
            items
                .iter()
                .filter(|b| item_field(b, "group") == Some(g))
                .count()
        };
        assert_eq!(
            groups.iter().map(|g| count(g)).collect::<Vec<_>>(),
            [2, 6, 1, 1]
        );
        let switched: Vec<&&str> = groups.iter().filter(|g| group_has_switch(g)).collect();
        assert_eq!(switched, [&"context"]);
        for (key, about) in [
            (THRESHOLD_ON_KEY, "sync"),
            (IDLE_ON_KEY, "idle_sync"),
            (RECLAIM_KEY, "session_optimize"),
            (crate::application::cache_watch::SETTING_KEY, "cache_watch"),
            (CONTEXT_ON_KEY, "context"),
        ] {
            assert_eq!(
                manifest_field(key, "caption_key"),
                format!("sup.about.{about}"),
                "{key}"
            );
        }
        assert!(
            !items
                .iter()
                .any(|b| item_field(b, "default_key").is_some_and(|k| k.starts_with("sup.about."))),
            "설명이 따로 된 글 칸으로 남았다"
        );
    }

    /// 컨텍스트 묶음 머리 스위치를 끄면 두 현행화가 모두 꺼진다 — 기능 스위치 · 기준 칸 값과 상관없다. 유휴
    /// 회수는 다른 묶음이라 그대로다. 다시 켜면 기능 스위치 값이 돌아온다.
    #[test]
    fn the_context_switch_turns_every_sync_off() {
        assert_eq!(manifest_field(CONTEXT_ON_KEY, "kind"), "group");
        assert_eq!(
            manifest_field(CONTEXT_ON_KEY, "default"),
            DEFAULT_CONTEXT_ON.to_string()
        );
        let read = |on: bool| {
            read_knobs(&Settings(vec![
                (CONTEXT_ON_KEY, json!(on)),
                (THRESHOLD_KEY, json!(300_000.0)),
                (IDLE_KEY, json!(120_000.0)),
            ]))
        };
        let k = read(false);
        assert_eq!((k.turn_tokens, k.idle_tokens), (0.0, 0.0));
        assert!(k.reclaim, "유휴 회수는 따로 끈다");
        let k = read(true);
        assert_eq!((k.turn_tokens, k.idle_tokens), (300_000.0, 120_000.0));
        assert_eq!(
            read_knobs(&Settings(vec![])).turn_tokens,
            DEFAULT_THRESHOLD_TOKENS,
            "저장값이 없으면 켬"
        );
    }

    /// 기능마다 붙인 왜 · 언제 글은 코드가 쓰는 수를 적는다 — 칸이 없어 화면에서 고칠 수 없으므로 글이 틀리면
    /// 사용자는 틀린 값을 믿는다. 세 언어 모두 본다.
    #[test]
    fn the_setting_descriptions_state_the_numbers_the_code_uses() {
        let grouped = |n: u64| {
            if n >= 1000 {
                format!("{},{:03}", n / 1000, n % 1000)
            } else {
                n.to_string()
            }
        };
        let short = grouped(CACHE_TTL_5M.saturating_sub(IDLE_LEAD).as_secs());
        let long = grouped(CACHE_TTL_1H.saturating_sub(IDLE_LEAD).as_secs());
        let urgent = format!(
            "{},{:03}",
            AUTO_URGENT_OVER as u64 / 1000,
            AUTO_URGENT_OVER as u64 % 1000
        );
        let rewrite = grouped(crate::application::cache_watch::MIN_REWRITE);
        let want: [(&str, Vec<String>); 4] = [
            (
                "sync",
                vec![
                    AUTO_QUIET.as_secs().to_string(),
                    urgent,
                    format!("{}%", (WINDOW_CAP * 100.0).round()),
                    (COOLDOWN.as_secs() / 60).to_string(),
                ],
            ),
            ("idle_sync", vec![short.clone(), long.clone()]),
            ("session_optimize", vec![short, long]),
            ("cache_watch", vec![rewrite]),
        ];
        for (lang, src) in [
            ("ko", include_str!("../../lang/ko.toml")),
            ("en", include_str!("../../lang/en.toml")),
            ("ja", include_str!("../../lang/ja.toml")),
            ("zh-Hans", include_str!("../../lang/zh-Hans.toml")),
        ] {
            let sec = src
                .split("[sup.about]")
                .nth(1)
                .and_then(|s| s.split("\n[").next())
                .unwrap_or_else(|| panic!("{lang}: [sup.about] 가 없다"));
            for (key, nums) in &want {
                let line = sec
                    .lines()
                    .find(|l| l.starts_with(&format!("{key} = ")))
                    .unwrap_or_else(|| panic!("{lang}: sup.about.{key} 가 없다"));
                for n in nums {
                    assert!(line.contains(n.as_str()), "{lang} {key}: {n} 이 없다");
                }
            }
        }
    }

    #[test]
    fn the_sync_prompts_are_editable_text_with_a_default() {
        // 문구 칸(`sync_prompt` · `compact_prompt`)은 고칠 수 있는 글이고 기본 글은 코드와 같은 번역 키다.
        // 고친 값을 따르고 비우면 기본 글이다(`text_or`). 두 현행화가 함께 쓰므로 컨텍스트 묶음에 든다.
        for (key, default_key) in [
            (PROMPT_KEY, "sup.default.sync_prompt"),
            (COMPACT_KEY, "sup.default.compact_prompt"),
        ] {
            assert_eq!(manifest_field(key, "kind"), "text", "{key}");
            assert_eq!(manifest_field(key, "editable"), "true", "{key}");
            assert_eq!(manifest_field(key, "default_key"), default_key);
            assert!(
                include_str!("sync_compact.rs")
                    .contains(&format!("crate::domain::i18n::t(\"{default_key}\")"))
            );
            assert_eq!(
                manifest_field(key, "group"),
                manifest_field(CONTEXT_ON_KEY, "id")
            );
        }
    }

    #[test]
    fn manifest_defaults_match_the_code() {
        let num = |k: &str, f: &str| manifest_field(k, f).parse::<f64>().expect("number");
        assert_eq!(num(THRESHOLD_KEY, "default"), DEFAULT_THRESHOLD_TOKENS);
        assert_eq!(num(THRESHOLD_KEY, "max"), IDLE_TOKENS_MAX);
        assert_eq!(num(IDLE_KEY, "default"), DEFAULT_IDLE_TOKENS);
        assert_eq!(num(IDLE_KEY, "max"), IDLE_TOKENS_MAX);
        assert_eq!(
            manifest_field(RECLAIM_KEY, "default"),
            DEFAULT_RECLAIM.to_string()
        );
        assert_eq!(
            manifest_field(THRESHOLD_ON_KEY, "default"),
            DEFAULT_THRESHOLD_ON.to_string()
        );
        assert_eq!(
            manifest_field(IDLE_ON_KEY, "default"),
            DEFAULT_IDLE_ON.to_string()
        );
        // 두 현행화의 스위치는 한 묶음 안의 토글이고 기준 칸이 바로 뒤에 같은 묶음으로 온다. 그 묶음은 머리
        // 스위치가 없어 한 기능을 꺼도 다른 기능의 칸이 숨지 않는다. 저장 키는 예전 머리 스위치와 같다.
        let mut groups = Vec::new();
        for (on, key) in [(THRESHOLD_ON_KEY, THRESHOLD_KEY), (IDLE_ON_KEY, IDLE_KEY)] {
            assert_eq!(manifest_field(on, "kind"), "toggle", "{on}");
            let group = manifest_field(on, "group");
            assert_eq!(manifest_field(key, "group"), group, "{key}");
            assert_eq!(
                item_index(on) + 1,
                item_index(key),
                "{key} 가 스위치 바로 뒤가 아니다"
            );
            assert_eq!(group, manifest_field(CONTEXT_ON_KEY, "id"), "{on}");
            groups.push(group);
        }
        assert!(
            groups.windows(2).all(|w| w[0] == w[1]),
            "현행화가 한 묶음이 아니다"
        );
        // 유휴 회수는 현행화 묶음 밖(메모리 절약)의 토글이다 — 머리 스위치가 없는 묶음이라 유휴 현행화를 꺼도
        // 회수 스위치는 숨지 않는다.
        assert_eq!(manifest_field(RECLAIM_KEY, "kind"), "toggle");
        let reclaim_group = manifest_field(RECLAIM_KEY, "group");
        assert_ne!(reclaim_group, groups[0], "유휴 회수가 현행화 묶음에 들었다");
        assert!(!group_has_switch(&reclaim_group));
        for key in [THRESHOLD_KEY, IDLE_KEY] {
            assert_eq!(num(key, "min"), IDLE_TOKENS_MIN, "끄는 것은 스위치뿐이다");
        }
    }

    /// 기준 칸의 0 은 끔이 아니다 — 스위치가 켜져 있으면 기본값으로 읽어 기능이 그대로 돈다.
    #[test]
    fn a_zero_threshold_reads_as_the_default_not_off() {
        let k = read_knobs(&Settings(vec![
            (THRESHOLD_KEY, json!(0.0)),
            (IDLE_KEY, json!(0.5)),
        ]));
        assert_eq!(k.turn_tokens, DEFAULT_THRESHOLD_TOKENS);
        assert_eq!(k.idle_tokens, DEFAULT_IDLE_TOKENS);
    }

    /// [현행화] 단추와 그 표시 설정은 없앴다 — 매니페스트에 칸이 남으면 설정 창에 아무것도
    /// 안 하는 스위치가 보인다.
    #[test]
    fn the_manifest_no_longer_offers_a_sync_button_switch() {
        let m = include_str!("../../tasty-plugin.toml");
        assert!(!m.contains("storage_key = \"sync_button\""));
    }

    /// 사용 한도 현행화 · API 트래픽 조절 · 따로 된 회수 스위치는 없앴다 — 칸이 남으면 아무것도 안 하는
    /// 스위치가 보인다. 회수는 「세션 최적화」(`release_hidden_webview`) 하나로 켠다.
    #[test]
    fn the_manifest_no_longer_offers_removed_switches() {
        let m = include_str!("../../tasty-plugin.toml");
        for key in [
            "limit_sync_on",
            "limit_sync_percent",
            "traffic_on",
            "traffic_up_kbps",
            "traffic_down_kbps",
            "idle_reclaim",
        ] {
            assert!(!m.contains(&format!("storage_key = \"{key}\"")), "{key}");
        }
        assert!(!m.contains("agent_session.traffic"));
        assert_eq!(RECLAIM_KEY, crate::application::view_state::SAVER_KEY);
    }

    /// 첫 턴 안내에 덧붙일 한 줄은 없앴다 — CLAUDE.md 와 스킬이 같은 일을 한다.
    #[test]
    fn the_manifest_no_longer_offers_a_brief_extra_line() {
        let m = include_str!("../../tasty-plugin.toml");
        assert!(!m.contains("storage_key = \"brief_extra\""));
        assert!(!include_str!("start.rs").contains("brief_extra"));
    }

    /// 설정 라벨은 80자 이하다 — 라벨은 줄바꿈 없이 그려지고 라벨 열이 가장 긴 라벨에 맞춰 넓어져, 길면 값 칸이
    /// 창 밖으로 밀려 하나도 안 보인다(VM 실측 2026-10-26: en 157자 라벨에서 값 칸 전부 사라짐 · en 80자 · ko 53자에서
    /// 전부 보임). 단위(`_suffix`) · 안내(`_hint`)는 라벨 열에 안 들어가 뺀다.
    #[test]
    fn settings_labels_fit_the_label_column() {
        // 라벨 수는 매니페스트 칸이 가리키는 라벨 키 수와 같다 — 남은 라벨 · 빠진 라벨을 둘 다 잡는다.
        let mut wanted: Vec<&str> = include_str!("../../tasty-plugin.toml")
            .lines()
            .filter_map(|l| l.trim().strip_prefix("label_key = \"sup.settings."))
            .map(|k| k.trim_end_matches('"'))
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        assert!(!wanted.is_empty(), "매니페스트에서 라벨 키를 못 읽었다");
        for (lang, src) in [
            ("ko", include_str!("../../lang/ko.toml")),
            ("en", include_str!("../../lang/en.toml")),
            ("ja", include_str!("../../lang/ja.toml")),
            ("zh-Hans", include_str!("../../lang/zh-Hans.toml")),
        ] {
            let sec = src
                .split("[sup.settings]")
                .nth(1)
                .and_then(|s| s.split("\n[").next())
                .unwrap_or_else(|| panic!("{lang}: [sup.settings] 가 없다"));
            let mut seen = 0;
            for line in sec.lines().filter(|l| l.contains(" = \"")) {
                let (key, val) = line.split_once(" = ").unwrap_or_default();
                if key.ends_with("_suffix") || key.ends_with("_hint") {
                    continue;
                }
                seen += 1;
                let n = val.trim_matches('"').chars().count();
                assert!(n <= 80, "{lang} {key}: {n}자 — 값 칸이 창 밖으로 밀린다");
            }
            assert_eq!(seen, wanted.len(), "{lang}: 라벨 수가 칸 수와 다르다");
        }
    }

    /// 세션 캐시 현행화 설명이 적은 고정 만료가 코드의 판정 시각과 같고 다른 수의 일부가 아니다 — 칸이 없어
    /// 화면에서 고칠 수 없으므로 설명이 틀리면 사용자는 틀린 값을 믿는다.
    #[test]
    fn the_idle_caption_states_the_fixed_expiry_the_code_uses() {
        let short = CACHE_TTL_5M.saturating_sub(IDLE_LEAD).as_secs();
        let long = CACHE_TTL_1H.saturating_sub(IDLE_LEAD).as_secs();
        assert_eq!((short, long), (240, 3540));
        let grouped = format!("{},{:03}", long / 1000, long % 1000);
        // 앞뒤가 숫자 · 쉼표가 아닌 자리 — `1240` · `3,5400` 같은 다른 수의 일부를 일치로 안 센다.
        let standalone = |line: &str, n: &str| {
            line.match_indices(n).any(|(i, _)| {
                let edge = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_digit() || c == ',');
                edge(line[..i].chars().next_back()) && edge(line[i + n.len()..].chars().next())
            })
        };
        for (lang, src) in [
            ("ko", include_str!("../../lang/ko.toml")),
            ("en", include_str!("../../lang/en.toml")),
            ("ja", include_str!("../../lang/ja.toml")),
            ("zh-Hans", include_str!("../../lang/zh-Hans.toml")),
        ] {
            // 칸 이름은 「유효 임계값」이라 기한은 스위치 아래 설명이 적는다.
            let line = src
                .split("[sup.about]")
                .nth(1)
                .and_then(|sec| sec.lines().find(|l| l.starts_with("idle_sync = ")))
                .unwrap_or_else(|| panic!("{lang}: idle_sync 설명이 없다"));
            assert!(standalone(line, &short.to_string()), "{lang}: {line}");
            assert!(standalone(line, &grouped), "{lang}: {line}");
        }
    }
}
