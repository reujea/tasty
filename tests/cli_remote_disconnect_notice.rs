//! `tasty remote disconnect` 의 출력과 종료 코드를 실제 바이너리로 확인한다. 이미 끊김 안내는 정적 명령의
//! 응답 출력 자리에서 stderr 로 나가므로 단위 시험이 지나지 못한다. 인스턴스를 띄우지 않고 임시
//! TASTY_HOME 의 포트 파일로 가짜 호스트에 붙인다.

#[path = "spawn_diag/mod.rs"]
mod spawn_diag;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

/// 모든 요청에 `reply`(`result` 나 `error` 를 담은 객체)로 답하는 가짜 호스트.
fn fake_host(home: &Path, reply: Value) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::fs::write(home.join("tasty.port"), port.to_string()).expect("port file");
    std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let mut writer = stream.try_clone().expect("clone");
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let request: Value = serde_json::from_str(&line).expect("request json");
                assert_eq!(request["method"], "remote.disconnect", "{request}");
                let mut response = reply.clone();
                response["jsonrpc"] = "2.0".into();
                response["id"] = request["id"].clone();
                if writer
                    .write_all(format!("{response}\n").as_bytes())
                    .is_err()
                {
                    return;
                }
                line.clear();
            }
        }
    });
}

fn disconnect(home: &Path, workspace: &str) -> Output {
    Command::new(spawn_diag::instance_bin())
        .env("TASTY_HOME", home)
        .env("TASTY_LOCALE", "en")
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .env_remove("TASTY_AGENT_ID")
        .env_remove("TASTY_PARENT_HOME")
        .args(["remote", "disconnect", "--workspace", workspace])
        .stdin(Stdio::null())
        .output()
        .expect("run tasty")
}

fn run(reply: Value, workspace: &str) -> (Option<i32>, String, String) {
    let home = tempfile::tempdir().expect("tempdir");
    fake_host(home.path(), reply);
    let out = disconnect(home.path(), workspace);
    (
        out.status.code(),
        String::from_utf8(out.stdout).expect("utf-8 stdout"),
        String::from_utf8(out.stderr).expect("utf-8 stderr"),
    )
}

const NOTICE: &str = "Workspace 6170 was already disconnected. Nothing changed.";

/// Given 이미 끊긴 저장 행이라는 응답, When `tasty remote disconnect` 를 실행하면, Then 종료 코드 0 으로
/// stdout 에 응답 JSON 만 쓰고 stderr 에 이미 끊겨 있다는 안내를 쓴다.
#[test]
fn an_already_disconnected_reply_prints_the_json_and_a_notice_on_stderr() {
    let result = json!({
        "disconnected": false, "already_disconnected": true, "workspace_id": 6170,
        "remote_workspace": 2055, "previous_state": "disconnected",
    });
    let (code, stdout, stderr) = run(json!({ "result": result }), "6170");
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("json stdout"),
        result
    );
    assert!(stderr.contains(NOTICE), "{stderr}");
}

/// Given 이번 호출이 끊었다는 응답, When 실행하면, Then 종료 코드 0 이고 안내는 없다.
#[test]
fn a_real_disconnect_prints_no_notice() {
    let result = json!({
        "disconnected": true, "already_disconnected": false, "workspace_id": 6170,
        "remote_workspace": 2055, "previous_state": "connected",
    });
    let (code, stdout, stderr) = run(json!({ "result": result }), "6170");
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("json stdout"),
        result
    );
    assert!(!stderr.contains("already disconnected"), "{stderr}");
}

/// Given mirror 가 아니라는 오류 응답, When 실행하면, Then 종료 코드 1 이고 stderr 에 오류와 `data:` 줄이 있다.
#[test]
fn a_refusal_exits_with_one_and_shows_the_reason() {
    let error = json!({
        "code": -32602,
        "message": "workspace 513 is not a mirror with an attach session",
        "data": {"reason": "not_a_mirror", "workspace_id": 513},
    });
    let (code, stdout, stderr) = run(json!({ "error": error }), "513");
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(
        stderr.contains("Error (-32602): workspace 513 is not a mirror with an attach session"),
        "{stderr}"
    );
    assert!(stderr.contains("\"reason\":\"not_a_mirror\""), "{stderr}");
}
