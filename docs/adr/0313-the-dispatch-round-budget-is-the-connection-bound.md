# ADR-0313: dispatch 회차 예산은 동시 연결 상한과 같은 수이고, 이월은 별도 배선 없이 성립한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ipc, dispatch, backpressure, headless
- **Group**: ipc-transport

## Context

IPC 명령을 큐에서 꺼내는 자리는 둘이다 — gui 의 `App::process_ipc` 와 headless 의
`headless_dispatch::pump_ipc`. 둘 다 같은 형태였다: `try_recv` 가 빌 때까지 `Vec` 에 전부
모으고, 그 `Vec` 을 전부 처리한 뒤에야 회차를 끝낸다.

그래서 **회차의 길이를 큐가 정했다.** 보내는 쪽이 처리 속도보다 빠르게 밀어 넣으면 같은
회차가 끝나지 않고, 그 동안 타이머 · 터미널 출력 · 창 이벤트가 전부 뒤에서 기다린다. 상한이
없으므로 최악의 경우가 값으로 안 적힌다.

이 큐를 같은 형태로 비우는 자리는 트리에 셋이고([`src/app/ipc.rs`] · [`src/boot/headless_dispatch.rs`] ·
`src/app/shutdown_machine.rs`), 다른 채널을 비우는 같은 모양의 루프가 여섯 더 있다.
종료 drain 은 정상 회차가 아니라 이 결정의 대상이 아니다(아래 Consequences).

[`src/app/ipc.rs`]: ../../src/app/ipc.rs
[`src/boot/headless_dispatch.rs`]: ../../src/boot/headless_dispatch.rs

## Decision

한 회차가 집어 드는 명령 수에 상한 `DRAIN_BUDGET_PER_ROUND` 를 두고, **그 값을 고르지 않고
`MAX_CONCURRENT_CONNECTIONS` 에서 파생한다**(같은 파일, 같은 수). 근거는 두 생산자가 모두
응답을 받을 때까지 블록한다는 성질이다 — `TcpIpcServer::dispatch_and_await` 와
`HostIpcInjector::dispatch` 둘 다 `send` 직후 응답을 기다린다. 그러므로 살아 있는 연결 하나가
큐에 동시에 올려 둘 수 있는 명령은 최대 하나이고, 연결 수는 [ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md)
가 이미 자른다. 예산이 그 상한과 같으면 **TCP 경로만으로는 회차가 잘릴 수 없고**, 잘릴 수
있는 것은 호스트 자신이 주입한 몫뿐이다.

> 부분 개정: 시간 예산([ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md))이 생긴 뒤로는 TCP 경로만으로도 회차가 잘린다 —
> 수 예산에 못 닿아도 16 ms 에서 멈춘다.

**이월 재개에 새 배선을 만들지 않는다.** 두 생산자 모두 `send` 직후 waker 를 정확히 한 번
부르므로, N 개를 넣으면 wake 도 N 개가 큐에 들어간다. 한 회차가 B(<N) 개만 집어 들면 남은
N-B 개의 wake 가 그대로 남아 루프를 다시 들여보낸다. 즉 이월 재개는 이미 있는 성질이고,
이 ADR 이 하는 일은 그 성질에 **의존한다고 적는 것**이다.

> 부분 개정: gui 에서는 [ADR-0413](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md) 이후 wake 가 회차 없이 건너뛰어질 수 있어 이 성질이
> 성립하지 않는다. 그래서 gui 는 아래 대안의 "남은 것을 회차 끝에서 직접 다시 깨운다" 를 채택했다 —
> 예산에서 잘린 회차가 스스로 한 번 더 깨우고, 그 재깨움은 양보 규칙을 `about_to_wait` 한 번 사이에
> 한 번 건너뛴다. headless 도 [ADR-0465](0465-headless-keeps-one-ipc-wake-in-its-channel-and-a-cut-round-wakes-it-again.md) 이후 이 성질에 기대지 않는다 — wake 를 하나로 접고 잘린 회차가 다시 깨운다.

## Consequences

- **얻은 것**: 회차의 최악 길이가 값으로 적힌다. 그 값이 다른 상한에서 파생되므로 두 수가
  따로 낡지 않는다. gui 와 headless 가 같은 상수를 쓰므로 **예산 자체는** 두 경로에서
  갈리지 않는다 — 다만 이월의 **안전망**은 갈린다(아래 트리거).
- **잃은 것**: 예산에 닿는 순간 그 회차는 큐를 다 못 비운다. 그 사실은 응답이 아니라
  **다음 회차**로 나타나므로, 클라이언트에게는 지연으로만 보인다(wire 는 안 바뀐다).
- **운영 비용 / 유지 부담**: `MAX_CONCURRENT_CONNECTIONS` 를 움직이면 예산이 같이 움직인다 —
  그것이 의도다. 다만 **호스트 주입은 그 상한 밖**이라, 주입이 연결 수 규모로 늘어나면
  파생의 전제가 헐거워진다. 주입자는 plugin host-call 말고도 여럿이다
  ([ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) 의 Context).
- **종료 drain 은 범위 밖이다.** `src/app/shutdown_machine.rs` 의 같은 형태는 "남은 것을
  거절하며 비우는" 종료 절차이고([ADR-0078](0078-shutdown-rejects-pending-ipc.md)),
  거기서 예산을 두면 **남은 것을 거절하지 못한 채 종료**할 수 있다. 정상 회차의 정책을
  그대로 적용하지 않는다.

### 진행 보장의 한계 (보강, 2026-09-21)

회차 예산은 명령과 명령 **사이**에서만 작동한다. **이미 실행 중인 동기 handler 는 선점하지
못한다** — 메인 스레드에서 도는 handler 하나가 오래 걸리면 그동안 이 회차도, 뒤의 요청도,
타이머와 화면도 기다린다. 그 handler 를 끊을 수단은 트리에 없다. 호출자가 실은 응답 대기 상한
([ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md))이
만료돼도 호스트는 응답 통로를 놓을 뿐이고 실행은 계속된다. 그래서 "진행이 보장된다" 는 말이
뜻하는 것은 **큐에 든 요청이 유한한 자리 뒤에서 차례를 받는다**는 것이지, 요청 하나의 **시간**이
유한하다는 것이 아니다. 회차 길이를 시간으로도 자르는 결정은
[ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
이고, 그 결정도 이 한계를 넘지 못한다.

## Alternatives Considered

- **예산 없이 두고 관측만 한다** — `record_drain` 이 이미 회차마다 집어 든 수를 기록한다.
  관측은 최악을 보여 주지만 **막지는 않는다**. 이 결정이 막으려는 것은 회차 길이의 무한대이지
  그 값을 모르는 것이 아니다.
- **예산을 고른 상수(예: 64 · 256)로 둔다** — 처음 구현이 그랬다. 값의 근거가 "충분히 큰
  수" 라 다음 사람이 그 수를 바꿔도 되는지 알 수 없다. 파생하면 그 물음이 사라진다.
- **남은 것을 회차 끝에서 직접 다시 깨운다**(waker 를 한 번 더 부르거나 이벤트를 재발행) —
  이월을 **명시적으로** 만드는 안이다. 안 고른 이유는 그 wake 가 이미 큐에 있기 때문이다.
  더 부르면 빈 회차가 그 수만큼 늘고, 그 잉여는 "명령마다 한 번" 이라는 지금의 단순한
  불변식을 깬다. 다만 생산자가 늘어 그 불변식이 깨지면 이 안이 다시 답이 된다(아래 트리거).
- **`try_recv` 대신 `recv_timeout` 으로 시간 예산을 둔다** — 회차 길이를 시간으로 자르는 안.
  수가 아니라 벽시계라 재현이 안 되고, 느린 핸들러 하나가 예산을 통째로 먹는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `DRAIN_BUDGET_PER_ROUND` 의 우변이 `MAX_CONCURRENT_CONNECTIONS` 가 아니게 된다 —
  파생이 끊기면 이 ADR 의 근거 절반이 사라진다. 지금 이 조건을 재는 가드는 없다.
- `IpcCommand` 를 큐에 넣는 자리가 `TcpIpcServer::dispatch_and_await` 와
  `HostIpcInjector::dispatch` 말고 하나 더 생긴다 — "명령마다 wake 한 번" 이 이월의 유일한
  근거이므로, 그 불변식을 안 지키는 생산자가 들어오면 이월이 조용히 깨진다. 지금 이 조건을
  재는 가드는 없다.
  **깨졌을 때의 양상이 두 경로에서 다르다.** gui 에는 두 번째 경로가 있다 —
  `App::about_to_wait` 가 매 iteration 조건 없이 `process_ipc` 를 부르므로, IPC wake 가
  하나도 안 와도 다른 활동(타이머 · 터미널 출력 · 창 이벤트)이 루프를 깨우면 그 회차가 큐를
  다시 집는다. headless 에는 그 경로가 없다 — `dispatch_headless_event` 의
  `AppEvent::IpcReady` 갈래 하나만 `headless_dispatch::pump_ipc` 를 부르고,
  `run_headless` 의 `Wait::Deadline` 갈래는 타이머만 돌리고 `continue` 한다. 그래서 같은
  사고가 gui 에서는 **한 프레임 지연**, headless 에서는 **멈춤**이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 정상 사용이 예산에 실제로 닿는다. 재는 법: `record_drain` 이 기록한 회차 크기의 분포를
  보고 예산과 같은 값이 나오는지 본다 — 닿았다면 그 회차는 큐를 다 못 비운 것이다.
- 예산이 지연으로 체감된다. 재는 법: 예산을 낮춰 `tests/e2e_tests.rs` 를 돌리고 **`test
  result:` 줄**을 본다. **벽시계로 재지 않는다** — `tests/common/mod.rs` 의
  `TastyInstance::call` 이 읽기 타임아웃 10 s 로 최대 세 번 재시도하므로, 한 호출이 한 번
  막히는 것만으로 타깃 전체의 벽시계가 10 s 단위로 계단처럼 뛴다. 그 계단은 지연의 크기가
  아니라 하네스 상수다.
  실측 2026-09-20(같은 트리, 각 3 회): 예산 1 에서도 이 타깃은 `ok. 60 passed; 0 failed`
  이고 벽시계만 6.6 s → 16 s 로 움직였는데, 그 차이가 계단이라는 것이 세 갈래로 갈렸다 —
  `concurrent_requests_are_all_answered` 를 빼면 6.5 s, 그 시험만 돌리면 1.0 s, 그리고
  **예산은 1 로 둔 채** 위 읽기 타임아웃만 2 s 로 낮추면 6.6 s 다. 즉 이 타깃은 지연의
  **유무도 크기도** 못 잰다. 지연이 있으려면 예산에 먼저 닿아야 하므로, 순서는 위
  `record_drain` 항이 먼저다 — 닿은 회차가 없으면 지연도 없다.

## References

- 부분 개정: [0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md) (회차 길이를 명령 수로만 자르는 조항 개정 — 시간 예산을 함께 둔다)
- 부분 개정: [0413](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md) (gui 의 이월 재개 — wake 를 양보시키고 잘린 회차가 스스로 다시 깨운다)
- 부분 개정: [0465](0465-headless-keeps-one-ipc-wake-in-its-channel-and-a-cut-round-wakes-it-again.md) (headless 의 이월 — "명령마다 wake 한 번" 이 채널 적체를 만들어, wake 를 하나로 접고 잘린 회차가 다시 깨운다)

- 상한의 상대 결정: [ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md)
  (요청 한 줄 바이트 · 동시 연결 수)
- 종료 경로가 다른 정책을 쓰는 근거: [ADR-0078](0078-shutdown-rejects-pending-ipc.md)
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/production/tcp_ipc_server.rs` 의
  `DRAIN_BUDGET_PER_ROUND` · `MAX_CONCURRENT_CONNECTIONS`, `src/app/ipc.rs` 의
  `App::process_ipc`, `src/boot/headless_dispatch.rs` 의 `pump_ipc`
- 이월을 **관측하는 센서**: `tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered`.
  재는 것이 아니다 — 착지 예산(`MAX_CONCURRENT_CONNECTIONS`)에서는 동시 연결 16 이 예산에
  안 닿고, 예산을 1 로 낮춰도 이월이 실제로 동작하므로 통과한다. 이월이 깨졌는지를 가르는
  가드는 위 트리거대로 **없다.** 이 시험의 값은 착지 상태에서 큐 깊이를 1 보다 크게 만드는
  유일한 시나리오를 타깃 안에 둔 것이다.
