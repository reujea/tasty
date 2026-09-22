//! `TaskStore` — task 의 persistent CRUD + state 전이.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use tasty_memory::{ListOpts, MemoryStorage, MemoryValue, PutOpts, Scope};
use tasty_utils::id::WorkspaceId;

use super::{
    InlineFallbackSpec, OnFailure, TASK_KEY_PREFIX, Task, TaskCommand, TaskGraph, TaskId,
    TaskResult, TaskState, apply_on_failure, is_valid_transition, referencing_task_ids, task_key,
    transitive_referencing_task_ids,
};
use crate::{AgentError, Result};

pub struct TaskStore<'a> {
    mem: &'a mut dyn MemoryStorage,
    owner: String,
    seq: &'a AtomicU64,
}

/// [`TaskStore::create`] 인자 묶음.
///
/// `name` 은 String 으로 받음 — 기존의 `impl Into<String>` 은 generic 이지만 Opts struct
/// 안에 두려면 명시 타입 필요. 호출자가 `&str` 이면 `.into()` 또는 `.to_string()` 호출.
pub struct TaskCreateOpts {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub command: TaskCommand,
    pub depends_on: Vec<TaskId>,
    pub on_failure: OnFailure,
    pub metadata: serde_json::Value,
    pub now_ms: u64,
}

/// [`TaskStore::delete_checked`] 옵션.
#[derive(Debug, Clone, Copy, Default)]
pub struct TaskDeleteOpts {
    /// 참조자가 있어도 전이적 참조자 전부를 함께 지운다(결정 1).
    pub cascade: bool,
    /// 참조 검사만 우회한다 — 상태 제약(Running 금지, 결정 2)은 이걸로 못 뚫는다.
    pub force: bool,
}

/// [`TaskStore::delete_checked`] 결과. `cascade` 가 아니면 `deleted` 는 항상
/// 대상 task id 하나뿐이다.
#[derive(Debug, Clone, Default)]
pub struct TaskDeleteReport {
    pub deleted: Vec<TaskId>,
}

/// [`TaskStore::plan_sweep`] 필터 — 상태 집합 ∩ 경과시간 조건을 모두 만족하는
/// task 만 후보로 삼는다. 둘 다 `None` 이면 워크스페이스 전체가 후보가 되므로,
/// 최소 하나를 지정하도록 강제하는 건 호출자(`Core::task_purge`/CLI) 의 몫이다.
#[derive(Debug, Clone)]
pub struct TaskPurgeFilter {
    /// 이 상태 이름 목록(`TaskState::name()`, 예 `"succeeded"`/`"failed"`)에
    /// 속한 task 만 후보. `None` 이면 상태 무관. `Vec<TaskState>` 가 아니라
    /// 이름 문자열인 이유: `Failed { error }` 는 데이터를 갖고 있어 "실패한
    /// task 전부"를 표현하려면 호출자가 임의 sentinel error 문자열을 만들어야
    /// 하는 문제가 생긴다 — `TaskStore::list`/`handle_task_list` 의 기존
    /// `state.name() == filter` 관례를 그대로 따른다.
    pub states: Option<Vec<String>>,
    /// `now_ms - 기준시각 >= older_than_ms` 인 task 만 후보. 기준시각은 terminal
    /// task 는 `finished_at`, 그 외(Waiting/Ready)는 `created_at`.
    pub older_than_ms: Option<u64>,
    pub now_ms: u64,
}

/// [`TaskStore::plan_sweep`]/[`TaskStore::apply_sweep_plan`] 이 공유하는 계획.
/// dry-run 은 `plan_sweep` 결과를 그대로 보여주면 되고, 실제 삭제는
/// `apply_sweep_plan` 이 `deleted` 를 지운다 — 두 경로가 정확히 같은 후보 선정
/// 로직을 타도록 하나의 순수 함수(`plan_sweep`)로 통일했다.
#[derive(Debug, Clone, Default)]
pub struct TaskSweepPlan {
    /// 필터를 만족하고, Running 이 아니며, 후보 집합 밖에서 참조되지 않아
    /// 안전하게 지울 수 있는 task id.
    pub deleted: Vec<TaskId>,
    /// 필터는 만족했지만 후보 집합 밖의 task 가 여전히 참조 중이라 이번 sweep
    /// 에서는 제외된 task id — 그 참조자가 먼저(또는 같이) 지워지면 이후 sweep
    /// 에서 지워진다.
    pub retained: Vec<TaskId>,
}

impl<'a> TaskStore<'a> {
    /// `owner`는 memory의 owner 필드로 들어간다. 호스트는 보통 `"_host"`를 쓴다.
    pub fn new(
        mem: &'a mut dyn MemoryStorage,
        owner: impl Into<String>,
        seq: &'a AtomicU64,
    ) -> Self {
        Self {
            mem,
            owner: owner.into(),
            seq,
        }
    }

    /// 새 task ID 발급. `t-<now_ms>-<seq:06>`.
    pub fn new_id(&self, now_ms: u64) -> TaskId {
        let s = self.seq.fetch_add(1, Ordering::Relaxed);
        format!("t-{now_ms}-{s:06}")
    }

    /// task 영속. 신규/갱신 모두 동일 (overwrite).
    pub fn put(&mut self, task: &Task) -> Result<()> {
        let scope = Scope::Workspace(task.workspace_id);
        let key = task_key(&task.id)?;
        let value = MemoryValue::Json(serde_json::to_value(task)?);
        self.mem
            .put(&self.owner, &scope, &key, &value, &PutOpts::default())?;
        Ok(())
    }

    /// 단건 조회.
    pub fn get(&self, workspace_id: WorkspaceId, id: &TaskId) -> Result<Option<Task>> {
        let scope = Scope::Workspace(workspace_id);
        let entry = self.mem.get(&scope, &task_key(id)?)?;
        match entry {
            Some(e) => match e.value {
                MemoryValue::Json(v) => Ok(Some(serde_json::from_value(v)?)),
                _ => Err(AgentError::InvalidArgument(format!(
                    "task entry is not json: {id}"
                ))),
            },
            None => Ok(None),
        }
    }

    /// 워크스페이스 전체 task 목록.
    pub fn list(&self, workspace_id: WorkspaceId) -> Result<Vec<Task>> {
        let scope = Scope::Workspace(workspace_id);
        let opts = ListOpts {
            prefix: Some(TASK_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        let entries = self.mem.list(&scope, &opts)?;
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if let MemoryValue::Json(v) = e.value {
                out.push(serde_json::from_value(v)?);
            }
        }
        Ok(out)
    }

    /// task 삭제 (드물게 사용; 보통은 Cancelled 상태로 유지).
    pub fn delete(&mut self, workspace_id: WorkspaceId, id: &TaskId) -> Result<()> {
        let scope = Scope::Workspace(workspace_id);
        self.mem.delete(&self.owner, &scope, &task_key(id)?, None)?;
        Ok(())
    }

    /// 신규 task 생성. 사이클 검출 + 초기 state 계산 후 영속.
    /// `now_ms`는 호스트가 주입 (테스트 결정성).
    pub fn create(&mut self, opts: TaskCreateOpts) -> Result<Task> {
        let TaskCreateOpts {
            workspace_id,
            name,
            command,
            depends_on,
            on_failure,
            metadata,
            now_ms,
        } = opts;
        let id = self.new_id(now_ms);
        let mut existing = self.list(workspace_id)?;

        // S4: Fallback variant 의 task/inline 정확히 하나 검증.
        if let OnFailure::Fallback {
            task: fb_task,
            inline,
        } = &on_failure
        {
            match (fb_task.is_some(), inline.is_some()) {
                (false, false) => {
                    return Err(AgentError::InvalidArgument(
                        "OnFailure::Fallback requires either 'task' or 'inline'".into(),
                    ));
                }
                (true, true) => {
                    return Err(AgentError::InvalidArgument(
                        "OnFailure::Fallback cannot have both 'task' and 'inline'".into(),
                    ));
                }
                _ => {}
            }
        }

        // unknown dep 검출
        let known: HashSet<&TaskId> = existing.iter().map(|t| &t.id).collect();
        for dep in &depends_on {
            if !known.contains(dep) {
                return Err(AgentError::UnknownDependency(dep.clone()));
            }
        }

        // `OnFailure::Fallback { task }` 대상 존재 검증. 미존재를 그대로 저장하면
        // main 이 실패할 때 조용히 무시되고(아래 `set_state`), 그 main 에 의존하는
        // downstream 이 영구 `Waiting` 에 빠진다 — `depends_on` 과 동일한 정책으로
        // 생성 시점에 거부한다(결정 2). `inline` 은 생성 시점엔 아직 존재하지
        // 않는 게 정상(실패 전이 시 동적 생성)이므로 검증 대상이 아니다.
        if let OnFailure::Fallback {
            task: Some(fb_id), ..
        } = &on_failure
            && !known.contains(fb_id)
        {
            return Err(AgentError::UnknownDependency(fb_id.clone()));
        }

        // `TaskCommand::Reduce { inputs }` 대상 존재 검증. 그래프 엣지 자체(암묵적
        // 의존성 승격, 사이클 검출 포함)는 `TaskGraph`(graph.rs) 가 담당하고, 여기
        // 서는 신규 생성 시점의 존재 검증만 한다(결정 1) — 미검증 시 dispatch
        // 시점에야 실패하거나(구 동작), 미완 입력 위에서 조용히 오답을 낸다.
        if let TaskCommand::Reduce { inputs, .. } = &command {
            for input_id in inputs {
                if !known.contains(input_id) {
                    return Err(AgentError::UnknownDependency(input_id.clone()));
                }
            }
        }

        let mut new_task = Task {
            id: id.clone(),
            workspace_id,
            name,
            command,
            depends_on,
            state: TaskState::Waiting,
            created_at: now_ms,
            started_at: None,
            finished_at: None,
            result: None,
            on_failure,
            metadata,
            reserved_for_fallback: false,
        };

        // 임시로 그래프에 포함시켜 사이클 검출
        existing.push(new_task.clone());
        {
            let graph = TaskGraph::build(&existing);
            graph.detect_cycles()?;
            // 초기 readiness 계산
            if let Some(state) = graph.evaluate_readiness(&new_task.id) {
                new_task.state = state;
            }
        }

        self.put(&new_task)?;

        // `Fallback{task}` 대상은 이 main(new_task) 보다 먼저 존재해야 하므로
        // (위 존재 검증), 그 fallback 자신의 초기 state 는 이 main 이 생기기
        // *전에* 이미 계산돼 있었다 — 그 시점엔 아직 아무도 그걸 fallback 으로
        // 참조하지 않았으므로, `depends_on` 이 비어 있으면 곧장 `Ready` 로
        // 확정된다(`evaluate_readiness`의 dormant 판정은 참조하는 main 이
        // 존재할 때만 걸린다). 이제 막 그 main 이 생겼으니, fallback 이 아직
        // dispatch 되지 않고 `Ready` 로 남아 있다면 지금 `Waiting` 으로
        // 되돌려 "main 실패 전엔 돌지 않는다" 계약을 소급 적용한다. 그 사이
        // 이미 `Running`/종결로 넘어갔다면(runner 가 그 틈에 tick 했다면)
        // 되돌릴 방법이 없어 건드리지 않는다 — 이 잔여 레이스는
        // `create_reserved_for_fallback`(아래)로 fallback 을 미리 예약
        // 생성해야 완전히 닫힌다: 예약된 fallback 은 애초에 `Ready` 를 거치지
        // 않으므로 이 시점에 이미 dispatch 됐을 수가 없다.
        //
        // `reserved_for_fallback` 예약도 여기서 함께 해제한다 — 예약 중엔
        // state 가 `Ready` 를 거치지 않으므로 위 조건(`Ready`)만으론 절대
        // 걸리지 않는다. 이제 실제 main 이 생겼으니 이후로는 "참조하는 main
        // 이 존재하고 아직 Failed 가 아니다"라는 정상 dormant 판정이 이어받고,
        // 예약 플래그는 더 이상 필요 없다(끄지 않으면 main 이 나중에 Failed
        // 로 전이해도 영구히 dormant 로 남아 fallback 이 결코 승격되지 못한다).
        if let OnFailure::Fallback {
            task: Some(fb_id), ..
        } = &new_task.on_failure
            && let Some(mut fb) = self.get(workspace_id, fb_id)?
        {
            let mut changed = false;
            if fb.reserved_for_fallback {
                fb.reserved_for_fallback = false;
                changed = true;
            }
            if matches!(fb.state, TaskState::Ready) {
                fb.state = TaskState::Waiting;
                changed = true;
            }
            if changed {
                self.put(&fb)?;
            }
        }

        Ok(new_task)
    }

    /// [`Self::create`] 와 동일하지만, 생성된 task 를 아직 아무 main 도
    /// 참조하지 않는 동안에도 `Ready`(또는 `Skipped`)로 노출하지 않고
    /// `Waiting` 에 묶어둔다(`Task::reserved_for_fallback = true`) — 앞으로
    /// `OnFailure::Fallback{task: Some(이 id)}` 로 자신을 참조할 main 을
    /// 곧 만들 계획이라는 걸 호출자가 미리 선언하는 용도다.
    ///
    /// 왜 필요한가: 평범한 `create()` 은 아직 아무도 참조하지 않는 fallback
    /// 후보를 (의존성이 없다면) 곧장 `Ready` 로 확정한다 — 그 시점엔 아직
    /// "이건 fallback 이다"라는 정보가 store 어디에도 없기 때문이다. 이후
    /// 그 fallback 을 참조하는 main 을 만드는 별도 `create()` 호출까지
    /// 러너가 tick 해 이미 dispatch(`Ready`→`Running`) 해버리면, main 생성
    /// 시점의 소급 정정(`Ready`→`Waiting`)이 무력화돼 fallback 이 main 의
    /// 성공/실패와 무관하게 실행된다(TOCTOU — 두 CLI 호출 사이의 지연은
    /// LLM 사고 시간·네트워크 왕복 등으로 임의로 길어질 수 있어 "짧게
    /// 묶어 부르면 된다"는 낙관이 성립하지 않는다). 이 메서드로 만든
    /// fallback 은 `state` 가 결코 `Ready` 를 거치지 않으므로, 두 호출
    /// 사이의 지연이 얼마든 러너가 손댈 수 없다.
    ///
    /// 주의: 이렇게 예약한 task 를 참조하는 main 을 끝내 만들지 않으면
    /// (계획을 접었거나) 이 task 는 `Waiting` 에 영구 잔류한다 — opt-in
    /// 계약이므로 호출자 책임이다. `delete_checked` 로 정리하고 다시
    /// 만들면 된다(참조자가 없고 `Running` 도 아니므로 항상 지울 수 있다).
    pub fn create_reserved_for_fallback(&mut self, opts: TaskCreateOpts) -> Result<Task> {
        let mut task = self.create(opts)?;
        if !matches!(task.state, TaskState::Waiting) {
            task.state = TaskState::Waiting;
        }
        task.reserved_for_fallback = true;
        self.put(&task)?;
        Ok(task)
    }

    /// task의 state를 변경. 변경 규칙은 [`TaskState`] 문서 참조.
    /// 변경 후 downstream의 readiness를 자동 재평가해 `Waiting → Ready/Skipped`로
    /// 전이시키고 영속한다. 반환값은 (갱신된 자기 자신, 자동 전이된 downstream).
    pub fn set_state(
        &mut self,
        workspace_id: WorkspaceId,
        id: &TaskId,
        new_state: TaskState,
        now_ms: u64,
    ) -> Result<(Task, Vec<Task>)> {
        let mut task = self
            .get(workspace_id, id)?
            .ok_or_else(|| AgentError::TaskNotFound(id.clone()))?;
        if !is_valid_transition(&task.state, &new_state) {
            return Err(AgentError::InvalidTransition {
                from: task.state.name().to_string(),
                to: new_state.name().to_string(),
            });
        }
        match new_state {
            TaskState::Running => {
                task.started_at = Some(now_ms);
            }
            TaskState::Succeeded
            | TaskState::Failed { .. }
            | TaskState::Cancelled
            | TaskState::Skipped => {
                task.finished_at = Some(now_ms);
            }
            _ => {}
        }
        let became = task.state.clone();
        task.state = new_state.clone();
        self.put(&task)?;

        let mut transitioned = Vec::new();

        // Failed 전이 + on_failure=Fallback → fallback task 를 자동 Ready 로 (가능하면).
        // S4: existing(task) + inline(자동 생성) 두 경로 모두 지원.
        if matches!(new_state, TaskState::Failed { .. })
            && let OnFailure::Fallback {
                task: fb_id_opt,
                inline: inline_opt,
            } = task.on_failure.clone()
        {
            // 케이스 1: existing fallback (기존 동작).
            if let Some(fb_id) = fb_id_opt
                && let Some(fb) = self.advance_existing_fallback(workspace_id, &task.id, &fb_id)?
            {
                transitioned.push(fb);
            }
            // 케이스 2: inline → 동적 생성.
            if let Some(spec) = inline_opt
                && let Some(new_fb) =
                    self.materialize_inline_fallback(workspace_id, &task, *spec, now_ms)?
            {
                transitioned.push(new_fb);
            }
        }

        // Failed 가 아닌 다른 terminal 로 전이 + on_failure=Fallback(existing) →
        // 그 fallback 은 다시는 깨어날 일이 없다. `Fallback` 은 "main 이 실패했을
        // 때만 도는 대체 경로" 계약이라 승격 진입로가 Failed 전이(위 분기) 하나
        // 뿐인데, main 이 Succeeded/Cancelled/Skipped 로 끝나면 그 진입로 자체가
        // 영영 열리지 않는다 — dormant 상태 그대로 `Waiting` 에 방치하면
        // task-graph/task-list/purge 후보 판정에 계속 남으므로 `Skipped` 로
        // 마감해 종결시킨다. inline fallback 은 Failed 분기에서만 동적
        // 생성되므로, 여기 도달했다는 건 애초에 만들어진 적이 없다는 뜻이라
        // 정리할 대상 자체가 없다.
        if matches!(
            new_state,
            TaskState::Succeeded | TaskState::Cancelled | TaskState::Skipped
        ) && let OnFailure::Fallback {
            task: Some(fb_id), ..
        } = task.on_failure.clone()
            && matches!(
                self.get(workspace_id, &fb_id)?.map(|t| t.state),
                Some(TaskState::Waiting)
            )
        {
            let (fb_task, more) =
                self.set_state(workspace_id, &fb_id, TaskState::Skipped, now_ms)?;
            transitioned.push(fb_task);
            transitioned.extend(more);
        }

        // downstream 재평가
        transitioned.extend(self.cascade_downstream(workspace_id, id, now_ms)?);

        // terminal 전이 시: 자기를 fallback 으로 지정한 main task 가 있으면 그 main
        // 의 downstream 도 재평가 (main 입장에선 fallback 결과로 effective state 가 정해짐).
        if matches!(
            new_state,
            TaskState::Succeeded
                | TaskState::Failed { .. }
                | TaskState::Cancelled
                | TaskState::Skipped
        ) {
            let all_now = self.list(workspace_id)?;
            let parent_main_ids: Vec<TaskId> = all_now
                .iter()
                .filter(|t| match &t.on_failure {
                    // existing 경로: main 의 task field 가 id 를 직접 가리킴.
                    OnFailure::Fallback {
                        task: Some(fb_id), ..
                    } => fb_id == id,
                    _ => false,
                })
                .map(|t| t.id.clone())
                .collect();
            // inline 경로: self.metadata.fallback_of 가 main id — main 을 찾으려면 reverse lookup.
            let mut inline_main_ids: Vec<TaskId> = Vec::new();
            if let Some(self_task) = all_now.iter().find(|t| t.id == *id)
                && let Some(main_id) = self_task
                    .metadata
                    .get("fallback_of")
                    .and_then(|v| v.as_str())
            {
                inline_main_ids.push(main_id.to_string());
            }
            for main_id in parent_main_ids.into_iter().chain(inline_main_ids) {
                transitioned.extend(self.cascade_downstream(workspace_id, &main_id, now_ms)?);
            }
        }
        let _ = became; // 이전 state 스냅샷 — 현재 미사용(향후 전이 로그 후보). 값 drop, Result 아님.
        Ok((task, transitioned))
    }

    /// `set_state`의 Failed→Fallback 분기 중 "케이스 1: existing fallback" 처리.
    /// fallback 대상이 `Ready`/`Skipped` 로 진행 가능하면 그 상태로 올리고 반환.
    fn advance_existing_fallback(
        &mut self,
        workspace_id: WorkspaceId,
        main_task_id: &TaskId,
        fb_id: &TaskId,
    ) -> Result<Option<Task>> {
        let Some(mut fb) = self.get(workspace_id, fb_id)? else {
            // `create()` 가 이제 신규 생성 시점에 `Fallback.task` 존재를 검증하므로
            // (결정 2) 정상적으로는 도달하지 않는다 — 이 경로는 본 검증 도입 이전에
            // 저장된 dangling 참조(결정 3, 마이그레이션 안 함)에서만 발생한다.
            // 조용히 무시하면 이 main 에 의존하는 downstream 이 영구 `Waiting` 에
            // 빠지는데 원인을 알 방법이 없으므로, 최소한 관측 가능한 신호를 남긴다.
            tracing::warn!(
                task_id = %main_task_id,
                fallback_task_id = %fb_id,
                "on_failure.fallback.task references a task that no longer exists; \
                 downstream depending on this task will remain Waiting indefinitely"
            );
            return Ok(None);
        };
        let all_now = self.list(workspace_id)?;
        let Some(target) = TaskGraph::build(&all_now).evaluate_readiness(fb_id) else {
            return Ok(None);
        };
        if target == TaskState::Waiting || !is_valid_transition(&fb.state, &target) {
            return Ok(None);
        }
        fb.state = target;
        self.put(&fb)?;
        Ok(Some(fb))
    }

    /// `set_state`의 Failed→Fallback 분기 중 "케이스 2: inline → 동적 생성" 처리.
    /// 같은 main 이 이미 inline fallback 을 만든 적 있으면(idempotency) `None`.
    fn materialize_inline_fallback(
        &mut self,
        workspace_id: WorkspaceId,
        main_task: &Task,
        spec: InlineFallbackSpec,
        now_ms: u64,
    ) -> Result<Option<Task>> {
        let existing = self.list(workspace_id)?;
        let already = existing.iter().any(|t| {
            t.metadata.get("fallback_of").and_then(|v| v.as_str()) == Some(main_task.id.as_str())
        });
        if already {
            return Ok(None);
        }
        let mut metadata = spec.metadata;
        if !metadata.is_object() {
            metadata = serde_json::json!({});
        }
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert(
                "fallback_of".into(),
                serde_json::Value::String(main_task.id.clone()),
            );
        }
        let opts = TaskCreateOpts {
            workspace_id,
            name: spec.name,
            command: spec.command,
            depends_on: spec
                .depends_on_override
                .unwrap_or_else(|| main_task.depends_on.clone()),
            on_failure: spec.on_failure,
            metadata,
            now_ms,
        };
        Ok(Some(self.create(opts)?))
    }

    /// task의 result를 기록 (state 전이는 별도). 보통 set_state(Succeeded/Failed) 전에 호출.
    pub fn set_result(
        &mut self,
        workspace_id: WorkspaceId,
        id: &TaskId,
        result: TaskResult,
    ) -> Result<Task> {
        let mut task = self
            .get(workspace_id, id)?
            .ok_or_else(|| AgentError::TaskNotFound(id.clone()))?;
        task.result = Some(result);
        self.put(&task)?;
        Ok(task)
    }

    /// `task_id`의 모든 transitive downstream에서 `Waiting` 상태인 것들을 평가해
    /// 가능하면 `Ready/Skipped`로 전이. on_failure 정책도 함께 적용.
    ///
    /// `now_ms`는 `Skipped`(terminal)로 전이하는 downstream의 `finished_at`을 채우는 데
    /// 쓰인다 — 이 경로는 `set_state`를 거치지 않고 상태를 직접 갈아끼우므로, `set_state`의
    /// terminal 타임스탬프 기록을 스스로 재현해야 한다.
    fn cascade_downstream(
        &mut self,
        workspace_id: WorkspaceId,
        task_id: &TaskId,
        now_ms: u64,
    ) -> Result<Vec<Task>> {
        let all = self.list(workspace_id)?;
        let downstream_ids = {
            let graph = TaskGraph::build(&all);
            graph.transitive_downstream(task_id)
        };
        let mut updated = Vec::new();
        // BFS 순서대로: 각 단계마다 최신 상태로 다시 list/build.
        for d_id in downstream_ids {
            let all_now = self.list(workspace_id)?;
            let graph = TaskGraph::build(&all_now);
            let d_task = match self.get(workspace_id, &d_id)? {
                Some(t) => t,
                None => continue,
            };
            if !matches!(d_task.state, TaskState::Waiting) {
                continue;
            }
            let target = match graph.evaluate_readiness(&d_id) {
                Some(TaskState::Skipped) => apply_on_failure(&d_task, &all_now),
                other => other,
            };
            if let Some(next) = target
                && next != TaskState::Waiting
                && is_valid_transition(&d_task.state, &next)
            {
                let is_terminal = next.is_terminal();
                let mut nt = d_task;
                nt.state = next;
                if is_terminal {
                    nt.finished_at = Some(now_ms);
                }
                self.put(&nt)?;
                updated.push(nt);
            }
        }
        Ok(updated)
    }

    /// 사용자가 명시적으로 cancel.
    pub fn cancel(
        &mut self,
        workspace_id: WorkspaceId,
        id: &TaskId,
        now_ms: u64,
    ) -> Result<(Task, Vec<Task>)> {
        let task = self
            .get(workspace_id, id)?
            .ok_or_else(|| AgentError::TaskNotFound(id.clone()))?;
        if task.state.is_terminal() {
            return Err(AgentError::AlreadyTerminal(task.state.name().to_string()));
        }
        self.set_state(workspace_id, id, TaskState::Cancelled, now_ms)
    }

    /// retry. 현재 state가 Failed/Cancelled/Skipped/Unknown인 경우만 허용.
    /// `reset_downstream=true`면 downstream 중 Skipped/Failed인 것도 Waiting으로 되돌림.
    pub fn retry(
        &mut self,
        workspace_id: WorkspaceId,
        id: &TaskId,
        reset_downstream: bool,
        now_ms: u64,
    ) -> Result<Task> {
        let mut task = self
            .get(workspace_id, id)?
            .ok_or_else(|| AgentError::TaskNotFound(id.clone()))?;
        match &task.state {
            TaskState::Failed { .. }
            | TaskState::Cancelled
            | TaskState::Skipped
            | TaskState::Unknown => {}
            other => {
                return Err(AgentError::InvalidTransition {
                    from: other.name().to_string(),
                    to: "waiting (retry)".to_string(),
                });
            }
        }
        task.state = TaskState::Waiting;
        task.started_at = None;
        task.finished_at = None;
        task.result = None;
        self.put(&task)?;

        // readiness 즉시 평가 — deps 가 이미 종결(예: 여전히 실패/skip 상태)이면 이 자리에서
        // 곧장 Skipped 로 되돌아갈 수 있다. set_state 를 거치지 않는 직접-put 이므로, 그
        // terminal 타임스탬프 기록을 여기서 재현한다(cascade_downstream 과 동일 이유).
        let all = self.list(workspace_id)?;
        if let Some(next) = TaskGraph::build(&all).evaluate_readiness(id)
            && next != TaskState::Waiting
        {
            let is_terminal = next.is_terminal();
            let mut nt = task.clone();
            nt.state = next;
            if is_terminal {
                nt.finished_at = Some(now_ms);
            }
            self.put(&nt)?;
            task = nt;
        }

        if reset_downstream {
            let downstream = TaskGraph::build(&all).transitive_downstream(id);
            for d_id in downstream {
                if let Some(mut d) = self.get(workspace_id, &d_id)?
                    && matches!(
                        d.state,
                        TaskState::Skipped | TaskState::Failed { .. } | TaskState::Cancelled
                    )
                {
                    d.state = TaskState::Waiting;
                    d.started_at = None;
                    d.finished_at = None;
                    d.result = None;
                    self.put(&d)?;
                }
            }
            // downstream 모두 갱신했으니 cascade 한번 더 — 재-skip 되는 downstream 의
            // finished_at 은 cascade_downstream 이 now_ms 로 다시 채운다.
            self.cascade_downstream(workspace_id, id, now_ms)?;
        }

        Ok(task)
    }

    /// 참조 무결성 + 상태 제약을 지키는 task 삭제(결정 1·2). `raw delete`
    /// (위 [`Self::delete`])는 이 검사들을 전혀 하지 않으므로 직접 호출하면
    /// dangling 참조·영구 `Waiting`·자원 누수를 만들 수 있다 — 호스트/CLI 는
    /// 항상 이 메서드를 거쳐야 한다.
    ///
    /// - `Running` 상태는 `cascade`/`force` 와 무관하게 항상 거부(결정 2) —
    ///   먼저 `cancel` 로 정리해야 한다.
    /// - 기본(둘 다 `false`): 참조자가 하나라도 있으면 거부하고 그 목록을 반환.
    /// - `cascade`: 전이적 참조자 전부를 함께 지운다. 참조자 중 `Running` 이
    ///   있으면 그 하나 때문에 통째로 거부한다(부분 cascade 없음).
    /// - `force`(비-cascade): 참조 검사만 생략. 대상 자체의 `Running` 제약은
    ///   여전히 적용된다.
    pub fn delete_checked(
        &mut self,
        workspace_id: WorkspaceId,
        id: &TaskId,
        opts: TaskDeleteOpts,
    ) -> Result<TaskDeleteReport> {
        let task = self
            .get(workspace_id, id)?
            .ok_or_else(|| AgentError::TaskNotFound(id.clone()))?;
        if matches!(task.state, TaskState::Running) {
            return Err(AgentError::TaskRunning(id.clone()));
        }

        let all = self.list(workspace_id)?;
        let targets: Vec<TaskId> = if opts.cascade {
            let mut seen: HashSet<TaskId> = HashSet::new();
            seen.insert(id.clone());
            seen.extend(transitive_referencing_task_ids(&all, id));
            for t_id in &seen {
                if let Some(t) = all.iter().find(|t| &t.id == t_id)
                    && matches!(t.state, TaskState::Running)
                {
                    return Err(AgentError::TaskRunning(t_id.clone()));
                }
            }
            seen.into_iter().collect()
        } else {
            if !opts.force {
                let referencers = referencing_task_ids(&all, id);
                if !referencers.is_empty() {
                    return Err(AgentError::TaskReferenced {
                        task: id.clone(),
                        referenced_by: referencers,
                    });
                }
            }
            vec![id.clone()]
        };

        for t_id in &targets {
            self.delete(workspace_id, t_id)?;
        }
        Ok(TaskDeleteReport { deleted: targets })
    }

    /// `filter` 를 만족하는 task 중 안전하게 지울 수 있는 것만 골라낸다
    /// (순수 함수, 영속 변경 없음). Running 은 항상 후보에서 제외되고(결정 2),
    /// 후보 집합 밖에서 여전히 참조되는 task 는 fixed-point 로 반복 제외한다 —
    /// 그래야 "후보 A 를 참조하는 후보 B" 처럼 후보끼리의 참조는 함께 지워지되,
    /// 후보 밖 task 의 참조는 안전하게 보존된다. dry-run 은 이 결과를 그대로
    /// 보여주면 되고, 실제 삭제는 [`Self::apply_sweep_plan`] 이 이어받는다.
    pub fn plan_sweep(
        &self,
        workspace_id: WorkspaceId,
        filter: &TaskPurgeFilter,
    ) -> Result<TaskSweepPlan> {
        let all = self.list(workspace_id)?;
        let candidates: HashSet<TaskId> = all
            .iter()
            .filter(|t| !matches!(t.state, TaskState::Running))
            .filter(|t| match &filter.states {
                None => true,
                Some(states) => states.iter().any(|s| s == t.state.name()),
            })
            .filter(|t| match filter.older_than_ms {
                None => true,
                Some(threshold) => {
                    let base = t.finished_at.unwrap_or(t.created_at);
                    filter.now_ms.saturating_sub(base) >= threshold
                }
            })
            .map(|t| t.id.clone())
            .collect();

        let mut eligible = candidates.clone();
        loop {
            let blocked: Vec<TaskId> = eligible
                .iter()
                .filter(|id| {
                    referencing_task_ids(&all, id)
                        .iter()
                        .any(|r| !eligible.contains(r))
                })
                .cloned()
                .collect();
            if blocked.is_empty() {
                break;
            }
            for id in blocked {
                eligible.remove(&id);
            }
        }

        let mut deleted: Vec<TaskId> = eligible.iter().cloned().collect();
        deleted.sort();
        let mut retained: Vec<TaskId> = candidates.difference(&eligible).cloned().collect();
        retained.sort();
        Ok(TaskSweepPlan { deleted, retained })
    }

    /// [`Self::plan_sweep`] 이 만든 계획을 실제로 적용(삭제)한다. 계획 자체가
    /// 이미 Running 제외 + 참조 안전을 보장하므로 추가 검증 없이 그대로 지운다.
    pub fn apply_sweep_plan(
        &mut self,
        workspace_id: WorkspaceId,
        plan: &TaskSweepPlan,
    ) -> Result<()> {
        for id in &plan.deleted {
            self.delete(workspace_id, id)?;
        }
        Ok(())
    }
}
