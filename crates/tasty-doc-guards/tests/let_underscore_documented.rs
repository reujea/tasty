//! 제품 코드의 `let _ =`에 값을 버리는 이유를 적었는지 확인한다.
//! 타입을 알 수 없는 텍스트 검사이므로 Result 외의 값도 대상이다.
//! 정책은 docs/dev-guide/error-handling.md의 의도적 무시 절에 있다.
//!
//! 문장 내부·바로 다음 줄 또는 앞의 유의미한 줄에 `//` 주석이 있으면 통과한다.
//! 앞줄을 찾을 때 빈 줄과 속성은 건너뛰지만, 멀리 떨어진 블록 설명은 인정하지 않는다.
//! C.6이 판정기 사본으로 판정할 때 이 범위는 C.6이 허용하는 위치를 포함한다.
//! 사본 없이 정규식으로 판정한 줄은 예외가 있다(error-handling.md). 주석의 타당성은 사람이 검토한다.
//!
//! tests 디렉터리와 벤치마크 타깃, 테스트 아이템과 test를 요구하는 cfg 범위는 제외한다.
//! `all(test, ...)`는 제외하지만 `any(test, ...)`는 제품 코드일 수 있어 제외하지 않는다.
//! 따라서 테스트에서 값을 버려 검증을 빠뜨리는 문제는 이 검사로 찾을 수 없다.
//!
//! 코드 검색에서는 주석·리터럴을 지우고, 사유 검색에서는 리터럴만 지운다.
//! 문자열의 `//`는 사유로 인정하지 않는다. pre-commit C.6은 블록 주석을 코드로 보고,
//! 판정기 사본이 없으면 한 줄 안에서 닫히는 문자열만 지우는 정규식으로 판정하므로 이 검사와 결과가 다를 수 있다.

use std::fs;
use std::path::{Path, PathBuf};

/// (경로, 허용 코드 조각). 해당 조각을 담은 줄만 면제한다.
/// vendor/tiny_http의 상류 코드를 불필요하게 수정하지 않기 위한 예외다(docs/features/webhook/index.md).
/// 같은 파일의 다른 위반은 계속 검사한다. pre-commit C.6은 이 예외를 사용하지 않는다.
const ALLOWLIST: &[(&str, &[&str])] = &[(
    "vendor/tiny_http/src/lib.rs",
    &[
        "let _ = stream.shutdown(Shutdown::Both);",
        "let _ = std::fs::remove_file(path);",
    ],
)];

fn is_allowlisted(entries: &[(&str, &[&str])], rel: &str, line: &str) -> bool {
    entries
        .iter()
        .any(|(path, snippets)| *path == rel && snippets.iter().any(|s| line.contains(s)))
}

const PRUNE_DIRS: &[&str] = &[
    "target",
    "dist",
    "_site",
    ".worktree",
    ".git",
    ".idea",
    // Git 추적 여부와 무관하게 개발 도구의 캐시는 제외한다.
    ".serena",
    ".playwright-mcp",
    "node_modules",
    "assets",
];

/// 경로 성분 어디에 있든 제외할 테스트 디렉터리.
const TEST_DIRS: &[&str] = &["tests", "benches"];

/// 로컬 경로를 문서 인용으로 오인하지 않도록 순회용 이름을 조립한다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

/// 추적되는 숨김 디렉터리도 있으므로 점으로 시작하는 이름을 모두 제외하지 않는다.
fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

fn gather(path: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if p.is_dir() {
            // 이름이 다른 CARGO_TARGET_DIR도 빌드 캐시 표식으로 제외한다.
            if !is_pruned(name) && !tasty_doc_guards::is_build_cache_dir(&p) {
                gather(&p, out);
            }
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_test_path(rel: &str) -> bool {
    rel.split('/').any(|seg| TEST_DIRS.contains(&seg))
}

/// mask_non_code로 주석·리터럴을 지운 줄을 받아 `let _ =` 마다 `let` 의 위치를 돌려준다.
fn let_underscores(code: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0;
    while let Some(off) = code[i..].find("let") {
        let start = i + off;
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let rest = &code[start + 3..];
        if before_ok {
            let trimmed = rest.trim_start();
            if rest.len() != trimmed.len()
                && let Some(after_underscore) = trimmed.strip_prefix('_')
                && after_underscore.trim_start().starts_with('=')
                && !after_underscore.trim_start().starts_with("==")
            {
                out.push(start);
            }
        }
        i = start + 3;
    }
    out
}

/// 식별자를 이루는 바이트. Rust 식별자는 비ASCII 문자(`값`)도 쓸 수 있으므로 UTF-8 의 비ASCII 바이트를
/// 식별자로 본다. 마스킹 사본에서 주석·리터럴 안 문자는 이미 공백이고, Rust 가 공백으로 받는 비ASCII
/// 글자는 `rust_spaces_to_ascii` 가 먼저 ASCII 공백으로 바꾼다.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// test 또는 이를 포함한 all 조건만 테스트 전용으로 본다.
/// any(test, ...)는 다른 조건으로 제품 코드로 사용될 수 있으므로 제외하지 않는다.
fn cfg_requires_test(pred: &str) -> bool {
    let p = pred.trim();
    if p == "test" {
        return true;
    }
    match p.strip_prefix("all(").and_then(|r| r.strip_suffix(')')) {
        Some(inner) => cfg_args(inner).iter().any(|a| cfg_requires_test(a)),
        None => false,
    }
}

/// 최상위 쉼표로만 자른다 — 중첩 괄호 안의 쉼표는 인자 경계가 아니다.
fn cfg_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in inner.chars() {
        match ch {
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// 속성 내용(대괄호 안)이 `test` 이거나 test를 요구하는 `cfg(..)` 인가.
fn is_test_marker(content: &str) -> bool {
    let c: String = content.chars().filter(|ch| !ch.is_whitespace()).collect();
    c == "test"
        || c.strip_prefix("cfg(")
            .and_then(|r| r.strip_suffix(')'))
            .is_some_and(cfg_requires_test)
}

/// Rust 가 공백으로 받지만 `u8::is_ascii_whitespace` 가 아닌 글자(Pattern_White_Space 의 U+000B·U+0085·
/// U+200E·U+200F·U+2028·U+2029)를
/// 같은 바이트 수의 ASCII 공백으로 바꾼다. 바이트 위치는 그대로다.
fn rust_spaces_to_ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{0B}' | '\u{85}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}' => {
                " ".repeat(c.len_utf8())
            }
            _ => c.to_string(),
        })
        .collect()
}

/// 바이트 구간(끝은 제외).
type Span = std::ops::Range<usize>;

const NONE: usize = usize::MAX;

/// 마스킹 사본의 괄호 구조. 주석·리터럴이 공백이므로 남은 괄호는 모두 코드다.
struct Code<'a> {
    b: &'a [u8],
    /// 여는 괄호 위치 → 짝인 닫는 괄호 위치. 닫는 괄호 위치 → 짝인 여는 괄호 위치.
    pair: Vec<usize>,
    /// 각 바이트를 감싸는 가장 안쪽 여는 괄호(닫는 괄호는 자기 짝). 최상위는 `NONE`.
    parent: Vec<usize>,
    /// 문자열·문자 리터럴(따옴표 포함)이 있던 바이트. 마스킹으로 공백이 됐지만 피연산자다.
    lit: Vec<bool>,
    /// `match` 의 팔 목록을 여는 `{`. 검사식이 제어식이면 그룹 머리만으로는 `match` 를 못 찾는다.
    match_body: Vec<bool>,
}

impl<'a> Code<'a> {
    fn new(masked: &'a str) -> Self {
        let mut code = Code::build(masked);
        code.mark_match_bodies();
        code
    }

    /// 괄호 짝과 부모만 채운 구조(리터럴 표시와 `match` 본문 표시는 비어 있다).
    fn build(masked: &'a str) -> Self {
        let b = masked.as_bytes();
        let mut pair = vec![NONE; b.len()];
        let mut parent = vec![NONE; b.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (i, &c) in b.iter().enumerate() {
            parent[i] = stack.last().copied().unwrap_or(NONE);
            match c {
                b'(' | b'[' | b'{' => stack.push(i),
                b')' | b']' | b'}' => {
                    let want = match c {
                        b')' => b'(',
                        b']' => b'[',
                        _ => b'{',
                    };
                    if let Some(&open) = stack.last()
                        && b[open] == want
                    {
                        stack.pop();
                        pair[open] = i;
                        pair[i] = open;
                    }
                }
                _ => {}
            }
        }
        Code {
            b,
            pair,
            parent,
            lit: vec![false; b.len()],
            match_body: vec![false; b.len()],
        }
    }

    /// 모든 `match` 의 본문 `{` 를 표시한다(본문 판별은 `block_body`).
    fn mark_match_bodies(&mut self) {
        let found: Vec<usize> = (0..self.b.len())
            .filter(|&k| self.word_at(k, b"match"))
            .filter_map(|k| {
                let g = self.parent[k];
                let limit = if g == NONE || self.pair[g] == NONE {
                    self.b.len()
                } else {
                    self.pair[g]
                };
                // `(`·`[` 안과 매크로 입력 `name! { .. }` 은 쉼표에서 멈춘다. 그 밖의 `{` 그룹의 종류는 이
                // 표시를 써서 정하므로 여기서는 보지 않는다.
                let list = g != NONE
                    && (self.b[g] != b'{'
                        || self.prev_non_ws(g).is_some_and(|p| self.b[p] == b'!'));
                self.block_body(k, limit, list)
            })
            .collect();
        self.match_body.fill(false);
        for open in found {
            self.match_body[open] = true;
        }
    }

    /// `norm`(`rust_spaces_to_ascii(masked)`) 위에 원문 `raw` 의 리터럴 위치를 더한다.
    /// `masked` 는 `mask_non_code(raw)` 이고 `norm` 과 바이트 길이가 같다.
    fn with_raw(norm: &'a str, masked: &str, raw: &str) -> Self {
        let mut code = Code::build(norm);
        let lits = tasty_doc_guards::source_text::mask_literals(raw);
        // 세 사본은 문자 수가 같다. 리터럴 안 문자는 마스킹 사본에서 공백 한 글자가 된다.
        let mut at = 0;
        for ((r, m), l) in raw.chars().zip(masked.chars()).zip(lits.chars()) {
            let n = m.len_utf8();
            if r != l && !r.is_whitespace() {
                code.lit[at..at + n].fill(true);
            }
            at += n;
        }
        code.mark_match_bodies();
        code
    }

    fn skip_ws(&self, mut i: usize, end: usize) -> usize {
        while i < end && self.b[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    }

    /// `hash` 가 매크로 입력(`name!(..)`·`name![..]`·`name! {..}`·`macro_rules! name {..}`) 안에 있는가.
    /// 매크로 입력은 Rust 문법이 아닌 토큰 열일 수 있어 요소의 끝을 텍스트로 정할 수 없다.
    fn in_macro_input(&self, hash: usize) -> bool {
        let mut g = self.parent[hash];
        while g != NONE {
            if let Some(p) = self.prev_non_ws(g) {
                let bang_after_name = |p: usize| {
                    self.b[p] == b'!'
                        && self
                            .prev_non_ws(p)
                            .is_some_and(|q| is_ident_byte(self.b[q]))
                };
                if bang_after_name(p) {
                    return true;
                }
                if is_ident_byte(self.b[p]) {
                    let mut s0 = p;
                    while s0 > 0 && is_ident_byte(self.b[s0 - 1]) {
                        s0 -= 1;
                    }
                    if self.prev_non_ws(s0).is_some_and(|q| {
                        self.b[q] == b'!' && q >= 11 && self.word_at(q - 11, b"macro_rules")
                    }) {
                        return true;
                    }
                }
            }
            g = self.parent[g];
        }
        false
    }

    /// `body` 에서 아이템 선언(`fn`·`mod`·`impl`·`struct` 등)이 시작하는가. 가시성(`pub`, `pub(..)`)과
    /// `async`·`unsafe`·`extern "..."`·`default` 를 건너뛴다. `const {` 는 블록 식이다.
    fn starts_item(&self, body: usize) -> bool {
        let mut k = body;
        loop {
            let next = |w: &[u8]| self.skip_ws(k + w.len(), self.b.len());
            if self.word_at(k, b"pub") {
                k = next(b"pub");
                if self.b.get(k) == Some(&b'(') && self.pair[k] != NONE {
                    k = self.skip_ws(self.pair[k] + 1, self.b.len());
                }
            } else if let Some(w) = [&b"async"[..], b"unsafe", b"default"]
                .into_iter()
                .find(|w| self.word_at(k, w))
            {
                k = next(w);
            } else if self.word_at(k, b"extern") {
                k = next(b"extern");
                while k < self.b.len() && self.lit[k] {
                    k += 1;
                }
                k = self.skip_ws(k, self.b.len());
            } else {
                break;
            }
        }
        if self.word_at(k, b"const") {
            return self.b.get(self.skip_ws(k + 5, self.b.len())) != Some(&b'{');
        }
        [
            &b"fn"[..],
            b"mod",
            b"impl",
            b"trait",
            b"struct",
            b"enum",
            b"union",
            b"type",
            b"static",
            b"use",
            b"macro_rules",
        ]
        .iter()
        .any(|w| self.word_at(k, w))
    }

    /// `i` 앞의 마지막 비공백 위치.
    fn prev_non_ws(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&k| !self.b[k].is_ascii_whitespace())
    }

    /// `#[` 또는 `#![` 로 시작하는 속성이면 (`[` 위치, `]` 위치).
    fn attr_at(&self, hash: usize, inner: bool) -> Option<(usize, usize)> {
        let open = hash + if inner { 2 } else { 1 };
        if self.b.get(hash) != Some(&b'#')
            || (inner && self.b.get(hash + 1) != Some(&b'!'))
            || self.b.get(open) != Some(&b'[')
        {
            return None;
        }
        let close = self.pair[open];
        (close != NONE).then_some((open, close))
    }

    /// `i` 에 단어 `word` 가 있는가. raw 식별자(`r#match`)는 키워드가 아니므로 맞지 않는다.
    fn word_at(&self, i: usize, word: &[u8]) -> bool {
        self.b[i..].starts_with(word)
            && (i == 0 || !is_ident_byte(self.b[i - 1]))
            && !(i >= 2 && &self.b[i - 2..i] == b"r#")
            && self
                .b
                .get(i + word.len())
                .is_none_or(|&c| !is_ident_byte(c))
    }

    /// 바깥 속성 하나(`hash` 의 `#`)가 속한 묶음의 시작과, 묶음 뒤 요소의 시작.
    fn attr_run(&self, hash: usize) -> (usize, usize) {
        let mut start = hash;
        while let Some(p) = self.prev_non_ws(start) {
            let open = self.pair[p];
            if self.b[p] != b']' || open == NONE || open == 0 || self.b[open - 1] != b'#' {
                break;
            }
            start = open - 1;
        }
        let mut k = hash;
        while let Some((_, close)) = self.attr_at(k, false) {
            k = self.skip_ws(close + 1, self.b.len());
        }
        (start, k)
    }

    /// `open` 그룹의 머리(같은 깊이에서 앞 요소의 끝 뒤부터 `open` 앞까지)에 나오는 단어,
    /// `open` 앞의 이름(뒤에 붙은 제네릭 인자는 건너뛴다), 머리의 시작 위치.
    fn header(&self, open: usize) -> (Vec<&'a str>, &'a str, usize) {
        let mut words = Vec::new();
        let mut last = "";
        let mut k = open;
        let mut start = open;
        while k > 0 {
            k -= 1;
            let c = self.b[k];
            match c {
                b')' | b']' if self.pair[k] != NONE => {
                    k = self.pair[k];
                    start = k;
                    if last.is_empty() {
                        last = ")";
                    }
                    continue;
                }
                b';' | b'{' | b'}' | b',' | b'(' | b'[' => break,
                // 짝인 `<` 가 이름·`::` 뒤에 있는 `>` 만 제네릭 인자의 끝으로 보고 통째로 건너뛴다.
                b'>' if k > 0 && !matches!(self.b[k - 1], b'-' | b'=') => {
                    if let Some(lt) = self.generic_open(k) {
                        k = lt;
                        start = k;
                        continue;
                    }
                }
                _ => {}
            }
            if !c.is_ascii_whitespace() {
                start = k;
            }
            if is_ident_byte(c) {
                let end = k + 1;
                while k > 0 && is_ident_byte(self.b[k - 1]) {
                    k -= 1;
                }
                start = k;
                let w = std::str::from_utf8(&self.b[k..end]).unwrap_or("");
                if last.is_empty() {
                    last = w;
                }
                // raw 식별자(`r#match`)는 키워드로 세지 않는다.
                if !(k >= 2 && &self.b[k - 2..k] == b"r#") {
                    words.push(w);
                }
            } else if !c.is_ascii_whitespace() && c != b':' && last.is_empty() {
                last = "?";
            }
        }
        (words, last, start)
    }

    /// `close` 의 `>` 와 짝인 `<` 가 이름이나 `::` 바로 뒤에 있으면 그 위치. 비교 연산자면 `None`.
    /// 제네릭 인자 안에 나올 수 없는 `=>`·`==`·`&&`·`||`·`..` 를 만나면 비교식으로 본다.
    fn generic_open(&self, close: usize) -> Option<usize> {
        let mut depth = 1;
        let mut j = close;
        while j > 0 {
            j -= 1;
            let pair_with = |c: u8| j > 0 && self.b[j - 1] == c;
            match self.b[j] {
                b'>' if pair_with(b'=') => return None,
                b'=' if pair_with(b'=') => return None,
                b'&' if pair_with(b'&') => return None,
                b'|' if pair_with(b'|') => return None,
                // 범위식 `a < b .. c > d` 의 양 끝은 각각 비교식이다.
                b'.' if pair_with(b'.') => return None,
                b'>' if !pair_with(b'-') => depth += 1,
                b'<' => {
                    depth -= 1;
                    if depth == 0 {
                        let p = self.prev_non_ws(j)?;
                        return (is_ident_byte(self.b[p]) || self.b[p] == b':').then_some(j);
                    }
                }
                b')' | b']' if self.pair[j] != NONE => j = self.pair[j],
                // 제네릭 안의 `{ .. }` 는 `<`·`,` 바로 뒤의 const generic 인자뿐이다.
                b'}' if self.pair[j] != NONE => {
                    let open = self.pair[j];
                    match self.prev_non_ws(open).map(|p| self.b[p]) {
                        Some(b'<' | b',') => j = open,
                        _ => return None,
                    }
                }
                b';' | b'{' | b'}' | b'(' | b'[' => return None,
                _ => {}
            }
        }
        None
    }

    /// `body` 에서 시작한 요소(아이템·문장·필드·인자·match 팔)가 끝나는 위치(제외).
    fn element_end(&self, body: usize) -> usize {
        let g = self.parent.get(body).copied().unwrap_or(NONE);
        let limit = if g == NONE || self.pair[g] == NONE {
            self.b.len()
        } else {
            self.pair[g]
        };
        let (list, type_ctx) = self.group_kind(g);
        let stmt = self.stmt_kind(body);
        // 목록 안의 제어식은 본문과 갈래를 읽은 뒤 목록 규칙(후위 연산자, 쉼표)으로 이어 간다.
        // match 팔(`pat => 값`)과 구조체 리터럴 필드(`a: 값`)는 값 자리의 제어식을 같은 규칙으로 읽는다.
        // 블록 식을 값으로 둔 match 팔은 쉼표 없이 그 블록에서 끝나므로 문장 규칙으로 읽는다.
        // 팔 값은 `match` 본문으로 표시된 그룹에서만, 필드 값은 타입 문맥이 아닌 그 밖의 `{` 목록에서만 찾는다.
        let value = if g == NONE || !list {
            None
        } else if self.match_body[g] {
            Some(true)
        } else if !type_ctx && self.b[g] == b'{' {
            Some(false)
        } else {
            None
        };
        let control = match stmt {
            Stmt::BlockLike => Some((body, if list { Ctx::List } else { Ctx::Stmt })),
            _ => value
                .and_then(|arm| self.value_start(body, limit, arm))
                .filter(|&(v, _)| matches!(self.stmt_kind(v), Stmt::BlockLike))
                .map(|(v, arm)| (v, if arm { Ctx::Arm } else { Ctx::List })),
        };
        let from = match control {
            Some((start, ctx)) => match self.block_like_end(start, limit, ctx) {
                BlockEnd::Done(end) => return end,
                BlockEnd::More(k) => k,
                // 본문을 끝까지 확정하지 못하면 이미 읽은 앞부분도 덮지 않는다(속성만 남긴다).
                BlockEnd::Fail => return body,
            },
            None => body,
        };
        let stmt = if list { Stmt::Other } else { stmt };
        self.scan_element(body, from, limit, list, type_ctx, stmt)
    }

    /// 목록 요소의 값 자리 시작과 그것이 match 팔인가. 팔(`arm`)이면 최상위 `=>` 뒤, 필드면 최상위
    /// `:`(경로 `::`·라벨 제외) 뒤. 괄호는 짝으로, 팔 패턴의 제네릭 인자는 각괄호 짝으로 건너뛴다.
    /// 그 전에 `,`·`;` 를 만나면 `None`.
    fn value_start(&self, body: usize, limit: usize, arm: bool) -> Option<(usize, bool)> {
        let mut angle = 0i32;
        let mut k = body;
        while k < limit {
            match self.b[k] {
                b'(' | b'[' | b'{' if self.pair[k] != NONE => {
                    k = self.pair[k] + 1;
                    continue;
                }
                // 짝 없는 괄호(깨진 입력) 뒤에서는 값 자리를 찾지 않는다.
                b'(' | b'[' | b'{' => return None,
                b'<' if arm && self.opens_angle(k, angle, body) => angle += 1,
                b'>' if angle > 0 && !matches!(self.b[k - 1], b'-' | b'=') => angle -= 1,
                b',' | b';' if angle == 0 => return None,
                b'=' if arm && self.b.get(k + 1) == Some(&b'>') => {
                    return Some((self.skip_ws(k + 2, limit), true));
                }
                b':' if !arm
                    && self.b.get(k + 1) != Some(&b':')
                    && (k == 0 || self.b[k - 1] != b':') =>
                {
                    return Some((self.skip_ws(k + 1, limit), false));
                }
                _ => {}
            }
            k += 1;
        }
        None
    }

    /// `k` 의 `<` 가 각괄호(제네릭 인자·한정 경로)를 여는가: 이미 연 각괄호 안, turbofish(`::<`),
    /// 또는 피연산자 끝 뒤가 아닌 자리(식 처음의 `<Type as Trait>::..`). 피연산자 뒤면 비교 연산자다.
    /// 앞 토큰은 요소 시작 `lo` 아래로 찾지 않는다(앞 속성의 `]` 를 피연산자로 읽지 않게).
    /// 피연산자 뒤 `<<` 의 둘째 `<` 는 시프트 연산자의 일부다.
    fn opens_angle(&self, k: usize, angle: i32, lo: usize) -> bool {
        if k > lo && self.b[k - 1] == b'<' && !self.opens_angle(k - 1, angle, lo) {
            return false;
        }
        angle > 0 || (k >= 2 && &self.b[k - 2..k] == b"::") || self.in_prefix(k, lo)
    }

    /// `k` 가 피연산자 끝 바로 뒤가 아닌 자리(식이 새로 시작하는 자리)인가. `lo` 아래는 보지 않는다.
    fn in_prefix(&self, k: usize, lo: usize) -> bool {
        (lo..k)
            .rev()
            .find(|&j| self.lit[j] || !self.b[j].is_ascii_whitespace())
            .is_none_or(|p| !(self.lit[p] || self.operand_end(p)))
    }

    /// 블록 식 문장(`if`·`match`·`while`·`for`·`loop`·`unsafe {`·`{`)의 끝.
    /// 본문 `{` 의 짝에서 끝나고 `else` 는 다음 갈래로 이어 간다. 마지막 본문 뒤가 `.`·`?` 이면
    /// 식이 이어지므로 `More(그 위치)` 를 돌려 일반 규칙으로 계속 읽게 한다. 어느 갈래에서든 본문이나
    /// 짝을 못 찾으면 `Fail` 이다.
    fn block_like_end(&self, body: usize, limit: usize, ctx: Ctx) -> BlockEnd {
        let list = ctx != Ctx::Stmt;
        let mut k = body;
        loop {
            // 본문을 못 찾으면(Rust 식으로 읽히지 않는 입력) 실패다. 앞 갈래나 머리의 무엇을 덮든 제품
            // 코드를 테스트로 숨길 수 있으므로, 테스트 자리를 제품으로 보고 위반으로 드러내는 쪽을 고른다.
            let Some(open) = self.block_body(k, limit, list) else {
                return BlockEnd::Fail;
            };
            let close = self.pair[open];
            if close == NONE || close >= limit {
                return BlockEnd::Fail;
            }
            let next = self.skip_ws(close + 1, limit);
            if next < limit && self.word_at(next, b"else") {
                k = self.skip_ws(next + 4, limit);
                continue;
            }
            return match self.b.get(next) {
                _ if ctx == Ctx::List => BlockEnd::More(close + 1),
                Some(b'.' | b'?') if next < limit => BlockEnd::More(close + 1),
                Some(b';') if next < limit => BlockEnd::Done(next + 1),
                _ => BlockEnd::Done(close + 1),
            };
        }
    }

    /// `start`(블록 식의 키워드나 `{`)가 여는 본문 `{` 의 위치.
    /// `if let`·`while let` 의 패턴(`=` 까지)과 `for` 의 패턴(`in` 까지) 안의 `{..}` 는 구조체 패턴이고,
    /// 식 자리의 `{` 는 앞이 피연산자의 끝(이름·리터럴·`)`·`]`·`}`·`?`)일 때만 본문이다
    /// (조건·검사식 자리에는 괄호 없는 구조체 리터럴이 올 수 없다). 그 밖의 `{..}` 는 블록 피연산자다.
    /// `list` 이면 제네릭 인자 밖의 `,` 에서 멈춘다(목록의 다음 요소).
    fn block_body(&self, start: usize, limit: usize, list: bool) -> Option<usize> {
        if self.b.get(start) == Some(&b'{') {
            return Some(start);
        }
        let start = self.after_label(start);
        if self.b.get(start) == Some(&b'{') {
            return Some(start);
        }
        for (w, len) in [(&b"unsafe"[..], 6), (b"loop", 4), (b"const", 5)] {
            if self.word_at(start, w) {
                let k = self.skip_ws(start + len, limit);
                return (self.b.get(k) == Some(&b'{')).then_some(k);
            }
        }
        let kw = [&b"if"[..], b"while", b"match", b"for"]
            .into_iter()
            .find(|w| self.word_at(start, w))?;
        let mut pattern = kw == b"for";
        let mut angle = 0i32;
        let mut k = start + kw.len();
        while k < limit {
            let c = self.b[k];
            match c {
                b'(' | b'[' => {
                    // 짝 없는 괄호(깨진 입력)에서는 본문을 찾지 않는다.
                    let close = self.pair[k];
                    if close == NONE {
                        return None;
                    }
                    k = close + 1;
                    continue;
                }
                b'<' if self.opens_angle(k, angle, start) => angle += 1,
                b'>' if angle > 0 && !matches!(self.b[k - 1], b'-' | b'=') => angle -= 1,
                b',' if list && angle == 0 => return None,
                // 피연산자 뒤의 `||` 는 논리합이다.
                b'|' if self.b.get(k + 1) == Some(&b'|') && !self.in_prefix(k, start) => {
                    k += 2;
                    continue;
                }
                // 식 자리에서 여는 클로저의 매개변수(`|a: u8, b: u8|`)는 쉼표·괄호째 건너뛴다.
                b'|' if !pattern && self.in_prefix(k, start) => {
                    k = self.closure_params_end(k, limit)?;
                    continue;
                }
                b'{' => {
                    let close = self.pair[k];
                    if close == NONE {
                        return None;
                    }
                    let prev = (0..k)
                        .rev()
                        .find(|&j| self.lit[j] || !self.b[j].is_ascii_whitespace());
                    if !pattern && prev.is_some_and(|p| self.lit[p] || self.operand_end(p)) {
                        return Some(k);
                    }
                    k = close + 1;
                    continue;
                }
                b';' => return None,
                b'=' if pattern
                    && kw != b"for"
                    && !matches!(self.b.get(k + 1), Some(b'=' | b'>'))
                    && !matches!(
                        k.checked_sub(1).map(|p| self.b[p]),
                        Some(b'=' | b'!' | b'<' | b'>')
                    ) =>
                {
                    pattern = false;
                }
                _ if is_ident_byte(c) && (k == 0 || !is_ident_byte(self.b[k - 1])) => {
                    if self.word_at(k, b"let") {
                        pattern = true;
                    } else if kw == b"for" && pattern && self.word_at(k, b"in") {
                        pattern = false;
                    } else if !pattern
                        && [&b"if"[..], b"match", b"while", b"for", b"loop", b"unsafe"]
                            .iter()
                            .any(|w| self.word_at(k, w))
                    {
                        // 머리 안의 제어식(`match if a { .. } else { .. } {`)은 갈래를 끝까지 읽는다.
                        let ctx = if list { Ctx::List } else { Ctx::Stmt };
                        // 안쪽 제어식을 못 읽으면 바깥 본문도 찾지 않는다.
                        k = match self.block_like_end(k, limit, ctx) {
                            BlockEnd::Done(end) | BlockEnd::More(end) => end.max(k + 1),
                            BlockEnd::Fail => return None,
                        };
                        continue;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        None
    }

    /// 라벨(`'done:`, raw 라벨 `'r#async:`)이 있으면 그 뒤 첫 글자, 없으면 `k`.
    fn after_label(&self, k: usize) -> usize {
        if self.b.get(k) != Some(&b'\'') {
            return k;
        }
        let name = if self.b[k + 1..].starts_with(b"r#") {
            k + 3
        } else {
            k + 1
        };
        let mut e = name;
        while e < self.b.len() && is_ident_byte(self.b[e]) {
            e += 1;
        }
        let colon = self.skip_ws(e, self.b.len());
        if e > name && self.b.get(colon) == Some(&b':') && self.b.get(colon + 1) != Some(&b':') {
            self.skip_ws(colon + 1, self.b.len())
        } else {
            k
        }
    }

    /// `p` 의 글자가 피연산자의 끝인가(키워드가 아닌 이름·숫자, 닫는 괄호, `?`, 제네릭 인자의 `>`).
    /// 리터럴은 호출하는 쪽이 `lit` 로 따로 본다.
    fn operand_end(&self, p: usize) -> bool {
        let c = self.b[p];
        if c == b'>' {
            return self.generic_open(p).is_some();
        }
        if matches!(c, b')' | b']' | b'}' | b'?' | b'"' | b'\'') {
            return true;
        }
        if !is_ident_byte(c) {
            return false;
        }
        let mut s0 = p;
        while s0 > 0 && is_ident_byte(self.b[s0 - 1]) {
            s0 -= 1;
        }
        // raw 식별자(`r#match`)는 키워드가 아니다.
        if s0 >= 2 && &self.b[s0 - 2..s0] == b"r#" {
            return true;
        }
        let word = &self.b[s0..=p];
        ![
            &b"if"[..],
            b"match",
            b"while",
            b"for",
            b"in",
            b"unsafe",
            b"else",
            b"loop",
            b"let",
            b"return",
            b"break",
            b"mut",
            b"ref",
            b"move",
            b"async",
            b"const",
        ]
        .contains(&word)
    }

    /// `from` 부터 요소의 끝을 찾는 일반 규칙(괄호 짝, `;`·`,`, 블록 뒤 이어짐).
    fn scan_element(
        &self,
        body: usize,
        from: usize,
        limit: usize,
        list: bool,
        type_ctx: bool,
        stmt: Stmt,
    ) -> usize {
        let mut angle = 0i32;
        let mut k = from;
        while k < limit {
            let c = self.b[k];
            match c {
                b'(' | b'[' | b'{' => {
                    let close = self.pair[k];
                    // 짝이 없는 괄호(깨진 입력)면 이 요소를 덮지 않는다(속성만 남긴다).
                    if close == NONE || close >= limit {
                        return body;
                    }
                    if c == b'{' {
                        let next = self.skip_ws(close + 1, limit);
                        let continues = match stmt {
                            // `let` 은 `;` 에서만 끝난다.
                            Stmt::Let => true,
                            _ => self.continues(close + 1, next, limit, list),
                        };
                        if !continues && !(list && angle > 0) {
                            // 블록 뒤 `;` 는 같은 문장·선언의 끝이다.
                            return if !list && self.b.get(next) == Some(&b';') {
                                next + 1
                            } else {
                                close + 1
                            };
                        }
                    }
                    k = close + 1;
                    continue;
                }
                b';' if !list => return k + 1,
                // 클로저 매개변수 `|a, b|` 의 쉼표는 구분자가 아니다.
                b'|' if list && self.closure_may_start(body, k) => {
                    match self.closure_params_end(k, limit) {
                        Some(e) => k = e,
                        // 닫는 `|` 를 못 찾으면 이 요소를 덮지 않는다(속성만 남긴다).
                        None => return body,
                    }
                    continue;
                }
                b',' if list && angle == 0 => return k,
                // 타입 문맥, turbofish(`::<`), 이미 연 제네릭 인자 안의 `<` 는 짝을 센다.
                // 타입 문맥, 또는 각괄호를 여는 자리(turbofish·한정 경로·이미 연 각괄호 안).
                b'<' if type_ctx || self.opens_angle(k, angle, body) => angle += 1,
                b'>' if angle > 0 && !(k > 0 && matches!(self.b[k - 1], b'-' | b'=')) => angle -= 1,
                _ => {}
            }
            k += 1;
        }
        limit
    }

    /// 그룹 `g` 가 쉼표 목록인가와, 그 목록이 타입 문맥(각괄호 안 쉼표가 구분자가 아님)인가.
    fn group_kind(&self, g: usize) -> (bool, bool) {
        if g == NONE {
            return (false, false);
        }
        if self.match_body[g] {
            return (true, false);
        }
        let (words, last, _) = self.header(g);
        let has = |w: &str| words.contains(&w);
        let type_ctx = ["struct", "enum", "union", "fn"].iter().any(|w| has(w));
        if self.b[g] != b'{' {
            return (true, type_ctx);
        }
        // match 팔 목록은 `match_body` 표시로만 인정한다(머리의 `match` 단어로 보지 않는다:
        // `match unsafe { .. } { .. }` 의 첫 블록은 문장 그룹이다).
        if ["struct", "enum", "union"].iter().any(|w| has(w)) {
            return (true, type_ctx);
        }
        let literal = (last == "Self" || last.starts_with(|c: char| c.is_ascii_uppercase()))
            && ![
                "impl", "trait", "mod", "fn", "if", "while", "for", "loop", "else", "unsafe",
                "where", "match",
            ]
            .iter()
            .any(|w| has(w));
        (literal, false)
    }

    /// `k` 의 `|` 가 클로저 매개변수를 열 수 있는 자리인가(요소 처음, `move` 뒤, `(`·`,`·`=`·`=>` 뒤).
    fn closure_may_start(&self, body: usize, k: usize) -> bool {
        if k == body {
            return true;
        }
        match self.prev_non_ws(k) {
            Some(p) if p >= body => {
                matches!(self.b[p], b'(' | b',' | b'=')
                    || (self.b[p] == b'>' && p > 0 && self.b[p - 1] == b'=')
                    || (p >= 3 && self.word_at(p - 3, b"move"))
            }
            _ => true,
        }
    }

    /// `k` 의 `|` 로 연 클로저 매개변수의 끝(닫는 `|` 다음). 패턴 안의 괄호는 짝으로 건너뛴다.
    /// 닫는 `|` 를 못 찾으면 `None`. 매개변수 자리에 올 수 없는(또는 드문) `{`·`;`·짝 없는 괄호를
    /// 만나도 `None` 이다(다른 요소의 `|` 를 닫는 기호로 잡지 않게. 구조체 패턴 매개변수
    /// `|S { a }|` 는 읽지 못하고 범위를 좁히는 쪽이 된다).
    fn closure_params_end(&self, k: usize, limit: usize) -> Option<usize> {
        if self.b.get(k + 1) == Some(&b'|') {
            return Some(k + 2);
        }
        let mut e = k + 1;
        while e < limit {
            match self.b[e] {
                b'|' => return Some(e + 1),
                // 매개변수 패턴의 괄호는 짝으로 건너뛰되, 안에 `{`·`;` 가 있으면 패턴으로 보지 않는다.
                b'(' | b'[' if self.pair[e] != NONE => {
                    let close = self.pair[e];
                    if (e + 1..close).any(|j| !self.lit[j] && matches!(self.b[j], b'{' | b';')) {
                        return None;
                    }
                    e = close + 1;
                }
                b'(' | b'[' | b'{' | b';' => return None,
                _ => e += 1,
            }
        }
        None
    }

    /// 문장 그룹의 요소가 `let` 문장인지, 블록 식(`if`·`match`·`loop`·`while`·`for`·`unsafe`·`{`)
    /// 문장인지, 그 밖(아이템·일반 식)인지.
    fn stmt_kind(&self, body: usize) -> Stmt {
        if self.word_at(body, b"let") {
            return Stmt::Let;
        }
        let body = self.after_label(body);
        // `unsafe fn`·`const fn`·`const X` 같은 아이템은 블록 식이 아니다.
        let block_after = |w: &[u8]| {
            self.word_at(body, w)
                && self.b.get(self.skip_ws(body + w.len(), self.b.len())) == Some(&b'{')
        };
        let block_like = self.b.get(body) == Some(&b'{')
            || block_after(b"unsafe")
            || block_after(b"const")
            || [&b"if"[..], b"match", b"loop", b"while", b"for"]
                .iter()
                .any(|w| self.word_at(body, w));
        if block_like {
            Stmt::BlockLike
        } else {
            Stmt::Other
        }
    }

    /// 블록 닫힘 뒤 `next` 의 글자가 같은 식을 이어 가는가(`.`·`?`·`else`·`as`·이항 연산자,
    /// 같은 줄의 호출·인덱싱 `(`·`[`, 문장 그룹에서는 시그니처 안 타입의 `,`).
    /// 문장 그룹에서 줄이 바뀐 뒤의 `*`·`-`·`&`·`|`·`!`·`(`·`[` 는 다음 식의 시작으로 본다.
    fn continues(&self, after: usize, next: usize, limit: usize, list: bool) -> bool {
        if next >= limit {
            return false;
        }
        let c = self.b[next];
        let pair_op = |op: u8| self.b.get(next + 1) == Some(&op);
        // 목록 안에서는 줄바꿈이 식을 끊지 않는다(문장 경계가 없다).
        let newline = !list && self.b[after..next].contains(&b'\n');
        match c {
            b'.' | b'?' | b'+' | b'/' | b'%' | b'^' | b'<' | b'>' | b'=' => true,
            b'&' => !newline || pair_op(b'&'),
            b'|' => !newline || pair_op(b'|'),
            b'-' | b'*' => !newline,
            b'!' => pair_op(b'='),
            b',' => !list,
            b'(' | b'[' => !newline,
            _ => self.word_at(next, b"else") || self.word_at(next, b"as"),
        }
    }

    /// 안쪽 속성(`#![`)이 적용되는 구간. 파일 머리면 파일 전체, `{` 그룹 바로 안이면 그 그룹.
    /// 매크로 입력(`(`·`[` 안)이나 파일 머리가 아닌 최상위는 속성으로 보지 않는다.
    fn inner_attr_span(&self, hash: usize) -> Option<Span> {
        let g = self.parent[hash];
        if g == NONE {
            return self.is_file_head(hash).then_some(0..self.b.len());
        }
        let close = self.pair[g];
        if self.b[g] != b'{' || close == NONE {
            return None;
        }
        // 함수 몸체의 안쪽 속성은 함수 아이템 전체(시그니처 포함)에 붙는다.
        let (words, _, start) = self.header(g);
        let start = if words.contains(&"fn") { start } else { g };
        Some(start..close + 1)
    }

    /// `hash` 앞에 공백과 다른 안쪽 속성만 있는가.
    fn is_file_head(&self, hash: usize) -> bool {
        let mut k = self.skip_ws(0, hash);
        while k < hash {
            let Some((_, close)) = self.attr_at(k, true) else {
                return false;
            };
            k = self.skip_ws(close + 1, hash);
        }
        k == hash
    }
}

/// `block_like_end` 의 결과: 요소의 끝, 일반 규칙으로 이어 읽을 위치, 또는 본문을 확정하지 못함.
#[derive(Clone, Copy)]
enum BlockEnd {
    Done(usize),
    More(usize),
    Fail,
}

/// 블록 식을 읽는 자리. 목록·팔에서는 본문 탐색이 다음 요소의 쉼표를 넘지 않는다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// 문장 그룹의 문장. 본문 뒤 `;`·`.`·`?` 규칙을 쓴다.
    Stmt,
    /// 쉼표 목록의 요소(인자·필드 값). 본문 뒤에는 목록 규칙으로 이어 간다.
    List,
    /// match 팔의 값. 블록 식이면 쉼표 없이 그 블록에서 끝난다.
    Arm,
}

/// 문장 그룹 안 요소의 종류. `}` 뒤에서 요소가 끝나는지 정한다.
#[derive(Clone, Copy)]
enum Stmt {
    Let,
    BlockLike,
    Other,
}

/// 이 lint 의 수준. `deny` 는 안쪽 `allow` 로 풀 수 있어 `warn` 과 같이 다룬다.
/// `forbid` 는 안쪽 `allow` 가 컴파일 오류(E0453)라 풀 수 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Allow,
    Warn,
    Forbid,
}

/// 한 파일의 테스트 구간과 lint 수준 구간.
struct Scopes {
    test: Vec<Span>,
    /// (구간, 수준). 면제(`allow`)와 다시 켬(`warn`·`deny`·`forbid`)을 함께 담는다.
    levels: Vec<(Span, Level)>,
    /// 파일 머리 안쪽 속성이 정한 최종 수준(뒤의 속성이 이긴다). 크레이트 루트 판정에 쓴다.
    head_level: Option<Level>,
}

impl Scopes {
    /// `pos` 를 감싸는 가장 안쪽 lint 수준(표기 없음은 `None`). 시작이 같은 구간(같은 속성 묶음)이면
    /// 뒤의 속성이 이긴다. 감싸는 층 어디에든 `forbid` 가 있으면 안쪽 `allow` 와 관계없이 `Forbid` 다.
    fn level_at(&self, pos: usize) -> Option<Level> {
        let around = || self.levels.iter().filter(|(span, _)| span.contains(&pos));
        if around().any(|(_, l)| *l == Level::Forbid) {
            return Some(Level::Forbid);
        }
        around().max_by_key(|(span, _)| span.start).map(|(_, l)| *l)
    }
}

fn content(code: &Code, open: usize, close: usize) -> String {
    String::from_utf8_lossy(&code.b[open + 1..close])
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// cfg 술어를 테스트 빌드에서 `test` 만 알고 계산한다. OS·feature 처럼 `test` 로 정해지지 않는
/// 부분이 결과를 정하면 `None`.
fn eval_in_test(pred: &str) -> Option<bool> {
    let p = pred.trim();
    if p == "test" {
        return Some(true);
    }
    let args = |name: &str| {
        p.strip_prefix(name)
            .and_then(|r| r.strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
            .map(cfg_args)
    };
    if let Some(a) = args("not") {
        return a.first().and_then(|x| eval_in_test(x)).map(|v| !v);
    }
    if let Some(a) = args("all") {
        let v: Vec<Option<bool>> = a.iter().map(|x| eval_in_test(x)).collect();
        return if v.contains(&Some(false)) {
            Some(false)
        } else if v.iter().all(|x| *x == Some(true)) {
            Some(true)
        } else {
            None
        };
    }
    if let Some(a) = args("any") {
        let v: Vec<Option<bool>> = a.iter().map(|x| eval_in_test(x)).collect();
        return if v.contains(&Some(true)) {
            Some(true)
        } else if v.iter().all(|x| *x == Some(false)) {
            Some(false)
        } else {
            None
        };
    }
    None
}

/// 속성 내용(공백 제거)이 테스트 빌드에서 정하는 이 lint 의 수준. `cfg_attr(P, a, b, ..)` 는 P 가 테스트 빌드에서 거짓으로 정해지면 무시하고,
/// 그 밖(참, 또는 OS·feature 조건이라 알 수 없음)이면 인자를 순서대로 적용해 마지막 수준을 쓴다.
fn lint_level(content: &str) -> Option<Level> {
    if let Some(inner) = content
        .strip_prefix("cfg_attr(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let args = cfg_args(inner);
        let pred = args.first()?;
        if eval_in_test(pred) == Some(false) {
            return None;
        }
        // 한 묶음 안에서도 `forbid` 뒤의 `allow` 는 컴파일 오류라 `forbid` 가 남는다.
        let levels: Vec<Level> = args[1..]
            .iter()
            .filter_map(|a| lint_level(a.trim()))
            .collect();
        return if levels.contains(&Level::Forbid) {
            Some(Level::Forbid)
        } else {
            levels.last().copied()
        };
    }
    for (name, level) in [
        ("allow", Level::Allow),
        ("warn", Level::Warn),
        ("deny", Level::Warn),
        ("forbid", Level::Forbid),
    ] {
        let names = content
            .strip_prefix(name)
            .and_then(|r| r.strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'));
        if let Some(names) = names
            && cfg_args(names)
                .iter()
                .any(|n| n.trim() == "clippy::let_underscore_must_use")
        {
            return Some(level);
        }
    }
    None
}

/// 마스킹 사본 전체에서 테스트 구간(test 표식이 붙은 요소, test cfg 안쪽 속성이 덮는 블록·파일)과
/// lint 수준 구간(면제·다시 켬 속성이 붙은 요소·블록·파일)을 모은다.
fn scopes(masked: &str, raw: &str) -> Scopes {
    let norm = rust_spaces_to_ascii(masked);
    let code = Code::with_raw(&norm, masked, raw);
    let mut s = Scopes {
        test: Vec::new(),
        levels: Vec::new(),
        head_level: None,
    };
    for hash in memchr_all(code.b, b'#') {
        let inner = code.attr_at(hash, true);
        let Some((open, close)) = inner.or_else(|| code.attr_at(hash, false)) else {
            continue;
        };
        let text = content(&code, open, close);
        let marker = is_test_marker(&text);
        let level = lint_level(&text);
        if !marker && level.is_none() {
            continue;
        }
        // 매크로 입력 안에서 아이템이 아닌 요소(식·문장·팔)에 붙은 테스트 표식과 면제는 그 속성만 덮는다
        // (범위를 좁히는 쪽). 아이템(`proptest! { #[test] fn .. }`)은 일반 규칙으로 읽는다. warn·deny·forbid 는
        // 범위를 좁히면 다시 켠 진단을 놓치므로 일반 규칙대로 읽는다.
        let macro_input = inner.is_none()
            && code.in_macro_input(hash)
            && !code.starts_item(code.attr_run(hash).1);
        let span = if inner.is_some() {
            let Some(span) = code.inner_attr_span(hash) else {
                continue;
            };
            if level.is_some() && span == (0..code.b.len()) && s.head_level != Some(Level::Forbid) {
                s.head_level = level;
            }
            span
        } else {
            let (start, body) = code.attr_run(hash);
            start..code.element_end(body).max(body)
        };
        let narrow = code.attr_run(hash).0..close + 1;
        if marker {
            s.test.push(if macro_input {
                narrow.clone()
            } else {
                span.clone()
            });
        }
        if let Some(level) = level {
            let span = if macro_input && level == Level::Allow {
                narrow
            } else {
                span
            };
            s.levels.push((span, level));
        }
    }
    s
}

fn memchr_all(b: &[u8], needle: u8) -> impl Iterator<Item = usize> + '_ {
    b.iter()
        .enumerate()
        .filter(move |(_, c)| **c == needle)
        .map(|(i, _)| i)
}

fn covers(spans: &[Span], pos: usize) -> bool {
    spans.iter().any(|s| s.contains(&pos))
}

/// 각 줄의 시작 바이트 위치. `str::lines` 와 같은 줄 번호를 쓴다.
fn line_starts(masked: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(masked.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// 문장의 마지막 줄 index — 괄호 깊이가 0 으로 돌아오며 `;` 가 나오는 줄.
fn statement_end(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    for (offset, line) in lines.iter().enumerate().skip(start).take(80) {
        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 && line.contains(';') {
            return offset;
        }
    }
    start
}

/// 바로 앞의 유의미한 줄이 `//` 주석인가 — 빈 줄과 `#[..]` 속성은 건너뛴다.
fn preceded_by_comment(lines: &[&str], start: usize) -> bool {
    let mut j = start;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim();
        if t.is_empty() || t.starts_with("#[") || t.starts_with("#![") {
            continue;
        }
        return t.starts_with("//");
    }
    false
}

fn violations_in(text: &str) -> Vec<(usize, String)> {
    let raw: Vec<&str> = text.lines().collect();
    // 코드 탐색과 사유 탐색에 필요한 마스킹 범위가 다르다.
    let code_src = tasty_doc_guards::source_text::mask_non_code(text);
    let lit_src = tasty_doc_guards::source_text::mask_literals(text);
    let code: Vec<&str> = code_src.lines().collect();
    let lit: Vec<&str> = lit_src.lines().collect();
    let tests = scopes(&code_src, text).test;
    let starts = line_starts(&code_src);
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        // 한 줄에 여럿이면 테스트 구간 밖의 것이 하나라도 있을 때 검사한다.
        if !let_underscores(line)
            .iter()
            .any(|&col| !covers(&tests, starts[i] + col))
        {
            continue;
        }
        let end = statement_end(&code, i);
        // 문장 범위 + 그 다음 한 줄 (rustfmt 가 trailing 주석을 다음 줄로 밀어내는 형태).
        let scan_to = (end + 1).min(code.len().saturating_sub(1));
        let inline = lit[i..=scan_to].iter().any(|l| l.contains("//"));
        if inline || preceded_by_comment(&lit, i) {
            continue;
        }
        out.push((i + 1, raw[i].trim().to_string()));
    }
    out
}

/// 순회 실패로 빈 결과가 통과하지 않게 하는 하한이다(docs/dev-guide/guard-verification.md).
/// 2026-09-05에 gather가 수집한 Rust 파일은 1180개였다.
/// 파일 정리를 허용할 여유를 두며, 하한 통과가 수집의 완전성을 보장하지는 않는다.
const MIN_SCANNED_FILES: usize = 700;

fn scan_is_credible(found: usize) -> bool {
    found >= MIN_SCANNED_FILES
}

#[test]
fn the_scan_refuses_to_report_zero_from_an_empty_walk() {
    assert!(!scan_is_credible(0), "빈 스캔을 믿을 만하다고 판정했다");
    assert!(!scan_is_credible(MIN_SCANNED_FILES - 1));
    assert!(scan_is_credible(MIN_SCANNED_FILES));
}

#[test]
fn every_let_underscore_in_production_code_says_why() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut files = Vec::new();
    gather(root, &mut files);
    files.sort();
    assert!(
        scan_is_credible(files.len()),
        "Rust 파일을 {}개만 수집했다(하한 {MIN_SCANNED_FILES}). 경로와 PRUNE_DIRS를 확인한다. `git ls-files '*.rs' | wc -l`로 추적 파일 수와 비교하되 제외 디렉터리의 차이를 고려한다. 실제 수집 범위가 줄었다면 다시 측정한 근거와 함께 하한을 조정한다.",
        files.len()
    );

    let mut report = Vec::new();
    for file in &files {
        let rel = rel_of(file, root);
        if is_test_path(&rel) {
            continue;
        }
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (line, snippet) in violations_in(&text) {
            if is_allowlisted(ALLOWLIST, &rel, &snippet) {
                continue;
            }
            report.push(format!("  {rel}:{line}  {snippet}"));
        }
    }

    assert!(
        report.is_empty(),
        "사유 없이 값을 버리는 `let _` 이 있다 ({} 건).\n{}\n\n\
         왜 값을 버리는지 한 줄 주석을 같은 줄·윗줄·다음 줄 중 한 곳에 단다 \
         (docs/dev-guide/error-handling.md \"의도적 무시\").\n\
         값을 버리면 안 되는 것이었다면 주석 대신 처리하거나 로그를 남긴다.",
        report.len(),
        report.join("\n")
    );
}

const TARGET_EXEMPTION: &str = "#![allow(clippy::let_underscore_must_use)]";

/// 테스트에서만 lint를 면제해 제품 코드의 목록은 유지한다.
const CRATE_EXEMPTION: &str = "#![cfg_attr(test, allow(clippy::let_underscore_must_use))]";

/// 통합 테스트와 벤치는 별도 타깃이므로 크레이트 루트의 면제가 적용되지 않는다.
fn is_own_target(rel: &str) -> bool {
    is_test_path(rel)
}

/// `rel` 이 속한 크레이트의 루트 소스 경로들(있는 것만).
fn crate_roots(root: &Path, rel: &str) -> Vec<PathBuf> {
    let dir = if let Some(rest) = rel.strip_prefix("crates/") {
        match rest.split_once('/') {
            Some((name, _)) => root.join("crates").join(name),
            None => root.to_path_buf(),
        }
    } else {
        root.to_path_buf()
    };
    ["src/lib.rs", "src/main.rs"]
        .iter()
        .map(|n| dir.join(n))
        .filter(|p| p.is_file())
        .collect()
}

/// 테스트 범위 `let _ =` 의 (줄 번호, 그 자리의 lint 수준). 제품 코드 검사와 같은 마스킹·범위 판정을 쓴다.
/// 통합 테스트는 #[test] 밖의 헬퍼도 테스트 코드이므로 whole_file로 포함한다.
fn test_scope_sites(text: &str, whole_file: bool) -> Vec<(usize, Option<Level>)> {
    let code_src = tasty_doc_guards::source_text::mask_non_code(text);
    let s = scopes(&code_src, text);
    let starts = line_starts(&code_src);
    let mut out = Vec::new();
    for (i, line) in code_src.lines().enumerate() {
        for col in let_underscores(line) {
            let pos = starts[i] + col;
            if whole_file || covers(&s.test, pos) {
                out.push((i, s.level_at(pos)));
            }
        }
    }
    out
}

/// 면제되지 않은 자리가 고쳐야 할 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Need {
    /// 감싸는 `warn`·`deny` 가 lint 를 다시 켰다. 그보다 안쪽에 `allow` 를 단다.
    Reenabled,
    /// 감싸는 `forbid` 는 안쪽 `allow` 로 풀 수 없다.
    Forbidden,
    /// 수준 표기가 없다. 파일 머리·감싸는 아이템, 또는 크레이트 루트에 면제를 단다.
    Exemption,
    /// 크레이트 루트 후보의 머리 수준이 `allow`·`forbid` 로 갈려, 이 파일을 컴파일하는 타깃의 수준을
    /// 정할 수 없다.
    RootConflict,
}

/// 크레이트 루트가 물려주는 수준.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Root {
    Known(Option<Level>),
    Conflict,
}

/// 면제되지 않은 자리와 고칠 것. 크레이트 루트 수준(`root`)은 파일 안에 수준 표기가 없는 자리에만
/// 물려준다(안쪽에서 다시 켠 자리는 루트 면제가 있어도 남는다). 루트가 `forbid` 면 안쪽 `allow` 는
/// 컴파일 오류이므로 모든 자리가 `Forbidden` 이다. 루트 후보가 갈리면 `forbid` 아래가 아닌 자리를
/// 판정하지 않고 `RootConflict` 로 알린다.
fn needs(sites: &[(usize, Option<Level>)], root: Root) -> Vec<(usize, Need)> {
    sites
        .iter()
        .filter_map(|&(i, level)| match (root, level) {
            (Root::Known(Some(Level::Forbid)), _) | (_, Some(Level::Forbid)) => {
                Some((i, Need::Forbidden))
            }
            // 루트가 정해지지 않으면 안쪽 `allow` 를 다는 안내도 `forbid` 아래에서는 오류가 된다.
            (Root::Conflict, _) => Some((i, Need::RootConflict)),
            (_, Some(Level::Warn)) => Some((i, Need::Reenabled)),
            (_, Some(Level::Allow)) | (Root::Known(Some(Level::Allow)), None) => None,
            (Root::Known(_), None) => Some((i, Need::Exemption)),
        })
        .collect()
}

/// 크레이트 루트 후보들의 머리 수준. 후보 중 하나라도 면제면 면제(어느 타깃이 이 파일을
/// 컴파일하는지 따지지 않는 근사), 아니면 하나라도 `forbid` 면 `forbid`. 면제와 `forbid` 가 함께 있으면
/// 어느 쪽이 적용되는지 정할 수 없어 `Conflict` 다.
fn root_level_of(heads: &[Option<Level>]) -> Root {
    let has = |l: Level| heads.contains(&Some(l));
    match (has(Level::Allow), has(Level::Forbid)) {
        (true, true) => Root::Conflict,
        (true, false) => Root::Known(Some(Level::Allow)),
        (false, true) => Root::Known(Some(Level::Forbid)),
        (false, false) => Root::Known(None),
    }
}

fn root_level(roots: &[PathBuf]) -> Root {
    let heads: Vec<Option<Level>> = roots
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .map(|t| scopes(&tasty_doc_guards::source_text::mask_non_code(&t), &t).head_level)
        .collect();
    root_level_of(&heads)
}

/// 제품 코드의 값 무시 목록에 테스트가 섞이지 않도록 lint 면제를 확인한다.
/// 타입을 모르는 검사이므로 테스트 범위의 `let _ =`마다 그 자리를 덮는 면제를 요구한다.
/// 인정하는 자리는 파일 머리·감싸는 블록 안쪽의 `#![..]`, 감싸는 아이템·문장의 `#[..]`, 크레이트 루트다.
#[test]
fn test_scope_stays_out_of_the_lint_roster() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut files = Vec::new();
    gather(root, &mut files);
    files.sort();
    assert!(
        scan_is_credible(files.len()),
        "스캔한 `.rs` 가 {}개다(하한 {MIN_SCANNED_FILES}) — 순회가 깨졌다",
        files.len()
    );

    // 경로 기반·cfg 기반 수집을 따로 확인해야 한쪽 실패를 다른 쪽 결과가 가리지 않는다.
    let mut seen_by_path = 0usize;
    let mut seen_by_region = 0usize;
    let mut report = Vec::new();
    for file in &files {
        let rel = rel_of(file, root);
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let own_target = is_own_target(&rel);
        let sites = test_scope_sites(&text, own_target);
        if sites.is_empty() {
            continue;
        }
        if own_target {
            seen_by_path += sites.len();
        } else {
            seen_by_region += sites.len();
        }
        let roots = if own_target {
            Vec::new()
        } else {
            crate_roots(root, &rel)
        };
        let left = needs(&sites, root_level(&roots));
        for kind in [
            Need::Reenabled,
            Need::Forbidden,
            Need::Exemption,
            Need::RootConflict,
        ] {
            let Some(&(line, _)) = left.iter().find(|(_, n)| *n == kind) else {
                continue;
            };
            let fix = match kind {
                Need::Reenabled => "감싸는 아이템·블록이 lint 를 다시 켰다(`warn`·`deny`). 다시 켠 속성보다 안쪽에 `#[allow(clippy::let_underscore_must_use)]` 를 단다".to_string(),
                Need::Forbidden => "감싸는 `forbid(clippy::let_underscore_must_use)` 는 안쪽 `allow` 로 풀 수 없다(E0453). `forbid` 를 `deny` 로 바꾸고 안쪽에 `allow` 를 단다".to_string(),
                Need::Exemption if own_target => format!(
                    "파일 머리에 `{TARGET_EXEMPTION}` 또는 감싸는 아이템에 `#[allow(clippy::let_underscore_must_use)]`"
                ),
                Need::Exemption => {
                    let where_to = roots
                        .first()
                        .map(|p| rel_of(p, root))
                        .unwrap_or_else(|| "(크레이트 루트를 못 찾았다)".to_string());
                    format!("크레이트 루트 `{where_to}` 에 `{CRATE_EXEMPTION}`")
                }
                Need::RootConflict => {
                    let names: Vec<String> = roots.iter().map(|p| rel_of(p, root)).collect();
                    format!(
                        "크레이트 루트 후보 {} 의 머리 수준이 `allow`·`forbid` 로 갈려 이 파일에 적용되는 수준을 정할 수 없다(가드는 어느 타깃이 이 파일을 컴파일하는지 따지지 않는다). `forbid` 를 단 루트의 `forbid` 를 `deny` 로 바꾸고, 다른 후보의 면제에 기대지 말고 이 파일 머리나 감싸는 아이템에 `#[allow(clippy::let_underscore_must_use)]` 를 단다. 파일 안에서 `warn`·`deny` 로 다시 켠 자리는 그 속성보다 안쪽에 단다",
                        names.join("·")
                    )
                }
            };
            report.push(format!("  {rel}:{}  ← {fix}", line + 1));
        }
    }

    assert!(
        seen_by_path > 0,
        "테스트 타깃(`tests/`·`benches/`)에서 `let _` 무시를 한 자리도 못 봤다 — 순회가 깨졌다"
    );
    assert!(
        seen_by_region > 0,
        "프로덕션 파일 안의 `#[cfg(test)]`/`#[test]` 본문에서 `let _` 무시를 한 자리도 \
         못 봤다 — 범위 판정(`scopes`)이 깨졌다"
    );

    report.sort();
    report.dedup();
    assert!(
        report.is_empty(),
        "테스트 코드의 `let _`에 lint 면제가 없다({}곳).\n{}\n제품 코드의 값 무시 목록에 테스트가 섞이지 않도록 줄마다 적은 안내대로 면제를 달거나 lint 수준을 고친다(docs/dev-guide/error-handling.md).",
        report.len(),
        report.join("\n")
    );
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    /// pre-commit C.6에 면제 기능이 없으므로 금지 형태를 런타임에 조립한다.
    fn ignore_stmt() -> String {
        format!("let {}= g();", "_ ")
    }

    fn scan(src: &str) -> Vec<usize> {
        violations_in(src).into_iter().map(|(l, _)| l).collect()
    }

    fn fixture(template: &str) -> Vec<usize> {
        scan(&template.replace("{IGNORE}", &ignore_stmt()))
    }

    /// 줄마다 테스트 구간에 드는 코드 글자가 있는가(시험용 줄 표).
    fn test_lines(masked: &str, raw: &str) -> Vec<bool> {
        let s = scopes(masked, raw);
        let starts = line_starts(masked);
        masked
            .lines()
            .enumerate()
            .map(|(i, line)| {
                line.bytes()
                    .enumerate()
                    .any(|(c, b)| !b.is_ascii_whitespace() && covers(&s.test, starts[i] + c))
            })
            .collect()
    }

    /// 면제 구간(파일 머리·블록 안쪽 `#![..]`, 감싸는 아이템·문장의 `#[..]`)에 들지 않는 테스트 범위 자리.
    fn uncovered_test_sites(text: &str, whole_file: bool) -> Vec<usize> {
        uncovered(&test_scope_sites(text, whole_file), false)
    }

    fn uncovered(sites: &[(usize, Option<Level>)], root_exempt: bool) -> Vec<usize> {
        needs(sites, Root::Known(root_exempt.then_some(Level::Allow)))
            .into_iter()
            .map(|(i, _)| i)
            .collect()
    }

    /// 줄 `line` 의 `let _ =` 가 면제 구간에 드는가.
    fn item_is_exempt(text: &str, line: usize) -> bool {
        let code_src = tasty_doc_guards::source_text::mask_non_code(text);
        let s = scopes(&code_src, text);
        let starts = line_starts(&code_src);
        code_src
            .lines()
            .nth(line)
            .map(let_underscores)
            .and_then(|cols| cols.first().copied())
            .is_some_and(|col| s.level_at(starts[line] + col) == Some(Level::Allow))
    }

    fn with_ignore(template: &str) -> String {
        template.replace("{IGNORE}", &ignore_stmt())
    }

    fn masked_lines(template: &str) -> Vec<bool> {
        let src = with_ignore(template);
        test_lines(&tasty_doc_guards::source_text::mask_non_code(&src), &src)
    }

    const EXEMPT: &str = "#[allow(clippy::let_underscore_must_use)]";

    #[test]
    fn a_test_item_ending_in_a_semicolon_does_not_swallow_the_next_function() {
        assert_eq!(
            fixture("#[cfg(test)]\n#[path = \"x.rs\"]\nmod tests;\n\nfn f() {\n    {IGNORE}\n}\n"),
            vec![6]
        );
        assert_eq!(
            fixture("#[cfg(test)]\nuse foo::Bar;\nfn f() {\n    {IGNORE}\n}\n"),
            vec![4]
        );
        assert_eq!(
            fixture("#[cfg(test)]\nconst X: [u8; 2] = [1, 2];\nfn f() {\n    {IGNORE}\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn a_test_field_or_statement_ends_at_its_own_separator() {
        assert_eq!(
            fixture("struct S {\n    #[cfg(test)]\n    a: u8,\n}\nfn f() { {IGNORE} }\n"),
            vec![5]
        );
        assert_eq!(
            fixture("fn g() {\n    #[cfg(test)]\n    h();\n    k()\n}\nfn f() { {IGNORE} }\n"),
            vec![6]
        );
    }

    #[test]
    fn separators_inside_brackets_do_not_end_the_test_item() {
        let src =
            "#[cfg(test)]\nconst X: [u8; 2] = [\n    1,\n    2,\n];\nfn f() {\n    {IGNORE}\n}\n";
        assert_eq!(
            masked_lines(src),
            vec![true, true, true, true, true, false, false, false]
        );
        assert_eq!(fixture(src), vec![7]);
    }

    #[test]
    fn signatures_where_clauses_and_const_generics_stay_inside_the_test_item() {
        assert_eq!(
            fixture(
                "#[cfg(test)]\nimpl<A, B> X for Y<A, B>\nwhere\n    A: C,\n{\n    fn t() {\n        {IGNORE}\n    }\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![11]
        );
        assert_eq!(
            fixture(
                "#[cfg(test)]\nfn t(\n    a: u8,\n    b: HashMap<u8, u8>,\n) -> Result<u8, u8>\nwhere\n    T: X,\n{\n    {IGNORE}\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![12]
        );
        assert_eq!(
            fixture(
                "#[cfg(test)]\nfn t() -> Buffer<{\n    N\n}> {\n    {IGNORE}\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn a_block_followed_by_a_method_call_is_one_statement() {
        let src = "fn g() {\n    #[cfg(test)]\n    match y {\n        _ => h(),\n    }\n    .unwrap();\n    {IGNORE}\n}\n";
        assert!(
            masked_lines(src)[5],
            "`.unwrap();` 줄이 테스트 문장에서 빠졌다"
        );
        assert_eq!(fixture(src), vec![7]);
    }

    #[test]
    fn match_arms_end_at_their_comma_or_block() {
        assert_eq!(
            fixture(
                "fn g(x: u8) -> u8 {\n    match x {\n        #[cfg(test)]\n        x if x < 10 => 1,\n        _ => {\n            {IGNORE}\n            0\n        }\n    }\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g(x: u8) {\n    match x {\n        #[cfg(test)]\n        0 => {\n            {IGNORE}\n        }\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn a_turbofish_split_over_lines_stays_in_the_test_argument() {
        let src = "fn g() {\n    f(\n        #[cfg(test)]\n        make::<\n            u8,\n            u16,\n        >(),\n        x,\n    );\n}\n";
        assert_eq!(
            masked_lines(src),
            vec![
                false, false, true, true, true, true, true, false, false, false
            ]
        );
    }

    #[test]
    fn markers_are_recognised_on_the_same_line_and_across_lines() {
        assert_eq!(
            masked_lines("struct S {\n    #[cfg(test)] a: u8,\n    b: u8,\n}\n"),
            vec![false, true, false, false]
        );
        assert_eq!(
            fixture(
                "#[cfg(all(\n    test,\n    unix\n))]\nmod m {\n    fn t() {\n        {IGNORE}\n    }\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![11]
        );
    }

    #[test]
    fn an_inner_cfg_test_covers_its_block_not_the_file() {
        assert_eq!(
            fixture(
                "mod m {\n    #![cfg(test)]\n    fn t() {\n        {IGNORE}\n    }\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
        assert_eq!(
            fixture("m!(\n    #![cfg(test)]\n);\nfn f() {\n    {IGNORE}\n}\n"),
            vec![5],
            "매크로 입력 안의 `#![cfg(test)]` 는 파일 속성이 아니다"
        );
    }

    #[test]
    fn an_allow_on_a_sibling_test_does_not_cover_a_helper() {
        let src = with_ignore(&format!(
            "#[cfg(test)]\nmod t {{\n    #[test]\n    {EXEMPT}\n    fn a() {{}}\n    fn helper() {{\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert!(!item_is_exempt(&src, 6));
    }

    #[test]
    fn an_allow_on_the_enclosing_module_covers_inner_tests() {
        let src = with_ignore(&format!(
            "#[cfg(test)]\n{EXEMPT}\nmod m {{\n    #[test]\n    fn t() {{\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert!(item_is_exempt(&src, 5));
    }

    #[test]
    fn a_comment_between_attributes_keeps_the_attribute_run() {
        let src = with_ignore(&format!(
            "#[cfg(test)]\n// 이유: 시험 정리\n{EXEMPT}\nmod m {{\n    fn t() {{\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert!(item_is_exempt(&src, 5));
        let multi = with_ignore(
            "#[cfg(test)]\n#[cfg_attr(\n    unix,\n    allow(clippy::let_underscore_must_use)\n)]\nmod m {\n    fn t() {\n        {IGNORE}\n    }\n}\n",
        );
        assert!(item_is_exempt(&multi, 7));
    }

    #[test]
    fn a_statement_allow_covers_only_that_statement() {
        let src = with_ignore(&format!(
            "#[cfg(test)]\nmod m {{\n    fn t() {{\n        {EXEMPT}\n        {{IGNORE}}\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert_eq!(uncovered_test_sites(&src, false), vec![5]);
    }

    #[test]
    fn an_allow_on_a_non_item_in_macro_input_covers_only_itself() {
        let src = with_ignore(&format!(
            "#[cfg(test)]\nmod m {{\n    fn t() {{\n        emit!(\n            {EXEMPT}\n            {{\n                {{IGNORE}}\n            }} + if @,\n            0\n        );\n    }}\n}}\n"
        ));
        assert_eq!(uncovered_test_sites(&src, false), vec![6]);
    }

    #[test]
    fn an_inner_allow_covers_its_module_not_the_file() {
        let src = with_ignore(
            "fn prod() {}\n#[cfg(test)]\nmod a {\n    #![allow(clippy::let_underscore_must_use)]\n    #[test]\n    fn t() {\n        {IGNORE}\n    }\n}\n#[cfg(test)]\nmod b {\n    #[test]\n    fn u() {\n        {IGNORE}\n    }\n}\n",
        );
        assert_eq!(uncovered_test_sites(&src, false), vec![13]);
    }

    #[test]
    fn an_inner_allow_covers_only_its_own_block_on_a_shared_line() {
        let src = with_ignore(
            "#[cfg(test)]\nmod t {\n    fn g(b: bool) {\n        match b {\n            true => {\n                #![allow(clippy::let_underscore_must_use)]\n                {IGNORE}\n            }, false => {\n                {IGNORE}\n            }\n        }\n    }\n}\n",
        );
        assert_eq!(uncovered_test_sites(&src, false), vec![8]);
    }

    #[test]
    fn test_targets_accept_the_file_head_and_enclosing_item_exemptions() {
        let head =
            with_ignore("#![allow(clippy::let_underscore_must_use)]\nfn h() {\n    {IGNORE}\n}\n");
        assert!(uncovered_test_sites(&head, true).is_empty());
        let item = with_ignore(&format!(
            "{EXEMPT}\nfn helper() {{\n    {{IGNORE}}\n}}\nfn other() {{\n    {{IGNORE}}\n}}\n"
        ));
        assert_eq!(uncovered_test_sites(&item, true), vec![5]);
    }

    #[test]
    fn generic_headers_and_turbofish_literals_keep_their_list_kind() {
        assert_eq!(
            masked_lines("struct Decision<E, F> {\n    #[cfg(test)]\n    a: E,\n    b: F,\n}\n"),
            vec![false, true, true, false, false]
        );
        assert_eq!(
            masked_lines(
                "fn g() -> X {\n    HandlerDecl::<P> {\n        #[cfg(test)]\n        a: 1,\n        b: 2,\n    }\n}\n"
            ),
            vec![false, false, true, true, false, false, false]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    match parse::<A, B>() {\n        #[cfg(test)]\n        Ok(_) => 1,\n        _ => {\n            {IGNORE}\n            2\n        }\n    };\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn closure_parameters_do_not_end_a_test_argument() {
        let src = "fn g() {\n    run(\n        #[cfg(test)]\n        |ws, id| {\n            {IGNORE}\n        },\n        x,\n    );\n}\n";
        assert_eq!(
            masked_lines(src),
            vec![false, false, true, true, true, true, false, false, false]
        );
        assert!(fixture(src).is_empty());
    }

    #[test]
    fn a_block_inside_a_signature_or_before_a_unary_expression() {
        assert_eq!(
            fixture(
                "#[cfg(test)]\nfn t() -> Buffer<{ 1 }, u8> {\n    {IGNORE}\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() -> bool {\n    #[cfg(test)]\n    {\n        h();\n    }\n    !{\n        {IGNORE}\n        true\n    }\n}\n"
            ),
            vec![7]
        );
    }

    #[test]
    fn a_function_body_inner_allow_also_covers_its_signature() {
        let src = with_ignore(
            "#[cfg(test)]\nmod m {\n    fn t() -> A<{\n        {IGNORE}\n        1\n    }> {\n        #![allow(clippy::let_underscore_must_use)]\n    }\n}\n",
        );
        assert!(uncovered_test_sites(&src, false).is_empty());
    }

    #[test]
    fn only_an_allow_active_in_test_builds_exempts() {
        let inactive = with_ignore(
            "#[cfg(test)]\n#[cfg_attr(not(test), allow(clippy::let_underscore_must_use))]\nmod m {\n    fn t() {\n        {IGNORE}\n    }\n}\n",
        );
        assert!(!item_is_exempt(&inactive, 4));
        let active = inactive.replace("not(test)", "test");
        assert!(item_is_exempt(&active, 4));
        let rewarned = with_ignore(&format!(
            "#[cfg(test)]\n{EXEMPT}\nmod m {{\n    #[warn(clippy::let_underscore_must_use)]\n    fn t() {{\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert_eq!(uncovered_test_sites(&rewarned, false), vec![5]);
    }

    #[test]
    fn each_let_on_a_shared_line_is_judged_by_its_own_position() {
        let mixed = with_ignore(&format!(
            "#[cfg(test)]\nmod m {{\n    fn t() {{\n        {EXEMPT} {{IGNORE}} {{IGNORE}}\n    }}\n}}\n"
        ));
        assert_eq!(uncovered_test_sites(&mixed, false), vec![3]);
        assert_eq!(
            fixture("#[cfg(test)] fn t() { {IGNORE} } fn f() { {IGNORE} }\n"),
            vec![1]
        );
    }

    #[test]
    fn tests_declared_inside_a_macro_block_stay_test_scope() {
        assert_eq!(
            fixture(
                "proptest! {\n    #[test]\n    fn p(x in 0..1u8) {\n        {IGNORE}\n    }\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn a_comparison_before_a_block_is_not_a_generic_argument() {
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        x if x < 3 => 0,\n        0 => match n {\n            _ => {}\n        },\n        _ => if n > 1 {\n            #[cfg(test)]\n            h();\n            {IGNORE}\n        },\n    }\n}\n"
            ),
            vec![10]
        );
    }

    #[test]
    fn closure_parameters_are_found_through_patterns_and_after_an_arm_arrow() {
        assert_eq!(
            masked_lines(
                "fn g() {\n    run(\n        #[cfg(test)]\n        |(Some(_) | None), y| {\n            h();\n        },\n        x,\n    );\n}\n"
            ),
            vec![false, false, true, true, true, true, false, false, false]
        );
        assert_eq!(
            masked_lines(
                "fn g() {\n    match () {\n        #[cfg(test)]\n        () => |a, b| {\n            h();\n        },\n        _ => k(),\n    }\n}\n"
            ),
            vec![false, false, true, true, true, true, false, false, false]
        );
    }

    #[test]
    fn a_let_runs_to_its_semicolon_and_a_block_statement_ends_at_its_brace() {
        let continued =
            "fn g() {\n    #[cfg(test)]\n    let v = { f }\n        - { 1 };\n    {IGNORE}\n}\n";
        assert!(masked_lines(continued)[3], "`let` 이 `;` 전에 끊겼다");
        assert_eq!(fixture(continued), vec![5]);
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    {\n        h();\n    }\n    || {\n        {IGNORE}\n    };\n}\n"
            ),
            vec![7]
        );
    }

    #[test]
    fn cfg_attr_lints_apply_in_order_and_need_an_active_condition() {
        let last_wins = with_ignore(
            "#[cfg(test)]\n#[cfg_attr(test, allow(clippy::let_underscore_must_use), warn(clippy::let_underscore_must_use))]\nmod m {\n    fn t() {\n        {IGNORE}\n    }\n}\n",
        );
        assert!(!item_is_exempt(&last_wins, 4));
        let never = with_ignore(
            "#[cfg(test)]\n#[cfg_attr(not(any(test, unix)), allow(clippy::let_underscore_must_use))]\nmod m {\n    fn t() {\n        {IGNORE}\n    }\n}\n",
        );
        assert!(!item_is_exempt(&never, 4));
        assert_eq!(eval_in_test("all(test, unix)"), None);
        assert_eq!(eval_in_test("any(test, unix)"), Some(true));
        assert_eq!(eval_in_test("all(unix, not(test))"), Some(false));
    }

    #[test]
    fn the_crate_root_fallback_does_not_override_an_inner_warn() {
        let src = with_ignore(
            "#[cfg(test)]\nmod m {\n    fn a() {\n        {IGNORE}\n    }\n    #[warn(clippy::let_underscore_must_use)]\n    fn b() {\n        {IGNORE}\n    }\n}\n",
        );
        let sites = test_scope_sites(&src, false);
        assert_eq!(uncovered(&sites, true), vec![7]);
        assert_eq!(uncovered(&sites, false), vec![3, 7]);
        let head = "#![allow(clippy::let_underscore_must_use)]\n#![warn(clippy::let_underscore_must_use)]\nfn f() {}\n";
        assert_eq!(scopes(head, head).head_level, Some(Level::Warn));
    }

    #[test]
    fn only_a_named_angle_pair_is_a_generic_argument() {
        fn at(src: &str, needle: &str) -> Option<String> {
            let close = src.find(needle)? + needle.len() - 1;
            Code::new(src)
                .generic_open(close)
                .map(|lt| src[lt..=close].to_string())
        }
        assert_eq!(at("struct D<E, F> {}", "F>"), Some("<E, F>".to_string()));
        assert_eq!(at("x::<A, B>()", "B>"), Some("<A, B>".to_string()));
        assert_eq!(
            at("match n { x if x < 3 => 0, _ => if n > 1 {} }", "n >"),
            None,
            "비교식 `<`·`>` 를 제네릭으로 묶었다"
        );
        assert_eq!(at("if a < b && c > d {}", "c >"), None);
    }

    #[test]
    fn a_block_statement_ends_at_its_body_not_at_a_pattern_or_scrutinee() {
        assert_eq!(
            fixture(
                "fn g(v: S) {\n    #[cfg(test)]\n    if let S { n: 1 } = v {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    match { 1u8 } {\n        _ => {\n            {IGNORE}\n        }\n    }\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
        assert_eq!(
            fixture(
                "#[cfg(test)]\nunsafe fn t() -> Buffer<{ 1 }> {\n    {IGNORE}\n}\nfn f() {\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn a_call_on_a_block_argument_stays_in_that_argument() {
        assert_eq!(
            masked_lines(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        { id }({\n            h();\n            1\n        }),\n        x,\n    );\n}\n"
            ),
            vec![
                false, false, true, true, true, true, true, false, false, false
            ]
        );
    }

    #[test]
    fn a_block_statement_finds_its_body_across_lines_and_operand_blocks() {
        assert_eq!(
            fixture("fn g() {\n    #[cfg(test)]\n    { () } {\n        {IGNORE}\n    }\n}\n"),
            vec![4],
            "속성이 붙은 블록 뒤의 다른 블록까지 테스트로 묶었다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    if { value() } != 0 {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    if unsafe { f() } != 0 {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    match { 1u8 }\n    {\n        _ => {\n            {IGNORE}\n        }\n    }\n    {IGNORE}\n}\n"
            ),
            vec![9]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    if a {\n        h();\n    } else if let S { n } = v {\n        {IGNORE}\n    } else {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![10]
        );
    }

    #[test]
    fn a_call_on_a_block_argument_continues_on_the_next_line() {
        assert_eq!(
            masked_lines(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        { id }\n        ({\n            h();\n            1\n        }),\n        x,\n    );\n}\n"
            ),
            vec![
                false, false, true, true, true, true, true, true, false, false, false
            ]
        );
    }

    #[test]
    fn a_nested_turbofish_keeps_its_commas() {
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        build::<Option<u8>, u16>({\n            {IGNORE}\n            1\n        }),\n        x,\n    );\n    {IGNORE}\n}\n"
            ),
            vec![10]
        );
    }

    #[test]
    fn each_unexempt_site_keeps_its_own_fix() {
        let forbid = with_ignore(&format!(
            "#![forbid(clippy::let_underscore_must_use)]\n#[cfg(test)]\nmod m {{\n    {EXEMPT}\n    fn t() {{\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert_eq!(
            needs(
                &test_scope_sites(&forbid, false),
                Root::Known(Some(Level::Allow))
            ),
            vec![(5, Need::Forbidden)]
        );
        let sites = [(3, Some(Level::Warn)), (3, None), (4, Some(Level::Allow))];
        assert_eq!(
            needs(&sites, Root::Known(Some(Level::Allow))),
            vec![(3, Need::Reenabled)]
        );
        assert_eq!(
            needs(&sites, Root::Known(None)),
            vec![(3, Need::Reenabled), (3, Need::Exemption)]
        );
        assert_eq!(
            needs(&sites, Root::Known(Some(Level::Forbid))),
            vec![
                (3, Need::Forbidden),
                (3, Need::Forbidden),
                (4, Need::Forbidden)
            ],
            "루트 forbid 아래의 안쪽 allow 를 면제로 봤다"
        );
        assert_eq!(
            lint_level(
                "cfg_attr(test,forbid(clippy::let_underscore_must_use),allow(clippy::let_underscore_must_use))"
            ),
            Some(Level::Forbid)
        );
    }

    #[test]
    fn a_scrutinee_ending_in_a_literal_or_generic_finds_its_body() {
        // 첫 `{..}` 가 블록 피연산자라 본문을 못 찾았을 때의 처리로는 맞출 수 없는 입력.
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    if { a } == \"x\" {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    if { a } == None::<u8> {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    match \"probe\" {\n        _ => {\n            {IGNORE}\n        }\n    }\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    match None::<u8> {\n        _ => {\n            {IGNORE}\n        }\n    }\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn a_control_expression_inside_a_head_is_read_to_its_end() {
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    match if yes { 1 } else if no { 2 } else { 3 } {\n        _ => {\n            {IGNORE}\n        }\n    }\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    for x in if yes { 0..1 } else { 1..2 } {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn an_unfound_body_does_not_hide_the_rest_of_the_block() {
        let found = fixture(
            "fn g() {\n    #[cfg(test)]\n    match ( {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n",
        );
        assert!(
            found.contains(&6),
            "본문을 못 찾고 감싸는 블록 끝까지 테스트로 덮었다: {found:?}"
        );
    }

    #[test]
    fn a_match_on_a_control_expression_keeps_its_arm_list() {
        assert_eq!(
            fixture(
                "fn g(flag: bool) {\n    match if flag { 0 } else { 1 } {\n        #[cfg(test)]\n        0 => (),\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![6],
            "테스트 팔 뒤의 제품 팔까지 테스트로 덮었다"
        );
    }

    #[test]
    fn a_control_expression_argument_runs_through_its_branches() {
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        if { flag() } {\n            {IGNORE}\n            1\n        } else {\n            2\n        },\n        3,\n    );\n    {IGNORE}\n}\n"
            ),
            vec![12]
        );
    }

    #[test]
    fn comparisons_on_both_ends_of_a_range_are_not_a_generic_pair() {
        assert_eq!(
            fixture(
                "fn g() {\n    #[cfg(test)]\n    match 1 < 2 .. 3 > { 4 } {\n        _ => {\n            {IGNORE}\n        }\n    }\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn a_head_without_any_brace_ends_at_its_semicolon() {
        assert_eq!(
            fixture("emit! {\n    #[cfg(test)]\n    match;\n    {\n        {IGNORE}\n    }\n}\n"),
            vec![5]
        );
    }

    #[test]
    fn crate_root_candidates_that_disagree_are_reported() {
        let head = |t: &str| scopes(&tasty_doc_guards::source_text::mask_non_code(t), t).head_level;
        let lib = head("#![cfg_attr(test, allow(clippy::let_underscore_must_use))]\npub mod m;\n");
        let main = head("#![forbid(clippy::let_underscore_must_use)]\nmod m;\nfn main() {}\n");
        let plain = head("mod m;\nfn main() {}\n");
        assert_eq!(root_level_of(&[lib, main]), Root::Conflict);
        assert_eq!(
            root_level_of(&[lib, plain]),
            Root::Known(Some(Level::Allow))
        );
        assert_eq!(
            root_level_of(&[plain, main]),
            Root::Known(Some(Level::Forbid))
        );
        let sites = [(1, None), (2, Some(Level::Allow)), (3, Some(Level::Warn))];
        assert_eq!(
            needs(&sites, root_level_of(&[lib, main])),
            vec![
                (1, Need::RootConflict),
                (2, Need::RootConflict),
                (3, Need::RootConflict)
            ]
        );
    }

    #[test]
    fn a_raw_identifier_is_not_a_keyword() {
        assert_eq!(
            fixture(
                "fn g(r#match: bool) {\n    if r#match {\n        #[cfg(test)]\n        {IGNORE}\n        {IGNORE}\n    }\n}\n"
            ),
            vec![5],
            "그룹 머리의 raw 식별자를 키워드로 읽었다"
        );
        assert_eq!(
            fixture(
                "fn g(r#match: bool, flag: bool) {\n    if r#match && { flag } {\n        #[cfg(test)]\n        {IGNORE}\n        {IGNORE}\n    }\n}\n"
            ),
            vec![5],
            "raw 식별자 `r#match` 를 키워드로 읽어 if 본문을 match 팔 목록으로 봤다"
        );
    }

    #[test]
    fn a_control_expression_in_an_arm_or_field_value_runs_through_its_branches() {
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        #[cfg(test)]\n        0 => if { flag() } {\n            {IGNORE}\n        } else {\n            ()\n        }\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![10],
            "쉼표 없는 블록 식 팔이 다음 팔까지 이어졌다"
        );
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        #[cfg(test)]\n        0 => if { flag() } {\n            {IGNORE}\n        } else {\n            ()\n        },\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![10]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    let s = S {\n        #[cfg(test)]\n        a: if { flag() } {\n            {IGNORE}\n            1\n        } else {\n            2\n        },\n        b: {\n            {IGNORE}\n            3\n        },\n    };\n}\n"
            ),
            vec![11]
        );
    }

    #[test]
    fn a_list_fallback_stops_at_the_next_comma() {
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match x,\n        y {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![6],
            "목록의 본문 탐색이 쉼표를 넘었다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match,\n        {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn a_raw_identifier_ends_a_condition() {
        assert_eq!(
            fixture(
                "fn g(r#match: bool) {\n    #[cfg(test)]\n    if { flag } && r#match {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn an_arm_pattern_with_generics_or_a_labelled_guard_finds_its_arrow() {
        assert_eq!(
            fixture(
                "fn g(e: E) {\n    match e {\n        #[cfg(test)]\n        Example::<u8, u16>::V => if { flag() } {\n            {IGNORE}\n        } else {\n            ()\n        },\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![10]
        );
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        #[cfg(test)]\n        0 if 'guard: { break 'guard true } => if { flag() } {\n            {IGNORE}\n        } else {\n            ()\n        },\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![10]
        );
    }

    #[test]
    fn a_qualified_path_keeps_its_commas_in_a_list() {
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        if <Pair<u8, u16> as Test>::FLAG {\n            {IGNORE}\n            1\n        } else {\n            2\n        },\n        3,\n    );\n    {IGNORE}\n}\n"
            ),
            vec![12]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        if a < b {\n            1\n        } else {\n            2\n        },\n        c > d,\n        {IGNORE},\n    );\n}\n"
            ),
            vec![10],
            "비교 연산자를 각괄호로 묶었다"
        );
    }

    #[test]
    fn arrows_and_commas_in_macro_input_do_not_widen_the_scope() {
        // 매크로 인자 목록의 `=>` 는 팔로 보지 않는다. 그 뒤 제어식은 읽지 못하고 제품으로 드러난다.
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        0 => if { a } { 1 } else { 2 } + {\n            {IGNORE}\n            3\n        },\n        x,\n    );\n}\n"
            ),
            vec![5]
        );
        // 팔 판별을 match 본문에만 두는 규칙이 맡는 입력: 팔로 읽으면 if 식을 테스트로 덮는다.
        assert_eq!(
            fixture(
                "fn g() {
    emit!(
        #[cfg(test)]
        0 => if { flag } {
            {IGNORE}
        } else {},
        3,
    );
}
"
            ),
            vec![5]
        );
        // 팔 본문 탐색의 쉼표 멈춤과 비교 `<` 를 각괄호로 보지 않는 규칙이 각각 혼자 맡는 입력.
        assert_eq!(
            fixture(
                "m! {\n    match x {\n        #[cfg(test)]\n        0 => if,\n        1 => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match a < b,\n        c > d {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        0 => match,\n        x {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![6]
        );
        assert_eq!(
            fixture(
                "emit! {\n    match q, x {\n        #[cfg(test)]\n        {IGNORE}\n        {IGNORE}\n    }\n}\n"
            ),
            // 매크로 입력 안의 아이템이 아닌 문장은 표식이 그 속성만 덮어 둘 다 드러난다.
            vec![4, 5]
        );
    }

    #[test]
    fn non_item_elements_in_macro_input_are_covered_only_by_their_attributes() {
        for (src, want, why) in [
            (
                "fn g() {\n    emit! {\n        #[cfg(test)]\n        let x = {\n            {IGNORE}\n        } = | arg;\n    }\n}\n",
                vec![5],
                "문장 그룹의 닫히지 않은 클로저",
            ),
            (
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match | stub, (\n            {IGNORE}\n        ), | tail {}\n    );\n}\n",
                vec![5],
                "문장이 든 괄호를 클로저 매개변수로",
            ),
            (
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        {\n            {IGNORE}\n        } + if @,\n        0\n    );\n}\n",
                vec![5],
                "이어 읽은 뒤의 제어식",
            ),
            (
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match const x {\n            {IGNORE}\n        }\n    );\n}\n",
                vec![5],
                "검사식 첫머리의 const",
            ),
        ] {
            assert_eq!(fixture(src), want, "{why}");
        }
        // 매크로 입력 안이어도 아이템은 일반 규칙으로 읽는다.
        assert_eq!(
            fixture(
                "m! {\n    #[cfg(test)]\n    pub(crate) fn t() {\n        {IGNORE}\n    }\n}\nfn p() {\n    {IGNORE}\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn raw_labels_are_labels() {
        assert_eq!(
            fixture(
                "fn g(p: (u8, u8)) {\n    match p {\n        #[cfg(test)]\n        (0, 0) => 'r#async: loop {\n            break 'r#async;\n        }\n        (a, b) => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![8]
        );
    }

    #[test]
    fn labelled_and_const_block_arms_end_at_their_block() {
        assert_eq!(
            fixture(
                "fn g(p: (u8, u8)) {\n    match p {\n        #[cfg(test)]\n        (0, 0) => 'done: loop {\n            break 'done;\n        }\n        (a, b) => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![8],
            "라벨 루프 팔 뒤의 제품 팔을 테스트로 덮었다"
        );
        assert_eq!(
            fixture(
                "fn g(p: (u8, u8)) {\n    match p {\n        #[cfg(test)]\n        (0, 0) => const { () }\n        (a, b) => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![6],
            "const 블록 팔 뒤의 제품 팔을 테스트로 덮었다"
        );
    }

    #[test]
    fn non_ascii_identifiers_are_identifiers() {
        assert_eq!(
            fixture(
                "fn g(값match: bool, flag: bool) {\n    if 값match && { flag } {\n        #[cfg(test)]\n        {IGNORE}\n        {IGNORE}\n    }\n}\n"
            ),
            vec![5],
            "식별자 안의 match 를 키워드로 읽었다"
        );
        assert_eq!(
            fixture(
                "fn g(준비: bool) {\n    #[cfg(test)]\n    if { flag } && 준비 {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn a_qualified_path_or_spaced_turbofish_keeps_its_commas_anywhere() {
        assert_eq!(
            fixture(
                "fn g(e: E) {\n    match e {\n        #[cfg(test)]\n        <Pair<u8, u16> as Test>::VALUE => if { flag } {\n            {IGNORE}\n        } else {\n            ()\n        },\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![10]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        <Pair<u8, u16> as Test>::value({\n            {IGNORE}\n            1\n        }),\n        3,\n    );\n    {IGNORE}\n}\n"
            ),
            vec![10]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        value:: /* c */ <u8, u16>({\n            {IGNORE}\n            1\n        }),\n        3,\n    );\n    {IGNORE}\n}\n"
            ),
            vec![10]
        );
    }

    #[test]
    fn closure_parameters_in_a_head_are_not_list_separators() {
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        #[cfg(test)]\n        match |a: u8, b: u8| {\n            {IGNORE}\n            a + b\n        } {\n            _ => 1,\n        },\n        3,\n    );\n    {IGNORE}\n}\n"
            ),
            vec![12]
        );
    }

    #[test]
    fn an_unpaired_bracket_ends_the_scope_where_it_stands() {
        assert_eq!(
            fixture("m! {\n    #[cfg(test)]\n    f(\n    {IGNORE}\n}\n"),
            vec![4],
            "짝 없는 괄호 뒤를 테스트로 덮었다"
        );
        assert_eq!(
            fixture("m! {\n    #[cfg(test)]\n    if a {\n    {IGNORE}\n"),
            vec![4]
        );
    }

    #[test]
    fn logical_or_and_shift_are_operators_not_closures_or_generics() {
        assert_eq!(
            fixture(
                "fn g(a: bool, b: bool) {\n    #[cfg(test)]\n    if a || b {\n        {IGNORE}\n    }\n    {IGNORE}\n}\n"
            ),
            vec![6],
            "`||` 를 클로저로 읽어 뒤를 덮었다"
        );
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        #[cfg(test)]\n        n if n << 1 == 2 => (),\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![6],
            "`<<` 를 각괄호로 읽어 다음 팔을 덮었다"
        );
        assert_eq!(
            fixture(
                "fn g(x: u8) {\n    let s = S {\n        #[cfg(test)]\n        a: x << 1,\n        b: {\n            {IGNORE}\n            1\n        },\n    };\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn broken_heads_end_where_they_break() {
        assert_eq!(
            fixture("m! {\n    #[cfg(test)]\n    match (\n    {\n        {IGNORE}\n    }\n}\n"),
            vec![5]
        );
        assert_eq!(
            fixture("m! {\n    #[cfg(test)]\n    match [\n    {\n        {IGNORE}\n    }\n}\n"),
            vec![5]
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match | stub,\n        {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn non_ascii_rust_whitespace_is_whitespace() {
        assert_eq!(
            fixture(
                "fn g(flag: bool, n: u8) {\n    \u{200E}match if flag { 0 } else { 1 } {\n        #[cfg(test)]\n        0 => (),\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn an_unreadable_block_head_covers_only_its_attributes() {
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match @ {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![5],
            "본문을 못 찾은 머리의 첫 블록을 테스트로 덮었다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match | stub,\n        {\n            {IGNORE}\n        },\n        | tail\n    );\n}\n"
            ),
            vec![6],
            "다른 인자의 `|` 를 클로저 끝으로 잡았다"
        );
        assert_eq!(
            fixture("m! {\n    #[cfg(test)]\n    match | ( | post {\n        {IGNORE}\n    }\n}\n"),
            vec![4]
        );
        assert_eq!(
            fixture("S {\n    #[cfg(test)]\n    a ( : {\n        {IGNORE}\n    }\n}\n"),
            vec![4]
        );
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        #[cfg(test)]\n        n if n\u{0B}<< 1 == 2 => (),\n        _ => {\n            {IGNORE}\n        }\n    }\n}\n"
            ),
            vec![6]
        );
    }

    #[test]
    fn a_body_search_failure_anywhere_covers_only_the_attributes() {
        assert_eq!(
            fixture(
                "fn g() {\n    match unsafe {\n        #[cfg(test)]\n        stub();\n        {IGNORE}\n    } {\n        _ => {}\n    }\n}\n"
            ),
            vec![5],
            "검사식 블록을 팔 목록으로 읽었다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match unsafe x {\n            {IGNORE}\n        }\n    );\n}\n"
            ),
            vec![5],
            "안쪽 제어식 실패 뒤 바깥 본문을 찾았다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        match | stub, ({\n            {IGNORE}\n        }), | tail {}\n    );\n}\n"
            ),
            vec![5],
            "블록이 든 괄호를 클로저 매개변수로 건너뛰었다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    emit!(\n        #[cfg(test)]\n        if yes {\n            {IGNORE}\n        } else nope\n    );\n}\n"
            ),
            vec![5],
            "실패한 갈래 앞의 갈래를 덮었다"
        );
        assert_eq!(
            fixture(
                "fn g(n: u8) {\n    match n {\n        #[cfg(test)]\n        0 if {\n            {IGNORE}\n        } => match @,\n        _ => {}\n    }\n}\n"
            ),
            vec![5],
            "실패한 팔 값 앞의 가드를 덮었다"
        );
    }

    #[test]
    fn broken_input_after_read_code_covers_only_the_attributes() {
        assert_eq!(
            fixture("fn g() {\n    #[cfg(test)]\n    if a {\n        {IGNORE}\n    } else {\n"),
            vec![4],
            "짝 없는 갈래 앞의 갈래를 덮었다"
        );
        assert_eq!(
            fixture("fn g() {\n    #[cfg(test)]\n    x + {\n        {IGNORE}\n    } + (\n"),
            vec![4],
            "짝 없는 괄호 앞의 블록을 덮었다"
        );
        assert_eq!(
            fixture(
                "fn g() {\n    f(\n        #[cfg(test)]\n        a = {\n            {IGNORE}\n        } = | x\n    );\n}\n"
            ),
            vec![5],
            "닫히지 않은 클로저 앞의 블록을 덮었다"
        );
    }

    #[test]
    fn comparisons_in_different_arguments_are_not_paired() {
        assert_eq!(
            fixture(
                "fn g() {\n    visit(\n        match x < y {\n            _ => 1,\n        },\n        if z > x {\n            #[cfg(test)]\n            h();\n            {IGNORE}\n        },\n    );\n}\n"
            ),
            vec![9]
        );
    }

    #[test]
    fn a_lint_listed_with_others_or_a_reason_is_still_found() {
        let src = with_ignore(&format!(
            "#[cfg(test)]\n{EXEMPT}\nmod m {{\n    #[warn(unused_variables, clippy::let_underscore_must_use)]\n    fn t() {{\n        {{IGNORE}}\n    }}\n}}\n"
        ));
        assert_eq!(uncovered_test_sites(&src, false), vec![5]);
        assert_eq!(
            lint_level("allow(clippy::let_underscore_must_use,reason=\"\")"),
            Some(Level::Allow)
        );
        assert_eq!(lint_level("allow(clippy::let_underscore_untyped)"), None);
    }

    #[test]
    fn detects_a_bare_ignore() {
        assert_eq!(fixture("fn f() {\n    {IGNORE}\n}\n"), vec![2]);
    }

    #[test]
    fn same_line_previous_line_and_next_line_comments_all_count() {
        assert!(fixture("fn f() {\n    {IGNORE} // 이유\n}\n").is_empty());
        assert!(fixture("fn f() {\n    // 이유\n    {IGNORE}\n}\n").is_empty());
        assert!(fixture("fn f() {\n    {IGNORE}\n    // 이유\n}\n").is_empty());
    }

    #[test]
    fn blank_lines_and_attributes_are_skipped_when_looking_up() {
        assert!(fixture("fn f() {\n    // 이유\n\n    {IGNORE}\n}\n").is_empty());
        assert!(
            fixture("fn f() {\n    // 이유\n    #[allow(unused)]\n    {IGNORE}\n}\n").is_empty()
        );
    }

    #[test]
    fn a_comment_inside_a_multiline_statement_counts() {
        let src = format!(
            "fn f() {{\n    let {}= g(Bar {{\n        // 이유\n        a: 1,\n    }});\n}}\n",
            "_ "
        );
        assert!(scan(&src).is_empty());
    }

    #[test]
    fn a_block_comment_far_above_does_not_count() {
        assert_eq!(
            fixture("fn f() {\n    // 블록 상단 설명\n    h();\n    {IGNORE}\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn test_bodies_are_exempt() {
        assert!(fixture("#[test]\nfn t() {\n    {IGNORE}\n}\n").is_empty());
        assert!(
            fixture("#[cfg(test)]\nmod tests {\n    fn t() {\n        {IGNORE}\n    }\n}\n")
                .is_empty()
        );
    }

    #[test]
    fn a_cfg_that_requires_test_among_other_predicates_is_still_test() {
        assert!(
            fixture(
                "#[cfg(all(test, unix))]\nmod tests {\n    fn t() {\n        {IGNORE}\n    }\n}\n"
            )
            .is_empty()
        );
        assert!(
            fixture(
                "#[cfg(all(unix, all(test, feature = \"gui\")))]\nmod tests {\n    fn t() {\n        {IGNORE}\n    }\n}\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_cfg_that_merely_mentions_test_is_not_test_scope() {
        assert_eq!(
            fixture("#[cfg(any(unix, test))]\nmod m {\n    fn f() {\n        {IGNORE}\n    }\n}\n"),
            vec![4]
        );
        assert_eq!(
            fixture("#[cfg(not(test))]\nmod m {\n    fn f() {\n        {IGNORE}\n    }\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn a_file_level_cfg_that_requires_test_exempts_the_whole_file() {
        assert!(fixture("#![cfg(all(test, unix))]\n\nfn f() {\n    {IGNORE}\n}\n").is_empty());
        assert_eq!(
            fixture("#![cfg(unix)]\n\nfn f() {\n    {IGNORE}\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn a_whole_file_marked_cfg_test_is_exempt() {
        assert!(fixture("#![cfg(test)]\nfn f() {\n    {IGNORE}\n}\n").is_empty());
    }

    #[test]
    fn identifier_lookalikes_are_not_matched() {
        assert!(scan("fn f() {\n    let _y = g();\n}\n").is_empty());
        assert!(scan("fn f() {\n    if _ == g() {}\n}\n").is_empty());
        assert!(fixture("fn f() {\n    out{IGNORE}\n}\n").is_empty());
    }

    #[test]
    fn a_form_inside_a_string_literal_is_not_a_violation() {
        let src = format!(
            "fn f() {{\n    let s = \"{}\";\n    drop(s);\n}}\n",
            ignore_stmt()
        );
        assert!(scan(&src).is_empty());
    }

    #[test]
    fn a_slash_slash_inside_a_string_is_not_a_reason() {
        let src = format!("fn f() {{\n    let {}= get(\"http://x\");\n}}\n", "_ ");
        assert_eq!(scan(&src), vec![2]);
    }

    #[test]
    fn a_form_inside_a_line_comment_is_not_a_violation() {
        assert!(fixture("fn f() {\n    // 예: {IGNORE}\n}\n").is_empty());
    }

    #[test]
    fn the_allowlist_exempts_a_registered_snippet_not_the_whole_file() {
        let entries: &[(&str, &[&str])] = &[("src/x.rs", &["RULE_TEXT"])];

        assert!(is_allowlisted(
            entries,
            "src/x.rs",
            "let s = \"RULE_TEXT\";"
        ));
        assert!(
            !is_allowlisted(entries, "src/x.rs", &ignore_stmt()),
            "등록 파일의 다른 위반까지 면제되면 파일 통째 면제와 같아진다"
        );
        assert!(
            !is_allowlisted(entries, "src/y.rs", "let s = \"RULE_TEXT\";"),
            "같은 조각이어도 등록되지 않은 파일은 면제가 아니다"
        );
    }

    #[test]
    fn test_dirs_are_recognised_by_any_path_segment() {
        assert!(is_test_path("tests/common/mod.rs"));
        assert!(is_test_path(
            "crates/tasty-agent/tests/runner_integration.rs"
        ));
        assert!(!is_test_path("src/core/agent/task.rs"));
    }
}
