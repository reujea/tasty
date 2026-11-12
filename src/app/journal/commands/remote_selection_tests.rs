//! A remote client's new tab or pane split leaves the server's own View selection where it was.
//! The client chose its source in its own View; the server View is a separate selection.
use super::close_history_tests::{boot, workspace_with_terminals};
use super::inbound::InboundOp;
use super::*;
use crate::app::journal::stall_budget::StallBudget;
use crate::state::navigation::NavigationState;
use tasty_ipc::stream::{ForwardOrigin, SplitAxis, StructuralOp};

/// Runs every pending request through the same resolution the GUI uses until none is left.
fn complete(journal: &mut JournalApplication, session: &mut EngineSession) {
    let mut stall = StallBudget::new(journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [&mut *session], None).unwrap();
        for (ticket, request) in journal.requests_needing_resolution() {
            if request.method == "remote.structural" {
                let missing = journal.unresolved_remote_anchor(ticket, [session.as_ref()]);
                journal.resolve_remote_request(ticket, session, missing);
            } else {
                journal.resolve_ipc_for_engine(ticket, session);
            }
        }
        stall.nap("remote creation");
    }
}

/// Applies a completion to the server View the way the GUI does: an intent's activation selects its
/// surface, replacements remap, and a changed engine reconciles the selection with the new structure.
fn apply_to_view(
    journal: &mut JournalApplication,
    session: &EngineSession,
    navigation: &mut NavigationState,
) {
    let workspaces = session.core_state.workspaces();
    for result in std::mem::take(&mut journal.commands.completed_intents) {
        let Some(surface) = result.view.and_then(|view| view.activate_surface) else {
            continue;
        };
        let (index, pane) = session
            .core_state
            .find_workspace_index_for_surface(surface)
            .unwrap();
        let workspace = workspaces.iter().nth(index).unwrap();
        let pane = workspace.pane_layout().find_pane(pane).unwrap();
        let tab = pane
            .tabs
            .iter()
            .find(|tab| tab.contains_surface(surface))
            .unwrap();
        navigation.select_workspace(&workspaces, workspace.id);
        navigation.select_pane(workspace, pane.id);
        navigation.select_tab(pane, tab.id);
        navigation.select_surface(tab, surface);
    }
    for (_, replacement) in std::mem::take(&mut journal.replacements) {
        navigation.apply_replacement(replacement);
    }
    if !journal.take_changed_engines().is_empty() {
        navigation.reconcile(&workspaces);
    }
}

/// (workspace, focused pane, selected tab of that pane, selected surface of that tab)
fn selection(session: &EngineSession, navigation: &NavigationState) -> (u32, u32, u32, u32) {
    let workspaces = session.core_state.workspaces();
    let workspace = navigation.workspace_id(&workspaces).unwrap();
    let workspace = workspaces.iter().find(|ws| ws.id == workspace).unwrap();
    let pane = navigation.pane_id(workspace).unwrap();
    let pane = workspace.pane_layout().find_pane(pane).unwrap();
    let tab = navigation.tab_id(pane).unwrap();
    let tab = pane
        .tabs
        .iter()
        .find(|candidate| candidate.id == tab)
        .unwrap();
    (
        workspace.id,
        pane.id,
        tab.id,
        navigation.surface_id(tab).unwrap(),
    )
}

/// The server View selects the first tab; the client works in the second. A remote new tab and a remote
/// pane split complete through the journal and reach the server View, which still selects the first tab.
#[test]
fn a_remote_creation_leaves_the_server_selection_where_it_was() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
    let (server_choice, client_choice) = (terminals[0], terminals[1]);
    let mut navigation = NavigationState::default();
    {
        let workspaces = session.core_state.workspaces();
        navigation.reconcile(&workspaces);
        let ws = workspaces.iter().find(|ws| ws.id == workspace).unwrap();
        let (_, pane) = session
            .core_state
            .find_workspace_index_for_surface(server_choice)
            .unwrap();
        let pane = ws.pane_layout().find_pane(pane).unwrap();
        let tab = session
            .core_state
            .find_tab_for_surface(server_choice)
            .unwrap();
        let tab_ref = pane
            .tabs
            .iter()
            .find(|candidate| candidate.id == tab)
            .unwrap();
        navigation.select_workspace(&workspaces, workspace);
        navigation.select_pane(ws, pane.id);
        navigation.select_tab(pane, tab);
        navigation.select_surface(tab_ref, server_choice);
    }
    let before = selection(&session, &navigation);
    assert_eq!(before.3, server_choice);
    journal.update_completion_view(session.id, &session.core_state, &navigation);

    let hub = tasty_ipc::stream_hub::StreamHub::new();
    let client = hub.alloc_id();
    let _sink = hub.register(client);
    let members = session
        .core_state
        .local_workspaces()
        .iter()
        .find(|ws| ws.id == workspace)
        .unwrap()
        .all_surface_ids();
    let lock = session
        .live
        .occupancy
        .acquire_workspace(workspace, &terminals, &members, client)
        .unwrap();
    session
        .live
        .occupancy
        .set_attachment_ready(client, lock.granted_seq, true);
    let epoch = journal.runtime_epoch.unwrap();
    let pane_count = |session: &EngineSession| {
        session.core_state.local_workspaces()[0]
            .pane_layout()
            .all_pane_ids()
            .len()
    };
    let panes_before = pane_count(&session);
    let tabs_before = session.core_state.local_workspaces()[0]
        .all_surface_ids()
        .len();

    let ops = [
        StructuralOp::NewTab {
            anchor_surface_id: client_choice,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            inherit_cwd: None,
        },
        StructuralOp::SplitPane {
            anchor_surface_id: client_choice,
            direction: SplitAxis::Vertical,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            inherit_cwd: None,
        },
    ];
    for (op_id, op) in (1..).zip(ops) {
        let admitted = journal.admit_remote(
            session.id,
            &session.live,
            epoch,
            &hub,
            InboundOp {
                client,
                op_id,
                op,
                origin: ForwardOrigin::User,
            },
        );
        assert!(admitted.is_some(), "op {op_id} was not admitted");
        complete(&mut journal, &mut session);
        let results = journal.take_remote_results();
        assert_eq!(results.len(), 1);
        let (_, response) = &results[0];
        assert!(response.error.is_none(), "{response:?}");
        apply_to_view(&mut journal, &session, &mut navigation);
        assert_eq!(
            selection(&session, &navigation),
            before,
            "op {op_id} moved the server View selection"
        );
    }
    assert_eq!(pane_count(&session), panes_before + 1, "the split ran");
    assert_eq!(
        session.core_state.local_workspaces()[0]
            .all_surface_ids()
            .len(),
        tabs_before + 2,
        "both creations ran"
    );
}
