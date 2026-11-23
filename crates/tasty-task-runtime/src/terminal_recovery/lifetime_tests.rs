//! Actual queued reads retain their delivery identity beyond confirmed acknowledgement.
use super::*;
use crate::{TaskScope, TaskService};
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tasty_agent::task::{Completion, TaskCreateOpts, TaskPurgeFilter};
use tasty_agent::{OnFailure, Task, TaskCommand, TaskResult, TaskState, TaskStore};
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::writer::{MemoryWriter, WriterStore, settle};

const WAIT: Duration = Duration::from_secs(3);
struct Fixture {
    writer: MemoryWriter<WriterStore>,
    memory: Arc<Mutex<dyn MemoryStorage>>,
    faults: Arc<Faults>,
    service: TaskService,
    scope: TaskScope,
}
impl Fixture {
    fn new() -> Self {
        let faults = Arc::new(Faults::default().writing(terminal::PREFIX));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
        let changes = Arc::new(tasty_memory::writer::ChangeFeed::default());
        let writer = tasty_memory::writer::spawn(
            WriterStore::shared(memory.clone()),
            changes.clone(),
            tasty_utils::batch_writer::Limits::default(),
            Arc::new(tasty_utils::batch_writer::SystemClock),
            Arc::new(|| {}),
        )
        .unwrap();
        let service = TaskService::new(
            MemoryAccess::writer(writer.handle(), changes),
            Arc::new(OnceLock::new()),
            Arc::new(crate::completion::fixture::Resolver::default()),
        );
        let scope = TaskScope::new(service.runner_registry().clone());
        Self {
            writer,
            memory,
            faults,
            service,
            scope,
        }
    }
    fn read(&self) -> Confirmed {
        self.read_result().unwrap()
    }
    fn read_result(&self) -> tasty_agent::Result<Confirmed> {
        let target = Target {
            hub: self.scope.waker_hub().clone(),
            workspace: 1,
            settle: None,
        };
        let mut ticket = self
            .service
            .memory_access()
            .submit(MemoryRequest::read(0, move |mem| {
                Ok(load(mem, vec![target.clone()]))
            }))
            .unwrap();
        settle(ticket.wait_timeout(WAIT).unwrap())
            .unwrap()
            .into_value::<tasty_agent::Result<Confirmed>>()
            .unwrap()
    }
    fn drain(&self) {
        let until = Instant::now() + WAIT;
        // reap_detached 는 끝난 ticket 만 거둔다. 마지막 읽기 앞에서도 그대로 돈다.
        assert!(
            tasty_utils::test_wait::holds_by_every(until, Duration::from_millis(1), |_| {
                let access = self.service.memory_access();
                access.reap_detached();
                access.detached_len() == 0
            }),
            "owned acknowledgement did not settle"
        );
    }
    fn pending(&self) -> usize {
        terminal::pending(&*self.memory.lock().unwrap(), 1)
            .unwrap()
            .len()
    }
    fn events(&self) -> usize {
        self.scope
            .event_queue()
            .take_pending()
            .0
            .into_iter()
            .filter(|e| matches!(e, crate::event_feed::AgentEvent::TaskFinished { .. }))
            .count()
    }
    fn seed(&self) {
        let mut mem = self.memory.lock().unwrap();
        let task: Task = serde_json::from_value(serde_json::json!({
            "id":"p", "workspace_id":1,"name":"p","created_at":0,
            "command":{"kind":"custom","ipc_method":"system.ping","params":{}},
            "state":{"kind":"succeeded"},"on_failure":{"kind":"abort"}
        }))
        .unwrap();
        TaskStore::new(&mut *mem, HOST_OWNER, self.scope.agent_seq())
            .put(&task)
            .unwrap();
        assert_eq!(terminal::pending(&*mem, 1).unwrap().len(), 1);
    }
    fn retained(&self) -> usize {
        self.scope.waker_hub().retained_delivery_count()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.writer.begin_shutdown();
        let until = Instant::now() + WAIT;
        // poll 은 끝난 worker 를 join 하기만 한다. 마지막 읽기로 쓴다.
        assert!(tasty_utils::test_wait::holds_by_every(
            until,
            Duration::from_millis(1),
            |_| self.writer.poll_shutdown() == 0
        ));
    }
}

#[test]
fn two_queued_read_outputs_share_claim_after_confirmed_ack() {
    let f = Fixture::new();
    f.seed();
    let earlier = f.read();
    let delayed = f.read();
    deliver(earlier, f.service.memory_access());
    f.drain();
    assert_eq!(f.pending(), 0);
    assert_eq!(f.events(), 1);
    // Ack confirmation must free the registry even while the delayed output lives.
    assert_eq!(f.retained(), 0);
    deliver(delayed, f.service.memory_access());
    f.drain();
    assert_eq!(
        f.events(),
        0,
        "old read must retain its already-delivered claim"
    );
    assert_eq!(f.retained(), 0);
}

#[test]
fn confirmed_complete_ack_and_purge_do_not_retain_history() {
    let f = Fixture::new();
    for n in 0..64 {
        let task = f
            .service
            .task_create(
                &f.scope,
                TaskCreateOpts {
                    workspace_id: 1,
                    name: format!("cycle-{n}"),
                    command: TaskCommand::Custom {
                        ipc_method: "system.ping".into(),
                        params: serde_json::json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::json!({}),
                    now_ms: 1,
                },
                false,
            )
            .unwrap();
        f.service
            .task_set_state(&f.scope, 1, &task.id, TaskState::Running, 2)
            .unwrap();
        f.service
            .task_complete(
                &f.scope,
                1,
                &task.id,
                Completion::succeeded(
                    None,
                    TaskResult {
                        exit_code: Some(0),
                        output: None,
                        error: None,
                    },
                ),
                3,
            )
            .unwrap();
        f.drain();
        assert_eq!(f.pending(), 0);
        assert_eq!(f.events(), 1);
        f.service
            .task_purge(
                &f.scope,
                1,
                TaskPurgeFilter {
                    states: Some(vec!["succeeded".into()]),
                    older_than_ms: None,
                    now_ms: 4,
                },
                false,
            )
            .unwrap();
        f.drain();
        assert!(f.service.task_list(&f.scope, 1).unwrap().is_empty());
    }
    assert_eq!(
        f.retained(),
        0,
        "acknowledged UUID history must be released"
    );
}

#[test]
fn failed_and_unknown_ack_keep_claim_until_ordered_confirmation() {
    for after_commit in [false, true] {
        let f = Fixture::new();
        f.seed();
        let earlier = f.read();
        let original = earlier.deliveries[0].notice.clone();
        let delayed = f.read();
        let fault = if after_commit {
            &f.faults.units_after_writing
        } else {
            &f.faults.units_before_body
        };
        Faults::arm(fault, 1);
        deliver(earlier, f.service.memory_access());
        f.drain();
        assert_eq!(fault.load(Ordering::SeqCst), 0);
        assert_eq!(f.events(), 1);
        assert_eq!(f.pending(), usize::from(!after_commit));
        assert_eq!(f.retained(), 1, "unconfirmed ack must retain claim");
        // A malformed pending row must not be interpreted as confirmed absence.
        let key = format!("{}{}", terminal::PREFIX, original.id);
        {
            let mut mem = f.memory.lock().unwrap();
            let mut unknown = original.clone();
            unknown.schema = 999;
            mem.put(
                HOST_OWNER,
                &tasty_memory::Scope::Workspace(1),
                &key,
                &tasty_memory::MemoryValue::Json(serde_json::to_value(unknown).unwrap()),
                &Default::default(),
            )
            .unwrap();
        }
        assert!(f.read_result().is_err());
        assert_eq!(f.retained(), 1);
        {
            let mut mem = f.memory.lock().unwrap();
            if after_commit {
                mem.delete(HOST_OWNER, &tasty_memory::Scope::Workspace(1), &key, None)
                    .unwrap();
            } else {
                mem.put(
                    HOST_OWNER,
                    &tasty_memory::Scope::Workspace(1),
                    &key,
                    &tasty_memory::MemoryValue::Json(serde_json::to_value(original).unwrap()),
                    &Default::default(),
                )
                .unwrap();
            }
        }
        // A strong read confirms absence after an unanswered committed ack, or shares the
        // pending identity before an idempotent ack. Neither may publish another event.
        let retry = f.read();
        deliver(retry, f.service.memory_access());
        f.drain();
        assert_eq!(f.pending(), 0);
        assert_eq!(f.retained(), 0);
        deliver(delayed, f.service.memory_access());
        f.drain();
        assert_eq!(f.events(), 0);
        assert_eq!(f.retained(), 0);
    }
}

#[test]
fn cached_settlement_cannot_remove_a_retried_running_attempt() {
    for typed in [false, true] {
        cached_settlement_after_retry(typed);
    }
}

fn cached_settlement_after_retry(typed: bool) {
    use crate::TaskEffects;
    use crate::runner_host::handle_key;
    use tasty_memory::{MemoryValue, Scope};
    let f = Fixture::new();
    if typed {
        let graph = serde_json::from_value(serde_json::json!({
            "contract_version":2,"tasks":[{"id":"p",
            "command":{"kind":"run","workspace_id":1,"command":["true"]},
            "output_schema":{"type":"int64"},"metadata":{"semaphore":{"name":"owned"}}}]
        }))
        .unwrap();
        f.service
            .task_graph_submit(&f.scope, 1, graph, false, 1)
            .unwrap();
    } else {
        let task: Task = serde_json::from_value(serde_json::json!({
            "id":"p","workspace_id":1,"name":"p","created_at":1,
            "command":{"kind":"custom","ipc_method":"system.ping","params":{}},
            "state":{"kind":"ready"},"on_failure":{"kind":"abort"},
            "metadata":{"semaphore":{"name":"owned"}}
        }))
        .unwrap();
        TaskStore::new(
            &mut *f.memory.lock().unwrap(),
            HOST_OWNER,
            f.scope.agent_seq(),
        )
        .put(&task)
        .unwrap();
    }
    let old = f
        .service
        .task_set_state(&f.scope, 1, &"p".into(), TaskState::Running, 2)
        .unwrap()
        .0;
    let old_id = old.attempt.map(|a| a.id);
    let record = |attempt: Option<&str>| {
        let mut value =
            serde_json::json!({"kind":"barrier_poll","data":{"workspace_id":1,"name":"owned"}});
        if let Some(id) = attempt {
            value["attempt_id"] = id.into();
        }
        value
    };
    let install = |attempt: Option<&str>| {
        f.memory
            .lock()
            .unwrap()
            .put(
                HOST_OWNER,
                &Scope::Workspace(1),
                &handle_key("p"),
                &MemoryValue::Json(record(attempt)),
                &Default::default(),
            )
            .unwrap();
    };
    install(old_id.as_deref());
    {
        let mut mem = f.memory.lock().unwrap();
        let mut store = tasty_agent::SemaphoreStore::new(&mut *mem, HOST_OWNER);
        store.create(1, "owned", 2, 1).unwrap();
        for id in ["p", "foreign"] {
            assert!(store.acquire(1, "owned", id, None, 2).unwrap().acquired);
        }
    }
    let scope = f.scope.clone();
    let mut ticket = f
        .service
        .memory_access()
        .submit(
            f.service
                .unit_request(0, move |tx| tx.task_cancel(&scope, 1, &"p".into(), 3)),
        )
        .unwrap();
    let (result, _effects) = settle(ticket.wait_timeout(WAIT).unwrap())
        .unwrap()
        .into_value::<(tasty_agent::Result<(Task, Vec<Task>)>, TaskEffects)>()
        .unwrap();
    result.unwrap();
    let read = || {
        let target = Target {
            hub: f.scope.waker_hub().clone(),
            workspace: 1,
            settle: Some(f.service.runner_context(&f.scope)),
        };
        let mut ticket = f
            .service
            .memory_access()
            .submit(MemoryRequest::read(0, move |mem| {
                Ok(load(mem, vec![target.clone()]))
            }))
            .unwrap();
        settle(ticket.wait_timeout(WAIT).unwrap())
            .unwrap()
            .into_value::<tasty_agent::Result<Confirmed>>()
            .unwrap()
            .unwrap()
    };
    let independent = read();
    let delayed = read();
    deliver(independent, f.service.memory_access());
    let until = Instant::now() + WAIT;
    // reap_detached 는 끝난 ticket 만 거둔다. 마지막 읽기 앞에서도 그대로 돈다.
    assert!(tasty_utils::test_wait::holds_by_every(
        until,
        Duration::from_millis(1),
        |_| {
            f.service.memory_access().reap_detached();
            f.memory
                .lock()
                .unwrap()
                .get(&Scope::Workspace(1), &handle_key("p"))
                .unwrap()
                .is_none()
        }
    ));
    f.drain();
    assert_eq!(f.events(), 1);
    f.service
        .task_retry(&f.scope, 1, &"p".into(), false, 4)
        .unwrap();
    let new = f
        .service
        .task_set_state(&f.scope, 1, &"p".into(), TaskState::Running, 5)
        .unwrap()
        .0;
    let new_id = new.attempt.map(|a| a.id);
    if typed {
        assert_ne!(old_id, new_id);
    }
    install(new_id.as_deref());
    {
        let mut mem = f.memory.lock().unwrap();
        assert!(
            tasty_agent::SemaphoreStore::new(&mut *mem, HOST_OWNER)
                .acquire(1, "owned", "p", None, 5)
                .unwrap()
                .acquired
        );
    }
    deliver(delayed, f.service.memory_access());
    f.drain();
    // The worker has its own bounded lifetime. Observe the same two-second interval as the
    // counterexample, checking continuously rather than ending before it can run.
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        let mut mem = f.memory.lock().unwrap();
        let entry = mem
            .get(&Scope::Workspace(1), &handle_key("p"))
            .unwrap()
            .expect("cached old settlement deleted the new running handle");
        assert_eq!(entry.value, MemoryValue::Json(record(new_id.as_deref())));
        let mut holders = tasty_agent::SemaphoreStore::new(&mut *mem, HOST_OWNER)
            .get(1, "owned")
            .unwrap()
            .unwrap()
            .holders
            .into_iter()
            .map(|h| h.id)
            .collect::<Vec<_>>();
        holders.sort();
        assert_eq!(holders, ["foreign", "p"]);
        drop(mem);
        std::thread::sleep(Duration::from_millis(2));
    }
    let current = f
        .service
        .task_get(&f.scope, 1, &"p".into())
        .unwrap()
        .unwrap();
    assert_eq!(current.state, TaskState::Running);
    assert_eq!(current.attempt.map(|a| a.id), new_id);
    assert_eq!(f.events(), 0);
}
