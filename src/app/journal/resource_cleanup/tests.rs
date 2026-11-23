//! Which retirement phases pause observation, and what an in-flight retirement still holds.
use super::*;

fn journal_with_cleanup() -> (JournalApplication, EngineId, OperationId) {
    let mut journal = JournalApplication::new(std::sync::Arc::new(|| {})).unwrap();
    let engine = EngineId::for_test();
    let operation = OperationId("cmd-1-1/prepare/0".into());
    journal
        .queue_resource_retirement(engine, "structure:slot-1".into(), operation.clone())
        .unwrap();
    (journal, engine, operation)
}

fn set_phase(journal: &mut JournalApplication, phase: Phase) {
    for entry in journal.resource_cleanups.values_mut() {
        entry.phase = match &phase {
            Phase::Claim => Phase::Claim,
            Phase::Running => Phase::Running,
            Phase::Reconcile => Phase::Reconcile,
            Phase::Reconciled => Phase::Reconciled,
            Phase::Finish { uncertain } => Phase::Finish {
                uncertain: *uncertain,
            },
        };
    }
}

#[test]
fn only_claim_and_finish_pause_observation() {
    let (mut journal, _, operation) = journal_with_cleanup();
    assert!(journal.cleanup_pauses_observation(), "Claim");
    assert!(!journal.cleanup_awaits_receipts(&operation));
    set_phase(&mut journal, Phase::Running);
    assert!(!journal.cleanup_pauses_observation(), "Running");
    assert!(journal.cleanup_awaits_receipts(&operation));
    set_phase(&mut journal, Phase::Finish { uncertain: false });
    assert!(journal.cleanup_pauses_observation(), "Finish");
    assert!(!journal.cleanup_awaits_receipts(&operation));
    set_phase(&mut journal, Phase::Reconcile);
    assert!(!journal.cleanup_pauses_observation(), "Reconcile");
    assert!(!journal.cleanup_awaits_receipts(&operation));
}

/// Engine retirement waits for receipts independently of the observation pause.
#[test]
fn a_running_retirement_still_holds_its_engine() {
    let (mut journal, engine, _) = journal_with_cleanup();
    set_phase(&mut journal, Phase::Running);
    assert!(journal.has_resource_cleanup(engine));
    #[cfg(feature = "gui")]
    assert!(journal.has_pending_engine_effects(engine));
    assert!(!journal.is_ready(engine));
}

#[test]
fn another_operation_is_not_mistaken_for_one_awaiting_receipts() {
    let (mut journal, _, _) = journal_with_cleanup();
    set_phase(&mut journal, Phase::Running);
    assert!(!journal.cleanup_awaits_receipts(&OperationId("cmd-1-2/prepare/0".into())));
}

/// Observation continues during the receipt wait, but shutdown must not drop the close's reply or
/// its recorded outcome, so it still waits for that phase.
#[test]
fn shutdown_waits_for_a_retirement_that_only_awaits_receipts() {
    let (mut journal, _, _) = journal_with_cleanup();
    set_phase(&mut journal, Phase::Running);
    assert!(!journal.pauses_observation());
    assert!(journal.shutdown_waits_for_publication());
    set_phase(&mut journal, Phase::Claim);
    assert!(journal.shutdown_waits_for_publication());
    set_phase(&mut journal, Phase::Reconcile);
    assert!(!journal.shutdown_waits_for_publication());
}

/// A publication wait that never progresses ends at the deadline fixed by its first poll, and
/// ends by halting: nothing reports the unfinished retirement as done.
#[test]
fn shutdown_stops_waiting_for_publication_at_its_first_deadline() {
    let (mut journal, _, _) = journal_with_cleanup();
    set_phase(&mut journal, Phase::Claim);
    let start = std::time::Instant::now();
    let mut deadline = None;
    assert!(journal.shutdown_publication_pending(&mut deadline, start));
    let fixed = deadline.expect("the first wait fixes the deadline");
    let late = fixed - std::time::Duration::from_millis(1);
    assert!(journal.shutdown_publication_pending(&mut deadline, late));
    assert_eq!(deadline, Some(fixed), "a later poll does not extend it");
    assert!(!journal.is_halted());
    assert!(!journal.shutdown_publication_pending(&mut deadline, fixed));
    assert!(journal.is_halted());
    assert!(!journal.shutdown_publication_pending(&mut deadline, fixed));
}

/// 은퇴한 surface를 쓰던 앞 소유자의 metadata와, 같은 ID의 새 소유자가 정리를 넣은 뒤에 쓰는 metadata.
const RETIRED_SURFACE: u32 = 900;

struct WriterEngine {
    session: crate::runtime::engine_session::EngineSession,
    store: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    access: tasty_memory::writer::MemoryAccess,
    writer: tasty_memory::writer::MemoryWriter<tasty_memory::writer::WriterStore>,
}

impl WriterEngine {
    /// 수집 창을 넓혀 정리 요청을 넣은 직후에는 결과가 없게 한다.
    fn new() -> Self {
        let store: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().expect("store"),
            ));
        let changes = std::sync::Arc::new(tasty_memory::writer::ChangeFeed::default());
        let wake = tasty_utils::batch_writer::WakeLatch::new(std::sync::Arc::new(|| {}));
        let writer = tasty_memory::writer::spawn(
            tasty_memory::writer::WriterStore::shared(store.clone()),
            changes.clone(),
            tasty_utils::batch_writer::Limits {
                max_delay: std::time::Duration::from_millis(200),
                ..Default::default()
            },
            std::sync::Arc::new(tasty_utils::batch_writer::SystemClock),
            wake.notifier(),
        )
        .expect("memory writer");
        let access = tasty_memory::writer::MemoryAccess::writer(writer.handle(), changes);
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, std::sync::Arc::new(|| {}))
                .expect("engine");
        session.runtime.use_memory_writer(access.clone());
        Self {
            session,
            store,
            access,
            writer,
        }
    }

    fn put_now(&self, key: &str, value: &str) {
        self.store
            .lock()
            .unwrap()
            .put(
                tasty_memory::HOST_OWNER,
                &tasty_memory::Scope::Surface(RETIRED_SURFACE),
                key,
                &tasty_memory::MemoryValue::Text(value.into()),
                &tasty_memory::PutOpts::default(),
            )
            .expect("put");
    }

    /// writer 순서로, 이미 넣은 정리 요청 뒤에 쓴다.
    fn put_after(&self, key: &str, value: &str) {
        let mut ticket = self
            .access
            .submit(tasty_memory::writer::MemoryRequest::Write(
                tasty_memory::WriteOp::Put {
                    owner: tasty_memory::HOST_OWNER.into(),
                    scope: tasty_memory::Scope::Surface(RETIRED_SURFACE),
                    key: key.into(),
                    value: tasty_memory::MemoryValue::Text(value.into()),
                    opts: tasty_memory::PutOpts::default(),
                },
            ))
            .expect("submit");
        let completion = ticket
            .wait_timeout(std::time::Duration::from_secs(10))
            .expect("write result");
        tasty_memory::writer::settle(completion).expect("write landed");
    }

    fn has(&self, key: &str) -> bool {
        self.store
            .lock()
            .unwrap()
            .get(&tasty_memory::Scope::Surface(RETIRED_SURFACE), key)
            .expect("get")
            .is_some()
    }

    fn retire(&mut self, operation: &OperationId) {
        self.session.pending_resource_retirements.insert(
            operation.clone(),
            crate::runtime::resource_retirement::ResourceRetirement::awaiting_metadata_for_test(
                vec![tasty_core::EntityId {
                    kind: tasty_core::IdKind::Surface,
                    id: RETIRED_SURFACE,
                }],
                crate::runtime::journal_product::EffectLease {
                    effect_id: "retire-effect".into(),
                    operation: operation.clone(),
                    stream: "structure:slot-1".into(),
                    runtime_epoch: 1,
                    resource_generation: 1,
                    attempt: 1,
                },
            ),
        );
    }

    fn shutdown(mut self) {
        self.writer.begin_shutdown();
        while self.writer.poll_shutdown() != 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

fn journal_for(engine: EngineId, phase: Phase) -> (JournalApplication, OperationId) {
    let mut journal = JournalApplication::new(std::sync::Arc::new(|| {})).unwrap();
    let operation = OperationId("cmd-1-1/prepare/0".into());
    journal
        .queue_resource_retirement(engine, "structure:slot-1".into(), operation.clone())
        .unwrap();
    set_phase(&mut journal, phase);
    (journal, operation)
}

fn only_entry(journal: &JournalApplication) -> &Cleanup {
    journal.resource_cleanups.values().next().expect("cleanup")
}

/// scope 정리 결과가 오기 전에는 은퇴가 Running으로 남아 다시 폴링하고, 결과 뒤에 끝난다. 정리는 앞
/// 소유자의 metadata만 지우고 같은 ID의 새 소유자가 그 뒤에 쓴 값은 남긴다.
#[test]
fn a_pending_scope_purge_keeps_the_retirement_running_until_its_result() {
    let mut engine = WriterEngine::new();
    engine.put_now("old.owner", "before close");
    let (mut journal, operation) = journal_for(engine.session.id, Phase::Running);
    engine.retire(&operation);

    journal
        .poll_resource_cleanup(&mut [&mut engine.session], None)
        .expect("poll");
    assert!(
        matches!(only_entry(&journal).phase, Phase::Running),
        "결과 전에 끝내면 안 된다"
    );
    assert!(
        engine.has("old.owner"),
        "수집 창 안에서는 정리가 아직 반영되지 않았다"
    );

    engine.put_after("new.owner", "after close");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut started = false;
    // 원래 순서(읽기 → 기한 검사 → 5ms → poll → 읽기) 그대로, 기한 뒤 읽기는 poll 없이 단계만 본다.
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, std::time::Duration::ZERO, |last| {
            if !last && std::mem::replace(&mut started, true) {
                std::thread::sleep(std::time::Duration::from_millis(5));
                journal
                    .poll_resource_cleanup(&mut [&mut engine.session], None)
                    .expect("poll");
            }
            !matches!(only_entry(&journal).phase, Phase::Running)
        }),
        "정리 결과를 거두지 못했다"
    );
    // 끝난 작업은 바로 journal worker로 넘어가므로 단계로 결과를 본다.
    assert!(matches!(
        only_entry(&journal).phase,
        Phase::Finish { uncertain: false }
    ));
    assert!(!engine.has("old.owner"), "앞 소유자의 값은 지운다");
    assert!(
        engine.has("new.owner"),
        "정리 뒤에 쓴 새 소유자의 값은 남긴다"
    );
    engine.shutdown();
}

/// 결과가 불확실해 Reconcile로 간 은퇴는 정리를 다시 넣고, 결과가 오기 전에는 증거를 내지 않는다.
#[test]
fn a_reconciling_retirement_waits_for_its_resubmitted_purge() {
    let mut engine = WriterEngine::new();
    engine.put_now("old.owner", "before close");
    let (mut journal, operation) = journal_for(engine.session.id, Phase::Reconcile);
    engine.retire(&operation);

    journal
        .poll_resource_cleanup(&mut [&mut engine.session], None)
        .expect("poll");
    assert!(
        matches!(only_entry(&journal).phase, Phase::Reconcile),
        "결과 전에는 증거가 없다"
    );
    assert!(engine.has("old.owner"));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut started = false;
    // 원래 순서(읽기 → 기한 검사 → 5ms → 재제출 poll → 읽기) 그대로, 기한 뒤 읽기는 재제출 없이 단계만 본다.
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, std::time::Duration::ZERO, |last| {
            if !last && std::mem::replace(&mut started, true) {
                std::thread::sleep(std::time::Duration::from_millis(5));
                for entry in journal.resource_cleanups.values_mut() {
                    entry.next_reconcile = std::time::Instant::now();
                }
                journal
                    .poll_resource_cleanup(&mut [&mut engine.session], None)
                    .expect("poll");
            }
            !matches!(only_entry(&journal).phase, Phase::Reconcile)
        }),
        "정리 결과를 거두지 못했다"
    );
    assert!(matches!(only_entry(&journal).phase, Phase::Reconciled));
    assert!(!engine.has("old.owner"));
    engine.shutdown();
}
