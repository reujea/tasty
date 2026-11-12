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

/// `fn <name>` 선언부터 그 함수의 닫는 중괄호까지. 중괄호 수만 센다.
fn fn_body<'a>(source: &'a str, name: &str, rel: &str) -> &'a str {
    let start = source
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("{rel}에서 fn {name}을 찾지 못했다"));
    let open = start
        + source[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{rel}의 fn {name}에 본문이 없다"));
    let mut depth = 0usize;
    for (i, c) in source[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("{rel}의 fn {name} 본문이 닫히지 않는다")
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

fn enum_variants(source: &str, name: &str, rel: &str) -> BTreeSet<String> {
    let start = source
        .find(&format!("pub enum {name} {{"))
        .unwrap_or_else(|| panic!("{rel}에서 enum {name}을 찾지 못했다"));
    let body = &source[start..];
    let body = &body[body.find('{').unwrap_or(0) + 1..body.find('}').unwrap_or(body.len())];
    body.split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
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
    let code: BTreeSet<_> = code_transitions(fn_body(&lifecycle, "phase_transition", LIFECYCLE))
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

/// 줄 주석(`//`)과 블록 주석(`/* */`)을 지운 코드. 문자열 리터럴 안의 `//`는 그대로 둔다.
/// 주석 속 `=> "x"`·`=> S::X`를 갈래로 읽지 않게 한다.
fn strip_comments(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut chars = code.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut depth = 1usize;
                while depth > 0 {
                    match (chars.next(), chars.peek()) {
                        (Some('*'), Some('/')) => {
                            chars.next();
                            depth -= 1;
                        }
                        (Some('/'), Some('*')) => {
                            chars.next();
                            depth += 1;
                        }
                        (Some(_), _) => {}
                        (None, _) => break,
                    }
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// 함수 본문의 첫 `match` 블록을 갈래 `(패턴, 값)` 목록으로 나눈다. 주석을 먼저 지우고,
/// 괄호 깊이 0의 쉼표로 갈래를 자르므로 여러 줄로 나뉜 OR 패턴도 갈래 하나로 모인다.
/// 화살표(`=>`)가 없는 갈래가 있으면 실패한다.
fn match_arms(body: &str, what: &str) -> Vec<(String, String)> {
    let code = strip_comments(body);
    let start = code
        .find("match ")
        .unwrap_or_else(|| panic!("{what}에서 match를 찾지 못했다"));
    let open = start
        + code[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{what}의 match에 블록이 없다"));
    let mut arms = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for c in code[open + 1..].chars() {
        if in_string {
            current.push(c);
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth -= 1,
            '}' if depth == 0 => break,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                arms.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    arms.push(current);
    arms.iter()
        .map(|arm| arm.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|arm| !arm.is_empty())
        .map(|arm| {
            let (pattern, value) = arm
                .split_once("=>")
                .unwrap_or_else(|| panic!("{what}에 화살표 없는 갈래가 있다: {arm}"));
            (pattern.trim().to_string(), value.trim().to_string())
        })
        .collect()
}

/// 패턴 안의 문자열 리터럴 전부. `Some("a" | "b")` 같은 OR 패턴은 두 문자열을 모두 돌려준다.
fn quoted(pattern: &str) -> Vec<String> {
    pattern
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// `row_state_name`이 돌려주는 문자열. 갈래 값이 문자열 리터럴 하나가 아니면 실패한다.
fn code_row_states(body: &str) -> BTreeSet<String> {
    let what = format!("{ROW_USE}의 row_state_name");
    match_arms(body, &what)
        .into_iter()
        .map(|(_, value)| {
            let names = quoted(&value);
            match names.as_slice() {
                [name] if value == format!("\"{name}\"") => name.clone(),
                _ => panic!("{what}의 갈래 값이 문자열 리터럴 하나가 아니다: {value}"),
            }
        })
        .collect()
}

/// `remote_row_state`의 문자열 → `S::<값>` 대응과 나머지 갈래(`_ =>`)의 값.
/// OR 패턴은 줄 수와 관계없이 모든 문자열을 같은 값으로 읽는다. 가드가 있거나 문자열이 없는
/// 패턴, `S::<이름>`이 아닌 값처럼 읽지 못하는 형태는 실패한다.
fn code_sidebar_map(body: &str) -> (Vec<(String, String)>, String) {
    let what = format!("{ENGINE_READ}의 remote_row_state");
    let mut map = Vec::new();
    let mut fallback = None;
    for (pattern, value) in match_arms(body, &what) {
        let name = value
            .strip_prefix("S::")
            .filter(|v| !v.is_empty() && v.chars().all(char::is_alphanumeric))
            .unwrap_or_else(|| {
                panic!("{what}의 갈래 값이 `S::<이름>`이 아니다: {pattern} => {value}")
            })
            .to_string();
        if pattern == "_" {
            fallback = Some(name);
            continue;
        }
        let names = quoted(&pattern);
        assert!(
            !names.is_empty() && !pattern.contains(" if "),
            "{what}에 읽지 못하는 패턴이 있다(문자열 리터럴과 `|`만 읽는다): {pattern}"
        );
        for n in names {
            map.push((n, name.clone()));
        }
    }
    let fallback = fallback.unwrap_or_else(|| panic!("{what}에서 `_ => S::..` 갈래를 찾지 못했다"));
    (map, fallback)
}

#[test]
fn row_state_table_matches_the_row_state_source() {
    let row_use = read(ROW_USE);
    let code = code_row_states(fn_body(&row_use, "row_state_name", ROW_USE));
    assert!(
        code.len() >= 3,
        "{ROW_USE}의 row_state_name에서 상태 문자열을 {}개만 읽었다. 파서가 형태를 놓쳤다",
        code.len()
    );
    let engine_read = read(ENGINE_READ);
    let (map, fallback) = code_sidebar_map(fn_body(&engine_read, "remote_row_state", ENGINE_READ));

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
