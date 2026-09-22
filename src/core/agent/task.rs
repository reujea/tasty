//! Task store wrapper. handler 의 `core.with_memory + TaskStore::new` 조립을
//! 본 모듈로 흡수. `agent_seq` 의 시퀀스 공유는 그대로 유지.

use tasty_agent::task::{
    TaskCreateOpts, TaskDeleteOpts, TaskDeleteReport, TaskPurgeFilter, TaskSweepPlan,
};
use tasty_agent::{
    AgentError, DagSummary, ReducerInput, Task, TaskId, TaskResult, TaskState, TaskStore,
    group_tasks_into_dags,
};
use tasty_memory::HOST_OWNER;

use crate::core::Core;
use crate::core::CoreState;
use crate::core::agent::runner_host::evict_task_side_keys;

impl Core {
    /// Task 생성 — `TaskStore::create`/`create_reserved_for_fallback` wrapper.
    /// `reserved_for_fallback=true` 면 이 task 를 앞으로 다른 main 의
    /// `on_failure.fallback.task` 로 참조할 계획이라는 뜻 — 그 main 이 생기기
    /// 전까지 `Ready` 로 노출되지 않아 러너가 dispatch 할 수 없다(TOCTOU 레이스
    /// 방지, `crates/tasty-agent/src/task/store.rs::create_reserved_for_fallback`).
    pub(crate) fn task_create(
        &self,
        engine: &CoreState,
        opts: TaskCreateOpts,
        reserved_for_fallback: bool,
    ) -> Result<Task, AgentError> {
        let seq = engine.agent_seq.clone();
        self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            if reserved_for_fallback {
                store.create_reserved_for_fallback(opts)
            } else {
                store.create(opts)
            }
        })
    }

    /// Task 목록.
    pub(crate) fn task_list(
        &self,
        engine: &CoreState,
        workspace_id: u32,
    ) -> Result<Vec<Task>, AgentError> {
        // `Core::memory` 와 `CoreState::memory` 는 같은 Arc 다(부팅이 전자를 clone 해
        // 후자에 주입) — 읽기는 어느 쪽으로 들어가도 같은 store 다. 본체를 free fn
        // 으로 두어 `Core` 를 손에 쥐지 못하는 호출자(렌더 경로)도 같은 구현을 쓴다.
        task_list_from_state(engine, workspace_id)
    }

    /// 등록된 DAG 목록. `workspace_id` 가 `None` 이면 **지금 살아있는 전 workspace**
    /// 를 순회한다(원칙 3 — 활성 workspace 에 의존하지 않는다).
    ///
    /// 삭제된 workspace 에 남은 고아 task 는 열거하지 않는다 — 영속 scope 를 직접
    /// 훑으면(`MemoryStore::scopes`) 드러나겠지만, 목록이 보여줄 대상은 사람이 지금
    /// 조작 가능한 workspace 의 DAG 다. 고아 정리는 부팅 시 자동 GC 의 책임이다
    /// (`docs/dev-guide/agent-runner.md` "자동 GC").
    pub(crate) fn dag_list(
        &self,
        engine: &CoreState,
        workspace_id: Option<u32>,
    ) -> Result<Vec<DagSummary>, AgentError> {
        // `task_list` 와 같은 이유로 본체는 free fn 이다 — `Core` 를 손에 쥐지 못하는
        // 렌더 경로(DAG 목록 popup)가 같은 구현을 쓴다.
        dag_list_from_state(engine, workspace_id)
    }

    /// DAG 하나 + 그 DAG 에 속한 task 전체. 못 찾으면 `None`.
    ///
    /// `workspace_id` 가 `None` 이면 전 workspace 를 오름차순으로 훑어 첫 일치를
    /// 돌려준다 — explicit id(`d:<metadata.dag>`)는 사용자가 정한 키라 서로 다른
    /// workspace 가 같은 값을 쓸 수 있으므로, 그 경우를 구분하려면 호출자가
    /// `workspace_id` 를 함께 준다.
    pub(crate) fn dag_get(
        &self,
        engine: &CoreState,
        workspace_id: Option<u32>,
        dag_id: &str,
    ) -> Result<Option<(DagSummary, Vec<Task>)>, AgentError> {
        for wid in dag_scan_workspaces(engine, workspace_id) {
            let tasks = self.task_list(engine, wid)?;
            let Some(dag) = group_tasks_into_dags(&tasks)
                .into_iter()
                .find(|d| d.id == dag_id)
            else {
                continue;
            };
            let subset = tasks
                .into_iter()
                .filter(|t| dag.task_ids.contains(&t.id))
                .collect();
            return Ok(Some((dag, subset)));
        }
        Ok(None)
    }

    /// Task 단건 조회.
    pub(crate) fn task_get(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        task_id: &TaskId,
    ) -> Result<Option<Task>, AgentError> {
        let seq = engine.agent_seq.clone();
        self.with_memory(|mem| {
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(workspace_id, task_id)
        })
    }

    /// Task 취소 — downstream cascade 포함.
    pub(crate) fn task_cancel(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        task_id: &TaskId,
        now_ms: u64,
    ) -> Result<(Task, Vec<Task>), AgentError> {
        let seq = engine.agent_seq.clone();
        let result = self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.cancel(workspace_id, task_id, now_ms)
        });
        if let Ok((ref task, ref downstream)) = result {
            self.fire_waker_if_terminal(engine, workspace_id, task);
            for d in downstream {
                self.fire_waker_if_terminal(engine, workspace_id, d);
            }
        }
        result
    }

    /// S5: task state 가 종결 (Succeeded/Failed/Cancelled/Skipped) 이면 waker hub 에
    /// fire. set_state / cancel / runner thread 등 *모든* terminal 진입 경로에서
    /// 호출되어야 누락 없음 (R-5 회피).
    fn fire_waker_if_terminal(&self, engine: &CoreState, workspace_id: u32, task: &Task) {
        if !task.state.is_terminal() {
            return;
        }
        engine.task_waker_hub.fire(
            workspace_id,
            &task.id,
            crate::core::agent::task_waker::TerminalSnapshot {
                state: task.state.clone(),
                result: task.result.clone(),
            },
        );
    }

    /// Task retry — 옵션에 따라 downstream reset.
    pub(crate) fn task_retry(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        task_id: &TaskId,
        reset_downstream: bool,
        now_ms: u64,
    ) -> Result<Task, AgentError> {
        let seq = engine.agent_seq.clone();
        self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.retry(workspace_id, task_id, reset_downstream, now_ms)
        })
    }

    /// Task state 강제 전이 — runner 가 dispatch / poll 결과에 따라 호출.
    /// downstream cascade 도 함께 수행. 반환: (갱신된 자기, 자동 전이된 downstream).
    pub(crate) fn task_set_state(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        task_id: &TaskId,
        new_state: TaskState,
        now_ms: u64,
    ) -> Result<(Task, Vec<Task>), AgentError> {
        let seq = engine.agent_seq.clone();
        let result = self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.set_state(workspace_id, task_id, new_state, now_ms)
        });
        if let Ok((ref task, ref downstream)) = result {
            self.fire_waker_if_terminal(engine, workspace_id, task);
            for d in downstream {
                self.fire_waker_if_terminal(engine, workspace_id, d);
            }
        }
        result
    }

    /// Task result 영속. set_state(Succeeded/Failed) 직전에 호출.
    pub(crate) fn task_set_result(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        task_id: &TaskId,
        result: TaskResult,
    ) -> Result<Task, AgentError> {
        let seq = engine.agent_seq.clone();
        self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.set_result(workspace_id, task_id, result)
        })
    }

    /// `hook_id` 에 매핑된 대기 중 task 가 있으면 완료
    /// 처리한다. 없으면 no-op. `engine` 은 `task_set_state` 의 waker 발화
    /// (`task_waker_hub`)에 필요하다 — 호출자는 이 훅이 발화한 그 window/state
    /// 의 engine 을 넘겨야 `agent.task_await` 대기자가 정확히 깨어난다(다른
    /// window 의 engine 을 넘기면 waker 가 엉뚱한 hub 에 발화한다).
    ///
    /// `exit_code` — 실제 관측된 값(`HookEvent::CommandCompleted` 발화만 보유,
    /// 그 외 push 신호는 `None`). `Some(0)` 또는 `None` 이면 Succeeded,
    /// `Some(비0)` 이면 그 코드를 실은 Failed — 결정 7 "exit 0 은 succeeded,
    /// 비-0 은 failed" 를 여기서 강제한다. exit code 개념이 없는 임의 push
    /// 완료 신호(예: 향후 claude/codex 전략)는 `None` 이라 기존처럼 Succeeded.
    pub(crate) fn resolve_hook_task_wait(
        &self,
        engine: &CoreState,
        hook_id: u64,
        exit_code: Option<i32>,
    ) {
        let Some((workspace_id, task_id)) = self.hook_task_waits.resolve(hook_id) else {
            return;
        };
        let result = TaskResult {
            exit_code,
            output: None,
            error: None,
        };
        if let Err(e) = self.task_set_result(engine, workspace_id, &task_id, result) {
            tracing::warn!("resolve_hook_task_wait: set_result {task_id} failed: {e}");
            return;
        }
        let now_ms = self.clock.now_unix_millis() as u64;
        let new_state = match exit_code {
            Some(code) if code != 0 => TaskState::Failed {
                error: format!("command exited with code {code}"),
            },
            _ => TaskState::Succeeded,
        };
        if let Err(e) = self.task_set_state(engine, workspace_id, &task_id, new_state, now_ms) {
            tracing::warn!("resolve_hook_task_wait: set_state {task_id} failed: {e}");
        }
    }

    /// Reducer 단계 1: 입력 task 들의 결과를 `ReducerInput` 형태로 수집.
    /// 실제 reducer / shell I/O 는 handler 가 *memory lock 바깥에서* 실행.
    pub(crate) fn task_reduce_collect(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        inputs: &[TaskId],
    ) -> Result<Vec<ReducerInput>, AgentError> {
        let seq = engine.agent_seq.clone();
        self.with_memory(|mem| {
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let mut out: Vec<ReducerInput> = Vec::with_capacity(inputs.len());
            for tid in inputs {
                let task = match store.get(workspace_id, tid)? {
                    Some(t) => t,
                    None => return Err(AgentError::TaskNotFound(tid.clone())),
                };
                let succeeded = matches!(task.state, TaskState::Succeeded);
                let output = task
                    .result
                    .and_then(|r| r.output)
                    .unwrap_or(serde_json::Value::Null);
                out.push(ReducerInput {
                    succeeded,
                    task_id: tid.clone(),
                    output,
                });
            }
            Ok(out)
        })
    }

    /// Task 삭제 — `TaskStore::delete_checked`(참조 무결성 + Running 거부)로
    /// 지운 뒤, 실제로 지워진 task 마다 host 측 side-key(handle/
    /// run_result)도 정리한다. side-key 정리는 memory lock 을 놓은 뒤 별도로
    /// 순회한다 — `with_memory` 안에서 `RunnerContext::with_memory` 를 또 호출하면
    /// 같은 `Arc<Mutex<_>>` 재진입 lock 으로 deadlock.
    pub(crate) fn task_delete(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        task_id: &TaskId,
        opts: TaskDeleteOpts,
    ) -> Result<TaskDeleteReport, AgentError> {
        let seq = engine.agent_seq.clone();
        let report = self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.delete_checked(workspace_id, task_id, opts)
        })?;
        let ctx = self.runner_context(engine);
        for id in &report.deleted {
            evict_task_side_keys(&ctx, workspace_id, id);
        }
        Ok(report)
    }

    /// Task 일괄 정리 — `filter` 로 선정된 후보를 sweep. 상태/
    /// 경과시간 둘 다 미지정이면 워크스페이스 전체가 후보가 되어버려 위험하므로
    /// 여기서 거부한다(IPC 로 직접 호출되는 경로라 CLI 가드만으로는 부족).
    /// `dry_run=true` 면 계획만 계산하고 아무것도 지우지 않는다 — `plan_sweep`
    /// 자체가 순수 함수라 dry-run/실제 실행이 후보 선정 로직을 100% 공유한다.
    pub(crate) fn task_purge(
        &self,
        engine: &CoreState,
        workspace_id: u32,
        filter: TaskPurgeFilter,
        dry_run: bool,
    ) -> Result<TaskSweepPlan, AgentError> {
        if filter.states.is_none() && filter.older_than_ms.is_none() {
            return Err(AgentError::InvalidArgument(
                "task_purge requires at least one of 'states'/'older_than_ms'".into(),
            ));
        }
        let seq = engine.agent_seq.clone();
        let plan = self.with_memory(|mem| {
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.plan_sweep(workspace_id, &filter)
        })?;
        if dry_run {
            return Ok(plan);
        }
        self.with_memory(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.apply_sweep_plan(workspace_id, &plan)
        })?;
        let ctx = self.runner_context(engine);
        for id in &plan.deleted {
            evict_task_side_keys(&ctx, workspace_id, id);
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod hook_wait_tests {
    use std::sync::{Arc, Mutex};

    use tasty_agent::task::{OnFailure, TaskCommand};
    use tasty_memory::MemoryStorage;
    use tasty_themes::{ThemeStorage, ThemeStore};

    use crate::adapters::test::{
        fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
        mock_process::MockProcessSpawner, tmp_home::TmpHome,
    };
    use crate::core::CoreState;
    use crate::core::builder::CoreBuilder;
    use crate::ports::notification_sound::NoopPlayer;

    use super::*;

    fn engine() -> CoreState {
        let waker: tasty_terminal::Waker = Arc::new(|| {});
        CoreState::new(80, 24, waker).expect("engine")
    }

    fn core() -> (Core, tempfile::TempDir) {
        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn ThemeStorage> = Arc::new(ThemeStore::new());
        let home_tmp = tempfile::tempdir().expect("test tempdir");
        let home = TmpHome::new(home_tmp.path().to_path_buf());

        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(home))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core build");
        (core, home_tmp)
    }

    fn mk_ready_task(core: &Core, engine: &CoreState, workspace_id: u32) -> TaskId {
        let opts = TaskCreateOpts {
            workspace_id,
            name: "t".to_string(),
            command: TaskCommand::Run {
                command: vec!["true".into()],
                workspace_id,
                cwd: None,
            },
            depends_on: Vec::new(),
            on_failure: OnFailure::default(),
            metadata: serde_json::Value::Null,
            now_ms: 1,
        };
        core.task_create(engine, opts, false)
            .expect("task_create")
            .id
    }

    /// hook_task_wait 전체 생애주기: register → (해당 hook 발화 시뮬레이션인)
    /// resolve 호출 → task 가 Succeeded 로 마감. `agent.task_set_result` 외부
    /// 호출과 동형의 결과를 훅 경유로 재현한다.
    #[test]
    fn register_then_resolve_completes_the_waiting_task() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let task_id = mk_ready_task(&core, &engine, ws);
        // dispatch 가 됐다고 가정 — Ready → Running (실제 러너의 0단계 전이와 동형).
        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

        core.hook_task_waits
            .register(42, ws, task_id.clone(), u64::MAX);
        core.resolve_hook_task_wait(&engine, 42, Some(0));

        let task = core
            .task_reduce_collect(&engine, ws, std::slice::from_ref(&task_id))
            .expect("collect")
            .remove(0);
        assert!(
            task.succeeded,
            "task should be Succeeded after hook resolve"
        );
    }

    /// 등록되지 않은 hook_id 는 no-op — 대부분의 훅 발화(§B 미구현이라 오늘은
    /// 전부)가 여기 해당하므로 반드시 안전해야 한다.
    #[test]
    fn resolve_unregistered_hook_id_does_not_touch_any_task() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let task_id = mk_ready_task(&core, &engine, ws);
        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

        // 아무것도 등록 안 한 채 임의 hook_id resolve — task 는 Running 그대로.
        core.resolve_hook_task_wait(&engine, 999, None);

        let task = core
            .task_reduce_collect(&engine, ws, std::slice::from_ref(&task_id))
            .expect("collect")
            .remove(0);
        assert!(!task.succeeded);
    }

    /// resolve 는 1회성 소비 — 같은 hook_id 가 두 번 발화해도(예: 재등록 없이
    /// 중복 이벤트) 두 번째는 이미 소비된 매핑이라 no-op(에러 없이 조용히 무시).
    #[test]
    fn resolve_is_one_shot() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let task_id = mk_ready_task(&core, &engine, ws);
        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

        core.hook_task_waits
            .register(7, ws, task_id.clone(), u64::MAX);
        core.resolve_hook_task_wait(&engine, 7, None);
        // 두 번째 발화 — 매핑이 이미 소비돼 no-op. 패닉/에러 없이 조용히 지나간다.
        core.resolve_hook_task_wait(&engine, 7, None);

        let task = core
            .task_reduce_collect(&engine, ws, std::slice::from_ref(&task_id))
            .expect("collect")
            .remove(0);
        assert!(task.succeeded);
    }

    /// 결정 7 — 비-0 exit code 로 발화하면 task 는 Failed 로 마감된다(Succeeded
    /// 아님). `command-completed` 내장 push 전략의 핵심 계약.
    #[test]
    fn resolve_with_nonzero_exit_code_fails_the_task() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let task_id = mk_ready_task(&core, &engine, ws);
        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

        core.hook_task_waits
            .register(1, ws, task_id.clone(), u64::MAX);
        core.resolve_hook_task_wait(&engine, 1, Some(1));

        let task = core
            .task_get(&engine, ws, &task_id)
            .expect("task_get")
            .expect("task exists");
        assert!(matches!(task.state, TaskState::Failed { .. }));
        assert_eq!(task.result.and_then(|r| r.exit_code), Some(1));
    }

    /// `reserved_for_fallback` 배선 확인 — `Core::task_create(.., reserved_for_fallback: true)`
    /// 가 `handle_task_create` 의 `reserved_for_fallback` JSON param 부터
    /// `TaskStore::create_reserved_for_fallback` 까지 실제로 이어진다.
    /// 세부 readiness/dormant 로직은 `crates/tasty-agent` 크레이트 테스트가
    /// 이미 폭넓게 덮으므로, 여기서는 Core 계층 배선만 확인한다.
    #[test]
    fn task_create_reserved_for_fallback_wires_through_core_to_waiting_state() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let opts = TaskCreateOpts {
            workspace_id: ws,
            name: "fallback".to_string(),
            command: TaskCommand::Run {
                command: vec!["true".into()],
                workspace_id: ws,
                cwd: None,
            },
            depends_on: Vec::new(),
            on_failure: OnFailure::default(),
            metadata: serde_json::Value::Null,
            now_ms: 1,
        };
        let task = core
            .task_create(&engine, opts, true)
            .expect("task_create reserved");
        assert_eq!(
            task.state,
            TaskState::Waiting,
            "reserved_for_fallback=true 는 의존성이 없어도 Ready 를 거치지 않아야 한다"
        );
    }
}

/// host 층(`Core::task_delete`) 이 실제로
/// side-key(handle/run_result) 를 정리하고, `Running` 삭제 거부가 자원(세마포어
/// permit)을 건드리지 않는지 검증. store 층의 참조/상태 검사 자체는
/// `crates/tasty-agent/src/task/tests.rs` 가 더 폭넓게 덮는다.
#[cfg(test)]
mod task_delete_tests {
    use std::sync::{Arc, Mutex};

    use tasty_agent::task::{OnFailure, TaskCommand};
    use tasty_agent::{AgentError, SemaphoreStore};
    use tasty_memory::{HOST_OWNER, MemoryStorage, MemoryValue, PutOpts, Scope};
    use tasty_themes::{ThemeStorage, ThemeStore};

    use crate::adapters::test::{
        fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
        mock_process::MockProcessSpawner, tmp_home::TmpHome,
    };
    use crate::core::CoreState;
    use crate::core::agent::runner_host::{handle_key, run_result_key};
    use crate::core::builder::CoreBuilder;
    use crate::ports::notification_sound::NoopPlayer;

    use super::*;

    fn engine() -> CoreState {
        let waker: tasty_terminal::Waker = Arc::new(|| {});
        CoreState::new(80, 24, waker).expect("engine")
    }

    fn core() -> (Core, tempfile::TempDir) {
        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn ThemeStorage> = Arc::new(ThemeStore::new());
        let home_tmp = tempfile::tempdir().expect("test tempdir");
        let home = TmpHome::new(home_tmp.path().to_path_buf());

        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(home))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core build");
        (core, home_tmp)
    }

    fn mk_ready_task(core: &Core, engine: &CoreState, workspace_id: u32) -> TaskId {
        let opts = TaskCreateOpts {
            workspace_id,
            name: "t".to_string(),
            command: TaskCommand::Run {
                command: vec!["true".into()],
                workspace_id,
                cwd: None,
            },
            depends_on: Vec::new(),
            on_failure: OnFailure::default(),
            metadata: serde_json::Value::Null,
            now_ms: 1,
        };
        core.task_create(engine, opts, false)
            .expect("task_create")
            .id
    }

    /// 시나리오 4: terminal 로 마감된 task 를 지우면 `tasty.agent.handle.<id>`/
    /// `tasty.agent.run_result.<id>` 두 side-key 가 모두 evict 된다.
    #[test]
    fn task_delete_evicts_handle_and_run_result_side_keys() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let task_id = mk_ready_task(&core, &engine, ws);

        core.with_memory(|mem| {
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(ws),
                &handle_key(&task_id),
                &MemoryValue::Json(serde_json::json!({"kind": "shell_process", "pid": 123})),
                &PutOpts::default(),
            )
        })
        .expect("persist handle");
        core.with_memory(|mem| {
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(ws),
                &run_result_key(&task_id),
                &MemoryValue::Json(serde_json::json!({"kind": "done", "exit_code": 0})),
                &PutOpts::default(),
            )
        })
        .expect("persist run_result");

        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");
        core.task_set_state(&engine, ws, &task_id, TaskState::Succeeded, 3)
            .expect("Running -> Succeeded");

        core.task_delete(&engine, ws, &task_id, TaskDeleteOpts::default())
            .expect("delete succeeded task");

        let handle_gone = core
            .with_memory(|mem| mem.get(&Scope::Workspace(ws), &handle_key(&task_id)))
            .expect("get handle");
        let run_result_gone = core
            .with_memory(|mem| mem.get(&Scope::Workspace(ws), &run_result_key(&task_id)))
            .expect("get run_result");
        assert!(
            handle_gone.is_none(),
            "handle side-key must be evicted on delete"
        );
        assert!(
            run_result_gone.is_none(),
            "run_result side-key must be evicted on delete"
        );
    }

    /// 시나리오 3: `Running` task 는 세마포어 permit 을 쥐고 있어도(결정 2에
    /// 따라) 항상 거부된다 — 그리고 거부된 삭제 시도는 그 permit 을 건드리지
    /// 않는다(부분 실패로 인한 자원 누수 없음).
    #[test]
    fn task_delete_rejects_running_task_without_touching_held_semaphore_permit() {
        let (core, _home) = core();
        let engine = engine();
        let ws = 1;
        let task_id = mk_ready_task(&core, &engine, ws);

        core.with_memory(|mem| {
            let mut sem = SemaphoreStore::new(mem, HOST_OWNER);
            sem.create(ws, "gate", 1, 1)?;
            sem.acquire(ws, "gate", &task_id, None, 1000)?;
            Ok::<_, AgentError>(())
        })
        .expect("acquire permit");

        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

        let err = core
            .task_delete(&engine, ws, &task_id, TaskDeleteOpts::default())
            .expect_err("Running task delete must be rejected");
        assert!(matches!(err, AgentError::TaskRunning(_)));

        let sem_after = core
            .with_memory(|mem| SemaphoreStore::new(mem, HOST_OWNER).get(ws, "gate"))
            .expect("get semaphore")
            .expect("semaphore exists");
        assert_eq!(
            sem_after
                .holders
                .iter()
                .map(|h| h.id.clone())
                .collect::<Vec<_>>(),
            vec![task_id.clone()],
            "rejected delete must not touch the held permit"
        );
    }
}

/// `CoreState` 만으로 task 목록을 읽는다.
///
/// `Core` 를 인자로 받지 못하는 호출자용 — 특히 egui 렌더 경로(`draw_egui_panels`)
/// 는 `state`/`engine` 만 받는다. `Core::task_list` 가 이 함수에 위임하므로 읽기
/// 규칙(owner·시퀀스)이 두 벌로 갈라지지 않는다.
pub(crate) fn task_list_from_state(
    engine: &CoreState,
    workspace_id: u32,
) -> Result<Vec<Task>, AgentError> {
    let seq = engine.agent_seq.clone();
    // poison 은 다른 스레드의 패닉 흔적일 뿐 store 자체는 읽을 수 있다 —
    // `Core::with_memory` 와 같은 처리(거기서 유일한 lock 정책을 정한다).
    let mut guard = crate::poison::recover_mutex(
        engine.memory.lock(),
        crate::core::MEMORY_WHAT,
        &crate::core::MEMORY_POISONED,
    );
    let store = TaskStore::new(&mut *guard, HOST_OWNER, seq.as_ref());
    store.list(workspace_id)
}

/// DAG 조회가 훑을 workspace id 목록. 순회 순서를 id 오름차순으로 고정해
/// `dag_list` / `dag_get` 결과가 workspace 생성 순서에 흔들리지 않게 한다.
///
/// `Core` 메서드와 `CoreState` 전용 경로가 **같은 순회 규칙**을 써야 한다 — 한쪽만
/// 고치면 popup 과 CLI 가 같은 질문에 다른 순서로 답한다.
pub(crate) fn dag_scan_workspaces(engine: &CoreState, workspace_id: Option<u32>) -> Vec<u32> {
    match workspace_id {
        Some(w) => vec![w],
        None => {
            let mut ids: Vec<u32> = engine.workspaces.iter().map(|w| w.id).collect();
            ids.sort_unstable();
            ids
        }
    }
}

/// 등록된 DAG 목록 — `CoreState` 만으로 조회한다([`Core::dag_list`] 의 본체).
///
/// `workspace_id` 가 `None` 이면 지금 살아있는 전 workspace 를 id 오름차순으로
/// 순회한다(원칙 3 — 활성 workspace 에 의존하지 않는다).
pub(crate) fn dag_list_from_state(
    engine: &CoreState,
    workspace_id: Option<u32>,
) -> Result<Vec<DagSummary>, AgentError> {
    let mut out = Vec::new();
    for wid in dag_scan_workspaces(engine, workspace_id) {
        out.extend(group_tasks_into_dags(&task_list_from_state(engine, wid)?));
    }
    Ok(out)
}

/// 러너 스레드의 생사 — `CoreState` 만으로 조회한다.
///
/// 카운트(ready/running)는 함께 돌려주지 않는다. 호출자(DAG surface)는 **자기가
/// 보고 있는 DAG 의 부분집합** 을 세야 하는데 러너 레지스트리는 workspace 전체를
/// 세기 때문이다 — 화면이 12 개짜리 DAG 를 띄워 놓고 옆 DAG 의 ready 를 합산해
/// 보여주면 배지가 거짓말을 한다.
///
/// 반환은 `(running, crashed)`. 레지스트리가 아직 주입되지 않았으면(headless 초기·
/// 테스트) `(false, false)` — "러너 없음" 으로 읽힌다.
#[cfg(feature = "gui")]
pub(crate) fn runner_liveness(engine: &CoreState, workspace_id: u32) -> (bool, bool) {
    engine
        .agent_runner_registry
        .get()
        .map(|registry| registry.liveness(workspace_id))
        .unwrap_or((false, false))
}
