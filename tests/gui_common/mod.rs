//! GUI integration test harness for tasty.
//!
//! Launches a single shared tasty GUI instance for all tests.
//! Each test creates its own workspace for isolation — no state reset needed.
//! 이 "인스턴스 1 개 + workspace 격리" 원칙 전체는 `docs/dev-guide/e2e-tests.md` §1
//! (근거는 ADR-0090). IPC 전용 e2e 는 `tests/common/mod.rs` 의 `shared()` 를 쓴다 —
//! 이쪽이 `MutexGuard` 로 테스트를 직렬화하는 건 실제 데스크톱 입력을 주입하기 때문이다.

// 시험 하네스라 unsafe 는 시험을 세우는 데만 쓴다. `cfg_attr(test, ..)` 형태를 쓰는 것은
// 이 파일이 `check-allow-reason` 의 좌변 밖(루트 `tests/`)이라 사유 주석이 어느 게이트에도
// 안 걸리기 때문이다 — 그래서 "테스트라서 뺐다" 를 **형태**가 남기게 한다. 프로덕션 자리는
// 같은 lint 라도 무조건 `#![allow]` + 사유이고, 그 둘이 형태로 갈린다.
#![cfg_attr(test, allow(clippy::multiple_unsafe_ops_per_block))]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다 — 전수 가드
// (`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)가 테스트 본문을 제외하므로, 여기서 나는
// `let_underscore_must_use` 경고는 정책상 조치 대상이 될 수 없다. 끄지 않으면
// 프로덕션의 진짜 신호가 그 안에 묻힌다 — `docs/dev-guide/error-handling.md`.
#![allow(clippy::let_underscore_must_use)]
// 다중 test binary 가 공유하는 test-support 모듈 — binary 마다 사용하는 부분집합이
// 달라 개별 binary 기준 dead_code 판정이 무의미하다 (의도된 superset API).
#![allow(dead_code)]

#[path = "../spawn_diag/mod.rs"]
mod spawn_diag;

#[cfg(all(test, target_os = "linux"))]
mod stderr_tests;

// stderr 포착의 생산자는 `spawn_diag` 한 곳이다 — 이 하네스가 들고 있던 링·시각·배출
// 스레드가 형제 둘과 바이트 단위로 같았다.
use spawn_diag::{STDERR_TAIL_LINES, StderrCapture};

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enigo::{Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings as EnigoSettings};
use serde_json::Value;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClientRect, GetWindowRect, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

// --- Shared instance ---

static SHARED_INSTANCE: OnceLock<Mutex<GuiTestInstance>> = OnceLock::new();
static CLEANUP_PID: AtomicU32 = AtomicU32::new(0);
/// 공유 인스턴스의 격리 홈. **`Drop` 으로는 못 지운다** — `SHARED_INSTANCE` 는 `static`
/// 이고 Rust 는 static 을 프로세스 종료 시 drop 하지 않는다. 그래서 정리를 `Drop` 에만
/// 두면 이 하네스에서는 **한 번도 안 돈다**. 실측: 그 상태로 14 회 돌려 `/tmp` 에
/// 1.1 GB × 14 = 15 GB 가 남았다(번들 plugin 사본이 홈마다 들어간다). PID kill 과 같은
/// atexit 콜백에 함께 태운다.
static CLEANUP_HOME: OnceLock<PathBuf> = OnceLock::new();
/// 공유 인스턴스의 port 파일. 홈과 **같은 이유로** 여기 있어야 한다 — `Drop` 은 이
/// 하네스에서 안 돈다.
///
/// 이것만 빠져 있었다. 실측 2026-09-07: `/tmp` 에 죽은 `tasty-gui-test-*.port` **112 개**,
/// 남은 `tasty-gui-test-home-*` **0 개** — 홈은 지워지고 port 파일만 쌓인 자국이다.
/// 그 잔해가 계기를 망친다: 실행 중인 인스턴스를 글롭(`/tmp/tasty-gui-test-*.port`)으로
/// 찾으면 죽은 파일이 전부 잡혀 `Connection refused` 만 나오고, 그 증상은 "대상이 없다"
/// 가 아니라 **"대상이 있는데 안 붙는다"** 로 보여 계기가 아니라 표적을 의심하게 만든다.
static CLEANUP_PORT: OnceLock<PathBuf> = OnceLock::new();
/// 첫 spawn 이 실패했을 때 뒤 테스트가 **실제로 다시 프로세스를 띄우는 것**을 막는다.
/// 기전은 `spawn_diag` 에 있고 상태만 여기 둔다 — 이유는 그쪽 doc 주석 참조.
/// 형제 하네스 `tests/common` 은 같은 래치를 자기 안에 손으로 갖고 있다(그 파일은
/// 이 lane 소유가 아니라 여기서 옮기지 않았다). 그쪽도 이 타입으로 모으면 정의가 하나가 된다.
static SPAWN_LATCH: spawn_diag::SpawnOnceLatch = spawn_diag::SpawnOnceLatch::new();

/// Acquire the shared GUI test instance.
/// The first call spawns the tasty process; subsequent calls reuse it.
pub fn shared() -> std::sync::MutexGuard<'static, GuiTestInstance> {
    let guard = SHARED_INSTANCE.get_or_init(|| {
        // ★ 이 클로저는 panic 하면 `OnceLock` 을 **미초기화로 남긴다** — 다음 테스트가
        // 그대로 다시 돈다. 실측(디스플레이 없이 6 건): spawn 시도 6 회, 패닉 자리 1 곳.
        // 래치를 spawn **앞**에 두는 것이 요점이다 — 두 번째 프로세스를 띄우기 전에 막는다.
        SPAWN_LATCH.entering("gui 공유 인스턴스");
        let inst = GuiTestInstance::spawn();
        SPAWN_LATCH.succeeded();
        // Register atexit to kill tasty when the test process exits
        CLEANUP_PID.store(inst.process_id(), Ordering::Relaxed);
        // reason: `set` 은 이미 값이 있을 때만 `Err` 인데, 이 자리는 `get_or_init`
        // 클로저 안이라 프로세스당 한 번만 돈다. 두 번째 호출이 있다면 그것은 이 설계가
        // 깨진 것이고, 그때도 먼저 넣은 경로가 유효하므로 덮어쓰지 않는 것이 옳다.
        let _ = CLEANUP_HOME.set(inst.isolated_home.clone());
        // 위와 같은 이유(첫 호출에서 한 번만 돈다)로 `set` 의 `Err` 은 무시한다.
        let _ = CLEANUP_PORT.set(inst.port_file.clone());
        extern "C" fn on_exit() {
            let pid = CLEANUP_PID.load(Ordering::Relaxed);
            if pid != 0 {
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                #[cfg(not(target_os = "windows"))]
                // SAFETY: SIGTERM 송신은 thread-safe POSIX. pid가 이미 종료된 상태여도
                // kill은 errno만 set하고 UB 없음.
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
            if let Some(home) = CLEANUP_HOME.get() {
                // reason: 정리 실패가 시험 판정을 바꾸지 않는다 — 이미 끝난 회차의 임시
                // 디렉터리이고, 남아도 다음 회차는 자기 `unique` 로 새 경로를 쓴다.
                // atexit 안이라 패닉시킬 수도 없다.
                let _ = std::fs::remove_dir_all(home);
            }
            if let Some(port) = CLEANUP_PORT.get() {
                // reason: 위와 같다. 다만 이쪽은 남았을 때의 값이 다르다 — 홈은 디스크만
                // 먹지만 port 파일은 **다음 사람의 계기를 망친다**(위 `CLEANUP_PORT` 주석).
                let _ = std::fs::remove_file(port);
            }
        }
        // SAFETY: atexit는 process-lifetime callback을 등록. on_exit는 'static fn 포인터.
        // shared() 첫 호출 시 한 번만 등록되며 (OnceLock get_or_init), 중복 등록 없음.
        unsafe {
            libc::atexit(on_exit);
        }
        Mutex::new(inst)
    });
    // 오염된 락에서 복구한다 — `.unwrap()` 이면 **한 건의 패닉이 나머지 전부를 죽인다.**
    // 이 인스턴스는 33 건이 공유하므로, 한 테스트가 단정에서 죽으면 그 뒤의 모든 테스트가
    // 자기 물음을 묻지도 못하고 `PoisonError` 로 실패한다 — 실측: 진짜 실패 1 건이
    // 화면에 31 건으로 나왔다. 그러면 회차가 세는 수가 사건 수가 아니게 되고,
    // "격리하면 도는가" 같은 물음에 그 수로 답할 수 없다.
    // 복구가 안전한 이유: 보호 대상은 자식 프로세스 핸들과 Enigo 뿐이라 테스트 단정의
    // 패닉이 그 둘의 불변식을 깨지 않는다. 인스턴스가 실제로 죽었으면 뒤 테스트는
    // 자기 자리에서 자기 이유로 실패한다 — 그것이 오염 실패보다 정확하다.
    guard
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// --- GuiTestInstance ---

/// GUI test instance: a running tasty GUI process with IPC access and input simulation.
pub struct GuiTestInstance {
    process: Child,
    /// 부팅 뒤에도 실패 진단에 쓰며, 자식을 거둔 뒤 배출 스레드를 join 한다.
    stderr: StderrCapture,
    port: u16,
    port_file: PathBuf,
    /// 이 인스턴스 전용 `TASTY_HOME`. `Drop` 이 지운다.
    isolated_home: PathBuf,
    pub enigo: Enigo,
    #[cfg(target_os = "windows")]
    hwnd: HWND,
}

// SAFETY: GuiTestInstance는 OnceLock<Mutex<>> 안에 들어가 모든 접근이 Mutex로 직렬화된다.
// HWND(*mut c_void) 자체는 OS thread affinity 측면에서 보면 main thread 윈도우 객체지만,
// 테스트 코드는 (1) instance를 단일 thread에서만 spawn하고 (2) HWND를 SetForegroundWindow
// 등 thread-safe Win32 호출에만 전달한다. 따라서 임의 스레드에서 HWND를 "소유"하는 게
// 아니라 단지 포인터 값을 전달하는 수준이므로 Send/Sync 추가가 안전하다.
unsafe impl Send for GuiTestInstance {}
// SAFETY: 위 Send와 동일 근거 — Mutex 직렬화 + 단순 포인터 값 전달.
unsafe impl Sync for GuiTestInstance {}

/// GUI 부팅 상한. IPC 전용 하네스보다 짧게 둔 값을 그대로 유지한다 — 이 회차는
/// 상한을 바꾸지 않는다(상한 조정은 처방이 아니다).
const GUI_SPAWN_PORT_TIMEOUT: Duration = Duration::from_secs(15);

impl GuiTestInstance {
    /// Spawn a tasty GUI instance for testing.
    /// Waits for the window to appear and focuses it.
    pub fn spawn() -> Self {
        // 유일화 키에 **시각을 안 쓴다.** 시계의 해상도는 플랫폼의 성질이라 같은 코드가
        // 어떤 OS 에서는 유일하고 어떤 OS 에서는 겹친다 — 겹치면 두 완주가 같은 경로를
        // 쓰고 먼저 끝난 쪽이 다른 쪽의 파일을 지운다. 단조 카운터는 해상도가 없어
        // 플랫폼을 안 읽고, 프로세스 전역이라 같은 스레드의 재호출도 가른다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let port_file = std::env::temp_dir().join(format!("tasty-gui-test-{unique}.port"));

        // 이 인스턴스 전용 tasty 루트. 형제 하네스 둘이 가진 것을 이 하나만 안 가졌다.
        let isolated_home = std::env::temp_dir().join(format!("tasty-gui-test-home-{unique}"));

        // Launch tasty in GUI mode with port-file for IPC.
        // TASTY_DEBUG_SUPPRESS_NATIVE_MENU: egui 프레임이 세우는 컨텍스트 메뉴(explorer 등)를
        // 블로킹 native 팝업 없이 `debug_captured_menu` 로 포획하게 해, headless 에서
        // `debug.pending_menu` 로 관찰 가능케 한다(debug 격리, release 미노출).
        let mut command = Command::new(env!("CARGO_BIN_EXE_tasty"));
        command
            .arg("--port-file")
            .arg(port_file.to_str().unwrap())
            .env("TASTY_DEBUG_SUPPRESS_NATIVE_MENU", "1")
            // ★ 전용 tasty 루트. 이것이 없으면 자식은 **사용자의 진짜 `~/.tasty-debug`** 를
            // 쓴다 — 번들 plugin 을 거기 설치하고, 거기 저장된 레이아웃을 **복원한다.**
            // 뒤쪽이 이 스위트의 마우스 판정을 통째로 무효로 만들고 있었다: 복원된
            // workspace 에는 surface 가 여럿인데 rect 를 가진 것은 **활성 탭 하나**이고,
            // `first_surface_id()` 가 집는 것은 배경 탭이다. 그러면
            // `debug_inject_mesh_pointer` 가 `surface_rect_by_id == None` 으로 **false** 를
            // 내고 아무 일도 안 일어난다 — 시험은 그 빈 출력을 "보고가 없다" 로 읽는다.
            //
            // 실측(같은 Xvfb·같은 커밋, `TASTY_HOME` 하나만 바꿈):
            //     실제 홈  active_ws surface 26 · injected false · 보고 ""
            //     격리 홈  active_ws surface  1 · injected true  · 보고 `\e[<35;48;23M…`
            //
            // ☆ `HOME` 은 **일부러 격리하지 않는다.** 형제 `tests/common` 은 그것까지
            // 하지만 거기엔 짝이 되는 격리 config 작성이 함께 있다(shell auto-detect 를
            // 막아 port file 이 반드시 써지게 한다). 그 짝 없이 `HOME` 만 옮기면 shell
            // setup 모드로 빠질 수 있고, 그 조합은 여기서 **재지 않았다.** 재고 나서 옮긴다.
            .env("TASTY_HOME", &isolated_home)
            // **부모 세션의 `TASTY_*` 를 끊는다.** 이 값들이 들어오면 자식은 자기가
            // 다른 tasty 안에서 도는 CLI 라고 판단해 **GUI 를 안 띄우고 help 를 찍고
            // 종료한다**(실측: 지우면 같은 바이너리가 port file 을 쓴다). 형제 하네스
            // 둘(`tests/common`·`tests/webhook_common`)은 격리 HOME·TASTY_HOME 으로
            // 이 경로를 이미 막고 있고, 이 하네스만 안 막고 있었다.
            //
            // 이 한 줄이 없을 때 나오는 문구는 원인을 안 가리킨다 —
            // "tasty GUI failed to write the port file" 은 디스플레이나 GPU 를
            // 의심하게 만든다. 그런데 이 스위트를 돌리라고 지시하는 문서
            // (`docs/ai-verification/`)의 독자가 바로 **tasty 안에서 도는 에이전트**라,
            // 지시받은 대로 돌린 사람이 100% 이 실패를 본다. 그래서 이건
            // 하네스의 편의가 아니라 그 문서가 성립하기 위한 조건이다.
            .env_remove("TASTY_PARENT_HOME")
            .env_remove("TASTY_SURFACE_ID")
            .env_remove("TASTY_AGENT_ID")
            .env_remove("TASTY_SESSION_TOKEN")
            .stderr(std::process::Stdio::piped());

        // 자식의 OS 열기(브라우저 · 파일 관리자)는 실행자의 데스크톱에 닿는다 — 기록만 하게 한다.
        spawn_diag::apply_os_open_record(&mut command, &isolated_home);
        // 이 스위트가 번들 plugin 을 안 부르면 빈 번들 루트를 준다 — 형제 하네스 둘이
        // 이미 하는 것이고, 이 하네스만 안 하고 있었다. 안 하면 부팅마다 격리 홈에
        // 번들 전량(debug 45 파일 ≈ 1.1 GB)을 복사한다. 명부·판정은 `spawn_diag` 한 곳이다.
        spawn_diag::apply_bundle_opt_in(&mut command);

        // 두 겹으로 회수한다. 형제 하네스 둘은 이미 가진 것이고 이 하네스만 둘 다 없었다.
        //   ① `spawn_child` — 이 테스트 바이너리가 즉사하면(배경 작업이 끊기거나 사람이
        //      회차를 멈추면) 커널이 창을 대신 죽인다(Linux). `Drop`·atexit 은 그때 안 돈다.
        //   ② `ChildReaper` — 핸들이 서기 전의 패닉(포트 파일 상한 초과 · 창 대기 ·
        //      입력 장치 생성 실패)에서 창을 죽이고 거둔다. 이것이 없던 동안 상한 초과
        //      갈래는 창을 **살려 둔 채** 패닉했고, 뒤늦게 뜬 창은 시험보다 오래 살았다.
        let mut process = spawn_diag::ChildReaper::new(
            spawn_diag::spawn_child(command).expect("failed to spawn tasty GUI"),
        );

        // stderr 를 링에 담고 마지막 줄의 시각을 남긴다 — `tests/common`·`tests/webhook_common`
        // 과 같은 형태다. 이 하네스만 **셋 다 없었다**: 꼬리도, 죽은 자식 판정도, 느림/멈춤
        // 구분도. 그래서 GUI 부팅이 실패하면 "15 초 안에 port file 이 안 나왔다" 한 줄이
        // 전부였고, 디스플레이 부재처럼 **즉사하는** 흔한 실패까지 그 문장을 썼다.
        //
        // 부팅 성공 뒤에는 인스턴스에 보관하여 이후 실패 진단에도 쓴다. 전용 인스턴스의
        // `Drop` 은 자식 kill → wait 뒤 배출 스레드를 join 한다. 공유 `static` 인스턴스는
        // Drop되지 않으며, 기존 atexit 정리 경로는 배출 스레드를 join하지 않는다.
        let mut stderr = StderrCapture::start(process.child().stderr.take(), STDERR_TAIL_LINES);

        // Wait for port file (IPC ready)
        let start = Instant::now();
        let port = loop {
            if start.elapsed() > GUI_SPAWN_PORT_TIMEOUT {
                // `last_line_age()` 가 메서드라 가드가 `panic!` 의 임시 범위에 안 들어간다 —
                // 예전에는 여기서 오염된 Mutex 가 배출 스레드를 죽여 **이후 실패의 stderr
                // tail 이 조용히 사라졌다.**
                panic!(
                    "{}",
                    spawn_diag::spawn_timeout_message(
                        "tasty GUI failed to write the port file",
                        GUI_SPAWN_PORT_TIMEOUT,
                        stderr.tail_lines(),
                        &stderr.tail(),
                        stderr.last_line_age(),
                    )
                );
            }
            if let Ok(content) = std::fs::read_to_string(&port_file)
                && let Ok(port) = content.trim().parse::<u16>()
            {
                break port;
            }
            // 자식이 이미 죽었으면 상한을 기다리지 않는다. GUI 부팅 실패는 대부분 즉사라
            // (디스플레이 부재·GPU 초기화 실패) 이 확인 하나가 15 초를 통째로 아낀다.
            if let Ok(Some(status)) = process.child().try_wait() {
                panic!(
                    "{}",
                    spawn_diag::early_exit_message(
                        &status.to_string(),
                        stderr.tail_lines(),
                        // ★ 여기서만 `tail()` 이 아니라 이것을 쓴다 — 자식이 즉사하면
                        // `try_wait()` 가 배출 스레드를 이겨 링이 비어 있고, 진단이
                        // "볼 것이 없다" 로 나간다(그 메서드의 실측 참조).
                        &stderr.tail_after_exit(spawn_diag::STDERR_SETTLE_BUDGET),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };

        // Wait for the window to appear
        #[cfg(target_os = "windows")]
        let hwnd = Self::wait_for_window("Tasty", Duration::from_secs(15));

        // Let the window fully initialize (GPU, terminal, etc.)
        std::thread::sleep(Duration::from_millis(1500));

        let enigo = Enigo::new(&EnigoSettings::default()).expect("failed to create enigo instance");

        let instance = Self {
            process: process.release(),
            stderr,
            port,
            port_file,
            isolated_home,
            enigo,
            #[cfg(target_os = "windows")]
            hwnd,
        };

        // Focus the window
        instance.focus();
        std::thread::sleep(Duration::from_millis(300));

        instance
    }

    /// Get the child process ID.
    pub fn process_id(&self) -> u32 {
        self.process.id()
    }

    /// Focus the tasty window.
    pub fn focus(&self) {
        #[cfg(target_os = "windows")]
        // SAFETY: self.hwnd는 spawn 시 FindWindowW로 찾은 활성 윈도우 핸들.
        // 본 instance가 살아있는 동안 valid (Drop이 process 종료를 보장).
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(self.hwnd);
        }
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
            let pid = self.process.id() as i32;
            if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
                app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
            }
        }
        #[cfg(target_os = "linux")]
        self.focus_x11();
        std::thread::sleep(Duration::from_millis(100));
    }

    /// X11 에서 이 인스턴스의 창에 입력 포커스를 준다.
    ///
    /// `enigo` 는 *그 순간 OS 포커스를 가진 무엇* 에 키를 넣는다. WM 이 없는 Xvfb 에는
    /// 포커스를 옮겨 주는 주체가 아예 없으므로, 여기서 안 주면 키 자극이 이 창에
    /// 도달했는지 자체가 안 정해진다. `windowactivate` 는 WM 에 요청하는 것이라 WM
    /// 없는 디스플레이에서 실패하므로 `windowfocus` 를 쓴다.
    ///
    /// 창 고르기: 한 프로세스가 여러 X 창을 가질 수 있어(작은 보조 창이 섞인다) pid 로
    /// 찾은 것 중 **넓이가 가장 큰 것**을 고른다.
    ///
    /// ★ 단, **모달이 떠 있으면 그 창이 먼저다.** "가장 큰 창" 규칙은 모달을 원리적으로
    /// 못 고른다 — 모달은 main 창보다 작다. 그 상태로 키를 넣으면 main 이 받는데, main 은
    /// 모달이 떠 있는 동안 `KeyboardInput` 을 통째로 끊는다(`src/view/main.rs` 의 모달
    /// 분기) ⇒ 자극이 **아무 데도 안 닿는다.**
    ///
    /// 그 증상은 "제품이 안 닫는다" 처럼 보이지만 재고 있는 것은 하네스다. 실측
    /// (2026-09-07, WM 없는 Xvfb): 설정 창을 `Ctrl+,` · `Escape` 로 닫는 시험 다섯이 전부
    /// 3 초 타임아웃이었고, 그때 `ui.state` 는 `modal_open: true` ·
    /// `active_modal_kind: Some(Settings)` 였다 — 창은 떠 있었고 키만 못 갔다.
    ///
    /// winit 의 X11 `WindowId` 는 X window id 그대로라 `active_modal_id` 를 그대로 쓴다.
    ///
    /// 예외 하나: **본창이 키를 받아야 하는 시험**은 모달이 떠 있어도 본창을 골라야 한다
    /// (`focus_main_window_x11`). 그쪽은 아래 "가장 큰 창" 규칙을 그대로 쓴다.
    #[cfg(target_os = "linux")]
    fn focus_x11(&self) {
        use std::process::Command;
        if let Some(modal) =
            self.call("ui.state", serde_json::json!({}))["active_modal_id"].as_u64()
        {
            let out = Command::new("xdotool")
                .args(["windowfocus", &modal.to_string()])
                .output()
                .expect("xdotool windowfocus 를 못 돌렸다");
            assert!(
                out.status.success(),
                "모달 창 {modal} 에 포커스를 못 줬다 — 이대로 키를 넣으면 main 이 받고 \
                 main 은 모달 중에 키를 끊는다. 자극이 아무 데도 안 닿는다.\nxdotool stderr: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            std::thread::sleep(Duration::from_millis(50));
            return;
        }
        self.focus_main_window_x11();
    }

    /// 모달을 **무시하고** 본창에 포커스를 준다 — pid 로 찾은 X 창 중 가장 큰 것.
    ///
    /// 위 `focus_x11` 의 기본 갈래이자, "모달이 떠 있는 동안 본창이 키를 끊는가" 를 재는
    /// 시험이 직접 부르는 손이다. 그 시험은 키가 **본창에 도착해야** 성립한다 — 모달이
    /// 받아 버리면 재는 것이 "본창의 게이트" 가 아니라 "모달이 포커스를 가져갔다" 가 된다.
    #[cfg(target_os = "linux")]
    fn focus_main_window_x11(&self) {
        use std::process::Command;
        let pid = self.process.id().to_string();
        // 못 하면 **크게** 죽는다. 조용히 넘어가면 뒤따르는 키 단정이 전부 "자극이
        // 도착했는가" 를 안 정한 채 색을 내고, 그 색은 제품이 아니라 하네스를 잰다 —
        // 이 분기가 생긴 이유가 정확히 그 사고다.
        let found = Command::new("xdotool")
            .args(["search", "--pid", &pid])
            .output()
            .expect("xdotool 을 못 돌렸다 — Linux gui 스위트는 창 포커스를 이것으로 준다");
        let mut best: Option<(u64, String)> = None;
        for wid in String::from_utf8_lossy(&found.stdout).lines() {
            let wid = wid.trim();
            if wid.is_empty() {
                continue;
            }
            let Ok(geom) = Command::new("xdotool")
                .args(["getwindowgeometry", wid])
                .output()
            else {
                continue;
            };
            let text = String::from_utf8_lossy(&geom.stdout);
            let Some(dims) = text.split("Geometry:").nth(1) else {
                continue;
            };
            let dims = dims.split_whitespace().next().unwrap_or("");
            let mut parts = dims.split('x');
            let (Some(w), Some(h)) = (parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(w), Ok(h)) = (w.trim().parse::<u64>(), h.trim().parse::<u64>()) else {
                continue;
            };
            let area = w * h;
            if best.as_ref().is_none_or(|(a, _)| area > *a) {
                best = Some((area, wid.to_string()));
            }
        }
        let (_, wid) = best.unwrap_or_else(|| panic!("pid {pid} 의 X 창을 못 찾았다"));
        // `.output()` 인 이유: `.status()` 는 xdotool 의 stderr(창이 없다 · 디스플레이를 못
        // 연다)를 시험 포착 밖으로 흘려보내 실패 문구에 종료 코드만 남긴다.
        let out = Command::new("xdotool")
            .args(["windowfocus", &wid])
            .output()
            .expect("xdotool windowfocus 를 못 돌렸다");
        assert!(
            out.status.success(),
            "xdotool windowfocus {wid} 실패: {}\nxdotool stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    /// Send a JSON-RPC request and return the result.
    pub fn call(&self, method: &str, params: Value) -> Value {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", self.port))
            .unwrap_or_else(|e| self.fail(format_args!("failed to connect for '{method}': {e}")));
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let mut msg = serde_json::to_string(&request).unwrap();
        msg.push('\n');
        stream
            .write_all(msg.as_bytes())
            .unwrap_or_else(|e| self.fail(format_args!("failed to send '{method}': {e}")));

        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .unwrap_or_else(|e| self.fail(format_args!("failed to read '{method}': {e}")));

        let resp: Value = serde_json::from_str(&line).unwrap_or_else(|e| {
            self.fail(format_args!("invalid JSON response for '{method}': {e}"))
        });
        if let Some(error) = resp.get("error") {
            self.fail(format_args!("IPC error for '{method}': {error}"));
        }
        resp.get("result").cloned().unwrap_or(Value::Null)
    }

    /// 살아 있는 자식을 join 하지 않고, 지금까지 수집한 stderr 를 실패에 붙인다.
    fn fail(&self, message: impl std::fmt::Display) -> ! {
        panic!(
            "{message}\n--- stderr (last {} lines, captured so far) ---\n{}",
            self.stderr.tail_lines(),
            self.stderr.tail(),
        );
    }

    /// Query the UI overlay state.
    pub fn ui_state(&self) -> UiState {
        let result = self.call("ui.state", serde_json::json!({}));
        UiState {
            settings_open_requested: result["settings_open_requested"].as_bool().unwrap_or(false),
            keyboard_shortcuts_gated: result["keyboard_shortcuts_gated"]
                .as_bool()
                .unwrap_or(false),
            modal_open: result["modal_open"].as_bool().unwrap_or(false),
            active_modal_id: result["active_modal_id"].as_u64(),
            active_modal_kind: result["active_modal_kind"].as_str().map(ModalKind::parse),
            gate_terms: GateTerms {
                fullscreen_stage_active: result["gate_fullscreen_stage_active"]
                    .as_bool()
                    .unwrap_or(false),
                settings_open_requested: result["gate_settings_open_requested"]
                    .as_bool()
                    .unwrap_or(false),
                input_dialog_open: result["gate_input_dialog_open"].as_bool().unwrap_or(false),
                host_popup_focused: result["gate_host_popup_focused"].as_bool().unwrap_or(false),
                plugin_popup_open: result["gate_plugin_popup_open"].as_bool().unwrap_or(false),
            },
            notification_panel_open: result["notification_panel_open"].as_bool().unwrap_or(false),
            workspace_count: result["workspace_count"].as_u64().unwrap_or(0) as usize,
            active_workspace: result["active_workspace"].as_u64().unwrap_or(0) as usize,
            pane_count: result["pane_count"].as_u64().unwrap_or(0) as usize,
            tab_count: result["tab_count"].as_u64().unwrap_or(0) as usize,
            active_tab: result["active_tab"].as_u64().unwrap_or(0) as usize,
        }
    }

    /// Create a new workspace via IPC. Returns the workspace index (0-based).
    #[allow(dead_code)]
    pub fn create_workspace(&self, name: &str) -> usize {
        let _result = self.call("workspace.create", serde_json::json!({ "name": name }));
        let state = self.ui_state();
        state.workspace_count - 1
    }

    /// Get the first surface ID from surface.list.
    #[allow(dead_code)]
    pub fn first_surface_id(&self) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    /// Get the first pane ID from pane.list.
    #[allow(dead_code)]
    pub fn first_pane_id(&self) -> u64 {
        let panes = self.call("pane.list", serde_json::json!({}));
        panes.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    // --- Mouse-routing injection helpers (debug build only) ---
    // 실제 데스크톱 마우스를 뺏지 않고, IPC 로 winit 레벨 포인터 이벤트를 주입해
    // `handle_mouse_input` 라우팅을 그대로 태운다 (원칙 1·3: debug 격리).

    /// active workspace 의 id 를 workspace.list(active 플래그) 로 조회.
    #[allow(dead_code)]
    pub fn active_workspace_id(&self) -> u64 {
        let list = self.call("workspace.list", serde_json::json!({}));
        for ws in list.as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
            if ws["active"].as_bool().unwrap_or(false) {
                return ws["id"].as_u64().unwrap();
            }
        }
        panic!("no active workspace in workspace.list: {list}");
    }

    /// 주어진 workspace_id 에 속한 surface id 목록 (surface.list 필터).
    #[allow(dead_code)]
    pub fn surface_ids_in_workspace(&self, ws_id: u64) -> Vec<u64> {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces
            .as_array()
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|s| s["workspace_id"].as_u64() == Some(ws_id))
            .filter_map(|s| s["id"].as_u64())
            .collect()
    }

    /// winit 레벨 포인터 이벤트 주입. `event_type` ∈ move/press/release/scroll,
    /// `button` 0=left/1=middle/2=right, (fx,fy) surface-local 정규화 [0,1].
    #[allow(dead_code)]
    pub fn inject_mouse(&self, surface_id: u64, fx: f32, fy: f32, event_type: &str, button: u8) {
        self.call(
            "debug.inject_window_mouse",
            serde_json::json!({
                "surface_id": surface_id,
                "fx": fx,
                "fy": fy,
                "event_type": event_type,
                "button": button,
            }),
        );
    }

    /// egui 입력 큐 레벨 포인터 주입 (window 정규화 [0,1] 좌표). `debug.inject_window_mouse`
    /// (winit 경로)와 달리 egui 이벤트를 직접 넣어 egui 위젯(explorer 그리드/컨텍스트 메뉴
    /// 등)의 `secondary_clicked` 라우팅을 그대로 탄다. `event_type` ∈ move/press/release,
    /// `button` 0=left/1=middle/2=right.
    #[allow(dead_code)]
    pub fn inject_egui_mouse(
        &self,
        surface_id: u64,
        fx: f32,
        fy: f32,
        event_type: &str,
        button: u8,
    ) {
        self.call(
            "debug.inject_egui_mouse",
            serde_json::json!({
                "surface_id": surface_id,
                "fx": fx,
                "fy": fy,
                "event_type": event_type,
                "button": button,
            }),
        );
    }

    /// 로컬 텍스트 선택 상태 dump (read-only debug IPC).
    #[allow(dead_code)]
    pub fn debug_selection(&self) -> Value {
        self.call("debug.selection", serde_json::json!({}))
    }

    /// 대기 중 컨텍스트 메뉴 dump (read-only debug IPC, 주입 포획본 관찰).
    #[allow(dead_code)]
    pub fn debug_pending_menu(&self) -> Value {
        self.call("debug.pending_menu", serde_json::json!({}))
    }

    /// 현재 포커스된 surface id (없으면 None).
    #[allow(dead_code)]
    pub fn debug_focused_surface(&self) -> Option<u64> {
        let v = self.call("debug.focused_surface", serde_json::json!({}));
        v["surface_id"].as_u64()
    }

    /// 활성 모달에 **창 닫기 요청**을 보낸다 — 사용자가 창 닫기 버튼을 누른 것의 재현.
    ///
    /// 키보드로 설정 창을 닫을 수 없어서 있는 손이다. `SettingsView::handle_event` 에
    /// Escape 분기가 없고, `open_settings_modal` 은 이미 열려 있으면 그냥 return 해서
    /// `Ctrl+,` 도 토글이 아니다. 남은 실재 경로는 창 닫기 요청과 egui 액션 둘인데,
    /// WM 없는 Xvfb 에는 앞의 것을 보낼 손이 없다(`xdotool windowclose` 는
    /// `XDestroyWindow` 를 불러 winit 을 패닉시키고, `wmctrl -i -c` 는 WM 이 없으면
    /// 아무도 처리하지 않는다). 그래서 `debug.modal.close_request` 로 보낸다.
    ///
    /// **닫을 모달이 없었으면 패닉한다.** 그 둘을 같은 모양으로 두면 "닫혔다" 를
    /// 확인하는 자리가 사라지고, cleanup 이 조용히 아무 일도 안 한 회차가 초록이 된다.
    pub fn close_active_modal(&self) {
        let v = self.call("debug.modal.close_request", serde_json::json!({}));
        assert_eq!(
            v["closed"].as_bool(),
            Some(true),
            "닫을 모달이 없었다 — 이 자리는 모달이 떠 있다고 보고 부른 곳이다. 응답: {v}"
        );
    }

    /// 모달이 떠 있어도 **본창**에 텍스트를 넣는다.
    ///
    /// `type_text` 는 모달이 있으면 모달에 포커스를 준다(그쪽이 키를 받아야 할 창이라서).
    /// 그런데 "모달이 떠 있는 동안 본창이 `KeyboardInput` 을 끊는가"(`src/view/main.rs` 의
    /// 모달 분기)를 재려면 키가 **본창에 도착해야 한다.** 모달이 받아 버리면 재는 것이
    /// 그 게이트가 아니라 "모달이 포커스를 가져갔다" 가 된다 — 두 초록은 같은 모양이다.
    ///
    /// ☆ Linux 전용 구분이다. Windows·macOS 의 `focus()` 는 앱/창 활성화라 어느 창이
    /// 받는지를 여기서 정하지 못한다 — 그 두 조합에서는 이 손이 `type_text` 와 같고,
    /// 그 사실은 **재지 않았다.**
    pub fn type_text_into_main_window(&mut self, text: &str) {
        #[cfg(target_os = "linux")]
        self.focus_main_window_x11();
        #[cfg(not(target_os = "linux"))]
        self.focus();
        std::thread::sleep(Duration::from_millis(150));
        self.enigo.text(text).expect("text input failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    // --- Input simulation helpers ---

    /// Press a key combination (e.g., Ctrl+Comma).
    pub fn press_key(&mut self, key: Key) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .key(key, Direction::Click)
            .expect("key press failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Press Ctrl + a key.
    pub fn press_ctrl(&mut self, key: Key) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .key(Key::Control, Direction::Press)
            .expect("ctrl press failed");
        std::thread::sleep(Duration::from_millis(30));
        self.enigo
            .key(key, Direction::Click)
            .expect("key click failed");
        std::thread::sleep(Duration::from_millis(30));
        self.enigo
            .key(Key::Control, Direction::Release)
            .expect("ctrl release failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Press Ctrl+Shift + a key.
    pub fn press_ctrl_shift(&mut self, key: Key) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .key(Key::Control, Direction::Press)
            .expect("ctrl press failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(Key::Shift, Direction::Press)
            .expect("shift press failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(key, Direction::Click)
            .expect("key click failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(Key::Shift, Direction::Release)
            .expect("shift release failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(Key::Control, Direction::Release)
            .expect("ctrl release failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Press Alt+Shift + a key.
    ///
    /// **소문자 `key` 를 넘겨라.** 대문자 char 로 Shift 를 대신할 수 없다 — `enigo` 의
    /// `Key::Unicode` 는 keysym 을 **레벨 0 에서만** 찾고(못 찾으면 미사용 keycode 에
    /// 새로 바인딩한다) Shift 를 합성하지 않는다. 그래서 `press_alt(Unicode('W'))` 는
    /// Shift 없는 'W' 이벤트가 되고, 수정자를 정확히 비교하는 매처
    /// (`crates/tasty-key-match/src/lib.rs`)에서 `alt+shift+w` 가 아니라
    /// **`alt+w` 에 닿는다.** 이 헬퍼는 그 통로를 구조로 막는다: 대소문자는 수정자가 아니다.
    pub fn press_alt_shift(&mut self, key: Key) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .key(Key::Alt, Direction::Press)
            .expect("alt press failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(Key::Shift, Direction::Press)
            .expect("shift press failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(key, Direction::Click)
            .expect("key click failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(Key::Shift, Direction::Release)
            .expect("shift release failed");
        std::thread::sleep(Duration::from_millis(20));
        self.enigo
            .key(Key::Alt, Direction::Release)
            .expect("alt release failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Press Alt + a key.
    pub fn press_alt(&mut self, key: Key) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .key(Key::Alt, Direction::Press)
            .expect("alt press failed");
        std::thread::sleep(Duration::from_millis(30));
        self.enigo
            .key(key, Direction::Click)
            .expect("key click failed");
        std::thread::sleep(Duration::from_millis(30));
        self.enigo
            .key(Key::Alt, Direction::Release)
            .expect("alt release failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Type text into the focused terminal.
    pub fn type_text(&mut self, text: &str) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo.text(text).expect("text input failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Click at a position relative to the window's client area.
    #[allow(dead_code)]
    pub fn click_at(&mut self, x: i32, y: i32) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));

        // Convert window-relative coordinates to screen coordinates
        let (screen_x, screen_y) = self.client_to_screen(x, y);

        self.enigo
            .move_mouse(screen_x, screen_y, Coordinate::Abs)
            .expect("mouse move failed");
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .button(enigo::Button::Left, Direction::Click)
            .expect("mouse click failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Get window client area size (width, height) in physical pixels.
    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    pub fn client_size(&self) -> (i32, i32) {
        let mut rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: self.hwnd valid (위 focus 주석과 동일). GetClientRect는 thread-safe.
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    /// Get window client area size (width, height) in physical pixels.
    ///
    /// Windows 는 `GetClientRect(HWND)` 로 직접 읽지만, macOS/Linux 에서는 테스트
    /// 프로세스가 tasty 의 winit Window 핸들을 갖지 않는다(별도 프로세스 + IPC 구조).
    /// tasty 의 `debug.info` IPC 가 노출하는 viewport(= `window.inner_size()`, 물리 픽셀
    /// client area)를 조회해 Windows 와 동일 의미의 값을 크로스플랫폼으로 얻는다.
    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub fn client_size(&self) -> (i32, i32) {
        let info = self.call("debug.info", serde_json::json!({}));
        let w = info["viewport_width"].as_i64().unwrap_or(0) as i32;
        let h = info["viewport_height"].as_i64().unwrap_or(0) as i32;
        (w, h)
    }

    /// Convert client-relative (x, y) to screen coordinates.
    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    fn client_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
        let mut window_rect = windows::Win32::Foundation::RECT::default();
        let mut client_rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: self.hwnd valid. GetWindowRect/GetClientRect는 thread-safe Win32 호출.
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut window_rect);
            let _ = GetClientRect(self.hwnd, &mut client_rect);
        }
        // The client area offset from window top-left
        let border_x =
            ((window_rect.right - window_rect.left) - (client_rect.right - client_rect.left)) / 2;
        let title_height = (window_rect.bottom - window_rect.top)
            - (client_rect.bottom - client_rect.top)
            - border_x;

        (
            window_rect.left + border_x + x,
            window_rect.top + title_height + y,
        )
    }

    #[cfg(not(target_os = "windows"))]
    fn client_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
        // Fallback: assume no offset (non-Windows)
        (x, y)
    }

    /// Wait until a condition on ui_state is met, or panic after timeout.
    pub fn wait_for_ui<F: Fn(&UiState) -> bool>(
        &self,
        description: &str,
        timeout: Duration,
        condition: F,
    ) -> UiState {
        let start = Instant::now();
        loop {
            let state = self.ui_state();
            if condition(&state) {
                return state;
            }
            if start.elapsed() > timeout {
                self.fail(format_args!(
                    "Timeout waiting for UI condition: {}. Current state: {:?}",
                    description, state
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Shutdown the instance gracefully via IPC.
    #[allow(dead_code)]
    pub fn shutdown(&self) {
        let _ = self.call("system.shutdown", serde_json::json!({}));
    }

    // --- Windows-specific helpers ---

    #[cfg(target_os = "windows")]
    fn wait_for_window(title: &str, timeout: Duration) -> HWND {
        let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let start = Instant::now();
        loop {
            if start.elapsed() > timeout {
                panic!("Window '{}' did not appear within {:?}", title, timeout);
            }
            // SAFETY: wide_title은 null-terminated UTF-16 local Vec, 호출 동안 살아있음.
            let hwnd = unsafe { FindWindowW(None, windows::core::PCWSTR(wide_title.as_ptr())) };
            match hwnd {
                Ok(h) if !h.is_invalid() => return h,
                _ => {
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    }
}

impl Drop for GuiTestInstance {
    fn drop(&mut self) {
        // On Windows, kill the entire process tree (tasty + child shells).
        // process.kill() only kills the parent, leaving orphan shell processes.
        #[cfg(target_os = "windows")]
        {
            let pid = self.process.id();
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = self.process.kill();
        }
        let _ = self.process.wait();
        // 파이프 EOF 는 자식 종료 뒤 온다. 살아 있는 자식보다 먼저 join 하면 멈춘다.
        self.stderr.join();
        let _ = std::fs::remove_file(&self.port_file);
        // reason: 정리 실패가 시험 판정을 바꾸지 않는다 — 이미 끝난 인스턴스의 임시
        // 디렉터리이고, 지우다 실패해도 `/tmp` 에 남을 뿐이라 다음 회차는 자기 `unique`
        // 로 새 경로를 쓴다. 여기서 패닉하면 진짜 실패 원인을 정리 오류가 덮는다.
        let _ = std::fs::remove_dir_all(&self.isolated_home);
    }
}

/// `ui.state` 가 내는 모달 종류. **열거다** — 문자열로 비교하면 오타가 "그 모달이
/// 아니다" 와 같은 모양이 되어 시험이 조용히 통과한다.
///
/// 본체 쪽 정의는 `src/state.rs` 의 `ModalKind` 이고, IPC 를 건널 때만 납작해진다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalKind {
    Settings,
    Plugins,
    Quit,
    /// 본체가 새 모달을 추가했는데 이 열거를 안 늘린 경우.
    ///
    /// ★ `None` 과 **섞지 않는다.** 섞으면 "모달이 없다" 와 "모르는 모달이 떠 있다" 가
    /// 같은 모양이 되고, 그러면 새 모달이 설정 창 자리를 차지해도 시험이 못 본다.
    Unknown(String),
}

impl ModalKind {
    fn parse(raw: &str) -> Self {
        match raw {
            "settings" => Self::Settings,
            "plugins" => Self::Plugins,
            "quit" => Self::Quit,
            other => Self::Unknown(other.to_string()),
        }
    }
}

/// Snapshot of UI overlay state, queried via IPC.
/// 단축키 게이트를 이루는 다섯 항 — **거짓도 값으로 담는다.**
///
/// 참인 것만 나열하면 "그 항이 거짓이라 빠졌다" 와 "보고가 그 항을 아예 모른다" 가
/// 같은 모양이 된다. 다섯 칸이 항상 차 있으면 그 둘이 갈린다.
///
/// 이름은 술어 `state::keyboard_overlay_open` 의 매개변수 그대로다(무대 항만 술어 밖).
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
// reason: 실패 메시지(`Debug`)로 읽히는 진단 구조다. 코드가 분기에 쓰지 않는다.
pub struct GateTerms {
    pub fullscreen_stage_active: bool,
    pub settings_open_requested: bool,
    pub input_dialog_open: bool,
    pub host_popup_focused: bool,
    pub plugin_popup_open: bool,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub settings_open_requested: bool,
    /// 단축키 경로가 **막혀 있는가**. `handle_keyboard_input` 은 오버레이가 열려 있으면
    /// 단축키를 아예 안 소비하는데, 그 게이트를 여는 조건이 넷이고 `settings_open_requested` 은
    /// 그중 하나다. 이 필드가 없으면 실패 메시지가 "안 먹었다" 까지만 말하고 **왜인지를
    /// 못 말한다** — 도착 카나리아가 죽은 회차가 그 자리였다.
    #[allow(dead_code)]
    // reason: 실패 메시지(`Debug`)로 읽히는 진단 필드다. 코드가 분기에 쓰지 않는다.
    pub keyboard_shortcuts_gated: bool,
    /// 그중 **무엇이** 막았는가. 위 bool 은 다섯 항을 `||` 로 뭉치므로 "막혔다" 까지만
    /// 말한다 — 실측으로 한 회차가 21 건 연속 `true` 였는데 그 값만으로는 다섯 중 무엇이
    /// 열린 채 남았는지 못 골랐다.
    #[allow(dead_code)]
    // reason: 실패 메시지(`Debug`)로 읽히는 진단 필드다. 코드가 분기에 쓰지 않는다.
    pub gate_terms: GateTerms,
    /// 모달이 **실제로 떠 있는가**. `settings_open_requested` 은 열기 요청 래치라 다음 프레임에
    /// 지워지므로, 모달이 화면에 있는 동안 그 값은 거짓이다 — 그 둘을 가르려고 둔다.
    #[allow(dead_code)]
    // reason: 실패 메시지(`Debug`)로 읽히는 진단 필드다. 코드가 분기에 쓰지 않는다.
    pub modal_open: bool,
    #[allow(dead_code)]
    // reason: 위와 같다 — 어느 모달인지 구별할 때만 쓴다.
    pub active_modal_id: Option<u64>,
    /// **어느** 모달인가. `modal_open` 은 "무언가 떠 있다" 까지만 말하므로, 그 값만으로
    /// 설정 창을 기다리면 plugins·quit 창이 떠도 같은 모양이 된다 — 시험이 자기가 안 연
    /// 창을 보고 통과한다.
    pub active_modal_kind: Option<ModalKind>,
    pub notification_panel_open: bool,
    pub workspace_count: usize,
    pub active_workspace: usize,
    pub pane_count: usize,
    pub tab_count: usize,
    /// 포커스된 pane 의 활성 탭 인덱스. **`tab_count` 로는 전환이 안 보인다** — 전환해도
    /// 수가 그대로라, 전환을 재려면 이 축이 필요하다.
    pub active_tab: usize,
}

impl UiState {
    /// 설정 모달이 **실제로 화면에 있는가.**
    ///
    /// ★ `settings_open_requested` 로 이것을 묻지 마라. 그 값은 "열기 요청이 이 프레임에
    /// 걸려 있다" 는 래치이고, 사이드바 경로가 세운 것을 다음 redraw 의
    /// `dispatch_pending_modal_opens` 가 **같은 패스에서 지운다.** 게다가 이 스위트가 쓰는
    /// `Ctrl+,` 는 `AppEvent` 직행 경로라 그 필드를 **아예 안 건드린다** — 그것을 기다린
    /// 일곱 시험은 낡은 채널을 기다린 게 아니라 **없는 것**을 기다렸다.
    ///
    /// 두 값을 함께 보는 이유: `modal_open` 만 보면 plugins·quit 창도 같은 모양이고,
    /// 종류만 보면 `modal_open` 이 거짓인데 종류가 남은 조합(있으면 안 되지만, 시험이
    /// 그 불변식에 기대지 않는 편이 낫다)을 못 가른다.
    pub fn settings_modal_is_up(&self) -> bool {
        self.modal_open && self.active_modal_kind == Some(ModalKind::Settings)
    }
}
