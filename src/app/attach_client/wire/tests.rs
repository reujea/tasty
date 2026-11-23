use super::*;

use crate::ipc::stream::StreamControl;
use tasty_remote::client_session::MirrorEvent;

#[test]
fn a_loss_notice_becomes_a_desync_event() {
    let payload = serde_json::to_vec(&StreamControl::Loss { frames: 7 }).unwrap();
    assert!(
        matches!(
            mirror_event_from_control(&payload),
            Some(MirrorEvent::Desynced { frames: 7 })
        ),
        "Loss 가 재동기화 이벤트로 옮겨지지 않았다"
    );
}

#[test]
fn parse_markdown_content_result_reads_both_shapes_and_ignores_other_events() {
    let ok = serde_json::json!({
        "event": "markdown_content_result", "request_id": 4, "surface_id": 30,
        "ok": true, "file": "/r/a.md", "source": "# hi", "truncated": true,
    });
    match parse_markdown_content_result(&serde_json::to_vec(&ok).unwrap()) {
        Some(MirrorEvent::MarkdownContentResult {
            request_id: 4,
            surface_id: 30,
            ok: true,
            file,
            source,
            truncated: true,
            reason: None,
        }) => {
            assert_eq!(file.as_deref(), Some("/r/a.md"));
            assert_eq!(source.as_deref(), Some("# hi"));
        }
        _ => panic!("success shape not parsed"),
    }
    let failed = serde_json::json!({
        "event": "markdown_content_result", "request_id": 5, "surface_id": 30,
        "ok": false, "reason": "permission denied",
    });
    match parse_markdown_content_result(&serde_json::to_vec(&failed).unwrap()) {
        Some(MirrorEvent::MarkdownContentResult {
            ok: false, reason, ..
        }) => assert_eq!(reason.as_deref(), Some("permission denied")),
        _ => panic!("failure shape not parsed"),
    }
    let other = serde_json::json!({ "event": "git_query_result", "request_id": 1 });
    assert!(parse_markdown_content_result(&serde_json::to_vec(&other).unwrap()).is_none());
}

#[test]
fn parse_markdown_changed_reads_the_remote_id_and_ignores_other_events() {
    let changed = serde_json::json!({ "event": "markdown_changed", "surface_id": 30 });
    assert!(matches!(
        parse_markdown_changed(&serde_json::to_vec(&changed).unwrap()),
        Some(MirrorEvent::MarkdownChanged { surface_id: 30 })
    ));
    let result = serde_json::json!({
        "event": "markdown_content_result", "request_id": 4, "surface_id": 30, "ok": true,
    });
    assert!(parse_markdown_changed(&serde_json::to_vec(&result).unwrap()).is_none());
    assert!(parse_markdown_content_result(&serde_json::to_vec(&changed).unwrap()).is_none());
}

#[test]
fn a_resize_rejection_becomes_its_own_event() {
    let payload = serde_json::to_vec(&StreamControl::ResizeRejected {
        surface_id: 4,
        cols: 90,
        rows: 30,
    })
    .unwrap();
    assert!(matches!(
        mirror_event_from_control(&payload),
        Some(MirrorEvent::ResizeRejected(4, 90, 30))
    ));
}

#[test]
fn capture_result_keeps_the_image_scope_and_reads_an_old_server_as_path_only() {
    let new_ok = serde_json::json!({
        "event": "capture_result", "upload_id": 7, "ok": true,
        "path": "/r/s.png", "image": true,
    });
    let Some(MirrorEvent::CaptureResult {
        upload_id,
        ok,
        image,
        image_reason,
        ..
    }) = parse_capture_result(&serde_json::to_vec(&new_ok).unwrap())
    else {
        panic!("not a capture result");
    };
    assert_eq!(
        (upload_id, ok, image, image_reason),
        (Some(7), true, Some(true), None)
    );

    let path_only = serde_json::json!({
        "event": "capture_result", "upload_id": 8, "ok": true,
        "path": "/r/s.png", "image": false, "image_reason": "decode",
    });
    let Some(MirrorEvent::CaptureResult {
        image,
        image_reason,
        ..
    }) = parse_capture_result(&serde_json::to_vec(&path_only).unwrap())
    else {
        panic!("not a capture result");
    };
    assert_eq!(
        (image, image_reason.as_deref()),
        (Some(false), Some("decode"))
    );

    // 옛 서버는 image 를 싣지 않는다. 이미지까지 복사했다고 읽으면 안 된다.
    let old = serde_json::json!({
        "event": "capture_result", "upload_id": 9, "ok": true, "path": "/r/s.png",
    });
    let Some(MirrorEvent::CaptureResult { ok, image, .. }) =
        parse_capture_result(&serde_json::to_vec(&old).unwrap())
    else {
        panic!("not a capture result");
    };
    assert_eq!((ok, image), (true, None));

    // upload_id 없는 거절 회신도 읽는다.
    let rejected = serde_json::json!({ "event": "capture_result", "ok": false, "reason": "x" });
    assert!(matches!(
        parse_capture_result(&serde_json::to_vec(&rejected).unwrap()),
        Some(MirrorEvent::CaptureResult {
            upload_id: None,
            ok: false,
            ..
        })
    ));
}

#[test]
fn a_notification_sound_becomes_its_own_event() {
    let payload = serde_json::to_vec(&StreamControl::NotificationSound { surface_id: 5 }).unwrap();
    assert!(matches!(
        mirror_event_from_control(&payload),
        Some(MirrorEvent::NotificationSound(5))
    ));
}
