//! 실제 tasty 바이너리를 띄워 JSON-RPC로 조작하는 IPC 시험 하네스다.
//! shared는 시험 바이너리마다 인스턴스 하나를 공유하고 각 시험은 별도 workspace를 만든다.
//! 시작 설정이나 프로세스 전체 측정이 다를 때만 전용 spawn을 사용한다.
//! 격리·대기 정책은 docs/dev-guide/e2e-tests.md, 실행 채널은 docs/dev-guide/ci-gates.md를 따른다.

// 이유: unsafe 허용은 시험 빌드의 프로세스 하네스에만 적용한다.
#![cfg_attr(test, allow(clippy::multiple_unsafe_ops_per_block))]
// 이유: 시험 정리의 결과 무시는 제품 코드의 오류 처리 목록과 구분한다.
#![allow(clippy::let_underscore_must_use)]
// 이유: 각 시험 바이너리가 공유 헬퍼의 일부만 사용한다.
#![allow(dead_code)]

// 소비자가 spawn_diag를 다시 mod로 넣어 중복 컴파일하지 않도록 재공개한다.
#[path = "../spawn_diag/mod.rs"]
pub mod spawn_diag;

mod startup;
use startup::{StartupFailure, StartupTimeline};

/// 일부 스위트가 직접 붙이는 번들 준비 실패 진단.
#[allow(dead_code)]
pub fn bundle_staging_note() -> String {
    spawn_diag::bundle_staging_note()
}

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use spawn_diag::{SPAWN_PORT_TIMEOUT, SPAWN_SHELL_TIMEOUT, STDERR_TAIL_LINES, StderrCapture};

static SHARED_INSTANCE: OnceLock<TastyInstance> = OnceLock::new();
static SHARED_SPAWN_COUNT: AtomicUsize = AtomicUsize::new(0);
/// OnceLock 초기화가 panic하면 다음 호출이 재시도하므로 별도 상태로 반복 spawn을 막는다.
static SHARED_SPAWN_FAILED: AtomicBool = AtomicBool::new(false);

/// 바이너리마다 인스턴스 하나를 공유한다. Cargo 전체에 걸친 공유는 아니다.
/// 시험은 IPC를 병렬로 보내며 workspace별 상태는 각자 만든 workspace로 분리한다.
/// PTY·global hook·notification 등 합쳐서 조회되는 목록은 다른 시험의 항목도 포함하므로
/// 전체 길이나 첫 항목 대신 자신의 ID가 있는지 확인한다.
pub fn shared() -> &'static TastyInstance {
    SHARED_INSTANCE.get_or_init(|| {
        assert!(
            !SHARED_SPAWN_FAILED.swap(true, Ordering::SeqCst),
            "공유 인스턴스 spawn 이 이미 실패했다 — 재시도하지 않는다 (첫 실패의 panic 메시지에 stderr tail 이 붙어 있다)"
        );
        let instance = TastyInstance::spawn();
        SHARED_SPAWN_FAILED.store(false, Ordering::SeqCst);
        SHARED_SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
        // 정적 인스턴스는 Drop되지 않으므로 정상 프로세스 종료 때 atexit으로 정리한다.
        // SAFETY: 콜백은 static 함수이며 get_or_init 안에서 한 번 등록한다.
        unsafe {
            libc::atexit(on_shared_exit);
        }
        instance
    })
}

pub fn shared_spawn_count() -> usize {
    SHARED_SPAWN_COUNT.load(Ordering::Relaxed)
}

extern "C" fn on_shared_exit() {
    if let Some(instance) = SHARED_INSTANCE.get() {
        instance.terminate();
    }
}

/// 소유한 자식의 강제 종료를 요청한다. Windows는 프로세스 트리, Unix는 전달받은 PID에 신호를 보낸다.
fn force_kill(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        // 종료 정리에서는 taskkill 실패를 무시한다. 이미 종료된 PID도 실패할 수 있다.
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    // SAFETY: 소유한 자식 PID에 SIGKILL을 보낸다. 실패는 errno로 보고되며 메모리 안전성을 해치지 않는다.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

/// 시험별 workspace 정보. 개별 시험에서 닫지 않고 공유 인스턴스 종료 때 정리하므로 다른 시험의 workspace를 수정하지 않는다.
#[derive(Debug, Clone, Copy)]
pub struct TestWorkspace {
    /// attach 점유·조회에 쓰는 workspace ID.
    pub id: u64,
    /// 생성 시점의 workspace 인덱스.
    pub index: usize,
    /// 함께 생성된 첫 surface ID.
    pub surface_id: u64,
}

pub struct TastyInstance {
    process: Child,
    port: u16,
    port_file: PathBuf,
    isolated_home: PathBuf,
    /// 부팅 실패를 진단할 stderr 꼬리와 마지막 출력 시각.
    stderr: StderrCapture,
    startup: StartupTimeline,
}

/// 격리 설정의 셸이다. 사용자 셸 설정에 의존하지 않도록 /bin/sh, Windows는 시험용 Git Bash를 쓴다.
/// 셸 설정을 잠시 바꾼 시험이 공유 인스턴스를 이 값으로 되돌린다.
pub const ISOLATED_SHELL: &str = if cfg!(windows) {
    "C:/Program Files/Git/bin/bash.exe"
} else {
    "/bin/sh"
};

fn write_isolated_config(isolated_home: &std::path::Path, inherit_cwd: bool, restore_layout: bool) {
    let tasty_dir = isolated_home.join(".tasty");
    std::fs::create_dir_all(&tasty_dir).expect("failed to create isolated .tasty dir");

    let shell_path = ISOLATED_SHELL;

    let config = format!(
        r#"[general]
shell = "{shell}"
shell_args = ""
startup_command = ""
language = "en"
scrollback_lines = 10000
confirm_close_running = false
click_to_move_cursor = true
inherit_cwd = {inherit_cwd}
close_behavior = "ask"
restore_layout = {restore_layout}
restore_surface_content = false
link_click_modifier = "ctrl"
"#,
        shell = shell_path,
        inherit_cwd = inherit_cwd,
        restore_layout = restore_layout
    );
    std::fs::write(tasty_dir.join("config.toml"), config)
        .expect("failed to write isolated config.toml");
}

/// 응답 한 줄을 기다리는 상한이다. 응답 속도를 단언하는 값이 아니라 멈춘 인스턴스에서 시험을 끝내는 값이다.
/// 구조를 바꾸는 요청은 journal에서 단계마다 동기 commit을 거치므로, 다른 프로세스가 디스크 동기화를
/// 채우면 응답이 수십 초 늦어질 수 있다. 근거와 측정은 docs/dev-guide/e2e-tests.md의 공유 하네스 절에 있다.
pub const RESPONSE_WAIT: Duration = Duration::from_secs(60);

/// 요청 한 줄을 보내고 응답 한 줄을 읽어 전체 JSON-RPC 응답을 돌려준다.
/// 아무것도 보내지 못한 연결 실패만 다시 시도한다. 보낸 뒤에 응답을 못 받으면 요청이 실행됐을 수 있으므로
/// 다시 보내지 않는다. 다시 보내면 workspace.create 같은 요청이 두 번 실행된다.
pub fn exchange(port: u16, method: &str, params: &Value, wait: Duration) -> Result<Value, String> {
    let mut connected = None;
    for attempt in 0..3 {
        match TcpStream::connect(format!("127.0.0.1:{}", port)) {
            Ok(s) => {
                connected = Some(s);
                break;
            }
            Err(e) if attempt == 2 => {
                return Err(format!("failed to connect for '{}': {}", method, e));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    let mut stream = connected.expect("the loop returns or connects");
    stream.set_read_timeout(Some(wait)).ok();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });
    let mut msg = serde_json::to_string(&request).unwrap();
    msg.push('\n');
    let unanswered = "the request may have run, so it was not sent again";
    stream
        .write_all(msg.as_bytes())
        .map_err(|e| format!("failed to send '{}': {}; {}", method, e, unanswered))?;

    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(|e| {
        format!(
            "failed to read response for '{}' within {:?}: {}; {}",
            method, wait, e, unanswered
        )
    })?;
    serde_json::from_str(&line)
        .map_err(|e| format!("invalid JSON response for '{}': {}", method, e))
}

impl TastyInstance {
    /// 시작 설정이 다르거나 프로세스 전체를 측정해야 할 때만 전용 인스턴스를 사용한다.
    pub fn spawn() -> Self {
        Self::spawn_with_inherit_cwd(false)
    }

    /// inherit_cwd 시작 설정을 바꿔 cwd 전달을 검증할 전용 인스턴스를 만든다.
    pub fn spawn_with_inherit_cwd(inherit_cwd: bool) -> Self {
        Self::spawn_configured(inherit_cwd, false, &[])
    }

    /// 추가 환경변수로 실행할 전용 인스턴스를 만든다.
    pub fn spawn_with_env(extra_env: &[(&str, &str)]) -> Self {
        Self::spawn_configured(false, false, extra_env)
    }

    /// restore_layout을 켜야 하는 시작 동작은 별도 인스턴스에서 확인한다.
    pub fn spawn_with_restore_layout() -> Self {
        Self::spawn_configured(false, true, &[])
    }

    fn spawn_configured(
        inherit_cwd: bool,
        restore_layout: bool,
        extra_env: &[(&str, &str)],
    ) -> Self {
        // 시계 해상도와 무관하게 같은 프로세스의 반복 호출을 구별하도록 단조 카운터를 쓴다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let port_file = std::env::temp_dir().join(format!("tasty-test-{}.port", unique));

        // 사용자의 셸 설정과 Tasty 상태를 읽지 않도록 홈을 격리한다.
        let isolated_home = std::env::temp_dir().join(format!("tasty-test-home-{}", unique));
        std::fs::create_dir_all(&isolated_home).expect("failed to create isolated home");
        // ZDOTDIR이 가리키는 격리 홈에 빈 셸 설정을 둔다.
        std::fs::write(isolated_home.join(".zshrc"), "").ok();
        std::fs::write(isolated_home.join(".bashrc"), "").ok();

        // 셸을 명시한 설정을 미리 써 사용자 환경의 자동 탐지를 피한다.
        write_isolated_config(&isolated_home, inherit_cwd, restore_layout);

        let mut command = Command::new(spawn_diag::instance_bin());
        command
            .arg("--port-file")
            .arg(port_file.to_str().unwrap())
            .env("HOME", &isolated_home)
            // Windows 경로 해석은 HOME만으로 격리되지 않아 TASTY_HOME도 명시한다.
            .env("TASTY_HOME", isolated_home.join(".tasty"))
            .env("ZDOTDIR", &isolated_home)
            .env_remove("OH_MY_ZSH")
            .env_remove("ZSH")
            .env_remove("SHELL")
            // 부모 surface 정보가 자식을 CLI 도움말 경로로 보내지 않도록 제거한다.
            .env_remove("TASTY_SURFACE_ID")
            // stderr는 계속 비우고 로그 수준도 제한해 파이프 역압에 자식이 멈추지 않도록 한다.
            .env(spawn_diag::LOG_ENV, spawn_diag::LOG_FILTER)
            .stderr(Stdio::piped());
        command.envs(extra_env.iter().copied());
        // 시험이 사용자 데스크톱에서 브라우저·파일 관리자를 열지 않도록 기록 모드를 사용한다.
        spawn_diag::apply_os_open_record(&mut command, &isolated_home);
        // 번들 사용을 선언한 시험만 번들을 준비한다. 나머지는 빈 번들로 시작한다.
        spawn_diag::apply_bundle_opt_in(&mut command, &isolated_home.join(".tasty"));
        // 사용자가 보고 있는 디스플레이를 그대로 물려받지 않도록 시험용 화면 설정을 적용한다.
        spawn_diag::apply_display_policy(&mut command);
        // 부모가 종료돼 Drop이 실행되지 않는 경우의 자식 정리는 spawn_child가 맡는다. Self 생성 전 panic도 process 소유 객체가 처리한다.
        let started = Instant::now();
        let process = spawn_diag::spawn_child(command).expect("failed to spawn tasty");
        Self::finish_startup(process, port_file, isolated_home, started)
    }

    /// 소유한 자식으로 정상 시작 절차를 진행한다. 회귀 시험은 모의 자식을 주입한다.
    pub(super) fn finish_startup(
        process: Child,
        port_file: PathBuf,
        isolated_home: PathBuf,
        started: Instant,
    ) -> Self {
        let mut process = spawn_diag::ChildReaper::new(process);
        let startup = StartupTimeline::new(started, process.child().id());
        let _failure = StartupFailure::new(&startup, None);

        // 파이프가 차서 자식이 멈추지 않도록 별도 스레드에서 stderr를 계속 읽는다.
        let mut stderr = StderrCapture::start(process.child().stderr.take(), STDERR_TAIL_LINES);

        let start = Instant::now();
        let port = loop {
            if start.elapsed() > SPAWN_PORT_TIMEOUT {
                // Self 생성 전이므로 process 소유 객체가 panic 시 자식을 정리한다.
                let _ = std::fs::remove_file(&port_file);
                let _ = std::fs::remove_dir_all(&isolated_home);
                // panic 인자에 MutexGuard를 남기면 unwind 중 락이 오염될 수 있어 메서드 안에서 락을 해제한 진단값만 받는다.
                panic!(
                    "{}",
                    spawn_diag::spawn_timeout_message(
                        "tasty failed to start",
                        SPAWN_PORT_TIMEOUT,
                        stderr.tail_lines(),
                        &stderr.tail(),
                        stderr.last_line_age(),
                    )
                );
            }
            if let Ok(content) = std::fs::read_to_string(&port_file)
                && let Ok(port) = content.trim().parse::<u16>()
            {
                startup.port_found();
                break port;
            }
            // 자식이 이미 끝났다면 전체 시작 제한 시간까지 기다리지 않는다.
            if let Ok(Some(status)) = process.child().try_wait() {
                let _ = std::fs::remove_file(&port_file);
                let _ = std::fs::remove_dir_all(&isolated_home);
                panic!(
                    "{}",
                    spawn_diag::early_exit_message(
                        &status.to_string(),
                        stderr.tail_lines(),
                        // 자식 종료 직후에는 stderr 읽기가 덜 끝났을 수 있어 배출을 기다린 꼬리를 받는다.
                        &stderr.tail_after_exit(spawn_diag::STDERR_SETTLE_BUDGET),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };

        let instance = Self {
            process: process.release(),
            port,
            port_file,
            isolated_home,
            stderr,
            startup: startup.clone(),
        };

        instance.wait_for_shell(instance.first_surface_id());
        instance.startup.shell_ready();

        instance
    }

    /// 새 surface를 사용하기 전에 PTY의 첫 출력을 기다린다.
    pub fn wait_for_shell(&self, surface_id: u64) {
        let start = Instant::now();
        loop {
            let text = self.screen_text_of(surface_id);
            if !text.trim().is_empty() {
                return;
            }
            if start.elapsed() > SPAWN_SHELL_TIMEOUT {
                panic!(
                    "{}",
                    spawn_diag::spawn_timeout_message(
                        "shell did not produce output",
                        SPAWN_SHELL_TIMEOUT,
                        self.stderr.tail_lines(),
                        &self.stderr.tail(),
                        self.stderr.last_line_age(),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// 추가 IPC 없이 이미 수집한 제한된 진단 기록을 읽는다.
    pub fn find_stderr(&self, pred: impl Fn(&str) -> bool) -> Option<String> {
        self.stderr.find(pred)
    }

    /// JSON-RPC 결과를 반환한다. 재시도 범위와 응답 대기 상한은 [`exchange`]를 따른다.
    pub fn call(&self, method: &str, params: Value) -> Value {
        let _failure = StartupFailure::new(&self.startup, Some(&self.stderr));
        let resp =
            exchange(self.port, method, &params, RESPONSE_WAIT).unwrap_or_else(|e| panic!("{e}"));
        self.startup.first_response();
        if let Some(error) = resp.get("error") {
            panic!(
                "IPC error for '{}': {}{}",
                method,
                error,
                bundle_staging_note()
            );
        }
        resp.get("result").cloned().unwrap_or(Value::Null)
    }

    /// 회전하는 stderr 꼬리와 별도로 보관한 시작 기록.
    pub fn startup_diagnostics(&self) -> String {
        self.startup.snapshot()
    }

    /// 오류를 포함한 전체 JSON-RPC 응답. 연결·I/O 실패는 panic한다.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn call_raw(&self, method: &str, params: Value) -> Value {
        exchange(self.port, method, &params, RESPONSE_WAIT).unwrap_or_else(|e| panic!("{e}"))
    }

    /// workspace 생성은 활성 대상을 바꾸지 않으므로 시험별로 분리해 사용할 수 있다. PTY가 필요하면 wait_for_shell로 기다린다.
    pub fn create_workspace(&self, name: &str) -> TestWorkspace {
        let result = self.call("workspace.create", serde_json::json!({ "name": name }));
        TestWorkspace {
            id: result["id"].as_u64().expect("workspace.create returns id"),
            index: result["index"]
                .as_u64()
                .expect("workspace.create returns index") as usize,
            surface_id: result["surface_id"]
                .as_u64()
                .expect("workspace.create returns surface_id"),
        }
    }

    /// 전용 인스턴스의 첫 surface. 공유 서버에서는 workspace ID로 찾거나 TestWorkspace의 surface_id를 사용한다.
    pub fn first_surface_id(&self) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    /// 전용 인스턴스의 첫 pane. 공유 서버에서는 workspace ID로 찾는다.
    pub fn first_pane_id(&self) -> u64 {
        let panes = self.call("pane.list", serde_json::json!({}));
        panes.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    /// 지정한 workspace의 첫 surface ID.
    pub fn first_surface_id_in_workspace(&self, workspace_id: u64) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["workspace_id"].as_u64() == Some(workspace_id))
            .unwrap_or_else(|| panic!("no surface in workspace {workspace_id}"))["id"]
            .as_u64()
            .unwrap()
    }

    /// 지정한 workspace의 첫 pane ID.
    pub fn first_pane_id_in_workspace(&self, workspace_id: u64) -> u64 {
        let panes = self.call("pane.list", serde_json::json!({}));
        panes
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["workspace_id"].as_u64() == Some(workspace_id))
            .unwrap_or_else(|| panic!("no pane in workspace {workspace_id}"))["id"]
            .as_u64()
            .unwrap()
    }

    pub fn send_text(&self, surface_id: u64, text: &str) {
        self.call(
            "surface.send",
            serde_json::json!({ "surface_id": surface_id, "text": text }),
        );
    }

    pub fn set_mark(&self, surface_id: u64) {
        self.call(
            "surface.set_mark",
            serde_json::json!({ "surface_id": surface_id }),
        );
    }

    pub fn read_since_mark(&self, surface_id: u64) -> String {
        let result = self.call(
            "surface.read_since_mark",
            serde_json::json!({ "surface_id": surface_id, "strip_ansi": true }),
        );
        result
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    }

    pub fn wait_for_output(&self, surface_id: u64, expected: &str, timeout: Duration) -> String {
        let start = Instant::now();
        loop {
            let output = self.read_since_mark(surface_id);
            if output.contains(expected) {
                return output;
            }
            if start.elapsed() > timeout {
                panic!(
                    "timeout waiting for '{}' in output. Got:\n{}",
                    expected, output
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn screen_text_of(&self, surface_id: u64) -> String {
        let result = self.call(
            "surface.screen_text",
            serde_json::json!({ "surface_id": surface_id }),
        );
        result
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// 프로세스 자원 측정에 쓰는 자식 PID.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn pid(&self) -> u32 {
        self.process.id()
    }

    /// 설정 변경·reload 시험에 쓰는 격리 TASTY_HOME.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn tasty_home(&self) -> PathBuf {
        self.isolated_home.join(".tasty")
    }

    /// 실제로 실행하지 않고 기록한 OS 열기 요청의 파일.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn os_open_log(&self) -> PathBuf {
        self.isolated_home.join(spawn_diag::OS_OPEN_LOG_FILE)
    }

    /// raw attach 연결 등에 쓰는 loopback IPC 포트.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 종료 IPC의 연결·전송·응답 실패를 무시한다. 이미 끝났거나 헤드리스에서 지원하지 않을 수 있다.
    /// 이후 강제 종료를 시도하므로 일반 call의 panic·재시도 경로를 사용하지 않는다.
    pub fn shutdown(&self) {
        let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{}", self.port)) else {
            return;
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "system.shutdown",
            "params": {},
            "id": 1
        });
        let Ok(mut msg) = serde_json::to_string(&request) else {
            return;
        };
        msg.push('\n');
        if stream.write_all(msg.as_bytes()).is_err() {
            return;
        }
        let mut line = String::new();
        // 서버가 종료 요청을 처리할 시간을 주기 위해 응답을 읽되 성공 여부는 무시한다.
        let _ = BufReader::new(&stream).read_line(&mut line);
    }

    /// 공유 인스턴스의 atexit 정리. 정적 &self만 있어 PID로 종료를 요청하고 자식 wait는 수행하지 않는다.
    fn terminate(&self) {
        self.shutdown();
        std::thread::sleep(Duration::from_millis(200));
        force_kill(self.process.id());
        // atexit에서는 출력 대상도 닫혔을 수 있어 정리 실패를 무시한다.
        let _ = std::fs::remove_file(&self.port_file);
        let _ = std::fs::remove_dir_all(&self.isolated_home);
    }
}

/// 전용 인스턴스의 정리. 정적 공유 인스턴스는 atexit 경로를 사용한다.
impl Drop for TastyInstance {
    fn drop(&mut self) {
        self.shutdown();
        std::thread::sleep(Duration::from_millis(200));
        force_kill(self.process.id());
        let _ = self.process.wait();
        // 자식이 종료돼야 stderr EOF가 오므로 종료 뒤 배출 스레드를 join한다.
        self.stderr.join();
        let _ = std::fs::remove_file(&self.port_file);
        let _ = std::fs::remove_dir_all(&self.isolated_home);
    }
}
