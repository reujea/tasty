# ADR-0412: in-flight 는 "시작됐고 호출자가 아직 기다리는" 요청을 센다 — 큐 계측은 한 자리에서 읽는다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, telemetry, diagnostics, observability, queue, adr-0305, adr-0391, adr-0411
- **Group**: ipc-transport

## Context

요청 지연을 진단하는 값 가운데 **in-flight 요청 수**가 없었다. `system.pressure` 의
`connections.live` 는 TCP 연결 수라, 요청 없이 붙어 있는 연결과 요청을 기다리는 연결을 가르지
못한다.

값을 두지 않은 데는 이유가 있었다. 메인 스레드의 동기 handler 는 한 번에 하나라 "지금 handler 안에
있는 요청" 을 세면 늘 0 아니면 1 이다. 조사 문서는 그래서 이 게이지를 dispatch 가 바뀐 **뒤**로
미뤘다.

바뀐 것은 둘이다. 회차가 명령을 하나씩 꺼내 실행하고([ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)),
명령마다 "시작했는가" 를 한 번 정하는 상태 칸이 생겼다([ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md)).
그리고 동기 handler 가 하나라도 **동시에 여럿이 진행 중인** 요청은 원래 있었다 — 응답을 워커
스레드로 넘기는 요청(`approval.await` · `agent.task_await` · plugin namespace 호출)은 메인
스레드를 떠난 뒤에도 호출자가 기다린다. "시작" 이 값으로 생긴 지금, 이 인구를 셀 좌변이 있다.

큐에 **든** 쪽의 값(바이트 · 명령 수 · 거절 누계)은 입장 장부([ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md))에
있고 "노출 자리는 아직 없다" 였다. 그 장부는 서버와 주입기가 나눠 들 뿐 `Core` 에는 없었다.

## Decision

**in-flight = 실행을 시작했고, 그 응답을 기다리는 쪽이 아직 기다리는 요청 수.**

- 시작은 [ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md)
  의 상태 칸이 `STARTED` 로 옮겨 가는 순간이다. 그 순간 몫(`FlightTicket`)이 그 칸에 실린다.
- 몫은 **그 칸을 든 마지막 쪽이 놓을 때** 빠진다. 칸은 명령과 기다리는 쪽(소켓의 연결 스레드,
  호스트 주입의 `HostIpcInjector::dispatch`)이 나눠 든다. 그래서 응답을 워커로 넘긴 요청은
  메인 스레드가 명령을 놓은 뒤에도 기다리는 쪽이 돌아갈 때까지 센다.
- **기다리는 쪽이 상한에서 돌아가면 그 요청은 빠진다** — 실행이 계속되더라도 받을 사람이 없는 일은
  세지 않는다. 그 인구(결과 불명으로 끝난 뒤에도 도는 일)는 이 값의 모수가 아니다.
- 실행하지 않은 요청(실행 전 만료)은 in-flight 에 안 들고 `expired_before_run` 에 든다.
- 최댓값(`in_flight_max`)과 시작 누계(`started`)를 함께 든다. 원자값이고 호출 수와 무관하게 자라지
  않으며 caller 로 나누지 않는다([ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
  의 축).

**큐 계측은 한 자리에서 읽는다** — `tasty_ipc::dispatch::CommandQueueSnapshot::read(장부, dispatch 누계)`.
장부 값은 그대로 싣고 다시 정의하지 않는다. 호스트는 `Core::dispatch()` 와, 주입기가 든 장부
(`HostIpcInjector::admission()` — 서버와 **같은** 장부다)로 이 함수를 부른다.

**이 결정은 IPC·CLI 노출을 하지 않는다.** 읽는 자리는 `system.pressure` 가 될 것이고 그 핸들러는
다른 작업이 같은 시기에 고치는 중이라, 덩어리를 더하는 것은 그 작업이 착지한 뒤의 별도 단계다.
그때까지 이 값을 읽는 것은 시험뿐이다.

#### 보강 — 노출 착지 (2026-09-21, [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md))

위 문단이 미룬 단계가 착지했다. `system.pressure` 가 이 함수 하나로 두 원천을 읽어 덩어리 둘로
싣는다 — 장부는 `queue_admission`(장부가 없으면 `null`), dispatch 누계는 `queue_dispatch`. 칸 이름은
`AdmissionSnapshot` · `DispatchSnapshot` 의 필드 이름 그대로이고, 장부 쪽에 그 장부가 집행하는 상한 둘이
더해진다. 이 결정이 정한 in-flight 의 정의는 그대로다.

## Consequences

- **얻은 것**: "연결은 많은데 요청이 없는가 · 요청이 긴 대기에 걸려 있는가 · 메인 스레드가 막혔는가"
  가 각각 다른 값으로 갈린다(`connections.live` · `in_flight` · `queue_before_gate`).
- **얻은 것**: 큐에 든 쪽과 꺼낸 쪽을 한 번에 읽는 자리가 생겨, 노출 단계가 값을 새로 정의할 일이
  없다.
- **잃은 것**: 결과 불명으로 끝난 뒤에도 계속 도는 일은 안 보인다. 그 일이 메인 스레드를 막으면
  `queue_before_gate` 의 대기로 나타난다.
- **잃은 것**: 장부에 닿는 길이 주입기를 거친다. 서버가 안 뜬 조립(시험용 `Core`)에서는 주입기가
  없어 장부 값이 `None` 이다 — "잰 적이 없다" 로 읽힌다.
- **운영 비용**: 결정 시점에는 노출 전이라 값이 프로세스 안에만 있었다(ADR-0305 가 겪은 것과 같은
  형태). 보강: [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md) 로 노출이 착지해 지금은 `tasty list pressure` 가 읽는다.

## Alternatives Considered

- **handler 안에 있는 동안만 센다** — 가장 작다. 안 고른 이유: 동기 handler 가 하나라 늘 0/1 이고,
  진짜로 동시에 진행 중인 인구(워커로 넘긴 요청)를 못 센다.
- **응답이 실제로 나갈 때까지 센다(응답 통로를 감싼다)** — 기다리는 쪽이 돌아간 뒤의 일까지 센다.
  안 고른 이유: 응답 통로(`SyncSender`)가 핸들러 수십 곳으로 복제돼 옮겨 다니므로 타입을 감싸는
  변경이 넓고, 그 인구는 "받을 사람이 없는 일" 이라 진단에서 따로 볼 값이다.
- **연결 스레드에서 센다** — 소켓 경로만 보고 호스트 주입을 못 센다. 그리고 그 파일은 다른 작업이
  같은 시기에 고친다.
- **지금 `system.pressure` 에 덧붙인다** — 한 번에 끝난다. 안 고른 이유: 같은 핸들러를 동시에 두
  작업이 고치면 덩어리 수를 세는 문장 여러 자리가 서로 다른 수로 갈린다(앞서 한 덩어리를 더한
  작업은 모듈 머리말 · CLI 도움말과 그 세 언어 사본 · API 참조 · 기능 문서 · 사용자 가이드 두 언어의
  수를 함께 옮겨야 했다). 노출은 착지 순서를 정하는 쪽이 한 번에 한다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 몫이 기다리는 쪽보다 먼저 빠지면(명령만 놓아도 줄면)
  `a_command_within_its_deadline_runs_and_is_in_flight_until_its_waiter_lets_go` 가 빨개진다.
- 실행하지 않은 요청이 in-flight 에 들면 `a_started_command_is_in_flight_until_both_holders_let_go`
  가 빨개진다.
- 스냅샷이 장부 값을 바꿔 실으면 `the_queue_snapshot_carries_the_ledger_as_is` 가 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **노출 단계가 착지했는가.** 보강: 착지했다([ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)). 이제 채널이 붙는다 — 응답이 `Core` 의
  누계와 주입기의 장부를 안 읽으면
  `pressure::tests::the_new_blocks_read_the_ledger_the_dispatch_gauge_and_the_process_store` 가 빨개진다.
- **결과 불명 뒤에도 도는 일이 진단에 필요해지는가.** 재는 법: 만료 로그(`IPC response wait of`)
  뒤에 메인 스레드 대기가 오르는 사례가 반복되는지 본다.

## References

- 관련: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) ·
  [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) ·
  [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md) ·
  [ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md)
- 결정이 실현된 현재 위치: `crates/tasty-ipc/src/dispatch.rs` 의 `DispatchStats::begin_flight` ·
  `FlightTicket` · `DispatchSnapshot::in_flight` · `CommandQueueSnapshot::read`,
  `crates/tasty-ipc/src/server.rs` 의 `IpcCommand::claim` · `CommandLifecycle`,
  `crates/tasty-ipc/src/host_call.rs` 의 `HostIpcInjector::admission`, 게이지 보유자 `src/core/mod.rs`
  의 `Core::dispatch`
- 정본 문서: [data-flows](../architecture/data-flows.md) §3
