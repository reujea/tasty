//! 상태 모델 문서(docs/concepts/state-model/)가 코드와 같은 목록으로 적은 두 표를 실물과 대조한다.
//!
//! - surface.md "활성화 단계" 표: 이전 단계마다 허용되는 이후 단계. 실물은
//!   crates/tasty-core/src/evolve/lifecycle.rs의 `phase_transition`이 받는 쌍이고,
//!   단계 이름 집합은 crates/tasty-core/src/operation.rs의 `ActivationPhase`다.
//! - workspace.md "연결 상태" 표: 원격 행 상태 문자열과 사이드바 상태. 실물은
//!   src/app/auto_attach/row_use.rs의 `row_state_name`이 돌려주는 문자열과
//!   src/runtime/engine_read.rs의 `remote_row_state`가 고르는 `RemoteRowState` 값이다.
//!
//! 두 표 모두 행 순서는 보지 않고 집합으로 비교한다. 산문 문장에 적은 값(예: `ChildState` 목록)은
//! 검사하지 않는다. 코드 파싱은 해당 함수 본문의 정해진 형태만 읽는 단순 파서이며,
//! 형태가 바뀌어 아무것도 읽지 못하면 실패한다.

use std::collections::BTreeSet;

const SURFACE_DOC: &str = "docs/concepts/state-model/surface.md";
const WORKSPACE_DOC: &str = "docs/concepts/state-model/workspace.md";
const LIFECYCLE: &str = "crates/tasty-core/src/evolve/lifecycle.rs";
const OPERATION: &str = "crates/tasty-core/src/operation.rs";
const ROW_USE: &str = "src/app/auto_attach/row_use.rs";
const ENGINE_READ: &str = "src/runtime/engine_read.rs";

fn read(rel: &str) -> String {
    let path = tasty_doc_guards::repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}을 읽지 못했다: {e}"))
}

/// 함수 본문의 원본 조각과 같은 길이의 가린 사본(`mask_code`). 같은 바이트 오프셋이 같은 글자를
/// 가리키므로 구조는 `mask`에서 찾고 문자열 값은 같은 위치의 `src`에서 꺼낸다.
struct Masked {
    src: String,
    mask: String,
}

/// `mask`에서 `decl`이 시작하는 위치. 앞 글자가 식별자 글자이면 건너뛴다. 사본에서 찾으므로
/// 주석이나 문자열 속 같은 글자는 선언으로 보지 않는다. 스코프는 따지지 않으므로 같은 선언이
/// 파일에 둘 이상이면 어느 것을 읽을지 정하지 않고 실패한다.
fn find_decl(mask: &str, decl: &str, rel: &str) -> usize {
    let m = mask.as_bytes();
    let found: Vec<usize> = mask
        .match_indices(decl)
        .map(|(k, _)| k)
        .filter(|&k| k == 0 || !(m[k - 1].is_ascii_alphanumeric() || m[k - 1] == b'_'))
        .collect();
    match found.as_slice() {
        [] => panic!("{rel}에서 `{decl}` 선언을 찾지 못했다"),
        [k] => *k,
        _ => {
            let lines: Vec<usize> = found
                .iter()
                .map(|&k| mask[..k].matches('\n').count() + 1)
                .collect();
            panic!(
                "{rel}에 `{decl}` 선언이 둘 이상이다(줄 {lines:?}). 같은 이름 선언은 하나만 둔다"
            )
        }
    }
}

/// `mask[open]`의 여는 괄호와 짝을 이루는 닫는 괄호 위치. 세 종류 괄호를 함께 센다.
fn closing(mask: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in mask.bytes().enumerate().skip(open) {
        match c {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `fn <name>` 함수의 본문(여는 중괄호부터 닫는 중괄호까지). 파일 전체를 가린 사본에서 선언과
/// 중괄호 짝을 찾고, 같은 오프셋으로 원본을 자른다. 같은 이름 선언이 파일에 둘 이상이면 실패한다.
fn fn_body(source: &str, name: &str, rel: &str) -> Masked {
    let mask = mask_code(source);
    let start = find_decl(&mask, &format!("fn {name}("), rel);
    let open = start
        + mask[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{rel}의 fn {name}에 본문이 없다"));
    let close =
        closing(&mask, open).unwrap_or_else(|| panic!("{rel}의 fn {name} 본문이 닫히지 않는다"));
    Masked {
        src: source[open..=close].to_string(),
        mask: mask[open..=close].to_string(),
    }
}

/// `### <heading>` 절 안의 첫 Markdown 표에서 헤더와 구분 줄을 뺀 데이터 행의 칸들.
fn table_rows(doc: &str, heading: &str, rel: &str) -> Vec<Vec<String>> {
    let marker = format!("### {heading}\n");
    let start = doc
        .find(&marker)
        .unwrap_or_else(|| panic!("{rel}에서 `### {heading}` 절을 찾지 못했다"))
        + marker.len();
    let section = &doc[start..];
    let section = section.find("\n#").map_or(section, |end| &section[..end]);
    let rows: Vec<Vec<String>> = section
        .lines()
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .skip(2)
        .map(|line| {
            line.trim()
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .collect();
    assert!(
        !rows.is_empty(),
        "{rel}의 `### {heading}` 절에서 표 행을 하나도 읽지 못했다"
    );
    rows
}

/// 칸 안의 백틱 이름들. 백틱이 없는 칸(예: "없음")은 빈 목록이다.
fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// `pub enum <name>`의 변형 이름. 파일 전체를 가린 사본에서 선언과 본문을 찾으므로 주석은 읽지 않는다.
/// 같은 이름 선언이 파일에 둘 이상이면 실패한다. 속성(`#[...]`)은 괄호 묶음째 건너뛰고, 괄호 깊이
/// 0의 쉼표로 나눈 토막마다 앞쪽 식별자를 이름으로 읽는다. 원시 식별자 `r#Name`은 `Name`으로 읽는다.
fn enum_variants(source: &str, name: &str, rel: &str) -> BTreeSet<String> {
    let mask = mask_code(source);
    let start = find_decl(&mask, &format!("pub enum {name} {{"), rel);
    let open = start + mask[start..].find('{').unwrap_or(0);
    let close =
        closing(&mask, open).unwrap_or_else(|| panic!("{rel}의 enum {name}이 닫히지 않는다"));
    let body = &mask.as_bytes()[open + 1..close];
    let mut pieces = vec![String::new()];
    let mut i = 0;
    let mut depth = 0usize;
    while i < body.len() {
        let c = body[i];
        if c == b'#' && body.get(i + 1) == Some(&b'[') {
            let end = closing(&mask, open + 2 + i)
                .unwrap_or_else(|| panic!("{rel}의 enum {name}에 닫히지 않는 속성이 있다"));
            i = end - open;
            continue;
        }
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                pieces.push(String::new());
                i += 1;
                continue;
            }
            _ => {}
        }
        if let Some(piece) = pieces.last_mut() {
            piece.push(char::from(c));
        }
        i += 1;
    }
    pieces
        .iter()
        .map(|piece| {
            let piece = piece.trim();
            piece
                .strip_prefix("r#")
                .unwrap_or(piece)
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|v| !v.is_empty())
        .collect()
}

/// `phase_transition`의 `matches!` 안 `(A | B, C | D)` 쌍을 펼친다.
fn code_transitions(body: &str) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    let mut rest = body;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let Some((from, to)) = inner.split_once(',') else {
            continue;
        };
        let names = |side: &str| -> Vec<String> {
            side.split('|')
                .map(str::trim)
                .filter(|n| !n.is_empty() && n.chars().all(char::is_alphanumeric))
                .map(str::to_string)
                .collect()
        };
        for f in names(from) {
            for t in names(to) {
                pairs.insert((f.clone(), t.clone()));
            }
        }
    }
    pairs
}

fn doc_transitions(rows: &[Vec<String>]) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    let mut froms = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for row in rows {
        let [from, to] = row.as_slice() else {
            panic!("{SURFACE_DOC} 활성화 단계 표의 행은 두 칸이어야 한다: {row:?}");
        };
        for f in backticked(from) {
            froms.insert(f.clone());
            for t in backticked(to) {
                pairs.insert((f.clone(), t));
            }
        }
    }
    (froms, pairs)
}

#[test]
fn activation_transition_table_matches_the_journal_rule() {
    let lifecycle = read(LIFECYCLE);
    let variants = enum_variants(&read(OPERATION), "ActivationPhase", OPERATION);
    // 시그니처와 `matches!`의 `(from, to)`처럼 단계 이름이 아닌 괄호는 버린다.
    let code: BTreeSet<_> =
        code_transitions(&fn_body(&lifecycle, "phase_transition", LIFECYCLE).mask)
            .into_iter()
            .filter(|(f, t)| variants.contains(f) && variants.contains(t))
            .collect();
    assert!(
        code.len() >= 10,
        "{LIFECYCLE}의 phase_transition에서 전이 쌍을 {}개만 읽었다. 파서가 형태를 놓쳤다",
        code.len()
    );
    let rows = table_rows(&read(SURFACE_DOC), "활성화 단계", SURFACE_DOC);
    let (froms, doc) = doc_transitions(&rows);
    assert_eq!(
        froms, variants,
        "{SURFACE_DOC} 활성화 단계 표의 '이전' 열은 ActivationPhase의 모든 값을 한 번씩 담아야 한다"
    );
    let missing: Vec<_> = code.difference(&doc).collect();
    let extra: Vec<_> = doc.difference(&code).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{SURFACE_DOC} 활성화 단계 표가 {LIFECYCLE}의 phase_transition과 다르다. \
         문서에 없는 전이: {missing:?}, 코드에 없는 전이: {extra:?}"
    );
}

/// `b[i..]`에서 시작하는 리터럴의 (내용 시작, 내용 끝, 리터럴 끝). 다루는 리터럴은 일반·바이트·C
/// 문자열(`"…"`·`b"…"`·`c"…"`, 이스케이프 포함), 원시·바이트 원시·C 원시 문자열(`r#"…"#`·`br#"…"#`·
/// `cr#"…"#`, 해시 개수까지 맞춘다), 문자·바이트 문자 리터럴(`'x'`·`'\''`·`'\u{..}'`·`b'x'`)이다.
/// 따옴표 다음이 역슬래시이거나 한 글자 뒤가 따옴표일 때만 문자 리터럴로 보므로 수명(`'a`)은
/// 코드로 남는다. 앞 글자가 식별자 글자이면 `b`·`c`·`r`을 접두로 보지 않지만, 그 뒤에 바로 오는
/// 큰따옴표는 일반 문자열로 읽는다.
fn literal_span(code: &str, i: usize) -> Option<(usize, usize, usize)> {
    let b = code.as_bytes();
    let at = |k: usize| b.get(k).copied();
    let mut j = i;
    if i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        if matches!(at(j), Some(b'b' | b'c')) {
            j += 1;
        }
        if at(j) == Some(b'r') {
            let hashes = b[j + 1..].iter().take_while(|&&c| c == b'#').count();
            let quote = j + 1 + hashes;
            if at(quote) != Some(b'"') {
                return None;
            }
            let close = format!("\"{}", "#".repeat(hashes));
            let end = code[quote + 1..]
                .find(&close)
                .map_or(b.len(), |e| quote + 1 + e);
            return Some((quote + 1, end, (end + close.len()).min(b.len())));
        }
    }
    match at(j) {
        Some(b'"') => {
            let mut k = j + 1;
            while k < b.len() && b[k] != b'"' {
                k += if b[k] == b'\\' { 2 } else { 1 };
            }
            let k = k.min(b.len());
            Some((j + 1, k, (k + 1).min(b.len())))
        }
        Some(b'\'') if j == i || at(i) == Some(b'b') => {
            let close = if at(j + 1) == Some(b'\\') {
                j + 3 + code.get(j + 3..)?.find('\'')?
            } else {
                let after = j + 1 + code.get(j + 1..)?.chars().next()?.len_utf8();
                (at(after) == Some(b'\'')).then_some(after)?
            };
            Some((j + 1, close, close + 1))
        }
        _ => None,
    }
}

/// `code`와 같은 길이의 가린 사본. 줄·블록 주석(중첩 포함)은 공백으로, 리터럴(`literal_span`)은
/// 접두와 따옴표를 남기고 안쪽을 공백으로 바꾼다. 줄바꿈은 그대로 둔다.
fn mask_code(code: &str) -> String {
    let b = code.as_bytes();
    let mut out = b.to_vec();
    let mut blank = |from: usize, to: usize| {
        for c in &mut out[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"//") {
            let end = b[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(b.len(), |e| i + e);
            blank(i, end);
            i = end;
        } else if b[i..].starts_with(b"/*") {
            let (mut k, mut depth) = (i, 0usize);
            while k < b.len() {
                if b[k..].starts_with(b"/*") {
                    depth += 1;
                    k += 2;
                } else if b[k..].starts_with(b"*/") {
                    depth -= 1;
                    k += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    k += 1;
                }
            }
            blank(i, k);
            i = k;
        } else if let Some((from, to, end)) = literal_span(code, i) {
            blank(from, to);
            i = end;
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).expect("ASCII 공백으로 바꾼 사본은 UTF-8이다")
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// match 갈래 하나. `pattern`·`value`는 가린 사본의 글자(공백 정규화), `*_strings`는 그 범위의
/// 문자열 리터럴 내용을 원본에서 꺼낸 것이고 `text`는 실패 메시지용 원본이다.
struct Arm {
    pattern: String,
    value: String,
    pattern_strings: Vec<String>,
    value_strings: Vec<String>,
    text: String,
}

/// `body.mask[from..to]`의 큰따옴표 쌍마다 같은 위치의 원본 내용. 사본에 남은 큰따옴표는
/// 문자열 리터럴의 여닫이뿐이다.
fn strings_in(body: &Masked, from: usize, to: usize) -> Vec<String> {
    let quotes: Vec<usize> = body.mask[from..to]
        .match_indices('"')
        .map(|(k, _)| from + k)
        .collect();
    quotes
        .chunks(2)
        .map(|pair| match pair {
            [open, close] => body.src[open + 1..*close].to_string(),
            _ => panic!("짝이 없는 큰따옴표: {}", &body.src[from..to]),
        })
        .collect()
}

/// 함수 본문의 첫 `match` 블록을 갈래 목록으로 나눈다. `match` 키워드, 블록 시작, 괄호 깊이 0의
/// 쉼표, 화살표는 모두 가린 사본에서 찾으므로 주석이나 리터럴 속 글자는 구조로 읽지 않는다.
/// 여러 줄로 나뉜 OR 패턴도 갈래 하나로 모인다. 화살표(`=>`)가 없는 갈래가 있으면 실패한다.
fn match_arms(body: &Masked, what: &str) -> Vec<Arm> {
    let m = body.mask.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let start = body
        .mask
        .match_indices("match")
        .map(|(k, _)| k)
        .find(|&k| {
            (k == 0 || !ident(m[k - 1])) && m.get(k + 5).is_some_and(u8::is_ascii_whitespace)
        })
        .unwrap_or_else(|| panic!("{what}에서 match를 찾지 못했다"));
    let open = start
        + body.mask[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{what}의 match에 블록이 없다"));
    let mut ranges = Vec::new();
    let mut from = open + 1;
    let mut depth = 0usize;
    for (k, &c) in m.iter().enumerate().skip(open + 1) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' => depth -= 1,
            b'}' if depth == 0 => {
                ranges.push((from, k));
                break;
            }
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                ranges.push((from, k));
                from = k + 1;
            }
            _ => {}
        }
    }
    ranges
        .into_iter()
        .filter(|&(from, to)| !body.mask[from..to].trim().is_empty())
        .map(|(from, to)| {
            let text = normalized(&body.src[from..to]);
            let arrow = from
                + body.mask[from..to]
                    .find("=>")
                    .unwrap_or_else(|| panic!("{what}에 화살표 없는 갈래가 있다: {text}"));
            Arm {
                pattern: normalized(&body.mask[from..arrow]),
                value: normalized(&body.mask[arrow + 2..to]),
                pattern_strings: strings_in(body, from, arrow),
                value_strings: strings_in(body, arrow + 2, to),
                text,
            }
        })
        .collect()
}

/// `row_state_name`이 돌려주는 문자열. 갈래 값이 문자열 리터럴 하나가 아니면 실패한다.
fn code_row_states(body: &Masked) -> BTreeSet<String> {
    let what = format!("{ROW_USE}의 row_state_name");
    match_arms(body, &what)
        .into_iter()
        .map(|arm| {
            let one_literal = arm.value.len() >= 2
                && arm.value.starts_with('"')
                && arm.value.ends_with('"')
                && arm.value[1..arm.value.len() - 1].trim().is_empty();
            match arm.value_strings.as_slice() {
                [name] if one_literal => name.clone(),
                _ => panic!(
                    "{what}의 갈래 값이 문자열 리터럴 하나가 아니다: {}",
                    arm.text
                ),
            }
        })
        .collect()
}

/// `remote_row_state`의 문자열 → `S::<값>` 대응과 나머지 갈래(`_ =>`)의 값.
/// OR 패턴은 줄 수와 관계없이 모든 문자열을 같은 값으로 읽는다. 가드가 있거나 문자열이 없는
/// 패턴, `S::<이름>`이 아닌 값처럼 읽지 못하는 형태는 실패한다.
fn code_sidebar_map(body: &Masked) -> (Vec<(String, String)>, String) {
    let what = format!("{ENGINE_READ}의 remote_row_state");
    let mut map = Vec::new();
    let mut fallback = None;
    for arm in match_arms(body, &what) {
        let name = arm
            .value
            .strip_prefix("S::")
            .filter(|v| !v.is_empty() && v.chars().all(char::is_alphanumeric))
            .unwrap_or_else(|| panic!("{what}의 갈래 값이 `S::<이름>`이 아니다: {}", arm.text))
            .to_string();
        if arm.pattern == "_" {
            fallback = Some(name);
            continue;
        }
        assert!(
            !arm.pattern_strings.is_empty() && !arm.pattern.contains(" if "),
            "{what}에 읽지 못하는 패턴이 있다(문자열 리터럴과 `|`만 읽는다): {}",
            arm.text
        );
        for n in arm.pattern_strings {
            map.push((n, name.clone()));
        }
    }
    let fallback = fallback.unwrap_or_else(|| panic!("{what}에서 `_ => S::..` 갈래를 찾지 못했다"));
    (map, fallback)
}

#[test]
fn row_state_table_matches_the_row_state_source() {
    let row_use = read(ROW_USE);
    let code = code_row_states(&fn_body(&row_use, "row_state_name", ROW_USE));
    assert!(
        code.len() >= 3,
        "{ROW_USE}의 row_state_name에서 상태 문자열을 {}개만 읽었다. 파서가 형태를 놓쳤다",
        code.len()
    );
    let engine_read = read(ENGINE_READ);
    let (map, fallback) = code_sidebar_map(&fn_body(&engine_read, "remote_row_state", ENGINE_READ));

    let rows = table_rows(&read(WORKSPACE_DOC), "연결 상태", WORKSPACE_DOC);
    let mut doc = BTreeSet::new();
    for row in &rows {
        let [name, _, sidebar] = row.as_slice() else {
            panic!("{WORKSPACE_DOC} 연결 상태 표의 행은 세 칸이어야 한다: {row:?}");
        };
        let [name] = backticked(name)
            .try_into()
            .unwrap_or_else(|v: Vec<String>| {
                panic!("{WORKSPACE_DOC} 연결 상태 표 첫 칸에는 백틱 이름이 하나여야 한다: {v:?}")
            });
        let [sidebar] = backticked(sidebar)
            .try_into()
            .unwrap_or_else(|v: Vec<String>| {
                panic!("{WORKSPACE_DOC} 연결 상태 표 셋째 칸에는 백틱 이름이 하나여야 한다: {v:?}")
            });
        let expected = map
            .iter()
            .find(|(s, _)| *s == name)
            .map_or(fallback.as_str(), |(_, v)| v.as_str());
        assert_eq!(
            sidebar, expected,
            "{WORKSPACE_DOC} 연결 상태 표의 `{name}` 사이드바 값이 {ENGINE_READ}의 remote_row_state와 다르다"
        );
        doc.insert(name);
    }
    let missing: Vec<_> = code.difference(&doc).collect();
    let extra: Vec<_> = doc.difference(&code).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{WORKSPACE_DOC} 연결 상태 표가 {ROW_USE}의 row_state_name과 다르다. \
         문서에 없는 값: {missing:?}, 코드에 없는 값: {extra:?}"
    );
}
