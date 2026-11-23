//! 실행한 서버에 loopback attach를 연결해 markdown 역할·파일 경로와 원문 조회 응답을 확인한다.
//! 서버의 실제 파일을 사용하지만 클라이언트 GUI의 문서 렌더링은 검사하지 않는다(docs/features/remote-attach/index.md).

// 이유: 시험의 정리용 결과 무시는 제품 코드의 오류 처리 목록과 구분한다.
#![allow(clippy::let_underscore_must_use)]

mod attach_common;
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use attach_common::{
    AttachStream, open_stream_without_attach, try_open_workspace_attach_stream,
    wait_for_control_event, write_control_frame,
};
use common::TastyInstance;
use serde_json::{Value, json};

const DOC_BODY: &str = "# remote doc\n\n원격에서만 존재하는 문서다.\n";

/// 같은 프로세스의 반복 호출도 구분하도록 임시 경로에 단조 카운터를 넣는다. 호출자가 정리한다.
fn write_doc(tag: &str) -> std::path::PathBuf {
    write_doc_with_body(tag, DOC_BODY)
}

fn write_doc_with_body(tag: &str, body: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "tasty_md_content_loopback_{tag}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // 이유: 이전 임시 디렉터리가 있으면 정리를 시도한다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("README.md");
    std::fs::write(&file, body).unwrap();
    file
}

fn open_markdown_surface(server: &TastyInstance, workspace_id: u64, file: &std::path::Path) -> u64 {
    let pane_id = server.first_pane_id_in_workspace(workspace_id);
    let r = server.call(
        "tab.create",
        json!({ "pane_id": pane_id, "type": "markdown", "file": file.to_string_lossy() }),
    );
    r["surface_id"]
        .as_u64()
        .expect("tab.create returned surface_id")
}

fn surface_descriptor(descriptor: &Value, surface_id: u64) -> Option<Value> {
    descriptor["surfaces"]
        .as_array()?
        .iter()
        .find(|s| s["remote_id"].as_u64() == Some(surface_id))
        .cloned()
}

/// 파일 snapshot은 tab.create 응답 뒤에 비동기로 도착할 수 있다. handshake에 경로가 실릴 때까지 연결을 닫고 다시 시도한다.
fn attach_when_descriptor_has_file(
    server: &TastyInstance,
    workspace_id: u64,
    surface_id: u64,
) -> (AttachStream, Value) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = None;
    // 읽기마다 새 연결 handshake 로 점유를 얻는다. 기한 뒤 읽기는 원래 루프가 만들지 않던 새 연결이 되고,
    // descriptor 는 handshake 로만 보이므로 연결 없이 관측할 길이 없어 원래 루프를 둔다.
    while Instant::now() < deadline {
        if let Some((stream, descriptor)) =
            try_open_workspace_attach_stream(server.port(), workspace_id)
        {
            let entry = surface_descriptor(&descriptor, surface_id);
            if entry
                .as_ref()
                .and_then(|e| e["file"].as_str())
                .is_some_and(|f| !f.is_empty())
            {
                return (stream, entry.expect("entry checked above"));
            }
            last = entry;
            drop(stream); // 점유를 놓고 다시 붙는다 — snapshot 이 아직 안 왔다.
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("markdown 디스크립터에 file 이 끝내 실리지 않았다: {last:?}");
}

#[test]
fn markdown_surface_is_sent_as_its_own_role_with_the_remote_path() {
    let server = common::shared();
    let ws = server.create_workspace("md-content-role");
    let file = write_doc("role");
    let surface_id = open_markdown_surface(server, ws.id, &file);

    let (_stream, entry) = attach_when_descriptor_has_file(server, ws.id, surface_id);

    assert_eq!(
        entry["role"], "markdown",
        "markdown 은 placeholder 가 아니라 전용 role 이어야 한다: {entry:?}"
    );
    assert_eq!(entry["file"], file.to_string_lossy().as_ref());
    assert!(
        entry["display_name"]
            .as_str()
            .is_some_and(|n| !n.is_empty()),
        "탭 제목용 display_name 이 함께 실려야 한다: {entry:?}"
    );

    // 이유: 결과 확인 뒤 임시 디렉터리 삭제 실패는 무시한다.
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn markdown_content_request_round_trips_over_attach_channel() {
    let server = common::shared();
    let ws = server.create_workspace("md-content-round-trip");
    let file = write_doc("roundtrip");
    let surface_id = open_markdown_surface(server, ws.id, &file);

    let (mut stream, _entry) = attach_when_descriptor_has_file(server, ws.id, surface_id);

    write_control_frame(
        &mut stream,
        &json!({
            "event": "markdown_content_request",
            "request_id": 1,
            "surface_id": surface_id,
        }),
    );
    let result = wait_for_control_event(&mut stream, "markdown_content_result");

    assert_eq!(result["request_id"], 1);
    assert_eq!(result["surface_id"], surface_id);
    assert_eq!(result["ok"], true, "expected ok reply: {result:?}");
    assert_eq!(
        result["source"], DOC_BODY,
        "서버가 실제 디스크의 원문을 그대로 실어야 한다: {result:?}"
    );
    assert_eq!(result["file"], file.to_string_lossy().as_ref());
    assert_eq!(result["truncated"], false);

    // 이유: 결과 확인 뒤 임시 디렉터리 삭제 실패는 무시한다.
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn markdown_content_request_reports_a_reason_for_a_missing_file() {
    let server = common::shared();
    let ws = server.create_workspace("md-content-missing");
    let file = write_doc("missing");
    let surface_id = open_markdown_surface(server, ws.id, &file);

    let (mut stream, _entry) = attach_when_descriptor_has_file(server, ws.id, surface_id);
    // 경로를 받은 뒤 파일을 지워 경로 발견 실패와 파일 읽기 실패를 구별한다.
    std::fs::remove_file(&file).unwrap();

    write_control_frame(
        &mut stream,
        &json!({
            "event": "markdown_content_request",
            "request_id": 2,
            "surface_id": surface_id,
        }),
    );
    let result = wait_for_control_event(&mut stream, "markdown_content_result");

    assert_eq!(result["request_id"], 2);
    assert_eq!(result["ok"], false, "expected error reply: {result:?}");
    assert!(result["reason"].as_str().is_some_and(|r| !r.is_empty()));
    assert!(
        result["source"].is_null(),
        "실패 회신에 원문이 실리면 안 된다"
    );

    // 이유: 결과 확인 뒤 임시 디렉터리 삭제 실패는 무시한다.
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

/// 점유 없는 클라이언트의 원문 조회가 거절되는지 확인한다(docs/features/remote-attach/index.md).
#[test]
fn markdown_content_request_rejected_without_workspace_occupancy() {
    let server = common::shared();
    let mut stream = open_stream_without_attach(server.port());

    write_control_frame(
        &mut stream,
        &json!({
            "event": "markdown_content_request",
            "request_id": 3,
            "surface_id": 1,
        }),
    );
    let result = wait_for_control_event(&mut stream, "markdown_content_result");

    assert_eq!(result["request_id"], 3);
    assert_eq!(
        result["ok"], false,
        "unattached client must be rejected: {result:?}"
    );
    assert!(result["source"].is_null(), "no source on rejection");
}

/// 따옴표로 채운 원문은 400KiB지만 JSON 이스케이프 뒤에는 약 800KiB다. 700KiB 예산을 원문이 아니라 직렬화 크기에 적용하는지 확인한다.
#[test]
fn markdown_content_over_budget_arrives_truncated_instead_of_killing_the_session() {
    const BUDGET: usize = 700 * 1024;
    const RAW_QUOTES: usize = 400 * 1024;

    let server = common::shared();
    let ws = server.create_workspace("md-content-truncated");
    let body = "\"".repeat(RAW_QUOTES);
    let file = write_doc_with_body("truncated", &body);
    let surface_id = open_markdown_surface(server, ws.id, &file);

    let (mut stream, _entry) = attach_when_descriptor_has_file(server, ws.id, surface_id);

    write_control_frame(
        &mut stream,
        &json!({
            "event": "markdown_content_request",
            "request_id": 4,
            "surface_id": surface_id,
        }),
    );
    let result = wait_for_control_event(&mut stream, "markdown_content_result");

    assert_eq!(result["request_id"], 4);
    assert_eq!(result["ok"], true, "예산 초과는 실패가 아니다: {result:?}");
    assert_eq!(
        result["truncated"], true,
        "예산을 넘는 문서는 잘렸다고 알려야 한다"
    );

    let source = result["source"].as_str().expect("source on ok reply");
    assert!(
        source.len() < RAW_QUOTES,
        "잘렸다면 원문보다 짧아야 한다: {} vs {RAW_QUOTES}",
        source.len()
    );
    assert!(
        source.chars().all(|c| c == '"'),
        "실린 부분은 원문의 접두사 그대로여야 한다"
    );
    let serialized = serde_json::to_vec(&Value::String(source.to_string()))
        .expect("string always serializes")
        .len();
    assert!(
        serialized <= BUDGET,
        "직렬화 길이가 예산을 넘었다: {serialized} > {BUDGET}"
    );
    assert_eq!(
        serialized, BUDGET,
        "따옴표만 있는 문서는 예산에 정확히 차야 한다(2 + n*2)"
    );

    // 같은 연결로 다시 요청해 큰 응답 뒤에도 세션을 사용할 수 있는지 확인한다.
    write_control_frame(
        &mut stream,
        &json!({
            "event": "markdown_content_request",
            "request_id": 5,
            "surface_id": surface_id,
        }),
    );
    let again = wait_for_control_event(&mut stream, "markdown_content_result");
    assert_eq!(again["request_id"], 5);
    assert_eq!(
        again["ok"], true,
        "예산 초과 회신 뒤에도 세션이 살아 있어야 한다"
    );

    // 이유: 결과 확인 뒤 임시 디렉터리 삭제 실패는 무시한다.
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

/// 같은 engine의 다른 workspace만 점유한 클라이언트도 문서 원문을 조회할 수 있어야 한다. 대상 workspace holder로 제한된 인가가 아니다(docs/features/remote-attach/index.md).
#[test]
fn markdown_content_request_is_authorized_engine_wide_not_per_workspace() {
    let server = common::shared();
    let doc_ws = server.create_workspace("md-content-engine-wide-doc");
    let held_ws = server.create_workspace("md-content-engine-wide-held");
    let file = write_doc("engine-wide");
    let surface_id = open_markdown_surface(server, doc_ws.id, &file);

    // 문서 경로가 담긴 snapshot을 받은 뒤 그 workspace의 점유를 놓고 다른 workspace만 점유한다.
    let (probe, _entry) = attach_when_descriptor_has_file(server, doc_ws.id, surface_id);
    drop(probe);

    let deadline = Instant::now() + Duration::from_secs(20);
    // 읽기마다 새 연결 handshake 로 점유를 얻는다. 기한 뒤 읽기를 관측만으로 할 길이 없어 원래 루프를 둔다.
    let mut stream = loop {
        if let Some((s, _)) = try_open_workspace_attach_stream(server.port(), held_ws.id) {
            break s;
        }
        assert!(
            Instant::now() < deadline,
            "다른 워크스페이스 점유를 끝내 얻지 못했다"
        );
        std::thread::sleep(Duration::from_millis(250));
    };

    write_control_frame(
        &mut stream,
        &json!({
            "event": "markdown_content_request",
            "request_id": 6,
            "surface_id": surface_id,
        }),
    );
    let result = wait_for_control_event(&mut stream, "markdown_content_result");

    assert_eq!(result["request_id"], 6);
    assert_eq!(
        result["ok"], true,
        "다른 워크스페이스만 점유한 client 도 인가된다(engine 전체): {result:?}"
    );
    assert_eq!(
        result["source"], DOC_BODY,
        "원문이 그대로 실려야 한다: {result:?}"
    );

    // 이유: 결과 확인 뒤 임시 디렉터리 삭제 실패는 무시한다.
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
