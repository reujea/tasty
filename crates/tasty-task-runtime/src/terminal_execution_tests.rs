//! Exercise real shell dispatch and downstream progression with a lost completion response.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tasty_agent::runner::RunnerLoop;
use tasty_agent::task::TaskGraphSpec;
use tasty_agent::{TaskState, TaskStore};
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::writer::MemoryAccess;
use tasty_memory::{HOST_OWNER, MemoryStorage};

use crate::event_feed::AgentEvent;
use crate::runner_host::HostExecutor;
use crate::{TaskScope, TaskService};

/// Append `word` as one line to `executions` in the task's working directory with the platform shell.
fn append_line(word: &str) -> serde_json::Value {
    #[cfg(windows)]
    let argv = serde_json::json!(["cmd", "/C", "echo", format!("{word}>>executions")]);
    #[cfg(not(windows))]
    let argv = serde_json::json!(["sh", "-c", format!("echo {word} >> executions")]);
    argv
}

#[test]
fn unknown_completion_does_not_execute_either_shell_or_downstream_twice() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("executions");
    let faults = Arc::new(Faults::default().writing("tasty.agent.typed_task."));
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
    let service = TaskService::new(
        MemoryAccess::inline(memory),
        Arc::new(OnceLock::new()),
        Arc::new(crate::completion::fixture::Resolver::default()),
    );
    let scope = TaskScope::new(service.runner_registry().clone());
    let ctx = service.runner_context(&scope);
    let graph: TaskGraphSpec = serde_json::from_value(serde_json::json!({
        "contract_version":2,"tasks":[
          {"id":"parent", "command":{"kind":"run","workspace_id":1,"cwd":dir.path(),
            "command":append_line("parent")}, "output_schema":{"type":"int64"}},
          {"id":"child", "depends_on":["parent"], "command":{"kind":"run","workspace_id":1,"cwd":dir.path(),
            "command":append_line("child")}, "output_schema":{"type":"int64"}}
        ]})).unwrap();
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, &AtomicU64::new(0))
            .submit_graph(1, graph, 1)
            .unwrap()
    });
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    let mut reports = super::CompletionReports::default();
    let mut injected = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    for tick in 1..1000 {
        let tasks = ctx.with_store(|mem| {
            TaskStore::new(mem, HOST_OWNER, scope.agent_seq())
                .list(1)
                .unwrap()
        });
        assert!(
            Instant::now() < deadline,
            "fixture did not finish: {tasks:?}"
        );
        let for_set = ctx.clone();
        runner.tick(
            1,
            tick * 100,
            &tasks,
            move |ws, id, state, now| {
                let seq = for_set.agent_seq.clone();
                let id = id.clone();
                for_set.with_memory(move |mem| {
                    TaskStore::new(mem, HOST_OWNER, &seq)
                        .set_state(ws, &id, state.clone(), now)
                        .map(|_| ())
                })
            },
            |ws, id, completion, now| {
                if !injected {
                    Faults::arm(&faults.units_after_writing, 1);
                    injected = true;
                    let unknown = reports.report(&ctx, ws, id, completion, now);
                    assert!(
                        unknown.is_err(),
                        "the first completion must keep its unknown response"
                    );
                    unknown
                } else {
                    reports.report(&ctx, ws, id, completion, now)
                }
            },
        );
        if tasks
            .iter()
            .all(|task| matches!(task.state, TaskState::Succeeded))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(injected);
    assert_eq!(faults.units_after_writing.load(Ordering::SeqCst), 0);
    let tasks = ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, scope.agent_seq())
            .list(1)
            .unwrap()
    });
    assert!(
        tasks.iter().all(|t| t.state == TaskState::Succeeded),
        "{tasks:?}"
    );
    let lines = std::fs::read_to_string(marker).unwrap();
    assert_eq!(lines.lines().filter(|line| *line == "parent").count(), 1);
    assert_eq!(lines.lines().filter(|line| *line == "child").count(), 1);
    let events = scope.event_queue().take_pending().0;
    let mut finished = events
        .into_iter()
        .filter_map(|event| match event {
            AgentEvent::TaskFinished {
                task_id,
                terminal_id: Some(_),
                ..
            } => Some(task_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    finished.sort();
    assert_eq!(finished, vec!["child", "parent"]);
}
