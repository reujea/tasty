//! 대화 탭 상태줄의 **모델 · 컨텍스트 · 비용 · 사용량** — agent 원문 줄에서 꺼낸다.
//!
//! # 왜 원문인가
//!
//! 데몬의 진행 이벤트(`started`·`output`·`finished`)에는 이 값들이 없다. claude 의
//! stream-json 원문에는 있다. 데몬이 구독에 원문을 실어 주기로 했고(`event_raw` ·
//! `protocol` 6 · `format` 칸), 해석은 tasty 가 맡는다. 형식마다 줄의 모양이 다르므로
//! `format` 을 모르면 아무것도 꺼내지 않는다 — 틀린 값보다 빈 칸이 낫다.
//!
//! # 읽는 줄 셋 (실측 2026-10-13, Claude Code 2.1.287 · haiku)
//!
//! | 줄 | 꺼내는 것 |
//! |---|---|
//! | `system/init` | `model` · `claude_code_version` · `permissionMode` · `output_style` |
//! | `result` | 턴의 `usage`(컨텍스트 점유) · `modelUsage.*.contextWindow` · `total_cost_usd` · `modelUsage` 의 토큰 합 · `duration_ms`·`duration_api_ms` |
//! | `rate_limit_event` | `rate_limit_info.unifiedWindows` 의 `five_hour`·`seven_day` |
//!
//! - `total_cost_usd` 는 누적이다. 한 프로세스로 두 턴을 돌리면 둘째 값이 첫째 값에 둘째 턴 몫을
//!   더한 것이었다(0.0173 → 0.0225). 그 범위는 프로세스가 아니라 **대화**다 — 아래 항목.
//! - `usage` 의 토큰은 **그 턴 몫**이다. 컨텍스트 점유는 마지막 호출(`iterations` 의 끝)의
//!   입력 · 캐시 읽기 · 캐시 쓰기 · 출력의 합이다 — 다음 턴이 그만큼을 들고 시작한다.
//! - `modelUsage` 의 토큰 · `total_cost_usd` · `duration_api_ms` 는 **대화의 누적**이다 — `--resume`
//!   으로 다시 뜬 프로세스도 이전 값에서 이어 센다(실측 2026-10-21, 2.1.304 · haiku: 첫 턴 입력 27428 ·
//!   재개한 둘째 턴 54979 = 27428 + 27551, API 대기 1149 → 1973 으로 그 턴 벽시계 1183 보다 컸다).
//!   입력(↑)은 입력 · 캐시 읽기 · 캐시 쓰기의 합, 출력(↓)은 출력이다.
//! - 턴 몫은 `usage` 와 `duration_ms` 뿐이다. 그래서 턴의 API 대기는 **직전 누적과의 차**로 낸다 —
//!   붙은 뒤 첫 `result` 는 앞 값을 모르므로 벽시계만 보인다.
//! - 5시간·7일 사용률은 세션이 아니라 **계정** 값이다. 같은 계정의 탭들은 같은 값을 본다.
//! - `rate_limit_event` 는 늘 오지 않는다(실측 2026-10-13, claude CLI 를 데몬 없이 직접 띄움 ·
//!   haiku · 한 턴): API 키 인증(`system/init` 의 `apiKeySource` 가 `ANTHROPIC_API_KEY`)
//!   프로필과 구독 인증(`none`)인 한 프로필 모두 0줄이었다. 데몬 쪽 실측으로는 다른 구독
//!   프로필에서 프로세스당 한 줄, 첫 턴의 `result` 앞에 왔다. 그래서 API 키 세션은 두 칸을
//!   아예 안 그리고, 나머지는 `-` 로 두며 툴팁이 그 사정을 말한다.
//! - `system/init` 은 프로세스가 뜰 때 한 번 온다. 이미 돌던 세션에 붙으면 못 보므로,
//!   모델을 아직 모르면 `result` 의 `modelUsage` 키(창이 가장 큰 것)로 채운다.
//!
//! # 화면
//!
//! 값만 모은다. 그리는 것은 [`crate::adapters::driving::statusline`] 이고, 값이 바뀌면 상태줄을 통째로 다시 그린다 —
//! 문서는 다시 싣지 않는다(재적재는 깜빡이고 입력 중이던 글을 지운다).

use serde_json::Value;

/// claude 의 stream-json — 데몬이 `format` 칸에 싣는 이름.
pub const CLAUDE_STREAM_JSON: &str = "claude-stream-json";

/// 사용량 창 하나 — 사용률(0.0~1.0)과 리셋 시각(유닉스 초).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub utilization: f64,
    pub resets_at: i64,
}

/// 상태줄에 실을 값들. 아직 못 본 것은 `None` 이다.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub model: Option<String>,
    /// Claude Code 버전 · 권한 모드 · 출력 스타일 — `system/init` 이 프로세스가 뜰 때 한 번 낸다.
    pub cc_version: Option<String>,
    pub permission_mode: Option<String>,
    pub output_style: Option<String>,
    /// 이 대화의 누적 입력(캐시 포함) · 출력 토큰 — 재개해도 이어진다.
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    /// 마지막 턴의 벽시계 · API 대기(ms). API 대기는 누적의 차라 앞 값을 알 때만 있다.
    pub turn_ms: Option<u64>,
    pub api_ms: Option<u64>,
    /// 마지막으로 본 `duration_api_ms`(대화 누적) — 다음 턴의 차를 내는 기준.
    pub api_total: Option<u64>,
    /// 마지막 턴이 끝났을 때 컨텍스트에 든 토큰 수.
    pub context_used: Option<u64>,
    pub context_window: Option<u64>,
    /// 이 대화의 누적 비용(USD) — 재개해도 이어진다.
    pub cost_usd: Option<f64>,
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    /// API 키 인증 세션 — 계정 한도 사용률이 오지 않으므로 두 칸을 안 그린다.
    pub api_key: bool,
    /// API 로 도는 세션의 잔액 대비 사용([`crate::adapters::driven::store::budget`]) — 구독 워커가 실어 보낸다.
    pub budget: Option<crate::adapters::driven::store::budget::Budget>,
    /// 마지막 호출이 쓴 프롬프트 캐시가 1시간짜리인가 — `cache_creation` 의 `ephemeral_1h` · `ephemeral_5m`
    /// 로 가른다. 캐시를 새로 안 쓴 턴은 판단을 바꾸지 않는다(모르면 `None`, 5분으로 본다).
    pub cache_1h: Option<bool>,
    /// 마지막 턴 **첫 호출**의 캐시 쓰기 — 쉰 뒤·접두부가 바뀐 뒤 대화 전체를 다시 쓰는 것은 그 턴의
    /// 첫 호출이다([`crate::application::cache_watch`]). 턴 끝 줄을 아직 못 봤으면 `None`.
    pub first_write: Option<u64>,
    /// 그 첫 호출의 캐시 읽기 — 읽기가 크면 대화를 다시 쓴 것이 아니라 새 내용을 더한 것이다.
    pub first_read: Option<u64>,
    /// 그 첫 호출이 쓴 캐시가 1시간짜리인가(5분만이면 `Some(false)`, 안 썼거나 섞였으면 `None`).
    pub first_1h: Option<bool>,
}

/// 구독 워커가 실은 `budget` 칸을 반영한다 — 값이 바뀌었으면 `true`.
///
/// 모양은 `{"spent": 18.17 | null, "balance": 50.0}` 이다. 칸이 틀리면 아무것도 안 바꾼다.
pub fn apply_budget(usage: &mut Usage, v: &Value) -> bool {
    let Some(balance) = v.get("balance").and_then(Value::as_f64) else {
        return false;
    };
    let next = Some(crate::adapters::driven::store::budget::Budget {
        spent: v.get("spent").and_then(Value::as_f64),
        balance,
    });
    let changed = usage.budget != next;
    usage.budget = next;
    changed
}

/// 탭 스냅샷에 싣는 모양 — 다시 뜬 plugin · 복원된 탭이 **다음 턴 전에도** 마지막으로 본 값을 그린다.
///
/// 원문은 값마다 오는 때가 다르다. 모델 · 버전은 claude 프로세스가 뜰 때, 한도 사용률은 그 프로세스의
/// 첫 턴에만 온다. 저장하지 않으면 plugin 이 다시 뜬 뒤 그 칸들은 프로세스가 다시 뜰 때까지 비고,
/// 나머지도 다음 턴이 끝날 때까지 빈다. 다시 실은 값은 "마지막으로 본 것"이고, 새 원문이 오면 덮는다.
pub fn to_snapshot(u: &Usage) -> Value {
    let win = |w: &Option<Window>| {
        w.map(|w| serde_json::json!({ "utilization": w.utilization, "resets_at": w.resets_at }))
    };
    serde_json::json!({
        "model": u.model,
        "cc_version": u.cc_version,
        "permission_mode": u.permission_mode,
        "output_style": u.output_style,
        "tokens_in": u.tokens_in,
        "tokens_out": u.tokens_out,
        "turn_ms": u.turn_ms,
        "api_ms": u.api_ms,
        "api_total": u.api_total,
        "context_used": u.context_used,
        "context_window": u.context_window,
        "cost_usd": u.cost_usd,
        "five_hour": win(&u.five_hour),
        "seven_day": win(&u.seven_day),
        "api_key": u.api_key,
        "budget": u.budget.map(|b| serde_json::json!({ "spent": b.spent, "balance": b.balance })),
    })
}

/// [`to_snapshot`] 을 되읽는다 — 모양이 틀린 칸은 비운다(모르는 값으로 남는다).
pub fn from_snapshot(v: &Value) -> Usage {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let n = |k: &str| v.get(k).and_then(Value::as_u64);
    let win = |k: &str| {
        let w = v.get(k)?;
        Some(Window {
            utilization: w.get("utilization")?.as_f64()?,
            resets_at: w.get("resets_at")?.as_i64()?,
        })
    };
    let mut u = Usage {
        model: s("model"),
        cc_version: s("cc_version"),
        permission_mode: s("permission_mode"),
        output_style: s("output_style"),
        tokens_in: n("tokens_in"),
        tokens_out: n("tokens_out"),
        turn_ms: n("turn_ms"),
        api_ms: n("api_ms"),
        api_total: n("api_total"),
        context_used: n("context_used"),
        context_window: n("context_window"),
        cost_usd: v.get("cost_usd").and_then(Value::as_f64),
        five_hour: win("five_hour"),
        seven_day: win("seven_day"),
        api_key: v.get("api_key").and_then(Value::as_bool).unwrap_or(false),
        budget: None,
        // 저장하지 않는다 — 턴 끝 시각(`last_result`)도 안 남으니 다음 턴 끝에서 다시 안다.
        cache_1h: None,
        first_write: None,
        first_read: None,
        first_1h: None,
    };
    if let Some(b) = v.get("budget").filter(|b| b.is_object()) {
        apply_budget(&mut u, b);
    }
    u
}

/// 원문 한 줄을 반영한다 — 값이 바뀌었으면 `true`(그때만 다시 그린다).
///
/// 모르는 형식 · JSON 이 아닌 줄 · 관심 없는 줄은 아무것도 안 바꾼다.
pub fn apply_raw(usage: &mut Usage, format: &str, line: &str) -> bool {
    if format != CLAUDE_STREAM_JSON {
        return false;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    let before = usage.clone();
    match (
        v.get("type").and_then(Value::as_str),
        v.get("subtype").and_then(Value::as_str),
    ) {
        (Some("system"), Some("init")) => {
            if let Some(m) = v.get("model").and_then(Value::as_str) {
                usage.model = Some(m.to_string());
            }
            if let Some(src) = v.get("apiKeySource").and_then(Value::as_str) {
                usage.api_key = src != "none";
            }
            let text = |k: &str| {
                v.get(k)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            if let Some(x) = text("claude_code_version") {
                usage.cc_version = Some(x);
            }
            if let Some(x) = text("permissionMode") {
                usage.permission_mode = Some(x);
            }
            if let Some(x) = text("output_style") {
                usage.output_style = Some(x);
            }
        }
        (Some("result"), _) => apply_result(usage, &v),
        (Some("rate_limit_event"), _) => {
            let w = v.pointer("/rate_limit_info/unifiedWindows");
            if let Some(x) = w.and_then(|w| window(w.get("five_hour"))) {
                usage.five_hour = Some(x);
            }
            if let Some(x) = w.and_then(|w| window(w.get("seven_day"))) {
                usage.seven_day = Some(x);
            }
        }
        _ => {}
    }
    *usage != before
}

/// 이 원문 줄이 턴의 끝(`result`)인가 — 텔레메트리를 남기는 자리다(`stream`).
pub fn is_result(format: &str, line: &str) -> bool {
    format == CLAUDE_STREAM_JSON
        && serde_json::from_str::<Value>(line)
            .is_ok_and(|v| v.get("type").and_then(Value::as_str) == Some("result"))
}

/// 턴 첫 호출의 캐시 쓰기와 그 수명. 반복 항목(`usage.iterations[0]`)에 `cache_creation` 이 없는
/// 판(실측 2026-10-13 문면)이면 수명만 턴 합계(`usage.cache_creation`)에서 읽는다 — 쓰기 토큰은
/// 첫 반복 항목에 늘 있다.
fn apply_first_call(usage: &mut Usage, v: &Value) {
    let first = v
        .pointer("/usage/iterations")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .or_else(|| v.get("usage"));
    let Some(u) = first else {
        return;
    };
    usage.first_write = u.get("cache_creation_input_tokens").and_then(Value::as_u64);
    usage.first_read = u.get("cache_read_input_tokens").and_then(Value::as_u64);
    let split = |base: &Value| {
        let n = |k: &str| {
            base.pointer(&format!("/cache_creation/{k}"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        (
            n("ephemeral_1h_input_tokens"),
            n("ephemeral_5m_input_tokens"),
        )
    };
    let (h1, m5) = match split(u) {
        (0, 0) => v.get("usage").map(split).unwrap_or((0, 0)),
        got => got,
    };
    usage.first_1h = match (h1 > 0, m5 > 0) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    };
}

fn apply_result(usage: &mut Usage, v: &Value) {
    if let Some(c) = v.get("total_cost_usd").and_then(Value::as_f64) {
        usage.cost_usd = Some(c);
    }
    // 마지막 호출이 정본이다 — 도구를 쓴 턴은 호출이 여럿이고, 위쪽 `usage` 는 그 합이다.
    let last = v
        .pointer("/usage/iterations")
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .or_else(|| v.get("usage"));
    if let Some(u) = last {
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        let total = n("input_tokens")
            + n("cache_read_input_tokens")
            + n("cache_creation_input_tokens")
            + n("output_tokens");
        if total > 0 {
            usage.context_used = Some(total);
        }
        let written = |k: &str| {
            u.pointer(&format!("/cache_creation/{k}"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        if written("ephemeral_1h_input_tokens") > 0 {
            usage.cache_1h = Some(true);
        } else if written("ephemeral_5m_input_tokens") > 0 {
            usage.cache_1h = Some(false);
        }
    }
    apply_first_call(usage, v);
    // 모델이 여럿이면(하위 agent) 창이 가장 큰 것 — 대화를 든 모델이 그것이다.
    let main = v
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|m| {
            m.iter()
                .filter_map(|(name, x)| {
                    Some((name, x.get("contextWindow").and_then(Value::as_u64)?))
                })
                .max_by_key(|(_, w)| *w)
        });
    if let Some((name, w)) = main {
        usage.context_window = Some(w);
        if usage.model.is_none() {
            usage.model = Some(name.clone());
        }
    }
    // 토큰은 모델을 가리지 않고 더한다 — 하위 agent 가 쓴 것도 이 프로세스가 쓴 것이다.
    if let Some(m) = v.get("modelUsage").and_then(Value::as_object) {
        let sum = |keys: &[&str]| -> u64 {
            m.values()
                .map(|x| {
                    keys.iter()
                        .map(|k| x.get(*k).and_then(Value::as_u64).unwrap_or(0))
                        .sum::<u64>()
                })
                .sum()
        };
        if !m.is_empty() {
            usage.tokens_in = Some(sum(&[
                "inputTokens",
                "cacheReadInputTokens",
                "cacheCreationInputTokens",
            ]));
            usage.tokens_out = Some(sum(&["outputTokens"]));
        }
    }
    if let Some(ms) = v.get("duration_ms").and_then(Value::as_u64) {
        usage.turn_ms = Some(ms);
    }
    if let Some(total) = v.get("duration_api_ms").and_then(Value::as_u64) {
        // 앞 값보다 작으면 다른 대화로 바뀐 것이다 — 차를 내지 않고 기준만 옮긴다.
        usage.api_ms = usage.api_total.and_then(|prev| total.checked_sub(prev));
        usage.api_total = Some(total);
    }
}

fn window(v: Option<&Value>) -> Option<Window> {
    let v = v?;
    Some(Window {
        utilization: v.get("utilization").and_then(Value::as_f64)?,
        resets_at: v.get("resetsAt").and_then(Value::as_i64)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 스냅샷을 거쳐도 상태줄 값이 그대로다 — 다시 뜬 plugin 이 다음 턴 전에도 같은 줄을 그린다.
    /// 마지막 호출의 캐시 쓰기 종류로 1시간/5분을 가른다 — 캐시를 안 쓴 턴은 판단을 안 바꾼다.
    #[test]
    fn the_last_call_tells_which_prompt_cache_lifetime_the_session_has() {
        let line = |h1: u64, m5: u64| {
            format!(
                r#"{{"type":"result","usage":{{"input_tokens":3,"cache_read_input_tokens":10,"cache_creation_input_tokens":{t},"output_tokens":4,"iterations":[{{"input_tokens":3,"cache_read_input_tokens":10,"cache_creation_input_tokens":{t},"output_tokens":4,"cache_creation":{{"ephemeral_1h_input_tokens":{h1},"ephemeral_5m_input_tokens":{m5}}}}}]}}}}"#,
                t = h1 + m5
            )
        };
        let mut u = Usage::default();
        assert_eq!(u.cache_1h, None);
        apply_raw(&mut u, CLAUDE_STREAM_JSON, &line(354_883, 0));
        assert_eq!(u.cache_1h, Some(true));
        apply_raw(&mut u, CLAUDE_STREAM_JSON, &line(0, 0));
        assert_eq!(u.cache_1h, Some(true), "캐시를 안 쓴 턴이 판단을 지웠다");
        apply_raw(&mut u, CLAUDE_STREAM_JSON, &line(0, 13_065));
        assert_eq!(u.cache_1h, Some(false));
    }

    /// 캐시 재작성 판정(`cache_watch`)은 턴 **첫 호출**의 쓰기를 본다 — 쉰 뒤 대화 전체를 다시 쓰는
    /// 것은 첫 호출이다. 반복 항목에 수명 칸이 없는 판이면 턴 합계에서 수명을 읽는다(U7).
    #[test]
    fn the_first_call_of_the_turn_carries_the_cache_rewrite() {
        let mut u = Usage::default();
        apply_raw(
            &mut u,
            CLAUDE_STREAM_JSON,
            r#"{"type":"result","usage":{"input_tokens":5,"cache_read_input_tokens":10,"cache_creation_input_tokens":550010,"output_tokens":9,"cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":550010},
                "iterations":[{"input_tokens":3,"cache_read_input_tokens":0,"cache_creation_input_tokens":550000,"output_tokens":4},
                              {"input_tokens":2,"cache_read_input_tokens":550000,"cache_creation_input_tokens":10,"output_tokens":5,"cache_creation":{"ephemeral_1h_input_tokens":10,"ephemeral_5m_input_tokens":0}}]}}"#,
        );
        assert_eq!((u.first_write, u.first_read), (Some(550_000), Some(0)));
        // 첫 반복 항목에 수명 칸이 없다 — 턴 합계(5분만)에서 읽는다. 마지막 호출의 1시간이 아니다.
        assert_eq!(u.first_1h, Some(false));
        assert_eq!(u.cache_1h, Some(true));
        // 첫 반복 항목에 칸이 있으면 그것을 쓴다
        let mut v = Usage::default();
        apply_raw(
            &mut v,
            CLAUDE_STREAM_JSON,
            r#"{"type":"result","usage":{"input_tokens":1,"cache_read_input_tokens":0,"cache_creation_input_tokens":70000,"output_tokens":1,
                "iterations":[{"input_tokens":1,"cache_read_input_tokens":0,"cache_creation_input_tokens":70000,"output_tokens":1,"cache_creation":{"ephemeral_1h_input_tokens":70000,"ephemeral_5m_input_tokens":0}}]}}"#,
        );
        assert_eq!((v.first_write, v.first_1h), (Some(70_000), Some(true)));
    }

    #[test]
    fn usage_survives_the_snapshot() {
        let mut u = Usage::default();
        for l in [
            r#"{"type":"system","subtype":"init","model":"claude-haiku-5-5","permissionMode":"bypassPermissions","output_style":"default","apiKeySource":"none","claude_code_version":"2.1.304"}"#,
            r#"{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.03,"resetsAt":1791878400},"seven_day":{"utilization":0.69,"resetsAt":1791936000}}}}"#,
            r#"{"type":"result","duration_ms":1504,"duration_api_ms":1051,"total_cost_usd":0.0259,"usage":{"input_tokens":3,"cache_read_input_tokens":14167,"cache_creation_input_tokens":12251,"output_tokens":4},"modelUsage":{"claude-haiku-5-5":{"inputTokens":3,"outputTokens":4,"cacheReadInputTokens":14167,"cacheCreationInputTokens":12251,"contextWindow":200000}}}"#,
        ] {
            apply_raw(&mut u, CLAUDE_STREAM_JSON, l);
        }
        apply_budget(&mut u, &serde_json::json!({"spent": 1.5, "balance": 50.0}));
        assert!(u.five_hour.is_some() && u.cost_usd.is_some() && u.context_window.is_some());
        let back = from_snapshot(&serde_json::from_str(&to_snapshot(&u).to_string()).unwrap());
        // 턴 첫 호출의 캐시 값은 그 턴의 판정에만 쓴다 — 저장하지 않는다(다시 뜬 plugin 은 다음
        // 턴 끝까지 판정하지 않는다, `cache_watch`).
        assert!(u.first_write.is_some());
        let mut want = u.clone();
        want.first_write = None;
        want.first_read = None;
        want.first_1h = None;
        assert_eq!(back, want);
        assert_eq!(
            from_snapshot(&serde_json::json!("깨진 값")),
            Usage::default()
        );
    }

    // 실측 줄(2026-10-13, 2.1.287 · haiku). `system/init` 은 5.6KB 라 쓰는 칸만 남겼다.
    const INIT: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/raw","session_id":"80845b41-c185-42b4-bfcb-13eeb8d72908","model":"claude-haiku-4-5-20251001","permissionMode":"default","apiKeySource":"none","claude_code_version":"2.1.287"}"#;
    const INIT_API_KEY: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/raw","session_id":"80845b41-c185-42b4-bfcb-13eeb8d72908","model":"claude-haiku-4-5-20251001","permissionMode":"default","apiKeySource":"ANTHROPIC_API_KEY","claude_code_version":"2.1.287"}"#;
    const RATE: &str = r#"{"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "resetsAt": 1791878400, "rateLimitType": "five_hour", "overageStatus": "rejected", "overageDisabledReason": "org_level_disabled", "isUsingOverage": false, "unifiedWindows": {"five_hour": {"utilization": 0.03, "resetsAt": 1791878400}, "seven_day": {"utilization": 0.69, "resetsAt": 1791936000}}}, "uuid": "1907968a-5cf4-4068-b6a7-7f26155c361f", "session_id": "80845b41-c185-42b4-bfcb-13eeb8d72908"}"#;
    const RESULT: &str = r#"{"duration_api_ms": 691, "stop_reason": "end_turn", "session_id": "80845b41-c185-42b4-bfcb-13eeb8d72908", "total_cost_usd": 0.025941699999999998, "usage": {"input_tokens": 3, "cache_creation_input_tokens": 12251, "cache_read_input_tokens": 14167, "output_tokens": 4, "output_tokens_details": {"thinking_tokens": 0}, "service_tier": "standard", "cache_creation": {"ephemeral_1h_input_tokens": 12251, "ephemeral_5m_input_tokens": 0}, "iterations": [{"input_tokens": 3, "output_tokens": 4, "cache_read_input_tokens": 14167, "cache_creation_input_tokens": 12251, "type": "message"}], "speed": "standard"}, "modelUsage": {"claude-haiku-4-5-20251001": {"inputTokens": 3, "outputTokens": 4, "cacheReadInputTokens": 14167, "cacheCreationInputTokens": 12251, "costUSD": 0.025941699999999998, "contextWindow": 200000, "maxOutputTokens": 32000}}, "is_error": false, "num_turns": 1, "subtype": "success", "result": "OK", "type": "result", "duration_ms": 733}"#;

    /// 비용은 오차 안에서 견준다 — serde_json 의 실수 파싱이 마지막 자리에서 원문과 다를 수 있다.
    fn assert_cost(u: &Usage, want: f64) {
        let got = u.cost_usd.expect("비용이 없다");
        assert!((got - want).abs() < 1e-9, "{got} != {want}");
    }

    fn fed(lines: &[&str]) -> Usage {
        let mut u = Usage::default();
        for l in lines {
            apply_raw(&mut u, CLAUDE_STREAM_JSON, l);
        }
        u
    }

    #[test]
    fn the_three_lines_fill_every_field() {
        let u = fed(&[INIT, RATE, RESULT]);
        assert_eq!(u.model.as_deref(), Some("claude-haiku-4-5-20251001"));
        assert_eq!(u.context_used, Some(3 + 14167 + 12251 + 4));
        assert_eq!(u.context_window, Some(200_000));
        assert_cost(&u, 0.0259417);
        assert_eq!(
            u.five_hour,
            Some(Window {
                utilization: 0.03,
                resets_at: 1791878400
            })
        );
        assert_eq!(u.seven_day.map(|w| w.utilization), Some(0.69));
    }

    /// 모르는 형식은 아무것도 안 꺼낸다 — 다른 agent 의 줄을 claude 로 읽으면 틀린 값이 뜬다.
    #[test]
    fn an_unknown_format_changes_nothing() {
        let mut u = Usage::default();
        assert!(!apply_raw(&mut u, "pi-json", RESULT));
        assert_eq!(u, Usage::default());
    }

    /// 다시 그리는 신호는 값이 바뀔 때만 — 매 줄 그리면 깜빡인다.
    #[test]
    fn only_a_change_is_reported() {
        let mut u = Usage::default();
        assert!(apply_raw(&mut u, CLAUDE_STREAM_JSON, RATE));
        assert!(
            !apply_raw(&mut u, CLAUDE_STREAM_JSON, RATE),
            "같은 줄에 바뀌었다고 했다"
        );
        assert!(!apply_raw(&mut u, CLAUDE_STREAM_JSON, "not json"));
        assert!(!apply_raw(
            &mut u,
            CLAUDE_STREAM_JSON,
            r#"{"type":"assistant","message":{}}"#
        ));
    }

    /// 도구를 쓴 턴은 호출이 여럿이다 — 컨텍스트는 합이 아니라 **마지막 호출**이다.
    #[test]
    fn context_comes_from_the_last_call_of_the_turn() {
        let line = r#"{"type":"result","usage":{"input_tokens":20,"cache_read_input_tokens":2000,"cache_creation_input_tokens":0,"output_tokens":30,
            "iterations":[{"input_tokens":10,"cache_read_input_tokens":900,"cache_creation_input_tokens":0,"output_tokens":10},
                          {"input_tokens":10,"cache_read_input_tokens":1100,"cache_creation_input_tokens":0,"output_tokens":20}]}}"#;
        assert_eq!(fed(&[line]).context_used, Some(1130));
    }

    /// 이미 돌던 세션에 붙어 `system/init` 을 못 봤으면 `result` 의 모델로 채운다.
    #[test]
    fn without_init_the_model_comes_from_the_result() {
        assert_eq!(
            fed(&[RESULT]).model.as_deref(),
            Some("claude-haiku-4-5-20251001")
        );
        // `init` 을 봤으면 그 값이 정본이다 — 뒤의 `result` 가 덮지 않는다.
        let init = INIT.replace("claude-haiku-4-5-20251001", "from-init");
        assert_eq!(fed(&[&init, RESULT]).model.as_deref(), Some("from-init"));
    }

    /// init 의 버전 · 권한 · 스타일과 result 의 누적 토큰 · 지연이 칸이 된다(실측 2026-10-21, 2.1.304).
    #[test]
    fn init_and_result_fill_the_process_items() {
        let init = r#"{"type":"system","subtype":"init","model":"claude-haiku-5-5","permissionMode":"bypassPermissions","output_style":"default","apiKeySource":"none","claude_code_version":"2.1.304"}"#;
        let result = r#"{"type":"result","subtype":"success","duration_ms":1504,"duration_api_ms":1051,"total_cost_usd":0.02,
            "modelUsage":{"claude-haiku-5-5":{"inputTokens":2,"outputTokens":4,"cacheReadInputTokens":14141,"cacheCreationInputTokens":13285,"contextWindow":200000},
                          "sub":{"inputTokens":10,"outputTokens":6,"cacheReadInputTokens":0,"cacheCreationInputTokens":0,"contextWindow":100000}}}"#;
        let u = fed(&[init, result]);
        assert_eq!(u.cc_version.as_deref(), Some("2.1.304"));
        assert_eq!(u.permission_mode.as_deref(), Some("bypassPermissions"));
        assert_eq!(u.output_style.as_deref(), Some("default"));
        assert_eq!(
            u.tokens_in,
            Some(2 + 14141 + 13285 + 10),
            "하위 모델까지 더한다"
        );
        assert_eq!(u.tokens_out, Some(10));
        // 붙은 뒤 첫 result — API 대기는 누적이라 앞 값 없이는 턴 몫을 모른다.
        assert_eq!((u.turn_ms, u.api_ms), (Some(1504), None));
        // 재개한 둘째 턴(실측 모양) — 누적의 차가 그 턴의 API 대기다.
        let second = r#"{"type":"result","duration_ms":1183,"duration_api_ms":2200}"#;
        let u2 = fed(&[init, result, second]);
        assert_eq!((u2.turn_ms, u2.api_ms), (Some(1183), Some(2200 - 1051)));
    }

    /// API 키 인증 세션을 가른다 — 계정 한도 대신 잔액 대비 사용을 보이는 근거다(`statusline`).
    #[test]
    fn the_init_line_tells_an_api_key_session() {
        assert!(fed(&[INIT_API_KEY]).api_key);
        assert!(!fed(&[INIT]).api_key);
        let mut u = fed(&[INIT_API_KEY]);
        assert!(apply_budget(
            &mut u,
            &serde_json::json!({"spent": 1.0, "balance": 50.0})
        ));
        assert!(
            !apply_budget(&mut u, &serde_json::json!({"spent": 1.0})),
            "칸이 틀린 값을 받았다"
        );
    }

    /// 칸이 빠진 줄이 앞서 본 값을 지우지 않는다.
    #[test]
    fn a_line_missing_a_field_keeps_what_was_seen() {
        let u = fed(&[
            RESULT,
            r#"{"type":"result","subtype":"error_during_execution"}"#,
        ]);
        assert_cost(&u, 0.0259417);
        assert_eq!(u.context_window, Some(200_000));
    }
}
