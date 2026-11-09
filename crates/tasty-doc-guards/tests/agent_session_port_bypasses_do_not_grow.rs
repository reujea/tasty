//! agent-session 플러그인 제품 코드에서 포트를 우회하는 참조 수를 기준값에 고정한다.
//! 로직은 `HostCall`·`Supervisor` 포트만 보고, SDK 구체 타입·어댑터 생성·IPC 오류 타입은
//! 조립 루트(main.rs)와 IPC 처리기(ipc.rs)에만 둔다는 방향이다. 규칙마다 그 밖의 수를 기준값으로
//! 두고(지금은 모두 0), 늘면 새 우회라 실패하고 줄면 기준값을 같은 커밋에서 낮추도록 실패한다 —
//! 낮춘 값 아래로 되돌아가지 않게 하는 래칫이다.
//!
//! 주석·문자열, 인라인 cfg(test) 항목, cfg(test)로만 선언된 파일은 제외한다.
//! 식별자 경계로 일치를 찾으며 `use … as` 별칭, 매크로가 만든 코드, 줄을 넘는 경로는 보지 못한다.
//! 자리는 일치가 있는 줄 수다. 통과할 때의 측정 수는 다음 명령으로 볼 수 있다.
//! ```text
//! cargo test -p tasty-doc-guards --test agent_session_port_bypasses_do_not_grow -- --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use tasty_doc_guards::cfg_predicate::cfg_gated_lines;
use tasty_doc_guards::repo_root;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};

const SCAN_ROOT: &str = "crates/tasty-plugin-agent-session/src";

/// 2026-11-05 제품 파일 63개(전체 64개 중 테스트 전용 1개 제외)를 기준으로 둔 수집 하한.
/// 모듈 통폐합은 허용하고 스캔 경로가 틀려 거의 아무것도 못 읽는 경우를 잡는다.
const MIN_SHIPPED_FILES: usize = 40;

struct Rule {
    /// 찾는 표기. 양 끝에 식별자 경계를 요구한다.
    needle: &'static str,
    /// 기준값에서 빼는 파일. 판정 대상이 아니라 일치가 있는지로 수집·일치기가 동작함을 확인한다.
    allowed: &'static [&'static str],
    /// allowed 밖에서 일치가 있는 파일 수의 기준값.
    modules: usize,
    /// allowed 밖에서 일치가 있는 줄 수의 기준값.
    sites: usize,
    /// 늘었을 때 대신 쓸 방법.
    instead: &'static str,
}

const MAIN: &str = "crates/tasty-plugin-agent-session/src/main.rs";
const IPC: &str = "crates/tasty-plugin-agent-session/src/adapters/driving/ipc.rs";

/// 기준값은 2026-11-05 측정값이다. 줄면 그 값으로 낮춘다. 올리지 않는다.
const RULES: &[Rule] = &[
    Rule {
        needle: "HostHandle",
        allowed: &[MAIN],
        modules: 0,
        sites: 0,
        instead: "SDK 핸들 대신 `H: HostCall` 을 받고 main.rs 에서 `SdkHost` 로 감싸 넘긴다",
    },
    Rule {
        needle: "LocalSupervisor::new",
        allowed: &[MAIN],
        modules: 0,
        sites: 0,
        instead: "안에서 만들지 말고 `S: Supervisor` 인자로 받아 main.rs 에서 주입한다",
    },
    Rule {
        needle: "SdkHost",
        allowed: &[MAIN],
        modules: 0,
        sites: 0,
        instead: "`&impl HostCall` 을 받아 호출자가 감싼 값을 그대로 넘긴다",
    },
    Rule {
        needle: "IpcMethodError",
        allowed: &[MAIN, IPC],
        modules: 0,
        sites: 0,
        instead: "로직은 자체 오류를 돌려주고 IPC 오류 변환은 ipc.rs 에서 한다",
    },
];

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 마스킹한 한 줄에 needle 이 식별자 경계를 갖춰 나오는가.
fn mentions(code: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = code[from..].find(needle) {
        let at = from + pos;
        let end = at + needle.len();
        let before = code[..at].chars().next_back().is_some_and(is_ident);
        let after = code[end..].chars().next().is_some_and(is_ident);
        if !before && !after {
            return true;
        }
        from = end;
    }
    false
}

/// 한 파일에서 테스트 전용이 아닌 코드 줄 가운데 needle 이 나오는 줄(1 기반 번호, 원문).
fn shipped_hits(text: &str, needle: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let gated = cfg_gated_lines(&lines, "test");
    let masked = mask_non_code(text);
    masked
        .lines()
        .enumerate()
        .filter(|(i, code)| !gated[*i] && mentions(code, needle))
        .map(|(i, _)| (i + 1, lines[i].trim().to_string()))
        .collect()
}

struct Count {
    modules: usize,
    sites: Vec<String>,
    allowed_hits: usize,
}

fn count(sources: &[(PathBuf, String)], rule: &Rule) -> Count {
    let mut per_file: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    let mut allowed_hits = 0usize;
    for (rel, text) in sources {
        let rel_s = rel.display().to_string();
        let hits = shipped_hits(text, rule.needle);
        if rule.allowed.contains(&rel_s.as_str()) {
            allowed_hits += hits.len();
        } else if !hits.is_empty() {
            per_file.insert(rel_s, hits);
        }
    }
    let sites = per_file
        .iter()
        .flat_map(|(f, hits)| hits.iter().map(move |(n, l)| format!("  {f}:{n}: {l}")))
        .collect();
    Count {
        modules: per_file.len(),
        sites,
        allowed_hits,
    }
}

#[test]
fn port_bypasses_stay_at_the_baseline() {
    let root = repo_root();
    let all = rust_sources(&root, &[SCAN_ROOT]);
    let test_only = test_only_files(&root, &all);
    let shipped: Vec<(PathBuf, String)> = all
        .iter()
        .filter(|(p, _)| !test_only.contains(p))
        .cloned()
        .collect();
    assert!(
        shipped.len() >= MIN_SHIPPED_FILES,
        "{SCAN_ROOT} 의 제품 파일을 {}개만 수집했다(하한 {MIN_SHIPPED_FILES}). 스캔 경로를 확인한다.",
        shipped.len()
    );

    let mut problems: Vec<String> = Vec::new();
    for rule in RULES {
        let c = count(&shipped, rule);
        let sites = c.sites.len();
        println!(
            "[agent-session 포트 우회] {} · 모듈 {}/{} · 자리 {}/{} · 허용 파일 일치 {}",
            rule.needle, c.modules, rule.modules, sites, rule.sites, c.allowed_hits
        );
        // 허용 파일에서도 0이면 일치기나 수집이 망가져 아무것도 안 본 것일 수 있다.
        if c.allowed_hits == 0 {
            problems.push(format!(
                "{}: 허용 파일({})에서도 일치가 0이다. 일치기·수집 범위를 확인한다 — 이 상태의 통과는 판정이 아니다.",
                rule.needle,
                rule.allowed.join(", ")
            ));
            continue;
        }
        if c.modules > rule.modules || sites > rule.sites {
            problems.push(format!(
                "{}: 포트 우회가 늘었다 — 모듈 {} (기준 {}) · 자리 {} (기준 {}). {}.\n{}",
                rule.needle,
                c.modules,
                rule.modules,
                sites,
                rule.sites,
                rule.instead,
                c.sites.join("\n")
            ));
        } else if c.modules < rule.modules || sites < rule.sites {
            problems.push(format!(
                "{}: 포트 우회가 줄었다 — 모듈 {} (기준 {}) · 자리 {} (기준 {}). 이 파일의 RULES 기준값을 modules: {}, sites: {} 로 낮춘다.",
                rule.needle, c.modules, rule.modules, sites, rule.sites, c.modules, sites
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

/// 데몬 핸들 실물이 정의된 파일 — `Default` 를 갖지 않는지 본다.
const LOCAL: &str = "crates/tasty-plugin-agent-session/src/adapters/driven/local.rs";

/// 한 파일의 제품 코드에서 `LocalSupervisor` 에 `Default` 를 주는 자리(줄 번호, 원문).
///
/// `#[derive(Default)]` 구조체의 칸이 `LocalSupervisor` 면 `default()` 가 핸들을 몰래 만든다. 그 생성은
/// 소스에 `LocalSupervisor::new` 로 안 나타나 위 규칙이 못 센다. 핸들에 `Default` 가 없으면 그런 derive 를
/// 컴파일러가 막으므로, 여기서는 핸들 쪽에 `Default` 가 생기지 않는지만 본다 — `impl Default for` 와
/// 구조체 바로 위 derive 의 `Default` 두 형태다.
fn default_for_handle(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let gated = cfg_gated_lines(&lines, "test");
    let masked = mask_non_code(text);
    let code: Vec<&str> = masked.lines().collect();
    let mut hits = Vec::new();
    for (i, l) in code.iter().enumerate() {
        if gated[i] {
            continue;
        }
        let compact: String = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if compact.contains("Default for LocalSupervisor") && mentions(l, "LocalSupervisor") {
            hits.push((i + 1, lines[i].trim().to_string()));
        }
        if mentions(l, "LocalSupervisor") && l.trim_start().starts_with("pub struct") {
            // 구조체 바로 위의 속성 줄들.
            let mut j = i;
            while j > 0 && code[j - 1].trim_start().starts_with("#[") {
                j -= 1;
                if code[j].contains("derive") && mentions(code[j], "Default") {
                    hits.push((j + 1, lines[j].trim().to_string()));
                }
            }
        }
    }
    hits
}

#[test]
fn the_supervisor_handle_has_no_default() {
    let root = repo_root();
    let text = std::fs::read_to_string(root.join(LOCAL))
        .unwrap_or_else(|e| panic!("{LOCAL} 를 못 읽었다: {e}"));
    // 분모 — 구조체 정의를 못 찾으면 아래 판정은 아무것도 안 본 것이다.
    assert!(
        text.contains("pub struct LocalSupervisor"),
        "{LOCAL} 에 `pub struct LocalSupervisor` 가 없다. 옮겼으면 LOCAL 경로를 같은 커밋에서 고친다."
    );
    let hits = default_for_handle(&text);
    assert!(
        hits.is_empty(),
        "LocalSupervisor 에 Default 가 생겼다 — `default()` 가 조립 루트 밖에서 핸들을 몰래 만든다. \
         `SupervisorPlugin::new(sup)` 처럼 인자로 받는다.\n{}",
        hits.iter()
            .map(|(n, l)| format!("  {LOCAL}:{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// 판정기가 두 형태를 잡고, 시험 코드 · 다른 타입은 안 잡는지 합성 입력으로 본다.
#[test]
fn the_default_detector_sees_both_forms() {
    let derive = "#[derive(Clone, Copy, Default)]\npub struct LocalSupervisor {\n}\n";
    let manual =
        "impl Default for LocalSupervisor {\n    fn default() -> Self { Self::new() }\n}\n";
    let fine = "#[derive(Clone, Copy)]\npub struct LocalSupervisor {\n}\n\
                #[derive(Default)]\npub struct Other {}\n\
                #[cfg(test)]\nimpl Default for LocalSupervisor {\n    fn default() -> Self { todo!() }\n}\n";
    assert_eq!(default_for_handle(derive).len(), 1, "derive 를 못 잡았다");
    assert_eq!(default_for_handle(manual).len(), 1, "impl 을 못 잡았다");
    assert!(
        default_for_handle(fine).is_empty(),
        "잡으면 안 되는 것을 잡았다: {:?}",
        default_for_handle(fine)
    );
}

#[test]
fn the_matcher_requires_identifier_boundaries() {
    let cases: &[(&str, &str, bool)] = &[
        ("fn f(h: &HostHandle) {}", "HostHandle", true),
        ("use tasty_plugin_sdk::{HostHandle, X};", "HostHandle", true),
        ("fn f(h: &HostHandleExt) {}", "HostHandle", false),
        ("let x = MyHostHandle;", "HostHandle", false),
        (
            "crate::local::LocalSupervisor::new()",
            "LocalSupervisor::new",
            true,
        ),
        (
            "LocalSupervisor::new_for(reg)",
            "LocalSupervisor::new",
            false,
        ),
        ("crate::SdkHost(host)", "SdkHost", true),
    ];
    let mut wrong = Vec::new();
    for (code, needle, want) in cases {
        let got = mentions(code, needle);
        if got != *want {
            wrong.push(format!("  `{code}` / {needle} → {got} (기대 {want})"));
        }
    }
    assert!(
        wrong.is_empty(),
        "식별자 경계 판정이 틀렸다:\n{}",
        wrong.join("\n")
    );
}

/// 실제 소스에 해당 형태가 없어도 테스트 코드·주석·문자열 제외가 동작하는지 합성 입력으로 확인한다.
#[test]
fn test_code_comments_and_literals_are_not_sites() {
    let src = "\
fn shipped(h: &HostHandle) {}

// HostHandle 은 주석 안에 있다
const S: &str = \"HostHandle\";

#[cfg(test)]
mod tests {
    fn fake(h: &HostHandle) {}
}

#[cfg(test)]
fn helper(h: &HostHandle) {}
";
    let got: Vec<usize> = shipped_hits(src, "HostHandle")
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(
        got,
        vec![1],
        "제품 코드 줄 하나(1 행)만 잡혀야 한다. 3·4 행이 잡히면 마스킹이, 8·12 행이 잡히면 \
         cfg(test) 제외가 적용되지 않은 것이다."
    );
}
