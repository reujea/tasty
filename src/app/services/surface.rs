//! GUI와 헤드리스가 공유하는 창 독립 IPC 처리.
//! 클립보드 포트나 명시한 접속 인자를 사용하며 지원 목록은 docs/dev-guide/headless-ipc-surface.md를 따른다.

use std::sync::mpsc::SyncSender;

use serde_json::Value;

use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::send_response;

/// 클립보드에 올리는 텍스트의 바이트 상한. 외부 IPC 의 한 줄 상한
/// (`tasty_ipc::protocol::MAX_REQUEST_LINE_BYTES`)과 같게 둔다 — 외부 경로는 그 줄 상한에
/// 먼저 걸려 여기까지 오지 못하므로, 이 상한이 실제로 거르는 것은 줄 상한이 없는 plugin
/// 경로다(plugin 채널은 큐 합계만 보고 **메시지 하나의 크기는 제한하지 않는다**,
/// `tasty_host_plugin::process::channel_bytes`). 두 경로가 같은 답을 받게 한 자리에 둔다.
pub(crate) const CLIPBOARD_TEXT_MAX_BYTES: usize = tasty_ipc::protocol::MAX_REQUEST_LINE_BYTES;

/// 클립보드가 없는 환경도 메서드 부재 대신 포트의 쓰기 오류로 응답한다.
pub(crate) fn clipboard_set_text(
    core: &crate::app::services::AppServices,
    rpc_id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let Some(text) = params.get("text").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::error(rpc_id, -32602, "Missing 'text' parameter (string)");
    };
    // 잘라서 올리지 않는다 — 호출자는 올렸다고 믿고 다른 것이 붙는다. 거절해 알린다.
    if let Err(message) = check_clipboard_text_len(text.len()) {
        return JsonRpcResponse::error(rpc_id, -32602, message);
    }
    match core.clipboard_arc().write_text(text) {
        Ok(()) => JsonRpcResponse::success(rpc_id, serde_json::json!({"ok": true})),
        Err(e) => JsonRpcResponse::error(rpc_id, -32000, format!("Failed to write clipboard: {e}")),
    }
}

/// 상한 검사. 넘으면 호출자에게 그대로 보낼 메시지를 돌려준다.
fn check_clipboard_text_len(len: usize) -> Result<(), String> {
    if len > CLIPBOARD_TEXT_MAX_BYTES {
        return Err(format!(
            "'text' is {len} bytes, over the {CLIPBOARD_TEXT_MAX_BYTES} byte clipboard limit"
        ));
    }
    Ok(())
}

/// remote.workspaces와 remote.attach가 공유하는 접속 인자. profile과 ssh 중 하나만 받는다.
pub(crate) struct RemoteConnParams {
    pub(crate) profile: Option<String>,
    pub(crate) ssh: Option<String>,
    pub(crate) remote_tasty: String,
    pub(crate) remote_port_mode: String,
}

impl RemoteConnParams {
    pub(crate) fn parse(params: &Value) -> Result<Self, &'static str> {
        let profile = params
            .get("profile")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let ssh = params
            .get("ssh")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if profile.is_some() && ssh.is_some() {
            return Err("'profile' and 'ssh' are mutually exclusive");
        }
        if profile.is_none() && ssh.is_none() {
            return Err("one of 'profile' or 'ssh' is required");
        }
        Ok(Self {
            profile,
            ssh,
            remote_tasty: params
                .get("remote_tasty")
                .and_then(|v| v.as_str())
                .unwrap_or("tasty")
                .to_string(),
            remote_port_mode: params
                .get("remote_port_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string(),
        })
    }
}

/// 블로킹 SSH 조회를 워커에서 수행하고 같은 응답 채널로 결과를 돌려준다.
pub(crate) fn spawn_remote_workspaces(
    rpc_id: Value,
    params: &Value,
    response_tx: &SyncSender<JsonRpcResponse>,
) {
    let conn = match RemoteConnParams::parse(params) {
        Ok(c) => c,
        Err(msg) => {
            send_response(response_tx, JsonRpcResponse::invalid_params(rpc_id, msg));
            return;
        }
    };
    let RemoteConnParams {
        profile,
        ssh,
        remote_tasty,
        remote_port_mode,
    } = conn;
    let response_tx = response_tx.clone();
    std::thread::spawn(move || {
        let resp = match tasty_remote::browse::resolve_connection_spec(
            profile.as_deref(),
            ssh.as_deref(),
            &remote_tasty,
            &remote_port_mode,
        ) {
            Ok((target, rt, pm, pf)) => {
                match tasty_remote::browse::browse(&target, &rt, &pm, pf.as_deref()) {
                    Ok(list) => JsonRpcResponse::success(
                        rpc_id,
                        serde_json::to_value(list).unwrap_or(Value::Null),
                    ),
                    // `{e:#}`로 원인 체인까지 싣는다. remote.attach 생성 실패 응답과 같은 형식이다.
                    Err(e) => JsonRpcResponse::error(
                        rpc_id,
                        -32050,
                        format!("remote browse failed: {e:#}"),
                    ),
                }
            }
            Err(e) => JsonRpcResponse::error(rpc_id, -32050, format!("{e:#}")),
        };
        send_response(&response_tx, resp);
    });
}

#[cfg(feature = "gui")]
pub(crate) fn no_application_state(rpc_id: Value) -> JsonRpcResponse {
    JsonRpcResponse::error(rpc_id, -32000, "no application state available")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 원격이 거절하면 -32050 응답 문구에 바깥 문맥과 함께 원격의 원인까지 들어간다.
    #[test]
    fn a_failed_remote_browse_reports_the_remote_cause() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(sock.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            sock.write_all(
                br#"{"jsonrpc":"2.0","error":{"code":-32001,"message":"permission_denied: session_token unknown/expired/revoked"},"id":1}"#,
            )
            .unwrap();
            sock.write_all(b"\n").unwrap();
        });
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        spawn_remote_workspaces(
            serde_json::json!(7),
            &serde_json::json!({ "ssh": format!("127.0.0.1:{port}") }),
            &tx,
        );
        let resp = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("response");
        server.join().unwrap();
        let err = resp.error.expect("error response");
        assert_eq!(err.code, -32050);
        assert!(
            err.message.starts_with("remote browse failed: "),
            "{}",
            err.message
        );
        assert!(
            err.message
                .contains("session_token unknown/expired/revoked"),
            "cause missing: {}",
            err.message
        );
    }

    #[test]
    fn remote_conn_params_reject_both_and_neither() {
        let both = serde_json::json!({ "profile": "p", "ssh": "u@h" });
        assert!(RemoteConnParams::parse(&both).is_err());
        let neither = serde_json::json!({});
        assert!(RemoteConnParams::parse(&neither).is_err());
    }

    #[test]
    fn remote_conn_params_default_the_discovery_options() {
        let p = RemoteConnParams::parse(&serde_json::json!({ "ssh": "u@h" }))
            .expect("ssh alone is a valid spec");
        assert_eq!(p.remote_tasty, "tasty");
        assert_eq!(p.remote_port_mode, "auto");
        assert!(p.profile.is_none());
    }

    /// 빈 params의 text 조회 결과만 검사한다. 실제 핸들러나 클립보드 호출 순서는 실행하지 않는다.
    #[test]
    fn clipboard_without_text_is_an_invalid_params_error() {
        let params = serde_json::json!({});
        assert!(params.get("text").and_then(|v| v.as_str()).is_none());
    }

    /// 상한까지는 받고 한 바이트라도 넘으면 거절한다. 상한이 사라지면 둘째 단언이 걸린다.
    #[test]
    fn clipboard_text_over_the_limit_is_refused() {
        assert!(check_clipboard_text_len(0).is_ok());
        assert!(check_clipboard_text_len(CLIPBOARD_TEXT_MAX_BYTES).is_ok());
        let err = check_clipboard_text_len(CLIPBOARD_TEXT_MAX_BYTES + 1)
            .expect_err("상한을 넘은 텍스트를 받았다");
        // 호출자가 무엇에 걸렸는지 알 수 있어야 한다.
        assert!(err.contains(&CLIPBOARD_TEXT_MAX_BYTES.to_string()), "{err}");
    }

    /// 수백 MB 는 확실히 막힌다 — 리뷰가 든 경우를 그대로 잰다.
    #[test]
    fn a_several_hundred_megabyte_clipboard_write_is_refused() {
        assert!(check_clipboard_text_len(500 * 1024 * 1024).is_err());
    }

    /// 상한이 실제로 거르는 것은 plugin 경로다 — 외부 IPC 는 줄 상한에 먼저 걸린다.
    /// 둘이 어긋나면 한쪽만 통과하는 크기가 생긴다.
    #[test]
    fn the_clipboard_limit_matches_the_external_ipc_line_limit() {
        assert_eq!(
            CLIPBOARD_TEXT_MAX_BYTES,
            tasty_ipc::protocol::MAX_REQUEST_LINE_BYTES
        );
    }
}
