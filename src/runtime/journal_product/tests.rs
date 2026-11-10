mod aggregate;
#[cfg(all(unix, feature = "gui", target_os = "linux"))]
mod capture_cost;
mod commit_knowledge;
mod group;
mod observability;
mod pressure;
mod retirement;

use super::stall_budget::StallBudget;
use super::*;
use tasty_core::{StructuralCommand, StructureModels, evolve_streams};

/// PTY 출력·회수처럼 `pid` 자식과 관련된 조건을 기다린다. 기한은
/// [`StallBudget::for_process`]가 정한다(사실상 쉰 시간 10초).
#[cfg(unix)]
fn wait_for(what: &str, pid: Option<u32>, mut done: impl FnMut() -> bool) {
    let mut stall = StallBudget::for_process(pid);
    while !done() {
        stall.nap(what);
    }
}

/// worker의 다음 완료를 기다린다. 기한은 [`StallBudget`]이 정한다. worker가 일하거나 디스크 I/O를
/// 기다리는 시간은 세지 않으므로 부하로 늦어진 완료는 기다리고, 잠든 채 답하지 않는 worker는 정체로
/// 실패한다. worker가 끝나 채널이 끊기면 바로 실패한다.
fn receive(worker: &JournalWorker) -> Completion {
    let completions = worker.completions.as_ref().unwrap();
    let mut stall = StallBudget::for_worker(worker);
    loop {
        match completions.try_recv() {
            Ok(completion) => return completion,
            Err(mpsc::TryRecvError::Disconnected) => {
                panic!("the journal worker closed its completion channel before answering")
            }
            Err(mpsc::TryRecvError::Empty) => stall.nap("journal completion"),
        }
    }
}

fn start(home: &std::path::Path) -> JournalWorker {
    let worker = JournalWorker::spawn(home.to_owned(), Arc::new(|| {})).unwrap();
    match receive(&worker) {
        Completion::Ready {
            cut,
            bootstrap,
            runtime_epoch,
            ..
        } => {
            assert!(runtime_epoch > 0);
            assert_eq!(cut, bootstrap.batch);
            worker.acknowledge(cut.unwrap_or(0), Ok(())).unwrap();
        }
        other => panic!("{other:?}"),
    }
    worker
}

fn submit(worker: &JournalWorker, ticket: u64, work: Work) {
    worker.submit(Request { ticket, work }).unwrap();
}

fn header(key: &str) -> Admission {
    Admission {
        key: Some(CommandKey {
            caller_scope: "test-agent".into(),
            idempotency_key: key.into(),
        }),
        original_digest: b"original method and params".to_vec(),
        actor: "agent".into(),
        origin: "ipc".into(),
        causation_id: None,
    }
}

fn finished(worker: &JournalWorker, ticket: u64) -> Result<ResultValue, String> {
    match receive(worker) {
        Completion::Finished {
            ticket: got,
            result,
        } => {
            assert_eq!(got, ticket);
            result.map_err(|error| error.to_string())
        }
        other => panic!("{other:?}"),
    }
}

fn category(stream: &str, id: u32) -> StreamCommand {
    StreamCommand {
        stream: format!("structure:{stream}"),
        command: StructuralCommand::CreateCategory {
            reserved_id: id,
            name: format!("category-{id}"),
        },
    }
}

#[test]
fn startup_barrier_backpressures_without_blocking_the_application_thread() {
    let home = tempfile::tempdir().unwrap();
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    assert!(matches!(
        receive(&worker),
        Completion::Ready { cut: None, .. }
    ));
    for ticket in 0..QUEUE_CAPACITY as u64 {
        submit(&worker, ticket, Work::ReadEngine("structure:slot-1".into()));
    }
    assert_eq!(
        worker.submit(Request {
            ticket: 999,
            work: Work::CancelAdmission
        }),
        Err(SubmitError::Busy)
    );
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    worker.acknowledge(0, Ok(())).unwrap();
    for ticket in 0..QUEUE_CAPACITY as u64 {
        let ResultValue::Engine(model) = finished(&worker, ticket).unwrap() else {
            panic!("engine")
        };
        assert!(model.workspaces.is_empty());
    }
}

#[test]
fn duplicate_admission_resolves_once_and_multistream_result_waits_for_publication() {
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    submit(&worker, 1, Work::Admit(header("create")));
    assert!(matches!(
        finished(&worker, 1).unwrap(),
        ResultValue::NeedsResolution
    ));
    submit(&worker, 2, Work::Admit(header("create")));
    assert!(matches!(
        finished(&worker, 2).unwrap(),
        ResultValue::JoinedAdmission { leader_ticket: 1 }
    ));
    submit(&worker, 2, Work::Reserve(vec![(IdKind::Category, 2)]));
    assert!(finished(&worker, 2).unwrap_err().contains("admitted"));
    submit(&worker, 1, Work::Reserve(vec![(IdKind::Category, 2)]));
    let ResultValue::Reserved(ranges) = finished(&worker, 1).unwrap() else {
        panic!("reservation")
    };
    assert_eq!((ranges[0].start, ranges[0].end), (1, 3));
    submit(
        &worker,
        1,
        Work::Resolve {
            changes: vec![category("slot-1", 1), category("slot-2", 2)],
            response: None,
        },
    );
    let Completion::Publish { batch, before, .. } = receive(&worker) else {
        panic!("publication before response")
    };
    assert_eq!(batch.streams.len(), 2);
    assert!(
        before
            .values()
            .all(|model| model.applied.revision.is_none())
    );
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    let mut live = StructureModels::default();
    evolve_streams(&mut live, &batch).unwrap();
    worker.acknowledge(batch.batch_id, Ok(())).unwrap();
    let ResultValue::Executed(first) = finished(&worker, 1).unwrap() else {
        panic!("execute")
    };
    let ResultValue::Executed(joined) = finished(&worker, 2).unwrap() else {
        panic!("joined")
    };
    assert_eq!(joined, first);
    assert_eq!(live.streams["structure:slot-1"].category_order, vec![1]);
    assert_eq!(live.streams["structure:slot-2"].category_order, vec![2]);
    submit(&worker, 3, Work::Admit(header("create")));
    let ResultValue::Stored(stored) = finished(&worker, 3).unwrap() else {
        panic!("stored")
    };
    assert_eq!(stored.command_id, first.command_id);
    assert_eq!(stored.batch_ids, vec![batch.batch_id]);
    drop(worker);
    let reopened = start(home.path());
    submit(&reopened, 4, Work::Admit(header("create")));
    assert!(matches!(
        finished(&reopened, 4).unwrap(),
        ResultValue::Stored(_)
    ));
}

#[test]
fn rejected_publication_halts_later_commands_without_returning_success() {
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    submit(&worker, 1, Work::Admit(header("create")));
    finished(&worker, 1).unwrap();
    submit(
        &worker,
        1,
        Work::Reserve(vec![(IdKind::Category, 1), (IdKind::Surface, 1)]),
    );
    assert!(matches!(
        finished(&worker, 1).unwrap(),
        ResultValue::Reserved(_)
    ));
    submit(
        &worker,
        1,
        Work::Resolve {
            changes: vec![category("slot-1", 1)],
            response: None,
        },
    );
    let Completion::Publish { batch, before, .. } = receive(&worker) else {
        panic!("publish")
    };
    assert!(
        before
            .values()
            .all(|model| model.applied.revision.is_none())
    );
    worker
        .acknowledge(batch.batch_id, Err("injected live apply failure".into()))
        .unwrap();
    let Completion::Halted(reason) = receive(&worker) else {
        panic!("projection refusal must notify the owner before completing its request");
    };
    assert!(reason.contains("injected live apply failure"));
    assert!(finished(&worker, 1).unwrap_err().contains("halted"));
    submit(&worker, 2, Work::Admit(header("create")));
    assert!(finished(&worker, 2).unwrap_err().contains("halted"));
}

#[test]
fn shutdown_unblocks_a_worker_waiting_for_bootstrap_ack() {
    let home = tempfile::tempdir().unwrap();
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    assert!(matches!(receive(&worker), Completion::Ready { .. }));
    drop(worker);
    // The previous owner released its writer lock and the explicit binding remains usable.
    drop(start(home.path()));
}

#[test]
fn an_active_binding_never_recreates_a_lost_database() {
    let home = tempfile::tempdir().unwrap();
    drop(start(home.path()));
    std::fs::remove_file(home.path().join("structure/journal.db")).unwrap();
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    let Completion::StartupFailed(error) = receive(&worker) else {
        panic!("must refuse lost database")
    };
    assert!(!error.is_home_in_use());
    assert!(error.to_string().contains("refusing to recreate"));
    assert!(!home.path().join("structure/journal.db").exists());
}

/// 선점은 잠금이 잡혀 있으면 writer 잠금 재시도 구간 동안 다시 시도한다. 다른 테스트의 PTY fork 자식이
/// exec 전까지 잠금 파일 설명을 잠깐 공유해도 그 구간 안에 풀리므로 얻는다.
fn preempt_after_release(database: &std::path::Path) -> tasty_event_store::WriterLock {
    match tasty_event_store::preempt_writer_lock(database).unwrap() {
        tasty_event_store::WriterPreempt::Acquired(lock) => lock,
        tasty_event_store::WriterPreempt::Held => panic!("the released writer lock stayed held"),
    }
}

#[test]
fn a_held_writer_lock_is_reported_as_home_in_use() {
    let home = tempfile::tempdir().unwrap();
    drop(start(home.path()));
    let database = super::identity::journal_database_path(home.path());
    let lock = preempt_after_release(&database);
    // 핸들 없이 여는 경로(헤드리스·테스트)도 같은 종류로 알린다.
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    let Completion::StartupFailed(error) = receive(&worker) else {
        panic!("a held writer lock must refuse startup")
    };
    assert!(error.is_home_in_use(), "{error}");
    drop(lock);
}

#[test]
fn a_held_binding_lock_is_reported_as_home_in_use() {
    let home = tempfile::tempdir().unwrap();
    let structure = home.path().join("structure");
    std::fs::create_dir_all(&structure).unwrap();
    let binding = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(structure.join("journal.binding-lock"))
        .unwrap();
    binding.try_lock().unwrap();
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    let Completion::StartupFailed(error) = receive(&worker) else {
        panic!("a held binding lock must refuse startup")
    };
    assert!(error.is_home_in_use(), "{error}");
    drop(binding);
}

#[test]
fn a_preempted_writer_lock_lets_the_worker_open_and_keeps_others_out() {
    let home = tempfile::tempdir().unwrap();
    let database = super::identity::journal_database_path(home.path());
    let lock = preempt_after_release(&database);
    // 빈 홈(첫 실행)에서도 선점 잠금으로 저널을 초기화하고 writer가 된다.
    let worker =
        JournalWorker::spawn_with(home.path().to_owned(), Arc::new(|| {}), Some(lock)).unwrap();
    assert!(matches!(receive(&worker), Completion::Ready { .. }));
    assert!(matches!(
        tasty_event_store::preempt_writer_lock(&database).unwrap(),
        tasty_event_store::WriterPreempt::Held
    ));
    drop(worker);
    drop(preempt_after_release(&database));
}

#[test]
fn interrupted_first_initialization_keeps_its_explicit_identity() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join("structure")).unwrap();
    let id = "journal-0123456789abcdef0123456789abcdef";
    std::fs::write(
        home.path().join("structure/journal.json"),
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "journal_id": id, "phase": "Preparing"
        }))
        .unwrap(),
    )
    .unwrap();
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    let Completion::Ready { journal_id, .. } = receive(&worker) else {
        panic!("ready")
    };
    assert_eq!(journal_id, id);
    worker.acknowledge(0, Ok(())).unwrap();
    let binding: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.path().join("structure/journal.json")).unwrap())
            .unwrap();
    assert_eq!(binding["phase"], "Ready");
}

fn publish(worker: &JournalWorker) -> tasty_core::StreamBatch {
    let Completion::Publish { batch, .. } = receive(worker) else {
        panic!("committed batch")
    };
    worker.acknowledge(batch.batch_id, Ok(())).unwrap();
    batch
}

fn prepare_workspace(
    worker: &JournalWorker,
    ticket: u64,
    name: &str,
    kind: &str,
) -> tasty_core::OperationId {
    use tasty_core::{CreationDestination, CreationPlan, DataRef, OperationId, SurfaceSpec};
    submit(worker, ticket, Work::Admit(header(name)));
    assert!(matches!(
        finished(worker, ticket).unwrap(),
        ResultValue::NeedsResolution
    ));
    submit(
        worker,
        ticket,
        Work::Reserve(vec![
            (IdKind::Workspace, 1),
            (IdKind::Pane, 1),
            (IdKind::Tab, 1),
            (IdKind::Surface, 1),
        ]),
    );
    let ResultValue::Reserved(ranges) = finished(worker, ticket).unwrap() else {
        panic!("IDs")
    };
    let id = |kind: IdKind| {
        ranges
            .iter()
            .find(|range| range.kind == kind.label())
            .unwrap()
            .start as u32
    };
    submit(
        worker,
        ticket,
        Work::PutPreparation(PreparationInput {
            adopt: None,
            child: None,
            kind: kind.into(),
            cwd: None,
            params: serde_json::json!({}),
            shell: (kind == "terminal").then(|| ShellRecipe {
                executable: "/bin/sh".into(),
                arguments: vec![
                    "-c".into(),
                    "printf '\\033]2;prepared-title\\007PREPARED-JOURNAL'; exec sleep 60".into(),
                ],
                environment: Vec::new(),
                cols: 80,
                rows: 24,
                scrollback_lines: 100,
                disk_scrollback: false,
                disk_scrollback_lines: None,
                startup_command: String::new(),
                restore_command: None,
            }),
            restore: None,
            discard_capture: false,
        }),
    );
    let ResultValue::InputStored(DataRef(input)) = finished(worker, ticket).unwrap() else {
        panic!("input")
    };
    submit(
        worker,
        ticket,
        Work::Resolve {
            changes: vec![StreamCommand {
                stream: "structure:slot-1".into(),
                command: StructuralCommand::PrepareCreation {
                    operation: OperationId(String::new()),
                    command_id: String::new(),
                    input: DataRef(input),
                    plan: CreationPlan {
                        destination: CreationDestination::Workspace {
                            workspace: id(IdKind::Workspace),
                            pane: id(IdKind::Pane),
                            tab: id(IdKind::Tab),
                            name: name.into(),
                            category: 1,
                            subtitle: String::new(),
                            description: String::new(),
                        },
                        surface: SurfaceSpec {
                            id: id(IdKind::Surface),
                            kind: kind.into(),
                            data: None,
                        },
                        tab_name: "created".into(),
                        explicit_name: None,
                    },
                },
            }],
            response: None,
        },
    );
    let batch = publish(worker);
    let operation = batch.streams["structure:slot-1"]
        .iter()
        .find_map(|recorded| match &recorded.event {
            tasty_core::DomainEvent::OperationPrepared { operation } => Some(operation.id.clone()),
            _ => None,
        })
        .unwrap();
    let ResultValue::Executed(executed) = finished(worker, ticket).unwrap() else {
        panic!("prepare commit")
    };
    assert_eq!(
        executed.status,
        tasty_event_store::CommandStatus::InProgress
    );
    let progress: Vec<tasty_core::StructuralResult> =
        serde_json::from_slice(executed.response.as_ref().unwrap()).unwrap();
    assert!(matches!(
        &progress[..],
        [tasty_core::StructuralResult::Pending { .. }]
    ));
    operation
}

fn seed_category(worker: &JournalWorker) {
    submit(worker, 100, Work::Admit(header("category")));
    finished(worker, 100).unwrap();
    submit(
        worker,
        100,
        Work::Reserve(vec![(IdKind::Category, 1), (IdKind::Surface, 1)]),
    );
    finished(worker, 100).unwrap();
    submit(
        worker,
        100,
        Work::Resolve {
            changes: vec![category("slot-1", 1)],
            response: None,
        },
    );
    publish(worker);
    finished(worker, 100).unwrap();
}

/// A live engine whose only surface cannot collide with the journal's reservations.
///
/// A fresh journal hands out surface IDs from 1 (`seed_category` takes 1, `prepare_workspace`
/// takes 2). `test_state()` numbers its surface from a process-wide counter, so its ID depends
/// on how many fixtures ran before in this process; when it lands on the reserved ID, preparation
/// rejects the claim as already owned. The engine therefore owns a fixed, unrelated surface.
fn unrelated_owner_state() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
        tasty_core::DomainEvent::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        tasty_core::DomainEvent::WorkspaceCreated {
            id: 100,
            name: "unrelated".into(),
            category: 0,
            index: 0,
            pane: 100,
        },
        tasty_core::DomainEvent::TabCreated {
            id: 100,
            pane: 100,
            index: 0,
            name: "unrelated".into(),
            surface: tasty_core::SurfaceSpec {
                id: 100,
                kind: "empty".into(),
                data: None,
            },
        },
    ]))
}

fn claim(
    worker: &JournalWorker,
    ticket: u64,
    operation: tasty_core::OperationId,
) -> ClaimedPreparation {
    submit(
        worker,
        ticket,
        Work::ClaimPreparation {
            stream: "structure:slot-1".into(),
            operation,
        },
    );
    let ResultValue::Claimed(claimed) = finished(worker, ticket).unwrap() else {
        panic!("claim")
    };
    claimed
}

#[test]
fn mixed_operation_leases_are_rejected_before_either_effect_or_model_changes() {
    use tasty_core::{OperationId, PreparationResult};
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    seed_category(&worker);
    let first = prepare_workspace(&worker, 1, "first", "empty");
    let second = prepare_workspace(&worker, 2, "second", "empty");
    let first = claim(&worker, 11, first);
    let second = claim(&worker, 12, second);
    assert_eq!(first.lease.attempt, second.lease.attempt);
    assert_eq!(
        first.lease.resource_generation,
        second.lease.resource_generation
    );
    let mut mixed = first.lease.clone();
    mixed.operation = second.lease.operation.clone();
    submit(
        &worker,
        20,
        Work::Prepared {
            lease: mixed,
            result: PreparationResult::Ready { data: None },
        },
    );
    assert!(
        finished(&worker, 20)
            .unwrap_err()
            .contains("does not belong")
    );
    submit(&worker, 21, Work::ReadEngine("structure:slot-1".into()));
    let ResultValue::Engine(model) = finished(&worker, 21).unwrap() else {
        panic!("model")
    };
    assert!(model.workspaces.is_empty());
    for operation in [&first.lease.operation, &second.lease.operation] {
        assert_eq!(model.operations[operation].outcome, None);
    }
    // Each original receipt remains usable; neither lease was consumed by the mixed attempt.
    for (ticket, claimed) in [(30, first), (31, second)] {
        submit(
            &worker,
            ticket,
            Work::Prepared {
                lease: claimed.lease,
                result: PreparationResult::Failed {
                    reason: "fixture declined preparation".into(),
                },
            },
        );
        publish(&worker);
        finished(&worker, ticket).unwrap();
    }
    submit(
        &worker,
        32,
        Work::CleanupFinished {
            view: crate::runtime::journal_product::CompletionView::default(),
            lease: EffectLease {
                effect_id: "missing".into(),
                operation: OperationId("missing".into()),
                stream: "structure:slot-1".into(),
                runtime_epoch: 1,
                resource_generation: 1,
                attempt: 1,
            },
        },
    );
    assert!(finished(&worker, 32).is_err());
}

/// 저장 자료의 cwd가 prepare를 지나 kind의 restore까지 닿는다. 파일이 없는 html HTTP·inline은
/// 이 값으로만 복원 뒤 cwd를 되찾는다.
#[test]
fn a_saved_generic_cwd_reaches_the_restored_surface_through_prepare() {
    use crate::core::layout_persistence::import::surface_data::SurfaceData;
    use crate::runtime::effect_runner::{self, ExecutionBinding};
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    seed_category(&worker);
    let (_view, mut session) = unrelated_owner_state();
    let (sender, _receiver) = std::sync::mpsc::channel();
    let declaration = serde_json::from_value(serde_json::json!({
        "kind": "test-saved-cwd", "display_name_i18n_key": "surface.kind.html",
        "rendering": "webview",
    }))
    .unwrap();
    crate::plugin_bridge::remote_kind::register_remote_kind(
        &session.runtime.surface_registry,
        "com.test.saved-cwd",
        &declaration,
        sender,
    );
    let operation = prepare_workspace(&worker, 1, "saved", "test-saved-cwd");
    let mut claimed = claim(&worker, 2, operation);
    let held = std::path::PathBuf::from(if cfg!(windows) { r"C:\held" } else { "/held" });
    claimed.capture = Some(
        SurfaceData::Generic {
            data: serde_json::json!({ "url": "http://127.0.0.1:9/" }),
            cwd: Some(held.to_string_lossy().into_owned()),
        }
        .encode()
        .unwrap(),
    );
    let binding = ExecutionBinding {
        stream: claimed.lease.stream.clone(),
        runtime_epoch: claimed.lease.runtime_epoch,
        engine_incarnation: claimed.engine_incarnation,
    };
    let spawner = effect_runner::TerminalSpawner::spawn(Arc::new(|| {})).unwrap();
    let mut engine = session.borrow_mut();
    let prepared = effect_runner::prepare_and_wait(&mut engine, &binding, claimed, &spawner)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(prepared.leaf.surface.source_cwd(), Some(held));
}

#[cfg(unix)]
#[test]
fn a_durable_claim_precedes_real_pty_preparation_and_the_candidate_stays_private() {
    use crate::runtime::effect_runner::{self, ExecutionBinding};
    use tasty_core::PreparationResult;
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    seed_category(&worker);
    let operation = prepare_workspace(&worker, 1, "terminal", "terminal");
    let claimed = claim(&worker, 2, operation);
    let binding = ExecutionBinding {
        stream: claimed.lease.stream.clone(),
        runtime_epoch: claimed.lease.runtime_epoch,
        engine_incarnation: claimed.engine_incarnation,
    };
    let (_view, mut session) = unrelated_owner_state();
    let mut engine = session.borrow_mut();
    let old_ids = engine.live_surface_ids();
    let reserved_surface = claimed.plan.surface.id;
    let spawner = effect_runner::TerminalSpawner::spawn(Arc::new(|| {})).unwrap();
    let prepared = effect_runner::prepare_and_wait(&mut engine, &binding, claimed, &spawner)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        engine.live_surface_ids(),
        old_ids,
        "preparing does not insert a candidate into the live tree"
    );
    assert!(prepared.publication.is_none());
    assert_eq!(prepared.leaf.surface.surface_id(), Some(reserved_surface));
    let (terminal, pty) = prepared.connection.as_ref().unwrap();
    assert_eq!(terminal.resource_generation(), pty.generation());
    let child = pty.process_id();
    assert!(child.is_some());
    wait_for("prepared output", child, || {
        terminal
            .with_content(|reader| reader.screen_text(false))
            .contains("PREPARED-JOURNAL")
    });
    let lease = prepared.lease.clone();
    let (_, candidate) = prepared.connection.unwrap();
    let retired = candidate.retire();
    wait_for("candidate reap", child, || {
        let phase = retired.observation().phase;
        assert_ne!(phase, tasty_terminal::PtyPhase::WaitFailed);
        phase == tasty_terminal::PtyPhase::Reaped
    });
    submit(
        &worker,
        3,
        Work::Prepared {
            lease,
            result: PreparationResult::Failed {
                reason: "candidate deliberately discarded".into(),
            },
        },
    );
    publish(&worker);
    finished(&worker, 3).unwrap();
    assert_eq!(engine.live_surface_ids(), old_ids);
}

#[cfg(unix)]
#[test]
fn committed_installation_preserves_initial_observations_and_defers_command_completion() {
    use crate::runtime::effect_runner;
    use tasty_core::projection;
    use tasty_core::{DomainBatch, PreparationResult};
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    seed_category(&worker);
    let operation = prepare_workspace(&worker, 1, "install", "terminal");
    let claimed = claim(&worker, 2, operation);
    let sid = claimed.plan.surface.id;
    let binding = effect_runner::ExecutionBinding {
        stream: claimed.lease.stream.clone(),
        runtime_epoch: claimed.lease.runtime_epoch,
        engine_incarnation: claimed.engine_incarnation,
    };
    let (_view, mut session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
            tasty_core::DomainEvent::CategoryCreated {
                id: 1,
                name: "category-1".into(),
                index: 0,
            },
        ]));
    let mut engine = session.borrow_mut();
    // An unrelated live resource catches accidental SID replacement without another Core tree.
    engine.runtime.terminals.insert(
        u32::MAX,
        tasty_terminal::Terminal::new_detached(80, 24),
        None,
    );
    let mut prepared = effect_runner::prepare_and_wait(
        &mut engine,
        &binding,
        claimed,
        &effect_runner::TerminalSpawner::spawn(Arc::new(|| {})).unwrap(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let generation = prepared
        .connection
        .as_ref()
        .unwrap()
        .0
        .resource_generation();
    let child = prepared.connection.as_ref().unwrap().1.process_id();
    wait_for("prepared output", child, || {
        prepared
            .connection
            .as_ref()
            .unwrap()
            .0
            .with_content(|view| view.screen_text(false))
            .contains("PREPARED-JOURNAL")
    });
    let lease = prepared.lease.clone();
    submit(
        &worker,
        3,
        Work::Prepared {
            lease,
            result: PreparationResult::Ready { data: None },
        },
    );
    let Completion::Publish { batch, before, .. } = receive(&worker) else {
        panic!("publication")
    };
    let mut installation = prepared
        .begin_installation(&engine.runtime.surface_registry)
        .unwrap();
    let mut retired = Vec::new();
    projection::apply(
        engine.core,
        &before[&binding.stream],
        &DomainBatch {
            batch_id: batch.batch_id,
            events: batch.streams[&binding.stream].clone(),
        },
        &mut retired,
    )
    .unwrap();
    let installed: effect_runner::Installed =
        installation.install(&mut engine, None, None).unwrap();
    assert_eq!(engine.runtime.terminals.generation(sid), Some(generation));
    worker.acknowledge(batch.batch_id, Ok(())).unwrap();
    finished(&worker, 3).unwrap();
    submit(&worker, 4, Work::Admit(header("install")));
    let ResultValue::Stored(record) = finished(&worker, 4).unwrap() else {
        panic!("stored")
    };
    assert_eq!(record.status, tasty_event_store::CommandStatus::InProgress);
    // 새 SID 에 설치했으므로 회수할 이전 PTY 가 없다. 진행을 맡은 프로세스가 없어 기다린 시간을 모두 센다.
    wait_for("cleanup", None, || installed.cleanup_complete().unwrap());
    submit(
        &worker,
        5,
        Work::CleanupFinished {
            view: crate::runtime::journal_product::CompletionView::default(),
            lease: installed.lease.clone(),
        },
    );
    let Completion::Publish { batch, before, .. } = receive(&worker) else {
        panic!("completion publication")
    };
    engine
        .runtime
        .surfaces
        .insert(sid, prepared.into_leaf(&installed).unwrap().surface);
    projection::apply(
        engine.core,
        &before[&binding.stream],
        &DomainBatch {
            batch_id: batch.batch_id,
            events: batch.streams[&binding.stream].clone(),
        },
        &mut retired,
    )
    .unwrap();
    worker.acknowledge(batch.batch_id, Ok(())).unwrap();
    finished(&worker, 5).unwrap();
    submit(&worker, 6, Work::Admit(header("install")));
    let ResultValue::Stored(record) = finished(&worker, 6).unwrap() else {
        panic!("stored")
    };
    assert_eq!(record.status, tasty_event_store::CommandStatus::Completed);
    assert!(record.response.is_some());
    assert!(
        engine
            .runtime
            .terminals
            .get(sid)
            .unwrap()
            .with_content(|view| view.screen_text(false))
            .contains("PREPARED-JOURNAL")
    );
    let events = engine.collect_events();
    assert!(events.iter().any(|event| event.surface_id == sid && event.generation == generation && matches!(&event.kind, tasty_terminal::TerminalEventKind::TitleChanged(title) if title == "prepared-title")));
    complete_conversion_and_reap(&worker, &mut engine, &binding, sid);
}

#[test]
fn kind_withdrawal_after_claim_prevents_factory_execution() {
    use crate::runtime::effect_runner::{self, ExecutionBinding};
    let home = tempfile::tempdir().unwrap();
    let worker = start(home.path());
    seed_category(&worker);
    let operation = prepare_workspace(&worker, 1, "withdrawn", "late-kind");
    let claimed = claim(&worker, 2, operation);
    let binding = ExecutionBinding {
        stream: claimed.lease.stream.clone(),
        runtime_epoch: claimed.lease.runtime_epoch,
        engine_incarnation: claimed.engine_incarnation,
    };
    let (_view, mut session) = unrelated_owner_state();
    let mut engine = session.borrow_mut();
    let (sender, receiver) = std::sync::mpsc::channel();
    let declaration = serde_json::from_value(serde_json::json!({"kind":"late-kind", "display_name_i18n_key":"surface.kind.markdown", "rendering":"remote"})).unwrap();
    crate::plugin_bridge::remote_kind::register_remote_kind(
        &engine.runtime.surface_registry,
        "com.test.late",
        &declaration,
        sender,
    );
    engine
        .runtime
        .surface_registry
        .withdraw_plugin("com.test.late");
    assert!(engine.runtime.surface_registry.get("late-kind").is_some());
    let error = match effect_runner::prepare(&mut engine, &binding, claimed) {
        Ok(_) => panic!("withdrawn kind prepared"),
        Err(error) => error,
    };
    assert!(!error.uncertain, "withdrawal precedes factory execution");
    assert!(error.reason.contains("late-kind"));
    assert!(receiver.try_recv().is_err());
}

#[cfg(unix)]
fn complete_conversion_and_reap(
    worker: &JournalWorker,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    binding: &crate::runtime::effect_runner::ExecutionBinding,
    sid: u32,
) {
    use crate::runtime::effect_runner;
    use tasty_core::projection;
    use tasty_core::{
        CreationDestination, CreationPlan, DomainBatch, DomainEvent, OperationId,
        PreparationResult, SurfaceSpec,
    };
    let original = engine.runtime.terminals.generation(sid).unwrap();
    let original_pid = engine
        .runtime
        .terminals
        .pty(sid)
        .unwrap()
        .process_id()
        .unwrap();
    submit(worker, 10, Work::Admit(header("convert")));
    finished(worker, 10).unwrap();
    submit(
        worker,
        10,
        Work::PutPreparation(PreparationInput {
            adopt: None,
            child: None,
            kind: "empty".into(),
            cwd: None,
            params: serde_json::json!({}),
            shell: None,
            restore: None,
            discard_capture: false,
        }),
    );
    let ResultValue::InputStored(input) = finished(worker, 10).unwrap() else {
        panic!("input")
    };
    submit(
        worker,
        10,
        Work::Resolve {
            changes: vec![StreamCommand {
                stream: binding.stream.clone(),
                command: StructuralCommand::PrepareCreation {
                    operation: OperationId(String::new()),
                    command_id: String::new(),
                    input,
                    plan: CreationPlan {
                        destination: CreationDestination::Convert {
                            surface: sid,
                            previous_activation: Some(1),
                            explicit_name: Some(None),
                        },
                        surface: SurfaceSpec {
                            id: sid,
                            kind: "empty".into(),
                            data: None,
                        },
                        tab_name: String::new(),
                        explicit_name: None,
                    },
                },
            }],
            response: None,
        },
    );
    let batch = publish(worker);
    let operation = batch.streams[&binding.stream]
        .iter()
        .find_map(|event| match &event.event {
            DomainEvent::OperationPrepared { operation } => Some(operation.id.clone()),
            _ => None,
        })
        .unwrap();
    finished(worker, 10).unwrap();
    let claimed = claim(worker, 11, operation);
    let mut candidate = effect_runner::prepare_and_wait(
        engine,
        binding,
        claimed,
        &effect_runner::TerminalSpawner::spawn(Arc::new(|| {})).unwrap(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(engine.runtime.terminals.generation(sid), Some(original));
    assert_eq!(
        engine.runtime.terminals.pty(sid).unwrap().process_id(),
        Some(original_pid)
    );
    let lease = candidate.lease.clone();
    submit(
        worker,
        12,
        Work::Prepared {
            lease,
            result: PreparationResult::Ready { data: None },
        },
    );
    let Completion::Publish { batch, before, .. } = receive(worker) else {
        panic!("publication")
    };
    let mut installation = candidate
        .begin_installation(&engine.runtime.surface_registry)
        .unwrap();
    let mut retired = Vec::new();
    projection::apply(
        engine.core,
        &before[&binding.stream],
        &DomainBatch {
            batch_id: batch.batch_id,
            events: batch.streams[&binding.stream].clone(),
        },
        &mut retired,
    )
    .unwrap();
    assert!(
        retired.is_empty(),
        "authorization does not remove the old kind"
    );
    assert_eq!(engine.find_surface_by_id(sid).unwrap().kind(), "terminal");
    let old = effect_runner::RetiringKind::capture(engine.find_surface_by_id(sid).unwrap());
    let installed = installation.install(engine, None, Some(old)).unwrap();
    assert!(engine.runtime.terminals.get(sid).is_none());
    worker.acknowledge(batch.batch_id, Ok(())).unwrap();
    finished(worker, 12).unwrap();
    submit(worker, 13, Work::Admit(header("convert")));
    let ResultValue::Stored(record) = finished(worker, 13).unwrap() else {
        panic!("stored")
    };
    assert_eq!(record.status, tasty_event_store::CommandStatus::InProgress);
    wait_for("exact original child reap", Some(original_pid), || {
        installed.cleanup_complete().unwrap()
    });
    #[cfg(target_os = "linux")]
    assert!(!std::path::Path::new(&format!("/proc/{original_pid}")).exists());
    submit(
        worker,
        14,
        Work::CleanupFinished {
            view: crate::runtime::journal_product::CompletionView::default(),
            lease: installed.lease.clone(),
        },
    );
    let Completion::Publish { batch, before, .. } = receive(worker) else {
        panic!("completion")
    };
    engine
        .runtime
        .surfaces
        .insert(sid, candidate.into_leaf(&installed).unwrap().surface);
    projection::apply(
        engine.core,
        &before[&binding.stream],
        &DomainBatch {
            batch_id: batch.batch_id,
            events: batch.streams[&binding.stream].clone(),
        },
        &mut retired,
    )
    .unwrap();
    worker.acknowledge(batch.batch_id, Ok(())).unwrap();
    finished(worker, 14).unwrap();
    submit(worker, 15, Work::Admit(header("convert")));
    let ResultValue::Stored(record) = finished(worker, 15).unwrap() else {
        panic!("stored")
    };
    assert_eq!(record.status, tasty_event_store::CommandStatus::Completed);
    assert_eq!(engine.find_surface_by_id(sid).unwrap().kind(), "empty");
}

#[test]
fn kind_withdrawal_or_replacement_after_prepare_rejects_installation_before_publication() {
    use crate::runtime::effect_runner::{self, ExecutionBinding};
    use tasty_core::PreparationResult;
    for reload in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let worker = start(home.path());
        seed_category(&worker);
        let operation = prepare_workspace(&worker, 1, "late-install", "late-kind");
        let claimed = claim(&worker, 2, operation);
        let binding = ExecutionBinding {
            stream: claimed.lease.stream.clone(),
            runtime_epoch: claimed.lease.runtime_epoch,
            engine_incarnation: claimed.engine_incarnation,
        };
        let (_view, mut session) = unrelated_owner_state();
        let mut engine = session.borrow_mut();
        let original_ids = engine.live_surface_ids();
        let (sender, receiver) = std::sync::mpsc::channel();
        let declaration = serde_json::from_value(serde_json::json!({"kind":"late-kind","display_name_i18n_key":"surface.kind.markdown","rendering":"remote"})).unwrap();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.runtime.surface_registry,
            "com.test.late",
            &declaration,
            sender.clone(),
        );
        let mut candidate = effect_runner::prepare_and_wait(
            &mut engine,
            &binding,
            claimed,
            &effect_runner::TerminalSpawner::spawn(Arc::new(|| {})).unwrap(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let lease = candidate.lease.clone();
        submit(
            &worker,
            3,
            Work::Prepared {
                lease: lease.clone(),
                result: PreparationResult::Ready { data: None },
            },
        );
        publish(&worker);
        finished(&worker, 3).unwrap();
        engine
            .runtime
            .surface_registry
            .withdraw_plugin("com.test.late");
        if reload {
            crate::plugin_bridge::remote_kind::register_remote_kind(
                &engine.runtime.surface_registry,
                "com.test.late",
                &declaration,
                sender,
            );
        }
        let error = match candidate.begin_installation(&engine.runtime.surface_registry) {
            Ok(_) => panic!("stale kind installed"),
            Err(error) => error,
        };
        assert!(
            receiver.try_recv().is_err(),
            "no stale Created was published"
        );
        assert_eq!(engine.live_surface_ids(), original_ids);
        assert!(
            matches!(candidate.discard(&mut engine), Ok(None)),
            "remote proxy had no physical PTY to retire"
        );
        submit(
            &worker,
            4,
            Work::InstallationRejected {
                lease,
                reason: error.to_string(),
            },
        );
        publish(&worker);
        finished(&worker, 4).unwrap();
        submit(&worker, 5, Work::Admit(header("late-install")));
        let ResultValue::Stored(record) = finished(&worker, 5).unwrap() else {
            panic!("record")
        };
        assert_eq!(record.status, tasty_event_store::CommandStatus::Failed);
        submit(&worker, 6, Work::ReadEngine(binding.stream));
        let ResultValue::Engine(model) = finished(&worker, 6).unwrap() else {
            panic!("model")
        };
        assert!(
            model.workspaces.is_empty(),
            "authorization never published a workspace"
        );
    }
}
