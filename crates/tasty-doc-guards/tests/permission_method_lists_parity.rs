//! 권한 문서의 메서드 목록을 METHOD_TABLE과 대조한다.
//! 메서드만 나열한 행은 전체 집합을, 전부라고 쓴 글롭은 해당 접두 전체를 비교한다.
//! 등을 붙인 행은 일부 예시이므로 적힌 메서드만 확인한다. 검사 범위를 임의로 줄이지 않도록
//! 그런 행의 개수도 고정한다.
//!
//! memory.read/write의 조회·변경 한정어는 자동 분류하지 못한다. 대신 두 권한의 합이
//! secret을 제외한 memory 메서드 전부를 포함하고 서로 겹치지 않는지 확인한다.
//!
//! 백틱 항목은 METHOD_TABLE에서 먼저 찾고, 없으면 권한 토큰인지 확인한다.
//! 나머지 중 점이 없는 이름은 산문으로 본다. 따라서 점이 없는 메서드명의 오타는 놓칠 수 있다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tasty_doc_guards::{KNOWN_CTORS, method_table};

const TOKEN_SOURCE: &str = "crates/tasty-plugin-manifest/src/types.rs";
const METHOD_SOURCE: &str = "crates/tasty-ipc/src/method_meta.rs";
const DOC: &str = "docs/dev-guide/plugin-permissions.md";

/// 역방향 누락 검사를 하지 않는 부분 목록의 수다. 행 분류가 바뀌면 함께 검토한다.
const OPEN_ROWS: usize = 4;

fn root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `Permission::as_token` 의 팔에서 variant → 토큰을 뽑는다.
fn variant_tokens(src: &str) -> BTreeMap<String, String> {
    let start = src
        .find("pub fn as_token")
        .expect("as_token 을 못 찾았다 — 파서가 낡았다");
    let body = &src[start..];
    let end = body
        .find("\n    }\n")
        .expect("as_token 본문의 끝을 못 찾았다");
    let mut out = BTreeMap::new();
    for line in body[..end].lines() {
        let Some(rest) = line.trim().strip_prefix("Self::") else {
            continue;
        };
        let variant: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if variant.is_empty() {
            continue;
        }
        let Some(q0) = line.find('"') else { continue };
        let Some(q1) = line[q0 + 1..].find('"') else {
            continue;
        };
        out.insert(variant, line[q0 + 1..q0 + 1 + q1].to_string());
    }
    out
}

/// 소스의 MethodMeta 생성자를 도출해 파서가 모르는 형식이 추가됐는지 확인한다.
fn constructors_in_source(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("const fn ") else {
            continue;
        };
        if !t.contains("-> MethodMeta") {
            continue;
        }
        if let Some((name, _)) = rest.split_once('(') {
            out.insert(name.trim().to_string());
        }
    }
    out
}

/// 문서 표의 `| \`토큰\` | 메서드 열 | …` 행.
fn doc_rows(md: &str, tokens: &BTreeSet<String>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in md.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() < 4 {
            continue;
        }
        let head = cells[1].trim();
        let Some(tok) = head.strip_prefix('`').and_then(|s| s.strip_suffix('`')) else {
            continue;
        };
        // scoped 토큰은 문서에 `ipc.invoke:<prefix>` 처럼 자리표시자와 함께 실린다.
        let base = tok.split(':').next().unwrap_or(tok);
        let scoped = format!("{base}:");
        if tokens.contains(tok) || tokens.contains(&scoped) {
            out.push((tok.to_string(), cells[2].to_string()));
        }
    }
    out
}

/// 열에서 백틱으로 감싼 항목을 뽑는다.
fn backticked(col: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = col;
    while let Some(a) = rest.find('`') {
        let after = &rest[a + 1..];
        let Some(b) = after.find('`') else { break };
        out.push(after[..b].to_string());
        rest = &after[b + 1..];
    }
    out
}

/// `a.b.c/d/e` 축약을 편다 — 슬래시 뒤는 **마지막 점 뒤 세그먼트**를 갈아 끼운다.
fn expand(lit: &str) -> Vec<String> {
    if !lit.contains('/') {
        return vec![lit.to_string()];
    }
    let mut parts = lit.split('/');
    let first = parts.next().unwrap_or_default().to_string();
    let base = match first.rfind('.') {
        Some(i) => first[..i].to_string(),
        None => return vec![first],
    };
    let mut out = vec![first];
    for p in parts {
        out.push(format!("{base}.{p}"));
    }
    out
}

#[derive(PartialEq, Debug)]
enum Kind {
    /// 나열이 곧 전부라는 주장.
    Closed,
    /// `` `x.*` 전부 `` — 접두로 양방향 검사.
    GlobTotal,
    /// 한국어 한정어가 붙은 글롭(`memory.bb_*` 조회) — 쌍 단위로만 검사.
    GlobQualified,
    /// `등` — 불완전을 명시적으로 선언한 행.
    Open,
}

fn kind(col: &str) -> Kind {
    if col.contains("전부") {
        return Kind::GlobTotal;
    }
    if col.contains('등') {
        return Kind::Open;
    }
    if backticked(col).iter().any(|b| b.contains('*')) {
        return Kind::GlobQualified;
    }
    Kind::Closed
}

struct Model {
    tok: BTreeMap<String, String>,
    meth: BTreeMap<String, Option<Vec<String>>>,
    rows: Vec<(String, String)>,
    by_tok: BTreeMap<String, BTreeSet<String>>,
}

fn model() -> Model {
    let tok = variant_tokens(&read(TOKEN_SOURCE));
    let meth = method_table(&read(METHOD_SOURCE));
    let tokens: BTreeSet<String> = tok.values().cloned().collect();
    let rows = doc_rows(&read(DOC), &tokens);
    let mut by_tok: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, req) in &meth {
        let Some(vs) = req else { continue };
        for v in vs {
            let t = tok
                .get(v)
                .unwrap_or_else(|| panic!("METHOD_TABLE 의 variant `{v}` 가 as_token 에 없다"));
            by_tok.entry(t.clone()).or_default().insert(name.clone());
        }
    }
    Model {
        tok,
        meth,
        rows,
        by_tok,
    }
}

/// 모르는 생성자를 건너뛰어 문서 오류로 오인하지 않도록 파서의 지원 목록과 대조한다.
#[test]
fn the_known_constructors_cover_what_the_source_defines() {
    let src = read(METHOD_SOURCE);
    let defined = constructors_in_source(&src);
    assert!(
        defined.len() >= 3,
        "MethodMeta 생성자를 {}개만 찾았다(2026-09-05 측정 3개). const fn의 반환 형식과 파서를 확인한다.",
        defined.len()
    );
    let known: BTreeSet<String> = KNOWN_CTORS.iter().map(|(n, _)| (*n).to_string()).collect();
    let unhandled: Vec<&String> = defined.difference(&known).collect();
    assert!(
        unhandled.is_empty(),
        "파서가 지원하지 않는 MethodMeta 생성자가 있다. 호출 가능 범위를 확인해 KNOWN_CTORS를 갱신한다: {unhandled:?}"
    );
    let stale: Vec<&String> = known.difference(&defined).collect();
    assert!(
        stale.is_empty(),
        "KNOWN_CTORS에 있지만 소스에 없는 생성자다: {stale:?}"
    );
}

/// 빈 결과와 항목 형식별 누락을 따로 확인한다.
#[test]
fn the_parsers_are_alive() {
    let m = model();
    assert!(
        m.tok.len() >= 25,
        "as_token 파싱이 깨졌다 — variant {}개",
        m.tok.len()
    );
    assert!(
        m.meth.len() >= 200,
        "METHOD_TABLE 파싱이 깨졌다 — 항목 {}개",
        m.meth.len()
    );
    assert!(
        m.rows.len() >= 25,
        "문서 표 파싱이 깨졌다 — 행 {}개",
        m.rows.len()
    );
    assert_eq!(
        m.meth.get("terminal.tell").cloned().flatten(),
        Some(vec!["TerminalWrite".to_string()]),
        "한 줄 항목을 못 읽는다"
    );
    assert!(
        m.meth
            .get("terminal.spawn")
            .cloned()
            .flatten()
            .is_some_and(|v| v.contains(&"TerminalSpawn".to_string())),
        "여러 줄 항목(`terminal.spawn`)을 못 읽는다 — 한 줄 파서로 되돌아갔다"
    );
    assert_eq!(
        m.meth.get("timer.list").cloned(),
        Some(None),
        "local_only 항목을 못 읽는다"
    );
    assert!(
        m.meth
            .get("banner.open")
            .cloned()
            .flatten()
            .is_some_and(|v| v.contains(&"UiBanner".to_string())),
        "plugin_only 항목을 읽지 못했다. 이 항목을 누락하면 문서에만 메서드가 있다고 잘못 보고한다."
    );
    assert_eq!(
        expand("approval.summary.get/set"),
        vec![
            "approval.summary.get".to_string(),
            "approval.summary.set".to_string()
        ]
    );
}

/// 문서가 이름으로 적은 메서드는 실재하고, local_only 가 아니며, Agent 가 부를 때 그 권한 토큰을 요구한다.
/// Plugin 호출자는 토큰 없이 메서드의 plugin_callable 여부만 검사받으므로 이 표의 대상이 아니다.
#[test]
fn every_method_the_doc_names_really_needs_that_permission() {
    let m = model();
    let tokens: BTreeSet<String> = m.tok.values().cloned().collect();
    let mut bad = Vec::new();
    for (t, col) in &m.rows {
        for lit in backticked(col) {
            if lit.contains('*') {
                continue;
            }
            for name in expand(&lit) {
                if !m.meth.contains_key(&name) {
                    // 백틱 안의 토큰 참조와 산문(모듈 이름 등)은 메서드가 아니다.
                    if tokens.contains(&name) || !name.contains('.') {
                        continue;
                    }
                }
                match m.meth.get(&name) {
                    None => bad.push(format!(
                        "  [{t}] `{lit}` → `{name}` 이 METHOD_TABLE 에 없다"
                    )),
                    Some(None) => bad.push(format!(
                        "  [{t}] `{lit}` → `{name}` 은 local_only 라 agent 가 못 부른다"
                    )),
                    Some(Some(vs)) => {
                        let have: BTreeSet<&String> =
                            vs.iter().filter_map(|v| m.tok.get(v)).collect();
                        if !have.contains(t) {
                            bad.push(format!(
                                "  [{t}] `{name}` 은 그 권한을 요구하지 않는다 (실제 {have:?})"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "문서가 적은 메서드가 코드와 어긋난다 ({}건).\n{}\n\n\
         단일 출처는 {METHOD_SOURCE} 의 METHOD_TABLE 이다 — 문서를 그쪽에 맞춘다.",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn closed_rows_list_exactly_what_the_code_requires() {
    let m = model();
    let mut bad = Vec::new();
    let mut checked = 0usize;
    for (t, col) in &m.rows {
        if kind(col) != Kind::Closed {
            continue;
        }
        checked += 1;
        let listed: BTreeSet<String> = backticked(col)
            .iter()
            .filter(|l| !l.contains('*'))
            .flat_map(|l| expand(l))
            .filter(|n| m.meth.contains_key(n))
            .collect();
        let code = m.by_tok.get(t).cloned().unwrap_or_default();
        let missing: Vec<&String> = code.difference(&listed).collect();
        let extra: Vec<&String> = listed.difference(&code).collect();
        if !missing.is_empty() {
            bad.push(format!("  [{t}] 문서에 없다: {missing:?}"));
        }
        if !extra.is_empty() {
            bad.push(format!("  [{t}] 코드가 요구하지 않는다: {extra:?}"));
        }
    }
    assert!(
        checked >= 15,
        "닫힌 행이 {checked}개뿐이다 — 분류가 깨졌거나 표가 통째로 `등` 이 됐다"
    );
    assert!(
        bad.is_empty(),
        "메서드만 나열한 행은 그 나열이 전부라는 주장이다 ({}건 어긋남).\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn total_glob_claims_hold_in_both_directions() {
    let m = model();
    let mut bad = Vec::new();
    let mut checked = 0usize;
    for (t, col) in &m.rows {
        if kind(col) != Kind::GlobTotal {
            continue;
        }
        checked += 1;
        let lits = backticked(col);
        let prefixes: Vec<String> = lits
            .iter()
            .filter(|l| l.ends_with('*'))
            .map(|l| l.trim_end_matches('*').to_string())
            .collect();
        let extras: BTreeSet<String> = lits
            .iter()
            .filter(|l| !l.contains('*'))
            .flat_map(|l| expand(l))
            .filter(|n| m.meth.contains_key(n))
            .collect();
        assert!(!prefixes.is_empty(), "[{t}] `전부` 인데 글롭이 없다");
        let code = m.by_tok.get(t).cloned().unwrap_or_default();
        for name in &code {
            if !prefixes.iter().any(|p| name.starts_with(p)) && !extras.contains(name) {
                bad.push(format!(
                    "  [{t}] `{name}` 이 그 권한을 요구하는데 문서의 `{prefixes:?}` 밖이고 이름으로도 안 적혔다"
                ));
            }
        }
        for (name, req) in &m.meth {
            let Some(vs) = req else { continue };
            if !prefixes.iter().any(|p| name.starts_with(p)) {
                continue;
            }
            let have: BTreeSet<&String> = vs.iter().filter_map(|v| m.tok.get(v)).collect();
            if !have.contains(t) {
                bad.push(format!(
                    "  [{t}] `{name}` 이 문서의 `{prefixes:?}` 안인데 그 권한을 요구하지 않는다 (실제 {have:?})"
                ));
            }
        }
    }
    assert!(
        checked >= 3,
        "`전부` 행이 {checked}개뿐이다 — 분류가 깨졌다"
    );
    assert!(
        bad.is_empty(),
        "`전부` 는 접두 전체를 덮는다는 주장이다 ({}건 어긋남).\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// `memory.read` / `memory.write` 는 어느 글롭이 어느 쪽인지 기계가 못 가른다
/// (`` `memory.bb_*` 조회 `` / `` … 변경 ``). 대신 **쌍으로** 검사한다.
#[test]
fn the_memory_pair_covers_every_memory_method_exactly_once() {
    let m = model();
    let read_set = m.by_tok.get("memory.read").cloned().unwrap_or_default();
    let write_set = m.by_tok.get("memory.write").cloned().unwrap_or_default();
    let union: BTreeSet<String> = read_set.union(&write_set).cloned().collect();

    let expected: BTreeSet<String> = m
        .meth
        .iter()
        .filter(|(n, req)| {
            req.is_some() && n.starts_with("memory.") && !n.starts_with("memory.secret.")
        })
        .map(|(n, _)| n.clone())
        .collect();
    assert!(
        expected.len() >= 30,
        "memory.* 가 {}개뿐이다 — 파싱이 깨졌다",
        expected.len()
    );

    let uncovered: Vec<&String> = expected.difference(&union).collect();
    assert!(
        uncovered.is_empty(),
        "`memory.read` 와 `memory.write` 가 합쳐도 안 덮는 memory 메서드가 있다: {uncovered:?}\n\
         문서 두 행은 `memory.*` 전체를 조회/변경으로 가른다고 말한다."
    );

    let both: Vec<&String> = read_set.intersection(&write_set).collect();
    assert!(
        both.is_empty(),
        "한 메서드가 `memory.read` 와 `memory.write` 를 함께 요구한다: {both:?}\n\
         문서는 둘을 배타적인 조회/변경으로 서술한다 — 서술이나 코드 한쪽을 고쳐야 한다."
    );

    // 접두 밖 예외는 문서가 이름으로 적은 것뿐이어야 한다.
    let outside: BTreeSet<String> = union.difference(&expected).cloned().collect();
    let named: BTreeSet<String> = m
        .rows
        .iter()
        .flat_map(|(_, col)| backticked(col))
        .filter(|l| !l.contains('*'))
        .flat_map(|l| expand(&l))
        .collect();
    let unnamed: Vec<&String> = outside.difference(&named).collect();
    assert!(
        unnamed.is_empty(),
        "`memory.*` 밖인데 memory 권한을 요구하고 문서 어디에도 이름이 없다: {unnamed:?}"
    );
}

#[test]
fn the_rows_that_declare_themselves_incomplete_do_not_grow() {
    let m = model();
    let open: Vec<&String> = m
        .rows
        .iter()
        .filter(|(_, col)| kind(col) == Kind::Open)
        .map(|(t, _)| t)
        .collect();
    let found = open.len();
    assert!(
        found <= OPEN_ROWS,
        "등을 붙인 부분 목록이 {found}개로 상한 {OPEN_ROWS}을 넘었다: {open:?}. 부분 목록은 코드에만 있는 메서드의 누락을 검사하지 않는다. 행을 완전한 목록으로 바꿀 수 있는지 확인한다. 줄어든 것은 실패가 아니다."
    );
}
