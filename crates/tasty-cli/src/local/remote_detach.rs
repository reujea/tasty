//! `remote detach --all`: 이 인스턴스를 점유한 원격 client를 모두 강제로 끊는다.
//! 서버에는 GUI 조작과 같은 단위의 두 메서드(`attach.force_detach_workspace`,
//! `attach.force_detach`)만 두고, 전부 끊기는 `attach.list`를 읽어 대상마다 부른다.
//! 대상마다 응답을 그대로 출력하므로 무엇을 끊었는지 호출자가 확인할 수 있다.
//!
//! 목록을 읽은 뒤 대상이 사라지는 등 한 대상의 호출이 JSON-RPC 오류로 끝나면 그 항목에 오류를
//! 적고 나머지를 계속 처리한다. 전체 결과를 출력한 뒤 하나라도 실패했으면 종료 코드 1로 끝낸다.
//! 스크립트가 종료 코드로 부분 실패를 알고, 출력으로 어느 대상이 실패했는지 확인하게 하려는 것이다.
//! 연결 자체가 끊긴 오류는 뒤 대상도 부를 수 없으므로 그 항목에 적고 멈춘다.

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
        let mut run = DetachRun::default();
        let list = read_list(&mut conn);
        for id in occupied_ids(&list, "workspaces", "workspace_id") {
            if run.connection_lost {
                break;
            }
            let item = run.call(
                &mut conn,
                "attach.force_detach_workspace",
                "workspace_id",
                id,
            );
            run.workspaces.push(item);
        }
        // workspace 해제는 멤버 surface 점유도 지우므로 남은 surface는 목록을 다시 읽어 고른다.
        if !run.connection_lost {
            let list = read_list(&mut conn);
            for id in occupied_ids(&list, "attached", "surface_id") {
                if run.connection_lost {
                    break;
                }
                let item = run.call(&mut conn, "attach.force_detach", "surface_id", id);
                run.surfaces.push(item);
            }
        }
        outln!(
            "{}",
            json!({ "workspaces": run.workspaces, "surfaces": run.surfaces })
        )?;
        if run.failed {
            std::process::exit(1);
        }
        Ok(())
    }
}

#[derive(Default)]
struct DetachRun {
    workspaces: Vec<Value>,
    surfaces: Vec<Value>,
    failed: bool,
    connection_lost: bool,
}

impl DetachRun {
    /// 한 대상을 끊는다. 실패하면 대상 ID와 오류를 담은 항목을 돌려준다.
    fn call(&mut self, conn: &mut IpcConnection, method: &str, key: &str, id: u64) -> Value {
        match conn.send(&rpc_request(method, json!({ key: id }))) {
            Ok(value) => value,
            Err(e) => {
                self.failed = true;
                let error = match e.downcast_ref::<JsonRpcCallError>() {
                    Some(rpc) => json!({ "code": rpc.code, "message": rpc.message }),
                    None => {
                        self.connection_lost = true;
                        json!({ "message": e.to_string() })
                    }
                };
                failure_item(key, id, error)
            }
        }
    }
}

fn failure_item(key: &str, id: u64, error: Value) -> Value {
    json!({ "force_detached": false, key: id, "error": error })
}

/// 목록을 읽지 못하면 대상을 정할 수 없으므로 그 오류로 끝낸다.
fn read_list(conn: &mut IpcConnection) -> Value {
    conn.send(&rpc_request("attach.list", json!({})))
        .unwrap_or_else(|e| crate::rpc_error::exit_with(&e))
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
    use super::{failure_item, occupied_ids};
    use serde_json::json;

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
