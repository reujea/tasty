//! `remote detach --all`: 이 인스턴스를 점유한 원격 client를 모두 강제로 끊는다.
//! 서버에는 GUI 조작과 같은 단위의 두 메서드(`attach.force_detach_workspace`,
//! `attach.force_detach`)만 두고, 전부 끊기는 `attach.list`를 읽어 대상마다 부른다.
//! 대상마다 응답을 그대로 출력하므로 무엇을 끊었는지 호출자가 확인할 수 있다.

use std::net::TcpStream;

use anyhow::Result;
use serde_json::{Value, json};
use tasty_ipc::client::IpcConnection;

use crate::dispatch::{ClientCommand, ClientCtx};
use crate::out::outln;
use crate::request::rpc_request;

pub struct RemoteDetachAll;

impl ClientCommand for RemoteDetachAll {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        let mut conn = connect(ctx.port_file)?;
        let list = call(&mut conn, "attach.list", json!({}));
        let mut workspaces = Vec::new();
        for id in occupied_ids(&list, "workspaces", "workspace_id") {
            workspaces.push(call(
                &mut conn,
                "attach.force_detach_workspace",
                json!({ "workspace_id": id }),
            ));
        }
        // workspace 해제는 멤버 surface 점유도 지우므로 남은 surface는 목록을 다시 읽어 고른다.
        let list = call(&mut conn, "attach.list", json!({}));
        let mut surfaces = Vec::new();
        for id in occupied_ids(&list, "attached", "surface_id") {
            surfaces.push(call(
                &mut conn,
                "attach.force_detach",
                json!({ "surface_id": id }),
            ));
        }
        outln!(
            "{}",
            json!({ "workspaces": workspaces, "surfaces": surfaces })
        )?;
        Ok(())
    }
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

/// 실패하면 그 오류로 끝낸다. 앞서 끊은 대상은 되돌리지 않는다.
fn call(conn: &mut IpcConnection, method: &str, params: Value) -> Value {
    conn.send(&rpc_request(method, params))
        .unwrap_or_else(|e| crate::rpc_error::exit_with(&e))
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
    use super::occupied_ids;
    use serde_json::json;

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
