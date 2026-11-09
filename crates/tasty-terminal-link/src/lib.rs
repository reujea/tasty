//! 터미널 텍스트에서 OSC 8 링크·URL·파일 경로를 찾고 강조할 셀 범위를 제공한다.
//! OSC 8은 셀에 붙은 URI를 사용하고 일반 텍스트는 정규식으로 찾는다.
//! 좌표는 표시 컬럼 단위이며 scrollback과 현재 화면을 모두 처리한다.
//! 링크를 여는 OS 동작은 호스트가 맡으므로 이 크레이트는 GUI 없이 사용할 수 있다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use termwiz::cell::CellAttributes;

use tasty_cell_width::unicode_width;

/// 링크가 걸쳐 있는 화면상 한 행의 컬럼 범위.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSegment {
    /// 절대 row (scrollback 0 = 가장 오래된 라인).
    pub absolute_row: usize,
    /// 시작 컬럼(포함).
    pub start_col: usize,
    /// 끝 컬럼(포함).
    pub end_col: usize,
}

/// 검출된 링크 1건. 단일 행이면 `segments`가 원소 1개, wrap 으로 여러 화면
/// 행에 걸치면 화면 순서(위→아래)대로 여러 개 — 각 세그먼트는 그 행 안에서의
/// 정확한 컬럼 범위를 담는다.
#[derive(Debug, Clone)]
pub struct LinkSpan {
    pub segments: Vec<LinkSegment>,
    /// 링크 대상 URI.
    pub uri: String,
}

impl LinkSpan {
    /// 단일 행짜리 span 생성 (기존 `detect_scrollback_line`/`detect_screen_line`
    /// 등 한 행만 보는 검출 경로용).
    fn single(absolute_row: usize, start_col: usize, end_col: usize, uri: String) -> Self {
        Self {
            segments: vec![LinkSegment {
                absolute_row,
                start_col,
                end_col,
            }],
            uri,
        }
    }

    /// (col, absolute_row)가 이 링크 범위 안에 있는지 — 세그먼트 중 하나라도 덮으면 true.
    pub fn contains(&self, col: usize, absolute_row: usize) -> bool {
        self.segments
            .iter()
            .any(|s| s.absolute_row == absolute_row && col >= s.start_col && col <= s.end_col)
    }

    /// `absolute_row` 행에서의 컬럼 범위(있으면).
    fn range_at(&self, absolute_row: usize) -> Option<(usize, usize)> {
        self.segments
            .iter()
            .find(|s| s.absolute_row == absolute_row)
            .map(|s| (s.start_col, s.end_col))
    }
}

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // 스킴 + 호스트/경로. URL 끝에 붙을 수 있는 구두점(. , ; : ) ] } " ' !) 은 후처리에서 제거.
        Regex::new(r"(?i)\b(?:https?|ftp|file)://[^\s<>\[\]\{\}\\^`|]+").expect("URL regex compile")
    })
}

/// 스키마 없는 경로 후보를 찾는 regex.
///
/// - Unix 절대: `/foo/bar`
/// - Windows 절대: `C:\...` 또는 `C:/...`
/// - 상대(접두사 있음): `./foo`, `../foo`
/// - 상대(접두사 없음): `src/main.rs`, `crates/x/Cargo.toml` — 구분자(`/`, Windows 는 `\` 포함)
///   가 1개 이상 포함된 토큰만. 단어 하나(`Makefile`)는 제외.
///
/// 후보일 뿐이며 실제 파일 존재 여부는 별도로 검증한다. 접두사 없는 상대경로는
/// 슬래시 prefilter(이 regex) + cwd 기준 `exists()`(resolve_path) 2단으로 오탐을 억제한다.
fn path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // 접두사 없는 상대경로(rel_bare)의 경로 구분자: Windows 는 `/` 와 `\` 모두,
        // 그 외 플랫폼은 `/` 만(리눅스/macOS 에서 `\` 는 정상 파일명 문자라 산문 오탐 방지).
        #[cfg(windows)]
        let rel_bare = r"(?P<rel_bare>[A-Za-z0-9._\-]+(?:[\\/][A-Za-z0-9._\-]+)+)";
        #[cfg(not(windows))]
        let rel_bare = r"(?P<rel_bare>[A-Za-z0-9._\-]+(?:/[A-Za-z0-9._\-]+)+)";

        // 접두사가 있는 상대·절대 경로를 먼저 검사한다. ASCII 경로 후보만 추출한다.
        let pattern = format!(
            r#"(?x)
            (?:
              (?:^|[\s"'(\[<])                              # 앞 경계(시작/공백/구두점)
              (?P<rel>\.{{1,2}}/[A-Za-z0-9._\-/]+)          # ./foo, ../foo/bar
            )
            |
            (?:
              (?:^|[\s"'(\[<])
              (?P<unix>/[A-Za-z0-9._\-/]+)                  # /foo/bar
            )
            |
            (?:
              \b(?P<win>[A-Za-z]:[\\/][A-Za-z0-9._\-\\/]+)  # C:\foo, C:/foo
            )
            |
            (?:
              (?:^|[\s"'(\[<])
              {rel_bare}                                    # src/main.rs, crates/x/Cargo.toml
            )
            "#
        );
        Regex::new(&pattern).expect("path regex compile")
    })
}

/// URL 뒤에 딸려 붙기 쉬운 구두점을 잘라낸다.
/// 예: "see https://a.com/x." → "https://a.com/x"
fn trim_trailing_punct(s: &str) -> &str {
    let trimmed = s.trim_end_matches(['.', ',', ';', ':', '!', '?']);
    // 닫는 괄호는 짝이 맞을 때만 보존.
    let mut end = trimmed.len();
    for (ch, open) in [(')', '('), (']', '['), ('}', '{'), ('>', '<')] {
        while trimmed[..end].ends_with(ch) {
            let inner = &trimmed[..end - 1];
            let opens = inner.chars().filter(|c| *c == open).count();
            let closes = inner.chars().filter(|c| *c == ch).count();
            if closes >= opens {
                end -= 1;
            } else {
                break;
            }
        }
    }
    &trimmed[..end]
}

/// scrollback 라인(Vec<(String, CellAttributes)>)에서 링크를 검출한다.
pub fn detect_scrollback_line(
    line: &[(String, CellAttributes)],
    absolute_row: usize,
    cwd: Option<&Path>,
    mirror: bool,
) -> Vec<LinkSpan> {
    // 1) OSC 8 hyperlink 수집 (연속된 동일 uri 셀을 하나로 묶음).
    let mut result = Vec::new();
    let mut col_of_byte: Vec<usize> = Vec::with_capacity(line.len());
    let mut text = String::new();
    let mut col: usize = 0;
    let mut current: Option<(String, usize, usize)> = None; // (uri, start_col, end_col)
    for (cell_text, attrs) in line {
        col_of_byte.extend(std::iter::repeat_n(col, cell_text.len()));
        text.push_str(cell_text);
        let ch = cell_text.chars().next().unwrap_or(' ');
        let width = unicode_width(ch);
        let end_col = col + width.saturating_sub(1);
        let uri = attrs.hyperlink().map(|h| h.uri().to_string());
        match (&mut current, uri) {
            (Some(cur), Some(u)) if cur.0 == u => cur.2 = end_col,
            (Some(cur), Some(u)) => {
                result.push(LinkSpan::single(absolute_row, cur.1, cur.2, cur.0.clone()));
                *cur = (u, col, end_col);
            }
            (Some(cur), None) => {
                result.push(LinkSpan::single(absolute_row, cur.1, cur.2, cur.0.clone()));
                current = None;
            }
            (None, Some(u)) => current = Some((u, col, end_col)),
            (None, None) => {}
        }
        col += width;
    }
    if let Some(cur) = current {
        result.push(LinkSpan::single(absolute_row, cur.1, cur.2, cur.0));
    }

    // 2) 일반 텍스트에서 URL regex 검출. OSC8 범위와 겹치지 않는 경우만 추가.
    append_regex_matches(&text, &col_of_byte, absolute_row, &mut result);
    // 3) 스키마 없는 경로 (CWD 기준 exists 검증; mirror 면 검증 건너뜀).
    append_path_matches(&text, &col_of_byte, absolute_row, cwd, mirror, &mut result);
    result
}

/// screen 라인(termwiz Line)에서 링크를 검출한다.
pub fn detect_screen_line(
    line: &termwiz::surface::line::Line,
    absolute_row: usize,
    cwd: Option<&Path>,
    mirror: bool,
) -> Vec<LinkSpan> {
    let mut result = Vec::new();
    let mut text = String::new();
    let mut col_of_byte: Vec<usize> = Vec::new();
    let mut current: Option<(String, usize, usize)> = None;
    for cell_ref in line.visible_cells() {
        let col = cell_ref.cell_index();
        let cell_text = cell_ref.str();
        col_of_byte.extend(std::iter::repeat_n(col, cell_text.len()));
        text.push_str(cell_text);
        let ch = cell_text.chars().next().unwrap_or(' ');
        let width = unicode_width(ch);
        let end_col = col + width.saturating_sub(1);
        let uri = cell_ref.attrs().hyperlink().map(|h| h.uri().to_string());
        match (&mut current, uri) {
            (Some(cur), Some(u)) if cur.0 == u => cur.2 = end_col,
            (Some(cur), Some(u)) => {
                result.push(LinkSpan::single(absolute_row, cur.1, cur.2, cur.0.clone()));
                *cur = (u, col, end_col);
            }
            (Some(cur), None) => {
                result.push(LinkSpan::single(absolute_row, cur.1, cur.2, cur.0.clone()));
                current = None;
            }
            (None, Some(u)) => current = Some((u, col, end_col)),
            (None, None) => {}
        }
    }
    if let Some(cur) = current {
        result.push(LinkSpan::single(absolute_row, cur.1, cur.2, cur.0));
    }

    append_regex_matches(&text, &col_of_byte, absolute_row, &mut result);
    append_path_matches(&text, &col_of_byte, absolute_row, cwd, mirror, &mut result);
    result
}

fn append_regex_matches(
    text: &str,
    col_of_byte: &[usize],
    absolute_row: usize,
    out: &mut Vec<LinkSpan>,
) {
    for m in url_regex().find_iter(text) {
        let raw = m.as_str();
        let trimmed = trim_trailing_punct(raw);
        if trimmed.is_empty() {
            continue;
        }
        let start_byte = m.start();
        let end_byte = start_byte + trimmed.len();
        // col_of_byte는 셀 첫 바이트에만 column을 찍고 나머지는 동일한 값이 쌓임.
        // end_byte - 1에 해당하는 문자의 끝 column을 구해야 정확함.
        let Some(&start_col) = col_of_byte.get(start_byte) else {
            continue;
        };
        let last_byte = end_byte.saturating_sub(1);
        let Some(&last_start_col) = col_of_byte.get(last_byte) else {
            continue;
        };
        // 마지막 셀의 width 계산: 원문에서 해당 char를 얻어야 함.
        let end_ch = text[..end_byte].chars().next_back().unwrap_or(' ');
        let end_col = last_start_col + unicode_width(end_ch).saturating_sub(1);

        // 이미 OSC8이 덮고 있으면 스킵.
        let overlap = out.iter().any(|s| {
            s.range_at(absolute_row)
                .is_some_and(|(s_start, s_end)| !(end_col < s_start || start_col > s_end))
        });
        if overlap {
            continue;
        }
        out.push(LinkSpan::single(
            absolute_row,
            start_col,
            end_col,
            trimmed.to_string(),
        ));
    }
}

/// 링크 조회 한 번 동안의 행 읽기. history 행을 처음 찾으면 주변 몇 행을 한 구간으로 읽어
/// 두어, wrap 체인을 위아래로 따라가는 동안 같은 행이나 이웃 행을 다시 읽지 않는다.
/// 조회 한 번(같은 content cut) 안에서만 쓰므로 무효화가 필요 없다.
struct RowReader<'a, 'v> {
    terminal: &'a tasty_terminal::TerminalReadView<'v>,
    scrollback_len: usize,
    history: HashMap<usize, Option<tasty_terminal::ScrollbackLine>>,
    screen: Option<Vec<termwiz::surface::line::Line>>,
}

/// history 행을 처음 읽을 때 함께 읽는 위아래 행 수.
const HISTORY_WINDOW: usize = 2;

impl<'a, 'v> RowReader<'a, 'v> {
    fn new(terminal: &'a tasty_terminal::TerminalReadView<'v>) -> Self {
        Self {
            terminal,
            scrollback_len: terminal.screen_start(),
            history: HashMap::new(),
            screen: None,
        }
    }

    fn history(&mut self, row: usize) -> Option<&tasty_terminal::ScrollbackLine> {
        if !self.history.contains_key(&row) {
            let start = row.saturating_sub(HISTORY_WINDOW);
            let end = (row + HISTORY_WINDOW + 1).min(self.scrollback_len);
            for r in start..end {
                self.history.entry(r).or_insert(None);
            }
            let history = &mut self.history;
            self.terminal
                .for_each_scrollback_line(start..end, |r, line| {
                    history.insert(r, Some(line.clone()));
                });
        }
        self.history.get(&row)?.as_ref()
    }

    fn screen_line(&mut self, screen_row: usize) -> Option<&termwiz::surface::line::Line> {
        let terminal = self.terminal;
        self.screen
            .get_or_insert_with(|| terminal.screen_lines())
            .get(screen_row)
    }
}

/// 주어진 절대 row 하나에서 링크를 검출한다 — scrollback/screen 판별 포함.
fn detect_row_links(
    rows: &mut RowReader<'_, '_>,
    absolute_row: usize,
    cwd: Option<&Path>,
    mirror: bool,
) -> Option<Vec<LinkSpan>> {
    if absolute_row < rows.scrollback_len {
        let line = rows.history(absolute_row)?.to_cells();
        Some(detect_scrollback_line(&line, absolute_row, cwd, mirror))
    } else {
        let screen_row = absolute_row - rows.scrollback_len;
        let line = rows.screen_line(screen_row)?;
        Some(detect_screen_line(line, absolute_row, cwd, mirror))
    }
}

/// 행의 줄바꿈 여부. scrollback은 저장된 wrapped를 쓰고 현재 화면은
/// 오른쪽 끝의 공백 아닌 글리프로 추정한다. tasty-terminal의 캡처와 같은 기준이다.
fn row_wrapped(rows: &mut RowReader<'_, '_>, absolute_row: usize) -> Option<bool> {
    if absolute_row < rows.scrollback_len {
        rows.history(absolute_row).map(|line| line.wrapped)
    } else {
        let screen_row = absolute_row - rows.scrollback_len;
        let line = rows.screen_line(screen_row)?;
        Some(screen_line_soft_wrapped(line, line.len()))
    }
}

/// `absolute_row` 행의 컬럼 수(그 행 자체의 셀 개수 — 과거 리사이즈로 현재
/// 터미널 폭과 다를 수 있는 scrollback 행도 정확히 반영).
fn row_cols(rows: &mut RowReader<'_, '_>, absolute_row: usize) -> Option<usize> {
    if absolute_row < rows.scrollback_len {
        rows.history(absolute_row).map(|line| line.cell_count())
    } else {
        let screen_row = absolute_row - rows.scrollback_len;
        rows.screen_line(screen_row).map(|l| l.len())
    }
}

/// screen 라인 소프트 wrap 휴리스틱: 맨 오른쪽 컬럼이 공백 아닌 grapheme 로
/// 채워져 있으면 wrap. `tasty-terminal` 의 scrollback 캡처용 휴리스틱과 동일 기준.
fn screen_line_soft_wrapped(line: &termwiz::surface::line::Line, cols: usize) -> bool {
    if cols == 0 {
        return false;
    }
    for cell in line.visible_cells() {
        let idx = cell.cell_index();
        let width = cell.width().max(1);
        if idx + width == cols {
            let s = cell.str();
            return !s.is_empty() && s.trim() != "";
        }
    }
    false
}

/// OSC8 wrap 체인을 아래 방향으로 병합한다. `span`의 마지막 행이 wrapped 이고
/// 다음 행이 col 0 부터 동일 `uri`의 OSC8 링크로 시작하면 그 행의 세그먼트를
/// 이어붙이고, 그 행도 wrapped 면 계속 내려간다(3행 이상 체인 대응).
fn merge_wrap_chain_down(
    rows: &mut RowReader<'_, '_>,
    cwd: Option<&Path>,
    mirror: bool,
    span: &mut LinkSpan,
) {
    while let Some(last) = span.segments.last() {
        let last_row = last.absolute_row;
        if row_wrapped(rows, last_row) != Some(true) {
            break;
        }
        let next_row = last_row + 1;
        let Some(next_spans) = detect_row_links(rows, next_row, cwd, mirror) else {
            break;
        };
        let next = next_spans.into_iter().find(|s| {
            s.uri == span.uri
                && s.segments
                    .first()
                    .is_some_and(|seg| seg.absolute_row == next_row && seg.start_col == 0)
        });
        let Some(next) = next else { break };
        span.segments.extend(next.segments);
    }
}

/// OSC8 wrap 체인을 위 방향으로 병합한다 — `merge_wrap_chain_down`과 대칭.
/// `span`의 첫 행 바로 위 행이 wrapped(그 행이 현재 행으로 이어짐)이고 동일
/// `uri`의 OSC8 링크로 끝나면 그 행의 세그먼트를 앞에 붙이고 계속 올라간다.
fn merge_wrap_chain_up(
    rows: &mut RowReader<'_, '_>,
    cwd: Option<&Path>,
    mirror: bool,
    span: &mut LinkSpan,
) {
    while let Some(first) = span.segments.first() {
        let first_row = first.absolute_row;
        let Some(prev_row) = first_row.checked_sub(1) else {
            break;
        };
        if row_wrapped(rows, prev_row) != Some(true) {
            break;
        }
        let Some(prev_spans) = detect_row_links(rows, prev_row, cwd, mirror) else {
            break;
        };
        let Some(prev_cols) = row_cols(rows, prev_row) else {
            break;
        };
        let prev = prev_spans.into_iter().find(|s| {
            s.uri == span.uri
                && s.segments
                    .iter()
                    .any(|seg| seg.absolute_row == prev_row && seg.end_col + 1 == prev_cols)
        });
        let Some(prev) = prev else { break };
        let mut merged = prev.segments;
        merged.append(&mut span.segments);
        span.segments = merged;
    }
}

/// 해당 위치의 링크를 찾는다. 인접 행의 wrap 표시와 URI가 같으면 위아래로 합친다.
/// 일반 텍스트 URL·경로는 이어진 행에서 보통 같은 URI로 검출되지 않아 병합되지 않는다.
pub fn link_at(
    terminal: &tasty_terminal::Terminal,
    col: usize,
    absolute_row: usize,
    cwd: Option<&Path>,
    mirror: bool,
) -> Option<LinkSpan> {
    terminal.with_content(|view| link_at_view(&view, col, absolute_row, cwd, mirror))
}

/// Hit test using the same locked content cut as pixel-to-grid conversion.
pub fn link_at_view(
    terminal: &tasty_terminal::TerminalReadView<'_>,
    col: usize,
    absolute_row: usize,
    cwd_ref: Option<&Path>,
    mirror: bool,
) -> Option<LinkSpan> {
    let cwd_ref = terminal.cached_cwd().or(cwd_ref);
    if absolute_row < terminal.first_row() {
        return None;
    }
    let mut rows = RowReader::new(terminal);
    let spans = detect_row_links(&mut rows, absolute_row, cwd_ref, mirror)?;
    let mut found = spans.into_iter().find(|s| s.contains(col, absolute_row))?;
    merge_wrap_chain_down(&mut rows, cwd_ref, mirror, &mut found);
    merge_wrap_chain_up(&mut rows, cwd_ref, mirror, &mut found);
    Some(found)
}

/// 렌더러에 전달하는 링크 하이라이트 정보. hovered된 단일 링크를(wrap 으로
/// 여러 화면 행에 걸치면 그 모든 행을) 해당 셀 범위에 대해 fg/bg 색으로
/// 오버라이드한다.
#[derive(Debug, Clone)]
pub struct LinkHighlight {
    pub epoch: tasty_terminal::ContentEpoch,
    pub segments: Vec<LinkSegment>,
    pub fg: tasty_type_appearance::color::GpuRgba,
    pub bg: tasty_type_appearance::color::GpuRgba,
    pub underline: tasty_type_appearance::color::GpuRgba,
}

impl LinkHighlight {
    pub fn covers(&self, col: usize, absolute_row: usize) -> bool {
        self.segments
            .iter()
            .any(|s| s.absolute_row == absolute_row && col >= s.start_col && col <= s.end_col)
    }
}

/// 경로 후보를 `file://` URI 형식의 String 으로 변환해서 반환.
/// - 절대경로면 그대로 사용.
/// - 상대경로면 `cwd` 기준으로 해석.
/// - `mirror` 가 false(로컬 surface)면 실제 존재하는 경로만 반환(오탐 방지).
/// - `mirror` 가 true(원격 mirror surface)면 화면 경로가 원격 호스트 경로라
///   로컬 `exists()` 검증을 건너뛰고 (원격 cwd 기준 결합한) 경로를 그대로 emit.
fn resolve_link_target(candidate: &str, cwd: Option<&Path>, mirror: bool) -> Option<String> {
    let p = Path::new(candidate);
    // Windows의 Path는 /remote/...를 절대경로로 보지 않으므로 mirror는 Unix 루트도 인정한다.
    let is_abs = p.is_absolute() || (mirror && candidate.starts_with('/'));
    let abs: PathBuf = if is_abs {
        p.to_path_buf()
    } else {
        cwd?.join(p)
    };
    // canonicalize로 심볼릭 링크나 Windows UNC 표기를 바꾸지 않고 원래 경로를 URI로 만든다.
    if !mirror && !abs.exists() {
        return None;
    }
    Some(path_to_file_uri(&abs))
}

/// 절대 경로를 `file://` URI로 변환. Windows 백슬래시는 `/`로 치환.
fn path_to_file_uri(abs: &Path) -> String {
    let s = abs.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        // Windows 드라이브 문자 경로: `C:/...` → `file:///C:/...`
        format!("file:///{s}")
    }
}

/// 경로 후보를 링크로 추가한다. 로컬 경로만 존재 여부를 확인한다.
fn append_path_matches(
    text: &str,
    col_of_byte: &[usize],
    absolute_row: usize,
    cwd: Option<&Path>,
    mirror: bool,
    out: &mut Vec<LinkSpan>,
) {
    for caps in path_regex().captures_iter(text) {
        let m = caps
            .name("rel")
            .or_else(|| caps.name("unix"))
            .or_else(|| caps.name("win"))
            .or_else(|| caps.name("rel_bare"));
        let Some(m) = m else { continue };
        let raw = m.as_str();
        let trimmed = trim_trailing_punct(raw);
        if trimmed.is_empty() {
            continue;
        }
        let Some(uri) = resolve_link_target(trimmed, cwd, mirror) else {
            continue;
        };
        let start_byte = m.start();
        let end_byte = start_byte + trimmed.len();
        let Some(&start_col) = col_of_byte.get(start_byte) else {
            continue;
        };
        let last_byte = end_byte.saturating_sub(1);
        let Some(&last_start_col) = col_of_byte.get(last_byte) else {
            continue;
        };
        let end_ch = text[..end_byte].chars().next_back().unwrap_or(' ');
        let end_col = last_start_col + unicode_width(end_ch).saturating_sub(1);
        let overlap = out.iter().any(|s| {
            s.range_at(absolute_row)
                .is_some_and(|(s_start, s_end)| !(end_col < s_start || start_col > s_end))
        });
        if overlap {
            continue;
        }
        out.push(LinkSpan::single(absolute_row, start_col, end_col, uri));
    }
}

/// 선택 문자열 또는 가장 긴 접두사가 파일·폴더 경로인지 확인한다.
/// 전체 문자열로 시작해 실패할 때마다 마지막 / 앞까지 줄인다. 정규식으로 먼저 자르면
/// 비ASCII 부분이 사라져 의도한 접두사 검사가 달라질 수 있다.
pub fn longest_existing_selection_path(
    raw_selected_text: &str,
    cwd: Option<&Path>,
    mirror: bool,
) -> Option<PathBuf> {
    let mut candidate = trim_trailing_punct(raw_selected_text.trim());
    loop {
        if candidate.is_empty() {
            return None;
        }
        if let Some(path) = resolve_selection_path_candidate(candidate, cwd, mirror) {
            return Some(path);
        }
        candidate = &candidate[..candidate.rfind('/')?];
    }
}

/// 선택 경로를 PathBuf로 반환한다. 상대경로는 cwd에 붙이고 mirror는 exists 검사를 생략한다.
fn resolve_selection_path_candidate(
    candidate: &str,
    cwd: Option<&Path>,
    mirror: bool,
) -> Option<PathBuf> {
    let p = Path::new(candidate);
    let is_abs = p.is_absolute() || (mirror && candidate.starts_with('/'));
    let abs: PathBuf = if is_abs {
        p.to_path_buf()
    } else {
        cwd?.join(p)
    };
    if !mirror && !abs.exists() {
        return None;
    }
    Some(abs)
}

#[cfg(test)]
mod disk_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_hit_test_uses_the_anchored_row_after_front_trim() {
        let mut terminal = tasty_terminal::Terminal::new_detached(40, 3);
        terminal.set_scrollback_limit(3);
        terminal.feed_bytes(b"zero\r\nhttps://example.com\r\ntwo\r\nthree\r\nfour");
        let mut viewport = tasty_terminal::TerminalViewport::LIVE;
        viewport.scroll_up(terminal.content_cut(), 1);
        terminal.feed_bytes(b"\r\nfive\r\nsix");
        terminal.with_view(&viewport, |view| {
            let row = view.viewport().top_row;
            let link = link_at_view(&view, 3, row, None, false).unwrap();
            assert_eq!(link.uri, "https://example.com");
            assert!(link.contains(3, row));
            assert!(link_at_view(&view, 3, 0, None, false).is_none());
        });
    }

    #[test]
    fn path_regex_matches_common_forms() {
        let re = path_regex();
        let cases = [
            ("open ./src/main.rs now", "./src/main.rs"),
            ("see /etc/passwd", "/etc/passwd"),
            ("at ../foo/bar", "../foo/bar"),
            ("C:\\Users\\a.txt file", "C:\\Users\\a.txt"),
            ("C:/Users/a.txt yay", "C:/Users/a.txt"),
        ];
        for (input, expected) in cases {
            let caps = re
                .captures(input)
                .unwrap_or_else(|| panic!("no match: {input}"));
            let m = caps
                .name("rel")
                .or_else(|| caps.name("unix"))
                .or_else(|| caps.name("win"))
                .unwrap();
            assert_eq!(m.as_str(), expected, "input: {input}");
        }
    }

    #[test]
    fn detects_bare_relative_path() {
        let re = path_regex();
        let caps = re
            .captures("open src/main.rs now")
            .expect("bare relative path should match");
        assert_eq!(caps.name("rel_bare").unwrap().as_str(), "src/main.rs");

        let caps = re
            .captures("see crates/x/Cargo.toml here")
            .expect("nested bare relative path should match");
        assert_eq!(
            caps.name("rel_bare").unwrap().as_str(),
            "crates/x/Cargo.toml"
        );
    }

    #[test]
    fn rejects_single_word_without_separator() {
        let re = path_regex();
        assert!(
            re.captures("see Makefile here").is_none(),
            "single word without separator must not match"
        );
    }

    #[test]
    fn bare_relative_does_not_steal_prefixed_or_absolute() {
        let re = path_regex();
        let prefixed = re.captures("open ./src/main.rs now").unwrap();
        assert!(prefixed.name("rel").is_some());
        assert!(prefixed.name("rel_bare").is_none());

        let unix = re.captures("see /etc/passwd").unwrap();
        assert!(unix.name("unix").is_some());
        assert!(unix.name("rel_bare").is_none());
    }

    #[test]
    fn slash_non_path_rejected_by_exists() {
        assert!(resolve_link_target("and/or", Some(std::path::Path::new(".")), false).is_none());
        assert!(resolve_link_target("TCP/IP", Some(std::path::Path::new(".")), false).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn detects_bare_relative_path_with_backslash() {
        let re = path_regex();
        let caps = re
            .captures("open src\\main.rs now")
            .expect("windows bare relative path should match");
        assert_eq!(caps.name("rel_bare").unwrap().as_str(), "src\\main.rs");
    }

    #[test]
    fn path_to_file_uri_forms() {
        assert_eq!(
            path_to_file_uri(std::path::Path::new("/home/user/a.txt")),
            "file:///home/user/a.txt"
        );
        assert_eq!(
            path_to_file_uri(std::path::Path::new("C:\\Users\\a.txt")),
            "file:///C:/Users/a.txt"
        );
    }

    #[test]
    fn resolve_path_rejects_nonexistent() {
        assert!(resolve_link_target("/definitely/not/a/real/path/xyz123", None, false).is_none());
        assert!(
            resolve_link_target(
                "./nope_does_not_exist_xyz",
                Some(std::path::Path::new(".")),
                false
            )
            .is_none()
        );
    }

    #[test]
    fn mirror_emits_path_without_exists_check() {
        let cwd = std::path::Path::new("/remote/project");
        let uri = resolve_link_target("src/main.rs", Some(cwd), true);
        assert_eq!(uri.as_deref(), Some("file:///remote/project/src/main.rs"));
        assert!(resolve_link_target("src/main.rs", Some(cwd), false).is_none());
    }

    #[test]
    fn mirror_absolute_path_without_cwd() {
        let uri = resolve_link_target("/remote/abs/file.rs", None, true);
        assert_eq!(uri.as_deref(), Some("file:///remote/abs/file.rs"));
    }

    #[test]
    fn link_span_contains_checks_all_segments() {
        let span = LinkSpan {
            uri: "https://example.com/a/b".into(),
            segments: vec![
                LinkSegment {
                    absolute_row: 5,
                    start_col: 10,
                    end_col: 19,
                },
                LinkSegment {
                    absolute_row: 6,
                    start_col: 0,
                    end_col: 7,
                },
            ],
        };
        assert!(span.contains(10, 5));
        assert!(span.contains(19, 5));
        assert!(span.contains(0, 6));
        assert!(!span.contains(9, 5), "5행 시작 컬럼 이전은 범위 밖");
        assert!(
            !span.contains(0, 5),
            "다른 행 컬럼은 그 행에 매치되면 안 됨"
        );
        assert!(!span.contains(8, 6), "6행 끝 컬럼 다음은 범위 밖");
    }

    #[test]
    fn link_highlight_covers_checks_all_segments() {
        let highlight = LinkHighlight {
            epoch: Default::default(),
            segments: vec![
                LinkSegment {
                    absolute_row: 5,
                    start_col: 10,
                    end_col: 19,
                },
                LinkSegment {
                    absolute_row: 6,
                    start_col: 0,
                    end_col: 7,
                },
            ],
            fg: tasty_type_appearance::color::GpuRgba::dangerously_force_from_array([
                0.0, 0.0, 0.0, 1.0,
            ]),
            bg: tasty_type_appearance::color::GpuRgba::dangerously_force_from_array([
                0.0, 0.0, 0.0, 1.0,
            ]),
            underline: tasty_type_appearance::color::GpuRgba::dangerously_force_from_array([
                0.0, 0.0, 0.0, 1.0,
            ]),
        };
        assert!(highlight.covers(15, 5));
        assert!(highlight.covers(3, 6));
        assert!(!highlight.covers(15, 6), "행이 다르면 매치되면 안 됨");
        assert!(!highlight.covers(20, 5));
    }

    #[test]
    fn trims_trailing_punct() {
        assert_eq!(trim_trailing_punct("https://a.com/x."), "https://a.com/x");
        assert_eq!(trim_trailing_punct("https://a.com/x)."), "https://a.com/x");
        assert_eq!(
            trim_trailing_punct("https://a.com/(x)"),
            "https://a.com/(x)"
        );
        assert_eq!(
            trim_trailing_punct("https://a.com/x?q=1!"),
            "https://a.com/x?q=1"
        );
    }

    #[test]
    fn selection_path_full_candidate_used_when_it_exists() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = longest_existing_selection_path("Cargo.toml", Some(cwd), false);
        assert_eq!(result, Some(cwd.join("Cargo.toml")));
    }

    #[test]
    fn selection_path_trims_back_to_last_slash_when_full_candidate_missing() {
        // 저장소 배치와 무관하게, 존재하는 경로 뒤의 비ASCII 부분을 자르는지 확인한다.
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(tmp.path().join("notes")).expect("fixture dir");
        let cwd = tmp.path();
        let result = longest_existing_selection_path("notes/에", Some(cwd), false);
        assert_eq!(result, Some(cwd.join("notes")));
    }

    #[test]
    fn selection_path_trims_repeatedly_across_multiple_slash_boundaries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("a/b/c")).expect("fixture dirs");
        let cwd = tmp.path();
        // 존재하는 a/b/c에 도달하려면 끝의 두 부분을 잘라야 한다.
        let result = longest_existing_selection_path("a/b/c/bogus_one/bogus_two", Some(cwd), false);
        assert_eq!(result, Some(cwd.join("a/b/c")));
    }

    #[test]
    fn selection_path_none_when_no_prefix_exists() {
        assert!(
            longest_existing_selection_path(
                "totally/bogus/path/xyz123",
                Some(Path::new(".")),
                false
            )
            .is_none()
        );
    }

    #[test]
    fn selection_path_mirror_skips_exists_check_like_resolve_link_target() {
        let cwd = Path::new("/remote/project");
        let result = longest_existing_selection_path("src/main.rs", Some(cwd), true);
        assert_eq!(result, Some(cwd.join("src/main.rs")));
    }
}
