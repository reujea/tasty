//! 점유 워크스페이스의 구조가 **forward 가 아닌 원인**으로 바뀌었을 때와, forward 가 이미
//! 사라진 대상을 지목했을 때의 서버 응답을 loopback `TcpStream` 으로 실제 실행 중인 서버에
//! 대해 본다. 같은 시험이 gui 조합과 `--no-default-features`(headless) 조합에서 각각 돌아
//! 두 빌드의 스트림 처리 자리(`app/event_handler.rs` · `boot/headless_stream.rs`)를 함께
//! 고정한다.
//!
//! - `a_shell_exit_on_the_server_reaches_the_holder_as_a_delta` — 서버에서 셸이 끝나 닫힌
//!   surface 가 `structural_delta` 로 holder 에게 간다(ADR-0481).
//! - `a_forward_naming_a_gone_surface_is_answered_like_ipc` — 워크스페이스는 살아 있는데
//!   anchor surface 가 사라졌으면 IPC 와 같은 `no live surface N` 사유로 거절된다(ADR-0482).

// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다 — 전수 가드
// (`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)가 테스트 본문을 제외하므로, 여기서 나는
// `let_underscore_must_use` 경고는 정책상 조치 대상이 될 수 없다. 끄지 않으면
// 프로덕션의 진짜 신호가 그 안에 묻힌다 — `docs/dev-guide/error-handling.md`.
#![allow(clippy::let_underscore_must_use)]

mod attach_common;
mod common;

use std::net::TcpStream;
use std::time::{Duration, Instant};

use attach_common::{
    TAG_CONTROL, open_workspace_attach, read_frame, write_control_frame, write_workspace_input,
};
use serde_json::{Value, json};

/// forward split 으로 두 번째 surface 를 만들고 그 id 를 돌려준다.
fn split_and_read_new_surface(stream: &mut TcpStream, anchor: u64) -> u64 {
    write_control_frame(
        stream,
        &json!({ "event": "structural_op", "op_id": 1, "op": {
            "kind": "split_surface", "surface_id": anchor, "direction": "vertical",
        }}),
    );
    let delta = next_structural(stream, "structural_delta");
    surface_ids(&delta)
        .into_iter()
        .find(|&sid| sid != anchor)
        .unwrap_or_else(|| panic!("split delta has no new surface: {delta:?}"))
}

/// 구조 프레임 하나를 기다리는 상한. 서버는 그동안에도 터미널 출력·활동 프레임을 계속
/// 보내므로 소켓 read timeout 은 걸리지 않는다 — 기다리던 것이 안 오는 회귀가 멈춤이 아니라
/// 실패로 끝나게 벽시계로 끊는다.
const STRUCTURAL_WAIT: Duration = Duration::from_secs(20);

/// `event` 이름의 구조 control 프레임을 기다린다. 다른 구조 프레임이 먼저 오면 실패 —
/// 이 시험들은 순서를 본다.
fn next_structural(stream: &mut TcpStream, event: &str) -> Value {
    let deadline = Instant::now() + STRUCTURAL_WAIT;
    loop {
        assert!(
            Instant::now() < deadline,
            "no {event} within {STRUCTURAL_WAIT:?}"
        );
        let (tag, payload) = read_frame(stream);
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        match v.get("event").and_then(|e| e.as_str()) {
            Some(e) if e == event => return v,
            Some("structural_result") if event != "structural_result" => {
                assert_eq!(v["ok"], true, "forward rejected: {v:?}");
            }
            Some(e @ ("structural_delta" | "structural_result")) => {
                panic!("expected {event}, got {e}: {v:?}")
            }
            _ => continue,
        }
    }
}

/// `until` 전에 `structural_delta` 가 오면 돌려준다. 다른 구조 프레임은 실패다.
fn structural_delta_until(stream: &mut TcpStream, until: Instant) -> Option<Value> {
    while Instant::now() < until {
        let (tag, payload) = read_frame(stream);
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        match v.get("event").and_then(|e| e.as_str()) {
            Some("structural_delta") => return Some(v),
            Some("structural_result") => panic!("unexpected structural_result: {v:?}"),
            _ => continue,
        }
    }
    None
}

fn surface_ids(delta: &Value) -> Vec<u64> {
    delta["surfaces"]
        .as_array()
        .expect("surfaces array")
        .iter()
        .filter_map(|s| s["remote_id"].as_u64())
        .collect()
}

/// 서버에서 셸이 끝나면(PTY 종료) 그 surface 는 forward 없이 닫힌다. holder 는 다음 forward
/// 를 기다리지 않고 닫힌 뒤의 트리를 `structural_delta` 로 받아야 한다 — 그 전에는 mirror
/// 가 사라진 탭을 계속 보였다.
#[test]
fn a_shell_exit_on_the_server_reaches_the_holder_as_a_delta() {
    let server = common::shared();
    let ws = server.create_workspace("structure-sync-exit");
    let mut stream = open_workspace_attach(server.port(), ws.id);
    let b = split_and_read_new_surface(&mut stream, ws.surface_id);

    // 셸이 첫 입력을 놓쳐도 시험이 멈추지 않게 delta 가 올 때까지 간격을 두고 다시 보낸다 —
    // 닫힌 뒤의 재전송은 서버가 버린다(살아 있지 않은 surface 로의 입력).
    let deadline = Instant::now() + STRUCTURAL_WAIT;
    let delta = loop {
        assert!(
            Instant::now() < deadline,
            "no structural_delta within {STRUCTURAL_WAIT:?}"
        );
        write_workspace_input(&mut stream, b as u32, b"exit\r");
        let resend_at = Instant::now() + Duration::from_secs(2);
        if let Some(d) = structural_delta_until(&mut stream, resend_at) {
            break d;
        }
    };
    assert_eq!(delta["workspace_id"], ws.id);
    assert_eq!(
        surface_ids(&delta),
        vec![ws.surface_id],
        "셸이 끝난 surface 가 빠진 트리여야 한다: {delta:?}"
    );
}

/// 워크스페이스는 살아 있는데 forward 가 지목한 surface 가 이미 없으면, 사유는 IPC 가 같은
/// 상황(살아 있지 않은 id 지목)에 쓰는 문구다 — 전에는 사실과 다른 "workspace not found"
/// 였다. 요청 이름 자리는 anchor 를 지목한 forward op 의 wire 이름이다.
#[test]
fn a_forward_naming_a_gone_surface_is_answered_like_ipc() {
    let server = common::shared();
    let ws = server.create_workspace("structure-sync-gone");
    let mut stream = open_workspace_attach(server.port(), ws.id);
    let b = split_and_read_new_surface(&mut stream, ws.surface_id);

    let close = json!({ "event": "structural_op", "op_id": 2, "op": {
        "kind": "close_surface", "surface_id": b,
    }});
    write_control_frame(&mut stream, &close);
    let first = next_structural(&mut stream, "structural_result");
    assert_eq!(first["ok"], true, "{first:?}");
    next_structural(&mut stream, "structural_delta");

    write_control_frame(&mut stream, &close);
    let again = next_structural(&mut stream, "structural_result");
    assert_eq!(again["ok"], false, "{again:?}");
    let reason = again["reason"].as_str().expect("reason");
    assert!(
        reason.starts_with(&format!(
            "no live surface {b} (named by 'structural_op.close_surface'); "
        )),
        "IPC 와 같은 문구여야 한다: {reason}"
    );

    // 대조: 같은 상황의 IPC 거절 문구와 앞머리까지 같다(요청 이름 자리만 다르다).
    let resp = server.call_raw("surface.close", json!({ "surface_id": b }));
    let ipc = resp["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("IPC close of a gone surface must fail: {resp:?}"))
        .to_string();
    assert!(
        ipc.contains(&format!("no live surface {b} (named by 'surface.close'); ")),
        "대조군 IPC 문구: {ipc}"
    );
    assert_eq!(
        reason.split_once("); ").map(|(_, tail)| tail),
        ipc.split_once("); ").map(|(_, tail)| tail),
        "꼬리 문장까지 같아야 한다"
    );
}
