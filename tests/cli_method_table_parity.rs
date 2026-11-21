//! CLI 요청의 메서드 이름을 METHOD_TABLE·DEBUG_METHODS·PREFIX_RULES와 대조한다.
//! request 디렉터리의 값 위치 리터럴과 CLI 전체의 method 필드, 플러그인 CLI 매니페스트를 수집한다.
//! 메서드별 params 키·실제 핸들러 동작은 이 검사 범위 밖이다.
//!
//! debug 표가 비는 release에서는 이 타깃 전체를 제외한다. 동적 생성 이름과 지원하지 않는 소스 형식도
//! 놓칠 수 있으므로 이름을 실제로 전송하거나 모든 경로를 실행하는 검사로 해석해서는 안 된다.
#![cfg(debug_assertions)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tasty_ipc::method_meta::{METHOD_TABLE, method_meta};

const CLI_REQUEST_SOURCES: &[&str] = &["crates/tasty-cli/src/request.rs"];

/// 요청 모듈이 늘어도 누락되지 않도록 디렉터리를 재귀로 수집한다.
const CLI_REQUEST_DIRS: &[&str] = &["crates/tasty-cli/src/request"];

/// request 밖에서 직접 조립하는 요청의 method 필드도 찾는다.
const CLI_CRATE_ROOT: &str = "crates/tasty-cli/src";

/// IPC 전에 로컬에서 처리해 전송하지 않는 이름과 사유. 실제 소스에서 없어진 항목은 제거한다.
const NOT_SENT_SENTINELS: &[(&str, &str)] = &[
    (
        "port.noop",
        "`tasty port` 는 run.rs 가 IPC 전에 로컬 처리한다",
    ),
    (
        "remote.check.noop",
        "`remote check` 는 run_client 가 SSH 터널 + 자체 IPC 로 선처리한다",
    ),
    (
        "remote.workspaces.noop",
        "`remote workspaces` 는 run_client 가 remote_browse 로 선처리한다",
    ),
    (
        "remote.new_workspace.noop",
        "`remote new-workspace` 는 run_client 가 remote_create 로 선처리한다",
    ),
    (
        "tool.ssh.noop",
        "`tasty tool ssh` 는 dispatch::classify 가 클라이언트 주도 실행으로 가져간다",
    ),
    (
        "tool.remote_profile.noop",
        "`tasty tool remote-profile` 은 dispatch::classify 가 가져간다(에이전트 조작은 remote.profile.* IPC)",
    ),
    (
        "tool.attach.noop",
        "`tasty tool attach` 는 dispatch::classify 가 가져간다",
    ),
    (
        "tool.passkey.noop",
        "`tasty tool passkey` 는 dispatch::classify 가 가져간다",
    ),
];

/// 소문자로 시작하고 점을 포함한 이름. JSON-RPC 버전 2.0은 제외한다.
fn is_method_name(name: &str) -> bool {
    name.contains('.')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
        && name.starts_with(|c: char| c.is_ascii_lowercase())
}

/// 바로 앞의 괄호·식별자로 호출 인자를 추정해 i18n 키 등을 제외한다. 전체 Rust 표현식을 해석하지는 않는다.
fn is_call_argument(before: &str, prev_lines: &[&str]) -> bool {
    let mut head = before.trim_end();
    if head.is_empty() {
        match prev_lines.iter().rev().find(|l| !l.trim().is_empty()) {
            Some(l) => head = l.trim(),
            None => return false,
        }
    }
    let Some(open) = head.strip_suffix('(') else {
        return false;
    };
    // 괄호 앞 식별자·!는 호출로 보고 단독 괄호와 => 뒤 괄호는 튜플로 본다.
    open.trim_end()
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '!')
}

fn value_position_literals<'a>(line: &'a str, prev_lines: &[&str]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    while let Some(rel) = line[idx..].find('"') {
        let start = idx + rel + 1;
        let Some(rel_end) = line[start..].find('"') else {
            break;
        };
        let end = start + rel_end;
        if !is_call_argument(&line[..start - 1], prev_lines) {
            out.push(&line[start..end]);
        }
        idx = end + 1;
    }
    out
}

/// 미등록 후보는 ns.method 형태로 제한한다. 점 없는 등록 메서드는 반대 방향의 비교로 확인한다.
fn value_position_methods<'a>(line: &'a str, prev_lines: &[&str]) -> Vec<&'a str> {
    value_position_literals(line, prev_lines)
        .into_iter()
        .filter(|name| is_method_name(name))
        .collect()
}

fn field_method(line: &str) -> Option<&str> {
    let pos = line.find("method:")?;
    let after = line[pos + "method:".len()..].trim_start();
    let rest = after.strip_prefix('"')?;
    let (name, _) = rest.split_once('"')?;
    is_method_name(name).then_some(name)
}

/// 바로 앞 비어 있지 않은 줄이 #[cfg(test)]인 인라인 mod를 제외한다.
/// 중괄호를 원문에서 세므로 리터럴·주석 속 괄호나 복합 cfg를 완전히 처리하지는 못한다.
fn test_module_lines(lines: &[&str]) -> Vec<bool> {
    let mut skip = vec![false; lines.len()];
    let mut i = 0usize;
    while i < lines.len() {
        let t = lines[i].trim();
        let is_mod_decl = t.starts_with("mod ")
            || t.starts_with("pub mod ")
            || t.starts_with("pub(crate) mod ")
            || t.starts_with("pub(super) mod ");
        let gated = lines[..i]
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .is_some_and(|l| l.trim() == "#[cfg(test)]");
        if is_mod_decl && gated && t.ends_with('{') {
            let mut depth = 0i32;
            while i < lines.len() {
                skip[i] = true;
                depth += lines[i].matches('{').count() as i32;
                depth -= lines[i].matches('}').count() as i32;
                i += 1;
                if depth <= 0 {
                    break;
                }
            }
            continue;
        }
        i += 1;
    }
    skip
}

fn gather_rs(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        gather_rs(&entry.path(), out);
    }
}

/// 별도 시험 파일은 공용 shipping_scope로 제외한다. 합성 메서드 이름을 센티널 예외로 등록해 숨기지 않는다.
fn declared_test_only(root: &Path, files: &[PathBuf]) -> BTreeSet<PathBuf> {
    let sources: Vec<(PathBuf, String)> = files
        .iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(root).ok()?.to_path_buf();
            let text = std::fs::read_to_string(p).ok()?.replace("\r\n", "\n");
            Some((rel, text))
        })
        .collect();
    tasty_doc_guards::shipping_scope::test_only_files(root, &sources)
        .into_iter()
        .map(|rel| root.join(rel))
        .collect()
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn every_cli_method_string_is_registered_in_method_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut request_files: Vec<PathBuf> =
        CLI_REQUEST_SOURCES.iter().map(|s| root.join(s)).collect();
    for dir in CLI_REQUEST_DIRS {
        gather_rs(&root.join(dir), &mut request_files);
    }
    request_files.sort();
    request_files.dedup();

    let mut found: Vec<(String, String)> = Vec::new(); // (메서드, 위치)
    for path in &request_files {
        let rel = rel_of(path, root);
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("CLI request 소스를 읽을 수 없다: {rel}: {e}"));
        let lines: Vec<&str> = src.lines().collect();
        let in_tests = test_module_lines(&lines);
        for (i, line) in lines.iter().enumerate() {
            if in_tests[i] {
                continue;
            }
            for name in value_position_methods(line, &lines[..i]) {
                found.push((name.to_string(), format!("{rel}:{}", i + 1)));
            }
        }
    }

    let mut all_files = Vec::new();
    gather_rs(&root.join(CLI_CRATE_ROOT), &mut all_files);
    all_files.sort();
    let declared_test_only = declared_test_only(root, &all_files);
    for path in &all_files {
        if declared_test_only.contains(path) {
            continue;
        }
        let rel = rel_of(path, root);
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        let in_tests = test_module_lines(&lines);
        for (i, line) in lines.iter().enumerate() {
            if in_tests[i] {
                continue;
            }
            if let Some(name) = field_method(line) {
                found.push((name.to_string(), format!("{rel}:{}", i + 1)));
            }
        }
    }

    assert!(
        found.len() > 200,
        "CLI 메서드 리터럴을 {}개만 수집했다(하한 200). 소스 목록과 추출 형식을 확인한다.",
        found.len()
    );

    let mut unknown: Vec<String> = Vec::new();
    let mut sentinel_hit: Vec<&str> = Vec::new();
    for (name, at) in &found {
        if let Some((s, _)) = NOT_SENT_SENTINELS.iter().find(|(s, _)| s == name) {
            sentinel_hit.push(s);
            continue;
        }
        if method_meta(name).is_none() {
            unknown.push(format!("  {at} — `{name}`"));
        }
    }
    unknown.sort();
    unknown.dedup();
    assert!(
        unknown.is_empty(),
        "CLI의 메서드 후보가 호스트 표에 없다. 오타면 수정하고 새 메서드는 표에 등록한다. IPC로 전송하지 않는 이름이면 NOT_SENT_SENTINELS에 근거를 적는다:\n{}",
        unknown.join("\n")
    );

    // 센티널이 사라진 뒤에도 예외로 남아 실제 전송 이름을 숨기지 않도록 대조한다.
    let stale: Vec<&str> = NOT_SENT_SENTINELS
        .iter()
        .map(|(s, _)| *s)
        .filter(|s| !sentinel_hit.contains(s))
        .collect();
    assert!(
        stale.is_empty(),
        "NOT_SENT_SENTINELS 에 있으나 소스에 없다 — 해당 경로가 삭제됐다면 목록에서도 지울 것:\n  {}",
        stale.join("\n  ")
    );
}

/// 미등록 후보 검색에서 빠지는 점 없는 메서드는 등록 표의 이름이 CLI 소스에 있는지 역방향으로 확인한다. 새 root 메서드는 명명 정책도 검토해야 한다.
#[test]
fn root_level_methods_are_reachable_from_the_cli() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut request_files: Vec<PathBuf> =
        CLI_REQUEST_SOURCES.iter().map(|s| root.join(s)).collect();
    for dir in CLI_REQUEST_DIRS {
        gather_rs(&root.join(dir), &mut request_files);
    }
    let corpus: String = request_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();

    let root_methods: Vec<&str> = METHOD_TABLE
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !name.contains('.'))
        .collect();
    assert!(
        !root_methods.is_empty(),
        "표에 무점 메서드가 하나도 없다 — 추출이 깨졌거나 규칙이 바뀌었다"
    );

    let unreachable: Vec<&str> = root_methods
        .iter()
        .copied()
        .filter(|name| !corpus.contains(&format!("\"{name}\"")))
        .collect();
    assert!(
        unreachable.is_empty(),
        "표의 root(무점) 메서드가 CLI request 소스에 리터럴로 없다 — 오타로 이름이 어긋났거나, \
         명명 규칙이 닫아 둔 root 예외에 CLI 없는 메서드가 새로 들어왔다:\n  {}",
        unreachable.join("\n  ")
    );
}

const CLI_GAP_DOC: &str = "docs/dev-guide/api-conventions.md";

const CLI_GAP_TABLE_HEADER: &str = "| 이유 | 메서드 | 왜 CLI 가 없나 |";

const CLI_GAP_DEBUG_TABLE_HEADER: &str = "| 이유 | debug 메서드 | 왜 CLI 가 없나 |";

/// 설명 문구를 제외한 백틱 코드 스팬의 이름을 정확히 비교한다. 접두사 일치로는 메서드 오타를 놓칠 수 있다.
fn code_spans(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// request 값 위치·CLI method 필드·번들 매니페스트의 ipc_method에서 읽은 이름을 합친다.
fn cli_reachable_methods(root: &Path) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();

    let mut request_files: Vec<PathBuf> =
        CLI_REQUEST_SOURCES.iter().map(|s| root.join(s)).collect();
    for dir in CLI_REQUEST_DIRS {
        gather_rs(&root.join(dir), &mut request_files);
    }
    for path in &request_files {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        let in_tests = test_module_lines(&lines);
        for (i, line) in lines.iter().enumerate() {
            if in_tests[i] {
                continue;
            }
            for name in value_position_literals(line, &lines[..i]) {
                out.insert(name.to_string());
            }
        }
    }

    let mut all_files = Vec::new();
    gather_rs(&root.join(CLI_CRATE_ROOT), &mut all_files);
    for path in &all_files {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        let in_tests = test_module_lines(&lines);
        for (i, line) in lines.iter().enumerate() {
            if !in_tests[i]
                && let Some(name) = field_method(line)
            {
                out.insert(name.to_string());
            }
        }
    }

    let mut manifests = Vec::new();
    collect_manifests(&root.join("crates"), &mut manifests);
    for path in &manifests {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in src.lines() {
            let mut rest = line;
            while let Some(pos) = rest.find("ipc_method") {
                let after = &rest[pos + "ipc_method".len()..];
                rest = after;
                let Some(eq) = after.trim_start().strip_prefix('=') else {
                    continue;
                };
                let Some(q) = eq.trim_start().strip_prefix('"') else {
                    continue;
                };
                if let Some((name, tail)) = q.split_once('"') {
                    out.insert(name.to_string());
                    rest = tail;
                }
            }
        }
    }
    out
}

/// 크레이트 바로 아래(`crates/<크레이트>/tasty-plugin.toml`)의 매니페스트만 모은다.
/// 더 깊은 곳의 매니페스트(시험 픽스처 등)는 이 workspace가 빌드하는 플러그인이 아니다.
fn collect_manifests(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join("tasty-plugin.toml");
        if manifest.is_file() {
            out.push(manifest);
        }
    }
    out.sort();
}

fn table_rows(text: &str, header: &str) -> Vec<Vec<String>> {
    let after_header = text
        .split_once(header)
        .unwrap_or_else(|| panic!("{CLI_GAP_DOC}: `{header}` 표 헤더를 찾지 못했다"))
        .1;
    after_header
        .lines()
        .skip(1) // `|---|---|---|` 구분선
        .take_while(|l| l.trim_start().starts_with('|'))
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
            if cells.len() != 3 {
                panic!("{CLI_GAP_DOC}: 표 행의 열 수가 3이 아니다: {line}");
            }
            if cells[0].trim().starts_with("---") {
                return None;
            }
            Some(cells.iter().map(|c| c.trim().to_string()).collect())
        })
        .collect()
}

fn listed_methods(text: &str, header: &str) -> Vec<String> {
    table_rows(text, header)
        .into_iter()
        .flat_map(|cells| {
            let methods = code_spans(&cells[1]);
            assert!(
                !methods.is_empty(),
                "{CLI_GAP_DOC}: 표 행의 메서드 열이 비었다: {cells:?}"
            );
            methods
        })
        .collect()
}

fn assert_documented(
    actual: &std::collections::BTreeSet<String>,
    text: &str,
    header: &str,
    count_marker: &str,
    what: &str,
) {
    let listed = listed_methods(text, header);
    let listed_set: std::collections::BTreeSet<String> = listed.iter().cloned().collect();
    assert_eq!(
        listed.len(),
        listed_set.len(),
        "{CLI_GAP_DOC}: 표에 같은 메서드가 두 번 적혔다 — 한 메서드의 이유는 한 곳이어야 한다"
    );

    let undocumented: Vec<&String> = actual.difference(&listed_set).collect();
    assert!(
        undocumented.is_empty(),
        "{what} 있으나 CLI 이름과 문서 사유를 찾지 못했다. 에이전트 기능이면 CLI를 제공하고 적용 대상이 아니라면 {CLI_GAP_DOC}에 이유를 적는다:\n  {}",
        undocumented
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    let stale: Vec<&String> = listed_set.difference(actual).collect();
    assert!(
        stale.is_empty(),
        "{CLI_GAP_DOC}의 사유 표와 현재 목록이 다르다. CLI 추가·메서드 삭제 여부를 확인해 오래된 행을 제거한다:\n  {}",
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    assert!(
        text.contains(count_marker),
        "{CLI_GAP_DOC}: 표 앞 산문의 개수가 실제({})와 다르다 — `{count_marker}` 로 맞춰라",
        actual.len()
    );
}

/// CLI 이름이 없는 release 메서드는 문서에 사유가 있어야 한다. 사유의 타당성은 자동으로 판단하지 않는다.
#[test]
fn methods_without_a_cli_entry_point_are_documented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let reachable = cli_reachable_methods(root);

    let actual: std::collections::BTreeSet<String> = METHOD_TABLE
        .iter()
        .map(|(name, _)| name.to_string())
        .filter(|name| !reachable.contains(name))
        .collect();

    let text = std::fs::read_to_string(root.join(CLI_GAP_DOC))
        .unwrap_or_else(|e| panic!("read {CLI_GAP_DOC}: {e}"));
    let marker = format!("총 {}개.", actual.len());
    assert_documented(
        &actual,
        &text,
        CLI_GAP_TABLE_HEADER,
        &marker,
        "release 표에",
    );
}

/// debug 전용 메서드의 CLI 부재도 별도 문서 표와 대조한다. release 표와 제공 빌드가 다르다.
#[test]
fn debug_methods_without_a_cli_entry_point_are_documented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let reachable = cli_reachable_methods(root);

    let actual: std::collections::BTreeSet<String> = tasty_ipc::method_meta::DEBUG_METHODS
        .iter()
        .map(|(name, _)| name.to_string())
        .filter(|name| !reachable.contains(name))
        .collect();

    let text = std::fs::read_to_string(root.join(CLI_GAP_DOC))
        .unwrap_or_else(|e| panic!("read {CLI_GAP_DOC}: {e}"));
    let marker = format!("debug 표 기준 총 {}개.", actual.len());
    assert_documented(
        &actual,
        &text,
        CLI_GAP_DEBUG_TABLE_HEADER,
        &marker,
        "debug 표에",
    );
}

fn top_level_commands(root: &Path) -> std::collections::BTreeSet<String> {
    let src = std::fs::read_to_string(root.join("crates/tasty-cli/src/lib.rs"))
        .expect("crates/tasty-cli/src/lib.rs 를 읽지 못했다");
    let start = src
        .find("pub enum Commands {")
        .expect("`pub enum Commands` 를 찾지 못했다 — 이 추출기가 낡았다");
    let body = &src[start..];
    let mut depth = 0i32;
    let mut out = std::collections::BTreeSet::new();
    let mut override_name: Option<String> = None;
    for line in body.lines() {
        let t = line.trim();
        if depth == 1 {
            if let Some(rest) = t.strip_prefix("#[command(name = \"") {
                if let Some((name, _)) = rest.split_once('"') {
                    override_name = Some(name.to_string());
                }
            } else if t.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                let ident: String = t
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !ident.is_empty() {
                    out.insert(override_name.take().unwrap_or_else(|| kebab(&ident)));
                }
            }
        }
        depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if depth <= 0 && !out.is_empty() {
            break;
        }
    }
    let mut manifests = Vec::new();
    collect_manifests(&root.join("crates"), &mut manifests);
    for path in manifests {
        let manifest: toml::Value = std::fs::read_to_string(&path)
            .expect("plugin manifest readable")
            .parse()
            .expect("plugin manifest valid TOML");
        if let Some(cli) = manifest
            .get("contributes")
            .and_then(|v| v.get("cli"))
            .and_then(toml::Value::as_array)
        {
            for command in cli {
                if let Some(name) = command.get("name").and_then(toml::Value::as_str) {
                    out.insert(name.to_owned());
                }
            }
        }
    }

    out
}

fn kebab(ident: &str) -> String {
    let mut out = String::new();
    for (i, c) in ident.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// 문서가 대안으로 안내한 최상위 명령의 존재를 확인한다. 하위 명령·옵션은 검사하지 않는다.
#[test]
fn commands_cited_as_alternatives_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let known = top_level_commands(root);
    assert!(
        known.len() > 20,
        "최상위 명령을 {}개만 추출했다. 선언 형식과 수집 범위를 확인한다.",
        known.len()
    );
    let text = std::fs::read_to_string(root.join(CLI_GAP_DOC))
        .unwrap_or_else(|e| panic!("read {CLI_GAP_DOC}: {e}"));

    // 없는 명령을 예로 설명한 일반 산문을 오인하지 않도록 사유 열만 읽는다.
    let reasons: Vec<String> = [CLI_GAP_TABLE_HEADER, CLI_GAP_DEBUG_TABLE_HEADER]
        .iter()
        .flat_map(|h| table_rows(&text, h))
        .map(|cells| cells[2].clone())
        .collect();

    let mut bad: Vec<String> = Vec::new();
    for span in reasons.iter().flat_map(|r| {
        r.split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect::<Vec<_>>()
    }) {
        let Some(rest) = span.strip_prefix("tasty ") else {
            continue;
        };
        let first = rest.split_whitespace().next().unwrap_or_default();
        if first.is_empty() || first.starts_with('<') || first.starts_with('-') {
            continue;
        }
        if !known.contains(first) {
            bad.push(format!(
                "`tasty {first} …` — 그런 최상위 명령이 없다 (인용: `{span}`)"
            ));
        }
    }
    bad.sort();
    bad.dedup();
    assert!(
        bad.is_empty(),
        "{CLI_GAP_DOC}가 없는 최상위 명령을 대안으로 안내한다:\n  {}",
        bad.join("\n  ")
    );
}
