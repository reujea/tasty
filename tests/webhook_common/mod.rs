//! 웹훅 설정을 미리 작성하고 실제 HTTP·CLI 응답을 확인하는 하네스다.
//! 선택한 포트는 spawn 직전까지 예약한다. 해제부터 자식 bind까지는 경합이 남아 제한된 횟수만 재시도한다.
//! 재시작 시험은 같은 홈과 웹훅 URL을 유지한다.

// 시험 하네스의 플랫폼 API 호출에 unsafe가 필요하다.
#![cfg_attr(test, allow(clippy::multiple_unsafe_ops_per_block))]
// 공유 모듈을 포함하는 테스트 바이너리마다 사용하는 함수가 달라 미사용 함수도 제공한다.
#![allow(dead_code)]
// 시험 본문은 제품 코드의 let _ 사유 주석 정책에서 제외된다.
#![allow(clippy::let_underscore_must_use)]

#[path = "../spawn_diag/mod.rs"]
mod spawn_diag;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use spawn_diag::{SPAWN_PORT_TIMEOUT, SPAWN_SHELL_TIMEOUT, STDERR_TAIL_LINES, StderrCapture};

const WEBHOOK_READY_TIMEOUT: Duration = Duration::from_secs(20);

/// bind 실패 시 재부팅 비용과 대기를 제한하기 위한 재시도 상한이다.
const PORT_STEAL_RETRIES: usize = 2;

/// bind 실패 로그의 고정 접두사다. 실패 이유가 포트 점유인지 권한 등 다른 원인인지는 이 표지만으로 구별하지 못한다.
const BIND_FAILED_MARKER: &str = "webhook listener bind";

fn is_bind_failure(line: &str) -> bool {
    line.contains(BIND_FAILED_MARKER) && line.contains("failed")
}

/// spawn 직전까지 임시 리스너로 포트를 예약한다. 해제 뒤 자식이 bind하기 전의 경합까지 없애지는 못한다.
pub struct PortLease {
    /// spawn 직전에 예약을 해제한다.
    listener: Option<std::net::TcpListener>,
    port: u16,
}

impl PortLease {
    pub fn port(&self) -> u16 {
        self.port
    }

    fn release(&mut self) {
        self.listener = None;
    }
}

pub fn free_port() -> PortLease {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind free port");
    let port = listener.local_addr().expect("local_addr").port();
    PortLease {
        listener: Some(listener),
        port,
    }
}

enum BindOutcome {
    Bound,
    /// bind 실패 로그가 수집됐다. 오류 종류는 따로 분류하지 않는다.
    Stolen(String),
    Silent,
}

fn config_toml() -> String {
    let shell_path = if cfg!(windows) {
        "C:/Program Files/Git/bin/bash.exe"
    } else {
        "/bin/sh"
    };
    format!(
        r#"[general]
shell = "{shell}"
shell_args = ""
startup_command = ""
language = "en"
scrollback_lines = 10000
confirm_close_running = false
click_to_move_cursor = true
inherit_cwd = false
close_behavior = "ask"
restore_layout = false
restore_surface_content = false
link_click_modifier = "ctrl"
"#,
        shell = shell_path
    )
}

pub struct Builder {
    /// 재시작은 기존 설정의 포트를 유지해야 하므로 새 포트를 예약하지 않는다.
    lease: Option<PortLease>,
    /// None이면 기존 webhooks.toml의 포트를 읽는다.
    webhook_port: Option<u16>,
    /// None이면 하네스가 홈을 만들고 삭제한다. Some이면 호출자 소유라 삭제하지 않는다.
    home: Option<PathBuf>,
    env: Vec<(String, String)>,
    files: Vec<(String, String)>,
}

impl Builder {
    /// 재시작 간 공유할 호출자 소유 홈이다. Drop에서 삭제하지 않는다.
    pub fn home(mut self, dir: PathBuf) -> Self {
        self.home = Some(dir);
        self
    }

    pub fn env(mut self, key: &str, val: &str) -> Self {
        self.env.push((key.to_string(), val.to_string()));
        self
    }

    pub fn file(mut self, name: &str, content: &str) -> Self {
        self.files.push((name.to_string(), content.to_string()));
        self
    }

    pub fn spawn(self) -> WebhookInstance {
        WebhookInstance::spawn_inner(self)
    }
}

/// 재시도마다 입력을 다시 사용할 수 있도록 빌더 대신 스펙을 빌려 준다.
struct SpawnSpec<'a> {
    webhook_port: Option<u16>,
    home: Option<PathBuf>,
    env: &'a [(String, String)],
    files: &'a [(String, String)],
}

pub struct WebhookInstance {
    process: Child,
    port: u16,
    webhook_port: u16,
    port_file: PathBuf,
    home: PathBuf,
    shell_home: PathBuf,
    own_home: bool,
    /// 부팅 실패 진단에 쓸 stderr 꼬리와 마지막 수집 시각이다. 시각만으로 정지와 진행을 구별하지는 않는다.
    stderr: StderrCapture,
}

impl WebhookInstance {
    pub fn builder(lease: PortLease) -> Builder {
        Builder {
            webhook_port: Some(lease.port()),
            lease: Some(lease),
            home: None,
            env: Vec::new(),
            files: Vec::new(),
        }
    }

    /// 기존 webhooks.toml의 포트로 재시작한다. URL을 유지해야 하므로 다른 포트를 선택하지 않는다. home 지정이 필요하다.
    pub fn builder_for_restart() -> Builder {
        Builder {
            lease: None,
            webhook_port: None,
            home: None,
            env: Vec::new(),
            files: Vec::new(),
        }
    }

    /// 이 호출이 새로 포트를 설정한 경우에만 bind 실패 후 다른 포트로 재시도한다.
    fn spawn_inner(builder: Builder) -> Self {
        let Builder {
            mut lease,
            mut webhook_port,
            home,
            env,
            files,
        } = builder;
        assert!(
            webhook_port.is_some() || home.is_some(),
            "builder_for_restart 는 기존 webhooks.toml 이 있는 홈(.home(..))이 필요하다"
        );
        for attempt in 0..=PORT_STEAL_RETRIES {
            let seeded;
            let instance = {
                let spec = SpawnSpec {
                    webhook_port,
                    home: home.clone(),
                    env: &env,
                    files: &files,
                };
                let (inst, did_seed) = Self::spawn_once(spec, lease.as_mut());
                seeded = did_seed;
                inst
            };
            match instance.wait_webhook_bound() {
                BindOutcome::Bound => return instance,
                BindOutcome::Stolen(line) if seeded && attempt < PORT_STEAL_RETRIES => {
                    // 하네스가 지정한 포트라 다른 번호로 재시도할 수 있다.
                    spawn_diag::init_test_tracing();
                    tracing::warn!(
                        "webhook bind failed on port {}; retrying with a new port ({line})",
                        instance.webhook_port
                    );
                    let seeded_file = instance.home.join("webhooks.toml");
                    drop(instance);
                    // 설정 삭제에 실패하면 다음 시도가 기존 포트를 다시 읽을 수 있다.
                    if let Err(e) = std::fs::remove_file(&seeded_file) {
                        tracing::warn!("could not reset {seeded_file:?}: {e}");
                    }
                    let fresh = free_port();
                    webhook_port = Some(fresh.port());
                    lease = Some(fresh);
                }
                outcome => panic!("{}", instance.bind_failure_report(&outcome, seeded)),
            }
        }
        unreachable!("재시도 루프는 return 또는 panic 으로만 빠져나간다");
    }

    fn spawn_once(spec: SpawnSpec<'_>, lease: Option<&mut PortLease>) -> (Self, bool) {
        let SpawnSpec {
            webhook_port,
            home: home_arg,
            env: env_args,
            files,
        } = spec;
        // PID와 프로세스 내 카운터로 임시 경로를 구별한다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let port_file = std::env::temp_dir().join(format!("tasty-wh-test-{unique}.port"));

        let (home, own_home) = match home_arg {
            Some(h) => (h, false),
            None => (
                std::env::temp_dir().join(format!("tasty-wh-home-{unique}")),
                true,
            ),
        };
        std::fs::create_dir_all(&home).expect("create TASTY_HOME");

        let shell_home = std::env::temp_dir().join(format!("tasty-wh-shellhome-{unique}"));
        std::fs::create_dir_all(&shell_home).expect("create shell HOME");
        std::fs::write(shell_home.join(".zshrc"), "").ok();
        std::fs::write(shell_home.join(".bashrc"), "").ok();

        std::fs::write(home.join("config.toml"), config_toml()).expect("write config.toml");
        // 기존 설정은 포트와 영속 항목을 유지한다. 파일이 없을 때만 새 포트를 쓴다.
        let webhooks_toml = home.join("webhooks.toml");
        let seeded = !webhooks_toml.exists();
        let webhook_port = if seeded {
            let port = webhook_port.expect("빈 홈에는 하네스가 포트를 골라 주어야 한다");
            std::fs::write(&webhooks_toml, format!("port = {port}\n")).expect("seed webhooks.toml");
            port
        } else {
            read_seeded_port(&webhooks_toml)
        };
        for (name, content) in files {
            std::fs::write(home.join(name), content).expect("write extra tasty file");
        }

        let mut command = Command::new(spawn_diag::instance_bin());
        command
            .arg("--port-file")
            .arg(port_file.to_str().unwrap())
            .env("HOME", &shell_home)
            .env("ZDOTDIR", &shell_home)
            .env("TASTY_HOME", &home)
            .env_remove("OH_MY_ZSH")
            .env_remove("ZSH")
            .env_remove("SHELL")
            .env_remove("TASTY_SURFACE_ID")
            // 공용 필터에 리스너 info만 더해 진단에 필요한 시작 로그를 남긴다.
            .env(spawn_diag::LOG_ENV, spawn_diag::LOG_FILTER_WEBHOOK)
            .stderr(Stdio::piped());
        // OS 열기가 기존 사용자 프로세스로 전달되지 않도록 기록 모드를 적용한다.
        spawn_diag::apply_os_open_record(&mut command, &home);
        spawn_diag::apply_bundle_opt_in(&mut command, &home);
        spawn_diag::apply_display_policy(&mut command);
        for (k, v) in env_args {
            command.env(k, v);
        }

        // 자식이 bind할 수 있게 spawn 직전에 예약을 해제한다.
        if let Some(lease) = lease {
            lease.release();
        }

        // 초기화 완료 전에는 ChildReaper가 자식을 소유한다. Linux 부모 종료 처리는 spawn_child가 설정한다.
        let mut process = spawn_diag::ChildReaper::new(
            spawn_diag::spawn_child(command).expect("failed to spawn tasty"),
        );

        // 파이프가 차서 자식이 멈추지 않도록 stderr를 계속 읽는다.
        let mut stderr = StderrCapture::start(process.child().stderr.take(), STDERR_TAIL_LINES);

        let start = Instant::now();
        let port = loop {
            if start.elapsed() > SPAWN_PORT_TIMEOUT {
                // panic 인자에 락 가드가 남지 않도록 메서드에서 시각을 읽는다.
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
                break port;
            }
            if let Ok(Some(status)) = process.child().try_wait() {
                panic!(
                    "{}",
                    spawn_diag::early_exit_message(
                        &status.to_string(),
                        stderr.tail_lines(),
                        // 종료 확인이 배출보다 빠를 수 있어 남은 stderr 수집을 기다린다.
                        &stderr.tail_after_exit(spawn_diag::STDERR_SETTLE_BUDGET),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };

        let instance = Self {
            process: process.release(),
            port,
            webhook_port,
            port_file,
            home,
            shell_home,
            own_home,
            stderr,
        };

        let start = Instant::now();
        loop {
            let text = instance.screen_text_of(instance.first_surface_id());
            if !text.trim().is_empty() {
                break;
            }
            if start.elapsed() > SPAWN_SHELL_TIMEOUT {
                panic!(
                    "{}",
                    spawn_diag::spawn_timeout_message(
                        "shell produced no output",
                        SPAWN_SHELL_TIMEOUT,
                        instance.stderr.tail_lines(),
                        &instance.stderr.tail(),
                        instance.stderr.last_line_age(),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        (instance, seeded)
    }

    pub fn webhook_port(&self) -> u16 {
        self.webhook_port
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn port_file(&self) -> &Path {
        &self.port_file
    }

    /// 설정을 다시 쓰고 reload할 때 사용할 이 인스턴스의 홈 경로다.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn tasty_home(&self) -> &std::path::Path {
        &self.home
    }

    pub fn wait_webhook_ready(&self) {
        match self.wait_webhook_bound() {
            BindOutcome::Bound => {}
            outcome => panic!("{}", self.bind_failure_report(&outcome, false)),
        }
    }

    /// TCP 연결 성공, bind 실패 로그, 시간 제한 초과를 구별한다. 연결을 수락한 프로세스의 신원까지 확인하지는 않는다.
    fn wait_webhook_bound(&self) -> BindOutcome {
        let start = Instant::now();
        let addr = format!("127.0.0.1:{}", self.webhook_port);
        loop {
            if TcpStream::connect(&addr).is_ok() {
                return BindOutcome::Bound;
            }
            if let Some(line) = self.stderr_bind_failure() {
                return BindOutcome::Stolen(line);
            }
            if start.elapsed() > WEBHOOK_READY_TIMEOUT {
                return BindOutcome::Silent;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn stderr_bind_failure(&self) -> Option<String> {
        self.stderr.find(is_bind_failure)
    }

    fn bind_failure_report(&self, outcome: &BindOutcome, seeded: bool) -> String {
        let port = self.webhook_port;
        let head = match outcome {
            BindOutcome::Bound => {
                "웹훅 포트에 TCP 연결이 됐다. 실패 진단 호출 위치를 확인한다.".to_string()
            }
            BindOutcome::Stolen(line) if seeded => format!(
                "웹훅 포트 {port}의 bind 실패 뒤 재시도 {PORT_STEAL_RETRIES}회를 소진했다. 포트 점유와 다른 bind 오류를 로그에서 확인한다: {line}"
            ),
            BindOutcome::Stolen(line) => format!(
                "웹훅 포트 {port} 에 bind 하지 못했다. 이 인스턴스는 이미 있는 webhooks.toml 의 \
                 포트를 그대로 쓰므로(재시작 시나리오 — URL 이 재시작 간 고정이어야 한다) \
                 하네스가 다른 번호를 고를 수 없다. 리스너 경고: {line}"
            ),
            BindOutcome::Silent => format!(
                "{WEBHOOK_READY_TIMEOUT:?} 안에 웹훅 포트 {port}로 연결하지 못했고 수집된 꼬리에도 bind 실패 표지가 없다. 부팅·리스너 상태와 로그 수집을 확인한다."
            ),
        };
        format!(
            "{head}\n--- stderr (last {} lines) ---\n{}",
            self.stderr.tail_lines(),
            self.stderr.tail()
        )
    }

    pub fn call(&self, method: &str, params: Value) -> Value {
        let resp = self.call_raw(method, params);
        if let Some(error) = resp.get("error").filter(|e| !e.is_null()) {
            panic!("IPC error for '{method}': {error}");
        }
        resp.get("result").cloned().unwrap_or(Value::Null)
    }

    pub fn call_raw(&self, method: &str, params: Value) -> Value {
        let mut stream =
            TcpStream::connect(format!("127.0.0.1:{}", self.port)).expect("connect to tasty IPC");
        stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
        let request = serde_json::json!({
            "jsonrpc": "2.0", "method": method, "params": params, "id": 1
        });
        let mut msg = serde_json::to_string(&request).unwrap();
        msg.push('\n');
        stream.write_all(msg.as_bytes()).expect("send IPC");
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read IPC response");
        serde_json::from_str(&line).expect("valid JSON response")
    }

    pub fn first_surface_id(&self) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces.as_array().unwrap()[0]["id"].as_u64().unwrap()
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

    /// path는 /로 시작해도 되고 생략해도 된다. 쿼리를 포함할 수 있으며 상태 코드와 본문을 반환한다.
    pub fn http(&self, method: &str, path: &str, body: &str) -> (u16, String) {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let addr = format!("127.0.0.1:{}", self.webhook_port);
        let mut stream = TcpStream::connect(&addr).expect("connect webhook listener");
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
            len = body.len()
        );
        stream.write_all(request.as_bytes()).expect("write HTTP");
        stream.flush().ok();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).ok();
        parse_http_response(&raw)
    }

    pub fn post(&self, path: &str, body: &str) -> (u16, String) {
        self.http("POST", path, body)
    }

    pub fn http_with_headers(
        &self,
        method: &str,
        path: &str,
        extra_headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String) {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let addr = format!("127.0.0.1:{}", self.webhook_port);
        let mut stream = TcpStream::connect(&addr).expect("connect webhook listener");
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let mut extra = String::new();
        for (k, v) in extra_headers {
            extra.push_str(&format!("{k}: {v}\r\n"));
        }
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{extra}Content-Length: {len}\r\nConnection: close\r\n\r\n{body}",
            len = body.len()
        );
        stream.write_all(request.as_bytes()).expect("write HTTP");
        stream.flush().ok();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).ok();
        parse_http_response(&raw)
    }

    pub fn cli(&self, args: &[&str]) -> Output {
        Command::new(spawn_diag::instance_bin())
            .arg("--port-file")
            .arg(self.port_file.to_str().unwrap())
            .args(args)
            .env_remove("TASTY_SURFACE_ID")
            // 부모 세션의 토큰으로 인증하지 않도록 제거하고 로컬 호출자로 연결한다.
            .env_remove("TASTY_SESSION_TOKEN")
            .env("TASTY_HOME", &self.home)
            .output()
            .expect("run tasty CLI")
    }

    pub fn shutdown(&self) {
        let _ = self.call_raw("system.shutdown", serde_json::json!({}));
    }
}

impl Drop for WebhookInstance {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.shutdown();
        }));
        std::thread::sleep(Duration::from_millis(200));
        #[cfg(target_os = "windows")]
        {
            let pid = self.process.id();
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = self.process.kill();
        }
        let _ = self.process.wait();
        // stderr join이 EOF를 기다리므로 자식 종료·회수 뒤 호출한다.
        self.stderr.join();
        let _ = std::fs::remove_file(&self.port_file);
        let _ = std::fs::remove_dir_all(&self.shell_home);
        if self.own_home {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }
}

/// 재시작 시 기존 webhooks.toml의 포트를 읽는다.
fn read_seeded_port(path: &Path) -> u16 {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .find_map(|line| line.split_once('=').filter(|(k, _)| k.trim() == "port"))
        .and_then(|(_, v)| v.trim().parse::<u16>().ok())
        .unwrap_or_else(|| panic!("{} 에 `port = N` 이 없다:\n{text}", path.display()))
}

/// HTTP 응답을 첫 CRLF 이중 줄바꿈으로 나누고 첫 줄의 상태 코드를 읽는다. chunked 등 일반 HTTP 응답 전체를 처리하는 파서는 아니다.
fn parse_http_response(raw: &[u8]) -> (u16, String) {
    let text = String::from_utf8_lossy(raw);
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = match text.split_once("\r\n\r\n") {
        Some((_, b)) => b.to_string(),
        None => String::new(),
    };
    (code, body)
}

pub fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn stderr_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[cfg(test)]
mod bind_detection_tests {
    use super::*;

    /// 제품의 bind 실패·성공·재초기화 생략 로그를 구별한다.
    #[test]
    fn only_the_bind_failure_line_counts_as_a_steal() {
        assert!(is_bind_failure(
            "WARN tasty::webhook::listener: webhook listener bind 0.0.0.0:28429 failed: \
             Address already in use (os error 98) — set a free port and check firewall"
        ));
        assert!(!is_bind_failure(
            "INFO tasty::webhook::listener: webhook listener bound on 0.0.0.0:28429"
        ));
        assert!(!is_bind_failure(
            "DEBUG tasty::webhook::listener: webhook listener already bound; skip re-init"
        ));
    }

    #[test]
    fn a_stolen_port_is_retried_a_bounded_number_of_times() {
        const {
            assert!(
                PORT_STEAL_RETRIES > 0,
                "포트 예약 해제 뒤 bind 실패에 대한 재시도가 필요하다"
            );
        }
        const {
            assert!(
                PORT_STEAL_RETRIES <= 3,
                "재시도가 길어지면 원인이 로그에 묻힌다"
            );
        }
    }
}
