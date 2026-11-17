//! Lua VM을 전용 워커에서만 사용한다. 메인은 실행 요청을 보내고 HostCommand를 처리하며
//! 읽기 전용 LuaSnapshot을 발행한다. 워커는 요청을 직렬로 실행한다.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, LuaSerdeExt, Table, Value};

use crate::bridge::{HostCommand, LuaSnapshot, SharedSnapshot};

/// Lua hook 시스템 진입 에러.
#[derive(Debug, thiserror::Error)]
pub enum LuaEngineError {
    #[error("lua init failed: {0}")]
    Init(mlua::Error),
    #[error("lua eval failed: {0}")]
    Eval(mlua::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// 워커 스레드가 종료되어 job 을 처리할 수 없음.
    #[error("lua worker unavailable")]
    WorkerGone,
}

impl From<mlua::Error> for LuaEngineError {
    fn from(e: mlua::Error) -> Self {
        LuaEngineError::Eval(e)
    }
}

/// 내부 Lua registry 키 — 등록된 hook 함수 목록 보관.
/// 구조: `{ [event_name: string] = { Function, Function, ... } }`.
const HOOKS_REGISTRY_KEY: &str = "tasty_hooks";

/// 메인→워커 실행 job 큐 용량. 초과 시 fire-and-forget job 은 drop + warn(backpressure).
const JOB_QUEUE_CAP: usize = 256;

/// 워커→메인 커맨드 큐 용량. host_api 쪽(`bridge`/`host_api`)이 공유.
pub(crate) const COMMAND_QUEUE_CAP: usize = 256;

/// 스크립트 한 번의 기본 실행 기한. Lua 명령 hook에서 초과를 확인해 중단한다.
const SCRIPT_DEADLINE: Duration = Duration::from_secs(5);

/// 현재 job의 절대 기한. 같은 워커의 set_hook 콜백이 확인한다.
type SharedDeadline = Arc<Mutex<Option<Instant>>>;
/// 기한 hook이 실행을 중단했으면 true다. 경과 시간만으로는 느린 호스트 호출 뒤의 일반 오류와 구분할 수 없다.
type DeadlineHit = Arc<AtomicBool>;

/// job 종료나 큐 폐기 때 완료 counter를 늘리는 RAII 토큰.
/// 실행되지 못한 job도 Drop으로 자동실행 재진입 제한을 해제한다.
pub struct CompletionToken(Arc<AtomicU64>);

impl CompletionToken {
    /// `counter` 를 완료 시 1 증가시키는 토큰 생성.
    pub fn new(counter: Arc<AtomicU64>) -> Self {
        Self(counter)
    }
}

impl Drop for CompletionToken {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// 보냈지만 아직 끝나지 않은 job 하나. 큐에 넣기 전에 만들어 수를 늘리고, 워커가 처리를 마치거나
/// 송신 거절·큐 폐기로 job이 버려질 때 drop되어 수를 줄인다. 송신 뒤에 늘리면 워커가 먼저 끝내
/// 수가 0을 지나칠 수 있다.
struct PendingJob(Arc<AtomicUsize>);

impl PendingJob {
    fn new(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}

impl Drop for PendingJob {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 큐에 넣는 단위. `Shutdown`만 진행 중 수에 들어가지 않는다.
struct Queued {
    job: LuaJob,
    pending: Option<PendingJob>,
}

/// [`LuaEngine::submit_script`]가 돌려주는 실행 식별자. 결과의 [`RunOutcome::ticket`]과 짝을 맞춘다.
pub type RunTicket = u64;

/// 실행 요청을 접수하지 않은 이유. job과 함께 완료 토큰도 drop된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunRejected {
    /// job 큐가 가득 찼거나, 메인이 아직 꺼내지 않은 결과가 결과 슬롯을 모두 차지했다.
    QueueFull,
    WorkerGone,
}

/// 워커에서 끝난 스크립트의 실패.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunFailure {
    Error(String),
    /// 실행 기한을 넘겨 중단했다.
    TimedOut,
}

/// 제출한 스크립트 한 건의 결과. 워커가 결과 채널로 보낸다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub ticket: RunTicket,
    pub result: Result<(), RunFailure>,
}

/// 결과 슬롯 수. 접수할 때 슬롯 하나를 잡고 메인이 결과를 꺼낼 때 돌려준다.
/// 결과 채널 용량과 같으므로 접수한 결과는 채널이 가득 차서 버려지지 않는다.
/// 슬롯이 모두 차 있으면 새 요청은 접수 단계에서 [`RunRejected::QueueFull`]로 거절한다.
const OUTCOME_QUEUE_CAP: usize = 256;

type OutcomeWaker = Arc<OnceLock<Arc<dyn Fn() + Send + Sync>>>;

/// 워커 스레드에 보내는 실행 요청. 워커는 이들을 **직렬**로 처리한다.
enum LuaJob {
    /// 임의 소스 실행 + 결과 회신 (블로킹 호출용 — 테스트/부팅 로드).
    Eval {
        source: String,
        name: Option<String>,
        reply: SyncSender<Result<(), LuaEngineError>>,
    },
    /// 임의 소스 실행 (메인을 막지 않음 — 단축키/디버그/자동실행 트리거).
    Run {
        source: String,
        name: Option<String>,
        /// 있으면 실행 결과를 결과 채널로 보낸다.
        ticket: Option<RunTicket>,
        /// 결과를 보낸 뒤 drop 되어 완료를 알린다 (자동실행 경로만 Some).
        token: Option<CompletionToken>,
    },
    /// 이벤트 hook 발화 (observe-only, fire-and-forget).
    Fire {
        event: String,
        ctx: serde_json::Value,
    },
    /// 등록 hook 전부 제거 + 결과 회신.
    ResetHooks {
        reply: SyncSender<Result<(), LuaEngineError>>,
    },
    /// 워커 루프 종료.
    Shutdown,
}

/// 호스트 측 Lua 워커 핸들. VM 자체는 워커 스레드가 소유하며, 메인은 이 핸들로만 통신한다.
pub struct LuaEngine {
    job_tx: SyncSender<Queued>,
    command_rx: Receiver<HostCommand>,
    outcome_rx: Receiver<RunOutcome>,
    outcome_waker: OutcomeWaker,
    /// 접수했지만 메인이 아직 꺼내지 않은 결과 수.
    reserved_outcomes: AtomicUsize,
    next_ticket: AtomicU64,
    snapshot: SharedSnapshot,
    /// 보냈지만 아직 끝나지 않은 job 수. [`PendingJob`]이 센다.
    pending_jobs: Arc<AtomicUsize>,
    published: AtomicU64,
    worker: Option<JoinHandle<()>>,
}

impl LuaEngine {
    /// 새 VM 생성 + 샌드박스 + API 설치 후 워커 스레드로 이동. 앱 부팅 시 1회.
    pub fn new() -> Result<Self, LuaEngineError> {
        Self::with_deadline(SCRIPT_DEADLINE)
    }

    /// [`LuaEngine::new`] 와 동일하되 스크립트 실행 deadline 을 지정한다 (테스트용 짧은 budget).
    pub fn with_deadline(budget: Duration) -> Result<Self, LuaEngineError> {
        let lua = Lua::new();
        crate::sandbox::apply(&lua)?;

        let (command_tx, command_rx) = sync_channel(COMMAND_QUEUE_CAP);
        let snapshot: SharedSnapshot = Arc::new(Mutex::new(Arc::new(LuaSnapshot::default())));
        let deadline: SharedDeadline = Arc::new(Mutex::new(None));

        install_hook_api(&lua)?;
        let hit: DeadlineHit = Arc::new(AtomicBool::new(false));
        install_interrupt(&lua, deadline.clone(), hit.clone());
        crate::host_api::install(&lua, command_tx, snapshot.clone())?;

        let (job_tx, job_rx) = sync_channel(JOB_QUEUE_CAP);
        let (outcome_tx, outcome_rx) = sync_channel(OUTCOME_QUEUE_CAP);
        let outcome_waker: OutcomeWaker = Arc::new(OnceLock::new());
        let worker_outcomes = Outcomes {
            tx: outcome_tx,
            waker: outcome_waker.clone(),
        };
        let worker = std::thread::Builder::new()
            .name("tasty-lua-worker".to_string())
            .spawn(move || worker_loop(lua, job_rx, budget, (deadline, hit), worker_outcomes))?;

        Ok(Self {
            job_tx,
            command_rx,
            outcome_rx,
            outcome_waker,
            reserved_outcomes: AtomicUsize::new(0),
            next_ticket: AtomicU64::new(1),
            snapshot,
            pending_jobs: Arc::new(AtomicUsize::new(0)),
            published: AtomicU64::new(0),
            worker: Some(worker),
        })
    }

    /// 최신 스냅샷을 교체한다. 임계구역은 완성된 Arc의 교체이므로 poison을 복구한다.
    /// poison 때문에 발행을 포기하면 이후에도 낡은 스냅샷만 남는다.
    pub fn publish_snapshot(&self, snap: LuaSnapshot) {
        *tasty_utils::poison::recover_mutex(
            self.snapshot.lock(),
            "the tasty-lua read snapshot",
            &SNAPSHOT_POISON_REPORTED,
        ) = Arc::new(snap);
        self.published.fetch_add(1, Ordering::Relaxed);
    }

    /// 지금까지 [`LuaEngine::publish_snapshot`]을 부른 횟수. 발행 조건을 시험과 진단에서 확인한다.
    pub fn published_snapshots(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// 보냈지만 워커가 아직 처리를 마치지 않은 job 수. 큐에서 기다리는 job도 센다.
    /// `eval`·`reset_hooks`의 응답은 이 수가 줄기 전에 도착할 수 있다.
    pub fn pending_jobs(&self) -> usize {
        self.pending_jobs.load(Ordering::SeqCst)
    }

    /// 진행 중 수를 늘린 뒤 job을 큐에 넣는다. 거절되면 돌려받은 job과 함께 수가 되돌아간다.
    fn try_queue(&self, job: LuaJob) -> Result<(), TrySendError<LuaJob>> {
        let pending = Some(PendingJob::new(&self.pending_jobs));
        self.job_tx
            .try_send(Queued { job, pending })
            .map_err(|e| match e {
                TrySendError::Full(q) => TrySendError::Full(q.job),
                TrySendError::Disconnected(q) => TrySendError::Disconnected(q.job),
            })
    }

    /// 워커가 쌓아둔 커맨드를 모두 꺼낸다. 메인이 안전지점에서 호출해 적용한다.
    pub fn drain_commands(&self) -> Vec<HostCommand> {
        self.command_rx.try_iter().collect()
    }

    /// 이벤트 hook 발화 (fire-and-forget). 워커가 등록 콜백을 순서대로 호출.
    pub fn fire(&self, event: &str, ctx: &serde_json::Value) {
        self.send_ff(
            LuaJob::Fire {
                event: event.to_string(),
                ctx: ctx.clone(),
            },
            "fire",
        );
    }

    /// 스크립트 소스를 워커에서 실행 (fire-and-forget, 결과 없음). 디버그 트리거용.
    pub fn run_script(&self, source: &str, name: Option<&str>) {
        self.send_ff(
            LuaJob::Run {
                source: source.to_string(),
                name: name.map(str::to_string),
                ticket: None,
                token: None,
            },
            "run_script",
        );
    }

    /// 스크립트 실행을 요청하고 결과를 [`LuaEngine::drain_run_outcomes`]로 돌려받는다.
    /// 메인을 막지 않는다. 큐 포화·결과 슬롯 소진·워커 부재는 곧바로 Err이고 결과는 오지 않는다.
    /// Ok로 접수한 요청은 결과 슬롯을 잡았으므로 결과가 반드시 채널에 들어간다.
    /// `token`은 결과를 보낸 뒤 drop된다. 실행되지 못한 job과 함께 버려질 때도 drop된다.
    /// 자동실행 재진입 가드가 이 신호로 실행 중인 job을 추적한다.
    pub fn submit_script(
        &self,
        source: &str,
        name: Option<&str>,
        token: Option<CompletionToken>,
    ) -> Result<RunTicket, RunRejected> {
        if self
            .reserved_outcomes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < OUTCOME_QUEUE_CAP).then_some(n + 1)
            })
            .is_err()
        {
            return Err(RunRejected::QueueFull);
        }
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        let job = LuaJob::Run {
            source: source.to_string(),
            name: name.map(str::to_string),
            ticket: Some(ticket),
            token,
        };
        let rejected = match self.try_queue(job) {
            Ok(()) => return Ok(ticket),
            Err(TrySendError::Full(_)) => RunRejected::QueueFull,
            Err(TrySendError::Disconnected(_)) => RunRejected::WorkerGone,
        };
        self.reserved_outcomes.fetch_sub(1, Ordering::AcqRel);
        Err(rejected)
    }

    /// 워커가 보낸 실행 결과를 모두 꺼내고 그 수만큼 결과 슬롯을 돌려준다. 메인이 안전지점에서 호출한다.
    pub fn drain_run_outcomes(&self) -> Vec<RunOutcome> {
        let outcomes: Vec<RunOutcome> = self.outcome_rx.try_iter().collect();
        if !outcomes.is_empty() {
            self.reserved_outcomes
                .fetch_sub(outcomes.len(), Ordering::AcqRel);
        }
        outcomes
    }

    /// 결과를 보낼 때 메인 루프를 깨울 함수를 한 번 등록한다.
    pub fn set_outcome_waker(&self, waker: Arc<dyn Fn() + Send + Sync>) {
        if self.outcome_waker.set(waker).is_err() {
            tracing::debug!(target: "tasty_lua", "outcome waker already installed");
        }
    }

    /// 임의 Lua 소스 실행 (블로킹 — 완료까지 대기해 결과 반환). 테스트/디버그 용도.
    pub fn eval(&self, source: &str) -> Result<(), LuaEngineError> {
        self.eval_named(source, None)
    }

    fn eval_named(&self, source: &str, name: Option<&str>) -> Result<(), LuaEngineError> {
        let (reply_tx, reply_rx) = sync_channel(1);
        self.try_queue(LuaJob::Eval {
            source: source.to_string(),
            name: name.map(str::to_string),
            reply: reply_tx,
        })
        .map_err(|_| LuaEngineError::WorkerGone)?;
        reply_rx.recv().map_err(|_| LuaEngineError::WorkerGone)?
    }

    /// 등록 hook 을 모두 비운다 (블로킹). reload 직전에 호출.
    pub fn reset_hooks(&self) -> Result<(), LuaEngineError> {
        let (reply_tx, reply_rx) = sync_channel(1);
        self.try_queue(LuaJob::ResetHooks { reply: reply_tx })
            .map_err(|_| LuaEngineError::WorkerGone)?;
        reply_rx.recv().map_err(|_| LuaEngineError::WorkerGone)?
    }

    /// fire-and-forget job 전송. 큐 포화/워커 종료 시 drop + warn (메인은 블록되지 않는다).
    fn send_ff(&self, job: LuaJob, what: &str) {
        match self.try_queue(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!(target: "tasty_lua", "lua job queue full — dropping {what}")
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::warn!(target: "tasty_lua", "lua worker gone — dropping {what}")
            }
        }
    }
}

impl LuaEngine {
    /// Shutdown을 보내고 현재 job을 마친 워커를 join한다.
    fn join_worker(&mut self) {
        // 큐가 가득 차 있어도 Shutdown 은 반드시 도달해야 하므로 블로킹 send 사용.
        // 이미 워커가 죽어 disconnect 여도 정상 — 아래 join 이 정리한다.
        let shutdown = Queued {
            job: LuaJob::Shutdown,
            pending: None,
        };
        if let Err(e) = self.job_tx.send(shutdown) {
            tracing::trace!(target: "tasty_lua", "shutdown send skipped (worker gone): {e}");
        }
        if let Some(handle) = self.worker.take()
            && let Err(e) = handle.join()
        {
            tracing::warn!(target: "tasty_lua", "lua worker join failed: {e:?}");
        }
    }
}

impl Drop for LuaEngine {
    fn drop(&mut self) {
        // 종료 대기 시간을 별도로 기록해 현재 job의 join 지연을 확인한다.
        let t_join = std::time::Instant::now();
        self.join_worker();
        tracing::info!(
            target: "tasty::shutdown",
            ms = t_join.elapsed().as_secs_f64() * 1000.0,
            "S5a lua_join (Shutdown send + worker join)"
        );
    }
}

/// job을 직렬로 실행한다. Lua 명령 hook이 기한 초과를 중단하며 다음 job은 계속 처리한다.
/// Shutdown 또는 채널 종료를 받으면 루프를 끝낸다.
#[allow(clippy::cognitive_complexity)] // complexity-exempt: LuaJob 평면 match 디스패치 — job 종류별 guarded exec + reply/로그 나열. 워커 스레드가 job 을 직렬 처리하는 단일 루프라 쪼개면 job 별 소함수만 늘고 흐름 추적이 어려워짐, 중첩 얕음.
fn worker_loop(
    lua: Lua,
    job_rx: Receiver<Queued>,
    budget: Duration,
    (deadline, hit): (SharedDeadline, DeadlineHit),
    outcomes: Outcomes,
) {
    // `pending`은 match 뒤에 drop되어 처리를 마친 job만 진행 중 수에서 빠진다.
    // 루프가 끝나 채널이 drop되면 큐에 남은 job도 함께 정산된다.
    while let Ok(Queued { job, pending }) = job_rx.recv() {
        match job {
            LuaJob::Eval {
                source,
                name,
                reply,
            } => {
                let result = guarded(&deadline, budget, || {
                    exec_source(&lua, &source, name.as_deref())
                });
                // reply 실패 = 호출자가 대기를 포기(receiver drop) — 무시해도 안전.
                if let Err(e) = reply.send(result) {
                    tracing::trace!(target: "tasty_lua", "eval reply dropped: {e}");
                }
            }
            LuaJob::Run {
                source,
                name,
                ticket,
                token,
            } => {
                let result = run_job(&lua, (&deadline, &hit), budget, &source, name.as_deref());
                if let Some(ticket) = ticket {
                    outcomes.send(RunOutcome { ticket, result });
                }
                // 실행이 끝난 뒤에야 완료를 알린다 — 재진입 가드의 in-flight 판정 기준.
                drop(token);
            }
            LuaJob::Fire { event, ctx } => {
                // 콜백별 오류는 fire_hooks가 기록한다. job 전체에도 실행 기한을 적용한다.
                if let Err(e) = guarded(&deadline, budget, || {
                    fire_hooks(&lua, &event, &ctx);
                    Ok(())
                }) {
                    tracing::warn!(target: "tasty_lua", "fire '{event}' aborted: {e}");
                }
            }
            LuaJob::ResetHooks { reply } => {
                if let Err(e) = reply.send(reset_hooks(&lua)) {
                    tracing::trace!(target: "tasty_lua", "reset_hooks reply dropped: {e}");
                }
            }
            LuaJob::Shutdown => break,
        }
        drop(pending);
    }
}

/// 스크립트 하나를 실행하고 실패 사유를 나눈다. 기한 hook의 오류는 mlua가 감싸므로 문구 대신
/// hook이 남긴 표지로 시간 초과를 판정하고, 그 밖의 오류는 원래 사유를 그대로 싣는다.
fn run_job(
    lua: &Lua,
    (deadline, hit): (&SharedDeadline, &DeadlineHit),
    budget: Duration,
    source: &str,
    name: Option<&str>,
) -> Result<(), RunFailure> {
    hit.store(false, Ordering::SeqCst);
    guarded(deadline, budget, || exec_source(lua, source, name)).map_err(|e| {
        tracing::warn!(target: "tasty_lua", "script run failed: {e}");
        if hit.swap(false, Ordering::SeqCst) {
            RunFailure::TimedOut
        } else {
            RunFailure::Error(first_line(&e))
        }
    })
}

/// 워커 쪽 결과 채널. 보내기는 막히지 않는다.
struct Outcomes {
    tx: SyncSender<RunOutcome>,
    waker: OutcomeWaker,
}

impl Outcomes {
    fn send(&self, outcome: RunOutcome) {
        match self.tx.try_send(outcome) {
            Ok(()) => {
                if let Some(wake) = self.waker.get() {
                    wake();
                }
            }
            // 접수할 때 슬롯을 잡았으므로 정상 흐름에서는 일어나지 않는다. 일어났다면 슬롯 계산이 틀렸다.
            Err(TrySendError::Full(o)) => {
                tracing::error!(target: "tasty_lua", ticket = o.ticket, "run outcome channel full despite a reserved slot")
            }
            // 엔진이 사라져 결과를 받을 쪽이 없다(종료 중).
            Err(TrySendError::Disconnected(o)) => {
                tracing::debug!(target: "tasty_lua", ticket = o.ticket, "run outcome receiver gone")
            }
        }
    }
}

/// 사용자에게 보일 실패 사유. Lua 오류의 stack traceback은 로그에만 남긴다.
fn first_line(e: &LuaEngineError) -> String {
    let text = match e {
        LuaEngineError::Eval(inner) => inner.to_string(),
        other => other.to_string(),
    };
    text.lines().next().unwrap_or_default().trim().to_string()
}

/// 스냅샷 락 poison은 한 번만 보고한다.
static SNAPSHOT_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// deadline 락 poison 을 첫 1 회만 보고한다(1 만 명령마다 도는 VM 훅이다).
static DEADLINE_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

const INTERRUPT_EVERY_N_INSTRUCTIONS: u32 = 10_000;

/// Lua 5.4의 명령 수 기반 set_hook으로 실행 기한을 확인한다.
fn install_interrupt(lua: &Lua, deadline: SharedDeadline, hit: DeadlineHit) {
    let triggers = mlua::HookTriggers::new().every_nth_instruction(INTERRUPT_EVERY_N_INSTRUCTIONS);
    lua.set_hook(triggers, move |_, _| {
        // 읽기 전용 임계구역의 poison을 복구해 실행 기한 검사가 꺼지지 않게 한다.
        let expired = tasty_utils::poison::recover_mutex(
            deadline.lock(),
            "the tasty-lua script deadline",
            &DEADLINE_POISON_REPORTED,
        )
        .is_some_and(|dl| Instant::now() >= dl);
        if expired {
            hit.store(true, Ordering::SeqCst);
            Err(mlua::Error::runtime(
                "script exceeded time budget (deadline)",
            ))
        } else {
            Ok(mlua::VmState::Continue)
        }
    });
}

/// job deadline 을 설정한 뒤 `f` 를 실행하고, 종료 시 deadline 을 clear 한다.
fn guarded<F>(deadline: &SharedDeadline, budget: Duration, f: F) -> Result<(), LuaEngineError>
where
    F: FnOnce() -> Result<(), LuaEngineError>,
{
    set_deadline(deadline, Some(Instant::now() + budget));
    let result = f();
    set_deadline(deadline, None);
    result
}

fn set_deadline(deadline: &SharedDeadline, value: Option<Instant>) {
    // Option<Instant> 대입의 poison을 복구한다. 쓰기를 포기하면 기한이 설정되지 않는다.
    *tasty_utils::poison::recover_mutex(
        deadline.lock(),
        "the tasty-lua script deadline",
        &DEADLINE_POISON_REPORTED,
    ) = value;
}

/// `tasty.on` dispatcher API 설치 (VM 생성 시 1회). host_api 는 별도.
fn install_hook_api(lua: &Lua) -> Result<(), LuaEngineError> {
    let registry = lua.create_table().map_err(LuaEngineError::Init)?;
    lua.set_named_registry_value(HOOKS_REGISTRY_KEY, registry)
        .map_err(LuaEngineError::Init)?;

    let tasty_table = lua.create_table().map_err(LuaEngineError::Init)?;

    let on = lua
        .create_function(|lua, (event, cb): (String, Function)| {
            let reg: Table = lua.named_registry_value(HOOKS_REGISTRY_KEY)?;
            let list: Table = match reg.get::<Value>(event.as_str())? {
                Value::Table(t) => t,
                _ => {
                    let t = lua.create_table()?;
                    reg.set(event.as_str(), t.clone())?;
                    t
                }
            };
            list.push(cb)?;
            Ok(())
        })
        .map_err(LuaEngineError::Init)?;
    tasty_table.set("on", on).map_err(LuaEngineError::Init)?;

    lua.globals()
        .set("tasty", tasty_table)
        .map_err(LuaEngineError::Init)?;
    Ok(())
}

/// 등록된 hook 을 모두 비운다 (워커 스레드 컨텍스트).
fn reset_hooks(lua: &Lua) -> Result<(), LuaEngineError> {
    let registry = lua.create_table().map_err(LuaEngineError::Init)?;
    lua.set_named_registry_value(HOOKS_REGISTRY_KEY, registry)
        .map_err(LuaEngineError::Init)?;
    Ok(())
}

/// 임의 소스를 text-only 모드로 실행 (워커 스레드 컨텍스트).
fn exec_source(lua: &Lua, source: &str, name: Option<&str>) -> Result<(), LuaEngineError> {
    let mut chunk = lua.load(source).set_mode(mlua::ChunkMode::Text);
    if let Some(n) = name {
        chunk = chunk.set_name(n);
    }
    chunk.exec().map_err(LuaEngineError::Eval)
}

/// 이벤트 hook 발화 — 등록 콜백을 순서대로 호출. 콜백 에러는 warn 로그만 (한 hook 이 전체를 막지 않게).
#[allow(clippy::cognitive_complexity)] // complexity-exempt: registry/ctx 조회 단계별 조기 반환 체인 + 콜백 순회 — 콜백 에러를 개별 격리(한 hook 실패가 전체를 막지 않음)하려면 단계마다 match 가 필요, 중첩은 얕음.
fn fire_hooks(lua: &Lua, event: &str, ctx: &serde_json::Value) {
    let reg: Table = match lua.named_registry_value(HOOKS_REGISTRY_KEY) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(target: "tasty_lua", "hook registry missing: {e}");
            return;
        }
    };
    let list: Table = match reg.get::<Value>(event) {
        Ok(Value::Table(t)) => t,
        _ => return,
    };
    let lua_ctx = match lua.to_value(ctx) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(target: "tasty_lua", "ctx serialize failed for '{event}': {e}");
            return;
        }
    };
    let len = list.raw_len();
    for i in 1..=len {
        let cb: Function = match list.get(i) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Err(e) = cb.call::<()>(lua_ctx.clone()) {
            tracing::warn!(target: "tasty_lua", "hook for '{event}' failed: {e}");
        }
    }
}

#[cfg(test)]
#[path = "engine_pending_tests.rs"]
mod pending_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_engine_runs_basic_lua() {
        let engine = LuaEngine::new().expect("init");
        engine.eval("local x = 1 + 1").expect("eval");
    }

    #[test]
    fn memory_limit_blocks_huge_string() {
        let engine = LuaEngine::new().expect("init");
        let result = engine.eval("local s = string.rep('a', 64 * 1024 * 1024)");
        assert!(result.is_err());
    }

    #[test]
    fn debug_library_removed() {
        let engine = LuaEngine::new().expect("init");
        assert!(engine.eval("return debug.getinfo").is_err());
    }

    #[test]
    fn native_module_loaders_removed() {
        let engine = LuaEngine::new().expect("init");
        // require가 남아 있어도 native 로더에 접근할 수 없는지 Lua 안에서 확인한다.
        engine
            .eval("if package.loadlib ~= nil then error('loadlib still reachable') end")
            .expect("package.loadlib 가 남아 있다");
        engine
            .eval("if package.searchers ~= nil then error('searchers still reachable') end")
            .expect("package.searchers 가 남아 있다");
        engine
            .eval("if package.cpath ~= '' then error('cpath not cleared') end")
            .expect("package.cpath 가 비워지지 않았다");
    }

    #[test]
    fn loadstring_removed() {
        let engine = LuaEngine::new().expect("init");
        assert!(engine.eval("loadstring('print(1)')").is_err());
    }

    #[test]
    fn tasty_on_registers_callback() {
        let engine = LuaEngine::new().expect("init");
        engine
            .eval(
                r#"
                tasty.on("test.event", function(ctx)
                    _G.last_received = ctx.value
                end)
                "#,
            )
            .expect("register");

        engine.fire("test.event", &json!({ "value": 42 }));
        // fire 는 fire-and-forget 이지만 직렬 워커라 다음 블로킹 eval 이 fire 뒤에 처리됨.
        engine
            .eval("assert(_G.last_received == 42)")
            .expect("callback fired");
    }

    #[test]
    fn fire_with_no_listeners_is_noop() {
        let engine = LuaEngine::new().expect("init");
        engine.fire("nothing.subscribes", &json!({}));
        engine
            .eval("return 1")
            .expect("worker alive after empty fire");
    }

    #[test]
    fn multiple_callbacks_per_event_all_fire() {
        let engine = LuaEngine::new().expect("init");
        engine
            .eval(
                r#"
                _G.counter = 0
                tasty.on("ping", function(_) _G.counter = _G.counter + 1 end)
                tasty.on("ping", function(_) _G.counter = _G.counter + 10 end)
                "#,
            )
            .unwrap();
        engine.fire("ping", &json!({}));
        engine.eval("assert(_G.counter == 11)").unwrap();
    }

    #[test]
    fn callback_error_does_not_abort_others() {
        let engine = LuaEngine::new().expect("init");
        engine
            .eval(
                r#"
                _G.second_ran = false
                tasty.on("e", function(_) error("boom") end)
                tasty.on("e", function(_) _G.second_ran = true end)
                "#,
            )
            .unwrap();
        engine.fire("e", &json!({}));
        engine.eval("assert(_G.second_ran == true)").unwrap();
    }

    #[test]
    fn reset_hooks_clears_registrations() {
        let engine = LuaEngine::new().expect("init");
        engine
            .eval(
                r#"
                _G.fired = false
                tasty.on("x", function(_) _G.fired = true end)
                "#,
            )
            .unwrap();
        engine.reset_hooks().unwrap();
        engine.fire("x", &serde_json::json!({}));
        engine.eval("assert(_G.fired == false)").unwrap();
    }

    #[test]
    fn run_script_is_serialized_with_eval() {
        let engine = LuaEngine::new().expect("init");
        engine.run_script("_G.ran = (_G.ran or 0) + 1", Some("t"));
        engine.run_script("_G.ran = _G.ran + 1", Some("t"));
        engine.eval("assert(_G.ran == 2)").expect("serial order");
    }

    #[test]
    fn write_api_enqueues_host_command() {
        let engine = LuaEngine::new().expect("init");
        engine.eval("tasty.run_cli('list')").expect("run_cli");
        let cmds = engine.drain_commands();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            HostCommand::RunCli(args) => assert_eq!(args, &vec!["list".to_string()]),
        }
    }

    /// poison 뒤 새 스냅샷이 실제로 발행되는지 확인한다.
    #[test]
    fn a_poisoned_snapshot_lock_still_publishes() {
        let engine = LuaEngine::new().expect("init");
        engine.publish_snapshot(LuaSnapshot {
            tree: vec![serde_json::json!({"id": 1})],
        });

        let poisoner = Arc::clone(&engine.snapshot);
        std::thread::spawn(move || {
            let _g = poisoner.lock().expect("fresh lock");
            panic!("poison the snapshot on purpose");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");
        assert!(engine.snapshot.is_poisoned(), "전제: 락이 poison 이다");

        engine.publish_snapshot(LuaSnapshot {
            tree: vec![serde_json::json!({"id": 2}), serde_json::json!({"id": 3})],
        });

        let snap = tasty_utils::poison::recover_mutex(
            engine.snapshot.lock(),
            "test read",
            &SNAPSHOT_POISON_REPORTED,
        )
        .clone();
        assert_eq!(
            snap.tree.len(),
            2,
            "poison 뒤의 발행이 반영돼야 한다 — 낡은 트리가 남으면 영구 정지다"
        );
    }

    #[test]
    fn publish_snapshot_replaces_value() {
        let engine = LuaEngine::new().expect("init");
        engine.publish_snapshot(LuaSnapshot {
            tree: vec![serde_json::json!({"id": 1})],
        });
        let snap = engine.snapshot.lock().unwrap().clone();
        assert_eq!(snap.tree.len(), 1);
    }

    #[test]
    fn tree_api_returns_published_snapshot() {
        let engine = LuaEngine::new().expect("init");
        engine.publish_snapshot(LuaSnapshot {
            tree: vec![
                serde_json::json!({"active": true, "panes": []}),
                serde_json::json!({"active": false, "panes": []}),
            ],
        });
        engine
            .eval(
                r#"
                local t = tasty.tree()
                assert(#t == 2, "two workspaces")
                assert(t[1].active == true, "first active")
                assert(t[2].active == false, "second inactive")
                "#,
            )
            .expect("tree read");
    }

    #[test]
    fn tree_api_empty_when_nothing_published() {
        let engine = LuaEngine::new().expect("init");
        engine
            .eval("assert(#tasty.tree() == 0)")
            .expect("empty tree");
    }

    #[test]
    fn command_queue_backpressure_drops_over_cap() {
        let engine = LuaEngine::new().expect("init");
        let src = format!(
            "for i = 1, {} do tasty.run_cli('x') end",
            COMMAND_QUEUE_CAP + 50
        );
        engine.eval(&src).expect("bulk run_cli");
        let cmds = engine.drain_commands();
        assert!(cmds.len() <= COMMAND_QUEUE_CAP);
    }

    #[test]
    fn worker_survives_error_job() {
        let engine = LuaEngine::new().expect("init");
        assert!(engine.eval("error('boom')").is_err());
        engine
            .eval("return 1 + 1")
            .expect("worker alive after error");
    }

    /// deadline의 읽기·쓰기 양쪽이 poison 뒤에도 실행 기한을 적용해야 한다.
    #[test]
    fn a_poisoned_deadline_lock_still_aborts_a_runaway_script() {
        let deadline: SharedDeadline = Arc::new(Mutex::new(None));

        let poisoner = Arc::clone(&deadline);
        std::thread::spawn(move || {
            let _g = poisoner.lock().expect("fresh lock");
            panic!("poison the deadline on purpose");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");
        assert!(deadline.is_poisoned(), "전제: 락이 poison 이다");

        let lua = Lua::new();
        install_interrupt(&lua, deadline.clone(), Arc::new(AtomicBool::new(false)));
        set_deadline(&deadline, Some(Instant::now() + Duration::from_millis(50)));

        // 기한 검사가 고장 나도 시험이 끝나도록 유한 루프를 사용한다.
        let t0 = Instant::now();
        let err = lua
            .load("local n = 0 for i = 1, 200000000 do n = n + 1 end return n")
            .exec()
            .expect_err("예산을 넘긴 스크립트는 abort 돼야 한다");
        assert!(
            err.to_string().contains("time budget"),
            "abort 사유가 예산이어야 한다 (실제: {err})"
        );
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "예산이 실제로 집행됐다면 즉시 끊긴다 (실측 {:?})",
            t0.elapsed()
        );
    }

    #[test]
    fn infinite_loop_aborts_within_deadline() {
        let engine = LuaEngine::with_deadline(Duration::from_millis(50)).expect("init");
        let start = Instant::now();
        let result = engine.eval("while true do end");
        assert!(result.is_err(), "무한 루프는 deadline 으로 abort 되어야 함");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "deadline 내 즉시 abort (실제 {:?})",
            start.elapsed()
        );
    }

    #[test]
    fn worker_survives_after_deadline_abort() {
        let engine = LuaEngine::with_deadline(Duration::from_millis(50)).expect("init");
        assert!(engine.eval("while true do end").is_err());
        engine
            .eval("return 1 + 1")
            .expect("worker alive after deadline abort");
    }

    #[test]
    fn normal_script_completes_under_deadline() {
        let engine = LuaEngine::with_deadline(Duration::from_secs(5)).expect("init");
        engine
            .eval("local s = 0; for i = 1, 100000 do s = s + i end")
            .expect("normal loop under budget");
    }

    /// 결과를 기다린다. 워커는 직렬이므로 뒤따른 eval이 끝나면 앞선 run의 결과는 채널에 있다.
    fn outcomes_after_sync(engine: &LuaEngine) -> Vec<RunOutcome> {
        engine.eval("return 0").expect("worker alive");
        engine.drain_run_outcomes()
    }

    #[test]
    fn submitted_runs_report_success_and_the_error_reason() {
        let engine = LuaEngine::new().expect("init");
        let ok = engine
            .submit_script("local x = 1", Some("ok"), None)
            .unwrap();
        let bad = engine
            .submit_script("error('boom')", Some("bad.lua"), None)
            .unwrap();
        let outcomes = outcomes_after_sync(&engine);
        assert_eq!(outcomes.len(), 2);
        assert_eq!(
            outcomes[0],
            RunOutcome {
                ticket: ok,
                result: Ok(())
            }
        );
        assert_eq!(outcomes[1].ticket, bad);
        match &outcomes[1].result {
            Err(RunFailure::Error(reason)) => {
                assert!(
                    reason.contains("boom"),
                    "사유에 오류 문구가 있어야 한다: {reason}"
                );
                assert!(
                    !reason.contains('\n'),
                    "traceback은 사유에 싣지 않는다: {reason}"
                );
            }
            other => panic!("오류 결과여야 한다: {other:?}"),
        }
    }

    #[test]
    fn a_run_over_the_deadline_reports_timed_out() {
        let engine = LuaEngine::with_deadline(Duration::from_millis(50)).expect("init");
        let t = engine
            .submit_script("while true do end", Some("loop"), None)
            .unwrap();
        assert_eq!(
            outcomes_after_sync(&engine),
            vec![RunOutcome {
                ticket: t,
                result: Err(RunFailure::TimedOut)
            }]
        );
    }

    /// 느린 호스트 호출로 예산을 넘긴 뒤 난 일반 오류는 시간 초과가 아니라 원래 사유로 남는다.
    #[test]
    fn an_error_after_a_slow_host_call_keeps_its_reason() {
        let lua = Lua::new();
        let deadline: SharedDeadline = Arc::new(Mutex::new(None));
        let hit: DeadlineHit = Arc::new(AtomicBool::new(false));
        install_interrupt(&lua, deadline.clone(), hit.clone());
        let slow = lua
            .create_function(|_, ms: u64| {
                std::thread::sleep(Duration::from_millis(ms));
                Ok(())
            })
            .unwrap();
        lua.globals().set("slow_host_call", slow).unwrap();
        let budget = Duration::from_millis(30);
        let started = Instant::now();
        let result = run_job(
            &lua,
            (&deadline, &hit),
            budget,
            "slow_host_call(80) error('boom')",
            Some("slow.lua"),
        );
        assert!(started.elapsed() >= budget, "전제: 예산을 넘겼다");
        match result {
            Err(RunFailure::Error(reason)) => assert!(reason.contains("boom"), "{reason}"),
            other => panic!("원래 사유가 남아야 한다: {other:?}"),
        }
        assert_eq!(
            run_job(&lua, (&deadline, &hit), budget, "while true do end", None),
            Err(RunFailure::TimedOut),
            "기한 hook이 중단한 실행만 시간 초과다"
        );
        assert!(
            matches!(
                run_job(&lua, (&deadline, &hit), budget, "error('next')", None),
                Err(RunFailure::Error(_))
            ),
            "앞선 시간 초과 표지는 다음 실행에 남지 않는다"
        );
    }

    #[test]
    fn untracked_run_script_sends_no_outcome() {
        let engine = LuaEngine::new().expect("init");
        engine.run_script("error('debug')", Some("debug.lua.eval"));
        assert!(outcomes_after_sync(&engine).is_empty());
    }

    #[test]
    fn a_full_job_queue_rejects_and_releases_the_token_once() {
        let engine = LuaEngine::with_deadline(Duration::from_millis(500)).expect("init");
        let counter = Arc::new(AtomicU64::new(0));
        engine
            .submit_script("while true do end", None, None)
            .unwrap();
        let mut rejected = None;
        for _ in 0..=JOB_QUEUE_CAP + 1 {
            match engine.submit_script("local x = 1", None, None) {
                Ok(_) => continue,
                Err(e) => {
                    rejected = Some(e);
                    break;
                }
            }
        }
        assert_eq!(rejected, Some(RunRejected::QueueFull));
        let result = engine.submit_script(
            "local x = 1",
            None,
            Some(CompletionToken::new(counter.clone())),
        );
        assert_eq!(result, Err(RunRejected::QueueFull));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "거절된 job의 token은 그 자리에서 한 번 drop된다"
        );
    }

    #[test]
    fn a_gone_worker_rejects_and_releases_the_token_once() {
        let mut engine = LuaEngine::new().expect("init");
        engine.join_worker();
        let counter = Arc::new(AtomicU64::new(0));
        let result = engine.submit_script(
            "local x = 1",
            None,
            Some(CompletionToken::new(counter.clone())),
        );
        assert_eq!(result, Err(RunRejected::WorkerGone));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// 결과를 받을 쪽이 사라져도 실행을 마친 job의 token은 한 번만 해제된다.
    #[test]
    fn a_dropped_outcome_receiver_still_releases_the_token_once() {
        let mut engine = LuaEngine::new().expect("init");
        let counter = Arc::new(AtomicU64::new(0));
        let (_, fresh_rx) = sync_channel(1);
        drop(std::mem::replace(&mut engine.outcome_rx, fresh_rx));
        engine
            .submit_script(
                "error('nobody listens')",
                None,
                Some(CompletionToken::new(counter.clone())),
            )
            .expect("queued");
        engine.eval("return 0").expect("sync");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// 엔진을 drop하면 실행 중이던 job과 대기 중이던 job의 token이 각각 한 번씩 해제된다.
    #[test]
    fn dropping_the_engine_releases_every_pending_token_once() {
        let engine = LuaEngine::with_deadline(Duration::from_millis(100)).expect("init");
        let counter = Arc::new(AtomicU64::new(0));
        let token = || Some(CompletionToken::new(counter.clone()));
        engine
            .submit_script("while true do end", None, token())
            .expect("running job");
        for _ in 0..3 {
            engine
                .submit_script("local x = 1", None, token())
                .expect("queued job");
        }
        drop(engine);
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }

    /// 메인이 결과를 꺼내지 않는 동안 결과 슬롯이 다 차면 접수 단계에서 거절한다.
    /// 접수한 결과는 하나도 버리지 않고, token은 접수·거절 어느 쪽이든 한 번씩 해제된다.
    #[test]
    fn a_slow_consumer_gets_rejections_instead_of_lost_results() {
        let engine = LuaEngine::new().expect("init");
        let counter = Arc::new(AtomicU64::new(0));
        let token = || Some(CompletionToken::new(counter.clone()));
        let mut accepted = Vec::new();
        // job 큐 거절과 섞이지 않도록 조금씩 넣고 워커가 비울 때까지 기다린다.
        while accepted.len() < OUTCOME_QUEUE_CAP {
            for _ in 0..16 {
                accepted.push(engine.submit_script("error('x')", None, token()).unwrap());
            }
            engine.eval("return 0").unwrap();
        }
        assert_eq!(
            engine.submit_script("error('x')", None, token()),
            Err(RunRejected::QueueFull),
            "결과 슬롯이 없으면 접수하지 않는다"
        );
        assert_eq!(counter.load(Ordering::SeqCst), OUTCOME_QUEUE_CAP as u64 + 1);

        let tickets: Vec<RunTicket> = engine
            .drain_run_outcomes()
            .into_iter()
            .map(|o| o.ticket)
            .collect();
        assert_eq!(tickets, accepted, "접수한 결과는 모두 순서대로 돌아온다");
        assert!(
            engine.submit_script("local x = 1", None, None).is_ok(),
            "결과를 꺼내면 슬롯이 돌아온다"
        );
    }

    #[test]
    fn a_rejected_submission_returns_its_result_slot() {
        let mut engine = LuaEngine::new().expect("init");
        engine.join_worker();
        for _ in 0..OUTCOME_QUEUE_CAP + 1 {
            assert_eq!(
                engine.submit_script("local x = 1", None, None),
                Err(RunRejected::WorkerGone)
            );
        }
        assert_eq!(engine.reserved_outcomes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_outcome_waker_runs_per_delivered_outcome() {
        let engine = LuaEngine::new().expect("init");
        let woke = Arc::new(AtomicU64::new(0));
        let w = woke.clone();
        engine.set_outcome_waker(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        engine.submit_script("local x = 1", None, None).unwrap();
        engine.submit_script("error('x')", None, None).unwrap();
        assert_eq!(outcomes_after_sync(&engine).len(), 2);
        assert_eq!(woke.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn tracked_run_signals_completion_after_execution() {
        let engine = LuaEngine::new().expect("init");
        let counter = Arc::new(AtomicU64::new(0));
        engine
            .submit_script(
                "local x = 1",
                Some("t"),
                Some(CompletionToken::new(counter.clone())),
            )
            .expect("queued");
        // 워커는 job 을 직렬 처리하므로, 후속 블로킹 eval 이 돌아오면 run 은 끝난 상태.
        engine.eval("return 0").expect("worker alive");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn tracked_run_completes_even_on_deadline_abort() {
        // 자동실행 경로(LuaJob::Run)도 deadline 이 그대로 적용된다 — 무한 루프는
        // abort 되고, abort 여도 완료 token 은 drop 되어 가드가 풀린다.
        let engine = LuaEngine::with_deadline(Duration::from_millis(50)).expect("init");
        let counter = Arc::new(AtomicU64::new(0));
        engine
            .submit_script(
                "while true do end",
                Some("loop"),
                Some(CompletionToken::new(counter.clone())),
            )
            .expect("queued");
        engine.eval("return 0").expect("worker alive after abort");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
