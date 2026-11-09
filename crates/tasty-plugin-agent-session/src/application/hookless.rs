//! 대화 탭이 **Claude Code hook 없이** 내는 신호 — 응답 흐름에서 읽는다. 자식 환경에서
//! `TASTY_*` 를 빼므로 claude hook 은 이 탭에 보고하지 않는다.
//!
//! 터미널 claude 는 hook(`tasty claude hook <event>`)이 상태를 알려 준다. 대화 탭은 데몬을 거쳐 이미
//! claude 의 원문(stream-json)과 세션 목록 행을 받으므로, 같은 사실을 거기서 꺼낸다.
//!
//! | hook 이 하던 일 | 여기서 쓰는 신호 |
//! |---|---|
//! | PreToolUse/PostToolUse[AskUserQuestion] → 입력 대기 | 원문 `assistant` 의 `tool_use`(이름 `AskUserQuestion`) · 짝 `tool_result` |
//! | 턴 중 오류 → `claude-error` | 원문 `result` 의 `is_error` · `subtype`(`error_*`) |
//! | SessionEnd → `claude-idle` · 완료 표시 | 세션 행 `state` 가 끝·실패·회수 아닌 멈춤으로 바뀜 |
//! | SessionStart → 대화 id 표지 | 세션 행 `agent_session_id` · 원문 `system/init` 의 `session_id` |
//!
//! 대화 id 표지는 `claude-session-id` 가 아니라 [`CONVERSATION_META_KEY`] 다 — 그 키가 있으면 claude
//! plugin 의 `reboot` 이 터미널로 보고 Ctrl+C 를 보낸다. 잇기 키(`surface_link::SESSION_META_KEY`)와도
//! 다르다 — 0.1.22 까지 둘이 같은 문자열이라 이어받은 · 비운 세션의 잇기가 대화 id 로 덮였다(B107,
//! [`repair_link`] 가 되돌린다).

use serde_json::{Value, json};

use crate::domain::convo::Convo;
use crate::ports::host::HostCall;

/// 대화 탭의 claude 대화 id 를 남기는 surface meta 키 — 읽는 곳은 없다(사람과 진단용 표지).
///
/// 잇기 키(`surface_link::SESSION_META_KEY`, supervisor 기록 id)와 **같으면 안 된다**: 기록 id 와 대화 id 는
/// 이어받기(`adopt`) · `/clear` 에서 갈린다(`local.rs` 의 `spawn` · `local/turns.rs` 의 `apply_line`).
pub const CONVERSATION_META_KEY: &str = "agent-session-conversation-id";

/// 원문 한 줄에서 읽은 신호.
#[derive(Debug, PartialEq, Eq)]
pub enum Raw {
    /// claude 가 사용자에게 물었다(`AskUserQuestion` 호출) — 그 tool_use id.
    AskStart(String),
    /// 그 물음에 답이 왔다(짝 `tool_result`).
    AskEnd(String),
    /// 턴 결과가 오류다.
    TurnError,
    /// 대화 id(`system/init`).
    SessionInit(String),
}

/// 탭 하나의 hook 없는 신호 상태. 저장하지 않는다.
#[derive(Debug, Default)]
pub struct State {
    /// 원문으로 본 물음 — 데몬 행의 물음(protocol 13)이 없을 때만 쓴다.
    pub ask: Option<String>,
    /// 이번 턴의 결과가 오류였다 — 턴 끝의 hook 을 `claude-error` 로 바꾼다. `started` 에서 지운다.
    pub turn_error: bool,
    /// 마지막으로 본 세션 행의 (state, reclaimed).
    pub last_row: Option<(String, bool)>,
    /// 마지막으로 남긴 대화 id 표지.
    pub meta_session: Option<String>,
    /// 잇기를 세션 목록과 맞춰 본 세션 id([`repair_link`]) — 같은 값은 다시 안 본다.
    pub link_checked: Option<String>,
}

fn content_items(v: &Value) -> impl Iterator<Item = &Value> {
    v.pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// 원문 한 줄의 신호들. claude stream-json 이 아니면 비어 있다.
pub fn signals(format: &str, line: &str) -> Vec<Raw> {
    if format != crate::domain::usage::CLAUDE_STREAM_JSON {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match v.get("type").and_then(Value::as_str) {
        Some("assistant") => {
            for c in content_items(&v) {
                if c.get("type").and_then(Value::as_str) == Some("tool_use")
                    && c.get("name").and_then(Value::as_str) == Some("AskUserQuestion")
                    && let Some(id) = c.get("id").and_then(Value::as_str)
                {
                    out.push(Raw::AskStart(id.to_string()));
                }
            }
        }
        Some("user") => {
            for c in content_items(&v) {
                if c.get("type").and_then(Value::as_str) == Some("tool_result")
                    && let Some(id) = c.get("tool_use_id").and_then(Value::as_str)
                {
                    out.push(Raw::AskEnd(id.to_string()));
                }
            }
        }
        Some("result") => {
            let is_error = v.get("is_error").and_then(Value::as_bool) == Some(true);
            let error_subtype = v
                .get("subtype")
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with("error"));
            if is_error || error_subtype {
                out.push(Raw::TurnError);
            }
        }
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            if let Some(id) = v.get("session_id").and_then(Value::as_str) {
                out.push(Raw::SessionInit(id.to_string()));
            }
        }
        _ => {}
    }
    out
}

fn is_claude(convo: &Convo) -> bool {
    convo.agent_kind.as_deref().is_none_or(|k| k == "claude")
}

/// 원문 한 줄을 반영한다 — 물음 강조·hook, 턴 오류 표지, 대화 id 표지.
pub fn on_raw<H: HostCall>(host: &H, sid: u32, convo: &mut Convo, format: &str, line: &str) {
    for s in signals(format, line) {
        match s {
            Raw::AskStart(id) => {
                // 데몬이 물음을 행에 싣는 세션(protocol 13)은 `prompts` 가 이미 알린다 — 겹쳐 쏘지 않는다.
                if convo.needs_input.is_some() || convo.hookless.ask.is_some() {
                    continue;
                }
                convo.hookless.ask = Some(id);
                if let Err(e) = host.call(
                    "surface.completion",
                    json!({ "surface_id": sid, "kind": "needs_input" }),
                ) {
                    tracing::warn!("[agent_session] surface={sid}: 입력 대기 표시를 못 켰다: {e}");
                }
                if is_claude(convo)
                    && let Err(e) = crate::application::surface_link::fire_hook(
                        host,
                        sid,
                        crate::application::prompts::NEEDS_INPUT_EVENT,
                    )
                {
                    tracing::warn!(
                        "[agent_session] surface={sid}: hook 'needs-input' 발사 실패: {e}"
                    );
                }
            }
            Raw::AskEnd(id) => {
                if convo.hookless.ask.as_deref() == Some(id.as_str()) {
                    convo.hookless.ask = None;
                    if let Err(e) =
                        crate::application::surface_link::clear_reclaimed_mark(host, sid)
                    {
                        tracing::debug!(
                            "[agent_session] surface={sid}: 입력 대기 표시를 못 거뒀다: {e}"
                        );
                    }
                }
            }
            Raw::TurnError => convo.hookless.turn_error = true,
            Raw::SessionInit(id) => note_session_meta(host, sid, convo, &id),
        }
    }
}

/// 대화 id 표지를 남긴다 — 바뀔 때만 쓴다.
pub fn note_session_meta<H: HostCall>(host: &H, sid: u32, convo: &mut Convo, id: &str) {
    if id.is_empty() || convo.hookless.meta_session.as_deref() == Some(id) {
        return;
    }
    match host.call(
        "surface.meta.set",
        json!({ "surface_id": sid, "key": CONVERSATION_META_KEY, "value": id }),
    ) {
        Ok(_) => convo.hookless.meta_session = Some(id.to_string()),
        Err(e) => tracing::debug!("[agent_session] surface={sid}: 대화 id 표지를 못 남겼다: {e}"),
    }
}

/// 잇기 값을 세션 목록과 맞춘 결과.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkCheck {
    /// 기록 id 다 — 고칠 것이 없다.
    Known,
    /// 기록 id 가 아니고 그 대화를 가진 기록이 하나다 — 그 기록 id 로 되돌린다.
    Restore(String),
    /// 기록 id 도 아니고 그 대화를 가진 기록도 없다.
    Unknown,
    /// 그 대화를 가진 기록이 둘 이상이다 — 고르지 않는다(잘못 고르면 한 대화에 다른 프로세스가 붙는다).
    Ambiguous(usize),
}

/// 잇기 값 `linked` 를 세션 목록 행(`id` · `agent_session_id`)과 맞춘다.
pub fn check_link(rows: &[Value], linked: &str) -> LinkCheck {
    let field = |r: &Value, k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
    if rows
        .iter()
        .any(|r| field(r, "id").as_deref() == Some(linked))
    {
        return LinkCheck::Known;
    }
    let owners: Vec<String> = rows
        .iter()
        .filter(|r| field(r, "agent_session_id").as_deref() == Some(linked))
        .filter_map(|r| field(r, "id"))
        .collect();
    match owners.as_slice() {
        [] => LinkCheck::Unknown,
        [one] => LinkCheck::Restore(one.clone()),
        many => LinkCheck::Ambiguous(many.len()),
    }
}

/// 대화 id 로 덮인 잇기를 기록 id 로 되돌린다 — 탭마다 세션 id 가 바뀔 때 한 번(B107).
///
/// 0.1.22 까지 대화 id 표지가 잇기 키에 쓰였다. 기록 id ≠ 대화 id 인 탭은 init 한 번에 meta 가 대화 id 가
/// 됐고, 그 값으로 `status` · `tell` · `output` · 복원이 "그런 세션이 없다" 로 끝났다. 이미 덮인 탭(복원된
/// 탭 포함)을 활동 폴링의 첫 목록에서 고친다. 고칠 근거가 없으면(모르는 값 · 모호) 그대로 두고 경고만 남긴다.
pub fn repair_link<H: HostCall>(host: &H, sid: u32, convo: &mut Convo, rows: &[Value]) {
    if convo.session_id.is_none() || convo.hookless.link_checked == convo.session_id {
        return;
    }
    // 호스트 호출이 실패했으면 표를 안 남긴다 — 다음 주기에 다시 본다.
    let linked = match crate::application::surface_link::session_of_surface(host, sid) {
        Ok(v) => v,
        Err(crate::application::surface_link::LinkError::NotLinked { .. }) => {
            convo.hookless.link_checked = convo.session_id.clone();
            return;
        }
        Err(crate::application::surface_link::LinkError::HostCall(e)) => {
            tracing::debug!("[agent_session] surface={sid}: 잇기를 못 읽었다: {e}");
            return;
        }
    };
    let check = check_link(rows, &linked);
    if let LinkCheck::Restore(id) = &check {
        if let Err(e) = crate::application::surface_link::link(host, sid, id) {
            tracing::warn!(
                "[agent_session] surface={sid}: 잇기를 못 되돌렸다({linked} → {id}): {e}"
            );
            return;
        }
        tracing::info!(
            "[agent_session] surface={sid}: 대화 id({linked})로 덮인 잇기를 기록 id({id})로 되돌렸다"
        );
        if convo.session_id.as_deref() == Some(linked.as_str()) {
            convo.session_id = Some(id.clone());
        }
    }
    convo.hookless.link_checked = convo.session_id.clone();
    match check {
        LinkCheck::Known | LinkCheck::Restore(_) => {}
        LinkCheck::Unknown => tracing::warn!(
            "[agent_session] surface={sid}: 이어진 값 {linked} 이 기록 id 도 대화 id 도 아니다 — 그대로 둔다"
        ),
        LinkCheck::Ambiguous(n) => tracing::warn!(
            "[agent_session] surface={sid}: 이어진 대화 id {linked} 를 가진 기록이 {n}개다 — 고르지 않고 둔다"
        ),
    }
}

/// 세션이 끝났을 때 낼 hook — 상태가 **바뀌는 순간** 한 번. 회수(reclaimed)는 쉬는 것이라 알리지 않는다.
pub fn session_end_event(
    prev: Option<&(String, bool)>,
    state: &str,
    reclaimed: bool,
) -> Option<&'static str> {
    let ended = match state {
        "finished" => Some("claude-idle"),
        "failed" => Some("claude-error"),
        "stopped" if !reclaimed => Some("claude-idle"),
        _ => None,
    }?;
    let same = prev.is_some_and(|(s, r)| s == state && *r == reclaimed);
    // 첫 관측(탭이 막 붙었을 때)은 이미 끝나 있던 세션이다 — 지금 끝난 것이 아니다.
    (!same && prev.is_some()).then_some(ended)
}

/// 세션 행 하나를 반영한다 — 세션 종료 알림과 대화 id 표지.
pub fn note_row<H: HostCall>(host: &H, sid: u32, convo: &mut Convo, row: &Value) {
    if let Some(id) = row.get("agent_session_id").and_then(Value::as_str) {
        note_session_meta(host, sid, convo, id);
    }
    let Some(state) = row.get("state").and_then(Value::as_str) else {
        return;
    };
    let reclaimed = row.get("reclaimed").and_then(Value::as_bool) == Some(true);
    let event = session_end_event(convo.hookless.last_row.as_ref(), state, reclaimed);
    convo.hookless.last_row = Some((state.to_string(), reclaimed));
    let Some(event) = event else {
        return;
    };
    tracing::info!("[agent_session] surface={sid}: 세션이 끝났다({state}) — '{event}'");
    if let Err(e) = crate::application::surface_link::mark_done(host, sid) {
        tracing::warn!("[agent_session] surface={sid}: 세션 종료 표시를 못 켰다: {e}");
    }
    if is_claude(convo)
        && let Err(e) = crate::application::surface_link::fire_hook(host, sid, event)
    {
        tracing::warn!("[agent_session] surface={sid}: hook '{event}' 발사 실패: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const F: &str = crate::domain::usage::CLAUDE_STREAM_JSON;

    #[test]
    fn an_ask_user_question_call_and_its_answer_are_read() {
        // U-G4
        let ask = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"x"},{"type":"tool_use","id":"tu1","name":"AskUserQuestion","input":{}}]}}"#;
        assert_eq!(signals(F, ask), vec![Raw::AskStart("tu1".into())]);
        let other = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu2","name":"Read","input":{}}]}}"#;
        assert!(signals(F, other).is_empty(), "다른 도구는 물음이 아니다");
        let ans = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"a"}]}}"#;
        assert_eq!(signals(F, ans), vec![Raw::AskEnd("tu1".into())]);
        assert!(
            signals("pi-json", ask).is_empty(),
            "claude 원문이 아니면 읽지 않는다"
        );
    }

    #[test]
    fn an_error_result_and_the_init_id_are_read() {
        // U-G6 · U-G3
        assert_eq!(
            signals(F, r#"{"type":"result","is_error":true}"#),
            vec![Raw::TurnError]
        );
        assert_eq!(
            signals(F, r#"{"type":"result","subtype":"error_max_turns"}"#),
            vec![Raw::TurnError]
        );
        assert!(
            signals(
                F,
                r#"{"type":"result","subtype":"success","is_error":false}"#
            )
            .is_empty()
        );
        assert_eq!(
            signals(
                F,
                r#"{"type":"system","subtype":"init","session_id":"abc"}"#
            ),
            vec![Raw::SessionInit("abc".into())]
        );
    }

    #[derive(Default)]
    struct Host(std::cell::RefCell<Vec<(String, Value)>>);
    impl HostCall for Host {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            self.0.borrow_mut().push((method.to_string(), params));
            Ok(Value::Null)
        }
    }
    impl Host {
        fn count(&self, method: &str) -> usize {
            self.0.borrow().iter().filter(|(m, _)| m == method).count()
        }
    }

    #[test]
    fn an_ask_is_signalled_once_and_not_on_top_of_a_daemon_question() {
        // U-G4
        let ask = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu1","name":"AskUserQuestion","input":{}}]}}"#;
        let host = Host::default();
        let mut c = Convo::default();
        on_raw(&host, 1, &mut c, F, ask);
        on_raw(&host, 1, &mut c, F, ask);
        assert_eq!(host.count("surface.fire_hook"), 1, "같은 물음은 한 번");
        let host = Host::default();
        let mut c = Convo {
            needs_input: Some(crate::application::prompts::Pending {
                request_id: "r1".into(),
                kind: "question".into(),
                tool: "AskUserQuestion".into(),
                input: Value::Null,
            }),
            ..Default::default()
        };
        on_raw(&host, 1, &mut c, F, ask);
        assert_eq!(
            host.count("surface.fire_hook"),
            0,
            "데몬 물음이 이미 알렸다"
        );
    }

    #[test]
    fn a_session_end_is_announced_once_on_the_transition() {
        // U-G5
        let run = Some(("running".to_string(), false));
        assert_eq!(
            session_end_event(run.as_ref(), "stopped", false),
            Some("claude-idle")
        );
        assert_eq!(
            session_end_event(run.as_ref(), "stopped", true),
            None,
            "회수는 알리지 않는다"
        );
        assert_eq!(
            session_end_event(run.as_ref(), "failed", false),
            Some("claude-error")
        );
        assert_eq!(
            session_end_event(run.as_ref(), "finished", false),
            Some("claude-idle")
        );
        let stopped = Some(("stopped".to_string(), false));
        assert_eq!(
            session_end_event(stopped.as_ref(), "stopped", false),
            None,
            "같은 상태 반복은 0"
        );
        assert_eq!(
            session_end_event(None, "stopped", false),
            None,
            "첫 관측은 이미 끝나 있던 것"
        );
    }

    #[test]
    fn the_session_meta_is_written_only_when_it_changes() {
        // U-G3
        let host = Host::default();
        let mut c = Convo::default();
        note_session_meta(&host, 1, &mut c, "a");
        note_session_meta(&host, 1, &mut c, "a");
        note_session_meta(&host, 1, &mut c, "b");
        assert_eq!(host.count("surface.meta.set"), 2);
        let keys: Vec<String> = host
            .0
            .borrow()
            .iter()
            .map(|(_, p)| p["key"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(
            keys.iter().all(|k| k == CONVERSATION_META_KEY),
            "claude-session-id 를 쓰지 않는다"
        );
    }

    /// meta 를 들고 있는 호스트 — `surface.meta.*` 를 실제로 기억한다.
    #[derive(Default)]
    struct MetaHost(std::cell::RefCell<std::collections::HashMap<(u64, String), String>>);
    impl HostCall for MetaHost {
        fn call(&self, method: &str, p: Value) -> Result<Value, String> {
            let key = (
                p["surface_id"].as_u64().unwrap_or_default(),
                p["key"].as_str().unwrap_or_default().to_string(),
            );
            let mut m = self.0.borrow_mut();
            match method {
                "surface.meta.set" => {
                    m.insert(key, p["value"].as_str().unwrap_or_default().to_string());
                    Ok(Value::Null)
                }
                "surface.meta.get" => Ok(json!({ "value": m.get(&key) })),
                _ => Ok(Value::Null),
            }
        }
    }

    /// 기록 id ≠ 대화 id 인 세션 — init 뒤에도 잇기가 기록 id 다(B107).
    #[test]
    fn the_init_id_does_not_overwrite_the_link() {
        assert_ne!(
            CONVERSATION_META_KEY,
            crate::application::surface_link::SESSION_META_KEY,
            "대화 id 표지와 잇기는 다른 키다"
        );
        let host = MetaHost::default();
        crate::application::surface_link::link(&host, 1, "rec-1").expect("잇기");
        let mut c = Convo {
            session_id: Some("rec-1".into()),
            ..Default::default()
        };
        on_raw(
            &host,
            1,
            &mut c,
            F,
            r#"{"type":"system","subtype":"init","session_id":"conv-9"}"#,
        );
        note_row(
            &host,
            1,
            &mut c,
            &json!({ "id": "rec-1", "agent_session_id": "conv-9" }),
        );
        assert_eq!(
            crate::application::surface_link::session_of_surface(&host, 1)
                .ok()
                .as_deref(),
            Some("rec-1"),
            "init 이 잇기를 덮었다"
        );
        let conv = host.call(
            "surface.meta.get",
            json!({ "surface_id": 1, "key": CONVERSATION_META_KEY }),
        );
        assert_eq!(conv.ok().map(|v| v["value"].clone()), Some(json!("conv-9")));
    }

    fn rows() -> Vec<Value> {
        vec![
            json!({ "id": "rec-1", "agent_session_id": "conv-9" }),
            json!({ "id": "rec-2", "agent_session_id": "rec-2" }),
            json!({ "id": "rec-3", "agent_session_id": "dup" }),
            json!({ "id": "rec-4", "agent_session_id": "dup" }),
        ]
    }

    #[test]
    fn a_link_is_checked_against_record_and_conversation_ids() {
        let r = rows();
        assert_eq!(check_link(&r, "rec-1"), LinkCheck::Known);
        assert_eq!(check_link(&r, "rec-2"), LinkCheck::Known);
        assert_eq!(check_link(&r, "conv-9"), LinkCheck::Restore("rec-1".into()));
        assert_eq!(check_link(&r, "gone"), LinkCheck::Unknown);
        assert_eq!(check_link(&r, "dup"), LinkCheck::Ambiguous(2));
    }

    /// 이미 대화 id 로 덮인 잇기(옛 판)는 기록 id 로 되돌아가고 탭의 세션 id 도 따라간다 — 한 번만 본다.
    #[test]
    fn an_overwritten_link_is_restored_to_the_record_id() {
        let host = MetaHost::default();
        crate::application::surface_link::link(&host, 451, "conv-9").expect("잇기");
        let mut c = Convo {
            session_id: Some("conv-9".into()),
            ..Default::default()
        };
        repair_link(&host, 451, &mut c, &rows());
        assert_eq!(
            crate::application::surface_link::session_of_surface(&host, 451)
                .ok()
                .as_deref(),
            Some("rec-1")
        );
        assert_eq!(c.session_id.as_deref(), Some("rec-1"));
        // 탭은 기록 id 를 쥐었는데 meta 만 덮인 경우도 같다.
        crate::application::surface_link::link(&host, 452, "conv-9").expect("잇기");
        let mut c = Convo {
            session_id: Some("rec-1".into()),
            ..Default::default()
        };
        repair_link(&host, 452, &mut c, &rows());
        assert_eq!(
            crate::application::surface_link::session_of_surface(&host, 452)
                .ok()
                .as_deref(),
            Some("rec-1")
        );
        // 고친 뒤 같은 세션 id 로는 다시 묻지 않는다.
        crate::application::surface_link::link(&host, 452, "conv-9").expect("잇기");
        repair_link(&host, 452, &mut c, &rows());
        assert_eq!(
            crate::application::surface_link::session_of_surface(&host, 452)
                .ok()
                .as_deref(),
            Some("conv-9"),
            "같은 세션 id 를 매 주기 다시 봤다"
        );
    }

    /// 모르는 값 · 모호한 값은 그대로 둔다(경고만).
    #[test]
    fn an_unknown_or_ambiguous_link_is_left_alone() {
        for v in ["gone", "dup"] {
            let host = MetaHost::default();
            crate::application::surface_link::link(&host, 7, v).expect("잇기");
            let mut c = Convo {
                session_id: Some(v.into()),
                ..Default::default()
            };
            repair_link(&host, 7, &mut c, &rows());
            assert_eq!(
                crate::application::surface_link::session_of_surface(&host, 7)
                    .ok()
                    .as_deref(),
                Some(v)
            );
            assert_eq!(c.session_id.as_deref(), Some(v));
        }
    }
}
