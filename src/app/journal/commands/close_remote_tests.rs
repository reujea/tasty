//! A remote holder closes only the workspace it holds; other holds and other targets are refused.
use super::close::{Cause, Request};
use super::close_history_tests::{boot, request, workspace_with_terminals};
use super::*;

fn close_workspace(workspace: u32) -> JsonRpcRequest {
    request(
        "intent.close",
        serde_json::json!({
            "target": tasty_core::CloseTarget::Workspace(workspace),
            "capture": false,
            "user_close": false,
        }),
        1,
    )
}

fn refusal(result: Result<Request, JsonRpcResponse>) -> String {
    match result {
        Ok(_) => panic!("the close was admitted"),
        Err(response) => response.error.expect("refusal carries an error").message,
    }
}

/// The holder may close its workspace despite its own hard hold. Another client, another target and an
/// ordinary close of the held workspace are refused.
#[test]
fn a_remote_holder_closes_only_its_held_workspace() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
    let other = session
        .core_state
        .local_workspaces()
        .iter()
        .map(|workspace| workspace.id)
        .find(|id| *id != workspace)
        .expect("a second workspace");
    session
        .live
        .occupancy
        .acquire_workspace(workspace, &terminals, &terminals, 7)
        .unwrap();
    let held = Cause::RemoteHolder {
        client: 7,
        workspace,
    };

    let admitted = Request::resolve(&close_workspace(workspace), &session, held)
        .unwrap_or_else(|response| panic!("{response:?}"));
    assert_eq!(
        admitted.target,
        tasty_core::CloseTarget::Workspace(workspace)
    );
    assert!(!admitted.replacement, "another server workspace remains");
    assert_eq!(admitted.expected.len(), terminals.len());

    let stranger = Cause::RemoteHolder {
        client: 8,
        workspace,
    };
    assert!(
        refusal(Request::resolve(
            &close_workspace(workspace),
            &session,
            stranger
        ))
        .contains("holder changed")
    );
    assert!(
        refusal(Request::resolve(&close_workspace(other), &session, held))
            .contains("outside its held workspace")
    );
    assert!(
        refusal(Request::resolve(
            &close_workspace(workspace),
            &session,
            Cause::Ordinary
        ))
        .contains("hard-occupied")
    );
}

/// A terminal that another client holds inside the held workspace blocks the holder's close.
#[test]
fn another_clients_hard_hold_blocks_the_remote_close() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
    session
        .live
        .occupancy
        .acquire_workspace(workspace, &terminals[..1], &terminals, 7)
        .unwrap();
    session.live.occupancy.acquire(terminals[1], 8).unwrap();
    let held = Cause::RemoteHolder {
        client: 7,
        workspace,
    };
    assert!(
        refusal(Request::resolve(
            &close_workspace(workspace),
            &session,
            held
        ))
        .contains(&format!("surface {}", terminals[1]))
    );
}

/// Closing the server's last workspace from a remote holder keeps the server window with a replacement
/// workspace. A local user's close of the last workspace keeps its window-closing contract.
#[test]
fn a_remote_close_of_the_servers_last_workspace_gets_a_replacement() {
    let (mut session, _journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    let terminals = session.core_state.local_workspaces()[0].all_surface_ids();
    session
        .live
        .occupancy
        .acquire_workspace(workspace, &terminals, &terminals, 7)
        .unwrap();
    let held = Cause::RemoteHolder {
        client: 7,
        workspace,
    };
    let admitted = Request::resolve(&close_workspace(workspace), &session, held)
        .unwrap_or_else(|response| panic!("{response:?}"));
    assert!(admitted.replacement);

    session.live.occupancy.force_detach_workspace(workspace);
    let local = Request::resolve(&close_workspace(workspace), &session, Cause::Ordinary)
        .unwrap_or_else(|response| panic!("{response:?}"));
    assert!(!local.replacement);
}

/// A workspace close that the server answered without closing (`closed:false`) is returned to the
/// client as a failure, so the mirror is not ended. A close that did happen is returned as a success.
#[test]
fn a_workspace_close_that_did_not_close_is_returned_as_a_failure() {
    use tasty_ipc::stream::StreamControl;
    let (mut session, _journal) = boot();
    let hub = tasty_ipc::stream_hub::StreamHub::new();
    let client = hub.alloc_id();
    let rx = hub.register(client);
    let engine = session.id;
    let reply = |op_id| inbound::RemoteReply {
        engine,
        ticket: op_id,
        client,
        op_id,
        workspace: 77,
        binding: hub.client_binding(client).expect("registered client"),
        user: true,
        restore: false,
        closes_workspace: true,
        converted: None,
        inherit_cwd: None,
    };
    let outcome = |rx: &tasty_ipc::stream_hub::SinkReceiver| {
        let frame = rx.recv().expect("structural result");
        match serde_json::from_slice::<StreamControl>(&frame.payload).expect("control frame") {
            StreamControl::StructuralResult {
                op_id, ok, reason, ..
            } => (op_id, ok, reason),
            other => panic!("unexpected control {other:?}"),
        }
    };

    let not_closed = reply(1);
    inbound::deliver_result(
        not_closed,
        JsonRpcResponse::success(
            serde_json::Value::Null,
            serde_json::json!({"closed": false}),
        ),
        &mut session.borrow_mut(),
        None,
        &hub,
    );
    assert_eq!(
        outcome(&rx),
        (1, false, Some("workspace was not closed".into()))
    );

    let closed = reply(2);
    inbound::deliver_result(
        closed,
        JsonRpcResponse::success(serde_json::Value::Null, serde_json::json!({"closed": true})),
        &mut session.borrow_mut(),
        None,
        &hub,
    );
    assert_eq!(outcome(&rx), (2, true, None));
}
