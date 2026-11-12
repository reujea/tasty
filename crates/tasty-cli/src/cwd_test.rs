//! 프로세스 cwd를 바꾸거나 읽는 lib 시험이 함께 쓰는 직렬화 락과 가드.
//! cwd는 이 시험 바이너리에 하나뿐이라 바꾸는 시험과 읽는 시험이 같은 락을 잡아야 한다.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// cwd를 바꾸거나 읽는 이 시험 바이너리의 모든 시험은 이 락으로 직렬화한다.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// 가드 수명 동안 `CWD_LOCK` 을 쥔다. `enter` 로 만들면 이전 cwd를 저장하고 패닉 시에도 복원한다.
/// Drop 본문이 필드보다 먼저 실행되므로 복원은 CWD_LOCK을 잡은 상태에서 이루어진다.
pub(crate) struct CwdGuard {
    prev: Option<PathBuf>,
    /// **복원이 락 안에서 나는 근거는 필드 순서가 아니다** — `Drop::drop` 이 필드보다
    /// 먼저 돌기 때문에 이 핸들이 어느 자리에 있든 [`Drop::drop`] 본문 뒤에 풀린다.
    _lock: MutexGuard<'static, ()>,
}

impl CwdGuard {
    /// 락을 잡고 cwd를 `dir` 로 바꾼다. 가드가 사라질 때 이전 cwd로 되돌린다.
    pub(crate) fn enter(dir: &Path) -> Self {
        // 락을 먼저 잡는다 — 아래 `current_dir()` 읽기까지 직렬화 범위에 넣는다.
        let lock = lock();
        let prev = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir).expect("set_current_dir");
        Self {
            prev: Some(prev),
            _lock: lock,
        }
    }
}

/// cwd는 바꾸지 않고 락만 잡는다. cwd를 두 번 이상 읽는 시험(해석값과 `current_dir()` 비교)이
/// 그 사이에 다른 시험의 `CwdGuard::enter` 로 cwd가 바뀌지 않게 한다.
pub(crate) fn hold() -> CwdGuard {
    CwdGuard {
        prev: None,
        _lock: lock(),
    }
}

fn lock() -> MutexGuard<'static, ()> {
    CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let Some(prev) = &self.prev else {
            return;
        };
        // 이유: CWD_LOCK은 Drop 본문 뒤에 해제된다.
        // 복원 실패를 알리되 이미 unwind 중이면 두 번째 패닉으로 abort하지 않는다.
        if std::env::set_current_dir(prev).is_err() && !std::thread::panicking() {
            panic!("cwd 를 {} 로 되돌리지 못했다", prev.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 주석·문자열을 제외한 코드에서 set_current_dir 호출 줄을 센다.
    fn cwd_mutation_call_lines(text: &str) -> usize {
        tasty_doc_guards::source_text::mask_non_code(text)
            .lines()
            .filter(|l| l.contains("set_current_dir("))
            .count()
    }

    /// `dir` 아래 `.rs` 파일을 모두 읽어 `visit(상대경로, 원문)` 을 부른다. 텍스트 스캔(런타임
    /// `read_dir`/`read_to_string` + `CARGO_MANIFEST_DIR`)이라 cfg 로 컴파일에서 빠지는 파일도
    /// 디스크에서 그대로 읽는다 — 두 CI 채널에서 같은 것을 본다.
    fn walk_rs_sources(dir: &Path, base: &Path, visit: &mut dyn FnMut(String, &str)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 빌드 산출물과 점 디렉터리의 로컬 사본은 검사하지 않는다.
                let skip = path.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n == "target" || n.starts_with('.')
                });
                if !skip {
                    walk_rs_sources(&path, base, visit);
                }
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path.strip_prefix(base).unwrap_or(&path);
            visit(rel.to_string_lossy().replace('\\', "/"), &text);
        }
    }

    /// 레포 소스 트리에서 `set_current_dir(` 호출이 있는 `.rs` 파일의 상대경로를 모은다.
    fn scan_cwd_mutations(root: &Path) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        walk_rs_sources(root, root, &mut |rel, text| {
            if cwd_mutation_call_lines(text) > 0 {
                out.insert(rel);
            }
        });
        out
    }

    fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = <repo>/crates/tasty-cli → 레포 루트는 그 조부모.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("repo root (crates/tasty-cli/../..)")
            .to_path_buf()
    }

    /// cwd 변경 파일의 추가와 삭제를 모두 확인한다. 새 호출은 직렬화한 뒤 등록한다.
    #[test]
    fn set_current_dir_is_confined_to_serialized_tests() {
        let found = scan_cwd_mutations(&repo_root());

        let expected: std::collections::BTreeSet<String> =
            ["crates/tasty-cli/src/cwd_test.rs".to_string()]
                .into_iter()
                .collect();

        assert_eq!(
            found, expected,
            "set_current_dir 를 만지는 소스 집합이 바뀌었다. 새로 생긴 것은 CWD_LOCK \
             같은 직렬화 처방을 적용한 뒤 이 집합에 등재하고, 사라진 것은 지워라 \
             (unit-test-isolation.md §7)."
        );
    }

    #[test]
    fn cwd_guard_detector_counts_calls_not_mentions() {
        // 언급은 세지 않는다 — 주석이든 문자열이든, 괄호가 붙어 있든.
        assert_eq!(cwd_mutation_call_lines("// set_current_dir 를 조심"), 0);
        assert_eq!(
            cwd_mutation_call_lines("let s = \"set_current_dir call\";"),
            0
        );
        assert_eq!(
            cwd_mutation_call_lines("const NEEDLE: &str = \"set_current_dir(\";"),
            0
        );
        assert_eq!(
            cwd_mutation_call_lines("let s = r#\"call set_current_dir(p) here\"#;"),
            0
        );
        assert_eq!(
            cwd_mutation_call_lines("let x = 1; // set_current_dir(p)"),
            0
        );
        assert_eq!(
            cwd_mutation_call_lines("    std::env::set_current_dir(p).unwrap();"),
            1
        );
    }

    #[test]
    fn cwd_guard_scan_root_is_intact() {
        // 스캔 루트가 통째로 깨지면(경로 오류 등) 집합이 비어 위 가드가 거짓 통과할 수
        // 있다 — 최소한 이 파일(실제 호출 보유)을 잡는지로 스캔이 살아있음을 고정한다.
        let found = scan_cwd_mutations(&repo_root());
        assert!(
            found.contains("crates/tasty-cli/src/cwd_test.rs"),
            "스캔이 자기 파일도 못 찾았다 — 스캔 루트가 깨졌다"
        );
    }

    /// 주석·문자열을 가린 원문에서 `#[test]` 함수마다 (이름, 본문)을 뽑는다.
    /// 함수 경계는 줄이 아니라 `fn` 다음 첫 `{` 부터의 중괄호 균형으로 정한다.
    fn test_fns(masked: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (at, _) in masked.match_indices("#[test]") {
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
            let mut end = masked.len();
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

    /// cwd를 읽으면서 `CWD_LOCK` 을 잡지 않는 `#[test]` 함수 이름을 모은다.
    /// 반환값은 (검사한 시험 수, 위반 이름).
    ///
    /// 같은 함수 본문 안에 호출 문자열이 있는지만 보는 제한된 텍스트 검사다. 한계:
    /// - 놓친다: `current_dir(` 를 직접 부르지 않고 상대 경로를 해석해 고정값과 비교하는
    ///   시험, 도우미 함수 안에서 cwd를 읽는 시험.
    /// - 놓친다: 함수 이름과 여는 괄호 사이(`current_dir /* */ ()`)나 `#[test]` 속성 안
    ///   (`#[test /* */]`)에 공백·주석이 낀 형태.
    /// - 잘못 귀속한다: `fn` 뒤에 공백 대신 줄바꿈이 오면 그 `#[test]` 본문 대신 다음에 나오는
    ///   `fn ` 함수의 본문을 읽는다. 그 시험은 빠지고, 다음 함수가 도우미면 거짓 위반이 나고,
    ///   다음 함수가 시험이면 두 번 센다.
    /// - 놓친다: `#[tokio::test]` 같은 다른 시험 속성은 수집하지 않는다.
    /// - 수집 하한(`MIN_SCANNED_TESTS`)은 전체 수만 보며 개별 함수의 누락·오귀속은 보지 않는다.
    /// - 잠금으로 인정한다: `hold()`·`CwdGuard::enter(` 호출의 실행 여부와 귀속을 보지 않는다.
    ///   이름이 `hold` 인 다른 호출, `stringify!` 같은 매크로 토큰, 중첩 함수 안의 호출도 인정한다.
    /// - 과하게 잡는다: `set_current_dir(`·`stringify!(current_dir())` 처럼 cwd를 읽지 않는
    ///   것도 읽기로 셀 수 있다.
    fn unlocked_cwd_readers(text: &str) -> (usize, Vec<String>) {
        let masked = tasty_doc_guards::source_text::mask_non_code(text);
        let fns = test_fns(&masked);
        let violations = fns
            .iter()
            .filter(|(_, body)| body.contains("current_dir("))
            .filter(|(_, body)| !takes_the_cwd_lock(body))
            .map(|(name, _)| name.clone())
            .collect();
        (fns.len(), violations)
    }

    /// `hold()`(경로 접두 허용, `threshold()` 같은 다른 이름 제외) 또는 `CwdGuard::enter(` 호출이 있는가.
    fn takes_the_cwd_lock(body: &str) -> bool {
        body.contains("CwdGuard::enter(")
            || body.match_indices("hold()").any(|(i, _)| {
                body[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
            })
    }

    /// 2026-11-12 crates/tasty-cli/src 에서 주석·문자열을 가린 `#[test]` 함수는 337개였다. 수집이 크게 줄면 판정하지 않는다.
    const MIN_SCANNED_TESTS: usize = 200;

    /// 같은 lib 시험 바이너리에서 cwd를 읽는 시험은 cwd를 바꾸는 시험과 같은 락을 잡아야 한다.
    /// 해석한 경로와 `current_dir()` 를 따로 읽어 비교하면 그 사이에 cwd가 바뀔 수 있다.
    #[test]
    fn every_test_that_reads_the_cwd_holds_the_cwd_lock() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0usize;
        let mut violations = Vec::new();
        walk_rs_sources(&src, &repo_root(), &mut |rel, text| {
            let (n, bad) = unlocked_cwd_readers(text);
            scanned += n;
            violations.extend(bad.into_iter().map(|name| format!("{rel}::{name}")));
        });
        assert!(
            scanned >= MIN_SCANNED_TESTS,
            "#[test] 함수를 {scanned}개만 찾았다(하한 {MIN_SCANNED_TESTS}). 순회와 수집을 확인한다."
        );
        assert!(
            violations.is_empty(),
            "cwd를 읽지만 CWD_LOCK을 잡지 않는 시험이 있다. 함수 첫 줄에 \
             let _cwd = crate::cwd_test::hold(); 를 넣는다(unit-test-isolation.md §7): {violations:#?}"
        );
    }

    #[test]
    fn cwd_read_detector_judges_whole_test_bodies() {
        let unlocked = "#[test]\nfn x() {\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(unlocked), (1, vec!["x".to_string()]));
        let held = "#[test]\nfn x() {\n    let _cwd = cwd_test::hold();\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(held), (1, vec![]));
        let entered = "#[test]\nfn x() {\n    let _cwd = CwdGuard::enter(p);\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(entered), (1, vec![]));
        let comment =
            "#[test]\nfn x() {\n    // current_dir()\n    let s = \"current_dir()\";\n}\n";
        assert_eq!(unlocked_cwd_readers(comment), (1, vec![]));
        let nested = "#[test]\nfn x() {\n    for a in b {\n        if c { let h = std::env::current_dir(); }\n    }\n}\n";
        assert_eq!(unlocked_cwd_readers(nested), (1, vec!["x".to_string()]));
        // 앞 함수의 잠금이 다음 함수로 넘어가지 않는다.
        let two = "#[test]\nfn a() {\n    let _cwd = cwd_test::hold();\n}\n#[test]\nfn b() {\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(two), (2, vec!["b".to_string()]));
        let lookalike = "#[test]\nfn x() {\n    let t = threshold();\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(lookalike), (1, vec!["x".to_string()]));
        let bare =
            "#[test]\nfn x() {\n    let _cwd = hold();\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(bare), (1, vec![]));
        // #[test] 가 아닌 도우미 함수는 대상이 아니다.
        let helper = "fn helper() {\n    let h = std::env::current_dir();\n}\n";
        assert_eq!(unlocked_cwd_readers(helper), (0, vec![]));
    }

    #[test]
    fn hold_keeps_the_cwd_and_blocks_enter_until_released() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let held = hold();
        let before = std::env::current_dir().expect("cwd");
        let (tx, rx) = std::sync::mpsc::channel();
        let dir = tmp.path().to_path_buf();
        let other = std::thread::spawn(move || {
            let _cwd = CwdGuard::enter(&dir);
            tx.send(()).expect("send");
        });
        // 락을 쥔 동안에는 다른 스레드가 cwd를 바꾸지 못한다.
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(200))
                .is_err()
        );
        assert_eq!(std::env::current_dir().expect("cwd"), before);
        drop(held);
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("enter proceeds once hold is released");
        other.join().expect("join");
        let _again = hold();
        assert_eq!(std::env::current_dir().expect("cwd"), before);
    }
}
