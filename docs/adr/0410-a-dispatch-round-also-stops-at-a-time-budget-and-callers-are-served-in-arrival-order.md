# ADR-0410: dispatch 회차는 시간 예산에서도 멈추고, 호출자는 도착 순으로 섬긴다 — ADR-0313 의 회차 길이 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, dispatch, backpressure, fairness, headless, measurement, adr-0313, adr-0305
- **Group**: ipc-transport

## Context

[ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) 은 한 회차가 집어 드는
명령 **수**에 상한(`DRAIN_BUDGET_PER_ROUND` = 동시 연결 상한 256)을 뒀다. 그 ADR 은 시간 예산을
대안으로 보고 기각했다 — "벽시계라 재현이 안 되고, 느린 핸들러 하나가 예산을 통째로 먹는다".

수 예산이 자르는 것은 회차의 **명령 수**이지 **길이**가 아니다. 두 경로(gui `App::process_ipc`,
headless `pump_ipc`)는 둘 다 집어 든 것을 전부 실행한 뒤에야 회차를 끝냈으므로, 회차의 길이는
"집은 수 × 명령 하나의 비용" 이었고 뒤의 인수에는 상한이 없었다.

그 인수를 먼저 쟀다(2026-09-21, debug gui 빌드 · Xvfb · 격리 홈, `system.pressure` 누계의 전후
차분).

| 부하 | 명령 | 회차 | 회차당 평균 · 최대 | handler 시간 분포(게이트 뒤) |
|---|---|---|---|---|
| 가벼운 조회·입력 8 종, 연결 16, 15 s | 125,323 | 22,977 | 5.5 · 16 | 62,664 건 중 1 ms 미만 62,657 · 최대 2.7 ms · 평균 23 µs |
| 탭 만들기·목록·닫기, 연결 4, 10 s | 1,732 | 651 | 2.7 · 16 | 10–31.6 ms 칸 449 · 31.6–100 ms 칸 9 · 최대 36 ms · 평균 5.5 ms |

가벼운 부하만 보면 시간 예산은 필요 없다 — 256 건을 다 채워도 6 ms 남짓이다. 무거운 명령이
섞이면 달라진다. 회차당 명령 수는 연결 수까지 오르므로(두 부하 모두 최대가 연결 수와 같았다),
무거운 명령을 보내는 연결이 수십이면 한 회차가 수백 ms, 256 이면 초 단위가 된다. 그동안 gui 는
렌더·타이머로 못 넘어가고 headless 는 타이머로 못 넘어간다. 즉 수 예산은 **명령 비용이 고르다는
가정 위에서만** 회차 길이의 상한이다.

또 두 루프는 명령을 **먼저 전부 모으고** 나중에 실행했다. 모은 순간 명령은 큐 밖이고 입장
장부([ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md))에서도
빠진다. 그 상태에서 회차를 중간에 멈추면 남은 명령은 장부에도 큐에도 없는 자리에 놓인다.

호출자 사이의 기아는 다루는 문서가 없었다. [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
는 압력 게이지를 caller 로 나누지 않는다고 정했으므로, per-caller 공정성을 도입하면 그 결정과
부딪친다.

## Decision

**회차는 시간 예산에서도 멈춘다.** `ROUND_TIME_BUDGET` = 16 ms(60 Hz 한 프레임)이고, 수 예산과
**함께** 걸려 먼저 닿는 쪽에서 멈춘다. 값은 파생이 아니다 — 회차가 끝나야 루프가 렌더·타이머로
넘어가므로, "IPC 부하가 한 프레임보다 오래 루프를 쥐지 않는다" 를 단위로 골랐다. gui 에서 이 약속은
시간 예산만으로는 안 선다 — 다음 회차가 사용자 이벤트로 곧바로 이어지기 때문이고, 그 이음을 끊는
것은 [ADR-0413](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md)
이다(실측 표도 거기 있다).

- **꺼내면서 실행한다.** 회차는 명령을 하나 꺼내 끝까지 처리한 뒤 다음 것을 꺼낸다. 그래서 회차가
  멈추면 남은 명령은 **큐에 그대로 있고** 장부의 바이트·깊이에도, 큐 대기 시간에도 그대로
  남는다.
- **예산은 명령과 명령 사이에서만 본다.** 첫 명령은 시간과 무관하게 늘 처리한다 — 회차마다
  진척이 하나는 있고, 예산보다 비싼 명령이 영영 못 도는 일이 없다.
- **이월은 headless 에서 ADR-0313 의 근거 그대로다.** 생산자는 명령마다 waker 를 한 번 부르고 회차는
  적어도 하나를 처리하므로, 남은 wake 수는 남은 명령 수보다 적어지지 않는다. 빈 큐를 만난 회차는
  곧바로 끝난다. gui 는 wake 를 회차 없이 건너뛸 수 있어서, 예산에서 멈춘 회차가 루프를 스스로 한 번
  더 깨운다(ADR-0413).
- **gui 와 headless 가 같은 규칙 하나를 쓴다** — `src/app/ipc_round.rs` 의 `IpcRound`. 회차를
  닫을 때 처리한 수(`record_drain`)와 멈춘 이유(`DispatchStats`: 큐가 빔 · 수 예산 · 시간
  예산)를 한 자리에서 센다.

**호출자 사이의 기아는 도착 순(FIFO)으로 막고, 호출자별 스케줄링은 두지 않는다.** 근거는 소켓
경로의 구조다 — 연결 하나는 응답을 받을 때까지 다음 줄을 안 읽으므로 큐에 명령을 **한 번에
하나만** 올린다. 그래서 드물게 부르는 호출자 B 의 요청 앞에 설 수 있는 것은 다른 연결마다
하나(연결 상한 − 1)와 큐에 든 호스트 주입 명령(주입 깊이 상한 256)뿐이다 — 자리 수가 유한하고,
도착 순이므로 뒤에 온 요청이 B 를 앞지르지 못한다.

그 "하나" 에는 예외가 하나 있다. 호출자가 실은 응답 대기 상한([ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md))이
만료되면 연결은 다음 줄을 읽는데 만료된 명령은 큐에 남는다. 그 명령은 꺼내질 때 실행되지 않고
버려진다([ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md)) —
그래서 연결마다 큐에 **살아 있는** 명령은 여전히 하나이고, 남은 것은 비용 없이 지나간다.

**요청별 순서의 뜻**: 한 연결 안에서는 보낸 순서 그대로 하나씩 실행된다. 연결 사이에서는 큐에
도착한 순서로 **실행을 시작한다**. 응답을 워커 스레드로 넘기는 요청(`approval.await` ·
`agent.task_await` · plugin namespace 호출)은 시작 순서와 **완료** 순서가 다를 수 있다.

**종료 drain 은 이 규칙을 쓰지 않는다** — ADR-0313 그대로다.

**개정하는 것**: ADR-0313 의 "회차 길이는 명령 수로만 자른다"(시간 예산 기각). **개정하지 않는
것**: 수 예산의 값과 그 파생(`DRAIN_BUDGET_PER_ROUND = MAX_CONCURRENT_CONNECTIONS`), 이월에 새
배선을 안 만든다는 결정, 종료 drain 을 범위 밖에 둔 결정. ADR-0305 는 개정하지 않는다 — 호출자별
관측도 호출자별 스케줄링도 없다.

## Consequences

- **얻은 것**: 회차 길이의 상한이 명령 비용과 무관하게 "16 ms + 명령 하나" 가 된다. headless 는 이것만으로
  회차 사이에 타이머가 끼어든다. gui 는 ADR-0413 의 양보와 함께일 때 렌더·타이머가 한 프레임 단위로
  끼어든다.
- **얻은 것**: 멈춘 회차의 남은 명령이 장부와 큐 대기 계측에 그대로 보인다.
- **얻은 것**: 호출자 사이의 순서가 값으로 적혔다 — 앞에 설 수 있는 자리 수가 유한하다.
- **★ 잃은 것 / 정직하게 적는 한계**: 예산은 **이미 실행 중인 명령을 끊지 못한다.** 메인 스레드의
  동기 handler 하나가 오래 걸리면 그동안 회차도 타이머도 화면도 기다린다. 선점할 수단이 없다 —
  이 사실은 ADR-0313 의 "진행 보장의 한계" 보강 절에도 적었다.
- **잃은 것**: 무거운 부하에서 처리량이 준다 — 회차 사이에 렌더가 끼므로 같은 명령 수를 끝내는
  벽시계가 늘 수 있다. 그것이 이 결정이 산 것(루프 점유 상한)의 값이다.
- **잃은 것**: 호출자 **정체**(에이전트 · plugin) 단위 공정성은 없다. 연결을 여럿 여는 호출자는
  연결 수만큼 자리를 얻는다 — 그 수는 연결 상한이 자른다. 호스트 주입(plugin host-call)은 주입
  깊이 상한 안에서 서로 구별되지 않는다.
- **운영 비용**: 회차가 시간으로 잘리면 "한 회차가 주입 적체를 한 번에 비운다" 는 ADR-0391 의
  관계(`INJECTED_DEPTH_LIMIT <= DRAIN_BUDGET_PER_ROUND`)가 **수로만** 참이다. 그 관계를 적은
  자리(`tasty_ipc::admission` 과 ADR-0391)에 같은 말을 덧붙였다.

## Alternatives Considered

- **시간 예산 없이 둔다(현상 유지)** — 가벼운 부하 실측으로는 충분하다. 안 고른 이유: 수 예산이
  상한인 것은 명령 비용이 고를 때뿐이고, 무거운 명령의 실측 꼬리(수십 ms)와 연결 상한(256)의 곱이
  초 단위다.
- **ADR-0313 이 기각한 `recv_timeout` 형태** — 큐가 빌 때까지 시간을 두고 기다리는 안. 안 고른 이유는
  그 ADR 과 같다. 이 결정의 시간 예산은 **기다리지 않고**(`try_recv` 그대로) 이미 쌓인 것을 꺼내는
  시간만 자르며, 첫 명령을 늘 처리하므로 느린 명령 하나가 회차를 통째로 먹어도 진척은 남는다.
- **모은 뒤 실행 단계에만 시간 예산을 두고, 남은 것을 앱 쪽 버퍼로 넘긴다** — 루프 모양을 덜
  바꾼다. 안 고른 이유: 넘긴 명령이 장부에서도 큐에서도 빠진 채 기다려 바이트·대기 계측이 거짓이
  되고, 그 버퍼를 비우는 배선이 따로 필요하다.
- **호출자별 라운드로빈(caller 로 나눈 큐)** — 정체 단위 공정성을 준다. 안 고른 이유: 소켓 경로에서
  이미 연결마다 자리가 하나라 도착 순이 사실상 연결 단위 라운드로빈이다. caller 는 게이트(세션 토큰
  해석) 뒤에야 알 수 있어 큐가 caller 로 나뉘려면 게이트를 큐 앞으로 옮겨야 하고, 게이지를
  caller 로 나누지 않는다는 ADR-0305 와도 부딪친다.
- **값을 다른 상수에서 파생한다** — 렌더 루프에 "한 프레임" 을 뜻하는 공용 상수가 없다(부팅·종료
  기계의 frame interval 은 gui 전용이고 headless 에서 못 본다). 파생할 곳이 없어 고른 값으로 두고
  근거를 여기 적는다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 멈춘 회차의 남은 명령이 큐에 그대로 남지 않게 되면(먼저 모으는 모양으로 돌아가면)
  `a_round_stopped_by_count_leaves_the_rest_queued_in_order` · 
  `a_spent_time_budget_still_lets_the_first_command_through` 가 빨개진다.
- 시간 예산이 첫 명령까지 막으면 `a_spent_time_budget_still_lets_the_first_command_through` 가
  빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **정상 사용에서 시간 예산이 얼마나 자주 회차를 자르는가.** 거의 안 자르면 값이 느슨한 것이고,
  가벼운 부하에서도 자주 자르면 값이 빡빡하거나 명령 비용이 늘어난 것이다. 재는 법:
  `Core::dispatch` 의 `rounds_stopped_by_time` 을 `rounds` 와 견준다(보강: 노출 자리는
  `system.pressure` 의 `queue_dispatch` 덩어리다 — [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)).
- **처리량 손실이 체감되는가.** 재는 법: 같은 무거운 부하를 이 결정 전후 바이너리로 돌려 명령 수 /
  벽시계를 견준다.
- **호출자 정체 단위 공정성이 필요해지는가** — 한 에이전트가 연결 수십 개로 다른 에이전트를
  밀어내는 보고. 재는 법: 연결 수를 caller 별로 센다(지금 그 계측은 없다).

## References

- 부분 개정: [0465](0465-headless-keeps-one-ipc-wake-in-its-channel-and-a-cut-round-wakes-it-again.md) (headless 이월 bullet — 명령마다의 wake 를 하나로 접고 잘린 회차가 다시 깨운다)

- 개정 대상: [ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) (회차 길이를
  명령 수로만 자르는 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 관련: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
  (caller 로 나누지 않는 게이지 — 개정하지 않음) ·
  [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) (입장 장부) ·
  [ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md)
  (만료된 명령을 실행하지 않아 연결당 살아 있는 명령 하나를 지키는 짝 결정) ·
  [ADR-0413](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md)
  (gui 에서 회차가 루프를 쥐지 않게 하는 짝 결정)
- 결정이 실현된 현재 위치: `src/app/ipc_round.rs` 의 `IpcRound` · `ROUND_TIME_BUDGET`, 두 호출자
  `src/app/ipc.rs` 의 `App::process_ipc` 와 `src/boot/headless_dispatch.rs` 의 `pump_ipc`, 누계
  `crates/tasty-ipc/src/dispatch.rs` 의 `DispatchStats`
- 정본 문서: [data-flows](../architecture/data-flows.md) §3
