# ADR-0411: 큐에서 기한이 지난 요청은 실행하지 않고 "실행 안 됨" 으로 답한다 — 만료는 취소가 아니다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, transport, timeout, wire, compatibility, cancellation, reliability, adr-0328, adr-0410
- **Group**: ipc-transport

## Context

[ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md)
은 호출자가 봉투에 응답 대기 상한(`response_timeout_ms`)을 싣게 하고, 만료를
`-32061`(결과 불명)로 답하게 했다. 판정은 연결 스레드의 `recv_timeout` 이 끝난 **뒤**에만 났다.

그래서 두 상태가 같은 답을 받았다.

- 요청이 메인 스레드에서 **실행되기 시작한 뒤** 만료됐다 — 결과를 정말 모른다.
- 요청이 **큐에서 기다리기만 하다** 만료됐다 — 호스트는 아직 아무것도 안 했다. 그런데 연결
  스레드는 만료만 답하고 명령을 큐에 그대로 두므로, 메인 스레드는 나중에 그 명령을 **꺼내 실행했다.**
  호출자는 이미 돌아갔으니 결과를 아무도 못 받는 효과만 남았다.

둘째 경우의 답이 "결과 불명" 인 것은 거짓이 아니었지만 — 실제로 나중에 실행됐으므로 — 그것은
호스트가 **만들어 낸** 불명이었다. 실행 직전에 기한을 보기만 했으면 "실행 안 됨" 이라고 확실히 말할
수 있었다.

또 이 경로는 [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
의 공정성 근거에 구멍을 냈다. 연결은 만료 뒤 다음 줄을 읽으므로, 만료된 명령이 큐에 남아 있는 동안
같은 연결이 새 명령을 또 올릴 수 있다 — "연결마다 큐에 하나" 가 깨진다.

그리고 ADR-0328 은 "요청은 계속 실행될 수 있다" 고만 적었고, **취소할 수 있는가**를 적은 자리가
없었다.

## Decision

**명령에 기한을 싣고 실행 직전에 본다.** 기한은 큐 진입 시각 + 호출자의 응답 대기 상한이다(상한이
없거나 0 이면 기한도 없다 — 봉투 규약 그대로). 메인 스레드는 명령을 꺼낸 직후, **게이트(권한 ·
audit · rate limit) 앞**에서 기한을 보고, 지났으면 실행하지 않는다.

**실행했는가는 명령마다 한 번, 비교-교환 하나로 정한다.** 명령과 그 응답을 기다리는 연결 스레드가
상태 칸 하나(`CommandLifecycle`)를 나눠 든다. 상태는 `QUEUED` 에서 `STARTED`(꺼낸 쪽이 실행을
시작) 또는 `WITHDRAWN`(기한이 지나 실행 안 함) 중 **먼저 온 쪽**으로 한 번만 움직인다.

- 꺼낸 쪽이 기한 경과를 보면 `WITHDRAWN` 으로 옮기고 스스로 "실행 안 됨" 을 답한다(기다리는 쪽이
  아직 있으면 그 답을 받는다).
- 기다리는 쪽의 상한이 먼저 끝나면 `WITHDRAWN` 으로 옮기려 한다. 성공했거나 이미 `WITHDRAWN`
  이면 "실행 안 됨" 을, 이미 `STARTED` 면 종전대로 "결과 불명"(`-32061`)을 답한다.
- 둘 다 이기거나 둘 다 지는 경우는 없다 — 시험이 경합을 200 회 돌려 그것을 본다.

**"실행 안 됨" 은 새 코드 `-32067`(`ERR_EXPIRED_BEFORE_RUN`)이다.** 전송 계층 구역(`-32060..-32069`,
[ADR-0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md))의
빈 자리이고, 이 구역의 다른 "실행 안 됨" 코드(`-32062` · `-32065` · `-32066`)처럼 사유마다 코드를
따로 둔다. 연결은 닫지 않는다. 응답은 요청의 `id` 를 돌려준다.

**서버는 이 동작을 이름으로 선언한다** — `system.info` 의 capability `ipc.response-timeout.not-run`
(판 1). 이 이름을 선언한 서버의 `-32061` 은 "요청이 이미 시작됐다" 까지 말한다.

**만료는 취소가 아니다 — 계약으로 적는다.** 시작 전이면 실행하지 않는다(위). **시작 뒤에는 끊지
않는다** — 메인 스레드의 동기 handler 를 선점할 수단이 없고([ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md)
의 "진행 보장의 한계"), 응답을 워커 스레드로 넘기는 요청(`approval.await` 등)도 만료로 멈추지
않는다. 만료가 하는 일은 **응답 통로를 놓는 것**뿐이고, 늦게 끝난 결과는 수신자 없음으로 버려진다.
그 밖의 취소 수단(명시적 취소 메서드 · 연결 끊김을 취소로 읽기)은 없다.

**호스트 주입(`HostIpcInjector`)은 바꾸지 않는다.** 주입기는 봉투에 상한을 싣지 않고(`None`) 제
시간 상한에서 물러날 때 명령을 큐에 남긴다 — 그 명령은 종전대로 나중에 실행된다.

**개정하는 것**: ADR-0328 의 "만료는 결과 불명이다" 를 "시작 뒤의 만료는 결과 불명, 시작 전의 만료는
실행 안 됨" 으로 좁힌다. **개정하지 않는 것**: 상한을 요청이 싣는다는 결정, 없거나 0 이면 상한이
없다는 규약, `-32061` 의 코드와 문장, 만료가 연결을 닫지 않는다는 결정, `ipc.response-timeout`
의 이름과 판(1).

## Consequences

- **얻은 것**: 큐에서 만료된 요청이 나중에 몰래 실행되지 않는다. 호출자는 `-32067` 을 받으면
  **그대로 다시 보내도 된다** — 상태를 먼저 읽을 필요가 없다.
- **얻은 것**: `-32061` 의 뜻이 좁아졌다 — 이제 "이미 시작됐다" 가 참이다. 그래서 그 답의 처방
  ("상태를 먼저 읽는다")이 과잉이던 경우가 사라졌다.
- **얻은 것**: 연결마다 큐에 **살아 있는** 명령은 하나라는 ADR-0410 의 근거가 다시 참이 됐다.
  만료된 명령은 큐에 남아도 꺼내질 때 비용 없이 버려진다.
- **얻은 것**: 실행되지 않은 요청이 게이트에 닿지 않는다 — rate limit 토큰도 audit 행도 안 쓴다.
- **잃은 것 / 호환**: 상한을 싣는 client 는 전에 `-32061` 을 받던 경우 일부에서 `-32067` 을
  받는다. 그 코드를 모르는 client 는 그것을 일반 오류로 읽는다 — 그 읽기는 **안전하다**(실제로
  아무것도 안 됐다). 다만 그런 client 는 "다시 보내도 된다" 는 정보를 못 쓴다. 상한을 안 싣는
  client(오늘의 기본 CLI 호출 전부)는 아무것도 안 바뀐다.
- **잃은 것**: 만료된 명령이 큐에서 꺼내질 때까지 입장 장부의 바이트·깊이에 남는다. 꺼내는
  순간 버려지므로 적체는 한 회차 안에서 풀린다.
- **운영 비용**: 기한 판정이 두 스레드에 있다. 어긋나지 않게 하는 것은 상태 칸 하나와, 두 쪽이
  같은 함수(`expired_before_run_response`)로 답을 만드는 것이다.

## Alternatives Considered

- **`-32061` 을 그대로 두고 `error.data` 에 "실행 안 됨" 표지를 싣는다** — 구 client 의 동작이
  글자 그대로 안 바뀐다. 안 고른 이유: 이 구역의 규약이 "사유마다 코드" 이고(`-32062` · `-32065` ·
  `-32066` 이 전부 그렇다), "결과 불명" 이라는 문장을 단 채 "실행 안 됨" 을 뜻하면 코드와 문장이
  서로를 부정한다. 모르는 코드를 받은 구 client 의 읽기(일반 오류)가 안전하다는 것도 확인했다.
- **꺼낸 쪽만 기한을 본다(상태 칸 없이)** — 가장 작다. 안 고른 이유: 연결 스레드의 타이머는 큐
  진입보다 조금 늦게 시작하므로 대개 꺼낸 쪽이 먼저 보지만, 메인 스레드가 서 있는 동안에는 연결
  스레드가 먼저 만료되고 그 순간 "시작했나" 를 물을 곳이 없다 — 그러면 다시 만들어 낸 불명이 된다.
- **연결 스레드가 만료 때 명령을 큐에서 빼낸다** — `mpsc` 큐는 중간 삭제를 못 한다. 상태 칸이 같은
  효과를 꺼낼 때 낸다.
- **`ipc.response-timeout` 의 판을 2 로 올린다** — 이름을 안 늘린다. 안 고른 이유: CLI 가 그 판을
  **최소 판으로 요구**하므로, 올리면 새 CLI 가 구 서버에 상한을 못 싣게 된다.
- **호스트 주입에도 같은 기한을 건다** — 주입기의 제 시간 상한을 봉투에 실으면 된다. 안 고른 이유:
  주입 호출자(웹훅 · plugin host-call · runner)가 `InjectError::Timeout` 을 결과 불명으로 읽도록
  짜여 있고, 그 갈래를 가르는 것은 이 단위의 범위 밖이다. 재검토 조건에 남긴다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 두 스레드가 둘 다 이기거나 둘 다 지면 `a_race_between_taker_and_waiter_has_one_winner` 가
  빨개진다.
- 큐에서 만료된 요청이 나중에 실행되면
  `a_wait_that_ends_while_queued_answers_not_run_and_the_command_is_not_run_later` 가 빨개진다.
- 시작된 요청의 만료가 "결과 불명" 이 아니게 되면
  `a_wait_past_the_callers_bound_answers_unknown_outcome` 이 빨개진다.
- 꺼낸 쪽이 만료된 명령을 실행하면 `a_command_past_its_deadline_is_answered_not_run_and_counted` 가
  빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **`-32067` 이 실제로 나오는가.** 상한을 싣는 호출자가 없으면 이 결정은 표면만 있다. 재는 법:
  `Core::dispatch` 의 `expired_before_run` 누계(보강: `system.pressure` 의 `queue_dispatch` 덩어리가
  싣는다 — [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md))나, 격리 인스턴스에서
  메인 스레드를 세워 둔 채 `--response-timeout-ms` 를 건 CLI 호출.
- **호스트 주입의 늦은 실행이 문제가 되는가** — 웹훅이 제 시간 상한 뒤에 실행돼 사고가 나는 보고.
  재는 법: `host_dispatch timeout after` 로그와 같은 요청의 실행 흔적을 시각으로 대조한다.

## References

- 개정 대상: [ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md)
  (만료의 뜻 — 시작 전/후로 가른다)
- 부분 개정: [0451](0451-a-host-injection-carries-its-wait-as-a-deadline.md) (호스트 주입은 바꾸지 않는다 — 조항 개정)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 관련: [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
  (연결마다 큐에 살아 있는 명령 하나 — 이 결정이 그 근거를 지킨다) ·
  [ADR-0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md)
  (전송 계층 코드 구역) · [ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md)
  (이름으로 선언하는 형태)
- 결정이 실현된 현재 위치: `crates/tasty-ipc/src/server.rs` 의 `IpcCommand::claim` ·
  `CommandLifecycle` · `LifecycleHandle::withdraw` · `expired_before_run_response`,
  `crates/tasty-ipc/src/protocol.rs` 의 `ERR_EXPIRED_BEFORE_RUN`, `crates/tasty-ipc/src/capability.rs`
  의 `RESPONSE_TIMEOUT_NOT_RUN`, 꺼내는 쪽 `src/app/ipc_round.rs` 의 `claim_or_answer`, 기다리는 쪽
  `src/adapters/production/tcp_ipc_server.rs` 의 `TcpIpcServer::await_dispatch_response`
- 정본 문서: [api-conventions](../dev-guide/api-conventions.md) "전송 계층이 직접 내는 코드"
