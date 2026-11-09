//! 번들 플러그인의 제품 코드에서 직접 적은 한글·가나·한자 문자열을 찾는다.
//! 사용자·에이전트에게 전달하는 문구는 [i18n](../../docs/dev-guide/i18n.md)에 따라 번역 파일을 사용한다.
//! 영어 리터럴이나 실제 전달 경로는 판별하지 못한다. 검출된 값이 번역 대상인지도 문맥으로 확인해야 한다.
//!
//! 선언 기반 test 전용 파일·구간과 진단 호출로 분류한 줄은 제외한다.
//! 진단 이름을 원문에서 찾고 괄호 수지로 줄 범위를 정하므로 같은 줄의 다른 문자열도 제외될 수 있다.
//! 문자열은 줄별로 읽고 //로 시작하는 주석을 제외한다. 여러 줄 raw 문자열·블록 주석을 완전히 해석하지 않는다.

use tasty_doc_guards::cfg_predicate as cfg_span;
use tasty_doc_guards::source_text::is_locale_specific;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::repo_root;

const CRATES_DIR: &str = "crates";
const PLUGIN_CRATE_PREFIX: &str = "tasty-plugin-";
const MANIFEST_NAME: &str = "tasty-plugin.toml";

/// 2026-09-05 매니페스트가 있는 플러그인 9개를 측정했다. 빈 수집을 찾는 하한이다.
const MIN_PLUGINS: usize = 8;

/// 번역 검사에서 제외할 진단 호출의 검색 문자열.
const DIAGNOSTIC_MARKERS: &[&str] = &[
    "assert",
    "panic!",
    "unreachable!",
    ".expect(",
    "tracing::",
    "warn!",
    "info!",
    "error!",
    "debug!",
    "trace!",
];

/// 한 줄의 일반 문자열·문자 리터럴·// 주석을 제외하고 괄호 수지를 센다. 완전한 Rust 렉서는 아니다.
fn paren_delta(line: &str) -> i32 {
    let b = line.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            break;
        }
        if c == b'"' {
            in_str = true;
        } else if c == b'\'' {
            let esc = i + 1 < b.len() && b[i + 1] == b'\\';
            let end = if esc { i + 3 } else { i + 2 };
            if end < b.len() && b[end] == b'\'' {
                i = end + 1;
                continue;
            }
        } else if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
        }
        i += 1;
    }
    depth
}

/// 진단 검색 문자열이 있는 줄부터 괄호 수지가 닫히는 줄까지 제외한다.
fn diagnostic_lines(lines: &[&str]) -> Vec<bool> {
    let mut out = vec![false; lines.len()];
    let mut i = 0usize;
    while i < lines.len() {
        if !DIAGNOSTIC_MARKERS.iter().any(|m| lines[i].contains(m)) {
            i += 1;
            continue;
        }
        out[i] = true;
        let mut depth = paren_delta(lines[i]);
        let mut j = i;
        while depth > 0 && j + 1 < lines.len() {
            j += 1;
            out[j] = true;
            depth += paren_delta(lines[j]);
        }
        i = j + 1;
    }
    out
}

pub(crate) fn locale_specific_literals(src: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let gated = cfg_span::cfg_gated_lines(&lines, "test");
    let diagnostic = diagnostic_lines(&lines);
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if gated[idx] || diagnostic[idx] {
            continue;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        for lit in string_literals(line) {
            if lit.chars().any(is_locale_specific) {
                out.push((idx + 1, lit));
            }
        }
    }
    out
}

fn string_literals(line: &str) -> Vec<String> {
    let b: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != '"' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut cur = String::new();
        while j < b.len() {
            if b[j] == '\\' {
                j += 2;
                continue;
            }
            if b[j] == '"' {
                break;
            }
            cur.push(b[j]);
            j += 1;
        }
        if j >= b.len() {
            break;
        }
        out.push(cur);
        i = j + 1;
    }
    out
}

fn bundled_plugin_srcs() -> Vec<PathBuf> {
    let root = repo_root().join(CRATES_DIR);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).expect("crates 디렉터리를 읽을 수 없다") {
        let entry = entry.expect("디렉터리 항목");
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PLUGIN_CRATE_PREFIX)
            && path.join(MANIFEST_NAME).is_file()
            && path.join("src").is_dir()
        {
            out.push(path.join("src"));
        }
    }
    out.sort();
    out
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn no_bundled_plugin_ships_a_locale_specific_literal() {
    let srcs = bundled_plugin_srcs();
    assert!(
        srcs.len() >= MIN_PLUGINS,
        "매니페스트가 있는 플러그인을 {}개만 찾았다(하한 {MIN_PLUGINS}, 2026-09-05 측정 9개). 탐색 범위를 확인한다.",
        srcs.len()
    );
    let root = repo_root();
    let test_only = super::sloc_gate_skip_proxy::test_only_files();
    let mut files = 0usize;
    let mut skipped = 0usize;
    let mut scanned: Vec<PathBuf> = Vec::new();
    let mut hits: Vec<String> = Vec::new();
    for dir in &srcs {
        for f in rs_files(dir) {
            // 공용 스캐너의 상대 경로와 Windows 구분자가 일치해야 test 전용 파일을 제외할 수 있다.
            let rel =
                tasty_doc_guards::source_text::repo_relative(f.strip_prefix(&root).unwrap_or(&f));
            if test_only.contains(&rel) {
                skipped += 1;
                continue;
            }
            scanned.push(rel);
            files += 1;
            let src = std::fs::read_to_string(&f)
                .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", f.display()))
                .replace("\r\n", "\n");
            let rel =
                tasty_doc_guards::source_text::repo_relative(f.strip_prefix(&root).unwrap_or(&f))
                    .to_string_lossy()
                    .to_string();
            for (line, lit) in locale_specific_literals(&src) {
                let shown: String = lit.chars().take(60).collect();
                hits.push(format!("{rel}:{line}  {shown}"));
            }
        }
    }
    assert!(
        files >= 10,
        "플러그인 소스를 {files}개만 수집했다(test 전용 제외 {skipped}개). 경로와 수집 범위를 확인한다."
    );
    // test 전용 파일이 실제 검사 목록에 들어오지 않았는지 대조한다.
    let leaked: Vec<String> = scanned
        .iter()
        .filter(|p| test_only.contains(*p))
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(
        leaked.is_empty(),
        "test 전용으로 선언된 파일이 제품 코드 검사에 포함됐다:\n  {}",
        leaked.join("\n  ")
    );
    assert!(
        hits.is_empty(),
        "번들 플러그인 코드에 직접 적은 CJK 문자열이 있다. 사용자·에이전트 문구라면 lang/{{en,ko,ja,zh-Hans}}.toml에 키를 두고 Translator::t로 읽는다. 검출 범위가 실제 문구 용도와 맞는지도 확인한다:\n  {}",
        hits.join("\n  ")
    );
}

#[test]
fn the_detector_skips_diagnostics_comments_and_tests() {
    let src = "\
// 주석 안의 \"한국어 문구\" 는 대상이 아니다
let msg = \"완료했습니다\";
tracing::warn!(\"경고 문구\");
assert!(x, \"단정 문구\");
let ok = \"plain english\";
#[cfg(test)]
mod tests {
    const T: &str = \"테스트 안의 문구\";
}
";
    let hits = locale_specific_literals(src);
    let lits: BTreeSet<&str> = hits.iter().map(|(_, l)| l.as_str()).collect();
    assert!(
        lits.contains("완료했습니다"),
        "전달 문구를 놓쳤다: {lits:?}"
    );
    assert!(
        !lits.contains("경고 문구") && !lits.contains("단정 문구"),
        "개발자 표면을 집었다: {lits:?}"
    );
    assert!(
        !lits.contains("한국어 문구"),
        "주석 안의 문구를 집었다: {lits:?}"
    );
    assert!(
        !lits.contains("테스트 안의 문구"),
        "`#[cfg(test)]` 이후를 집었다: {lits:?}"
    );
    assert!(!lits.contains("plain english"), "영어까지 집었다: {lits:?}");
}

#[test]
fn production_code_after_a_test_helper_is_still_scanned() {
    let src = "\
#[cfg(test)]
fn helper() -> &'static str { \"테스트 헬퍼 문구\" }

fn shipped() { notify(\"배포되는 문구\"); }
";
    let lits: BTreeSet<String> = locale_specific_literals(src)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert!(
        lits.contains("배포되는 문구"),
        "테스트 헬퍼 뒤의 프로덕션 문구를 놓쳤다 — 판정이 첫 `#[cfg(test)]` 에서 멈췄다: \
         {lits:?}"
    );
    assert!(
        !lits.contains("테스트 헬퍼 문구"),
        "게이트 안의 헬퍼를 집었다: {lits:?}"
    );
}

#[test]
fn a_wrapped_assert_message_is_still_a_diagnostic() {
    let src = "\
fn f() {
    assert!(
        cond,
        \"삭제가 error 상태로 감지되어야 한다\"
    );
    notify(\"사용자에게 나가는 문구\");
}
";
    let lits: BTreeSet<String> = locale_specific_literals(src)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert!(
        !lits.contains("삭제가 error 상태로 감지되어야 한다"),
        "줄바꿈된 단정 메시지를 전달 문구로 읽었다: {lits:?}"
    );
    assert!(
        lits.contains("사용자에게 나가는 문구"),
        "진단 호출이 끝난 뒤의 문구까지 제외했다: {lits:?}"
    );
}

#[test]
fn an_escaped_quote_does_not_split_the_literal() {
    let hits = locale_specific_literals("let s = \"그는 \\\"안녕\\\" 이라 했다\";");
    assert_eq!(hits.len(), 1, "리터럴이 쪼개졌다: {hits:?}");
}
