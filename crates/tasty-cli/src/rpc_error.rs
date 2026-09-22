//! 호스트가 JSON-RPC 오류로 답했을 때 CLI 가 stderr 에 내는 모양.
//!
//! 첫 줄은 종전 그대로 `Error (<code>): <message>` 다 — 그 줄을 파싱하는 스크립트가 있다.
//! 응답에 `error.data` 가 있으면 **둘째 줄** `data: <한 줄 JSON>` 으로 원형 그대로 싣는다.
//! `reason` · `storage_failure` 처럼 IPC 호출자가 분기하는 실패 분류를 CLI 호출자도 같은
//! 값으로 얻게 하는 것이 목적이다(원칙 2). 형식의 근거는
//! `docs/adr/0512-the-cli-relays-ipc-error-data-on-a-second-stderr-line.md`.

use tasty_ipc::client::JsonRpcCallError;

/// 둘째 줄의 접두. 값 자리는 로케일을 안 타는 JSON 이라 이 접두도 번역하지 않는다 —
/// 첫 줄의 `Error (` 와 같은 응답 형식의 일부다.
pub(crate) const DATA_LINE_PREFIX: &str = "data: ";

/// stderr 에 낼 줄들. JSON-RPC 오류가 아니면 종전처럼 `Display` 한 줄이다.
pub(crate) fn render(e: &anyhow::Error) -> String {
    let Some(rpc) = e.downcast_ref::<JsonRpcCallError>() else {
        return e.to_string();
    };
    match rpc.data.as_ref().filter(|d| !d.is_null()) {
        Some(data) => format!(
            "{rpc}\n{DATA_LINE_PREFIX}{}",
            // compact 직렬화는 중첩 값에도 개행을 넣지 않는다 — 둘째 줄은 정확히 한 줄이다.
            serde_json::to_string(data).unwrap_or_default()
        ),
        None => rpc.to_string(),
    }
}

/// 오류를 stderr 에 내고 종료 코드 1 로 끝낸다.
pub(crate) fn exit_with(e: &anyhow::Error) -> ! {
    crate::out::errln!("{}", render(e));
    std::process::exit(1);
}

/// 오류를 `main` 까지 올리는 경로(스트리밍 명령)용 — std 가 찍는 문구에 `data` 줄을 싣는다.
///
/// 그 경로의 첫 줄은 std 가 앞에 붙이는 `Error: ` 까지 포함해 `Error: Error (<code>): <message>`
/// 이고, 그 모양을 파싱하는 쪽이 있다. [`exit_with`] 로 옮기면 그 접두가 빠지므로 옮기지 않고,
/// 올라가는 값의 문구만 [`render`] 의 두 줄로 바꾼다. `data` 가 없으면 **값을 그대로** 돌려준다 —
/// 그 출력은 한 글자도 안 바뀐다.
pub(crate) fn with_data_line(e: anyhow::Error) -> anyhow::Error {
    let has_data = e
        .downcast_ref::<JsonRpcCallError>()
        .is_some_and(|rpc| rpc.data.as_ref().is_some_and(|d| !d.is_null()));
    if has_data {
        // 원인 체인(`Caused by:`)을 달지 않는다 — std 의 출력에 그 블록이 붙어 두 줄 모양이 깨진다.
        anyhow::anyhow!("{}", render(&e))
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call_error(data: Option<serde_json::Value>) -> anyhow::Error {
        JsonRpcCallError {
            code: -32603,
            message: "memory store write failed".into(),
            data,
        }
        .into()
    }

    #[test]
    fn data_rides_on_a_second_line_as_compact_json() {
        let e = call_error(Some(json!({
            "storage_failure": "read_only",
            "detail": { "path": "/x", "nested": [1, 2] }
        })));
        let rendered = render(&e);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 2, "{rendered}");
        assert_eq!(lines[0], "Error (-32603): memory store write failed");
        let data: serde_json::Value = serde_json::from_str(
            lines[1]
                .strip_prefix(DATA_LINE_PREFIX)
                .expect("second line starts with the data prefix"),
        )
        .expect("second line is JSON");
        assert_eq!(data["storage_failure"], "read_only");
        assert_eq!(data["detail"]["nested"], json!([1, 2]));
    }

    #[test]
    fn without_data_the_line_is_unchanged() {
        assert_eq!(
            render(&call_error(None)),
            "Error (-32603): memory store write failed"
        );
        assert_eq!(
            render(&call_error(Some(serde_json::Value::Null))),
            "Error (-32603): memory store write failed"
        );
    }

    /// std 가 `main` 의 `Err` 를 찍는 모양(`Error: {:?}`)으로 재 본다 — 첫 줄은 종전 그대로이고
    /// `data` 는 둘째 줄이다. `data` 가 없으면 한 줄 그대로다.
    #[test]
    fn the_main_path_keeps_its_first_line_and_adds_the_data_line() {
        // `RUST_BACKTRACE` 가 켜진 환경이면 anyhow 가 뒤에 backtrace 블록을 붙인다 — 그 앞까지만 본다.
        let termination = |e: anyhow::Error| {
            let full = format!("Error: {:?}", with_data_line(e));
            match full.split_once("\n\nStack backtrace:") {
                Some((head, _)) => head.to_string(),
                None => full,
            }
        };
        assert_eq!(
            termination(call_error(Some(json!({ "reason": "evicted" })))),
            "Error: Error (-32603): memory store write failed\ndata: {\"reason\":\"evicted\"}"
        );
        assert_eq!(
            termination(call_error(None)),
            "Error: Error (-32603): memory store write failed"
        );
        assert_eq!(
            termination(call_error(Some(serde_json::Value::Null))),
            "Error: Error (-32603): memory store write failed"
        );
        let other = anyhow::anyhow!("tasty instance closed the connection");
        assert_eq!(
            termination(other),
            "Error: tasty instance closed the connection"
        );
    }

    #[test]
    fn other_errors_keep_their_display() {
        let e = anyhow::anyhow!("tasty instance closed the connection");
        assert_eq!(render(&e), "tasty instance closed the connection");
    }
}
