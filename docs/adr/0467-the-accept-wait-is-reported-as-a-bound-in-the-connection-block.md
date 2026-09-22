# ADR-0467: accept 대기는 `connections` 덩어리에 상한으로 싣는다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: telemetry, pressure, ipc, accept, connection, measurement, adr-0333, adr-0340
- **Group**: ipc-transport

## Context

IPC 서버의 accept 루프는 논블로킹이다. `accept` 가 `WouldBlock` 을 돌려주면 100 ms 자고 다시 본다
(`src/adapters/production/tcp_ipc_server.rs` 의 `TcpIpcServer::start_with_port_file`). 그 사이 도착한
연결은 OS 의 accept 큐에서 기다린다. `system.pressure` 의 큐 대기(`queue_before_gate`)는 요청 줄을
읽은 **뒤에** 시작하므로 그 시간은 어느 덩어리에도 안 잡혔다.

실측(RF16 검증, 2026-09-21, headless · 20 회씩): 한 연결을 재사용하면 왕복 중앙값 0.2 ms, **연결마다
새로 열면 중앙값 100.1 ms(최소 95.6)**, 연결을 미리 열고 0.25 s 뒤 요청하면 0.6 ms. 차이는 전부 accept
대기다. CLI 처럼 호출마다 연결을 여는 client 의 약 100 ms 가 진단에서 "원인 없음" 으로 보였다.

이 결정은 그 대기를 **보이게** 하는 것까지다. 대기 자체(100 ms sleep)를 없애는 것은 별도 작업이다.

## Decision

**accept 루프가 꺼낸 연결마다 "accept 큐를 마지막으로 비어 있다고 본 뒤 지난 시간" 을 기록하고,
`connections` 덩어리에 싣는다.** 칸은 넷이다 — `accept_waits`(기록 수) · `accept_wait_bound_us_sum` ·
`accept_wait_bound_us_max` · `accept_wait_bound_us_mean`(기록이 없으면 `null`). 기존 칸과 덩어리 수는
안 바뀐다.

**이 값은 잰 대기가 아니라 상한이다.** OS 가 연결을 큐에 넣은 시각은 사용자 공간에서 안 보인다. 보이는
것은 루프가 큐를 마지막으로 비어 있다고 본 순간(`WouldBlock` 을 받은 순간)이고, 그 뒤에 꺼낸 연결은 그
순간 **뒤에** 도착했다. 그래서 `꺼낸 시각 − 마지막으로 비어 있던 시각` 은 실제 대기를 넘을 수 없다.
루프가 100 ms 자므로 이 값은 대개 0–100 ms 이고, 잠든 동안 고르게 도착한 연결의 실제 대기는 평균적으로
그 절반이다. 연속으로 꺼내는 동안(큐에 여럿이 쌓였던 때)에는 기준 순간이 안 바뀌므로 뒤에 꺼낸 것일수록
상한이 크다. 이름에 `bound` 를 넣은 것은 소비자가 이것을 잰 대기로 읽지 않게 하려는 것이다.

**덩어리는 `connections` 다.** 모수가 자리 계수와 같다 — 루프가 꺼낸 TCP 연결 전부이고, 자리를 받은
것과 상한에 걸려 거절된 것이 둘 다 들어간다. 그래서 `accept_waits = accepted + refused_saturated` 다.
[ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) 의 "모수마다
한 덩어리" 규칙에 맞는 자리이고, 새 덩어리를 만들면 같은 모수가 두 덩어리로 갈린다.

분포(`*_hist`)는 안 붙인다. 상한의 분포는 루프의 잠 주기를 그대로 되비칠 뿐이고, 잰 대기의 분포로
오독되기 쉽다.

## Consequences

- **얻은 것**: 호출마다 연결을 여는 client 의 지연 중 accept 몫이 `system.pressure` 에서 보인다 —
  `accept_wait_bound_us_mean` 이 수십 ms 이고 다른 덩어리가 작으면 그 시간은 accept 에 있었다.
- **잃은 것**: `connections` 가 "시간이 아니라 자리" 만 싣던 덩어리에서 시간 하나(상한)를 함께 싣는
  덩어리가 됐다. 그 문장을 적은 자리(CLI 도움말 · telemetry 문서 · API 참조 · site · 핸들러 모듈 doc)를
  같이 고쳤다.
- **운영 비용 / 유지 부담**: 원자값 셋과 accept 마다 `Instant::now()` 두 번. accept 루프가 이 기록을
  부르는지는 `accept_clock.rs` 의 `the_accept_loop_records_a_bound_for_every_connection_it_takes` 가
  실제 서버를 띄워 잰다(호출을 지우면 `accept_waits` 가 0 에 머물러 빨갛다).

## Alternatives Considered

- **새 덩어리 `accept` 를 더한다** — 모수가 `connections` 와 같아 한 모수가 두 덩어리로 갈린다. 덩어리
  수(열하나)를 적은 자리도 전부 움직여야 한다.
- **상한의 절반을 추정값으로 싣는다** — 도착이 고르다는 가정을 값에 박는다. 연속 도착에서는 틀린다.
  상한을 싣고 해석(절반)은 문서가 한다.
- **느린 요청 링의 호스트 줄에 accept 몫을 붙인다** — accept 는 연결 단위이고 한 연결이 여러 요청을
  보낼 수 있어 요청에 귀속할 수 없다(첫 요청에만 붙이면 두 번째부터는 0 이 아니라 "해당 없음" 이다).
- **accept 를 블로킹으로 바꿔 대기를 없앤다** — 이 결정의 범위 밖이다(종료 신호 처리까지 바뀐다).
  대기가 없어지면 이 상한도 0 근처로 내려가 그 변화가 보인다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- accept 루프가 블로킹이 되거나 readiness 통지(epoll 등)로 바뀐다 — 그때 "마지막으로 비어 있던 순간"
  의 뜻이 바뀐다. `accept_clock.rs` 의 모듈 doc 이 100 ms 잠을 전제로 적었다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 상한과 실제 대기의 차가 진단을 오도한다. 재는 법: 연결을 연 시각을 client 가 찍고 첫 응답까지의
  시간에서 다른 덩어리 몫을 뺀 값을 `accept_wait_bound_us_mean` 과 견준다.

## References

- 관련: [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md)
  (모수마다 한 덩어리) · [ADR-0340](0340-the-pressure-answer-counts-seats-and-carries-a-fixed-bound-distribution.md)
  (자리 덩어리와 분포) · [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)
  (덩어리 수 — 이 결정은 바꾸지 않는다)
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/production/tcp_ipc_server/accept_clock.rs` 의
  `AcceptClock`, `crates/tasty-telemetry/src/pressure.rs` 의 `ConnectionStats::record_accept_wait`,
  `src/adapters/ipc/handler/pressure.rs` 의 `snapshot_json`
- 설계 문서: [`docs/features/telemetry/index.md`](../features/telemetry/index.md)
