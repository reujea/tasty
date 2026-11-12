mod group_failure;
mod metadata_pipeline;

use super::*;
use crate::app::journal::stall_budget::StallBudget;
use std::time::{Duration, Instant};

fn request(method: &str, params: serde_json::Value, key: Option<&str>, id: u64) -> JsonRpcRequest {
    serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","method":method,"params":params,"id":id,"idempotency_key":key})).unwrap()
}

fn finish(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    receiver: &std::sync::mpsc::Receiver<JsonRpcResponse>,
) -> JsonRpcResponse {
    let mut stall = StallBudget::new(journal);
    loop {
        journal.poll_bootstrap(&mut [session], None).unwrap();
        for (ticket, request) in journal.requests_needing_resolution() {
            if request.method == "workspace.create" {
                journal.resolve_workspace_creation(ticket, session, None);
            } else {
                journal.resolve_ipc_for_engine(ticket, session);
            }
        }
        if let Ok(response) = receiver.try_recv() {
            return response;
        }
        stall.nap("structural IPC reply");
    }
}

fn send(
    journal: &mut JournalApplication,
    request: JsonRpcRequest,
) -> std::sync::mpsc::Receiver<JsonRpcResponse> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    assert!(journal.admit_ipc(
        &crate::ipc::server::IpcCommand::new(request, tx),
        &crate::ipc::caller::CallerContext::local()
    ));
    rx
}

#[test]
fn category_wire_results_and_rejections_survive_deletion_and_worker_restart() {
    let (mut session, mut journal) = boot();
    let mut stall = StallBudget::new(&journal);
    let create = request(
        "workspace_category.create",
        serde_json::json!({"name":"  Work  "}),
        Some("category-create"),
        1,
    );
    let first_rx = send(&mut journal, create.clone());
    let mut duplicate = create.clone();
    duplicate.id = Some(serde_json::json!(2));
    let duplicate_rx = send(&mut journal, duplicate);
    let created = finish(&mut journal, &mut session, &first_rx);
    let duplicated = finish(&mut journal, &mut session, &duplicate_rx);
    assert_eq!(created.result, duplicated.result);
    assert!(duplicated.idempotent_replay);
    let category = created.result.as_ref().unwrap()["id"].as_u64().unwrap();
    assert_eq!(created.result.as_ref().unwrap()["name"], "Work");
    assert_eq!(
        session.core_state.categories().len(),
        2,
        "duplicate admission reserved and created once"
    );
    let rename = request(
        "workspace_category.rename",
        serde_json::json!({"id":category,"name":"  Renamed  "}),
        Some("rename"),
        3,
    );
    let rx = send(&mut journal, rename.clone());
    let renamed = finish(&mut journal, &mut session, &rx);
    assert_eq!(renamed.result.as_ref().unwrap()["name"], "  Renamed  ");
    assert_eq!(session.core_state.categories()[1].name, "Renamed");
    let invalid = request(
        "workspace_category.create",
        serde_json::json!({"name":"renamed"}),
        Some("rejected-name"),
        4,
    );
    let rx = send(&mut journal, invalid.clone());
    let rejected = finish(&mut journal, &mut session, &rx);
    assert_eq!(rejected.error.as_ref().unwrap().code, -32602);
    let rx = send(
        &mut journal,
        request(
            "workspace_category.delete",
            serde_json::json!({"id":category}),
            Some("delete"),
            5,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
    assert_eq!(session.core_state.categories().len(), 1);
    drop(journal);
    let release_deadline = Instant::now() + Duration::from_secs(5);
    while !crate::runtime::resource_retirement::poll_engine_release(&mut session, None) {
        assert!(
            Instant::now() < release_deadline,
            "old test resources were not reaped"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    session.engine_release = None;
    session.core_state = crate::core::CoreState::new_base();
    session.journal_binding = None;
    // The old TerminalStore owners are not a second structural source; bootstrap installs replacements.
    let mut journal = JournalApplication::new(Arc::new(|| {})).unwrap();
    journal
        .begin_engine(
            &session,
            EngineSelection::Slot {
                slot: 1,
                resume: true,
            },
        )
        .unwrap();
    stall.watch(&journal);
    while !journal.is_ready(session.id) {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        journal.poll_restore_bootstrap(&session).unwrap();
        stall.nap("worker restart bootstrap");
    }
    for (original, expected) in [(create, created), (rename, renamed), (invalid, rejected)] {
        let rx = send(&mut journal, original);
        let replay = finish(&mut journal, &mut session, &rx);
        assert_eq!(replay.result, expected.result);
        assert_eq!(
            serde_json::to_value(replay.error).unwrap(),
            serde_json::to_value(expected.error).unwrap()
        );
        assert!(replay.idempotent_replay);
    }
    assert_eq!(
        session.core_state.categories().len(),
        1,
        "retries neither resolve missing category nor recreate it"
    );
}

fn boot() -> (EngineSession, JournalApplication) {
    boot_with_layout(None)
}

fn boot_with_layout(layout: Option<serde_json::Value>) -> (EngineSession, JournalApplication) {
    boot_with(layout, Arc::new(|| {}))
}

/// `wake` is the App's wake for the worker, the spawner and the journal itself.
fn boot_with(
    layout: Option<serde_json::Value>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> (EngineSession, JournalApplication) {
    let resume = layout.is_some();
    let mut settings = crate::settings::Settings::default();
    crate::app::journal::test_shell::use_quiet_shell(&mut settings);
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
    if let Some(layout) = layout {
        let directory = tasty_utils::path::tasty_home().unwrap().join("layouts");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("01.json"), layout.to_string()).unwrap();
    }
    let mut journal = JournalApplication::new(wake).unwrap();
    journal
        .begin_engine(&session, EngineSelection::Slot { slot: 1, resume })
        .unwrap();
    let mut stall = StallBudget::new(&journal);
    while !journal.is_ready(session.id) {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        if resume {
            journal.poll_restore_bootstrap(&session).unwrap();
        }
        stall.nap_with(|| format!(
            "bootstrap (started={} epoch={:?} binding={} opening={:?} creations={:?} cleanup={} restore_queue={} restore_reads={} restore_ready={} restore_done={} id_refills={} pending_materializations={})",
            journal.started,
            journal.runtime_epoch,
            session.journal_binding.is_some(),
            journal
                .opening
                .get(&session.id)
                .map(|opening| (opening.ticket, opening.projected)),
            journal
                .creations
                .iter()
                .map(|(key, creation)| (
                    key,
                    creation.ticket,
                    creation.pauses_observation(),
                    creation.needs_cleanup_poll()
                ))
                .collect::<Vec<_>>(),
            journal.resource_cleanups.len(),
            journal.restorations.queued_count(),
            journal.restorations.read_count(),
            journal.restorations.ready(session.id).len(),
            journal.restorations.boot_done(session.id),
            journal.execution_id_requests.len(),
            session.pending_materializations.len(),
        ));
    }
    (session, journal)
}

#[test]
fn oversized_admission_and_resolution_do_not_halt_other_requests() {
    let (mut session, mut journal) = boot();
    let too_big = "x".repeat(crate::runtime::journal_product::MAX_COMMAND_INPUT_BYTES);
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":too_big}),
            Some("oversized"),
            1,
        ),
    );
    let response = finish(&mut journal, &mut session, &rx);
    assert_eq!(
        response.error.unwrap().code,
        crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG
    );
    assert!(!journal.is_halted());
    let large = "a".repeat(2 * 1024 * 1024);
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":large}),
            Some("large-valid"),
            2,
        ),
    );
    let created = finish(&mut journal, &mut session, &rx);
    assert!(created.error.is_none());
    assert_eq!(
        created.result.unwrap()["name"].as_str().unwrap().len(),
        large.len()
    );

    // A leader is already admitted and a follower joined before internal resolution is refused.
    let keyed = request(
        "workspace_category.create",
        serde_json::json!({"name":"small"}),
        Some("oversized-resolution"),
        3,
    );
    let leader = send(&mut journal, keyed.clone());
    let follower = send(&mut journal, keyed);
    let mut stall = StallBudget::new(&journal);
    loop {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        if journal
            .commands
            .pending
            .values()
            .any(|pending| pending.replay)
        {
            break;
        }
        stall.nap("follower join");
    }
    let ticket = *journal.commands.pending.keys().next().unwrap();
    journal
        .commands
        .pending
        .get_mut(&ticket)
        .unwrap()
        .needs_resolution = false;
    journal.commands.pending.get_mut(&ticket).unwrap().queued = Some(Work::Resolve {
        changes: Vec::new(),
        response: Some(ResponsePlan::Fixed(JsonRpcResponse::success(
            serde_json::Value::Null,
            serde_json::Value::String("z".repeat(tasty_ipc::admission::QUEUED_BYTES_LIMIT + 1)),
        ))),
    });
    let first = finish(&mut journal, &mut session, &leader);
    let second = finish(&mut journal, &mut session, &follower);
    assert_eq!(
        first.error.unwrap().code,
        crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG
    );
    assert_eq!(
        second.error.unwrap().code,
        crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG
    );
    assert!(journal.commands.pending.is_empty());
    assert!(!journal.is_halted());
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"after-pressure"}),
            None,
            4,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
}

#[test]
fn resolved_rename_releases_raw_params_before_retaining_input_and_wire_reply() {
    let (mut session, mut journal) = boot();
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"a"}),
            None,
            1,
        ),
    );
    let id = finish(&mut journal, &mut session, &rx).result.unwrap()["id"]
        .as_u64()
        .unwrap();
    let name = "b".repeat(2 * 1024 * 1024);
    let rx = send(
        &mut journal,
        request(
            "workspace_category.rename",
            serde_json::json!({"id":id,"name":name}),
            None,
            2,
        ),
    );
    let mut stall = StallBudget::new(&journal);
    let ticket = loop {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        if let Some((ticket, _)) = journal.requests_needing_resolution().into_iter().next() {
            break ticket;
        }
        stall.nap("rename resolution");
    };
    journal.resolve_ipc_for_engine(ticket, &session);
    let pending = &journal.commands.pending[&ticket];
    assert!(pending.request.params.is_null());
    let Work::Resolve { changes, response } = pending.queued.as_ref().unwrap() else {
        panic!("resolved");
    };
    let retained = serde_json::to_vec(&pending.request).unwrap().len()
        + serde_json::to_vec(&(changes, response)).unwrap().len();
    assert!(
        retained <= pending.bytes,
        "retained={retained} reserved={}",
        pending.bytes
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
}

/// 실행 중 halt는 사유와 재시작 안내를 담은 오류 알림 하나와 비포커스 안내가 된다. 같은 halt는
/// 다시 알리지 않는다.
#[test]
fn a_runtime_halt_is_reported_once_as_an_error_notice() {
    let (mut session, mut journal) = boot();
    assert!(
        journal
            .take_notices()
            .iter()
            .all(|notice| notice.popup.as_ref().is_none_or(|popup| {
                popup.variant != crate::app::journal::notices::NoticeVariant::Halt
            })),
        "no halt notice before the halt"
    );
    journal
        .worker
        .fail_next_publication
        .store(true, std::sync::atomic::Ordering::Release);
    let _rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"halt-notice"}),
            Some("halt-notice"),
            1,
        ),
    );
    let mut stall = StallBudget::new(&journal);
    while journal.poll_bootstrap(&mut [&mut session], None).is_ok() {
        for (ticket, _) in journal.requests_needing_resolution() {
            journal.resolve_ipc_for_engine(ticket, &session);
        }
        stall.nap("publication halt");
    }
    let reason = journal.halted.clone().unwrap();
    let notices = journal.take_notices();
    let halts: Vec<_> = notices
        .iter()
        .filter(|notice| {
            notice.popup.as_ref().is_some_and(|popup| {
                popup.variant == crate::app::journal::notices::NoticeVariant::Halt
            })
        })
        .collect();
    assert_eq!(halts.len(), 1, "{notices:?}");
    let halt = halts[0];
    assert_eq!(halt.level, crate::notification::NotificationLevel::Error);
    assert_eq!(halt.title, crate::i18n::t("notice.runtime_halted_title"));
    assert!(halt.body.contains(&reason), "{}", halt.body);
    assert!(
        halt.body
            .contains(crate::i18n::t("notice.runtime_halted_body")),
        "{}",
        halt.body
    );
    let database_path = journal.database.display().to_string();
    assert!(halt.body.contains(&database_path), "{}", halt.body);
    assert!(journal.poll_bootstrap(&mut [&mut session], None).is_err());
    assert!(
        journal.take_notices().is_empty(),
        "the same halt is reported once"
    );
}

#[test]
fn committed_publication_failure_halts_readers_and_fails_all_pending_replies() {
    let (mut session, mut journal) = boot();
    journal
        .worker
        .fail_next_publication
        .store(true, std::sync::atomic::Ordering::Release);
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"committed-not-published"}),
            Some("halt-cut"),
            1,
        ),
    );
    let mut stall = StallBudget::new(&journal);
    loop {
        if journal.poll_bootstrap(&mut [&mut session], None).is_err() {
            break;
        }
        for (ticket, _) in journal.requests_needing_resolution() {
            journal.resolve_ipc_for_engine(ticket, &session);
        }
        stall.nap("publication halt");
    }
    assert!(journal.is_halted());
    assert!(
        rx.recv_timeout(Duration::from_secs(1))
            .unwrap()
            .error
            .is_some()
    );
    assert!(journal.commands.pending.is_empty());
    assert!(
        !session
            .core_state
            .categories()
            .iter()
            .any(|category| category.name == "committed-not-published")
    );
    assert!(
        journal
            .reject_halted_request(&request("tree", serde_json::json!({}), None, 2))
            .unwrap()
            .error
            .is_some()
    );
    let binding = session.journal_binding.clone().unwrap();
    let home = tasty_utils::path::tasty_home().unwrap();
    drop(journal);
    let store = tasty_event_store::EventStore::open(
        &home.join("structure/journal.db"),
        &binding.journal_id,
    )
    .unwrap();
    let model =
        crate::runtime::journal::load(&store, &tasty_event_store::StreamId::new(&binding.stream))
            .unwrap();
    assert!(
        model
            .categories
            .values()
            .any(|category| category.name == "committed-not-published"),
        "failure is after durable commit, not an admission refusal"
    );
}

#[cfg(feature = "gui")]
#[test]
fn stale_settings_reset_completion_cannot_overwrite_a_newer_settings_intent() {
    let (mut session, mut journal) = boot();
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"will-reset"}),
            None,
            8,
        ),
    );
    let category = finish(&mut journal, &mut session, &rx).result.unwrap()["id"]
        .as_u64()
        .unwrap() as u32;
    let workspace = session.core_state.local_workspaces()[0].id;
    journal
        .admit_fixed_intent(
            &session,
            vec![tasty_core::StructuralCommand::SetWorkspaceCategory {
                workspace_id: workspace,
                category,
            }],
            &crate::intent::IntentOrigin::System,
        )
        .unwrap();
    let mut stall = StallBudget::new(&journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        stall.nap("settings intent");
    }
    assert_eq!(session.core_state.categories().len(), 2);
    assert_eq!(session.core_state.local_workspaces()[0].category, category);
    let mut off = session.runtime.settings.clone();
    off.general.workspace_categories_enabled = false;
    let first = journal.note_settings_intent().unwrap();
    let (done, mut wait) = crate::app::settings_change::pair(std::sync::Arc::new(|| {}));
    journal.admit_settings_reset(
        first,
        vec![StreamCommand {
            stream: session.journal_binding.as_ref().unwrap().stream.clone(),
            command: tasty_core::StructuralCommand::ResetCategories,
        }],
        off,
        &crate::intent::IntentOrigin::System,
        Some(done),
    );
    let second = journal.note_settings_intent().unwrap();
    assert!(second > first);
    // The ordinary service-side update B has been applied while A's reset awaits its commit.
    session
        .runtime
        .settings
        .general
        .workspace_categories_enabled = true;
    session.runtime.settings.general.startup_command = "latest-setting-B".into();
    let mut stall = StallBudget::new(&journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        stall.nap("settings intent");
    }
    assert!(
        journal.take_settings_results().is_empty(),
        "the actual App continuation drain must not deliver obsolete Settings A"
    );
    assert!(
        matches!(
            wait.poll(),
            crate::app::settings_change::SettingsPoll::Ready(
                crate::app::settings_change::SettingsOutcome::Superseded
            )
        ),
        "the request that waited on obsolete Settings A learns it was replaced"
    );
    assert!(
        session
            .runtime
            .settings
            .general
            .workspace_categories_enabled
    );
    assert_eq!(
        session.runtime.settings.general.startup_command,
        "latest-setting-B"
    );
    assert_eq!(
        session.core_state.categories().len(),
        1,
        "committed reset facts are retained"
    );
    assert_eq!(session.core_state.local_workspaces()[0].category, 0);
}

/// category 초기화가 journal 오류로 끝나면 기다리는 요청은 거절을 받고 설정은 적용되지 않는다.
#[cfg(feature = "gui")]
#[test]
fn failed_settings_reset_rejects_the_waiting_request_without_applying_the_settings() {
    let (mut session, mut journal) = boot();
    let before = session.runtime.settings.general.startup_command.clone();
    let mut target = session.runtime.settings.clone();
    target.general.workspace_categories_enabled = false;
    target.general.startup_command = "must-not-apply".into();
    let generation = journal.note_settings_intent().unwrap();
    let (done, mut wait) = crate::app::settings_change::pair(std::sync::Arc::new(|| {}));
    // 없는 stream 을 가리켜 reset 이 journal 오류로 끝나게 한다.
    journal.admit_settings_reset(
        generation,
        vec![StreamCommand {
            stream: "missing-stream-for-reset".into(),
            command: tasty_core::StructuralCommand::ResetCategories,
        }],
        target,
        &crate::intent::IntentOrigin::System,
        Some(done),
    );
    let mut stall = StallBudget::new(&journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        stall.nap("failed settings reset");
    }
    assert!(
        matches!(
            wait.poll(),
            crate::app::settings_change::SettingsPoll::Pending
        ),
        "the journal completion alone does not answer the request"
    );
    let mut completions = journal.take_settings_results();
    assert_eq!(completions.len(), 1);
    let completion = completions.pop().unwrap();
    assert!(completion.response.error.is_some(), "the reset failed");
    let Err((_, message)) = completion.into_settings() else {
        panic!("a failed reset yields no settings to apply");
    };
    assert!(!message.is_empty());
    let crate::app::settings_change::SettingsPoll::Ready(
        crate::app::settings_change::SettingsOutcome::Rejected(reason),
    ) = wait.poll()
    else {
        panic!("the waiting request is rejected");
    };
    assert!(reason.starts_with("category reset failed"), "{reason}");
    assert_eq!(session.runtime.settings.general.startup_command, before);
    assert!(
        matches!(
            wait.poll(),
            crate::app::settings_change::SettingsPoll::Dropped
        ),
        "the request is answered once"
    );
}

#[cfg(feature = "gui")]
#[test]
fn fixed_intent_budget_counts_its_commands_and_rejects_before_admission_without_losing_capacity() {
    let (mut session, mut journal) = boot();
    let tab = session.core_state.local_workspaces()[0]
        .pane_layout()
        .first_pane()
        .unwrap()
        .tabs[0]
        .id;
    journal
        .admit_fixed_intent(
            &session,
            vec![tasty_core::StructuralCommand::RenameTab {
                tab_id: tab,
                name: Some("fixed-name".into()),
            }],
            &crate::intent::IntentOrigin::System,
        )
        .unwrap();
    let ticket = *journal.commands.pending.keys().next().unwrap();
    let pending = &journal.commands.pending[&ticket];
    assert!(pending.fixed.is_some());
    assert!(pending.request.params.is_null());
    assert_eq!(pending.bytes, pending_weight(pending));
    assert!(!pending.admitted);
    // A larger derived fixed payload hits the pre-Admit stage, so no Resolve can be sent.
    journal.commands.pending.get_mut(&ticket).unwrap().fixed = Some(vec![StreamCommand {
        stream: session.journal_binding.as_ref().unwrap().stream.clone(),
        command: tasty_core::StructuralCommand::RenameTab {
            tab_id: tab,
            name: Some("x".repeat(tasty_ipc::admission::QUEUED_BYTES_LIMIT)),
        },
    }]);
    journal.refresh_command_weight(ticket);
    assert!(journal.commands.pending.is_empty());
    let result = journal.commands.completed_intents.pop().unwrap();
    assert_eq!(
        result.response.error.unwrap().code,
        crate::ipc::protocol::ERR_COMMAND_QUEUE_FULL
    );
    let rx = send(
        &mut journal,
        request(
            "workspace_category.create",
            serde_json::json!({"name":"after-fixed-pressure"}),
            None,
            1,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rx).error.is_none());
    assert!(!journal.is_halted());
}

#[cfg(not(feature = "gui"))]
#[test]
fn headless_pending_category_intent_uses_the_explicit_engine_journal_admission() {
    let (mut session, mut journal) = boot();
    let mut core = crate::ipc::handler::cli_entry_tests::test_core();
    let mut state =
        crate::state::RequestContext::new(&session.as_ref().read(), core.preset_store.clone());
    state.engine_id = Some(session.id);
    state.dispatch_intent(
        crate::app::command::DomainIntent::CreateCategory {
            name: "headless-queued".into(),
        }
        .from_agent_ipc(),
    );
    crate::intent::headless::drain_pending_intents_in_app(
        &mut core,
        &mut state,
        &mut session.borrow_mut(),
        &mut journal,
    );
    assert_eq!(
        session.core_state.categories().len(),
        1,
        "draining the intent does not perform the old direct writer"
    );
    let pending = journal.commands.pending.values().next().unwrap();
    let Some(Work::Admit(header)) = &pending.queued else {
        panic!("journal admission");
    };
    assert_eq!(header.actor, "agent");
    let mut stall = StallBudget::new(&journal);
    while !journal.commands.pending.is_empty() {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        journal.resolve_headless_requests(&mut session, &mut state, &core);
        stall.nap("headless intent");
    }
    assert!(
        session
            .core_state
            .categories()
            .iter()
            .any(|category| category.name == "headless-queued")
    );
    assert!(!journal.is_halted());
}

#[path = "metadata_tests.rs"]
mod metadata;

#[path = "close_receipt_tests.rs"]
mod close_receipt;

#[path = "replacement_settle_tests.rs"]
mod replacement_settle;

#[path = "preparation_tests.rs"]
mod preparation;

#[path = "recovery_tests.rs"]
mod recovery;

/// The child command uses `sh` arithmetic and waits on the PTY child process.
#[cfg(unix)]
#[path = "child_input_tests.rs"]
mod child_input;

#[path = "closing_surfaces_tests.rs"]
mod closing_surfaces;

#[path = "read_command_tests.rs"]
mod read_command;

#[test]
fn each_keyed_journal_request_reports_only_its_initial_retry_decision() {
    use crate::ipc::handler::idempotency::RetryOutcome as O;
    let (_session, mut journal) = boot();
    for (key, result, expected) in [
        (
            Some("new"),
            Ok(ResultValue::NeedsResolution),
            Some(O::Executed),
        ),
        (
            Some("joined"),
            Ok(ResultValue::JoinedAdmission { leader_ticket: 100 }),
            Some(O::InFlight),
        ),
        (
            Some("conflict"),
            Err(JournalError::KeyConflict),
            Some(O::Conflicted),
        ),
        (None, Ok(ResultValue::NeedsResolution), None),
    ] {
        let ticket = journal.next_ticket;
        let _receiver = send(
            &mut journal,
            request(
                "workspace_category.create",
                serde_json::json!({"name":"counter"}),
                key,
                ticket,
            ),
        );
        let pending = journal.commands.pending.get_mut(&ticket).unwrap();
        assert_eq!(pending.take_retry_outcome(&result), expected);
        assert_eq!(pending.take_retry_outcome(&result), None);
        assert_eq!(
            pending.take_retry_outcome(&Ok(ResultValue::Cancelled)),
            None
        );
    }
}

#[cfg(feature = "gui")]
#[path = "divider_tests.rs"]
mod divider;

#[cfg(all(feature = "gui", target_os = "linux"))]
#[path = "font_notice_tests.rs"]
mod font_notice;

#[test]
fn workspace_create_waits_for_final_wire_and_replays_without_a_second_resource() {
    let (mut session, mut journal) = boot();
    let before = session.core_state.workspaces().len();
    let initial = request(
        "workspace.create",
        serde_json::json!({"name":"durable-created","subtitle":"sub","description":"desc"}),
        Some("workspace-created"),
        501,
    );
    let rx = send(&mut journal, initial.clone());
    let follower = send(&mut journal, initial.clone());
    let response = finish(&mut journal, &mut session, &rx);
    assert!(response.error.is_none(), "{response:?}");
    let body = response.result.as_ref().unwrap();
    let sid = body["surface_id"].as_u64().unwrap() as u32;
    assert_eq!(body["name"], "durable-created");
    assert_eq!(body["subtitle"], "sub");
    assert_eq!(body["description"], "desc");
    assert_eq!(body["index"], before);
    assert_eq!(session.core_state.workspaces().len(), before + 1);
    assert!(session.runtime.terminals.generation(sid).is_some());
    let generation = session.runtime.terminals.generation(sid);
    let joined = finish(&mut journal, &mut session, &follower);
    assert!(joined.idempotent_replay);
    assert_eq!(joined.result, response.result);
    use crate::core::host_event::PendingHostEvent as E;
    assert_eq!(
        journal
            .commands
            .completed_host_events
            .iter()
            .filter(|(_, event)| matches!(
                event,
                notification::Notification::Ready(E::WorkspaceCreated { .. })
            ))
            .count(),
        1
    );
    assert_eq!(
        journal
            .commands
            .completed_host_events
            .iter()
            .filter(|(_, event)| matches!(
                event,
                notification::Notification::Ready(E::WorkspaceRenamed { .. })
            ))
            .count(),
        1
    );
    assert_eq!(
        journal
            .commands
            .completed_host_events
            .iter()
            .filter(|(_, event)| matches!(
                event,
                notification::Notification::Ready(E::SurfaceCreated { .. })
            ))
            .count(),
        1
    );
    journal.commands.completed_host_events.clear();
    let ws = body["id"].as_u64().unwrap() as u32;
    let rename = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id":ws,"name":"later"}),
            None,
            502,
        ),
    );
    assert!(finish(&mut journal, &mut session, &rename).error.is_none());
    journal.commands.completed_host_events.clear();
    let retry = send(&mut journal, initial);
    let replay = finish(&mut journal, &mut session, &retry);
    assert!(replay.idempotent_replay);
    assert_eq!(replay.result, response.result);
    assert_eq!(session.runtime.terminals.generation(sid), generation);
    assert!(journal.commands.completed_host_events.is_empty());
}

#[test]
fn workspace_create_uses_completion_mirror_count_and_serialized_local_append_order() {
    let (mut session, mut journal) = boot();
    let first = send(
        &mut journal,
        request(
            "workspace.create",
            serde_json::json!({}),
            Some("create-index-1"),
            1,
        ),
    );
    let second = send(
        &mut journal,
        request(
            "workspace.create",
            serde_json::json!({}),
            Some("create-index-2"),
            2,
        ),
    );
    let mut stall = StallBudget::new(&journal);
    loop {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        let requests = journal.requests_needing_resolution();
        if let Some((ticket, _)) = requests.first() {
            journal.resolve_workspace_creation(*ticket, &session, None);
            break;
        }
        stall.nap("workspace creation");
    }
    // The remote display changes after admission but before the resource publication barrier.
    let mut mirror =
        crate::model::Workspace::new_with_terminal_marker(900, "remote".into(), 901, 902, 903);
    mirror.mirror = true;
    session.core_state.push_mirror_workspace(mirror);
    let a = finish(&mut journal, &mut session, &first).result.unwrap();
    let b = finish(&mut journal, &mut session, &second).result.unwrap();
    assert_eq!(a["name"], "Workspace 2");
    assert_eq!(a["index"], 2);
    assert_eq!(b["name"], "Workspace 4");
    assert_eq!(b["index"], 3);
    assert_eq!(
        session.core_state.workspace_at(2).unwrap().id,
        a["id"].as_u64().unwrap() as u32
    );
    assert_eq!(
        session.core_state.workspace_at(3).unwrap().id,
        b["id"].as_u64().unwrap() as u32
    );
    let retry = send(
        &mut journal,
        request(
            "workspace.create",
            serde_json::json!({}),
            Some("create-index-1"),
            3,
        ),
    );
    assert_eq!(finish(&mut journal, &mut session, &retry).result, Some(a));
}

#[test]
fn a_failed_public_factory_completes_the_request_without_halting_the_engine() {
    let (mut session, mut journal) = boot();
    let before = session.core_state.workspaces().len();
    let bad = send(
        &mut journal,
        request(
            "workspace.create",
            serde_json::json!({"type":"not-registered"}),
            Some("bad-kind"),
            1,
        ),
    );
    let response = finish(&mut journal, &mut session, &bad);
    assert!(response.error.is_some());
    assert!(!journal.is_halted());
    assert_eq!(session.core_state.workspaces().len(), before);
    assert!(session.pending_materializations.is_empty());
    let good = send(
        &mut journal,
        request("workspace.create", serde_json::json!({}), None, 2),
    );
    assert!(finish(&mut journal, &mut session, &good).error.is_none());
}

#[cfg(feature = "gui")]
#[test]
fn an_unfinished_external_effect_blocks_its_engine_but_not_another_engine() {
    let (mut first, mut journal) = boot();
    let mut second = EngineSession::new_with_ids_and_settings(
        crate::runtime::engine_session::EngineSessionSpec {
            cols: 80,
            rows: 24,
            waker: Arc::new(|| {}),
            shared_ids: None,
            layout_slot: Some(2),
            memory: tasty_memory::writer::MemoryAccess::inline(Arc::new(std::sync::Mutex::new(
                tasty_memory::testing::InMemoryStorage::new(),
            ))),
            runner_registry: Arc::new(tasty_task_runtime::RunnerRegistry::new()),
        },
        first.runtime.settings.clone(),
    )
    .unwrap();
    journal
        .begin_engine(
            &second,
            EngineSelection::Slot {
                slot: 2,
                resume: false,
            },
        )
        .unwrap();
    let mut stall = StallBudget::new(&journal);
    while !journal.is_ready(second.id) {
        journal
            .poll_bootstrap(&mut [&mut first, &mut second], None)
            .unwrap();
        stall.nap("held external effect");
    }
    let first_ws = first.core_state.local_workspaces()[0].id;
    let second_ws = second.core_state.local_workspaces()[0].id;
    let held_ticket = journal.next_ticket;
    let held = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id": first_ws, "name":"held"}),
            None,
            100,
        ),
    );
    while !journal.commands.pending[&held_ticket].needs_resolution {
        journal
            .poll_bootstrap(&mut [&mut first, &mut second], None)
            .unwrap();
        stall.nap("held external effect");
    }
    assert!(journal.bind_command_engine(held_ticket, first.id));
    // Persist a genuine external obligation but deliberately do not attach a transport runner.
    // Its command stays InProgress without delaying the storage worker or publication ACKs.
    let input = store_waiting_effect_input(&journal, held_ticket, &mut stall);
    journal
        .commands
        .pending
        .get_mut(&held_ticket)
        .unwrap()
        .queued = Some(Work::Resolve {
        changes: vec![StreamCommand {
            stream: first.journal_binding.as_ref().unwrap().stream.clone(),
            command: tasty_core::StructuralCommand::PrepareForward {
                operation: tasty_core::OperationId(String::new()),
                command_id: String::new(),
                input,
            },
        }],
        response: Some(ResponsePlan::Fixed(JsonRpcResponse::success(
            serde_json::Value::Null,
            serde_json::json!({"held":true}),
        ))),
    });
    while journal.commands.pending[&held_ticket]
        .waiting_command
        .is_none()
    {
        journal
            .poll_bootstrap(&mut [&mut first, &mut second], None)
            .unwrap();
        stall.nap("held external effect");
    }
    let same = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id": first_ws,"name":"same-engine"}),
            None,
            101,
        ),
    );
    let independent = send(
        &mut journal,
        request(
            "workspace.update",
            serde_json::json!({"id":second_ws,"name":"independent"}),
            None,
            102,
        ),
    );
    loop {
        journal
            .poll_bootstrap(&mut [&mut first, &mut second], None)
            .unwrap();
        for (ticket, request) in journal.requests_needing_resolution() {
            let session = if request.params["id"] == serde_json::json!(first_ws) {
                &first
            } else {
                &second
            };
            journal.resolve_ipc_for_engine(ticket, session);
        }
        if let Ok(response) = independent.try_recv() {
            assert!(response.error.is_none(), "{response:?}");
            break;
        }
        stall.nap("independent engine (blocked by external effects)");
    }
    assert!(matches!(
        held.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        same.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert_eq!(second.core_state.local_workspaces()[0].name, "independent");
    assert_ne!(first.core_state.local_workspaces()[0].name, "same-engine");
}

#[cfg(feature = "gui")]
fn store_waiting_effect_input(
    journal: &JournalApplication,
    held_ticket: u64,
    stall: &mut StallBudget,
) -> tasty_core::DataRef {
    use crate::runtime::journal_product::{Completion, Request as WorkerRequest};
    journal
        .worker
        .submit(WorkerRequest {
            ticket: held_ticket,
            work: Work::PutPayload(b"{}".to_vec()),
        })
        .unwrap();
    loop {
        match journal.worker.try_recv() {
            Ok(Completion::Finished {
                ticket,
                result: Ok(ResultValue::InputStored(input)),
            }) => {
                assert_eq!(ticket, held_ticket);
                return input;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                stall.nap("payload store");
            }
            other => panic!("unexpected payload result: {other:?}"),
        }
    }
}

/// 명시 cwd 는 kind 와 관계없이 create 에 간다. explorer 는 그 폴더를 루트로 열고 탭 이름도 그 폴더다.
#[test]
fn an_explicit_cwd_opens_an_explorer_there_from_every_creation_method() {
    let (mut session, mut journal) = boot();
    let base = tempfile::tempdir().unwrap();
    let folder = |name: &str| {
        let dir = base.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    };
    let explorer = |session: &EngineSession, sid: u32| {
        let surface = session.runtime.surfaces.get(&sid).expect("surface exists");
        let ex = surface
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .expect("explorer surface");
        (ex.cwd().to_path_buf(), ex.tabs.len())
    };
    let tab_name = |session: &EngineSession, sid: u32| {
        let pane = session.core_state.find_pane_for_surface(sid).unwrap();
        let tab = session.core_state.find_tab_for_surface(sid).unwrap();
        session
            .core_state
            .find_pane_by_id(pane)
            .unwrap()
            .tabs
            .iter()
            .find(|t| t.id == tab)
            .unwrap()
            .name
            .clone()
    };

    // workspace.create 는 실제 해소 함수로 cwd 를 정한 뒤 만든다.
    let ws_dir = folder("ws");
    let params = serde_json::json!({"type":"explorer","cwd":ws_dir.to_string_lossy()});
    let (mut state, scope_engine) = crate::state::tests::test_state();
    let cwd = crate::ipc::handler::workspace::resolve_create_cwd(
        &params,
        "explorer",
        &crate::ipc::request_scope::RequestScope::capture(
            &mut state,
            &scope_engine.core_state,
            #[cfg(feature = "gui")]
            None,
        ),
        &scope_engine.as_ref(),
        &|_| None,
        &serde_json::Value::Null,
    )
    .unwrap();
    assert_eq!(cwd.as_deref(), Some(ws_dir.as_path()));
    let rx = send(&mut journal, request("workspace.create", params, None, 901));
    let ws = loop {
        journal.poll_bootstrap(&mut [&mut session], None).unwrap();
        for (ticket, _) in journal.requests_needing_resolution() {
            journal.resolve_workspace_creation(ticket, &session, cwd.clone());
        }
        if let Ok(response) = rx.try_recv() {
            break response;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert!(ws.error.is_none(), "{ws:?}");
    let ws = ws.result.unwrap();
    let ws_sid = ws["surface_id"].as_u64().unwrap() as u32;
    assert_eq!(explorer(&session, ws_sid), (ws_dir.clone(), 1));
    assert_eq!(tab_name(&session, ws_sid), "ws");
    let pane = session.core_state.find_pane_for_surface(ws_sid).unwrap();

    // tab.create·split 은 창의 완료 단계와 같은 resolve_public_creation 으로 해소한다.
    let public = |journal: &mut JournalApplication,
                  session: &mut EngineSession,
                  rx: &std::sync::mpsc::Receiver<JsonRpcResponse>| {
        let mut stall = StallBudget::new(journal);
        loop {
            journal.poll_bootstrap(&mut [&mut *session], None).unwrap();
            for (ticket, _) in journal.requests_needing_resolution() {
                journal.resolve_public_creation(ticket, session);
            }
            if let Ok(response) = rx.try_recv() {
                return response;
            }
            stall.nap("public creation reply");
        }
    };
    let create = |journal: &mut JournalApplication,
                  session: &mut EngineSession,
                  method: &str,
                  params: serde_json::Value,
                  id: u64| {
        let rx = send(journal, request(method, params, None, id));
        let response = public(journal, session, &rx);
        assert!(response.error.is_none(), "{method}: {response:?}");
        let body = response.result.unwrap();
        body["surface_id"]
            .as_u64()
            .or_else(|| body["new_surface_id"].as_u64())
            .unwrap() as u32
    };

    // tab.create 는 명시 cwd 가 있으면 @home 기본 path 를 채우지 않는다.
    let tab_dir = folder("tab");
    let sid = create(
        &mut journal,
        &mut session,
        "tab.create",
        serde_json::json!({"pane_id":pane,"type":"explorer","cwd":tab_dir.to_string_lossy()}),
        902,
    );
    assert_eq!(explorer(&session, sid), (tab_dir.clone(), 1));
    assert_eq!(tab_name(&session, sid), "tab");

    // 명시 path 는 cwd 보다 먼저다.
    let path_dir = folder("given");
    let sid = create(
        &mut journal,
        &mut session,
        "tab.create",
        serde_json::json!({"pane_id":pane,"type":"explorer","cwd":tab_dir.to_string_lossy(),"path":path_dir.to_string_lossy()}),
        903,
    );
    assert_eq!(explorer(&session, sid).0, path_dir);

    // 둘 다 없으면 예전처럼 홈이고, 탭 이름은 그 홈의 이름이다. CLI 는 빠진 값을 null 로 보낸다.
    let home = crate::model::default_root();
    let home_name = home.file_name().unwrap().to_string_lossy().into_owned();
    let sid = create(
        &mut journal,
        &mut session,
        "tab.create",
        serde_json::json!({"pane_id":pane,"type":"explorer"}),
        904,
    );
    assert_eq!(explorer(&session, sid).0, home);
    assert_eq!(tab_name(&session, sid), home_name);
    let sid = create(
        &mut journal,
        &mut session,
        "tab.create",
        serde_json::json!({"pane_id":pane,"type":"explorer","path":null,"cwd":null}),
        908,
    );
    assert_eq!(explorer(&session, sid).0, home);
    assert_eq!(tab_name(&session, sid), home_name);

    for (level, name, id) in [
        ("surface", "split-surface", 905),
        ("pane", "split-pane", 906),
    ] {
        let dir = folder(name);
        let sid = create(
            &mut journal,
            &mut session,
            "split",
            serde_json::json!({"target_surface":ws_sid,"level":level,"type":"explorer","cwd":dir.to_string_lossy()}),
            id,
        );
        assert_eq!(explorer(&session, sid).0, dir, "split level {level}");
    }

    // 없는 cwd 는 kind 와 관계없이 거절한다.
    let rx = send(
        &mut journal,
        request(
            "tab.create",
            serde_json::json!({"pane_id":pane,"type":"explorer","cwd":base.path().join("missing").to_string_lossy()}),
            None,
            907,
        ),
    );
    let refused = public(&mut journal, &mut session, &rx);
    assert!(refused.error.is_some(), "{refused:?}");
}

#[test]
fn a_finished_intent_records_its_result_in_the_requesting_run() {
    use crate::request_runs::{begin, lookup};
    let result = |response, completion| IntentResult {
        #[cfg(feature = "gui")]
        view: None,
        engine: EngineId::for_test(),
        origin: crate::intent::IntentOrigin::System,
        response,
        completion,
    };
    let opened = begin("file_handler.dispatch", None, serde_json::Value::Null);
    result(
        JsonRpcResponse::success(
            serde_json::Value::Null,
            serde_json::json!({"ok": true, "surface_id": 9}),
        ),
        Some(opened),
    )
    .finish_completion();
    let v = lookup(opened, None).unwrap();
    assert_eq!(v["state"], "succeeded");
    assert_eq!(v["surface_id"], 9);
    assert_eq!(v["result"], "surface 9 is ready");

    let refused = begin("markdown.navigate", None, serde_json::Value::Null);
    result(
        JsonRpcResponse::error(serde_json::Value::Null, -32602, "surface 4 not found"),
        Some(refused),
    )
    .finish_completion();
    let v = lookup(refused, None).unwrap();
    assert_eq!(v["state"], "failed");
    assert_eq!(v["result"], "surface 4 not found");
    assert_eq!(v["code"], -32602);

    // run이 없는 명령은 다른 요청의 run을 건드리지 않는다.
    let untouched = begin("markdown.navigate", None, serde_json::Value::Null);
    result(
        JsonRpcResponse::error(serde_json::Value::Null, -32602, "x"),
        None,
    )
    .finish_completion();
    assert_eq!(lookup(untouched, None).unwrap()["state"], "running");
}
