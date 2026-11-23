//! App owns bounded DAG workers and validates live workspace ownership before publishing results.
use crate::runtime::dag_query::{
    BindingKey, PendingRead, ReadError, ReadResult, ReadTicket, WorkspaceSnapshot,
};
use crate::runtime::engine_session::EngineId;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::Duration;

const MAX_RUNNING: usize = 4;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    workspace: u32,
    engine: EngineId,
    binding: BindingKey,
}
fn target(registry: &super::engine_registry::EngineRegistry, workspace: u32) -> Option<Target> {
    let owner = registry
        .all_sessions()
        .find(|session| session.core_state.has_workspace(workspace))?;
    Some(Target {
        workspace,
        engine: owner.id,
        binding: BindingKey::capture(owner.journal_binding.as_ref()),
    })
}
struct Job {
    engine: EngineId,
    targets: Vec<Target>,
    request: Option<PendingRead>,
    receiver: mpsc::Receiver<Option<ReadResult>>,
    handle: JoinHandle<()>,
}
impl Job {
    fn publish(&mut self, current: &impl Fn(&Target) -> Option<(bool, bool)>) -> bool {
        if self.request.is_none() {
            return false;
        }
        let result = match self.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("DAG read worker disconnected".into()))
            }
        };
        let request = self.request.take().expect("unconsumed DAG result");
        if let Some(result) = result {
            let result = result.and_then(|mut snapshots| {
                for target in &self.targets {
                    let runner = current(target).ok_or(ReadError::Retired)?;
                    if let Some(snapshot) = snapshots.iter_mut().find(|s| s.id == target.workspace)
                    {
                        snapshot.runner = runner;
                    }
                }
                Ok(snapshots)
            });
            request.complete(result);
        }
        true
    }
}
pub(crate) struct DagReads {
    jobs: Vec<Job>,
    stopping: Arc<AtomicBool>,
    wake: tasty_terminal::Waker,
    cursor: usize,
}
impl DagReads {
    pub(crate) fn new(wake: tasty_terminal::Waker) -> Self {
        Self {
            jobs: Vec::new(),
            stopping: Arc::new(AtomicBool::new(false)),
            wake,
            cursor: 0,
        }
    }
    fn available(&self, engine: EngineId) -> bool {
        !self.stopping.load(Ordering::Acquire)
            && self.jobs.len() < MAX_RUNNING
            && !self.jobs.iter().any(|job| job.engine == engine)
    }
    fn start(
        &mut self,
        engine: EngineId,
        request: PendingRead,
        memory: tasty_memory::writer::MemoryAccess,
        sequence: Arc<AtomicU64>,
        targets: Vec<Target>,
    ) {
        if !self.available(engine) {
            request.complete(Err("DAG read capacity exhausted".into()));
            return;
        }
        let stopping = self.stopping.clone();
        let wake = self.wake.clone();
        let ticket = request.ticket();
        let workspaces = request.workspaces.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("dag-read".into())
            .spawn(move || {
                let result = read(&ticket, &workspaces, &memory, &sequence, stopping.as_ref());
                if sender.send(result).is_err() {
                    tracing::debug!("DAG read owner closed before completion");
                }
                wake();
            }) {
            Ok(handle) => self.jobs.push(Job {
                engine,
                targets,
                request: Some(request),
                receiver,
                handle,
            }),
            Err(error) => request.complete(Err(error.to_string().into())),
        }
    }
    fn reap(&mut self, current: impl Fn(&Target) -> Option<(bool, bool)>) -> Vec<EngineId> {
        let mut completed = Vec::new();
        let mut i = 0;
        while i < self.jobs.len() {
            if self.jobs[i].publish(&current) {
                completed.push(self.jobs[i].engine);
            }
            // A result can race this first poll. Keep the join receipt until its result is consumed.
            if self.jobs[i].request.is_none() && self.jobs[i].handle.is_finished() {
                let job = self.jobs.swap_remove(i);
                if job.handle.join().is_err() {
                    tracing::warn!("DAG read worker panicked");
                }
            } else {
                i += 1;
            }
        }
        completed
    }
    pub(crate) fn begin_shutdown(&mut self) {
        self.stopping.store(true, Ordering::Release);
    }
    pub(crate) fn poll_shutdown(&mut self) -> usize {
        self.begin_shutdown();
        self.reap(|_| None);
        self.jobs.len()
    }
}
impl Drop for DagReads {
    fn drop(&mut self) {
        let remaining = self.poll_shutdown();
        if remaining != 0 {
            tracing::warn!(remaining, "DAG reads remain unjoined at App drop");
        }
    }
}
/// 화면 DAG 목록을 writer 요청 하나로 읽는다. 결과는 짧게 나눠 기다리며, 종료하거나 요청이 버려지면
/// 기다림을 그만둔다. 요청은 읽기만 하므로 결과를 버려도 남는 것이 없다. writer 가 멈춰 있어도
/// worker 는 종료 요청을 보고 끝난다.
fn read(
    ticket: &ReadTicket,
    workspaces: &[u32],
    memory: &tasty_memory::writer::MemoryAccess,
    sequence: &Arc<AtomicU64>,
    stopping: &AtomicBool,
) -> Option<ReadResult> {
    let live = || !stopping.load(Ordering::Acquire) && ticket.is_live();
    if !live() {
        return None;
    }
    let (ids, seq) = (workspaces.to_vec(), sequence.clone());
    let request = tasty_memory::writer::MemoryRequest::unit(0, move |mem| {
        Ok(ids
            .iter()
            .map(|workspace| {
                tasty_agent::TaskStore::new(&mut *mem, tasty_memory::HOST_OWNER, seq.as_ref())
                    .list(*workspace)
                    .map(|tasks| (*workspace, tasks))
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, String>>())
    });
    let mut pending = match memory.submit(request) {
        Ok(pending) => pending,
        Err(error) => return Some(Err(error.to_string().into())),
    };
    let completion = loop {
        if let Some(completion) = pending.wait_timeout(Duration::from_millis(10)) {
            break completion;
        }
        #[cfg(test)]
        ticket.waiting.store(true, Ordering::Release);
        if !live() {
            return None;
        }
    };
    let listed = match tasty_memory::writer::settle(completion) {
        Ok(output) => output
            .into_value::<Result<Vec<(u32, Vec<tasty_agent::Task>)>, String>>()
            .unwrap_or_else(|| Err("unexpected memory output".into())),
        Err(failure) => Err(format!("{failure:?}")),
    };
    let snapshots = match listed {
        Ok(listed) => listed
            .into_iter()
            .map(|(id, tasks)| {
                let dags = tasty_agent::group_tasks_into_dags(&tasks);
                WorkspaceSnapshot {
                    id,
                    tasks,
                    dags,
                    runner: (false, false),
                }
            })
            .collect(),
        Err(error) => return Some(Err(error.into())),
    };
    live().then_some(Ok(snapshots))
}
impl super::App {
    pub(crate) fn poll_dag_reads(&mut self) {
        let registry = &self.engines;
        let completed = self.dag_reads.reap(|old| {
            (target(registry, old.workspace).as_ref() == Some(old)).then(|| {
                registry
                    .all_sessions()
                    .find(|session| session.id == old.engine)
                    .expect("validated DAG owner")
                    .task_scope
                    .runner_liveness(old.workspace)
            })
        });
        for engine in completed {
            if let Some(window) = self.engines.window_of(engine)
                && let Some(view) = self.view.views.get_mut(&window)
            {
                view.mark_dirty();
            }
        }
        let mut sessions: Vec<_> = self.engines.all_sessions().collect();
        if !sessions.is_empty() {
            let offset = self.dag_reads.cursor % sessions.len();
            sessions.rotate_left(offset);
            self.dag_reads.cursor = offset + 1;
        }
        for session in sessions {
            if self.dag_reads.available(session.id)
                && let Some(request) = session
                    .runtime
                    .dag_reads
                    .take(session.journal_binding.as_ref())
            {
                let targets: Option<Vec<_>> = request
                    .workspaces
                    .iter()
                    .map(|id| target(&self.engines, *id))
                    .collect();
                if let Some(targets) = targets {
                    self.dag_reads.start(
                        session.id,
                        request,
                        self.services.memory_access().clone(),
                        session.task_scope.agent_seq().clone(),
                        targets,
                    );
                } else {
                    request.complete(Err(ReadError::Retired));
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::dag_query::DagReadQueue;
    use std::sync::Mutex;
    use tasty_utils::test_wait::{holds_by, holds_by_every};
    type Store = Arc<Mutex<dyn tasty_memory::MemoryStorage>>;
    fn memory() -> Store {
        Arc::new(Mutex::new(
            tasty_memory::MemoryStore::open_in_memory().unwrap(),
        ))
    }
    /// 저장소 잠금을 잡으면 묶음을 실행하지 못하는 writer. 시험이 잠금을 잡아 writer 를 멈춘다.
    fn queued(
        store: &Store,
    ) -> (
        crate::app::services::HostMemoryWriter,
        tasty_memory::writer::MemoryAccess,
    ) {
        let changes = Arc::new(tasty_memory::writer::ChangeFeed::default());
        let writer = tasty_memory::writer::spawn(
            tasty_memory::writer::WriterStore::shared(store.clone()),
            changes.clone(),
            tasty_utils::batch_writer::Limits::default(),
            Arc::new(tasty_utils::batch_writer::SystemClock),
            Arc::new(|| {}),
        )
        .expect("memory writer");
        let access = tasty_memory::writer::MemoryAccess::writer(writer.handle(), changes);
        (writer, access)
    }
    fn await_busy(waiting: &AtomicBool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        assert!(
            holds_by(deadline, || waiting.load(Ordering::Acquire)),
            "worker never observed the held store lock"
        );
    }
    fn join(owner: &mut DagReads) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        // poll_shutdown 은 끝난 결과를 게시하고 끝난 worker 를 join 할 뿐 새 작업을 만들지 않는다.
        assert!(
            holds_by(deadline, || owner.poll_shutdown() == 0),
            "DAG workers did not join"
        );
    }
    #[test]
    fn a_locked_store_leaves_display_polling_free_and_shutdown_joins_without_unlocking_it() {
        let (_, engine) = crate::state::tests::test_state();
        let store = memory();
        let (writer, memory) = queued(&store);
        let held = store.lock().unwrap();
        let queue = DagReadQueue::default();
        let source = queue.source(None);
        let mut query = source.request(vec![1]);
        let mut owner = DagReads::new(Arc::new(|| {}));
        let request = queue.take(None).unwrap();
        let waiting = request.waiting.clone();
        owner.start(
            engine.id,
            request,
            memory.clone(),
            Arc::new(AtomicU64::new(0)),
            Vec::new(),
        );
        await_busy(&waiting);
        assert!(query.poll(source).is_none());
        assert_eq!(owner.jobs.len(), 1);
        join(&mut owner);
        assert!(query.poll(source).unwrap().is_err());
        drop(held);
        drop(writer);
    }
    #[test]
    fn dropping_the_receipt_cancels_a_waiting_read_without_stopping_the_service() {
        let (_, engine) = crate::state::tests::test_state();
        let store = memory();
        let (writer, memory) = queued(&store);
        let held = store.lock().unwrap();
        let queue = DagReadQueue::default();
        let query = queue.source(None).request(vec![1]);
        let mut owner = DagReads::new(Arc::new(|| {}));
        let request = queue.take(None).unwrap();
        let waiting = request.waiting.clone();
        owner.start(
            engine.id,
            request,
            memory.clone(),
            Arc::new(AtomicU64::new(0)),
            Vec::new(),
        );
        await_busy(&waiting);
        drop(query);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        // reap 은 끝난 결과 게시와 join 만 하므로 기한 뒤 관측에서도 거둔 뒤 읽는다.
        assert!(holds_by_every(deadline, Duration::ZERO, |last| {
            if last {
                owner.reap(|_| Some((false, false)));
            }
            owner.jobs.is_empty() || {
                if !last {
                    owner.reap(|_| Some((false, false)));
                }
                false
            }
        }));
        assert!(!owner.stopping.load(Ordering::Acquire));
        drop(held);
        let mut next = queue.source(None).request(vec![2]);
        owner.start(
            engine.id,
            queue.take(None).unwrap(),
            memory,
            Arc::new(AtomicU64::new(0)),
            Vec::new(),
        );
        // reap 은 게시·join, next.poll 은 수신 채널 읽기다(대기 요청이 없어 다시 넣지 않는다).
        let mut result = None;
        assert!(holds_by_every(deadline, Duration::ZERO, |_| {
            owner.reap(|_| Some((false, false)));
            result = next.poll(queue.source(None));
            result.is_some()
        }));
        let result = result.unwrap().unwrap();
        assert_eq!(result[0].id, 2);
        join(&mut owner);
        drop(writer);
    }
    #[test]
    fn admission_never_spawns_more_than_four_workers() {
        let store = memory();
        let (writer, memory) = queued(&store);
        let held = store.lock().unwrap();
        let mut owner = DagReads::new(Arc::new(|| {}));
        let queue = DagReadQueue::default();
        let mut queries = Vec::new();
        for _ in 0..MAX_RUNNING + 1 {
            let (_, engine) = crate::state::tests::test_state();
            queries.push(queue.source(None).request(vec![1]));
            owner.start(
                engine.id,
                queue.take(None).unwrap(),
                memory.clone(),
                Arc::new(AtomicU64::new(0)),
                Vec::new(),
            );
        }
        assert_eq!(owner.jobs.len(), MAX_RUNNING);
        assert!(
            queries
                .last_mut()
                .unwrap()
                .poll(queue.source(None))
                .unwrap()
                .is_err()
        );
        join(&mut owner);
        drop(held);
        drop(writer);
    }
    #[test]
    fn dag_results_use_the_actual_owner_and_reject_retired_target_generations() {
        let (_, origin) = crate::state::tests::test_state();
        let (_, destination) = crate::state::tests::test_state();
        assert_ne!(origin.id, destination.id);
        let destination_id = destination.id;
        let workspace = destination.core_state.workspace_at(0).unwrap().id;
        let mut registry = super::super::engine_registry::EngineRegistry::default();
        assert!(registry.insert_pending(destination).is_ok());
        let destination_target = target(&registry, workspace).unwrap();
        assert_eq!(destination_target.engine, destination_id);
        let queue = DagReadQueue::default();
        let mut owner = DagReads::new(Arc::new(|| {}));
        for retired in [false, true] {
            let mut query = queue.source(None).request(vec![workspace]);
            owner.start(
                origin.id,
                queue.take(None).unwrap(),
                tasty_memory::writer::MemoryAccess::inline(memory()),
                Arc::new(AtomicU64::new(0)),
                vec![destination_target.clone()],
            );
            // Wait for the worker, then change the ownership observation before App publishes.
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            assert!(holds_by(deadline, || owner.jobs[0].handle.is_finished()));
            if retired {
                assert!(registry.begin_retiring_pending(destination_id));
                assert!(registry.finish_retiring(destination_id).is_some());
            }
            owner.reap(|old| {
                (target(&registry, workspace).as_ref() == Some(old)).then_some((true, false))
            });
            let result = query.poll(queue.source(None)).unwrap();
            if retired {
                assert!(matches!(result, Err(ReadError::Retired)));
            } else {
                let snapshot = result.unwrap().remove(0);
                assert_eq!(snapshot.id, workspace);
                assert_eq!(snapshot.runner, (true, false));
            }
            assert!(owner.jobs.is_empty());
        }
        join(&mut owner);
    }
}
