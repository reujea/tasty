# ADR-0391: 명령 큐는 쌓인 바이트와 주입 깊이로 입장을 판정한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, resource-bounds, admission, backpressure, queue, host-call, webhook, adr-0304, adr-0313, adr-0327
- **Group**: ipc-transport

## Context

IPC 서버의 명령 큐(`src/adapters/production/tcp_ipc_server.rs` 의 `TcpIpcServer::start_with_port_file`
이 만드는 `mpsc::channel`)는 무제한이다. 거기에 명령을 올리는 생산자는 둘이다 — 소켓 연결 스레드
(`dispatch_and_await`)와 호스트 자신의 주입기(`tasty_ipc::host_call::HostIpcInjector::dispatch`).

그 위에 쌓일 수 있는 명령의 **개수**는 [ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md)
이 파생했다: 두 생산자 모두 응답을 받을 때까지 블록하므로 연결 하나는 한 번에 한 건만 올리고, 연결
수는 [ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md) 의 연결 상한이
자른다. 그 파생이 못 덮는 것이 둘이었다.

- **바이트.** 한 건의 크기는 줄 상한(8 MiB)이 자르지만 쌓인 양은 아무도 안 쟀다. 이론상 최대는 연결
  상한 256 × 줄 상한 8 MiB = 2 GiB 다.
- **호스트 자신의 주입.** 주입기는 응답을 **시간 상한까지만** 기다린다(웹훅 스텝 10 s · agent runner
  · tell 의 `\r` 재주입). 메인 루프가 서 있는 동안 상한이 지나면 호출자는 돌아가는데 명령은 큐에 남는다.
  웹훅·idle 훅처럼 밖에서 계속 오는 사건이 그 호출자면 큐가 끝없이 자란다. 연결 상한은 이 인구를
  안 센다. ADR-0313 이 "잘릴 수 있는 것은 호스트 자신이 주입한 몫뿐이고, 그것은 정상적으로 회수된다" 고
  적은 그 몫이 **상한 없이** 쌓일 수 있는 유일한 몫이었다.

## Decision

**큐 앞에 입장 장부 하나(`tasty_ipc::admission::CommandAdmission`)를 두고, 명령을 큐에 넣기 전에 두
판정을 한다.** 장부는 서버가 만들고 소켓 경로와 주입기가 같은 것을 나눠 든다.

1. **쌓인 바이트** — 큐에 든 명령 무게의 합에 이 명령을 더해 `QUEUED_BYTES_LIMIT`(64 MiB)를 넘으면
   거절한다. **빈 큐는 한 건을 늘 받는다** — 한 건의 크기는 이 장부가 아니라 줄 상한의 몫이고, 상한보다
   큰 한 건을 영영 못 들이면 그 요청은 어떤 부하에서도 못 들어간다.
2. **주입 깊이** — 호스트 주입 명령만, 큐에 든 수가 `INJECTED_DEPTH_LIMIT`(256)이면 거절한다. 소켓
   명령은 이 수에 안 든다(연결 상한이 이미 자른다).

**1 바이트의 정의 — 다음 계측이 그대로 쓴다.** 명령 하나의 무게는 **그 요청의 JSON 텍스트 한 줄의
바이트 수**다. 개행과 앞뒤 공백은 빼고 센다.

- 소켓으로 온 요청은 **받은 줄**을 잰다(`trim` 한 뒤의 길이). 다시 직렬화하지 않는다.
- 호스트가 주입한 요청은 줄이 없으므로 `serde_json::to_vec` 의 compact 직렬화 길이를 잰다.
- 파싱된 요청이 메모리에서 차지하는 실제 크기(힙·`Value` 트리)가 아니다. 이 단위를 고른 이유는 줄
  상한과 **같은 자**여야 두 상한의 관계를 적을 수 있어서다.

**반납 시점은 큐에서 꺼낼 때다** — 서버 port 의 `try_recv` 가 명령의 몫을 돌려준다. handler 가 실행
중인 명령은 이미 큐 밖이다. 몫은 표(`AdmissionTicket`)가 들고 표가 버려질 때 빠지므로, 큐째 버려지는
종료 경로나 송신 실패로 명령이 되돌아오는 경로도 따로 배선하지 않는다.
이 반납 시점은 `src/adapters/production/tcp_ipc_server.rs` 의 시험
`a_dequeued_command_stops_counting_while_it_is_still_held` 가 고정한다. 꺼낸 명령을 쥔 채로 장부가 이미
0 인지를 본다.

**거절은 응답으로 온다.**

- 소켓 요청 — `ERR_COMMAND_QUEUE_FULL`(`-32065`)과 요청의 `id` 로 답하고 **연결을 유지한다.** 줄은
  끝까지 읽혔으므로 다음 요청을 같은 연결로 받아도 된다. 문구는 사유와 상한, 그리고 "nothing ran;
  retry it as is after a pause" 를 싣는다.
- 주입 — `HostIpcInjector::dispatch` 의 오류가 `String` 에서 `InjectError` 로 바뀌고 거절은
  `InjectError::Refused` 다. `InjectError::nothing_ran` 이 거절·송신 실패를 시간 초과(결과 불명)와
  가른다. `Display` 는 이전 문구를 그대로 낸다(거절만 새 문구).

주입 호출자마다의 처리:

| 호출자 | 처리 | 이유 |
|---|---|---|
| 웹훅·idle 훅 스텝(`src/hook_handler/exec.rs` 의 `execute_sequence`) | 거절을 다른 문구의 `error!` 로 남기고 다음 스텝으로 간다. 재시도 안 함 | 밖에서 계속 오는 사건이라 재시도가 곧 적체를 키운다 |
| agent runner(`src/core/agent/runner_host.rs` 의 `dispatch_plugin`) | 문자열로 올려 task 결과로 전파 | 결과가 agent 에게 가는 자리이고, 문구가 "nothing ran" 을 싣는다 |
| tell/spawn 의 `\r` 재주입(`src/adapters/ipc/handler/terminal.rs`) | `warn!` 만. 재시도 안 함 | 늦은 `\r` 은 사용자가 그 사이 친 입력을 엉뚱하게 제출한다 |

**값은 파생이 아니다 — 관계만 고정한다.** 본체 `tcp_ipc_server.rs` 가 컴파일 시점에 단정한다.

- `QUEUED_BYTES_LIMIT >= 2 × MAX_REQUEST_LINE_BYTES` — 최대 크기 요청 두 건이 동시에 대기할 수 있다.
  아니면 큰 요청 하나 뒤의 다른 큰 요청이 늘 거절된다.
- `QUEUED_BYTES_LIMIT < MAX_CONCURRENT_CONNECTIONS × MAX_REQUEST_LINE_BYTES` — 이 상한 이전의 이론상
  최대보다 작다. 아니면 상한이 한 번도 안 걸린다.
- `INJECTED_DEPTH_LIMIT <= DRAIN_BUDGET_PER_ROUND` — 한 dispatch 회차가 주입 적체를 한 번에 비운다.
  (2026-09-21 보강: 이 관계는 **명령 수로만** 참이다. 회차는 시간 예산에서도 멈추므로 —
  [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md) —
  주입 명령이 무거우면 적체가 여러 회차에 나뉘어 비워진다. 남은 것은 큐에 그대로 있어 이 장부가
  계속 센다.)

그 사이에서 64 MiB 와 256 을 고른 근거는 정상 인구다. 정상 요청은 수백 바이트 규모이고, 최대 크기
요청(8 MiB)은 드문 `memory.set` 하나다. 64 MiB 는 그런 요청 여덟 건이 겹쳐도 받는 값이다. 256 은
dispatch 회차 예산과 같은 값이다. **실행 인스턴스에서 분포를 재어 보정한 값은 아니다.**

**시험을 위해 debug 빌드에서만 환경변수로 낮출 수 있다** — `TASTY_DEBUG_IPC_QUEUE_BYTES` ·
`TASTY_DEBUG_IPC_INJECT_DEPTH`. release 빌드에는 그 코드가 없다. 목적은 한 상한을 다른 상한과 독립으로
넘겨 보는 것이다.

## Consequences

- **얻은 것**
  - 큐에 쌓이는 바이트에 상한이 생겼다(2 GiB 이론치 → 64 MiB).
  - 멈춘 메인 루프 앞에서 주입이 끝없이 쌓이지 않는다.
  - 주입 호출자가 "안 됐다" 와 "모른다" 를 가를 수 있다.
  - 두 상한이 연결 상한·줄 상한·회차 예산과 맺는 관계가 컴파일러에 고정됐다.
- **잃은 것**
  - 밀린 호스트에서 요청이 대기 대신 즉시 거절될 수 있다. 호출자가 `-32065` 를 처리해야 한다. 모르는
    client 에게는 그냥 에러 한 줄이다.
  - 주입기의 오류 타입이 바뀌어 문자열을 받던 호출자가 `to_string` 을 부른다.
  - 모든 명령에 장부 락 두 번(입장·반납)이 붙는다.
- **운영 비용 / 유지 부담**
  - 장부의 누계(`CommandAdmission::snapshot`: 지금 바이트·명령·주입 수, 최고 바이트, 거절 누계 둘)를
    결정 시점에는 **아무도 밖으로 내보내지 않았다.** 거절은 요청자의 응답과 `debug` 로그로만 드러났다.
    보강(2026-09-21): [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md) 로 `system.pressure` 의 `queue_admission` 덩어리가 그 누계와 두 상한을
    싣는다.
  - `tasty-ipc` 는 번들 plugin 의 의존 폐포 밖이다. plugin 버전 bump 가 없다.

## Alternatives Considered

- **A. `mpsc::sync_channel(N)` 으로 큐 자체를 bounded 로.** 칸 수는 개수만 자르고 바이트를 못 본다.
  가득 찬 칸에서 송신은 거절이 아니라 **대기**다. 소켓 스레드가 거기서 막히면 연결 자리를 쥐고,
  주입 호출자는 자기 시간 상한과 무관하게 막힌다. `try_send` 로 바꾸면 거절은 되지만 바이트는 여전히
  안 본다. 안 골랐다.
- **B. 바이트 상한을 두지 않고 그 이유를 값으로 박는다.** 과제가 허용한 대안이다. 이유는 연결 상한 ×
  줄 상한이 이미 유한하다는 것이 될 텐데, 그 유한값이 2 GiB 다. 정상 사용이 닿지 않는 값이라면 상한이
  아니라 서술이다. 안 골랐다.
- **C. 주입 깊이 대신 주입 호출자마다 따로 자른다**(웹훅 큐 · runner 동시성). 문제는 호출자가 아니라
  "큐에 남은 명령" 이다. 호출자별 상한은 새 호출자가 생길 때마다 빠진다. 안 골랐다.
- **D. 반납을 명령이 버려질 때로**(표의 Drop 에만 맡긴다). 그러면 이 장부가 대기열이 아니라 꺼낸
  명령을 쥔 동안까지 센다. **지금 트리에서는 차이가 작다.** 소비자(`src/app/ipc.rs` 의 `process_ipc`)가
  한 회차에 꺼낸 명령을 그 회차 안에서 값으로 소비하고 버리며, handler 에 넘기는 것은 명령이 아니라
  응답 통로다. 그래서 `approval.await` 같은 장수 handler 도 몫을 쥐지 않는다. 이 결정이 막는 것은
  **앞으로** 누가 명령을 회차 밖에 보관하게 바꿀 때 생길 조용한 과계수다. 그때도 장부가 대기열만
  재도록 반납을 `try_recv` 에 묶었다. 안 골랐다.

## Reconsideration Triggers

**채널이 붙는 것**

- 위 관계 셋 중 하나를 깨는 값 변경 — `tcp_ipc_server.rs` 의 컴파일 시점 단정이 빌드를 멈춘다. 그때
  관계가 여전히 맞는 말인지부터 다시 본다.
- 명령 큐에 세 번째 생산자가 생기면 — `IpcCommand::new`·`IpcCommand::with_wire_bytes` 의 호출 자리가
  `host_call.rs`·`tcp_ipc_server.rs` 밖에서 나타난다. 재는 법:
  `git grep -n "IpcCommand::\(new\|with_wire_bytes\)" -- '*.rs'` 에 시험이 아닌 다른 자리가 있다. 그
  생산자가 장부를 거치는지 본다.

**원리적으로 안 붙는 것**

- **`system.pressure` 가 장부 누계를 노출한 뒤, 그 분포로 두 값을 재보정한다.** 지금 값은 관계로 고른
  것이고 분포를 본 적이 없다. 보강: 노출은 착지했다([ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md), `queue_admission`). 정상 사용 기간의
  분포는 아직 안 읽었다. 재는 법: 노출된 최고 바이트(`peak_bytes`)와 거절 누계를 정상 사용 기간
  동안 읽는다. 정상 사용에서 거절이 0 이 아니거나 최고치가 상한의 절반을 넘으면 값을 다시 고른다.
- `-32065` 를 받은 client 가 재시도 폭주를 만든다는 보고. 재는 법: 거절 누계가 짧은 구간에 몰리는지
  본다. 그때 응답에 재시도 지연 힌트를 싣는 것을 검토한다.

## References

- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-ipc/src/admission.rs` 의 `CommandAdmission` ·
  `crates/tasty-ipc/src/server.rs` 의 `IpcCommand::admit` · `crates/tasty-ipc/src/host_call.rs` 의
  `InjectError` · `src/adapters/production/tcp_ipc_server.rs` 의 `dispatch_and_await`
- [ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md) — 줄 상한과 연결 상한
- [ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) — dispatch 회차 예산
- [ADR-0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md) — 전송 계층은 침묵 대신 코드로 답한다
- [ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md) — 결과 불명 코드
- [`docs/dev-guide/api-conventions.md`](../dev-guide/api-conventions.md) — 전송 계층 오류 코드 표
