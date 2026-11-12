//! A user close keeps closing when terminal history does not fit the queue budget. The closed item
//! record keeps the terminals that fit with their history and records the rest without it.
use super::*;
use crate::app::journal::stall_budget::StallBudget;
use crate::core::layout_persistence::import::surface_data::SurfaceData;

const LIMIT: usize = tasty_ipc::admission::QUEUED_BYTES_LIMIT;
const SMALL_MARKER: &str = "small-terminal-history";
/// Disk scrollback file header: 4-byte magic and 4-byte version.
const DISK_HEADER: u64 = 8;

pub(super) fn boot() -> (EngineSession, JournalApplication) {
    let mut settings = crate::settings::Settings::default();
    crate::app::journal::test_shell::use_quiet_shell(&mut settings);
    settings.general.restore_layout = true;
    settings.general.restore_surface_content = true;
    let mut session = EngineSession::new_with_ids_and_settings(
        crate::runtime::engine_session::EngineSessionSpec {
            cols: 80,
            rows: 24,
            waker: Arc::new(|| {}),
            shared_ids: None,
            layout_slot: Some(1),
            memory: tasty_memory::writer::MemoryAccess::inline(Arc::new(std::sync::Mutex::new(
                tasty_memory::testing::InMemoryStorage::new(),
            ))),
            runner_registry: Arc::new(tasty_task_runtime::RunnerRegistry::new()),
        },
        settings,
    )
    .unwrap();
    let mut journal = JournalApplication::new(Arc::new(|| {})).unwrap();
    journal
        .begin_engine(
            &session,
            EngineSelection::Slot {
                slot: 1,
                resume: false,
            },
        )
        .unwrap();
    let mut stall = StallBudget::new(&journal);
    while !(session.journal_binding.is_some() && journal.is_ready(session.id)) {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        stall.nap("bootstrap");
    }
    (session, journal)
}

pub(super) fn request(method: &str, params: serde_json::Value, id: u64) -> JsonRpcRequest {
    serde_json::from_value(
        serde_json::json!({"jsonrpc":"2.0","method":method,"params":params,"id":id}),
    )
    .unwrap()
}

/// Run one public IPC request to its reply.
fn public(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    request: JsonRpcRequest,
) -> serde_json::Value {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    assert!(journal.admit_ipc(
        &crate::ipc::server::IpcCommand::new(request, tx),
        &crate::ipc::caller::CallerContext::local()
    ));
    let mut stall = StallBudget::new(journal);
    loop {
        journal.poll_bootstrap(&mut [&mut *session], None).unwrap();
        for (ticket, pending) in journal.requests_needing_resolution() {
            if pending.method == "workspace.create" {
                journal.resolve_workspace_creation(ticket, session, None);
            } else {
                journal.resolve_public_creation(ticket, session);
            }
        }
        if let Ok(response) = rx.try_recv() {
            assert!(response.error.is_none(), "{response:?}");
            return response.result.unwrap();
        }
        stall.nap("public request");
    }
}

/// The first workspace gets a second terminal tab; a second workspace keeps the engine open.
/// Returns the first workspace and its terminals in creation order.
pub(super) fn workspace_with_terminals(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    extra: usize,
) -> (u32, Vec<u32>) {
    let workspace = session.core_state.local_workspaces()[0].id;
    let first = session.core_state.local_workspaces()[0].all_surface_ids()[0];
    let pane = session.core_state.find_pane_for_surface(first).unwrap();
    public(
        journal,
        session,
        request("workspace.create", serde_json::json!({}), 900),
    );
    let mut terminals = vec![first];
    for index in 0..extra {
        let created = public(
            journal,
            session,
            request(
                "tab.create",
                serde_json::json!({"pane_id":pane,"type":"terminal"}),
                901 + index as u64,
            ),
        );
        terminals.push(created["surface_id"].as_u64().unwrap() as u32);
    }
    (workspace, terminals)
}

/// History rows of `cells` cells holding 60 000 bytes each.
fn rows(count: usize, cells: usize) -> Vec<tasty_terminal::ScrollbackLine> {
    let cell = "x".repeat(60_000);
    let attrs = termwiz::cell::CellAttributes::default();
    let row = tasty_terminal::ScrollbackLine::from_cells(
        std::iter::repeat_n((cell.as_str(), &attrs), cells),
        false,
    );
    vec![row; count]
}

fn small_history() -> Vec<tasty_terminal::ScrollbackLine> {
    let attrs = termwiz::cell::CellAttributes::default();
    vec![tasty_terminal::ScrollbackLine::from_cells(
        std::iter::once((SMALL_MARKER, &attrs)),
        false,
    )]
}

fn inject(session: &mut EngineSession, surface: u32, lines: Vec<tasty_terminal::ScrollbackLine>) {
    session
        .runtime
        .terminals
        .get_mut(surface)
        .unwrap()
        .inject_scrollback(lines);
}

fn lower_bound(session: &EngineSession, surface: u32) -> u64 {
    session
        .runtime
        .terminals
        .get(surface)
        .unwrap()
        .scrollback_serialized_len()
}

fn user_close_workspace(journal: &mut JournalApplication, session: &EngineSession, workspace: u32) {
    let origin = crate::intent::IntentOrigin::User {
        source: crate::core::origin::UserSource::Menu("close-history-test"),
    };
    journal.admit_intent_request(
        session.id,
        "intent.close",
        serde_json::json!({
            "target": tasty_core::CloseTarget::Workspace(workspace),
            "capture": true,
            "user_close": true,
        }),
        &origin,
        None,
    );
}

/// Resolve the admitted close without executing it.
fn resolve(journal: &mut JournalApplication, session: &mut EngineSession) -> u64 {
    let mut stall = StallBudget::new(journal);
    loop {
        journal.poll_bootstrap(&mut [&mut *session], None).unwrap();
        let requests = journal.requests_needing_resolution();
        if let Some((ticket, _)) = requests
            .iter()
            .find(|(_, request)| request.method == "intent.close")
        {
            journal.resolve_ipc_for_engine(*ticket, session);
            return *ticket;
        }
        stall.nap("close resolution");
    }
}

/// Per terminal, whether the closed item record holds history and whether it holds the marker.
fn captured_history(journal: &JournalApplication, ticket: u64) -> Vec<(u32, Option<bool>)> {
    let pending = &journal.commands.pending[&ticket];
    let Some(Work::CaptureClosed { surfaces, .. }) = &pending.queued else {
        panic!(
            "user close did not capture: {:?}",
            pending.queued.as_ref().map(|w| w.kind())
        );
    };
    let mut found: Vec<_> = surfaces
        .iter()
        .filter(|surface| surface.kind == "terminal")
        .map(
            |surface| match SurfaceData::decode(&surface.bytes).unwrap() {
                SurfaceData::Terminal { scrollback, .. } => (
                    surface.surface,
                    scrollback.map(|bytes| {
                        bytes
                            .windows(SMALL_MARKER.len())
                            .any(|window| window == SMALL_MARKER.as_bytes())
                    }),
                ),
                SurfaceData::Generic { .. } => panic!("terminal capture changed kind"),
            },
        )
        .collect();
    found.sort_unstable();
    found
}

fn total_pending_bytes(journal: &JournalApplication) -> usize {
    journal
        .commands
        .pending
        .values()
        .map(|pending| pending.bytes)
        .sum()
}

fn drain(journal: &mut JournalApplication, session: &mut EngineSession) {
    let mut stall = StallBudget::new(journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [&mut *session], None).unwrap();
        for (ticket, _) in journal.requests_needing_resolution() {
            journal.resolve_ipc_for_engine(ticket, session);
        }
        stall.nap("close completion");
    }
}

fn finish(journal: &mut JournalApplication, session: &mut EngineSession) -> JsonRpcResponse {
    drain(journal, session);
    journal
        .commands
        .completed_intents
        .last()
        .unwrap()
        .response
        .clone()
}

/// The disk file holding `surface`'s spilled rows, found by its owner and exact size.
fn disk_file(surface: u32, records: u64) -> std::path::PathBuf {
    // reason: reads the shared disk scrollback directory; the file is picked by owner and size.
    let directory = std::env::temp_dir().join("tasty-scrollback-debug");
    let prefix = format!("surface-{}-{surface}-", std::process::id());
    let found: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        // Size from the path, not the directory entry: on Windows the entry carries the size
        // recorded in the directory listing, which NTFS updates late while the owner still
        // holds its write handle open.
        .filter(|path| {
            std::fs::metadata(path).is_ok_and(|meta| meta.len() == records + DISK_HEADER)
        })
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    found.into_iter().next().unwrap()
}

/// Logs warnings emitted while `run` executes.
fn warnings_during(run: impl FnOnce()) -> String {
    #[derive(Clone)]
    struct Buf(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buf = Buf(Arc::new(std::sync::Mutex::new(Vec::new())));
    let sink = buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .with_writer(move || sink.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        run();
    });
    String::from_utf8_lossy(&buf.0.lock().unwrap()).into_owned()
}

#[test]
fn a_user_close_with_an_oversized_terminal_still_closes() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
    let (large, small) = (terminals[0], terminals[1]);
    // One spilled row on disk and one in memory: the history is larger than the queue limit.
    {
        let terminal = session.runtime.terminals.get_mut(large).unwrap();
        terminal.enable_disk_scrollback(large);
        terminal.set_scrollback_limit(1);
    }
    inject(&mut session, large, rows(2, 600));
    inject(&mut session, small, small_history());
    let bound = lower_bound(&session, large);
    assert!(bound > LIMIT as u64, "{bound}");
    // A read of the oversized history would now fail and log; the decision must not read it.
    let row_bytes = (bound - DISK_HEADER) / 2;
    std::fs::remove_file(disk_file(large, row_bytes)).unwrap();

    user_close_workspace(&mut journal, &session, workspace);
    let mut ticket = 0;
    let logs = warnings_during(|| ticket = resolve(&mut journal, &mut session));
    assert!(
        !logs.contains("disk scrollback range read failed"),
        "the oversized history was read: {logs}"
    );
    assert_eq!(
        captured_history(&journal, ticket),
        vec![(large, None), (small, Some(true))],
        "the large terminal is recorded without history and the small one with it"
    );
    assert!(total_pending_bytes(&journal) <= LIMIT);

    let response = finish(&mut journal, &mut session);
    assert!(response.error.is_none(), "{response:?}");
    let result = response.result.unwrap();
    assert_eq!(result["closed"], true);
    assert_eq!(result["history_omitted"], serde_json::json!([large]));
    assert!(
        session
            .core_state
            .local_workspaces()
            .iter()
            .all(|candidate| candidate.id != workspace),
        "the workspace closed"
    );
    let notices = journal.take_notices();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(
        notices[0].level,
        crate::notification::NotificationLevel::Warning
    );
    assert_eq!(notices[0].engine, Some(session.id));
    assert_eq!(
        notices[0].title,
        crate::i18n::t("recovery.close_history_omitted_title")
    );
}

#[test]
fn terminals_that_fit_alone_but_not_together_keep_the_smaller_history() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 2);
    let (larger, smaller, small) = (terminals[0], terminals[1], terminals[2]);
    inject(&mut session, larger, rows(1, 610));
    inject(&mut session, smaller, rows(1, 600));
    inject(&mut session, small, small_history());
    let (a, b) = (
        lower_bound(&session, larger),
        lower_bound(&session, smaller),
    );
    assert!(a < LIMIT as u64 && b < LIMIT as u64 && a + b > LIMIT as u64);

    user_close_workspace(&mut journal, &session, workspace);
    let ticket = resolve(&mut journal, &mut session);
    assert_eq!(
        captured_history(&journal, ticket),
        vec![(larger, None), (smaller, Some(false)), (small, Some(true))]
    );
    assert!(total_pending_bytes(&journal) <= LIMIT);
    let response = finish(&mut journal, &mut session);
    assert_eq!(
        response.result.unwrap()["history_omitted"],
        serde_json::json!([larger])
    );
    assert_eq!(journal.take_notices().len(), 1);
}

#[test]
fn bytes_held_by_other_requests_shrink_the_history_budget_without_failing_the_close() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
    let (medium, small) = (terminals[0], terminals[1]);
    inject(&mut session, medium, rows(1, 600));
    inject(&mut session, small, small_history());
    assert!(lower_bound(&session, medium) < LIMIT as u64);

    user_close_workspace(&mut journal, &session, workspace);
    // Another admitted request holds most of the budget while the close resolves.
    let origin = crate::intent::IntentOrigin::User {
        source: crate::core::origin::UserSource::Menu("close-history-test"),
    };
    let holder = journal.next_ticket;
    journal.admit_intent_request(
        session.id,
        "intent.remote-row",
        serde_json::json!({"id": 0, "remove": true}),
        &origin,
        None,
    );
    journal.commands.pending.get_mut(&holder).unwrap().bytes = LIMIT - 8 * 1024 * 1024;
    let ticket = resolve(&mut journal, &mut session);
    assert_eq!(
        captured_history(&journal, ticket),
        vec![(medium, None), (small, Some(true))]
    );
    assert!(total_pending_bytes(&journal) <= LIMIT);
    journal.commands.pending.get_mut(&holder).unwrap().bytes = 0;
    let response = finish(&mut journal, &mut session);
    let closed = journal
        .commands
        .completed_intents
        .iter()
        .find(|intent| {
            intent
                .response
                .result
                .as_ref()
                .is_some_and(|result| result.get("closed").is_some())
        })
        .map(|intent| intent.response.clone())
        .unwrap_or(response);
    assert!(closed.error.is_none(), "{closed:?}");
    assert_eq!(closed.result.unwrap()["closed"], true);
}

#[test]
fn a_close_that_fits_keeps_every_history_and_does_not_warn() {
    let (mut session, mut journal) = boot();
    let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
    inject(&mut session, terminals[0], small_history());
    inject(&mut session, terminals[1], small_history());
    user_close_workspace(&mut journal, &session, workspace);
    let ticket = resolve(&mut journal, &mut session);
    assert_eq!(
        captured_history(&journal, ticket),
        vec![(terminals[0], Some(true)), (terminals[1], Some(true))]
    );
    let response = finish(&mut journal, &mut session);
    let result = response.result.unwrap();
    assert_eq!(result["closed"], true);
    assert!(result.get("history_omitted").is_none(), "{result}");
    assert!(journal.take_notices().is_empty());
}

#[test]
fn a_remote_user_close_takes_the_same_history_budget() {
    for user in [true, false] {
        let (mut session, mut journal) = boot();
        let (workspace, terminals) = workspace_with_terminals(&mut journal, &mut session, 1);
        let (large, small) = (terminals[0], terminals[1]);
        inject(&mut session, large, rows(2, 600));
        inject(&mut session, small, small_history());
        let ticket = journal.next_ticket;
        user_close_workspace(&mut journal, &session, workspace);
        // Only the reply decides whether a remote close records the closed item.
        journal.commands.pending.get_mut(&ticket).unwrap().reply =
            Reply::Remote(inbound::RemoteReply {
                engine: session.id,
                ticket,
                client: 1,
                op_id: 1,
                workspace,
                binding: std::sync::Weak::new(),
                user,
                restore: false,
                closes_workspace: false,
                converted: None,
                inherit_cwd: None,
            });
        assert_eq!(resolve(&mut journal, &mut session), ticket);
        let pending = &journal.commands.pending[&ticket];
        if user {
            assert_eq!(
                captured_history(&journal, ticket),
                vec![(large, None), (small, Some(true))]
            );
            assert_eq!(
                pending.closing.as_ref().unwrap().omitted_history,
                Some((session.id, vec![large]))
            );
            assert!(total_pending_bytes(&journal) <= LIMIT);
            drain(&mut journal, &mut session);
            let (_, response) = journal
                .take_remote_results()
                .into_iter()
                .next()
                .expect("remote close reply");
            assert_eq!(response.result.as_ref().unwrap()["closed"], true);
            assert_eq!(inbound::history_omitted(&response), Some(1));
            assert!(
                journal.take_notices().is_empty(),
                "the requesting client warns; the server window does not"
            );
            let mut replay = response;
            replay.idempotent_replay = true;
            assert_eq!(
                inbound::history_omitted(&replay),
                None,
                "a replay does not warn again"
            );
        } else {
            assert!(
                matches!(pending.queued, Some(Work::PutPayload(_))),
                "a remote agent close records no closed item"
            );
        }
    }
}
