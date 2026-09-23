//! `task` 단위 테스트.

use super::*;
use crate::AgentError;
use std::sync::atomic::AtomicU64;
use tasty_memory::MemoryStore;
use tempfile::TempDir;

fn fresh_store() -> (TempDir, MemoryStore, AtomicU64) {
    let td = tempfile::tempdir().expect("tempdir");
    let mem = MemoryStore::open(&td.path().join("mem.db")).expect("mem");
    (td, mem, AtomicU64::new(0))
}

fn run_cmd() -> TaskCommand {
    TaskCommand::Run {
        command: vec!["echo".into(), "hi".into()],
        workspace_id: 1,
        cwd: None,
    }
}

fn reduce_cmd(inputs: Vec<TaskId>) -> TaskCommand {
    TaskCommand::Reduce {
        inputs,
        strategy: ReducerStrategy::All,
    }
}

#[test]
fn create_single_task_starts_ready() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let t = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "a".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    assert_eq!(t.state, TaskState::Ready);
    assert_eq!(t.workspace_id, 1);
}

#[test]
fn linear_dag_transitions_downstream() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![b.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    assert_eq!(a.state, TaskState::Ready);
    assert_eq!(b.state, TaskState::Waiting);
    assert_eq!(c.state, TaskState::Waiting);

    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    let (_, cascaded) = store
        .set_state(1, &a.id, TaskState::Succeeded, 3000)
        .unwrap();
    // B should be Ready now
    let b_after = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_after.state, TaskState::Ready);
    // C still Waiting
    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(c_after.state, TaskState::Waiting);
    assert!(cascaded.iter().any(|t| t.id == b.id));
}

#[test]
fn diamond_dag_parallel_then_join() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    let d = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "D".to_string(),
            command: run_cmd(),
            depends_on: vec![b.id.clone(), c.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1003,
        })
        .unwrap();

    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(1, &a.id, TaskState::Succeeded, 3000)
        .unwrap();
    assert_eq!(
        store.get(1, &b.id).unwrap().unwrap().state,
        TaskState::Ready
    );
    assert_eq!(
        store.get(1, &c.id).unwrap().unwrap().state,
        TaskState::Ready
    );
    assert_eq!(
        store.get(1, &d.id).unwrap().unwrap().state,
        TaskState::Waiting
    );

    // B done
    store.set_state(1, &b.id, TaskState::Running, 4000).unwrap();
    store
        .set_state(1, &b.id, TaskState::Succeeded, 5000)
        .unwrap();
    assert_eq!(
        store.get(1, &d.id).unwrap().unwrap().state,
        TaskState::Waiting
    );

    // C done → D ready
    store.set_state(1, &c.id, TaskState::Running, 6000).unwrap();
    store
        .set_state(1, &c.id, TaskState::Succeeded, 7000)
        .unwrap();
    assert_eq!(
        store.get(1, &d.id).unwrap().unwrap().state,
        TaskState::Ready
    );
}

#[test]
fn abort_propagates_skip() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    let b_after = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_after.state, TaskState::Skipped);
}

#[test]
fn continue_downstream_keeps_going() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::ContinueDownstream,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(1, &a.id, TaskState::Failed { error: "x".into() }, 3000)
        .unwrap();
    let b_after = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_after.state, TaskState::Ready);
}

#[test]
fn cycle_detected() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    // 직접 사이클을 만들 수 없으니, 내부 함수로 시도.
    // A -> B (created with dep A) ; 만약 A를 update해서 dep=B 추가하면 cycle.
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    // create 시 unknown dep는 거부
    let err = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "X".to_string(),
            command: run_cmd(),
            depends_on: vec!["nonexistent".into()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap_err();
    assert!(matches!(err, AgentError::UnknownDependency(_)));

    // 영속을 우회해 강제로 cycle 만든 뒤 detect
    let mut a_mut = a.clone();
    a_mut.depends_on = vec![b.id.clone()];
    store.put(&a_mut).unwrap();
    let all = store.list(1).unwrap();
    let graph = TaskGraph::build(&all);
    let err = graph.detect_cycles().unwrap_err();
    assert!(matches!(err, AgentError::DependencyCycle(_)));
}

#[test]
fn cancel_pending_task() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let (cancelled, _) = store.cancel(1, &a.id, 1500).unwrap();
    assert_eq!(cancelled.state, TaskState::Cancelled);
    assert_eq!(cancelled.finished_at, Some(1500));
}

#[test]
fn retry_resets_state() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(1, &a.id, TaskState::Failed { error: "e".into() }, 3000)
        .unwrap();
    let retried = store.retry(1, &a.id, false, 4000).unwrap();
    assert_eq!(retried.state, TaskState::Ready);
    assert!(retried.finished_at.is_none());
}

#[test]
fn retry_with_reset_downstream() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(1, &a.id, TaskState::Failed { error: "e".into() }, 3000)
        .unwrap();
    // B는 Skipped
    assert_eq!(
        store.get(1, &b.id).unwrap().unwrap().state,
        TaskState::Skipped
    );

    store.retry(1, &a.id, true, 4000).unwrap();
    // B는 Waiting으로 돌아오고 cascade 후 A가 Ready라서 B는 Waiting 유지 (A 미완료)
    let b_after = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_after.state, TaskState::Waiting);
}

/// cascade 로 `Skipped` 전이된 downstream 도 `set_state`의 terminal 타임스탬프 기록과
/// 동일하게 `finished_at`을 받아야 한다 — cascade 는 `set_state`를 거치지 않는 별도
/// 직접-put 경로이기 때문에 별도로 채워야 한다.
#[test]
fn cascade_skip_sets_finished_at() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    let b_after = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_after.state, TaskState::Skipped);
    assert_eq!(
        b_after.finished_at,
        Some(3000),
        "cascade 로 skip 된 downstream 도 upstream 이 종결된 시각을 finished_at 으로 받아야 한다"
    );
    assert!(b_after.started_at.is_none());
}

/// `task-retry --reset-downstream` 로 되돌린 뒤(finished_at 이 지워짐) upstream 이 다시
/// 실패해 downstream 이 재-skip 되면, `finished_at` 이 새 시각으로 갱신되어야 한다 — 과거
/// skip 시각이 그대로 남아 있으면 안 된다.
#[test]
fn retry_reset_downstream_then_reskip_refreshes_finished_at() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(1, &a.id, TaskState::Failed { error: "e1".into() }, 3000)
        .unwrap();
    let b_first_skip = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_first_skip.state, TaskState::Skipped);
    assert_eq!(b_first_skip.finished_at, Some(3000));

    // reset — B 는 Waiting 으로, finished_at 도 지워진다.
    store.retry(1, &a.id, true, 4000).unwrap();
    let b_reset = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_reset.state, TaskState::Waiting);
    assert!(b_reset.finished_at.is_none());

    // A 다시 실패 → B 재-skip, finished_at 이 새 시각(6000)으로 갱신되어야 함(3000 이 아니라).
    store.set_state(1, &a.id, TaskState::Running, 5000).unwrap();
    store
        .set_state(1, &a.id, TaskState::Failed { error: "e2".into() }, 6000)
        .unwrap();
    let b_reskip = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_reskip.state, TaskState::Skipped);
    assert_eq!(b_reskip.finished_at, Some(6000));
}

/// `plan_sweep`(→ `task-purge --older-than-ms`)의 나이 판정은 skip 시각(`finished_at`)
/// 기준이어야 한다 — 오래전에 생성됐지만 방금 skip 된 task 가 생성 시각 기준으로 오판돼
/// 즉시 지워지면 안 된다.
#[test]
fn plan_sweep_ages_skipped_task_from_finish_time_not_creation_time() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    // B 는 생성된 지 오래(now_ms=1000)지만, A 가 한참 뒤(50_000)에야 실패해 그때 skip된다.
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "late".into(),
            },
            50_000,
        )
        .unwrap();
    let b_after = store.get(1, &b.id).unwrap().unwrap();
    assert_eq!(b_after.state, TaskState::Skipped);
    assert_eq!(b_after.finished_at, Some(50_000));

    // now_ms=55_000, older_than_ms=10_000:
    // - 생성 시각(1000) 기준이면 age=54_000 >= 10_000 → 후보(버그).
    // - skip 시각(50_000) 기준이면 age=5_000 < 10_000 → 후보 아님(수정 후 정답).
    let filter = TaskPurgeFilter {
        states: Some(vec!["skipped".to_string()]),
        older_than_ms: Some(10_000),
        now_ms: 55_000,
    };
    let plan = store.plan_sweep(1, &filter).unwrap();
    assert!(
        plan.deleted.is_empty() && plan.retained.is_empty(),
        "방금 skip 된 task 가 생성 시각 기준으로 오판돼 즉시 purge 후보가 되면 안 된다: {plan:?}"
    );
}

/// `succeeded`/`failed` 는 원래부터 `set_state` 를 거쳐 `finished_at` 이 채워지던 경로다 —
/// cascade/retry 쪽을 고치면서 이 기존 동작에 회귀가 없는지 락인.
#[test]
fn succeeded_and_failed_still_set_finished_at_via_set_state() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    let (succeeded, _) = store
        .set_state(1, &a.id, TaskState::Succeeded, 3000)
        .unwrap();
    assert_eq!(succeeded.finished_at, Some(3000));

    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    store.set_state(1, &b.id, TaskState::Running, 2000).unwrap();
    let (failed, _) = store
        .set_state(
            1,
            &b.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3500,
        )
        .unwrap();
    assert_eq!(failed.finished_at, Some(3500));
}

#[test]
fn fallback_triggers_when_main_fails() {
    // A -> C (dep). A.on_failure = Fallback{A'}. A 실패 시 A' 가 자동 Ready.
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a_prime = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A_prime".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(a_prime.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    // A_prime 은 A 가 아직 실패하기 전까지 dormant — depends_on 이 비어 있어도
    // Ready 로 뜨지 않는다.
    let a_prime_dormant = store.get(1, &a_prime.id).unwrap().unwrap();
    assert_eq!(a_prime_dormant.state, TaskState::Waiting);
    // A 실패 시 A_prime 이 비로소 Ready 로 승격, C 는 Waiting 유지 (fallback 대기).
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(c_after.state, TaskState::Waiting, "C 는 fallback 대기");
    let a_prime_after = store.get(1, &a_prime.id).unwrap().unwrap();
    assert_eq!(a_prime_after.state, TaskState::Ready);
}

#[test]
fn fallback_success_propagates_to_main_downstream() {
    // A.on_failure=Fallback{A'}, C depends on A. A 실패 → A' Succeed → C Ready.
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a_prime = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A_prime".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(a_prime.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    store
        .set_state(1, &a_prime.id, TaskState::Running, 3500)
        .unwrap();
    store
        .set_state(1, &a_prime.id, TaskState::Succeeded, 4000)
        .unwrap();
    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(c_after.state, TaskState::Ready, "fallback 성공으로 C 진행");
}

#[test]
fn fallback_failure_also_skips_main_downstream() {
    // A.on_failure=Fallback{A'}, C depends on A. A 실패 → A' 도 실패 → C Skipped.
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a_prime = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A_prime".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(a_prime.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    store
        .set_state(1, &a_prime.id, TaskState::Running, 3500)
        .unwrap();
    store
        .set_state(
            1,
            &a_prime.id,
            TaskState::Failed {
                error: "boom2".into(),
            },
            4000,
        )
        .unwrap();
    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(
        c_after.state,
        TaskState::Skipped,
        "fallback 도 실패면 C 도 Skip"
    );
}

/// main 이 성공하면 그 fallback(existing task 참조)은 한 번도 dispatch 대상
/// (`Ready`)이 되지 않아야 한다 — "실패했을 때만 도는 대체 경로" 계약. 성공
/// 직후 fallback 은 `Waiting` 에 영구 잔류하지 않고 `Skipped` 로 마감된다
/// (다시는 main 이 Failed 될 일이 없으므로).
#[test]
fn fallback_task_never_runs_when_main_succeeds() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let fb = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "fallback".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    // fallback 자신을 만드는 시점엔 아직 아무 main 도 이걸 참조하지 않으므로
    // (Fallback{task} 대상은 반드시 main 보다 먼저 존재해야 한다), 의존성이
    // 없으면 이 시점엔 정상적으로 Ready 다 — dormant 판정은 참조하는 main 이
    // 생긴 "다음" 부터 걸린다.
    assert_eq!(fb.state, TaskState::Ready);
    let main = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "main".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(fb.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    // main 을 참조하는 순간에도(생성 시점 소급 정정 포함) fallback 은 그대로 dormant.
    let fb_after_main_create = store.get(1, &fb.id).unwrap().unwrap();
    assert_eq!(fb_after_main_create.state, TaskState::Waiting);

    store
        .set_state(1, &main.id, TaskState::Running, 2000)
        .unwrap();
    store
        .set_state(1, &main.id, TaskState::Succeeded, 3000)
        .unwrap();

    let fb_final = store.get(1, &fb.id).unwrap().unwrap();
    assert_eq!(
        fb_final.state,
        TaskState::Skipped,
        "main 성공으로 다시는 승격될 일이 없는 fallback 은 waiting 에 잔류하지 \
         않고 skipped 로 마감돼야 한다"
    );
}

/// main 이 실패하면 fallback 이 dormant(`Waiting`) → `Ready` → (러너가 dispatch
/// 해) `Running` → `Succeeded` 로 진행하고, main 에 의존하던 downstream 도 그
/// 결과를 받아 정상 진행해야 한다.
#[test]
fn fallback_ready_then_run_succeeded_and_downstream_proceeds() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let fb = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "fallback".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let main = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "main".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(fb.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let downstream = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "downstream".to_string(),
            command: run_cmd(),
            depends_on: vec![main.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();

    store
        .set_state(1, &main.id, TaskState::Running, 2000)
        .unwrap();
    store
        .set_state(
            1,
            &main.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    let fb_ready = store.get(1, &fb.id).unwrap().unwrap();
    assert_eq!(
        fb_ready.state,
        TaskState::Ready,
        "main 실패로 fallback 승격"
    );
    let downstream_waiting = store.get(1, &downstream.id).unwrap().unwrap();
    assert_eq!(
        downstream_waiting.state,
        TaskState::Waiting,
        "fallback 결과가 나오기 전까지 downstream 은 대기"
    );

    store
        .set_state(1, &fb.id, TaskState::Running, 3500)
        .unwrap();
    store
        .set_state(1, &fb.id, TaskState::Succeeded, 4000)
        .unwrap();

    let downstream_after = store.get(1, &downstream.id).unwrap().unwrap();
    assert_eq!(
        downstream_after.state,
        TaskState::Ready,
        "fallback 성공으로 main 에 의존하던 downstream 이 정상 진행"
    );
}

/// main 이 사용자에 의해 `Cancelled` 로 끝나면(실행 한번 못 해보고 취소되는
/// 경우 포함) 다시는 `Failed` 로 전이할 일이 없다 — 그 fallback 을 `Waiting`
/// 에 방치하면 안 된다.
#[test]
fn fallback_finalized_skipped_when_main_cancelled() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let fb = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "fallback".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let main = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "main".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(fb.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    store.cancel(1, &main.id, 2000).unwrap();

    let fb_after = store.get(1, &fb.id).unwrap().unwrap();
    assert_eq!(
        fb_after.state,
        TaskState::Skipped,
        "main 이 cancelled 로 끝나 다시 실패할 일이 없는 fallback 은 waiting \
         에 영구 잔류하지 않아야 한다"
    );
}

/// main 자신이 `Skipped` 로 끝나는 경우도 마찬가지로 그 fallback 이 waiting 에
/// 영구 잔류하면 안 된다. `Fallback` 이 설정된 task 는 자기 의존성이 실패해도
/// (기존 설계상) 직접 Skipped 로 떨어지지 않으므로(`apply_on_failure` 가
/// `Fallback` 에는 `None` 을 반환 — downstream 쪽 설정 오용 케이스), 이 상태를
/// 실제로 관찰하려면 main 자신이 *다른 main* 의 dormant fallback 이어서 그
/// 상위 main 이 성공/취소로 끝나 Skipped 로 마감되는 체인을 구성해야 한다 —
/// 그 체인이 main 자신의 fallback 정리 로직까지 재귀적으로 타는지 함께 검증한다.
#[test]
fn fallback_finalized_skipped_when_main_ends_skipped_via_chained_fallback() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    // grandparent 가 성공하면 main(= grandparent 의 fallback 대상) 은 다시는
    // 깨어나지 않으므로 Skipped 로 마감된다.
    let leaf_fb = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "leaf_fallback".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let main = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "main".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(leaf_fb.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let grandparent = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "grandparent".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(main.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();

    store
        .set_state(1, &grandparent.id, TaskState::Running, 2000)
        .unwrap();
    store
        .set_state(1, &grandparent.id, TaskState::Succeeded, 3000)
        .unwrap();

    let main_after = store.get(1, &main.id).unwrap().unwrap();
    assert_eq!(
        main_after.state,
        TaskState::Skipped,
        "grandparent 성공으로 main 은 다시 깨어날 일이 없다"
    );
    let leaf_fb_after = store.get(1, &leaf_fb.id).unwrap().unwrap();
    assert_eq!(
        leaf_fb_after.state,
        TaskState::Skipped,
        "main 이 skipped 로 마감되면 그 자신의 fallback 도 연쇄로 마감돼야 \
         waiting 에 영구 잔류하지 않는다"
    );
}

#[test]
fn list_returns_all_tasks() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store
        .create(TaskCreateOpts {
            workspace_id: 2,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    let ws1 = store.list(1).unwrap();
    assert_eq!(ws1.len(), 2);
    let ws2 = store.list(2).unwrap();
    assert_eq!(ws2.len(), 1);
}

#[test]
fn fallback_inline_materializes_on_failed_transition() {
    // A.on_failure=Fallback{inline:{A_prime}}, C depends on A.
    // A 실패 → A_prime task 자동 생성 (metadata.fallback_of=A.id) + Ready.
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: None,
                inline: Some(Box::new(InlineFallbackSpec {
                    name: "A_prime".into(),
                    command: run_cmd(),
                    depends_on_override: None,
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::json!({}),
                })),
            },
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".to_string(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();

    // A_prime 가 생성되었고 Ready 인지 확인.
    let all = store.list(1).unwrap();
    let a_prime = all
        .iter()
        .find(|t| t.metadata.get("fallback_of").and_then(|v| v.as_str()) == Some(a.id.as_str()))
        .expect("inline fallback materialized");
    assert_eq!(a_prime.name, "A_prime");
    assert_eq!(a_prime.state, TaskState::Ready);
    // C 는 여전히 Waiting — A_prime 의 결과 대기.
    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(c_after.state, TaskState::Waiting);

    // A_prime Succeed → C Ready.
    let a_prime_id = a_prime.id.clone();
    store
        .set_state(1, &a_prime_id, TaskState::Running, 3500)
        .unwrap();
    store
        .set_state(1, &a_prime_id, TaskState::Succeeded, 4000)
        .unwrap();
    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(c_after.state, TaskState::Ready, "fallback 성공 → C Ready");
}

#[test]
fn fallback_inline_idempotent_on_repeated_failed_calls() {
    // 같은 main 의 Failed 분기를 *여러 번* 호출해도 inline fallback task 는 1개만.
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".to_string(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: None,
                inline: Some(Box::new(InlineFallbackSpec {
                    name: "A_prime".into(),
                    command: run_cmd(),
                    depends_on_override: None,
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::json!({}),
                })),
            },
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();

    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();

    let count_first = store
        .list(1)
        .unwrap()
        .iter()
        .filter(|t| t.metadata.get("fallback_of").and_then(|v| v.as_str()) == Some(a.id.as_str()))
        .count();
    assert_eq!(count_first, 1);

    // retry 후 다시 Failed — fallback 이 중복 생성되면 안 됨.
    // 단 retry 는 task state 를 Waiting → Ready 로 보내므로, Failed 후 retry → Failed
    // 사이클을 시뮬레이션.
    store.retry(1, &a.id, false, 3100).unwrap();
    store.set_state(1, &a.id, TaskState::Running, 3200).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom2".into(),
            },
            3300,
        )
        .unwrap();
    let count_second = store
        .list(1)
        .unwrap()
        .iter()
        .filter(|t| t.metadata.get("fallback_of").and_then(|v| v.as_str()) == Some(a.id.as_str()))
        .count();
    assert_eq!(count_second, 1, "inline fallback must not duplicate");
}

#[test]
fn fallback_validation_rejects_both_task_and_inline() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    // 먼저 어떤 task 라도 만들어 task id 확보.
    let helper = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "helper".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let err = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "bad".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(helper.id),
                inline: Some(Box::new(InlineFallbackSpec {
                    name: "x".into(),
                    command: run_cmd(),
                    depends_on_override: None,
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::json!({}),
                })),
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap_err();
    assert!(
        matches!(err, AgentError::InvalidArgument(_)),
        "expected InvalidArgument, got {err:?}"
    );
}

#[test]
fn fallback_validation_rejects_neither_task_nor_inline() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let err = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "bad".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: None,
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap_err();
    assert!(
        matches!(err, AgentError::InvalidArgument(_)),
        "expected InvalidArgument, got {err:?}"
    );
}

// ============================================================
// 회귀: Fallback.task / Reduce.inputs 참조 검증
// ============================================================

#[test]
fn create_rejects_unknown_reduce_input() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let err = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "R".into(),
            command: reduce_cmd(vec!["t-does-not-exist".into()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap_err();
    assert!(
        matches!(err, AgentError::UnknownDependency(ref id) if id == "t-does-not-exist"),
        "expected UnknownDependency, got {err:?}"
    );
}

#[test]
fn create_rejects_unknown_fallback_task() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let err = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some("t-does-not-exist".into()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap_err();
    assert!(
        matches!(err, AgentError::UnknownDependency(ref id) if id == "t-does-not-exist"),
        "expected UnknownDependency, got {err:?}"
    );
}

#[test]
fn fallback_inline_is_not_rejected_as_unknown_target() {
    // inline fallback 은 생성 시점엔 아직 존재하지 않는 게 정상 — task 존재
    // 검증 대상이 아니어야 한다 (fallback_validation_rejects_* 와는 다른 축).
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let t = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: None,
                inline: Some(Box::new(InlineFallbackSpec {
                    name: "A_prime".into(),
                    command: run_cmd(),
                    depends_on_override: None,
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::json!({}),
                })),
            },
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    assert_eq!(t.state, TaskState::Ready);
}

/// 결정 1 (가장 중요): `Reduce.inputs` 가 암묵적 의존성으로 승격되어, 입력이
/// 전부 종결되기 전에는 Reduce 가 `Ready` 로 올라가지 않는다. 승격 이전에는
/// `depends_on` 없는 Reduce 가 생성 즉시 `Ready` → dispatch 되어 미완 입력을
/// `Null` 로 조용히 수집하고 `Succeeded` 로 마감했다 (현상 §3, 조용한 오답).
#[test]
fn reduce_waits_for_inputs_to_terminate_before_ready() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A (slow)".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    // depends_on 을 일부러 비워둔다 — 승격 전에는 이 경로가 즉시 Ready 였다.
    let r = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "R".into(),
            command: reduce_cmd(vec![a.id.clone()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    assert_eq!(
        r.state,
        TaskState::Waiting,
        "A 가 아직 미완이므로 R 은 생성 즉시 Ready 가 되면 안 된다"
    );

    // A 가 여전히 실행 중이어도 R 은 계속 Waiting.
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    let r_mid = store.get(1, &r.id).unwrap().unwrap();
    assert_eq!(r_mid.state, TaskState::Waiting);

    // A 가 종결(Succeeded)되어야 R 이 Ready.
    let (_, cascaded) = store
        .set_state(1, &a.id, TaskState::Succeeded, 3000)
        .unwrap();
    let r_after = store.get(1, &r.id).unwrap().unwrap();
    assert_eq!(r_after.state, TaskState::Ready);
    assert!(cascaded.iter().any(|t| t.id == r.id));
}

/// Reducer(특히 `all`)는 실패한 입력도 의도적으로 수집하는 계약이므로, 입력이
/// 실패했다고 Reduce 를 `depends_on` 처럼 `Skipped` 로 몰면 안 된다 — 종결
/// 되었으면 (성공이든 실패든) `Ready` 로 진행되어야 dispatch 가 실제로
/// 그 실패 결과를 합성할 수 있다.
#[test]
fn reduce_becomes_ready_after_failed_input_not_skipped() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let r = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "R".into(),
            command: reduce_cmd(vec![a.id.clone()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();
    let r_after = store.get(1, &r.id).unwrap().unwrap();
    assert_eq!(
        r_after.state,
        TaskState::Ready,
        "실패한 입력도 종결이므로 Reduce 는 Ready — Skipped 로 몰지 않는다"
    );
}

#[test]
fn reduce_input_cycle_via_implicit_edge_is_detected() {
    // A -> R (Reduce.inputs 로 R 이 A 에 의존). 이후 A 에 depends_on=[R] 을
    // 강제로 주입(create 검증 우회, cycle_detected 테스트와 동일 기법)하면
    // A -> R -> A 사이클이 만들어진다. depends_on 만 엣지였다면 못 잡는다.
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let r = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "R".into(),
            command: reduce_cmd(vec![a.id.clone()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    let mut a_mut = a.clone();
    a_mut.depends_on = vec![r.id.clone()];
    store.put(&a_mut).unwrap();
    let all = store.list(1).unwrap();
    let err = TaskGraph::build(&all).detect_cycles().unwrap_err();
    assert!(matches!(err, AgentError::DependencyCycle(_)));
}

/// 결정 3: 본 검증 도입 이전에 저장된 dangling `Fallback.task` 참조는
/// 마이그레이션하지 않는다 — `create()` 우회로 그 상태를 재현하고, main 이
/// 실패해도 패닉하지 않고 downstream 이 영구 `Waiting` 으로 남는지 확인한다.
#[test]
fn legacy_dangling_fallback_target_leaves_downstream_waiting() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".into(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    // create() 를 우회해 dangling fallback 참조를 강제로 주입 (레거시 데이터 재현).
    let mut a_mut = a.clone();
    a_mut.on_failure = OnFailure::Fallback {
        task: Some("t-does-not-exist".into()),
        inline: None,
    };
    store.put(&a_mut).unwrap();

    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();
    let (_, _cascaded) = store
        .set_state(
            1,
            &a.id,
            TaskState::Failed {
                error: "boom".into(),
            },
            3000,
        )
        .unwrap();

    let c_after = store.get(1, &c.id).unwrap().unwrap();
    assert_eq!(
        c_after.state,
        TaskState::Waiting,
        "dangling fallback 은 관측 가능한 경고만 남기고 downstream 은 영구 Waiting"
    );
}

// ============================================================
// delete_checked / plan_sweep / apply_sweep_plan
// ============================================================

#[test]
fn delete_checked_rejects_when_referenced_by_depends_on() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".into(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    let err = store
        .delete_checked(1, &a.id, TaskDeleteOpts::default())
        .unwrap_err();
    match err {
        AgentError::TaskReferenced {
            task,
            referenced_by,
        } => {
            assert_eq!(task, a.id);
            assert_eq!(referenced_by, vec![b.id.clone()]);
        }
        other => panic!("expected TaskReferenced, got {other:?}"),
    }
    // 거부됐으니 A 는 여전히 존재해야 한다.
    assert!(store.get(1, &a.id).unwrap().is_some());
}

#[test]
fn delete_checked_rejects_when_referenced_by_fallback_task() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let fb = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "FB".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let main = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "MAIN".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Fallback {
                task: Some(fb.id.clone()),
                inline: None,
            },
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    let err = store
        .delete_checked(1, &fb.id, TaskDeleteOpts::default())
        .unwrap_err();
    match err {
        AgentError::TaskReferenced {
            task,
            referenced_by,
        } => {
            assert_eq!(task, fb.id);
            assert_eq!(referenced_by, vec![main.id.clone()]);
        }
        other => panic!("expected TaskReferenced, got {other:?}"),
    }
}

#[test]
fn delete_checked_rejects_when_referenced_by_reduce_inputs() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let input = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "IN".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let reducer = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "REDUCE".into(),
            command: reduce_cmd(vec![input.id.clone()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    let err = store
        .delete_checked(1, &input.id, TaskDeleteOpts::default())
        .unwrap_err();
    match err {
        AgentError::TaskReferenced {
            task,
            referenced_by,
        } => {
            assert_eq!(task, input.id);
            assert_eq!(referenced_by, vec![reducer.id.clone()]);
        }
        other => panic!("expected TaskReferenced, got {other:?}"),
    }
}

/// 가장 중요한 시나리오: 참조 있는 task 를 cascade 삭제하면 참조자까지 함께
/// 지워지고, 그 뒤 같은 workspace 에 새 task 를 만드는 게 여전히 성공한다 —
/// raw `delete()` 를 그대로 노출했다면 dangling
/// `depends_on` 이 남아 이후 모든 `create()` 가 `detect_cycles` 의
/// `UnknownDependency` 로 깨졌을 것.
#[test]
fn cascade_delete_removes_referencers_and_workspace_stays_creatable() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".into(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    let report = store
        .delete_checked(
            1,
            &a.id,
            TaskDeleteOpts {
                cascade: true,
                force: false,
            },
        )
        .unwrap();
    assert_eq!(report.deleted.len(), 2);
    assert!(report.deleted.contains(&a.id));
    assert!(report.deleted.contains(&b.id));
    assert!(store.get(1, &a.id).unwrap().is_none());
    assert!(store.get(1, &b.id).unwrap().is_none());

    // scenario 2: B 가 영구 Waiting 으로 남지 않는다 — 아예 존재하지 않는다.
    // scenario 1 뒷부분: 이후 create() 가 여전히 정상 동작.
    let c = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "C".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 2000,
        })
        .unwrap();
    assert_eq!(c.state, TaskState::Ready);
}

#[test]
fn delete_checked_force_bypasses_reference_check_but_not_running() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".into(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();

    let report = store
        .delete_checked(
            1,
            &a.id,
            TaskDeleteOpts {
                cascade: false,
                force: true,
            },
        )
        .unwrap();
    assert_eq!(report.deleted, vec![a.id.clone()]);
    assert!(store.get(1, &a.id).unwrap().is_none());
    // force 는 상태 제약은 못 뚫는다 — 남은 B 는 dangling 참조 위에서 그냥
    // 영구 Waiting 으로 남는다(사용자가 명시적으로 감수한 결과).
    assert_eq!(
        store.get(1, &b.id).unwrap().unwrap().state,
        TaskState::Waiting
    );
}

#[test]
fn delete_checked_rejects_running_regardless_of_cascade_or_force() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    store.set_state(1, &a.id, TaskState::Running, 2000).unwrap();

    for opts in [
        TaskDeleteOpts::default(),
        TaskDeleteOpts {
            cascade: false,
            force: true,
        },
        TaskDeleteOpts {
            cascade: true,
            force: false,
        },
    ] {
        let err = store.delete_checked(1, &a.id, opts).unwrap_err();
        assert!(
            matches!(err, AgentError::TaskRunning(ref id) if *id == a.id),
            "expected TaskRunning, got {err:?}"
        );
    }
    assert!(store.get(1, &a.id).unwrap().is_some());
}

#[test]
fn delete_checked_allows_waiting_ready_and_terminal_states() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);

    // Ready (no deps, no referencers).
    let ready = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "READY".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    assert_eq!(ready.state, TaskState::Ready);
    store
        .delete_checked(1, &ready.id, TaskDeleteOpts::default())
        .unwrap();

    // Waiting (Reduce 대기 중 — 아직 input 이 terminal 아님).
    let input = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "IN".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    let waiting = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "WAIT".into(),
            command: reduce_cmd(vec![input.id.clone()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1002,
        })
        .unwrap();
    assert_eq!(waiting.state, TaskState::Waiting);
    store
        .delete_checked(1, &waiting.id, TaskDeleteOpts::default())
        .unwrap();

    // Succeeded (terminal).
    let term = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "TERM".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1003,
        })
        .unwrap();
    store
        .set_state(1, &term.id, TaskState::Running, 2000)
        .unwrap();
    store
        .set_state(1, &term.id, TaskState::Succeeded, 3000)
        .unwrap();
    store
        .delete_checked(1, &term.id, TaskDeleteOpts::default())
        .unwrap();

    assert!(store.get(1, &ready.id).unwrap().is_none());
    assert!(store.get(1, &waiting.id).unwrap().is_none());
    assert!(store.get(1, &term.id).unwrap().is_none());
}

/// 자동 GC 가 완전히 얽힌 `Waiting` 그래프를
/// 실제로 드레인하는지 — 방치된 `Reduce` (X) 가 그 input(Y) 을 참조로 붙잡고
/// 있어도, `Waiting` 은 금지 상태가 아니므로(결정 2) 후보 집합 안에서 둘 다
/// 함께 지워져야 한다(terminal 로 제한했다면 영원히 못 지웠을 그래프).
#[test]
fn plan_sweep_drains_entangled_waiting_reduce_graph() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let y = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "Y".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    assert_eq!(y.state, TaskState::Ready);
    let x = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "X".into(),
            command: reduce_cmd(vec![y.id.clone()]),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    assert_eq!(
        x.state,
        TaskState::Waiting,
        "Y 가 terminal 이 아니라 X 는 영원히 Waiting — 방치된 Reduce 재현"
    );

    let filter = TaskPurgeFilter {
        states: None,
        older_than_ms: Some(1000),
        now_ms: 50_000,
    };
    let plan = store.plan_sweep(1, &filter).unwrap();
    assert!(plan.retained.is_empty(), "retained: {:?}", plan.retained);
    assert!(plan.deleted.contains(&x.id));
    assert!(plan.deleted.contains(&y.id));

    store.apply_sweep_plan(1, &plan).unwrap();
    assert!(store.get(1, &x.id).unwrap().is_none());
    assert!(store.get(1, &y.id).unwrap().is_none());
}

#[test]
fn plan_sweep_retains_task_referenced_from_outside_candidate_set() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let a = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "A".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    // B 는 A 에 의존하지만 최근에 생성됨(older_than_ms 필터에 안 걸림) → 후보
    // 집합 밖에서 A 를 참조하는 상황을 만든다.
    let _b = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "B".into(),
            command: run_cmd(),
            depends_on: vec![a.id.clone()],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 49_500,
        })
        .unwrap();

    let filter = TaskPurgeFilter {
        states: None,
        older_than_ms: Some(10_000),
        now_ms: 50_000,
    };
    let plan = store.plan_sweep(1, &filter).unwrap();
    assert_eq!(plan.deleted, Vec::<TaskId>::new());
    assert_eq!(plan.retained, vec![a.id.clone()]);

    store.apply_sweep_plan(1, &plan).unwrap();
    assert!(store.get(1, &a.id).unwrap().is_some());
}

#[test]
fn plan_sweep_filters_by_state_name() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "_host", &seq);
    let ready = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "READY".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1000,
        })
        .unwrap();
    let term = store
        .create(TaskCreateOpts {
            workspace_id: 1,
            name: "TERM".into(),
            command: run_cmd(),
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
            now_ms: 1001,
        })
        .unwrap();
    store
        .set_state(1, &term.id, TaskState::Running, 2000)
        .unwrap();
    store
        .set_state(1, &term.id, TaskState::Succeeded, 3000)
        .unwrap();

    let filter = TaskPurgeFilter {
        states: Some(vec!["succeeded".to_string()]),
        older_than_ms: None,
        now_ms: 50_000,
    };
    let plan = store.plan_sweep(1, &filter).unwrap();
    assert_eq!(plan.deleted, vec![term.id.clone()]);
    assert!(!plan.deleted.contains(&ready.id));
}

// ============================================================
// DAG 그룹핑 (`task/dag.rs`)
// ============================================================

/// 그룹핑은 순수 함수라 store 를 거치지 않는다 — `Task` 를 직접 조립한다.
fn dag_task(id: &str, depends_on: &[&str], created_at: u64) -> Task {
    Task {
        id: id.to_string(),
        workspace_id: 1,
        name: id.to_string(),
        command: run_cmd(),
        depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
        state: TaskState::Waiting,
        created_at,
        started_at: None,
        finished_at: None,
        result: None,
        on_failure: OnFailure::Abort,
        metadata: serde_json::Value::Null,
        reserved_for_fallback: false,
    }
}

fn two_disconnected_pairs() -> Vec<Task> {
    vec![
        dag_task("t-a", &[], 1000),
        dag_task("t-b", &["t-a"], 1001),
        dag_task("t-c", &[], 1002),
        dag_task("t-d", &["t-c"], 1003),
    ]
}

#[test]
fn groups_disconnected_graphs_into_separate_dags() {
    let dags = group_tasks_into_dags(&two_disconnected_pairs());
    assert_eq!(dags.len(), 2);
    assert!(dags.iter().all(|d| d.task_count == 2));
    assert!(dags.iter().all(|d| d.source == "derived"));
    assert!(dags.iter().all(|d| !d.has_cycle));
    // derived id 는 그룹 내 `(created_at, id)` 최소 task 에서 나온다.
    let ids: Vec<&str> = dags.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, vec!["c:t-a", "c:t-c"]);
    // root 는 그룹 내 다른 task 를 참조하지 않는 쪽.
    assert_eq!(dags[0].root_task_ids, vec!["t-a".to_string()]);
    assert_eq!(dags[1].root_task_ids, vec!["t-c".to_string()]);
}

#[test]
fn dag_id_is_deterministic_across_calls() {
    let tasks = two_disconnected_pairs();
    let a: Vec<String> = group_tasks_into_dags(&tasks)
        .into_iter()
        .map(|d| d.id)
        .collect();
    let b: Vec<String> = group_tasks_into_dags(&tasks)
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(a, b);

    // 입력 순서가 뒤집혀도 같은 id 집합/순서가 나와야 한다 — store 가 돌려주는
    // 순서에 화면 선택 상태가 흔들리면 안 된다.
    let mut reversed = tasks.clone();
    reversed.reverse();
    let c: Vec<String> = group_tasks_into_dags(&reversed)
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(a, c);
}

#[test]
fn explicit_metadata_dag_overrides_connectivity() {
    let mut x = dag_task("t-x", &[], 1000);
    x.metadata = serde_json::json!({ "dag": "build" });
    let mut y = dag_task("t-y", &[], 1001);
    y.metadata = serde_json::json!({ "dag": "build" });

    let dags = group_tasks_into_dags(&[x, y]);
    assert_eq!(dags.len(), 1);
    assert_eq!(dags[0].source, "explicit");
    assert_eq!(dags[0].id, "d:build");
    assert_eq!(dags[0].name, "build");
    assert_eq!(dags[0].task_count, 2);
}

#[test]
fn explicit_dag_name_overrides_group_key() {
    let mut x = dag_task("t-x", &[], 1000);
    x.metadata = serde_json::json!({ "dag": "build", "dag_name": "Release build" });
    let dags = group_tasks_into_dags(&[x]);
    assert_eq!(dags[0].name, "Release build");
}

#[test]
fn derived_dag_name_falls_back_to_root_task_name() {
    let dags = group_tasks_into_dags(&two_disconnected_pairs());
    assert_eq!(dags[0].name, "t-a");
}

#[test]
fn reduce_fallback_and_fallback_of_edges_join_one_dag() {
    // p -> (reduce r) 로만 이어진 쌍, f 는 m 의 사전 존재 fallback,
    // i 는 m2 의 inline fallback 으로 동적 생성된 것(metadata.fallback_of).
    let p = dag_task("t-p", &[], 1000);
    let mut r = dag_task("t-r", &[], 1001);
    r.command = reduce_cmd(vec!["t-p".to_string()]);

    let mut m = dag_task("t-m", &[], 1002);
    m.on_failure = OnFailure::Fallback {
        task: Some("t-f".to_string()),
        inline: None,
    };
    let f = dag_task("t-f", &[], 1003);

    let m2 = dag_task("t-m2", &[], 1004);
    let mut i = dag_task("t-i", &[], 1005);
    i.metadata = serde_json::json!({ "fallback_of": "t-m2" });

    let dags = group_tasks_into_dags(&[p, r, m, f, m2, i]);
    assert_eq!(dags.len(), 3);
    let by_id: std::collections::HashMap<&str, &DagSummary> =
        dags.iter().map(|d| (d.id.as_str(), d)).collect();
    assert_eq!(by_id["c:t-p"].task_ids, vec!["t-p", "t-r"]);
    assert_eq!(by_id["c:t-m"].task_ids, vec!["t-m", "t-f"]);
    assert_eq!(by_id["c:t-m2"].task_ids, vec!["t-m2", "t-i"]);
}

#[test]
fn rollup_state_precedence_running_over_failed_over_terminal() {
    let states = |ss: &[TaskState]| -> &'static str {
        let tasks: Vec<Task> = ss
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // 서로 무관한 task 들이지만 explicit 키로 한 DAG 에 묶어 rollup 만 본다.
                let mut t = dag_task(&format!("t-{i}"), &[], 1000 + i as u64);
                t.state = s.clone();
                t.metadata = serde_json::json!({ "dag": "g" });
                t
            })
            .collect();
        let dags = group_tasks_into_dags(&tasks);
        assert_eq!(dags.len(), 1);
        dags[0].rollup_state
    };
    let failed = || TaskState::Failed {
        error: "x".to_string(),
    };

    // running 이 가장 우선 — failed 가 섞여 있어도 running.
    assert_eq!(
        states(&[TaskState::Running, failed(), TaskState::Ready]),
        "running"
    );
    // running 이 없으면 failed.
    assert_eq!(states(&[failed(), TaskState::Ready]), "failed");
    // 전부 terminal + 전부 succeeded → succeeded.
    assert_eq!(
        states(&[TaskState::Succeeded, TaskState::Succeeded]),
        "succeeded"
    );
    // 전부 terminal 인데 cancelled/skipped 가 섞임 → skipped.
    assert_eq!(
        states(&[TaskState::Succeeded, TaskState::Cancelled]),
        "skipped"
    );
    assert_eq!(states(&[TaskState::Skipped]), "skipped");
    // 미완이 남았고 ready 가 있으면 ready.
    assert_eq!(states(&[TaskState::Waiting, TaskState::Ready]), "ready");
    // 그 외는 waiting (unknown 도 미완으로 취급).
    assert_eq!(states(&[TaskState::Waiting, TaskState::Unknown]), "waiting");

    // state_counts 는 8종 전부를 센다.
    let tasks: Vec<Task> = [TaskState::Running, TaskState::Running, TaskState::Waiting]
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut t = dag_task(&format!("t-{i}"), &[], 1000 + i as u64);
            t.state = s.clone();
            t.metadata = serde_json::json!({ "dag": "g" });
            t
        })
        .collect();
    let dags = group_tasks_into_dags(&tasks);
    assert_eq!(dags[0].state_counts.running, 2);
    assert_eq!(dags[0].state_counts.waiting, 1);
    assert_eq!(dags[0].state_counts.succeeded, 0);
}

#[test]
fn dag_timestamps_span_the_group() {
    let mut a = dag_task("t-a", &[], 1000);
    a.started_at = Some(1500);
    a.finished_at = Some(2000);
    let b = dag_task("t-b", &["t-a"], 1200);

    let dags = group_tasks_into_dags(&[a, b]);
    assert_eq!(dags.len(), 1);
    assert_eq!(dags[0].created_at, 1000);
    assert_eq!(dags[0].updated_at, 2000);
}

#[test]
fn dags_do_not_span_workspaces() {
    let mut a = dag_task("t-a", &[], 1000);
    a.metadata = serde_json::json!({ "dag": "shared" });
    let mut b = dag_task("t-b", &[], 1001);
    b.workspace_id = 2;
    b.metadata = serde_json::json!({ "dag": "shared" });

    let dags = group_tasks_into_dags(&[a, b]);
    assert_eq!(dags.len(), 2);
    assert_eq!(dags[0].workspace_id, 1);
    assert_eq!(dags[1].workspace_id, 2);
    // 같은 explicit 키라 id 는 같다 — 신원은 (workspace_id, id) 쌍이다.
    assert_eq!(dags[0].id, dags[1].id);
}

#[test]
fn has_cycle_is_scoped_to_the_group() {
    // t-a <-> t-b 사이클(스토어를 안 거치므로 생성 검증에 막히지 않는다) +
    // 무관한 정상 그룹 하나.
    let a = dag_task("t-a", &["t-b"], 1000);
    let b = dag_task("t-b", &["t-a"], 1001);
    let c = dag_task("t-c", &[], 1002);

    let dags = group_tasks_into_dags(&[a, b, c]);
    assert_eq!(dags.len(), 2);
    let by_id: std::collections::HashMap<&str, &DagSummary> =
        dags.iter().map(|d| (d.id.as_str(), d)).collect();
    assert!(by_id["c:t-a"].has_cycle);
    assert!(!by_id["c:t-c"].has_cycle);
    // 사이클이면 그룹 내 모든 task 가 서로를 참조하므로 root 는 없다.
    assert!(by_id["c:t-a"].root_task_ids.is_empty());
}

#[test]
fn explicit_group_with_outside_dependency_is_not_a_cycle() {
    // explicit 그룹 밖의 task 를 depends_on 하면 부분집합 검출에서
    // `UnknownDependency` 가 나오는데, 그건 사이클이 아니다.
    let outside = dag_task("t-out", &[], 1000);
    let mut inside = dag_task("t-in", &["t-out"], 1001);
    inside.metadata = serde_json::json!({ "dag": "g" });

    let dags = group_tasks_into_dags(&[outside, inside]);
    let g = dags.iter().find(|d| d.id == "d:g").expect("explicit group");
    assert!(!g.has_cycle);
    assert_eq!(g.task_count, 1);
}

// `list` 는 손상된 task 엔트리를 **빈 목록으로 흡수하지 않는다.** 호스트 러너가
// "조회 실패" 와 "task 없음" 을 구분해 로그를 남길 수 있어야 하고(빈 목록으로
// 흡수되면 러너가 무음으로 정지한 것처럼 보인다), 그 구분의 전제가 여기서
// Err 이 실제로 나온다는 사실이다.
#[test]
fn list_reports_error_on_corrupt_task_entry() {
    let (_td, mut mem, seq) = fresh_store();
    {
        let mut store = TaskStore::new(&mut mem, "_host", &seq);
        store
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "a".to_string(),
                command: run_cmd(),
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata: serde_json::Value::Null,
                now_ms: 0,
            })
            .expect("create");
    }
    // task 접두사 아래에 Task 로 역직렬화되지 않는 값을 심는다.
    mem.put(
        "_host",
        &tasty_memory::Scope::Workspace(1),
        &format!("{TASK_KEY_PREFIX}corrupt"),
        &tasty_memory::MemoryValue::Json(serde_json::json!({ "not": "a task" })),
        &tasty_memory::PutOpts::default(),
    )
    .expect("put");

    let store = TaskStore::new(&mut mem, "_host", &seq);
    assert!(
        store.list(1).is_err(),
        "손상된 엔트리가 빈 목록으로 흡수되면 러너가 실패를 관측할 수 없다"
    );
}
