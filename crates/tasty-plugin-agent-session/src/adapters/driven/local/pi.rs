//! pi(`pi --mode rpc`) 의 줄을 claude stream-json 모양으로 바꾼다.
//!
//! 대화 탭 · 상태줄 · 현행화 · 출력 기록은 claude stream-json 줄을 읽는다. pi 세션도 같은 길을 타게
//! 실행기가 pi 의 줄을 여기서 바꿔 [`super::turns::apply_line`] 에 넣는다. 탭 쪽은 종류를 몰라도 된다.
//!
//! pi 0.85.1 rpc(pi 패키지의 rpc 문서 · 실측 2026-10-30): 명령은 stdin 에 `{"type":"prompt","message":…}`,
//! 응답은 `{"type":"response","command":…,"success":…}`, 진행은 이벤트 줄이다.
//!
//! | pi 줄 | 바꾼 줄 |
//! |---|---|
//! | `agent_start`(한 실행의 첫 번째) | `system/init`(`session_id` = 좌표) |
//! | `message_update` `text_delta` | `stream_event` `text_delta` |
//! | `message_end`(assistant) | `assistant` — `text` · `toolCall`→`tool_use` · `thinking` |
//! | `tool_execution_end` | `user` `tool_result` |
//! | `compaction_start` / `compaction_end` | `system/status` `compacting` / `null` |
//! | `agent_settled` | `result` — 실행 중 마지막 오류가 남았으면 `is_error` |
//! | `response` 실패(`prompt` · `compact`) | `result` `is_error` |
//! | `response` `compact` 성공 | `result` |
//! | `response` `new_session` 성공 | (stdin 에 `get_state` 를 쓴다) |
//! | `response` `get_state`(비우기 뒤) | `system/init`(새 좌표) + `result` |
//!
//! 실측에서 `stopReason:"error"` 인 `message_end` 뒤로 `auto_retry_start` 가 이어지고 마지막에
//! `agent_settled` 하나가 왔다(한도 429 세 번). 재시도가 성공하면 오류가 지워진다.
//!
//! pi 에는 `command_lifecycle` 이 없고 스스로 턴을 열지도 않는다. 실행 하나(`agent_settled`)가 그
//! 사이 쌓인 입력(`followUp`)을 함께 처리하므로 실행기는 `result` 를 기다리는 입력 전부에 준다.

use serde_json::{Value, json};

/// 실행기가 띄우는 pi 의 고정 인자.
pub const RPC_ARGS: &[&str] = &["--mode", "rpc"];

/// 바꾸는 쪽의 상태 — 세션 하나에 하나.
#[derive(Debug, Default)]
pub struct Translate {
    /// `agent_start` 를 봤고 `agent_settled` 를 아직 안 봤다.
    in_run: bool,
    /// 이 실행에서 마지막으로 난 오류 — 재시도가 성공하면 지운다.
    last_error: Option<String>,
    /// 마지막 assistant 메시지의 사용량(claude 모양).
    usage: Option<Value>,
    /// `/clear` 로 새 세션을 열었다 — `get_state` 답에서 새 좌표를 알린다.
    clearing: bool,
}

/// 바꾼 결과 — 반영할 줄과 pi stdin 에 쓸 줄.
#[derive(Debug, Default, PartialEq)]
pub struct Out {
    pub lines: Vec<Value>,
    pub write: Option<Value>,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

/// pi 의 사용량을 claude `usage` 칸 이름으로.
fn usage_of(u: &Value) -> Value {
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    json!({
        "input_tokens": n("input"),
        "output_tokens": n("output"),
        "cache_read_input_tokens": n("cacheRead"),
        "cache_creation_input_tokens": n("cacheWrite"),
    })
}

/// assistant 메시지 블록을 claude 블록으로 — 모르는 블록은 뺀다.
fn content_of(m: &Value) -> Vec<Value> {
    m.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| match s(c, "type") {
            "text" => Some(json!({ "type": "text", "text": s(c, "text") })),
            "thinking" => Some(json!({ "type": "thinking", "thinking": s(c, "thinking") })),
            "toolCall" => Some(json!({
                "type": "tool_use",
                "id": s(c, "id"),
                "name": s(c, "name"),
                "input": c.get("arguments").cloned().unwrap_or(Value::Null),
            })),
            _ => None,
        })
        .collect()
}

fn text_of_result(r: &Value) -> String {
    r.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| s(c, "type") == "text")
        .map(|c| s(c, "text"))
        .collect::<Vec<_>>()
        .join("")
}

fn result_line(error: Option<&str>, usage: Option<&Value>) -> Value {
    let mut r = json!({
        "type": "result",
        "subtype": if error.is_some() { "error_during_execution" } else { "success" },
        "is_error": error.is_some(),
    });
    if let Some(e) = error {
        r["errors"] = json!([e]);
    }
    if let Some(u) = usage {
        r["usage"] = u.clone();
    }
    r
}

fn init_line(session_id: &str) -> Value {
    json!({ "type": "system", "subtype": "init", "session_id": session_id })
}

impl Translate {
    /// pi 줄 하나를 바꾼다. `coord` 는 지금 좌표(pi 의 세션 id)다.
    pub fn line(&mut self, v: &Value, coord: &str) -> Out {
        let mut out = Out::default();
        match s(v, "type") {
            "agent_start" if !self.in_run => {
                self.in_run = true;
                self.last_error = None;
                out.lines.push(init_line(coord));
            }
            "message_update" => {
                let e = v.get("assistantMessageEvent").unwrap_or(&Value::Null);
                if s(e, "type") == "text_delta" {
                    out.lines.push(json!({ "type": "stream_event", "event": {
                        "type": "content_block_delta",
                        "index": e.get("contentIndex").cloned().unwrap_or(json!(0)),
                        "delta": { "type": "text_delta", "text": s(e, "delta") },
                    } }));
                }
            }
            "message_end" => {
                let m = v.get("message").unwrap_or(&Value::Null);
                if s(m, "role") == "assistant" {
                    if s(m, "stopReason") == "error" {
                        self.last_error = Some(s(m, "errorMessage").to_string());
                    } else {
                        self.last_error = None;
                    }
                    if let Some(u) = m.get("usage") {
                        self.usage = Some(usage_of(u));
                    }
                    let content = content_of(m);
                    if !content.is_empty() {
                        out.lines.push(json!({ "type": "assistant", "message": {
                            "role": "assistant",
                            "model": s(m, "model"),
                            "content": content,
                            "usage": self.usage.clone().unwrap_or(Value::Null),
                        } }));
                    }
                }
            }
            "tool_execution_end" => {
                let r = v.get("result").unwrap_or(&Value::Null);
                out.lines.push(
                    json!({ "type": "user", "message": { "role": "user", "content": [{
                    "type": "tool_result",
                    "tool_use_id": s(v, "toolCallId"),
                    "content": text_of_result(r),
                    "is_error": v.get("isError").and_then(Value::as_bool).unwrap_or(false),
                }] } }),
                );
            }
            "auto_retry_end" if v.get("success").and_then(Value::as_bool) == Some(false) => {
                self.last_error = Some(s(v, "finalError").to_string());
            }
            "compaction_start" => {
                out.lines
                    .push(json!({ "type": "system", "subtype": "status", "status": "compacting" }));
            }
            "compaction_end" => {
                out.lines
                    .push(json!({ "type": "system", "subtype": "status", "status": Value::Null }));
            }
            "agent_settled" => {
                self.in_run = false;
                let e = self.last_error.take();
                out.lines
                    .push(result_line(e.as_deref(), self.usage.as_ref()));
            }
            "response" => self.response(v, &mut out),
            _ => {}
        }
        out
    }

    fn response(&mut self, v: &Value, out: &mut Out) {
        let ok = v.get("success").and_then(Value::as_bool).unwrap_or(false);
        let why = || {
            let e = s(v, "error");
            if e.is_empty() { "rejected" } else { e }.to_string()
        };
        match s(v, "command") {
            "prompt" if !ok => out.lines.push(result_line(Some(&why()), None)),
            "compact" if ok => out.lines.push(result_line(None, None)),
            "compact" => out.lines.push(result_line(Some(&why()), None)),
            "new_session" if ok => {
                self.clearing = true;
                out.write = Some(json!({ "type": "get_state", "id": "clear-state" }));
            }
            "new_session" => out.lines.push(result_line(Some(&why()), None)),
            "get_state" if self.clearing => {
                self.clearing = false;
                let id = v.pointer("/data/sessionId").and_then(Value::as_str);
                if let Some(id) = id {
                    out.lines.push(init_line(id));
                }
                out.lines.push(result_line(None, None));
            }
            _ => {}
        }
    }
}

/// 입력 한 줄을 pi 명령으로 — `/clear` 는 새 세션, `/compact` 는 압축, 그 밖은 `prompt` 다.
///
/// `prompt` 는 늘 `followUp` 으로 보낸다 — 실행 중에 이 값이 없으면 pi 가 거절하고, 쉬는 중에는 그냥
/// 실행을 연다(실측 2026-10-30).
pub fn input_line(text: &str, tag: &str) -> Value {
    let t = text.trim();
    if t == "/clear" {
        return json!({ "type": "new_session", "id": tag });
    }
    if t == "/compact" || t.starts_with("/compact ") {
        let mut v = json!({ "type": "compact", "id": tag });
        let rest = t["/compact".len()..].trim();
        if !rest.is_empty() {
            v["customInstructions"] = Value::String(rest.to_string());
        }
        return v;
    }
    json!({ "type": "prompt", "id": tag, "message": text, "streamingBehavior": "followUp" })
}

/// 턴 취소.
pub fn abort_line() -> Value {
    json!({ "type": "abort" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(t: &mut Translate, lines: &[Value]) -> Vec<Value> {
        lines.iter().flat_map(|l| t.line(l, "c1").lines).collect()
    }

    /// 문서의 성공 흐름 — 첫 실행 시작이 init, 글자 조각, 도구 호출과 결과, 끝이 result.
    #[test]
    fn a_tool_turn_becomes_claude_shaped_lines() {
        let mut t = Translate::default();
        let got = feed(
            &mut t,
            &[
                json!({"type":"response","command":"prompt","success":true}),
                json!({"type":"agent_start"}),
                json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hi"}}),
                json!({"type":"message_end","message":{"role":"assistant","model":"m","stopReason":"toolUse",
                    "usage":{"input":10,"output":2,"cacheRead":5,"cacheWrite":1},
                    "content":[{"type":"text","text":"Hi"},{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"ls"}}]}}),
                json!({"type":"tool_execution_end","toolCallId":"t1","toolName":"bash",
                    "result":{"content":[{"type":"text","text":"a\nb"}]},"isError":false}),
                json!({"type":"agent_start"}),
                json!({"type":"agent_settled"}),
            ],
        );
        let types: Vec<&str> = got.iter().map(|l| l["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            vec!["system", "stream_event", "assistant", "user", "result"]
        );
        assert_eq!(got[0]["session_id"], "c1");
        assert_eq!(got[1]["event"]["delta"]["text"], "Hi");
        assert_eq!(got[2]["message"]["content"][1]["type"], "tool_use");
        assert_eq!(got[2]["message"]["content"][1]["input"]["command"], "ls");
        assert_eq!(got[3]["message"]["content"][0]["content"], "a\nb");
        assert_eq!(got[4]["is_error"], false);
        assert_eq!(got[4]["usage"]["cache_read_input_tokens"], 5);
    }

    /// 실측(429 세 번) 흐름 — 빈 오류 메시지는 화면에 안 싣고, 끝의 result 가 오류를 싣는다.
    #[test]
    fn retries_that_end_in_failure_close_the_run_as_an_error() {
        let mut t = Translate::default();
        let err = json!({"type":"message_end","message":{"role":"assistant","content":[],
            "stopReason":"error","errorMessage":"429 rate"}});
        let got = feed(
            &mut t,
            &[
                json!({"type":"agent_start"}),
                err.clone(),
                json!({"type":"agent_end"}),
                json!({"type":"auto_retry_start","attempt":1}),
                json!({"type":"agent_start"}),
                err,
                json!({"type":"auto_retry_end","success":false,"finalError":"429 final"}),
                json!({"type":"agent_settled"}),
            ],
        );
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[1]["is_error"], true);
        assert_eq!(got[1]["errors"][0], "429 final");
    }

    /// 재시도가 성공하면 앞선 오류를 지운다.
    #[test]
    fn a_retry_that_succeeds_clears_the_error() {
        let mut t = Translate::default();
        let got = feed(
            &mut t,
            &[
                json!({"type":"agent_start"}),
                json!({"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"error","errorMessage":"x"}}),
                json!({"type":"message_end","message":{"role":"assistant","stopReason":"stop","content":[{"type":"text","text":"ok"}]}}),
                json!({"type":"agent_settled"}),
            ],
        );
        assert_eq!(got.last().unwrap()["is_error"], false);
    }

    /// `/clear` → `new_session` → (실행기가 `get_state` 를 쓴다) → 새 좌표의 init 과 result.
    #[test]
    fn clearing_asks_for_the_new_session_id_and_announces_it() {
        assert_eq!(input_line("/clear", "u1")["type"], "new_session");
        let mut t = Translate::default();
        let o = t.line(
            &json!({"type":"response","command":"new_session","success":true,"data":{"cancelled":false}}),
            "c1",
        );
        assert!(o.lines.is_empty());
        assert_eq!(o.write.unwrap()["type"], "get_state");
        let o = t.line(
            &json!({"type":"response","command":"get_state","success":true,"data":{"sessionId":"c2"}}),
            "c1",
        );
        assert_eq!(o.lines[0]["session_id"], "c2");
        assert_eq!(o.lines[1]["type"], "result");
        // 비우기와 무관한 상태 답은 아무것도 안 낸다.
        assert!(t
            .line(&json!({"type":"response","command":"get_state","success":true,"data":{"sessionId":"c3"}}), "c2")
            .lines
            .is_empty());
    }

    #[test]
    fn inputs_map_to_pi_commands() {
        let p = input_line("hello", "u1");
        assert_eq!(p["type"], "prompt");
        assert_eq!(p["streamingBehavior"], "followUp");
        assert_eq!(p["id"], "u1");
        let c = input_line("/compact keep code", "u2");
        assert_eq!(c["type"], "compact");
        assert_eq!(c["customInstructions"], "keep code");
        assert!(
            input_line("/compact", "u3")
                .get("customInstructions")
                .is_none()
        );
        assert_eq!(input_line("/compactor", "u4")["type"], "prompt");
        let rejected = Translate::default().line(
            &json!({"type":"response","command":"prompt","success":false,"error":"busy"}),
            "c1",
        );
        assert_eq!(rejected.lines[0]["errors"][0], "busy");
    }
}
