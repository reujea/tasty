//! 세션이 **기다리는 물음**(권한 확인 · 질문)에 대화 탭에서 답한다 — 데몬 protocol 13.
//!
//! # 무엇이 오나 (데몬 consumer-contract §7-B)
//!
//! claude 를 `permission_prompts` 로 띄운 세션은 도구를 쓰기 전에 묻고 답을 기다린다. 데몬은 가장
//! 오래된 물음 하나를 세션 행의 `needs_input` 칸에 싣는다:
//!
//! ```text
//! {request_id, kind: "permission" | "question", tool, input, since_unix}
//! ```
//!
//! 기다리는 동안 `turn_active` 는 `true` 다. 답하거나, agent 가 거두거나, 턴이 끝나면 칸이 빠진다.
//! 답은 `session.respond {id, request_id, decision: allow|deny, updated_input?, message?}` 이다.
//! 질문(AskUserQuestion)의 답은 `allow` 에 물음의 입력 + `answers: {"<질문>": "<라벨>"}` 을 싣는다.
//!
//! # 화면
//!
//! 입력칸 위에 카드 하나(`#ask`). 목록 주기(`activity`)가 칸이 새로 오면 카드를 그리고 attention
//! (`needs_input`)을 켜며 `needs-input` hook 을 낸다 — claude plugin 이 Notification 훅에서 내는 것과
//! 같은 키다. 단추는 다른 조작처럼 fragment 로 plugin 에 온다(`parse_respond`).

use serde_json::{Map, Value, json};

use crate::application::session_surface;
use crate::domain::convo::Convo;
use crate::ports::host::HostCall;
use crate::ports::supervisor::Supervisor;

/// 답 단추가 내는 표식.
pub const RESPOND_MARK: &str = "agent-respond";

/// 대화 탭이 내는 hook 키 — claude plugin 의 Notification 훅과 같은 키다.
pub const NEEDS_INPUT_EVENT: &str = "needs-input";

/// 거부할 때 모델에게 갈 사유.
const DENY_MESSAGE: &str = "Denied from the tasty chat tab";

/// 카드에 보일 입력의 길이 상한 — 도구 입력은 파일 전체일 수 있다.
const INPUT_PREVIEW: usize = 2000;

/// 기다리는 물음 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub request_id: String,
    /// `permission` · `question`.
    pub kind: String,
    pub tool: String,
    pub input: Value,
}

impl Pending {
    /// 세션 행의 `needs_input` 칸 — 없거나 모양이 틀리면 `None`.
    pub fn from_row(row: &Value) -> Option<Self> {
        let n = row.get("needs_input")?;
        Some(Pending {
            request_id: n.get("request_id")?.as_str()?.to_string(),
            kind: n.get("kind")?.as_str()?.to_string(),
            tool: n
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            input: n.get("input").cloned().unwrap_or(Value::Null),
        })
    }
}

/// `session.respond` 에 실을 답.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub request_id: String,
    /// `allow` · `deny`.
    pub decision: String,
    pub updated_input: Option<Value>,
    pub message: Option<String>,
}

/// 단추가 낸 것 — `(request_id, 허용인가, 질문의 답)`.
#[derive(Debug, PartialEq)]
pub struct Clicked {
    pub request_id: String,
    pub allow: bool,
    /// 질문의 답 — `{"<질문>": "<라벨>"}`. 권한 확인이면 `None`.
    pub answers: Option<Map<String, Value>>,
}

/// 단추 URL 을 읽는다 — 우리 표식이 아니면 `None`.
pub fn parse_respond(url: &str) -> Option<Clicked> {
    let q = crate::adapters::driving::url::query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !crate::adapters::driving::url::has_mark(&fields, RESPOND_MARK) {
        return None;
    }
    let get = |k: &str| {
        fields
            .iter()
            .find_map(|kv| kv.strip_prefix(k))
            .map(crate::adapters::driving::url::percent_decode)
    };
    let request_id = get("r=").filter(|r| !r.is_empty())?;
    let allow = get("d=").as_deref() == Some("allow");
    let answers = get("a=")
        .and_then(|a| serde_json::from_str::<Value>(&a).ok())
        .and_then(|v| v.as_object().cloned());
    Some(Clicked {
        request_id,
        allow,
        answers,
    })
}

/// 누른 단추를 데몬에 보낼 답으로 — 질문이면 물음의 입력에 `answers` 를 더한다.
pub fn answer_for(pending: &Pending, clicked: &Clicked) -> Answer {
    if !clicked.allow {
        return Answer {
            request_id: clicked.request_id.clone(),
            decision: "deny".into(),
            updated_input: None,
            message: Some(DENY_MESSAGE.into()),
        };
    }
    let updated_input = clicked.answers.as_ref().map(|a| {
        let mut input = pending.input.clone();
        if let Some(o) = input.as_object_mut() {
            o.insert("answers".into(), Value::Object(a.clone()));
            input
        } else {
            json!({ "answers": a })
        }
    });
    Answer {
        request_id: clicked.request_id.clone(),
        decision: "allow".into(),
        updated_input,
        message: None,
    }
}

/// 목록 행의 물음을 탭에 반영한다 — 새 물음이면 카드 · attention · hook, 사라졌으면 거둔다.
pub fn note<H: HostCall>(host: &H, sid: u32, convo: &mut Convo, row: &Value) {
    let next = Pending::from_row(row);
    let same =
        next.as_ref().map(|p| &p.request_id) == convo.needs_input.as_ref().map(|p| &p.request_id);
    if same {
        return;
    }
    let had = convo.needs_input.is_some();
    convo.needs_input = next;
    session_surface::patch(host, sid, &ask_msg(convo.needs_input.as_ref()));
    match convo.needs_input.as_ref() {
        Some(_) => {
            if let Err(e) = host.call(
                "surface.completion",
                json!({ "surface_id": sid, "kind": "needs_input" }),
            ) {
                tracing::warn!("[agent_session] surface={sid}: 입력 대기 표시를 못 켰다: {e}");
            }
            let claude = convo.agent_kind.as_deref().is_none_or(|k| k == "claude");
            if claude
                && let Err(e) =
                    crate::application::surface_link::fire_hook(host, sid, NEEDS_INPUT_EVENT)
            {
                tracing::warn!(
                    "[agent_session] surface={sid}: hook '{NEEDS_INPUT_EVENT}' 발사 실패: {e}"
                );
            }
        }
        None if had => {
            if let Err(e) = crate::application::surface_link::clear_reclaimed_mark(host, sid) {
                tracing::debug!("[agent_session] surface={sid}: 입력 대기 표시를 못 거뒀다: {e}");
            }
        }
        None => {}
    }
}

/// 단추가 눌렸다 — 답을 보내고 카드를 거둔다. 우리 표식이 아니면 `false`.
///
/// 지금 기다리는 물음이 아닌 단추(이미 답했거나 거둬진 물음의 남은 카드)는 보내지 않는다 — 데몬도
/// `invalid_state` 로 거절하지만, 보내기 전에 걸러 사용자에게 엉뚱한 오류를 안 보인다.
pub fn handle_click<H: HostCall>(
    host: &H,
    sup: &impl Supervisor,
    sid: u32,
    convo: &mut Convo,
    url: &str,
) -> bool {
    let Some(clicked) = parse_respond(url) else {
        return false;
    };
    let (Some(session), Some(pending)) = (convo.session_id.clone(), convo.needs_input.clone())
    else {
        return true;
    };
    if pending.request_id != clicked.request_id {
        return true;
    }
    match sup.respond(&session, &answer_for(&pending, &clicked)) {
        Ok(()) => {
            convo.needs_input = None;
            session_surface::patch(host, sid, &ask_msg(None));
            if let Err(e) = crate::application::surface_link::clear_reclaimed_mark(host, sid) {
                tracing::debug!("[agent_session] surface={sid}: 입력 대기 표시를 못 거뒀다: {e}");
            }
        }
        Err(e) => {
            tracing::warn!("[agent_session] surface={sid}: 답을 못 보냈다: {e}");
            session_surface::patch(host, sid, &ask_error_msg(&e.to_string()));
        }
    }
    true
}

/// 카드 모양과 카드 단추가 부르는 함수 — 문서 끝에 한 번 싣는다.
///
/// 카드는 뒤에서 `outerHTML` 로 갈아 끼우므로 그 안의 `<script>` 는 돌지 않는다. 그래서 단추가 부를
/// 함수는 문서와 함께 한 번 싣고, 카드는 `onclick` 으로 그것을 부른다. 입력칸 스크립트보다 **뒤에**
/// 둔다 — 그 스크립트를 문서의 첫 `<script>` 로 읽는 자리가 있다.
pub fn script_html() -> String {
    format!(
        r#"<style>
 #ask{{margin:0 16px 8px;padding:10px 12px;border:1px solid #d8a33c;border-radius:4px;background:#1c1f26}}
 #ask[hidden]{{display:none}}
 #ask .t{{color:#d8a33c;margin-bottom:6px}}
 #ask pre{{max-height:12em;overflow:auto;white-space:pre-wrap;color:#9aa3b2;margin:4px 0 8px}}
 #ask .q{{margin:6px 0}} #ask .q b{{color:#c8ccd4;font-weight:400}}
 #ask label{{display:block;margin:2px 0 2px 1ch;cursor:pointer}} #ask label span{{color:#6b7280}}
 #ask .acts{{display:flex;gap:8px;margin-top:8px}}
 #ask .acts a{{padding:2px 10px;border:1px solid #3a4150;border-radius:3px;color:#8fd5a8;text-decoration:none}}
 #ask .acts a.no{{color:#e06c75}} #ask .err{{color:#e06c75;margin-top:6px}}
</style>
<script>
window.tastyAsk=function(id,allow){{
  var a=null, card=document.getElementById('ask');
  if(allow&&card&&card.querySelector('.q')){{
    a={{}};
    card.querySelectorAll('.q').forEach(function(q){{
      var picked=[].map.call(q.querySelectorAll('input:checked'),function(i){{return i.value;}});
      if(picked.length)a[q.getAttribute('data-q')]=picked.join(', ');
    }});
  }}
  location.hash='{MARK}='+Date.now()+'&r='+encodeURIComponent(id)+'&d='+(allow?'allow':'deny')
    +(a?'&a='+encodeURIComponent(JSON.stringify(a)):'');
  return false;
}};
</script>"#,
        MARK = RESPOND_MARK,
    )
}

/// 질문 카드의 화면 값 — 페이지 앱의 `AskCard` 가 그린다. 묻는 것이 없으면 `null`(카드 숨김).
pub fn card_state(pending: Option<&Pending>) -> Value {
    let Some(p) = pending else {
        return Value::Null;
    };
    let questions = p
        .input
        .get("questions")
        .and_then(Value::as_array)
        .filter(|_| p.kind == "question");
    let deny = crate::domain::i18n::t("sup.ask.deny");
    match questions {
        Some(qs) => {
            let qs: Vec<Value> = qs
                .iter()
                .map(|q| {
                    let options: Vec<Value> = q
                        .get("options")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|o| {
                            serde_json::json!({
                                "label": o.get("label").and_then(Value::as_str).unwrap_or_default(),
                                "desc": o.get("description").and_then(Value::as_str).unwrap_or_default(),
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "text": q.get("question").and_then(Value::as_str).unwrap_or_default(),
                        "multi": q.get("multiSelect").and_then(Value::as_bool) == Some(true),
                        "options": options,
                    })
                })
                .collect();
            serde_json::json!({
                "id": p.request_id,
                "title": crate::domain::i18n::t("sup.ask.question_title"),
                "questions": qs,
                "yes": crate::domain::i18n::t("sup.ask.answer"),
                "no": deny,
            })
        }
        None => serde_json::json!({
            "id": p.request_id,
            "title": crate::domain::i18n::t1("sup.ask.permission_title", &p.tool),
            "preview": preview(&p.input),
            "yes": crate::domain::i18n::t("sup.ask.allow"),
            "no": deny,
        }),
    }
}

/// 페이지 앱의 `AskCard` 와 같은 모양으로 그린다 — 시험 전용. 페이지 검사(jsdom)가 둘이 같은지 본다.
#[cfg(test)]
pub fn card_html(pending: Option<&Pending>) -> String {
    let v = card_state(pending);
    if v.is_null() {
        return r#"<div id="ask" hidden=""></div>"#.to_string();
    }
    let s = |x: &Value| crate::adapters::driving::html::escape(x.as_str().unwrap_or_default());
    let mut h = format!(r#"<div id="ask"><div class="t">{}</div>"#, s(&v["title"]));
    match v["questions"].as_array() {
        Some(qs) => {
            for (qi, q) in qs.iter().enumerate() {
                h.push_str(&format!(
                    r#"<div class="q" data-q="{}"><b>{}</b>"#,
                    s(&q["text"]),
                    s(&q["text"])
                ));
                for o in q["options"].as_array().into_iter().flatten() {
                    h.push_str(&format!(
                        r#"<label><input type="{}" name="q{qi}" value="{}"> {} <span>{}</span></label>"#,
                        if q["multi"] == true { "checkbox" } else { "radio" },
                        s(&o["label"]),
                        s(&o["label"]),
                        s(&o["desc"])
                    ));
                }
                h.push_str("</div>");
            }
        }
        None => h.push_str(&format!("<pre>{}</pre>", s(&v["preview"]))),
    }
    h.push_str(&format!(
        r##"<div class="acts"><a href="#">{}</a><a class="no" href="#">{}</a></div></div>"##,
        s(&v["yes"]),
        s(&v["no"])
    ));
    h
}

fn preview(input: &Value) -> String {
    let s = serde_json::to_string_pretty(input).unwrap_or_default();
    match s.char_indices().nth(INPUT_PREVIEW) {
        Some((at, _)) => format!("{}…", &s[..at]),
        None => s,
    }
}

/// 카드만 갈아 끼우는 스크립트.
pub fn ask_msg(pending: Option<&Pending>) -> Value {
    serde_json::json!({ "type": "ask.set", "ask": card_state(pending) })
}

/// 보내기에 실패했다 — 카드는 두고 사유를 붙인다.
fn ask_error_msg(why: &str) -> Value {
    serde_json::json!({ "type": "ask.error", "text": crate::domain::i18n::t1("sup.ask.failed", why) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permission() -> Value {
        json!({ "id": "s1", "needs_input": {
            "request_id": "r-1", "kind": "permission", "tool": "Bash",
            "input": { "command": "rm -rf build" }, "since_unix": 1 } })
    }

    fn question() -> Value {
        json!({ "id": "s1", "needs_input": {
            "request_id": "r-2", "kind": "question", "tool": "AskUserQuestion",
            "input": { "questions": [
                { "question": "Which DB?", "header": "DB", "multiSelect": false,
                  "options": [ { "label": "Postgres", "description": "relational" },
                               { "label": "<b>x</b>", "description": "" } ] } ] } } })
    }

    #[test]
    fn the_row_field_becomes_a_pending_prompt() {
        let p = Pending::from_row(&permission()).expect("칸을 못 읽었다");
        assert_eq!(
            (p.request_id.as_str(), p.kind.as_str(), p.tool.as_str()),
            ("r-1", "permission", "Bash")
        );
        assert_eq!(Pending::from_row(&json!({ "id": "s1" })), None);
    }

    /// 단추 URL — 허용 · 거부 · 질문의 답을 읽는다. 표식이 없으면 남의 링크다.
    #[test]
    fn a_button_url_is_read_back() {
        let c = parse_respond("about:blank#agent-respond=17&r=r-1&d=allow").expect("못 읽었다");
        assert_eq!(
            (c.request_id.as_str(), c.allow, c.answers.is_none()),
            ("r-1", true, true)
        );
        let a = r#"{"Which DB?":"Postgres"}"#;
        let url = format!(
            "about:blank#agent-respond=18&r=r-2&d=allow&a={}",
            urlencode(a)
        );
        let c = parse_respond(&url).expect("못 읽었다");
        assert_eq!(c.answers.unwrap()["Which DB?"], "Postgres");
        assert!(
            !parse_respond("about:blank#agent-respond=19&r=r-1&d=deny")
                .unwrap()
                .allow
        );
        assert_eq!(parse_respond("about:blank#r=r-1&d=allow"), None);
    }

    fn urlencode(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    /// 질문의 답은 물음의 입력에 `answers` 를 더해 허용한다 — 계약 §7-B 그대로. 거부는 사유를 싣는다.
    #[test]
    fn answers_go_back_inside_the_original_input() {
        let p = Pending::from_row(&question()).unwrap();
        let mut a = Map::new();
        a.insert("Which DB?".into(), "Postgres".into());
        let ans = answer_for(
            &p,
            &Clicked {
                request_id: "r-2".into(),
                allow: true,
                answers: Some(a),
            },
        );
        assert_eq!(ans.decision, "allow");
        let u = ans.updated_input.expect("답이 없다");
        assert_eq!(u["answers"]["Which DB?"], "Postgres");
        assert!(u["questions"].is_array(), "물음의 입력을 버렸다");
        let perm = Pending::from_row(&permission()).unwrap();
        let ok = answer_for(
            &perm,
            &Clicked {
                request_id: "r-1".into(),
                allow: true,
                answers: None,
            },
        );
        assert_eq!(
            (ok.decision.as_str(), ok.updated_input.is_none()),
            ("allow", true)
        );
        let no = answer_for(
            &perm,
            &Clicked {
                request_id: "r-1".into(),
                allow: false,
                answers: None,
            },
        );
        assert_eq!(no.decision, "deny");
        assert!(no.message.is_some());
    }

    /// 카드 — 권한이면 도구와 입력, 질문이면 선택지. 값은 모두 escape 된다.
    #[test]
    fn the_card_shows_what_is_asked() {
        let h = card_html(Pending::from_row(&permission()).as_ref());
        assert_eq!(
            card_state(Pending::from_row(&permission()).as_ref())["id"],
            "r-1"
        );
        assert!(include_str!("../../assets/app.js").contains("window.tastyAsk(ask.id, allow)"));
        assert!(h.contains("Bash") && h.contains("rm -rf build"), "{h}");
        let h = card_html(Pending::from_row(&question()).as_ref());
        assert!(
            h.contains(r#"type="radio""#) && h.contains("Postgres"),
            "{h}"
        );
        assert!(!h.contains("<b>x</b>"), "라벨이 escape 안 됐다: {h}");
        assert_eq!(card_html(None), r#"<div id="ask" hidden=""></div>"#);
        assert!(card_state(None).is_null());
        let m = ask_msg(Pending::from_row(&question()).as_ref());
        assert_eq!(m["type"], "ask.set");
        assert_eq!(
            m["ask"],
            card_state(Pending::from_row(&question()).as_ref())
        );
    }

    /// 새 물음이면 카드 · attention · hook, 같은 물음이면 아무것도, 사라지면 거둔다.
    #[test]
    fn a_new_prompt_is_raised_once_and_cleared_when_gone() {
        let host = crate::application::surface_link::tests_support::FakeHost::linked(7, "s1");
        let mut c = Convo {
            session_id: Some("s1".into()),
            ..Default::default()
        };
        note(&host, 7, &mut c, &permission());
        assert_eq!(
            c.needs_input.as_ref().map(|p| p.request_id.as_str()),
            Some("r-1")
        );
        assert_eq!(
            crate::application::surface_link::attention_of(&host, 7)
                .unwrap()
                .as_deref(),
            Some("needs_input")
        );
        note(&host, 7, &mut c, &json!({ "id": "s1" }));
        assert_eq!(c.needs_input, None);
        assert_eq!(
            crate::application::surface_link::attention_of(&host, 7).unwrap(),
            None,
            "물음이 끝났는데 표시가 남았다"
        );
    }

    /// 대화 탭의 조작 갈래가 답 단추를 이 모듈로 보낸다 — 빠지면 단추가 아무 일도 안 한다.
    #[test]
    fn the_tab_routes_answer_buttons_here() {
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn tab_control")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .expect("tab_control");
        assert!(
            body.contains("prompts::handle_click("),
            "답 단추 갈래가 없다"
        );
    }

    /// 단추 — 지금 기다리는 물음에만 답을 보내고 카드를 거둔다.
    #[test]
    fn a_click_answers_only_the_pending_prompt() {
        let host = crate::application::surface_link::tests_support::FakeHost::linked(7, "s1");
        let sup = crate::ports::supervisor::fake::FakeSupervisor::new();
        let mut c = Convo {
            session_id: Some("s1".into()),
            ..Default::default()
        };
        note(&host, 7, &mut c, &permission());
        assert!(handle_click(
            &host,
            &sup,
            7,
            &mut c,
            "about:blank#agent-respond=1&r=old&d=allow"
        ));
        assert!(
            c.needs_input.is_some(),
            "지난 물음의 단추로 지금 물음을 지웠다"
        );
        assert!(handle_click(
            &host,
            &sup,
            7,
            &mut c,
            "about:blank#agent-respond=2&r=r-1&d=allow"
        ));
        assert_eq!(c.needs_input, None);
        assert!(
            sup.calls
                .borrow()
                .iter()
                .any(|c| c.starts_with("respond:s1:r-1:allow")),
            "{:?}",
            sup.calls.borrow()
        );
        assert!(!handle_click(
            &host,
            &sup,
            7,
            &mut c,
            "about:blank#agent-send=3&t=x"
        ));
    }
}
