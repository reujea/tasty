//! 대화 기록의 **시스템 줄** — plugin 설정이 사람 대신 보낸 제어 턴을 화면에 남긴다.
//!
//! 현행화 뒤의 `/compact` 는 제어 턴이라 대화 턴으로 안 그린다(`sync_compact::start_compact`).
//! 그러면 사람은 컨텍스트가 왜 줄었는지 화면에서 볼 수 없다 — 그래서 그 자리에 `[system]` 줄을
//! 하나 두고, 어느 설정이 보냈는지와 끝(요약 중 · 끝 · 실패)을 싣는다.
//!
//! 줄은 **턴 사이 자리**(`at` = 그 줄 앞에 있던 턴 수)를 기억한다. 오래된 턴을 디스크로 내리면
//! 자리를 그만큼 당기고([`shift`]), 대화를 비우면 함께 지운다(`clear::reset`). 플러그인이 다시 뜨면
//! 대화 기록(jsonl)에서 턴을 다시 채우는데 시스템 줄은 거기서 오지 않는다 — 메모리에만 있다.

use crate::domain::convo::Convo;
use crate::domain::sync_rules::Mode;

/// 시스템 줄 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// 이 줄 앞에 있던 턴 수 — 그 턴 바로 뒤에 그린다.
    pub at: usize,
    /// 무엇을 보냈나 · 어느 설정이 보냈나(번역된 문구).
    pub text: String,
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed(String),
}

/// 그리는 모양 — 대화 턴보다 작고 흐리게, 왼쪽 줄로 턴과 가른다.
pub const STYLE: &str = r#"<style>
 .sys{color:#8b93a1;font-size:12px;margin:0 0 16px;padding:2px 0 2px 10px;border-left:2px solid #3a4150}
 .sys .st{margin-left:6px}
 .sys.done .st{color:#6a9955}
 .sys.failed .st{color:#e06c75}
</style>"#;

/// compact 제어 턴을 연 설정의 문구 — 모드가 곧 그 설정이다.
pub fn compact_text(mode: Mode) -> String {
    crate::domain::i18n::t(match mode {
        Mode::Auto => "sup.sys.compact_auto",
        Mode::Idle => "sup.sys.compact_idle",
        Mode::Manual => "sup.sys.compact_manual",
    })
}

/// 지금 턴 뒤에 줄을 단다 — 그 줄의 번호(화면 id)를 돌려준다.
pub fn open(c: &mut Convo, text: String) -> usize {
    c.sys.push(Note {
        at: c.turns.len(),
        text,
        state: State::Running,
    });
    c.sys.len() - 1
}

/// 아직 도는 마지막 줄을 끝낸다 — 끝낸 줄의 번호.
pub fn settle(c: &mut Convo, state: State) -> Option<usize> {
    let i = c.sys.iter().rposition(|n| n.state == State::Running)?;
    c.sys[i].state = state;
    Some(i)
}

/// 앞쪽 턴 `cut` 개가 빠졌다 — 그 앞의 줄은 버리고 나머지 자리를 당긴다.
pub fn shift(notes: &mut Vec<Note>, cut: usize) {
    notes.retain(|n| n.at >= cut);
    for n in notes.iter_mut() {
        n.at -= cut;
    }
}

fn class_and_status(state: &State) -> (&'static str, String) {
    match state {
        State::Running => ("", crate::domain::i18n::t("sup.sys.running")),
        State::Done => (" done", crate::domain::i18n::t("sup.sys.done")),
        State::Failed(why) => (" failed", crate::domain::i18n::t1("sup.sys.failed", why)),
    }
}

/// 줄 하나 — 페이지 앱이 그리는 값. 상태 문구는 로케일을 따라 여기서 만든다.
pub fn note_json(i: usize, n: &Note) -> serde_json::Value {
    let (cls, st) = class_and_status(&n.state);
    serde_json::json!({ "id": i, "at": n.at, "cls": cls, "text": n.text, "st": st })
}

/// 줄 하나를 문서에 넣거나 고친다 — 문서를 다시 싣지 않는다.
pub fn message(c: &Convo, i: usize) -> Option<serde_json::Value> {
    let n = c.sys.get(i)?;
    Some(serde_json::json!({ "type": "sys.line", "note": note_json(i, n) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convo::Turn;

    fn turn() -> Turn {
        Turn {
            sent: "x".into(),
            got: "y".into(),
            pending: false,
            at: None,
            error: None,
        }
    }

    #[test]
    fn a_line_sits_after_the_turns_it_followed_and_settles_once() {
        let mut c = Convo {
            turns: vec![turn(), turn()],
            ..Default::default()
        };
        let i = open(&mut c, compact_text(Mode::Idle));
        assert_eq!(c.sys[i].at, 2);
        let n = note_json(i, &c.sys[i]);
        assert_eq!(n["at"], 2);
        assert!(
            n["text"]
                .as_str()
                .unwrap_or_default()
                .starts_with("[system]"),
            "{n}"
        );
        assert_eq!(n["cls"], "");
        assert_eq!(settle(&mut c, State::Done), Some(i));
        assert_eq!(note_json(i, &c.sys[i])["cls"], " done");
        // 도는 줄이 없으면 아무것도 안 끝낸다.
        assert_eq!(settle(&mut c, State::Done), None);
    }

    #[test]
    fn each_mode_names_its_setting() {
        let texts: Vec<String> = [Mode::Auto, Mode::Idle, Mode::Manual]
            .into_iter()
            .map(compact_text)
            .collect();
        for t in &texts {
            assert!(t.starts_with("[system] /compact"), "{t}");
        }
        assert_ne!(texts[0], texts[1]);
        assert_ne!(texts[1], texts[2]);
    }

    #[test]
    fn archiving_turns_pulls_the_lines_and_drops_the_ones_before() {
        let mut notes = vec![
            Note {
                at: 1,
                text: "a".into(),
                state: State::Done,
            },
            Note {
                at: 5,
                text: "b".into(),
                state: State::Done,
            },
        ];
        shift(&mut notes, 3);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].at, 2);
    }

    #[test]
    fn the_line_is_escaped_and_the_failure_is_shown() {
        let mut c = Convo::default();
        let i = open(&mut c, "<b>".into());
        settle(&mut c, State::Failed("</script>".into()));
        let s = message(&c, i).expect("message");
        assert_eq!(s["type"], "sys.line");
        assert_eq!(s["note"]["text"], "<b>", "원문 그대로 — 앱이 글로 넣는다");
        assert_eq!(s["note"]["cls"], " failed");
        assert!(
            s["note"]["st"]
                .as_str()
                .unwrap_or_default()
                .contains("</script>")
        );
        assert!(message(&c, i + 1).is_none());
    }

    #[test]
    fn the_lines_are_drawn_opened_and_settled_here() {
        // 배선 — 렌더가 그리고, compact 를 보낼 때 열고, 끝에서 닫고, 비우기 · 내리기가 따라온다.
        let src = |p: &str| std::fs::read_to_string(p).expect(p);
        assert!(
            src("src/adapters/driving/client.rs")
                .contains("crate::domain::sys_note::note_json(i, n)")
        );
        assert!(src("assets/app.js").contains("if (n.at === i)"));
        let sync = src("src/application/sync_compact.rs");
        assert!(sync.contains(
            "crate::domain::sys_note::open(c, crate::domain::sys_note::compact_text(mode))"
        ));
        assert!(
            sync.contains(
                "crate::domain::sys_note::settle(c, crate::domain::sys_note::State::Done)"
            )
        );
        assert!(src("src/application/clear.rs").contains("c.sys.clear()"));
        assert!(
            src("src/adapters/driven/store/turn_archive.rs")
                .contains("crate::domain::sys_note::shift(&mut convo.sys, cut)")
        );
    }
}
