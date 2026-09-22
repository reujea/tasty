# ADR-0333: 압력 게이지는 local-only 한 메서드 하나로 읽고, 응답은 모수마다 갈린다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: telemetry, ipc, cli, method-effect, local-only, pressure, memory, plugin-host, adr-0305
- **Group**: ipc-transport

## Context

[ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) 가
요청 압력을 **프로세스 게이지**로 정하고 큐 대기 · 큐 깊이 · handler 시간을 재게 했지만,
같은 ADR 이 "잃은 것" 으로 **노출 경로가 없다**를 적었다. 값은 프로세스 안에만 있었고
읽는 것은 시험뿐이라, 운영자가 느린 응답을 보고도 원인을 고를 수단이 없었다.

노출을 열려면 세 가지가 함께 정해져야 했다.

1. **어느 이름으로 나가는가.** 저장소에는 이미 `telemetry.*` 12 개가 있고 전부
   `plugin(..., &[Telemetry])` 다 — caller 별 사용량을 caller 에게 돌려주는 축이다.
   압력 게이지는 그 축이 아니다(0305 의 결정 자체가 "caller 로 나누지 않는다" 이다).
2. **누가 부를 수 있는가.** 같은 시기에 착지한 사건 피드(`events.fetch`)가
   `local_only(Read)` 로 갔지만, 그쪽 근거는 "plugin 은 이미 push 를 받는다" 와
   "`wait_ms` 가 plugin SDK 의 단일 워커를 막는다" 였다 — 둘 다 이 게이지에는 안 걸린다.
   같은 결론에 **다른 근거**가 필요했다.
3. **응답이 어떤 모양인가.** 재는 자리가 게이트 앞(`App::process_ipc` ·
   `headless_dispatch::pump_ipc`)과 게이트 뒤(`handle_checked_request`)로 갈려 있어
   **모수가 다르다**. 앞쪽은 뒤에 거부될 요청도 세고 뒤쪽은 통과한 것만 센다. 두 수를
   한 이름 아래 묶으면 운영자가 그 차이를 못 고른다.

여기에 같은 회차가 두 모수를 더 열었다 — host→plugin 왕복 대기
(`PluginManager::handle_plugin_response`)와 DB commit/checkpoint 지연
(`MemoryStore`). 셋째는 호스트가 **남의 프로세스를 기다린** 시간이고 넷째는
**디스크가 받아준** 시간이라, 앞의 둘과 또 다른 축이다.

## Decision

압력 게이지는 **`system.pressure` 하나**로 읽는다. `local_only(Read)` 이고 CLI 는
`tasty list pressure` 다. 응답은 **모수마다 한 덩어리**이고 덩어리 이름이 그 경계를
말한다 — `queue_before_gate` · `handler_after_gate` · `plugin_round_trip` · `db`.
호스트는 두 수를 **빼지 않는다**: 게이트를 통과하고도 app 층에서 답하고 돌아가는
갈래가 있어 그 차는 거부 수가 아니다. 관측이 없는 평균은 `0` 이 아니라 `null` 이다.
**아직 재지 않는 값의 덩어리를 미리 비워 두지 않는다** — 0 인 덩어리는 관측된 0 으로
읽히고, 그것이 평균을 `null` 로 두는 이유와 같은 함정이다.

게이지 타입은 **기록자가 사는 크레이트**에 둔다. 큐·handler·plugin 왕복은
`tasty-telemetry`, DB 지연은 `tasty-memory` 다(`tasty-telemetry` 가 `tasty-memory` 를
의존하므로 반대로 못 놓는다). 각 게이지는 `Core` 가 핸들을 들고, `Core` 는 그것을
**읽기만** 한다. DB 게이지는 `MemoryStore` 가 열릴 때 함께 태어나고 부팅 wiring 이
핸들을 꺼내 `Core` 에 복제한다 — 진단이 스토어 뮤텍스를 잡지 않게 하기 위해서다.

## Consequences

- **얻은 것**: 0305 가 "잃은 것" 으로 적어 둔 노출 경로가 생겼다. 불가침 원칙 2(IPC +
  CLI 양면)와 3(포커스 독립 — 프로세스 게이지라 창이 없다)을 지킨다. 응답을 그대로
  덤프해도 네 모수가 갈리므로, 운영자가 "큐가 밀렸다 · 명령이 무겁다 · plugin 이
  늦다 · 디스크가 늦다" 를 고를 수 있다.
- **얻은 것**: DB 게이지를 스토어가 낳으므로 **부팅 checkpoint 가 재진다.** 주입식이면
  그 자리는 `Core` 보다 먼저 돌아 영영 0 이었다.
- **잃은 것**: 같은 모양의 집계 타입이 두 크레이트에 각각 산다(`PressureStats` 계열과
  `DbLatencyStats`). 공용 크레이트로 올리면 하나로 줄지만 그 자리는 번들 plugin 의
  의존 폐포 안이라, 계측 하나가 plugin 아홉 개의 버전 bump 를 끌고 온다.
- **잃은 것**: `plugin_round_trip` 의 주입 자리와 `db` 덩어리가 `Core` 에 붙는 자리에
  **채널이 없다.** 배선이 끊기면 값이 조용히 0 으로 남고, 그것은 "왕복이 없었다" 와
  응답에서 구분되지 않는다.
- **운영 비용**: 게이지마다 원자값 몇 개씩이고 호출 수와 무관하게 크기가 고정이다.
  응답은 `Core` 의 원자값을 읽어 만들 뿐이라 진단이 잠금을 잡지 않는다.

## Alternatives Considered

- **A: `telemetry.pressure` 로 낸다** — 이름은 자연스럽지만 그 네임스페이스의 12 개가
  전부 caller 별 사용량이라, 같은 접두사 아래 **다른 축**이 섞인다. 0305 가 정한 성질
  ("caller 로 나누지 않는다")이 이름에서 안 보이게 된다. `system.gpu_stats` 가 같은
  성질(프로세스 진단 게이지)로 이미 `system.*` 에 있어 선례가 그쪽이다.
- **B: plugin 에게도 연다(`plugin(Read, &[Telemetry])`)** — 이 값은 부르는 쪽으로
  나뉘지 않으므로, plugin 에게 주면 **자기 몫이 아닌 부하까지** 읽는다. 권한을 어떻게
  주든 좁혀지지 않는다 — 좁힐 축 자체가 없다.
- **C: 한 덩어리 평평한 객체로 낸다** — 필드 이름에 경계를 녹일 수는 있지만, 덩어리가
  없으면 `commands` 와 `calls` 가 나란히 놓여 **뺄셈을 부른다.** 그 차는 거부 수가
  아니다.
- **D: 연결 수 게이지 자리를 미리 비워 둔다** — 스키마가 나중에 안 바뀌는 이점이
  있지만, 값이 0 인 덩어리는 "연결이 없었다" 로 읽힌다. 재는 자리가 생길 때 덩어리도
  같이 만든다.
- **E: DB 게이지를 `MemoryStore` 에서 직접 읽는다** — 핸들 복제가 없어 단순하지만,
  진단이 `Arc<Mutex<..>>` 를 잡아야 한다. **적체를 재려는 진단이 적체하는 자물쇠 뒤에서
  같이 막히면** 답이 가장 필요한 순간에 안 나온다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 가시성이 `local_only` 인가는 `pressure::tests::a_plugin_caller_is_refused_this_gauge`
  가 잡는다. 표에서 `plugin(...)` 으로 바뀌면 그 시험이 죽는다.
- 모수가 덩어리로 갈려 있는가는 `pressure::tests::the_two_moduli_land_in_their_own_blocks`
  와 `pressure::tests::the_db_populations_do_not_mix` 가 잡는다. 한 덩어리의 값이 다른
  덩어리 자리로 새면 죽는다.
- 창을 안 고르는가(불가침 원칙 3)는 `source_guards::unrouted_dispatch_reasons` 의 명부가
  잡는다 — 사유 없이 명부에서 빠지면 그 가드가 이름으로 문다.
- DB 게이지가 **스토어의 것**인가는 `boot::wiring::tests::the_extracted_gauge_is_the_one_the_store_raises`
  가 잡는다. 꺼내는 자리가 새 기본값을 돌려주면 죽는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **연결 수 게이지가 들어올 때 이 스키마가 맞는가.** 위 D 를 기각했으므로 그 덩어리는
  나중에 추가된다. 재는 법: 그 값을 담을 때 이름이 기존 넷과 같은 기준(모수 경계)으로
  읽히는지 본다 — 안 읽히면 덩어리 이름 규칙을 다시 정할 때다.
- **운영자가 실제로 원인을 가르는가.** 0305 가 "값이 쓸모 있는가" 로 남긴 조건이다.
  재는 법: 격리 `TASTY_HOME` 으로 인스턴스를 띄우고 동시 호출로 적체를 만든 뒤
  `tasty list pressure` 를 읽어, 큐 대기 max 와 handler max 가 **자릿수로** 갈리는지
  본다. 실측 2026-09-20 에는 갈렸다(동시 60 건에서 큐 대기 max 25.4 ms vs handler max
  0.48 ms · 관측된 큐 깊이 18).
- **`checkpoints_busy` 가 실제로 오르는 상황이 있는가.** 부팅 경로에서만 checkpoint 를
  걸므로 경합을 만들 표면이 없다. 재는 법: 다른 커넥션이 읽는 중에 truncate 가 걸리는
  실사용이 보고되면 그때 값이 오르는지 본다 — 영영 0 이면 그 원자값은 빼도 된다.

## References

- [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) —
  게이지의 성질을 정한 결정. 이 ADR 이 그 "잃은 것"(노출 경로 부재)을 푼다.
- [ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md) —
  사건 피드가 `local_only(Read)` 로 간 결정. 같은 결론이지만 근거가 다르다(위 Context 2).
- [telemetry](../features/telemetry/index.md) — "요청 압력 게이지 (프로세스 축)" 절이
  현재 운영 상태를 기술한다.
- [ADR-0340](0340-the-pressure-answer-counts-seats-and-carries-a-fixed-bound-distribution.md) —
  위 재검토 조건의 첫 항목("연결 수 게이지가 들어올 때 이 스키마가 맞는가")에 답한 결정.
  `connections` 덩어리를 더하고 시간 덩어리 셋에 분포를 달았다.
- [reference/api](../reference/api.md) — `system.pressure` 의 표면.
- 코드 근거(결정이 실현된 **현재** 위치): `src/adapters/ipc/handler/pressure.rs` ·
  `crates/tasty-telemetry/src/pressure.rs` · `crates/tasty-memory/src/latency.rs` ·
  `src/boot/wiring.rs` 의 `db_latency_of`.
