//! 실제 GUI 대신 가짜 자식을 소유한다. Enigo 생성에는 격리 X 디스플레이가 필요하다.

use super::*;
use std::net::TcpListener;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Fixture {
    instance: GuiTestInstance,
    output: BufReader<std::process::ChildStdout>,
    _root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        // 키나 마우스 입력은 보내지 않는다. 실제 하네스의 필드만 초기화한다.
        let enigo = Enigo::new(&EnigoSettings::default()).expect("isolated display required");
        let root = tempfile::tempdir().expect("fixture directory");
        let isolated_home = root.path().join("home");
        std::fs::create_dir(&isolated_home).expect("fixture home");
        let port_file = root.path().join("instance.port");
        std::fs::write(&port_file, "0").expect("fixture port file");
        let mut child = spawn_diag::ChildReaper::new(
            Command::new("/bin/sh")
                .args([
                    "-c",
                    "printf 'ready\\n'; while IFS= read -r line; do printf '%s\\n' \"$line\" >&2; printf 'ack\\n'; done",
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("fake child"),
        );
        let stderr = StderrCapture::start(child.child().stderr.take(), STDERR_TAIL_LINES);
        let mut output = BufReader::new(child.child().stdout.take().expect("stdout"));
        let mut ready = String::new();
        output.read_line(&mut ready).expect("child readiness");
        assert_eq!(ready, "ready\n");
        Self {
            instance: GuiTestInstance {
                process: child.release(),
                stderr,
                port: 0,
                port_file,
                isolated_home,
                enigo,
            },
            output,
            _root: root,
        }
    }

    fn write_after_boot(&mut self, text: &str) {
        writeln!(
            self.instance.process.stdin.as_mut().expect("stdin"),
            "{text}"
        )
        .expect("tell fake child to write");
        let mut ack = String::new();
        self.output.read_line(&mut ack).expect("write acknowledged");
        assert_eq!(ack, "ack\n");
        // stdout ACK 는 stderr drain의 스케줄을 보장하지 않는다. 수집된 줄로 동기화한다.
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(
            tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(5), |_| {
                self.instance.stderr.find(|line| line == text).is_some()
            }),
            "stderr was not captured"
        );
    }

    fn reply_once(&mut self, response: &'static str) -> std::thread::JoinHandle<()> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake IPC listener");
        self.instance.port = listener.local_addr().expect("address").port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("fake IPC connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut request = String::new();
            BufReader::new(&stream)
                .read_line(&mut request)
                .expect("request");
            writeln!(stream, "{response}").expect("response");
        })
    }
}

fn panic_text(result: std::thread::Result<()>) -> String {
    let error = result.expect_err("the fake failure must panic");
    error
        .downcast_ref::<String>()
        .expect("string panic")
        .clone()
}

#[test]
#[ignore = "requires an isolated X display for Enigo; never sends desktop input"]
fn post_boot_ipc_failure_keeps_stderr_and_drop_reaps_child() {
    let mut fixture = Fixture::new();
    fixture.write_after_boot("post-boot IPC evidence");
    let server = fixture.reply_once(r#"{"error":{"code":-32601,"message":"synthetic failure"}}"#);
    let text = panic_text(catch_unwind(AssertUnwindSafe(|| {
        fixture.instance.call("fake.method", serde_json::json!({}));
    })));
    server.join().expect("fake server joined");
    assert!(
        text.contains("fake.method") && text.contains("synthetic failure"),
        "{text}"
    );
    assert!(text.contains("post-boot IPC evidence"), "{text}");
    assert!(text.contains("last 30 lines"), "{text}");

    let pid = fixture.instance.process.id();
    let home = fixture.instance.isolated_home.clone();
    let port_file = fixture.instance.port_file.clone();
    let Fixture {
        instance,
        output: _output,
        _root,
    } = fixture;
    drop(instance);
    // SAFETY: 직접 spawn한 자식 PID만 조회하며 시그널은 보내지 않는다.
    let waited = unsafe { libc::waitpid(pid as libc::pid_t, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(waited, -1, "Drop must already have reaped the child");
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    assert!(!home.exists() && !port_file.exists());
}

#[test]
#[ignore = "requires an isolated X display for Enigo; never sends desktop input"]
fn post_boot_ui_timeout_keeps_stderr_and_state() {
    let mut fixture = Fixture::new();
    fixture.write_after_boot("post-boot UI evidence");
    let server = fixture.reply_once(r#"{"result":{"workspace_count":7}}"#);
    let text = panic_text(catch_unwind(AssertUnwindSafe(|| {
        fixture
            .instance
            .wait_for_ui("synthetic condition", Duration::ZERO, |_| false);
    })));
    server.join().expect("fake server joined");
    assert!(text.contains("synthetic condition"), "{text}");
    assert!(text.contains("workspace_count: 7"), "{text}");
    assert!(text.contains("post-boot UI evidence"), "{text}");
}
