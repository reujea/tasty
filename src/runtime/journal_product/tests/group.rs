//! Real bounded channels, real SQL commits and request-specific ACK/failure ownership.
#[cfg(unix)]
mod crash;

use super::*;
use tasty_core::{CreationDestination, CreationPlan, OperationId, SurfaceSpec};
use tasty_event_store::{CommandStatus, CommitOutcome, CommitRequest, NewCommand};

struct Fixture {
    worker: JournalWorker,
    baseline: tasty_event_store::CommitStatistics,
    held: u64,
}

fn rename(name: &str) -> Work {
    Work::Resolve {
        changes: vec![StreamCommand {
            stream: "structure:slot-1".into(),
            command: StructuralCommand::RenameCategory {
                id: 1,
                name: name.into(),
            },
        }],
        response: None,
    }
}

fn fixture(home: &std::path::Path) -> Fixture {
    let worker = start(home);
    seed_category(&worker);
    for (ticket, key) in [
        (10, "held"),
        (11, "b1"),
        (12, "b2"),
        (13, "b3"),
        (21, "rejected"),
    ] {
        submit(&worker, ticket, Work::Admit(header(key)));
        assert!(matches!(
            finished(&worker, ticket).unwrap(),
            ResultValue::NeedsResolution
        ));
    }
    submit(&worker, 31, Work::Admit(header("b3")));
    assert!(matches!(
        finished(&worker, 31).unwrap(),
        ResultValue::JoinedAdmission { leader_ticket: 13 }
    ));
    submit(
        &worker,
        13,
        Work::Reserve(vec![
            (IdKind::Workspace, 1),
            (IdKind::Pane, 1),
            (IdKind::Tab, 1),
            (IdKind::Surface, 1),
        ]),
    );
    let ResultValue::Reserved(ids) = finished(&worker, 13).unwrap() else {
        panic!("IDs")
    };
    let id = |kind: IdKind| ids.iter().find(|r| r.kind == kind.label()).unwrap().start as u32;
    submit(
        &worker,
        13,
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
    let ResultValue::InputStored(input) = finished(&worker, 13).unwrap() else {
        panic!("input")
    };
    submit(&worker, 10, rename("held"));
    let Completion::Publish {
        batch,
        commit_statistics: baseline,
        ..
    } = receive(&worker)
    else {
        panic!("B0 publication")
    };
    let held = batch.batch_id;
    // B0 is ACK-blocked. All four logical requests below are ready before collection begins.
    submit(&worker, 11, rename("one"));
    submit(
        &worker,
        21,
        Work::Resolve {
            changes: vec![StreamCommand {
                stream: "structure:slot-1".into(),
                command: StructuralCommand::RenameCategory {
                    id: 999,
                    name: "missing".into(),
                },
            }],
            response: Some(ResponsePlan::Fixed(
                crate::ipc::protocol::JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({}),
                ),
            )),
        },
    );
    submit(&worker, 12, rename("two"));
    submit(
        &worker,
        13,
        Work::Resolve {
            changes: vec![StreamCommand {
                stream: "structure:slot-1".into(),
                command: StructuralCommand::PrepareCreation {
                    operation: OperationId(String::new()),
                    command_id: String::new(),
                    input,
                    plan: CreationPlan {
                        destination: CreationDestination::Workspace {
                            workspace: id(IdKind::Workspace),
                            pane: id(IdKind::Pane),
                            tab: id(IdKind::Tab),
                            name: "durable-b3".into(),
                            category: 1,
                            subtitle: String::new(),
                            description: String::new(),
                        },
                        surface: SurfaceSpec {
                            id: id(IdKind::Surface),
                            kind: "empty".into(),
                            data: None,
                        },
                        tab_name: "empty".into(),
                        explicit_name: None,
                    },
                },
            }],
            response: None,
        },
    );
    Fixture {
        worker,
        baseline,
        held,
    }
}

fn readonly(home: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        identity::journal_database_path(home),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}

fn command(conn: &rusqlite::Connection, key: &str) -> (String, String, Vec<u8>, Option<u64>) {
    conn.query_row("SELECT c.command_id,c.status,c.response,b.batch_id FROM commands c LEFT JOIN batches b ON b.command_id=c.command_id WHERE c.caller_scope='test-agent' AND c.idempotency_key=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap()
}

fn first_publication(fixture: &Fixture, home: &std::path::Path) -> u64 {
    fixture.worker.acknowledge(fixture.held, Ok(())).unwrap();
    finished(&fixture.worker, 10).unwrap();
    let Completion::Publish {
        batch,
        commit_statistics,
        ..
    } = receive(&fixture.worker)
    else {
        panic!("B1")
    };
    assert_eq!(
        commit_statistics.structural_commits - fixture.baseline.structural_commits,
        1,
        "actual successful physical structural COMMIT count"
    );
    assert_eq!(
        commit_statistics.logical_requests - fixture.baseline.logical_requests,
        4
    );
    // One shared COMMIT plus B1's unchanged auxiliary admission-holder release.
    assert_eq!(
        commit_statistics.physical_commits - fixture.baseline.physical_commits,
        2
    );
    let conn = readonly(home);
    assert_eq!(command(&conn, "b1").3, Some(batch.batch_id));
    assert!(command(&conn, "b2").3.unwrap() > batch.batch_id);
    assert!(command(&conn, "b3").3.unwrap() > command(&conn, "b2").3.unwrap());
    let rejected = command(&conn, "rejected");
    assert_eq!(rejected.1, "failed");
    assert!(rejected.3.is_none());
    let first = command(&conn, "b3");
    let pending: (String, u32) = conn
        .query_row(
            "SELECT state,attempt FROM effects WHERE command_id=?1",
            [&first.0],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pending, ("pending".into(), 0));
    batch.batch_id
}

fn acknowledge_first(fixture: &Fixture, batch: u64) -> u64 {
    fixture.worker.acknowledge(batch, Ok(())).unwrap();
    assert!(finished(&fixture.worker, 11).is_ok());
    let ResultValue::Executed(rejected) = finished(&fixture.worker, 21).unwrap() else {
        panic!("saved rejection")
    };
    assert_eq!(rejected.status, CommandStatus::Failed);
    let Completion::Publish { batch, before, .. } = receive(&fixture.worker) else {
        panic!("B2")
    };
    assert_eq!(before["structure:slot-1"].categories[&1].name, "one");
    batch.batch_id
}

fn verify_reopen_and_retries(home: &std::path::Path, expected_attempt: u32) {
    let mut store = identity::open(home, None).unwrap();
    let epoch = store.acquire_writer().unwrap();
    let before = store.read_batches_after(None, 100).unwrap();
    let replay = crate::runtime::journal::full_replay(&store).unwrap();
    assert_eq!(replay.stream("structure:slot-1").categories[&1].name, "two");
    let mut b3 = None;
    for key in ["b1", "rejected", "b2", "b3"] {
        let key = header(key).key.unwrap();
        let tasty_event_store::CommandLookup::Hit(saved) = store
            .lookup_command(&key, b"original method and params")
            .unwrap()
        else {
            panic!("saved key")
        };
        let mut retry = CommitRequest::new(epoch);
        retry.command = Some(NewCommand {
            command_id: format!("must-not-write-{}", key.idempotency_key),
            key: Some(key.clone()),
            request_digest: b"original method and params".to_vec(),
            resolved: vec![],
            status: CommandStatus::Completed,
            response: Some(vec![]),
        });
        assert_eq!(
            store.commit(&retry).unwrap(),
            CommitOutcome::Duplicate(saved.clone())
        );
        if key.idempotency_key == "b3" {
            b3 = Some(saved);
        }
    }
    assert_eq!(store.read_batches_after(None, 100).unwrap(), before);
    let b3 = b3.unwrap();
    let effects = store
        .effects_in_state(if expected_attempt == 0 {
            tasty_event_store::EffectState::Pending
        } else {
            tasty_event_store::EffectState::Running
        })
        .unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0].command_id.as_deref(),
        Some(b3.command_id.as_str())
    );
    assert_eq!(effects[0].attempt, expected_attempt);
    let conn = readonly(home);
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn b1_ack_b2_failure_preserves_b3_durable_identity_pins_followers_and_prevents_claim() {
    let home = tempfile::tempdir().unwrap();
    let fixture = fixture(home.path());
    let first = first_publication(&fixture, home.path());
    let blocked = fixture.worker.metrics_snapshot();
    assert!(blocked.unpublished_age_us.is_some());
    assert!(blocked.work.inflight_items >= 4);
    assert!(blocked.store.unwrap().grouped_logical_requests >= 4);
    let second = acknowledge_first(&fixture, first);
    assert!(
        fixture
            .worker
            .metrics_snapshot()
            .unpublished_age_us
            .is_some()
    );
    fixture
        .worker
        .acknowledge(second, Err("injected B2 live publication failure".into()))
        .unwrap();
    assert!(matches!(receive(&fixture.worker), Completion::Halted(_)));
    for ticket in [12, 13, 31] {
        assert!(
            finished(&fixture.worker, ticket)
                .unwrap_err()
                .contains("halted")
        );
    }
    let conn = readonly(home.path());
    let b3 = command(&conn, "b3");
    let effect: String = conn
        .query_row(
            "SELECT effect_id FROM effects WHERE command_id=?1",
            [&b3.0],
            |r| r.get(0),
        )
        .unwrap();
    let operation = OperationId(effect.strip_suffix("/prepare").unwrap().into());
    submit(
        &fixture.worker,
        99,
        Work::ClaimPreparation {
            stream: "structure:slot-1".into(),
            operation,
        },
    );
    assert!(finished(&fixture.worker, 99).is_err());
    let attempts: u64 = conn
        .query_row("SELECT count(*) FROM effect_attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(attempts, 0);
    assert_ack_failure_metrics(&fixture.worker);
    let admission_pins: u64 = conn
        .query_row(
            "SELECT count(*) FROM payload_pins WHERE holder LIKE 'admission/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(admission_pins, 0);
    let durable_pins: u64 = conn
        .query_row(
            "SELECT count(*) FROM payload_pins WHERE holder LIKE 'event:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(durable_pins > 0);
    assert!(matches!(
        fixture.worker.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    drop(conn);
    drop(fixture);
    verify_reopen_and_retries(home.path(), 0);
    verify_worker_recovery(home.path(), 0);
}

fn verify_worker_recovery(home: &std::path::Path, expected_attempt: u32) {
    let conn = readonly(home);
    let b1 = command(&conn, "b1");
    let b2 = command(&conn, "b2");
    let b3 = command(&conn, "b3");
    let worker = start(home);
    for (ticket, key, saved) in [(101, "b1", b1), (102, "b2", b2)] {
        submit(&worker, ticket, Work::Admit(header(key)));
        let ResultValue::Stored(record) = finished(&worker, ticket).unwrap() else {
            panic!("durable metadata replay")
        };
        assert_eq!(record.command_id, saved.0);
        assert_eq!(record.response, Some(saved.2));
    }
    submit(&worker, 103, Work::Admit(header("b3")));
    match (expected_attempt, finished(&worker, 103).unwrap()) {
        (0, ResultValue::Stored(record)) => {
            assert_eq!(record.command_id, b3.0);
            assert!(record.status.is_terminal());
        }
        (
            1,
            ResultValue::RecoveryRequired {
                command_id,
                replay: true,
                ..
            },
        ) => assert_eq!(command_id, b3.0),
        (_, other) => panic!("reopened effect result {other:?}"),
    }
    let effects: u64 = conn
        .query_row("SELECT count(*) FROM effects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(effects, 1);
    let attempt: u32 = conn
        .query_row(
            "SELECT attempt FROM effects WHERE command_id=?1",
            [&b3.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(attempt, expected_attempt);
    let state: String = conn
        .query_row(
            "SELECT state FROM effects WHERE command_id=?1",
            [&b3.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        state,
        if expected_attempt == 0 {
            "cancelled"
        } else {
            "uncertain"
        }
    );
}

fn assert_ack_failure_metrics(worker: &JournalWorker) {
    let failed = worker.metrics_snapshot();
    assert!(failed.publication_known);
    assert!(failed.unpublished_age_us.is_some());
    assert_eq!(failed.publication_errors, 1);
    assert_eq!(failed.work.pending_items, 0);
    assert!(failed.work.handled_error >= 3);
    assert_eq!(
        failed.store.unwrap().commits.failed_commits,
        0,
        "ACK failure is not failed SQL COMMIT"
    );
}
