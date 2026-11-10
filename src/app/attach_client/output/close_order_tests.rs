//! Server close result against EOF: the mirror ends only on the awaited success reply.
use super::*;
use crate::app::attach_client::tests::test_session;
use std::collections::HashMap;
use tasty_remote::client_session::WorkspaceClose;

/// Applies `events` through the real event dispatcher, then decides the session like the output loop.
fn end_after(
    events: Vec<MirrorEvent>,
    eof: bool,
    installing: bool,
) -> (DisconnectDisposition, bool) {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let mut sess = test_session(9_000, HashMap::from([(42, 9_003)]));
    sess.state.saved_row = Some(5);
    sess.state.workspace_close = WorkspaceClose::Pending(3);
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, events);
    }
    let phase = sess.state.phase;
    decide_session_end(&sess, eof, phase, true, installing)
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
    let phase = sess.state.phase;
    assert_eq!(
        decide_session_end(&sess, true, phase, true, false),
        (DisconnectDisposition::ServerClosed, false)
    );
}

/// The success reply ends an saved remote row without reconnecting, whether the server's EOF arrives in the
/// same batch, in a later tick, or while a reconnect install owns the epoch.
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
