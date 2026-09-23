//! 서버 attention 의 attach mirror push(`StreamControl::Attention`, server→client)를
//! loopback `TcpStream` 으로 실제 실행 중인 서버 인스턴스에 대해 검증한다.
//!
//! frame/handshake 헬퍼는 `tests/attach_common/mod.rs` 를 공유한다 — attach client 는
//! 실제 `tasty` GUI 앱이 아니라 raw `TcpStream` 으로 직접 핸드셰이크한다. 서버
//! 인스턴스는 `common::shared()` 하나를 이 test binary 전체가 함께 쓰고, 점유가 필요한
//! 테스트는 `create_workspace()` 로 자기 workspace 를 만든다.
//!
//! **GUI 두 인스턴스를 실제로 attach 해 미러 사이드바의 배지를 눈으로 확인하는 e2e**
//! 는 이 headless 작업 환경(GPU 디스플레이 없음)에서 실행할 수 없다 — 이 test 는 그
//! 대체로, (1) attach 점유 획득 → (2) `surface.completion` IPC 로 서버에서 attention
//! raise → (3) 1Hz forward tick 이 `attention` Control 프레임을 원격 surface id 와
//! 함께 push 하는 전체 경로를 프로토콜 레벨에서 실행한다.
//! `docs/features/surface-highlight/index.md` "검증 한계" 절 참고.
//!
//! 같은 1Hz tick 을 타는 **surface cwd push**(`StreamControl::Cwd`)도 이 binary 에서 검증한다 —
//! 인스턴스를 띄우는 test binary 수를 늘리지 않으려는 것이다(`docs/dev-guide/e2e-tests.md`).
//! cwd 는 OSC 7 에 기대지 않는 값(워크스페이스 명시 cwd, holder 입력으로 `cd`)으로 본다 — mirror
//! terminal 이 OSC 7 없이는 cwd 를 모른다는 것이 그 채널이 존재하는 이유다
//! (`docs/adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md`).

mod attach_common;
mod common;

use std::io::Read;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use attach_common::{TAG_CONTROL, open_workspace_attach, read_frame, write_workspace_input};
use serde_json::{Value, json};

/// dedup(스팸 없음) 확인용 정적 대기 — 1Hz forward tick 을 여러 번 지나칠 만큼만
/// 기다린다. `attach_common::read_frame` 은 타임아웃에 panic 하므로 여기서는
/// 타임아웃을 정상 종료로 다루는 자체 reader 를 쓴다.
const QUIET_WINDOW: Duration = Duration::from_millis(3_000);

/// 프레임 하나를 읽되, read 타임아웃이면 `None`. `attach_common::read_frame` 의
/// 비-panic 판(negative assertion 전용).
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

/// `attention` Control 프레임이 올 때까지 읽되, `kind` 가 non-null 인 첫 프레임을
/// 돌려준다. 세션 시작 직후 서버는 attention 이 없는 상태를 알리는 `kind: null`
/// baseline 을 1회 push 할 수 있으므로(점유 시작 시점과 raise 시점의 경합) 그건
/// 건너뛴다. 그 외 무관한 control 프레임(터미널 스냅샷·`activity` 등)도 건너뛴다.
fn wait_for_raised_attention(stream: &mut TcpStream) -> Value {
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

/// 다음 `attention` Control 프레임 하나를 kind 와 무관하게 돌려준다. 무관한 control
/// 프레임(터미널 스냅샷·`activity` 등)은 건너뛴다.
fn wait_for_attention_frame(stream: &mut TcpStream) -> Value {
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

/// 미러가 그 surface 를 확인했을 때 보내는 해제 edge 프레임.
/// `StreamControl::ClientAttentionClear`(client→server)의 wire 형태.
fn send_attention_clear(stream: &mut TcpStream, remote_surface_id: u64) {
    attach_common::write_control_frame(
        stream,
        &json!({ "event": "client_attention_clear", "surface_id": remote_surface_id }),
    );
}

/// 서버에서 `needs_input` attention 이 raise 되면 점유 client 에게 `attention`
/// Control 프레임이 원격 surface id 와 함께 push 된다. `NeedsInput` 은 서버(PTY 를
/// 가진 인스턴스)에서만 나오는 kind 라, 이 경로가 없으면 미러 사용자에게 "응답 필요"
/// 가 도달할 방법이 원천적으로 없다.
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
        "attention 은 **원격(서버) surface id** 로 앵커된다 — client 가 자기 매핑으로 로컬 mirror id 를 찾는다: {frame:?}"
    );
    assert_eq!(
        frame["kind"], "needs_input",
        "서버가 raise 한 kind 가 그대로 실려야 한다: {frame:?}"
    );
}

/// `completion` kind 도 같은 채널로 전달된다 — attention 은 kind 별 별도 채널이
/// 아니라 하나의 프레임에 kind 를 실어 보낸다.
#[test]
fn completion_kind_is_pushed_over_the_same_channel() {
    let server = common::shared();
    let ws = server.create_workspace("attention-push-completion");

    let mut stream = open_workspace_attach(server.port(), ws.id);

    // kind 생략 = Completion(하위 호환 — CLI/OSC 133 producer 경로).
    server.call("surface.completion", json!({ "surface_id": ws.surface_id }));

    let frame = wait_for_raised_attention(&mut stream);
    assert_eq!(frame["surface_id"], ws.surface_id);
    assert_eq!(frame["kind"], "completion", "{frame:?}");
}

/// 값이 바뀌지 않는 tick 에는 프레임이 나가지 않는다(스팸 없음) — 서버측
/// `last_forwarded_attention` dedup 이 실제 스트림에서도 동작하는지 확인한다.
/// 같은 kind 로 IPC 를 다시 호출해도 값이 그대로라 새 프레임이 없어야 한다.
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

    // 같은 kind 로 재발동 — 서버 store 값이 바뀌지 않으므로 forward 대상이 아니다.
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

/// 미러의 해제 edge 가 **서버 레코드를 실제로 지운다.** 서버 attention 을 직접
/// 조회하는 IPC 가 없으므로 diff push 의 성질로 관측한다 — ① 해제 프레임을 보내면
/// 서버가 값이 바뀌었다고 판단해 `kind: null` 을 되돌려 push 하고(레코드가 안 지워졌
/// 다면 값이 그대로라 이 프레임 자체가 없다), ② 같은 kind 로 다시 raise 했을 때
/// 프레임이 **다시** 도착한다(그 사이 서버 값이 실제로 비었다는 뜻 — 값이 남아
/// 있었다면 dedup 이 재전송을 막는다).
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

    // 미러 사용자가 그 surface 를 확인 → 해제 edge 1 회 전송.
    send_attention_clear(&mut stream, ws.surface_id);

    let cleared = wait_for_attention_frame(&mut stream);
    assert!(
        cleared["kind"].is_null(),
        "해제가 적용됐다면 서버 diff 가 `kind: null` 을 되돌려 push 한다: {cleared:?}"
    );
    assert_eq!(cleared["surface_id"], ws.surface_id, "{cleared:?}");

    // 같은 kind 로 재발동 — 서버 레코드가 실제로 비었어야 다시 push 된다.
    server.call(
        "surface.completion",
        json!({ "surface_id": ws.surface_id, "kind": "needs_input" }),
    );
    let reraised = wait_for_raised_attention(&mut stream);
    assert_eq!(
        reraised["kind"], "needs_input",
        "해제 후 재발동이 다시 push 되지 않으면 서버 레코드가 남아 있었다는 뜻이다: {reraised:?}"
    );
}

/// 해제는 **edge 신호**다 — 레코드가 없는 상태로 해제 프레임이 더 와도 서버 값은
/// 그대로라 되돌아오는 프레임이 없다. client 측에서 포커스를 유지해도 프레임이 한
/// 번만 나가는 성질(`CoreState::clear_attention` 의 제거 edge)의 서버측 대응 확인.
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

    // 이미 비어 있는 레코드에 해제를 두 번 더 — 서버 값이 안 바뀌므로 프레임 없음.
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

/// 하드 점유 중에는 **서버 로컬 포커스가 서버의 attention 을 지우지 못한다**(ADR-0109).
///
/// 이 인스턴스는 실제 GUI 라 `src/gfx/gpu.rs` 의 매 프레임 실-포커스 해제가 살아 있다 —
/// 활성 워크스페이스의 포커스 surface 를 매 프레임 해제 대상으로 삼는다. 점유한
/// 워크스페이스를 활성화시켜(`debug.switch_workspace`) 그 경로를 **실제로 밟게 한 뒤**,
/// 해제가 일어났다면 반드시 따라오는 `kind: null` diff push 가 오지 않는지 본다
/// (서버 레코드를 직접 조회하는 IPC 가 없으므로 push 의 부재로 관측한다 — 게이트가
/// 없으면 첫 프레임에 지워지고 다음 tick 에 `kind: null` 이 도착한다).
///
/// 워크스페이스 전환은 사용자 행동 재현이라 release 표면에 없고 debug IPC 로만
/// 가능하다(`docs/dev-guide/debug-ipc.md`).
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

    // 점유된 그 surface 가 서버 GUI 의 실 렌더 포커스를 얻게 한다. 활성 워크스페이스는
    // 공유 인스턴스의 전역 상태라, 원래 값을 기억해 뒀다가 끝에 되돌린다 — 이 바이너리의
    // 다른 테스트가 "포커스 없는 surface 의 attention 이 유지된다" 를 전제하게 되더라도
    // 실행 순서에 의존하지 않도록.
    let previous_active = server.call("ui.state", json!({}))["active_workspace"]
        .as_u64()
        .expect("ui.state 는 active_workspace 인덱스를 돌려준다");
    let switched = server.call("debug.switch_workspace", json!({ "index": ws.index }));
    assert_eq!(
        switched["switched"], true,
        "게이트가 밟히는 전제(활성 워크스페이스 전환)가 성립해야 한다: {switched:?}"
    );

    // 수백 프레임 + 여러 1Hz tick 이 지나도 해제 push 가 없어야 한다.
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
    // 활성 워크스페이스 원복 — assert 보다 먼저 해서 실패해도 전역 상태를 남기지 않는다.
    server.call(
        "debug.switch_workspace",
        json!({ "index": previous_active }),
    );

    assert!(
        frames.is_empty(),
        "점유 중 서버 로컬 포커스는 홀더의 신호를 지우지 못한다 — 해제 push 가 오면 게이트가 없는 것이다: {frames:?}"
    );
}

/// 하드 점유(원격 attach) 중인 surface 는 로컬 IPC 해제를 **거절**한다. 점유 중에는
/// 그 surface 의 상태를 holder 세션이 소유하므로, 로컬에서 지우면 서버 값만 내려가
/// holder 미러와 갈라진다(미러는 서버 값이 실제로 바뀌기 전까지 재-push 를 못 받는다).
/// 거절은 조용한 no-op 이 아니라 명시적 에러여야 한다 — 에이전트가 "지웠다" 고
/// 오인하면 안 된다.
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

    // 거절이므로 상태는 그대로다 — 조회 표면으로 직접 확인한다(점유 중에도 read 는 허용).
    let after = server.call(
        "surface.attention.get",
        json!({ "surface_id": ws.surface_id }),
    );
    assert_eq!(after["kind"], "needs_input", "{after:?}");
}

// ---- surface cwd push ----

/// 이 surface 에 대한 `cwd` control 이벤트 중 `expected` 값을 가진 것이 올 때까지 읽는다.
/// 1Hz tick 이라 상한은 넉넉히 둔다. 도중에 본 값은 실패문에 싣는다.
fn wait_for_cwd(
    stream: &mut TcpStream,
    surface_id: u64,
    expected: &str,
    limit: Duration,
) -> Vec<Value> {
    let start = Instant::now();
    let mut seen = Vec::new();
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
    // `/proc/<pid>/cwd` 는 심볼릭 링크를 푼 경로를 돌려준다(macOS 의 `/var` → `/private/var` 등).
    dir.canonicalize().unwrap()
}

/// 첫 단언은 `workspace.create` 의 명시 `cwd` 가 새 셸의 시작 cwd 로 **실리는가**도 함께 잰다 —
/// 핸들러가 계산한 값 대신 생성 intent 에 `cwd: None` 을 실으면 초기 push 가 격리 HOME 으로 와서
/// 여기서 실패한다(헤드리스 조합에서 변이로 확인). 그 물음의 1 차 채널은 핸들러 옆의
/// `create_cwd_tests::the_resolved_cwd_reaches_the_new_terminals_shell` 이고, 이 시험은 실행 중
/// 서버를 상대로 하는 두 번째 채널이다. 시작 디렉토리를 `workspace.create` 의 `cwd` 가 아니라
/// 나중의 `cd` 로 주도록 바꾸면 그 채널이 사라진다.
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

    // 점유 직후의 초기 push — 셸의 시작 cwd.
    wait_for_cwd(
        &mut stream,
        sid,
        &start_dir.to_string_lossy(),
        Duration::from_secs(10),
    );

    // 셸이 이동하면 다음 tick 들 안에 새 값이 나간다.
    // 점유 중에는 서버 로컬 입력이 막히므로 holder 입력 프레임으로 보낸다.
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
