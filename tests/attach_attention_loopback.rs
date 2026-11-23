//! 실행한 서버에 loopback TcpStream으로 attach해 attention과 cwd 프레임을 확인한다.
//! 서버 인스턴스는 이 시험 바이너리에서 공유하고 각 시험은 별도 workspace를 만든다.
//! 실제 클라이언트 GUI의 배지 렌더링을 검증하는 시험은 아니다.
//! cwd는 명시한 시작 경로와 holder의 cd 입력으로 확인해 OSC 7에 의존하지 않는다.

mod attach_common;
mod common;

use std::io::Read;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use attach_common::{TAG_CONTROL, open_workspace_attach, read_frame, write_workspace_input};
use serde_json::{Value, json};

/// 중복 프레임이 없는지 볼 때는 여러 forward tick 동안 기다리며 읽기 타임아웃을 정상 종료로 처리한다.
const QUIET_WINDOW: Duration = Duration::from_millis(3_000);

/// 읽기 타임아웃이면 None을 반환한다.
fn try_read_frame(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 5];
    if stream.read_exact(&mut hdr).is_err() {
        return None;
    }
    let tag = hdr[0];
    let len = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
    let mut payload = vec![0u8; len];
    if len > 0 && stream.read_exact(&mut payload).is_err() {
        return None;
    }
    Some((tag, payload))
}

/// 점유 직후 kind:null baseline이 올 수 있어 실제 attention이 담긴 첫 프레임을 기다린다.
fn wait_for_raised_attention(stream: &mut TcpStream) -> Value {
    // 전체 기한 없이 프레임마다 소켓 읽기 timeout 이 새로 시작된다. 기한 뒤 재읽기 꼴로 바꾸면 프레임 소비와
    // 소켓 대기 계약이 달라지므로 원래 루프를 둔다.
    loop {
        let (tag, payload) = attach_common::read_frame(stream);
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        if v.get("event").and_then(|e| e.as_str()) != Some("attention") {
            continue;
        }
        if v.get("kind").map(Value::is_null).unwrap_or(true) {
            continue; // baseline(해제 상태) push — 아직 raise 전.
        }
        return v;
    }
}

/// kind와 무관하게 다음 attention 프레임을 반환한다.
fn wait_for_attention_frame(stream: &mut TcpStream) -> Value {
    // 전체 기한 없이 프레임마다 소켓 읽기 timeout 이 새로 시작된다. 위 대기와 같은 이유로 원래 루프를 둔다.
    loop {
        let (tag, payload) = attach_common::read_frame(stream);
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        if v.get("event").and_then(|e| e.as_str()) == Some("attention") {
            return v;
        }
    }
}

/// 미러에서 확인한 attention을 해제하는 client→server 프레임.
fn send_attention_clear(stream: &mut TcpStream, remote_surface_id: u64) {
    attach_common::write_control_frame(
        stream,
        &json!({ "event": "client_attention_clear", "surface_id": remote_surface_id }),
    );
}

#[test]
fn attention_is_pushed_to_the_attach_holder() {
    let server = common::shared();
    let ws = server.create_workspace("attention-push");

    let mut stream = open_workspace_attach(server.port(), ws.id);

    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );

    let frame = wait_for_raised_attention(&mut stream);
    assert_eq!(
        frame["surface_id"], ws.surface_id,
        "attention의 surface_id가 서버의 원격 ID와 다르다. 클라이언트는 이 ID로 미러를 찾는다: {frame:?}"
    );
    assert_eq!(
        frame["kind"], "needs_input",
        "서버에서 설정한 attention kind와 다르다: {frame:?}"
    );
}

#[test]
fn completion_kind_is_pushed_over_the_same_channel() {
    let server = common::shared();
    let ws = server.create_workspace("attention-push-completion");

    let mut stream = open_workspace_attach(server.port(), ws.id);

    // kind 생략 시 Completion으로 처리하는 하위 호환을 확인한다.
    server.call("surface.completion", json!({ "surface_id": ws.surface_id }));

    let frame = wait_for_raised_attention(&mut stream);
    assert_eq!(frame["surface_id"], ws.surface_id);
    assert_eq!(frame["kind"], "completion", "{frame:?}");
}

#[test]
fn unchanged_attention_does_not_respam_the_stream() {
    let server = common::shared();
    let ws = server.create_workspace("attention-push-dedup");

    let mut stream = open_workspace_attach(server.port(), ws.id);
    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    let first = wait_for_raised_attention(&mut stream);
    assert_eq!(first["kind"], "needs_input");

    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );

    stream
        .set_read_timeout(Some(QUIET_WINDOW))
        .expect("set quiet-window read timeout");
    let mut extra: Vec<Value> = Vec::new();
    while let Some((tag, payload)) = try_read_frame(&mut stream) {
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        if v.get("event").and_then(|e| e.as_str()) == Some("attention") {
            extra.push(v);
        }
    }
    assert!(
        extra.is_empty(),
        "값이 그대로면 1Hz tick 이 여러 번 지나도 attention 프레임이 나가면 안 된다: {extra:?}"
    );
}

/// 해제 프레임 뒤 kind:null과 재발생 프레임을 관측해 해제·재전달을 확인한다.
#[test]
fn mirror_clear_frame_drops_the_server_attention_record() {
    let server = common::shared();
    let ws = server.create_workspace("attention-clear-forward");

    let mut stream = open_workspace_attach(server.port(), ws.id);
    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    let raised = wait_for_raised_attention(&mut stream);
    assert_eq!(raised["kind"], "needs_input", "{raised:?}");

    send_attention_clear(&mut stream, ws.surface_id);

    let cleared = wait_for_attention_frame(&mut stream);
    assert!(
        cleared["kind"].is_null(),
        "해제가 적용됐다면 서버 diff 가 `kind: null` 을 되돌려 push 한다: {cleared:?}"
    );
    assert_eq!(cleared["surface_id"], ws.surface_id, "{cleared:?}");

    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    let reraised = wait_for_raised_attention(&mut stream);
    assert_eq!(
        reraised["kind"], "needs_input",
        "해제 뒤 다시 설정한 attention 프레임을 받지 못했다: {reraised:?}"
    );
}

#[test]
fn repeated_clear_frames_do_not_respam_the_stream() {
    let server = common::shared();
    let ws = server.create_workspace("attention-clear-repeat");

    let mut stream = open_workspace_attach(server.port(), ws.id);
    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    wait_for_raised_attention(&mut stream);

    send_attention_clear(&mut stream, ws.surface_id);
    let cleared = wait_for_attention_frame(&mut stream);
    assert!(cleared["kind"].is_null(), "{cleared:?}");

    send_attention_clear(&mut stream, ws.surface_id);
    send_attention_clear(&mut stream, ws.surface_id);

    stream
        .set_read_timeout(Some(QUIET_WINDOW))
        .expect("set quiet-window read timeout");
    let mut extra: Vec<Value> = Vec::new();
    while let Some((tag, payload)) = try_read_frame(&mut stream) {
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        if v.get("event").and_then(|e| e.as_str()) == Some("attention") {
            extra.push(v);
        }
    }
    assert!(
        extra.is_empty(),
        "레코드가 없는 상태의 해제는 no-op 이라 프레임이 나가면 안 된다: {extra:?}"
    );
}

/// 점유된 workspace를 활성화한 뒤에도 로컬 포커스에 의한 해제 프레임이 오지 않는지 확인한다(docs/features/surface-highlight/index.md).
#[test]
fn hard_occupied_attention_survives_the_servers_local_focus() {
    let server = common::shared();
    let ws = server.create_workspace("attention-holder-only-gate");

    let mut stream = open_workspace_attach(server.port(), ws.id);
    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    let raised = wait_for_raised_attention(&mut stream);
    assert_eq!(raised["kind"], "needs_input", "{raised:?}");

    // 공유 서버의 활성 workspace는 다른 시험에 영향을 주지 않도록 저장했다가 복원한다.
    let previous_active = server.call("ui.state", json!({}))["active_workspace"]
        .as_u64()
        .expect("ui.state 는 active_workspace 인덱스를 돌려준다");
    let switched = server.call("debug.switch_workspace", json!({ "index": ws.index }));
    assert_eq!(
        switched["switched"], true,
        "로컬 포커스 해제를 확인할 workspace 전환이 실패했다: {switched:?}"
    );

    stream
        .set_read_timeout(Some(QUIET_WINDOW))
        .expect("set quiet-window read timeout");
    let mut frames: Vec<Value> = Vec::new();
    while let Some((tag, payload)) = try_read_frame(&mut stream) {
        if tag != TAG_CONTROL {
            continue;
        }
        let v: Value = serde_json::from_slice(&payload).unwrap();
        if v.get("event").and_then(|e| e.as_str()) == Some("attention") {
            frames.push(v);
        }
    }
    // 최종 단언이 실패해도 활성 workspace를 남기지 않도록 먼저 복원한다.
    server.call(
        "debug.switch_workspace",
        json!({ "index": previous_active }),
    );

    assert!(
        frames.is_empty(),
        "점유 중 로컬 포커스 변경 뒤 attention 프레임이 왔다: {frames:?}"
    );
}

/// 점유 중 로컬 해제가 미러와 상태를 다르게 만들지 않도록 명시적 오류로 거절하는지 확인한다.
#[test]
fn attention_clear_is_rejected_while_hard_occupied() {
    let server = common::shared();
    let ws = server.create_workspace("attention-clear-occupied");

    let mut stream = open_workspace_attach(server.port(), ws.id);
    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    let raised = wait_for_raised_attention(&mut stream);
    assert_eq!(raised["kind"], "needs_input");

    let denied = server.call_raw(
        "surface.attention.clear",
        json!({ "surface_id": ws.surface_id }),
    );
    let message = denied["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("하드 점유 중 해제는 에러여야 한다: {denied:?}"));
    assert!(
        message.contains("hard-occupied"),
        "거절 사유가 점유임을 밝혀야 한다: {message}"
    );

    let after = server.call(
        "surface.attention.get",
        json!({ "surface_id": ws.surface_id }),
    );
    assert_eq!(after["kind"], "needs_input", "{after:?}");
}

/// 해당 surface의 예상 cwd를 기다린다. 실패하면 관측한 값들도 출력한다.
fn wait_for_cwd(
    stream: &mut TcpStream,
    surface_id: u64,
    expected: &str,
    limit: Duration,
) -> Vec<Value> {
    let start = Instant::now();
    let mut seen = Vec::new();
    // 기한 검사가 프레임 소비 앞에 있다. 기한 뒤 재읽기는 소켓 timeout 까지 막히는 읽기로 프레임을 하나 더
    // 소비하게 되어 읽기 계약이 달라지므로 원래 루프를 둔다.
    while start.elapsed() < limit {
        let (tag, payload) = read_frame(stream);
        if tag != TAG_CONTROL {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&payload) else {
            continue;
        };
        if v["event"] != "cwd" || v["surface_id"].as_u64() != Some(surface_id) {
            continue;
        }
        let hit = v["cwd"].as_str() == Some(expected);
        seen.push(v);
        if hit {
            return seen;
        }
    }
    panic!(
        "surface {surface_id} 의 cwd 가 {limit:?} 안에 {expected:?} 로 push 되지 않았다. 본 값: {seen:?}"
    );
}

fn temp_dir(tag: &str, server_pid: u32) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "tasty_cwd_push_{tag}_{}_{server_pid}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("이전 실행의 잔여 디렉토리 정리");
    }
    std::fs::create_dir_all(&dir).unwrap();
    // 서버가 보고한 cwd의 심볼릭 링크 해석과 맞추기 위해 경로를 정규화한다.
    dir.canonicalize().unwrap()
}

/// 첫 cwd는 workspace.create로 지정해야 생성 시점의 경로 전달도 검증된다. 나중의 cd로 대체하면 이 검증이 빠진다.
#[test]
fn server_pushes_the_occupied_terminal_cwd_and_follows_cd() {
    let server = common::shared();
    let start_dir = temp_dir("start", server.pid());
    let moved_dir = temp_dir("moved", server.pid());

    let created = server.call(
        "workspace.create",
        json!({ "name": "cwd-push", "cwd": start_dir.to_string_lossy() }),
    );
    let ws_id = created["id"].as_u64().expect("workspace id");
    let sid = created["surface_id"].as_u64().expect("surface id");
    server.wait_for_shell(sid);

    let mut stream = open_workspace_attach(server.port(), ws_id);

    wait_for_cwd(
        &mut stream,
        sid,
        &start_dir.to_string_lossy(),
        Duration::from_secs(10),
    );

    // 점유 중에는 로컬 입력이 막히므로 holder의 Data 프레임으로 cd를 보낸다.
    write_workspace_input(
        &mut stream,
        sid as u32,
        format!("cd '{}'\r", moved_dir.to_string_lossy()).as_bytes(),
    );
    wait_for_cwd(
        &mut stream,
        sid,
        &moved_dir.to_string_lossy(),
        Duration::from_secs(10),
    );

    std::fs::remove_dir_all(&start_dir).expect("임시 디렉토리 정리");
    std::fs::remove_dir_all(&moved_dir).expect("임시 디렉토리 정리");
}
