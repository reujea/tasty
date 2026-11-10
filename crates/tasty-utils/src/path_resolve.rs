//! 한 Tasty 프로세스 안의 경로 해석 규칙. 규칙과 계약은 docs/concepts/path-resolution.md에 있다.
//!
//! 상대 경로는 프로세스가 시작될 때 한 번 읽어 고정한 실행 폴더를 기준으로 푼다. 결과는 파일시스템을
//! 보지 않은 어휘적 절대 경로라, 같은 입력은 언제 어디서 불러도 같은 값(또는 같은 오류)이 된다.
//! 존재·종류 검사와 [`canonical`]은 그 뒤의 별도 단계다.

#[cfg(any(windows, test))]
mod windows;

use std::ffi::OsStr;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// 플러그인 프로세스에 실행 폴더를 전달하는 환경변수. 호스트가 플러그인 Command에만 넣는다.
pub const LAUNCH_DIR_ENV: &str = "TASTY_LAUNCH_DIR";

/// 프로세스가 시작될 때 OS가 준 현재 작업 폴더.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchDir {
    Known(PathBuf),
    /// 시작 cwd를 읽지 못했다(지워진 폴더 등). 원인 문구.
    Unavailable(String),
}

/// 시작 때 고정한 기준값. 홈도 같은 시점에 읽어 실행 중 `HOME` 변경에 흔들리지 않는다.
#[derive(Debug, Clone)]
struct Fixed {
    launch: LaunchDir,
    home: Option<PathBuf>,
}

static FIXED: OnceLock<Fixed> = OnceLock::new();

/// 호스트·CLI 프로세스의 진입점 첫 줄에서 부른다. 이미 고정됐으면 아무것도 하지 않는다.
pub fn init_from_process() {
    fixed();
}

/// 플러그인 프로세스: SDK 시작에서 부른다. [`LAUNCH_DIR_ENV`]가 없거나 비었거나 상대 경로면
/// `Unavailable`로 고정한다. 플러그인의 cwd(설치 폴더)로 대신하지 않는다.
pub fn init_from_env() {
    FIXED.get_or_init(|| Fixed {
        launch: launch_dir_from_env(std::env::var_os(LAUNCH_DIR_ENV).as_deref()),
        home: crate::path::os_home_dir(),
    });
}

fn launch_dir_from_env(value: Option<&OsStr>) -> LaunchDir {
    match value {
        None => LaunchDir::Unavailable(format!("host did not provide {LAUNCH_DIR_ENV}")),
        Some(v) if v.is_empty() => LaunchDir::Unavailable(format!("{LAUNCH_DIR_ENV} is empty")),
        Some(v) if !Path::new(v).is_absolute() => {
            LaunchDir::Unavailable(format!("{LAUNCH_DIR_ENV} is not absolute"))
        }
        Some(v) => LaunchDir::Known(PathBuf::from(v)),
    }
}

fn fixed() -> &'static Fixed {
    FIXED.get_or_init(|| Fixed {
        launch: launch_dir_from_cwd(std::env::current_dir()),
        home: crate::path::os_home_dir(),
    })
}

/// OS가 준 cwd를 그대로 보존한다. 문자열로 바꿨다 되돌리면 비 UTF-8 이름이 다른 경로가 된다.
fn launch_dir_from_cwd(cwd: std::io::Result<PathBuf>) -> LaunchDir {
    match cwd {
        Ok(dir) => LaunchDir::Known(without_verbatim_prefix(dir)),
        Err(e) => LaunchDir::Unavailable(e.to_string()),
    }
}

/// Windows verbatim 접두사만 벗긴다. 다른 OS와 UTF-8이 아닌 경로는 받은 값 그대로다.
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str()
        && text.starts_with(r"\\?\")
    {
        return PathBuf::from(crate::path::strip_verbatim_prefix(text));
    }
    path
}

/// 고정한 실행 폴더를 로그 한 줄로 남긴다. 로그를 연 뒤에 부른다.
pub fn log_launch_dir() {
    match launch_dir() {
        LaunchDir::Known(dir) => tracing::info!("launch dir: {}", dir.display()),
        LaunchDir::Unavailable(why) => tracing::warn!(
            "launch dir unavailable: {why}; relative paths will be rejected instead of resolved"
        ),
    }
}

/// 고정한 실행 폴더. 시험 override → 고정값 → (아직 고정하지 않았으면) 지금 고정한 값.
pub fn launch_dir() -> LaunchDir {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(dir) = overrides::launch() {
        return dir;
    }
    fixed().launch.clone()
}

/// 고정한 OS 홈. `~` 해석에만 쓴다.
fn os_home() -> Option<PathBuf> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(home) = overrides::home() {
        return home;
    }
    fixed().home.clone()
}

/// 경로 해석 실패.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolveError {
    Empty,
    /// NUL 등 경로로 쓸 수 없는 문자.
    Invalid,
    /// `scheme://` URL. 로컬 경로가 아니다.
    NotLocalPath,
    /// 상대 입력인데 실행 폴더가 없다.
    LaunchDirUnavailable(String),
    /// `~`인데 OS 홈을 알 수 없다.
    HomeUnavailable,
    /// Windows `\a`·`/a`(MSYS 드라이브 형식 제외).
    RootRelative,
    /// Windows `C:a`.
    DriveRelative,
}

impl fmt::Display for PathResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("path is empty"),
            Self::Invalid => f.write_str("path contains a NUL character"),
            Self::NotLocalPath => f.write_str("a URL is not a local path"),
            Self::LaunchDirUnavailable(why) => write!(
                f,
                "relative path cannot be resolved because the launch directory is unavailable: {why}"
            ),
            Self::HomeUnavailable => f.write_str("home directory is unavailable"),
            Self::RootRelative => {
                f.write_str("a path rooted without a drive depends on the current drive")
            }
            Self::DriveRelative => {
                f.write_str("a drive-relative path depends on that drive's current directory")
            }
        }
    }
}

impl std::error::Error for PathResolveError {}

/// 파일·폴더 경로를 실행 폴더 기준으로 푼다. 구분자가 없는 이름도 실행 폴더 기준이다.
pub fn resolve_path(raw: &str) -> Result<PathBuf, PathResolveError> {
    resolve_in(raw, &launch_dir())
}

/// [`resolve_path`]와 같은 규칙에 기준만 호출자가 준다. `base`가 상대면 먼저 실행 폴더 기준으로 푼다.
pub fn resolve_path_against(raw: &str, base: &Path) -> Result<PathBuf, PathResolveError> {
    let base = base_against_launch_dir(base)?;
    resolve_in(raw, &LaunchDir::Known(base))
}

fn base_against_launch_dir(base: &Path) -> Result<PathBuf, PathResolveError> {
    let text = base.to_str().ok_or(PathResolveError::Invalid)?;
    resolve_path(text)
}

fn resolve_in(raw: &str, base: &LaunchDir) -> Result<PathBuf, PathResolveError> {
    if raw.is_empty() {
        return Err(PathResolveError::Empty);
    }
    if raw.contains('\0') {
        return Err(PathResolveError::Invalid);
    }
    if is_url(raw) {
        return Err(PathResolveError::NotLocalPath);
    }
    if let Some(rest) = tilde_rest(raw) {
        // 상대 홈(HOME=relhome)은 고정 기준이 아니므로 홈을 모르는 것과 같이 다룬다.
        let home = os_home()
            .filter(|home| home.is_absolute())
            .ok_or(PathResolveError::HomeUnavailable)?;
        return Ok(lexical(&if rest.is_empty() {
            home
        } else {
            home.join(rest)
        }));
    }
    #[cfg(windows)]
    {
        // 유니코드로 표현할 수 없는 실행 폴더(짝 없는 서로게이트)를 손실 변환하면 다른 폴더가 된다.
        // 그 폴더를 기준으로 하는 상대 입력은 실행 폴더가 없을 때와 같이 실패한다. 절대 입력은 영향이 없다.
        let base = match base {
            LaunchDir::Known(dir) => dir
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| "launch directory is not valid Unicode".to_owned()),
            LaunchDir::Unavailable(why) => Err(why.clone()),
        };
        windows::resolve(raw, base.as_deref().map_err(String::as_str)).map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        let path = Path::new(raw);
        if path.is_absolute() {
            return Ok(lexical(path));
        }
        match base {
            LaunchDir::Known(dir) => Ok(lexical(&dir.join(path))),
            LaunchDir::Unavailable(why) => Err(PathResolveError::LaunchDirUnavailable(why.clone())),
        }
    }
}

/// `scheme://` 형태인지. 드라이브 문자(`C://x`)는 한 글자라 scheme으로 보지 않는다.
fn is_url(raw: &str) -> bool {
    let Some(at) = raw.find("://") else {
        return false;
    };
    let scheme = &raw[..at];
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// `~`·`~/x`(Windows는 `~\x`도)이면 홈 뒤에 붙일 나머지. `~user`는 펴지 않는다.
/// 나머지 앞의 구분자는 모두 벗긴다. 남기면 `~//bin`의 `/bin`이 홈을 대신하는 루트 경로가 된다.
fn tilde_rest(raw: &str) -> Option<&str> {
    if raw == "~" {
        return Some("");
    }
    let rest = raw.strip_prefix('~')?;
    if !rest.starts_with(is_separator) {
        return None;
    }
    Some(rest.trim_start_matches(is_separator))
}

fn is_separator(c: char) -> bool {
    c == '/' || (cfg!(windows) && c == '\\')
}

/// `.`만 지우고 `..`는 남긴다. 심볼릭 링크 앞의 `..`를 OS와 다르게 접지 않기 위해서다.
fn lexical(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

/// 실행 명령의 해석 대상.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTarget {
    /// 구분자가 있거나 `~`·`~/…`·`.`·`..`: 실행 폴더(또는 홈) 기준 어휘적 절대 경로.
    Path(PathBuf),
    /// 이름만 쓴 값: PATH에서 찾는다.
    Search(String),
}

/// 실행 명령을 어휘로 분류한다. 구분자가 없으면 PATH 탐색 대상, 있으면 실행 폴더 기준 경로다.
pub fn resolve_command(raw: &str) -> Result<CommandTarget, PathResolveError> {
    command_in(raw, &launch_dir())
}

/// [`resolve_command`]와 같은 규칙에 기준만 호출자가 준다.
pub fn resolve_command_against(raw: &str, base: &Path) -> Result<CommandTarget, PathResolveError> {
    let base = base_against_launch_dir(base)?;
    command_in(raw, &LaunchDir::Known(base))
}

fn command_in(raw: &str, base: &LaunchDir) -> Result<CommandTarget, PathResolveError> {
    if raw.is_empty() {
        return Err(PathResolveError::Empty);
    }
    if raw.contains('\0') {
        return Err(PathResolveError::Invalid);
    }
    if is_bare_name(raw) {
        return Ok(CommandTarget::Search(raw.to_owned()));
    }
    resolve_in(raw, base).map(CommandTarget::Path)
}

/// 구분자가 없는 실행 명령 이름인지. 파일시스템을 보지 않는다.
/// `/`는 모든 OS의 구분자이고, Windows는 `\`와 드라이브 접두(`C:`)도 구분자로 본다.
/// `~`·`.`·`..`는 구분자가 없지만 폴더를 뜻하므로 이름이 아니다.
pub fn is_bare_name(raw: &str) -> bool {
    if raw.is_empty() || matches!(raw, "~" | "." | "..") || raw.contains('/') {
        return false;
    }
    if cfg!(windows) {
        let bytes = raw.as_bytes();
        let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
        return !raw.contains('\\') && !drive;
    }
    true
}

/// PATH 탐색 실패.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocateError {
    /// `Search`인데 PATH 어디에도 실행 파일이 없다.
    NotFoundInPath(String),
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFoundInPath(name) => write!(f, "{name} was not found in PATH"),
        }
    }
}

impl std::error::Error for LocateError {}

/// 파일시스템 단계(같은 값 계약 밖). `Path`는 그대로, `Search`는 `path`(자식에게 줄 PATH와 같은 값)에서
/// 첫 실행 파일의 절대 경로를 돌려준다. PATH의 상대·빈 항목은 실행 폴더 기준이다.
/// 판정과 실행은 이 결과 하나를 함께 쓴다.
pub fn locate_command_in(target: &CommandTarget, path: &OsStr) -> Result<PathBuf, LocateError> {
    locate_in(target, path, &launch_dir())
}

/// [`locate_command_in`]과 같은 규칙에 상대 PATH 항목의 기준만 호출자가 준다.
pub fn locate_command_in_against(
    target: &CommandTarget,
    path: &OsStr,
    base: &Path,
) -> Result<PathBuf, LocateError> {
    let base = match base_against_launch_dir(base) {
        Ok(dir) => LaunchDir::Known(dir),
        Err(e) => LaunchDir::Unavailable(e.to_string()),
    };
    locate_in(target, path, &base)
}

fn locate_in(
    target: &CommandTarget,
    path: &OsStr,
    base: &LaunchDir,
) -> Result<PathBuf, LocateError> {
    let name = match target {
        CommandTarget::Path(p) => return Ok(p.clone()),
        CommandTarget::Search(name) => name,
    };
    let candidates = candidate_names(name);
    for entry in std::env::split_paths(path) {
        let Some(dir) = path_entry_dir(entry, base) else {
            continue;
        };
        for candidate in &candidates {
            let file = lexical(&dir.join(candidate));
            if file.is_absolute() && is_executable_file(&file) {
                return Ok(file);
            }
        }
    }
    Err(LocateError::NotFoundInPath(name.clone()))
}

/// PATH 항목을 찾을 폴더로 바꾼다. 빈 항목은 POSIX에서 "현재 폴더"이므로 상대 항목과 함께 실행
/// 폴더 기준으로 고정한다. Windows의 드라이브 상대(`C:tools`)·루트 상대(`\tools`) 항목은 고정할 수
/// 없는 드라이브 상태에 기대므로 건너뛴다(명령 입력의 거부 규칙과 같다).
fn path_entry_dir(entry: PathBuf, base: &LaunchDir) -> Option<PathBuf> {
    if entry.is_absolute() {
        return Some(entry);
    }
    #[cfg(windows)]
    if entry
        .to_str()
        .is_none_or(windows::path_entry_depends_on_drive_state)
    {
        return None;
    }
    match base {
        LaunchDir::Known(dir) => Some(dir.join(entry)),
        LaunchDir::Unavailable(_) => None,
    }
}

/// 한 PATH 폴더에서 볼 파일 이름. Windows는 portable-pty 0.9의 `search_path`와 같이 원래 이름을
/// 먼저 보고 `PATHEXT` 순서(없으면 `.EXE`)로 확장자를 바꿔 본다. 자식은 호스트 `PATHEXT`를 받는다.
fn candidate_names(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let pathext = std::env::var("PATHEXT").ok();
        windows::candidate_names(name, pathext.as_deref())
    }
    #[cfg(not(windows))]
    {
        vec![name.to_owned()]
    }
}

/// 디렉터리가 아닌 파일이고 이 프로세스가 실행할 수 있는지. Unix는 `access(X_OK)`다.
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// 같은 값 계약 밖의 별도 단계. 심볼릭 링크를 푼 실제 경로(보안 경계·동일 파일 판정 전용).
/// Windows verbatim 접두사를 벗긴다. 없는 경로는 오류다.
pub fn canonical(resolved: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(resolved).map(without_verbatim_prefix)
}

/// 시험용 스레드 로컬 override. 자식 스레드에는 상속되지 않는다.
#[cfg(any(test, feature = "test-support"))]
mod overrides {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::LaunchDir;

    thread_local! {
        static LAUNCH: RefCell<Vec<LaunchDir>> = const { RefCell::new(Vec::new()) };
        static HOME: RefCell<Vec<Option<PathBuf>>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn launch() -> Option<LaunchDir> {
        LAUNCH.with(|s| s.borrow().last().cloned())
    }

    pub(super) fn home() -> Option<Option<PathBuf>> {
        HOME.with(|s| s.borrow().last().cloned())
    }

    pub(super) fn push_launch(dir: LaunchDir) {
        LAUNCH.with(|s| s.borrow_mut().push(dir));
    }

    pub(super) fn pop_launch() {
        LAUNCH.with(|s| {
            s.borrow_mut().pop();
        });
    }

    pub(super) fn push_home(dir: Option<PathBuf>) {
        HOME.with(|s| s.borrow_mut().push(dir));
    }

    pub(super) fn pop_home() {
        HOME.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

/// 이 스레드의 실행 폴더를 바꾼다. [`pop_launch_dir_override`]와 역순으로 짝짓는다.
#[cfg(any(test, feature = "test-support"))]
pub fn push_launch_dir_override(dir: LaunchDir) {
    overrides::push_launch(dir);
}

/// [`push_launch_dir_override`]로 세운 마지막 override를 되돌린다.
#[cfg(any(test, feature = "test-support"))]
pub fn pop_launch_dir_override() {
    overrides::pop_launch();
}

/// 이 스레드의 OS 홈을 바꾼다. `None`은 홈을 알 수 없는 상태다.
#[cfg(any(test, feature = "test-support"))]
pub fn push_os_home_override(dir: Option<PathBuf>) {
    overrides::push_home(dir);
}

/// [`push_os_home_override`]로 세운 마지막 override를 되돌린다.
#[cfg(any(test, feature = "test-support"))]
pub fn pop_os_home_override() {
    overrides::pop_home();
}

#[cfg(test)]
mod tests;
