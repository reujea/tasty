//! 커밋에 포함되지 않는 로컬 작업 문서·계획 번호·디자인 변경 기록의 인용을 찾는다.
//! 다른 개발자가 읽을 수 없는 참조 대신 이유를 직접 쓰거나 ADR·기능 문서를 연결한다(docs/documentation-model.md).
//!
//! P1~P6은 번호·경로·디자인 slug·앵커·로컬 폴더, P7은 산문의 TODO 언급,
//! P8~P10은 작업 분할 번호·계획 좌표·회차 번호를 찾는다. 세부 형식은 각 find 함수에 있다.
//! P1~P6은 대소문자를 구분하지 않는다. 번호 앞에 임의 문장부호까지 허용하면
//! 일반 문장도 걸리므로 공백·경로 구분자·짝 맞는 괄호 등 패턴별 구분자를 사용한다.
//!
//! Git 명령 없이 디렉터리를 순회하므로 tarball에서도 실행된다. 실제 추적 여부를 묻지는 않으며
//! 빌드·로컬 폴더와 등록한 외부 번들을 제외하고, 나머지 UTF-8 파일을 검사한다.
//! P7은 규칙 설명을 허용하기 위해 Markdown을 검사하지 않는다.
//!
//! 사용자 홈의 지침 경로는 예외다. 해당 경로 바로 앞의 접두나 HOME_NEARBY 단어로 판단한다.
//! 변수의 실제 값은 모르므로 다른 이름으로 홈을 표현하면 오탐할 수 있다.
//! 필요하면 이름을 HOME_NEARBY에 등록하되, 같은 줄의 다른 로컬 참조까지 면제하지 않도록 한다.

// 이유: 테스트의 값 무시를 제품 코드의 lint 목록에서 제외한다.
#![allow(clippy::let_underscore_must_use)]
use std::path::{Path, PathBuf};

/// (저장소 상대 경로, 허용할 패턴 ID). 파일 전체가 아니라 필요한 패턴만 면제한다.
/// CLAUDE.md는 규칙의 번호·디자인 slug 예시, .gitignore는 제외 경로 자체가 필요하다.
/// 이 검사는 패턴 설명과 합성 입력, check-allow-reason은 빈 사유 금지 규칙을 설명한다.
/// vendor 사본의 상류 할 일 표시는 외부 원문을 유지하기 위해 예외로 둔다.
/// 면제되지 않은 패턴의 합성 입력은 fx!로 조립한다.
const ALLOWLIST: &[(&str, &[&str])] = &[
    ("CLAUDE.md", &["P1", "P4"]),
    (".gitignore", &["P6"]),
    (
        "crates/tasty-doc-guards/tests/no_todo_file_citation.rs",
        &["P7", "P8", "P9", "P10"],
    ),
    ("scripts/check-allow-reason.sh", &["P7"]),
    ("vendor/tiny_http/src/response.rs", &["P7"]),
    ("vendor/epaint/src/stats.rs", &["P7"]),
];

/// 탐지 패턴 표 — (id, 설명, 판정 함수). 한 줄에 대해 **전부** 돌린다.
type Finder = fn(&str) -> Option<String>;
const PATTERNS: &[(&str, &str, Finder)] = &[
    ("P1", "번호 인용", find_p1),
    ("P2", "conductor 번호 인용", find_p2),
    ("P3", "경로 인용", find_p3),
    ("P4", "디자인 changelog slug", find_p4),
    ("P5", "앵커 슬러그 번호", find_p5),
    ("P6", "로컬 폴더 언급", find_p6),
    ("P7", "산문 TODO 언급", find_p7),
    ("P8", "작업 분할 번호", find_p8),
    ("P9", "작업 계획 좌표", find_p9),
    ("P10", "회차 번호", find_p10),
];

/// 제외할 파일·디렉터리 이름. worktree의 .git은 파일일 수도 있다.
const PRUNE_DIRS: &[&str] = &[
    "target",
    "dist",
    "_site",
    ".worktree",
    ".git",
    ".idea",
    // 개발 도구의 캐시는 Git 추적 여부와 무관하게 제외한다.
    ".serena",
    ".playwright-mcp",
    "node_modules",
    // 로컬 에이전트 지침은 도구가 요구하는 이름을 유지해야 하므로 이름으로 제외한다.
    // 새 clone에는 없을 수 있어 경로 존재를 요구하는 ALLOWLIST에는 넣지 않는다.
    "AGENTS.md",
];

/// 외부 minified 번들만 파일별로 제외한다. 큰 번들의 패턴 우연 일치와 스캔 비용을 피한다.
/// 디렉터리 전체를 제외하면 직접 작성한 assets도 빠질 수 있다.
const VENDORED_FILES: &[&str] = &[
    "crates/tasty-plugin-markdown/assets/highlight.min.js",
    "crates/tasty-plugin-markdown/assets/katex.min.css",
    "crates/tasty-plugin-markdown/assets/katex.min.js",
    "crates/tasty-plugin-markdown/assets/mermaid.min.js",
];

/// gitignored 로컬 폴더 이름의 조각. 이 파일 자신이 P6 에 걸리지 않도록 나눠 둔다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

/// 로컬 작업 폴더 이름(선행 `.` 없음).
fn ws_dir() -> String {
    format!("{LOCAL_HEAD}{LOCAL_TAIL}")
}

/// (로컬 폴더 이름, 선행 점 필요 여부). 접두가 겹쳐 긴 이름부터 검사한다.
/// 사용자 홈에도 있는 이름은 선행 점과 직전 문맥으로 구분한다.
fn local_dirs() -> Vec<(String, bool)> {
    vec![(ws_dir(), false), (LOCAL_HEAD.to_string(), true)]
}

/// 로컬 폴더의 심볼릭 링크를 따라 저장소 밖까지 순회하지 않도록 이름으로 제외한다.
fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == ws_dir())
}

/// 이름이 다른 CARGO_TARGET_DIR도 캐시 표식으로 제외한다.
fn is_pruned_dir(path: &Path, name: &str) -> bool {
    is_pruned(name) || tasty_doc_guards::is_build_cache_dir(path)
}

/// 금지되는 하위 디렉토리 — 이 넷 뒤에 오는 경로만 P3 가 잡는다.
const FORBIDDEN_SUBDIRS: &[&str] = &["todo-conductor", "todo", "plans", "conductor"];

/// 이름 **바로 앞** 에 붙는 홈 경로 접두. 경로 구분자 한 겹은 벗기고 본다.
const HOME_PREFIXES: &[&str] = &["~", "$home", "%userprofile%"];

/// 이름 직전의 짧은 구간에서 홈 문맥으로 보는 단어다.
/// 영숫자 경계를 확인해 Homebrew 같은 다른 낱말로 예외가 적용되지 않게 한다.
const HOME_NEARBY: &[&str] = &["home", "claude_config_dir", "홈의"];

/// 직전 문맥을 보는 창 크기(문자 수).
const HOME_WINDOW: usize = 32;

fn skip_run(bytes: &[u8], mut i: usize, allowed: &[u8]) -> usize {
    while i < bytes.len() && allowed.contains(&bytes[i]) {
        i += 1;
    }
    i
}

/// `bytes[i..]` 가 숫자로 시작하면 그 숫자열의 끝 인덱스.
fn digits_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return None;
    }
    let mut end = i;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    Some(end)
}

/// 한국어 서수 접미사 — 번호 뒤에 붙는다(`21번`).
const KOREAN_ORDINAL: &str = "번";

/// 괄호 안의 번호와 선택적 번 접미사를 읽는다. 닫는 괄호까지 일치해야 번호 묶음이다.
fn bracketed_number_end(line: &str, i: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let close = match bytes.get(i)? {
        b'(' => b')',
        b'[' => b']',
        _ => return None,
    };
    let num_start = skip_run(bytes, i + 1, b" \t");
    let after_digits = digits_end(bytes, num_start)?;
    let after_ordinal = match line[after_digits..].strip_prefix(KOREAN_ORDINAL) {
        Some(_) => after_digits + KOREAN_ORDINAL.len(),
        None => after_digits,
    };
    let end = skip_run(bytes, after_ordinal, b" \t");
    (bytes.get(end) == Some(&close)).then_some(end + 1)
}

/// P1: todo 뒤의 번호 또는 번호+번 뒤의 todo. 대소문자를 구분하지 않는다.
/// 뒤 번호는 공백·탭·슬래시와 선택적 하이픈, 또는 짝 맞는 괄호 묶음을 허용한다.
/// 숫자를 요구하므로 할 일 표시와 일반 식별자는 대상이 아니다.
fn find_p1(line: &str) -> Option<String> {
    // ASCII 소문자화는 바이트 길이를 보존하므로 원문 슬라이스로 보고할 수 있다.
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("todo") {
        let start = from + pos;
        let after_space = skip_run(bytes, start + 4, b" \t/");
        let mut i = after_space;
        if i < bytes.len() && bytes[i] == b'-' {
            i += 1;
        }
        if let Some(end) = digits_end(bytes, i) {
            return Some(line[start..end].to_string());
        }
        if let Some(end) = bracketed_number_end(line, after_space) {
            return Some(line[start..end].to_string());
        }
        if let Some(num_start) = korean_ordinal_start(line, start) {
            return Some(line[num_start..start + 4].to_string());
        }
        from = start + 4;
    }
    None
}

/// 앞 어순 판정 — `line[..todo_at]` 의 꼬리가 `숫자+번` + 공백 런인지 본다.
/// 맞으면 숫자열이 시작하는 바이트 인덱스.
fn korean_ordinal_start(line: &str, todo_at: usize) -> Option<usize> {
    let head = line[..todo_at].trim_end_matches([' ', '\t']);
    let digits = head.strip_suffix(KOREAN_ORDINAL)?;
    let num_start = digits.len()
        - digits
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .count();
    (num_start < digits.len()).then_some(num_start)
}

/// P2 — `todo-conductor`(대소문자 무시) + 구분자 런 + 숫자.
fn find_p2(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let needle = "todo-conductor";
    let mut from = 0;
    while let Some(pos) = lower[from..].find(needle) {
        let start = from + pos;
        let i = skip_run(bytes, start + needle.len(), b" \t/_-");
        if let Some(end) = digits_end(bytes, i) {
            return Some(line[start..end].to_string());
        }
        from = start + needle.len();
    }
    None
}

/// P3 — 로컬 작업 폴더 + 금지 하위 디렉토리. 대소문자·슬래시 개수로 회피되지 않는다.
fn find_p3(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let dir = ws_dir();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(&dir) {
        let start = from + pos;
        from = start + dir.len();
        let after = skip_run(bytes, from, b"/\\");
        if after == from {
            continue; // 폴더 이름 뒤에 경로 구분자가 없다 — 하위 인용이 아니다.
        }
        if let Some(sub) = FORBIDDEN_SUBDIRS
            .iter()
            .find(|s| lower[after..].starts_with(**s))
        {
            return Some(format!("{dir}/{sub}"));
        }
    }
    None
}

/// P4 — 디자인 changelog 판정 slug(`YYYY-MM-DD-<slug>`). 대소문자를 가리지 않는다.
fn find_p4(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let is_d = |i: usize| i < bytes.len() && bytes[i].is_ascii_digit();
    let is_dash = |i: usize| i < bytes.len() && bytes[i] == b'-';
    for start in 0..bytes.len() {
        // 앞 글자가 숫자면 연도 4 자리의 시작이 아니다(더 긴 숫자열의 중간).
        if start > 0 && bytes[start - 1].is_ascii_digit() {
            continue;
        }
        if !(is_d(start) && is_d(start + 1) && is_d(start + 2) && is_d(start + 3)) {
            continue;
        }
        if !(is_dash(start + 4) && is_d(start + 5) && is_d(start + 6)) {
            continue;
        }
        if !(is_dash(start + 7) && is_d(start + 8) && is_d(start + 9)) {
            continue;
        }
        if !is_dash(start + 10) {
            continue;
        }
        let mut end = start + 11;
        if !(end < bytes.len() && bytes[end].is_ascii_alphabetic()) {
            continue;
        }
        while end < bytes.len() && (bytes[end].is_ascii_alphabetic() || bytes[end] == b'-') {
            end += 1;
        }
        return Some(line[start..end].to_string());
    }
    None
}

/// 소문자로 받은 앵커에서 시작 또는 하이픈 뒤의 todo-번호를 찾는다.
fn slug_todo_number(slug: &str) -> Option<String> {
    let bytes = slug.as_bytes();
    let needle = "todo-";
    let mut from = 0;
    while let Some(pos) = slug[from..].find(needle) {
        let start = from + pos;
        // `start - 1` 이 멀티바이트 연속 바이트여도 `-`(ASCII) 와는 절대 같지 않다.
        let at_boundary = start == 0 || bytes[start - 1] == b'-';
        let digits_start = skip_run(bytes, start + needle.len(), b"-");
        from = start + needle.len();
        if !at_boundary {
            continue;
        }
        if let Some(end) = digits_end(bytes, digits_start) {
            return Some(slug[start..end].to_string());
        }
    }
    None
}

/// P5 — 마크다운 앵커(`#<슬러그>`) 안에 굳은 번호.
fn find_p5(line: &str) -> Option<String> {
    let mut from = 0;
    while let Some(pos) = line[from..].find('#') {
        let start = from + pos;
        let rest = &line[start + 1..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '`' | ',' | '"' | '\''))
            .map(|i| start + 1 + i)
            .unwrap_or(line.len());
        let slug = &line[start + 1..end];
        if let Some(hit) = slug_todo_number(&slug.to_ascii_lowercase()) {
            return Some(format!("#{slug} ({hit})"));
        }
        from = start + 1;
    }
    None
}

/// 줄 전체가 아닌 해당 경로 직전만 확인해 같은 줄의 다른 참조까지 면제하지 않게 한다.
fn home_context_before(lower: &str, at: usize) -> bool {
    let head = &lower[..at];
    // 이스케이프되거나 반복된 경로 구분자를 모두 건너뛴다.
    let trimmed = head.trim_end_matches(['/', '\\']);
    if HOME_PREFIXES.iter().any(|p| trimmed.ends_with(p)) {
        return true;
    }
    let window = head
        .char_indices()
        .rev()
        .take(HOME_WINDOW)
        .last()
        .map_or(head, |(i, _)| &head[i..]);
    HOME_NEARBY.iter().any(|a| contains_word(window, a))
}

/// `hay` 안에 `word` 가 **낱말로** 있는가 — 앞뒤가 영숫자면 다른 낱말의 일부다.
fn contains_word(hay: &str, word: &str) -> bool {
    let bytes = hay.as_bytes();
    let mut from = 0;
    while let Some(pos) = hay[from..].find(word) {
        let start = from + pos;
        from = start + word.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = bytes.get(from).is_none_or(|c| !c.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// P6: 하위 경로 유무와 관계없이 로컬 폴더 언급을 찾는다.
fn find_p6(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for (name, needs_dot) in local_dirs() {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&name) {
            let start = from + pos;
            from = start + name.len();
            let dotted = start > 0 && bytes[start - 1] == b'.';
            if needs_dot && !dotted {
                continue; // 홈에도 있는 이름은 경로 표기일 때만 대상이다.
            }
            // 이름(또는 선행 `.`) 앞이 식별자 문자면 더 긴 이름의 일부다
            // (reverse-DNS plugin id, `tasty-plugin-<이름>` 등). 경로 시작이 아니다.
            let prev = if dotted {
                start.checked_sub(2)
            } else {
                start.checked_sub(1)
            };
            if let Some(p) = prev {
                let c = bytes[p];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                    continue;
                }
            }
            // 뒤가 식별자 문자면 다른 이름이다. 긴 이름을 먼저 보므로 로컬 작업
            // 폴더는 이 검사에 걸리기 전에 잡힌다.
            if let Some(&c) = bytes.get(from)
                && (c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                continue;
            }
            if needs_dot && home_context_before(&lower, start - 1) {
                continue;
            }
            return Some(if dotted {
                format!(".{name}")
            } else {
                name.clone()
            });
        }
    }
    None
}

/// TODO: 또는 TODO(범위):의 콜론 뒤 인덱스.
/// 닫는 괄호만 있는 산문을 할 일 표시로 잘못 허용하지 않도록 콜론도 요구한다.
fn todo_marker_end(line: &str, after_todo: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let colon_at = match bytes.get(after_todo)? {
        b':' => return Some(after_todo + 1),
        b'(' => after_todo + 1 + line[after_todo + 1..].find(')')? + 1,
        _ => return None,
    };
    (bytes.get(colon_at) == Some(&b':')).then_some(colon_at + 1)
}

/// P7: 할 일 표시가 아닌 대문자 TODO. 소문자는 일반 영단어·식별자와 구별하기 어려워 제외한다.
fn find_p7(line: &str) -> Option<String> {
    let mut from = 0;
    while let Some(pos) = line[from..].find("TODO") {
        let start = from + pos;
        let after = start + 4;
        from = after;
        if todo_marker_end(line, after).is_none() {
            return Some(line[start..after].to_string());
        }
    }
    None
}

/// P8: 괄호 안의 두 자리 번호 중 앞자리가 0인 형태만 찾는다.
/// 그 밖의 숫자는 각주·데이터·버전과 구별하기 어려워 이 검사로 판단하지 않는다.
fn find_p8(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(pos) = line[from..].find('(') {
        let start = from + pos;
        from = start + 1;
        let d0 = bytes.get(start + 1);
        let d1 = bytes.get(start + 2);
        if d0 == Some(&b'0')
            && d1.is_some_and(|c| c.is_ascii_digit() && *c != b'0')
            && bytes.get(start + 3) == Some(&b')')
        {
            return Some(line[start..start + 4].to_string());
        }
    }
    None
}

/// P9: 대문자·숫자·대문자·대문자를 점으로 연결한 계획 좌표를 찾는다.
/// 뒤에 항목이 더 붙어도 앞 네 항목만 읽는다. 숫자로만 된 버전은 대상이 아니다.
fn find_p9(line: &str) -> Option<String> {
    let b = line.as_bytes();
    for i in 0..b.len() {
        // 앞이 단어 문자면 좌표의 머리가 아니다.
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.') {
            continue;
        }
        if !b[i].is_ascii_uppercase() || b.get(i + 1) != Some(&b'.') {
            continue;
        }
        let mut j = i + 2;
        let ds = j;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j == ds || b.get(j) != Some(&b'.') {
            continue;
        }
        j += 1;
        if !b.get(j).is_some_and(u8::is_ascii_uppercase) || b.get(j + 1) != Some(&b'.') {
            continue;
        }
        j += 2;
        if b.get(j).is_some_and(u8::is_ascii_uppercase) {
            return Some(line[i..=j].to_string());
        }
    }
    None
}

/// P10: 식별자 안이 아닌 R 뒤의 두 자리 이상 숫자를 찾는다.
/// 한 자리 숫자와 한국어 회차 표기는 데이터·수량과 구별하기 어려워 제외한다.
/// 번호만 없애지 말고 그 번호가 대신하던 이유를 직접 설명해야 한다.
fn find_p10(line: &str) -> Option<String> {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'R' {
            continue;
        }
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
            continue;
        }
        let mut j = i + 1;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j - (i + 1) < 2 {
            continue;
        }
        if b.get(j)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            continue;
        }
        return Some(line[i..j].to_string());
    }
    None
}

/// Markdown은 규칙 설명에 TODO를 쓸 수 있으므로 P7만 제외한다.
fn out_of_scope(id: &str, rel: &str) -> bool {
    id == "P7" && rel.to_ascii_lowercase().ends_with(".md")
}

/// 등록 외부 번들과 바이너리 확장자 외에는 확장자 없는 파일도 검사한다.
fn is_scan_target(rel: &str) -> bool {
    if VENDORED_FILES.contains(&rel) {
        return false;
    }
    let name = rel.rsplit('/').next().unwrap_or("");
    // 선행 `.` 은 확장자 구분자가 아니다 — dotfile 은 확장자 없음으로 본다.
    let ext = name
        .trim_start_matches('.')
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext {
        Some(e) => !tasty_doc_guards::is_binary_artifact_ext(&e),
        None => true,
    }
}

/// 각 주요 디렉터리에서 한 파일 이상 수집했는지 확인한다.
/// 전체 파일 수의 증감 폭을 측정하지 않아 개수 하한 대신 경로별 존재를 확인한다.
/// 디렉터리 안의 일부 누락은 찾지 못한다. 수집량 하한을 추가하려면 정상적인 증감 범위도 재야 한다.
const MUST_BE_SCANNED: &[&str] = &["docs/", "src/", "crates/", "scripts/", ".github/"];

/// 읽기 실패로 하위 경로가 조용히 제외되지 않도록 디렉터리·항목 읽기 실패를 보고한다.
fn gather(path: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        let rel = rel_of(path, root);
        if is_scan_target(&rel) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let entries = std::fs::read_dir(path)
        .unwrap_or_else(|e| panic!("스캔 대상 디렉토리를 읽지 못했다: {} — {e}", path.display()));
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|e| panic!("디렉토리 항목을 읽지 못했다: {} — {e}", path.display()));
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // worktree의 .git은 파일일 수 있어 종류를 확인하기 전에 이름으로 제외한다.
        if is_pruned(name) {
            continue;
        }
        if p.is_dir() && is_pruned_dir(&p, name) {
            continue;
        }
        gather(&p, root, out);
    }
}

/// 정확한 파일 경로의 예외만 적용한다. 접두나 접미가 겹치는 다른 파일로 확장하지 않는다.
fn allowed_patterns(rel: &str) -> &'static [&'static str] {
    ALLOWLIST
        .iter()
        .find(|(f, _)| *f == rel)
        .map_or(&[], |(_, pats)| *pats)
}

/// 파일 순회 없이 예외 적용까지 합성 입력으로 검사할 수 있게 분리한다.
fn violations_in_line(rel: &str, line: &str) -> Vec<String> {
    let allowed = allowed_patterns(rel);
    PATTERNS
        .iter()
        .filter(|(id, _, _)| !allowed.contains(id) && !out_of_scope(id, rel))
        .filter_map(|(id, kind, find)| find(line).map(|m| format!("{id} {kind}: `{m}`")))
        .collect()
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn no_todo_file_citation() {
    let root = &tasty_doc_guards::repo_root();
    let mut files = Vec::new();
    gather(root, root, &mut files);
    files.sort();

    let mut violations = Vec::new();
    for file in files {
        let rel = rel_of(&file, root);
        let Ok(contents) = std::fs::read_to_string(&file) else {
            continue; // UTF-8로 읽지 못한 파일은 검사하지 않는다.
        };
        for (i, line) in contents.lines().enumerate() {
            for found in violations_in_line(&rel, line) {
                violations.push(format!("  {}:{} — {found}", rel, i + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "저장소에 없는 로컬 문서·계획 번호·디자인 변경 기록을 인용했다(docs/documentation-model.md). 이유를 직접 쓰거나 커밋된 ADR·기능 문서로 연결한다. 앵커를 고치면 제목을 가리키는 링크도 함께 고친다. 로컬 지침은 위치 대신 로컬 지침이 정한다는 설명만 남긴다. 할 일 표시 TODO:는 허용한다. 해당 형태 자체가 필요한 파일만 ALLOWLIST에 경로와 패턴을 등록한다:\n{}",
        violations.join("\n")
    );
}

/// 면제되지 않은 금지 패턴은 fx!로 조립해 검사 소스 자체가 위반이 되지 않게 한다.
macro_rules! fx {
    ($($p:literal),+ $(,)?) => { concat!($($p),+) };
}

#[test]
fn p1_catches_numbered_todo_citation_only() {
    assert_eq!(
        find_p1(fx!("see TODO", " 40")),
        Some(fx!("TODO", " 40").into())
    );
    assert_eq!(find_p1(fx!("(TODO", "18)")), Some(fx!("TODO", "18").into()));
    assert_eq!(
        find_p1(fx!("TODO", "-7 은 이미 닫혔다")),
        Some(fx!("TODO", "-7").into())
    );
    assert_eq!(
        find_p1(fx!("see TODO", "  40")),
        Some(fx!("TODO", "  40").into())
    );
    assert_eq!(
        find_p1(fx!("see TODO", " \t 40")),
        Some(fx!("TODO", " \t 40").into())
    );
    assert_eq!(
        find_p1(fx!("see TODO", " -40")),
        Some(fx!("TODO", " -40").into())
    );
    assert_eq!(find_p1("// TODO: refactor this later"), None);
    assert_eq!(find_p1("TODOS 는 소문자 아님"), None);
    assert_eq!(
        find_p1(fx!("17번 ", "TODO", " — mesh mirror")),
        Some(fx!("17번 ", "TODO").into())
    );
    assert_eq!(
        find_p1(fx!("(18번", "TODO", ")")),
        Some(fx!("18번", "TODO").into())
    );
    assert_eq!(find_p1(fx!("이번 ", "TODO", " 는 크다")), None);
    assert_eq!(find_p1(fx!("번 ", "TODO")), None);

    assert_eq!(find_p1("TODO: 40"), None);
    assert_eq!(find_p1("TODO. 40"), None);
    assert_eq!(find_p1("TODO #40"), None);
    assert_eq!(find_p1("TODO_40"), None);
}

#[test]
fn p1_is_not_evaded_by_case() {
    assert_eq!(
        find_p1(fx!("see todo", " 40")),
        Some(fx!("todo", " 40").into())
    );
    assert_eq!(find_p1(fx!("ToDo", "-7")), Some(fx!("ToDo", "-7").into()));
    assert_eq!(find_p1(fx!("(todo", "18)")), Some(fx!("todo", "18").into()));
    assert_eq!(
        find_p1(fx!("see Todo", " 40")).as_deref(),
        Some(fx!("Todo", " 40"))
    );
    assert_eq!(find_p1("// todo: refactor this later"), None);
    assert_eq!(find_p1("// todo(권한모델): 도입 후 대체"), None);
    assert_eq!(find_p1("let todos = todo_marker_end(line, 4);"), None);
    assert_eq!(find_p1("todo_40"), None);
}

#[test]
fn p1_takes_the_slash_as_a_separator() {
    assert_eq!(
        find_p1(fx!("(todo", "/52 R2)")),
        Some(fx!("todo", "/52").into())
    );
    assert_eq!(find_p1(fx!("TODO", "/7")), Some(fx!("TODO", "/7").into()));
    assert_eq!(
        find_p1(fx!("x/todo", "//3.md")),
        Some(fx!("todo", "//3").into())
    );
    assert_eq!(find_p1("TODO/changelog 어느 쪽 인용이든"), None);
    assert_eq!(find_p1("note/todo/abstract/quote"), None);
}

#[test]
fn p1_catches_bracketed_ticket_numbers() {
    assert_eq!(
        find_p1(fx!("원 TODO", "(11)는 close 계열을 다뤘다")),
        Some(fx!("TODO", "(11)").into())
    );
    assert_eq!(
        find_p1(fx!("구현 TODO", "(21번)가 정한다")),
        Some(fx!("TODO", "(21번)").into())
    );
    assert_eq!(
        find_p1(fx!("TODO", "[7] 참조")),
        Some(fx!("TODO", "[7]").into())
    );
    assert_eq!(
        find_p1(fx!("TODO", "  ( 40 )")),
        Some(fx!("TODO", "  ( 40 )").into())
    );

    assert_eq!(find_p1(fx!("TODO", " (2026 년부터 바뀐다")), None);
    assert_eq!(find_p1(fx!("TODO", "(11 참조")), None);
    assert_eq!(find_p1(fx!("TODO", "(11]")), None);
    assert_eq!(find_p1("TODO(alice): 나중에 고친다"), None);
    assert_eq!(find_p1("TODO(fixme)"), None);
    assert_eq!(find_p1("TODO(v2): 캐시를 뺀다"), None);
    assert_eq!(find_p1("TODO(3rd)"), None);
    assert_eq!(find_p1("TODO(0.1.59)"), None);
}

#[test]
fn p2_catches_conductor_ticket_numbers() {
    assert_eq!(
        find_p2(fx!("todo-conductor", "/12 참조")),
        Some(fx!("todo-conductor", "/12").into())
    );
    assert_eq!(
        find_p2(fx!("TODO-CONDUCTOR", " 3")),
        Some(fx!("TODO-CONDUCTOR", " 3").into())
    );
    assert_eq!(
        find_p2(fx!("todo-conductor", "//12")),
        Some(fx!("todo-conductor", "//12").into())
    );
    assert_eq!(
        find_p2(fx!("todo-conductor", "  12")),
        Some(fx!("todo-conductor", "  12").into())
    );
    assert_eq!(find_p2("todo-conductor 디렉토리"), None);
    assert_eq!(find_p2("todo-conductor#12"), None);
}

#[test]
fn p3_catches_workspace_subdir_paths() {
    assert_eq!(
        find_p3(fx!("claude", "-workspace/todo", "/3.md")),
        Some(fx!("claude", "-workspace/todo").into())
    );
    assert_eq!(
        find_p3(fx!("claude", "-workspace/Todo", "/3.md")),
        Some(fx!("claude", "-workspace/todo").into())
    );
    assert_eq!(
        find_p3(fx!("claude", "-workspace//todo", "/3.md")),
        Some(fx!("claude", "-workspace/todo").into())
    );
    assert_eq!(find_p3(fx!("claude", "-workspace/temp/x.png")), None);
}

#[test]
fn p4_catches_design_changelog_slug() {
    assert_eq!(
        find_p4(fx!("판정 slug 는 2026-07-03", "-spacing-offgrid 였다")),
        Some(fx!("2026-07-03", "-spacing-offgrid").into())
    );
    assert_eq!(
        find_p4(fx!("2026-07-03", "-Spacing-Offgrid")),
        Some(fx!("2026-07-03", "-Spacing-Offgrid").into())
    );
    assert_eq!(find_p4("Date: 2026-09-04"), None);
    assert_eq!(find_p4("id 120260-07-03-x"), None);
}

#[test]
fn p5_catches_anchor_slug_number() {
    assert!(find_p5(fx!("[링크](x.md#a-todo", "-12-b)")).is_some());
    assert!(find_p5(fx!("[링크](x.md#a-TODO", "-12-b)")).is_some());
    assert!(find_p5(fx!("[링크](x.md#a-todo", "--12)")).is_some());
    assert_eq!(find_p5("[링크](x.md#todo-conductor-notes)"), None);
    assert_eq!(find_p5(fx!("[링크](x.md#todo", "12)")), None);
    assert_eq!(find_p5("# 평범한 마크다운 제목"), None);
}

#[test]
fn p6_catches_local_workspace_mentions() {
    assert_eq!(
        find_p6(fx!("산출물은 .", "claude", "-workspace 아래")),
        Some(fx!(".", "claude", "-workspace").into())
    );
    assert_eq!(
        find_p6(fx!("스크린샷은 .", "claude", "-workspace/temp/ 에")),
        Some(fx!(".", "claude", "-workspace").into())
    );
    assert_eq!(
        find_p6(fx!("claude", "-workspace/temp 에 둔다")),
        Some(fx!("claude", "-workspace").into())
    );
    assert_eq!(
        find_p6(fx!(".", "CLAUDE", "-WORKSPACE/temp")),
        Some(fx!(".", "claude", "-workspace").into())
    );
    assert_eq!(
        find_p6(fx!("설정은 .", "claude", "/CLAUDE.md 가 정한다")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("./.", "claude", "/x 를 읽는다")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!(r".\.", "claude", r"\x 를 읽는다")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("폴더는 .", "claude", " 하나뿐")),
        Some(fx!(".", "claude").into())
    );
}

#[test]
fn p6_home_exemption_is_adjacent_not_line_wide() {
    assert_eq!(find_p6("~/.claude/settings.json 을 머지한다"), None);
    assert_eq!(find_p6("$HOME/.claude/projects 아래를 훑는다"), None);
    assert_eq!(find_p6("%USERPROFILE%\\.claude\\settings.json"), None);
    assert_eq!(find_p6("Ok(base.home_dir().join(\".claude\"))"), None);
    assert_eq!(find_p6("아니면 홈의 `.claude/projects`."), None);
    assert_eq!(
        find_p6("$CLAUDE_CONFIG_DIR 미설정 시 .claude/projects"),
        None
    );
    assert_eq!(
        find_p6(fx!("~/.tasty 와 .", "claude", "/CLAUDE.md 를 비교")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("Homebrew 설치 후 .", "claude", "/CLAUDE.md 수정")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!(
            "renderHome() 은 .",
            "claude",
            "/settings.json 을 읽는다"
        )),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("홈 화면 설정은 .", "claude", "/CLAUDE.md 가 정한다")),
        Some(fx!(".", "claude").into())
    );
}

#[test]
fn p6_allows_identifiers_and_build_outputs() {
    assert_eq!(find_p6("id = \"com.tasty.claude\""), None);
    assert_eq!(find_p6("com.tasty.claude-design 은 제거됐다"), None);
    assert_eq!(
        find_p6("crates/tasty-plugin-claude/tasty-plugin.toml.sig"),
        None
    );
    assert_eq!(find_p6("site/release.json 을 읽는다"), None);
    assert_eq!(find_p6("target/release/tasty-plugin-claude"), None);
}

#[test]
fn p7_catches_prose_todo_but_not_task_markers() {
    assert_eq!(
        find_p7("//! 이 TODO 는 순수 구조 이관이라"),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("// \"단일 `*` 만 지원\" 이라는 TODO 문서 초기 서술과 달리"),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("/// (conductor-scoped — a separate TODO if needed)."),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("/// 자기 소스에 `TODO` 로 남겨 두었으므로"),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("// tracked as a separate TODO."),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("// later TODOs route through the same dispatch"),
        Some("TODO".into())
    );

    assert_eq!(
        find_p7("// TODO: winit PR 머지 후 공식 버전으로 교체"),
        None
    );
    assert_eq!(
        find_p7("// TODO(권한모델): manifest 권한 도입 후 대체"),
        None
    );
    assert_eq!(find_p7("// TODO(emilk): upstream 이 정한다"), None);
    assert_eq!(
        find_p7("// TODO: 이 TODO 는 아래와 이어진다"),
        Some("TODO".into())
    );

    assert_eq!(find_p7("// TODO(alice) 나중에"), Some("TODO".into()));
    assert_eq!(find_p7("// TODO 나중에 고친다"), Some("TODO".into()));
    assert_eq!(find_p7("let todo = 3; // todo list 를 만든다"), None);
    assert_eq!(find_p7("fn todo_marker() {}"), None);
}

#[test]
fn p9_catches_dotted_plan_coordinates_only() {
    assert_eq!(
        find_p9("refactor (D.3.C.B.1 step 1)"),
        Some("D.3.C.B".into())
    );
    assert_eq!(
        find_p9("계획 D.3.C.G.3.c 의 마지막"),
        Some("D.3.C.G".into())
    );
    assert_eq!(find_p9("D.3.C.B.10.1 을 본다"), Some("D.3.C.B".into()));
    assert_eq!(find_p9("tasty 0.9.31 릴리스"), None);
    assert_eq!(find_p9("1.2.3.4"), None);
    assert_eq!(find_p9("A.1.B.c 는 아니다"), None);
    assert_eq!(find_p9("U.S.A.B 형식"), None);
    assert_eq!(find_p9("xD.3.C.B.1"), None);
}

#[test]
fn p10_catches_round_numbers_only() {
    assert_eq!(find_p10("위 R476 과 같은 부류다"), Some("R476".into()));
    assert_eq!(find_p10("(R56)"), Some("R56".into()));
    assert_eq!(find_p10("R1147 축"), Some("R1147".into()));
    assert_eq!(find_p10("R1 축과 R2 축"), None);
    assert_eq!(find_p10("let R12x = 1;"), None);
    assert_eq!(find_p10("RGB12_FOO"), None);
    assert_eq!(find_p10("VAR12"), None);
    assert_eq!(find_p10("xR476"), None);
    assert_eq!(find_p10("_R476"), None);
    assert_eq!(find_p10("r476"), None);
}

#[test]
fn p7_is_out_of_scope_in_markdown_only() {
    let prose = "이 TODO 는 순수 구조 이관이다";
    for rel in [
        "src/core/state/attention.rs",
        "scripts/bench/perf-10-surfaces.sh",
        ".github/workflows/test.yml",
        "site/vendor/gallery/components/index.jsx",
        "Justfile",
    ] {
        assert!(
            violations_in_line(rel, prose)
                .iter()
                .any(|v| v.starts_with("P7")),
            "P7 이 코드 자리에서 안 잡혔다: {rel}"
        );
    }
    for rel in [
        "CLAUDE.md",
        "docs/dev-guide/ci-gates.md",
        "site/content/help/troubleshooting.md",
        "crates/tasty-plugin-markdown/assets/NOTICE.md",
    ] {
        assert!(
            violations_in_line(rel, prose).is_empty(),
            "마크다운이 P7 범위에 들어왔다: {rel}"
        );
    }
    assert!(
        violations_in_line("docs/dev-guide/ci-gates.md", fx!("see TODO", " 40"))
            .iter()
            .any(|v| v.starts_with("P1"))
    );
}

#[test]
fn scan_target_covers_scripts_ci_and_root_docs() {
    assert!(is_scan_target("scripts/bench/perf-10-surfaces.sh"));
    assert!(is_scan_target("CLAUDE.md"));
    assert!(is_scan_target("crates/tasty-design-tokens/README.md"));
    assert!(is_scan_target(".github/workflows/test.yml"));
    assert!(is_scan_target(".githooks/pre-commit"));
    assert!(is_scan_target("Justfile"));
    assert!(is_scan_target("site/content/help/troubleshooting.md"));
    assert!(!is_scan_target("assets/icon.png"));
    assert!(!is_scan_target(
        "crates/tasty-plugin-claude/tasty-plugin.toml.sig"
    ));
}

/// worktree와 일반 checkout의 .git 형태가 다르므로 합성 파일로 이름 제외를 검사한다.
#[test]
fn pruning_is_by_name_not_by_kind() {
    let dir = std::env::temp_dir().join(format!("tasty-prune-kind-{}", std::process::id()));
    // 이전 실행의 임시 파일을 정리한다. 없어도 정상이다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리");
    std::fs::write(dir.join(".git"), "gitdir: elsewhere\n").expect("쓰기");
    std::fs::write(dir.join(".worktree"), "x\n").expect("쓰기");
    std::fs::write(dir.join("keep.md"), "x").expect("쓰기");

    let mut files = Vec::new();
    gather(&dir, &dir, &mut files);
    let mut seen: Vec<String> = files.iter().map(|f| rel_of(f, &dir)).collect();
    seen.sort();
    // 임시 파일 정리 실패가 검사 결과를 가리지 않게 한다.
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        seen,
        vec!["keep.md".to_string()],
        "가지치기가 종류를 물었다 — 가지치기 이름을 가진 파일이 모집단에 들어왔다"
    );
}

#[test]
fn a_build_dir_under_another_name_is_still_pruned() {
    let dir = std::env::temp_dir().join(format!("tasty-prune-{}", std::process::id()));
    // 임시 파일 정리 실패가 검사 결과를 가리지 않게 한다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리");

    assert!(
        !is_pruned_dir(&dir, "target-e2e-headless"),
        "표식이 없으면 이름이 빌드 디렉토리처럼 보여도 가지치기하지 않는다"
    );

    std::fs::write(
        dir.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .expect("표식 쓰기");
    assert!(
        is_pruned_dir(&dir, "target-e2e-headless"),
        "표식이 있으면 이름과 무관하게 가지치기한다"
    );

    // 임시 파일 정리 실패가 검사 결과를 가리지 않게 한다.
    let _ = std::fs::remove_dir_all(&dir);
}

/// 등록된 번들의 존재와 같은 디렉터리의 미등록 minified 번들을 함께 확인한다.
#[test]
fn the_vendored_list_matches_what_is_there() {
    let root = &tasty_doc_guards::repo_root();
    for rel in VENDORED_FILES {
        assert!(
            root.join(rel).is_file(),
            "vendored 목록이 없는 파일을 가리킨다: {rel} — 번들이 옮겨졌으면 목록도 옮겨라"
        );
    }

    let dir = root.join("crates/tasty-plugin-markdown/assets");
    let mut unlisted = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("vendored 디렉토리를 읽지 못했다") {
        let path = entry.expect("디렉토리 항목").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.ends_with(".min.js") || name.ends_with(".min.css")) {
            continue;
        }
        let rel = format!("crates/tasty-plugin-markdown/assets/{name}");
        if !VENDORED_FILES.contains(&rel.as_str()) {
            unlisted.push(rel);
        }
    }
    assert!(
        unlisted.is_empty(),
        "목록에 없는 minified 번들이 있다 — 열거를 갱신해라: {unlisted:?}"
    );
}

#[test]
fn our_own_files_under_assets_are_scanned() {
    for rel in [
        "assets/linux/tasty.desktop",
        "assets/icons/tasty-melon.svg",
        "crates/tasty-plugin-markdown/assets/NOTICE.md",
    ] {
        assert!(
            is_scan_target(rel),
            "우리가 쓴 파일이 스캔 대상에서 빠졌다: {rel}"
        );
        let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let name = dir.rsplit('/').next().unwrap_or("");
        assert!(
            !is_pruned(name),
            "그 파일이 든 디렉토리가 이름으로 가지치기된다: {dir}"
        );
    }
    assert!(!is_scan_target(
        "crates/tasty-plugin-markdown/assets/katex.min.js"
    ));
}

#[test]
fn prunes_build_outputs_and_local_dirs_but_not_assets() {
    assert!(is_pruned("target"));
    assert!(is_pruned("node_modules"));
    assert!(!is_pruned("assets"));
    assert!(is_pruned(fx!(".", "claude")));
    assert!(is_pruned(fx!(".", "claude", "-workspace")));
    assert!(!is_pruned(fx!("claude")));
    assert!(!is_pruned("src"));
}

#[test]
fn allowlist_exempts_only_the_named_pattern_not_the_whole_file() {
    let planted = fx!("claude", "-workspace/todo", "/3.md");
    let found = violations_in_line("CLAUDE.md", planted);
    assert!(
        found.iter().any(|v| v.starts_with("P3")),
        "면제 파일에 심은 비면제 패턴이 통과했다: {found:?}"
    );

    assert!(violations_in_line("CLAUDE.md", fx!("see TODO", " 40")).is_empty());
    assert!(
        violations_in_line("src/main.rs", fx!("see TODO", " 40"))
            .iter()
            .any(|v| v.starts_with("P1"))
    );

    let script = "scripts/check-allow-reason.sh";
    assert!(
        violations_in_line(script, fx!("(TODO", "18)"))
            .iter()
            .any(|v| v.starts_with("P1"))
    );
}

#[test]
fn allowlist_paths_match_exactly_not_by_prefix_or_suffix() {
    assert!(allowed_patterns("CLAUDE.md").contains(&"P1"));
    assert!(allowed_patterns("docs/CLAUDE.md").is_empty());
    assert!(allowed_patterns("CLAUDE.md.bak").is_empty());
    assert!(allowed_patterns("crates/x/CLAUDE.md").is_empty());
    assert!(allowed_patterns("").is_empty());
    assert!(!allowed_patterns("CLAUDE.md").contains(&"P3"));
    assert!(!allowed_patterns("CLAUDE.md").contains(&"P6"));
}

/// 예외의 파일·패턴 ID 존재만 확인한다. 예외가 아직 필요한지는 별도 검토해야 한다.
#[test]
fn allowlist_entries_point_at_things_that_exist() {
    let root = &tasty_doc_guards::repo_root();
    let ids: Vec<&str> = PATTERNS.iter().map(|(id, _, _)| *id).collect();
    for (rel, pats) in ALLOWLIST {
        assert!(
            root.join(rel).exists(),
            "면제 항목이 가리키는 파일이 없다 — 옮겼거나 지웠으면 항목도 지워라: {rel}"
        );
        assert!(
            !pats.is_empty(),
            "빈 면제 목록은 항목을 지우라는 뜻이다: {rel}"
        );
        for pat in *pats {
            assert!(
                ids.contains(pat),
                "면제가 없는 패턴 id 를 가리킨다(오탈자·개명): {rel} → {pat}"
            );
        }
    }
    assert!(
        !ALLOWLIST.is_empty(),
        "예외 목록이 비어 참조 검사가 실행되지 않았다. 예외가 모두 없어졌다면 이 검사도 제거한다."
    );
}

#[test]
fn the_scan_reaches_every_major_branch_of_the_tree() {
    let root = &tasty_doc_guards::repo_root();
    let mut files = Vec::new();
    gather(root, root, &mut files);
    let rels: Vec<String> = files.iter().map(|f| rel_of(f, root)).collect();
    let missing: Vec<&str> = MUST_BE_SCANNED
        .iter()
        .copied()
        .filter(|d| !rels.iter().any(|r| r.starts_with(d)))
        .collect();
    assert!(
        missing.is_empty(),
        "다음 경로에서 한 파일도 수집하지 못했다: {missing:?} (전체 {}개). is_pruned/is_scan_target/VENDORED_FILES와 순회 경로를 확인한다. 실제 검사 범위가 바뀐 근거 없이 경로를 목록에서 제거하지 않는다.",
        rels.len()
    );
}
