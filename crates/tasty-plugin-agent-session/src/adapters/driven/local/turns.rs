//! 자식 stdout 한 줄을 세션 상태에 반영한다 — 진행 이벤트 변환 · 권한 물음 · 턴 끝을 받을 입력 고르기 · 자식 종료 정리.
//!
//! 짝짓기 규칙은 상위 모듈 문서의 "어느 `result` 가 어느 입력의 답인가" 절에 있다.

use std::time::Instant;

use serde_json::{Value, json};

use super::{FORMAT, Inner, OUTPUT_KEEP, Pending, Waiter, now_unix, reap};
use crate::ports::supervisor::ClientError;

/// 줄 하나를 진행 이벤트로 — 데몬의 `Progress` 와 같은 `phase` 꼬리표.
pub fn progress_of(line: &Value) -> Value {
    let ty = line.get("type").and_then(Value::as_str).unwrap_or_default();
    match ty {
        "system" if line.get("subtype").and_then(Value::as_str) == Some("init") => {
            json!({ "phase": "started" })
        }
        "assistant" => {
            let content = line
                .pointer("/message/content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(t) = content
                .iter()
                .find(|c| c.get("type").and_then(Value::as_str) == Some("tool_use"))
            {
                json!({ "phase": "tool_use", "name": t.get("name").and_then(Value::as_str).unwrap_or_default() })
            } else {
                let text: Vec<&str> = content
                    .iter()
                    .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|c| c.get("text").and_then(Value::as_str))
                    .collect();
                json!({ "phase": "output", "text": text.join("") })
            }
        }
        "stream_event" => match line.pointer("/event/delta") {
            Some(d) if d.get("type").and_then(Value::as_str) == Some("text_delta") => {
                json!({ "phase": "text_delta", "text": d.get("text").and_then(Value::as_str).unwrap_or_default() })
            }
            _ => json!({ "phase": "other", "kind": ty }),
        },
        "result" => {
            let ok = !line
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut ev = json!({ "phase": "finished", "ok": ok });
            if let Some(reason) = finish_reason(line) {
                ev["reason"] = Value::String(reason);
            }
            ev
        }
        _ => json!({ "phase": "other", "kind": ty }),
    }
}

/// 실패한 턴의 사유 — API 오류는 `subtype` 이 `success` 인 채 본문에만 실려 온다(예: `API Error: Request
/// rejected (429) …`). 본문을 실어야 429 재전송·차단기(`retry_429` · `rate_block`)가 알아본다. 그때의
/// `success` 는 실패 사유로 읽히면 거꾸로라 빼고 본문만 싣는다.
fn failed_detail(line: &Value, text: &str) -> String {
    let text = text.trim();
    let reason = finish_reason(line).filter(|r| r != "success");
    match (reason, text.is_empty()) {
        (None, false) => text.to_string(),
        (None, true) => "error".into(),
        (Some(reason), true) => reason,
        (Some(reason), false) => {
            crate::domain::i18n::tn("sup.err.failed_with_output", &[&reason, text])
        }
    }
}

/// `result` 줄의 사유 — `errors[]` 를 ` · ` 로 잇고, 없으면 `subtype`.
fn finish_reason(line: &Value) -> Option<String> {
    let errs: Vec<&str> = line
        .get("errors")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !errs.is_empty() {
        return Some(errs.join(" · "));
    }
    line.get("subtype")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn frame(progress: Value, raw: &str) -> Value {
    json!({ "status": "ok", "result": { "event_raw": {
        "progress": progress, "raw": raw, "format": FORMAT,
    } } })
}

/// 구독자에게 보낸다 — 받을 쪽이 사라진 것은 빼 둔다.
fn publish(inner: &mut Inner, f: &Value) {
    inner.subs.retain(|tx| tx.send(f.clone()).is_ok());
}

/// 자식 stdout 한 줄을 받는다 — pi 세션이면 claude 모양으로 바꿔 넣는다([`super::pi`]).
///
/// pi 의 실행 하나(`agent_settled`)는 그동안 쌓인 입력(`followUp`)을 함께 처리하므로, 바꾼 `result` 는
/// 기다리는 입력 **전부**의 답이다 — claude 의 합쳐진 턴과 같은 규칙(`started` 표지)으로 준다.
pub(super) fn feed(inner: &mut Inner, line: &str) -> bool {
    if inner.rec.kind != "pi" {
        return apply_line(inner, line);
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    let coord = inner.rec.coord.clone();
    let out = inner.pi.line(&v, &coord);
    if let Some(w) = &out.write
        && let Err(e) = super::write_line(inner, w)
    {
        tracing::warn!("pi 에 후속 명령을 못 썼다: {e}");
    }
    let mut changed = false;
    for l in out.lines {
        if l.get("type").and_then(Value::as_str) == Some("result") {
            inner.lifecycle_seen = true;
            for w in inner.waiters.iter_mut() {
                w.started = true;
            }
        }
        changed |= apply_line(inner, &l.to_string());
    }
    changed
}

/// 원문 한 줄을 반영한다 — 구독 프레임을 돌려준다(이미 보냈다).
pub(super) fn apply_line(inner: &mut Inner, line: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    let Some(ty) = v.get("type").and_then(Value::as_str) else {
        return false;
    };
    inner.last_event = Some(Instant::now());
    let mut coord_changed = false;
    match ty {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            inner.turn_active = true;
            if let Some(sid) = v.get("session_id").and_then(Value::as_str)
                && (sid != inner.rec.coord || !inner.rec.has_conversation)
            {
                inner.rec.coord = sid.to_string();
                inner.rec.has_conversation = true;
                coord_changed = true;
            }
        }
        "assistant" => {
            if let Value::Object(_) = &v
                && let Some(Value::String(t)) = progress_of(&v).get("text")
                && !t.is_empty()
            {
                inner.turn_text.push(t.clone());
            }
        }
        "control_request" => {
            if v.pointer("/request/subtype").and_then(Value::as_str) == Some("can_use_tool") {
                let tool = v
                    .pointer("/request/tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let interactive = tool == "AskUserQuestion"
                    || v.pointer("/request/requires_user_interaction")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                inner.prompts.push_back(Pending {
                    request_id: v
                        .get("request_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    kind: if interactive {
                        "question"
                    } else {
                        "permission"
                    },
                    tool,
                    input: v.pointer("/request/input").cloned().unwrap_or(Value::Null),
                    since_unix: now_unix(),
                });
            }
        }
        "system" => {
            inner.tasks.apply(&v);
        }
        "command_lifecycle" => {
            inner.lifecycle_seen = true;
            if v.get("state").and_then(Value::as_str) == Some("started")
                && let Some(tag) = v.get("command_uuid").and_then(Value::as_str)
                && let Some(w) = inner.waiters.iter_mut().find(|w| w.tag == tag)
            {
                w.started = true;
            }
        }
        "control_cancel_request" => {
            let id = v
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            inner.prompts.retain(|p| p.request_id != id);
        }
        "result" => {
            inner.prompts.clear();
            let text = std::mem::take(&mut inner.turn_text).join("\n\n");
            if !text.is_empty() {
                inner.output.push_back(text.clone());
                inner.output_total += 1;
                while inner.output.len() > OUTPUT_KEEP {
                    inner.output.pop_front();
                }
            }
            let outcome = if v.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
                Err(ClientError::Daemon {
                    kind: "precondition".into(),
                    detail: failed_detail(&v, &text),
                })
            } else {
                Ok(text)
            };
            let owners: Vec<Waiter> = if inner.lifecycle_seen {
                let (mine, rest) = std::mem::take(&mut inner.waiters)
                    .into_iter()
                    .partition(|w| w.started);
                inner.waiters = rest;
                mine.into()
            } else {
                inner.waiters.pop_front().into_iter().collect()
            };
            if owners.is_empty() {
                tracing::debug!("받을 입력이 없는 턴 끝 — 자가 턴");
            }
            for w in &owners {
                w.wake(outcome.clone());
            }
            // 자가 턴 도중 쓴 입력은 아직 제 턴을 기다린다.
            inner.turn_active = !inner.waiters.is_empty();
        }
        _ => {}
    }
    let f = frame(progress_of(&v), line.trim());
    publish(inner, &f);
    coord_changed
}

/// 자식이 내려갔다 — 기다리던 턴을 모두 실패로 깨운다.
pub(super) fn settle_exit(inner: &mut Inner, generation: u64) {
    if inner.generation != generation {
        return;
    }
    if let Some(mut l) = inner.live.take() {
        reap(&mut l.child);
    }
    inner.turn_active = false;
    inner.tasks.clear();
    inner.prompts.clear();
    inner.turn_text.clear();
    let tail: Vec<String> = inner.stderr_tail.iter().cloned().collect();
    for w in inner.waiters.drain(..) {
        let mut why = crate::domain::i18n::t("sup.err.stream_ended");
        if !tail.is_empty() {
            why = format!("{why}\n{}", tail.join("\n"));
        }
        w.wake(Err(ClientError::Daemon {
            kind: "failed".into(),
            detail: why,
        }));
    }
}
