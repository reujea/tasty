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
