//! IPC dispatch 한 회차의 규칙 — gui `App::process_ipc` 와 headless `pump_ipc` 가 같은 것을 쓴다.
//!
//! 회차는 큐에서 명령을 **하나 꺼내 끝까지 처리하고, 다음 것을 꺼낸다.** 먼저 전부 모아 두고
//! 나중에 처리하지 않는다. 그래서 회차를 멈추면 남은 명령은 **큐에 그대로 있다** — 입장 장부의
//! 바이트·깊이에도 그대로 남고, 큐 대기 시간에도 계속 쌓인다. 모은 뒤에 멈췄다면 그 명령들은
//! 장부에서는 빠졌는데 실행은 안 된, 어디에도 안 보이는 상태가 됐을 것이다.
//!
//! 회차는 두 예산 중 먼저 닿는 것에서 멈춘다.
//!
//! - **명령 수** — [`DRAIN_BUDGET_PER_ROUND`]. 동시 연결 상한에서 파생된 값이다(ADR-0313).
//! - **경과 시간** — [`ROUND_TIME_BUDGET`]. 명령 하나를 끝낼 때마다 본다. **첫 명령은 시간과
//!   무관하게 늘 처리한다** — 그래야 회차마다 적어도 하나가 진척되고, 예산보다 비싼 명령
//!   하나가 영영 못 도는 일이 없다.
//!
//! 수 예산만으로는 회차의 **시간**이 안 잘린다. 수 예산은 연결 상한과 같은 256 이고, 명령
//! 하나의 비용은 수십 µs 에서 수십 ms 까지 벌어진다(실측과 판단의 근거는
//! [ADR-0410](../../docs/adr/0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)).
//!
//! **남은 것은 다음 회차가 집는다.** 두 경로 모두 명령마다의 wake 를 이월의 근거로 삼지 않는다.
//! headless 는 이벤트 채널에 `IpcReady` 를 하나만 두고(명령마다 두면 채널에 적체가 쌓여 같은
//! 채널의 plugin·PTY wake 가 굶는다), 회차가 끝났을 때 큐에 명령이 남았으면 부르는 쪽이 루프를
//! 한 번 더 깨운다(ADR-0465). gui 는 wake 를 회차 없이 건너뛸 수 있어서, 회차가 예산에서 멈추면
//! [`IpcRound::finish`] 가 돌려준 이유를 보고 루프를 스스로 한 번 더 깨운다(ADR-0413). 빈 큐를
//! 만난 회차는 곧바로 끝난다(busy-spin 없음).
//!
//! **이 예산은 이미 실행 중인 명령을 끊지 못한다.** 예산은 명령과 명령 **사이**에서만 본다.
//! 메인 스레드에서 도는 동기 handler 하나가 오래 걸리면 그 시간 동안 회차도, 타이머도, 화면도
//! 그 뒤에서 기다린다 — 선점할 수단이 없다(ADR-0313 의 "진행 보장의 한계").
//!
//! 종료 drain(`shutdown_machine`)은 이 규칙을 쓰지 않는다 — 남은 것을 전부 거절로 답해야 하는
//! 절차라 예산을 두면 답 없이 끝날 수 있다(ADR-0313).

use std::sync::Arc;
use std::time::{Duration, Instant};

use tasty_ipc::dispatch::{DispatchStats, RoundEnd};
use tasty_telemetry::PressureStats;

use crate::adapters::production::tcp_ipc_server::DRAIN_BUDGET_PER_ROUND;
use crate::ipc::server::{Claim, IpcCommand, expired_before_run_response};
use crate::ports::ipc_server::IpcServerPort;

/// 한 회차가 명령을 꺼내는 데 쓸 수 있는 시간. **파생이 아니다** — 근거는 ADR-0410.
///
/// 60 Hz 한 프레임이다. 회차가 끝나야 루프가 렌더·타이머로 넘어가므로, 이 값이 곧 "IPC 부하가
/// 화면 한 프레임보다 오래 루프를 쥐지 않는다" 는 약속이다. gui 에서는 이 값만으로 안 되고, 다음
/// 회차가 사용자 이벤트로 곧바로 이어지지 않게 하는 양보(`crate::app::ipc::IpcPacer`, ADR-0413)가
/// 같은 값을 간격으로 쓴다. 명령 하나가 이보다 비싸면 그 명령만큼은 넘친다(첫 명령은 늘 처리한다
/// — 모듈 doc).
pub(crate) const ROUND_TIME_BUDGET: Duration = Duration::from_millis(16);

/// 이 프로세스의 회차 시간 예산 — release 에서는 늘 [`ROUND_TIME_BUDGET`] 이다.
///
/// debug 빌드에서만 `TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS` 로 **줄일 수** 있다(늘리는 값은
/// 버린다). 같은 계열 손잡이(`tcp_ipc_server` 의 `debug_env_usize`)처럼 덮어쓰기가 먹으면
/// `warn!`, 숫자가 아니거나 제품값 이상이라 버리면 그 사유를 `warn!` 으로 남긴다 — 그 헬퍼를
/// 그대로 쓰지 않는 것은 파싱되는 순간 "덮어쓴다" 를 찍어, 제품값 이상을 버리는 이 자리에서는
/// 로그가 서로 어긋나기 때문이다. 기본 예산에서는 시험의 동시 요청이 회차를 안 자르므로, 잘린 회차가 루프를 다시
/// 깨우는 갈래(ADR-0465 · ADR-0413)를 실행 파일째로 지나게 할 다른 길이 없다. 그 갈래를 재는
/// 시험은 `tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered_when_every_round_is_cut`.
fn round_time_budget() -> Duration {
    #[cfg(debug_assertions)]
    {
        static SHRUNK: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
        if let Some(budget) = *SHRUNK.get_or_init(debug_shrunk_round_time_budget) {
            return budget;
        }
    }
    ROUND_TIME_BUDGET
}

/// debug 전용 덮어쓰기를 읽는다. 없으면 `None`, 쓸 수 없는 값이면 사유를 남기고 `None`.
#[cfg(debug_assertions)]
fn debug_shrunk_round_time_budget() -> Option<Duration> {
    const NAME: &str = "TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS";
    let millis = debug_env_millis(NAME)?;
    let budget = Duration::from_millis(millis);
    if budget >= ROUND_TIME_BUDGET {
        tracing::warn!(
            "{NAME}={millis} is not below the product round time budget of {} ms, ignored",
            ROUND_TIME_BUDGET.as_millis()
        );
        return None;
    }
    tracing::warn!("{NAME}={millis} overrides the IPC round time budget (debug build)");
    Some(budget)
}

/// debug 전용 환경변수를 밀리초로 읽는다. 없으면 `None`, 숫자가 아니면 경고를 남기고 `None`.
#[cfg(debug_assertions)]
fn debug_env_millis(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    raw.trim()
        .parse::<u64>()
        .inspect_err(|e| tracing::warn!("{name}={raw:?} is not a number, ignored: {e}"))
        .ok()
}

/// 진행 중인 회차 하나.
pub(crate) struct IpcRound {
    started: Instant,
    taken: usize,
    count_budget: usize,
    time_budget: Duration,
    end: RoundEnd,
}

impl IpcRound {
    /// 제품 예산으로 지금 회차를 시작한다.
    pub(crate) fn begin() -> Self {
        Self::with_budgets(DRAIN_BUDGET_PER_ROUND, round_time_budget())
    }

    /// 예산을 골라 시작한다 — 시험이 작은 값을 넣는 자리.
    pub(crate) fn with_budgets(count_budget: usize, time_budget: Duration) -> Self {
        Self {
            started: Instant::now(),
            taken: 0,
            count_budget,
            time_budget,
            end: RoundEnd::Drained,
        }
    }

    /// 다음 명령. 예산에 닿았거나 큐가 비었으면(또는 서버가 없으면) `None` 이고, 그 뒤로도
    /// 계속 `None` 이다.
    pub(crate) fn next(&mut self, server: Option<&dyn IpcServerPort>) -> Option<IpcCommand> {
        if self.end != RoundEnd::Drained {
            return None;
        }
        if self.taken >= self.count_budget {
            self.end = RoundEnd::CountBudget;
            return None;
        }
        if self.taken > 0 && self.started.elapsed() >= self.time_budget {
            self.end = RoundEnd::TimeBudget;
            return None;
        }
        let cmd = server?.try_recv().ok()?;
        self.taken += 1;
        Some(cmd)
    }

    /// 회차를 닫고 센다. 하나도 못 꺼낸 회차는 세지 않는다 — 비어 있던 회차를 세면 "명령이
    /// 있었던 회차" 의 깊이 분포가 0 으로 희석된다.
    ///
    /// 돌려주는 값은 회차가 왜 멈췄는가다. `Drained` 가 아니면 큐에 명령이 남아 있을 수 있다.
    pub(crate) fn finish(self, pressure: &PressureStats, dispatch: &DispatchStats) -> RoundEnd {
        if self.taken == 0 {
            return self.end;
        }
        // 이 회차가 처리한 명령 수. 예산에 붙은 값이 나오면 그 회차는 큐를 다 비우지 못했을
        // 수 있다.
        pressure.record_drain(self.taken);
        dispatch.record_round(self.end);
        self.end
    }
}

/// 꺼낸 명령 하나의 관측 — 큐 대기를 재고, 명령을 다 다룬 뒤 호스트 몫을 느린 요청 링에 넘긴다.
///
/// gui `process_ipc` 와 headless `pump_ipc` 가 **같은 자리**(명령을 꺼낸 직후 · 다 다룬 직후)에서
/// 이것을 부른다. 두 경로의 모수가 갈리면 한쪽에서만 보이는 느린 요청이 생긴다.
///
/// 명령을 빌리지 않고 필요한 값만 복사해 든다 — 명령은 dispatch 로 옮겨지고, dispatch 는
/// `Core` 를 가변으로 빌린다. 그래서 `begin` 과 `finish` 가 따로 있다.
pub(crate) struct CommandObservation {
    request_seq: u64,
    /// canonical 메서드(모르는 이름은 받은 그대로 — 링이 실을 때 길이를 자른다). `system.pressure` 자신이면 `None` — 진단 조회가 링을 밀어내면 조회할
    /// 때마다 원인 요청이 한 칸씩 사라진다.
    method: Option<String>,
    caller: tasty_telemetry::slow_requests::CallerKind,
    queue_wait: Duration,
    started: Instant,
    /// 그 명령의 답이 끝난 방식 — 기다리는 쪽이 답을 받을 때 채우므로 줄은 칸째 든다.
    outcome: tasty_telemetry::slow_requests::HostOutcomeCell,
}

impl CommandObservation {
    /// 명령을 막 꺼냈다 — 큐 대기를 접고 호스트 처리의 시작을 찍는다. 큐 대기 계측이 기한
    /// 판정(`claim_or_answer`)보다 먼저다: 만료된 요청도 큐에 앉아 있었다.
    pub(crate) fn begin(pressure: &PressureStats, cmd: &IpcCommand) -> Self {
        let queue_wait = cmd.queue_wait();
        pressure.record_queue_wait(queue_wait);
        let method = tasty_ipc::alias::canonicalize(&cmd.request.method);
        // 봉투가 말한 종류다. 토큰이 무효면 게이트가 곧바로 거절하므로 느린 줄이 될 일이 드물다.
        let caller = if cmd.request.session_token.is_some() {
            tasty_telemetry::slow_requests::CallerKind::Agent
        } else {
            tasty_telemetry::slow_requests::CallerKind::Local
        };
        Self {
            request_seq: cmd.request_seq().get(),
            method: (method != PRESSURE_METHOD).then(|| method.to_string()),
            caller,
            queue_wait,
            started: Instant::now(),
            outcome: cmd.outcome_cell(),
        }
    }

    /// 명령을 다 다뤘다(답을 냈거나 plugin 으로 넘겼다).
    pub(crate) fn finish(self, slow: &tasty_telemetry::SlowRequestLog) {
        let Some(method) = self.method else {
            return;
        };
        slow.finish_host(tasty_telemetry::slow_requests::HostLeg {
            request_seq: self.request_seq,
            method: &method,
            caller: self.caller,
            queue_wait: self.queue_wait,
            host: self.started.elapsed(),
            outcome: self.outcome,
        });
    }
}

/// 링에 넣지 않는 진단 조회.
const PRESSURE_METHOD: &str = "system.pressure";

/// 꺼낸 명령을 **실행하기 직전**에 부른다. 실행해도 되면 `true`.
///
/// 호출자가 실은 응답 대기 상한이 큐에서 기다리는 동안 지났으면 실행하지 않고 `-32067` 로
/// 답한다(ADR-0411). 기다리던 쪽이 먼저 물러났으면 그쪽이 이미 답했으므로 조용히 버린다. 두
/// 경우 모두 게이트(권한·audit·rate limit)에 닿기 **전**이라 실행되지 않은 요청이 토큰을 쓰거나
/// 감사 행을 남기지 않는다. 큐 대기 계측은 이보다 먼저다 — 만료된 요청도 큐에 앉아 있었다.
pub(crate) fn claim_or_answer(cmd: &IpcCommand, dispatch: &Arc<DispatchStats>) -> bool {
    match cmd.claim(dispatch) {
        Claim::Run => true,
        Claim::Expired { waited, bound } => {
            tracing::debug!(
                "IPC {} not run: waited {:?} in the queue past the caller's {:?} bound",
                cmd.request.method,
                waited,
                bound
            );
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            crate::ipc::server::send_response(
                &cmd.response_tx,
                expired_before_run_response(id, bound),
            );
            false
        }
        Claim::Withdrawn => false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::adapters::test::mock_ipc_server::MockIpcServer;
    use crate::ipc::protocol::JsonRpcRequest;

    fn push(server: &MockIpcServer, n: usize) {
        let tx = server.tx_clone();
        for i in 0..n {
            let (resp_tx, _resp_rx) = mpsc::sync_channel(1);
            let req = JsonRpcRequest {
                response_timeout_ms: None,
                idempotency_key: None,
                jsonrpc: "2.0".to_string(),
                method: format!("test.m{i}"),
                id: Some(serde_json::Value::from(i as u64)),
                params: serde_json::Value::Null,
                session_token: None,
            };
            tx.send(IpcCommand::new(req, resp_tx)).expect("mock queue");
        }
    }

    fn drain(round: &mut IpcRound, server: &MockIpcServer) -> Vec<String> {
        let mut seen = Vec::new();
        while let Some(cmd) = round.next(Some(server)) {
            seen.push(cmd.request.method.clone());
        }
        seen
    }

    /// 수 예산에서 멈추면 남은 것은 큐에 **그대로** 남고, 다음 회차가 이어서 같은 순서로 집는다.
    #[test]
    fn a_round_stopped_by_count_leaves_the_rest_queued_in_order() {
        let server = MockIpcServer::new();
        push(&server, 5);
        let mut first = IpcRound::with_budgets(2, Duration::MAX);
        assert_eq!(drain(&mut first, &server), ["test.m0", "test.m1"]);
        assert_eq!(first.end, RoundEnd::CountBudget);
        let mut second = IpcRound::with_budgets(2, Duration::MAX);
        assert_eq!(drain(&mut second, &server), ["test.m2", "test.m3"]);
        let mut third = IpcRound::with_budgets(2, Duration::MAX);
        assert_eq!(drain(&mut third, &server), ["test.m4"]);
        assert_eq!(third.end, RoundEnd::Drained, "the queue ran dry first");
    }

    /// 시간 예산이 0 이어도 첫 명령은 처리한다 — 회차마다 진척이 하나는 있다.
    #[test]
    fn a_spent_time_budget_still_lets_the_first_command_through() {
        let server = MockIpcServer::new();
        push(&server, 3);
        let mut round = IpcRound::with_budgets(256, Duration::ZERO);
        assert_eq!(drain(&mut round, &server), ["test.m0"]);
        assert_eq!(round.end, RoundEnd::TimeBudget);
        let mut left = 0;
        while server.try_recv().is_ok() {
            left += 1;
        }
        assert_eq!(left, 2, "the other two are still queued");
    }

    /// 한 번 멈춘 회차는 다시 꺼내지 않는다 — 큐에 새것이 들어와도 그 회차의 몫이 아니다.
    #[test]
    fn a_stopped_round_stays_stopped() {
        let server = MockIpcServer::new();
        push(&server, 1);
        let mut round = IpcRound::with_budgets(1, Duration::MAX);
        assert_eq!(drain(&mut round, &server).len(), 1);
        push(&server, 1);
        assert!(round.next(Some(&server)).is_none());
    }

    /// 빈 회차는 세지 않고, 명령이 있던 회차는 처리한 수와 멈춘 이유를 한 번씩 센다.
    #[test]
    fn only_rounds_that_took_something_are_counted() {
        let pressure = PressureStats::default();
        let dispatch = DispatchStats::default();
        let server = MockIpcServer::new();

        let mut empty = IpcRound::with_budgets(4, Duration::MAX);
        assert!(empty.next(Some(&server)).is_none());
        assert_eq!(empty.finish(&pressure, &dispatch), RoundEnd::Drained);
        assert_eq!(pressure.snapshot().queue_drains, 0);
        assert_eq!(dispatch.snapshot().rounds, 0);

        push(&server, 3);
        let mut round = IpcRound::with_budgets(2, Duration::MAX);
        drain(&mut round, &server);
        assert_eq!(round.taken, 2);
        assert_eq!(
            round.finish(&pressure, &dispatch),
            RoundEnd::CountBudget,
            "the caller learns that work may be left"
        );
        let p = pressure.snapshot();
        assert_eq!((p.queue_drains, p.queue_commands), (1, 2));
        let d = dispatch.snapshot();
        assert_eq!((d.rounds, d.rounds_stopped_by_count), (1, 1));
    }

    fn bounded(
        ms: u64,
    ) -> (
        IpcCommand,
        mpsc::Receiver<crate::ipc::protocol::JsonRpcResponse>,
    ) {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            response_timeout_ms: Some(ms),
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: "workspace.create".to_string(),
            id: Some(serde_json::Value::from(5u64)),
            params: serde_json::Value::Null,
            session_token: None,
        };
        (IpcCommand::new(req, resp_tx), resp_rx)
    }

    /// 기한이 큐에서 지난 명령은 실행하지 않고, 꺼낸 쪽이 `-32067` 로 답하고 센다.
    #[test]
    fn a_command_past_its_deadline_is_answered_not_run_and_counted() {
        let dispatch = Arc::new(DispatchStats::default());
        let (cmd, rx) = bounded(1);
        std::thread::sleep(Duration::from_millis(5));
        assert!(!claim_or_answer(&cmd, &dispatch), "it must not run");
        let resp = rx.try_recv().expect("the taker answers it");
        assert_eq!(
            resp.error.expect("error").code,
            crate::ipc::protocol::ERR_EXPIRED_BEFORE_RUN
        );
        assert_eq!(dispatch.snapshot().expired_before_run, 1);
    }

    /// 기다리던 쪽이 먼저 물러난 명령은 조용히 버린다 — 답은 그쪽이 이미 썼다.
    #[test]
    fn a_withdrawn_command_is_dropped_without_a_second_answer() {
        let dispatch = Arc::new(DispatchStats::default());
        let (cmd, rx) = bounded(60_000);
        cmd.lifecycle().withdraw();
        assert!(!claim_or_answer(&cmd, &dispatch));
        assert!(rx.try_recv().is_err(), "no second answer");
        assert_eq!(dispatch.snapshot().expired_before_run, 1);
    }

    /// 기한 안의 명령은 실행한다.
    #[test]
    fn a_command_within_its_deadline_runs_and_is_in_flight_until_its_waiter_lets_go() {
        let dispatch = Arc::new(DispatchStats::default());
        let (cmd, _rx) = bounded(60_000);
        let waiter = cmd.lifecycle();
        assert!(claim_or_answer(&cmd, &dispatch));
        assert_eq!(dispatch.snapshot().expired_before_run, 0);
        assert_eq!(dispatch.snapshot().in_flight, 1);
        // 메인 스레드가 명령을 놓아도(응답을 워커로 넘긴 경우) 기다리는 쪽이 있는 동안은 실행 중이다.
        drop(cmd);
        assert_eq!(dispatch.snapshot().in_flight, 1);
        drop(waiter);
        assert_eq!(dispatch.snapshot().in_flight, 0);
    }

    /// 서버가 없는 조립에서는 아무것도 꺼내지 않는다.
    #[test]
    fn no_server_means_an_empty_round() {
        let mut round = IpcRound::begin();
        assert!(round.next(None).is_none());
        assert_eq!(round.taken, 0);
    }

    fn named(method: &str) -> IpcCommand {
        let (resp_tx, _resp_rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            id: Some(serde_json::Value::from(1u64)),
            params: serde_json::Value::Null,
            session_token: Some("token-is-not-copied".to_string()),
        };
        IpcCommand::new(req, resp_tx)
    }

    /// 문턱을 넘긴 명령은 호스트 번호와 함께 링에 들고, `system.pressure` 자신은 같은 시간을
    /// 써도 안 든다 — 진단 조회가 링을 밀어내면 조회할 때마다 원인 요청이 한 칸씩 사라진다.
    /// 둘 다 큐 대기는 센다(링에서만 뺀다).
    #[test]
    fn a_slow_command_is_kept_and_the_pressure_query_itself_is_not() {
        let pressure = PressureStats::default();
        let slow = tasty_telemetry::SlowRequestLog::default();
        let over =
            tasty_telemetry::slow_requests::SLOW_REQUEST_THRESHOLD + Duration::from_millis(5);

        let query = named("system.pressure");
        let observed = CommandObservation::begin(&pressure, &query);
        std::thread::sleep(over);
        observed.finish(&slow);
        assert!(
            slow.snapshot().rows.is_empty(),
            "the query landed in its own ring"
        );

        let cmd = named("workspace.list");
        let observed = CommandObservation::begin(&pressure, &cmd);
        std::thread::sleep(over);
        observed.finish(&slow);
        let rows = slow.snapshot().rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].request_seq, cmd.request_seq().get());
        let host = rows[0].host.as_ref().expect("host part");
        assert_eq!(host.method, "workspace.list");
        assert_eq!(
            host.caller,
            tasty_telemetry::slow_requests::CallerKind::Agent
        );
        assert!(host.host_us >= 100_000, "{host:?}");
        assert_eq!(pressure.snapshot().queue_wait_hist.total(), 2);

        // 줄은 명령의 결과 칸을 든다 — 줄이 들어간 **뒤에** 기다리는 쪽이 답을 적어도 보인다.
        assert_eq!(host.outcome.get(), None, "nobody answered yet");
        cmd.lifecycle()
            .record_answer(&tasty_ipc::protocol::JsonRpcResponse::success(
                serde_json::json!(1),
                serde_json::Value::Null,
            ));
        let rows = slow.snapshot().rows;
        let host = rows[0].host.as_ref().expect("host part");
        assert_eq!(
            host.outcome.get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Ok)
        );
    }

    /// 두 dispatch 루프가 **같은 자리**에서 관측한다 — 꺼낸 직후 `begin`, 다 다룬 직후 `finish`,
    /// 그리고 큐 대기를 루프 본문이 따로 재지 않는다(재면 한 명령이 두 번 접힌다).
    #[test]
    fn both_dispatch_loops_observe_at_the_same_place_and_once() {
        for (name, src) in [
            ("gui", include_str!("ipc.rs")),
            ("headless", include_str!("../boot/headless_dispatch.rs")),
        ] {
            let body = src.split("\n#[cfg(test)]").next().unwrap_or(src);
            assert_eq!(
                body.matches("CommandObservation::begin(").count(),
                1,
                "{name}"
            );
            assert_eq!(body.matches("observed.finish(").count(), 1, "{name}");
            assert_eq!(body.matches("record_queue_wait(").count(), 0, "{name}");
        }
    }

    /// IPC 명령을 plugin namespace 로 넘기는 두 자리(gui 라우터 · headless)가 **그 명령의 번호**를
    /// 넘긴다 — `None` 을 넘기면 plugin hop 이 호스트 몫과 이어지지 않아 링에 두 줄로 갈리거나
    /// 아예 안 남는데, 매니저 쪽 시험은 번호를 직접 넣으므로 그것을 못 본다(ADR-0436).
    #[test]
    fn both_namespace_forwards_pass_the_commands_request_seq() {
        for (name, src) in [
            ("gui", include_str!("ipc/routing.rs")),
            ("headless", include_str!("../boot/headless_dispatch.rs")),
        ] {
            let body = src.split("\n#[cfg(test)]").next().unwrap_or(src);
            let calls: Vec<&str> = body
                .split("forward_namespace_call(")
                .skip(1)
                .map(|rest| rest.split(");").next().unwrap_or(rest))
                .collect();
            assert_eq!(calls.len(), 1, "{name}");
            // 보존소를 지나는 갈래(`forward_keeping_the_key`)에서는 키를 뗀 사본 `c` 가 원 명령의
            // 번호를 그대로 든다(`IpcCommand::continuing`).
            assert!(
                calls[0].contains("Some(c.request_seq())"),
                "{name}: {}",
                calls[0]
            );
        }
    }
}
