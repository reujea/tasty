//! 기본 출력이 부모 ID 표인지 실제 바이너리로 확인한다. 표 갈래는 응답을 stdout 에 쓰는 자리라
//! 단위 시험이 지나지 못한다. 인스턴스를 띄우지 않고 임시 TASTY_HOME 의 포트 파일로 가짜 호스트에 붙인다.

#[path = "spawn_diag/mod.rs"]
mod spawn_diag;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{Receiver, channel};

use serde_json::{Value, json};

/// 모든 요청에 `result` 로 답하는 가짜 호스트. 받은 요청은 돌려준 채널로 읽는다.
fn fake_host(home: &Path, result: Value) -> Receiver<Value> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::fs::write(home.join("tasty.port"), port.to_string()).expect("port file");
    let (sent, received) = channel();
    std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let mut writer = stream.try_clone().expect("clone");
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let request: Value = serde_json::from_str(&line).expect("request json");
                let reply = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
                if writer.write_all(format!("{reply}\n").as_bytes()).is_err() {
                    return;
                }
                // 요청을 읽지 않는 시험도 있어 받는 쪽이 없어도 계속 답한다.
                drop(sent.send(request));
                line.clear();
            }
        }
    });
    received
}

fn tasty(home: &Path, args: &[&str]) -> Output {
    Command::new(spawn_diag::instance_bin())
        .env("TASTY_HOME", home)
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .env_remove("TASTY_AGENT_ID")
        .env_remove("TASTY_PARENT_HOME")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run tasty")
}

fn stdout(out: &Output) -> String {
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    String::from_utf8(out.stdout.clone()).expect("utf-8 stdout")
}

fn graph_response() -> Value {
    json!({
        "format": "json",
        "nodes": [
            {"id": "b", "name": "lint", "state": "ready", "skip": null},
            {"id": "c", "name": "merge", "state": "waiting", "skip": null},
            {"id": "a", "name": "build", "state": "succeeded", "skip": null},
        ],
        "edges": [
            {"from": "b", "to": "c", "kind": "depends_on"},
            {"from": "a", "to": "c", "kind": "depends_on"},
            {"from": "a", "to": "c", "kind": "binding"},
        ],
        "cycle": "a -> b -> a",
        "runner": {"running": false, "crashed": false, "ready_count": 1, "running_count": 0},
    })
}

const GRAPH_TABLE: &str = "| id | parents | name | state |\n\
                           |---|---|---|---|\n\
                           | a | - | build | succeeded |\n\
                           | c | a,b | merge | waiting |\n\
                           | b | - | lint | ready |\n";

const GRAPH_COMMANDS: [&[&str]; 2] = [
    &["agent", "task-graph", "--workspace", "1"],
    &["agent", "dag-get", "--id", "c:a"],
];

#[test]
fn graph_commands_print_the_table_by_default_and_keep_notes_on_stderr() {
    for command in GRAPH_COMMANDS {
        let home = tempfile::tempdir().expect("tempdir");
        let requests = fake_host(home.path(), graph_response());
        let out = tasty(home.path(), command);
        assert_eq!(stdout(&out), GRAPH_TABLE, "{command:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("cycle: a -> b -> a"), "{stderr}");
        assert!(stderr.contains("runner: stopped"), "{stderr}");
        let request = requests.recv().expect("request");
        assert_eq!(request["params"]["format"], "json", "{command:?}");
    }
}

#[test]
fn graph_commands_keep_json_and_dot_when_asked() {
    for command in GRAPH_COMMANDS {
        let home = tempfile::tempdir().expect("tempdir");
        fake_host(home.path(), graph_response());
        let json_args = [command, &["--format", "json"][..]].concat();
        let printed: Value =
            serde_json::from_str(&stdout(&tasty(home.path(), &json_args))).expect("json stdout");
        assert_eq!(printed, graph_response(), "{command:?}");

        let home = tempfile::tempdir().expect("tempdir");
        let dot = json!({"format": "dot", "dot": "digraph G {\n}\n", "cycle": null});
        let requests = fake_host(home.path(), dot);
        let dot_args = [command, &["--format", "dot"][..]].concat();
        assert_eq!(stdout(&tasty(home.path(), &dot_args)), "digraph G {\n}\n");
        let request = requests.recv().expect("request");
        assert_eq!(request["params"]["format"], "dot", "{command:?}");
    }
}
