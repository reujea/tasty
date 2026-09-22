# ADR-0435: 큐 입장 · 큐 꺼냄 · 재시도 판정은 압력 응답에 세 덩어리로 더한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, cli, telemetry, pressure, admission, dispatch, idempotency, retry, compatibility, adr-0333, adr-0391, adr-0412, adr-0422
- **Group**: ipc-transport

## Context

요청 압력 응답(`system.pressure`, [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md))
은 모수마다 한 덩어리를 싣는다. 세 값 묶음이 프로세스 안에서 세지고 있었지만 그 응답에는 없었다:

- 명령 큐 **입장 장부** — 지금 든 바이트 · 명령 수 · 주입 명령 수, 최고 바이트, 거절 누계 둘
  ([ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md)).
- 큐에서 **꺼낸** 쪽의 누계 — 회차와 그 끝, 실행 전 만료, 시작한 요청, 지금 실행 중인 요청
  ([ADR-0412](0412-in-flight-counts-a-started-request-while-its-caller-still-waits.md)).
- 멱등 키를 실은 요청이 보존소에서 받은 **판정** 누계
  ([ADR-0422](0422-the-retry-counts-are-kept-by-the-store-that-decides.md)).

세 ADR 은 모두 "노출은 압력 응답의 핸들러를 고치는 걸음이 한 번에 한다" 로 끝났다. 그 걸음에서
정할 것이 둘이었다 — 덩어리를 몇 개로 나누는가, 이름과 칸을 무엇으로 하는가. 기존 호출자가 이미
읽는 덩어리(`queue_before_gate` 등 일곱)의 모양은 바꾸지 않는다는 것이 전제다.

## Decision

**세 묶음을 새 덩어리 셋으로 더한다 — `queue_admission` · `queue_dispatch` · `keyed_requests`.
기존 덩어리의 이름 · 칸 · 뜻은 하나도 바꾸지 않는다.**

- **셋으로 나누는 이유는 모수가 셋이라서다.** 응답의 규칙이 "모수마다 한 덩어리, 덩어리 이름에 그
  모수의 경계" 이고(ADR-0333), 세 묶음은 세는 대상이 서로 다르다. 입장 장부는 큐에 **들어오려던**
  요청(거절된 것 포함)을, 꺼냄 누계는 큐에서 **나온** 명령과 그 회차를, 판정 누계는 **키를 실은**
  요청만을 센다. 한 덩어리에 넣으면 `in_flight` 두 개(실행 중인 요청 · 진행 중 키에 합류한 요청)가
  한 이름 아래에서 부딪친다.
- **칸 이름은 원천 스냅샷의 필드 이름 그대로다.** 뜻의 정본은 원천이다(ADR-0391 · ADR-0412 ·
  ADR-0422). 응답 쪽에서 이름을 새로 지으면 같은 값에 이름이 둘 생긴다. 예외는 입장 장부의 상한
  둘이다 — `limit_bytes` · `limit_injected_depth` 로 싣는다. 상한은 게이지가 아니라 장부가 집행하는
  상수이고, `connections.limit` · `stream_push.sink_capacity` 와 같은 이유로 같이 나가야 지금 값이
  얼마나 찼는지가 한 응답에서 읽힌다. 원천 필드 이름(`queued_bytes`)을 그대로 쓰면 지금 값과
  이름이 겹친다.
- **원천은 `..` 없이 분해한다.** 필드를 이름으로 옮기는 응답은 원천에 필드가 더해져도 컴파일되고
  새 필드를 조용히 빠뜨린다. 분해가 모든 필드를 이름으로 부르게 하면 그 추가가 이 자리에서 빌드를
  멈춘다.
- **`queue_admission` 은 장부가 없으면 `null` 이다.** 장부는 IPC 서버가 만들고 주입기가 같은 것을
  든다. 서버가 안 뜬 조립(단위 시험, 서버 기동 실패)에는 장부가 없다. 0 이 든 덩어리를 내면 "관측된
  0" 으로 읽힌다 — `stream_push` 와 같은 규칙이다. 나머지 둘은 `Core` 와 프로세스가 늘 들고 있어
  `null` 이 되지 않는다. gui · 헤드리스 두 조합 모두 부팅이 장부 달린 주입기를 `Core` 에 넣으므로
  실행 중인 인스턴스에서는 셋 다 값이 있다.
- **두 원천은 한 번에 읽는다.** 입장 장부와 꺼냄 누계는 `CommandQueueSnapshot::read` 한 자리로
  읽는다(ADR-0412 가 노출 걸음을 위해 만든 자리). 두 읽기 사이의 원자성은 없다 — 각자 원자값이고
  진단용이다.

## Consequences

- **얻은 것**: "응답이 없다" 가 **큐가 찼다**(`queue_admission.refused_*`) · **꺼내는 쪽이 밀린다**
  (`queue_dispatch.rounds_stopped_by_*` · `expired_before_run`) · **실행 중인 것이 많다**
  (`queue_dispatch.in_flight`) · **재시도가 몰린다**(`keyed_requests`) 로 한 응답 안에서 갈린다.
  ADR-0391 이 재보정 조건으로 걸어 둔 분포(최고 바이트 · 거절 누계)를 이제 밖에서 모을 수 있다.
- **잃은 것**: 응답이 덩어리 열 개로 커진다. 덩어리 수를 세는 문장이 모듈 머리말 · CLI 도움말과
  그 번역 셋 · API 참조 · 기능 문서 · 사용자 가이드 두 언어 · CHANGELOG 에 흩어져 있어, 이 걸음이
  그 전부를 함께 옮긴다.
- **운영 비용**: 읽기마다 장부의 잠금 한 번과 보존소의 잠금 한 번. 진단 요청 빈도에서 무시할 수 있다.
  원천 스냅샷에 필드를 더하는 걸음은 이 응답의 분해도 함께 고쳐야 빌드된다 — 의도한 비용이다.

## Alternatives Considered

- **한 덩어리(`queue` 같은 이름)에 세 묶음을 다 넣는다** — 응답 모양이 가장 작게 는다. 안 고른 이유:
  모수가 셋이라 ADR-0333 의 규칙을 깨고, `in_flight` 두 개가 한 덩어리에서 부딪쳐 한쪽 이름을
  바꿔야 한다. 원천 필드 이름을 그대로 싣는다는 원칙이 첫 걸음에서 무너진다.
- **큐 둘을 한 덩어리(`command_queue`), 재시도를 따로** — `CommandQueueSnapshot` 의 모양과 1:1 이다.
  안 고른 이유: 입장 장부는 거절된 요청까지 세고 꺼냄 누계는 들어온 것만 센다. 모수가 다른 두
  묶음이 한 이름 아래 있으면 `queued_commands` 와 `started` 의 차를 운영자가 뺄셈으로 읽게 되는데,
  그 차는 "아직 큐에 있다" 가 아니다(큐 안에서 만료된 것 · 기다리던 쪽이 물러난 것이 섞인다).
- **기존 `queue_before_gate` 에 칸을 더한다** — 새 덩어리가 없어 문서의 덩어리 수가 안 바뀐다. 안
  고른 이유: 그 덩어리의 모수는 "큐에서 나온 직후, 게이트 앞" 이고 입장 장부는 큐에 **들기 전**이다.
  기존 덩어리의 뜻이 바뀌는 것은 칸을 더하는 것이라도 기존 호출자에게 호환 파괴다.
- **새 메서드(`system.queue` 등)로 따로 낸다** — 기존 응답이 전혀 안 바뀐다. 안 고른 이유: ADR-0333 이
  압력 진단을 한 메서드로 모은 이유(한 시점에 원인을 고른다)가 그대로 선다. 둘로 나누면 두 호출
  사이에 값이 움직인다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 세 덩어리가 원천 대신 다른 값을 읽으면
  `pressure::tests::the_new_blocks_read_the_ledger_the_dispatch_gauge_and_the_process_store` 가
  빨개진다(변이 확인 2026-09-21: 장부를 `None` 으로 · dispatch 누계를 새 기본값으로 · 판정 누계를
  기본값으로 · 상한을 제품 기본값으로 바꾼 넷이 모두 이 시험 하나로 실패했다).
- 칸이 다른 칸 자리로 새면 `the_queue_and_keyed_blocks_carry_their_sources_slot_by_slot` 이, 덩어리가
  응답에서 빠지면 `the_router_answers_this_name_for_a_local_caller` 가 빨개진다.
- 원천 스냅샷(`AdmissionSnapshot` · `DispatchSnapshot` · `RetryCounts`)에 필드가 더해지면 — 응답을 만드는
  세 함수(`queue_admission_json` · `queue_dispatch_json` · `keyed_requests_json`)가 원천을 `..` 없이
  분해하므로 빌드가 E0027(pattern does not mention field)로 멈춘다(변이 확인 2026-09-21: 세 구조체에
  각각 임시 필드를 더하고 생성 자리만 맞추자 셋 다 `pressure.rs` 의 분해에서 E0027 로 멈췄다). 그
  필드를 응답에 실을지 · 싣지 않을지를 그 걸음이 **이름으로** 정해야 한다 — 조용히 빠지는 길은 없다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 덩어리가 열을 넘어 한 응답이 읽기 어려워지는가. 재는 법: `tasty list pressure` 출력 한 번을 사람이
  읽고 원인 고르기에 쓸 수 있는지 본다.

## References

- 관련 ADR: [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) ·
  [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) ·
  [ADR-0412](0412-in-flight-counts-a-started-request-while-its-caller-still-waits.md) ·
  [ADR-0422](0422-the-retry-counts-are-kept-by-the-store-that-decides.md) ·
  [ADR-0400](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md)
  (`stream_push` 의 `null` 규칙 선례).
- 관련 문서: [`docs/features/telemetry/index.md`](../features/telemetry/index.md) ·
  [`docs/reference/api.md`](../reference/api.md).
- **코드 근거 (결정이 실현된 현재 위치)**: `adapters::ipc::handler::pressure::handle_system_pressure` ·
  `queue_admission_json` · `queue_dispatch_json` · `keyed_requests_json` ·
  `tasty_ipc::dispatch::CommandQueueSnapshot::read` · `idempotency::retry_counts`.
