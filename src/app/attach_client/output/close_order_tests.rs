//! Server close result against EOF: the mirror ends only on the awaited success reply.
use super::*;
use crate::app::attach_client::tests::test_session;
use std::collections::HashMap;
use tasty_remote::client_session::{ObservedEnd, WorkspaceClose};
use tasty_remote::transport_end::EndReason;

/// Records a settled end the way the round observation does.
fn observe_end(sess: &mut AttachClientSession, reason: EndReason, routed: bool) {
    crate::app::attach_client::tests::settle_transport_end(sess, reason);
    sess.state.observed_end = Some(ObservedEnd {
        epoch: sess.transport.frame_tx.epoch(),
        reason,
        routed,
    });
}

/// Applies `events` through the real event dispatcher, then routes the session like the output loop.
/// `eof` records a settled loss before the events; `routed` marks it already handled, as after a
/// resync queued its handshake.
fn end_after(events: Vec<MirrorEvent>, eof: bool, routed: bool) -> (DisconnectDisposition, bool) {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
    sess.state.saved_row = Some(5);
    sess.state.workspace_close = WorkspaceClose::Pending(3);
    if eof {
        observe_end(&mut sess, EndReason::Lost, routed);
    }
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, events);
    }
    let phase = sess.state.phase;
    route_session_end(&mut sess, phase, false, false)
}

/// Two closes of one mirror prepared before the first is sent: both pass the preparation check, the
/// send step lets only the first through, and that first success still ends the mirror.
#[test]
fn a_second_queued_close_is_not_sent_and_the_first_success_still_ends_the_mirror() {
    use crate::app::attach_client::workspace_close::{note_sent, refusal_of, send_refusal};
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
    sess.state.saved_row = Some(5);
    assert_eq!(refusal_of(&sess), None, "first close prepared");
    assert_eq!(
        refusal_of(&sess),
        None,
        "second close prepared before the first is sent"
    );

    assert_eq!(send_refusal(&sess), None);
    note_sent(&mut sess, 1);
    assert!(
        send_refusal(&sess).is_some(),
        "the second close is not sent"
    );
    assert_eq!(sess.state.workspace_close, WorkspaceClose::Pending(1));

    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::StructuralSucceeded(1, None, 0)],
        );
    }
    observe_end(&mut sess, EndReason::Lost, false);
    let phase = sess.state.phase;
    assert_eq!(
        route_session_end(&mut sess, phase, false, false),
        (DisconnectDisposition::ServerClosed, false)
    );
}

/// The success reply ends an saved remote row without reconnecting, whether the server's EOF arrives in the
/// same batch, in a later tick, or after the end was already routed.
#[test]
fn the_awaited_success_ends_the_mirror_before_or_with_its_eof() {
    let success = || vec![MirrorEvent::StructuralSucceeded(3, None, 0)];
    assert_eq!(
        end_after(success(), true, false),
        (DisconnectDisposition::ServerClosed, false)
    );
    assert_eq!(
        end_after(success(), false, false),
        (DisconnectDisposition::ServerClosed, false),
        "the EOF of the next tick is not waited for"
    );
    assert_eq!(
        end_after(success(), true, true),
        (DisconnectDisposition::ServerClosed, false)
    );
}

/// A reply for another op, a refusal, or an EOF before any reply keeps the saved remote row reconnecting.
/// Only the missing reply is reported as an unknown result.
#[test]
fn without_the_awaited_success_the_mirror_is_not_ended() {
    assert_eq!(
        end_after(
            vec![MirrorEvent::StructuralSucceeded(2, None, 0)],
            true,
            false
        ),
        (DisconnectDisposition::Reconnect, true)
    );
    assert_eq!(
        end_after(vec![], true, false),
        (DisconnectDisposition::Reconnect, true)
    );
    assert_eq!(
        end_after(
            vec![MirrorEvent::StructuralFailed(3, Some("refused".into()))],
            true,
            false
        ),
        (DisconnectDisposition::Reconnect, false)
    );
    assert_eq!(
        end_after(vec![], false, false),
        (DisconnectDisposition::None, false),
        "a live connection keeps waiting without an unknown notice"
    );
}

/// S13·S7: 관측·설치 단계가 정한 멈춤은 ID 예약을 기다리는 이벤트가 남은 동안 미룬다. 그 이벤트에 닫기
/// 성공 회신이 있으면 멈춤보다 서버 닫기가 먼저이고, 회신 없이 풀리면 멈추며 결과 불명을 알린다.
#[test]
fn a_decided_stop_waits_for_deferred_events_and_yields_to_the_close_reply() {
    let stop_after = |events: Vec<MirrorEvent>| {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
        sess.state.saved_row = Some(5);
        sess.state.workspace_close = WorkspaceClose::Pending(3);
        sess.state.stop_after_events = Some(sess.transport.frame_tx.epoch());
        let phase = sess.state.phase;
        assert_eq!(
            route_session_end(&mut sess, phase, true, false),
            (DisconnectDisposition::None, false),
            "deferred while events wait for an id reservation"
        );
        assert!(sess.state.stop_after_events.is_some(), "the stop is kept");
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        {
            let mut host = MirrorHost::parked(&mut state, &mut engine);
            apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, events);
        }
        route_session_end(&mut sess, phase, false, false)
    };
    assert_eq!(
        stop_after(vec![MirrorEvent::StructuralSucceeded(3, None, 0)]),
        (DisconnectDisposition::ServerClosed, false)
    );
    assert_eq!(
        stop_after(vec![]),
        (DisconnectDisposition::Stop, true),
        "no reply: stop with an unknown close result"
    );
}

/// ifix1-1: Given 옛 전송에 정한 멈춤이 ID 예약 때문에 미뤄졌다, When 그 사이 새 전송이 설치되면, Then
/// 처리 단계는 새 전송을 멈추지 않고 옛 멈춤을 지운다. 새 전송의 끝이 없으므로 아무 처리도 없다.
#[test]
fn a_stop_decided_for_the_old_transport_does_not_stop_the_new_one() {
    let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
    sess.state.saved_row = Some(5);
    sess.state.stop_after_events = Some(sess.transport.frame_tx.epoch());
    let phase = sess.state.phase;
    assert_eq!(
        route_session_end(&mut sess, phase, true, false),
        (DisconnectDisposition::None, false),
        "deferred"
    );
    sess.transport = test_session(9_000, HashMap::new()).transport;

    assert_eq!(
        route_session_end(&mut sess, phase, false, false),
        (DisconnectDisposition::None, false),
        "the new transport is not stopped"
    );
    assert!(sess.state.stop_after_events.is_none(), "the old stop ends");
}

/// ifix1-2: Given 닫기 회신을 기다리던 연결이 끊겨 재연결 진입 때 결과 불명을 이미 알렸다, When 그 재연결
/// 중인 세션에 정한 멈춤이 적용되면, Then 멈추기만 하고 결과 불명을 다시 알리지 않는다.
#[test]
fn a_stop_of_a_reconnecting_session_does_not_repeat_the_unknown_close() {
    let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
    sess.state.saved_row = Some(5);
    sess.state.workspace_close = WorkspaceClose::Pending(3);
    sess.state.phase = SessionState::Reconnecting;
    sess.state.stop_after_events = Some(sess.transport.frame_tx.epoch());

    assert_eq!(
        route_session_end(&mut sess, SessionState::Reconnecting, false, false),
        (DisconnectDisposition::Stop, false)
    );
}

/// ifix1-3 (U8): Given 막힌 저장 행이 손실 복구(재동기화)를 시작한 뒤 옛 전송이 끊겼고 ID 예약을 기다리는
/// 이벤트가 남았다, When 처리하면, Then 바로 멈추지 않고 지금 전송의 멈춤으로 미룬다. 그 이벤트에 닫기 성공
/// 회신이 있으면 서버 닫기로 끝나고, 없이 풀리면 멈추며 결과 불명을 알린다.
#[test]
fn a_blocked_resync_waits_for_deferred_events_and_yields_to_the_close_reply() {
    let stop_after = |events: Vec<MirrorEvent>| {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
        sess.state.saved_row = Some(5);
        sess.state.workspace_close = WorkspaceClose::Pending(3);
        sess.state.resync_pending = Some(1);
        observe_end(&mut sess, EndReason::Lost, false);
        let phase = sess.state.phase;
        assert_eq!(
            route_session_end(&mut sess, phase, true, true),
            (DisconnectDisposition::None, false),
            "deferred while events wait for an id reservation"
        );
        assert!(
            sess.state.stop_after_events.is_some(),
            "a stop of this transport"
        );
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        {
            let mut host = MirrorHost::parked(&mut state, &mut engine);
            apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, events);
        }
        route_session_end(&mut sess, phase, false, true)
    };
    assert_eq!(
        stop_after(vec![MirrorEvent::StructuralSucceeded(3, None, 0)]),
        (DisconnectDisposition::ServerClosed, false)
    );
    assert_eq!(
        stop_after(vec![]),
        (DisconnectDisposition::Stop, true),
        "no reply: stop with an unknown close result"
    );
}

/// ifix1-4 (U8): Given 창에 있는 저장 행, When 손실 통지(`Desynced`)를 실제 이벤트 적용 경로로 받고 옛 전송이
/// 끊기면, Then 막힌 행은 Detach 만 보내고 복구 안내를 내지 않으며 처리는 멈춤이다. 막히지 않은 행은 복구
/// 안내를 한 번 내고 재동기화한다.
#[test]
fn a_loss_on_a_blocked_row_gives_no_recovery_notice_and_stops() {
    for blocked in [true, false] {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let (mut sess, frames_out) = crate::app::attach_client::tests::test_session_with_frames(
            9_000,
            HashMap::from([(42, 9_003)]),
        );
        sess.state.saved_row = Some(5);
        let toasts_before = state.toasts.len();
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        {
            let mut host = MirrorHost::windowed(&mut state, &mut engine).row_blocked(blocked);
            apply_mirror_events(
                &mut sess,
                &mut host,
                &mut plugin_manager,
                vec![MirrorEvent::Desynced { frames: 1 }],
            );
        }
        let sent: Vec<_> = frames_out
            .try_iter()
            .map(|queued| queued.frame.tag)
            .collect();
        assert_eq!(
            sent,
            vec![tasty_ipc::stream::StreamTag::Detach],
            "{blocked}"
        );
        assert_eq!(
            state.toasts.len(),
            toasts_before + usize::from(!blocked),
            "recovery notice only when it really resyncs ({blocked})"
        );
        observe_end(&mut sess, EndReason::Lost, false);
        let phase = sess.state.phase;
        let expected = if blocked {
            DisconnectDisposition::Stop
        } else {
            DisconnectDisposition::Resync
        };
        assert_eq!(
            route_session_end(&mut sess, phase, false, blocked),
            (expected, false)
        );
    }
}

/// S7: 같은 묶음에 닫기 성공 회신과 강제 끊김이 함께 오면 서버 닫기로 끝난다(G2-a). 손실 끝도 같다.
#[test]
fn a_close_reply_wins_over_a_forced_or_lost_end_in_the_same_batch() {
    for reason in [EndReason::Forced, EndReason::Lost] {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
        sess.state.saved_row = Some(5);
        sess.state.workspace_close = WorkspaceClose::Pending(3);
        observe_end(&mut sess, reason, false);
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        {
            let mut host = MirrorHost::parked(&mut state, &mut engine);
            apply_mirror_events(
                &mut sess,
                &mut host,
                &mut plugin_manager,
                vec![MirrorEvent::StructuralSucceeded(3, None, 0)],
            );
        }
        let phase = sess.state.phase;
        assert_eq!(
            route_session_end(&mut sess, phase, false, false),
            (DisconnectDisposition::ServerClosed, false),
            "{reason:?}"
        );
    }
}
