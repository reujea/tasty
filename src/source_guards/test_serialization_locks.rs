//! 프로세스 전역 상태를 쓰는 시험이 등록된 직렬화 락도 언급하는지 확인한다.
//! 병렬 시험의 상태 변경이 서로 섞이지 않으려면 해당 전역에 접근하는 모든 시험이 같은 락을 사용해야 한다.
//!
//! 전역 이름·접근 함수 이름과 락·획득 헬퍼 이름의 존재를 텍스트로 비교한다.
//! 실제 획득·락 수명·실행 경로까지 보장하지 않으며 등록되지 않은 간접 접근은 놓칠 수 있다.
//! 환경변수·cwd·외부 자원을 지키는 락은 별도 사유 명부로 관리한다.
//! static Mutex<()>·RwLock<()> 형태의 선언을 모아 새 락의 미등록도 확인한다.

mod mutations;

use std::collections::BTreeSet;

use super::{
    mask_non_code, repo_root, rust_sources, rust_sources_with_integration_tests, word_positions,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// 다른 파일에서 접근할 수 없을 때만 선언 파일 안으로 검사 범위를 제한한다.
    File,
    /// 다른 파일에서 접근할 수 있어 크레이트 전체를 검사한다.
    Crate,
}

struct Serialized {
    file: &'static str,
    lock: &'static str,
    /// 락 이름과 획득 헬퍼의 검색 형태.
    acquire: &'static [&'static str],
    /// 보호하는 전역 및 접근 함수의 검색 이름.
    guarded: &'static [&'static str],
    scope: Scope,
    /// 이 상태를 병렬로 바꾸면 생기는 구체적인 경합.
    why: &'static str,
}

const SERIALIZED: &[Serialized] = &[
    Serialized {
        file: "src/host_api/webview/http_endpoint/runtime/tests.rs",
        lock: "TEST",
        acquire: &["TEST", "serial()"],
        guarded: &["retirement::owners()", "reaped()"],
        scope: Scope::Crate,
        why: "Runtime ownership/count controls share the process-wide retired runtime registry and eight setup permits across test modules.",
    },
    Serialized {
        file: "src/runtime/surface_registry/webview_kind.rs",
        lock: "WEBVIEW_KIND_TEST_LOCK",
        acquire: &["WEBVIEW_KIND_TEST_LOCK"],
        guarded: &[
            "WEBVIEW_KINDS",
            "register_webview_kind",
            "is_webview_kind",
            "webview_settings_source",
            "reset_for_test",
        ],
        scope: Scope::Crate,
        why: "다른 파일의 시험도 webview kind를 등록한다. 동시에 markdown 미등록 상태를 확인하는 시험과 겹치면 단언이 실패할 수 있다.",
    },
    Serialized {
        file: "crates/tasty-platform/src/stall_watchdog.rs",
        lock: "GLOBALS",
        acquire: &["GLOBALS"],
        guarded: &["SEQ", "PAUSED"],
        scope: Scope::File,
        why: "워치독의 시퀀스·일시정지 상태가 프로세스 전역이라, 그것을 실제로 바꾸는 \
              테스트끼리 순서에 의존한다",
    },
    Serialized {
        file: "crates/tasty-ipc/src/method_meta_tests.rs",
        lock: "TEST_LOCK",
        acquire: &["TEST_LOCK", "test_lock()"],
        guarded: &[
            "test_namespace_table",
            "ns_clear",
            "ns_register",
            "ns_unregister",
        ],
        scope: Scope::Crate,
        why: "프로세스에 한 번 설치한 namespace 표를 모든 시험이 공유한다. 한 시험이 비우거나 채우는 중에 다른 시험이 읽으면 결과가 달라진다.",
    },
    Serialized {
        file: "crates/tasty-themes/src/plugin_defaults.rs",
        lock: "LOCK",
        acquire: &["LOCK", "test_guard()", "reset()"],
        guarded: &["PLUGIN_DEFAULTS", "USER_DEFINED_KINDS"],
        scope: Scope::Crate,
        why: "테마 기본값 전역을 `reset()` 으로 비우고 채우는 형태라, 병렬로 돌면 서로의 \
              초기화가 상대의 단언 중간에 끼어든다",
    },
];

/// 전역 변수의 이름 비교 대신 다른 규칙으로 검사할 락과 근거.
const OTHER_LOCKS: &[(&str, &str, &str)] = &[
    (
        "src/clipboard/files.rs",
        "OS_CLIPBOARD_LOCK",
        "실제 OS 클립보드(X11 CLIPBOARD 선택). 격리된 X 서버에서 도는 ignored 시험들이 같은 선택을 \
         잡으므로 하나씩 돌린다",
    ),
    (
        "crates/tasty-test-support/src/lib.rs",
        "TASTY_HOME_ENV_LOCK",
        "`TASTY_HOME` 환경변수. 획득·복원을 `TastyHomeGuard` 가 함께 맡는다",
    ),
    (
        "crates/tasty-host-plugin/src/test_support.rs",
        "HOME_ENV_LOCK",
        "홈 환경변수 변경은 같은 모듈의 소스 검사에서 지원 가드를 통하도록 확인한다.",
    ),
    (
        "crates/tasty-settings/src/general.rs",
        "SERIAL",
        "`TASTY_HOME` 을 실제로 만질 수밖에 없는 소수의 테스트(상대 경로 해석 자체가 \
         검증 대상)와 cwd 오염 canary",
    ),
    (
        "crates/tasty-telemetry/src/agent_id.rs",
        "ENV_LOCK",
        "`TASTY_AGENT_ID` 환경변수",
    ),
    (
        "crates/tasty-cli/src/cwd_test.rs",
        "CWD_LOCK",
        "프로세스 cwd를 보호한다. 같은 크레이트의 set_current_dir 재진입 검사와 cwd 읽기 검사에서 별도로 확인한다.",
    ),
    (
        "tests/attach_common/mod.rs",
        "WRITE_LOCK",
        "attach 스트림의 heartbeat와 본문 프레임 쓰기가 섞이지 않도록 직렬화한다.",
    ),
    (
        "tests/e2e_tests.rs",
        "WINDOW_EXCLUSIVE",
        "GUI 창을 사용하는 시나리오를 읽기·쓰기 락으로 구분한다. 프로세스 전역 변수의 보호와는 다르다.",
    ),
];

/// 2026-09-06 직렬화 대상 시험 14개를 측정했다. 대상이 비거나 크게 줄면 수집을 확인할 하한이다.
const MIN_GUARDED_TESTS: usize = 10;

fn test_fns(masked: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for at in word_positions(masked, "#[test]") {
        let Some(f) = masked[at..].find("fn ") else {
            continue;
        };
        let after = at + f + 3;
        let Some(paren) = masked[after..].find('(') else {
            continue;
        };
        let name = masked[after..after + paren].trim().to_string();
        let Some(open) = masked[after..].find('{') else {
            continue;
        };
        let start = after + open;
        let mut depth = 0usize;
        let mut end = start;
        for (i, c) in masked[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push((name, masked[start..end].to_string()));
    }
    out
}

fn crate_root(file: &str) -> String {
    file.split_once('/')
        .filter(|(head, _)| *head == "crates")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map_or_else(|| "src".to_string(), |(name, _)| format!("crates/{name}"))
}

/// STATE와 STATE_POISON_REPORTED를 혼동하지 않도록 식별자 전체를 비교한다.
fn mentions(body: &str, names: &[&str]) -> bool {
    names.iter().any(|n| {
        n.strip_suffix("()").map_or_else(
            || !word_positions(body, n).is_empty(),
            |bare| body.contains(&format!("{bare}(")),
        )
    })
}

#[test]
fn every_test_that_touches_a_serialized_global_holds_its_lock() {
    let mut seen = 0usize;
    let mut violations: Vec<String> = Vec::new();
    let sources = rust_sources();
    for entry in SERIALIZED {
        let root = crate_root(entry.file);
        for (path, text) in &sources {
            let rel = path.to_string_lossy().replace('\\', "/");
            let in_scope = match entry.scope {
                Scope::File => rel == entry.file,
                Scope::Crate => rel.starts_with(&root),
            };
            if !in_scope {
                continue;
            }
            let masked = mask_non_code(text);
            for (name, body) in test_fns(&masked) {
                if !mentions(&body, entry.guarded) {
                    continue;
                }
                seen += 1;
                if !mentions(&body, entry.acquire) {
                    violations.push(format!("{rel}::{name} — `{}` 를 안 잡는다", entry.lock));
                }
            }
        }
    }
    assert!(
        seen >= MIN_GUARDED_TESTS,
        "직렬화 대상 시험을 {seen}개만 찾았다(하한 {MIN_GUARDED_TESTS}, 2026-09-06 측정 14개). 시험과 접근 이름의 수집을 확인한다."
    );
    assert!(
        violations.is_empty(),
        "전역 상태를 쓰는 시험에서 직렬화 락의 사용을 찾지 못했다. 모든 접근이 같은 락으로 보호되는지 확인한다: {violations:#?}"
    );
}

/// The declaration parser is shared with negative controls, including instance-owned state.
fn declared_locks(masked: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in masked.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("static ").or_else(|| {
            t.strip_prefix("pub static ")
                .or_else(|| t.split_once("static ").map(|(_, r)| r))
        }) else {
            continue;
        };
        let Some((name, ty)) = rest.split_once(':') else {
            continue;
        };
        let ty = ty.replace(' ', "");
        if ty.contains("Mutex<()>") || ty.contains("RwLock<()>") {
            found.insert(name.trim().to_string());
        }
    }
    found
}

fn lock_drift(
    actual: &BTreeSet<(String, String)>,
    listed: &BTreeSet<(String, String)>,
) -> Vec<String> {
    actual
        .difference(listed)
        .map(|(f, l)| format!("unlisted: {f}::{l}"))
        .chain(
            listed
                .difference(actual)
                .map(|(f, l)| format!("stale: {f}::{l}")),
        )
        .collect()
}

/// static Mutex<()>·RwLock<()> 선언의 정확한 명부다. 다른 타입·선언 형식은 놓칠 수 있다.
#[test]
fn every_serialization_lock_is_listed() {
    let listed: BTreeSet<(String, String)> = SERIALIZED
        .iter()
        .map(|e| (e.file.to_string(), e.lock.to_string()))
        .chain(
            OTHER_LOCKS
                .iter()
                .map(|(f, l, _)| (f.to_string(), l.to_string())),
        )
        .collect();
    let mut found = BTreeSet::new();
    // 루트 통합 시험도 프로세스 전역을 공유할 수 있어 수집에 포함한다.
    for (path, text) in rust_sources_with_integration_tests() {
        let rel = path.to_string_lossy().replace('\\', "/");
        found.extend(
            declared_locks(&mask_non_code(&text))
                .into_iter()
                .map(|name| (rel.clone(), name)),
        );
    }
    assert!(
        found.len() >= listed.len(),
        "직렬화 락을 {}개만 찾았다(명부 {}개). 선언 수집을 확인한다.",
        found.len(),
        listed.len()
    );
    let drift = lock_drift(&found, &listed);
    assert!(
        drift.is_empty(),
        "직렬화 락 명부와 실제 선언이 다르다: {drift:?}. 보호하는 전역과 접근 범위를 SERIALIZED에 적거나, 환경변수·cwd 등 다른 자원이면 OTHER_LOCKS에 이유를 등록한다."
    );
}

#[test]
fn each_entry_names_something_that_exists() {
    for entry in SERIALIZED {
        let text = std::fs::read_to_string(repo_root().join(entry.file))
            .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", entry.file));
        let masked = mask_non_code(&text);
        assert!(
            !word_positions(&masked, entry.lock).is_empty(),
            "{} 에 `{}` 이 없다 — 락이 사라졌거나 이름이 바뀌었다",
            entry.file,
            entry.lock
        );
        assert!(
            entry.why.chars().count() >= 20,
            "`{}` 의 사유가 너무 짧다 — 락마다 다른 경합을 적는 자리다",
            entry.lock
        );
        for name in entry.guarded {
            let root = crate_root(entry.file);
            let anywhere = rust_sources().into_iter().any(|(p, t)| {
                p.to_string_lossy().replace('\\', "/").starts_with(&root)
                    && !word_positions(&mask_non_code(&t), name).is_empty()
            });
            assert!(
                anywhere,
                "`{}` 이 지킨다는 `{name}` 이 {root} 어디에도 없다 — 접근면이 낡았다",
                entry.lock
            );
        }
    }
    for (file, lock, why) in OTHER_LOCKS {
        let text = std::fs::read_to_string(repo_root().join(file))
            .unwrap_or_else(|e| panic!("{file} 을 읽지 못했다: {e}"));
        assert!(
            !word_positions(&mask_non_code(&text), lock).is_empty(),
            "{file} 에 `{lock}` 이 없다"
        );
        assert!(why.chars().count() >= 10, "`{lock}` 의 사유가 비었다");
    }
}

/// OTHER_LOCKS의 락이 비공개인 전제를 확인한다. 외부에 공개하려면 해당 자원을 보호하는 검증도 검토해야 한다.
/// 선언 형식을 읽지 못하면 비공개로 추정하지 않고 실패시킨다. 콜론 앞 공백도 이 판독기는 지원하지 않는다.
/// 다른 파일의 시험에서 쓰도록 공개한 SERIALIZED 락에는 이 조건을 적용하지 않는다.
#[test]
fn every_listed_lock_is_still_module_private() {
    let mut checked = 0usize;
    let mut widened = Vec::new();
    for (file, lock, _) in OTHER_LOCKS {
        let text = std::fs::read_to_string(repo_root().join(file))
            .unwrap_or_else(|e| panic!("{file} 을 읽지 못했다: {e}"));
        let masked = mask_non_code(&text);
        let decl = masked
            .lines()
            .enumerate()
            .find_map(|(i, l)| {
                let t = l.trim_start();
                let rest = t.strip_prefix("pub").map_or(t, |r| {
                    r.trim_start()
                        .strip_prefix('(')
                        .and_then(|r| r.split_once(')'))
                        .map_or(r.trim_start(), |(_, after)| after.trim_start())
                });
                let after = rest.strip_prefix("static ")?;
                after
                    .trim_start()
                    .starts_with(&format!("{lock}:"))
                    .then_some((i + 1, t.to_string()))
            })
            .unwrap_or_else(|| {
                panic!("{file}에서 static {lock} 선언을 읽지 못했다. 이름과 선언 형식을 확인한다.")
            });
        checked += 1;
        if decl.1.starts_with("pub") {
            widened.push(format!("  {file}:{}  {}", decl.0, decl.1));
        }
    }
    assert_eq!(
        checked,
        OTHER_LOCKS.len(),
        "명부 {}개 중 {checked}개만 읽었다. 선언 추출을 확인한다.",
        OTHER_LOCKS.len()
    );
    assert!(
        widened.is_empty(),
        "OTHER_LOCKS의 락이 공개됐다:\n{}\n외부 접근을 허용해야 한다면 해당 자원을 보호할 검사와 근거도 함께 갱신한다.",
        widened.join("\n")
    );
}

/// 파일 단위 검사로 제한한 항목의 보호 대상 선언이 공개돼 있지 않은지 확인한다. 일부 선언 형태만 읽는 텍스트 검사다.
#[test]
fn a_file_scoped_entry_guards_only_names_invisible_outside_it() {
    let root = repo_root();
    let mut checked = 0usize;
    for e in SERIALIZED.iter().filter(|e| e.scope == Scope::File) {
        let src = std::fs::read_to_string(root.join(e.file))
            .unwrap_or_else(|err| panic!("{} 을 읽지 못했다: {err}", e.file));
        let masked = mask_non_code(&src);
        let mut found_any = false;
        for name in e.guarded {
            for line in masked.lines() {
                let t = line.trim_start();
                let decl = t.contains(&format!("static {name}"))
                    || t.contains(&format!("fn {name}("))
                    || t.contains(&format!("struct {name}"));
                if !decl {
                    continue;
                }
                found_any = true;
                checked += 1;
                assert!(
                    !t.starts_with("pub"),
                    "{}의 {name}이 공개돼 있지만 파일 안에서만 검사한다. 범위를 Crate로 넓히거나 공개 범위를 제한한다: {}",
                    e.file,
                    t.trim()
                );
            }
        }
        assert!(
            found_any,
            "{}에서 보호 대상 선언을 찾지 못했다. 이동·이름·선언 형식과 검사 범위를 확인한다.",
            e.file
        );
    }
    assert!(
        checked > 0,
        "파일 범위로 검사할 항목이 없다. 명부와 검사 필요성을 확인한다."
    );
}
