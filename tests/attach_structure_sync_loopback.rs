//! 셸 종료로 바뀐 구조가 attach holder에 전달되고, 이미 사라진 surface의 요청은 IPC와 같은 이유로 거절되는지 확인한다.
//! holder가 아닌 로컬 IPC가 점유된 workspace에 탭을 끼워 넣거나 닫는 우회 경로도 거절되는지 확인한다.
//! 각 실행은 빌드한 GUI 또는 헤드리스 서버 경로 하나를 검증한다(docs/dev-guide/attach-behavior.md).

// 이유: 시험의 정리용 결과 무시는 제품 코드의 오류 처리 목록과 구분한다.
#![allow(clippy::let_underscore_must_use)]

mod attach_common;
mod common;

use std::net::TcpStream;
use std::time::{Duration, Instant};

use attach_common::{
    TAG_CONTROL, open_workspace_attach, read_frame, write_control_frame, write_workspace_input,
};
use serde_json::{Value, json};

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

/// 다른 출력 프레임이 계속 오면 소켓 타임아웃이 발생하지 않을 수 있어 별도 경과 시간 제한을 둔다.
/// holder의 구조 요청도 IPC 요청처럼 journal commit을 거치므로 하네스의 응답 대기 상한을 쓴다.
const STRUCTURAL_WAIT: Duration = common::RESPONSE_WAIT;

/// 예상한 구조 프레임을 기다린다. 성공한 structural_result는 delta 앞에서 건너뛰고 다른 순서는 실패시킨다.
fn next_structural(stream: &mut TcpStream, event: &str) -> Value {
    let deadline = Instant::now() + STRUCTURAL_WAIT;
    // 기한 검사가 프레임 소비 앞에 있다. 기한 뒤 재읽기는 소켓 timeout 까지 막히는 읽기로 프레임을 하나 더
    // 소비하게 되어 읽기 계약이 달라지므로 원래 루프를 둔다.
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

#[test]
fn a_shell_exit_on_the_server_reaches_the_holder_as_a_delta() {
    let server = common::shared();
    let ws = server.create_workspace("structure-sync-exit");
    let mut stream = open_workspace_attach(server.port(), ws.id);
    let b = split_and_read_new_surface(&mut stream, ws.surface_id);

    // 셸이 아직 입력을 못 받을 수 있어 delta가 올 때까지 exit를 반복한다. 종료된 surface의 입력은 서버가 버린다.
    let deadline = Instant::now() + STRUCTURAL_WAIT;
    // 기한 검사가 exit 재전송 앞에 있다. 기한 뒤 읽기는 exit 를 다시 써서 기다리던 delta 를 새로 만들 수 있고,
    // 재전송 없이 delta 를 관측할 길이 없어 원래 루프를 둔다.
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

/// workspace는 남아 있고 surface만 사라진 경우의 거절을 IPC와 비교한다. 요청 이름은 structural op에 맞춰 다르다.
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

/// forward된 move_surface의 target이 점유하지 않은 workspace에 있으면 서버가 거절한다.
/// 받아들이면 source가 다른 workspace로 옮겨 가고 그 자리의 surface가 파괴된다.
#[test]
fn a_move_surface_target_outside_the_held_workspace_is_rejected() {
    let server = common::shared();
    let held = server.create_workspace("structure-sync-move-held");
    let other = server.create_workspace("structure-sync-move-other");
    let mut stream = open_workspace_attach(server.port(), held.id);
    // source가 떠나도 held가 비지 않도록 surface를 하나 더 만든다.
    let src = split_and_read_new_surface(&mut stream, held.surface_id);

    write_control_frame(
        &mut stream,
        &json!({ "event": "structural_op", "op_id": 2, "op": {
            "kind": "move_surface", "source_surface_id": src,
            "target_surface_id": other.surface_id,
        }}),
    );
    let result = next_structural(&mut stream, "structural_result");
    assert_eq!(
        result["ok"], false,
        "다른 workspace target 을 받아들였다: {result:?}"
    );
    let reason = result["reason"].as_str().expect("reason");
    assert!(
        reason.starts_with(&format!(
            "no live surface {} (named by 'structural_op.move_surface'); ",
            other.surface_id
        )),
        "점유 범위 밖 target 은 없는 surface 와 같은 문구로 거절한다: {reason}"
    );

    let list = server.call("surface.list", json!({}));
    let arr = list.as_array().expect("surface.list array");
    let ws_of = |sid: u64| {
        arr.iter()
            .find(|s| s["id"].as_u64() == Some(sid))
            .map(|s| s["workspace_id"].clone())
    };
    assert_eq!(
        ws_of(other.surface_id),
        Some(json!(other.id)),
        "다른 workspace 의 surface 는 그대로여야 한다"
    );
    assert_eq!(
        ws_of(src),
        Some(json!(held.id)),
        "source 는 held 에 남아야 한다"
    );
}

/// preset.apply와 pty.attach_surface도 tab.create처럼 점유된 workspace의 구조를 바꾸므로 거절돼야 한다.
/// 거절됐다면 holder에게 보낼 구조 변경도 없다.
#[test]
fn a_non_holder_cannot_insert_tabs_through_preset_or_pty_adoption() {
    let server = common::shared();
    let src = server.create_workspace("structure-sync-preset-src");
    let held = server.create_workspace("structure-sync-preset-held");
    let src_pane = server.first_pane_id_in_workspace(src.id);
    let held_pane = server.first_pane_id_in_workspace(held.id);
    let src_tab = server.call("tab.list", json!({ "pane_id": src_pane }))["tabs"][0]["id"]
        .as_u64()
        .expect("source tab id");
    server.call(
        "preset.capture",
        json!({ "kind": "tab", "source_id": src_tab, "name": "structure-sync-tab-preset" }),
    );
    let pty_id = server.call("pty.spawn", json!({}))["pty_id"]
        .as_u64()
        .expect("pty.spawn returns pty_id");

    let mut stream = open_workspace_attach(server.port(), held.id);
    let tabs = |pane: u64| {
        server.call("tab.list", json!({ "pane_id": pane }))["tabs"]
            .as_array()
            .expect("tabs array")
            .len()
    };
    let before = tabs(held_pane);

    for (method, params) in [
        (
            "preset.apply",
            json!({ "kind": "tab", "name": "structure-sync-tab-preset", "target_pane_id": held_pane }),
        ),
        (
            "pty.attach_surface",
            json!({ "id": pty_id, "pane_id": held_pane }),
        ),
    ] {
        let resp = server.call_raw(method, params);
        let message = resp["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{method} must be refused on a held workspace: {resp:?}"));
        assert!(
            message.contains("hard-occupied"),
            "{method}: tab.create 와 같은 점유 사유여야 한다: {message}"
        );
    }

    assert_eq!(tabs(held_pane), before, "거절된 요청은 탭을 만들면 안 된다");
    assert!(
        structural_delta_until(&mut stream, Instant::now() + Duration::from_secs(2)).is_none(),
        "거절됐으므로 holder 에게 보낼 구조 변경이 없어야 한다"
    );
}

/// terminal.kill은 child 터미널을 닫으므로 surface.close와 같은 이유로 거절돼야 한다.
/// 거절 전에 점유를 풀면 holder가 workspace에서 강제로 떨어져 나간다.
#[test]
fn a_non_holder_cannot_kill_a_child_terminal_in_a_held_workspace() {
    let server = common::shared();
    let parent_ws = server.create_workspace("structure-sync-kill-parent");
    let held = server.create_workspace("structure-sync-kill-held");
    let split = server.call(
        "split",
        json!({ "level": "surface", "direction": "vertical", "target_surface": held.surface_id }),
    );
    let child = split["new_surface_id"].as_u64().expect("new_surface_id");
    let adopt = server.call(
        "terminal.adopt",
        json!({ "surface": parent_ws.surface_id, "target": child }),
    );
    let idx = adopt["child_index"].as_u64().expect("child_index");

    let mut stream = open_workspace_attach(server.port(), held.id);

    let control = server.call_raw("surface.close", json!({ "surface_id": child }));
    assert!(
        control.get("error").is_some(),
        "대조군 surface.close 는 거절돼야 한다: {control:?}"
    );
    let kill = server.call_raw(
        "terminal.kill",
        json!({ "surface": parent_ws.surface_id, "child": idx }),
    );
    let message = kill["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("terminal.kill must be refused on a held workspace: {kill:?}"));
    assert!(message.contains("hard-occupied"), "{message}");

    let surfaces = server.call("surface.list", json!({}));
    assert!(
        surfaces
            .as_array()
            .expect("surface list")
            .iter()
            .any(|s| s["id"].as_u64() == Some(child)),
        "거절된 kill 은 surface 를 닫으면 안 된다"
    );
    let children = server.call(
        "terminal.children",
        json!({ "surface": parent_ws.surface_id }),
    );
    assert!(
        children["children"]
            .as_array()
            .expect("children array")
            .iter()
            .any(|c| c["surface_id"].as_u64() == Some(child)),
        "거절된 kill 은 child 관계를 지우면 안 된다: {children}"
    );
    // holder가 강제 해제됐다면 force_detached가 오고 이후 split은 holder 거절로 실패한다.
    let r = split_and_read_new_surface(&mut stream, held.surface_id);
    assert_ne!(
        r, held.surface_id,
        "holder 는 여전히 구조 변경을 forward 할 수 있어야 한다"
    );
}
