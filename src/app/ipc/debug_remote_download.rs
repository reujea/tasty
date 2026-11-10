//! `debug.remote_download.*` — mirror workspace 의 원격 파일 하나를 "다운로드 후 실행" 한다.
//! 버튼·확인 팝업은 디자인이 정해지지 않아 아직 없다. 그동안 사용자 조작(시작·1 MiB 초과 확인·취소)을
//! 격리 인스턴스에서 재현하는 진입점이다. 진행과 결과는 메인 루프의 [`App::pump_file_downloads`] 가 처리한다.

use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::server::{IpcCommand, send_response};
use crate::remote::file_download::client as downloads;

impl App {
    pub(super) fn ipc_handle_debug_remote_download(&mut self, cmd: &IpcCommand) -> IpcStep {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let p = &cmd.request.params;
        let response = match cmd.request.method.as_str() {
            "debug.remote_download.start" => {
                match (
                    params::require_u32(p, "workspace_id", &id),
                    p.get("path").and_then(|v| v.as_str()),
                ) {
                    (Err(e), _) => e,
                    (Ok(_), None) => {
                        JsonRpcResponse::error(id, -32602, "missing string param: path".to_string())
                    }
                    (Ok(ws), Some(path)) => {
                        if !self
                            .remote
                            .sessions
                            .iter()
                            .any(|s| s.state.local_workspace == ws)
                        {
                            JsonRpcResponse::error(
                                id,
                                -32602,
                                format!("workspace {ws} is not an attach mirror"),
                            )
                        } else {
                            match downloads::client().start(ws, path.to_string()) {
                                Ok(download) => JsonRpcResponse::success(
                                    id,
                                    serde_json::json!({ "download_id": download }),
                                ),
                                Err(failure) => JsonRpcResponse::success(
                                    id,
                                    serde_json::json!({ "refused": format!("{failure:?}") }),
                                ),
                            }
                        }
                    }
                }
            }
            "debug.remote_download.confirm" | "debug.remote_download.cancel" => {
                match p.get("download_id").and_then(|v| v.as_u64()) {
                    None => JsonRpcResponse::error(
                        id,
                        -32602,
                        "missing integer param: download_id".to_string(),
                    ),
                    Some(download) => {
                        let mut client = downloads::client();
                        if cmd.request.method.ends_with("confirm") {
                            client.confirm(download);
                        } else {
                            client.cancel(download);
                        }
                        JsonRpcResponse::success(id, serde_json::json!({}))
                    }
                }
            }
            other => JsonRpcResponse::error(id, -32601, format!("unknown method: {other}")),
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }
}
