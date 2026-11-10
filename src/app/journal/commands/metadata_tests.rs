use super::*;

fn mirror(id: u32, name: &str) -> crate::model::Workspace {
    let mut workspace =
        crate::model::Workspace::new_with_terminal_marker(id, name.into(), id + 1, id + 2, id + 3);
    workspace.mirror = true;
    workspace
}

fn resolve_without_executing(journal: &mut JournalApplication, session: &mut EngineSession) {
    let mut stall = StallBudget::new(journal);
    loop {
        journal.poll_bootstrap(&mut [session], None).unwrap();
        let requests = journal.requests_needing_resolution();
        if !requests.is_empty() {
            for (ticket, _) in requests {
                journal.resolve_ipc_for_engine(ticket, session);
            }
            return;
        }
        stall.nap("resolve without executing");
    }
}

#[test]
fn workspace_response_is_frozen_and_host_notification_is_emitted_once() {
    let (mut session, mut journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    let sid = session.core_state.local_workspaces()[0].all_surface_ids()[0];
    let generation = session.runtime.terminals.generation(sid);
    let initial = request(
        "workspace.update",
        serde_json::json!({"id": workspace, "name": "first", "subtitle":"subtitle"}),
        Some("workspace-meta"),
        1,
    );
    let counts_before = crate::ipc::handler::idempotency::retry_counts();
    let rx = send(&mut journal, initial.clone());
    let response = finish(&mut journal, &mut session, &rx);
    assert!(response.error.is_none(), "{response:?}");
    assert_eq!(response.result.as_ref().unwrap()["name"], "first");
    assert_eq!(session.core_state.local_workspaces()[0].name, "first");
    assert_eq!(
        session.runtime.terminals.generation(sid),
        generation,
        "metadata preserves the physical owner"
    );
    assert!(
        matches!(journal.commands.completed_host_events.as_slice(), [(engine, notification::Notification::Ready(crate::core::host_event::PendingHostEvent::WorkspaceRenamed { workspace_id, name: Some(name), user_direct: false, .. }))] if *engine == session.id && *workspace_id == workspace && name == "first")
    );
    journal.commands.completed_host_events.clear();
    let rx = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id": workspace,"name":"second"}),
            None,
            2,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
    journal.commands.completed_host_events.clear();
    let rx = send(&mut journal, initial);
    let replay = finish(&mut journal, &mut session, &rx);
    assert!(replay.idempotent_replay);
    let counts_after = crate::ipc::handler::idempotency::retry_counts();
    assert!(counts_after.executed > counts_before.executed);
    assert!(counts_after.replayed > counts_before.replayed);
    assert_eq!(replay.result, response.result);
    assert_eq!(session.core_state.local_workspaces()[0].name, "second");
    assert!(journal.commands.completed_host_events.is_empty());
}

#[test]
fn mirror_delta_and_reconnect_replacement_cancel_stale_annotations_and_retries_do_not_reapply() {
    let (mut session, mut journal) = boot();
    session
        .core_state
        .push_mirror_workspace(mirror(900, "remote-initial"));
    for (index, replacement) in ["delta-name", "reconnected-name"].into_iter().enumerate() {
        let original = request(
            "workspace.update",
            serde_json::json!({"id":900, "name":"late-name","subtitle":"late-subtitle"}),
            Some(&format!("mirror-{index}")),
            index as u64,
        );
        let rx = send(&mut journal, original.clone());
        resolve_without_executing(&mut journal, &mut session);
        assert_eq!(session.core_state.mirror_workspaces()[0].subtitle, "");
        // Both production delta and reconnect replace through this exact owner boundary.
        assert!(
            session
                .core_state
                .replace_mirror_workspace(mirror(900, replacement))
                .is_ok()
        );
        let response = finish(&mut journal, &mut session, &rx);
        assert!(response.error.is_none());
        assert_eq!(session.core_state.mirror_workspaces()[0].name, replacement);
        assert_eq!(session.core_state.mirror_workspaces()[0].subtitle, "");
        assert!(journal.commands.completed_host_events.is_empty());
        let rx = send(&mut journal, original);
        assert!(finish(&mut journal, &mut session, &rx).idempotent_replay);
        assert_eq!(session.core_state.mirror_workspaces()[0].name, replacement);
    }
    let rx = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id":900, "name":"current"}),
            None,
            5,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
    assert_eq!(session.core_state.mirror_workspaces()[0].name, "current");
    assert_eq!(journal.commands.completed_host_events.len(), 1);
    assert!(
        session
            .core_state
            .local_workspaces()
            .iter()
            .all(|workspace| workspace.id != 900)
    );
}

#[test]
fn mixed_order_keeps_local_canonical_order_and_stored_move_does_not_move_again() {
    let (mut session, mut journal) = boot();
    let local = session.core_state.local_workspaces()[0].id;
    let ticket = journal.next_ticket;
    journal.next_ticket += 1;
    let creation = crate::app::journal::creation::Creation::default_workspace(
        ticket,
        &session,
        &journal.worker,
    )
    .unwrap();
    journal.creations.insert((session.id, ticket), creation);
    let mut stall = StallBudget::new(&journal);
    while !journal.creations.is_empty() {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        stall.nap("canonical order resolution");
    }
    let other = session.core_state.local_workspaces()[1].id;
    session
        .core_state
        .push_mirror_workspace(mirror(900, "remote"));
    assert!(
        session
            .core_state
            .apply_workspace_display_order(vec![local, 900, other])
    );
    let original = request(
        "workspace.move",
        serde_json::json!({"from_index":0,"to_index":2}),
        Some("mixed-order"),
        1,
    );
    let rx = send(&mut journal, original.clone());
    let result = finish(&mut journal, &mut session, &rx);
    assert_eq!(result.result.unwrap()["moved"], true);
    assert_eq!(
        session
            .core_state
            .workspaces()
            .iter()
            .map(|workspace| workspace.id)
            .collect::<Vec<_>>(),
        [900, other, local]
    );
    assert_eq!(
        session
            .core_state
            .local_workspaces()
            .iter()
            .map(|workspace| workspace.id)
            .collect::<Vec<_>>(),
        [other, local]
    );
    let binding = session.journal_binding.as_ref().unwrap();
    let store = tasty_event_store::EventStore::open(
        &tasty_utils::path::tasty_home()
            .unwrap()
            .join("structure/journal.db"),
        &binding.journal_id,
    )
    .unwrap();
    let model =
        crate::runtime::journal::load(&store, &tasty_event_store::StreamId::new(&binding.stream))
            .unwrap();
    assert_eq!(model.workspace_order, [other, local]);
    assert!(!model.workspaces.contains_key(&900));
    drop(store);
    let rx = send(
        &mut journal,
        request(
            "workspace.move",
            serde_json::json!({"id":local,"to_index":0}),
            None,
            2,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
    let rx = send(&mut journal, original);
    assert!(finish(&mut journal, &mut session, &rx).idempotent_replay);
    assert_eq!(session.core_state.workspace_at(0).unwrap().id, local);
}

pub(super) fn finish_intents(journal: &mut JournalApplication, session: &mut EngineSession) {
    let mut stall = StallBudget::new(journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [session], None).unwrap();
        for (ticket, _) in journal.requests_needing_resolution() {
            journal.resolve_ipc_for_engine(ticket, session);
        }
        stall.nap("finish intents");
    }
}

#[test]
fn direct_rename_commits_before_notification_and_clear_uses_current_selected_title() {
    use crate::core::host_event::PendingHostEvent as Event;
    use crate::intent::{IntentOrigin, rename::DirectRename};
    let (mut session, mut journal) = boot();
    let workspace = session.core_state.local_workspaces()[0].id;
    let sid = session.core_state.local_workspaces()[0].all_surface_ids()[0];
    let tab_id = session.core_state.find_tab_for_surface(sid).unwrap();
    let pane_id = session.core_state.find_pane_for_tab(tab_id).unwrap();
    let origin = IntentOrigin::User {
        source: crate::core::origin::UserSource::Menu("rename-test"),
    };
    journal.admit_direct_rename(
        session.id,
        &DirectRename::WorkspaceName {
            workspace_id: workspace,
            name: "direct-name".into(),
        },
        &origin,
    );
    assert_ne!(session.core_state.local_workspaces()[0].name, "direct-name");
    assert!(journal.commands.completed_host_events.is_empty());
    finish_intents(&mut journal, &mut session);
    let mut navigation = crate::state::navigation::NavigationState::default();
    navigation.reconcile(&session.core_state.workspaces());
    let (_, notification) = journal.commands.completed_host_events.pop().unwrap();
    assert!(
        matches!(notification.resolve(&session.as_ref(), &navigation), Some(Event::WorkspaceRenamed { workspace_id, user_direct: true, .. }) if workspace_id == workspace)
    );
    journal.admit_direct_rename(
        session.id,
        &DirectRename::TabName {
            tab_id,
            name: Some("explicit".into()),
        },
        &origin,
    );
    finish_intents(&mut journal, &mut session);
    journal.commands.completed_host_events.clear();
    journal.admit_direct_rename(
        session.id,
        &DirectRename::TabName { tab_id, name: None },
        &origin,
    );
    resolve_without_executing(&mut journal, &mut session);
    let tab = &session.core_state.find_pane_by_id(pane_id).unwrap().tabs[0];
    assert_eq!(tab.explicit_name.as_deref(), Some("explicit"));
    session
        .live
        .surface_titles
        .entry(sid)
        .or_default()
        .osc_title = Some("latest observed title".into());
    finish_intents(&mut journal, &mut session);
    assert!(
        session.core_state.find_pane_by_id(pane_id).unwrap().tabs[0]
            .explicit_name
            .is_none()
    );
    let (_, notification) = journal.commands.completed_host_events.pop().unwrap();
    assert!(
        matches!(notification.resolve(&session.as_ref(), &navigation), Some(Event::TabRenamed { title, user_direct: true, .. }) if title == "latest observed title")
    );
    journal.admit_direct_rename(
        session.id,
        &DirectRename::TabName {
            tab_id: 999_999,
            name: Some("missing".into()),
        },
        &origin,
    );
    finish_intents(&mut journal, &mut session);
    assert!(journal.commands.completed_host_events.is_empty());
    assert!(
        journal
            .commands
            .completed_intents
            .last()
            .unwrap()
            .response
            .error
            .is_some()
    );
}

fn saved_row(id: u32, name: &str) -> crate::model::Workspace {
    let mut workspace = mirror(id, name);
    workspace.remote_row = Some(crate::model::RemoteBinding {
        profile: "mac".into(),
        remote_workspace: 4,
    });
    workspace
}

fn saved_row_changes(
    resolved: &crate::app::journal::commands::workspace::Resolved,
) -> Vec<(u32, tasty_core::RemoteRow)> {
    resolved
        .changes
        .iter()
        .filter_map(|change| match change {
            tasty_core::StructuralCommand::SetRemoteRow { id, row } => Some((*id, row.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn saved_remote_rows_store_renames_and_display_moves_but_agent_mirrors_do_not() {
    let (mut session, mut journal) = boot();
    session
        .core_state
        .push_mirror_workspace(saved_row(900, "remote"));
    session
        .core_state
        .push_mirror_workspace(mirror(901, "agent"));
    let rename = |id| {
        request(
            "workspace.update",
            serde_json::json!({"id": id, "name": "renamed", "description": "d"}),
            None,
            1,
        )
    };
    let resolved =
        crate::app::journal::commands::workspace::resolve(&rename(900), &session, "stream")
            .ok()
            .unwrap();
    let rows = saved_row_changes(&resolved);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, 900);
    assert_eq!(rows[0].1.name, "renamed");
    assert_eq!(rows[0].1.description, "d");
    assert_eq!(rows[0].1.index, 1, "after the single local workspace");
    let agent = crate::app::journal::commands::workspace::resolve(&rename(901), &session, "stream")
        .ok()
        .unwrap();
    assert!(
        agent.changes.is_empty(),
        "an agent mirror stays display-only"
    );
    let moved = crate::app::journal::commands::workspace::resolve(
        &request(
            "workspace.move",
            serde_json::json!({"id": 900, "to_index": 0}),
            None,
            2,
        ),
        &session,
        "stream",
    )
    .ok()
    .unwrap();
    let rows = saved_row_changes(&moved);
    assert_eq!(rows.len(), 1, "only the saved row is stored");
    assert_eq!(rows[0].1.index, 0);
    let current = crate::app::journal::commands::workspace::resolve(
        &request(
            "intent.remote-row",
            serde_json::json!({"id": 900, "remove": false}),
            None,
            3,
        ),
        &session,
        "stream",
    )
    .ok()
    .unwrap();
    assert_eq!(saved_row_changes(&current)[0].1.name, "remote");
    journal.admit_remote_row(session.id, 900, false);
    journal.admit_remote_row(session.id, 900, true);
    finish_intents(&mut journal, &mut session);
    assert!(
        journal
            .commands
            .completed_intents
            .iter()
            .all(|intent| intent.response.error.is_none()),
        "storing and removing a saved row commit"
    );
}
