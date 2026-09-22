# ADR-0468: 느린 요청 줄의 호스트 몫은 호출자가 받은 답을 싣는다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: telemetry, pressure, ipc, slow-requests, outcome, measurement, adr-0436, adr-0411
- **Group**: ipc-transport

## Context

`system.pressure` 의 `slow_requests` 링([ADR-0436](0436-an-ipc-request-is-numbered-by-the-host-and-slow-ones-are-kept-in-a-ring.md))은
느린 요청 한 건의 시간이 어디에 있었는지를 싣는다. plugin hop 에는 결과(`outcome` = `ok`/`error`/`expired`/`cancelled`)가
있지만 호스트 몫(`host`)에는 없었다.

같은 느린 줄이라도 처방이 다르다. 큐에서 기한이 지나 실행하지 않은 요청(`-32067`, [ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md))은
"호스트가 밀렸다", 게이트가 거절한 요청(`-32001`)은 "권한/토큰", 정상 답은 "그냥 느렸다" 다. RF16 검증
(2026-09-21)에서 부하 중 링의 줄 세 종류가 모양이 같아 링만으로는 못 갈랐다 — 다른 덩어리의 누계(`queue_dispatch.expired_before_run`)는
건수만 말하고 어느 줄인지는 말하지 않는다.

답을 보내는 자리는 dispatch 안에 여럿이다(게이트 거절 · 조기 응답 · handler · 멱등 relay · plugin forward 의
뒤늦은 답). 그리고 상한 만료의 두 답(`-32067` · `-32061`)은 dispatch 가 아니라 **기다리는 쪽**(소켓 연결
스레드 · 호스트 주입기)이 스스로 만든다. 호스트가 명령을 다 다룬 순간(`CommandObservation::finish`)에는 plugin 으로
넘긴 요청의 답이 아직 없다.

## Decision

**호스트 몫에 `outcome`(`ok`/`error` — hop 과 같은 낱말)과 `error_code`(오류면 JSON-RPC 코드, 아니면 `null`)를
더한다.** 기존 칸은 안 바뀐다.

**값은 호출자에게 실제로 나간 답이고, 기다리는 쪽이 한 번 적는다.** 명령의 실행 상태 칸(`CommandLifecycle`)에
결과 칸(`HostOutcomeCell` — 한 번만 채워지는 공유 칸)을 두고, 두 대기 자리가 적는다:

- 소켓 경로(`tcp_ipc_server::await_dispatch_response`): 받은 답, 또는 상한에서 만든 `-32067` · `-32061` 을 쓰기 직전에.
- 호스트 주입(`HostIpcInjector::dispatch_inner`): 받은 답, 또는 기한을 실은 대기가 물러날 때 소켓 경로가 그
  자리에서 보냈을 코드(`-32067` · `-32061`). 기한 없는 대기가 물러나면 **안 적는다** — 명령이 큐에 남아 뒤에
  실행될 수 있어 호출자에게 나간 답이 없다.

**줄은 값이 아니라 칸을 든다.** `CommandObservation::begin` 이 명령에서 칸을 받아 `finish` 가 링에 넘기고,
`system.pressure` 가 읽는 순간의 값을 싣는다. 그래서 plugin 의 뒤늦은 답도 같은 줄에 보인다. 읽는 순간 아직
비어 있으면 두 칸 다 `null` 이다.

## Consequences

- **얻은 것**: `-32067` · `-32001` · 정상 답이 링만으로 갈린다. 코드가 그대로 실려 `-32061`(결과 불명)이나
  handler 의 오류도 구분된다.
- **잃은 것**: 줄이 링에 든 뒤에도 값이 바뀐다(비어 있던 칸이 채워진다). 두 번 읽은 같은 줄이 다를 수 있다 —
  한 번 채워지면 안 바뀐다.
- **운영 비용 / 유지 부담**: 명령마다 `Arc<OnceLock>` 하나. 새 대기 자리가 생기면 그 자리도 적어야 한다 —
  안 적으면 그 경로의 줄은 늘 `null` 이다. 두 대기 자리의 기록은 `tcp_ipc_server.rs` 의
  `a_wait_that_ends_while_queued_answers_not_run_and_the_command_is_not_run_later` ·
  `a_wait_past_the_callers_bound_answers_unknown_outcome`, `host_call.rs` 의 시험들이 칸 값으로 잰다.
  줄이 명령의 칸을 드는지는 `ipc_round.rs` 의 `a_slow_command_is_kept_and_the_pressure_query_itself_is_not` 가 잰다.

## Alternatives Considered

- **dispatch 가 답을 보내는 자리마다 적는다** — 자리가 여럿이고, 상한 만료의 두 답은 dispatch 밖에서 만들어져
  빠진다. 호출자가 받은 것과 적힌 것이 갈릴 수 있다.
- **`finish` 순간의 값을 복사해 싣는다** — plugin 으로 넘긴 요청은 그 순간 답이 없어 늘 비어 있다.
- **`JsonRpcResponse` 를 보내는 통로(`response_tx`)를 감싸 적는다** — 상한 만료의 답은 그 통로를 안 지난다.
  멱등 relay 는 중간에 자기 통로를 끼웠다가 원 통로로 답을 옮기므로, 통로 쪽에서는 같은 답이 두 번 지난다.
  기다리는 쪽은 relay 가 옮긴 답을 한 번 받는다.
- **hop 의 `HopOutcome` 을 재사용한다** — `expired`/`cancelled` 는 plugin 대기의 사건이고, 호스트 답의 오류는 코드가
  여럿이라 낱말 하나로 못 싣는다. 낱말(`ok`/`error`)만 맞추고 코드는 칸을 따로 둔다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `IpcCommand` 의 응답을 기다리는 자리가 셋째로 생긴다(예: 새 transport). 그 자리가 `LifecycleHandle::record_answer`
  를 안 부르면 그 경로의 줄은 늘 `null` 이다.
- 멱등 relay 가 원 요청의 통로가 아니라 자기 통로로 답하게 바뀐다 — 그때 relay 된 명령의 칸을 누가 적는지가 바뀐다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 부하 중 링에 `outcome` 이 `null` 인 줄이 오래 남는다. 대기 자리가 적지 않는 경로가 있다는 뜻이다. 재는 법:
  `tasty list pressure` 를 몇 초 간격으로 두 번 떠, 같은 `request_seq` 의 `host.outcome` 이 계속 `null` 인 줄의
  `method` 를 본다.

## References

- 관련: [ADR-0436](0436-an-ipc-request-is-numbered-by-the-host-and-slow-ones-are-kept-in-a-ring.md) (링과 요청 번호) ·
  [ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md) (실행 전 만료) ·
  [ADR-0451](0451-a-host-injection-carries-its-wait-as-a-deadline.md) (주입의 기한)
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-telemetry/src/slow_requests.rs` 의 `HostOutcome` · `HostOutcomeCell`,
  `crates/tasty-ipc/src/server.rs` 의 `LifecycleHandle::record_answer` · `IpcCommand::outcome_cell`,
  `src/adapters/production/tcp_ipc_server.rs` 의 `await_dispatch_response`, `crates/tasty-ipc/src/host_call.rs` 의
  `dispatch_inner`, `src/app/ipc_round.rs` 의 `CommandObservation`, `src/adapters/ipc/handler/pressure.rs` 의 `slow_requests_json`
- 설계 문서: [`docs/features/telemetry/index.md`](../features/telemetry/index.md)
