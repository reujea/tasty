# ADR-0422: 재시도 누계는 판정하는 보존소가 센다 — 노출 전 스냅샷까지만

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, idempotency, telemetry, retry, pressure, observability, adr-0305, adr-0333, adr-0421
- **Group**: ipc-contract

## Context

요청 압력 진단(`system.pressure`, [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md))
은 큐 대기 · handler 실행 · plugin 왕복 · DB · 연결을 센다. 진단 티켓이 열거한 축 가운데 **재시도**가
없었다. 재시도는 멱등 키를 실은 요청이 보존소에서 받는 판정으로 이미 갈려 있었다 — 재생 · 충돌 ·
버려짐 · (App 층의 지연 응답 이후) 진행 중([ADR-0421](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md)).
그 판정은 어디에도 세지지 않았다.

압력 응답의 핸들러는 같은 회차에 다른 작업이 고친다. 이 결정은 그 파일을 안 건드리고 **값을 읽을 수
있는 자리**까지만 만든다.

## Decision

**보존소(`idempotency::Store`)가 `decide` 안에서 판정 갈래마다 칸 하나씩 센다. 프로세스 누계는
`idempotency::retry_counts()` 로 읽는다.**

- **칸은 `Decision` 의 갈래와 1:1 이다** — `executed` · `replayed` · `conflicted` · `discarded` ·
  `in_flight`. `executed` 는 재시도가 아니지만 **모수**다. 재생 수만으로는 그것이 키 실은 실행 열 건 중
  하나인지 만 건 중 하나인지 모른다([ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
  의 "모수를 가른다" 와 같은 판단).
- **세는 자리는 판정 함수 하나이고, 두 층을 지나는 갈래 하나만 되돌린다.** engine 라우터(`begin`)와
  App 층(`run_app_layer`)이 둘 다 `Store::decide` 로 판정하므로, 층마다 세는 줄을 두지 않고 판정 함수
  안에서 센다. 그런데 **한 요청이 두 층의 판정을 지나는** 갈래가 있다 — App 층은 모든 키 실은
  `Mutate` 를 먼저 판정하고, 그 이름을 안 맡으면(`NotHandled`) 요청을 다음 층으로 넘긴다. engine
  라우터로 가면 거기서 한 번 더 `Execute` 로 세지고, 보존소를 안 지나는 층(GUI debug step · plugin
  namespace forward)으로 가면 실행되지 않은 셈이 모수에 남는다. 그래서 그 갈래에서 연 자리를 닫는
  `Store::abandon_unhandled` 가 앞 층이 올린 `executed` 를 함께 되돌린다. 결과로 한 요청은 **자기를
  실제로 맡은 층의 판정**으로만 한 번 세진다. 새 층이 보존소를 쓰면 판정은 저절로 세지지만, 그 층도
  요청을 넘기는 갈래가 있다면 같은 되돌림을 가져야 한다.
- **보존소의 잠금 안에 있다.** 원자 변수를 따로 두지 않는다 — 판정과 셈이 같은 잠금 아래라 둘이
  어긋날 수 없고, 로컬 `Store` 로 시험할 때 정확한 값이 나온다.
- **레이블이 없다.** 메서드 · 주체로 가르지 않는다 — 주체는 agent id 라 칸 수를 호출자가 정하게 된다
  (ADR-0305 가 호출자별 관측을 기각한 이유와 같다).
- **노출하지 않는다.** IPC 응답 모양은 이 결정에서 안 바뀐다. 소비자가 붙기 전까지 `retry_counts` 에
  사유를 단 `dead_code` 억제가 붙어 있다.

#### 보강 — 노출 착지 (2026-09-21, [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md))

위 마지막 조항이 미룬 걸음이 착지했다. `system.pressure` 의 `keyed_requests` 덩어리가
`retry_counts()` 를 칸 이름 그대로 싣고(`executed` · `replayed` · `conflicted` · `discarded` ·
`in_flight`), 그 걸음이 `dead_code` 억제를 지웠다. 칸의 뜻과 "한 요청 한 번" 규칙은 그대로다.

## Consequences

- **얻은 것**: 재시도가 실제로 얼마나 오는지, 그중 몇이 실행을 막았는지(재생 · 충돌 · 버려짐 · 합류)를
  프로세스 안에서 읽을 수 있다.
- **잃은 것**: 결정 시점에는 호출자가 못 읽었다. 보강: 노출하는 걸음이 [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md) 로 착지했다.
- **운영 비용**: 판정마다 정수 하나 증가. 보존소 잠금은 이미 잡혀 있다.

## Alternatives Considered

- **압력 게이지(`Core::pressure()`)에 둔다** — App 층의 relay 스레드와 보존소 입구는 `Core` 를 들고
  있지 않다. 판정이 나는 자리에 `Core` 를 끌고 가야 한다.
- **층마다 원자 카운터를 둔다** — 판정 자리가 둘이라 셈 자리도 둘이 되고, 한쪽을 빠뜨리면 조용하다.
- **App 층이 표의 선언(`Kept { since: 2 }`)으로 먼저 걸러 자기 이름만 판정한다** — GUI 에서는 이중
  셈이 사라지지만, 헤드리스에서는 판 2 이름 가운데 `plugin.request_permission` 만 App 층이 맡고 나머지는
  engine 라우터로 떨어지므로 같은 이중 셈이 남는다. 되돌림은 두 조합을 함께 덮는다.
- **지금 `system.pressure` 에 붙인다** — 그 핸들러는 같은 회차에 다른 작업이 고치는 파일이다. 두 작업이
  같은 덩어리를 고치면 병합에서 한쪽이 사라진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 한 요청이 두 층에서 두 번 세지면 `idempotency::tests::a_request_that_crosses_both_layers_is_executed_once`
  가, 계약 밖 호출(debug step `Mutate` · namespace forward)이 `executed` 에 들면
  `a_call_outside_the_contract_is_not_counted_as_executed` 가 빨개진다(변이 확인: 되돌림을 0 으로
  바꾸면 둘 다 실패했다, 2026-09-21). 두 시험은 다른 시험과 안 섞이는 보존소에서 **정확한 값**을 본다.
- 셈이 빠지면 `idempotency::tests::every_decision_is_counted_once_in_its_own_slot` 과
  `the_process_counts_move_where_the_router_decides` 가 빨개진다(변이 확인: 증가분을 0 으로 바꾸면 둘 다
  실패했다, 2026-09-21).
- `Decision` 에 갈래가 더해지면 `decide` 의 셈 `match` 가 컴파일 오류로 칸을 요구한다.
- `retry_counts` 에 소비자가 붙으면 — 사유를 단 `dead_code` 억제를 그 걸음이 지운다(억제는 소비자가 생겨도
  경고 없이 남는다. 그 걸음의 검토가 유일한 채널이다). 보강: 소비자가 붙었고(`system.pressure`,
  [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)) 억제는 지워졌다. 이제 소비자가 사라지면 `dead_code` 가 빌드를 멈춘다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 칸 이름이 노출될 때 호출자가 읽는 뜻과 맞는가. 재는 법: 노출 걸음에서 응답 문서가 이 ADR 의 칸 정의를
  그대로 옮기는지 본다. 보강: 노출 걸음([ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md))에서 옮겼다 —
  [telemetry](../features/telemetry/index.md) 의 `keyed_requests` 문단과 CLI 도움말이 이 ADR 의 칸 정의를
  옮긴다. 옮긴 사본이 이 정의와 갈리는지는 여전히 사람이 본다.

## References

- 관련 ADR: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md),
  [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md),
  [ADR-0421](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md).
- **코드 근거 (결정이 실현된 현재 위치)**: `idempotency::RetryCounts` · `Store::decide` ·
  `Store::abandon_unhandled` · `Store::counts` ·
  `idempotency::retry_counts`.
