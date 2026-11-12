//! 빌드에서 제외되는 플랫폼·feature 코드도 소스로 읽어 검사한다.
//! 이 모듈은 tasty 라이브러리의 단위 시험이므로 GUI의 --lib --bins 실행과 헤드리스 전체 시험에 포함된다.
//! 검사 명령·트리거·실행 결과는 docs/dev-guide/ci-gates.md에서 확인한다. CI 명령이 있다는 사실만으로
//! 최근 실행이 성공했다고 판단하지 않는다.
//!
//! 주석·리터럴을 가리는 mask_non_code와 리터럴을 남기는 strip_comments 중 검사에 맞는 것을 사용한다.
//! 공용 수집에는 검사 파일 자체도 포함되며, 개별 검사는 필요한 범위를 따로 정한다.
//! 순수 판정 함수를 분리해 실제 저장소와 합성 입력에서 같은 조건을 검증한다.
//! 예외를 추가할 때는 예외 안의 실제 위반도 검출되는지 시험해야 한다.
//!
//! 함수 본문 분석만으로는 호출자의 루프·분기·호출 횟수를 알 수 없다.
//! callers_of는 호출자 한 단계의 이름·함수·루프 위치를 제공하고 arg_at은 그 호출의 인자를 읽는다.
//! 이 도구들도 타입 해석이나 실행 경로 전체를 검증하지는 않는다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// 약 1100개를 수집하던 시점에 일부 정상 감소를 허용해 하한 900을 정했다. 일부 파일 누락을 놓칠 수 있다.
const MIN_SCANNED_FILES: usize = 900;

/// 본체와 크레이트 소스를 수집한다. 내부 시험 코드도 포함되며 test 전용 여부는 소비자가 별도로 분류한다. 루트 tests·build.rs는 범위 밖이다.
const SCAN_ROOTS: &[&str] = &["src", "crates"];

/// 테스트 코드를 검사할 때 쓰는 범위. 루트 tests도 포함하며 제품 코드를 묻는 검사와 구분한다.
const SCAN_ROOTS_WITH_INTEGRATION_TESTS: &[&str] = &["src", "crates", "tests"];

/// 공용 렉서로 주석을 가리고 주석뿐인 줄은 제거한다. 리터럴을 보존하지만 원문의 줄 번호는 유지하지 않는다.
fn strip_comments(src: &str) -> String {
    let masked = tasty_doc_guards::source_text::mask_comments(src);
    let mut out = String::with_capacity(src.len());
    for (orig, line) in src.lines().zip(masked.lines()) {
        if line.trim().is_empty() && !orig.trim().is_empty() {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 직접 IPC 라우터에서 메서드를 읽는 표현식. 변경하면 소비자의 수집 조건도 함께 확인한다.
const METHOD_EXPR: &str = "request.method";

/// 메서드 비교에 리터럴이 아닌 이름을 쓰는 곳을 찾는다. 리터럴 수집만으로는 이런 이름을 놓친다.
fn opaque_method_sites(body: &str) -> Vec<String> {
    opaque_sites_for(body, METHOD_EXPR)
}

/// 지정한 표현식의 ==·starts_with·match 분기를 검사한다.
/// &str 인자의 match도 읽으며 식별자 경계를 확인해 다른 이름의 일부를 잘못 찾지 않도록 한다.
fn opaque_sites_for(body: &str, expr: &str) -> Vec<String> {
    // 주석 속 괄호·이름이 경계 계산과 비교에 섞이지 않도록 먼저 제거한다.
    let body = strip_comments(body);
    let body = body.as_str();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(i) = body[at..].find(expr) {
        let start = at + i;
        at = start + expr.len();
        if body[..start].chars().next_back().is_some_and(is_word)
            || body[at..].chars().next().is_some_and(is_word)
        {
            continue;
        }
        let rest = body[at..].trim_start();
        if let Some(r) = rest.strip_prefix("==") {
            if !r.trim_start().starts_with('"') {
                out.push(format!("`== {}`", head(r)));
            }
        } else if let Some(r) = rest.strip_prefix(".starts_with(") {
            if !r.trim_start().starts_with('"') {
                out.push(format!("`.starts_with({}`", head(r)));
            }
        } else if let Some(r) = rest.strip_prefix(".as_str()") {
            let r = r.trim_start();
            if r.starts_with('{') {
                out.extend(non_literal_arms(r));
            }
        } else if rest.starts_with('{') && body[..start].trim_end().ends_with("match") {
            out.extend(non_literal_arms(rest));
        }
    }
    out
}

fn head(s: &str) -> String {
    s.trim_start()
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(40)
        .collect()
}

/// match 패턴은 문자열 또는 guard 없는 _·소문자 바인딩을 허용한다. 읽지 못한 형태는 검토 대상으로 반환한다.
fn non_literal_arms(block: &str) -> Vec<String> {
    use tasty_doc_guards::match_arms::{Source, matching_close};
    let src = Source::new(block);
    let Some(close) = matching_close(&src.code, 0) else {
        return vec!["`match` 블록이 닫히지 않는다".to_string()];
    };
    let arms = match src.match_arms(0..close + 1) {
        Ok(arms) => arms,
        Err(e) => return vec![format!("`match` 팔을 못 읽었다 — {e}")],
    };
    let mut out = Vec::new();
    for arm in arms {
        for alt in src.alternatives(&arm.pattern) {
            let text = src.slice(&alt);
            let binding = arm.guard.is_none()
                && (text == "_"
                    || text
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
            if src.plain_string(&alt).is_none() && !binding {
                out.push(format!("`match` 팔 `{text}`"));
            }
        }
    }
    out
}

/// 시그니처의 함수 정의가 **하나일 때** 그 원문 본문을 반환한다. 없으면 None 이다.
///
/// 같은 이름의 정의가 둘 이상이면(cfg 로 갈린 정의) 첫 정의만 읽지 않고 실패한다. 첫 정의만
/// 읽으면 뒤 정의에만 들어간 위반을 검출하지 못하고 통과한다. 둘 이상을 만난 소비자는 [`fn_bodies_of`]
/// 로 옮기고 전부 대조할지, 하나만 있어야 하는지, 첫 것만 볼지(사유와 함께)를 정한다.
/// 재는 변이: `should_rate_limit` 뒤에 `#[cfg(any())]` 로 둘째 정의를 두면 이 함수를 쓰는
/// 소비자는 실패한다(`fn_body_refuses_a_second_definition`).
fn fn_body(src: &str, signature: &str) -> Option<String> {
    let mut bodies = fn_bodies_of(src, signature);
    assert!(
        bodies.len() <= 1,
        "`{signature}` 정의가 {}개다. fn_body 는 첫 정의만 읽을 수 없다 — 소비자를 fn_bodies_of 로 \
         옮기고 둘 이상일 때의 대조 방법을 정하라",
        bodies.len()
    );
    bodies.pop()
}

/// 시그니처(`fn 이름(` · `pub(crate) fn 이름` …)의 함수 이름과 같은 정의의 원문 본문을 모두
/// 반환한다. cfg 로 갈린 정의도 포함하며 주석·리터럴 속 언급과 이름이 더 긴 함수는 제외한다.
fn fn_bodies_of(src: &str, signature: &str) -> Vec<String> {
    fn_definitions_of(src, signature)
        .into_iter()
        .map(|d| d.body)
        .collect()
}

/// 함수 정의 하나 — 앞에 붙은 속성 줄들과 원문 본문.
struct FnDefinition {
    /// `fn` 줄 바로 앞에 이어진 `#[…]` 줄들(주석·doc 줄은 건너뛴다).
    attrs: String,
    body: String,
}

/// [`fn_bodies_of`] 와 같은 정의들을 앞 속성과 함께 돌려준다. 속성은 줄 단위로 읽으므로
/// `fn` 과 같은 줄에 쓴 속성은 들지 않는다.
fn fn_definitions_of(src: &str, signature: &str) -> Vec<FnDefinition> {
    let name: String = signature
        .rsplit_once("fn ")
        .map_or("", |(_, rest)| rest)
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    assert!(
        !name.is_empty(),
        "시그니처에서 함수 이름을 못 읽었다: {signature}"
    );
    let source = tasty_doc_guards::match_arms::Source::new(src);
    let head = format!("fn {name}");
    source
        .fn_bodies(&name)
        .iter()
        .map(|r| {
            let fn_at = source.code[..r.start].rfind(&head).unwrap_or(r.start);
            let line_start = src[..fn_at].rfind('\n').map_or(0, |i| i + 1);
            let mut attrs: Vec<&str> = src[..line_start]
                .lines()
                .rev()
                .map(str::trim)
                .take_while(|l| l.starts_with("#[") || l.starts_with("//"))
                .filter(|l| l.starts_with("#["))
                .collect();
            attrs.reverse();
            FnDefinition {
                attrs: attrs.join("\n"),
                body: source.slice(r).to_string(),
            }
        })
        .collect()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 소스를 저장소 상대 경로와 LF 내용으로 수집한다. 빌드 캐시는 공용 순회 정책으로 제외한다.
fn rust_sources() -> Vec<(PathBuf, String)> {
    let out = tasty_doc_guards::source_text::rust_sources(&repo_root(), SCAN_ROOTS);
    assert!(
        out.len() >= MIN_SCANNED_FILES,
        "소스를 {}개만 수집했다(하한 {MIN_SCANNED_FILES}). 루트와 순회 결과를 확인한다.",
        out.len()
    );
    out
}

/// 루트 tests를 더했을 때 실제 수집 수가 늘어나는지 독립 순회로 비교한다.
/// 고정 하한은 시험의 크레이트 이동 때마다 낡을 수 있어 두 범위를 비교한다.
/// tests 파일이 1개 이상인지 확인하는 것만으로는 좁은 범위에 tests가 잘못 들어간 경우를 찾지 못한다.
fn rust_sources_with_integration_tests() -> Vec<(PathBuf, String)> {
    let out = tasty_doc_guards::source_text::rust_sources(
        &repo_root(),
        SCAN_ROOTS_WITH_INTEGRATION_TESTS,
    );
    // 넓은 결과에서 빼서 계산하지 않아야 두 루트 목록이 같아진 실수를 검출할 수 있다.
    let narrow = rust_sources().len();
    assert!(
        out.len() > narrow,
        "tests를 포함한 수집 {}개가 기본 수집 {narrow}개보다 많지 않다. 두 루트 목록과 tests의 실제 파일을 확인한다. 파일 이동에 따라 낡는 고정 하한으로 대신하지 않는다.",
        out.len()
    );
    out
}

/// 전체 파일 수 하한을 통과해도 크레이트 하나가 빠질 수 있어 src와 각 crates 디렉터리의 집합도 비교한다.
fn scanned_units(files: &[(PathBuf, String)]) -> BTreeSet<String> {
    files.iter().filter_map(|(rel, _)| unit_of(rel)).collect()
}

/// 경로가 속한 수집 단위. 범위 밖은 None.
fn unit_of(rel: &Path) -> Option<String> {
    let mut parts = rel.components();
    let first = parts.next()?.as_os_str().to_string_lossy().into_owned();
    match first.as_str() {
        "src" => Some("src".to_owned()),
        "crates" => {
            let name = parts.next()?.as_os_str().to_string_lossy().into_owned();
            Some(format!("crates/{name}"))
        }
        _ => None,
    }
}

/// 같은 순회의 누락이 양쪽에 반복되지 않도록 매니페스트로 크레이트를 별도 수집한다.
fn expected_units() -> BTreeSet<String> {
    let root = repo_root();
    assert!(root.join("src").is_dir(), "`src` 스캔 루트가 없다");
    let mut out = BTreeSet::from(["src".to_owned()]);
    let entries = std::fs::read_dir(root.join("crates")).expect("`crates` 를 읽을 수 없다");
    for entry in entries {
        let entry = entry.expect("디렉터리 항목을 읽을 수 없다");
        let is_dir = entry.file_type().expect("파일 종류를 알 수 없다").is_dir();
        if is_dir && entry.path().join("Cargo.toml").is_file() {
            out.insert(format!("crates/{}", entry.file_name().to_string_lossy()));
        }
    }
    out
}

/// 수집에서 빠진 단위와 추가된 단위. 두 변화의 수가 상쇄될 수 있어 양쪽을 비교한다.
fn unit_diff(
    scanned: &BTreeSet<String>,
    expected: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    (
        expected.difference(scanned).cloned().collect(),
        scanned.difference(expected).cloned().collect(),
    )
}

fn unit_counts(files: &[(PathBuf, String)]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (rel, _) in files {
        if let Some(unit) = unit_of(rel) {
            *out.entry(unit).or_insert(0usize) += 1;
        }
    }
    out
}

/// feature로 제외될 수 있는 gfx와 macOS 전용 webview를 수집 표본으로 삼는다.
/// 소스 순회는 빌드 cfg와 무관하게 두 파일을 읽어야 한다.
/// 표본에 실제 어떤 상위 cfg가 걸렸는지는 이 검사에서 증명하지 않는다.
/// gfx에 target_os가 없다는 전제만 별도 검사한다.
const FEATURE_AXIS_DIR: &str = "src/gfx/";

const CFG_EXCLUDED_SAMPLES: &[&str] = &["src/gfx/gpu.rs", "src/host_api/webview/macos.rs"];

#[test]
fn the_feature_axis_sample_is_not_also_target_gated() {
    let sources = rust_sources();
    let considered: Vec<&(PathBuf, String)> = sources
        .iter()
        .filter(|(path, _)| {
            path.to_string_lossy()
                .replace('\\', "/")
                .starts_with(FEATURE_AXIS_DIR)
        })
        .collect();
    assert!(
        !considered.is_empty(),
        "{FEATURE_AXIS_DIR}에서 파일을 찾지 못했다. 경로 이동과 접두사를 확인한다."
    );
    let dirty: Vec<String> = considered
        .iter()
        .filter(|(_, text)| mask_non_code(text).contains("target_os"))
        .map(|(path, _)| path.display().to_string())
        .collect();
    assert!(
        dirty.is_empty(),
        "`src/gfx/` 에 target_os 분기가 생겼다 — feature 축 표본이 두 축을 섞게 된다. \
         `CFG_EXCLUDED_SAMPLES` 의 feature 축 표본을 순수한 파일로 옮겨라: {dirty:?}"
    );
}

/// 총량 하한이 놓칠 수 있는 cfg별 대표 파일의 누락을 이름으로 확인한다.
#[test]
fn the_scan_reads_files_this_build_never_compiles() {
    let scanned: BTreeSet<String> = rust_sources()
        .iter()
        .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
        .collect();
    let missing: Vec<&str> = CFG_EXCLUDED_SAMPLES
        .iter()
        .copied()
        .filter(|path| !scanned.contains(*path))
        .collect();
    assert!(
        missing.is_empty(),
        "cfg별 대표 파일이 수집에 없다: {missing:?}. 파일 이동 여부와 검사 목록을 확인한다."
    );
}

#[test]
fn every_scan_unit_contributes_at_least_one_file() {
    let files = rust_sources();
    let (missing, extra) = unit_diff(&scanned_units(&files), &expected_units());
    assert!(
        missing.is_empty() && extra.is_empty(),
        "스캔 단위 집합이 어긋난다 — 빠진 단위 {missing:?} / 여분 {extra:?}. \
         개수 하한은 단위 하나가 통째로 빠져도 통과하므로 이 대조가 따로 필요하다"
    );
}

mod scan_unit_mutations;

mod scan_population;

/// SLOC 검사에서 파일 이름으로 제외한 범위와 실제 test 전용 선언의 차이를 확인한다.
mod sloc_gate_skip_proxy;

/// 주석·문자열·문자 리터럴 마스킹은 루트 통합 시험에서도 쓰는 공용 구현을 사용한다.
pub(crate) use tasty_doc_guards::source_text::mask_non_code;

/// 바이트 위치 pos의 줄 번호(1부터 시작).
fn line_of(masked: &str, pos: usize) -> usize {
    masked[..pos].bytes().filter(|b| *b == b'\n').count() + 1
}

fn is_word_boundary(masked: &str, pos: usize, word: &str) -> bool {
    let before = masked[..pos].chars().next_back();
    let after = masked[pos + word.len()..].chars().next();
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    !before.is_some_and(ident) && !after.is_some_and(ident)
}

fn word_positions(masked: &str, word: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find(word) {
        let pos = from + rel;
        if is_word_boundary(masked, pos, word) {
            out.push(pos);
        }
        from = pos + word.len();
    }
    out
}

/// 매크로가 쓰는 (), {}, [] 구분자 짝의 바이트 위치를 찾는다.
fn matching_delim(masked: &str, open: usize) -> Option<usize> {
    let opener = masked[open..].chars().next()?;
    let closer = match opener {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        _ => return None,
    };
    let mut depth = 0usize;
    for (offset, c) in masked[open..].char_indices() {
        if c == opener {
            depth += 1;
        } else if c == closer {
            depth -= 1;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

/// 같은 괄호 깊이의 첫 블록 범위. 헤더의 ()·[]는 건너뛰고 ;를 먼저 만나면 본문이 없는 것으로 본다.
fn block_after(masked: &str, from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i < masked.len() {
        let c = masked[i..].chars().next()?;
        match c {
            '{' => return matching_delim(masked, i).map(|end| (i, end)),
            '(' | '[' => i = matching_delim(masked, i)? + 1,
            ';' => return None,
            _ => i += c.len_utf8(),
        }
    }
    None
}

/// fn의 이름·본문 시작/끝·키워드 위치. 본문 없는 선언은 제외한다.
fn fn_spans(masked: &str) -> Vec<(String, usize, usize, usize)> {
    let mut out = Vec::new();
    for kw in word_positions(masked, "fn") {
        let after = kw + "fn".len();
        let name: String = masked[after..]
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue; // `fn(u32) -> u32` 같은 타입 자리.
        }
        let Some((open, close)) = block_after(masked, after) else {
            continue;
        };
        out.push((name, open, close, kw));
    }
    out
}

/// 위치를 포함하는 가장 안쪽 fn.
fn enclosing_fn(
    spans: &[(String, usize, usize, usize)],
    pos: usize,
) -> Option<&(String, usize, usize, usize)> {
    spans
        .iter()
        .filter(|(_, open, close, _)| *open < pos && pos < *close)
        .min_by_key(|(_, open, close, _)| close - open)
}

/// 본문 안 for·while·loop 블록의 범위.
fn loop_blocks(masked: &str, open: usize, close: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for kw in ["for", "while", "loop"] {
        for pos in word_positions(masked, kw) {
            if pos <= open || pos >= close {
                continue;
            }
            if let Some(span) = block_after(masked, pos + kw.len())
                && span.1 <= close
            {
                out.push(span);
            }
        }
    }
    out
}

/// 이름으로 찾은 호출 위치와 그 호출을 포함한 함수·루프 정보.
struct CallSite {
    /// 저장소 상대 경로. 구분자는 /로 정규화한다.
    rel: String,
    /// 인자 추출에도 같은 마스킹 결과를 사용하도록 보관한다. 같은 파일의 호출들은 Rc를 공유한다.
    masked: Rc<str>,
    /// 호출 이름의 바이트 위치. arg_at에 전달한다.
    at: usize,
    /// None이면 함수 추출 실패다. 이때 in_loop=false를 루프 밖이라는 검사 결과로 취급해서는 안 된다.
    caller: Option<String>,
    /// 찾은 함수 본문 안에서 루프에 포함되는지 여부.
    in_loop: bool,
    /// 실패 위치의 줄 번호.
    line: usize,
}

/// 동명 메서드·자유 함수 호출을 수집하고 정의는 제외한다. 수신자 타입을 구분하지 못하므로 소비자는 이름 충돌을 고려해야 한다.
fn callers_of(name: &str, skip_dirs: &[&str]) -> Vec<CallSite> {
    let needle = format!("{name}(");
    let mut out = Vec::new();
    for (rel, src) in rust_sources() {
        let rel = rel.to_string_lossy().into_owned();
        if skip_dirs.iter().any(|d| rel.starts_with(d)) {
            continue;
        }
        let masked: Rc<str> = Rc::from(mask_non_code(&src).as_str());
        if !masked.contains(&needle) {
            continue;
        }
        let spans = fn_spans(&masked);
        for (at, _) in masked.match_indices(&needle) {
            let before = &masked[..at];
            if before
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let head = before.trim_end();
            let is_def = head.ends_with("fn")
                && head[..head.len() - 2]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            if is_def {
                continue;
            }
            let enclosing = enclosing_fn(&spans, at);
            let in_loop = enclosing.is_some_and(|(_, open, close, _)| {
                loop_blocks(&masked, *open, *close)
                    .iter()
                    .any(|(lo, hi)| *lo < at && at < *hi)
            });
            out.push(CallSite {
                rel: rel.clone(),
                masked: Rc::clone(&masked),
                at,
                caller: enclosing.map(|(n, ..)| n.clone()),
                in_loop,
                line: line_of(&masked, at),
            });
        }
    }
    out
}

/// 0부터 센 호출 인자를 읽는다. 수신자 self는 제외하며 중첩 괄호의 쉼표는 인자 경계가 아니다.
fn arg_at(masked: &str, call_at: usize, index: usize) -> Option<String> {
    let open = masked[call_at..].find('(')? + call_at;
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let (mut depth, mut nth, mut start) = (0usize, 0usize, open + 1);
    for (offset, c) in masked[open..].char_indices() {
        let here = open + offset;
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return (nth == index).then(|| norm(&masked[start..here]));
                }
            }
            ',' if depth == 1 => {
                if nth == index {
                    return Some(norm(&masked[start..here]));
                }
                nth += 1;
                start = here + 1;
            }
            _ => {}
        }
    }
    None
}

fn first_arg(masked: &str, call_at: usize) -> Option<String> {
    arg_at(masked, call_at, 0)
}

fn next_opening_delim(masked: &str, from: usize) -> Option<usize> {
    masked[from..]
        .char_indices()
        .find(|(_, c)| matches!(c, '(' | '{' | '['))
        .map(|(offset, _)| from + offset)
}

/// 검사 디렉터리의 모든 Rust 파일이 공용 수집에 포함되는지 대조한다.
#[test]
fn the_guard_file_scans_itself() {
    let dir: PathBuf = ["src", "source_guards"].iter().collect();
    let mut own: Vec<PathBuf> = std::fs::read_dir(repo_root().join(&dir))
        .expect("가드 모듈 디렉토리를 읽지 못했다 — 경로가 바뀌었는지 확인해라")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .filter(|name| Path::new(name).extension().is_some_and(|ext| ext == "rs"))
        .map(|name| dir.join(name))
        .collect();
    own.sort();
    assert!(
        !own.is_empty(),
        "가드 모듈에서 .rs 를 하나도 찾지 못했다 — 0 개를 통과로 세지 않는다"
    );

    let scanned: BTreeSet<PathBuf> = rust_sources().into_iter().map(|(path, _)| path).collect();
    let missing: Vec<&PathBuf> = own.iter().filter(|path| !scanned.contains(*path)).collect();
    assert!(
        missing.is_empty(),
        "가드 자신의 파일이 스캔 모수에서 빠졌다 — 자기 제외 면제가 다시 생겼는지 확인해라: {missing:?}"
    );
}

mod define_class_return;

mod read_only_handle_mtime;

// 시험 실행에는 --no-fail-fast가 있어야 앞 바이너리의 실패 뒤에도 나머지 타깃을 실행할 수 있다.

const WORKFLOW_DIR: &str = ".github/workflows";

/// Git 워크플로 목록이 비지 않았는지 확인하는 하한. 2026-09-05 yml 파일 9개를 측정했다.
const MIN_GIT_LISTED_WORKFLOWS: usize = 5;

/// flatten_workflow와 cargo_test_invocations로 센 파일별 호출 수다. 미등록 파일의 기대값은 0이다.
/// 합계만 보면 한 파일의 삭제와 다른 파일의 추가가 상쇄되므로 파일별로 고정한다.
/// 같은 파일 안의 호출 교체는 구별하지 못한다.
const EXPECTED_TEST_INVOCATIONS: &[(&str, usize)] = &[
    // macOS GUI 단위 시험.
    ("crossplatform-check-macos.yml", 1),
    // Windows·Linux GUI 단위 시험, 헤드리스 전체 시험, Linux GUI E2E·webview 레이아웃·바탕 포커스와 Windows 통합 시험.
    ("crossplatform-check.yml", 7),
    ("doc-guards.yml", 1),
    ("test.yml", 3),
];

/// 줄 시작의 주석과 name 항목을 제외하고 공백으로 합친다.
/// 여러 줄 명령의 플래그를 함께 읽기 위한 처리이며 YAML·셸을 완전히 파싱하지 않는다.
/// 인라인 주석이나 run 이외 필드의 문자열은 남을 수 있다.
fn flatten_workflow(yaml: &str) -> String {
    yaml.replace("\r\n", "\n")
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with('#') && !t.starts_with("- name:") && !t.starts_with("name:")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// cargo test부터 다음 cargo 문자열 앞까지 자른다. 셸 명령 경계 전체를 해석하지 않는다.
fn cargo_test_invocations(flat: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = flat[from..].find("cargo test") {
        let start = from + rel;
        let rest = &flat[start + "cargo test".len()..];
        let end = rest
            .find("cargo ")
            .map_or(flat.len(), |n| start + "cargo test".len() + n);
        out.push(&flat[start..end]);
        from = start + "cargo test".len();
    }
    out
}

/// 컴파일만 하는 --no-run은 제외하고 --no-fail-fast 없는 실행 명령을 찾는다.
fn test_invocations_missing_no_fail_fast(yaml: &str) -> Vec<String> {
    let flat = flatten_workflow(yaml);
    cargo_test_invocations(&flat)
        .into_iter()
        .filter(|inv| !inv.contains("--no-run") && !inv.contains("--no-fail-fast"))
        .map(|inv| inv.split_whitespace().take(8).collect::<Vec<_>>().join(" "))
        .collect()
}

mod debug_gate_dagger;
mod debug_handler_isolation;
mod dispatch_name_literals;
mod test_serialization_locks;

mod jobs_anchored_at_boot;

mod derived_plugin_tables_are_not_bypassed;

mod builtin_plugin_roster;
mod bundled_plugin_namespace_coverage;

#[cfg(test)]
mod gallery_copied_dimensions;

#[cfg(test)]
mod gallery_specimen_parity;

#[cfg(test)]
mod gallery_widget_coverage;

#[cfg(test)]
mod repo_relative_paths;

#[cfg(test)]
mod home_env_has_one_door;

/// 시험의 X11 EventLoop 는 연결 실패 진단을 싣는 도우미 한 곳에서 만든다.
#[cfg(test)]
mod x11_test_loop_has_one_door;

/// OS 휴지통 호출은 시험 빌드의 대역을 지나는 한 곳에서만 한다.
#[cfg(test)]
mod trash_has_one_door;

/// 스트림 수신 깨움은 StreamWakeGate를 거쳐서만 보낸다.
#[cfg(test)]
mod stream_wakes_go_through_the_gate;

#[cfg(test)]
mod icon_texture_loader_installed;
/// 제품 코드에서 memory.db 저장소를 memory writer만 소유하는지 확인한다.
mod memory_store_has_one_owner;

mod headless_app_layer_coverage;
mod key_contract_by_layer;

#[cfg(test)]
mod length_constant_frontier;

#[cfg(test)]
mod test_gate;

#[cfg(test)]
mod on_scale_length_literal;

mod platform_gated_dispatch_complement;

mod plugin_only_dispatch_parity;

mod plugin_locale_specific_literals;

mod geometry_surface_shape;
mod params_chokepoint;

mod reserved_ipc_prefixes;
mod routing_key_coverage;

mod routing_key_method_scope;

mod unrouted_dispatch_reasons;

#[cfg(test)]
mod workflow_fail_fast_tests;

#[cfg(test)]
mod floor_arguments_are_not_discarded;
mod frame_draw_order;

#[cfg(test)]
mod webview_overlay_targets;

#[cfg(test)]
mod frame_clock_arming;

#[cfg(test)]
mod frame_phase_order;

#[cfg(test)]
mod committed_tap_order;

#[cfg(test)]
mod mesh_bootstrap_order;

#[cfg(test)]
mod theme_changed_after_install;

#[cfg(test)]
mod popover_frame_call_sites;

#[cfg(test)]
mod text_edit_hint_color;

#[cfg(test)]
mod modifier_hint_paint_order;

#[cfg(test)]
mod shutdown_channel_order;

#[cfg(test)]
mod headless_loop_reaps_both_hubs;

mod committed_ratio_boundary;

#[cfg(test)]
mod fn_body_definitions {
    use super::{fn_bodies_of, fn_body, fn_definitions_of};

    const TWO: &str = "\
// fn f() { in_comment }
#[cfg(unix)]
fn f(m: &str) { first }
#[cfg(any())]
fn f(m: &str) { second }
fn ff() { longer_name }
";

    #[test]
    fn fn_bodies_of_reads_every_definition() {
        assert_eq!(fn_bodies_of(TWO, "fn f("), ["{ first }", "{ second }"]);
        assert_eq!(fn_bodies_of(TWO, "pub(crate) fn ff"), ["{ longer_name }"]);
    }

    #[test]
    #[should_panic(expected = "정의가 2개다")]
    fn fn_body_refuses_a_second_definition() {
        fn_body(TWO, "fn f(");
    }

    #[test]
    fn each_definition_carries_its_own_attributes() {
        let attrs: Vec<String> = fn_definitions_of(TWO, "fn f(")
            .into_iter()
            .map(|d| d.attrs)
            .collect();
        assert_eq!(attrs, ["#[cfg(unix)]", "#[cfg(any())]"]);
    }

    #[test]
    fn fn_body_reads_a_single_definition_by_its_exact_name() {
        assert_eq!(fn_body(TWO, "fn ff").as_deref(), Some("{ longer_name }"));
        assert_eq!(fn_body(TWO, "fn missing("), None);
    }
}
