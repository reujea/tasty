use super::*;

const S: u32 = 10;

fn acked() -> (ResizeSync, Instant) {
    let mut sync = ResizeSync::default();
    sync.reset_for_connection(true);
    (sync, Instant::now())
}

fn failed(sync: &ResizeSync) -> Vec<u32> {
    sync.failed().collect()
}

#[test]
fn a_matching_reply_ends_the_wait_without_a_banner() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    assert_eq!(sync.next_deadline(), Some(t0 + RESIZE_ACK_TIMEOUT));
    sync.on_resize(S, 80, 24);
    assert_eq!(sync.next_deadline(), None);
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT * 3).is_empty());
    assert!(failed(&sync).is_empty());
}

#[test]
fn no_reply_resends_once_then_fails() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT / 2).is_empty());

    let t1 = t0 + RESIZE_ACK_TIMEOUT;
    let resend = sync.take_due(t1);
    assert_eq!(
        resend,
        vec![Resend {
            started: t0,
            surface_id: S,
            cols: 80,
            rows: 24
        }]
    );
    assert!(
        sync.should_send(S, 80, 24),
        "같은 요청을 다시 보낼 수 있어야 한다"
    );
    sync.note_resent(resend[0], t1);
    assert!(failed(&sync).is_empty());

    let t2 = t1 + RESIZE_ACK_TIMEOUT;
    assert!(sync.take_due(t2).is_empty(), "자동 재시도는 한 번뿐이다");
    assert_eq!(failed(&sync), vec![S]);
    assert_eq!(sync.next_deadline(), None, "실패한 뒤에는 기다리지 않는다");
}

#[test]
fn a_rejection_waits_for_the_deadline_and_a_second_rejection_fails_at_once() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    sync.on_rejected(S, 80, 24);
    assert!(
        sync.take_due(t0 + Duration::from_secs(1)).is_empty(),
        "잠시 기다린 뒤 다시 보낸다"
    );
    let t1 = t0 + RESIZE_ACK_TIMEOUT;
    let resend = sync.take_due(t1);
    assert_eq!(resend.len(), 1);
    sync.note_resent(resend[0], t1);
    sync.on_rejected(S, 80, 24);
    assert_eq!(failed(&sync), vec![S]);
    assert_eq!(sync.next_deadline(), None);
}

#[test]
fn a_late_reply_to_an_earlier_request_keeps_the_latest_wait() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    sync.note_sent(S, 100, 30, t0 + Duration::from_secs(1));
    sync.on_resize(S, 80, 24);
    sync.on_rejected(S, 80, 24);
    assert_eq!(
        sync.next_deadline(),
        Some(t0 + Duration::from_secs(1) + RESIZE_ACK_TIMEOUT),
        "이전 요청의 응답은 최신 대기를 풀지 않는다"
    );
    sync.on_resize(S, 100, 30);
    assert_eq!(sync.next_deadline(), None);
}

#[test]
fn the_banner_retry_resends_and_fails_again_without_an_automatic_retry() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    let t1 = t0 + RESIZE_ACK_TIMEOUT;
    let resend = sync.take_due(t1);
    sync.note_resent(resend[0], t1);
    let t2 = t1 + RESIZE_ACK_TIMEOUT;
    assert!(sync.take_due(t2).is_empty());
    assert_eq!(failed(&sync), vec![S]);

    let retry = sync.retry_failed(t2);
    assert_eq!(
        retry,
        vec![Resend {
            started: t2,
            surface_id: S,
            cols: 80,
            rows: 24
        }]
    );
    assert!(
        failed(&sync).is_empty(),
        "다시 시도하는 동안 배너 목록에서 뺀다"
    );
    assert!(sync.retrying());

    let t3 = t2 + RESIZE_ACK_TIMEOUT;
    assert!(
        sync.take_due(t3).is_empty(),
        "다시 시도는 자동 재시도 없이 판정한다"
    );
    assert_eq!(failed(&sync), vec![S]);
    assert!(!sync.retrying());

    let retry = sync.retry_failed(t3);
    sync.on_resize(S, retry[0].cols, retry[0].rows);
    assert!(failed(&sync).is_empty());
    assert!(!sync.retrying());
}

#[test]
fn dismiss_closes_the_banner_until_a_new_failure() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    let resend = sync.take_due(t0 + RESIZE_ACK_TIMEOUT);
    sync.note_resent(resend[0], t0 + RESIZE_ACK_TIMEOUT);
    sync.on_rejected(S, 80, 24);
    assert_eq!(failed(&sync), vec![S]);
    sync.dismiss_failed();
    assert!(failed(&sync).is_empty());

    sync.note_sent(S, 90, 24, t0);
    let resend = sync.take_due(t0 + RESIZE_ACK_TIMEOUT);
    sync.note_resent(resend[0], t0 + RESIZE_ACK_TIMEOUT);
    sync.on_rejected(S, 90, 24);
    assert_eq!(failed(&sync), vec![S]);
}

#[test]
fn an_older_server_is_never_waited_on() {
    let mut sync = ResizeSync::default();
    sync.reset_for_connection(false);
    let t0 = Instant::now();
    sync.note_sent(S, 80, 24, t0);
    assert!(!sync.should_send(S, 80, 24), "중복 전송 방지는 그대로다");
    assert_eq!(sync.next_deadline(), None);
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT * 10).is_empty());
    assert!(failed(&sync).is_empty());
}

#[test]
fn a_new_connection_loss_and_surface_removal_clear_the_state() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    sync.note_sent(S + 1, 80, 24, t0);
    sync.forget_pending();
    assert_eq!(sync.next_deadline(), None);
    assert!(
        sync.should_send(S, 80, 24),
        "손실 뒤에는 같은 크기도 다시 보낸다"
    );

    sync.note_sent(S, 80, 24, t0);
    sync.note_sent(S + 1, 80, 24, t0);
    sync.retain_surfaces(|id| id == S);
    assert_eq!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT).len(), 1);

    sync.reset_for_connection(true);
    assert_eq!(sync.next_deadline(), None);
    assert!(failed(&sync).is_empty());
    assert!(sync.should_send(S, 80, 24));
}

#[test]
fn the_descriptor_capability_list_decides_whether_to_wait() {
    use crate::transport::descriptor_has_capability;
    let name = tasty_ipc::stream::RESIZE_ACK_CAPABILITY;
    let key = tasty_ipc::stream::DESCRIPTOR_CAPABILITIES;
    let current = serde_json::json!({ "event": "attached_workspace", (key): [name] });
    let older = serde_json::json!({ "event": "attached_workspace" });
    assert!(descriptor_has_capability(&current, name));
    assert!(!descriptor_has_capability(&older, name));
    assert!(!descriptor_has_capability(
        &serde_json::json!({ (key): ["ipc.other"] }),
        name
    ));
}

#[test]
fn a_late_confirmation_of_the_failed_size_clears_the_failure() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    let resend = sync.take_due(t0 + RESIZE_ACK_TIMEOUT);
    sync.note_resent(resend[0], t0 + RESIZE_ACK_TIMEOUT);
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT * 2).is_empty());
    assert_eq!(failed(&sync), vec![S]);
    sync.on_resize(S, 90, 24);
    assert_eq!(failed(&sync), vec![S], "다른 크기는 그 실패를 풀지 않는다");
    sync.on_resize(S, 80, 24);
    assert!(
        failed(&sync).is_empty(),
        "밀린 요청이 늦게 확정되면 실패가 아니다"
    );
}

#[test]
fn the_banner_keeps_retrying_surfaces_and_dismiss_hides_a_retry_in_flight() {
    let (mut sync, t0) = acked();
    for id in [S, S + 1] {
        sync.note_sent(id, 80, 24, t0);
    }
    for resend in sync.take_due(t0 + RESIZE_ACK_TIMEOUT) {
        sync.note_resent(resend, t0 + RESIZE_ACK_TIMEOUT);
    }
    let t1 = t0 + RESIZE_ACK_TIMEOUT * 2;
    assert!(sync.take_due(t1).is_empty());
    assert_eq!(sync.banner_surfaces(), vec![S, S + 1]);

    sync.retry_failed(t1);
    assert_eq!(
        sync.banner_surfaces(),
        vec![S, S + 1],
        "다시 시도하는 동안에도 배너는 그 이름을 보인다"
    );
    sync.on_resize(S, 80, 24);
    assert_eq!(sync.banner_surfaces(), vec![S + 1]);

    sync.dismiss_failed();
    assert!(sync.banner_surfaces().is_empty());
    assert!(!sync.retrying(), "닫으면 다시 시도 표시도 사라진다");
    assert!(
        sync.next_deadline().is_some(),
        "닫아도 진행 중인 요청은 계속 기다린다"
    );
    assert!(sync.take_due(t1 + RESIZE_ACK_TIMEOUT).is_empty());
    assert_eq!(
        sync.banner_surfaces(),
        vec![S + 1],
        "그 요청이 실패하면 배너가 다시 보인다"
    );
}

/// Given 숨은 mirror의 대기, When 마감이 지나면, Then 재전송도 실패도 없이 사라지고 같은 크기를 다시 보낼 수
/// 있다. 마감 전 대기와 이미 확정된 실패는 남는다.
#[test]
fn a_hidden_mirror_deadline_is_abandoned_without_resend_or_failure() {
    const T: u32 = 11;
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    // T는 먼저 실패로 확정된다.
    sync.note_sent(T, 90, 30, t0);
    let resend = sync.take_due(t0 + RESIZE_ACK_TIMEOUT);
    for r in resend {
        if r.surface_id == T {
            sync.note_resent(r, t0 + RESIZE_ACK_TIMEOUT);
        }
    }
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT * 3).is_empty());
    assert_eq!(failed(&sync), vec![T]);

    let t1 = t0 + RESIZE_ACK_TIMEOUT * 4;
    sync.note_sent(S, 80, 24, t1);
    sync.abandon_due(t1 + RESIZE_ACK_TIMEOUT / 2, |_| true, |_, _| true);
    assert_eq!(
        sync.next_deadline(),
        Some(t1 + RESIZE_ACK_TIMEOUT),
        "마감 전 대기는 남는다"
    );

    sync.abandon_due(t1 + RESIZE_ACK_TIMEOUT, |id| id == S, |_, _| false);
    assert!(sync.take_due(t1 + RESIZE_ACK_TIMEOUT * 3).is_empty());
    assert_eq!(
        failed(&sync),
        vec![T],
        "확정된 실패는 남고 숨은 대기는 실패가 아니다"
    );
    assert!(
        sync.should_send(S, 80, 24),
        "보이게 되면 같은 크기를 다시 보낸다"
    );
}

/// Given 크기 A, B를 차례로 보낸 뒤 숨은 mirror, When A의 거절이 늦게 오면, Then B의 대기와 중복 전송
/// 기록은 남는다. When B의 거절이 오면, Then 대기를 버리고 보이게 되면 B를 다시 보낼 수 있다.
#[test]
fn a_hidden_late_rejection_of_an_earlier_size_keeps_the_latest_wait() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 80, 24, t0);
    let t1 = t0 + Duration::from_secs(1);
    sync.note_sent(S, 100, 30, t1);

    sync.abandon_rejected(S, 80, 24);
    assert_eq!(sync.next_deadline(), Some(t1 + RESIZE_ACK_TIMEOUT));
    assert!(!sync.should_send(S, 100, 30), "B는 이미 보냈다");

    sync.abandon_rejected(S, 100, 30);
    assert_eq!(sync.next_deadline(), None);
    assert!(sync.should_send(S, 100, 30));
    assert!(failed(&sync).is_empty());
}

/// 마감 판정 술어는 그 대기의 요청 크기를 받는다. 지금 목표와 다른 요청만 버릴 수 있다.
#[test]
fn the_abandon_predicate_sees_the_requested_size() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 100, 30, t0);
    sync.abandon_due(
        t0 + RESIZE_ACK_TIMEOUT,
        |_| false,
        |_, target| target == (80, 24),
    );
    assert_eq!(sync.next_deadline(), Some(t0 + RESIZE_ACK_TIMEOUT));
    sync.abandon_due(
        t0 + RESIZE_ACK_TIMEOUT,
        |_| false,
        |_, target| target == (100, 30),
    );
    assert_eq!(sync.next_deadline(), None);
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT * 3).is_empty());
}

/// Given 보이는 mirror의 요청이 실패한 뒤 레이아웃 목표가 바뀌었고, When 배너의 다시 시도가 마감을 넘기면,
/// Then 수동 재시도 대기는 목표 대조로 버리지 않고 실패로 다시 올린다(배너가 남는다).
#[test]
fn a_manual_retry_is_not_dropped_by_the_target_check() {
    let (mut sync, t0) = acked();
    sync.note_sent(S, 100, 30, t0);
    for r in sync.take_due(t0 + RESIZE_ACK_TIMEOUT) {
        sync.note_resent(r, t0 + RESIZE_ACK_TIMEOUT);
    }
    assert!(sync.take_due(t0 + RESIZE_ACK_TIMEOUT * 2).is_empty());
    assert_eq!(failed(&sync), vec![S]);

    let t1 = t0 + RESIZE_ACK_TIMEOUT * 3;
    assert_eq!(sync.retry_failed(t1).len(), 1);
    assert!(sync.retrying());
    let due = t1 + RESIZE_ACK_TIMEOUT;
    sync.abandon_due(due, |_| false, |_, target| target != (80, 24));
    assert!(sync.take_due(due).is_empty());
    assert_eq!(failed(&sync), vec![S], "수동 재시도는 배너에 다시 오른다");

    // 같은 상황의 숨은 mirror는 수동 재시도도 버린다(기존 규칙).
    let t2 = due + RESIZE_ACK_TIMEOUT;
    assert_eq!(sync.retry_failed(t2).len(), 1);
    sync.abandon_due(t2 + RESIZE_ACK_TIMEOUT, |_| true, |_, _| false);
    assert!(sync.take_due(t2 + RESIZE_ACK_TIMEOUT).is_empty());
    assert!(failed(&sync).is_empty());
}
