//! Linux에서 winit과 GDK가 쓸 표시 백엔드를 정한다.
//!
//! 메뉴·WebView·파일 끌어오기·클립보드는 X11 경로에 있으므로 `DISPLAY`가 있으면 X11을 고른다.
//! winit은 `WAYLAND_DISPLAY`가 있으면 Wayland를 먼저 고르고 GDK는 winit과 따로 고르므로 둘 다 고정한다.
//! 환경변수로 고정하지 않는 이유는 PTY 자식과 플러그인이 세션 값을 그대로 받아야 하기 때문이다.
//!
//! X 연결에 실패한 winit은 같은 프로세스에서 다시 만들 수 없다. 그래서 빌드 전에 연결해 보고
//! Wayland로 내려갈 수 있을 때만 내려간다.

use std::ffi::OsStr;
use std::sync::OnceLock;

use tasty_ui_widgets::DisplayNotice;

/// 시작 때 고른 표시 백엔드. 창을 만든 뒤의 안내가 원인을 고를 때 읽는다.
// 이유: 판정을 기록하는 곳은 Linux 이벤트 루프 설정뿐이라 다른 OS에서는 값이 만들어지지 않는다.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartBackend {
    X11,
    /// `DISPLAY`가 없어 네이티브 Wayland로 실행했다(XWayland 없음).
    Wayland,
    /// `DISPLAY`의 X 서버에 연결하지 못해 Wayland로 내려갔다.
    WaylandFallback,
    /// 표시 서버가 없다.
    Unset,
}

/// 시작 판정과 시작 환경에서 읽은 값. 환경은 읽기만 하고 바꾸지 않는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartDisplay {
    pub(crate) backend: StartBackend,
    /// 시작 환경의 `GDK_BACKEND`가 x11을 허용하지 않는다.
    pub(crate) gdk_backend_blocks_x11: bool,
}

static START: OnceLock<StartDisplay> = OnceLock::new();

/// 시작 판정을 남긴다. 이벤트 루프는 한 번만 만들어지므로 첫 값만 남긴다.
// 이유: Linux 이벤트 루프 설정만 부른다.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn record_start(start: StartDisplay) {
    if START.set(start).is_err() {
        tracing::debug!(?start, "display backend start already recorded");
    }
}

/// 이 프로세스의 시작 판정. Linux가 아니거나 아직 정하지 않았으면 `None`이다.
pub(crate) fn start() -> Option<StartDisplay> {
    START.get().copied()
}

/// 네이티브 Wayland 창이 `DISPLAY`의 X 서버 연결 실패로 생겼는지(원인 1b).
pub(crate) fn started_after_x_failure() -> bool {
    start().is_some_and(|start| start.backend == StartBackend::WaylandFallback)
}

/// `GDK_BACKEND` 값이 x11 백엔드를 허용하지 않는지. GDK는 쉼표로 나눈 이름을 그대로 비교하고
/// `*`는 어느 백엔드든 받는다. `help`는 목록을 출력한 뒤 기본 목록을 쓴다. 값이 없으면 기본 목록을 쓰고,
/// 빈 값은 고를 백엔드가 없어 x11도 열지 못한다. GDK처럼 바이트로 비교하므로 UTF-8이 아닌 항목이
/// 섞여 있어도 `x11`이나 `*` 항목이 있으면 막지 않는다.
// 이유: GDK는 Linux에서만 쓰므로 Linux 이벤트 루프 설정만 부른다.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn gdk_backend_blocks_x11(value: Option<&OsStr>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let bytes = value.as_encoded_bytes();
    let allows = bytes == b"help"
        || bytes
            .split(|&byte| byte == b',')
            .any(|name| name == b"x11" || name == b"*");
    !allows
}

/// WebView를 만들 수 없는 표시 경로의 안내 kind를 고른다. Wayland 창이면 GTK 결과와 관계없이 원인 1·1b다.
/// X11 창에서 GTK 초기화가 실패하면 `GDK_BACKEND`가 x11을 막을 때만 원인 2이고, 그 밖은 원인을 모르는
/// GTK 초기화 실패다.
pub(crate) fn classify(start: Option<StartDisplay>, gtk_failed: bool) -> Option<DisplayNotice> {
    let start = start?;
    match start.backend {
        StartBackend::Wayland => Some(DisplayNotice::NativeWayland),
        StartBackend::WaylandFallback => Some(DisplayNotice::NativeWaylandXFailed),
        StartBackend::X11 | StartBackend::Unset if gtk_failed => {
            Some(if start.gdk_backend_blocks_x11 {
                DisplayNotice::GdkBackend
            } else {
                DisplayNotice::GtkInit
            })
        }
        StartBackend::X11 | StartBackend::Unset => None,
    }
}

/// 지금 이 프로세스에서 보일 안내 kind. Linux가 아니면 언제나 `None`이다.
pub(crate) fn current_notice() -> Option<DisplayNotice> {
    #[cfg(target_os = "linux")]
    let gtk_failed = tasty_platform::gtk_init::failed();
    #[cfg(not(target_os = "linux"))]
    let gtk_failed = false;
    classify(start(), gtk_failed)
}

/// 환경변수가 가리키는 표시 서버. 값이 비어 있으면 없는 것으로 본다. winit도 같게 읽는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplayEnv {
    pub(crate) x11: bool,
    pub(crate) wayland: bool,
}

impl DisplayEnv {
    pub(crate) fn from_values(
        display: Option<&OsStr>,
        wayland_display: Option<&OsStr>,
        wayland_socket: Option<&OsStr>,
    ) -> Self {
        let present = |value: Option<&OsStr>| value.is_some_and(|value| !value.is_empty());
        Self {
            x11: present(display),
            wayland: present(wayland_display) || present(wayland_socket),
        }
    }

    pub(crate) fn current() -> Self {
        Self::from_values(
            std::env::var_os("DISPLAY").as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").as_deref(),
            std::env::var_os("WAYLAND_SOCKET").as_deref(),
        )
    }

    /// 창이 없는 두 번째 실행의 로그 값. X 연결을 시험하지 않으므로 내림은 반영하지 않는다.
    pub(crate) fn expected_backend_name(self) -> &'static str {
        if self.x11 {
            "x11"
        } else if self.wayland {
            "wayland"
        } else {
            "none"
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::configure;

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use winit::event_loop::EventLoopBuilder;
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    use winit::platform::x11::EventLoopBuilderExtX11;
    use x11rb::rust_connection::RustConnection;

    use super::{DisplayEnv, StartBackend, StartDisplay};
    use crate::AppEvent;

    /// 끊긴 원격 `DISPLAY`에서 시작을 얼마나 기다릴지의 상한.
    /// 컴포지터가 첫 연결에 XWayland를 띄우는 시간보다 길어야 한다.
    const X_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

    #[derive(Debug, PartialEq, Eq)]
    enum Choice {
        X11,
        /// X 연결에 실패해 내려갔다.
        WaylandFallback(String),
        Wayland,
        /// 표시 서버가 없다. winit의 원래 오류로 끝나도록 고정하지 않는다.
        Unset,
    }

    /// 내려갈 곳이 없으면 X 연결을 시험하지 않는다. winit이 같은 오류를 낸다.
    fn choose<P>(
        env: DisplayEnv,
        probe: impl FnOnce() -> Result<P, String>,
    ) -> (Choice, Option<P>) {
        match (env.x11, env.wayland) {
            (true, true) => match probe() {
                Ok(connection) => (Choice::X11, Some(connection)),
                Err(reason) => (Choice::WaylandFallback(reason), None),
            },
            (true, false) => (Choice::X11, None),
            (false, true) => (Choice::Wayland, None),
            (false, false) => (Choice::Unset, None),
        }
    }

    /// 연결이 멎어도 시작이 멈추지 않도록 별도 스레드에서 연결한다. 시간을 넘긴 스레드는 버린다.
    fn with_timeout<T: Send + 'static>(
        timeout: Duration,
        connect: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("x-probe".to_string())
            .spawn(move || {
                // 받는 쪽이 시간을 넘겨 떠났으면 늦은 연결을 닫는 것 말고 할 일이 없다.
                drop(sender.send(connect()));
            })
            .map_err(|error| format!("cannot start the X probe thread: {error}"))?;
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(format!("no reply from the X server in {timeout:?}"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("the X probe thread ended without a result".to_string())
            }
        }
    }

    fn connect_x(display: Option<String>) -> Result<RustConnection, String> {
        x11rb::rust_connection::RustConnection::connect(display.as_deref())
            .map(|(connection, _screen)| connection)
            .map_err(|error| error.to_string())
    }

    /// 빌더와 GDK의 백엔드를 고정한다. 돌려준 연결은 `build()`가 끝날 때까지 들고 있는다.
    /// 컴포지터는 클라이언트가 없는 XWayland를 내릴 수 있어, 시험과 winit 연결 사이가 비면 안 된다.
    pub(crate) fn configure(builder: &mut EventLoopBuilder<AppEvent>) -> Option<RustConnection> {
        let env = DisplayEnv::current();
        let started = Instant::now();
        let (choice, connection) =
            choose(env, || with_timeout(X_PROBE_TIMEOUT, || connect_x(None)));
        apply(builder, &choice);
        log_choice(&choice, env, started.elapsed());
        super::record_start(StartDisplay {
            backend: start_backend(&choice),
            gdk_backend_blocks_x11: super::gdk_backend_blocks_x11(
                std::env::var_os("GDK_BACKEND").as_deref(),
            ),
        });
        connection
    }

    fn start_backend(choice: &Choice) -> StartBackend {
        match choice {
            Choice::X11 => StartBackend::X11,
            Choice::WaylandFallback(_) => StartBackend::WaylandFallback,
            Choice::Wayland => StartBackend::Wayland,
            Choice::Unset => StartBackend::Unset,
        }
    }

    fn apply(builder: &mut EventLoopBuilder<AppEvent>, choice: &Choice) {
        match choice {
            Choice::X11 => {
                builder.with_x11();
                // GDK는 WAYLAND_DISPLAY가 없어도 기본 소켓(wayland-0)이 있으면 Wayland를 고른다.
                // 첫 gtk::init 전에 불러야 효력이 있다.
                gtk::gdk::set_allowed_backends("x11");
            }
            Choice::WaylandFallback(_) | Choice::Wayland => {
                builder.with_wayland();
            }
            Choice::Unset => {}
        }
    }

    fn log_choice(choice: &Choice, env: DisplayEnv, elapsed: Duration) {
        match choice {
            Choice::X11 => log_selected("x11", env, elapsed),
            Choice::Wayland => log_selected("wayland", env, elapsed),
            Choice::WaylandFallback(reason) => log_fallback(reason, elapsed),
            Choice::Unset => log_unset(),
        }
    }

    fn log_selected(backend: &str, env: DisplayEnv, elapsed: Duration) {
        tracing::info!(backend, ?env, ?elapsed, "display backend selected");
    }

    fn log_fallback(reason: &str, elapsed: Duration) {
        tracing::warn!(
            reason,
            ?elapsed,
            "display backend: wayland, because the X server in DISPLAY is unreachable"
        );
    }

    fn log_unset() {
        tracing::warn!("display backend: no display server in the environment");
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn env(x11: bool, wayland: bool) -> DisplayEnv {
            DisplayEnv { x11, wayland }
        }

        #[test]
        fn x11_is_chosen_whenever_the_x_server_answers() {
            let (choice, connection) = choose(env(true, true), || Ok::<_, String>(7));
            assert_eq!((choice, connection), (Choice::X11, Some(7)));
        }

        #[test]
        fn an_unreachable_x_server_falls_back_to_wayland() {
            let (choice, connection) = choose(env(true, true), || Err::<(), _>("gone".to_string()));
            assert_eq!(
                (choice, connection),
                (Choice::WaylandFallback("gone".to_string()), None)
            );
        }

        #[test]
        fn the_probe_runs_only_when_there_is_somewhere_to_fall_back_to() {
            let unreachable = || -> Result<(), String> { panic!("must not probe") };
            assert_eq!(choose(env(true, false), unreachable).0, Choice::X11);
            assert_eq!(choose(env(false, true), unreachable).0, Choice::Wayland);
            assert_eq!(choose(env(false, false), unreachable).0, Choice::Unset);
        }

        #[test]
        fn a_stalled_connection_ends_at_the_timeout() {
            let started = Instant::now();
            let result = with_timeout::<()>(Duration::from_millis(50), || {
                std::thread::sleep(Duration::from_secs(30));
                Ok(())
            });
            assert!(result.unwrap_err().contains("no reply"));
            assert!(started.elapsed() < Duration::from_secs(10));
        }

        /// Given 시작 판정, Then 안내가 읽는 시작 백엔드는 같은 갈래다.
        #[test]
        fn every_choice_keeps_its_start_backend() {
            assert_eq!(start_backend(&Choice::X11), StartBackend::X11);
            assert_eq!(start_backend(&Choice::Wayland), StartBackend::Wayland);
            assert_eq!(
                start_backend(&Choice::WaylandFallback("gone".to_string())),
                StartBackend::WaylandFallback
            );
            assert_eq!(start_backend(&Choice::Unset), StartBackend::Unset);
        }

        #[test]
        fn a_display_with_no_server_is_a_connection_error() {
            // 절대 경로 소켓을 쓰는 번호가 아니라 파일이 없는 소켓을 가리켜 실제 서버와 겹치지 않는다.
            let result = connect_x(Some("/nonexistent/tasty-no-x-server:0".to_string()));
            assert!(result.is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(display: &str, wayland: &str, socket: &str) -> DisplayEnv {
        DisplayEnv::from_values(
            Some(OsStr::new(display)),
            Some(OsStr::new(wayland)),
            Some(OsStr::new(socket)),
        )
    }

    #[test]
    fn empty_values_count_as_absent() {
        assert_eq!(
            read("", "", ""),
            DisplayEnv {
                x11: false,
                wayland: false
            }
        );
        assert_eq!(DisplayEnv::from_values(None, None, None), read("", "", ""),);
    }

    #[test]
    fn the_wayland_socket_alone_counts_as_wayland() {
        assert!(read("", "", "3").wayland);
    }

    #[test]
    fn the_expected_name_prefers_x11() {
        assert_eq!(read(":0", "wayland-0", "").expected_backend_name(), "x11");
        assert_eq!(read(":0", "", "").expected_backend_name(), "x11");
        assert_eq!(read("", "wayland-0", "").expected_backend_name(), "wayland");
        assert_eq!(read("", "", "").expected_backend_name(), "none");
    }

    fn started(backend: StartBackend, gdk_backend_blocks_x11: bool) -> Option<StartDisplay> {
        Some(StartDisplay {
            backend,
            gdk_backend_blocks_x11,
        })
    }

    /// Given `GDK_BACKEND` 값, When x11 허용 여부를 본다, Then GDK가 x11을 열 수 있는 값만 막지 않는다.
    #[test]
    fn only_values_that_let_gdk_open_x11_do_not_block_it() {
        for open in [
            None,
            Some("x11"),
            Some("wayland,x11"),
            Some("*"),
            Some("help"),
        ] {
            assert!(!gdk_backend_blocks_x11(open.map(OsStr::new)), "{open:?}");
        }
        for blocked in ["wayland", "broadway", "", "X11", "x11 ", "wayland,"] {
            assert!(
                gdk_backend_blocks_x11(Some(OsStr::new(blocked))),
                "{blocked:?}"
            );
        }
    }

    /// Given UTF-8이 아닌 바이트가 섞인 `GDK_BACKEND`, When x11 허용 여부를 본다,
    /// Then `x11`·`*` 항목이 있으면 막지 않고 없으면 막는다.
    #[cfg(unix)]
    #[test]
    fn non_utf8_entries_do_not_hide_an_x11_entry() {
        use std::os::unix::ffi::OsStrExt;
        for open in [&b"x11,\xff"[..], b"*,\xff"] {
            assert!(
                !gdk_backend_blocks_x11(Some(OsStr::from_bytes(open))),
                "{open:?}"
            );
        }
        for blocked in [&b"x\xff"[..], b"wayland,\xff"] {
            assert!(
                gdk_backend_blocks_x11(Some(OsStr::from_bytes(blocked))),
                "{blocked:?}"
            );
        }
    }

    /// Given Wayland 창, When GTK 결과와 `GDK_BACKEND`가 무엇이든, Then 원인 1·1b 안내다.
    #[test]
    fn a_wayland_window_always_names_native_wayland() {
        for gtk_failed in [false, true] {
            for blocks in [false, true] {
                assert_eq!(
                    classify(started(StartBackend::Wayland, blocks), gtk_failed),
                    Some(DisplayNotice::NativeWayland)
                );
                assert_eq!(
                    classify(started(StartBackend::WaylandFallback, blocks), gtk_failed),
                    Some(DisplayNotice::NativeWaylandXFailed)
                );
            }
        }
    }

    /// Given X11 창, When GTK 초기화가 실패했다, Then `GDK_BACKEND`가 x11을 막을 때만 원인 2이고 그 밖은 gtk-init이다.
    #[test]
    fn a_gtk_failure_on_x11_names_gdk_backend_only_when_it_blocks_x11() {
        assert_eq!(
            classify(started(StartBackend::X11, true), true),
            Some(DisplayNotice::GdkBackend)
        );
        assert_eq!(
            classify(started(StartBackend::X11, false), true),
            Some(DisplayNotice::GtkInit)
        );
    }

    /// Given X11 창과 정상 GTK 또는 판정 전, Then 안내가 없다.
    #[test]
    fn a_working_x11_window_or_no_start_has_no_notice() {
        for blocks in [false, true] {
            assert_eq!(classify(started(StartBackend::X11, blocks), false), None);
            assert_eq!(classify(started(StartBackend::Unset, blocks), false), None);
        }
        assert_eq!(classify(None, true), None);
    }

    /// Tasty는 `GDK_BACKEND`를 읽기만 한다. 지우거나 덮으면 PTY 자식과 플러그인이 다른 값을 받는다.
    #[test]
    fn no_source_writes_gdk_backend() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut stack = vec![root.join("src"), root.join("crates")];
        let mut scanned = 0usize;
        let mut writes = Vec::new();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n != "target") {
                        stack.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                scanned += 1;
                for line in text.lines().filter(|l| l.contains("\"GDK_BACKEND\"")) {
                    if ["set_var", "remove_var", "env_remove", ".env("]
                        .iter()
                        .any(|w| line.contains(w))
                    {
                        writes.push(format!("{}: {line}", path.display()));
                    }
                }
            }
        }
        assert!(scanned > 1000, "scanned only {scanned} files");
        assert!(writes.is_empty(), "GDK_BACKEND writes: {writes:#?}");
    }
}
