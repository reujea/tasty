//! 시험이 X11 winit `EventLoop` 를 만드는 유일한 입구다.
//!
//! winit 은 X 연결을 `XOpenDisplay(NULL)` 한 번으로 열고 실패도 프로세스 전역에 보관한다.
//! 같은 프로세스에서는 다시 시도할 수 없으므로 여기서도 재시도하지 않는다. 대신 실패하면
//! 연결 실패의 원인 후보(다른 서버·인증 파일·서버 소멸·fd 고갈)를 가를 수 있는 환경 상태를
//! panic 메시지에 싣는다. 서버 쪽 사정(리셋, 클라이언트 상한)은 Xvfb 로그와 함께 읽는다.
//! 공유 도우미 밖의 `.with_x11()` 은 `source_guards::x11_test_loop_has_one_door` 가 막는다.

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use winit::event_loop::EventLoop;
use winit::platform::x11::EventLoopBuilderExtX11;

/// X11 백엔드로 아무 스레드에서나 도는 `EventLoop` 를 만든다. 실패하면 진단을 싣고 panic 한다.
pub(crate) fn x11_event_loop<T: 'static>() -> EventLoop<T> {
    match EventLoop::<T>::with_user_event()
        .with_x11()
        .with_any_thread(true)
        .build()
    {
        Ok(event_loop) => event_loop,
        Err(error) => {
            let display = std::env::var("DISPLAY").ok();
            let xauthority = std::env::var("XAUTHORITY").ok();
            panic!(
                "X11 EventLoop could not be built: {error:?}\n{}",
                diagnose(display.as_deref(), xauthority.as_deref(), Path::new("/tmp"))
            )
        }
    }
}

/// `DISPLAY` 의 디스플레이 번호. `:N`, `:N.S`, `host:N` 형식을 읽는다.
fn display_number(display: &str) -> Option<u32> {
    let (_, rest) = display.rsplit_once(':')?;
    rest.split('.').next()?.parse().ok()
}

/// 연결 실패를 가를 환경 상태. `tmp` 는 X 소켓·lock 이 있는 폴더(`/tmp`)이며 시험이 바꿔 넣는다.
fn diagnose(display: Option<&str>, xauthority: Option<&str>, tmp: &Path) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("{}.{:03}", d.as_secs(), d.subsec_millis()))
        .unwrap_or_else(|e| format!("clock before epoch: {e}"));
    let mut lines = vec![
        format!("failed at unix time: {now}"),
        format!("DISPLAY: {display:?}"),
        format!("XAUTHORITY: {xauthority:?}"),
        match xauthority {
            Some(path) => format!("XAUTHORITY file: {}", describe_file(Path::new(path))),
            None => "XAUTHORITY file: (no XAUTHORITY)".to_string(),
        },
    ];
    match display.and_then(display_number) {
        Some(n) => {
            let socket = tmp.join(".X11-unix").join(format!("X{n}"));
            let state = if socket.exists() { "exists" } else { "missing" };
            lines.push(format!("socket {}: {state}", socket.display()));
            let lock = tmp.join(format!(".X{n}-lock"));
            lines.push(format!("lock {}: {}", lock.display(), describe_lock(&lock)));
        }
        None => lines.push("socket/lock: DISPLAY has no display number".to_string()),
    }
    lines.push(format!("open fds: {}", open_fd_count()));
    lines.push(format!("RLIMIT_NOFILE: {}", descriptor_limit()));
    lines.join("\n")
}

/// 존재·읽기 결과·소유자·권한·크기. 쿠키 내용은 싣지 않는다.
fn describe_file(path: &Path) -> String {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) => return format!("metadata failed: {e}"),
    };
    let read = match std::fs::read(path) {
        Ok(bytes) => format!("read {} bytes", bytes.len()),
        Err(e) => format!("read failed: {e}"),
    };
    format!(
        "exists, uid {} mode {:o} size {}, {read}",
        meta.uid(),
        meta.mode() & 0o7777,
        meta.len()
    )
}

/// lock 파일 존재와 그 안의 pid 가 살아 있는지.
fn describe_lock(lock: &Path) -> String {
    let text = match std::fs::read_to_string(lock) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return "missing".to_string(),
        Err(e) => return format!("read failed: {e}"),
    };
    match text.trim().parse::<u32>() {
        Ok(pid) => {
            let alive = Path::new("/proc").join(pid.to_string()).exists();
            format!(
                "exists, pid {pid} {}",
                if alive { "alive" } else { "not running" }
            )
        }
        Err(_) => format!("exists, unparsable pid {:?}", text.trim()),
    }
}

fn open_fd_count() -> String {
    match std::fs::read_dir("/proc/self/fd") {
        Ok(entries) => entries.count().to_string(),
        Err(e) => format!("unknown ({e})"),
    }
}

fn descriptor_limit() -> String {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: limit is a live rlimit for the call.
    let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) };
    if result == 0 {
        format!("soft {} hard {}", limit.rlim_cur, limit.rlim_max)
    } else {
        format!("unknown ({})", std::io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_numbers_are_read_from_local_and_host_forms() {
        assert_eq!(display_number(":99"), Some(99));
        assert_eq!(display_number(":7.0"), Some(7));
        assert_eq!(display_number("localhost:12.1"), Some(12));
        assert_eq!(display_number("wayland-0"), None);
        assert_eq!(display_number(":"), None);
    }

    #[test]
    fn a_missing_server_is_described_without_the_cookie() {
        let tmp = tempfile::tempdir().unwrap();
        let auth = tmp.path().join("auth");
        std::fs::write(&auth, b"SECRET-COOKIE").unwrap();
        let text = diagnose(Some(":9999"), auth.to_str(), tmp.path());
        assert!(text.contains("DISPLAY: Some(\":9999\")"), "{text}");
        assert!(text.contains("X11-unix/X9999: missing"), "{text}");
        assert!(text.contains(".X9999-lock: missing"), "{text}");
        assert!(text.contains("read 13 bytes"), "{text}");
        assert!(!text.contains("SECRET-COOKIE"), "{text}");
        assert!(text.contains("open fds: "), "{text}");
        assert!(text.contains("RLIMIT_NOFILE: soft "), "{text}");
    }

    #[test]
    fn a_stale_lock_and_a_live_socket_are_told_apart() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".X11-unix")).unwrap();
        std::fs::write(tmp.path().join(".X11-unix/X5"), b"").unwrap();
        // pid_max 를 넘는 pid 는 살아 있을 수 없다.
        std::fs::write(tmp.path().join(".X5-lock"), b"  4194305\n").unwrap();
        let text = diagnose(Some(":5"), Some("/nonexistent/auth"), tmp.path());
        assert!(text.contains("X11-unix/X5: exists"), "{text}");
        assert!(text.contains("pid 4194305 not running"), "{text}");
        assert!(text.contains("XAUTHORITY file: metadata failed"), "{text}");
    }
}
