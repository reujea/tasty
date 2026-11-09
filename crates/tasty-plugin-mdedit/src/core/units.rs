//! 미리보기의 블록 손잡이(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다)가 다루는 단위 — 통째로 옮기고 · 복제하고 · 지우고 · 형식을
//! 바꾸는 블록.
//!
//! 단위는 최상위 블록과 목록 항목(하위 목록 포함)이다. 목록은 그 자체가 아니라 항목이 단위다 —
//! 최상위 목록과 항목 안의 하위 목록이 항목을 담는 그릇이다. 인용 · 표 · 각주 안의 블록은 단위가
//! 아니다(그 블록째 옮긴다).
//!
//! 모든 연산은 단위의 **원문 줄을 통째로** 잘라 붙인다 — 단위 앞뒤의 구분(빈 줄 · 줄바꿈)만
//! 함께 다루고, 그 밖의 바이트는 한 개도 바뀌지 않는다. 번호 목록의 번호도 다시 매기지 않는다
//! (다시 매기면 옮긴 줄 밖이 바뀐다 — 화면 번호는 렌더러가 첫 항목부터 차례로 그린다).

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

use super::blocks::{EditError, parser_options};

/// 단위의 종류 — 웹뷰가 요소를 짝짓고 형식 이름을 보일 때 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Para,
    Heading(u8),
    Item,
    Quote,
    Code,
    Table,
    Rule,
    DefList,
    /// 미리보기에 제자리 요소가 없는 것(HTML · 각주 정의 · 머리말) — 구분 계산에만 쓴다.
    Hidden,
}

impl UnitKind {
    pub fn code(self) -> String {
        match self {
            Self::Para => "p".into(),
            Self::Heading(n) => format!("h{n}"),
            Self::Item => "li".into(),
            Self::Quote => "quote".into(),
            Self::Code => "code".into(),
            Self::Table => "table".into(),
            Self::Rule => "hr".into(),
            Self::DefList => "dl".into(),
            Self::Hidden => "none".into(),
        }
    }
}

/// 손잡이가 붙는 단위 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub kind: UnitKind,
    /// 첫 줄 머리부터 마지막 줄 끝(줄바꿈 제외)까지.
    pub range: Range<usize>,
    /// 담긴 그릇 — `None` 이면 문서 최상위, `Some(k)` 면 목록 `k` 의 항목.
    pub list: Option<usize>,
    /// 항목이면 그 항목이 체크 항목인가.
    pub task: bool,
}

/// 항목을 담는 목록 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub ordered: bool,
    /// 목록이 들어 있는 항목(단위 번호) — `None` 이면 최상위 목록.
    pub parent: Option<usize>,
    pub range: Range<usize>,
}

/// 최상위의 한 자리 — 단위이거나 최상위 목록이다(문서 순서).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Unit(usize),
    List(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Units {
    pub units: Vec<Unit>,
    pub lists: Vec<List>,
    pub top: Vec<Entry>,
}

/// 형식 변경의 목표.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Body,
    H1,
    H2,
    H3,
    Bullet,
    Numbered,
    Task,
    Quote,
}

impl Format {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "paragraph" => Self::Body,
            "h1" => Self::H1,
            "h2" => Self::H2,
            "h3" => Self::H3,
            "bullet" => Self::Bullet,
            "numbered" => Self::Numbered,
            "task" => Self::Task,
            "quote" => Self::Quote,
            _ => return None,
        })
    }
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(src: &str, at: usize) -> usize {
    src[at..].find('\n').map_or(src.len(), |i| at + i)
}

/// 이벤트 범위를 줄 단위로 — 첫 줄 머리부터, 마지막 비공백 글자가 있는 줄의 끝까지.
fn lines_of(src: &str, r: &Range<usize>) -> Range<usize> {
    let start = line_start(src, r.start);
    let body = &src[start..r.end.min(src.len())];
    match body.trim_end().char_indices().last() {
        None => start..start,
        Some((i, _)) => start..line_end(src, start + i),
    }
}

pub fn units(src: &str) -> Units {
    let mut out = Units::default();
    // 열린 태그 — (태그, 그 태그가 연 단위 · 목록 번호).
    enum Open {
        Unit(usize),
        List(usize),
        Other,
    }
    let mut stack: Vec<Open> = Vec::new();
    for (ev, range) in Parser::new_ext(src, parser_options()).into_offset_iter() {
        match ev {
            Event::Start(tag) => {
                // 단위를 받는 자리: 최상위이거나, 단위 목록 바로 안의 항목.
                let parent_list = match stack.last() {
                    None => Some(None),
                    Some(Open::List(k)) => Some(Some(*k)),
                    _ => None,
                };
                let open = match (&tag, parent_list) {
                    (Tag::List(start), Some(None)) => {
                        let k = out.lists.len();
                        out.lists.push(List {
                            ordered: start.is_some(),
                            parent: None,
                            range: lines_of(src, &range),
                        });
                        out.top.push(Entry::List(k));
                        Open::List(k)
                    }
                    (Tag::List(start), None) => {
                        // 단위 항목 바로 안의 하위 목록만 그릇이다(항목 안 문단 뒤에 올 수도 있다).
                        match stack.last() {
                            Some(Open::Unit(u)) if out.units[*u].kind == UnitKind::Item => {
                                let k = out.lists.len();
                                out.lists.push(List {
                                    ordered: start.is_some(),
                                    parent: Some(*u),
                                    range: lines_of(src, &range),
                                });
                                Open::List(k)
                            }
                            _ => Open::Other,
                        }
                    }
                    (Tag::Item, Some(Some(k))) => {
                        let u = out.units.len();
                        out.units.push(Unit {
                            kind: UnitKind::Item,
                            range: lines_of(src, &range),
                            list: Some(k),
                            task: false,
                        });
                        Open::Unit(u)
                    }
                    (_, Some(None)) => {
                        let kind = match &tag {
                            Tag::Paragraph => UnitKind::Para,
                            Tag::Heading { level, .. } => UnitKind::Heading(match level {
                                HeadingLevel::H1 => 1,
                                HeadingLevel::H2 => 2,
                                HeadingLevel::H3 => 3,
                                HeadingLevel::H4 => 4,
                                HeadingLevel::H5 => 5,
                                HeadingLevel::H6 => 6,
                            }),
                            Tag::BlockQuote(_) => UnitKind::Quote,
                            Tag::CodeBlock(_) => UnitKind::Code,
                            Tag::Table(_) => UnitKind::Table,
                            Tag::DefinitionList => UnitKind::DefList,
                            _ => UnitKind::Hidden,
                        };
                        let u = out.units.len();
                        out.units.push(Unit {
                            kind,
                            range: lines_of(src, &range),
                            list: None,
                            task: false,
                        });
                        out.top.push(Entry::Unit(u));
                        Open::Unit(u)
                    }
                    _ => Open::Other,
                };
                stack.push(open);
            }
            Event::End(TagEnd::Item | TagEnd::List(_)) | Event::End(_) => {
                stack.pop();
            }
            Event::TaskListMarker(_) => {
                if let Some(Open::Unit(u)) = stack.iter().rev().find(|o| matches!(o, Open::Unit(_)))
                {
                    out.units[*u].task = true;
                }
            }
            Event::Rule if stack.is_empty() => {
                out.units.push(Unit {
                    kind: UnitKind::Rule,
                    range: lines_of(src, &range),
                    list: None,
                    task: false,
                });
                out.top.push(Entry::Unit(out.units.len() - 1));
            }
            Event::Html(_) if stack.is_empty() => {
                out.units.push(Unit {
                    kind: UnitKind::Hidden,
                    range: lines_of(src, &range),
                    list: None,
                    task: false,
                });
                out.top.push(Entry::Unit(out.units.len() - 1));
            }
            _ => {}
        }
    }
    out
}

impl Units {
    fn entry_range(&self, e: Entry) -> &Range<usize> {
        match e {
            Entry::Unit(u) => &self.units[u].range,
            Entry::List(k) => &self.lists[k].range,
        }
    }

    /// 단위 `u` 와 같은 그릇의 자리들(문서 순서).
    pub fn siblings(&self, u: usize) -> Vec<Entry> {
        match self.units[u].list {
            None => self.top.clone(),
            Some(k) => (0..self.units.len())
                .filter(|&i| self.units[i].list == Some(k))
                .map(Entry::Unit)
                .collect(),
        }
    }

    fn visible(&self, e: Entry) -> bool {
        match e {
            Entry::Unit(u) => self.units[u].kind != UnitKind::Hidden,
            Entry::List(_) => true,
        }
    }
}

fn get<'a>(us: &'a Units, src: &str, u: usize, old: &str) -> Result<&'a Unit, EditError> {
    let unit = us.units.get(u).ok_or(EditError::Stale)?;
    if src.get(unit.range.clone()) != Some(old) {
        return Err(EditError::Stale);
    }
    Ok(unit)
}

fn default_sep(unit: &Unit) -> &'static str {
    if unit.list.is_some() { "\n" } else { "\n\n" }
}

/// 단위를 지울 때 함께 지울 범위와, 그 단위를 다른 데 넣을 때 쓸 구분. 바로 뒤 자리와의 사이가
/// 공백뿐이면 그 구분을, 아니면 바로 앞 자리와의 구분을 쓴다(링크 정의처럼 이벤트가 없는 줄은
/// 공백이 아니므로 지우지 않는다).
fn cut(us: &Units, src: &str, u: usize) -> (Range<usize>, String) {
    let sib = us.siblings(u);
    let me = sib.iter().position(|e| *e == Entry::Unit(u)).unwrap_or(0);
    let r = us.units[u].range.clone();
    if let Some(next) = sib.get(me + 1) {
        let gap = &src[r.end..us.entry_range(*next).start];
        if gap.trim().is_empty() {
            return (r.start..us.entry_range(*next).start, gap.to_string());
        }
    }
    if me > 0 {
        let prev = us.entry_range(sib[me - 1]).end;
        let gap = &src[prev..r.start];
        if gap.trim().is_empty() {
            return (prev..r.end, gap.to_string());
        }
    }
    // 이웃과의 사이에 이벤트가 없는 줄(링크 정의 등)이 끼어 있거나 그릇에 혼자다. 줄 단위로만 지운다 —
    // 단위 줄의 줄바꿈과 공백뿐인 줄까지, 이웃 줄의 글자 · 들여쓰기 · 줄 끝 공백은 건드리지 않는다.
    // 뒤에 무엇이 있으면 뒤 빈 줄을, 문서 끝이면 앞 빈 줄과 앞 줄의 줄바꿈을 지운다.
    if src[r.end..].trim().is_empty() && r.start > 0 {
        let mut s = r.start;
        while s > 0 {
            let ls = line_start(src, s - 1);
            if ls > 0 && src[ls..s - 1].trim().is_empty() {
                s = ls;
            } else {
                break;
            }
        }
        return (s - 1..r.end, src[s - 1..r.start].to_string());
    }
    let mut end = r.end;
    if src[end..].starts_with('\n') {
        end += 1;
        while let Some(i) = src[end..].find('\n') {
            if !src[end..end + i].trim().is_empty() {
                break;
            }
            end += i + 1;
        }
    }
    (r.start..end, default_sep(&us.units[u]).to_string())
}

fn splice(src: &str, r: Range<usize>, with: &str) -> String {
    let mut s = String::with_capacity(src.len() + with.len());
    s.push_str(&src[..r.start]);
    s.push_str(with);
    s.push_str(&src[r.end..]);
    s
}

/// 단위 `u` 를 지운다(그 단위 줄과 구분 하나).
pub fn delete(src: &str, u: usize, old: &str) -> Result<String, EditError> {
    let us = units(src);
    get(&us, src, u, old)?;
    let (r, _) = cut(&us, src, u);
    Ok(splice(src, r, ""))
}

/// 단위 `u` 바로 뒤에 같은 단위를 하나 더 넣는다. 사본의 머리 바이트를 돌려준다.
pub fn duplicate(src: &str, u: usize, old: &str) -> Result<(String, usize), EditError> {
    let us = units(src);
    let unit = get(&us, src, u, old)?;
    let (_, sep) = cut(&us, src, u);
    let at = unit.range.end;
    let ins = format!("{sep}{old}");
    Ok((splice(src, at..at, &ins), at + sep.len()))
}

/// 단위 `u` 를 같은 그릇의 자리 `before` 앞으로(없으면 그릇 끝으로) 옮긴다. 옮긴 단위의 머리
/// 바이트를 돌려준다.
pub fn move_to(
    src: &str,
    u: usize,
    old: &str,
    before: Option<Entry>,
) -> Result<(String, usize), EditError> {
    let us = units(src);
    get(&us, src, u, old)?;
    let sib = us.siblings(u);
    if before.is_some_and(|b| !sib.contains(&b) || b == Entry::Unit(u)) {
        return Err(EditError::Unsupported);
    }
    let (cut_r, sep) = cut(&us, src, u);
    // 지운 뒤의 원문에서 넣을 자리를 다시 잰다 — 지운 범위 뒤의 자리는 그만큼 당겨진다.
    let shift = |p: usize| if p >= cut_r.end { p - cut_r.len() } else { p };
    let removed = splice(src, cut_r.clone(), "");
    let (at, ins) = match before {
        Some(b) => {
            let at = shift(us.entry_range(b).start);
            (at, format!("{old}{sep}"))
        }
        None => {
            let last = sib
                .iter()
                .rev()
                .find(|e| **e != Entry::Unit(u))
                .ok_or(EditError::Unsupported)?;
            let at = shift(us.entry_range(*last).end);
            (at, format!("{sep}{old}"))
        }
    };
    let head = if before.is_some() { at } else { at + sep.len() };
    let out = splice(&removed, at..at, &ins);
    if out == src {
        return Err(EditError::Unsupported);
    }
    Ok((out, head))
}

/// 위(`up`)나 아래의 보이는 이웃과 자리를 바꾼다.
pub fn shift(src: &str, u: usize, old: &str, up: bool) -> Result<(String, usize), EditError> {
    let us = units(src);
    get(&us, src, u, old)?;
    let sib: Vec<Entry> = us
        .siblings(u)
        .into_iter()
        .filter(|e| us.visible(*e) || *e == Entry::Unit(u))
        .collect();
    let me = sib
        .iter()
        .position(|e| *e == Entry::Unit(u))
        .ok_or(EditError::Stale)?;
    if up {
        let to = me.checked_sub(1).ok_or(EditError::Unsupported)?;
        move_to(src, u, old, Some(sib[to]))
    } else {
        if me + 1 >= sib.len() {
            return Err(EditError::Unsupported);
        }
        move_to(src, u, old, sib.get(me + 2).copied())
    }
}

/// 항목 `u` 뒤(하위 목록 뒤)에 같은 수준의 빈 항목을 넣는다 — 손잡이의 `+`. 새 항목의 내용
/// 자리를 돌려준다.
pub fn new_item_after(src: &str, u: usize, old: &str) -> Result<(String, usize), EditError> {
    let us = units(src);
    let unit = get(&us, src, u, old)?;
    if unit.kind != UnitKind::Item {
        return Err(EditError::Unsupported);
    }
    let line = &src[unit.range.start..line_end(src, unit.range.start)];
    let (indent, marker_end, _) = item_head(line).ok_or(EditError::Unsupported)?;
    let marker = &line[indent..marker_end];
    let next = match marker.strip_suffix(['.', ')']) {
        Some(n) => format!(
            "{}{}",
            n.parse::<u64>().map_err(|_| EditError::Unsupported)? + 1,
            &marker[marker.len() - 1..]
        ),
        None => marker.to_string(),
    };
    let mut ins = format!("\n{}{next} ", &line[..indent]);
    if unit.task {
        ins.push_str("[ ] ");
    }
    let at = unit.range.end;
    Ok((splice(src, at..at, &ins), at + ins.len()))
}

/// 최상위 단위 `u` 뒤에 새 블록 `text` 를 빈 줄로 띄워 넣는다 — 손잡이 `+` 의 새 줄 입력칸.
/// 새 블록의 머리를 돌려준다.
pub fn insert_after_unit(
    src: &str,
    u: usize,
    old: &str,
    text: &str,
) -> Result<(String, usize), EditError> {
    let us = units(src);
    let unit = get(&us, src, u, old)?;
    let text = text.trim_end_matches('\n');
    if unit.list.is_some() || text.trim().is_empty() {
        return Err(EditError::Unsupported);
    }
    let at = unit.range.end;
    let next_blank = {
        let rest = src.get(at + 1..).unwrap_or("");
        at >= src.len() || rest.is_empty() || rest.starts_with('\n')
    };
    let mut ins = format!("\n\n{text}");
    if !next_blank {
        ins.push('\n');
    }
    Ok((splice(src, at..at, &ins), at + 2))
}

/// 항목 첫 줄의 표지(들여쓰기 · 글머리나 번호 · 체크 상자)와 그 뒤 내용의 시작.
fn item_head(line: &str) -> Option<(usize, usize, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let marker = if rest.starts_with(['-', '*', '+']) {
        1
    } else {
        let d = rest.bytes().take_while(u8::is_ascii_digit).count();
        if d == 0 || d > 9 || !rest[d..].starts_with(['.', ')']) {
            return None;
        }
        d + 1
    };
    let after = &rest[marker..];
    let sp = after.len() - after.trim_start_matches(' ').len();
    if sp == 0 && !after.is_empty() {
        return None;
    }
    let content = indent + marker + sp.min(4);
    let tail = &line[content..];
    let task = if tail.starts_with("[ ] ") || tail.starts_with("[x] ") || tail.starts_with("[X] ") {
        4
    } else {
        0
    };
    Some((indent, indent + marker, content + task))
}

/// 형식을 바꾼다 — 첫 줄 머리의 표지만 바꾸고 내용 바이트는 그대로 둔다.
///
/// 문단 · 제목은 무엇으로든 바꾼다(여러 줄 문단은 제목이 될 수 없다). 항목은 목록 형식(글머리 ·
/// 번호 · 체크박스) 사이만 바꾼다 — 목록 밖 형식으로 바꾸면 목록이 갈라지며 앞뒤 빈 줄이 새로
/// 필요해 그 항목 줄 밖이 바뀐다. 하위 블록을 가진 항목은 표지 폭이 바뀌면 하위 줄의 들여쓰기가
/// 어긋나므로 같은 폭일 때만 바꾼다.
pub fn convert(src: &str, u: usize, old: &str, to: Format) -> Result<String, EditError> {
    let us = units(src);
    let unit = get(&us, src, u, old)?;
    let first_end = line_end(src, unit.range.start);
    let line = &src[unit.range.start..first_end];
    let multiline = first_end < unit.range.end;
    let (head, prefix): (Range<usize>, String) = match unit.kind {
        UnitKind::Para | UnitKind::Heading(_) => {
            let content = match unit.kind {
                UnitKind::Heading(_) => {
                    let t = line.trim_start_matches(' ');
                    let hashes = t.len() - t.trim_start_matches('#').len();
                    if hashes == 0 {
                        return Err(EditError::Unsupported); // setext
                    }
                    let after = &t[hashes..];
                    (line.len() - t.len()) + hashes + (after.len() - after.trim_start().len())
                }
                _ => line.len() - line.trim_start_matches(' ').len(),
            };
            if multiline && matches!(to, Format::H1 | Format::H2 | Format::H3) {
                return Err(EditError::Newline);
            }
            let p = match to {
                Format::Body => "",
                Format::H1 => "# ",
                Format::H2 => "## ",
                Format::H3 => "### ",
                Format::Bullet => "- ",
                Format::Numbered => "1. ",
                Format::Task => "- [ ] ",
                Format::Quote => "> ",
            };
            (unit.range.start..unit.range.start + content, p.to_string())
        }
        UnitKind::Item => {
            let (indent, marker_end, content) = item_head(line).ok_or(EditError::Unsupported)?;
            let ind = &line[..indent];
            let marker = &line[indent..marker_end];
            let ordered = marker.ends_with(['.', ')']);
            let bullet = if ordered { "-" } else { marker };
            let number = if ordered { marker } else { "1." };
            let p = match to {
                Format::Bullet => format!("{ind}{bullet} "),
                Format::Numbered => format!("{ind}{number} "),
                Format::Task => {
                    let m = if ordered { number } else { bullet };
                    format!("{ind}{m} [ ] ")
                }
                _ => return Err(EditError::Unsupported),
            };
            if multiline {
                let old_w = line[..content].len() - if unit.task { 4 } else { 0 };
                let new_w = p.len() - if to == Format::Task { 4 } else { 0 };
                if old_w != new_w {
                    return Err(EditError::Unsupported);
                }
            }
            (unit.range.start..unit.range.start + content, p)
        }
        _ => return Err(EditError::Unsupported),
    };
    let out = splice(src, head, &prefix);
    if out == src {
        return Err(EditError::Unsupported);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\ntitle: x\n---\n\n# 할 일\n\n- [ ] a\n- b\n  - b1\n  - b2\n- c\n\n문단 하나\n둘째 줄\n\n> [!agent] 10:00 기록\n> 내용\n\n```\ncode\n```\n\n[ref]: https://example.com\n\n## 끝\n";

    fn find(us: &Units, src: &str, starts: &str) -> usize {
        us.units
            .iter()
            .position(|u| src[u.range.clone()].starts_with(starts))
            .unwrap_or_else(|| panic!("no unit {starts:?}"))
    }

    fn changed_lines(a: &str, b: &str) -> (Vec<String>, Vec<String>) {
        let (al, bl): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
        let gone = al
            .iter()
            .filter(|l| !bl.contains(l))
            .map(|s| s.to_string())
            .collect();
        let new = bl
            .iter()
            .filter(|l| !al.contains(l))
            .map(|s| s.to_string())
            .collect();
        (gone, new)
    }

    #[test]
    fn units_cover_top_level_blocks_and_list_items() {
        let us = units(DOC);
        let kinds: Vec<String> = us.units.iter().map(|u| u.kind.code()).collect();
        assert_eq!(
            kinds,
            [
                "none", "h1", "li", "li", "li", "li", "li", "p", "quote", "code", "h2"
            ],
            "{:#?}",
            us.units
        );
        let b = find(&us, DOC, "- b");
        assert_eq!(&DOC[us.units[b].range.clone()], "- b\n  - b1\n  - b2");
        assert!(us.units[find(&us, DOC, "- [ ] a")].task);
        assert_eq!(us.lists.len(), 2);
        assert_eq!(us.lists[1].parent, Some(b));
    }

    #[test]
    fn shift_swaps_whole_units_and_keeps_every_other_byte() {
        let us = units(DOC);
        // 하위 목록째 아래로.
        let b = find(&us, DOC, "- b");
        let (out, _) = shift(DOC, b, &DOC[us.units[b].range.clone()], false).unwrap();
        assert_eq!(
            out,
            DOC.replace("- b\n  - b1\n  - b2\n- c", "- c\n- b\n  - b1\n  - b2")
        );
        // 맨 위 항목은 위로 못 간다.
        let a = find(&us, DOC, "- [ ] a");
        assert_eq!(shift(DOC, a, "- [ ] a", true), Err(EditError::Unsupported));
        // 최상위 문단을 위로 — 목록을 통째로 넘는다.
        let p = find(&us, DOC, "문단");
        let (out, head) = shift(DOC, p, "문단 하나\n둘째 줄", true).unwrap();
        assert_eq!(
            out,
            DOC.replace(
                "- [ ] a\n- b\n  - b1\n  - b2\n- c\n\n문단 하나\n둘째 줄",
                "문단 하나\n둘째 줄\n\n- [ ] a\n- b\n  - b1\n  - b2\n- c"
            )
        );
        assert!(out[head..].starts_with("문단 하나"));
        // 마지막 제목을 위로 — 보이지 않는 링크 정의 줄은 제자리.
        let h = find(&us, DOC, "## 끝");
        let (out, _) = shift(DOC, h, "## 끝", true).unwrap();
        let (gone, new) = changed_lines(DOC, &out);
        assert!(gone.is_empty() && new.is_empty(), "{gone:?} {new:?}");
        assert!(out.contains("[ref]: https://example.com"));
        assert!(out.find("## 끝").unwrap() < out.find("```\ncode").unwrap());
    }

    #[test]
    fn delete_and_duplicate_touch_only_the_unit_lines() {
        let us = units(DOC);
        let q = find(&us, DOC, "> [!agent]");
        let out = delete(DOC, q, "> [!agent] 10:00 기록\n> 내용").unwrap();
        assert_eq!(out, DOC.replace("> [!agent] 10:00 기록\n> 내용\n\n", ""));
        let c = find(&us, DOC, "- c");
        let (out, head) = duplicate(DOC, c, "- c").unwrap();
        assert_eq!(out, DOC.replace("- c\n", "- c\n- c\n"));
        assert_eq!(&out[head..head + 3], "- c");
        // 마지막 단위를 지우면 앞 구분을 함께 지운다.
        let h = find(&us, DOC, "## 끝");
        assert_eq!(
            delete(DOC, h, "## 끝").unwrap(),
            DOC.replace("\n\n## 끝", "")
        );
        assert_eq!(delete(DOC, h, "## 다름"), Err(EditError::Stale));
        // 하위 목록의 하나뿐인 항목을 지워도 다음 줄의 들여쓰기는 그대로다.
        const NEST: &str = "- a\n    * only\n  * next\n";
        let nu = units(NEST);
        let only = find(&nu, NEST, "    * only");
        assert_eq!(delete(NEST, only, "    * only").unwrap(), "- a\n  * next\n");
        // 문서 끝 단위를 지워도 앞 줄 끝 공백(강제 줄바꿈)은 남는다.
        const HARD: &str = "줄  \n\n# 끝\n";
        let hu = units(HARD);
        let h = find(&hu, HARD, "# 끝");
        assert_eq!(delete(HARD, h, "# 끝").unwrap(), "줄  \n");
        // 링크 정의 줄 바로 앞의 단위를 지워도 정의 줄은 앞 블록과 빈 줄로 띄워진 채 남는다.
        let c = find(&us, DOC, "```");
        assert_eq!(
            delete(DOC, c, "```\ncode\n```").unwrap(),
            DOC.replace("```\ncode\n```\n\n", "")
        );
    }

    #[test]
    fn move_to_places_a_unit_before_a_sibling_or_at_the_end() {
        let us = units(DOC);
        let a = find(&us, DOC, "- [ ] a");
        let (out, _) = move_to(DOC, a, "- [ ] a", None).unwrap();
        assert_eq!(
            out,
            DOC.replace(
                "- [ ] a\n- b\n  - b1\n  - b2\n- c",
                "- b\n  - b1\n  - b2\n- c\n- [ ] a"
            )
        );
        // 다른 그릇으로는 못 옮긴다.
        let b1 = find(&us, DOC, "  - b1");
        assert_eq!(
            move_to(DOC, a, "- [ ] a", Some(Entry::Unit(b1))),
            Err(EditError::Unsupported)
        );
    }

    #[test]
    fn plus_adds_a_sibling_item_or_a_block_after_the_unit() {
        let us = units(DOC);
        let b = find(&us, DOC, "- b");
        let (out, at) = new_item_after(DOC, b, "- b\n  - b1\n  - b2").unwrap();
        assert_eq!(out, DOC.replace("  - b2\n", "  - b2\n- \n"));
        assert_eq!(&out[at - 2..at], "- ");
        let a = find(&us, DOC, "- [ ] a");
        let (out, _) = new_item_after(DOC, a, "- [ ] a").unwrap();
        assert_eq!(out, DOC.replace("- [ ] a\n", "- [ ] a\n- [ ] \n"));
        let q = find(&us, DOC, "> [!agent]");
        let (out, at) =
            insert_after_unit(DOC, q, "> [!agent] 10:00 기록\n> 내용", "새 문단").unwrap();
        assert_eq!(out, DOC.replace("> 내용\n", "> 내용\n\n새 문단\n"));
        assert!(out[at..].starts_with("새 문단"));
        const OL: &str = "1. a\n2. b\n";
        let us = units(OL);
        let (out, _) = new_item_after(OL, us.units.len() - 1, "2. b").unwrap();
        assert_eq!(out, "1. a\n2. b\n3. \n");
    }

    #[test]
    fn convert_changes_only_the_head_marker() {
        let us = units(DOC);
        let h = find(&us, DOC, "# 할 일");
        assert_eq!(
            convert(DOC, h, "# 할 일", Format::H3).unwrap(),
            DOC.replace("# 할 일", "### 할 일")
        );
        assert_eq!(
            convert(DOC, h, "# 할 일", Format::Body).unwrap(),
            DOC.replace("# 할 일", "할 일")
        );
        let p = find(&us, DOC, "문단");
        assert_eq!(
            convert(DOC, p, "문단 하나\n둘째 줄", Format::Quote).unwrap(),
            DOC.replace("문단 하나", "> 문단 하나")
        );
        assert_eq!(
            convert(DOC, p, "문단 하나\n둘째 줄", Format::H2),
            Err(EditError::Newline)
        );
        let c = find(&us, DOC, "- c");
        assert_eq!(
            convert(DOC, c, "- c", Format::Task).unwrap(),
            DOC.replace("- c", "- [ ] c")
        );
        assert_eq!(
            convert(DOC, c, "- c", Format::Numbered).unwrap(),
            DOC.replace("- c", "1. c")
        );
        assert_eq!(
            convert(DOC, c, "- c", Format::H2),
            Err(EditError::Unsupported)
        );
        let a = find(&us, DOC, "- [ ] a");
        assert_eq!(
            convert(DOC, a, "- [ ] a", Format::Bullet).unwrap(),
            DOC.replace("- [ ] a", "- a")
        );
        // 하위 목록이 있는 항목은 표지 폭이 같을 때만.
        let b = find(&us, DOC, "- b");
        let old = "- b\n  - b1\n  - b2";
        assert_eq!(
            convert(DOC, b, old, Format::Numbered),
            Err(EditError::Unsupported)
        );
        let q = find(&us, DOC, "> [!agent]");
        assert_eq!(
            convert(DOC, q, "> [!agent] 10:00 기록\n> 내용", Format::Body),
            Err(EditError::Unsupported)
        );
    }
}
