//! executor: 가짜 decider와 실제 journal로 확정 실패 시 상태·응답 없음, 재오픈 후 같은 키는
//! decide하지 않음, 같은 키 동시 실행은 decide 1회이고 다른 요청은 충돌함을 확인한다.

mod group;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tasty_core::{Decider, Decision, DecisionContext, DomainEvent, JournalModel, SurfaceSpec};
use tasty_event_store::{
    CommandKey, CommandLookup, EffectState, EventStore, NewEffect, OpaquePayload, Revision,
    StoreError, StoredBatch, StreamId,
};
use tasty_model::WorkspaceId;

use super::common::{JOURNAL, db_path};
use crate::runtime::command_executor::{
    ExecError, Executed, Executor, JournalDecider, MAX_DECIDE_ATTEMPTS, Request, Source,
};
use crate::runtime::journal;

#[derive(Debug, Clone, PartialEq)]
enum Cmd {
    CreateWorkspace {
        name: String,
    },
    RenameWorkspace {
        id: WorkspaceId,
        name: String,
    },
    /// workspace를 만들면서 effect 의무를 함께 남긴다.
    CreateWithEffect {
        name: String,
        effect_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Reject {
    NoSuchWorkspace,
    IdInUse,
}

/// decide 진입을 알리고 풀어 줄 때까지 기다리게 하는 문.
#[derive(Default)]
struct Gate {
    entered: AtomicBool,
    release: AtomicBool,
}

impl Gate {
    fn pass(&self) {
        self.entered.store(true, Ordering::SeqCst);
        wait_until(|| self.release.load(Ordering::SeqCst));
    }
}

fn wait_until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(1), |_| done()),
        "timed out"
    );
}

#[derive(Default, Clone)]
struct Fake {
    decides: Arc<AtomicUsize>,
    gate: Option<Arc<Gate>>,
    /// 이 횟수만큼 revision을 틀리게 알려 저장소의 revision 충돌을 일으킨다.
    stale_revisions: Arc<AtomicUsize>,
    fail_apply: Arc<AtomicBool>,
    fail_load: Arc<AtomicBool>,
}

impl Fake {
    fn decides(&self) -> usize {
        self.decides.load(Ordering::SeqCst)
    }
}

impl Decider for Fake {
    type State = JournalModel;
    type Command = Cmd;
    type Event = DomainEvent;
    type Effect = NewEffect;
    type Rejection = Reject;

    fn request_digest(&self, command: &Cmd) -> Vec<u8> {
        format!("{command:?}").into_bytes()
    }

    fn decide(
        &self,
        state: &JournalModel,
        command: &Cmd,
        _ctx: &mut DecisionContext<'_>,
    ) -> Result<Decision<DomainEvent, NewEffect>, Reject> {
        self.decides.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.pass();
        }
        match command {
            Cmd::CreateWorkspace { name } => create(state, name),
            Cmd::CreateWithEffect { name, effect_id } => {
                let mut decision = create(state, name)?;
                decision.effects.push(NewEffect {
                    claim_kind: tasty_event_store::ClaimKind::Activation,
                    effect_id: effect_id.clone(),
                    operation_id: format!("spawn:{name}"),
                    resource_generation: 0,
                    payload: OpaquePayload {
                        type_tag: "test.effect".to_owned(),
                        schema_version: 1,
                        bytes: Vec::new(),
                    },
                    initial: EffectState::Pending,
                });
                Ok(decision)
            }
            Cmd::RenameWorkspace { id, name } => {
                if !state.workspaces.contains_key(id) {
                    return Err(Reject::NoSuchWorkspace);
                }
                Ok(Decision {
                    events: vec![DomainEvent::WorkspaceRenamed {
                        id: *id,
                        name: name.clone(),
                    }],
                    effects: Vec::new(),
                    resolved: id.to_be_bytes().to_vec(),
                    response: b"renamed".to_vec(),
                })
            }
        }
    }
}

impl JournalDecider for Fake {
    fn stream_revision(&self, state: &JournalModel, _stream: &StreamId) -> Option<Revision> {
        let lie = self
            .stale_revisions
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        let actual = state.applied.revision;
        if lie {
            Some(actual.unwrap_or(0) + 100)
        } else {
            actual
        }
    }

    fn event_stream(&self, _event: &DomainEvent) -> StreamId {
        super::common::stream()
    }

    fn record(
        &self,
        _state: &JournalModel,
        _command: &Cmd,
        decision: &Decision<DomainEvent, NewEffect>,
    ) -> Result<crate::runtime::command_executor::CommandRecordPlan, String> {
        let pending = !decision.effects.is_empty();
        Ok(crate::runtime::command_executor::CommandRecordPlan {
            status: if pending {
                tasty_event_store::CommandStatus::InProgress
            } else {
                tasty_event_store::CommandStatus::Completed
            },
            response: if pending {
                None
            } else {
                Some(decision.response.clone())
            },
            command_updates: Vec::new(),
            effect_transitions: Vec::new(),
        })
    }

    fn encode(&self, event: &DomainEvent) -> Result<OpaquePayload, String> {
        journal::to_payload(event).map_err(|e| e.to_string())
    }

    fn apply(&self, state: &mut JournalModel, batch: &StoredBatch) -> Result<(), String> {
        if self.fail_apply.swap(false, Ordering::SeqCst) {
            return Err("injected post-commit apply failure".into());
        }
        journal::apply(state, &super::common::stream(), batch).map_err(|e| e.to_string())
    }

    fn load(&self, store: &EventStore) -> Result<JournalModel, String> {
        if self.fail_load.load(Ordering::SeqCst) {
            return Err("injected recovery failure".into());
        }
        journal::load(store, &super::common::stream()).map_err(|e| e.to_string())
    }
}

fn create(state: &JournalModel, name: &str) -> Result<Decision<DomainEvent, NewEffect>, Reject> {
    let next = |keys: Vec<u32>| keys.into_iter().max().unwrap_or(0) + 1;
    let ws = next(state.workspaces.keys().copied().collect());
    let pane = next(state.panes.keys().copied().collect());
    let tab = next(state.tabs.keys().copied().collect());
    let surface = next(state.surfaces.keys().copied().collect());
    if state.workspaces.contains_key(&ws) || state.panes.contains_key(&pane) {
        return Err(Reject::IdInUse);
    }
    let mut events = Vec::new();
    if !state.categories.contains_key(&0) {
        events.push(DomainEvent::CategoryCreated {
            id: 0,
            name: "normal".to_owned(),
            index: 0,
        });
    }
    events.push(DomainEvent::WorkspaceCreated {
        id: ws,
        name: name.to_owned(),
        category: 0,
        index: state.workspace_order.len(),
        pane,
    });
    events.push(DomainEvent::TabCreated {
        id: tab,
        pane,
        index: 0,
        name: "shell".to_owned(),
        surface: SurfaceSpec {
            id: surface,
            kind: "terminal".to_owned(),
            data: None,
        },
    });
    Ok(Decision {
        events,
        effects: Vec::new(),
        resolved: Vec::new(),
        response: format!("workspace:{ws}").into_bytes(),
    })
}

fn key(k: &str) -> CommandKey {
    CommandKey {
        caller_scope: "agent:test".to_owned(),
        idempotency_key: k.to_owned(),
    }
}

fn request(k: Option<&str>, command: Cmd) -> Request<Cmd> {
    Request {
        key: k.map(key),
        actor: "agent".to_owned(),
        origin: "test".to_owned(),
        causation_id: None,
        command,
    }
}

fn create_cmd(name: &str) -> Cmd {
    Cmd::CreateWorkspace {
        name: name.to_owned(),
    }
}

fn open(path: &std::path::Path, fake: &Fake) -> Executor<Fake> {
    let store = EventStore::open(path, JOURNAL).expect("open journal");
    Executor::open(fake.clone(), store).expect("executor")
}

fn state(executor: &Executor<Fake>) -> JournalModel {
    executor.with_state(Clone::clone).expect("state")
}

fn committed(result: &Executed) -> bool {
    matches!(result.source, Source::Committed { batch: Some(_) })
}

#[test]
fn committed_batch_is_applied_and_matches_the_journal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = db_path(&dir);
    let fake = Fake::default();
    let executor = open(&path, &fake);
    let first = executor
        .execute(&request(Some("a"), create_cmd("main")))
        .expect("create");
    assert!(committed(&first));
    assert_eq!(first.response.as_deref(), Some(b"workspace:1".as_slice()));
    let second = executor
        .execute(&request(None, create_cmd("side")))
        .expect("create without key");
    assert!(committed(&second));
    let model = state(&executor);
    assert_eq!(model.workspace_order, vec![1, 2]);
    drop(executor);
    let store = EventStore::open(&path, JOURNAL).expect("reopen");
    assert_eq!(
        journal::full_replay(&store)
            .expect("replay")
            .stream(super::common::stream().as_str()),
        model
    );
}

#[test]
fn failed_commit_leaves_state_unchanged_and_sends_no_response() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = db_path(&dir);
    let fake = Fake::default();
    let executor = open(&path, &fake);
    let with_effect = |name: &str| Cmd::CreateWithEffect {
        name: name.to_owned(),
        effect_id: "fx-same".to_owned(),
    };
    executor
        .execute(&request(Some("k1"), with_effect("one")))
        .expect("first");
    let before = state(&executor);

    // 같은 effect id라 저장소가 transaction 전체를 되돌린다.
    let failed = executor.execute(&request(Some("k2"), with_effect("two")));
    assert!(matches!(failed, Err(ExecError::Store(_))), "{failed:?}");
    assert_eq!(state(&executor), before);

    let rejected = executor.execute(&request(
        Some("k3"),
        Cmd::RenameWorkspace {
            id: 99,
            name: "x".to_owned(),
        },
    ));
    assert!(matches!(
        rejected,
        Err(ExecError::Rejected(Reject::NoSuchWorkspace))
    ));
    assert_eq!(state(&executor), before);
    drop(executor);

    let store = EventStore::open(&path, JOURNAL).expect("reopen");
    for (k, cmd) in [
        ("k2", with_effect("two")),
        (
            "k3",
            Cmd::RenameWorkspace {
                id: 99,
                name: "x".to_owned(),
            },
        ),
    ] {
        let digest = fake.request_digest(&cmd);
        assert_eq!(
            store.lookup_command(&key(k), &digest).expect("lookup"),
            CommandLookup::Miss
        );
    }
    assert_eq!(
        journal::load(&store, &super::common::stream()).expect("load"),
        before
    );
}

#[test]
fn after_reopen_the_same_key_is_answered_without_deciding() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = db_path(&dir);
    let before_restart = Fake::default();
    let executor = open(&path, &before_restart);
    let first = executor
        .execute(&request(Some("k"), create_cmd("main")))
        .expect("create");
    let model = state(&executor);
    drop(executor);

    let after_restart = Fake::default();
    let executor = open(&path, &after_restart);
    assert_eq!(state(&executor), model);
    let again = executor
        .execute(&request(Some("k"), create_cmd("main")))
        .expect("retry");
    assert_eq!(after_restart.decides(), 0);
    assert_eq!(again.source, Source::Stored);
    assert_eq!(again.response, first.response);
    assert_eq!(state(&executor), model);

    let conflict = executor.execute(&request(Some("k"), create_cmd("other")));
    assert!(matches!(conflict, Err(ExecError::KeyConflict(_))));
    assert_eq!(after_restart.decides(), 0);

    let next = executor
        .execute(&request(Some("k2"), create_cmd("side")))
        .expect("new command after reopen");
    assert_eq!(next.response.as_deref(), Some(b"workspace:2".as_slice()));
}

#[test]
fn concurrent_same_key_decides_once_and_other_request_conflicts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let gate = Arc::new(Gate::default());
    let fake = Fake {
        gate: Some(gate.clone()),
        ..Fake::default()
    };
    let executor = open(&db_path(&dir), &fake);
    let same = request(Some("k"), create_cmd("main"));

    std::thread::scope(|scope| {
        let leader = scope.spawn(|| executor.execute(&same));
        wait_until(|| gate.entered.load(Ordering::SeqCst));
        let followers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| executor.execute(&same)))
            .collect();

        let other = executor.execute(&request(Some("k"), create_cmd("other")));
        assert!(matches!(other, Err(ExecError::KeyConflict(_))), "{other:?}");

        gate.release.store(true, Ordering::SeqCst);
        let first = leader.join().expect("leader").expect("execute");
        assert!(committed(&first));
        for follower in followers {
            let result = follower.join().expect("follower").expect("execute");
            assert_eq!(result.response, first.response);
            assert!(matches!(result.source, Source::Joined | Source::Stored));
        }
    });
    assert_eq!(fake.decides(), 1);
    assert_eq!(state(&executor).workspaces.len(), 1);

    let later = executor.execute(&request(Some("k"), create_cmd("other")));
    assert!(matches!(later, Err(ExecError::KeyConflict(_))));
    assert_eq!(fake.decides(), 1);
}

#[test]
fn revision_conflict_reloads_and_retries_then_gives_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake = Fake::default();
    let executor = open(&db_path(&dir), &fake);
    executor
        .execute(&request(Some("a"), create_cmd("main")))
        .expect("create");

    fake.stale_revisions.store(1, Ordering::SeqCst);
    let retried = executor
        .execute(&request(Some("b"), create_cmd("side")))
        .expect("retry succeeds");
    assert!(committed(&retried));
    assert_eq!(fake.decides(), 3);
    assert_eq!(executor.scalar_retry_reader().load(Ordering::Relaxed), 1);

    let before = state(&executor);
    fake.stale_revisions
        .store(MAX_DECIDE_ATTEMPTS as usize, Ordering::SeqCst);
    let exhausted = executor.execute(&request(Some("c"), create_cmd("third")));
    assert!(matches!(
        exhausted,
        Err(ExecError::RetriesExhausted(n)) if n == MAX_DECIDE_ATTEMPTS
    ));
    assert_eq!(fake.decides(), 3 + MAX_DECIDE_ATTEMPTS as usize);
    assert_eq!(
        executor.scalar_retry_reader().load(Ordering::Relaxed),
        u64::from(MAX_DECIDE_ATTEMPTS)
    );
    assert_eq!(state(&executor), before);
}

#[test]
fn fenced_writer_stops_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake = Fake::default();
    let executor = open(&db_path(&dir), &fake);
    executor
        .execute(&request(Some("a"), create_cmd("main")))
        .expect("create");
    let before = state(&executor);
    executor
        .inner
        .lock()
        .expect("inner")
        .store
        .acquire_writer()
        .expect("newer epoch");

    let fenced = executor.execute(&request(Some("b"), create_cmd("side")));
    assert!(
        matches!(&fenced, Err(ExecError::Store(e)) if matches!(**e, StoreError::Fenced { .. })),
        "{fenced:?}"
    );
    let decides = fake.decides();
    let halted = executor.execute(&request(Some("c"), create_cmd("third")));
    assert!(matches!(halted, Err(ExecError::Halted)));
    assert_eq!(fake.decides(), decides);
    assert_eq!(state(&executor), before);
}

#[test]
fn lost_writer_lock_stops_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake = Fake::default();
    let executor = open(&db_path(&dir), &fake);
    executor.inner.lock().expect("inner").store.release_writer();
    let lost = executor.execute(&request(Some("a"), create_cmd("main")));
    assert!(
        matches!(&lost, Err(ExecError::Store(e)) if matches!(**e, StoreError::NotWriter)),
        "{lost:?}"
    );
    assert!(matches!(
        executor.execute(&request(Some("b"), create_cmd("side"))),
        Err(ExecError::Halted)
    ));
}

#[test]
fn committed_retry_and_reads_wait_for_recovery_after_apply_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake = Fake::default();
    let executor = open(&db_path(&dir), &fake);
    let command = request(Some("committed"), create_cmd("main"));
    fake.fail_apply.store(true, Ordering::SeqCst);
    assert!(matches!(
        executor.execute(&command),
        Err(ExecError::Apply(_))
    ));
    assert!(executor.inner.lock().unwrap().state.workspaces.is_empty());
    fake.fail_load.store(true, Ordering::SeqCst);
    assert!(matches!(
        executor.execute(&command),
        Err(ExecError::Apply(_))
    ));
    let mut observed = false;
    assert!(matches!(
        executor.with_state(|_| observed = true),
        Err(ExecError::Apply(_))
    ));
    assert!(!observed, "a stale read callback must not run");
    assert_eq!(fake.decides(), 1);
    fake.fail_load.store(false, Ordering::SeqCst);
    let recovered = executor
        .execute(&command)
        .expect("stored result after recovery");
    assert_eq!(recovered.source, Source::Stored);
    assert_eq!(state(&executor).workspaces[&1].name, "main");
    assert_eq!(fake.decides(), 1, "committed creation is never repeated");
}

#[test]
fn pending_command_is_stored_without_a_final_response_and_preserves_causation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake = Fake::default();
    let executor = open(&db_path(&dir), &fake);
    let mut command = request(
        Some("pending"),
        Cmd::CreateWithEffect {
            name: "main".into(),
            effect_id: "spawn".into(),
        },
    );
    command.causation_id = Some("parent-command".into());
    let first = executor.execute(&command).expect("prepare");
    assert_eq!(first.status, tasty_event_store::CommandStatus::InProgress);
    assert_eq!(first.response, None);
    let retry = executor.execute(&command).expect("pending retry");
    assert_eq!(retry.status, first.status);
    assert_eq!(retry.response, None);
    assert_eq!(retry.command_id, first.command_id);
    assert_eq!(fake.decides(), 1);
    let inner = executor.inner.lock().unwrap();
    let batches = inner.store.read_batches_after(None, usize::MAX).unwrap();
    assert_eq!(batches.len(), 1);
    assert!(
        batches[0]
            .events
            .iter()
            .all(|event| event.causation_id.as_deref() == Some("parent-command"))
    );
}
