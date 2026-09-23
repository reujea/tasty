//! Tasty 의 사용자 데이터 디렉토리 헬퍼.
//!
//! `tasty_home()` 이 기반 경로 (`~/.tasty/`) 를 반환한다. 도메인별 경로
//! (`themes_dir`, `default_db_path`, `config_path` 등) 는 각 도메인 crate 가
//! 이 함수를 호출해 자체 정의한다 — utils 는 *공통 기반* 만 제공.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

/// Tasty 의 사용자 데이터 디렉토리.
///
/// 우선순위:
/// 1. 환경변수 `TASTY_HOME` 이 비어있지 않으면 그 경로를 그대로 루트로 사용
///    (임시 루트 override — 테스트/샌드박스/다중 인스턴스용).
/// 2. fallback: debug 빌드 → `~/.tasty-debug/`, release 빌드 → `~/.tasty/`.
///    루트 자체를 갈라 debug 인스턴스가 release 데이터(state.db / layout.json /
///    memory.db / plugins / 포트파일 등)를 공유·오염하지 않게 격리한다.
///
/// AI 에이전트가 경로를 외우기 쉽게 단일화 (Linux 규약상 `~/.config/tasty/` 가
/// 자연스럽지만, agent 접근성 우선).
pub fn tasty_home() -> Option<PathBuf> {
    // 0) 테스트 override (feature-gated) — 이 **스레드**에 한해 최우선. `TASTY_HOME` env 를
    //    전혀 읽지 않으므로(프로세스 전역이라 병렬 테스트·다른 완주와 경합한다) 완전히
    //    격리된다. 자세한 계약·구멍은 [`push_home_override`] 참고.
    #[cfg(any(test, feature = "test-support"))]
    if let Some(dir) = home_override::current() {
        return Some(dir);
    }
    // 1) TASTY_HOME 환경변수 우선 (임시 루트 override)
    if let Ok(custom) = std::env::var("TASTY_HOME") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    // 2) fallback: debug → ~/.tasty-debug, release → ~/.tasty
    let dirname = if cfg!(debug_assertions) {
        ".tasty-debug"
    } else {
        ".tasty"
    };
    BaseDirs::new().map(|dirs| dirs.home_dir().join(dirname))
}

/// 테스트가 `tasty_home()` 을 임시 루트로 고정하는 스레드 로컬 override 스택.
///
/// 이 스레드에서 [`push_home_override`] 로 세운 값이 있으면 `tasty_home()` 이 `TASTY_HOME`
/// env 를 **읽기도 전에** 그것을 돌려준다. env 를 만지지 않으므로 병렬 테스트·다른 git
/// worktree 완주와 경합하지 않는다(그게 이 훅의 존재 이유다 — 처방 등급 ⓒ 계열, 근거는
/// `docs/adr/0155-global-state-race-prescription-by-parameterization.md`).
///
/// **구멍: 자식 스레드에는 상속되지 않는다.** 프로덕션이 `std::thread::spawn` 등으로 만든
/// 스레드 본문에서 `tasty_home()` 을 읽으면 이 override 를 보지 못하고 실제 홈/env 로
/// 폴백한다. 그 구멍이 host-plugin 스위트에서 실제로 걸리는 자리가 0 이라는 측정 위에서
/// 이 처방을 골랐고, 그 전제는 host-plugin 의
/// `tests::spawned_thread_bodies_do_not_read_tasty_home` 소스 스캔 가드가 지킨다(전제가
/// 깨지면 ADR-0155 를 다시 연다).
#[cfg(any(test, feature = "test-support"))]
mod home_override {
    use std::cell::RefCell;
    use std::path::PathBuf;

    thread_local! {
        static STACK: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn current() -> Option<PathBuf> {
        STACK.with(|s| s.borrow().last().cloned())
    }

    pub(super) fn push(dir: PathBuf) {
        STACK.with(|s| s.borrow_mut().push(dir));
    }

    pub(super) fn pop() {
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

/// 이 스레드의 `tasty_home()` 을 `dir` 로 고정한다. [`pop_home_override`] 와 짝을 이루는
/// 스택이라 중첩해도 안전하다 — RAII 가드가 생성 시 push, drop 시 pop 하면 된다.
///
/// **env 를 만지지 않는다.** 그래서 이 override 를 쓰는 테스트는 `set_var` 를 부르는
/// 테스트(홈 해석 자체가 검증 대상이라 env 가 피험자인 소수)와 병렬로 돌아도 읽기-쓰기
/// 경합이 없다. 자식 스레드 비상속 구멍은 [`home_override`] 문서 참고.
#[cfg(any(test, feature = "test-support"))]
pub fn push_home_override(dir: PathBuf) {
    home_override::push(dir);
}

/// [`push_home_override`] 로 세운 이 스레드의 마지막 override 를 되돌린다.
#[cfg(any(test, feature = "test-support"))]
pub fn pop_home_override() {
    home_override::pop();
}

/// OS 사용자 홈 디렉토리 — **tasty 데이터 루트가 아니다.**
///
/// [`tasty_home`] 은 `~/.tasty{-debug}`(또는 `TASTY_HOME` override)를 돌려주므로
/// 홈 자체가 필요한 경로(`~/.ssh/config` 등 tasty 밖의 규약 경로)에는 쓸 수 없다.
/// 이 함수는 override 를 보지 않는다 — 다른 프로그램(ssh 등)이 읽는 경로를 계산하는
/// 용도라, tasty 의 테스트용 루트 이동이 그 프로그램의 홈까지 옮기지는 않기 때문이다.
pub fn os_home_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// 읽거나 해석하지 못한 사용자 파일을 옆자리로 옮겨 보존한다. 성공하면 옮긴 경로.
///
/// 설정·레이아웃처럼 **해석 실패를 기본값으로 폴백하는** 경로에서 쓴다. 그런 경로의
/// 진짜 위험은 폴백 자체가 아니라 **그 뒤의 저장**이다 — 기본값을 원래 자리에 쓰면
/// 사용자가 쓴 원본이 사라진다. 원본을 먼저 옮겨 두면 이후 저장이 새 파일을 만들어도
/// 데이터가 남는다.
///
/// `rename` 이므로 원본 자리는 비고, 다음 로드는 "파일 없음"(정상 기본값 경로)이 된다 —
/// 같은 파일에 대해 백업이 반복 생성되지 않는다. `<name>.bak` 이 이미 있으면 덮어쓰지
/// 않고 `<name>.bak.2`, `.bak.3` … 를 찾는다. 먼저 만들어진 백업이 더 원본에 가깝기
/// 때문이다.
///
/// **실패하면 호출자는 그 자리에 쓰면 안 된다.** 원본을 보호할 다른 수단이 없다.
/// 읽기 자체가 실패한 경우(권한·IO 오류)에는 이 함수를 부르지 않는다 — 내용을 확인하지
/// 못한 파일을 옮기면 일시적 오류에도 사용자 파일이 자리를 뜬다.
///
/// `Ok(None)` 은 **옮길 것이 이미 없었다**는 뜻이다. 부팅 중 같은 파일을 두 곳에서
/// 읽으면(설정은 실제로 그렇다) 둘 다 해석에 실패하고, 한쪽이 먼저 옮긴 뒤 다른 쪽의
/// `rename` 이 `NotFound` 로 실패한다. 이때 원본은 이미 안전하게 백업에 있으므로 저장을
/// 막을 이유가 없다 — 실패로 취급하면 경합에 진 쪽이 애먼 저장 금지를 걸어버린다.
pub fn preserve_corrupt_file(path: &Path) -> std::io::Result<Option<PathBuf>> {
    let Some(candidate) = next_backup_slot(path) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{MAX_BACKUPS} backups already exist for {}", path.display()),
        ));
    };
    match std::fs::rename(path, &candidate) {
        Ok(()) => Ok(Some(candidate)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 한 파일이 쌓아둘 수 있는 백업 개수(`.bak` + `.bak.2` … `.bak.9`).
const MAX_BACKUPS: u32 = 9;

/// [`preserve_corrupt_file`] 이 지금 쓸 백업 이름. 예산이 소진됐으면 `None`.
fn next_backup_slot(path: &Path) -> Option<PathBuf> {
    let mut candidate = with_appended_extension(path, "bak");
    let mut n = 2;
    while candidate.exists() {
        if n > MAX_BACKUPS {
            return None;
        }
        candidate = with_appended_extension(path, &format!("bak.{n}"));
        n += 1;
    }
    Some(candidate)
}

/// [`preserve_corrupt_file`] 이 **지금 부르면 예산 소진으로 실패하는가** — 파일을
/// 건드리지 않고 자리만 센다.
///
/// 보존은 첫 저장 시점에 일어나는데, 그보다 훨씬 이른 **부팅 알림** 시점에 이미
/// "이 파일은 옆으로 옮겨질 것" 이라고 말할지 "옮기지 못해 저장이 막힌다" 고 말할지를
/// 정해야 한다. 그 판정을 예산 계산을 복제하지 않고 여기서 함께 답한다 — 두 곳이
/// 어긋나면 사용자에게 **사실과 반대인 안내**가 나간다(보관됐다고 알리고 실제로는
/// 보관되지 않는다).
///
/// 판정과 실제 보존 사이에 다른 프로세스가 `.bak` 을 지우거나 만들 수 있다 — 이 값은
/// 알림 문구를 고르는 데만 쓰고, 저장 여부는 [`preserve_corrupt_file`] 의 실제 결과가
/// 정한다.
pub fn backup_budget_is_exhausted(path: &Path) -> bool {
    next_backup_slot(path).is_none()
}

/// `config.toml` + `bak` → `config.toml.bak`. `Path::with_extension` 은 기존 확장자를
/// **대체**하므로(`config.bak`) 쓸 수 없다 — 원래 이름이 백업에 그대로 남아야 사용자가
/// 무엇의 백업인지 안다.
fn with_appended_extension(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(ext);
    PathBuf::from(name)
}

/// 홈 아래 경로를 `~/…` 로 줄여 **표시용** 문자열로 만든다. 홈 밖이거나 홈을 못 찾으면
/// 원래 경로 그대로. 파일 접근에 쓰라고 만든 값이 아니다 — 표 한 칸을 홈 접두사가
/// 통째로 잡아먹는 것을 막는 용도다.
pub fn tilde_abbreviate(p: &Path) -> String {
    match os_home_dir().and_then(|home| p.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => p.display().to_string(),
    }
}

/// 자식 프로세스·외부 도구에 넘길 경로에서 Windows verbatim(extended-length,
/// `\\?\`) prefix 를 제거한다.
///
/// `std::fs::canonicalize` 는 Windows 에서 `\\?\C:\...` 형태의 verbatim 경로를
/// 돌려준다. 이 경로가 자식 PTY 의 working_dir 로 전달되면 자식의 `process.cwd()`
/// 가 `\\?\...` 가 되는데, 일부 도구(예: Claude Code/bun 의 `pathToFileURL`)는
/// 이를 file URL 로 변환하지 못해 깨진다. 그래서 외부로 내보내는 경로는 일반
/// 형태로 되돌린다.
///
/// - `\\?\UNC\server\share\..` → `\\server\share\..`
/// - `\\?\C:\..`               → `C:\..`
/// - 이미 일반 경로 / 비-Windows → 입력 그대로 (no-op)
///
/// strip 로직은 `#[cfg(windows)]` 안에만 존재하므로 다른 플랫폼의 동작은 변하지
/// 않는다.
pub fn strip_verbatim_prefix(p: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = p.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    p.to_string()
}

/// 경로의 `.` / `..` 세그먼트를 **순수 lexical** 로(파일시스템 접근 없이) 붕괴시킨다.
///
/// `std::path::absolute` 와의 차이: `absolute` 는 Unix 에서 symlink 안전성 때문에
/// `..` 를 보존한다(`/a/b/../c` 를 `/a/c` 로 줄이면 `b` 가 symlink 일 때 실제 대상과
/// 달라지므로). 이 함수는 **모든 플랫폼에서 동일하게** `..` 를 붕괴시킨다 —
/// markdown 링크의 표시/dedup 용도에는 lexical 붕괴가 사용자 의도에 맞고 dedup 키가
/// 안정적이다.
///
/// 트레이드오프: 붕괴 대상 세그먼트가 symlink 이면 lexical 결과가 OS 의 실제 해석과
/// 달라질 수 있다(`b` 가 symlink 면 `/a/b/../c` ≠ `/a/c`). markdown 링크 용도에서는
/// 이 동작이 의도된 것이며 허용된다. 루트/prefix 위로는 올라가지 않는다.
pub fn lexically_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                // 앞선 일반 세그먼트를 pop 해서 `..` 를 붕괴.
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // 루트/prefix 위로는 올라갈 수 없다 — `..` 무시.
                Some(Component::RootDir | Component::Prefix(_)) => {}
                // 상대경로 선두의 `..` 는 붕괴 대상이 없으므로 보존.
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 경로 문자열의 백슬래시 구분자를 `/` 로 통일한다.
///
/// `file_format` 의 `PathGlob` 패턴처럼 저장·비교를 OS 와 무관하게 항상 `/` 로
/// 해야 하는 문자열에 쓴다 — 입력이 어느 OS 에서 작성됐는지 알 수 없으므로(예:
/// 동기화된 설정 파일) **실행 OS 와 무관하게 항상** 변환한다. 실제 파일시스템
/// 호출 등 OS 경계에서만 [`from_slash`] 로 되돌린다.
pub fn to_slash(p: &str) -> String {
    p.replace('\\', "/")
}

/// [`to_slash`] 의 역변환 — `/` 를 **현재 OS** 네이티브 구분자로 되돌린다.
/// `Path`/`PathBuf` 와 주고받는 지점(OS 경계)에서만 쓴다. non-Windows 는 이미
/// `/` 가 네이티브 구분자라 no-op.
pub fn from_slash(p: &str) -> String {
    if cfg!(windows) {
        p.replace('/', "\\")
    } else {
        p.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn normalizes_parent_and_cur_segments() {
        assert_eq!(
            lexically_normalize(Path::new("/docs/md/../sibling.md")),
            PathBuf::from("/docs/sibling.md")
        );
        assert_eq!(
            lexically_normalize(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
        // 루트 위로는 못 올라간다.
        assert_eq!(lexically_normalize(Path::new("/../x")), PathBuf::from("/x"));
        // 상대경로 선두의 `..` 는 보존.
        assert_eq!(
            lexically_normalize(Path::new("../a/b")),
            PathBuf::from("../a/b")
        );
    }

    #[test]
    #[cfg(windows)]
    fn normalizes_parent_segments_windows() {
        assert_eq!(
            lexically_normalize(Path::new(r"C:\docs\md\..\sibling.md")),
            PathBuf::from(r"C:\docs\sibling.md")
        );
    }

    #[test]
    #[cfg(windows)]
    fn strips_verbatim_disk_prefix() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\E:\workspace\tasty\.worktree\wt-1"),
            r"E:\workspace\tasty\.worktree\wt-1"
        );
    }

    #[test]
    #[cfg(windows)]
    fn strips_verbatim_unc_prefix() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\dir"),
            r"\\server\share\dir"
        );
    }

    #[test]
    #[cfg(windows)]
    fn leaves_normal_path_untouched() {
        assert_eq!(
            strip_verbatim_prefix(r"E:\already\normal"),
            r"E:\already\normal"
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn noop_on_non_windows() {
        // 비-Windows 에서는 어떤 입력이든 그대로 반환한다.
        assert_eq!(strip_verbatim_prefix("/usr/lib/tasty"), "/usr/lib/tasty");
        assert_eq!(strip_verbatim_prefix(r"\\?\X"), r"\\?\X");
    }

    // `tilde_abbreviate` 는 홈을 `os_home_dir()` 로 찾는다. 테스트에서 `HOME` 을
    // 갈아끼우는 대신(edition 2024 의 `set_var` 는 unsafe 이고 병렬 테스트와
    // 레이스가 난다) 실제 홈을 입력 생성에 그대로 쓴다 — 홈 값 자체가 아니라
    // "홈 접두사를 `~` 로 바꾼다" 는 규칙만 검증하면 되기 때문이다.
    #[test]
    fn tilde_abbreviate_replaces_home_prefix() {
        let Some(home) = os_home_dir() else {
            return; // 홈을 못 찾는 환경(컨테이너 등)에서는 검증 대상 자체가 없다.
        };
        let expected = format!("~/{}", Path::new("workspace").join("tasty").display());
        assert_eq!(
            tilde_abbreviate(&home.join("workspace").join("tasty")),
            expected
        );
    }

    #[test]
    fn tilde_abbreviate_maps_home_itself_to_bare_tilde() {
        let Some(home) = os_home_dir() else { return };
        // strip_prefix 결과가 빈 경로라 `~/` 로 끝난다 — 표시용이라 이 형태로 굳힌다.
        assert_eq!(tilde_abbreviate(&home), "~/");
    }

    #[test]
    fn tilde_abbreviate_leaves_paths_outside_home_untouched() {
        let Some(home) = os_home_dir() else { return };
        // 홈의 형제 경로 — 홈 접두사가 아니므로 그대로 나와야 한다. 홈이
        // 루트 직하가 아닌 실제 환경에서 부모가 존재한다.
        let outside = home.parent().map_or_else(
            || PathBuf::from("/definitely-not-home"),
            |parent| parent.join("__tasty_not_home__"),
        );
        assert_eq!(tilde_abbreviate(&outside), outside.display().to_string());
    }

    #[test]
    fn tilde_abbreviate_leaves_relative_paths_untouched() {
        // 상대경로는 어떤 절대 홈으로도 strip 되지 않는다.
        let rel = Path::new("relative").join("file.txt");
        assert_eq!(tilde_abbreviate(&rel), rel.display().to_string());
    }

    #[test]
    fn to_slash_replaces_backslash_regardless_of_os() {
        assert_eq!(to_slash(r"src\file\format.rs"), "src/file/format.rs");
        assert_eq!(to_slash("already/slash"), "already/slash");
        assert_eq!(to_slash(r"mixed/a\b"), "mixed/a/b");
    }

    #[test]
    #[cfg(windows)]
    fn from_slash_restores_backslash_on_windows() {
        assert_eq!(from_slash("src/file/format.rs"), r"src\file\format.rs");
    }

    #[test]
    #[cfg(not(windows))]
    fn from_slash_is_noop_on_non_windows() {
        assert_eq!(from_slash("src/file/format.rs"), "src/file/format.rs");
    }

    /// 백업은 원래 이름을 통째로 남기고(`config.toml.bak`), 두 번째부터 번호가 붙는다 —
    /// 먼저 만들어진 백업이 더 원본에 가까우므로 덮어쓰지 않는다.
    #[test]
    fn backups_do_not_clobber_each_other() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        std::fs::write(&path, "first").unwrap();
        let first = preserve_corrupt_file(&path).unwrap().unwrap();
        assert_eq!(first, tmp.path().join("config.toml.bak"));

        std::fs::write(&path, "second").unwrap();
        let second = preserve_corrupt_file(&path).unwrap().unwrap();
        assert_eq!(second, tmp.path().join("config.toml.bak.2"));

        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
        assert!(!path.exists(), "원본은 자리를 떠야 다음 저장이 안전하다");
    }

    /// 옮길 것이 이미 없으면 실패가 아니다. 부팅 중 같은 파일을 두 곳에서 읽으면 한쪽이
    /// 먼저 옮기고, 다른 쪽은 여기로 온다 — 그때 저장을 막으면 애먼 금지가 걸린다.
    #[test]
    fn already_moved_file_is_not_a_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("config.toml");
        assert_eq!(preserve_corrupt_file(&missing).unwrap(), None);
    }
}
