//! 시험용 홈 경로 격리. docs/dev-guide/unit-test-isolation.md 참고.

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// HOME과 TASTY_HOME을 함께 바꾸는 시험의 공용 잠금.
/// 두 키를 따로 보호하면 다른 시험의 임시 경로를 덮어쓸 수 있다.
/// HomeEnvGuard가 획득·복원을 맡는다. 잠금을 쓰지 않는 외부 환경 접근은 보호하지 못한다.
static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

/// 임시 홈을 지정하고 원상 복구한다.
/// tasty_home은 환경 변수를 바꾸지 않는 스레드 로컬 override를 쓴다.
/// derived_from_home은 HOME에서 경로를 계산하는 규칙 자체를 시험하므로
/// 공용 잠금을 잡고 HOME·TASTY_HOME을 바꾼다.
pub(crate) struct HomeEnvGuard {
    dir: tempfile::TempDir,
    mode: GuardMode,
}

enum GuardMode {
    /// override 격리(env 미접촉). `Drop` 은 이 스레드의 override 만 pop 한다.
    Override,
    /// 실제 env 를 바꾼 갈래. `Drop` 은 두 키를 복원한다. `_lock` 은 마지막 필드라
    /// 복원(및 임시 디렉토리 삭제)이 끝난 뒤에 풀린다.
    Env {
        prev_home: Option<OsString>,
        prev_tasty_home: Option<OsString>,
        _lock: MutexGuard<'static, ()>,
    },
}

impl HomeEnvGuard {
    /// `tasty_home()` 을 임시 루트로 고정한다 — [`Self::path`] 와 같은 경로. env 를 만지지
    /// 않는 스레드 로컬 override 를 쓴다.
    pub(crate) fn tasty_home() -> Self {
        // tempdir도 환경 변수를 읽으므로 이 모듈의 환경 변경과 겹치지 않게 한다.
        let dir = {
            let _lock = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            tempfile::tempdir().expect("tempdir")
        };
        tasty_utils::path::push_home_override(dir.path().to_path_buf());
        Self {
            dir,
            mode: GuardMode::Override,
        }
    }

    /// HOME에서 데이터 루트를 계산하는 규칙을 시험한다.
    /// HOME을 바꾸고 우선순위가 높은 TASTY_HOME을 지워야 해당 규칙이 적용된다.
    pub(crate) fn derived_from_home() -> Self {
        let lock = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev_home = std::env::var_os("HOME");
        let prev_tasty_home = std::env::var_os("TASTY_HOME");
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: HOME_ENV_LOCK으로 이 모듈의 환경 변경을 직렬화한다.
        // 다른 스레드의 환경 조회까지 막지는 못하므로 잠금만으로 모든 동시 접근 안전을 보장하지 않는다.
        unsafe { std::env::set_var("HOME", dir.path()) };
        // SAFETY: 상동.
        unsafe { std::env::remove_var("TASTY_HOME") };
        Self {
            dir,
            mode: GuardMode::Env {
                prev_home,
                prev_tasty_home,
                _lock: lock,
            },
        }
    }

    /// 이 가드가 세운 임시 디렉토리.
    pub(crate) fn path(&self) -> &Path {
        self.dir.path()
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        match &self.mode {
            GuardMode::Override => tasty_utils::path::pop_home_override(),
            GuardMode::Env {
                prev_home,
                prev_tasty_home,
                ..
            } => {
                restore("HOME", prev_home.as_ref());
                restore("TASTY_HOME", prev_tasty_home.as_ref());
            }
        }
    }
}

fn restore(key: &str, prev: Option<&OsString>) {
    match prev {
        // SAFETY: HOME_ENV_LOCK을 보유한 채 복원한다. 이 잠금을 쓰지 않는 환경 접근은 별도 제약이다.
        Some(v) => unsafe { std::env::set_var(key, v) },
        // SAFETY: 상동.
        None => unsafe { std::env::remove_var(key) },
    }
}

/// 이 시험 바이너리의 본체 버전을 1.0.0·거절로 정한다. 같은 값이면 여러 시험이 불러도 된다.
/// 다른 시험의 매니페스트에는 `min_tasty_version`이 없어 결과가 바뀌지 않는다.
pub(crate) fn host_tasty_1_0_0() {
    tasty_plugin_manifest::set_host_tasty_version(
        "1.0.0",
        tasty_plugin_manifest::VersionShortfall::Refuse,
    )
    .expect("every test in this binary sets the same host version");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 두 진입점이 실제로 `tasty_home()` 을 임시 루트로 옮기고, 스코프를 벗어나면
    /// 실행 환경의 원값이 그대로 돌아오는지.
    #[test]
    fn guard_isolates_then_restores_both_keys() {
        let before_home = std::env::var_os("HOME");
        let before_tasty = std::env::var_os("TASTY_HOME");

        {
            let g = HomeEnvGuard::tasty_home();
            assert_eq!(
                tasty_utils::path::tasty_home().as_deref(),
                Some(g.path()),
                "tasty_home() 은 가드가 세운 임시 루트여야 한다"
            );
        }
        assert_eq!(std::env::var_os("HOME"), before_home);
        assert_eq!(std::env::var_os("TASTY_HOME"), before_tasty);

        {
            let g = HomeEnvGuard::derived_from_home();
            // 가드는 HOME을 바꾸고 TASTY_HOME을 비운다.
            assert_eq!(
                std::env::var_os("HOME").as_deref(),
                Some(g.path().as_os_str())
            );
            assert!(std::env::var_os("TASTY_HOME").is_none());

            // Unix는 HOME에서 데이터 루트를 계산하지만 Windows는 OS 사용자 프로필을 조회한다.
            // 이 단언은 Unix의 실제 환경 변수 폴백을 시험하므로 thread-local override로 대체하지 않는다.
            #[cfg(unix)]
            {
                let expected = g.path().join(if cfg!(debug_assertions) {
                    ".tasty-debug"
                } else {
                    ".tasty"
                });
                assert_eq!(
                    tasty_utils::path::tasty_home().as_deref(),
                    Some(expected.as_path()),
                    "TASTY_HOME 을 비웠으므로 HOME 파생 경로여야 한다"
                );
            }
        }
        assert_eq!(std::env::var_os("HOME"), before_home);
        assert_eq!(std::env::var_os("TASTY_HOME"), before_tasty);
    }

    /// 이 크레이트의 HOME·TASTY_HOME 변경이 공용 가드를 통하는지 검사한다.
    /// 파일 수 하한은 잘못된 경로나 읽기 실패로 빈 결과가 통과하는 것을 막는다.
    /// 2026-09-05에는 Rust 파일 37개를 읽었고, 하한은 20개다. 전체 소스 검토를 보장하는 값은 아니다.
    const MIN_SCANNED_FILES: usize = 20;

    /// 파일 수 하한을 통과하는지 판정한다.
    fn scan_is_credible(found: usize) -> bool {
        found >= MIN_SCANNED_FILES
    }

    /// 빈 결과를 믿을 만하다고 판정하지 않아야 한다.
    #[test]
    fn the_scan_refuses_to_report_zero_from_an_empty_walk() {
        assert!(!scan_is_credible(0), "빈 스캔을 믿을 만하다고 판정했다");
        assert!(!scan_is_credible(MIN_SCANNED_FILES - 1));
        assert!(scan_is_credible(MIN_SCANNED_FILES));
    }

    #[test]
    fn home_env_is_only_touched_through_this_module() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        visit(&root, &mut offenders, &mut scanned);
        assert!(
            scan_is_credible(scanned),
            "읽은 Rust 파일이 {scanned}개로 하한 {MIN_SCANNED_FILES}보다 적다. 검사 경로와 읽기 오류를 확인해야 한다"
        );
        assert!(
            offenders.is_empty(),
            "HOME/TASTY_HOME env 변경은 test_support::HomeEnvGuard 를 통해서만 한다 \
             (별도 락 경로가 생기면 테스트끼리 서로의 임시 홈을 지운다). 위반: {offenders:#?}"
        );
    }

    fn visit(dir: &Path, offenders: &mut Vec<String>, scanned: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, offenders, scanned);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs")
                || path.file_name().is_some_and(|n| n == "test_support.rs")
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            *scanned += 1;
            for (i, line) in text.lines().enumerate() {
                let touches_env = line.contains("env::set_var") || line.contains("env::remove_var");
                if touches_env && (line.contains("\"HOME\"") || line.contains("\"TASTY_HOME\"")) {
                    offenders.push(format!("{}:{}", path.display(), i + 1));
                }
            }
        }
    }

    /// thread-local 홈 override가 상속되지 않는 자식 스레드의 직접 홈 조회를 찾는다.
    /// spawn 클로저의 직접 호출만 찾는 근사 검사이며 다른 함수를 통한 간접 호출은 놓친다.
    /// 위반 시 docs/dev-guide/unit-test-isolation.md의 격리 방식을 다시 검토해야 한다.
    #[test]
    fn spawned_thread_bodies_do_not_read_tasty_home() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        visit_spawn(&root, &mut offenders, &mut scanned);
        assert!(
            scan_is_credible(scanned),
            "읽은 Rust 파일이 {scanned}개로 하한 {MIN_SCANNED_FILES}보다 적다. 검사 경로와 읽기 오류를 확인해야 한다"
        );
        assert!(
            offenders.is_empty(),
            "자식 스레드가 tasty_home()을 직접 조회한다. 홈 override가 상속되지 않으므로 \
             docs/dev-guide/unit-test-isolation.md에서 격리 방식을 다시 확인해야 한다. 위반: {offenders:#?}"
        );
    }

    fn visit_spawn(dir: &Path, offenders: &mut Vec<String>, scanned: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit_spawn(&path, offenders, scanned);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs")
                || path.file_name().is_some_and(|n| n == "test_support.rs")
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            *scanned += 1;
            for line in spawn_bodies_reading_home(&text) {
                offenders.push(format!("{}:{line}", path.display()));
            }
        }
    }

    /// spawn 클로저의 tasty_home 직접 호출을 찾아 1부터 시작하는 줄 번호를 반환한다.
    /// 문자열 기반 검사이며 완전한 Rust 파서는 아니다.
    fn spawn_bodies_reading_home(text: &str) -> Vec<usize> {
        let bytes = text.as_bytes();
        let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let mut hits = Vec::new();
        let mut i = 0;
        while let Some(rel) = text[i..].find("spawn") {
            let start = i + rel;
            i = start + "spawn".len();
            // 단어 경계 — respawn/spawner 등 다른 식별자면 건너뛴다.
            if start > 0 && is_ident(bytes[start - 1]) {
                continue;
            }
            // `spawn` 뒤 첫 `{` 가 클로저 본문 시작. 그 전에 `;` 가 오면 클로저 블록이
            // 아니다(예: `let h = spawner();`).
            let Some(brace_rel) = text[i..].find('{') else {
                break;
            };
            let brace = i + brace_rel;
            if text[i..brace].contains(';') {
                continue;
            }
            // 균형 매칭으로 닫는 `}` 를 찾는다.
            let mut depth = 0i32;
            let mut end = None;
            for (off, &b) in bytes[brace..].iter().enumerate() {
                match b {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(brace + off);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                break;
            };
            if text[brace..=end].contains("tasty_home") {
                let line = text[..start].bytes().filter(|&b| b == b'\n').count() + 1;
                hits.push(line);
            }
            i = end + 1;
        }
        hits
    }

    /// 의존 크레이트로 빌드할 때 이 시험 모듈이 포함되지 않도록 cfg(test)를 확인한다.
    /// 다른 홈 잠금과 같은 시험 바이너리에 포함되면 두 잠금이 서로를 보호하지 못한다.
    #[test]
    fn the_module_declaration_stays_test_only() {
        let lib_rs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
        let text = std::fs::read_to_string(&lib_rs)
            .unwrap_or_else(|e| panic!("{} 를 읽지 못했다: {e}", lib_rs.display()));
        let lines: Vec<&str> = text.lines().collect();
        let declaration = lines
            .iter()
            .position(|l| {
                let t = l.trim();
                t == "mod test_support;" || t == "pub mod test_support;"
            })
            .unwrap_or_else(|| {
                panic!(
                    "{} 에 test_support 모듈 선언이 없다 — 이 가드가 볼 대상이 사라졌다",
                    lib_rs.display()
                )
            });
        let gated = declaration > 0 && lines[declaration - 1].trim() == "#[cfg(test)]";
        assert!(
            gated,
            "{}:{}의 test_support 선언에 #[cfg(test)]가 없다. 의존 크레이트에 시험용 홈 잠금이 \
             포함되지 않도록 해야 한다(docs/dev-guide/unit-test-isolation.md).",
            lib_rs.display(),
            declaration + 1
        );
    }
}
