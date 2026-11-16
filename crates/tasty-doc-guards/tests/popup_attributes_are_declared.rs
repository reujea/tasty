//! 팝업 층의 공용 코드가 팝업 id를 비교해 그리기 속성을 정하지 않는지 확인한다.
//!
//! 팝업 사이의 차이(scrim, 그림자, 콘텐츠 여백, 셸 색 등)는 `PopupDef.decl`의 선언 값으로 정한다.
//! 이 검사는 문법만 본다. 주석을 가린 원문에서 다음 형태를 찾는다.
//! - `matches!(<..id>, "…" | …)`, `matches!(<..id>, X_ID | …)`
//! - `match <..id> { "…" => …` 처럼 첫 갈래가 문자열이나 `*_ID` 상수인 match
//! - `<..id> == "…"`, `<..id> == X_ID`(`!=`와 좌우를 바꾼 형태 포함)
//! - `const X: &[PopupId]` 같은 id 명부 상수와 `X.contains(&<..id>)`
//!
//! `<..id>`는 마지막 이름이 `id`이거나 `_id`로 끝나는 식이다(`id`, `popup_id`, `self.id`).
//! 한계: 다른 이름의 변수에 담은 id, 함수 호출을 거친 비교, 해시맵 조회는 잡지 못한다.
//! 같은 팝업끼리 찾는 비교(`p.id == id`)는 양쪽이 모두 변수라 걸리지 않는다.
//!
//! 대상 범위: `src/adapters/ui/popup.rs`와 `src/adapters/ui/popup/` 바로 아래 파일 중 공용 코드다.
//! 팝업 하나의 모듈(자기 id 상수를 문자열로 정의하는 파일)과 하위 디렉터리, `*_tests.rs`는 뺀다.
//! 팝업 모듈이 자기 id나 닫을 대상 id를 비교하는 것은 조회·열기 대상 지정이라 허용한다.

use std::path::Path;

use tasty_doc_guards::floored_walk::{CountedOn, Floor, entries_with_floor};
use tasty_doc_guards::repo_root;
use tasty_doc_guards::source_text::mask_comments;

const POPUP_DIR: &str = "src/adapters/ui/popup";
const POPUP_ROOT_FILE: &str = "src/adapters/ui/popup.rs";

const POPUP_DIR_FLOOR: Floor = Floor {
    min: 25,
    measured: 32,
    measured_on: "2026-11-16",
    counted_on: CountedOn::Tree(
        "c9a443c66 — git ls-tree로 src/adapters/ui/popup 바로 아래 엔트리 32개(파일 27, 디렉터리 5)를 셌다. 선언 표 시험 파일이 더해져 33이 된다.",
    ),
    why_this_gap: "팝업 모듈 몇 개가 하위 디렉터리로 묶이거나 합쳐져도 통과하도록 7개를 남겼다. 25 미만이면 루트를 잘못 잡았거나 read_dir이 실패한 것이므로 다시 잰다.",
};

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_path_char(c: char) -> bool {
    is_ident_char(c) || c == '.' || c == ':'
}

/// 경로식의 마지막 이름.
fn last_segment(path: &str) -> &str {
    path.rsplit(['.', ':']).next().unwrap_or(path)
}

fn is_id_expr(path: &str) -> bool {
    let seg = last_segment(path);
    seg == "id" || seg.ends_with("_id")
}

fn is_id_const(path: &str) -> bool {
    last_segment(path).ends_with("_ID")
}

/// `s`의 앞 공백을 건너뛴 나머지.
fn skip_ws(s: &str) -> &str {
    s.trim_start()
}

/// 앞쪽 경로식 하나와 나머지.
fn take_path(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !is_path_char(c)).unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// 문자열 리터럴이나 `*_ID` 상수로 시작하는지.
fn starts_with_id_value(s: &str) -> bool {
    let s = skip_ws(s);
    if s.starts_with('"') {
        return true;
    }
    let (path, _) = take_path(s);
    !path.is_empty() && is_id_const(path)
}

fn line_of(src: &str, at: usize) -> usize {
    src[..at].matches('\n').count() + 1
}

/// 금지 형태가 있는 줄 번호와 이유.
fn violations(src: &str) -> Vec<(usize, &'static str)> {
    let text = mask_comments(src);
    let mut out = Vec::new();

    for (at, _) in text.match_indices("matches!(") {
        let rest = skip_ws(&text[at + "matches!(".len()..]);
        let (path, rest) = take_path(rest);
        if is_id_expr(path)
            && let Some(rest) = skip_ws(rest).strip_prefix(',')
            && starts_with_id_value(rest)
        {
            out.push((line_of(&text, at), "matches!로 팝업 id를 고른다"));
        }
    }

    for (at, _) in text.match_indices("match ") {
        if at > 0 && text[..at].ends_with(is_ident_char) {
            continue;
        }
        let rest = skip_ws(&text[at + "match ".len()..]);
        let (path, rest) = take_path(rest);
        if is_id_expr(path)
            && let Some(rest) = skip_ws(rest).strip_prefix('{')
            && starts_with_id_value(rest)
        {
            out.push((line_of(&text, at), "match로 팝업 id를 고른다"));
        }
    }

    for op in ["==", "!="] {
        for (at, _) in text.match_indices(op) {
            let left = text[..at].trim_end();
            let left_start = left.rfind(|c: char| !is_path_char(c)).map_or(0, |i| i + 1);
            let left_path = &left[left_start..];
            let right = &text[at + op.len()..];
            let left_is_value = left.ends_with('"') || is_id_const(left_path);
            let (right_path, _) = take_path(skip_ws(right));
            let hit = (is_id_expr(left_path) && starts_with_id_value(right))
                || (left_is_value && is_id_expr(right_path));
            if hit {
                out.push((line_of(&text, at), "팝업 id를 상수와 비교한다"));
            }
        }
    }

    for (at, _) in text.match_indices("&[PopupId]") {
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        if text[line_start..at].contains("const ") || text[line_start..at].contains("static ") {
            out.push((line_of(&text, at), "팝업 id 명부 상수"));
        }
    }

    for (at, _) in text.match_indices(".contains(") {
        let receiver_start = text[..at]
            .rfind(|c: char| !is_path_char(c))
            .map_or(0, |i| i + 1);
        let receiver = last_segment(&text[receiver_start..at]);
        let arg = skip_ws(&text[at + ".contains(".len()..]);
        let arg = arg.strip_prefix('&').unwrap_or(arg);
        let (arg_path, _) = take_path(arg);
        let is_const_list =
            !receiver.is_empty() && receiver.chars().all(|c| c.is_ascii_uppercase() || c == '_');
        if is_const_list && is_id_expr(arg_path) {
            out.push((line_of(&text, at), "팝업 id 명부에서 찾는다"));
        }
    }

    out.sort_unstable();
    out.dedup();
    out
}

/// 팝업 하나의 모듈인지. 자기 id 상수를 문자열 리터럴로 정의한다.
fn defines_own_popup_id(src: &str) -> bool {
    let text = mask_comments(src);
    text.lines().any(|line| {
        let mut line = line.trim_start();
        for vis in ["pub(crate) ", "pub(super) ", "pub "] {
            line = line.strip_prefix(vis).unwrap_or(line);
        }
        let Some(rest) = line.strip_prefix("const ") else {
            return false;
        };
        let end = rest.find(|c: char| !is_ident_char(c)).unwrap_or(rest.len());
        let (name, after) = rest.split_at(end);
        name.ends_with("_ID") && after.contains('=') && after.contains('"')
    })
}

fn scanned_files(root: &Path) -> Vec<(String, String)> {
    let entries = entries_with_floor(&root.join(POPUP_DIR), root, &POPUP_DIR_FLOOR, 80)
        .unwrap_or_else(|why| panic!("{why}"));
    let mut out = Vec::new();
    let root_file = std::fs::read_to_string(root.join(POPUP_ROOT_FILE))
        .unwrap_or_else(|e| panic!("{POPUP_ROOT_FILE} 를 읽을 수 없다: {e}"));
    out.push((POPUP_ROOT_FILE.to_owned(), root_file));
    for w in entries {
        if !w.path.is_file() || !w.rel.ends_with(".rs") || w.rel.ends_with("_tests.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&w.path)
            .unwrap_or_else(|e| panic!("{} 를 읽을 수 없다: {e}", w.rel));
        if defines_own_popup_id(&src) {
            continue;
        }
        out.push((w.rel, src));
    }
    out
}

#[test]
fn shared_popup_code_does_not_pick_attributes_by_popup_id() {
    let root = repo_root();
    let files = scanned_files(&root);
    let names: Vec<&str> = files.iter().map(|(rel, _)| rel.as_str()).collect();
    for must in [
        POPUP_ROOT_FILE,
        "src/adapters/ui/popup/draw.rs",
        "src/adapters/ui/popup/defs.rs",
        "src/adapters/ui/popup/frame.rs",
    ] {
        assert!(
            names.contains(&must),
            "{must} 가 검사 대상에서 빠졌다. 대상: {names:?}"
        );
    }
    let mut found = Vec::new();
    for (rel, src) in &files {
        for (line, why) in violations(src) {
            found.push(format!("{rel}:{line}: {why}"));
        }
    }
    assert!(
        found.is_empty(),
        "팝업 공용 코드가 팝업 id로 속성을 정한다:\n{}\n\
         팝업마다 다른 값은 PopupDef.decl의 선언 값으로 옮긴다(docs/dev-guide/popup-implementation.md).",
        found.join("\n")
    );
}

/// 판독기가 금지 형태를 실제로 찾는지 합성 입력으로 확인한다.
#[test]
fn the_reader_finds_each_forbidden_shape() {
    let cases = [
        "fn f(id: PopupId) -> bool {\n    matches!(\n        id,\n        \"a\" | \"b\"\n    )\n}",
        "fn f(id: PopupId) -> bool { matches!(id, super::x::A_POPUP_ID | \"b\") }",
        "fn f(popup_id: PopupId) -> u8 { match popup_id { \"a\" => 1, _ => 0 } }",
        "fn f(popup_id: PopupId) -> u8 { match popup_id { x::A_ID => 1, _ => 0 } }",
        "fn f(&self) -> bool { self.id == \"a\" }",
        "fn f(id: PopupId) -> bool { id != super::x::A_POPUP_ID }",
        "fn f(id: PopupId) -> bool { A_POPUP_ID == id }",
        "const LIST: &[PopupId] = &[\"a\"];",
        "fn f(id: PopupId) -> bool { LIST.contains(&id) }",
    ];
    for case in cases {
        assert!(!violations(case).is_empty(), "놓쳤다: {case}");
    }
}

/// 같은 팝업을 찾는 비교와 주석 속 예시는 금지 형태가 아니다.
#[test]
fn lookups_and_comments_are_not_flagged() {
    let cases = [
        "fn f(&self, id: PopupId) -> bool { self.popups.iter().any(|p| p.id == id) }",
        "fn f(a: u32, b: u32) -> bool { a == b }",
        "// matches!(id, \"a\" | \"b\")\nfn f() {}",
        "fn f(rect: Rect, pos: Pos2) -> bool { rect.contains(pos) }",
    ];
    for case in cases {
        assert_eq!(violations(case), Vec::new(), "잘못 걸렸다: {case}");
    }
}

/// 팝업 모듈 판정은 자기 id 상수를 문자열로 정의한 파일만 고른다.
#[test]
fn only_files_that_define_their_own_popup_id_are_popup_modules() {
    assert!(defines_own_popup_id(
        "pub(crate) const A_POPUP_ID: &str = \"a\";"
    ));
    assert!(defines_own_popup_id(
        "pub const A_POPUP_ID: super::PopupId = \"a\";"
    ));
    assert!(!defines_own_popup_id(
        "use super::x::A_POPUP_ID;\nfn f() {}"
    ));
    assert!(!defines_own_popup_id(
        "const RESIZE_BAND: LogicalPx = LogicalPx(6.0);"
    ));
}
