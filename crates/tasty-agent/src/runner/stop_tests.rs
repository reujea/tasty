//! 정지 요청 뒤에는 같은 tick 안에서도 남은 Ready 작업을 보내지 않는다.

use std::path::PathBuf;

use super::*;
use crate::task::{OnFailure, TaskCommand};

/// dispatch 하나가 끝날 때 정지가 요청되는 실행기. 호스트 응답을 기다리는 동안 정지가 들어온 경우다.
#[derive(Default)]
struct StopsAfterFirst {
    dispatched: Vec<TaskId>,
    stop: bool,
}

impl TaskExecutor for StopsAfterFirst {
    fn dispatch(&mut self, task: &Task) -> DispatchOutcome {
        self.dispatched.push(task.id.clone());
        self.stop = true;
        DispatchOutcome::Deferred
    }
    fn poll(&mut self, _h: &DispatchHandle) -> PollOutcome {
        PollOutcome::Active
    }
    fn stop_requested(&self) -> bool {
        self.stop
    }
}

fn ready(id: &str) -> Task {
    Task {
        id: id.into(),
        workspace_id: 1,
        name: id.into(),
        command: TaskCommand::Run {
            command: vec!["true".into()],
            workspace_id: 1,
            cwd: None as Option<PathBuf>,
        },
        depends_on: vec![],
        state: TaskState::Ready,
        created_at: 0,
        started_at: None,
        finished_at: None,
        result: None,
        on_failure: OnFailure::default(),
        metadata: serde_json::Value::Null,
        reserved_for_fallback: false,
        contract: None,
        typed_result: None,
        graph_id: None,
        input_snapshot: None,
        accepted: None,
        report_token: None,
        attempt: None,
        route: None,
        skip: None,
    }
}

#[test]
fn a_stop_requested_during_a_dispatch_leaves_the_remaining_ready_tasks_unsent() {
    let mut runner = RunnerLoop::new(StopsAfterFirst::default());
    let tasks: Vec<Task> = (0..5).map(|i| ready(&format!("t-{i}"))).collect();
    runner.tick(1, 10, &tasks, |_, _, _, _| Ok(()), |_, _, _, _| Ok(()));
    assert_eq!(runner.executor.dispatched, vec!["t-0".to_string()]);
}

#[test]
fn a_stop_requested_before_the_tick_sends_nothing() {
    let mut runner = RunnerLoop::new(StopsAfterFirst {
        stop: true,
        ..Default::default()
    });
    let tasks: Vec<Task> = (0..3).map(|i| ready(&format!("t-{i}"))).collect();
    runner.tick(1, 10, &tasks, |_, _, _, _| Ok(()), |_, _, _, _| Ok(()));
    assert!(runner.executor.dispatched.is_empty());
}
