//! 대화 탭의 **Claude Code 슬래시 커맨드** — 입력칸에 `/` 를 치면 고를 수 있는 목록과, 커맨드가
//! 남기는 빈 답을 사람이 읽을 결과로 바꾸는 자리.
//!
//! # 무엇을 싣나
//!
//! 데몬이 띄우는 claude(`-p --input-format stream-json`)에 **텍스트로 보내 실제로 도는 것만** 싣는다.
//! 실측(2026-10-21, claude 2.1.304, 데몬과 같은 플래그): `/add-dir` 를 뺀 아래 일곱은 모두 `result` 한 줄로 끝나고
//! 모델 턴을 쓰지 않는다(`num_turns` 0) — `/compact` 만 요약에 모델을 쓴다. `/add-dir` 는 같은 자리에서
//! "isn't available in this environment" 로 답하므로 탭이 가로채 데몬의 세션별 추가 폴더로 보낸다
//! (`add_dir` 모듈 · 상대 protocol 14). 재지 않은 내장 커맨드(`/init` · `/review` 등)도 싣지 않는다 — 목록에 있으면
//! 되는 줄 안다.
//!
//! 그 뒤에 그 세션의 **스킬**(사용자 · 플러그인 · 프로젝트, `skills` 모듈)을 붙인다. 스킬은 텍스트 `/<이름>` 으로
//! 보내면 실제로 돈다(실측 2026-11-04, claude 2.1.311 — `num_turns` 1). 고정 커맨드와 이름이 겹치면 고정이 이긴다.
//! pi 세션에는 스킬을 붙이지 않는다.
//!
//! 보내는 길은 보통 말과 같다(`send_input`). `/clear` 만 탭 쪽 정리가 필요해 `clear` 모듈이 가로챈다.
use serde_json::{Value, json};

/// 목록의 한 줄 — 이름과, 뒤에 인자를 받는지.
struct Cmd {
    name: &'static str,
    takes_arg: bool,
}

const COMMANDS: &[Cmd] = &[
    Cmd {
        name: "/add-dir",
        takes_arg: true,
    },
    Cmd {
        name: "/clear",
        takes_arg: false,
    },
    Cmd {
        name: "/compact",
        takes_arg: false,
    },
    Cmd {
        name: "/context",
        takes_arg: false,
    },
    Cmd {
        name: "/cost",
        takes_arg: false,
    },
    Cmd {
        name: "/model",
        takes_arg: true,
    },
    Cmd {
        name: "/reload-plugins",
        takes_arg: false,
    },
    Cmd {
        name: "/reload-skills",
        takes_arg: false,
    },
];

/// 커맨드 설명 — 키를 `i18n::t("…")` 로 바로 부른다(키 존재 가드가 이 꼴만 뽑는다).
fn describe(name: &str) -> String {
    match name {
        "/add-dir" => crate::domain::i18n::t("sup.cmd.add_dir"),
        "/clear" => crate::domain::i18n::t("sup.cmd.clear"),
        "/compact" => crate::domain::i18n::t("sup.cmd.compact"),
        "/context" => crate::domain::i18n::t("sup.cmd.context"),
        "/cost" => crate::domain::i18n::t("sup.cmd.cost"),
        "/model" => crate::domain::i18n::t("sup.cmd.model"),
        "/reload-plugins" => crate::domain::i18n::t("sup.cmd.reload_plugins"),
        "/reload-skills" => crate::domain::i18n::t("sup.cmd.reload_skills"),
        _ => String::new(),
    }
}

/// pi 세션에 보이는 커맨드 — 실행기가 pi 명령으로 바꾸는 것(`/clear` → `new_session`, `/compact` →
/// `compact`)과 탭이 가로채는 `/add-dir` 뿐이다. 나머지는 claude 의 커맨드라 pi 에는 그냥 글로 간다.
const PI_COMMANDS: &[&str] = &["/add-dir", "/clear", "/compact"];

/// 페이지에 실을 목록 — `[{n, d, a, k?}]`(이름 · 설명 · 인자를 받나 · 스킬이면 `k:"skill"`).
/// 종류를 아직 모르면 claude 목록이다.
pub fn menu_items(
    agent_kind: Option<&str>,
    skills: &[crate::adapters::driven::skills::Skill],
) -> Value {
    let pi = agent_kind == Some("pi");
    let mut items: Vec<Value> = COMMANDS
        .iter()
        .filter(|c| !pi || PI_COMMANDS.contains(&c.name))
        .map(|c| json!({ "n": c.name, "d": describe(c.name), "a": c.takes_arg }))
        .collect();
    if !pi {
        items.extend(
            skills
                .iter()
                .map(|s| format!("/{}", s.name))
                .zip(skills)
                .filter(|(n, _)| !COMMANDS.iter().any(|c| c.name == n))
                .map(|(n, s)| {
                    json!({ "n": n, "d": s.description, "a": true, "k": crate::domain::i18n::t("sup.cmd.kind_skill") })
                }),
        );
    }
    Value::Array(items)
}

fn menu_json(
    agent_kind: Option<&str>,
    skills: &[crate::adapters::driven::skills::Skill],
) -> String {
    menu_items(agent_kind, skills)
        .to_string()
        .replace("</", r"<\/")
}

/// 커맨드가 답 글을 비워 두면 화면에 보일 결과 — 아니면 받은 답 그대로.
///
/// `/compact` 는 요약을 마치고 `result` 글을 비운다(실측) — 빈 답이면 무슨 일이 있었는지 안 보인다.
pub fn shown_answer(sent: &str, got: String) -> String {
    if got.trim().is_empty() && sent.trim() == "/compact" {
        return crate::domain::i18n::t("sup.cmd.compacted");
    }
    got
}

/// 목록 자리 — 입력 폼 바로 위.
pub fn menu_html() -> String {
    r#"<div id="cmdmenu" hidden></div>"#.to_string()
}

/// 목록 모양과 동작 — 문서 끝에 한 번 싣는다.
///
/// 입력이 `/` 로 시작하고 아직 공백이 없을 때만 뜬다. 떠 있는 동안 ↑ · ↓ 는 목록을 고르고(입력
/// 히스토리보다 먼저 받는다 — 캡처 단계) Tab 은 고른 것으로 채운다. Enter 는 이미 다 친 커맨드면 그대로
/// 보내고, 덜 쳤으면 채운다. Esc 는 목록만 닫는다 — 도는 턴을 멈추는 Esc 로 새지 않게 한다.
///
/// 문서를 다시 싣지 않고 목록을 바꿀 때는 `{type:"commands", items}` 메시지가 온다(`skills` 를 다시 훑은 뒤).
pub fn script_html(
    agent_kind: Option<&str>,
    skills: &[crate::adapters::driven::skills::Skill],
) -> String {
    format!(
        r#"<style>
 #cmdmenu{{margin:0 16px;border:1px solid #2a2e37;border-bottom:0;background:#1a1d23;max-height:40vh;overflow-y:auto}}
 #cmdmenu[hidden]{{display:none}}
 #cmdmenu .c{{display:flex;gap:2ch;padding:2px 8px;cursor:pointer}}
 #cmdmenu .c b{{color:#5ac17d;font-weight:400;min-width:16ch}}
 #cmdmenu .c span{{color:#8b93a1}}
 #cmdmenu .c.on{{background:#2a2e37}}
 #cmdmenu .c i{{color:#5c6370;font-style:normal;margin-left:auto}}
</style>
<script>
(function(){{
var t=document.getElementById('t'), m=document.getElementById('cmdmenu');
if(!t||!m)return;
var all={items}, shown=[], sel=0;
function close(){{ m.hidden=true; shown=[]; }}
function draw(){{
  m.textContent='';
  shown.forEach(function(c,i){{
    var d=document.createElement('div'); d.className='c'+(i===sel?' on':'');
    var b=document.createElement('b'); b.textContent=c.n;
    var s=document.createElement('span'); s.textContent=c.d;
    d.appendChild(b); d.appendChild(s);
    if(c.k){{ var k=document.createElement('i'); k.textContent=c.k; d.appendChild(k); }}
    d.addEventListener('mousedown',function(e){{ e.preventDefault(); pick(c); }});
    m.appendChild(d);
  }});
  m.hidden=shown.length===0;
}}
function pick(c){{ t.value=c.n+(c.a?' ':''); close(); t.focus(); t.dispatchEvent(new Event('input')); }}
t.addEventListener('input',function(){{
  var v=t.value;
  if(!/^\/[\w:.-]*$/.test(v)){{ close(); return; }}
  shown=all.filter(function(c){{ return c.n.indexOf(v)===0; }});
  sel=0; draw();
}});
t.addEventListener('keydown',function(e){{
  if(m.hidden||!shown.length)return;
  if(e.isComposing||e.keyCode===229)return;
  var k=e.key;
  if(k==='ArrowDown'||k==='ArrowUp'){{
    sel=(sel+(k==='ArrowDown'?1:shown.length-1))%shown.length; draw();
  }}else if(k==='Tab'){{
    pick(shown[sel]);
  }}else if(k==='Escape'){{
    close();
  }}else if(k==='Enter'&&!e.shiftKey){{
    if(t.value===shown[sel].n&&!shown[sel].a){{ close(); return; }}
    pick(shown[sel]);
  }}else return;
  e.preventDefault(); e.stopImmediatePropagation();
}},true);
t.addEventListener('blur',function(){{ setTimeout(close,150); }});
window.addEventListener('tasty:message',function(e){{
  var x=e.detail;
  if(x&&x.type==='commands'&&Array.isArray(x.items)){{ all=x.items; if(!m.hidden)t.dispatchEvent(new Event('input')); }}
}});
}})();
</script>"#,
        items = menu_json(agent_kind, skills),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_command_has_a_description() {
        for c in COMMANDS {
            let d = describe(c.name);
            assert!(
                !d.is_empty() && !d.starts_with("sup.cmd."),
                "{} 의 설명이 없다: {d}",
                c.name
            );
        }
    }

    /// `/add-dir` 는 claude 에 텍스트로 보내면 안 돈다(실측) — 목록에는 있되 탭이 가로채 데몬으로 보낸다.
    #[test]
    fn add_dir_is_offered_and_handled_by_the_tab() {
        assert!(
            COMMANDS
                .iter()
                .any(|c| c.name == crate::application::add_dir::COMMAND && c.takes_arg)
        );
    }

    #[test]
    fn an_empty_compact_answer_says_what_happened() {
        let shown = shown_answer(" /compact ", String::new());
        assert_eq!(shown, crate::domain::i18n::t("sup.cmd.compacted"));
        assert_ne!(shown, "sup.cmd.compacted", "문구 키가 lang 에 없다");
        // 답이 있으면 그대로 · 다른 말의 빈 답도 그대로다.
        assert_eq!(shown_answer("/compact", "done".into()), "done");
        assert_eq!(shown_answer("hi", String::new()), "");
    }

    /// 턴 끝 처리가 이 함수를 거쳐야 빈 `/compact` 답이 바뀐다 — 함수만 있고 안 부르면 아무 일도 없다.
    #[test]
    fn the_turn_end_runs_the_answer_through_here() {
        let main = crate::test_support::wiring_source();
        let done = main
            .split("fn turn_done(")
            .nth(1)
            .expect("턴 끝 처리가 없다");
        let body = done.split("fn brief_done(").next().unwrap_or(done);
        assert!(
            body.contains("commands::shown_answer("),
            "턴 끝이 커맨드 답을 안 거친다"
        );
        let page = crate::adapters::driving::view::render(
            &crate::domain::convo::Convo {
                session_id: Some("s".into()),
                ..Default::default()
            },
            false,
        );
        assert!(page.contains(r#"id="cmdmenu""#) && page.contains("getElementById('cmdmenu')"));
    }

    #[test]
    fn the_menu_cannot_close_the_script() {
        assert!(!menu_json(None, &[]).contains("</"));
        let s = script_html(None, &[]);
        // 목록이 떠 있을 때의 화살표 · Esc 는 히스토리와 턴 중지보다 먼저 받는다.
        assert!(s.contains("e.stopImmediatePropagation();\n}},true)") || s.contains("},true);"));
        // 플러그인 스킬 이름(`a:b`)을 치는 동안에도 목록이 닫히지 않는다.
        assert!(s.contains("/^\\/[\\w:.-]*$/"));
    }

    /// 스킬은 고정 8 뒤에 `k:"skill"` 로 붙고, 고정과 같은 이름이면 고정이 이긴다.
    #[test]
    fn skills_follow_the_fixed_commands_and_lose_a_name_clash() {
        let skills = [
            crate::adapters::driven::skills::Skill {
                name: "clear".into(),
                description: "clash".into(),
            },
            crate::adapters::driven::skills::Skill {
                name: "p:tool".into(),
                description: "from plugin".into(),
            },
        ];
        let items = menu_items(Some("claude"), &skills);
        let items = items.as_array().expect("array");
        assert_eq!(items.len(), COMMANDS.len() + 1, "{items:?}");
        let last = items.last().expect("last");
        assert_eq!(last["n"], "/p:tool");
        assert_eq!(last["k"], crate::domain::i18n::t("sup.cmd.kind_skill"));
        assert_ne!(last["k"], "sup.cmd.kind_skill", "꼬리표 키가 lang 에 없다");
        assert_eq!(last["a"], true);
        let clear: Vec<_> = items.iter().filter(|i| i["n"] == "/clear").collect();
        assert_eq!(clear.len(), 1);
        assert!(clear[0].get("k").is_none(), "고정 /clear 가 스킬에 밀렸다");
        let s = script_html(Some("claude"), &skills);
        assert!(
            s.contains("x.type==='commands'"),
            "새 목록을 받는 처리기가 없다"
        );
    }

    /// pi 세션의 목록에는 pi 가 받는 커맨드만 있다 — claude 커맨드를 고르면 그냥 글로 간다.
    #[test]
    fn a_pi_session_lists_only_what_pi_takes() {
        let skill = [crate::adapters::driven::skills::Skill {
            name: "probe".into(),
            description: "d".into(),
        }];
        let pi = menu_json(Some("pi"), &skill);
        for c in PI_COMMANDS {
            assert!(pi.contains(&format!("\"{c}\"")), "{c} 가 빠졌다: {pi}");
        }
        assert!(!pi.contains("/context") && !pi.contains("/model"), "{pi}");
        assert!(!pi.contains("/probe"), "pi 에 스킬이 붙었다: {pi}");
        assert!(menu_json(Some("claude"), &[]).contains("/context"));
    }
}
