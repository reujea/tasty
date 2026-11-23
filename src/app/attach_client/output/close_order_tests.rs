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
        resync_discarded: false,
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
        sess.state.stop_after_events = Some(tasty_remote::client_session::DeferredStop::new(
            sess.transport.frame_tx.epoch(),
        ));
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
    sess.state.stop_after_events = Some(tasty_remote::client_session::DeferredStop::new(
        sess.transport.frame_tx.epoch(),
    ));
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

/// Given 닫기 회신을 기다리는(`Pending`) 재연결 중인 세션, When 그 세션에 정한 멈춤이 적용되면, Then 멈추기만
/// 하고 결과 불명은 Connected 세션의 끝이 아니라 알리지 않으며 기록도 바꾸지 않는다. 이미 알린(`Unanswered`)
/// 세션도 같다.
#[test]
fn a_stop_of_a_reconnecting_session_does_not_tell_the_unknown_close() {
    for close in [WorkspaceClose::Pending(3), WorkspaceClose::Unanswered(3)] {
        let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
        sess.state.saved_row = Some(5);
        sess.state.workspace_close = close;
        sess.state.phase = SessionState::Reconnecting;
        sess.state.stop_after_events = Some(tasty_remote::client_session::DeferredStop::new(
            sess.transport.frame_tx.epoch(),
        ));

        assert_eq!(
            route_session_end(&mut sess, SessionState::Reconnecting, false, false),
            (DisconnectDisposition::Stop, false),
            "{close:?}"
        );
        assert_eq!(sess.state.workspace_close, close, "the record is untouched");
    }
}

/// Given 미룬 멈춤에 명시적 연결 의사가 남았다, When 이벤트가 풀린 뒤 처리하면, Then 의사를 실은 멈춤
/// (`StopThenConnect`)으로 끝나고 멈춤은 지워진다. 연결된 세션의 닫기 회신을 기다리던 중이면 결과 불명도
/// 알린다. 같은 회차에 닫기 성공 회신이 있으면 서버 닫기가 이긴다.
#[test]
fn a_deferred_stop_with_a_connect_intent_routes_as_stop_then_connect() {
    let route = |close: WorkspaceClose| {
        let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
        sess.state.saved_row = Some(5);
        sess.state.workspace_close = close;
        let mut stop =
            tasty_remote::client_session::DeferredStop::new(sess.transport.frame_tx.epoch());
        stop.connect_after = true;
        sess.state.stop_after_events = Some(stop);
        let phase = sess.state.phase;
        let routed = route_session_end(&mut sess, phase, false, false);
        (routed, sess.state.stop_after_events.is_none())
    };
    assert_eq!(
        route(WorkspaceClose::Idle),
        ((DisconnectDisposition::StopThenConnect, false), true)
    );
    assert_eq!(
        route(WorkspaceClose::Pending(3)),
        ((DisconnectDisposition::StopThenConnect, true), true),
        "an unknown close result is told like a plain stop"
    );
    assert_eq!(
        route(WorkspaceClose::Closed).0,
        (DisconnectDisposition::ServerClosed, false)
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

/// S13 실제 배선: Given 멈춤이 정해진 연결의 출력 버퍼에 구조 변경(새 surface)과 그 뒤의 닫기 성공 회신이
/// 있고 ID 예약이 아직 없다, When 실제 버퍼 비우기로 적용하면, Then 구조 변경이 ID 예약을 기다려 그 뒤
/// 이벤트와 함께 버퍼로 돌아가고 처리는 멈춤을 미룬다. 예약이 채워진 뒤의 적용에서 닫기 회신이 들어와
/// 서버 닫기로 끝난다. 회차의 `has_pending` 연결(`finish_round`)은 같은 값을 넘긴다.
#[test]
fn a_decided_stop_waits_for_a_real_id_reservation_and_the_close_reply_behind_it_wins() {
    use crate::app::attach_client::tests::{ParkedEngine, parked_with_mirror, supply_ids};
    let ws_id = 9_000u32;
    let (survivor_remote, survivor_local, new_remote) = (42u32, 9_003u32, 43u32);
    let mut parked = parked_with_mirror(ws_id, survivor_local);
    let mut sess = test_session(ws_id, HashMap::from([(survivor_remote, survivor_local)]));
    sess.state.saved_row = Some(5);
    sess.state.workspace_close = WorkspaceClose::Pending(3);
    sess.state.stop_after_events = Some(tasty_remote::client_session::DeferredStop::new(
        sess.transport.frame_tx.epoch(),
    ));
    let tree = serde_json::json!({
        "id": 7, "name": "mirror", "focused_pane": 70,
        "panes": [ {
            "id": 70,
            "tabs": [ {
                "id": 30, "name": "Shell", "active": true, "focused_surface": survivor_remote,
                "layout": {
                    "type": "Split", "direction": "vertical", "ratio": 0.5,
                    "focus_second": false,
                    "first": { "type": "Leaf", "id": survivor_remote, "kind": "terminal" },
                    "second": { "type": "Leaf", "id": new_remote, "kind": "terminal" }
                }
            } ]
        } ]
    });
    let surfaces = vec![
        serde_json::json!({ "remote_id": survivor_remote, "role": "terminal", "cols": 80, "rows": 24 }),
        serde_json::json!({ "remote_id": new_remote, "role": "terminal", "cols": 80, "rows": 24 }),
    ];
    assert!(sess.transport.output.push(MirrorEvent::StructuralDelta {
        workspace_id: 7,
        tree,
        surfaces,
    }));
    assert!(
        sess.transport
            .output
            .push(MirrorEvent::StructuralSucceeded(3, None, 0))
    );
    let ParkedEngine {
        view_restore: state,
        session,
    } = &mut parked[1];
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut engine = session.borrow_mut();
        let mut host = MirrorHost::parked(state, &mut engine);
        host.drain_and_apply(&mut sess, &mut plugin_manager);
    }
    let deferred = sess.transport.output.has_pending();
    assert!(deferred, "the delta and the reply behind it went back");
    let phase = sess.state.phase;
    assert_eq!(
        route_session_end(&mut sess, phase, deferred, false),
        (DisconnectDisposition::None, false)
    );
    {
        let mut engine = session.borrow_mut();
        supply_ids(&engine);
        let mut host = MirrorHost::parked(state, &mut engine);
        host.drain_and_apply(&mut sess, &mut plugin_manager);
    }
    let deferred = sess.transport.output.has_pending();
    assert!(!deferred);
    assert_eq!(
        route_session_end(&mut sess, phase, deferred, false),
        (DisconnectDisposition::ServerClosed, false)
    );
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

/// Given 닫기 회신을 기다리던 연결된 세션의 끝, When 처리 단계가 재연결·재동기화로 분류하면, Then 결과 불명을
/// 한 번 알리고 그 요청에 알렸다고 기록한다. 같은 요청은 다음 처리에서 다시 알리지 않는다.
#[test]
fn routing_the_end_records_the_unknown_close_it_tells() {
    for resync in [false, true] {
        let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
        sess.state.saved_row = Some(5);
        sess.state.workspace_close = WorkspaceClose::Pending(3);
        if resync {
            sess.state.resync_pending = Some(1);
        }
        observe_end(&mut sess, EndReason::Lost, false);
        let expected = if resync {
            DisconnectDisposition::Resync
        } else {
            DisconnectDisposition::Reconnect
        };
        assert_eq!(
            route_session_end(&mut sess, SessionState::Connected, false, false),
            (expected, true),
            "resync {resync}"
        );
        assert_eq!(
            sess.state.workspace_close,
            WorkspaceClose::Unanswered(3),
            "resync {resync}"
        );
        // 같은 전송에 정한 멈춤을 Connected 단계로 다시 처리해도 같은 요청은 알리지 않는다.
        sess.state.stop_after_events = Some(tasty_remote::client_session::DeferredStop::new(
            sess.transport.frame_tx.epoch(),
        ));
        assert_eq!(
            route_session_end(&mut sess, SessionState::Connected, false, false),
            (DisconnectDisposition::Stop, false),
            "resync {resync}"
        );
    }
}

/// Given 옛 전송에서 op 1 의 결과 불명을 알렸다, When 재연결 설치가 새 전송을 넘기면
/// (`adopt_reconnected_transport`), Then op 번호와 닫기 진행이 새로 시작해 같은 번호의 새 닫기를 보낼 수 있고
/// 그 닫기가 결과 불명이 되면 다시 한 번 알린다.
#[test]
fn a_new_transport_tells_the_same_op_number_again() {
    use crate::app::attach_client::workspace_close::{
        note_sent, refusal_of, take_unknown_close_notice,
    };
    let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
    sess.state.op_seq = 7;
    sess.state.workspace_close = WorkspaceClose::Unanswered(1);
    let fresh = test_session(9_000, HashMap::new()).transport;

    crate::app::attach_client::connection::adopt_reconnected_transport(
        &mut sess,
        tasty_remote::client_session::InstallGuard::new(fresh),
    );

    assert_eq!(sess.state.op_seq, 0);
    assert_eq!(sess.state.workspace_close, WorkspaceClose::Idle);
    assert_eq!(refusal_of(&sess), None, "a close can be sent again");
    note_sent(&mut sess, 1);
    assert!(
        take_unknown_close_notice(&mut sess),
        "told on the new transport"
    );
    assert!(!take_unknown_close_notice(&mut sess));
}
