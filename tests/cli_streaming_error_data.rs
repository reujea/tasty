//! 스트리밍 CLI의 호스트 오류가 기존 첫 줄 형식을 유지하고 error.data만 둘째 줄 JSON으로 추가하는지 확인한다.
//! 실제 CLI를 loopback 모의 호스트에 연결하며 TASTY_HOME과 포트 파일은 임시 디렉터리에 둔다(docs/dev-guide/cli-structure.md).

#[path = "spawn_diag/mod.rs"]
mod spawn_diag;

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 연결·요청이 오지 않아 시험이 무한히 기다리지 않도록 제한한다.
const HOST_DEADLINE: Duration = Duration::from_secs(10);

/// 첫 JSON-RPC 요청에 오류로 답한다. 실패하면 연결·요청 중 어디에서 멈췄는지 명령 라벨과 함께 반환한다.
fn fake_host(
    home: &Path,
    label: String,
    error: serde_json::Value,
) -> std::thread::JoinHandle<Result<String, String>> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    std::fs::write(home.join("tasty.port"), port.to_string()).expect("write port file");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    std::thread::spawn(move || {
        let deadline = Instant::now() + HOST_DEADLINE;
        let mut accepted = None;
        // 기한 뒤 읽기도 이미 들어와 대기 중인 연결을 꺼내는 accept 하나뿐이다.
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(10), |_| {
            match listener.accept() {
                Ok((stream, _)) => {
                    accepted = Some(Ok(stream));
                    true
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => false,
                Err(e) => {
                    accepted = Some(Err(e));
                    true
                }
            }
        });
        let stream = match accepted {
            Some(Ok(stream)) => stream,
            Some(Err(e)) => return Err(format!("{label}: accept 실패: {e}")),
            None => {
                return Err(format!(
                    "{label}: 가짜 호스트가 {HOST_DEADLINE:?} 동안 연결을 받지 못했다 \
                     (CLI 가 포트 {port} 에 붙지 않았다)"
                ));
            }
        };
        // 플랫폼에 따라 리스너의 nonblocking이 상속될 수 있어 소켓을 명시적으로 blocking으로 바꾼다.
        stream.set_nonblocking(false).expect("blocking stream");
        let remaining = deadline.saturating_duration_since(Instant::now());
        stream
            .set_read_timeout(Some(remaining.max(Duration::from_millis(1))))
            .expect("read timeout");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return Err(format!("{label}: 연결은 됐으나 요청 없이 닫혔다")),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(format!(
                    "{label}: 연결은 됐으나 기한({HOST_DEADLINE:?}) 안에 요청 한 줄이 오지 않았다"
                ));
            }
            Err(e) => return Err(format!("{label}: 요청 읽기 실패: {e}")),
        }
        let req: serde_json::Value = serde_json::from_str(line.trim()).expect("request is JSON");
        let resp = serde_json::json!({ "jsonrpc": "2.0", "id": req["id"], "error": error });
        let mut out = stream;
        out.write_all(format!("{resp}\n").as_bytes())
            .expect("write response");
        Ok(req["method"].as_str().unwrap_or_default().to_string())
    })
}

fn host_method(host: std::thread::JoinHandle<Result<String, String>>, stderr: &str) -> String {
    host.join()
        .expect("fake host thread panicked")
        .unwrap_or_else(|e| panic!("{e}\nCLI stderr:\n{stderr}"))
}

fn run(home: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(spawn_diag::instance_bin())
        .args(args)
        .env("TASTY_HOME", home)
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        // backtrace가 출력 줄 수를 바꾸지 않도록 제거한다.
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .expect("run tasty");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const STREAMING: [(&str, &[&str]); 2] = [
    ("events.fetch", &["events", "follow"]),
    (
        "plugin.audit_follow",
        &["plugin", "audit-follow", "--interval-ms", "10"],
    ),
];

#[test]
fn streaming_commands_add_the_data_line_under_an_unchanged_first_line() {
    for (method, args) in STREAMING {
        let home = tempfile::tempdir().expect("tempdir");
        let host = fake_host(
            home.path(),
            format!("{args:?}"),
            serde_json::json!({
                "code": -32065,
                "message": "permission denied",
                "data": { "reason": "denied", "detail": { "ids": [1, 2] } }
            }),
        );
        let (code, stderr) = run(home.path(), args);
        assert_eq!(host_method(host, &stderr), method, "{args:?}");
        let lines: Vec<&str> = stderr.lines().collect();
        assert_eq!(lines.len(), 2, "{args:?}: stderr:\n{stderr}");
        assert_eq!(
            lines[0], "Error: Error (-32065): permission denied",
            "{args:?}"
        );
        let data: serde_json::Value = serde_json::from_str(
            lines[1]
                .strip_prefix("data: ")
                .unwrap_or_else(|| panic!("{args:?}: 둘째 줄이 data 접두가 아니다: {stderr}")),
        )
        .expect("둘째 줄은 JSON");
        assert_eq!(
            data,
            serde_json::json!({ "reason": "denied", "detail": { "ids": [1, 2] } }),
            "{args:?}"
        );
        assert_eq!(code, Some(1), "{args:?}: stderr:\n{stderr}");
    }
}

#[test]
fn streaming_commands_without_data_print_the_single_line_as_before() {
    for data in [None, Some(serde_json::Value::Null)] {
        for (method, args) in STREAMING {
            let home = tempfile::tempdir().expect("tempdir");
            let mut error = serde_json::json!({ "code": -32601, "message": "Method not found" });
            if let Some(d) = &data {
                error["data"] = d.clone();
            }
            let host = fake_host(home.path(), format!("{args:?}"), error);
            let (code, stderr) = run(home.path(), args);
            assert_eq!(host_method(host, &stderr), method, "{args:?}");
            assert_eq!(
                stderr, "Error: Error (-32601): Method not found\n",
                "{args:?} data={data:?}"
            );
            assert_eq!(code, Some(1), "{args:?}");
        }
    }
}
