//! child 완료 알림을 caller별 로그에 한 줄씩 덧붙인다. reader는 tail -F 등으로 읽는다.
//! 경로는 TASTY_PARENT_HOME을 우선하고 없으면 tasty_home 아래 notify/<surface>.log를 쓴다.
//! writer와 reader에 같은 부모 경로를 전달해야 한다. 부모 정보를 TASTY_HOME과 분리해
//! 자식의 debug/release 데이터 경로 선택을 덮어쓰지 않는다.
//!
//! 로그에는 호스트 세대 ID가 없다. 재시작 시 surface 번호가 다시 쓰이므로 호스트는
//! 부팅 때 notify 디렉터리를 정리한다. 같은 데이터 루트를 두 호스트가 공유하면 서로의
//! 로그를 지울 수 있다. 포트 파일을 데이터 루트 밖으로 옮긴 경우의 정리 예외는
//! 호스트 TcpIpcServer::notify_dir_to_clear가 결정한다.
//!
//! 쓰기 전 파일 크기가 NOTIFY_LOG_CAP_BYTES 이상이면 전체를 비운다. 쓰는 한 줄과
//! 동시 writer 때문에 크기가 이 값을 넘을 수 있다. 시간·파일 수 제한은 없으며 닫힌
//! surface의 파일도 다음 부팅 정리까지 남는다.
//!
//! 옆의 .log.meta는 비운 바이트 누계 retention_start를 저장한다. reader가 보관한
//! 논리 위치보다 누계가 크면 그 차이는 읽지 못한 손실이다. 메타가 없거나 값을 읽지
//! 못하면 0으로 해석한다. 세대 식별이나 모든 손실 검출을 보장하지는 않는다.
//!
//! 정상 경로는 메타 파일의 공유 잠금 아래 append하고 배타 잠금 아래 비우기·누계 갱신을
//! 수행한다. 배타 잠금을 기다리는 동안 다른 writer가 먼저 비워 크기가 기준 아래로 내려가면
//! 기다림을 그만둔다. 배타 잠금을 제한 시간 안에 얻지 못하면 잠금 없이 비우고 누계는 올리지 않는다.
//! 이때 append와 겹칠 수 있고, reader가 모든 비우기를 알아내지는 못한다. 잠금·메타 갱신
//! 실패는 로그에 남긴다. 공유 잠금 획득과 파일 I/O에는 별도 시간 제한이 없다.
//!
//! 데이터 파일은 완료 알림만 담고 보존 메타를 섞지 않는다. reader 절차와 한계는
//! docs/dev-guide/external-interaction.md#재개하는-reader--caller_surfacelogmeta 참조.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// 쓰기 전 전체 비우기를 시도할 파일 크기. append 이후 크기의 절대 상한은 아니다.
const NOTIFY_LOG_CAP_BYTES: u64 = 256 * 1024;

/// caller별 완료 로그 경로. 비어 있지 않은 TASTY_PARENT_HOME을 우선하고
/// 없으면 tasty_home을 사용한다. 둘 다 해석하지 못하면 None이다.
pub fn notify_log_path(caller_surface: u32) -> Option<PathBuf> {
    resolve_home(
        std::env::var("TASTY_PARENT_HOME").ok(),
        crate::path::tasty_home,
    )
    .map(|home| notify_log_path_in(&home, caller_surface))
}

/// 홈 루트 선택 로직(순수 — env 접근 없이 테스트 가능). `TASTY_PARENT_HOME` 값이
/// 비어있지 않으면 그것을, 아니면 `fallback`(보통 `tasty_home()`)을 쓴다. `fallback` 은
/// parent 가 없을 때만 호출하는 클로저라 불필요한 홈 해석을 피한다.
fn resolve_home(
    parent_env: Option<String>,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(raw) = parent_env {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    fallback()
}

/// 순수 경로 조립(파일시스템 접근 없음) — 단위 테스트 대상.
fn notify_log_path_in(home: &Path, caller_surface: u32) -> PathBuf {
    home.join("notify").join(format!("{caller_surface}.log"))
}

/// 완료 로그 옆의 메타 파일 경로 — `<log>.meta`. 로그 경로에서 순수하게 만든다.
fn notify_meta_path(log: &Path) -> PathBuf {
    let mut raw = log.as_os_str().to_owned();
    raw.push(".meta");
    PathBuf::from(raw)
}

/// 메타 파일에서 버린 바이트 누계를 담는 키(모듈 문서 "독자 복구").
const RETENTION_START_KEY: &str = "retention_start";

/// 누계 값의 고정 폭(u64 최대 20자리). 공백 채움으로 셸의 8진수 해석을 피하고
/// 정상 갱신에서 파일을 줄이지 않는다. 잠금 없는 reader의 일관된 읽기까지 보장하지는 않는다.
const RETENTION_START_WIDTH: usize = 20;

/// 메타 파일 한 벌의 내용. 파싱은 관대하다 — 모르는 키는 건너뛰고, 값의 앞뒤 공백을
/// 벗기며, 못 읽으면 0(아직 안 비웠다)으로 본다.
fn parse_retention_start(meta: &str) -> u64 {
    meta.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == RETENTION_START_KEY)
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0)
}

fn format_retention_start(value: u64) -> String {
    format!("{RETENTION_START_KEY}={value:<RETENTION_START_WIDTH$}\n")
}

/// child 알림 한 줄의 타입. 줄 맨 앞의 번역하지 않는 ASCII 표지로 남아 reader 가 언어와 무관하게 거를 수 있다.
/// 상태 변경·결과 저장은 `Info`, 출력이 없다는 알림은 `Warning`, 오류가 섞이면 무조건 `Error` 다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeType {
    Info,
    Warning,
    Error,
}

impl NoticeType {
    /// 줄 맨 앞에 붙는 표지. 뒤에 공백 하나가 이어진다.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Info => "[info]",
            Self::Warning => "[warning]",
            Self::Error => "[error]",
        }
    }
}

/// 타입 표지를 붙인 로그 줄. 쓰기와 시험이 같은 형식을 쓰도록 한 곳에서 만든다.
pub fn notice_line(ty: NoticeType, line: &str) -> String {
    format!("{} {line}", ty.tag())
}

/// 완료 로그에 타입 표지와 개행을 붙여 쓴다. 필요한 디렉터리를 만들며 I/O 실패는 호출자에게 반환한다.
/// 타입 없이 쓰는 함수는 두지 않는다. 모든 writer 가 줄마다 타입을 정해야 컴파일된다.
pub fn append_notice(caller_surface: u32, ty: NoticeType, line: &str) -> io::Result<()> {
    let path = notify_log_path(caller_surface)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tasty_home() unavailable"))?;
    append_line_to(&path, &notice_line(ty, line), NOTIFY_LOG_CAP_BYTES)
}

/// 경로와 크기 기준을 받아 완료 알림을 기록한다.
/// 여러 writer가 같은 로그를 쓰므로 항상 append 모드로 열고 한 줄을 먼저 조립한다.
/// write_all은 부분 쓰기를 재시도할 수 있어 한 번의 OS write나 줄 단위 원자성을
/// 보장하지는 않는다. 크기 확인·비우기는 별도 핸들에서 수행하며 잠금 실패 시의
/// 정확한 손실 집계도 보장하지 않는다.
fn append_line_to(path: &Path, line: &str, cap: u64) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let meta_path = notify_meta_path(path);
    let meta = open_meta(&meta_path);
    // 기존 크기가 기준 이상이면 비우기를 시도한 뒤 새 줄을 쓴다.
    if std::fs::metadata(path)
        .map(|m| m.len() >= cap)
        .unwrap_or(false)
    {
        truncate_and_account(path, cap, meta.as_ref(), &meta_path);
    }
    // 공유 잠금을 얻으면 정상 배타 비우기와 겹치지 않는다. 잠금 실패 때는 그대로 쓴다.
    let _shared = meta.as_ref().and_then(|m| MetaLock::shared(m, &meta_path));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut record = String::with_capacity(line.len() + 1);
    record.push_str(line);
    record.push('\n');
    file.write_all(record.as_bytes())
}

/// 메타 파일을 연다. 실패해도 완료 알림 쓰기는 진행하지만 잠금과 누계 갱신은 생략한다.
/// 이때 reader가 모든 손실을 알아낼 수는 없다.
fn open_meta(meta_path: &Path) -> Option<std::fs::File> {
    match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(meta_path)
    {
        Ok(f) => Some(f),
        Err(e) => {
            tracing::warn!(
                "notify meta {} open failed: {e} — appending without the lock, a truncation now \
                 would not be counted",
                meta_path.display()
            );
            None
        }
    }
}

/// 배타 잠금을 재시도할 시간 예산. 완료 통지 지연과 정상적인 짧은 쓰기 경합을 고려한 값이다.
/// 선택 근거: docs/dev-guide/external-interaction.md#재개하는-reader--caller_surfacelogmeta.
const EXCLUSIVE_LOCK_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);

/// 배타 잠금 재시도 간격. 실제 시도 횟수와 경과 시간은 스케줄링에 따라 달라진다.
const EXCLUSIVE_LOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(5);

/// 메타 파일의 공유·배타 잠금. 공유 획득은 기다리고 배타 획득은 제한 시간 동안 재시도한다.
/// Drop에서 잠금을 해제한다.
struct MetaLock<'a>(&'a std::fs::File);

impl<'a> MetaLock<'a> {
    fn shared(file: &'a std::fs::File, meta_path: &Path) -> Option<Self> {
        match file.lock_shared() {
            Ok(()) => Some(Self(file)),
            Err(e) => {
                tracing::warn!(
                    "notify meta {} shared lock failed: {e}",
                    meta_path.display()
                );
                None
            }
        }
    }

    /// try_lock을 예산 동안 재시도한다. 기다리는 동안 `still_needed`가 거짓이 되면 그만둔다.
    /// 예산 안에 얻지 못하면 호출자는 잠금 없이 비울 수 있으나 그 손실은 누계에 더하지 않는다.
    ///
    /// 여러 writer가 같은 크기 초과를 보고 함께 기다리면 먼저 얻은 writer가 비우고 나머지는 이미
    /// 해소된 요청으로 계속 기다린다. 그동안 다른 writer의 append가 공유 잠금을 번갈아 쥐어 예산을
    /// 넘기면, 다시 자란 파일을 잠금 없이 비워 집계되지 않는 손실을 만든다. 그래서 재시도마다
    /// 요청이 아직 유효한지 확인한다.
    fn exclusive(
        file: &'a std::fs::File,
        meta_path: &Path,
        still_needed: impl Fn() -> bool,
    ) -> Exclusive<'a> {
        let deadline = std::time::Instant::now() + EXCLUSIVE_LOCK_BUDGET;
        loop {
            match file.try_lock() {
                Ok(()) => return Exclusive::Locked(Self(file)),
                Err(std::fs::TryLockError::WouldBlock) => {
                    if !still_needed() {
                        return Exclusive::NotNeeded;
                    }
                    if std::time::Instant::now() >= deadline {
                        tracing::warn!(
                            "notify meta {} is still locked by another holder after {:?} — \
                             truncating without the lock",
                            meta_path.display(),
                            EXCLUSIVE_LOCK_BUDGET
                        );
                        return Exclusive::Unavailable;
                    }
                    std::thread::sleep(EXCLUSIVE_LOCK_RETRY);
                }
                Err(std::fs::TryLockError::Error(e)) => {
                    tracing::warn!(
                        "notify meta {} exclusive lock failed: {e}",
                        meta_path.display()
                    );
                    return Exclusive::Unavailable;
                }
            }
        }
    }
}

/// 배타 잠금 시도의 결과.
enum Exclusive<'a> {
    Locked(MetaLock<'a>),
    /// 기다리는 동안 다른 writer가 이미 비워 요청이 해소됐다.
    NotNeeded,
    /// 잠금을 얻지 못했다. 잠금 없이 비울 수 있으나 누계는 올리지 않는다.
    Unavailable,
}

impl Drop for MetaLock<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.0.unlock() {
            tracing::warn!("notify meta unlock failed: {e}");
        }
    }
}

/// 파일을 비우고 배타 잠금을 얻은 경우에만 손실 누계를 갱신한다. 기다리는 동안 다른 writer가
/// 이미 비워 크기가 기준 아래로 내려가면 비우지 않는다. 잠금을 얻지 못했거나 갱신에 실패하면 실제 손실과 누계가 달라질 수 있다.
fn truncate_and_account(path: &Path, cap: u64, meta: Option<&std::fs::File>, meta_path: &Path) {
    let over_cap = || {
        std::fs::metadata(path)
            .map(|m| m.len() >= cap)
            .unwrap_or(false)
    };
    let exclusive = match meta.map(|m| MetaLock::exclusive(m, meta_path, over_cap)) {
        Some(Exclusive::Locked(lock)) => Some(lock),
        Some(Exclusive::NotNeeded) => return,
        Some(Exclusive::Unavailable) | None => None,
    };
    let Some(discarded) = truncate_over_cap(path, cap) else {
        return;
    };
    let retention_start = match (&exclusive, meta) {
        (Some(_), Some(m)) => advance_retention_start(m, discarded, meta_path),
        _ => None,
    };
    // 완료 로그에 메타 줄을 섞지 않고 진단 로그에 비운 양을 남긴다.
    match retention_start {
        Some(start) => tracing::warn!(
            "notify log {} hit the {cap}-byte cap — discarded {discarded} bytes of completion lines, \
             including anything a lagging reader had not read yet (retention_start is now {start})",
            path.display()
        ),
        None => tracing::warn!(
            "notify log {} hit the {cap}-byte cap — discarded {discarded} bytes of completion lines, \
             including anything a lagging reader had not read yet (retention_start NOT advanced — \
             this truncation may be missed by a resuming reader)",
            path.display()
        ),
    }
}

/// 누계를 읽어 `discarded` 를 더하고 제자리에 다시 쓴다. 새 누계를 돌려준다. 호출자가 배타
/// 잠금을 쥐고 있어야 한다.
fn advance_retention_start(meta: &std::fs::File, discarded: u64, meta_path: &Path) -> Option<u64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = meta;
    let mut current = String::new();
    let result = handle
        .seek(SeekFrom::Start(0))
        .and_then(|_| handle.read_to_string(&mut current))
        .and_then(|_| {
            let next = parse_retention_start(&current).saturating_add(discarded);
            // 내용이 이 모듈이 쓴 한 줄의 폭이 아니면(빈 파일 포함) 먼저 비운다. 폭이 같으면
            // 줄이는 단계 없이 덮어쓴다(`RETENTION_START_WIDTH` 의 이유).
            if current.len() != format_retention_start(0).len() {
                handle.set_len(0)?;
            }
            handle.seek(SeekFrom::Start(0))?;
            handle.write_all(format_retention_start(next).as_bytes())?;
            Ok(next)
        });
    match result {
        Ok(next) => Some(next),
        Err(e) => {
            tracing::warn!("notify meta {} update failed: {e}", meta_path.display());
            None
        }
    }
}

/// 별도 핸들로 크기를 다시 확인하고 기준 이상이면 비운다.
/// 성공하면 비우기 전에 관측한 크기를 반환한다. 잠금 없이 호출하면 그 사이
/// 다른 writer가 쓸 수 있어 정확한 손실량은 아니다. 열기·비우기 실패나 기준 미달은 None이다.
fn truncate_over_cap(path: &Path, cap: u64) -> Option<u64> {
    let opened = std::fs::OpenOptions::new().write(true).open(path);
    // 이후 append가 I/O 오류를 반환할 수 있으므로 여기서는 중복 보고하지 않는다.
    let file = opened.ok()?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < cap {
        return None;
    }
    if let Err(e) = file.set_len(0) {
        // 비우기 실패를 기록하되 append는 계속한다. 파일이 기준보다 커질 수 있다.
        tracing::warn!("notify log truncate failed for {}: {e}", path.display());
        return None;
    }
    Some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 환경변수를 바꾸지 않고 실제 경로 선택 함수에 값을 전달해 검사한다.

    // TASTY_PARENT_HOME 이 설정돼 있으면 그 값을 쓴다(fallback 은 호출조차 안 함).
    #[test]
    fn resolve_home_prefers_parent_env() {
        let got = resolve_home(Some("/tmp/fake-parent-home".to_string()), || {
            panic!("fallback must not run when parent env is set")
        });
        assert_eq!(got, Some(PathBuf::from("/tmp/fake-parent-home")));
    }

    // 이를 notify_log_path 경로 조립까지 연결하면 최종 경로가 parent home 밑에 놓인다.
    #[test]
    fn parent_env_drives_full_notify_path() {
        let home = resolve_home(Some("/tmp/fake-parent-home".to_string()), || None).unwrap();
        assert_eq!(
            notify_log_path_in(&home, 9),
            PathBuf::from("/tmp/fake-parent-home/notify/9.log")
        );
    }

    // TASTY_PARENT_HOME 이 없으면 fallback(= tasty_home())을 쓴다.
    #[test]
    fn resolve_home_falls_back_when_parent_absent() {
        let got = resolve_home(None, || Some(PathBuf::from("/tmp/fallback-root")));
        assert_eq!(got, Some(PathBuf::from("/tmp/fallback-root")));
    }

    // 빈/공백 값도 미설정으로 간주하고 fallback 한다.
    #[test]
    fn resolve_home_treats_empty_parent_as_absent() {
        let got = resolve_home(Some("   ".to_string()), || {
            Some(PathBuf::from("/tmp/fallback-root2"))
        });
        assert_eq!(got, Some(PathBuf::from("/tmp/fallback-root2")));
    }

    #[test]
    fn path_in_builds_notify_subdir_and_log_suffix() {
        let home = Path::new("/home/u/.tasty");
        assert_eq!(
            notify_log_path_in(home, 42),
            PathBuf::from("/home/u/.tasty/notify/42.log")
        );
    }

    /// 세 타입은 서로 다른 고정 표지로 줄 맨 앞에 붙는다.
    #[test]
    fn every_notice_type_starts_the_line_with_its_own_tag() {
        assert_eq!(
            notice_line(NoticeType::Info, "surface 3: x"),
            "[info] surface 3: x"
        );
        assert_eq!(
            notice_line(NoticeType::Warning, "surface 3: x"),
            "[warning] surface 3: x"
        );
        assert_eq!(
            notice_line(NoticeType::Error, "surface 3: x"),
            "[error] surface 3: x"
        );
    }

    #[test]
    fn append_creates_dir_and_writes_line_with_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("notify").join("7.log");
        append_line_to(&path, "surface 7 작업 완료 (호출 방식: spawn)", 1024).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "surface 7 작업 완료 (호출 방식: spawn)\n");
    }

    #[test]
    fn append_accumulates_multiple_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.log");
        append_line_to(&path, "first", 1024).unwrap();
        append_line_to(&path, "second", 1024).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first\nsecond\n");
    }

    // 여러 스레드의 서로 다른 길이의 줄이 섞이지 않는지 검사한다.
    // 별도 프로세스나 OS의 부분 쓰기를 재현하는 시험은 아니다.
    #[test]
    fn concurrent_writers_never_leave_a_partial_line() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("race.log");
        // truncate가 쓰기 오류의 흔적을 지우지 않도록 cap을 충분히 크게 둔다.
        // 이 시험은 truncate와 append의 경합을 검사하지 않는다.
        const CAP: u64 = 1 << 30;
        const ROUNDS: usize = 2_000;
        // 스레드를 늘려 동시 append 경합을 만든다.
        const WRITERS_PER_SHAPE: usize = 4;
        let long = "X".repeat(60);
        std::thread::scope(|scope| {
            for _ in 0..WRITERS_PER_SHAPE {
                scope.spawn(|| {
                    for i in 0..ROUNDS {
                        append_line_to(&path, &format!("A-{i:06}"), CAP)
                            .expect("append_line_to failed");
                    }
                });
                scope.spawn(|| {
                    for i in 0..ROUNDS {
                        append_line_to(&path, &format!("B-{i:06}-{long}"), CAP)
                            .expect("append_line_to failed");
                    }
                });
            }
        });
        let text = std::fs::read_to_string(&path).unwrap();
        let malformed: Vec<&str> = text
            .lines()
            .filter(|l| {
                let a = l.len() == 8 && l.starts_with("A-");
                let b = l.len() == 69 && l.starts_with("B-");
                !(a || b)
            })
            .collect();
        assert!(
            malformed.is_empty(),
            "둘 중 누구의 줄도 아닌 잔해가 {} 줄 남았다 — 덮어썼거나 한 줄이 여러 번의 \
             write 로 쪼개져 끼어들었다는 뜻이다. 처음 셋: {:?}",
            malformed.len(),
            &malformed[..malformed.len().min(3)]
        );
    }

    // 비우기 전에 관측한 크기가 반환되는지 확인한다.
    #[test]
    fn truncate_reports_how_many_bytes_it_threw_away() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("over.log");
        std::fs::write(&path, vec![b'x'; 300]).unwrap();
        assert_eq!(truncate_over_cap(&path, 256), Some(300));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }

    // 다시 확인한 크기가 기준 미만이면 비우지 않았다는 None을 반환한다.
    #[test]
    fn truncate_reports_nothing_when_another_writer_already_emptied_it() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("raced.log");
        std::fs::write(&path, b"short\n").unwrap();
        assert_eq!(truncate_over_cap(&path, 256), None);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 6);
    }

    // 파일을 열지 못하면 버린 양을 반환하지 않는다.
    #[test]
    fn truncate_reports_nothing_when_the_file_cannot_be_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("absent.log");
        assert_eq!(truncate_over_cap(&path, 1), None);
    }

    // 기준에 도달하면 초과분만 자르지 않고 전체 파일을 비운다.
    #[test]
    fn hitting_the_cap_discards_the_whole_file_not_just_the_excess() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("whole.log");
        for i in 0..20 {
            append_line_to(&path, &format!("unread-{i:02}"), 64).unwrap();
        }
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.lines().count() < 20,
            "cap 을 넘겼는데도 20 줄이 다 남았다 — 비우기가 안 돌았다"
        );
        assert!(
            !content.contains("unread-00"),
            "첫 줄이 남아 있다 — 비우기가 초과분만 잘라냈다는 뜻이고, 그러면 보존 \
             범위를 '마지막 비우기 이후' 로 말할 수 없다: {content:?}"
        );
    }

    // 가이드의 재개 절차를 시험용 reader로 구현해 writer와 대조한다.
    // 제품 reader는 이 크레이트에 포함되지 않는다.

    /// 한 번의 재개 결과. 어휘는 `events.fetch` 의 것이다.
    #[derive(Debug)]
    struct Resume {
        lines: Vec<String>,
        next_offset: u64,
        truncated: bool,
        /// None은 논리 위치가 현재 끝보다 커서 집계되지 않은 비우기를 감지한 경우다.
        skipped: Option<u64>,
    }

    fn resume(log: &Path, next_offset: u64) -> Resume {
        let meta_path = notify_meta_path(log);
        // 공유 잠금 아래에서 누계와 로그를 함께 읽는다 — 비우기가 그 사이에 끼지 않는다.
        let meta = std::fs::OpenOptions::new().read(true).open(&meta_path).ok();
        let _guard = meta.as_ref().and_then(|m| MetaLock::shared(m, &meta_path));
        let base = std::fs::read_to_string(&meta_path)
            .map(|t| parse_retention_start(&t))
            .unwrap_or(0);
        let bytes = std::fs::read(log).unwrap_or_default();
        let len = bytes.len() as u64;
        let (physical, truncated, skipped) = if next_offset < base {
            (0, true, Some(base - next_offset))
        } else if next_offset - base > len {
            (0, true, None)
        } else {
            (next_offset - base, false, Some(0))
        };
        let tail = &bytes[physical as usize..];
        // 완결된 줄만 소비한다 — 마지막 개행 뒤의 조각은 다음 재개로 넘긴다.
        let consumed = tail.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        let lines = String::from_utf8_lossy(&tail[..consumed])
            .lines()
            .map(str::to_owned)
            .collect();
        Resume {
            lines,
            next_offset: base + physical + consumed as u64,
            truncated,
            skipped,
        }
    }

    // 한 번도 안 비웠으면 누계는 0 이고, 논리 오프셋은 파일 안 위치와 같다.
    #[test]
    fn before_any_truncation_the_offset_is_the_file_position() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("notify").join("3.log");
        append_line_to(&log, "one", 1024).unwrap();
        append_line_to(&log, "two", 1024).unwrap();
        let first = resume(&log, 0);
        assert_eq!(first.lines, ["one", "two"]);
        assert_eq!(first.next_offset, 8);
        assert!(!first.truncated);
        assert_eq!(
            parse_retention_start(&std::fs::read_to_string(notify_meta_path(&log)).unwrap()),
            0
        );
        let again = resume(&log, first.next_offset);
        assert!(again.lines.is_empty());
        assert_eq!(again.next_offset, 8);
    }

    // 비우기마다 버린 양이 누계에 **더해진다** — 덮어쓰는 것이 아니다. 두 번 비운 뒤의
    // 값이 두 비우기의 합이어야 reader 가 두 번 사이에 멈췄어도 맞게 센다.
    #[test]
    fn retention_start_accumulates_what_every_truncation_threw_away() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("acc.log");
        // cap 8: "aaaaaa\n"(7) 다음 "bbbbbb\n" 로 14 → 세 번째 append 전에 14 를 버린다.
        for l in ["aaaaaa", "bbbbbb", "cccccc", "dddddd", "eeeeee"] {
            append_line_to(&log, l, 8).unwrap();
        }
        // 버린 것: [aaaaaa bbbbbb](14) 그리고 [cccccc dddddd](14).
        let meta = std::fs::read_to_string(notify_meta_path(&log)).unwrap();
        assert_eq!(parse_retention_start(&meta), 28);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "eeeeee\n");
    }

    // 확인 절차 1 — 고유 표식으로 cap 전후와 reader 중지/재개를 대조한다. reader 가 멈춘
    // 사이 비우기가 났으면 `truncated` 이고, `skipped` 는 **못 읽은 줄의 바이트 합과 정확히
    // 같다.** 재개 뒤 받은 줄은 비우기 이후의 것뿐이다.
    #[test]
    fn a_resuming_reader_is_told_exactly_how_many_bytes_it_lost() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("resume.log");
        const CAP: u64 = 64;
        let mut written = Vec::new();
        let push = |i: usize| {
            let l = format!("mark-{i:04}");
            append_line_to(&log, &l, CAP).unwrap();
            l
        };
        for i in 0..3 {
            written.push(push(i));
        }
        let first = resume(&log, 0);
        assert_eq!(first.lines, written[..3]);
        // reader 가 멈춘 동안 cap 을 몇 번 넘긴다.
        for i in 3..30 {
            written.push(push(i));
        }
        let second = resume(&log, first.next_offset);
        assert!(
            second.truncated,
            "멈춘 사이 비우기가 있었는데 truncated 가 아니다"
        );
        let got: std::collections::BTreeSet<&str> =
            second.lines.iter().map(String::as_str).collect();
        let lost_bytes: u64 = written[3..]
            .iter()
            .filter(|l| !got.contains(l.as_str()))
            .map(|l| l.len() as u64 + 1)
            .sum();
        assert_eq!(second.skipped, Some(lost_bytes));
        // 받은 줄은 쓴 순서의 **꼬리**여야 한다 — 비우기 이후 것만 남는다.
        assert_eq!(second.lines, written[written.len() - second.lines.len()..]);
        assert!(!second.lines.is_empty());
    }

    // 누계 없이 파일이 줄어 현재 끝보다 저장 위치가 커진 경우를 감지한다.
    #[test]
    fn an_unaccounted_truncation_is_reported_as_unknown_loss() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("foreign.log");
        append_line_to(&log, "first-line", 1024).unwrap();
        let first = resume(&log, 0);
        // 메타 없이 비운 것처럼 만든다.
        std::fs::write(&log, b"x\n").unwrap();
        let second = resume(&log, first.next_offset);
        assert!(second.truncated);
        assert_eq!(second.skipped, None);
        assert_eq!(second.lines, ["x"]);
    }

    // 완결되지 않은 줄 조각은 소비하지 않는다 — 다음 재개가 통째로 받는다.
    #[test]
    fn a_partial_trailing_line_is_left_for_the_next_resume() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("partial.log");
        std::fs::write(&log, b"done\nhal").unwrap();
        let first = resume(&log, 0);
        assert_eq!(first.lines, ["done"]);
        assert_eq!(first.next_offset, 5);
    }

    // 고정 폭과 앞자리 0이 없는 셸 호환 표기를 확인한다.
    #[test]
    fn the_meta_line_has_a_fixed_width_and_no_leading_zeros() {
        let small = format_retention_start(7);
        let big = format_retention_start(u64::MAX);
        assert_eq!(small.len(), big.len());
        // 문서에 적힌 메타 파일 폭과 일치해야 한다.
        assert_eq!(small.len(), 37);
        assert!(small.starts_with("retention_start=7 "), "{small:?}");
        assert!(small.ends_with('\n'));
        assert_eq!(parse_retention_start(&small), 7);
        assert_eq!(parse_retention_start(&big), u64::MAX);
        assert_eq!(parse_retention_start(""), 0);
        assert_eq!(parse_retention_start("other=3\n"), 0);
    }

    // 기다리는 동안 다른 writer가 먼저 비워 요청이 해소되면 배타 잠금을 더 기다리지 않는다.
    // 계속 기다리다 예산을 넘기면 다시 자란 파일을 잠금 없이 비워 집계되지 않는 손실을 만든다.
    #[test]
    fn a_waiting_truncation_stops_once_another_writer_has_emptied_the_log() {
        let tmp = tempfile::tempdir().unwrap();
        let meta_path = tmp.path().join("stale.log.meta");
        let holder = open_meta(&meta_path).unwrap();
        holder.lock_shared().unwrap();
        let contender = open_meta(&meta_path).unwrap();

        let checks = std::cell::Cell::new(0u32);
        // 처음 확인에서는 아직 크기 초과, 다음 확인에서는 다른 writer가 비운 뒤다.
        let outcome = MetaLock::exclusive(&contender, &meta_path, || {
            checks.set(checks.get() + 1);
            checks.get() == 1
        });
        assert!(
            matches!(outcome, Exclusive::NotNeeded),
            "해소된 요청은 잠금 없이 비우는 경로로 가지 않는다"
        );
        assert_eq!(checks.get(), 2, "재시도마다 요청을 다시 확인한다");
        holder.unlock().unwrap();
    }

    // reader가 공유 잠금을 유지해도 배타 재시도를 끝내고 잠금 없이 비운다.
    // 이 fixture는 축소된 파일 끝이 이전 읽기 위치보다 작아 집계되지 않은 손실을 감지한다.
    #[test]
    fn a_reader_holding_the_shared_lock_does_not_stall_a_truncating_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("held.log");
        append_line_to(&log, "read-before-hold", 1 << 20).unwrap();
        let before = resume(&log, 0);
        std::fs::write(&log, vec![b'x'; 300]).unwrap();
        // reader 가 공유 잠금을 쥐고 멈춘 상태.
        let meta = std::fs::File::open(notify_meta_path(&log)).unwrap();
        meta.lock_shared().unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let writer_log = log.clone();
        let started = std::time::Instant::now();
        // scope 가 아니라 떼어 낸 스레드다 — 되돌린 코드에서 이 시험이 죽을 때 막힌 writer
        // 를 join 하느라 시험 자체가 멈추지 않게 한다.
        std::thread::spawn(move || {
            let r = append_line_to(&writer_log, "after-cap", 256);
            // 수신 쪽이 이미 시간 초과로 떠났으면 보낼 곳이 없다 — 그 결과는 버린다.
            tx.send(r.is_ok()).ok();
        });
        let finished = rx.recv_timeout(EXCLUSIVE_LOCK_BUDGET * 10);
        let elapsed = started.elapsed();
        assert_eq!(
            finished,
            Ok(true),
            "공유 잠금을 쥔 reader 때문에 cap 을 넘긴 append 가 {elapsed:?} 뒤에도 안 끝났다"
        );
        assert!(
            elapsed >= EXCLUSIVE_LOCK_BUDGET,
            "재시도 없이 바로 포기했다: {elapsed:?}"
        );
        meta.unlock().unwrap();

        assert_eq!(std::fs::read_to_string(&log).unwrap(), "after-cap\n");
        let meta_text = std::fs::read_to_string(notify_meta_path(&log)).unwrap();
        assert_eq!(
            parse_retention_start(&meta_text),
            0,
            "잠금 없이 잰 수로 누계를 올렸다"
        );
        let after = resume(&log, before.next_offset);
        assert!(after.truncated);
        assert_eq!(after.skipped, None, "누계 밖 비우기는 '모른다' 여야 한다");
        assert_eq!(after.lines, ["after-cap"]);
    }

    // 운영 가이드와 같은 잠금 시간 예산·재시도 간격을 사용하는지 확인한다.
    #[test]
    fn the_exclusive_lock_budget_matches_the_documented_values() {
        assert_eq!(EXCLUSIVE_LOCK_BUDGET, std::time::Duration::from_millis(200));
        assert_eq!(EXCLUSIVE_LOCK_RETRY, std::time::Duration::from_millis(5));
        assert_eq!(
            EXCLUSIVE_LOCK_BUDGET.as_millis() / EXCLUSIVE_LOCK_RETRY.as_millis(),
            40
        );
    }

    #[test]
    fn meta_path_sits_next_to_the_log() {
        assert_eq!(
            notify_meta_path(Path::new("/h/notify/42.log")),
            PathBuf::from("/h/notify/42.log.meta")
        );
    }

    // 동시 쓰기와 재개 읽기에서 받은 바이트 + 기록된 손실 = 쓴 바이트인지 확인한다.
    #[test]
    fn under_concurrent_writers_read_plus_skipped_equals_written() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("conc.log");
        const CAP: u64 = 512;
        const WRITERS: usize = 6;
        const ROUNDS: usize = 1_500;
        let done = std::sync::atomic::AtomicUsize::new(0);
        let (mut read_bytes, mut skipped_bytes, mut unknown) = (0u64, 0u64, 0usize);
        let mut seen = std::collections::HashSet::new();
        std::thread::scope(|scope| {
            for w in 0..WRITERS {
                let log = &log;
                let done = &done;
                scope.spawn(move || {
                    for i in 0..ROUNDS {
                        append_line_to(log, &format!("w{w}-{i:05}"), CAP).unwrap();
                    }
                    done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
            }
            let mut offset = 0u64;
            loop {
                let finished = done.load(std::sync::atomic::Ordering::SeqCst) == WRITERS;
                let r = resume(&log, offset);
                for l in &r.lines {
                    read_bytes += l.len() as u64 + 1;
                    assert!(seen.insert(l.clone()), "같은 줄을 두 번 받았다: {l}");
                }
                match r.skipped {
                    Some(n) => skipped_bytes += n,
                    None => unknown += 1,
                }
                offset = r.next_offset;
                if finished {
                    break;
                }
                // reader가 공유 잠금을 계속 차지해 배타 획득을 막지 않도록 사이에 쉰다.
                // 잠금 예산 초과 시 누계 미갱신은 별도 시험에서 확인한다.
                std::thread::sleep(EXCLUSIVE_LOCK_RETRY);
            }
        });
        let written: u64 = (0..WRITERS)
            .flat_map(|w| (0..ROUNDS).map(move |i| format!("w{w}-{i:05}").len() as u64 + 1))
            .sum();
        assert_eq!(unknown, 0, "누계에 안 잡힌 비우기를 {unknown} 번 봤다");
        assert_eq!(
            read_bytes + skipped_bytes,
            written,
            "받은 {read_bytes} + 잃은 {skipped_bytes} 가 쓴 {written} 과 다르다"
        );
    }

    #[test]
    fn append_truncates_when_over_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cap.log");
        // cap=8: "aaaaaa\n" = 7 bytes < 8 이므로 첫 줄은 남고 다음 append 전 크기는 7.
        append_line_to(&path, "aaaaaa", 8).unwrap();
        // 이제 파일 크기 7 < 8 → 두 번째는 append. 크기 7+"bbbbbb\n"(7)=14 ≥ 8.
        append_line_to(&path, "bbbbbb", 8).unwrap();
        // 세 번째 append 시점: 기존 14 ≥ cap 8 → truncate 후 이 줄만 남는다.
        append_line_to(&path, "cccccc", 8).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "cccccc\n");
    }
}
