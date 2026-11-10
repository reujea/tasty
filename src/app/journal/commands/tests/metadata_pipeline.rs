use super::*;

fn admitted(journal: &mut JournalApplication, session: &mut EngineSession, tickets: &[u64]) {
    let mut stall = StallBudget::new(journal);
    while !tickets.iter().all(|ticket| {
        journal
            .commands
            .pending
            .get(ticket)
            .is_some_and(|pending| pending.needs_resolution)
    }) {
        journal.poll_bootstrap(&mut [session], None).unwrap();
        stall.nap("metadata admission");
    }
}

#[test]
fn explicit_metadata_fixes_inputs_before_predecessor_ack_and_keeps_each_first_response() {
    let (mut session, mut journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    let mut tickets = Vec::new();
    let mut receivers = Vec::new();
    let mut requests = Vec::new();
    for (index, params) in [
        serde_json::json!({"id":workspace,"name":"pipeline-name"}),
        serde_json::json!({"id":workspace,"subtitle":"pipeline-subtitle"}),
        serde_json::json!({"id":workspace,"description":"pipeline-description"}),
    ]
    .into_iter()
    .enumerate()
    {
        let req = request(
            "workspace.update",
            params,
            Some(&format!("metadata-{index}")),
            index as u64 + 1,
        );
        tickets.push(journal.next_ticket);
        receivers.push(send(&mut journal, req.clone()));
        requests.push(req);
    }
    admitted(&mut journal, &mut session, &tickets);
    for ticket in &tickets {
        assert!(journal.bind_ipc_command_engine(*ticket, session.id, &session.core_state));
        journal.resolve_ipc_for_engine(*ticket, &session);
    }
    assert!(tickets.iter().all(|ticket| {
        let p = &journal.commands.pending[ticket];
        p.ordered_metadata && !p.needs_resolution && matches!(p.queued, Some(Work::Resolve { .. }))
    }));
    journal.submit_commands().unwrap();
    assert!(
        tickets
            .iter()
            .all(|ticket| journal.commands.pending[ticket].in_flight)
    );
    assert!(receivers.iter().all(|receiver| matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    )));
    let replies: Vec<_> = receivers
        .iter()
        .map(|receiver| finish(&mut journal, &mut session, receiver))
        .collect();
    for response in &replies {
        assert!(response.error.is_none(), "{response:?}");
    }
    assert_eq!(replies[0].result.as_ref().unwrap()["name"], "pipeline-name");
    assert_eq!(replies[1].result.as_ref().unwrap()["name"], "pipeline-name");
    assert_eq!(
        replies[1].result.as_ref().unwrap()["subtitle"],
        "pipeline-subtitle"
    );
    assert_eq!(
        replies[2].result.as_ref().unwrap()["description"],
        "pipeline-description"
    );
    let receiver = send(&mut journal, requests[0].clone());
    let repeated = finish(&mut journal, &mut session, &receiver);
    assert_eq!(repeated.result, replies[0].result);
    assert!(repeated.idempotent_replay);
    assert!(
        receivers
            .iter()
            .all(|receiver| receiver.try_recv().is_err())
    );
}

#[test]
fn index_target_remains_a_barrier_between_two_explicit_metadata_inputs() {
    let (mut session, mut journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    let mut tickets = Vec::new();
    let mut receivers = Vec::new();
    for (index, params) in [
        serde_json::json!({"id":workspace,"name":"first"}),
        serde_json::json!({"index":0,"name":"index-second"}),
        serde_json::json!({"id":workspace,"name":"third"}),
    ]
    .into_iter()
    .enumerate()
    {
        tickets.push(journal.next_ticket);
        receivers.push(send(
            &mut journal,
            request("workspace.update", params, None, index as u64),
        ));
    }
    admitted(&mut journal, &mut session, &tickets);
    for ticket in &tickets {
        journal.resolve_ipc_for_engine(*ticket, &session);
    }
    assert!(matches!(
        journal.commands.pending[&tickets[0]].queued,
        Some(Work::Resolve { .. })
    ));
    for ticket in &tickets[1..] {
        let pending = &journal.commands.pending[ticket];
        assert!(pending.needs_resolution);
        assert!(pending.queued.is_none());
    }
    for receiver in &receivers {
        assert!(finish(&mut journal, &mut session, receiver).error.is_none());
    }
    assert_eq!(session.core_state.local_workspaces()[0].name, "third");
}

#[test]
fn unresolved_scope_and_other_structural_methods_cannot_be_overtaken() {
    let (mut session, mut journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    let first = journal.next_ticket;
    let _barrier = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"barrier"}),
            None,
            1,
        ),
    );
    let second = journal.next_ticket;
    let _metadata = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id":workspace,"name":"later"}),
            None,
            2,
        ),
    );
    admitted(&mut journal, &mut session, &[first, second]);
    journal.resolve_ipc_for_engine(second, &session);
    assert!(journal.commands.pending[&second].needs_resolution);
    assert!(journal.commands.pending[&second].queued.is_none());
}

#[test]
fn only_explicit_local_patch_fields_are_eligible_for_pipelining() {
    let (session, _journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    assert!(scheduling::fixed_metadata(
        &request(
            "workspace.update",
            serde_json::json!({"id":workspace,"name":"yes"}),
            None,
            1
        ),
        &session.core_state
    ));
    for (method, params) in [
        (
            "workspace.update",
            serde_json::json!({"index":0,"name":"no"}),
        ),
        (
            "workspace.update",
            serde_json::json!({"id":workspace,"category":0}),
        ),
        (
            "workspace.update",
            serde_json::json!({"id":u32::MAX,"name":"missing"}),
        ),
        ("workspace.close", serde_json::json!({"id":workspace})),
        (
            "terminal.spawn",
            serde_json::json!({"workspace":workspace.to_string()}),
        ),
        (
            "workspace.move",
            serde_json::json!({"id":workspace,"to_index":0}),
        ),
    ] {
        assert!(!scheduling::fixed_metadata(
            &request(method, params, None, 1),
            &session.core_state
        ));
    }
}
