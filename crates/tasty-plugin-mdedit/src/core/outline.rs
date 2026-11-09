//! 문서의 헤딩·섹션 구조. 줄 단위로 훑고 **바이트 오프셋**으로 답한다 — 편집 연산이
//! 원문 바이트를 그대로 잘라 붙이기 위해서다(파싱한 결과로 다시 쓰지 않는다).
//!
//! 헤딩은 ATX(`#`~`######` + 공백)만 본다. 펜스 코드(```` ``` ```` · `~~~`) 안의 `#` 줄과
//! 4 칸 이상 들여 쓴 줄은 헤딩이 아니다. setext(`===` 밑줄) 헤딩은 다루지 않는다 —
//! 섹션 경로로 지정하기에 줄 하나로 식별되지 않는다.

use std::ops::Range;

/// 헤딩 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1..=6.
    pub level: u8,
    /// 0 기준 줄 번호.
    pub line: usize,
    /// 줄 전체(줄바꿈 제외, 앞뒤 공백 제거). 예: `# 2026.10.22`.
    pub text: String,
    /// `#` 와 닫는 `#` 를 뗀 제목. 예: `2026.10.22`.
    pub title: String,
    /// 이 줄의 바이트 범위(줄바꿈 포함).
    pub span: Range<usize>,
}

/// 헤딩 하나가 덮는 구간 — 그 헤딩 줄부터 같거나 높은 레벨의 다음 헤딩 전까지(하위 섹션 포함).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub heading: Heading,
    /// 헤딩 줄 다음부터 섹션 끝까지.
    pub body: Range<usize>,
    /// 헤딩 줄 시작부터 섹션 끝까지.
    pub whole: Range<usize>,
}

/// `text` 를 줄로 나눈 바이트 범위들(각 범위는 줄바꿈을 포함한다). 빈 문자열은 줄 0 개.
pub fn line_spans(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            out.push(start..i + 1);
            start = i + 1;
        }
    }
    if start < text.len() {
        out.push(start..text.len());
    }
    out
}

fn strip_eol(line: &str) -> &str {
    line.strip_suffix('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .unwrap_or(line)
}

/// 펜스 여닫이 판정용 — 들여쓰기 3 칸 이하 + 같은 문자 3 개 이상.
fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = rest.bytes().next()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let run = rest.bytes().take_while(|&b| b == ch).count();
    (run >= 3).then_some((ch, run))
}

fn parse_atx(line: &str) -> Option<(u8, String)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = &rest[hashes..];
    if !after.is_empty() && !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    let mut title = after.trim();
    // 닫는 `#` 열(앞에 공백이 있을 때만 닫는 열이다).
    let trimmed = title.trim_end_matches('#');
    if trimmed.len() != title.len() && (trimmed.is_empty() || trimmed.ends_with([' ', '\t'])) {
        title = trimmed.trim_end();
    }
    Some((hashes as u8, title.to_string()))
}

/// 문서의 ATX 헤딩 전부(문서 순서).
pub fn headings(text: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for (no, span) in line_spans(text).into_iter().enumerate() {
        let line = strip_eol(&text[span.clone()]);
        if let Some((ch, len)) = fence {
            if let Some((c2, l2)) = fence_marker(line)
                && c2 == ch
                && l2 >= len
                && line
                    .trim_start()
                    .trim_start_matches(ch as char)
                    .trim()
                    .is_empty()
            {
                fence = None;
            }
            continue;
        }
        if let Some(m) = fence_marker(line) {
            fence = Some(m);
            continue;
        }
        if let Some((level, title)) = parse_atx(line) {
            out.push(Heading {
                level,
                line: no,
                text: line.trim().to_string(),
                title,
                span,
            });
        }
    }
    out
}

/// 모든 헤딩의 섹션.
pub fn sections(text: &str) -> Vec<Section> {
    let hs = headings(text);
    hs.iter()
        .enumerate()
        .map(|(i, h)| {
            let end = hs[i + 1..]
                .iter()
                .find(|n| n.level <= h.level)
                .map_or(text.len(), |n| n.span.start);
            Section {
                heading: h.clone(),
                body: h.span.end..end,
                whole: h.span.start..end,
            }
        })
        .collect()
}

/// 섹션 경로 지정이 실패한 이유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Empty,
    NotFound(String),
    /// 같은 경로에 맞는 헤딩이 둘 이상 — 각 헤딩의 1 기준 줄 번호.
    Ambiguous(String, Vec<usize>),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty heading path"),
            Self::NotFound(p) => write!(f, "no heading matches '{p}'"),
            Self::Ambiguous(p, lines) => write!(
                f,
                "heading '{p}' matches {} headings (lines {}) — add the parent with ' > '",
                lines.len(),
                lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// 경로 한 마디가 헤딩에 맞는가. `#` 로 시작하면 레벨까지 견주고(`# 참조`), 아니면 제목만
/// 견준다(`참조` — 레벨 무관).
fn segment_matches(seg: &str, h: &Heading) -> bool {
    let seg = seg.trim();
    if seg.starts_with('#') {
        parse_atx(seg).is_some_and(|(lvl, title)| lvl == h.level && title == h.title)
    } else {
        seg == h.title
    }
}

/// 헤딩 경로(`# 할 일 > ## Project A` 처럼 ` > ` 로 조상을 잇는다)로 섹션 하나를 고른다.
/// 0 개나 2 개 이상이면 오류 — 줄 번호를 추측하지 않는다.
pub fn resolve(text: &str, path: &str) -> Result<Section, ResolveError> {
    let segs: Vec<&str> = path
        .split(" > ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let Some((last, parents)) = segs.split_last() else {
        return Err(ResolveError::Empty);
    };
    let secs = sections(text);
    let hits: Vec<&Section> = secs
        .iter()
        .enumerate()
        .filter(|(i, s)| segment_matches(last, &s.heading) && ancestors_match(&secs, *i, parents))
        .map(|(_, s)| s)
        .collect();
    match hits.as_slice() {
        [] => Err(ResolveError::NotFound(path.to_string())),
        [one] => Ok((*one).clone()),
        many => Err(ResolveError::Ambiguous(
            path.to_string(),
            many.iter().map(|s| s.heading.line + 1).collect(),
        )),
    }
}

/// `idx` 의 조상 사슬(가까운 것부터)에 `parents` 가 바깥→안 순서로 들어 있는가.
fn ancestors_match(secs: &[Section], idx: usize, parents: &[&str]) -> bool {
    let mut want = parents.iter().rev().peekable();
    let mut level = secs[idx].heading.level;
    for s in secs[..idx].iter().rev() {
        if s.heading.level >= level {
            continue;
        }
        level = s.heading.level;
        if let Some(seg) = want.peek()
            && segment_matches(seg, &s.heading)
        {
            want.next();
        }
    }
    want.peek().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "<!-- c -->\n\n# 할 일\n\n## Project A\n- [ ] a\n\n# 참조\n## 업무\n```\n# not\n```\n# 2026.10.22\n- x\n\n# 2026.10.21\n## Project A\n- y\n";

    #[test]
    fn headings_skip_fenced_code() {
        let hs = headings(DOC);
        let titles: Vec<_> = hs.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(
            titles,
            [
                "# 할 일",
                "## Project A",
                "# 참조",
                "## 업무",
                "# 2026.10.22",
                "# 2026.10.21",
                "## Project A"
            ]
        );
    }

    #[test]
    fn section_runs_to_next_same_or_higher_heading() {
        let s = resolve(DOC, "# 2026.10.22").unwrap();
        assert_eq!(&DOC[s.whole.clone()], "# 2026.10.22\n- x\n\n");
        let s = resolve(DOC, "# 할 일").unwrap();
        assert_eq!(&DOC[s.body.clone()], "\n## Project A\n- [ ] a\n\n");
    }

    #[test]
    fn duplicate_heading_is_ambiguous_until_parent_given() {
        assert!(matches!(
            resolve(DOC, "## Project A"),
            Err(ResolveError::Ambiguous(_, ref l)) if l == &vec![5, 17]
        ));
        let s = resolve(DOC, "# 2026.10.21 > ## Project A").unwrap();
        assert_eq!(s.heading.line, 16);
        let s = resolve(DOC, "할 일 > Project A").unwrap();
        assert_eq!(s.heading.line, 4);
    }

    #[test]
    fn missing_heading_is_an_error() {
        assert!(matches!(
            resolve(DOC, "# 2026.10.23"),
            Err(ResolveError::NotFound(_))
        ));
        assert!(matches!(resolve(DOC, "  "), Err(ResolveError::Empty)));
        // 레벨을 적으면 레벨까지 맞아야 한다.
        assert!(matches!(
            resolve(DOC, "## 참조"),
            Err(ResolveError::NotFound(_))
        ));
    }

    #[test]
    fn closing_hashes_and_non_headings() {
        assert_eq!(parse_atx("## a ##"), Some((2, "a".into())));
        assert_eq!(parse_atx("#a"), None);
        assert_eq!(parse_atx("    # code"), None);
        assert_eq!(parse_atx("####### seven"), None);
        assert_eq!(parse_atx("# C#"), Some((1, "C#".into())));
    }
}
