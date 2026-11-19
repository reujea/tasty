//! `remote detach --all`: 이 인스턴스를 점유한 원격 client를 모두 강제로 끊는다.
//! 서버에는 GUI 조작과 같은 단위의 두 메서드(`attach.force_detach_workspace`,
//! `attach.force_detach`)만 두고, 전부 끊기는 `attach.list`를 읽어 대상마다 부른다.
//! 대상마다 응답을 그대로 출력하므로 무엇을 끊었는지 호출자가 확인할 수 있다.
//!
//! 목록을 읽은 뒤 대상이 사라지는 등 한 대상의 호출이 JSON-RPC 오류로 끝나면 그 항목에 오류를
//! 적고 나머지를 계속 처리한다. 전체 결과를 출력한 뒤 하나라도 실패했으면 종료 코드 1로 끝낸다.
//! 스크립트가 종료 코드로 부분 실패를 알고, 출력으로 어느 대상이 실패했는지 확인하게 하려는 것이다.
//! 연결 자체가 끊긴 오류는 뒤 대상도 부를 수 없으므로 그 항목에 적고 멈춘다.
//! workspace를 끊은 뒤 surface 목록을 다시 읽지 못하면 그 오류를 `list_error`에 적고 멈춘다.
//! 이때도 이미 끊은 결과를 출력하고 종료 코드 1로 끝낸다.

use std::net::TcpStream;

use anyhow::Result;
use serde_json::{Value, json};
use tasty_ipc::client::{IpcConnection, JsonRpcCallError};

use crate::dispatch::{ClientCommand, ClientCtx};
use crate::out::outln;
use crate::request::rpc_request;

pub struct RemoteDetachAll;

impl ClientCommand for RemoteDetachAll {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        let mut conn = connect(ctx.port_file)?;
        // 첫 목록을 읽지 못하면 아직 아무것도 끊지 않았으므로 그 오류로 끝낸다.
        let run = detach_all(&mut conn).unwrap_or_else(|e| crate::rpc_error::exit_with(&e));
        outln!("{}", run.output())?;
        if run.failed {
            std::process::exit(1);
        }
        Ok(())
    }
}

/// 요청 하나를 보내고 결과를 받는 연결. 시험에서 응답을 바꿔 끼우려고 둔다.
trait Rpc {
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}

impl Rpc for IpcConnection {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.send(&rpc_request(method, params))
    }
}

/// 첫 목록 조회가 실패하면 `Err`다. 그 뒤의 실패는 결과에 적고 계속하거나 멈춘다.
fn detach_all(conn: &mut impl Rpc) -> Result<DetachRun> {
    let mut run = DetachRun::default();
    let list = conn.call("attach.list", json!({}))?;
    for id in occupied_ids(&list, "workspaces", "workspace_id") {
        if run.connection_lost {
            break;
        }
        let item = run.call(conn, "attach.force_detach_workspace", "workspace_id", id);
        run.workspaces.push(item);
    }
    if run.connection_lost {
        return Ok(run);
    }
    // workspace 해제는 멤버 surface 점유도 지우므로 남은 surface는 목록을 다시 읽어 고른다.
    // 이미 끊은 workspace가 있으므로 여기서 실패해도 지금까지의 결과를 출력하고 실패로 끝낸다.
    let list = match conn.call("attach.list", json!({})) {
        Ok(list) => list,
        Err(e) => {
            run.failed = true;
            run.connection_lost = true;
            run.list_error = Some(error_value(&e));
            return Ok(run);
        }
    };
    for id in occupied_ids(&list, "attached", "surface_id") {
        if run.connection_lost {
            break;
        }
        let item = run.call(conn, "attach.force_detach", "surface_id", id);
        run.surfaces.push(item);
    }
    Ok(run)
}

#[derive(Default)]
struct DetachRun {
    workspaces: Vec<Value>,
    surfaces: Vec<Value>,
    /// workspace를 끊은 뒤 surface 목록을 다시 읽지 못한 오류.
    list_error: Option<Value>,
    failed: bool,
    connection_lost: bool,
}

impl DetachRun {
    /// 한 대상을 끊는다. 실패하면 대상 ID와 오류를 담은 항목을 돌려준다.
    fn call(&mut self, conn: &mut impl Rpc, method: &str, key: &str, id: u64) -> Value {
        match conn.call(method, json!({ key: id })) {
            Ok(value) => value,
            Err(e) => {
                self.failed = true;
                if e.downcast_ref::<JsonRpcCallError>().is_none() {
                    self.connection_lost = true;
                }
                failure_item(key, id, error_value(&e))
            }
        }
    }

    fn output(&self) -> Value {
        let mut out = json!({ "workspaces": self.workspaces, "surfaces": self.surfaces });
        if let Some(error) = &self.list_error {
            out["list_error"] = error.clone();
        }
        out
    }
}

/// JSON-RPC 오류는 `code`·`message`, 연결 오류는 `message`만 담는다.
fn error_value(e: &anyhow::Error) -> Value {
    match e.downcast_ref::<JsonRpcCallError>() {
        Some(rpc) => json!({ "code": rpc.code, "message": rpc.message }),
        None => json!({ "message": e.to_string() }),
    }
}

fn failure_item(key: &str, id: u64, error: Value) -> Value {
    json!({ "force_detached": false, key: id, "error": error })
}

fn connect(port_file: Option<&str>) -> Result<IpcConnection> {
    let port = crate::port_file::read_port(port_file)?;
    let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.request.connect_failed",
                &port.to_string(),
                &e.to_string()
            )
        )
    })?;
    IpcConnection::new(stream)
}

/// `attach.list` 응답의 한 목록에서 점유된 대상 ID를 꺼낸다.
fn occupied_ids(list: &Value, key: &str, field: &str) -> Vec<u64> {
    list.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get(field).and_then(Value::as_u64))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Rpc, detach_all, failure_item, occupied_ids};
    use anyhow::Result;
    use serde_json::{Value, json};
    use std::collections::VecDeque;

    /// 정해 둔 응답을 차례로 돌려주고 받은 호출을 기록한다.
    struct Script {
        replies: VecDeque<Result<Value>>,
        calls: Vec<(String, Value)>,
    }

    impl Rpc for Script {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            self.calls.push((method.to_owned(), params));
            self.replies.pop_front().expect("unexpected call")
        }
    }

    /// workspace를 끊은 뒤 두 번째 목록 조회가 실패해도 끊은 결과를 남기고 실패로 끝낸다.
    #[test]
    fn a_failed_second_list_keeps_the_workspaces_already_detached() {
        let mut conn = Script {
            replies: VecDeque::from([
                Ok(json!({ "workspaces": [{ "workspace_id": 513 }], "attached": [] })),
                Ok(json!({ "force_detached": true, "workspace_id": 513 })),
                Err(anyhow::anyhow!("connection reset")),
            ]),
            calls: Vec::new(),
        };
        let run = detach_all(&mut conn).expect("the first list succeeded");
        assert!(run.failed && run.connection_lost);
        let out = run.output();
        assert_eq!(out["workspaces"][0]["workspace_id"], 513);
        assert_eq!(out["surfaces"], json!([]));
        assert_eq!(out["list_error"]["message"], "connection reset");
        assert_eq!(conn.calls.len(), 3);
    }

    /// 첫 목록 조회 실패는 아직 아무것도 끊지 않았으므로 그대로 오류다.
    #[test]
    fn a_failed_first_list_is_an_error() {
        let mut conn = Script {
            replies: VecDeque::from([Err(anyhow::anyhow!("connection reset"))]),
            calls: Vec::new(),
        };
        assert!(detach_all(&mut conn).is_err());
    }

    #[test]
    fn a_failed_target_keeps_its_id_and_the_error() {
        let item = failure_item(
            "workspace_id",
            513,
            json!({ "code": -32602, "message": "workspace not found" }),
        );
        assert_eq!(item["workspace_id"], 513);
        assert_eq!(item["force_detached"], false);
        assert_eq!(item["error"]["code"], -32602);
    }

    #[test]
    fn occupied_ids_reads_both_lists_of_attach_list() {
        let list = json!({
            "attached": [
                { "surface_id": 4, "holder": 46, "granted_seq": 1 },
                { "surface_id": 9, "holder": 47, "granted_seq": 2 },
            ],
            "workspaces": [{ "workspace_id": 513, "holder": 46, "granted_seq": 1 }],
        });
        assert_eq!(occupied_ids(&list, "workspaces", "workspace_id"), [513]);
        assert_eq!(occupied_ids(&list, "attached", "surface_id"), [4, 9]);
        assert!(occupied_ids(&json!({}), "attached", "surface_id").is_empty());
    }
}
