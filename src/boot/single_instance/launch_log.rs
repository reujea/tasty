//! 두 번째 프로세스의 상세 기록 `<home>/launch.log`.
//!
//! 앱 목록·Dock·시작 메뉴로 실행하면 stderr를 볼 수 없으므로 홈의 별도 파일에 남긴다. 실행 중인
//! 인스턴스의 공유 로그는 열지 않는다. 그 파일은 생성 모드로 열려 있어 끼워 넣은 줄을 덮어쓰거나,
//! 같은 함수로 열면 실행 중인 쪽의 로그를 자른다. 여러 두 번째 프로세스가 동시에 쓸 수 있으므로
//! `launch.log.lock`을 잡고 "크기 검사 → 필요하면 앞부분 버림 → 기록"을 한 구간에서 한다.
//! 토큰 값은 기록하지 않는다. 호출자는 증거의 종류와 유무만 넘긴다.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FILE_NAME: &str = "launch.log";
const LOCK_NAME: &str = "launch.log.lock";
/// 이 크기를 넘으면 뒤쪽 절반만 남긴다.
pub(crate) const MAX_BYTES: u64 = 256 * 1024;
/// 기록 잠금을 기다리는 상한. 넘기면 stderr에만 남긴다.
const LOCK_WAIT: Duration = Duration::from_secs(1);

pub(crate) struct LaunchLog {
    home: Option<PathBuf>,
    max_bytes: u64,
}

impl LaunchLog {
    pub(crate) fn new(home: Option<&Path>) -> Self {
        Self {
            home: home.map(Path::to_path_buf),
            max_bytes: MAX_BYTES,
        }
    }

    #[cfg(test)]
    fn with_max_bytes(home: &Path, max_bytes: u64) -> Self {
        Self {
            home: Some(home.to_path_buf()),
            max_bytes,
        }
    }

    /// 한 줄을 stderr와 파일에 남긴다. 파일 기록 실패는 stderr에만 알린다.
    pub(crate) fn line(&self, message: &str) {
        let line = format!(
            "{} pid={} {}\n",
            crate::clock::local_now("%Y-%m-%dT%H:%M:%S%.3f%:z"),
            std::process::id(),
            message.replace('\n', " ")
        );
        eprint!("tasty launch: {line}");
        let Some(home) = &self.home else {
            return;
        };
        if let Err(e) = append(home, line.as_bytes(), self.max_bytes) {
            eprintln!(
                "tasty launch: could not write {}: {e}",
                home.join(FILE_NAME).display()
            );
        }
    }
}

pub(crate) fn path(home: &Path) -> PathBuf {
    home.join(FILE_NAME)
}

fn lock(home: &Path) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join(LOCK_NAME))?;
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "launch log lock is busy",
                ));
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
        }
    }
}

fn append(home: &Path, line: &[u8], max_bytes: u64) -> std::io::Result<()> {
    let _guard = lock(home)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path(home))?;
    let len = file.metadata()?.len();
    if len + line.len() as u64 > max_bytes {
        keep_tail(&mut file, max_bytes / 2)?;
    }
    file.seek(SeekFrom::End(0))?;
    file.write_all(line)?;
    file.sync_data()
}

/// 마지막 `keep` 바이트 안에서 줄 경계부터 남기고 나머지를 버린다.
fn keep_tail(file: &mut File, keep: u64) -> std::io::Result<()> {
    let len = file.metadata()?.len();
    let start = len.saturating_sub(keep);
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let cut = if start == 0 {
        0
    } else {
        tail.iter()
            .position(|b| *b == b'\n')
            .map_or(tail.len(), |i| i + 1)
    };
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&tail[cut..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_appended_and_old_lines_are_dropped_past_the_cap() {
        let home = tempfile::tempdir().unwrap();
        let log = LaunchLog::with_max_bytes(home.path(), 400);
        for n in 0..40 {
            log.line(&format!("entry {n:02}"));
        }
        let text = std::fs::read_to_string(path(home.path())).unwrap();
        assert!(text.len() as u64 <= 400, "{}", text.len());
        assert!(text.contains("entry 39"));
        assert!(!text.contains("entry 00"));
        // 남은 줄은 모두 온전하다.
        for line in text.lines() {
            assert!(
                line.contains(" pid=") && line.contains("entry "),
                "{line:?}"
            );
        }
    }

    #[test]
    fn concurrent_writers_at_the_cap_keep_every_line_whole() {
        let home = tempfile::tempdir().unwrap();
        let writers: Vec<_> = (0..8)
            .map(|w| {
                let home = home.path().to_path_buf();
                std::thread::spawn(move || {
                    let log = LaunchLog::with_max_bytes(&home, 2048);
                    for n in 0..25 {
                        log.line(&format!("writer {w} entry {n:02} end"));
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        let text = std::fs::read_to_string(path(home.path())).unwrap();
        assert!(text.len() as u64 <= 2048);
        assert!(text.ends_with('\n'));
        for line in text.lines() {
            assert!(line.contains(" pid=") && line.ends_with(" end"), "{line:?}");
        }
        // 마지막 기록은 절단 뒤에 붙으므로 사라지지 않는다.
        assert!(text.lines().count() >= 10);
    }

    /// 줄 머리는 밀리초 셋째 자리와 `+09:00` 같은 숫자 오프셋을 가진 로컬 시각이다.
    #[test]
    fn each_line_starts_with_a_local_millisecond_time_and_numeric_offset() {
        let home = tempfile::tempdir().unwrap();
        LaunchLog::with_max_bytes(home.path(), 400).line("stamp");
        let text = std::fs::read_to_string(path(home.path())).unwrap();
        let stamp = text.split(' ').next().unwrap();
        assert_eq!(stamp.len(), 29, "{stamp}");
        let b = stamp.as_bytes();
        for (i, c) in [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')] {
            assert_eq!(b[i], c, "{stamp}");
        }
        assert_eq!((b[19], b[26]), (b'.', b':'), "{stamp}");
        assert!(matches!(b[23], b'+' | b'-'), "{stamp}");
    }

    #[test]
    fn without_a_home_only_stderr_is_written() {
        LaunchLog::new(None).line("no home");
    }
}
