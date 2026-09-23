# ADR-0305: 요청 압력은 프로세스 게이지다 — caller 별 관측과 다른 축이다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ipc, telemetry, diagnostics, observability
- **Group**: ipc-transport

## Context

호스트가 IPC 경로에서 남기던 값은 `ipc_calls` 카운터 하나와 `rss_bytes` 샘플뿐이었고,
**요청이 얼마나 걸렸는지를 재는 자리는 한 곳도 없었다.** 그래서 응답이 느릴 때 그것이
큐 적체인지 handler 비용인지 고를 수단이 없었다. 뒤따르는 작업들(수신 상한의 값 정하기,
회차 공정성, plugin 격리)이 전부 "얼마나 밀렸나" 를 전제하는데 그 어휘가 없었다.

두 제약이 설계를 좁혔다.

- **기록 폭주.** 기존 `TelemetryEvent` 경로는 호출마다 저장소에 한 행을 남긴다. 지연을
  그 경로로 재면 진단이 진단하려는 문제를 스스로 만든다. [ADR-0085](0085-ipc-log-retention-bounded.md)
  가 같은 형태(allow audit 초당 14 건)를 이미 겪고 기록을 끄는 쪽으로 정리한 자리다.
- **관측 횟수 계약.** [ADR-0277](0277-ipc-admission-and-observation-run-once.md) 은
  "통과한 요청만 telemetry 와 Allow audit 을 **한 번** 수행한다" 로 정했다. 지금 throttled
  호출이 `record_ipc_call` 을 건너뛰는 것은 그 결정의 **구현이지 누락이 아니다.**

그런데 지연 진단의 요구는 반대로 읽힌다 — 거부된 요청도 큐에는 앉아 있었으므로 그 대기는
실재한다. 그것을 안 세면 적체의 절반이 안 보인다.

## Decision

요청 압력을 **caller 별 관측과 다른 축**으로 둔다. `PressureStats` 는 프로세스 수명 동안
누적되는 고정 크기 게이지다 — 원자값 여덟 개(결정 시점 — 아홉째 `queue_waits` 는 [ADR-0466](0466-the-queue-wait-mean-divides-by-the-waits-it-summed.md) 이 더했다)이고, 호출 수와 무관하게 자라지 않으며,
저장소를 거치지 않고, **caller 로 나누지 않는다.**

세는 값은 큐 깊이 · 큐 대기 · handler 실행 시간이다. 각각 count·sum·max 를 들며, 평균은
파생이라 메서드로 낸다. **분위수는 답하지 못한다** — 버킷 경계는 실제 분포를 보고 정할
일이라 지금 지어내지 않았다.

재는 자리가 게이트를 기준으로 갈린다. **큐 대기와 큐 깊이는 게이트 앞**에서, 명령이
큐에서 나온 직후에 잰다 — 거부될 요청도 큐에 앉아 있었으므로 그 시간은 실재한다.
**handler 실행 시간은 게이트 뒤**, `handle_checked_request` 에서 잰다 — 거부는 handler 를
돌리지 않았으므로 실행 비용이 아니다.

**이것은 ADR-0277 을 개정하지 않는다.** 그 결정이 "한 번" 으로 묶은 것은 caller 별
`ipc_calls` 이벤트와 Allow audit 이고, 그 둘은 **cap 평가와 anomaly 검출의 입력**이라
(`record_ipc_call` → `evaluate_caps_after_record` · `detect_anomalies_after_ipc`) 거부까지
세면 거부가 그 두 판정을 밀어 올린다. 여기 게이지는 caller 별도 아니고 영속도 아니며 그
두 판정의 입력도 아니다. 그래서 `ipc_calls` 를 거부까지 넓히는 대신 **다른 축을 하나
세우는 것**으로 같은 질문에 답한다.

rate-limit 은 **여기 섞지 않는다.** 토큰 소비는 게이트가 따로 하고
(`handler.rs` 의 `core.rate_limit_try_consume(agent, "ipc_calls", 1, …)`), 거기 쓰인
`"ipc_calls"` 는 token bucket 을 고르는 **버킷 이름 문자열**이다. `RateLimitStore` 는
telemetry 행을 한 건도 읽지 않으므로 `record_ipc_call` 을 거부까지 넓혀도 예산은 1 토큰도
안 깎인다. 두 축을 하나로 읽으면 실재하지 않는 결합을 근거로 삼게 된다.

## Consequences

- **얻은 것**: 적체와 handler 비용이 **다른 값**이 됐다. 그전에는 둘 다 없었다.
- **얻은 것**: GUI drain 과 headless drain 이 같은 자리에서 같은 값을 센다 — 창 없이만
  나는 적체도 보인다.
- **잃은 것**: **노출 경로가 없다.** 값은 프로세스 안에만 있고 이것을 읽는 IPC/CLI 가
  없다. 새 메서드는 wire 표면이라 별도 결정이다. 지금 이 값을 볼 수 있는 것은 시험뿐이다.
- **잃은 것**: **창이 없다.** 프로세스 수명 누계라 "지금 밀리는 중" 과 "부팅 직후 한 번
  밀렸다" 가 같은 max 로 보인다. 시간 창은 정책이라 뒤로 미뤘다.
- **잃은 것**: 스냅샷이 **원자적이지 않다.** 원자값을 따로 읽으므로 한 스냅샷 안의
  값들이 같은 순간의 것은 아니다. 진단값이라 허용했다.
- **운영 비용**: 관측 자리가 셋(GUI drain · headless drain · handler)이다. 넷째 진입
  경로가 생기면 그 자리도 세야 하고, 안 세면 그 경로만 조용히 0 이 된다.

## Alternatives Considered

- **`ipc_calls` 를 거부까지 넓히고 ADR-0277 을 개정한다** — 이 회차 규칙상 열려 있던 길이다.
  안 고른 이유는 그 카운터가 **cap 평가와 anomaly 검출의 입력**이라는 것이다. 거부를 거기
  실으면 거부된 요청이 cap 을 밀어 올려 cap 이 다시 거부를 부르고, anomaly 의 호출 burst
  판정도 거부로 부풀어 오른다. 관측을 고치려다 집행을 바꾸게 된다. (rate-limit 은 이
  논증에 안 들어간다 — 위 Decision 의 마지막 문단.)
- **`TelemetryEvent` 로 지연을 기록한다** — 호출당 한 행. ADR-0085 가 이미 이 형태를 껐다.
- **histogram 버킷을 지금 정한다** — 경계를 분포 없이 고르면 그 경계가 곧 분포를 말하는
  것처럼 읽힌다. count·sum·max 는 덜 말하지만 틀린 말은 안 한다.
- **`Clock` port 로 큐 진입 시각을 찍는다** — 명령을 만드는 두 production 자리(소켓 accept
  스레드, plugin host-call 주입부)가 `Core` 를 들고 있지 않다. 재는 것이 도메인 시각이
  아니라 큐 체류 시간이라 monotonic 원천이면 충분하다. handler 쪽은 `Core` 가 있으므로
  양 끝을 모두 port 로 읽는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 게이트 경계의 **뒤쪽 반**(거부는 handler 시간에 안 세고 통과는 센다)은
  `checked::tests::only_a_request_that_passed_the_gate_is_timed_as_a_handler` 가 잡는다.
  그 시험이 보는 것은 `handler_calls` 하나이므로 **거기까지다** — 아래 "안 붙는 것" 의
  첫 항목이 나머지 반이다.
- 집계 자체의 계약(최댓값이 안 내려가는 것, 1 마이크로초 미만도 횟수는 오르는 것,
  관측이 없으면 평균이 `None` 인 것)은 `tasty_telemetry::pressure` 의 시험이 잡는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **★ 게이트 경계의 앞쪽 반 — 큐 대기·깊이가 게이트보다 먼저 재지는가.** 이 결정이
  "거부될 요청도 큐에 앉아 있었으므로 그 시간은 실재한다" 고 말하는 근거가 이것인데,
  **채널이 없다.** 그 두 줄은 `App::process_ipc` 안에 있고 그 함수를 부르려면 창과 GPU 를
  든 `App` 이 필요해 단위 시험이 닿지 않는다(그래서 이 레포에 `App` 하네스가 없다).
  재는 법: `src/app/ipc.rs` 의 `record_drain` 과 `record_queue_wait` 두 줄을 지우고
  `cargo test -p tasty --lib --no-fail-fast` 를 돌린다 — **아무것도 안 죽으면** 채널이 없는
  것이고, 그것이 지금 상태다(실측 2026-09-20). 같은 방법으로 두 줄을
  `gates_before_routing` **뒤로** 옮겨도 안 죽는다. 그 이동이 바로 이 결정을 조용히
  거짓으로 만드는 변경이다.
  (2026-09-21 좌표 이동: `record_drain` 은 회차 규칙과 함께 `src/app/ipc_round.rs` 의
  `IpcRound::finish` 로 옮겨 gui·headless 가 한 자리를 쓴다 — 그 한 줄에는
  `only_rounds_that_took_something_are_counted` 가 채널로 붙었다.
  `record_queue_wait` 는 `src/app/ipc_round.rs` 의 `CommandObservation::begin` 첫 줄로 옮겨
  두 dispatch 루프가 명령을 꺼낸 직후 부른다 — 이 한 줄의 채널 유무는 미측정이다.)
- **값이 쓸모 있는가.** 노출 경로가 없으므로 지금은 아무도 안 읽는다. 재는 법: 노출
  메서드가 생긴 뒤, 실제 적체 상황에서 큐 대기 max 와 handler max 가 원인을 갈라 주는지
  본다. 안 갈라지면 재는 자리가 틀린 것이다.
- **누계가 창 없이 견디는가.** 재는 법: 장시간 세션에서 max 들이 초반 값에 고정되는지
  본다. 고정되면 창이 필요하다는 뜻이다.
- **네 번째 요청 진입 경로가 생겼는가.** 재는 법: `IpcCommand::new` 의 호출자를 센다.
  세 자리(소켓 · plugin host-call · 시험 더블)보다 늘었는데 drain 관측이 둘 그대로면 그
  경로의 대기 시간이 안 세지고 있다. 이 세는 법이 성립하는 것은 `IpcCommand::enqueued_at`
  이 비공개라 크레이트 밖에서 구조체 리터럴로 이 타입을 만들 수 없기 때문이다 — 공개였을
  때는 생성자를 우회한 경로가 이 계수에 안 잡혀 트리거가 거짓 초록을 냈다.

## References

- 후속: [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) — 이 결정이 "잃은 것" 으로 적은 노출 경로를 연다(본 결정의 본체는 그대로 유효하다).
- 관련 ADR: [ADR-0277](0277-ipc-admission-and-observation-run-once.md) — caller 별 관측의
  횟수 계약. 본 ADR 은 그것을 개정하지 않고 **다른 축**임을 밝힌다.
- 관련 ADR: [ADR-0085](0085-ipc-log-retention-bounded.md) — 호출당 영속 기록이 폭주한 선례.
- 관련 ADR: [ADR-0246](0246-telemetry-has-no-opt-out-and-its-token-is-a-declaration.md) —
  caller 별 telemetry 의 정책 축.
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty_telemetry::PressureStats` ·
  `PressureSnapshot` (`crates/tasty-telemetry/src/pressure.rs`), `Core::pressure`
  (`src/core/mod.rs`), `IpcCommand::queue_wait` (`crates/tasty-ipc/src/server.rs`),
  관측 자리 셋 — `App::process_ipc` (`src/app/ipc.rs`) · `pump_ipc`
  (`src/boot/headless_dispatch.rs`) · `handle_checked_request`
  (`src/adapters/ipc/handler.rs`).
- 부분 개정: [0340](0340-the-pressure-answer-counts-seats-and-carries-a-fixed-bound-distribution.md) (분위수 유보 조항 개정 — 고정 경계 histogram 이 들어왔다)
