//! 대화 로그를 페이지에서 그린다 — 서버는 상태 JSON 만 싣고, 페이지 앱(`assets/app.js`, Preact + htm)이
//! 그 상태로 로그를 그린 뒤 `webview.post_message` 메시지로 상태를 바꿔 다시 그린다.
//!
//! 라이브러리는 내장 파일이다(출처 · 라이선스: `assets/NOTICE.md`). 원격 로드는 막혀 있어 문서 안에 싣는다.

use serde_json::{Value, json};

use crate::domain::convo::Convo;
use crate::domain::convo::Turn;

const PREACT: &str = include_str!("../../../assets/preact.min.js");
const HOOKS: &str = include_str!("../../../assets/preact-hooks.min.js");
const HTM: &str = include_str!("../../../assets/htm.min.js");
const APP: &str = include_str!("../../../assets/app.js");

/// 턴 하나 — 앱이 그리는 데 쓰는 값만.
fn turn(t: &Turn, stopped: bool, overdue: bool) -> Value {
    json!({
        "sent": t.sent,
        "got": t.got,
        "error": t.error,
        "pending": t.pending,
        "at": t.at,
        "stopped": stopped,
        "overdue": overdue,
    })
}

/// 대화 로그의 상태 — 턴 · 이전 대화 · 시스템 줄 · 하위 에이전트 줄.
pub fn log_state(c: &Convo) -> Value {
    json!({
        "turns": c.turns.iter().enumerate()
            .map(|(i, t)| turn(t, c.stopped.contains(&i), c.overdue.contains(&i)))
            .collect::<Vec<_>>(),
        "older": {
            "archived": c.archived,
            "items": c.older.iter().map(|t| turn(t, false, false)).collect::<Vec<_>>(),
        },
        "sys": c.sys.iter().enumerate()
            .map(|(i, n)| crate::domain::sys_note::note_json(i, n))
            .collect::<Vec<_>>(),
    })
}

/// 앱이 쓰는 문구 — 로케일을 따른다.
fn labels() -> Value {
    use crate::domain::i18n::t;
    json!({
        "auto": t("sup.chat.auto_turn"),
        "stopped": t("sup.chat.stopped"),
        "thinking": t("sup.chat.thinking") + "…",
        "overdue": t("sup.chat.overdue") + "…",
        "older": t("sup.chat.older_link"),
        "empty": t("sup.chat.empty"),
        "olderMark": crate::adapters::driving::url::OLDER_MARK,
    })
}

/// 라이브러리 · 상태 · 앱 스크립트. 표(`md_table`) 스크립트 뒤에 싣는다 — 앱이 처음 그릴 때 표를 짓는다.
pub fn script_html(c: &Convo, busy: bool) -> String {
    let state = json!({
        "log": log_state(c),
        "status": crate::adapters::driving::statusline::render(c, busy),
        "ask": crate::application::prompts::card_state(c.needs_input.as_ref()),
        "subs": crate::adapters::driven::subagents::state(c),
        "labels": labels(),
    });
    // `<` 를 전부 `<` 로 — 이 값은 `<script>` 안에 그대로 실린다.
    let boot = state.to_string().replace('<', "\\u003c");
    format!(
        "<style>#subpanelbar{{display:contents}}#statusbar{{flex-shrink:0}}</style><script>{PREACT}\n{HOOKS}\n{HTM}</script>\
         <script>window.__AS_STATE={boot};</script><script>{APP}</script>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convo() -> Convo {
        let mut c = Convo {
            session_id: Some("s1".into()),
            ..Default::default()
        };
        c.turns.push(Turn {
            sent: "hi".into(),
            got: "<b>yo</b>".into(),
            pending: false,
            error: None,
            at: Some(1_730_512_345),
        });
        c.turns.push(Turn {
            sent: "wait".into(),
            got: String::new(),
            pending: true,
            error: None,
            at: None,
        });
        c.overdue.push(1);
        c
    }

    #[test]
    fn the_log_state_carries_each_turn_as_data() {
        let s = log_state(&convo());
        assert_eq!(
            s["turns"][0]["got"], "<b>yo</b>",
            "원문 그대로 — 앱이 글로 넣는다"
        );
        assert_eq!(s["turns"][0]["at"], 1_730_512_345);
        assert_eq!(s["turns"][1]["pending"], true);
        assert_eq!(s["turns"][1]["overdue"], true);
        assert_eq!(s["turns"][0]["overdue"], false);
        assert_eq!(s["older"]["archived"], 0);
    }

    #[test]
    fn the_boot_state_cannot_close_the_script() {
        let mut c = convo();
        c.turns[0].got = "</script><script>alert(1)</script>".into();
        let h = script_html(&c, false);
        assert!(!h.contains("</script><script>alert"));
        assert!(h.contains("window.__AS_STATE="));
    }

    /// 페이지 검사(jsdom)에 쓸 실제 대화 문서를 낸다 — `AS_DUMP=<경로>` 를 줄 때만 쓴다.
    #[test]
    #[ignore]
    fn dump_document_for_page_checks() {
        let Ok(path) = std::env::var("AS_DUMP") else {
            return;
        };
        let mut c = convo();
        c.archived = 3;
        c.older.push(Turn {
            sent: "old".into(),
            got: "| a | b |\n|---|---|\n| 1 | 2 |".into(),
            pending: false,
            error: None,
            at: Some(1_730_000_000),
        });
        crate::domain::sys_note::open(&mut c, "[system] /compact".into());
        c.surface_id = 7;
        c.listed_cwd = Some("/w/proj".into());
        c.usage.model = Some("claude-<x>".into());
        c.usage.five_hour = Some(crate::domain::usage::Window {
            utilization: 0.69,
            resets_at: 1_791_878_400,
        });
        c.needs_input = Some(crate::application::prompts::Pending {
            request_id: "r-1".into(),
            kind: "question".into(),
            tool: String::new(),
            input: json!({ "questions": [{
                "question": "DB?",
                "multiSelect": false,
                "options": [
                    { "label": "Postgres", "description": "<b>rel</b>" },
                    { "label": "Mongo", "description": "doc" }
                ]
            }] }),
        });
        std::fs::write(
            format!("{path}.ask.html"),
            crate::application::prompts::card_html(c.needs_input.as_ref()),
        )
        .expect("카드 쓰기");
        let thread = |d: &str, finished: bool| crate::adapters::driven::subagents::Thread {
            description: d.into(),
            agent_type: "Explore".into(),
            finished,
            prompt: "look <here>".into(),
            answer: if finished {
                "found".into()
            } else {
                String::new()
            },
            tools: 3,
            last_tool: Some("Grep".into()),
            started: Some(100),
            updated: Some(130),
            ..Default::default()
        };
        let mut run = thread("<run>", false);
        run.id = "agent-run".into();
        let step = |id: &str, name: &str, input: Value| {
            json!({"type":"assistant","timestamp":"2026-11-04T01:02:03Z","message":{"content":[
                {"type":"tool_use","id":id,"name":name,"input":input}]}})
        };
        run.steps
            .apply(&step("r1", "Grep", json!({"pattern":"<fn>"})));
        run.steps.apply(&json!({"type":"user","message":{"content":[
            {"type":"tool_result","tool_use_id":"r1","is_error":true}]}}));
        c.subs.view =
            crate::adapters::driven::subagents::View::Loaded(vec![thread("done one", true), run]);
        c.subs.open = true;
        let main_log = format!("{path}.main.jsonl");
        std::fs::write(
            &main_log,
            format!(
                "{}\n{}\n",
                step("m1", "Bash", json!({"command":"ls -la"})),
                json!({"type":"user","message":{"content":[
                    {"type":"tool_result","tool_use_id":"m1","is_error":false}]}})
            ),
        )
        .expect("메인 기록 쓰기");
        c.subs.main = Some(
            crate::adapters::driven::agent_log::MainLog::open(std::path::Path::new(&main_log))
                .expect("메인 기록"),
        );
        c.subs
            .bg
            .seed(&[json!({"id":"b1","type":"local_bash","description":"dev server"})]);
        std::fs::write(
            format!("{path}.subs.html"),
            crate::adapters::driven::subagents::html(&c)
                + &crate::adapters::driven::subagents::panel_html(&c),
        )
        .expect("하위 에이전트 쓰기");
        std::fs::write(&path, crate::adapters::driving::view::render(&c, false))
            .expect("문서 쓰기");
        std::fs::write(
            format!("{path}.status.html"),
            crate::adapters::driving::statusline::render_html(&c, false),
        )
        .expect("상태줄 쓰기");
    }

    /// 라이브러리 → 상태 → 앱 순서 — 앱은 앞의 둘을 읽는다.
    #[test]
    fn the_libraries_load_before_the_state_and_the_app() {
        let h = script_html(&convo(), false);
        let lib = h.find("htm").expect("htm");
        let state = h.find("window.__AS_STATE=").expect("state");
        let app = h.find("window.__asApp").expect("app");
        assert!(lib < state && state < app);
    }
}
