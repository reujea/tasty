//! GUI와 헤드리스가 공유하는 IPC 회차 예산. 명령을 하나씩 꺼내 처리한다.
//! 수·시간 예산에 닿으면 나머지는 큐와 대기 계측에 남기고 호출자가 다음 회차를 깨운다.
//! 시간은 명령 사이에서 확인하며 실행 중인 동기 핸들러를 중단하지 못한다.
//! 시간 예산은 첫 명령 뒤부터 확인한다. 종료 중 남은 요청을 거절하는 drain은 이 예산을 쓰지 않는다.
//! [IPC 실행 정책](../../docs/architecture/ipc-server.md)을 따른다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tasty_ipc::dispatch::{DispatchStats, RoundEnd};
use tasty_telemetry::PressureStats;

use crate::adapters::production::tcp_ipc_server::DRAIN_BUDGET_PER_ROUND;
use crate::ipc::server::{Claim, IpcCommand, expired_before_run_response};
use crate::ports::ipc_server::IpcServerPort;

/// 회차 시간 예산. 실행 중인 한 명령이 오래 걸리면 넘을 수 있다.
/// GUI의 IpcPacer도 회차 사이 간격으로 이 값을 사용한다.
pub(crate) const ROUND_TIME_BUDGET: Duration = Duration::from_millis(16);

/// debug에서는 환경변수로 예산을 줄여 이월·재깨움 경로를 시험할 수 있다. 늘리는 값은 무시한다.
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

#[cfg(debug_assertions)]
fn debug_env_millis(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    raw.trim()
        .parse::<u64>()
        .inspect_err(|e| tracing::warn!("{name}={raw:?} is not a number, ignored: {e}"))
        .ok()
}

pub(crate) struct IpcRound {
    started: Instant,
    taken: usize,
    count_budget: usize,
    time_budget: Duration,
    end: RoundEnd,
}

impl IpcRound {
    pub(crate) fn begin() -> Self {
        Self::with_budgets(DRAIN_BUDGET_PER_ROUND, round_time_budget())
    }

    pub(crate) fn with_budgets(count_budget: usize, time_budget: Duration) -> Self {
        Self {
            started: Instant::now(),
            taken: 0,
            count_budget,
            time_budget,
            end: RoundEnd::Drained,
        }
    }

    /// 예산으로 끝났으면 계속 None이다. 큐가 비거나 서버가 없는 경우는 종료 상태를 기록하지 않아
    /// 재호출 시 조건이 달라지면 새 명령을 꺼낼 수 있다.
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

    /// 명령을 꺼낸 회차만 계측한다. Drained가 아니면 큐에 명령이 남아 있을 수 있다.
    pub(crate) fn finish(self, pressure: &PressureStats, dispatch: &DispatchStats) -> RoundEnd {
        if self.taken == 0 {
            return self.end;
        }
        pressure.record_drain(self.taken);
        dispatch.record_round(self.end);
        round_perf::record(self.taken, self.started.elapsed());
        self.end
    }
}

/// 명령을 처리한 회차의 길이를 모아 일정 회차마다 p50/p99/max 한 줄을 낸다.
/// 회차 길이는 명령 핸들러 시간의 합과 큐에서 꺼내는 비용이다. 100ms 이상만 남기는 느린 요청
/// 기록과 달리 짧은 회차도 모두 센다. 로그: TASTY_LOG=tasty::ipc::perf=info.
mod round_perf {
    use std::cell::RefCell;
    use std::time::Duration;

    use crate::perf_stats::SampleWindow;

    const WINDOW: usize = 300;

    #[derive(Clone, Copy)]
    pub(super) struct RoundSample {
        pub commands: usize,
        pub round_ms: f64,
    }

    pub(super) struct RoundPerf {
        rounds: SampleWindow<RoundSample>,
        since_dump: usize,
    }

    impl RoundPerf {
        fn new() -> Self {
            Self {
                rounds: SampleWindow::new(WINDOW),
                since_dump: 0,
            }
        }

        /// 표본을 넣고, dump할 때가 됐으면 줄을 돌려준다.
        pub(super) fn push(&mut self, sample: RoundSample) -> Option<String> {
            self.rounds.push(sample);
            self.since_dump += 1;
            if self.since_dump < WINDOW {
                return None;
            }
            self.since_dump = 0;
            let commands = self.rounds.percentiles(|s| s.commands as f64);
            let round = self.rounds.percentiles(|s| s.round_ms);
            Some(format!(
                "ipc_rounds n={} commands p50={:.0} p99={:.0} max={:.0} \
                 round_ms p50={:.2} p99={:.2} max={:.2}",
                self.rounds.len(),
                commands.p50,
                commands.p99,
                commands.max,
                round.p50,
                round.p99,
                round.max,
            ))
        }
    }

    thread_local! {
        static ROUNDS: RefCell<RoundPerf> = RefCell::new(RoundPerf::new());
    }

    pub(super) fn record(commands: usize, elapsed: Duration) {
        if !tracing::enabled!(target: "tasty::ipc::perf", tracing::Level::INFO) {
            return;
        }
        let sample = RoundSample {
            commands,
            round_ms: elapsed.as_secs_f64() * 1000.0,
        };
        if let Some(line) = ROUNDS.with_borrow_mut(|perf| perf.push(sample)) {
            tracing::info!(target: "tasty::ipc::perf", "{line}");
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn dumps_once_per_window() {
            let mut perf = RoundPerf::new();
            let mut lines = Vec::new();
            for i in 0..(WINDOW * 2) {
                if let Some(line) = perf.push(RoundSample {
                    commands: 1 + i % 3,
                    round_ms: 0.5,
                }) {
                    lines.push(line);
                }
            }
            assert_eq!(lines.len(), 2);
            assert_eq!(
                lines[0],
                "ipc_rounds n=300 commands p50=2 p99=3 max=3 round_ms p50=0.50 p99=0.50 max=0.50"
            );
        }
    }
}

/// 큐 대기와 호스트 처리 시간을 두 루프에서 같은 기준으로 기록한다.
/// 명령은 dispatch로 옮겨지므로 관측에 필요한 값만 보관하고 begin·finish를 나눈다.
pub(crate) struct CommandObservation {
    request_seq: u64,
    /// pressure 조회 자체가 원인 기록을 밀어내지 않도록 해당 메서드는 None으로 제외한다.
    method: Option<String>,
    caller: tasty_telemetry::slow_requests::CallerKind,
    queue_wait: Duration,
    started: Instant,
    /// 응답이 나중에 끝나도 이미 기록된 항목에서 결과를 볼 수 있도록 공유한다.
    outcome: tasty_telemetry::slow_requests::HostOutcomeCell,
}

impl CommandObservation {
    /// 기한이 지난 요청도 대기했으므로 실행 여부를 판단하기 전에 큐 시간을 기록한다.
    pub(crate) fn begin(pressure: &PressureStats, cmd: &IpcCommand) -> Self {
        let queue_wait = cmd.queue_wait();
        pressure.record_queue_wait(queue_wait);
        let method = tasty_ipc::alias::canonicalize(&cmd.request.method);
        // 인증 결과가 아닌 session_token 필드의 존재로 호출자 종류를 기록한다.
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

    /// 호스트 처리를 끝내거나 플러그인에 넘긴 시점까지 기록한다. 비동기 응답 완료 시각과는 다르다.
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

const PRESSURE_METHOD: &str = "system.pressure";

/// 기한이 지났거나 기다리던 쪽이 이미 응답했으면 실행하지 않는다.
/// 권한·감사·rate limit을 소비하기 전이며 큐 대기 시간은 이미 기록됐다.
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
                caller_agent_id: None,
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

    /// 수 예산에 도달한 회차는 새 명령이 와도 다시 시작하지 않는다.
    #[test]
    fn a_stopped_round_stays_stopped() {
        let server = MockIpcServer::new();
        push(&server, 1);
        let mut round = IpcRound::with_budgets(1, Duration::MAX);
        assert_eq!(drain(&mut round, &server).len(), 1);
        push(&server, 1);
        assert!(round.next(Some(&server)).is_none());
    }

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
            caller_agent_id: None,
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

    #[test]
    fn a_withdrawn_command_is_dropped_without_a_second_answer() {
        let dispatch = Arc::new(DispatchStats::default());
        let (cmd, rx) = bounded(60_000);
        cmd.lifecycle().withdraw();
        assert!(!claim_or_answer(&cmd, &dispatch));
        assert!(rx.try_recv().is_err(), "no second answer");
        assert_eq!(dispatch.snapshot().expired_before_run, 1);
    }

    #[test]
    fn a_command_within_its_deadline_runs_and_is_in_flight_until_its_waiter_lets_go() {
        let dispatch = Arc::new(DispatchStats::default());
        let (cmd, _rx) = bounded(60_000);
        let waiter = cmd.lifecycle();
        assert!(claim_or_answer(&cmd, &dispatch));
        assert_eq!(dispatch.snapshot().expired_before_run, 0);
        assert_eq!(dispatch.snapshot().in_flight, 1);
        // 응답 대기자가 남아 있으면 명령 객체를 놓은 뒤에도 in-flight로 센다.
        drop(cmd);
        assert_eq!(dispatch.snapshot().in_flight, 1);
        drop(waiter);
        assert_eq!(dispatch.snapshot().in_flight, 0);
    }

    #[test]
    fn no_server_means_an_empty_round() {
        let mut round = IpcRound::begin();
        assert!(round.next(None).is_none());
        assert_eq!(round.taken, 0);
    }

    fn named(method: &str) -> IpcCommand {
        let (resp_tx, _resp_rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            caller_agent_id: None,
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

    /// 진단 조회도 큐 대기는 세지만 느린 요청 기록에서는 제외한다.
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

        // 기록 이후에 응답 결과가 채워져도 같은 결과 셀에서 보인다.
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

    /// 두 루프 원문에 begin·finish가 한 번씩 있는지 확인한다. 실제 호출 횟수나 순서는 분석하지 않는다.
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

    /// 두 파일의 namespace 전달 호출 문자열과 Some(c.request_seq()) 인자를 확인한다.
    /// 주석·문자열도 포함한 원문 검사이며 호출 횟수나 c의 실제 바인딩은 확인하지 않는다.
    /// 다른 파일의 전달 경로는 대상이 아니다. 명령당 전달 횟수와 넘긴 번호는 stub 플러그인을
    /// 쓰는 행동 시험이 잰다. gui 의 `app::ipc::routing::namespace_forward_tests` 는
    /// `forward_owned_namespace` 안의 전달만 잰다(GUI App 을 시험에서 만들 수 없다).
    /// `ipc_step_routing` 에서 그 함수 밖에 둔 전달은 e2e
    /// `a_plugin_namespace_call_is_forwarded_to_the_plugin_exactly_once` 가 잰다. headless 의
    /// `boot::headless_dispatch::namespace_forward_tests`(`--no-default-features`)는
    /// `dispatch_command` 전체를 지나므로 구조상 그 밖의 전달도 덮지만 그 위치의 변이는 재지 않았다.
    /// 키를 뗀 사본의 번호 보존은 idempotency 시험이 확인한다.
    #[test]
    fn both_namespace_forwards_pass_the_commands_request_seq() {
        for (name, src) in [
            ("gui", include_str!("ipc/routing.rs")),
            ("headless", include_str!("../boot/headless_dispatch.rs")),
        ] {
            let body = src.split("\n#[cfg(test)]").next().unwrap_or(src);
            let calls: Vec<&str> = body
                .split(".forward_namespace_call(")
                .skip(1)
                .map(|rest| rest.split(");").next().unwrap_or(rest))
                .collect();
            assert_eq!(calls.len(), 1, "{name}");
            assert_eq!(
                body.matches("::forward_namespace_call").count(),
                0,
                "{name}: found ::forward_namespace_call outside the method-call form checked here; inspect code, comments, and string literals"
            );
            assert!(
                calls[0].contains("Some(c.request_seq())"),
                "{name}: {}",
                calls[0]
            );
        }
    }
}
