//! 대화 탭 하단 상태줄 — Claude Code 상태줄처럼 **텍스트 줄**로 그리고, 접을 수 있다.
//!
//! # 모양
//!
//! ```text
//! ▾ >> ~/dev/…/src  @dev(me@example.com)(이름)  PID:6571  mem:1.1G  cc:v2.1.304
//!   opus-5-5  style:default  ↑704k/↓133  sess:c8e27346-…
//!   71% ■■■■░░ [710k/1.0M]  lat:2.6s(API 2.1s)  up:5h16m  cache:14:32:05  rest:14:31:05
//!   5h:25%(6:50 PM) 7d:8%(10-28 09:00)  Cost:$47.964
//!   ⏵⏵ bypassPermissions · plugin:<supervisor id> · turns:3 · last reply ok
//! ```
//!
//! 값을 아직 모르는 토큰은 빼고, 빈 줄은 안 그린다. 접으면 한 줄만 남는다 —
//! `~/dev/…/src  dev(me@example.com)  claude(c8e27346-…)  cache:14:32:05  rest:14:31:05`(작업 폴더 ·
//! 프로필(계정) · 종류(대화 id) · 캐시 만료 · 회수 예정 시각). 접힘은 탭(`Convo::status_folded`)이 쥔다 — 문서를 다시 실어도 풀리지 않는다.
//!
//! # 갱신
//!
//! 값이 바뀌면 상태줄을 **통째로** 다시 그린다(`activity::status_script`). 칸 하나만 갈아 끼우던
//! 예전 방식은 줄 구성이 값에 따라 바뀌는 이 모양에서는 자리가 안 맞는다.
//!
//! 리셋 시각은 유닉스 초로 싣고(`<time data-ts>`) 페이지가 **보는 사람의 현지 시각**으로 바꾼다
//! ([`script_html`]) — plugin 에는 시간대를 아는 수단이 없다.

use crate::adapters::driven::session_info::Proc;
use crate::application::session_surface;
use crate::domain::convo::Convo;
use crate::domain::usage::Usage;
use serde_json::{Value, json};

/// 접기 단추가 내는 표식.
pub const FOLD_MARK: &str = "agent-status";

/// 작업 폴더를 눌렀다 — 값은 누를 때마다 바뀐다(같은 hash 는 통지가 안 나간다).
pub const COPY_CWD_MARK: &str = "agent-copy-cwd";

/// surface id 를 눌렀다 — 값은 누를 때마다 바뀐다.
pub const COPY_SURFACE_MARK: &str = "agent-copy-surface";

/// 대화 id 를 눌렀다 — 값은 누를 때마다 바뀐다.
pub const COPY_SESSION_MARK: &str = "agent-copy-session";

/// 컨텍스트 막대의 칸 수.
const BAR_CELLS: usize = 6;

/// 작업 폴더를 눌렀으면 그 탭의 **전체** 경로(줄여 보이기 전)를 호스트 클립보드에 넣는다 — 우리
/// 표식이면 `true`.
pub fn copy_cwd<H: crate::ports::host::HostCall>(
    host: &H,
    sid: u32,
    convo: &Convo,
    url: &str,
) -> bool {
    copy_marked(host, sid, url, COPY_CWD_MARK, "cwd", full_cwd(convo))
}

/// surface id 를 눌렀으면 그 id 를 호스트 클립보드에 넣는다 — 우리 표식이면 `true`.
pub fn copy_surface<H: crate::ports::host::HostCall>(host: &H, sid: u32, url: &str) -> bool {
    copy_marked(
        host,
        sid,
        url,
        COPY_SURFACE_MARK,
        "sid",
        Some(&sid.to_string()),
    )
}

/// 대화 id 를 눌렀으면 그 탭의 **전체** 대화 id 를 호스트 클립보드에 넣는다 — 우리 표식이면 `true`.
pub fn copy_session<H: crate::ports::host::HostCall>(
    host: &H,
    sid: u32,
    convo: &Convo,
    url: &str,
) -> bool {
    copy_marked(host, sid, url, COPY_SESSION_MARK, "sess", session_of(convo))
}

/// 표식이 맞으면 `text` 를 복사하고 그 칸(`.status .<cls>`) 옆에 잠깐 "복사했다" 를 띄운다.
fn copy_marked<H: crate::ports::host::HostCall>(
    host: &H,
    sid: u32,
    url: &str,
    mark: &str,
    cls: &str,
    text: Option<&str>,
) -> bool {
    let ours = crate::adapters::driving::url::query_of(url).is_some_and(|q| {
        let fields: Vec<&str> = q.split('&').collect();
        crate::adapters::driving::url::has_mark(&fields, mark)
    });
    if !ours {
        return false;
    }
    let Some(text) = text else {
        return true;
    };
    match host.call("clipboard.set_text", serde_json::json!({ "text": text })) {
        Ok(_) => {
            session_surface::patch(
                host,
                sid,
                &serde_json::json!({
                    "type": "status.copied",
                    "cls": cls,
                    "text": crate::domain::i18n::t("sup.line.copied"),
                }),
            );
        }
        Err(e) => tracing::warn!("[agent_session] surface={sid}: 복사하지 못했다: {e}"),
    }
    true
}

/// 상태줄이 보이는 작업 폴더의 전체 값 — 데몬 행의 것, 없으면 시작 폼에서 고른 것.
pub(crate) fn full_cwd(convo: &Convo) -> Option<&str> {
    convo
        .listed_cwd
        .as_deref()
        .or(Some(convo.cwd_pick.as_str()))
        .filter(|c| !c.trim().is_empty())
}

/// 접기 단추를 눌렀나.
pub fn parse_toggle(url: &str) -> bool {
    crate::adapters::driving::url::query_of(url).is_some_and(|q| {
        let fields: Vec<&str> = q.split('&').collect();
        crate::adapters::driving::url::has_mark(&fields, FOLD_MARK)
    })
}

/// 토큰 하나 — `라벨:값`. 라벨이 없으면 값만. 값은 늘 `<b>` 안이다(색은 `cls`).
fn tok(label: Option<&'static str>, val: &str, cls: &str, hint: Option<&'static str>) -> Value {
    json!({
        "t": "tok",
        "label": label.map(crate::domain::i18n::t),
        "val": val,
        "cls": cls,
        "title": hint.map(crate::domain::i18n::t),
    })
}

/// 상태줄 전체.
pub fn render(convo: &Convo, busy: bool) -> Value {
    let folded = convo.status_folded;
    let (arrow, hint) = if folded {
        ("▸", "sup.line.unfold")
    } else {
        ("▾", "sup.line.fold")
    };
    let lines = if folded {
        vec![folded_line(convo, busy)]
    } else {
        full_lines(convo, busy)
    };
    json!({
        "folded": folded,
        "arrow": arrow,
        "foldTitle": crate::domain::i18n::t(hint),
        "foldMark": FOLD_MARK,
        "lines": lines.into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>(),
    })
}

fn folded_line(convo: &Convo, busy: bool) -> Vec<Value> {
    // `{작업 폴더} {프로필}({계정}) {종류}({대화 id}) cache:{만료 시각} rest:{회수 시각}` — 어느 자리에서 누가
    // 무엇으로 돌고, 언제 캐시가 식고 회수되는지만.
    let mut l = Vec::new();
    if let Some(c) = cwd_link(convo) {
        l.push(c);
    }
    if let Some(w) = profile(convo) {
        l.push(tok(None, &w, "", None));
    }
    let kind = convo.agent_kind.as_deref();
    match (kind, session_link(convo, None)) {
        (Some(k), Some(mut s)) => {
            s["pre"] = json!(format!("{k}("));
            s["post"] = json!(")");
            l.push(s);
        }
        (Some(k), None) => l.push(tok(None, k, "", None)),
        (None, Some(mut s)) => {
            s["pre"] = json!("(");
            s["post"] = json!(")");
            l.push(s);
        }
        (None, None) => {}
    }
    l.extend(crate::application::idle_clock::tokens(
        convo,
        busy,
        std::time::Instant::now(),
        crate::adapters::driven::store::budget::now(),
    ));
    l
}

/// 대화 id — 누르면 전체 id 를 복사한다(`copy_session`). 접은 줄은 앞뒤에 `종류(` · `)` 를 붙인다.
fn session_link(convo: &Convo, label: Option<&'static str>) -> Option<Value> {
    let s = session_of(convo)?;
    Some(json!({
        "t": "sess",
        "label": label.map(crate::domain::i18n::t),
        "val": s,
        "title": crate::domain::i18n::t("sup.line.copy_session"),
        "mark": COPY_SESSION_MARK,
    }))
}

/// 작업 폴더 — 줄여 보이되, 누르면 전체 경로를 복사한다(`copy_cwd`). 툴팁이 전체 경로다.
fn cwd_link(convo: &Convo) -> Option<Value> {
    let c = full_cwd(convo)?;
    Some(json!({
        "t": "cwd",
        "val": short_path(c),
        "title": format!("{c} — {}", crate::domain::i18n::t("sup.line.copy_cwd")),
        "mark": COPY_CWD_MARK,
    }))
}

/// agent 프로필 이름 — 그 프로필이 로그인한 계정을 옆에 붙인다(`account_email`, 알 때만).
fn profile(convo: &Convo) -> Option<String> {
    let a = convo.agent_name.as_deref()?;
    Some(match convo.account_email.as_deref() {
        Some(e) => format!("{a}({e})"),
        None => a.to_string(),
    })
}

/// 대화 id — claude 의 것, 없으면 이 탭의 supervisor id.
fn session_of(convo: &Convo) -> Option<&str> {
    convo
        .agent_session_id
        .as_deref()
        .or(convo.session_id.as_deref())
}

/// 현행화(임계를 넘었나 · 도는 중 · 직전 실패 — 사유는 탭의 알림 줄)와 캐시 재작성(직전 턴이 쉬지
/// 않았는데 대화 전체를 다시 썼다, `cache_watch`) 토큰.
fn sync_and_cache_tokens(convo: &Convo) -> Vec<Value> {
    let sync = crate::application::sync_compact::status_label(&convo.sync)
        .map(|(val, cls, hint)| tok(None, &crate::domain::i18n::t(val), cls, Some(hint)));
    let cache = convo.cache.shown.map(|_| {
        tok(
            None,
            &crate::domain::i18n::t("sup.cache.rewrite_token"),
            "wait",
            Some("sup.cache.rewrite_tip"),
        )
    });
    sync.into_iter().chain(cache).collect()
}

fn full_lines(convo: &Convo, busy: bool) -> Vec<Vec<Value>> {
    let u = &convo.usage;
    let mut out = Vec::new();

    // 1 — 자리 · 누구 · 프로세스.
    let mut l = vec![json!({ "t": "k", "text": ">>" })];
    if let Some(c) = cwd_link(convo) {
        l.push(c);
    }
    let who = match (profile(convo), convo.display_name()) {
        (Some(a), Some(n)) => Some(format!("@{a}({n})")),
        (Some(a), None) => Some(format!("@{a}")),
        (None, Some(n)) => Some(format!("({n})")),
        (None, None) => None,
    };
    if let Some(w) = who {
        l.push(tok(None, &w, "", None));
    }
    l.extend(proc_tokens(
        convo.process.as_ref(),
        crate::adapters::driven::store::budget::now(),
    ));
    if let Some(v) = &u.cc_version {
        l.push(tok(Some("sup.line.cc"), &format!("v{v}"), "", None));
    }
    out.push(l);

    // 2 — 모델 · 스타일 · 토큰 · 대화.
    let mut l = Vec::new();
    if let Some(m) = &u.model {
        l.push(tok(None, m, "", None));
    }
    if let Some(s) = &u.output_style {
        l.push(tok(Some("sup.line.style"), s, "", None));
    }
    if let (Some(i), Some(o)) = (u.tokens_in, u.tokens_out) {
        l.push(tok(
            None,
            &format!("↑{}/↓{}", tokens(i), tokens(o)),
            "",
            Some("sup.status.tokens_hint"),
        ));
    }
    if let Some(s) = session_link(convo, Some("sup.line.sess")) {
        l.push(s);
    }
    out.push(l);

    // 3 — 컨텍스트 · 지연 · 경과.
    let mut l = Vec::new();
    if let Some(used) = u.context_used {
        match u.context_window.filter(|w| *w > 0) {
            Some(w) => {
                let r = used as f64 / w as f64;
                l.push(tok(
                    None,
                    &format!(
                        "{}% {} [{}/{}]",
                        percent(r),
                        bar(r),
                        tokens(used),
                        tokens(w)
                    ),
                    level(r),
                    None,
                ));
            }
            None => l.push(tok(None, &tokens(used), "", None)),
        }
    }
    l.extend(sync_and_cache_tokens(convo));
    if let Some(ms) = u.turn_ms {
        let val = match u.api_ms {
            Some(api) => format!("{}(API {})", seconds(ms), seconds(api)),
            None => seconds(ms),
        };
        l.push(tok(
            Some("sup.line.lat"),
            &val,
            "",
            Some("sup.status.latency_hint"),
        ));
    }
    if let Some(at) = convo.process.as_ref().and_then(|p| p.started_unix) {
        l.push(tok(
            Some("sup.line.up"),
            &crate::adapters::driven::session_info::elapsed(
                (crate::adapters::driven::store::budget::now() - at).max(0),
            ),
            "",
            Some("sup.status.uptime_hint"),
        ));
    }
    l.extend(crate::application::idle_clock::tokens(
        convo,
        busy,
        std::time::Instant::now(),
        crate::adapters::driven::store::budget::now(),
    ));
    out.push(l);

    // 4 — 한도 · 비용 · 그 프로필의 최근 1분.
    let mut l = limit_tokens(u);
    l.extend(meter_tokens(convo));
    if let Some(c) = u.cost_usd {
        let val = if c >= 1.0 {
            format!("${c:.2}")
        } else {
            format!("${c:.3}")
        };
        l.push(tok(
            Some("sup.line.cost"),
            &val,
            "",
            Some("sup.status.cost_hint"),
        ));
    }
    out.push(l);

    // 5 — 권한 · 이 탭의 세션 · 주고받음 · 마지막 응답.
    let mut l = Vec::new();
    if let Some(m) = &u.permission_mode {
        l.push(json!({ "t": "perm", "tok": tok(None, m, "", None) }));
    }
    // 이 탭의 supervisor id — 대화 id(`sess:`)와 다를 때만. 같으면 같은 값을 두 번 싣는다.
    if let Some(s) = convo
        .session_id
        .as_deref()
        .filter(|s| convo.agent_session_id.as_deref().is_some_and(|a| a != *s))
    {
        l.push(tok(Some("sup.line.plugin"), s, "", None));
    }
    // 이 탭의 surface id — CLI · IPC 가 대상을 지정하는 값이다. 누르면 복사한다(`copy_surface`).
    if convo.surface_id != 0 {
        l.push(json!({
            "t": "sid",
            "label": crate::domain::i18n::t("sup.line.surface"),
            "val": convo.surface_id.to_string(),
            "title": crate::domain::i18n::t("sup.line.copy_surface"),
            "mark": COPY_SURFACE_MARK,
        }));
    }
    // 주고받은 수 — 디스크로 옮기거나 버린 턴까지 센다(`turn_archive`). 메모리에 남은 수만 세면 한도를
    // 걸자마자 숫자가 줄어든다.
    l.push(tok(
        Some("sup.line.turns"),
        &(convo.turns.len() + convo.archived + convo.dropped).to_string(),
        "",
        None,
    ));
    l.push(last_reply(convo, busy));
    out.push(l);
    out
}

/// 5시간 · 7일 한도 또는 잔액 대비 사용(API 세션).
///
/// 한도 사용률은 claude 가 `rate_limit_event` 를 낼 때만 온다 — 다른 값이 이미 보이는데 칸이 통째로
/// 빠지면 기능이 없는 것처럼 읽히므로 `-` 로 자리를 지킨다. API 세션은 그 값이 오지 않으므로 잔액
/// 대비 사용(`Total`)을 보이고, 한도 줄이 이미 왔으면 그쪽이 이긴다(`budget`).
fn limit_tokens(u: &Usage) -> Vec<Value> {
    let windows_seen = u.five_hour.is_some() || u.seven_day.is_some();
    if !windows_seen && (u.budget.is_some() || u.api_key) {
        let balance = u
            .budget
            .map_or_else(crate::adapters::driven::store::budget::balance, |b| {
                b.balance
            });
        let spent = u.budget.and_then(|b| b.spent);
        let (val, cls) = match spent {
            Some(s) if balance > 0.0 => (format!("${s:.2}/${balance:.2}"), level(s / balance)),
            Some(s) => (format!("${s:.2}/∞"), ""),
            None => (format!("-/${balance:.2}"), ""),
        };
        return vec![tok(
            Some("sup.line.total"),
            &val,
            cls,
            Some("sup.status.total_hint"),
        )];
    }
    let seen =
        !u.api_key && (u.model.is_some() || u.context_used.is_some() || u.cost_usd.is_some());
    let mut l = Vec::new();
    for (label, w) in [
        ("sup.line.five", u.five_hour),
        ("sup.line.seven", u.seven_day),
    ] {
        match w {
            Some(w) => l.push(json!({
                "t": "limit",
                "label": crate::domain::i18n::t(label),
                "cls": level(w.utilization),
                "pct": percent(w.utilization),
                "ts": w.resets_at,
                "title": crate::domain::i18n::t("sup.status.account_hint"),
            })),
            None if seen => l.push(tok(
                Some(label),
                "-",
                "",
                Some("sup.status.account_pending"),
            )),
            None => {}
        }
    }
    l
}

/// 그 agent 프로필 전체의 최근 1분 사용 · 최근 5분 429 · 차단 — 한도는 프로필이 쓰는 계정에 걸린다
/// (`rate_meter` · `rate_block`). 아직 못 쟀으면 빼고, 1분 동안 요청이 없으면 `-` 다.
fn meter_tokens(convo: &Convo) -> Vec<Value> {
    let mut l = Vec::new();
    if let Some(m) = convo.meter {
        let val = if m.requests == 0 {
            "-".to_string()
        } else {
            format!(
                "{}req ↑{}/↓{}",
                m.requests,
                tokens(m.input),
                tokens(m.output)
            )
        };
        l.push(tok(
            Some("sup.line.minute"),
            &val,
            "",
            Some("sup.status.minute_hint"),
        ));
        if m.rejected > 0 {
            l.push(tok(
                Some("sup.line.rejected"),
                &m.rejected.to_string(),
                "bad",
                Some("sup.status.rejected_hint"),
            ));
        }
    }
    if let Some(secs) = convo.held_secs {
        l.push(tok(
            Some("sup.line.held"),
            &crate::adapters::driven::session_info::elapsed(secs as i64),
            "bad",
            Some("sup.status.held_hint"),
        ));
    }
    l
}

/// 마지막 턴의 결과 — 색으로 가른다. 실패가 대화 중간에 묻히면 답이 안 온 것과 오류를 못 가른다.
fn last_reply(convo: &Convo, busy: bool) -> Value {
    let (cls, key) = if busy {
        ("wait", "sup.status.waiting")
    } else {
        match convo.turns.last() {
            Some(t) if t.error.is_some() => ("bad", "sup.status.last_err"),
            Some(_) if convo.stopped.contains(&(convo.turns.len() - 1)) => {
                ("k", "sup.status.last_stopped")
            }
            Some(_) => ("ok", "sup.status.last_ok"),
            None => ("k", "sup.status.idle"),
        }
    };
    json!({ "t": "plain", "cls": cls, "text": crate::domain::i18n::t(key) })
}

/// pid · 메모리 토큰. 데몬이 살아 있다고 판정한 세션에만 값이 온다(`session_info`).
pub fn proc_tokens(p: Option<&Proc>, _now: i64) -> Vec<Value> {
    let Some(p) = p else {
        return Vec::new();
    };
    let mut l = Vec::new();
    if let Some(pid) = p.pid {
        l.push(tok(Some("sup.line.pid"), &pid.to_string(), "", None));
    }
    if let Some(kb) = p.rss_kb {
        l.push(tok(
            Some("sup.line.mem"),
            &crate::adapters::driven::session_info::memory(kb),
            "",
            Some("sup.status.memory_hint"),
        ));
    }
    l
}

/// 프로세스 칸이 바뀌었나를 가르는 값 — 경과는 시각이 흘러 바뀌므로 그린 모양으로 견준다(`activity`).
pub fn proc_key(p: Option<&Proc>, now: i64) -> String {
    let up = p
        .and_then(|p| p.started_unix)
        .map(|at| crate::adapters::driven::session_info::elapsed((now - at).max(0)))
        .unwrap_or_default();
    format!("{}|{up}", Value::Array(proc_tokens(p, now)))
}

/// 집 폴더를 `~` 로, 깊으면 `~/첫/…/끝` 으로.
fn short_path(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let p = match p.strip_prefix(&home).filter(|_| !home.is_empty()) {
        Some(rest) => format!("~{rest}"),
        None => p.to_string(),
    };
    let parts: Vec<&str> = p.split('/').collect();
    if parts.len() > 4 {
        format!("{}/{}/…/{}", parts[0], parts[1], parts[parts.len() - 1])
    } else {
        p
    }
}

fn bar(r: f64) -> String {
    let full = ((r * BAR_CELLS as f64).round() as usize).min(BAR_CELLS);
    format!("{}{}", "■".repeat(full), "░".repeat(BAR_CELLS - full))
}

fn percent(r: f64) -> u64 {
    (r * 100.0).round().max(0.0) as u64
}

/// 토큰 수를 짧게 — `26k`, `1.0M`.
fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", (n as f64 / 1_000.0).round() as u64)
    } else {
        n.to_string()
    }
}

/// 밀리초를 짧게 — `0.7s`, `48s`, `3m12s`.
fn seconds(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else if ms < 10_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{s}s")
    }
}

/// 차오른 정도의 색 — 상태줄의 기존 색(`wait`·`bad`)을 그대로 쓴다.
fn level(r: f64) -> &'static str {
    if r >= 0.9 {
        "bad"
    } else if r >= 0.75 {
        "wait"
    } else {
        ""
    }
}

/// 상태줄 모양과 리셋 시각을 현지 시각으로 바꾸는 함수 — 문서 끝에 한 번 싣는다. 상태줄을 다시
/// 그린 뒤에도 부른다(`activity::status_script`). 24시간 안이면 시각만, 넘으면 `MM-DD HH:mm`.
pub fn style_html() -> &'static str {
    r#"<style>
 .status{display:flex;gap:8px;align-items:flex-start;padding:6px 16px;border-top:1px solid #2a2e37;
         background:#14161b;color:#6b7280;font-size:11px;line-height:1.6;flex-shrink:0}
 .status .fold{color:#525a68;text-decoration:none;user-select:none}
 .status .cwd,.status .sid,.status .sess{text-decoration:none;cursor:pointer}
 .status .cwd:hover b,.status .sid:hover b,.status .sess:hover b{color:#c8ccd4;text-decoration:underline}
 .status .lines{min-width:0;flex:1}
 .status .ln{white-space:pre-wrap;word-break:break-all}
 .status b{color:#8b93a1;font-weight:400;user-select:text}
 .status .k{color:#525a68}
 .status .ok{color:#5ac17d} .status .bad{color:#e06c75} .status .wait{color:#d8a33c}
</style>"#
}

/// 페이지 앱의 `StatusBar` 와 같은 모양으로 그린다 — 시험 전용. 페이지 검사(jsdom)가 둘이 같은지 본다.
#[cfg(test)]
pub(crate) fn render_html(convo: &Convo, busy: bool) -> String {
    html_of(&render(convo, busy))
}

#[cfg(test)]
pub(crate) fn html_of(v: &Value) -> String {
    use crate::adapters::driving::html::escape;
    let s = |x: &Value| escape(x.as_str().unwrap_or_default());
    fn tok_html(k: &Value) -> String {
        use crate::adapters::driving::html::escape;
        let s = |x: &Value| escape(x.as_str().unwrap_or_default());
        let title = k["title"]
            .as_str()
            .map(|t| format!(r#" title="{}""#, escape(t)))
            .unwrap_or_default();
        let label = k["label"]
            .as_str()
            .map(|l| format!(r#"<span class="k">{}:</span>"#, escape(l)))
            .unwrap_or_default();
        let cls = k["cls"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(|c| format!(r#" class="{c}""#))
            .unwrap_or_default();
        format!(r#"<span{title}>{label}<b{cls}>{}</b></span>"#, s(&k["val"]))
    }
    let token = |k: &Value| -> String {
        match k["t"].as_str().unwrap_or_default() {
            "tok" => tok_html(k),
            "k" => format!(r#"<span class="k">{}</span>"#, s(&k["text"])),
            "perm" => format!(r#"<span class="k">⏵⏵</span> {}"#, tok_html(&k["tok"])),
            "cwd" => format!(
                r##"<a class="cwd" href="#" title="{}"><b>{}</b></a>"##,
                s(&k["title"]),
                s(&k["val"])
            ),
            "sid" => format!(
                r##"<a class="sid" href="#" title="{}"><span class="k">{}:</span><b>{}</b></a>"##,
                s(&k["title"]),
                s(&k["label"]),
                s(&k["val"])
            ),
            "sess" => format!(
                r##"{}<a class="sess" href="#" title="{}">{}<b>{}</b></a>{}"##,
                s(&k["pre"]),
                s(&k["title"]),
                k["label"]
                    .as_str()
                    .map(|l| format!(r#"<span class="k">{}:</span>"#, escape(l)))
                    .unwrap_or_default(),
                s(&k["val"]),
                s(&k["post"])
            ),
            "limit" => format!(
                r#"<span title="{}"><span class="k">{}:</span><b class="{}">{}%</b>(<time data-ts="{}"></time>)</span>"#,
                s(&k["title"]),
                s(&k["label"]),
                s(&k["cls"]),
                k["pct"],
                k["ts"]
            ),
            "clock" => format!(
                r#"<span title="{}"><span class="k">{}:</span><b><time data-until="{}"></time></b></span>"#,
                s(&k["title"]),
                s(&k["label"]),
                k["ts"]
            ),
            "plain" => format!(r#"<span class="{}">{}</span>"#, s(&k["cls"]), s(&k["text"])),
            _ => String::new(),
        }
    };
    let mut h = format!(
        r##"<div class="status{}"><a class="fold" href="#{}=1" title="{}">{}</a><div class="lines">"##,
        if v["folded"] == true { " folded" } else { "" },
        s(&v["foldMark"]),
        s(&v["foldTitle"]),
        s(&v["arrow"]),
    );
    for l in v["lines"].as_array().into_iter().flatten() {
        let parts: Vec<String> = l.as_array().into_iter().flatten().map(token).collect();
        h.push_str(r#"<div class="ln">"#);
        h.push_str(&parts.join("  "));
        h.push_str("</div>");
    }
    h.push_str("</div></div>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usage::{CLAUDE_STREAM_JSON, Window, apply_raw};

    const INIT: &str = r#"{"type":"system","subtype":"init","model":"claude-haiku-5-5","permissionMode":"bypassPermissions","output_style":"default","apiKeySource":"none","claude_code_version":"2.1.304"}"#;
    const RESULT: &str = r#"{"type":"result","duration_ms":1504,"duration_api_ms":1051,"total_cost_usd":0.0259,"usage":{"input_tokens":3,"cache_read_input_tokens":14167,"cache_creation_input_tokens":12251,"output_tokens":4},"modelUsage":{"claude-haiku-5-5":{"inputTokens":3,"outputTokens":4,"cacheReadInputTokens":14167,"cacheCreationInputTokens":12251,"contextWindow":200000}}}"#;
    const RATE: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.03,"resetsAt":1791878400},"seven_day":{"utilization":0.69,"resetsAt":1791936000}}}}"#;

    fn convo(lines: &[&str]) -> Convo {
        let mut c = Convo {
            session_id: Some("sup-1".into()),
            agent_session_id: Some("conv-1".into()),
            agent_name: Some("dev".into()),
            listed_cwd: Some("/w/proj".into()),
            listed_name: Some("named".into()),
            process: Some(Proc {
                pid: Some(4242),
                rss_kb: Some(2048),
                started_unix: Some(0),
            }),
            ..Default::default()
        };
        for l in lines {
            apply_raw(&mut c.usage, CLAUDE_STREAM_JSON, l);
        }
        c
    }

    /// 다섯 줄에 실측 값이 다 실린다 — 순서대로.
    #[test]
    fn the_full_status_line_carries_every_value_in_order() {
        let h = render_html(&convo(&[INIT, RATE, RESULT]), false);
        let want = [
            "/w/proj",
            "@dev(named)",
            ">4242<",
            ">2MB<",
            ">v2.1.304<",
            ">claude-haiku-5-5<",
            ">default<",
            "↑26k/↓4",
            ">conv-1<",
            "13% ■░░░░░ [26k/200k]",
            "1.5s",
            "data-ts=\"1791878400\"",
            "69%",
            "$0.026",
            "⏵⏵",
            ">bypassPermissions<",
            ">sup-1<",
        ];
        let mut at = 0;
        for w in want {
            let i = h[at..]
                .find(w)
                .unwrap_or_else(|| panic!("{w} 이 없거나 순서가 다르다: {h}"));
            at += i;
        }
        assert_eq!(h.matches(r#"<div class="ln">"#).count(), 5);
    }

    /// 넷째 줄 끝에 그 프로필의 최근 1분 · 최근 5분 429 · 차단이 붙는다. 아직 못 쟀으면 없고, 요청이
    /// 없던 1분은 `-` 다. 429 와 차단은 있을 때만, 붉게.
    #[test]
    fn the_profile_minute_rides_on_the_limit_line() {
        let mut c = convo(&[INIT, RESULT]);
        let quiet = render_html(&c, false);
        assert!(!quiet.contains("req ↑"), "{quiet}");
        c.meter = Some(crate::adapters::driven::rate_meter::Meter::default());
        let h = render_html(&c, false);
        assert!(
            h.contains(&format!(
                "{}:</span><b>-</b>",
                crate::domain::i18n::t("sup.line.minute")
            )),
            "{h}"
        );
        let rejected = format!("{}:</span>", crate::domain::i18n::t("sup.line.rejected"));
        assert!(!h.contains(&rejected), "429 가 없는데 칸을 그렸다: {h}");
        c.meter = Some(crate::adapters::driven::rate_meter::Meter {
            requests: 21,
            input: 406_000,
            output: 38_000,
            rejected: 4,
            ..Default::default()
        });
        c.held_secs = Some(720);
        let h = render_html(&c, false);
        let line = h
            .split(r#"<div class="ln">"#)
            .nth(4)
            .unwrap_or_else(|| panic!("넷째 줄이 없다: {h}"));
        let mut at = 0;
        for w in [
            "21req ↑406k/↓38k",
            r#"class="bad">4<"#,
            r#"class="bad">12m"#,
        ] {
            at += line[at..]
                .find(w)
                .unwrap_or_else(|| panic!("{w} 이 없거나 순서가 다르다: {line}"));
        }
    }

    /// 접으면 한 줄 — `{작업 폴더} {프로필}({계정}) {종류}({대화 id})`, 그 순서로. 단추는 펴기로 바뀐다.
    #[test]
    fn a_folded_status_line_is_one_line() {
        let mut c = convo(&[INIT, RESULT]);
        c.status_folded = true;
        c.agent_kind = Some("claude".into());
        c.account_email = Some("me@example.com".into());
        let h = render_html(&c, false);
        assert_eq!(h.matches(r#"<div class="ln">"#).count(), 1, "{h}");
        let mut at = 0;
        for w in [
            r#"class="cwd""#,
            ">/w/proj<",
            ">dev(me@example.com)<",
            r#"claude(<a class="sess""#,
            ">conv-1<",
        ] {
            at += h[at..]
                .find(w)
                .unwrap_or_else(|| panic!("{w} 이 없거나 순서가 다르다: {h}"));
        }
        // 시계를 알면 끝에 두 만료 시각이 붙는다.
        c.sync.last_result = Some(std::time::Instant::now());
        c.sync.clock.reclaim = true;
        let with_clocks = render_html(&c, false);
        let sess = with_clocks.find(">conv-1<").expect("대화 id");
        let cache = with_clocks
            .find("data-until")
            .expect("접은 줄에 시계가 없다");
        assert!(sess < cache, "{with_clocks}");
        assert_eq!(
            with_clocks.matches("data-until").count(),
            2,
            "{with_clocks}"
        );
        c.sync.last_result = None;
        // 접힌 줄에는 그 넷과 시계만 — 모델 · 컨텍스트 · 세션 이름은 편 줄에 있다.
        assert!(
            !h.contains("claude-haiku-5-5") && !h.contains("13%") && !h.contains("named"),
            "{h}"
        );
        assert!(h.contains("▸") && h.contains(r#"class="status folded""#));
        assert!(parse_toggle("about:blank#agent-status=1"));
        assert!(!parse_toggle("about:blank#agent-send=1&t=x"));
        // 계정을 모르면 프로필 이름만 · 대화 id 가 아직 없으면 supervisor id.
        c.account_email = None;
        c.agent_session_id = None;
        let h = render_html(&c, false);
        assert!(
            h.contains(">dev<") && h.contains("<b>sup-1</b></a>)"),
            "{h}"
        );
    }

    /// 편 줄도 프로필 옆에 계정이 붙는다 — 세션 이름은 그 뒤다.
    #[test]
    fn the_full_line_suffixes_the_account_to_the_profile() {
        let mut c = convo(&[INIT]);
        c.account_email = Some("me@example.com".into());
        let h = render_html(&c, false);
        assert!(h.contains(">@dev(me@example.com)(named)<"), "{h}");
    }

    /// 한도 줄이 아직 안 왔으면 `-` 로 자리를 지키고, API 세션은 Total 이다. 차오른 칸은 색이 붙는다.
    #[test]
    fn limits_hold_their_place_and_api_sessions_show_total() {
        let h = render_html(&convo(&[INIT, RESULT]), false);
        assert_eq!(h.matches("<b>-</b>").count(), 2, "{h}");
        let keyed = INIT.replace(
            r#""apiKeySource":"none""#,
            r#""apiKeySource":"ANTHROPIC_API_KEY""#,
        );
        let mut c = convo(&[&keyed, RESULT]);
        assert!(!render_html(&c, false).contains("<b>-</b>"));
        crate::domain::usage::apply_budget(
            &mut c.usage,
            &serde_json::json!({"spent": 40.0, "balance": 50.0}),
        );
        assert!(render_html(&c, false).contains(r#"<b class="wait">$40.00/$50.00</b>"#));
        let mut w = convo(&[INIT, RATE, RESULT]);
        w.usage.seven_day = Some(Window {
            utilization: 0.95,
            resets_at: 0,
        });
        assert!(render_html(&w, false).contains(r#"<b class="bad">95%</b>"#));
    }

    /// 값을 모르면 그 토큰이 빠지고, 빈 줄은 안 그린다 — 첫 턴 전 탭은 짧다.
    #[test]
    fn unknown_values_are_left_out() {
        let c = Convo {
            session_id: Some("sup-1".into()),
            ..Default::default()
        };
        let h = render_html(&c, false);
        assert!(
            !h.contains("PID") && !h.contains("Cost") && !h.contains("■"),
            "{h}"
        );
        assert!(h.contains(">sup-1<"));
        assert_eq!(
            h.matches("sup-1").count(),
            1,
            "같은 id 를 두 번 실었다: {h}"
        );
    }

    /// 모델 이름 · 이름은 escape 된다 — agent 와 사용자가 낸 글자다.
    #[test]
    fn values_are_escaped() {
        let mut c = convo(&[]);
        c.usage.model = Some("<img src=x onerror=alert(1)>".into());
        c.listed_name = Some("<b>x</b>".into());
        let h = render_html(&c, false);
        assert!(!h.contains("<img") && !h.contains("<b>x</b>"), "{h}");
    }

    /// 대화 탭의 조작 갈래가 접기 단추를 여기로 보낸다 — 빠지면 단추가 아무 일도 안 한다.
    #[test]
    fn the_tab_routes_the_fold_button_here() {
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn tab_control")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .expect("tab_control");
        assert!(body.contains("statusline::parse_toggle(") && body.contains("status_folded = !"));
    }

    /// 작업 폴더를 누르면 **전체** 경로가 클립보드에 간다 — 화면의 줄인 값이 아니다.
    #[test]
    fn clicking_the_folder_copies_the_full_path() {
        #[derive(Default)]
        struct Clip(std::cell::RefCell<Vec<String>>);
        impl crate::ports::host::HostCall for Clip {
            fn call(
                &self,
                method: &str,
                p: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                if method == "clipboard.set_text" {
                    self.0
                        .borrow_mut()
                        .push(p["text"].as_str().unwrap_or_default().into());
                }
                Ok(serde_json::Value::Null)
            }
        }
        let mut c = convo(&[]);
        c.listed_cwd = Some("/a/b/c/d/e/f".into());
        let h = render_html(&c, false);
        assert!(
            h.contains(r#"class="cwd""#)
                && h.contains("/a/…/f")
                && h.contains(r#"title="/a/b/c/d/e/f"#),
            "{h}"
        );
        let clip = Clip::default();
        assert!(copy_cwd(
            &clip,
            7,
            &c,
            "about:blank#agent-copy-cwd=1792570000000"
        ));
        assert_eq!(*clip.0.borrow(), vec!["/a/b/c/d/e/f".to_string()]);
        assert!(!copy_cwd(&clip, 7, &c, "about:blank#agent-status=1"));
        assert_eq!(clip.0.borrow().len(), 1);
        // 이 탭의 조작 갈래가 여기로 보낸다.
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn tab_control")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .unwrap();
        assert!(body.contains("statusline::copy_cwd("));
    }

    /// 펼친 상태줄에 이 탭의 surface id 가 나오고, 누르면 그 id 가 복사된다. 모르면(0) 안 보인다.
    #[test]
    fn clicking_the_surface_id_copies_it() {
        #[derive(Default)]
        struct Clip(std::cell::RefCell<Vec<String>>);
        impl crate::ports::host::HostCall for Clip {
            fn call(
                &self,
                method: &str,
                p: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                if method == "clipboard.set_text" {
                    self.0
                        .borrow_mut()
                        .push(p["text"].as_str().unwrap_or_default().into());
                }
                Ok(serde_json::Value::Null)
            }
        }
        let mut c = convo(&[]);
        assert!(!render_html(&c, false).contains(r#"class="sid""#));
        c.surface_id = 42;
        let h = render_html(&c, false);
        assert!(
            h.contains(r#"class="sid""#) && h.contains("<b>42</b>"),
            "{h}"
        );
        let clip = Clip::default();
        assert!(copy_surface(
            &clip,
            42,
            "about:blank#agent-copy-surface=1792570000000"
        ));
        assert_eq!(*clip.0.borrow(), vec!["42".to_string()]);
        assert!(!copy_surface(&clip, 42, "about:blank#agent-copy-cwd=1"));
        assert_eq!(clip.0.borrow().len(), 1);
        // 이 탭의 조작 갈래가 여기로 보낸다.
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn tab_control")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .unwrap();
        assert!(body.contains("statusline::copy_surface("));
    }

    /// 대화 id 를 누르면 **전체** id 가 복사된다 — 펼친 줄(`sess:`)과 접은 줄(`종류(id)`) 모두 누를 수
    /// 있고, 대화 id 가 없으면 이 탭의 supervisor id 를 복사한다.
    #[test]
    fn clicking_the_session_id_copies_the_full_id() {
        #[derive(Default)]
        struct Clip(std::cell::RefCell<Vec<String>>);
        impl crate::ports::host::HostCall for Clip {
            fn call(
                &self,
                method: &str,
                p: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                if method == "clipboard.set_text" {
                    self.0
                        .borrow_mut()
                        .push(p["text"].as_str().unwrap_or_default().into());
                }
                Ok(serde_json::Value::Null)
            }
        }
        let full = "c8e27346-1f2e-4b6a-9d0c-0123456789ab";
        let mut c = convo(&[]);
        c.agent_session_id = Some(full.into());
        let h = render_html(&c, false);
        assert!(
            h.contains(r##"<a class="sess" href="#" title=""##)
                && h.contains(&format!(r#"<span class="k">sess:</span><b>{full}</b></a>"#)),
            "{h}"
        );
        c.status_folded = true;
        c.agent_kind = Some("claude".into());
        let h = render_html(&c, false);
        assert!(
            h.contains(r#"claude(<a class="sess""#) && h.contains(&format!("<b>{full}</b></a>)")),
            "{h}"
        );
        let clip = Clip::default();
        assert!(copy_session(
            &clip,
            7,
            &c,
            "about:blank#agent-copy-session=1792570000000"
        ));
        assert_eq!(*clip.0.borrow(), vec![full.to_string()]);
        assert!(!copy_session(&clip, 7, &c, "about:blank#agent-copy-cwd=1"));
        assert_eq!(clip.0.borrow().len(), 1);
        c.agent_session_id = None;
        assert!(copy_session(
            &clip,
            7,
            &c,
            "about:blank#agent-copy-session=2"
        ));
        assert_eq!(clip.0.borrow().last().map(String::as_str), Some("sup-1"));
        // 이 탭의 조작 갈래가 여기로 보낸다 · 페이지 앱이 같은 모양을 그린다.
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn tab_control")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .unwrap();
        assert!(body.contains("statusline::copy_session("));
        let app = include_str!("../../../assets/app.js");
        assert!(app.contains(r#"case 'sess': return html`${k.pre || ''}<a class="sess""#));
    }

    #[test]
    fn short_forms() {
        assert_eq!(bar(0.71), "■■■■░░");
        assert_eq!(seconds(48_000), "48s");
        assert_eq!(seconds(192_000), "3m12s");
        assert_eq!(short_path("/a/b/c/d/e"), "/a/…/e");
    }
}
