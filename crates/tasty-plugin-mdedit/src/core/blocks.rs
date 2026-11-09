//! 미리보기 직접 편집의 편집 단위 — 원문에서 블록을 뽑고, 블록 하나만 잘라 붙인다.
//!
//! 미리보기는 공유 렌더러의 결과라(docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일) 요소에 원문 위치가 없다. 그래서 여기서 원문을
//! 같은 옵션으로 다시 파싱해 편집 단위마다 **원문 범위**와 **렌더 결과에 보일 글자**를 낸다.
//! 웹뷰 스크립트가 같은 맥락의 요소를 순서대로 짝짓고 글자가 같을 때만 편집을 연다
//! (docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각). 짝이 안 맞는 요소는 편집 대상이 아니다 — 원문 모드로 고친다.
//!
//! 편집 단위는 넷이다: 최상위 문단 · ATX 제목 · 목록 항목의 첫 줄 · 최상위 표의 셀.
//! 인용(`> [!agent]` 포함)·각주·목록 안의 제목·표·문단 이어짐 줄은 단위가 아니다.
//!
//! 모든 연산은 [`super::ops`] 와 같이 **원문 바이트를 잘라 붙이기만** 한다 — 지정 범위 밖의
//! 바이트는 한 개도 바뀌지 않는다.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// 렌더러와 같은 파서 옵션. 렌더러의 옵션 함수는 그 모듈 비공개라 여기서 같은 집합을 적는다 —
/// 어긋나면 블록 수·체크박스 수·글자(스마트 따옴표)가 렌더 결과와 달라지고, 그 경우 짝짓기·
/// 토글이 거부한다. `parser_options_match_the_renderer` 가 렌더러 소스와 대조한다.
pub fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_GFM
        | Options::ENABLE_MATH
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Para,
    Heading,
    Item,
    Cell,
}

impl Kind {
    /// 웹뷰 스크립트가 쓰는 짧은 이름.
    pub fn code(self) -> &'static str {
        match self {
            Self::Para => "p",
            Self::Heading => "h",
            Self::Item => "li",
            Self::Cell => "td",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfo {
    /// 항목 줄의 들여쓰기(표지 앞 공백).
    pub indent: String,
    /// 다음 항목에 쓸 표지 — 글머리는 같은 문자, 번호는 +1.
    pub next_marker: String,
    pub task: bool,
    /// 항목 전체(하위 목록 포함)의 마지막 줄 끝.
    pub end: usize,
    /// 항목 첫 줄의 시작.
    pub line_start: usize,
    /// 하위 블록이 없는 한 줄 항목.
    pub simple: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellInfo {
    /// 이 셀이 있는 행 줄의 시작·끝(줄바꿈 제외).
    pub row: Range<usize>,
    pub head: bool,
    pub cols: usize,
    /// 구분 줄(`| --- |`)의 끝 — 머리 행 아래에 행을 넣을 자리.
    pub delim_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    /// 편집할 원문 범위(인라인 내용만). 빈 항목·셀이면 빈 범위(넣을 자리).
    pub range: Range<usize>,
    /// 렌더 결과에 보일 글자 — 웹뷰가 요소의 글자와 견준다.
    pub text: String,
    /// 미리보기에서 열 수 있는가. 여러 줄 항목·setext 제목 등은 짝짓기에만 쓰고 열지 않는다.
    pub editable: bool,
    /// 줄바꿈을 받는가(최상위 문단만).
    pub multiline: bool,
    /// 이 블록 뒤에 새 블록을 넣을 자리(그 블록 마지막 줄의 끝). 항목은 `None`.
    pub after: Option<usize>,
    pub item: Option<ItemInfo>,
    pub cell: Option<CellInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// 그 번호의 블록이 없거나 원문이 화면을 그린 뒤 바뀌었다.
    Stale,
    /// 그 블록은 이 연산을 받지 않는다.
    Unsupported,
    /// 줄바꿈을 받지 않는 블록에 줄바꿈이 들어왔다.
    Newline,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stale => "the block changed since the preview was drawn",
            Self::Unsupported => "the block does not take this edit",
            Self::Newline => "a line break is not allowed in this block",
        })
    }
}

struct Acc {
    kind: Kind,
    span: Option<Range<usize>>,
    text: String,
    image: usize,
    /// 체크 항목의 `[ ]` 끝.
    task_end: Option<usize>,
    /// 빈 셀의 넣을 자리.
    empty_at: Option<usize>,
    /// setext 제목 — 짝짓기에만 쓴다(범위가 밑줄 줄을 품지 않는다).
    setext: bool,
    /// 받는 중인가(항목은 첫 줄 뒤 하위 블록이 오면 닫는다).
    open: bool,
}

impl Acc {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            span: None,
            text: String::new(),
            image: 0,
            task_end: None,
            empty_at: None,
            setext: false,
            open: true,
        }
    }

    fn feed(&mut self, ev: &Event, r: &Range<usize>) {
        match ev {
            Event::TaskListMarker(_) => {
                self.task_end = Some(r.end);
                return;
            }
            Event::Text(t) if self.image == 0 => self.text.push_str(t),
            Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => self.text.push_str(t),
            Event::SoftBreak | Event::HardBreak => self.text.push(' '),
            Event::Start(Tag::Image { .. }) => self.image += 1,
            Event::End(TagEnd::Image) => self.image = self.image.saturating_sub(1),
            _ => {}
        }
        self.span = Some(match self.span.take() {
            Some(s) => s.start.min(r.start)..s.end.max(r.end),
            None => r.clone(),
        });
    }
}

enum Frame {
    /// 인용·각주·정의 목록 — 그 안은 편집 단위가 아니다.
    Fence,
    List,
    Item {
        acc: usize,
        start: usize,
        children: bool,
    },
    Para {
        acc: Option<usize>,
    },
    Heading {
        acc: Option<usize>,
    },
    Table {
        cells: Vec<usize>,
        cols: usize,
        head_row: Option<Range<usize>>,
    },
    Row {
        row: Range<usize>,
        head: bool,
    },
    Cell {
        acc: Option<usize>,
    },
    Other,
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(src: &str, at: usize) -> usize {
    src[at..].find('\n').map_or(src.len(), |i| at + i)
}

/// 범위 끝에서 줄바꿈을 걷어낸 마지막 내용 바이트 다음.
fn content_end(src: &str, r: &Range<usize>) -> usize {
    let mut e = r.end;
    while e > r.start && matches!(src.as_bytes()[e - 1], b'\n' | b'\r') {
        e -= 1;
    }
    e
}

/// 항목 표지(`-`·`*`·`+`·`N.`·`N)`)의 길이. 표지가 아니면 `None`.
fn marker_len(rest: &str) -> Option<usize> {
    if rest.starts_with(['-', '*', '+']) {
        return Some(1);
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && digits <= 9 && rest[digits..].starts_with(['.', ')'])).then_some(digits + 1)
}

fn next_marker(marker: &str) -> String {
    let digits: String = marker.chars().take_while(char::is_ascii_digit).collect();
    match digits.parse::<u64>() {
        Ok(n) => format!("{}{}", n + 1, &marker[digits.len()..]),
        Err(_) => marker.to_string(),
    }
}

/// 파서 이벤트를 한 번 훑으며 편집 단위를 모은다.
struct Walker<'s> {
    src: &'s str,
    accs: Vec<Acc>,
    out: Vec<Option<Block>>,
    stack: Vec<Frame>,
}

impl<'s> Walker<'s> {
    fn in_fence(&self) -> bool {
        self.stack.iter().any(|f| matches!(f, Frame::Fence))
    }

    fn in_list(&self) -> bool {
        self.stack
            .iter()
            .any(|f| matches!(f, Frame::List | Frame::Item { .. }))
    }

    /// 열려 있는 받는 자리 — 가장 안쪽 프레임의 것.
    fn current_acc(&self) -> Option<usize> {
        let acc = match self.stack.last()? {
            Frame::Para { acc } | Frame::Heading { acc } | Frame::Cell { acc } => *acc,
            Frame::Item { acc, .. } => Some(*acc),
            _ => None,
        }?;
        self.accs[acc].open.then_some(acc)
    }

    fn feed(&mut self, ev: &Event, r: &Range<usize>) {
        if let Some(a) = self.current_acc() {
            self.accs[a].feed(ev, r);
        }
    }

    fn new_acc(&mut self, kind: Kind) -> usize {
        self.accs.push(Acc::new(kind));
        self.out.push(None);
        self.accs.len() - 1
    }

    /// 항목이 하위 블록을 열면 그 항목의 첫 줄 받기를 닫는다.
    fn close_item_text(&mut self) {
        if let Some(Frame::Item { acc, children, .. }) = self.stack.last_mut() {
            let a = &mut self.accs[*acc];
            if a.span.is_some() || a.task_end.is_some() {
                *children = true;
            }
            a.open = false;
        }
    }

    fn start(&mut self, ev: &Event, tag: &Tag, r: &Range<usize>) {
        let frame = match tag {
            Tag::BlockQuote(_)
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::MetadataBlock(_) => {
                self.close_item_text();
                Frame::Fence
            }
            Tag::List(_) => {
                self.close_item_text();
                Frame::List
            }
            Tag::Item if self.in_fence() => Frame::Other,
            Tag::Item => Frame::Item {
                acc: self.new_acc(Kind::Item),
                start: r.start,
                children: false,
            },
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading { .. } => self.start_heading(r),
            Tag::Table(_) => {
                self.close_item_text();
                if self.in_fence() || self.in_list() {
                    Frame::Other
                } else {
                    Frame::Table {
                        cells: Vec::new(),
                        cols: 0,
                        head_row: None,
                    }
                }
            }
            Tag::TableHead | Tag::TableRow => self.start_row(matches!(tag, Tag::TableHead), r),
            Tag::TableCell => self.start_cell(r),
            Tag::CodeBlock(_) | Tag::HtmlBlock => {
                self.close_item_text();
                Frame::Other
            }
            t if is_inline(t) => {
                self.feed(ev, r);
                return;
            }
            _ => Frame::Other,
        };
        self.stack.push(frame);
    }

    fn start_paragraph(&mut self) -> Frame {
        let item_first = match self.stack.last() {
            Some(Frame::Item { acc, .. })
                if self.accs[*acc].open && self.accs[*acc].span.is_none() =>
            {
                Some(*acc)
            }
            _ => None,
        };
        let acc = if item_first.is_some() {
            item_first
        } else {
            self.close_item_text();
            self.stack.is_empty().then(|| self.new_acc(Kind::Para))
        };
        Frame::Para { acc }
    }

    fn start_heading(&mut self, r: &Range<usize>) -> Frame {
        self.close_item_text();
        if self.in_fence() || self.in_list() {
            return Frame::Heading { acc: None };
        }
        let a = self.new_acc(Kind::Heading);
        self.accs[a].setext = !self.src[r.start..].trim_start_matches(' ').starts_with('#');
        Frame::Heading { acc: Some(a) }
    }

    fn start_row(&mut self, head: bool, r: &Range<usize>) -> Frame {
        let row = line_start(self.src, r.start)..line_end(self.src, r.start);
        if head && let Some(Frame::Table { head_row, .. }) = self.stack.last_mut() {
            *head_row = Some(row.clone());
        }
        Frame::Row { row, head }
    }

    fn start_cell(&mut self, r: &Range<usize>) -> Frame {
        let in_table = self
            .stack
            .iter()
            .rev()
            .nth(1)
            .is_some_and(|f| matches!(f, Frame::Table { .. }));
        if !in_table {
            return Frame::Cell { acc: None };
        }
        let a = self.new_acc(Kind::Cell);
        // 빈 셀의 넣을 자리 — 셀 범위(파이프 제외, 공백 포함)의 앞 공백 하나 뒤.
        self.accs[a].empty_at = Some(r.start + usize::from(!r.is_empty()));
        if let Some(Frame::Table { cells, .. }) = self.stack.iter_mut().rev().nth(1) {
            cells.push(a);
        }
        Frame::Cell { acc: Some(a) }
    }

    fn end(&mut self, ev: &Event, end: &TagEnd, r: &Range<usize>) {
        if is_inline_end(end) {
            self.feed(ev, r);
            return;
        }
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let src = self.src;
        match frame {
            Frame::Para { acc: Some(a) } if self.accs[a].kind == Kind::Para => {
                self.out[a] = Some(finish_para(src, &self.accs[a]));
            }
            // 항목의 첫 문단 — 그 뒤는 항목 첫 줄이 아니다.
            Frame::Para { acc: Some(a) } => self.accs[a].open = false,
            Frame::Heading { acc: Some(a) } => {
                self.out[a] = Some(finish_heading(src, &self.accs[a], r));
            }
            Frame::Item {
                acc,
                start,
                children,
            } => {
                self.out[acc] = Some(finish_item(src, &self.accs[acc], start, children, r));
            }
            Frame::Cell { acc: Some(a) } => self.out[a] = Some(finish_cell(&self.accs[a])),
            Frame::Row { row, head } => self.end_row(row, head),
            Frame::Table {
                cells,
                cols,
                head_row,
            } => self.end_table(&cells, cols, head_row, r),
            _ => {}
        }
    }

    /// 행이 끝나면 그 행의 셀(아직 행 정보가 없는 것)에 행 줄을 적는다.
    fn end_row(&mut self, row: Range<usize>, head: bool) {
        let Some(Frame::Table { cells, cols, .. }) = self.stack.last_mut() else {
            return;
        };
        for &c in cells.iter() {
            if let Some(b) = self.out[c].as_mut()
                && b.cell.is_none()
            {
                b.cell = Some(CellInfo {
                    row: row.clone(),
                    head,
                    cols: 0,
                    delim_end: 0,
                });
            }
        }
        if head {
            *cols = cells.len();
        }
    }

    fn end_table(
        &mut self,
        cells: &[usize],
        cols: usize,
        head_row: Option<Range<usize>>,
        r: &Range<usize>,
    ) {
        let src = self.src;
        let after = line_end(src, content_end(src, r));
        let delim_end = head_row
            .map(|h| line_end(src, (h.end + 1).min(src.len())))
            .unwrap_or(after);
        for &c in cells {
            if let Some(b) = self.out[c].as_mut() {
                b.after = Some(after);
                if let Some(ci) = b.cell.as_mut() {
                    ci.cols = cols;
                    ci.delim_end = delim_end;
                }
            }
        }
    }

    fn leaf(&mut self, ev: &Event, r: &Range<usize>) {
        if self.current_acc().is_some() {
            self.feed(ev, r);
        } else if matches!(ev, Event::Rule) {
            self.close_item_text();
        }
    }
}

/// 원문의 편집 단위를 문서 순서로 낸다.
pub fn blocks(src: &str) -> Vec<Block> {
    let mut w = Walker {
        src,
        accs: Vec::new(),
        out: Vec::new(),
        stack: Vec::new(),
    };
    for (ev, r) in Parser::new_ext(src, parser_options()).into_offset_iter() {
        match &ev {
            Event::Start(tag) => w.start(&ev, tag, &r),
            Event::End(end) => w.end(&ev, end, &r),
            _ => w.leaf(&ev, &r),
        }
    }
    w.out.into_iter().flatten().collect()
}

fn is_inline(t: &Tag) -> bool {
    matches!(
        t,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn is_inline_end(t: &TagEnd) -> bool {
    matches!(
        t,
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image
    )
}

fn base(kind: Kind, range: Range<usize>, text: String) -> Block {
    Block {
        kind,
        range,
        text,
        editable: true,
        multiline: false,
        after: None,
        item: None,
        cell: None,
    }
}

fn finish_para(src: &str, a: &Acc) -> Block {
    let r = a.span.clone().unwrap_or(0..0);
    let mut b = base(Kind::Para, r.clone(), a.text.clone());
    b.multiline = true;
    b.after = Some(line_end(src, r.end));
    b
}

fn finish_heading(src: &str, a: &Acc, whole: &Range<usize>) -> Block {
    let r = a.span.clone().unwrap_or_else(|| {
        let e = content_end(src, whole);
        e..e
    });
    let mut b = base(Kind::Heading, r, a.text.clone());
    b.editable = !a.setext && a.span.is_some();
    b.after = Some(line_end(src, content_end(src, whole)));
    b
}

fn finish_item(src: &str, a: &Acc, start: usize, children: bool, whole: &Range<usize>) -> Block {
    let ls = line_start(src, start);
    let line = &src[ls..line_end(src, ls)];
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent_len..];
    let mlen = marker_len(rest).unwrap_or(0);
    let marker = &rest[..mlen];
    // 비었을 때 넣을 자리: 표지(와 체크박스) 뒤 공백 하나 다음, 줄 끝을 넘지 않는다.
    let after_marker = ls + indent_len + mlen;
    let empty_at = a.task_end.unwrap_or(after_marker);
    let le = line_end(src, ls);
    let empty_at = (empty_at + usize::from(src[empty_at..le].starts_with(' '))).min(le);
    let r = a.span.clone().unwrap_or(empty_at..empty_at);
    let single = !src[r.clone()].contains('\n');
    let end = line_end(src, content_end(src, whole).max(r.end));
    let mut b = base(Kind::Item, r, a.text.clone());
    b.editable = single && mlen > 0;
    b.item = Some(ItemInfo {
        indent: line[..indent_len].to_string(),
        next_marker: next_marker(marker),
        task: a.task_end.is_some(),
        end,
        line_start: ls,
        simple: single && !children && end == le,
    });
    b
}

/// 행 정보(`cell`)는 행이 끝날 때 [`Walker::end_row`] 가 채운다.
fn finish_cell(a: &Acc) -> Block {
    let at = a.empty_at.unwrap_or(0);
    let r = a.span.clone().unwrap_or(at..at);
    base(Kind::Cell, r, a.text.clone())
}

fn get<'a>(blocks: &'a [Block], id: usize, src: &str, old: &str) -> Result<&'a Block, EditError> {
    let b = blocks.get(id).ok_or(EditError::Stale)?;
    if !b.editable || &src[b.range.clone()] != old {
        return Err(EditError::Stale);
    }
    Ok(b)
}

/// 셀 글자 안의 `|` 를 `\|` 로 — 그대로 두면 셀이 갈린다.
fn escape_pipes(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut prev_backslash = false;
    for c in t.chars() {
        if c == '|' && !prev_backslash {
            out.push('\\');
        }
        prev_backslash = c == '\\' && !prev_backslash;
        out.push(c);
    }
    out
}

/// 블록 `id` 의 원문 `old` 를 `new` 로 바꾼 원문.
pub fn replace(src: &str, id: usize, old: &str, new: &str) -> Result<String, EditError> {
    let all = blocks(src);
    let b = get(&all, id, src, old)?;
    if !b.multiline && new.contains('\n') {
        return Err(EditError::Newline);
    }
    let new = if b.kind == Kind::Cell {
        escape_pipes(new)
    } else {
        new.to_string()
    };
    Ok(splice(src, b.range.clone(), &new))
}

fn splice(src: &str, r: Range<usize>, t: &str) -> String {
    let mut out = String::with_capacity(src.len() + t.len());
    out.push_str(&src[..r.start]);
    out.push_str(t);
    out.push_str(&src[r.end..]);
    out
}

/// 줄 끝 `at`(줄바꿈 자리 또는 문서 끝) 뒤에 빈 줄로 띄운 새 블록 `text` 를 넣는다. 다음 줄이
/// 비어 있지 않으면 뒤에도 빈 줄을 둔다. 새 블록 시작 바이트를 함께 돌려준다.
fn insert_block_at(src: &str, at: usize, text: &str) -> (String, usize) {
    let next_line_blank = {
        let rest = src.get(at + 1..).unwrap_or("");
        rest.is_empty() || rest.starts_with('\n') || at >= src.len()
    };
    let mut ins = String::from("\n\n");
    ins.push_str(text);
    if at < src.len() && !next_line_blank {
        ins.push('\n');
    }
    (splice(src, at..at, &ins), at + 2)
}

/// 블록 `id` 뒤(`id = None` 이면 문서 끝)에 새 블록 `text` 를 넣는다.
pub fn insert_after(
    src: &str,
    id: Option<usize>,
    text: &str,
) -> Result<(String, usize), EditError> {
    let text = text.trim_end_matches('\n');
    if text.trim().is_empty() {
        return Err(EditError::Unsupported);
    }
    let at = match id {
        None => content_end(src, &(0..src.len())),
        // 항목 뒤에 넣으면(빈 항목에서 목록을 빠져나갈 때) 그 항목 끝 — 빈 줄로 띄우므로 목록이 나뉜다.
        Some(id) => {
            let b = blocks(src).get(id).cloned().ok_or(EditError::Stale)?;
            b.after
                .or_else(|| b.item.as_ref().map(|i| i.end))
                .ok_or(EditError::Unsupported)?
        }
    };
    if id.is_none() && at == 0 {
        // 빈 문서 — 앞에 띄울 것이 없다.
        let tail = if src.is_empty() { "\n" } else { "" };
        return Ok((format!("{text}{tail}{src}"), 0));
    }
    Ok(insert_block_at(src, at, text))
}

/// 항목 `id` 뒤에 같은 수준의 빈 항목을 넣는다. 새 항목의 내용 자리(바이트)를 돌려준다.
pub fn new_item(src: &str, id: usize, old: &str) -> Result<(String, usize), EditError> {
    let all = blocks(src);
    let b = get(&all, id, src, old)?;
    let it = b.item.as_ref().ok_or(EditError::Unsupported)?;
    let mut line = format!("\n{}{} ", it.indent, it.next_marker);
    if it.task {
        line.push_str("[ ] ");
    }
    let focus = it.end + line.len();
    Ok((splice(src, it.end..it.end, &line), focus))
}

/// 빈 항목에서 Enter 로 목록을 빠져나간 결과(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// 하위 항목이라 한 단계 내어썼다 — 그 항목의 내용 자리.
    Outdent(usize),
    /// 최상위 항목이라 그 줄을 지웠다 — 바로 앞 항목 줄의 머리(그 뒤에 새 줄 입력칸을 연다).
    Removed(Option<usize>),
}

/// 빈 한 줄 항목 `id` 에서 목록을 빠져나간다. 하위 항목이면 부모와 같은 들여쓰기 · 부모의 다음
/// 표지로 한 단계 내어쓴다(그 줄의 머리만 바뀐다). 최상위 항목이면 그 줄을 지운다 — 거기에 쓴
/// 새 문단은 [`insert_after`] 가 빈 줄로 띄워 넣으므로 목록이 그 자리에서 나뉜다.
pub fn exit_item(src: &str, id: usize) -> Result<(String, Exit), EditError> {
    let all = blocks(src);
    let b = all.get(id).ok_or(EditError::Stale)?;
    let it = b.item.as_ref().ok_or(EditError::Unsupported)?;
    if !it.simple || !b.range.is_empty() {
        return Err(EditError::Unsupported);
    }
    if !it.indent.is_empty() {
        let parent = all[..id]
            .iter()
            .rev()
            .filter_map(|x| x.item.as_ref())
            .find(|p| p.indent.len() < it.indent.len())
            .ok_or(EditError::Unsupported)?;
        let mut head = format!("{}{} ", parent.indent, parent.next_marker);
        if it.task {
            head.push_str("[ ] ");
        }
        let out = splice(src, it.line_start..b.range.start, &head);
        return Ok((out, Exit::Outdent(it.line_start + head.len())));
    }
    let prev = all[..id]
        .iter()
        .rev()
        .find_map(|x| x.item.as_ref().filter(|p| p.indent.is_empty()))
        .filter(|p| p.end + 1 == it.line_start || src[p.end..it.line_start].trim().is_empty())
        .map(|p| p.line_start);
    let (out, _) = remove_item(src, id)?;
    Ok((out, Exit::Removed(prev)))
}

/// 빈 한 줄 항목 `id` 를 지운다(Enter 로 목록을 빠져나갈 때).
pub fn remove_item(src: &str, id: usize) -> Result<(String, usize), EditError> {
    let all = blocks(src);
    let b = all.get(id).ok_or(EditError::Stale)?;
    let it = b.item.as_ref().ok_or(EditError::Unsupported)?;
    if !it.simple || !b.range.is_empty() {
        return Err(EditError::Unsupported);
    }
    // 그 줄과 앞 줄바꿈을 지운다(첫 줄이면 뒤 줄바꿈).
    let (s, e) = if it.line_start > 0 {
        (it.line_start - 1, it.end)
    } else {
        (0, (it.end + 1).min(src.len()))
    };
    Ok((splice(src, s..e, ""), s))
}

/// 셀 `id` 의 행 위(`below = false`)나 아래에 빈 행을 넣는다. 머리 행 위에는 못 넣는다(머리가
/// 바뀐다). 새 행 첫 셀의 넣을 자리를 돌려준다.
pub fn add_row(src: &str, id: usize, below: bool) -> Result<(String, usize), EditError> {
    let all = blocks(src);
    let b = all.get(id).ok_or(EditError::Stale)?;
    let c = b.cell.as_ref().ok_or(EditError::Unsupported)?;
    if c.cols == 0 || (c.head && !below) {
        return Err(EditError::Unsupported);
    }
    let row = format!("|{}", "   |".repeat(c.cols));
    let indent_len = {
        let l = &src[c.row.clone()];
        l.len() - l.trim_start_matches(' ').len()
    };
    let indent = &src[c.row.start..c.row.start + indent_len];
    let (at, ins, first) = if c.head {
        let at = c.delim_end;
        (at, format!("\n{indent}{row}"), at + 1 + indent_len + 2)
    } else if below {
        let at = c.row.end;
        (at, format!("\n{indent}{row}"), at + 1 + indent_len + 2)
    } else {
        let at = c.row.start;
        (at, format!("{indent}{row}\n"), at + indent_len + 2)
    };
    Ok((splice(src, at..at, &ins), first))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(bs: &[Block], kind: Kind, text: &str) -> usize {
        bs.iter()
            .position(|b| b.kind == kind && b.text == text)
            .unwrap_or_else(|| panic!("{kind:?} '{text}' not in {bs:#?}"))
    }

    /// 바뀐 줄 번호(0 기준) — 줄 수가 같을 때.
    fn changed_lines(a: &str, b: &str) -> Vec<usize> {
        a.split('\n')
            .zip(b.split('\n'))
            .enumerate()
            .filter(|(_, (x, y))| x != y)
            .map(|(i, _)| i)
            .collect()
    }

    const DOC: &str = "# 2026.10.26 **월**\n\n첫 문단 `코드` 와 [링크](x)\n이어지는 줄\n\n- 항목 [pw:P1]\n  - 하위\n- [ ] 할 일\n1. 하나\n2) 둘\n\n> [!agent] 15:10 기록\n> - 인용 안 항목\n\n```\n- 코드 안\n```\n\n| 이름 | 값 |\n| --- | --- |\n| a |  |\n\n제목\n===\n";

    /// 렌더러(같은 소스를 컴파일한다 — docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일)의 `parser_options` 본문에 적힌 옵션과 이
    /// 사본이 같은 집합인가. 한쪽만 바뀌면 미리보기 글자와 짝짓기 기대 글자가 갈린다.
    #[test]
    fn parser_options_match_the_renderer() {
        let render = include_str!("../../../tasty-plugin-markdown/src/render.rs");
        let start = render
            .find("fn parser_options() -> Options {")
            .expect("renderer options fn");
        let body = &render[start..start + render[start..].find("\n}").expect("fn end")];
        let theirs: std::collections::BTreeSet<&str> = body
            .split("Options::")
            .skip(1)
            .map(|t| {
                t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or("")
            })
            .collect();
        let ours: std::collections::BTreeSet<&str> =
            parser_options().iter_names().map(|(n, _)| n).collect();
        assert!(!theirs.is_empty());
        assert_eq!(
            ours, theirs,
            "mdedit parser options drifted from the renderer"
        );
    }

    #[test]
    fn smart_quotes_are_in_the_expected_text_not_the_source_slice() {
        let src = "- 팀방에 \"출근\" 보내기\n";
        let b = &blocks(src)[0];
        assert_eq!(&src[b.range.clone()], "팀방에 \"출근\" 보내기");
        assert!(b.text.contains('\u{201C}'), "{}", b.text);
    }

    #[test]
    fn units_carry_their_inline_source_and_rendered_text() {
        let bs = blocks(DOC);
        let h = &bs[find(&bs, Kind::Heading, "2026.10.26 월")];
        assert_eq!(&DOC[h.range.clone()], "2026.10.26 **월**");
        let p = &bs[find(&bs, Kind::Para, "첫 문단 코드 와 링크 이어지는 줄")];
        assert_eq!(
            &DOC[p.range.clone()],
            "첫 문단 `코드` 와 [링크](x)\n이어지는 줄"
        );
        assert!(p.multiline);
        let it = &bs[find(&bs, Kind::Item, "항목 [pw:P1]")];
        assert_eq!(&DOC[it.range.clone()], "항목 [pw:P1]");
        assert!(!it.item.as_ref().unwrap().simple, "has a child list");
        let task = &bs[find(&bs, Kind::Item, "할 일")];
        assert_eq!(&DOC[task.range.clone()], "할 일");
        assert!(task.item.as_ref().unwrap().task);
        assert_eq!(
            bs[find(&bs, Kind::Item, "하나")]
                .item
                .as_ref()
                .unwrap()
                .next_marker,
            "2."
        );
        assert_eq!(
            bs[find(&bs, Kind::Item, "둘")]
                .item
                .as_ref()
                .unwrap()
                .next_marker,
            "3)"
        );
        // 인용·코드 안은 단위가 아니다.
        assert!(
            !bs.iter()
                .any(|b| b.text.contains("인용 안") || b.text.contains("코드 안"))
        );
        assert!(
            !bs.iter()
                .any(|b| b.text.contains("[!agent]") || b.text.contains("기록"))
        );
        // 셀 — 빈 셀도 넣을 자리를 갖는다.
        let empty = bs
            .iter()
            .find(|b| b.kind == Kind::Cell && b.text.is_empty())
            .unwrap();
        assert!(empty.range.is_empty());
        assert_eq!(empty.cell.as_ref().unwrap().cols, 2);
        // setext 제목은 짝짓기용으로만 낸다.
        let se = &bs[find(&bs, Kind::Heading, "제목")];
        assert!(!se.editable);
    }

    #[test]
    fn replacing_a_unit_changes_only_its_bytes() {
        let bs = blocks(DOC);
        for (id, b) in bs.iter().enumerate().filter(|(_, b)| b.editable) {
            let old = &DOC[b.range.clone()];
            assert_eq!(replace(DOC, id, old, old).unwrap(), DOC, "identity {id}");
            let new = format!("{old}X");
            let out = replace(DOC, id, old, &new).unwrap();
            assert_eq!(out.len(), DOC.len() + 1);
            let lines = changed_lines(DOC, &out);
            assert_eq!(lines.len(), 1, "block {id} {b:?} changed {lines:?}");
        }
    }

    #[test]
    fn replace_rejects_stale_text_and_line_breaks() {
        let bs = blocks(DOC);
        let id = find(&bs, Kind::Item, "할 일");
        assert_eq!(replace(DOC, id, "다른 글", "x"), Err(EditError::Stale));
        assert_eq!(replace(DOC, id, "할 일", "a\nb"), Err(EditError::Newline));
        assert_eq!(replace(DOC, 999, "", ""), Err(EditError::Stale));
        let cell = bs
            .iter()
            .position(|b| b.kind == Kind::Cell && b.text == "a")
            .unwrap();
        let out = replace(DOC, cell, "a", "x|y").unwrap();
        assert!(out.contains("| x\\|y |  |"), "{out}");
    }

    #[test]
    fn new_item_follows_the_marker_and_skips_children() {
        let bs = blocks(DOC);
        let id = find(&bs, Kind::Item, "항목 [pw:P1]");
        let (out, at) = new_item(DOC, id, "항목 [pw:P1]").unwrap();
        assert!(
            out.contains("- 항목 [pw:P1]\n  - 하위\n- \n- [ ] 할 일"),
            "{out}"
        );
        assert_eq!(&out[at - 2..at], "- ");
        let nb = blocks(&out);
        let fresh = nb
            .iter()
            .find(|b| b.kind == Kind::Item && b.range.start == at)
            .unwrap();
        assert!(fresh.range.is_empty() && fresh.editable);

        let id = find(&bs, Kind::Item, "할 일");
        let (out, at) = new_item(DOC, id, "할 일").unwrap();
        assert!(out.contains("- [ ] 할 일\n- [ ] \n1. 하나"), "{out}");
        assert!(
            blocks(&out)
                .iter()
                .any(|b| b.kind == Kind::Item && b.range.start == at)
        );

        let id = find(&bs, Kind::Item, "하나");
        let (out, _) = new_item(DOC, id, "하나").unwrap();
        assert!(out.contains("1. 하나\n2. \n2) 둘"), "{out}");

        let id = find(&bs, Kind::Item, "하위");
        let (out, _) = new_item(DOC, id, "하위").unwrap();
        assert!(out.contains("  - 하위\n  - \n- [ ] 할 일"), "{out}");
    }

    #[test]
    fn an_empty_new_item_can_be_removed_again() {
        let bs = blocks(DOC);
        let id = find(&bs, Kind::Item, "할 일");
        let (out, at) = new_item(DOC, id, "할 일").unwrap();
        let nb = blocks(&out);
        let fresh = nb
            .iter()
            .position(|b| b.kind == Kind::Item && b.range.start == at)
            .unwrap();
        let (back, _) = remove_item(&out, fresh).unwrap();
        assert_eq!(back, DOC);
        // 내용이 있는 항목은 안 지운다.
        assert_eq!(remove_item(DOC, id), Err(EditError::Unsupported));
    }

    #[test]
    fn insert_after_keeps_neighbours_and_separates_with_blank_lines() {
        let bs = blocks(DOC);
        let h = find(&bs, Kind::Heading, "2026.10.26 월");
        let (out, at) = insert_after(DOC, Some(h), "새 문단").unwrap();
        assert!(
            out.starts_with("# 2026.10.26 **월**\n\n새 문단\n\n첫 문단"),
            "{out}"
        );
        assert_eq!(&out[at..at + "새 문단".len()], "새 문단");
        let src = "a\nb\n";
        let a = blocks(src)
            .iter()
            .position(|b| b.text.starts_with('a'))
            .unwrap();
        let (out, _) = insert_after(src, Some(a), "## X").unwrap();
        assert_eq!(out, "a\nb\n\n## X\n", "paragraph a·b is one block");
        // 다음 줄이 바로 붙어 있으면 뒤에도 빈 줄 — 없으면 새 문단이 그 줄과 한 문단이 된다.
        let (out, _) = insert_after("# h\ntext\n", Some(0), "N").unwrap();
        assert_eq!(out, "# h\n\nN\n\ntext\n");
        assert_eq!(
            blocks(&out).iter().filter(|b| b.kind == Kind::Para).count(),
            2
        );
        let (out, _) = insert_after("x\n", None, "- y").unwrap();
        assert_eq!(out, "x\n\n- y\n");
        let (out, _) = insert_after("x", None, "y").unwrap();
        assert_eq!(out, "x\n\ny");
        let (out, at) = insert_after("", None, "y").unwrap();
        assert_eq!((out.as_str(), at), ("y\n", 0));
        assert_eq!(
            insert_after("x\n", None, "  \n"),
            Err(EditError::Unsupported)
        );
        let p = blocks("p\n\nq\n");
        let (out, _) = insert_after("p\n\nq\n", Some(0), "N").unwrap();
        assert_eq!(out, "p\n\nN\n\nq\n");
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn rows_go_above_or_below_but_never_above_the_head() {
        let bs = blocks(DOC);
        let head = bs
            .iter()
            .position(|b| b.kind == Kind::Cell && b.text == "이름")
            .unwrap();
        let body = bs
            .iter()
            .position(|b| b.kind == Kind::Cell && b.text == "a")
            .unwrap();
        assert_eq!(add_row(DOC, head, false), Err(EditError::Unsupported));
        let (out, at) = add_row(DOC, head, true).unwrap();
        assert!(out.contains("| --- | --- |\n|   |   |\n| a |  |"), "{out}");
        assert!(
            blocks(&out)
                .iter()
                .any(|b| b.kind == Kind::Cell && b.range.start == at)
        );
        let (out, at) = add_row(DOC, body, false).unwrap();
        assert!(out.contains("| --- | --- |\n|   |   |\n| a |  |"), "{out}");
        assert!(
            blocks(&out)
                .iter()
                .any(|b| b.kind == Kind::Cell && b.range.start == at)
        );
        let (out, at) = add_row(DOC, body, true).unwrap();
        assert!(out.contains("| a |  |\n|   |   |\n\n제목"), "{out}");
        assert!(
            blocks(&out)
                .iter()
                .any(|b| b.kind == Kind::Cell && b.range.start == at)
        );
    }
}
