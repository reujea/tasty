//! 작업 실행 자원을 소유하고 IPC·App이 부르는 작업 API를 제공한다.
//! 작업 정의·상태·결과의 원본은 memory의 TaskStore이며 이 서비스는 두 번째 사본을 만들지 않는다.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock};

use tasty_ipc::host_call::HostIpcInjector;
use tasty_memory::MemoryStorage;

use crate::event_feed::AgentEventQueue;
use crate::hook_wait::HookTaskWaits;
use crate::runner_host::RunnerContext;
use crate::runner_thread::{RunnerRegistry, RunnerStatus, RunnerStopReceipt};
use crate::task_waker::{AwaitOutcome, TaskWakerHub, TerminalSnapshot};

/// engine 하나의 작업 실행 범위. 완료 대기 허브와 사건 큐는 engine마다 따로 두며,
/// 완료 통지는 task가 속한 workspace를 가진 engine의 범위로만 간다.
/// 복제본은 같은 순번·허브·큐를 가리킨다.
#[derive(Clone)]
pub struct TaskScope {
    stopping: Arc<std::sync::atomic::AtomicBool>,
    /// 같은 밀리초에 만든 task ID를 구별하는 순번. 창을 새로 열면 기존 engine과 공유한다.
    agent_seq: Arc<AtomicU64>,
    /// task 종결을 대기자에게 알리고 같은 사건 큐에도 기록한다.
    waker_hub: Arc<TaskWakerHub>,
    /// runner 스레드에서 생성한 사건을 메인 루프로 넘기는 큐.
    event_queue: Arc<AgentEventQueue>,
    /// 서비스를 받지 않는 렌더 경로가 runner 상태를 조회한다. 모든 engine이 TaskService와 같은 Arc를 든다.
    runner_registry: Arc<RunnerRegistry>,
}

impl TaskScope {
    /// runner 등록부는 TaskService의 것을 넘긴다. 다른 등록부를 넘기면 화면의 runner 상태가 틀린다.
    pub fn new(runner_registry: Arc<RunnerRegistry>) -> Self {
        Self::with_seq(Arc::new(AtomicU64::new(0)), runner_registry)
    }

    /// 허브·사건 큐는 새로 만들고 task ID 순번은 받은 것을 쓴다.
    /// 새 창의 engine은 기존 engine의 순번을 넘겨 ID 발급을 공유한다.
    pub fn with_seq(agent_seq: Arc<AtomicU64>, runner_registry: Arc<RunnerRegistry>) -> Self {
        // 허브가 기록하는 큐와 메인 루프가 비우는 큐가 같아야 한다.
        let event_queue = Arc::new(AgentEventQueue::new());
        Self {
            stopping: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            agent_seq,
            waker_hub: Arc::new(TaskWakerHub::with_feed(Arc::clone(&event_queue))),
            event_queue,
            runner_registry,
        }
    }

    pub fn agent_seq(&self) -> &Arc<AtomicU64> {
        &self.agent_seq
    }

    pub fn waker_hub(&self) -> &Arc<TaskWakerHub> {
        &self.waker_hub
    }

    pub fn event_queue(&self) -> &Arc<AgentEventQueue> {
        &self.event_queue
    }

    /// Retire this scope's original runner only; dropping TaskScope itself does not stop it.
    pub fn request_stop_workspace(&self, workspace: u32) -> RunnerStopReceipt {
        self.runner_registry
            .request_stop(&self.waker_hub, Some(workspace))
    }

    pub fn runner_registry(&self) -> &Arc<RunnerRegistry> {
        &self.runner_registry
    }

    /// 화면은 표시 중인 DAG의 task만 세므로 러너 실행·crash 여부만 반환한다.
    pub fn runner_liveness(&self, workspace_id: u32) -> (bool, bool) {
        self.runner_registry
            .scoped_liveness(&self.waker_hub, workspace_id)
    }
}

/// 러너 등록부와 훅-작업 연결은 프로세스에 하나다. engine별 자원은 engine이 가진 범위로 받는다.
/// 복제본은 같은 저장소와 등록부를 가리킨다. memory writer의 호환 구간에 넘길 때 쓴다.
#[derive(Clone)]
pub struct TaskService {
    memory: tasty_memory::writer::MemoryAccess,
    /// App과 같은 Arc다. IPC 서버 시작 뒤 주입된 값을 러너 스레드가 읽는다.
    host_ipc: Arc<OnceLock<HostIpcInjector>>,
    runner_registry: Arc<RunnerRegistry>,
    hook_task_waits: Arc<HookTaskWaits>,
    agent_turns: Arc<crate::agent_turns::AgentTurns>,
    completion: Arc<dyn crate::completion::CompletionResolver>,
    /// 설정이 정하는 report 상한. 러너 스레드와 IPC 처리가 공유한다.
    report_limits: Arc<crate::runner_host::SharedReportLimits>,
    /// 저장소가 memory 대체 모드면 그 원인. 재시작 복구를 요구한 그래프를 거절하는 데 쓴다.
    store_fallback: Option<String>,
    /// 러너의 host IPC 응답 대기 상한(밀리초). 다음에 시작하는 러너부터 쓴다. 시험만 바꾼다.
    host_dispatch_timeout_ms: Arc<std::sync::atomic::AtomicU64>,
}

impl TaskService {
    pub fn new(
        memory: tasty_memory::writer::MemoryAccess,
        host_ipc: Arc<OnceLock<HostIpcInjector>>,
        completion: Arc<dyn crate::completion::CompletionResolver>,
    ) -> Self {
        Self {
            memory,
            host_ipc,
            completion,
            runner_registry: Arc::new(RunnerRegistry::new()),
            hook_task_waits: Arc::new(HookTaskWaits::new()),
            agent_turns: Arc::new(crate::agent_turns::AgentTurns::new()),
            report_limits: Default::default(),
            store_fallback: None,
            host_dispatch_timeout_ms: Arc::new(std::sync::atomic::AtomicU64::new(
                crate::runner_host::HOST_DISPATCH_TIMEOUT.as_millis() as u64,
            )),
        }
    }

    /// 저장소가 memory 대체 모드임을 알린다. `cause` 는 대체 모드의 원인 이름이다.
    pub fn with_store_fallback(mut self, cause: Option<String>) -> Self {
        self.store_fallback = cause;
        self
    }

    /// 저장소가 재시작 뒤에도 남는가.
    pub fn store_durable(&self) -> bool {
        self.store_fallback.is_none()
    }

    pub(crate) fn report_limits(&self) -> &crate::runner_host::SharedReportLimits {
        &self.report_limits
    }

    pub(crate) fn store_fallback(&self) -> Option<&str> {
        self.store_fallback.as_deref()
    }

    /// 본문을 memory writer 순서 안에서 실행하고 결과를 기다린다. 메인 루프는 부르지 않는다
    /// (writer 요청과 continuation을 쓴다). 확정하지 못하면 결과형의 저장소 오류다.
    pub(crate) fn with_memory<R: crate::store_call::FromBlocked + Send + 'static>(
        &self,
        f: impl Fn(&mut dyn MemoryStorage) -> R + Send + Sync + 'static,
    ) -> R {
        crate::store_call::blocking(&self.memory, f)
    }

    /// 이 서비스가 쓰는 memory writer 핸들.
    pub fn memory_access(&self) -> &tasty_memory::writer::MemoryAccess {
        &self.memory
    }

    pub(crate) fn runner_context(&self, scope: &TaskScope) -> RunnerContext {
        RunnerContext {
            scope_stopping: scope.stopping.clone(),
            memory: self.memory.clone(),
            agent_seq: scope.agent_seq().clone(),
            host_ipc: self.host_ipc.clone(),
            task_waker_hub: scope.waker_hub().clone(),
            hook_task_waits: self.hook_task_waits.clone(),
            agent_turns: self.agent_turns.clone(),
            completion: self.completion.clone(),
            report_limits: self.report_limits.clone(),
            host_dispatch_timeout: std::time::Duration::from_millis(
                self.host_dispatch_timeout_ms
                    .load(std::sync::atomic::Ordering::Acquire),
            ),
        }
    }

    /// 시험 전용. 다음에 시작하는 러너의 host IPC 응답 대기 상한을 바꾼다. 시험이 쥔 요청보다 상한이
    /// 먼저 끝나 러너가 그 dispatch 를 벗어나지 않게 길게 둘 때 쓴다.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_host_dispatch_timeout(&self, timeout: std::time::Duration) {
        self.host_dispatch_timeout_ms.store(
            u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
            std::sync::atomic::Ordering::Release,
        );
    }

    /// 받은 workspace에 남은 runner 상태를 정리한다. runner 스레드를 자동 시작하지는 않는다.
    /// 부팅 단계가 첫 창을 띄우기 전에 부른다. 정리는 저장소를 여러 번 읽고 쓰므로 별도 스레드에서
    /// writer 요청을 하나씩 기다리며 하고, 부팅 단계는 그 스레드가 끝나기를 기다린다. 이전 실행의
    /// 점유·handle 정리가 부팅 뒤 처리하는 IPC 요청보다 먼저 확정되게 하려는 것이다.
    pub fn purge_stale_agent_state_on_boot(&self, scope: &TaskScope, workspace_ids: &[u32]) {
        let ctx = self.runner_context(scope);
        std::thread::scope(|s| {
            let spawned = std::thread::Builder::new()
                .name("agent-boot-purge".into())
                .spawn_scoped(s, || {
                    crate::runner_thread::purge_stale_agent_state_on_boot(&ctx, workspace_ids)
                });
            match spawned {
                Ok(handle) => {
                    if handle.join().is_err() {
                        tracing::error!("agent boot purge panicked");
                    }
                }
                Err(error) => tracing::error!(%error, "agent boot purge thread not started"),
            }
        });
    }

    pub fn agent_turns(&self) -> &crate::agent_turns::AgentTurns {
        &self.agent_turns
    }

    pub fn hook_task_waits(&self) -> &HookTaskWaits {
        &self.hook_task_waits
    }

    /// engine의 TaskScope를 만들 때 넘길 등록부. 모든 engine이 이 Arc 하나를 공유한다.
    pub fn runner_registry(&self) -> &Arc<RunnerRegistry> {
        &self.runner_registry
    }

    /// 이미 실행 중이면 false다. 작업 취소나 OS 자식 종료와는 별개의 계약이다.
    pub fn runner_start(&self, scope: &TaskScope, workspace_id: u32) -> bool {
        self.runner_registry
            .start(self.runner_context(scope), workspace_id)
    }

    /// runner 스레드에 정지만 요청하고 바로 돌아온다. 작업 상태를 바꾸거나 실행 중인 자식 프로세스를
    /// 종료하지 않는다. 끝난 worker 는 메인 루프의 [`Self::poll_runner_stops`] 가 회수한다. 러너 스레드가
    /// 끝날 때까지는 [`Self::runner_status`] 가 `stopping` 으로 보고한다. 끝나는 시간의 상한은 없다.
    pub fn request_stop_workspace(&self, scope: &TaskScope, workspace: u32) -> RunnerStopReceipt {
        self.runner_registry
            .request_stop(scope.waker_hub(), Some(workspace))
    }
    pub fn request_stop_scope(&self, scope: &TaskScope) -> RunnerStopReceipt {
        scope
            .stopping
            .store(true, std::sync::atomic::Ordering::Release);
        self.runner_registry.request_stop(scope.waker_hub(), None)
    }
    pub fn poll_runner_stops(&self) -> usize {
        self.runner_registry.poll_stops()
    }

    /// 시험 전용. workspace 러너 스레드가 실제로 끝났는가. 회수하지 않는다.
    #[cfg(any(test, feature = "test-support"))]
    pub fn runner_worker_finished(&self, workspace_id: u32) -> Option<bool> {
        self.runner_registry.worker_finished(workspace_id)
    }

    /// 러너가 꺼져 있어도 저장소를 조회해 실제 작업 수를 채운다.
    pub fn runner_status(&self, scope: &TaskScope, workspace_id: u32) -> RunnerStatus {
        self.runner_registry
            .status(&self.runner_context(scope), workspace_id)
    }

    /// 외부 완료 신호를 기다리는 작업의 저장된 dispatch handle을 읽는다.
    pub fn dispatch_handle(
        &self,
        scope: &TaskScope,
        workspace_id: u32,
        task_id: &str,
    ) -> Option<tasty_agent::DispatchHandle> {
        crate::runner_host::load_dispatch_handle(&self.runner_context(scope), workspace_id, task_id)
    }

    /// 메인 루프 밖의 워커로 옮겨 기다릴 수 있도록 이 engine 범위의 대기 계약을 떼어 준다.
    pub fn awaiter(&self, scope: &TaskScope) -> TaskAwaiter {
        TaskAwaiter {
            memory: self.memory.clone(),
            agent_seq: scope.agent_seq().clone(),
            waker_hub: scope.waker_hub().clone(),
        }
    }
}

/// 한 engine 범위의 task 종결 대기. 허브에 대기자를 먼저 등록한 뒤 저장소를 읽어
/// 조회와 등록 사이에 지나간 완료를 놓치지 않는다.
pub struct TaskAwaiter {
    memory: tasty_memory::writer::MemoryAccess,
    agent_seq: Arc<AtomicU64>,
    waker_hub: Arc<TaskWakerHub>,
}

impl TaskAwaiter {
    /// None·Some(0)은 무기한 대기다. 저장소 조회가 실패하면 작업이 없는 것으로 본다.
    pub fn await_terminal(
        &self,
        workspace_id: u32,
        task_id: &tasty_agent::TaskId,
        timeout_ms: Option<u64>,
    ) -> AwaitOutcome {
        // 대기 worker 스레드에서 부른다. 조회는 writer 순서 안에서 앞서 접수된 쓰기를 본다.
        let load_current = || -> Option<TerminalSnapshot> {
            let seq = self.agent_seq.clone();
            let id = task_id.clone();
            let found = crate::store_call::blocking(&self.memory, move |mem| {
                let store = tasty_agent::TaskStore::new(mem, tasty_memory::HOST_OWNER, &seq);
                store.get(workspace_id, &id).and_then(|t| match t {
                    Some(t) => Ok(Some((t, store.revision(workspace_id, &id)?))),
                    None => Ok(None),
                })
            });
            match found {
                Ok(Some((t, revision))) => Some(TerminalSnapshot::of(&t, revision)),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(%error, workspace_id, %task_id, "task await lookup failed");
                    None
                }
            }
        };
        self.waker_hub
            .await_terminal(workspace_id, task_id, timeout_ms, load_current)
    }
}
