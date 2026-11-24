//! `surface.read_since_scan_mark` 응답: 기존 두 키에 연속성 키와 출처 정보를 더한다.
use super::*;
use crate::ipc::handler::idempotency::{self, MAX_STORED_RESPONSE_BYTES};
use std::time::Instant;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::{ERR_IDEMPOTENT_RESULT_DISCARDED, JsonRpcRequest};

const CONTINUITY: [&str; 7] = [
    "stream",
    "cursor",
    "next_cursor",
    "raw_bytes",
    "retention_start",
    "retention_end",
    "skipped",
];

fn first_surface(engine: &EngineMut<'_>) -> u32 {
    engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0]
}

fn put_terminal(engine: &mut EngineMut<'_>, sid: u32) {
    engine
        .runtime
        .terminals
        .insert(sid, tasty_terminal::Terminal::new_detached(80, 24), None);
}

fn feed(engine: &mut EngineMut<'_>, sid: u32, bytes: &[u8]) {
    engine
        .runtime
        .terminals
        .get_mut(sid)
        .expect("terminal")
        .feed_bytes(bytes);
}

fn scan(engine: &mut EngineMut<'_>, sid: u32, strip_ansi: bool) -> serde_json::Value {
    let resp = handle_read_since_scan_mark(
        engine,
        json!(1),
        &json!({ "surface_id": sid, "strip_ansi": strip_ansi }),
    );
    resp.result.expect("성공 응답")
}

/// Given 로컬 터미널, When 스캔 읽기, Then 기존 두 키, 연속성 키, text_origins가 있다.
#[test]
fn a_local_terminal_answers_with_continuity_and_origins() {
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = first_surface(&engine);
    put_terminal(&mut engine, sid);
    feed(&mut engine, sid, b"\x1b[31mAPI Error\x1b[0m");
    let r = scan(&mut engine, sid, true);
    assert_eq!(r["text"], "API Error");
    assert_eq!(r["surface_id"], sid);
    for key in CONTINUITY {
        assert!(r.get(key).is_some(), "{key} 없음: {r}");
    }
    assert_eq!(r["cursor"], 0);
    assert_eq!(r["skipped"], 0);
    assert_eq!(
        r["text_origins"],
        json!({ "ranges": [{ "start": 0, "end": 9, "origin": "normal" }], "gaps": [] })
    );
    let again = scan(&mut engine, sid, true);
    assert_eq!(again["cursor"], r["next_cursor"]);
    assert_eq!(again["text"], "");
    assert_eq!(again["text_origins"], json!({ "ranges": [], "gaps": [] }));
}

/// Given mirror workspace의 터미널, When 스캔 읽기, Then 연속성 키는 있고 text_origins는 없다.
#[test]
fn a_mirror_terminal_answers_without_origins() {
    let (_state, mut session) = crate::state::tests::test_mirror_state();
    let mut engine = session.borrow_mut();
    let sid = first_surface(&engine);
    assert!(engine.is_mirror_surface(sid));
    put_terminal(&mut engine, sid);
    feed(&mut engine, sid, b"remote output");
    let r = scan(&mut engine, sid, false);
    assert_eq!(r["text"], "remote output");
    for key in CONTINUITY {
        assert!(r.get(key).is_some(), "{key} 없음: {r}");
    }
    assert!(r.get("text_origins").is_none(), "{r}");
}

/// Given 로컬 터미널이 없는 surface, When 스캔 읽기, Then 지금처럼 두 키만 답한다.
#[test]
fn a_surface_without_a_terminal_keeps_the_two_key_answer() {
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = first_surface(&engine);
    engine.runtime.terminals.remove(sid);
    let r = scan(&mut engine, sid, false);
    assert_eq!(r, json!({ "text": "", "surface_id": sid }));
}

/// Given 같은 surface의 Terminal 교체, When 다시 읽으면, Then stream이 바뀌고 cursor는 0부터다.
#[test]
fn replacing_the_terminal_renews_the_stream() {
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = first_surface(&engine);
    put_terminal(&mut engine, sid);
    feed(&mut engine, sid, b"old");
    let before = scan(&mut engine, sid, false);
    put_terminal(&mut engine, sid);
    feed(&mut engine, sid, b"new");
    let after = scan(&mut engine, sid, false);
    assert_ne!(after["stream"], before["stream"]);
    assert_eq!(after["cursor"], 0);
    assert_eq!(after["text"], "new");
}

fn keyed(sid: u32, key: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        caller_agent_id: None,
        jsonrpc: "2.0".into(),
        method: "surface.read_since_scan_mark".into(),
        params: json!({ "surface_id": sid }),
        id: Some(json!(1)),
        session_token: None,
        response_timeout_ms: None,
        idempotency_key: Some(key.into()),
    }
}

/// 멱등 키를 준 스캔 읽기를 라우터처럼 처리한다. 재시도면 보관한 답(또는 거절)이 오고 핸들러는 돌지 않는다.
fn keyed_scan(engine: &mut EngineMut<'_>, req: &JsonRpcRequest, id: i64) -> JsonRpcResponse {
    let now = Instant::now();
    match idempotency::begin(now, &CallerContext::local(), req, &json!(id)) {
        Ok(pending) => {
            let resp = handle_read_since_scan_mark(engine, json!(id), &req.params);
            idempotency::finish(now, pending, &resp);
            resp
        }
        Err(answer) => answer,
    }
}

/// Given 64 KiB 이하 응답, When 같은 키로 다시 보내면, Then 출처를 포함한 같은 result가 재생된다.
#[test]
fn a_keyed_retry_replays_the_scan_answer_with_its_origins() {
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = first_surface(&engine);
    put_terminal(&mut engine, sid);
    feed(&mut engine, sid, b"small output");
    // 보존소는 프로세스 전역이라 다른 시험과 안 겹치는 키를 쓴다.
    let req = keyed(sid, "scan-origins-replay-probe");
    let first = keyed_scan(&mut engine, &req, 1);
    let retry = keyed_scan(&mut engine, &req, 2);
    assert!(retry.idempotent_replay);
    assert_eq!(retry.id, json!(2));
    assert_eq!(retry.result, first.result);
    assert!(first.result.expect("성공")["text_origins"].is_object());
}

/// Given 64 KiB를 넘는 응답, When 같은 키로 다시 보내면, Then 거절이고 커서는 더 전진하지 않는다.
#[test]
fn a_keyed_retry_of_an_oversized_scan_answer_is_refused_without_rereading() {
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = first_surface(&engine);
    put_terminal(&mut engine, sid);
    feed(&mut engine, sid, &vec![b'x'; MAX_STORED_RESPONSE_BYTES + 1]);
    let req = keyed(sid, "scan-origins-discard-probe");
    let first = keyed_scan(&mut engine, &req, 1).result.expect("성공");
    feed(&mut engine, sid, b"tail");
    let retry = keyed_scan(&mut engine, &req, 2);
    assert_eq!(
        retry.error.expect("거절").code,
        ERR_IDEMPOTENT_RESULT_DISCARDED
    );
    let next = scan(&mut engine, sid, false);
    assert_eq!(next["text"], "tail");
    assert_eq!(next["cursor"], first["next_cursor"]);
}
