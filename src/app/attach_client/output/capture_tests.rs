//! 원격 캡처 회신은 기다리던 업로드를 요청한 View에만 한 번 알린다.

use super::*;
use crate::app::attach_client::tests::{supply_ids, test_session};
use crate::app::attach_client::wire::mirror_event_from_control;
use std::collections::HashMap;
use std::sync::Arc;

fn capture_result(upload_id: u64) -> MirrorEvent {
    capture_result_at(upload_id, &format!("/srv/shot-{upload_id}.png"))
}

fn capture_result_at(upload_id: u64, path: &str) -> MirrorEvent {
    let payload = serde_json::to_vec(&serde_json::json!({
        "event": "capture_result",
        "upload_id": upload_id,
        "ok": true,
        "path": path,
        "image": true,
    }))
    .unwrap();
    mirror_event_from_control(&payload).expect("캡처 회신은 이벤트가 된다")
}

/// `view` 창에서 회신들을 적용한 뒤 쌓인 토스트 수.
fn toasts_after(sess: &mut AttachClientSession, view: &Arc<()>, events: Vec<MirrorEvent>) -> usize {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine).for_view(Arc::downgrade(view));
        apply_mirror_events(sess, &mut host, &mut plugin_manager, events);
    }
    state.toasts.len()
}

#[test]
fn a_reply_reaches_the_requesting_view_once() {
    let view = Arc::new(());
    let mut sess = test_session(9_000, HashMap::new());
    sess.state.capture_uploads.insert(7, Arc::downgrade(&view));
    assert_eq!(
        toasts_after(
            &mut sess,
            &view,
            vec![capture_result(7), capture_result_at(7, "/srv/late.png")]
        ),
        1,
        "같은 업로드의 두 번째 회신은 내용이 달라도 다시 알리지 않는다"
    );
    assert!(sess.state.capture_uploads.is_empty());
}

#[test]
fn a_reply_nobody_waits_for_is_not_shown() {
    let view = Arc::new(());
    let mut sess = test_session(9_000, HashMap::new());
    sess.state.capture_uploads.insert(7, Arc::downgrade(&view));
    assert_eq!(
        toasts_after(&mut sess, &view, vec![capture_result(8)]),
        0,
        "다른 요청의 회신을 이 요청의 결과로 보이면 안 된다"
    );
    assert!(sess.state.capture_uploads.contains_key(&7));
}

#[test]
fn a_reply_for_another_view_is_not_shown_here() {
    let requester = Arc::new(());
    let other = Arc::new(());
    let mut sess = test_session(9_000, HashMap::new());
    sess.state
        .capture_uploads
        .insert(7, Arc::downgrade(&requester));
    assert_eq!(
        toasts_after(&mut sess, &other, vec![capture_result(7)]),
        0,
        "요청한 View 가 아닌 창(교체된 View 포함)에 안내하지 않는다"
    );
}

#[test]
fn concurrent_uploads_are_answered_out_of_order() {
    let view = Arc::new(());
    let mut sess = test_session(9_000, HashMap::new());
    for id in [1, 2] {
        sess.state.capture_uploads.insert(id, Arc::downgrade(&view));
    }
    assert_eq!(
        toasts_after(&mut sess, &view, vec![capture_result(2), capture_result(1)]),
        2
    );
    assert!(sess.state.capture_uploads.is_empty());
}

/// 기록이 있는 mirror에 회신을 적용한 뒤의 기록.
fn paste_after_reply(
    workspace: u32,
    recorded_for: u32,
    events: Vec<MirrorEvent>,
) -> Option<crate::remote::state::MirrorCapturePaste> {
    let view = Arc::new(());
    let mut sess = test_session(workspace, HashMap::new());
    sess.state.capture_uploads.insert(7, Arc::downgrade(&view));
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    engine.remote.mirror_capture_pastes.insert(
        recorded_for,
        crate::remote::state::MirrorCapturePaste {
            clipboard_before: 42,
            remote_path: None,
        },
    );
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut host =
            MirrorHost::windowed(&mut state, &mut engine).for_view(Arc::downgrade(&view));
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, events);
    }
    engine
        .remote
        .mirror_capture_pastes
        .get(&recorded_for)
        .cloned()
}

#[test]
fn a_reply_makes_the_capture_pasteable_until_the_local_clipboard_changes() {
    let paste = paste_after_reply(9_000, 9_000, vec![capture_result(7)]).expect("record");
    assert_eq!(paste.path_for(42), Some("/srv/shot-7.png"));
    assert_eq!(paste.path_for(43), None, "새로 복사한 로컬 내용이 우선한다");
}

#[test]
fn an_unanswered_capture_is_not_pasteable() {
    let paste = paste_after_reply(9_000, 9_000, vec![capture_result(8)]).expect("record");
    assert_eq!(paste.path_for(42), None);
}

#[test]
fn a_saved_capture_stays_pasteable_when_the_remote_clipboard_write_failed() {
    let payload = serde_json::to_vec(&serde_json::json!({
        "event": "capture_result",
        "upload_id": 7,
        "ok": false,
        "path": "/srv/saved.png",
        "reason": "no display",
    }))
    .unwrap();
    let event = mirror_event_from_control(&payload).expect("캡처 회신은 이벤트가 된다");
    let paste = paste_after_reply(9_000, 9_000, vec![event]).expect("record");
    assert_eq!(paste.path_for(42), Some("/srv/saved.png"));
}

#[test]
fn a_reply_does_not_change_another_mirrors_record() {
    let paste = paste_after_reply(9_000, 9_001, vec![capture_result(7)]).expect("record");
    assert_eq!(paste.remote_path, None);
}
