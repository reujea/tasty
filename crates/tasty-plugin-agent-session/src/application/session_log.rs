//! 세션 현황 색인 — 현행화가 끝날 때 tasty memory 에 그 세션의 상태 · 로그와 워크스페이스
//! 요약을 남긴다(B51 S1).
//!
//! 계획 · 현황의 **정본은 프로젝트 문서**(spec · prd · 재개 지점)다. 여기 남기는 것은 기계가
//! 찾아 읽을 색인뿐이다 — 어느 세션이 언제 어떤 현행화를 했고 잘 끝났는지, 그때 컨텍스트가
//! 얼마였는지. 키는 surface id 가 아니라 **에이전트 대화 id 앞 8자**다 — surface id 는
//! tasty 를 다시 띄울 때마다 바뀐다. scope 는 그 탭이 있는 워크스페이스(`workspace:<id>`)다.
//!
//! | 키 | 값 |
//! |---|---|
//! | `sess.<id8>.status` | 마지막 현행화와 세션 정체 — 덮어쓴다 |
//! | `sess.<id8>.log.<unix ms>` | 현행화 한 번 — 30일 뒤 사라진다 |
//! | `sess.summary` | 그 워크스페이스의 `sess.*.status` 를 모은 목록 |
//!
//! 기록이 실패해도 현행화는 그대로 간다 — 색인이 없다고 일을 막지 않는다.

use serde_json::{Value, json};

use crate::domain::convo::Convo;
use crate::domain::sync_rules::Mode;
use crate::ports::host::HostCall;

/// 로그 한 줄이 남는 기간 — 지나면 호스트가 지운다.
const LOG_TTL_MS: i64 = 30 * 86_400 * 1000;

/// 세션 키 — 에이전트 대화 id 앞 8자, 모르면 supervisor 세션 id 앞 8자.
pub fn key_id(c: &Convo) -> Option<String> {
    c.agent_session_id
        .as_deref()
        .or(c.session_id.as_deref())
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(8).collect())
}

/// 그 surface 가 있는 워크스페이스 — `surface.list` 에서 찾는다(`surface.locate` 는 안 싣는다).
fn workspace_of<H: HostCall>(host: &H, sid: u32) -> Option<u64> {
    let list = host.call("surface.list", json!({})).ok()?;
    let rows = list
        .as_array()
        .or_else(|| list.get("surfaces").and_then(Value::as_array))?;
    rows.iter()
        .find(|r| r.get("id").and_then(Value::as_u64) == Some(u64::from(sid)))
        .and_then(|r| r.get("workspace_id").and_then(Value::as_u64))
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Auto => "auto",
        Mode::Manual => "manual",
        Mode::Idle => "idle",
        Mode::Limit => "limit",
    }
}

/// 현행화 한 번의 기록.
pub fn entry(c: &Convo, sid: u32, mode: Mode, failed: Option<&str>, at_ms: i64) -> Value {
    json!({
        "mode": mode_name(mode),
        "ok": failed.is_none(),
        "reason": failed,
        "at_ms": at_ms,
        "context_tokens": c.usage.context_used,
        "surface_id": sid,
    })
}

/// 세션 상태 — 마지막 현행화와 그 세션이 누구인지.
pub fn status(c: &Convo, last: &Value) -> Value {
    json!({
        "agent_session_id": c.agent_session_id,
        "session_id": c.session_id,
        "agent": c.agent_name,
        "cwd": c.listed_cwd,
        "turns": c.turns.len(),
        "last_sync": last,
    })
}

/// 현행화가 끝났다 — 상태 · 로그 · 요약을 쓴다. 실패는 로그만 남긴다.
pub fn record<H: HostCall>(host: &H, sid: u32, c: &Convo, mode: Mode, failed: Option<&str>) {
    let Some(id8) = key_id(c) else {
        return;
    };
    let Some(ws) = workspace_of(host, sid) else {
        tracing::debug!("[agent_session] surface={sid}: 워크스페이스를 몰라 현황을 안 남겼다");
        return;
    };
    let scope = format!("workspace:{ws}");
    let at_ms = crate::adapters::driven::store::budget::now() * 1000;
    let last = entry(c, sid, mode, failed, at_ms);
    let puts = [
        json!({ "scope": scope, "key": format!("sess.{id8}.status"), "value": status(c, &last) }),
        json!({ "scope": scope, "key": format!("sess.{id8}.log.{at_ms}"), "value": last,
                "expires_at": at_ms + LOG_TTL_MS }),
    ];
    for p in puts {
        if let Err(e) = host.call("memory.put", p) {
            tracing::warn!("[agent_session] surface={sid}: 세션 현황을 못 남겼다: {e}");
            return;
        }
    }
    write_summary(host, &scope);
}

/// 그 워크스페이스의 `sess.*.status` 를 모아 `sess.summary` 에 쓴다 — 한 번 읽어 전체를 본다.
fn write_summary<H: HostCall>(host: &H, scope: &str) {
    let listed = match host.call("memory.list", json!({ "scope": scope, "prefix": "sess." })) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("[agent_session] 세션 요약을 못 읽었다: {e}");
            return;
        }
    };
    let sessions = summary_of(&listed);
    if let Err(e) = host.call(
        "memory.put",
        json!({ "scope": scope, "key": "sess.summary", "value": { "sessions": sessions } }),
    ) {
        tracing::warn!("[agent_session] 세션 요약을 못 남겼다: {e}");
    }
}

/// `memory.list` 응답에서 `.status` 항목만 골라 `{id, …상태}` 목록으로 — 최근 현행화가 앞이다.
fn summary_of(listed: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = listed
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let key = e.get("key").and_then(Value::as_str)?;
            let id = key.strip_prefix("sess.")?.strip_suffix(".status")?;
            let mut v = e.get("value")?.clone();
            v.as_object_mut()?.insert("id".into(), id.into());
            Some(v)
        })
        .collect();
    let at = |v: &Value| {
        v.pointer("/last_sync/at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    out.sort_by_key(|v| std::cmp::Reverse(at(v)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// `surface.list` 와 `memory.*` 를 흉내 낸다 — 쓴 것을 들고 있다가 list 로 돌려준다.
    #[derive(Default)]
    struct Host {
        puts: RefCell<Vec<Value>>,
    }

    impl HostCall for Host {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            match method {
                "surface.list" => Ok(json!([{ "id": 5, "workspace_id": 3 }])),
                "memory.put" => {
                    self.puts.borrow_mut().push(params);
                    Ok(json!({ "ok": true }))
                }
                "memory.list" => {
                    let entries: Vec<Value> = self
                        .puts
                        .borrow()
                        .iter()
                        .map(|p| json!({ "key": p["key"], "value": p["value"] }))
                        .collect();
                    Ok(json!({ "entries": entries }))
                }
                other => Err(format!("unexpected {other}")),
            }
        }
    }

    fn convo() -> Convo {
        Convo {
            session_id: Some("sup-session-1".into()),
            agent_session_id: Some("b5afb537-63ce".into()),
            agent_name: Some("dev".into()),
            ..Default::default()
        }
    }

    #[test]
    fn a_sync_end_writes_status_log_and_summary_in_the_tabs_workspace() {
        let host = Host::default();
        record(&host, 5, &convo(), Mode::Idle, None);
        let puts = host.puts.borrow();
        let keys: Vec<&str> = puts.iter().map(|p| p["key"].as_str().unwrap()).collect();
        assert_eq!(keys[0], "sess.b5afb537.status", "대화 id 앞 8자가 키다");
        assert!(keys[1].starts_with("sess.b5afb537.log."));
        assert_eq!(keys[2], "sess.summary");
        assert!(puts.iter().all(|p| p["scope"] == "workspace:3"));
        assert!(
            puts[1]["expires_at"].as_i64().is_some(),
            "로그가 영영 쌓인다"
        );
        assert_eq!(puts[0]["value"]["last_sync"]["mode"], "idle");
        assert_eq!(puts[0]["value"]["last_sync"]["ok"], true);
        let summary = &puts[2]["value"]["sessions"];
        assert_eq!(
            summary.as_array().unwrap().len(),
            1,
            "로그 줄을 요약에 넣었다"
        );
        assert_eq!(summary[0]["id"], "b5afb537");
    }

    #[test]
    fn a_failed_sync_is_recorded_with_its_reason() {
        let host = Host::default();
        record(&host, 5, &convo(), Mode::Auto, Some("compact 실패"));
        let s = &host.puts.borrow()[0]["value"]["last_sync"];
        assert_eq!(s["ok"], false);
        assert_eq!(s["reason"], "compact 실패");
    }

    #[test]
    fn without_a_known_tab_or_session_nothing_is_written() {
        let host = Host::default();
        record(&host, 99, &convo(), Mode::Manual, None);
        record(&host, 5, &Convo::default(), Mode::Manual, None);
        assert!(host.puts.borrow().is_empty());
    }

    #[test]
    fn the_summary_puts_the_latest_sync_first() {
        let listed = json!({ "entries": [
            { "key": "sess.aaaa.status", "value": { "last_sync": { "at_ms": 1 } } },
            { "key": "sess.aaaa.log.1", "value": {} },
            { "key": "sess.bbbb.status", "value": { "last_sync": { "at_ms": 9 } } },
        ]});
        let s = summary_of(&listed);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0]["id"], "bbbb");
    }

    /// 현행화가 끝나는 두 자리(compact 끝 · 현행화 턴 실패)가 이 기록을 부른다.
    #[test]
    fn both_sync_endings_record() {
        let src = include_str!("sync_compact.rs");
        let shipped = &src[..src.find("#[cfg(test)]").unwrap()];
        for f in ["fn sync_done(", "fn sync_failed("] {
            let body = shipped
                .split(f)
                .nth(1)
                .and_then(|b| b.split("\n    }\n").next())
                .unwrap();
            assert!(
                body.contains("session_log::record("),
                "{f} 가 현황을 안 남긴다"
            );
        }
    }
}
