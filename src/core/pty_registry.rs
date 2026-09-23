//! 호스트 headless PTY registry (`pty.*` primitive — Registry + IO. ADR-0050 ·
//! features/headless-pty 참고).
//!
//! 에이전트가 Surface(Tab) 없이 백그라운드에서 굴리는 **headless PTY** 의 메타데이터와
//! 실제 종료코드(exit-code)를 호스트가 단일 SoT 로 보관한다. `child_terminal.rs` 의
//! [`ChildTerminalRegistry`](crate::core::child_terminal::ChildTerminalRegistry) 와
//! 병렬 구조지만 역할이 다르다:
//!
//! - `child_terminal`: `terminal.spawn` 으로 만든 **자식 터미널 surface** 의 parent/index/
//!   idle 매핑 (ADR-0040 occupancy, GUI 에 보이는 장수명 child-agent). Surface 가 있다.
//! - `pty_registry`(본 모듈): Surface 가 아예 없는 1 회성 자동화 PTY. `child.wait()` 로
//!   **진짜 exit-code** 를 잡고, GUI 안전망(닫기 버튼)이 없으므로 **동시 개수 상한 +
//!   idle TTL** 로 좀비 누적을 스스로 막는다.
//!
//! **경계 (레지스트리 파편화 방지)**: `session.rs` 의 `SessionStore`(권한 토큰),
//! `runner_host.rs` 의 `shell_children`(DAG 러너 subprocess), `child_terminal.rs`(자식
//! 터미널 surface) 와 모두 다른 서브시스템이며 통합 대상이 아니다.
//!
//! host-IPC-free — 단위 테스트 가능. 시간 의존 연산([`register`](PtyRegistry::register)/
//! [`touch`](PtyRegistry::touch)/[`sweep_idle`](PtyRegistry::sweep_idle))은 `now: Instant`
//! 를 주입받아 테스트가 5 분 경과를 sleep 없이 재현한다.
//!
//! **비영속**: headless PTY 자식 프로세스는 호스트와 수명을 같이하므로(재부팅 후 살아있는
//! PTY 는 없다) `child_terminal` 과 달리 JSON 영속화하지 않는다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// 동시 headless PTY 개수 기본 상한 (사용자 확정, 2026-07-14). `rate_limit.rs` 철학대로
/// 코드에 기본값을 박아두되 [`PtyRegistry::with_limits`] 로 override 가능하다.
pub const DEFAULT_MAX_CONCURRENT: usize = 8;

/// idle(무 IO 활동) 상태가 이 시간을 넘으면 [`sweep_idle`](PtyRegistry::sweep_idle) 이
/// 정리 대상으로 반환한다. 기본 5 분 (사용자 확정, 2026-07-14).
pub const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(300);

/// PTY id 시작값. u32 키스페이스를 둘로 가르는 경계다 — `[1, PTY_ID_BASE)` 는 surface id
/// 공간, `[PTY_ID_BASE, u32::MAX]` 는 PTY id 공간. headless `Terminal` 이 surface id 와 같은
/// `TerminalStore` 에 재사용 등록돼도 실 surface id 와 겹치지 않게 하는 근거다.
///
/// **이 disjoint 는 상수 하나로 저절로 성립하지 않는다 — 세 방어가 강제한다**
/// (`docs/adr/0094-surface-id-space-bounded-below-pty-base.md`):
///
/// 1. **호스트 내부 쓰기 방어** — OSC 133 명령 인덱싱
///    ([`CommandIndex::on_boundary`](crate::core::command_index::CommandIndex::on_boundary))은
///    `TerminalStore` 키를 그대로 surface id 로 받는데, headless PTY 의 `Terminal` 은 그
///    store 에 pty id 로 등록돼 있다. 이 값 이상으로 들어온 boundary 는 인덱싱하지 않는다 —
///    하면 `Scope::Surface(pty id)` 가 memory.db 에 심긴다.
/// 2. **floor 시딩 방어** — 복원 직전 surface 카운터 floor 를 memory.db 에서 시딩할 때
///    ([`seed_surface_id_floor`](crate::core::impl_workspace::seed_surface_id_floor)) PTY
///    공간을 침범한 `Scope::Surface` 는 floor 산정에서 제외하고 그 자리에서 purge 한다.
///    이것이 없으면 오염된 scope 하나가 카운터를 영구히 PTY 공간으로 밀어 올린다(비가역
///    래칫).
/// 3. **입력 방어** — IPC 가 `surface_id` 파라미터 / `scope=surface:<id>` 로 이 값 이상을
///    받으면 거부한다([`is_surface_id_space`]).
///
/// 방어가 없던 시절 실사용 인스턴스의 surface id 가 실제로 2^31 을 넘긴 사례가 있으므로,
/// "surface id 가 2^31 까지 자랄 일은 없다" 는 가정이 아니라 위 세 방어의 **결과** 다.
pub const PTY_ID_BASE: u32 = 0x8000_0000;

/// `id` 가 surface id 공간(`< PTY_ID_BASE`)에 속하는가. surface id 를 받는 모든 경계
/// (IPC 파라미터 검증, memory scope 검증, 카운터 floor 시딩)가 이 술어 하나를 공유해
/// 경계값 해석이 갈리지 않게 한다.
pub const fn is_surface_id_space(id: u32) -> bool {
    id < PTY_ID_BASE
}

/// headless PTY 자식의 실제 종료 결과. `runner_host.rs` 의
/// `shell_outcome_from_status(pid, code, success)` 와 동형(pid 는 registry 가 이미 id 로
/// 귀속하므로 생략).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyExit {
    /// OS exit code. 시그널 종료 등 code 가 없는 경우 `None`.
    pub code: Option<i32>,
    /// `ExitStatus::success()` — code == 0.
    pub success: bool,
}

impl PtyExit {
    pub fn from_status(code: Option<i32>, success: bool) -> Self {
        Self { code, success }
    }
}

/// [`PtyRegistry::register`] 실패 사유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtySpawnError {
    /// 동시 개수 상한 초과. spawn 요청을 실패시켜야 하며 panic 하지 않는다.
    LimitReached { current: usize, max: usize },
}

impl std::fmt::Display for PtySpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtySpawnError::LimitReached { current, max } => write!(
                f,
                "headless PTY concurrency limit reached ({current}/{max})"
            ),
        }
    }
}

impl std::error::Error for PtySpawnError {}

/// exit-watcher 가 어디까지 갔는지. **cell 이 안 찬 이유를 가르는 값**이다.
///
/// 왜 필요한가: 대기가 상한을 다 쓰면 `exit` 는 `None` 하나만 낸다. 그 `None` 은 두 개의
/// 다른 사건을 같은 얼굴로 낸다 — 자식이 안 죽어서 `wait()` 가 아직 안 돌아온 것과,
/// 자식은 죽었는데 우리 쪽이 그것을 잡을 자리에 애초에 못 간 것(스레드 생성 실패,
/// 스케줄 못 받음). 뒤쪽은 지금까지 **어디에도 안 보였다**. 위상은 그 둘을 가른다.
///
/// **테스트 전용**(`#[cfg(test)]`) — 이 값을 읽는 자리가 실패 갈래 진단뿐이라 그렇다.
/// 같은 파일의 [`PtyRegistry::wait_for_exit`] 와 같은 정책이다: 프로덕션 소비자가 생기면
/// 그때 게이트를 벗긴다. **찍는 쪽은 게이트하지 않는다** — 두 빌드가 다른 코드를 돌면
/// 재현이 갈린다. 원자적 store 한 번이라 비용도 그 자리에서 끝난다.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchPhase {
    /// watcher 를 못 걸었다 — cell 은 영영 안 찬다. 자식의 생사와 무관하게 우리 쪽 사건이다.
    NotAttached,
    /// 스레드는 떴는데 아직 `wait_fn` 에 들어가지 않았다(스케줄 못 받음).
    Spawned,
    /// `wait_fn` 에 들어가 아직 안 돌아왔다 — **자식이 안 죽었다**는 뜻이다.
    Waiting,
    /// `wait_fn` 이 돌아와 결과를 채웠다.
    Reaped,
}

#[cfg(test)]
impl WatchPhase {
    fn from_raw(v: u8) -> Self {
        match v {
            1 => Self::Spawned,
            2 => Self::Waiting,
            3 => Self::Reaped,
            _ => Self::NotAttached,
        }
    }
}

/// [`WatchPhase`] 의 원시 표현 — watcher 스레드와 공유하는 칸에 담기는 값.
const PHASE_NOT_ATTACHED: u8 = 0;
const PHASE_SPAWNED: u8 = 1;
const PHASE_WAITING: u8 = 2;
const PHASE_REAPED: u8 = 3;

/// headless PTY 하나의 메타데이터 + exit-code 캡처 cell. `Terminal` 인스턴스 자체는
/// 담지 않는다 — 18-b 에서 동일 id 로 `engine.terminals`(`TerminalStore`)가 보관한다.
pub struct PtyEntry {
    pub id: u32,
    /// AgentId — cap/telemetry 귀속용(위조 가능한 잠정 모델, docs/features/telemetry/index.md 의 "AgentId — agent 식별").
    pub owner_agent_id: String,
    pub cwd: Option<String>,
    pub command: Vec<String>,
    /// 생성 시각(monotonic). age 계산·정렬용.
    created_at: Instant,
    /// 마지막 IO 활동 시각(monotonic). idle TTL 판정 기준 — read/write 시 `touch`.
    last_activity: Instant,
    /// watcher-thread 가 `child.wait()` 완료 시 채우는 cell(`runner_host.rs` 패턴 이식).
    /// `Condvar` 를 짝지어, cell 을 채운 watcher 가 대기자를 깨운다 — 대기자는 고정 간격
    /// 폴링(부하에 비례해 깨지는 형태 C) 대신 종료 즉시 반환한다([`wait_for_exit`]).
    exit_result: Arc<(Mutex<Option<PtyExit>>, Condvar)>,
    /// exit watcher 스레드 핸들. 살려두기만 하면 되므로 join 하지 않는다(detached).
    _watcher: Option<JoinHandle<()>>,
    /// watcher 가 남기는 마지막 관측([`WatchPhase`]). 스레드와 공유한다.
    watch_phase: Arc<AtomicU8>,
}

/// exit cell 의 poison 을 보고했는가(첫 1 회만).
///
/// cell 은 PTY 엔트리마다 하나씩이지만 보고는 클래스 단위로 한 번이면 된다 — poison 은
/// sticky 라 한 번 걸린 프로세스에서는 이후 모든 엔트리가 같은 경로를 탄다.
static EXIT_CELL_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// exit cell 락 이름(로그에 나가는 값).
const EXIT_CELL_WHAT: &str = "pty exit cell";

impl PtyEntry {
    /// 캡처된 종료 결과(있으면). watcher 가 아직 안 채웠으면 `None`(=실행 중).
    ///
    /// poison 은 복구한다. 임계구역은 `Option<PtyExit>` 한 칸을 읽고 쓰는 것뿐이라
    /// 패닉이 나도 불변식이 성립하고, 여기서 `None` 으로 조용히 빠지면 **이미 죽은 자식이
    /// 영원히 실행 중으로 보인다**(`has_exited` 가 계속 false) — 관측 지점 없이 상태가
    /// 굳는다. 근거 `docs/dev-guide/error-handling.md` "락 poison".
    pub fn exit(&self) -> Option<PtyExit> {
        crate::poison::recover_mutex(
            self.exit_result.0.lock(),
            EXIT_CELL_WHAT,
            &EXIT_CELL_POISON_REPORTED,
        )
        .clone()
    }

    /// 자식이 종료돼 exit-code 가 잡혔는가.
    pub fn has_exited(&self) -> bool {
        self.exit().is_some()
    }

    /// exit-watcher 의 마지막 관측. 실패 갈래 진단에서 `exit() == None` 의 이유를 가른다.
    #[cfg(test)]
    pub fn watch_phase(&self) -> WatchPhase {
        WatchPhase::from_raw(self.watch_phase.load(Ordering::Acquire))
    }

    // 이유: 상태바/진단 노출용 introspection getter — 18-b/18-c 소비 시점까지 production
    // 호출자가 없다(현재는 단위 테스트에서만 사용).
    #[allow(dead_code)]
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    #[allow(dead_code)]
    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }
}

/// headless PTY spawn 시 registry 에 넘기는 메타데이터.
#[derive(Debug, Clone)]
pub struct PtySpawnSpec {
    pub owner_agent_id: String,
    pub cwd: Option<String>,
    pub command: Vec<String>,
}

/// headless PTY 메타데이터 registry. 동시 개수 상한 + idle TTL 로 좀비 누적을 막는다.
pub struct PtyRegistry {
    entries: HashMap<u32, PtyEntry>,
    /// Surface id 와 disjoint 한 별도 카운터([`PTY_ID_BASE`] 부터).
    ///
    /// **engine 들이 이 Arc 를 공유한다.** registry 마다 따로 세면 두 창이 같은 pty id 를
    /// 발급하고, 라우팅은 그 id 를 가진 engine 을 **먼저 찾히는 순서로** 고르므로 나중
    /// 것은 어떤 요청으로도 못 닿는다(`IdGenerator` doc 의 "글로벌 유니크").
    next_id: std::sync::Arc<AtomicU32>,
    max_concurrent: usize,
    idle_ttl: Duration,
}

impl Default for PtyRegistry {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            next_id: std::sync::Arc::new(AtomicU32::new(PTY_ID_BASE)),
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            idle_ttl: DEFAULT_IDLE_TTL,
        }
    }
}

impl PtyRegistry {
    /// 카운터를 공유하지 않는 독립 registry — **단위 테스트 전용**이다.
    /// production 은 항상 [`PtyRegistry::with_counter`] 로 engine 간 공유 카운터를 든다.
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// engine 들이 공유하는 카운터로 만든다 — production 경로는 이쪽이다.
    pub fn with_counter(next_id: std::sync::Arc<AtomicU32>) -> Self {
        Self {
            next_id,
            ..Self::default()
        }
    }

    /// 상한/TTL override 생성자 — `rate_limit.rs` 철학(기본값은 박되 호출자 지정 가능).
    // 이유: production 은 항상 `new()`(=`default()`)만 쓴다(`state.rs`) — override 는
    // 현재 단위 테스트 전용(limit/TTL 경계 시나리오 재현). 설정 노출(18-b/18-c) 전까지
    // dead_code.
    #[allow(dead_code)]
    pub fn with_limits(max_concurrent: usize, idle_ttl: Duration) -> Self {
        Self {
            max_concurrent,
            idle_ttl,
            ..Self::default()
        }
    }

    // 이유: 상태바/진단 노출용 introspection getter — 18-b/18-c 소비 시점까지 production
    // 호출자가 없다(현재는 단위 테스트에서만 사용).
    #[allow(dead_code)]
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    #[allow(dead_code)]
    pub fn idle_ttl(&self) -> Duration {
        self.idle_ttl
    }

    /// 새 headless PTY 를 등록하고 발급된 id 를 반환한다. 동시 개수가 상한에 도달했으면
    /// [`PtySpawnError::LimitReached`] 로 **실패**시킨다(panic 하지 않는다).
    pub fn register(&mut self, spec: PtySpawnSpec, now: Instant) -> Result<u32, PtySpawnError> {
        if self.entries.len() >= self.max_concurrent {
            return Err(PtySpawnError::LimitReached {
                current: self.entries.len(),
                max: self.max_concurrent,
            });
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(
            id,
            PtyEntry {
                id,
                owner_agent_id: spec.owner_agent_id,
                cwd: spec.cwd,
                command: spec.command,
                created_at: now,
                last_activity: now,
                exit_result: Arc::new((Mutex::new(None), Condvar::new())),
                _watcher: None,
                watch_phase: Arc::new(AtomicU8::new(PHASE_NOT_ATTACHED)),
            },
        );
        Ok(id)
    }

    /// `id` 의 PTY 자식이 종료될 때까지 기다리는 watcher-thread 를 건다. `wait_fn` 은
    /// 소유한 `portable_pty::Child`(또는 임의의 waitable)를 close-over 해 `child.wait()`
    /// 로 실제 종료코드를 뽑아 [`PtyExit`] 로 돌려주면 된다 — registry 는 `portable_pty`
    /// 타입에 직접 의존하지 않는다(closure 로 decouple). 완료 시 결과를 entry 의 cell 에
    /// 채운다(`runner_host.rs:429-451` 와 동형). 미존재 id 면 `false`.
    pub fn attach_exit_watcher<F>(&mut self, id: u32, wait_fn: F) -> bool
    where
        F: FnOnce() -> PtyExit + Send + 'static,
    {
        let Some(entry) = self.entries.get_mut(&id) else {
            return false;
        };
        let cell = entry.exit_result.clone();
        let phase = entry.watch_phase.clone();
        // 스레드가 뜨기 **전에** 찍는다 — spawn 이 실패하면 아래에서 되돌린다. 반대로 두면
        // 스레드가 먼저 달려 위상을 올린 뒤 이 줄이 덮어써 관측이 뒤로 간다.
        phase.store(PHASE_SPAWNED, Ordering::Release);
        let thread_phase = phase.clone();
        let handle = thread::Builder::new()
            .name(format!("pty-exit-watcher-{id}"))
            .spawn(move || {
                // wait 에 들어가기 직전에 찍는다 — 이 위상에서 cell 이 비어 있으면
                // `wait_fn` 이 아직 안 돌아왔다는 뜻이고, 그것이 곧 자식이 안 죽었다는 관측이다.
                thread_phase.store(PHASE_WAITING, Ordering::Release);
                let outcome = wait_fn();
                // 여기서 조용히 버리면 종료 결과가 영영 안 채워져 자식이 계속 실행 중으로
                // 보인다 — 위 `exit` 와 같은 이유로 복구한다.
                *crate::poison::recover_mutex(
                    cell.0.lock(),
                    EXIT_CELL_WHAT,
                    &EXIT_CELL_POISON_REPORTED,
                ) = Some(outcome);
                // cell 을 채운 직후 대기자(`wait_for_exit`)를 깨운다. 깨우는 이 쪽은
                // 프로덕션 경로(exit-code 캡처 watcher 라 항상 돈다)인데 기다리는 쪽은
                // `#[cfg(test)]` 뿐이라 비대칭이다 — 의도한 것이다: notify 는 받을 대기자가
                // 없으면 아무 일도 안 하는 무비용 신호라, non-test 빌드에서 죽은 코드가 아니라
                // 그냥 아무도 받지 않는 신호일 뿐이다. 대기자가 아직 wait 에 들어가지 않았어도
                // 신호가 유실되지 않는다 — 대기자는 wait 전에 cell 을 먼저 검사하고, wait 는
                // 반드시 그 락을 쥔 채 시작하기 때문이다.
                cell.1.notify_all();
                thread_phase.store(PHASE_REAPED, Ordering::Release);
            });
        match handle {
            Ok(h) => {
                entry._watcher = Some(h);
                true
            }
            Err(e) => {
                // 위상을 되돌린다 — 스레드가 없으므로 cell 은 영영 안 찬다. 그 사실이
                // 진단에 남아야 한다(로그만으로는 실패문이 못 본다).
                phase.store(PHASE_NOT_ATTACHED, Ordering::Release);
                tracing::warn!("pty exit watcher spawn failed for {id}: {e}");
                false
            }
        }
    }

    /// `id` 의 자식이 종료될 때까지 블록 대기하고 종료 결과를 반환한다(상한 `timeout`).
    ///
    /// exit-watcher 가 cell 을 채우며 보내는 `Condvar` 신호로 깨어나므로, 고정 간격 폴링과
    /// 달리 러너 부하와 무관하게 **종료 즉시** 반환한다. 이 테스트류가 보증하려는 계약은
    /// "종료가 온다(그리고 코드가 정확하다)" 이지 "종료가 N 초 안에 온다" 가 아니므로, 시간은
    /// 사고다 — 고정 마감시각 단정은 부하가 높은 회차에서 확률적으로 깨진다(ADR-0129 형태 C).
    /// 상한은 그래서 신호가 영영 오지 않을 때만 걸리는 안전망이고, 넉넉히 준다. 상한 안에
    /// 종료가 안 오면 `None`, 미존재 id 도 `None`.
    ///
    /// **테스트 전용**(`#[cfg(test)]`). `pty.wait` IPC 핸들러는 이걸 쓰지 않는다 — 그쪽은
    /// 즉시 반환해야 IPC 스레드가 막히지 않으므로 [`PtyEntry::exit`] 스냅샷을 읽는다. 즉
    /// 프로덕션에는 블로킹 대기 소비자가 없다. 이 메서드는 e2e 테스트가 폴링+마감시각 단정
    /// 대신 종료 이벤트를 기다리게 하려고만 존재하므로 non-test 빌드에서 노출하지 않는다
    /// (노출하면 dead code 라 이 레포는 error 로 잡는다). 프로덕션 소비자가 생기면 그때
    /// 게이트를 벗긴다 — "나중에 쓸 것"은 미리 노출할 근거가 아니다.
    #[cfg(test)]
    pub fn wait_for_exit(&self, id: u32, timeout: std::time::Duration) -> Option<PtyExit> {
        let pair = self.entries.get(&id)?.exit_result.clone();
        let (lock, cvar) = &*pair;
        let deadline = Instant::now() + timeout;
        let mut guard =
            crate::poison::recover_mutex(lock.lock(), EXIT_CELL_WHAT, &EXIT_CELL_POISON_REPORTED);
        while guard.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            // 락을 쥔 채 wait 에 들어간다 — attach 스레드의 notify 가 이 사이에 끼어들어도
            // 유실되지 않는다. poison 은 값을 잃지 않고 그대로 복구한다(exit cell 은 한 칸뿐).
            guard = match cvar.wait_timeout(guard, remaining) {
                Ok((g, _)) => g,
                // 이 락은 헬퍼 밖(`Condvar::wait_timeout` 재획득)에서 poison 을 만난다 —
                // `recover_poisoned` 로 같은 exit cell 좌표에 첫-1 회 보고를 모은다.
                Err(p) => {
                    crate::poison::recover_poisoned(p, EXIT_CELL_WHAT, &EXIT_CELL_POISON_REPORTED).0
                }
            };
        }
        guard.clone()
    }

    /// IO 활동(read/write) 발생 시 idle 타이머를 리셋한다. 미존재 id 면 `false`.
    pub fn touch(&mut self, id: u32, now: Instant) -> bool {
        match self.entries.get_mut(&id) {
            Some(e) => {
                e.last_activity = now;
                true
            }
            None => false,
        }
    }

    pub fn get(&self, id: u32) -> Option<&PtyEntry> {
        self.entries.get(&id)
    }

    pub fn contains(&self, id: u32) -> bool {
        self.entries.contains_key(&id)
    }

    // 이유: 단위 테스트 전용 편의 getter — `pty.list`(handle_list, 18-b)는 실제로는
    // `iter()`를 쓰고 있어 production 호출자가 없다.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 살아있는 headless PTY id 목록. 순서 미보장.
    // 이유: 단위 테스트 전용(kill 대상 id 순회) — production 은 `iter()`로 entry 전체를
    // 순회한다(`handle_list`).
    #[allow(dead_code)]
    pub fn ids(&self) -> Vec<u32> {
        self.entries.keys().copied().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PtyEntry> {
        self.entries.values()
    }

    /// 명시적 제거(`pty.kill`/`attach_surface` 승격, 18-b·18-c). 반환된 entry 의 watcher
    /// 핸들은 drop 되지만 스레드는 detached 라 자식 wait 를 계속 수행한다.
    pub fn remove(&mut self, id: u32) -> Option<PtyEntry> {
        self.entries.remove(&id)
    }

    /// idle 이 TTL 을 초과한 항목을 제거하고 그 id 들을 반환한다(정리된 순서 미보장).
    /// GUI 안전망이 없는 headless PTY 의 좀비 누적 방지 — 에이전트가 `pty.kill`/`pty.wait`
    /// 를 잊어도 호스트가 스스로 회수한다. 반환 id 로 호출자가 `TerminalStore` 제거 등
    /// 나머지 회수를 이어서 처리한다(`CoreState::sweep_idle_ptys`).
    ///
    /// **접근 시점 lazy sweep + 주기 타이머 양쪽에서 호출된다.** lazy 만으로는 에이전트가
    /// 조용해진 순간 — 즉 좀비가 가장 오래 남는 순간 — 에 회수도 함께 멈춘다. 주기
    /// 경로가 그 사각을 메우고, lazy 는 spawn 상한 판정을 정확히 유지하려고 남는다
    /// (`docs/adr/0050-headless-pty-primitive.md` "좀비 회수 시점"). 시각을 주입받고
    /// idempotent 하므로 두 경로가 겹쳐 돌아도 안전하다.
    pub fn sweep_idle(&mut self, now: Instant) -> Vec<u32> {
        let ttl = self.idle_ttl;
        let expired: Vec<u32> = self
            .entries
            .iter()
            .filter(|(_, e)| now.duration_since(e.last_activity) >= ttl)
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            self.entries.remove(id);
        }
        expired
    }
}

#[cfg(test)]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다(전수 가드가 제외한다) —
// 여기 경고는 조치 대상이 될 수 없어 프로덕션 신호만 가린다. error-handling.md.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    fn spec(cmd: &[&str]) -> PtySpawnSpec {
        PtySpawnSpec {
            owner_agent_id: "agent-1".into(),
            cwd: None,
            command: cmd.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = PtyRegistry::new();
        let now = Instant::now();
        let id = reg.register(spec(&["echo", "hi"]), now).unwrap();
        assert!(id >= PTY_ID_BASE, "pty id must be in disjoint high range");
        assert_eq!(reg.len(), 1);
        assert!(reg.contains(id));
        let e = reg.get(id).unwrap();
        assert_eq!(e.command, vec!["echo".to_string(), "hi".to_string()]);
        assert_eq!(e.owner_agent_id, "agent-1");
        assert!(!e.has_exited());
    }

    /// 카운터를 공유한 두 registry 는 **같은 id 를 두 번 발급하지 않는다.**
    ///
    /// 창마다 registry 가 따로이므로 카운터가 registry 소유였을 때는 둘 다
    /// `PTY_ID_BASE` 부터 셌다. 그러면 두 pty 가 같은 id 를 갖고, 라우팅은 그 id 를 가진
    /// engine 을 먼저 찾히는 순서로 고르므로 **나중 것은 어떤 요청으로도 못 닿는다** —
    /// 실측(2026-09-05, 창 둘): 두 창의 pty 가 둘 다 `0x8000_0000` 이었고,
    /// `output.observe_info {observer_id:1}` 은 포커스와 무관하게 **같은 하나**만 돌려줬다.
    #[test]
    fn registries_sharing_a_counter_never_issue_the_same_id() {
        let shared = std::sync::Arc::new(AtomicU32::new(PTY_ID_BASE));
        let mut a = PtyRegistry::with_counter(std::sync::Arc::clone(&shared));
        let mut b = PtyRegistry::with_counter(std::sync::Arc::clone(&shared));
        let now = Instant::now();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..3 {
            assert!(seen.insert(a.register(spec(&["a"]), now).unwrap()));
            assert!(seen.insert(b.register(spec(&["b"]), now).unwrap()));
        }
        assert_eq!(seen.len(), 6, "공유 카운터인데 id 가 겹쳤다: {seen:?}");
        assert!(seen.iter().all(|id| *id >= PTY_ID_BASE));

        // 대조군 — 카운터를 안 나누면 겹친다. 이 축이 실제로 무엇을 막는지 고정한다.
        let mut c = PtyRegistry::new();
        let mut d = PtyRegistry::new();
        assert_eq!(
            c.register(spec(&["c"]), now).unwrap(),
            d.register(spec(&["d"]), now).unwrap(),
            "독립 카운터는 같은 값에서 시작한다 — 공유가 필요한 이유가 이것이다"
        );
    }

    #[test]
    fn ids_are_disjoint_from_surface_space() {
        // 발급된 pty id 는 전부 PTY 공간에 있어야 한다. 카운터의 *시작값* 만 보면
        // "갓 만든 registry" 가정에 기대게 되므로, 연속 발급분 전체를 경계 술어
        // (`is_surface_id_space`)로 판정한다 — surface 쪽 경계 방어와 같은 술어다.
        let mut reg = PtyRegistry::new();
        let now = Instant::now();
        let a = reg.register(spec(&["a"]), now).unwrap();
        let b = reg.register(spec(&["b"]), now).unwrap();
        assert_eq!(a, PTY_ID_BASE);
        assert_eq!(b, PTY_ID_BASE + 1);
        for id in [a, b] {
            assert!(
                !is_surface_id_space(id),
                "pty id {id} 가 surface id 공간을 침범했다"
            );
        }
        // 경계값 자체의 소속: PTY_ID_BASE 는 PTY 공간, 그 직전은 surface 공간.
        assert!(!is_surface_id_space(PTY_ID_BASE));
        assert!(is_surface_id_space(PTY_ID_BASE - 1));
    }

    /// surface 카운터 쪽 반대 방향 보장(오염된 memory.db 로도 PTY 공간에 진입하지
    /// 않는다)은 `impl_workspace.rs` 의 `surface_id_floor_tests` 가 담당한다 —
    /// 이 disjoint 는 두 테스트가 함께 지킨다.
    #[test]
    fn surface_counter_starts_inside_surface_space() {
        let ids = crate::core::state::IdGenerator::new();
        assert!(is_surface_id_space(ids.next_surface()));
    }

    #[test]
    fn register_fails_when_limit_reached() {
        let mut reg = PtyRegistry::with_limits(2, DEFAULT_IDLE_TTL);
        let now = Instant::now();
        assert!(reg.register(spec(&["a"]), now).is_ok());
        assert!(reg.register(spec(&["b"]), now).is_ok());
        let err = reg.register(spec(&["c"]), now).unwrap_err();
        assert_eq!(err, PtySpawnError::LimitReached { current: 2, max: 2 });
        assert_eq!(reg.len(), 2, "실패한 spawn 은 등록되지 않아야 한다");
    }

    #[test]
    fn removing_frees_a_concurrency_slot() {
        let mut reg = PtyRegistry::with_limits(1, DEFAULT_IDLE_TTL);
        let now = Instant::now();
        let id = reg.register(spec(&["a"]), now).unwrap();
        assert!(reg.register(spec(&["b"]), now).is_err());
        assert!(reg.remove(id).is_some());
        assert!(
            reg.register(spec(&["b"]), now).is_ok(),
            "제거 후 슬롯이 비어 재등록 가능해야 한다"
        );
    }

    #[test]
    fn sweep_removes_idle_beyond_ttl() {
        let ttl = Duration::from_secs(300);
        let mut reg = PtyRegistry::with_limits(8, ttl);
        let base = Instant::now();
        let id = reg.register(spec(&["sleep"]), base).unwrap();

        // TTL 이내: 정리 안 됨.
        let within = base + Duration::from_secs(299);
        assert!(reg.sweep_idle(within).is_empty());
        assert!(reg.contains(id));

        // TTL 초과: 정리됨.
        let beyond = base + Duration::from_secs(301);
        let removed = reg.sweep_idle(beyond);
        assert_eq!(removed, vec![id]);
        assert!(!reg.contains(id));
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn touch_resets_idle_timer() {
        let ttl = Duration::from_secs(300);
        let mut reg = PtyRegistry::with_limits(8, ttl);
        let base = Instant::now();
        let id = reg.register(spec(&["a"]), base).unwrap();

        // 활동 발생: idle 타이머 리셋.
        let activity = base + Duration::from_secs(250);
        assert!(reg.touch(id, activity));

        // 최초 등록으로부터 301s 지났지만 활동으로부터는 51s 뿐 — 정리 안 됨.
        let now = base + Duration::from_secs(301);
        assert!(reg.sweep_idle(now).is_empty());
        assert!(reg.contains(id));

        // 활동으로부터 TTL 초과 — 정리됨.
        let later = activity + Duration::from_secs(301);
        assert_eq!(reg.sweep_idle(later), vec![id]);
    }

    #[test]
    fn exit_watcher_captures_real_exit_code() {
        let mut reg = PtyRegistry::new();
        let now = Instant::now();
        let id = reg.register(spec(&["exit-3"]), now).unwrap();
        assert!(!reg.get(id).unwrap().has_exited());

        // 실 프로세스를 spawn 해 non-zero 종료코드를 watcher 스레드가 잡는지 검증한다.
        // (portable_pty::Child 대신 std::process::Child 로 동일 wait() 계약을 확인 —
        // registry 는 waitable 종류에 무관하다.)
        let ok = reg.attach_exit_watcher(id, || {
            #[cfg(windows)]
            let mut child = std::process::Command::new("cmd")
                .args(["/C", "exit 3"])
                .spawn()
                .expect("spawn");
            #[cfg(not(windows))]
            let mut child = std::process::Command::new("sh")
                .args(["-c", "exit 3"])
                .spawn()
                .expect("spawn");
            let status = child.wait().expect("wait");
            PtyExit::from_status(status.code(), status.success())
        });
        assert!(ok);

        // watcher 스레드가 cell 을 채울 때까지 bounded poll(최대 ~3s).
        let mut captured = None;
        for _ in 0..600 {
            if let Some(e) = reg.get(id).unwrap().exit() {
                captured = Some(e);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let exit = captured.expect("exit code should be captured within timeout");
        assert_eq!(exit.code, Some(3));
        assert!(!exit.success);
        assert!(reg.get(id).unwrap().has_exited());
    }

    #[test]
    fn the_watcher_phase_says_why_the_cell_is_still_empty() {
        // `exit() == None` 은 두 사건을 같은 얼굴로 낸다 — 자식이 안 죽어서 `wait` 가 아직
        // 안 돌아온 것과, 우리가 잡을 자리에 애초에 못 간 것. 위상이 그 둘을 가르는지 잰다.
        let mut reg = PtyRegistry::new();
        let id = reg.register(spec(&["blocks"]), Instant::now()).unwrap();

        // ① watcher 를 걸기 전 — 안 걸었다.
        assert_eq!(reg.get(id).unwrap().watch_phase(), WatchPhase::NotAttached);

        // ② wait 안에서 신호를 기다리는 watcher. 종료를 흉내내는 것이 아니라 **안 돌아오는
        //    wait** 를 만든다 — 그것이 macOS 에서 관측된 모양이다.
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        assert!(reg.attach_exit_watcher(id, move || {
            // 이유: 이 recv 는 신호를 받는 것 자체가 목적이라 결과가 필요 없다. 송신 측이
            // 먼저 drop 돼 Err 가 와도 wait 를 끝내는 동작은 같다(위상만 재는 자리다).
            let _ = release_rx.recv();
            PtyExit::from_status(Some(0), true)
        }));

        // 스레드가 wait 에 들어갈 때까지 bounded poll(최대 ~3s).
        let mut reached = false;
        for _ in 0..600 {
            if reg.get(id).unwrap().watch_phase() == WatchPhase::Waiting {
                reached = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(reached, "watcher 가 wait 에 들어간 것이 위상에 안 남았다");
        // 그리고 이 위상에서 cell 은 아직 비어 있다 — 이 조합이 "자식이 안 죽었다" 다.
        assert!(reg.get(id).unwrap().exit().is_none());

        // ③ 풀어 주면 결과가 채워지고 위상이 마지막까지 간다.
        release_tx.send(()).expect("release");
        let mut done = false;
        for _ in 0..600 {
            if reg.get(id).unwrap().watch_phase() == WatchPhase::Reaped {
                done = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(done, "결과를 채운 뒤에도 위상이 안 올라갔다");
        assert!(reg.get(id).unwrap().has_exited());
    }

    /// `Reaped` 인데 대기가 `None` 을 낸 조합이 **실제로 날 수 있는가**, 그리고 그때
    /// 무엇을 봐야 하는가.
    ///
    /// 이 갈래는 오래 문장만 있고 표본이 0 이었다. 재 보면 도달 가능하다 — 그런데 그
    /// 도달 경로가 하나뿐이다: 대기가 상한을 다 써서 마지막 검사를 마친 **뒤에** 자식이
    /// 끝나는 것. 대기 쪽 결함이 아니다. 아래 [`a_fill_is_never_lost_even_if_the_wakeup_never_comes`]
    /// 가 그 반대편(대기가 놓치는 경로)이 없다는 것을 같은 크레이트에서 잰다.
    #[test]
    fn the_reaped_but_unseen_branch_is_a_late_child_not_a_missed_wakeup() {
        let mut reg = PtyRegistry::new();
        let id = reg.register(spec(&["late"]), Instant::now()).unwrap();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        assert!(reg.attach_exit_watcher(id, move || {
            // 이유: 신호를 받는 것 자체가 목적이라 결과가 필요 없다(위 위상 시험과 같다).
            let _ = release_rx.recv();
            PtyExit::from_status(Some(0), true)
        }));

        // ① 자식이 아직 안 끝난 채로 예산을 다 쓴다 — 대기는 None 을 낸다.
        assert!(reg.wait_for_exit(id, Duration::from_millis(50)).is_none());

        // ② 예산이 끝난 **뒤에** 자식이 끝난다.
        release_tx.send(()).expect("release");
        let mut done = false;
        for _ in 0..600 {
            if reg.get(id).unwrap().watch_phase() == WatchPhase::Reaped {
                done = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(done, "자식이 끝났는데 위상이 안 올라갔다");

        // ③ 그래서 진단이 보는 순간의 상태가 바로 그 조합이다 — 위상 Reaped + cell 참 +
        //    그런데 대기는 이미 None 을 냈다. 이 조합이 가리키는 것은 늦은 자식이지
        //    고장난 대기가 아니다.
        assert_eq!(reg.get(id).unwrap().watch_phase(), WatchPhase::Reaped);
        assert!(reg.get(id).unwrap().exit().is_some());
    }

    /// 대기가 park 에 든 사이 cell 이 차면 그것을 놓치는가 — 놓치면 위 갈래의 옛 처방
    /// ("대기 쪽을 봐라")이 맞는 말이 된다. 안 놓치면 그 처방은 틀린 것이다.
    ///
    /// **깨우기가 아예 없어도 안 놓친다** — 이 시험의 이름이 그렇게 적혀 있는 이유다.
    /// 변이로 쟀다(2026-09-09): watcher 의 `notify_all()` 을 지우고 표본을 3 으로 줄여
    /// 돌리면 **여전히 초록**이고 벽시계만 15.01 s 로 는다(정상은 0.13 s). 대기가 상한
    /// 만료로 깬 뒤 cell 을 다시 검사하기 때문이다. 즉 `notify_all` 이 사는 것은 정확성이
    /// 아니라 **지연**이고, 이 시험이 지키는 것은 그 재검사다.
    ///
    /// 그 지연에는 시험을 안 붙였다. ADR-0181 의 순서대로 물으면 1(도장)·2(시계 지우기)가
    /// 안 되는 자리다 — 깨워서 왔든 만료로 왔든 대기가 내는 **값이 같아서** 두 사건을
    /// 관측값으로 가를 수 없다. 남는 것은 3(대조군 얹은 지연 단정)뿐인데, 그 지연을
    /// 소비하는 프로덕션 대기자가 없다(`wait_for_exit` 자체가 `#[cfg(test)]`). 아무도 안
    /// 쓰는 시간 값에 벽시계 단정을 붙이면 부하가 만드는 빨강만 생긴다.
    #[test]
    fn a_fill_is_never_lost_even_if_the_wakeup_never_comes() {
        const TRIALS: usize = 100;
        let mut missed = 0usize;
        for _ in 0..TRIALS {
            let mut reg = PtyRegistry::new();
            let id = reg.register(spec(&["parked"]), Instant::now()).unwrap();
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            assert!(reg.attach_exit_watcher(id, move || {
                // 이유: 위와 같다 — 신호 수신 자체가 목적이다.
                let _ = release_rx.recv();
                PtyExit::from_status(Some(0), true)
            }));
            // 대기가 park 에 들어갈 즈음 채운다. 예산은 넉넉해, None 이 나오면 그것은
            // 상한이 아니라 유실이다.
            let releaser = thread::spawn(move || {
                thread::sleep(Duration::from_millis(1));
                // 이유: 대기가 이미 끝나 수신 측이 drop 됐으면 Err 가 정상이다 — 이 스레드가
                // 하려는 일(자식을 끝내는 신호)은 그 경우 이미 필요 없다.
                let _ = release_tx.send(());
            });
            if reg.wait_for_exit(id, Duration::from_secs(5)).is_none() {
                missed += 1;
            }
            releaser.join().expect("releaser");
        }
        assert_eq!(missed, 0, "{TRIALS} 회 중 {missed} 회를 놓쳤다");
    }

    #[test]
    fn attach_exit_watcher_on_missing_id_is_false() {
        let mut reg = PtyRegistry::new();
        assert!(!reg.attach_exit_watcher(12345, || PtyExit::from_status(Some(0), true)));
    }

    /// exit cell 이 poison 돼도 읽기·쓰기가 모두 살아남는가.
    ///
    /// 조용히 빠지는 구현(`lock().ok()` / `if let Ok`)이면 두 방향 중 하나가 깨진다 —
    /// 읽기를 버리면 여기서 패닉하고, 쓰기를 버리면 값이 영영 안 채워져 아래 poll 이
    /// 타임아웃한다. 즉 이 테스트 하나가 두 지점을 같이 고정한다.
    #[test]
    fn exit_cell_survives_a_poisoned_lock() {
        let mut reg = PtyRegistry::new();
        let id = reg
            .register(spec(&["echo", "hi"]), Instant::now())
            .expect("register");

        let cell = reg.get(id).expect("entry").exit_result.clone();
        let poisoner = cell.clone();
        // 이유: 이 스레드는 패닉하는 것이 목적이라 join 결과는 항상 Err 다 — 버린다.
        let _ = thread::spawn(move || {
            let _guard = poisoner.0.lock().expect("fresh lock");
            panic!("poison the exit cell on purpose");
        })
        .join();
        assert!(
            cell.0.is_poisoned(),
            "락이 실제로 poison 됐어야 전제가 성립한다"
        );

        // 읽기: 패닉 없이 "아직 안 끝났다" 를 그대로 답한다.
        assert!(reg.get(id).expect("entry").exit().is_none());
        assert!(!reg.get(id).expect("entry").has_exited());

        // 쓰기: watcher 스레드가 poison 된 cell 에도 결과를 채운다.
        assert!(reg.attach_exit_watcher(id, || PtyExit::from_status(Some(7), false)));
        let mut captured = None;
        for _ in 0..600 {
            if let Some(e) = reg.get(id).expect("entry").exit() {
                captured = Some(e);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let exit = captured.expect("poison 이후에도 종료 결과가 채워져야 한다");
        assert_eq!(exit.code, Some(7));
        assert!(reg.get(id).expect("entry").has_exited());
    }
}
