//! A delayed unrelated continuation must not consume a cancellation's cleanup obligation.
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tasty_agent::{AgentError, LeaseStore, SemaphoreStore, Task, TaskStore};
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::writer::{MemoryAccess, MemoryRequest, MemoryWriter, WriterStore, settle};
use tasty_memory::{HOST_OWNER, MemoryStorage, MemoryValue, Scope};

use crate::event_feed::AgentEvent;
use crate::runner_host::handle_key;
use crate::{TaskEffects, TaskScope, TaskService};

const WAIT: Duration = Duration::from_secs(3);
struct Fixture {
    writer: Option<MemoryWriter<WriterStore>>,
    memory: Arc<Mutex<dyn MemoryStorage>>,
    faults: Arc<Faults>,
    service: TaskService,
    scope: TaskScope,
}

impl Fixture {
    fn new(queued: bool) -> Self {
        let faults = Arc::new(Faults::default().writing("tasty.agent.task."));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
        let changes = Arc::new(tasty_memory::writer::ChangeFeed::default());
        let writer = queued.then(|| {
            tasty_memory::writer::spawn(
                WriterStore::shared(memory.clone()),
                changes.clone(),
                tasty_utils::batch_writer::Limits::default(),
                Arc::new(tasty_utils::batch_writer::SystemClock),
                Arc::new(|| {}),
            )
            .unwrap()
        });
        let access = writer.as_ref().map_or_else(
            || MemoryAccess::inline(memory.clone()),
            |writer| MemoryAccess::writer(writer.handle(), changes),
        );
        let service = TaskService::new(
            access,
            Arc::new(OnceLock::new()),
            Arc::new(crate::completion::fixture::Resolver::default()),
        );
        let scope = TaskScope::new(service.runner_registry().clone());
        {
            let mut mem = memory.lock().unwrap();
            SemaphoreStore::new(&mut *mem, HOST_OWNER)
                .create(1, "owned", 3, 1)
                .unwrap();
            for (id, state, dependencies) in [
                ("parent", "running", vec![]),
                ("child", "waiting", vec!["parent"]),
            ] {
                let task: Task = serde_json::from_value(serde_json::json!({
                    "id":id,"workspace_id":1,"name":id,"created_at":1,
                    "command":{"kind":"custom","ipc_method":"system.ping","params":{}},
                    "state":{"kind":state},"depends_on":dependencies,
                    "on_failure":{"kind":"abort"},
                    "metadata":{"semaphore":{"name":"owned"},"lease":{"resource":id}}
                }))
                .unwrap();
                TaskStore::new(&mut *mem, HOST_OWNER, scope.agent_seq())
                    .put(&task)
                    .unwrap();
                mem.put(
                    HOST_OWNER,
                    &Scope::Workspace(1),
                    &handle_key(id),
                    &MemoryValue::Json(serde_json::json!({
                        "kind":"barrier_poll","data":{"workspace_id":1,"name":"owned"}
                    })),
                    &Default::default(),
                )
                .unwrap();
                assert!(
                    SemaphoreStore::new(&mut *mem, HOST_OWNER)
                        .acquire(1, "owned", id, None, 1)
                        .unwrap()
                        .acquired
                );
                LeaseStore::new(&mut *mem, HOST_OWNER)
                    .acquire(1, id, id, None, Default::default(), 1)
                    .unwrap();
            }
            let mut other = TaskStore::read_record(&*mem, 1, &"parent".into())
                .unwrap()
                .unwrap()
                .0;
            other.id = "other-running".into();
            other.metadata = serde_json::json!({});
            TaskStore::new(&mut *mem, HOST_OWNER, scope.agent_seq())
                .put(&other)
                .unwrap();
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(1),
                &handle_key(&other.id),
                &MemoryValue::Json(serde_json::json!({
                    "kind":"barrier_poll","data":{"workspace_id":1,"name":"other"}
                })),
                &Default::default(),
            )
            .unwrap();
            SemaphoreStore::new(&mut *mem, HOST_OWNER)
                .acquire(1, "owned", "foreign", None, 1)
                .unwrap();
            LeaseStore::new(&mut *mem, HOST_OWNER)
                .acquire(1, "untouched", "foreign", None, Default::default(), 1)
                .unwrap();
        }
        Self {
            writer,
            memory,
            faults,
            service,
            scope,
        }
    }

    fn effects(&self, request: MemoryRequest) -> Option<TaskEffects> {
        let mut ticket = self.service.memory_access().submit(request).unwrap();
        let completion = settle(ticket.wait_timeout(WAIT).expect("owned writer receipt"));
        completion.ok().map(|out| {
            let (result, effects) = out
                .into_value::<(Result<(Task, Vec<Task>), AgentError>, TaskEffects)>()
                .unwrap();
            result.unwrap();
            effects
        })
    }

    fn pending(&self) -> usize {
        tasty_agent::task::terminal::pending(&*self.memory.lock().unwrap(), 1)
            .unwrap()
            .len()
    }

    fn wait_for(&self, mut check: impl FnMut() -> bool) -> bool {
        let until = Instant::now() + WAIT;
        // reap_detached 는 끝난 ticket 만 거둔다. 마지막 읽기 앞에서도 그대로 돈다.
        tasty_utils::test_wait::holds_by_every(until, Duration::from_millis(2), |_| {
            self.service.memory_access().reap_detached();
            check()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(writer) = &mut self.writer {
            writer.begin_shutdown();
            let until = Instant::now() + WAIT;
            // poll 은 끝난 worker 를 join 하기만 한다(종료는 앞에서 시작했다). 마지막 읽기로 쓴다.
            assert!(
                tasty_utils::test_wait::holds_by_every(until, Duration::from_millis(2), |_| writer
                    .poll_shutdown()
                    == 0),
                "owned writer did not join"
            );
        }
    }
}

fn apply_delayed_continuations(f: &Fixture, stolen: bool, unknown: bool) {
    assert!(!f.service.runner_registry().has_live_runner(1));
    // Commit A and B before applying either continuation, as the actual queued IPC path can do.
    let scope = f.scope.clone();
    let earlier = f
        .effects(f.service.unit_request(0, move |tx| {
            tx.task_create(
                &scope,
                tasty_agent::task::TaskCreateOpts {
                    workspace_id: 1,
                    name: "unrelated".into(),
                    command: tasty_agent::TaskCommand::Custom {
                        ipc_method: "system.ping".into(),
                        params: serde_json::json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: tasty_agent::OnFailure::Abort,
                    metadata: serde_json::json!({}),
                    now_ms: 2,
                },
                false,
            )
            .map(|task| (task, Vec::<Task>::new()))
        }))
        .unwrap();
    let scope = f.scope.clone();
    let (request, recovery) = f
        .service
        .unit_request_with_recovery(0, move |tx| tx.task_cancel(&scope, 1, &"parent".into(), 3));
    if unknown {
        Faults::arm(&f.faults.units_after_writing, 1);
    }
    let cancel = f.effects(request);
    assert_eq!(
        cancel.is_none(),
        unknown,
        "Unknown response must stay unknown"
    );
    assert_eq!(
        f.pending(),
        2,
        "parent and downstream committed before continuations"
    );
    if stolen {
        earlier.apply();
        assert!(
            f.wait_for(|| f.pending() == 0),
            "A must ack B's notices first"
        );
    }
    if let Some(cancel) = cancel {
        cancel.apply();
    } else {
        recovery.schedule(f.service.memory_access());
    }
    recovery.schedule(f.service.memory_access());
}

fn cleanup_after_delayed_continuations(queued: bool, stolen: bool, unknown: bool) {
    let f = Fixture::new(queued);
    apply_delayed_continuations(&f, stolen, unknown);
    let cleared = f.wait_for(|| {
        let mem = f.memory.lock().unwrap();
        ["parent", "child"].iter().all(|id| {
            mem.get(&Scope::Workspace(1), &handle_key(id))
                .unwrap()
                .is_none()
        })
    });
    assert!(
        cleared,
        "cleanup lost: queued={queued}, earlier_recovery={stolen}, unknown={unknown}"
    );
    assert!(f.wait_for(|| f.pending() == 0));
    let mut mem = f.memory.lock().unwrap();
    assert!(
        mem.get(&Scope::Workspace(1), &handle_key("other-running"))
            .unwrap()
            .is_some()
    );
    let holders = SemaphoreStore::new(&mut *mem, HOST_OWNER)
        .get(1, "owned")
        .unwrap()
        .unwrap()
        .holders;
    assert_eq!(holders.len(), 1);
    assert_eq!(
        holders[0].id, "foreign",
        "only task-owned holdings may be returned"
    );
    for id in ["parent", "child"] {
        assert!(
            LeaseStore::new(&mut *mem, HOST_OWNER)
                .get(1, id)
                .unwrap()
                .is_none()
        );
        assert!(
            TaskStore::read_record(&*mem, 1, &id.into())
                .unwrap()
                .unwrap()
                .0
                .state
                .is_terminal()
        );
    }
    assert_eq!(
        LeaseStore::new(&mut *mem, HOST_OWNER)
            .get(1, "untouched")
            .unwrap()
            .unwrap()
            .holder,
        "foreign"
    );
    drop(mem);
    let events = f.scope.event_queue().take_pending().0;
    let mut finished: Vec<_> = events
        .into_iter()
        .filter_map(|event| match event {
            AgentEvent::TaskFinished {
                task_id,
                terminal_id,
                ..
            } => Some((task_id, terminal_id.unwrap())),
            _ => None,
        })
        .collect();
    finished.sort();
    assert_eq!(finished.len(), 2);
    assert_eq!(finished[0].0, "child");
    assert_eq!(finished[1].0, "parent");
    assert_ne!(finished[0].1, finished[1].1);
}

#[test]
fn inline_cleanup_is_independent_of_terminal_notice_consumption() {
    for stolen in [false, true] {
        for unknown in [false, true] {
            cleanup_after_delayed_continuations(false, stolen, unknown);
        }
    }
}

#[test]
fn queued_cleanup_is_independent_of_terminal_notice_consumption() {
    for stolen in [false, true] {
        for unknown in [false, true] {
            cleanup_after_delayed_continuations(true, stolen, unknown);
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn queued_cancel_after_an_earlier_ack_reaps_its_owned_shell_once() {
    use std::os::unix::process::CommandExt;
    struct Owned(std::process::Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            if self.0.try_wait().unwrap().is_none() {
                self.0.kill().unwrap();
                self.0.wait().unwrap();
            }
        }
    }
    let f = Fixture::new(true);
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("executions");
    let home = dir.path().join("home");
    let argv = ["-c", "printf 'once\\n' >> \"$1\"; exec sleep 60", "owned"];
    let mut command = std::process::Command::new("sh");
    command
        .args(argv)
        .arg(&marker)
        .env("TASTY_HOME", &home)
        .process_group(0);
    let mut child = Owned(command.spawn().unwrap());
    let pid = child.0.id();
    let started_at = tasty_agent::platform::process_start::start_time(pid).unwrap();
    assert!(f.wait_for(|| marker.exists()));
    let environment = std::fs::read(format!("/proc/{pid}/environ")).unwrap();
    let expected = format!("TASTY_HOME={}", home.display());
    assert!(
        environment
            .split(|b| *b == 0)
            .any(|entry| entry == expected.as_bytes())
    );
    {
        let mut mem = f.memory.lock().unwrap();
        let mut store = TaskStore::new(&mut *mem, HOST_OWNER, f.scope.agent_seq());
        let mut task = store.get(1, &"parent".into()).unwrap().unwrap();
        task.command = tasty_agent::TaskCommand::Run {
            command: std::iter::once("sh".into())
                .chain(argv.map(String::from))
                .chain([marker.to_string_lossy().into_owned()])
                .collect(),
            workspace_id: 1,
            cwd: None,
        };
        store.put(&task).unwrap();
        mem.put(
            HOST_OWNER,
            &Scope::Workspace(1),
            &handle_key("parent"),
            &MemoryValue::Json(serde_json::json!({
                "kind":"shell_process","data":{"pid":pid},"started_at":started_at
            })),
            &Default::default(),
        )
        .unwrap();
    }
    apply_delayed_continuations(&f, true, false);
    assert!(f.wait_for(|| child.0.try_wait().unwrap().is_some()));
    assert!(f.wait_for(|| {
        let mem = f.memory.lock().unwrap();
        ["parent", "child"].iter().all(|id| {
            mem.get(&Scope::Workspace(1), &handle_key(id))
                .unwrap()
                .is_none()
        })
    }));
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "once\n");
    let mut mem = f.memory.lock().unwrap();
    let holders = SemaphoreStore::new(&mut *mem, HOST_OWNER)
        .get(1, "owned")
        .unwrap()
        .unwrap()
        .holders;
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].id, "foreign");
    if let Some(path) = std::env::var_os("TASTY_TASK_CLEANUP_EVIDENCE_DIR") {
        std::fs::write(
            std::path::Path::new(&path).join("owned-shell.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "pid":pid,"started_at":started_at,"TASTY_HOME":home,
                "owned_environment_confirmed":true,"process_reaped":true,
                "process_absent":true,"execution_count":1,"handle_removed":true,
                "foreign_holder_preserved":true,"app_process_spawned":false
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
