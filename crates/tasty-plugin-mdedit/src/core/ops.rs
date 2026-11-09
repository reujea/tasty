//! 헤딩 경로로 지정한 섹션에 대한 편집 연산(A1)·체크박스 토글(E6)·에이전트 기록 블록(A6).
//!
//! 모든 연산은 **원문 바이트를 잘라 붙이기만** 한다 — 지정 범위 밖의 바이트는 한 개도
//! 바뀌지 않는다. 그래서 결과와 원문의 `diff` 는 연산이 건드린 줄뿐이다(E1).

use std::ops::Range;

use super::outline::{self, ResolveError, Section, line_spans};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpError {
    Heading(ResolveError),
    /// 섹션 안에 조건에 맞는 체크박스가 없다.
    TaskNotFound(String),
    /// 조건에 맞는 체크박스가 여럿 — 1 기준 줄 번호.
    TaskAmbiguous(String, Vec<usize>),
    InvalidArgument(String),
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Heading(e) => e.fmt(f),
            Self::TaskNotFound(m) => write!(f, "no task line in the section matches '{m}'"),
            Self::TaskAmbiguous(m, lines) => write!(
                f,
                "'{m}' matches {} task lines (lines {}) — use a longer --match",
                lines.len(),
                lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::InvalidArgument(m) => write!(f, "{m}"),
        }
    }
}

impl From<ResolveError> for OpError {
    fn from(e: ResolveError) -> Self {
        Self::Heading(e)
    }
}

/// 섹션 쓰기 연산의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionOp {
    /// 섹션 끝(마지막 내용 줄 뒤)에 붙인다. 섹션 뒤 빈 줄은 그대로 둔다.
    Append,
    /// 헤딩 줄 바로 뒤에 넣는다.
    Prepend,
    /// 헤딩 줄 앞에 넣는다. 넣은 블록 뒤에 빈 줄 하나를 보장한다.
    InsertBefore,
    /// 본문(헤딩 줄 제외, 섹션 끝 빈 줄 제외)을 갈아끼운다.
    Replace,
}

impl SectionOp {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "append" => Self::Append,
            "prepend" => Self::Prepend,
            "insert-before" | "insert_before" => Self::InsertBefore,
            "replace" => Self::Replace,
            _ => return None,
        })
    }
}

/// 섹션 본문에서 마지막 내용 줄의 끝(뒤따르는 빈 줄 앞). 내용이 없으면 본문 시작.
fn content_end(text: &str, sec: &Section) -> usize {
    let body = &text[sec.body.clone()];
    let mut end = sec.body.start;
    for span in line_spans(body) {
        if !body[span.clone()].trim().is_empty() {
            end = sec.body.start + span.end;
        }
    }
    end
}

fn ensure_trailing_newline(mut s: String) -> String {
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// 텍스트 한 군데를 바꾼 결과. 바뀐 범위는 원문 기준.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub insert: String,
}

impl Edit {
    pub fn apply(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() + self.insert.len());
        out.push_str(&text[..self.range.start]);
        out.push_str(&self.insert);
        out.push_str(&text[self.range.end..]);
        out
    }
}

/// `pos` 앞 문자가 줄바꿈이 아니면(파일 끝에 줄바꿈 없음) 줄바꿈을 앞에 붙인다.
fn needs_leading_newline(text: &str, pos: usize) -> bool {
    pos > 0 && !text[..pos].ends_with('\n')
}

pub fn section_edit(text: &str, path: &str, op: SectionOp, body: &str) -> Result<Edit, OpError> {
    let sec = outline::resolve(text, path)?;
    let block = ensure_trailing_newline(body.to_string());
    Ok(match op {
        SectionOp::Append => {
            let at = content_end(text, &sec).max(sec.body.start);
            let lead = if needs_leading_newline(text, at) {
                "\n"
            } else {
                ""
            };
            Edit {
                range: at..at,
                insert: format!("{lead}{block}"),
            }
        }
        SectionOp::Prepend => {
            let at = sec.body.start;
            let lead = if needs_leading_newline(text, at) {
                "\n"
            } else {
                ""
            };
            Edit {
                range: at..at,
                insert: format!("{lead}{block}"),
            }
        }
        SectionOp::InsertBefore => {
            let at = sec.whole.start;
            let mut insert = block;
            if !insert.ends_with("\n\n") {
                insert.push('\n');
            }
            Edit {
                range: at..at,
                insert,
            }
        }
        SectionOp::Replace => {
            let end = content_end(text, &sec);
            let lead = if needs_leading_newline(text, sec.body.start) {
                "\n"
            } else {
                ""
            };
            Edit {
                range: sec.body.start..end.max(sec.body.start),
                insert: format!("{lead}{block}"),
            }
        }
    })
}

/// 섹션 원문(헤딩 줄 포함).
pub fn section_get(text: &str, path: &str) -> Result<(Section, String), OpError> {
    let sec = outline::resolve(text, path)?;
    let s = text[sec.whole.clone()].to_string();
    Ok((sec, s))
}

/// 체크박스 줄 하나의 `[ ]`/`[x]` 안쪽 문자 위치.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMark {
    /// 체크 문자(` ` / `x` / `X`)의 바이트 오프셋.
    pub at: usize,
    pub checked: bool,
    /// 0 기준 줄 번호.
    pub line: usize,
}

/// 줄 하나가 리스트 체크박스면 그 체크 문자의 줄 안 오프셋.
pub fn task_char_in_line(line: &str) -> Option<(usize, bool)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let marker_len = if rest.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 || !rest[digits..].starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let after = &rest[marker_len..];
    let ws = after.len() - after.trim_start_matches([' ', '\t']).len();
    if ws == 0 {
        return None;
    }
    let b = &after.as_bytes()[ws..];
    if b.len() < 3 || b[0] != b'[' || b[2] != b']' {
        return None;
    }
    if b.len() > 3 && !matches!(b[3], b' ' | b'\t' | b'\n' | b'\r') {
        return None;
    }
    let checked = match b[1] {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    Some((indent + marker_len + ws + 1, checked))
}

/// 범위 안의 체크박스 줄 전부.
pub fn task_marks(text: &str, within: Range<usize>) -> Vec<TaskMark> {
    let mut out = Vec::new();
    for (no, span) in line_spans(text).into_iter().enumerate() {
        if span.start < within.start || span.start >= within.end {
            continue;
        }
        if let Some((off, checked)) = task_char_in_line(&text[span.clone()]) {
            out.push(TaskMark {
                at: span.start + off,
                checked,
                line: no,
            });
        }
    }
    out
}

/// 토글 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTo {
    Toggle,
    Checked,
    Unchecked,
}

impl TaskTo {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "toggle" => Self::Toggle,
            "checked" | "done" => Self::Checked,
            "unchecked" | "todo" => Self::Unchecked,
            _ => return None,
        })
    }
}

/// 체크 문자 한 바이트를 바꾸는 편집. 이미 그 상태면 빈 편집(범위 0, 같은 문자).
pub fn task_set(text: &str, mark: &TaskMark, to: TaskTo) -> Edit {
    let want = match to {
        TaskTo::Toggle => !mark.checked,
        TaskTo::Checked => true,
        TaskTo::Unchecked => false,
    };
    let ch = if want == mark.checked {
        text[mark.at..mark.at + 1].to_string()
    } else if want {
        "x".to_string()
    } else {
        " ".to_string()
    };
    Edit {
        range: mark.at..mark.at + 1,
        insert: ch,
    }
}

/// 섹션 안에서 `needle` 을 포함하는 체크박스 하나를 토글한다.
pub fn task_toggle(text: &str, path: &str, needle: &str, to: TaskTo) -> Result<Edit, OpError> {
    if needle.trim().is_empty() {
        return Err(OpError::InvalidArgument("--match must not be empty".into()));
    }
    let sec = outline::resolve(text, path)?;
    let spans = line_spans(text);
    let hits: Vec<TaskMark> = task_marks(text, sec.whole.clone())
        .into_iter()
        .filter(|m| text[spans[m.line].clone()].contains(needle))
        .collect();
    match hits.as_slice() {
        [] => Err(OpError::TaskNotFound(needle.to_string())),
        [one] => Ok(task_set(text, one, to)),
        many => Err(OpError::TaskAmbiguous(
            needle.to_string(),
            many.iter().map(|m| m.line + 1).collect(),
        )),
    }
}

/// `> [!agent] HH:MM 제목` 블록 원문. `time` 이 없으면 시각을 생략한다(일지 규칙: 정확히 알
/// 때만). 본문 줄은 모두 `> ` 로 시작한다 — 빈 줄도 `>` 로 이어 블록이 끊기지 않게 한다.
pub fn agent_note_block(time: Option<&str>, title: &str, body: &str) -> Result<String, OpError> {
    let title = title.trim();
    if title.is_empty() || title.contains('\n') {
        return Err(OpError::InvalidArgument(
            "--title must be one non-empty line".into(),
        ));
    }
    let mut out = String::from("> [!agent] ");
    if let Some(t) = time {
        out.push_str(t);
        out.push(' ');
    }
    out.push_str(title);
    out.push('\n');
    for line in body.lines() {
        if line.trim().is_empty() {
            out.push_str(">\n");
        } else {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(out)
}

/// 섹션 끝에 에이전트 기록 블록을 붙인다. 직전 내용 줄이 인용(`>`)이면 빈 줄을 하나 끼운다 —
/// 붙여 쓰면 앞 블록과 한 인용으로 합쳐져 별개 블록 규칙이 깨진다.
pub fn agent_note(text: &str, path: &str, block: &str) -> Result<Edit, OpError> {
    let sec = outline::resolve(text, path)?;
    let at = content_end(text, &sec).max(sec.body.start);
    let prev_line = text[..at].trim_end_matches('\n').rsplit('\n').next();
    let prev_is_quote = at > sec.body.start && prev_line.is_some_and(|l| l.starts_with('>'));
    let mut insert = String::new();
    if needs_leading_newline(text, at) {
        insert.push('\n');
    }
    if prev_is_quote {
        insert.push('\n');
    }
    insert.push_str(block);
    Ok(Edit {
        range: at..at,
        insert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str =
        "# 할 일\n## A\n- [ ] one\n- [x] two\n\n# 2026.10.22\n- x\n\n# 2026.10.21\n- y\n";

    fn run(op: SectionOp, path: &str, body: &str) -> String {
        section_edit(DOC, path, op, body).unwrap().apply(DOC)
    }

    #[test]
    fn append_goes_after_last_content_line_and_keeps_blank_separator() {
        assert_eq!(
            run(SectionOp::Append, "# 2026.10.22", "- new"),
            "# 할 일\n## A\n- [ ] one\n- [x] two\n\n# 2026.10.22\n- x\n- new\n\n# 2026.10.21\n- y\n"
        );
    }

    #[test]
    fn prepend_insert_before_replace() {
        assert_eq!(
            run(SectionOp::Prepend, "# 2026.10.22", "- first"),
            "# 할 일\n## A\n- [ ] one\n- [x] two\n\n# 2026.10.22\n- first\n- x\n\n# 2026.10.21\n- y\n"
        );
        assert_eq!(
            run(SectionOp::InsertBefore, "# 2026.10.22", "# 2026.10.23\n- z"),
            "# 할 일\n## A\n- [ ] one\n- [x] two\n\n# 2026.10.23\n- z\n\n# 2026.10.22\n- x\n\n# 2026.10.21\n- y\n"
        );
        assert_eq!(
            run(SectionOp::Replace, "# 2026.10.22", "- r1\n- r2\n"),
            "# 할 일\n## A\n- [ ] one\n- [x] two\n\n# 2026.10.22\n- r1\n- r2\n\n# 2026.10.21\n- y\n"
        );
    }

    #[test]
    fn last_section_without_trailing_newline() {
        let doc = "# a\n- x";
        let out = section_edit(doc, "# a", SectionOp::Append, "- y")
            .unwrap()
            .apply(doc);
        assert_eq!(out, "# a\n- x\n- y\n");
    }

    #[test]
    fn edit_outside_range_is_byte_identical() {
        let out = run(SectionOp::Append, "# 2026.10.21", "- tail");
        assert!(out.starts_with(&DOC[..DOC.len() - 1]));
    }

    #[test]
    fn task_toggle_changes_one_byte() {
        let e = task_toggle(DOC, "# 할 일", "one", TaskTo::Toggle).unwrap();
        let out = e.apply(DOC);
        assert_eq!(out.len(), DOC.len());
        assert_eq!(out.replace("- [x] one", "- [ ] one"), DOC);
        assert!(matches!(
            task_toggle(DOC, "# 할 일", "o", TaskTo::Toggle),
            Err(OpError::TaskAmbiguous(_, _))
        ));
        assert!(matches!(
            task_toggle(DOC, "# 2026.10.22", "x", TaskTo::Toggle),
            Err(OpError::TaskNotFound(_))
        ));
        // 이미 그 상태면 바이트가 그대로다.
        let e = task_toggle(DOC, "# 할 일", "two", TaskTo::Checked).unwrap();
        assert_eq!(e.apply(DOC), DOC);
    }

    #[test]
    fn task_line_shapes() {
        assert_eq!(task_char_in_line("- [ ] a"), Some((3, false)));
        assert_eq!(task_char_in_line("  * [X] a"), Some((5, true)));
        assert_eq!(task_char_in_line("1. [x] a"), Some((4, true)));
        assert_eq!(task_char_in_line("- [ ]"), Some((3, false)));
        assert_eq!(task_char_in_line("- [y] a"), None);
        assert_eq!(task_char_in_line("-[ ] a"), None);
        assert_eq!(task_char_in_line("- [ ]a"), None);
    }

    #[test]
    fn agent_note_appends_separate_block() {
        let block = agent_note_block(Some("15:10"), "정리", "- a\n\n- b").unwrap();
        assert_eq!(block, "> [!agent] 15:10 정리\n> - a\n>\n> - b\n");
        let doc = "# d\n> [!agent] old\n> - z\n\n# e\n";
        let out = agent_note(doc, "# d", &block).unwrap().apply(doc);
        assert_eq!(
            out,
            "# d\n> [!agent] old\n> - z\n\n> [!agent] 15:10 정리\n> - a\n>\n> - b\n\n# e\n"
        );
        let doc = "# d\n- user\n\n# e\n";
        let out = agent_note(doc, "# d", "> [!agent] t\n").unwrap().apply(doc);
        assert_eq!(out, "# d\n- user\n> [!agent] t\n\n# e\n");
        assert!(agent_note_block(None, " ", "").is_err());
        assert_eq!(agent_note_block(None, "t", "").unwrap(), "> [!agent] t\n");
    }
}
