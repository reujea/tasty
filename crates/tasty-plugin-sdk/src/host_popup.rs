//! 팝업을 실행 중에 선언하고 여는 호출(`popup.declare`, `popup.show`).

use serde_json::Value;
use tasty_plugin_protocol::{
    METHOD_POPUP_DECLARE, METHOD_POPUP_SHOW, PopupDeclSpec, PopupDeclareParams, PopupShowParams,
    PopupShowResult, PopupShowTarget,
};

use super::HostHandle;
use crate::error::PluginError;

impl HostHandle {
    /// 팝업을 선언한다. 같은 id면 바꾸고, 이미 열린 인스턴스는 연 때의 선언을 유지한다.
    ///
    /// 선언은 프로세스가 끝나면 지워지므로 [`crate::Plugin::on_start`]에서 선언한다.
    /// 같은 id의 매니페스트 선언보다 이 선언이 먼저다.
    pub fn declare_popup(
        &self,
        popup_id: impl Into<String>,
        decl: PopupDeclSpec,
    ) -> Result<(), PluginError> {
        let params = serde_json::to_value(PopupDeclareParams {
            popup_id: popup_id.into(),
            decl,
        })?;
        self.call(METHOD_POPUP_DECLARE, params).map(|_| ())
    }

    /// 선언한 팝업을 `target`에 열고 host가 발급한 instance_id를 돌려준다.
    ///
    /// 대상 종류는 선언의 범위 종류와 같아야 한다. 대상이 없거나 사라졌으면 열리지 않고
    /// [`PluginError::HostCall`]의 `code`가 `-32004`다. 형식·선언·종류 오류는 `-32602`다.
    /// 요청 처리 스레드에서 부르면 `popup.open` 콜백은 이 함수가 돌아온 뒤에 온다.
    pub fn show_popup(
        &self,
        popup_id: impl Into<String>,
        target: PopupShowTarget,
        context: Value,
    ) -> Result<u64, PluginError> {
        let params = serde_json::to_value(PopupShowParams {
            popup_id: popup_id.into(),
            target,
            context,
        })?;
        let result: PopupShowResult =
            serde_json::from_value(self.call(METHOD_POPUP_SHOW, params)?)?;
        Ok(result.instance_id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tasty_plugin_protocol::popup_decl::{PopupScopeKind, PopupSize};
    use tasty_type_geometry::length::LogicalPx;

    use super::*;
    use crate::host::{PendingCalls, deliver_ipc_result};

    /// host 쪽 소켓에서 호출 한 줄을 읽고 `reply`로 답하는 handle을 만든다.
    /// 읽은 (method, params)는 돌려받는 수신단으로 넘긴다.
    fn handle_answering(
        reply: Result<Value, (String, i32)>,
    ) -> (HostHandle, std::sync::mpsc::Receiver<(String, Value)>) {
        let pending: PendingCalls = Arc::new(Mutex::new(HashMap::new()));
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind localhost");
        let port = listener.local_addr().expect("addr").port();
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let (host_side, _) = listener.accept().expect("accept");
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let answer_to = Arc::clone(&pending);
        std::thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(host_side)
                .read_line(&mut line)
                .expect("read call line");
            let v: Value = serde_json::from_str(&line).expect("json line");
            let event = &v["event"];
            let call_id = event["call_id"].as_u64().expect("call_id");
            seen_tx
                .send((
                    event["method"].as_str().unwrap_or_default().to_string(),
                    event["params"].clone(),
                ))
                .expect("send seen");
            match reply {
                Ok(result) => deliver_ipc_result(&answer_to, call_id, Some(result), None, None),
                Err((msg, code)) => {
                    deliver_ipc_result(&answer_to, call_id, None, Some(msg), Some(code))
                }
            }
        });
        (
            HostHandle::new(Arc::new(Mutex::new(stream)), pending),
            seen_rx,
        )
    }

    #[test]
    fn declare_sends_the_partial_declaration() {
        let (handle, seen) = handle_answering(Ok(json!({})));
        let size = PopupSize {
            width: LogicalPx(360.0),
            height: LogicalPx(150.0),
        };
        handle
            .declare_popup("confirm", PopupDeclSpec::new(PopupScopeKind::Surface, size))
            .expect("declare");
        let (method, params) = seen.recv().expect("call");
        assert_eq!(method, "popup.declare");
        assert_eq!(
            params,
            json!({
                "popup_id": "confirm",
                "decl": { "scope_kind": "surface", "size": { "width": 360.0, "height": 150.0 } }
            })
        );
    }

    #[test]
    fn show_returns_the_instance_id() {
        let (handle, seen) = handle_answering(Ok(json!({ "instance_id": 17 })));
        let iid = handle
            .show_popup(
                "confirm",
                PopupShowTarget::Surface { id: 42 },
                json!({ "path": "/a.md" }),
            )
            .expect("show");
        assert_eq!(iid, 17);
        let (method, params) = seen.recv().expect("call");
        assert_eq!(method, "popup.show");
        assert_eq!(
            params,
            json!({
                "popup_id": "confirm",
                "target": { "kind": "surface", "id": 42 },
                "context": { "path": "/a.md" }
            })
        );
    }

    #[test]
    fn a_refused_show_keeps_the_host_code() {
        let (handle, _seen) =
            handle_answering(Err(("popup.show: no window is focused".into(), -32004)));
        let err = handle
            .show_popup("viewer", PopupShowTarget::FocusedWindow, Value::Null)
            .expect_err("refused");
        match err {
            PluginError::HostCall { code, message, .. } => {
                assert_eq!(code, Some(-32004));
                assert!(message.contains("no window is focused"), "{message}");
            }
            other => panic!("expected HostCall, got {other:?}"),
        }
    }
}
