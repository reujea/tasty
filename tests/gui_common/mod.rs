//! GUI 인스턴스를 공유하고 워크스페이스로 시험을 격리한다.
//! 실제 데스크톱 입력을 주입하므로 Mutex로 시나리오를 직렬화한다.

// 시험 하네스의 플랫폼 API 호출에 unsafe가 필요하다.
#![cfg_attr(test, allow(clippy::multiple_unsafe_ops_per_block))]
// 시험 본문은 제품 코드의 let _ 사유 주석 정책에서 제외된다.
#![allow(clippy::let_underscore_must_use)]
// 여러 테스트 바이너리가 서로 다른 헬퍼만 사용하므로 미사용 함수도 제공한다.
#![allow(dead_code)]

#[path = "../spawn_diag/mod.rs"]
mod spawn_diag;

#[cfg(all(test, target_os = "linux"))]
mod stderr_tests;

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

static SHARED_INSTANCE: OnceLock<Mutex<GuiTestInstance>> = OnceLock::new();
static CLEANUP_PID: AtomicU32 = AtomicU32::new(0);
/// static 인스턴스는 종료 시 Drop되지 않으므로 atexit에서 격리 홈을 별도로 지운다.
static CLEANUP_HOME: OnceLock<PathBuf> = OnceLock::new();
/// static 인스턴스의 포트 파일도 atexit에서 정리한다. 남으면 종료된 인스턴스로 잘못 조회할 수 있다.
static CLEANUP_PORT: OnceLock<PathBuf> = OnceLock::new();
/// 첫 spawn이 실패한 뒤 다른 테스트가 새 프로세스를 반복 생성하지 못하게 한다.
static SPAWN_LATCH: spawn_diag::SpawnOnceLatch = spawn_diag::SpawnOnceLatch::new();

pub fn shared() -> std::sync::MutexGuard<'static, GuiTestInstance> {
    let guard = SHARED_INSTANCE.get_or_init(|| {
        // OnceLock 초기화가 panic하면 다음 호출이 재시도할 수 있어 spawn 전에 래치를 설정한다.
        SPAWN_LATCH.entering("gui 공유 인스턴스");
        let inst = GuiTestInstance::spawn();
        SPAWN_LATCH.succeeded();
        CLEANUP_PID.store(inst.process_id(), Ordering::Relaxed);
        // reason: 최초 초기화에서 한 번 저장한다. 이미 값이 있으면 기존 경로를 유지한다.
        let _ = CLEANUP_HOME.set(inst.isolated_home.clone());
        // reason: 이미 저장된 포트 경로가 있으면 유지한다.
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
                // reason: atexit 정리 실패로 panic하지 않는다. 다음 시험은 별도 경로를 사용한다.
                let _ = std::fs::remove_dir_all(home);
            }
            if let Some(port) = CLEANUP_PORT.get() {
                // reason: atexit에서 포트 파일 삭제 실패는 무시한다.
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
    // 시험 단정의 panic이 뒤의 시험까지 poison 오류로 실패시키지 않도록 락을 복구한다.
    // 자식 프로세스가 종료됐다면 각 시험의 IPC나 입력 단계에서 실패한다.
    guard
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct GuiTestInstance {
    process: Child,
    /// 부팅 뒤 진단에도 쓰며 자식 종료 후 배출 스레드를 join한다.
    stderr: StderrCapture,
    port: u16,
    port_file: PathBuf,
    /// 인스턴스 전용 홈. 일반 인스턴스는 Drop, static 인스턴스는 atexit에서 정리한다.
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

const GUI_SPAWN_PORT_TIMEOUT: Duration = Duration::from_secs(15);

impl GuiTestInstance {
    pub fn spawn() -> Self {
        Self::spawn_with_config(None)
    }

    /// 격리 홈에 `config.toml` 을 먼저 써 두고 띄운다 — 시작할 때만 읽는 설정(plugin 설정 등)을 켠 채 시험한다.
    pub fn spawn_with_config(config: Option<&str>) -> Self {
        // 시계 해상도에 의존하지 않고 PID와 프로세스 내 카운터로 임시 경로를 구별한다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let port_file = std::env::temp_dir().join(format!("tasty-gui-test-{unique}.port"));

        let isolated_home = std::env::temp_dir().join(format!("tasty-gui-test-home-{unique}"));
        if let Some(config) = config {
            std::fs::create_dir_all(&isolated_home).expect("격리 홈");
            std::fs::write(isolated_home.join("config.toml"), config).expect("config.toml 쓰기");
        }

        // debug 기록 모드로 native 메뉴를 실제로 열지 않고 IPC에서 확인한다.
        let mut command = Command::new(env!("CARGO_BIN_EXE_tasty"));
        command
            .arg("--port-file")
            .arg(port_file.to_str().unwrap())
            .env("TASTY_DEBUG_SUPPRESS_NATIVE_MENU", "1")
            // 사용자의 레이아웃·플러그인 설치와 섞이지 않도록 TASTY_HOME을 격리한다.
            // HOME은 옮기지 않는다. 설정 준비 없이 HOME만 바꾸면 셸 설정 모드에 진입할 수 있으며 이 조합은 검증하지 않았다.
            .env("TASTY_HOME", &isolated_home)
            // 부모 세션 변수가 전달되면 CLI로 인식해 GUI 대신 도움말을 출력할 수 있어 제거한다.
            .env_remove("TASTY_PARENT_HOME")
            .env_remove("TASTY_SURFACE_ID")
            .env_remove("TASTY_AGENT_ID")
            .env_remove("TASTY_SESSION_TOKEN")
            .stderr(std::process::Stdio::piped());

        // OS 열기가 기존 사용자 프로세스에 전달되지 않도록 기록 모드를 사용한다.
        spawn_diag::apply_os_open_record(&mut command, &isolated_home);
        // 번들 사용 여부와 준비는 spawn_diag의 공통 목록을 따른다.
        spawn_diag::apply_bundle_opt_in(&mut command, &isolated_home);

        // spawn_child는 Linux 부모 종료 시 자식 종료를 설정하고, ChildReaper는 초기화 중 panic에서 자식을 회수한다.
        let mut process = spawn_diag::ChildReaper::new(
            spawn_diag::spawn_child(command).expect("failed to spawn tasty GUI"),
        );

        // 부팅 이후에도 stderr를 보관한다. 일반 Drop은 자식 종료 뒤 배출 스레드를 join하지만 static의 atexit 경로는 join하지 않는다.
        let mut stderr = StderrCapture::start(process.child().stderr.take(), STDERR_TAIL_LINES);

        let start = Instant::now();
        let port = loop {
            if start.elapsed() > GUI_SPAWN_PORT_TIMEOUT {
                // panic 전에 링 잠금을 해제하도록 last_line_age 메서드에서 값을 읽는다.
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
            if let Ok(Some(status)) = process.child().try_wait() {
                panic!(
                    "{}",
                    spawn_diag::early_exit_message(
                        &status.to_string(),
                        stderr.tail_lines(),
                        // 자식 종료 확인이 stderr 배출보다 먼저 끝날 수 있어 배출 완료를 기다려 꼬리를 얻는다.
                        &stderr.tail_after_exit(spawn_diag::STDERR_SETTLE_BUDGET),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };

        #[cfg(target_os = "windows")]
        let hwnd = Self::wait_for_window("Tasty", Duration::from_secs(15));

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

        instance.focus();
        std::thread::sleep(Duration::from_millis(300));

        instance
    }

    pub fn process_id(&self) -> u32 {
        self.process.id()
    }

    /// 격리 홈 — plugin 데이터(프로필 등)를 미리 깔아야 하는 시험이 쓴다.
    #[cfg(target_os = "macos")]
    pub fn home(&self) -> &std::path::Path {
        &self.isolated_home
    }

    /// 화면 좌표(논리 pt, 주 화면 왼쪽 위 기준)를 OS 수준으로 한 번 누른다. native WebView 처럼 egui · 터미널
    /// 주입 경로가 닿지 않는 곳에 쓴다. 좌표는 `debug.window.list` 의 `inner_position` 과
    /// `debug.surface_rect` 로 계산한다.
    #[cfg(target_os = "macos")]
    pub fn click_screen_points(&mut self, x: f64, y: f64) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs)
            .expect("mouse move failed");
        std::thread::sleep(Duration::from_millis(80));
        self.enigo
            .button(enigo::Button::Left, Direction::Click)
            .expect("mouse click failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// [`Self::click_screen_points`] 의 오른쪽 단추판. native WebView 의 우클릭(`contextmenu`)을 재현한다.
    #[cfg(target_os = "macos")]
    pub fn right_click_screen_points(&mut self, x: f64, y: f64) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs)
            .expect("mouse move failed");
        std::thread::sleep(Duration::from_millis(80));
        self.enigo
            .button(enigo::Button::Right, Direction::Click)
            .expect("mouse right click failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    /// 화면 좌표(논리 pt)에서 왼쪽 단추를 누른 채 다른 좌표로 끌고 놓는다. 창 테두리 끌기처럼 OS 가 처리하는
    /// 조작을 재현한다. 중간 지점을 여러 번 지나야 창 서버가 끌기로 받는다.
    #[cfg(target_os = "macos")]
    pub fn drag_screen_points(&mut self, from: (f64, f64), to: (f64, f64)) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        let at = |p: (f64, f64)| (p.0.round() as i32, p.1.round() as i32);
        let (x0, y0) = at(from);
        self.enigo
            .move_mouse(x0, y0, Coordinate::Abs)
            .expect("mouse move failed");
        std::thread::sleep(Duration::from_millis(150));
        self.enigo
            .button(enigo::Button::Left, Direction::Press)
            .expect("mouse press failed");
        std::thread::sleep(Duration::from_millis(150));
        for k in 1..=10 {
            let f = f64::from(k) / 10.0;
            let (x, y) = at((from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f));
            self.enigo
                .move_mouse(x, y, Coordinate::Abs)
                .expect("mouse move failed");
            std::thread::sleep(Duration::from_millis(40));
        }
        std::thread::sleep(Duration::from_millis(150));
        self.enigo
            .button(enigo::Button::Left, Direction::Release)
            .expect("mouse release failed");
        std::thread::sleep(Duration::from_millis(400));
    }

    pub fn focus(&self) {
        #[cfg(target_os = "windows")]
        // SAFETY: HWND는 spawn에서 얻은 Win32 핸들 값이며 이 호출에서 메모리 포인터로 역참조하지 않는다.
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

    /// WM 없는 Xvfb에서도 입력이 도달하도록 windowfocus로 직접 포커스를 준다.
    /// 활성 모달이 있으면 그 창을, 없으면 해당 PID의 가장 넓은 창을 선택한다.
    /// 본창의 입력 차단을 검사할 때는 focus_main_window_x11로 모달 선택을 건너뛴다.
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
                "모달 창 {modal}에 포커스를 주지 못해 입력 대상을 확인할 수 없다.\nxdotool stderr: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            std::thread::sleep(Duration::from_millis(50));
            return;
        }
        self.focus_main_window_x11();
    }

    /// 모달이 떠 있을 때 본창의 입력 차단을 검사하도록 가장 넓은 창을 직접 선택한다.
    #[cfg(target_os = "linux")]
    fn focus_main_window_x11(&self) {
        use std::process::Command;
        let pid = self.process.id().to_string();
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
        // xdotool stderr를 실패 진단에 남기도록 output을 사용한다.
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

    /// 살아 있는 자식을 join하지 않고 현재까지 수집한 stderr를 실패에 붙인다.
    fn fail(&self, message: impl std::fmt::Display) -> ! {
        panic!(
            "{message}\n--- stderr (last {} lines, captured so far) ---\n{}",
            self.stderr.tail_lines(),
            self.stderr.tail(),
        );
    }

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

    #[allow(dead_code)]
    pub fn create_workspace(&self, name: &str) -> usize {
        let _result = self.call("workspace.create", serde_json::json!({ "name": name }));
        let state = self.ui_state();
        state.workspace_count - 1
    }

    #[allow(dead_code)]
    pub fn first_surface_id(&self) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    #[allow(dead_code)]
    pub fn first_pane_id(&self) -> u64 {
        let panes = self.call("pane.list", serde_json::json!({}));
        panes.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

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

    /// winit 포인터 입력. fx·fy는 서피스 내부의 0~1 좌표이고 button은 0=left, 1=middle, 2=right다.
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

    /// egui 입력 큐에 직접 넣는다. fx·fy는 창 내부의 0~1 좌표이고 button은 0=left, 1=middle, 2=right다.
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

    #[allow(dead_code)]
    pub fn debug_selection(&self) -> Value {
        self.call("debug.selection", serde_json::json!({}))
    }

    #[allow(dead_code)]
    pub fn debug_pending_menu(&self) -> Value {
        self.call("debug.pending_menu", serde_json::json!({}))
    }

    #[allow(dead_code)]
    pub fn debug_focused_surface(&self) -> Option<u64> {
        let v = self.call("debug.focused_surface", serde_json::json!({}));
        v["surface_id"].as_u64()
    }

    /// WM 없는 Xvfb에서 창 파괴 대신 winit의 닫기 요청을 재현한다. 닫을 모달이 없으면 실패한다.
    pub fn close_active_modal(&self) {
        let v = self.call("debug.modal.close_request", serde_json::json!({}));
        assert_eq!(
            v["closed"].as_bool(),
            Some(true),
            "닫을 모달이 없었다 — 이 자리는 모달이 떠 있다고 보고 부른 곳이다. 응답: {v}"
        );
    }

    /// Linux에서는 모달이 떠 있어도 본창에 입력한다. Windows·macOS의 focus는 이 두 창을 구별하지 않으며 같은 동작인지 검증하지 않았다.
    pub fn type_text_into_main_window(&mut self, text: &str) {
        #[cfg(target_os = "linux")]
        self.focus_main_window_x11();
        #[cfg(not(target_os = "linux"))]
        self.focus();
        std::thread::sleep(Duration::from_millis(150));
        self.enigo.text(text).expect("text input failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    pub fn press_key(&mut self, key: Key) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo
            .key(key, Direction::Click)
            .expect("key press failed");
        std::thread::sleep(Duration::from_millis(200));
    }

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

    /// Alt+Shift 입력은 소문자 key를 받는다. 대문자 Unicode 키만으로 Shift 수정자 입력을 대신할 수 없다.
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

    pub fn type_text(&mut self, text: &str) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));
        self.enigo.text(text).expect("text input failed");
        std::thread::sleep(Duration::from_millis(200));
    }

    #[allow(dead_code)]
    pub fn click_at(&mut self, x: i32, y: i32) {
        self.focus();
        std::thread::sleep(Duration::from_millis(50));

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

    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    pub fn client_size(&self) -> (i32, i32) {
        let mut rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: HWND는 Win32 핸들 값이고 rect는 쓰기 가능한 지역 구조체다.
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    /// macOS·Linux는 다른 프로세스의 창 핸들을 소유하지 않아 IPC로 client area의 물리 픽셀 크기를 읽는다.
    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub fn client_size(&self) -> (i32, i32) {
        let info = self.call("debug.info", serde_json::json!({}));
        let w = info["viewport_width"].as_i64().unwrap_or(0) as i32;
        let h = info["viewport_height"].as_i64().unwrap_or(0) as i32;
        (w, h)
    }

    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    fn client_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
        let mut window_rect = windows::Win32::Foundation::RECT::default();
        let mut client_rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: HWND는 Win32 핸들 값이며 출력 구조체는 호출 동안 쓰기 가능한 지역 변수다.
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut window_rect);
            let _ = GetClientRect(self.hwnd, &mut client_rect);
        }
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
        // Windows 외 플랫폼에서는 화면 좌표 오프셋을 0으로 가정한다.
        (x, y)
    }

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

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        let _ = self.call("system.shutdown", serde_json::json!({}));
    }

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
        // Windows에서는 자식 셸도 종료하도록 프로세스 트리를 대상으로 한다.
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
        // 살아 있는 자식의 stderr를 먼저 join하면 EOF를 기다리며 멈출 수 있어 종료 뒤 join한다.
        self.stderr.join();
        let _ = std::fs::remove_file(&self.port_file);
        // reason: 임시 홈 삭제 실패로 원래 시험 오류를 덮지 않는다. 다음 실행은 별도 경로를 사용한다.
        let _ = std::fs::remove_dir_all(&self.isolated_home);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalKind {
    Settings,
    Plugins,
    Quit,
    /// 알 수 없는 종류를 모달 부재와 구별한다.
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

/// 단축키 차단 원인을 진단에 표시한다. 응답에서 빠진 키는 조회 코드가 false로 읽으므로 필드 누락까지 구별하지는 못한다.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
// reason: 실패 시 Debug 출력으로 읽는 진단 구조다.
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
    #[allow(dead_code)]
    // reason: 실패 시 Debug 출력으로 읽는 진단 필드다.
    pub keyboard_shortcuts_gated: bool,
    #[allow(dead_code)]
    // reason: 실패 시 Debug 출력으로 읽는 진단 필드다.
    pub gate_terms: GateTerms,
    #[allow(dead_code)]
    // reason: 실패 시 Debug 출력으로 읽는 진단 필드다.
    pub modal_open: bool,
    #[allow(dead_code)]
    // reason: 진단에서 모달을 구별할 때 사용한다.
    pub active_modal_id: Option<u64>,
    pub active_modal_kind: Option<ModalKind>,
    pub notification_panel_open: bool,
    pub workspace_count: usize,
    pub active_workspace: usize,
    pub pane_count: usize,
    pub tab_count: usize,
    pub active_tab: usize,
}

impl UiState {
    /// 열기 요청 래치는 실제 모달의 존재와 다르다. 모달 열림과 종류를 함께 확인한다.
    pub fn settings_modal_is_up(&self) -> bool {
        self.modal_open && self.active_modal_kind == Some(ModalKind::Settings)
    }
}
