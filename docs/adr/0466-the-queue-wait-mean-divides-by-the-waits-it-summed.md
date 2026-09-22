# ADR-0466: 큐 대기 평균은 대기를 더한 수로 나눈다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: telemetry, pressure, ipc, queue, modulus, measurement, adr-0305, adr-0333
- **Group**: ipc-transport

## Context

`system.pressure` 의 `queue_before_gate` 덩어리는 큐 대기의 합 · 최댓값 · 분포와 그 평균
(`wait_us_mean`)을 싣는다([ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) ·
[ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md)).
평균의 분모는 `queue_commands` 였다.

두 수가 오르는 자리가 다르다. 대기 합 · 최댓값 · 분포는 명령을 **꺼내는 순간** 오른다
(`CommandObservation::begin` → `PressureStats::record_queue_wait`). `queue_commands` 는 회차가
**끝날 때** 그 회차가 꺼낸 수만큼 오른다(`IpcRound::finish` → `record_drain`). 그리고
`system.pressure` 는 자기 회차 **안에서** 답한다. 그래서 한 답 안에서 분자는 지금 도는 회차(조회
자신 포함)의 대기를 담고 분모는 그것을 안 담는다 — 경합이 아니라 **매 조회마다 결정적으로** 생긴다.

실측(RF16 검증, 2026-09-21): 부팅 직후 첫 조회에서 `commands` 1, 분포 합 2, `wait_us_sum` 208,299,
`wait_us_max` 207,972, **`wait_us_mean` 208,299 > max**. 다른 인스턴스에서도 mean 249 > max 216 이었다.
구조체 문서가 "원자적 스냅샷이 아니다" 를 적고 있었지만 그것은 다른 스레드와의 경합을 말한 것이다.
단위 시험 하나는 오히려 "분모는 명령 수지 대기 관측 수가 아니다" 를 고정하고 있었다.

## Decision

**평균은 분자와 같은 자리에서 오르는 수로 나눈다.** `PressureStats` 에 `queue_waits`(대기를 기록한
명령 수)를 두고 `record_queue_wait` 에서 합 · 최댓값 · 분포와 함께 올린다.
`PressureSnapshot::queue_wait_us_mean` 은 `queue_wait_us_sum / queue_waits` 다. 분포의 합은 그 분모와
같다.

**응답은 칸을 더하고 기존 칸을 안 바꾼다.** `queue_before_gate` 에 `waits` 를 더한다. `commands` 는
그대로 "회차가 꺼낸 명령 수의 합" 이고, `depth_mean`(= `commands / drains`)의 분자로 남는다 — 그
둘은 같은 자리(`record_drain`)에서 오르므로 모수가 맞다. 키 이름은 하나도 안 바뀐다. 값이 바뀌는
것은 `wait_us_mean` 하나이고, 그 값은 이제 최댓값을 넘지 않는다.

## Consequences

- **얻은 것**: 한 답 안에서 `wait_us_mean ≤ wait_us_max` 이고 `sum(wait_us_hist.counts) = waits`
  다. 조회하는 쪽이 파생값을 믿을 수 있다.
- **잃은 것**: `wait_us_mean` 을 `commands` 로 나눠 온 소비자는 값이 조금 달라진다(회차 안 명령만큼).
  같은 차분은 다음 조회에서 이미 맞춰지던 값이라 누계를 차분하는 소비자에게는 차이가 없다.
- **운영 비용 / 유지 부담**: 원자값 하나. 두 수(`waits` · `commands`)가 한 덩어리에 나란히 있어
  무엇을 세는지 CLI 도움말 · telemetry 문서 · API 참조가 함께 적는다.

## Alternatives Considered

- **`queue_commands` 를 명령을 꺼낼 때 올린다** — 모수는 맞지만 `commands` 의 의미가 바뀐다
  (`depth_mean` 의 분자로서 "회차가 끝날 때의 회차 합" 이던 것이 어긋난다). 기존 키의 의미를 안
  바꾸는 쪽을 골랐다.
- **스냅샷을 찍을 때 조회 자신의 회차를 빼고 계산한다** — 조회 자리만 고치고, 다른 스레드가 읽는
  경우(시험 · 미래의 다른 소비자)는 그대로 둔다. 분모를 분자와 같은 자리에 두면 읽는 자리와 무관하게
  맞는다.
- **평균을 응답에서 뺀다** — 기존 키가 사라져 호환이 깨진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 큐 대기를 기록하는 자리가 `record_queue_wait` 밖에 하나 더 생긴다 — 그 자리가 `queue_waits` 를 안
  올리면 평균이 다시 갈린다. `tasty-telemetry` 의
  `the_wait_mean_shares_its_modulus_with_the_sum_while_a_round_is_open` 이 그 짝을 잰다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 소비자가 `commands` 와 `waits` 를 같은 수로 읽고 뺀다. 재는 법: 두 수의 차가 "실행 안 된 명령" 으로
  보고되는지 운영 보고를 본다(그 차는 아직 도는 회차다).

## References

- 관련: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) (게이지의 축) ·
  [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) (모수마다 한 덩어리) ·
  [ADR-0340](0340-the-pressure-answer-counts-seats-and-carries-a-fixed-bound-distribution.md) (분포)
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-telemetry/src/pressure.rs` 의
  `PressureStats::record_queue_wait` · `PressureSnapshot::queue_wait_us_mean`,
  `src/adapters/ipc/handler/pressure.rs` 의 `snapshot_json`
- 설계 문서: [`docs/features/telemetry/index.md`](../features/telemetry/index.md)
