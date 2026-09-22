//! 스트리밍 CLI 명령(`events follow` · `plugin audit-follow`)이 호스트 오류를 낼 때의 stderr 모양.
//!
//! 두 명령은 호스트 오류를 `main` 까지 올려 std 가 찍는다. 그래서 첫 줄은
//! `Error: Error (<code>): <message>` 이고 — 그 모양을 파싱하는 쪽이 있어 바꾸지 않는다 —
//! 응답에 `error.data` 가 있으면 둘째 줄 `data: <한 줄 JSON>` 이 붙는다. `data` 가 없으면
//! 한 줄 그대로다. 근거 `docs/adr/0512-the-cli-relays-ipc-error-data-on-a-second-stderr-line.md`.
//!
//! 실제 바이너리를 가짜 호스트(첫 요청에 JSON-RPC 오류로 답하는 loopback 리스너)에 붙여 잰다 —
//! std 가 `Err` 를 찍는 모양까지 이 경로에만 있다. `TASTY_HOME` 을 tempdir 로 격리하고 그 안에
//! 포트 파일을 쓴다.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

/// 첫 요청 한 줄을 읽고 `error` 로 답한 뒤 연결을 닫는 가짜 호스트. 받은 요청의 method 를 돌려준다.
fn fake_host(home: &Path, error: serde_json::Value) -> std::thread::JoinHandle<String> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    std::fs::write(home.join("tasty.port"), port.to_string()).expect("write port file");
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("read request");
        let req: serde_json::Value = serde_json::from_str(line.trim()).expect("request is JSON");
        let resp = serde_json::json!({ "jsonrpc": "2.0", "id": req["id"], "error": error });
        let mut out = stream;
        out.write_all(format!("{resp}\n").as_bytes())
            .expect("write response");
        req["method"].as_str().unwrap_or_default().to_string()
    })
}

/// 격리 홈에서 CLI 를 돌려 (종료 코드, stderr) 를 얻는다.
fn run(home: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tasty"))
        .args(args)
        .env("TASTY_HOME", home)
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        // anyhow 가 backtrace 블록을 붙이면 줄 수가 달라진다 — 이 시험은 출력 모양을 잰다.
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

/// `data` 가 있으면 첫 줄은 종전 그대로, 둘째 줄이 `data: ` + 원형 JSON 이다.
#[test]
fn streaming_commands_add_the_data_line_under_an_unchanged_first_line() {
    for (method, args) in STREAMING {
        let home = tempfile::tempdir().expect("tempdir");
        let host = fake_host(
            home.path(),
            serde_json::json!({
                "code": -32065,
                "message": "permission denied",
                "data": { "reason": "denied", "detail": { "ids": [1, 2] } }
            }),
        );
        let (code, stderr) = run(home.path(), args);
        assert_eq!(host.join().expect("fake host"), method, "{args:?}");
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

/// `data` 가 없으면(또는 `null` 이면) 출력은 종전의 한 줄 그대로다.
#[test]
fn streaming_commands_without_data_print_the_single_line_as_before() {
    for data in [None, Some(serde_json::Value::Null)] {
        for (method, args) in STREAMING {
            let home = tempfile::tempdir().expect("tempdir");
            let mut error = serde_json::json!({ "code": -32601, "message": "Method not found" });
            if let Some(d) = &data {
                error["data"] = d.clone();
            }
            let host = fake_host(home.path(), error);
            let (code, stderr) = run(home.path(), args);
            assert_eq!(host.join().expect("fake host"), method, "{args:?}");
            assert_eq!(
                stderr, "Error: Error (-32601): Method not found\n",
                "{args:?} data={data:?}"
            );
            assert_eq!(code, Some(1), "{args:?}");
        }
    }
}
