# ADR-0140: 호스트 IPC prefix 는 집행할 수 있는 자리에서 예약한다 — 파생이 아니라 고정으로

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: plugin, ipc, manifest, namespace, guards, compatibility, adr-0133
- **Group**: plugin-system

## Context

plugin 은 매니페스트 `[[contributes.ipc_namespace]]` 로 prefix 를 점유하고, 그 뒤
호스트가 모르는 `<prefix>.*` 호출이 그 plugin 으로 forward 된다. 점유를 막는 목록이
`tasty_plugin_manifest::validators::RESERVED_IPC_PREFIXES` 이고, 21 개다.

호스트가 자기 메서드에 실제로 쓰는 prefix 는 **45 개**다(`METHOD_TABLE` 276 건 +
`DEBUG_METHODS` + `PREFIX_RULES`, 점 없는 메서드 `split`·`tree` 포함). 차이의 내역은
예약 18 · 번들 plugin 이 같은 이름을 점유해 예약 불가 2 · **예약된 적 없는 25**, 그리고
반대 방향으로 메서드 없는 예약 3 이다.

25 개가 열려 있으면 두 가지가 샌다. 기존 메서드가 가려지지는 않는다 — 표에 있는 이름은
호스트가 먼저 가져간다. 새는 것은 **표에 없는 이름**이다.

- 호출자가 `terminal.<표에 없는 무엇이든>` 을 부르면 그 plugin 이 답한다. 호출자(CLI ·
  에이전트)는 자기가 호스트에 말하는 줄 안다. 호스트 namespace 는 신뢰 표지인데 그
  표지를 plugin 이 쓸 수 있다.
- 호스트가 나중에 `terminal.foo` 를 더하면 그 순간 plugin 이 받던 이름을 뺏긴다. 깨지는
  시점이 추가 시점이라 원인이 안 보인다.

목록이 21 에 머문 이유는 정책이 아니라 유지 방식이었다. 손으로 유지됐고 호스트가 새
prefix 를 만들 때 함께 갱신된다는 보장이 없었다.

## Decision

**호스트가 쓰는 prefix 45 개 중 번들 plugin 이 이미 점유한 2 개를 뺀 전부를 예약한다.
집행은 매니페스트 검증(`Manifest::validate`)에 그대로 두고, 목록은 파생하지 않고
가드(`src/source_guards/reserved_ipc_prefixes.rs`)로 고정한다.**

집행 지점이 이 결정의 형태를 정했다. 두 검증 층의 도달 범위가 다르다.

| 층 | 무엇을 아는가 | 언제 도는가 |
|----|---------------|-------------|
| `Manifest::validate` (manifest 크레이트) | 매니페스트만 | **모든 load** — discovery 가 매니페스트를 읽을 때마다 |
| `plugin_bridge::manifest_validate` (본체) | 본체의 file 도메인 등 호스트 지식 | 설치·추가 경로에서만 |

호스트 지식이 필요한 검증을 본체로 빼는 것은 이미 있는 관례다. 그런데 그 자리는 **설치
때만** 돈다 — 수동으로 복사된 plugin 은 지나간다. 모든 plugin 에 닿는 유일한 자리는
manifest 크레이트 쪽이고, 그 크레이트는 `METHOD_TABLE` 을 **볼 수 없다**. 의존 방향이
반대이기 때문이다(`tasty-ipc` 가 `tasty-plugin-manifest` 를 쓴다).

그래서 파생을 포기하고 고정을 택한다. 목록은 손으로 유지하되, 손으로 유지되는 것이
어긋나지 않도록 본체의 가드가 호스트 메서드 표와 **양방향 집합 동등**으로 대조한다
([ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) ③). 새 호스트 prefix
가 생기면 목록에 넣거나, 넣지 못하는 사유를 가드에 적어야 빨간불이 풀린다.

예약하지 않는 것은 둘뿐이고 이유가 측정된 것이다: `image` · `markdown` 은 번들 plugin
이 같은 이름의 namespace 를 갖고 있어서, 예약하면 그 매니페스트가 자기 검증에 걸려
plugin 이 뜨지 못한다.

## Consequences

- **얻은 것**: 호스트 namespace 를 plugin 이 점유할 수 없다. 신뢰 표지가 표지로
  남는다. 호스트가 나중에 메서드를 더해도 뺏을 이름이 없다.
- **잃은 것**: 25 개 이름이 plugin 에게서 사라진다. 그중 `theme` · `session` ·
  `preset` 처럼 plugin 이 자연히 고를 법한 이름이 있다.
- **가장 큰 비용 — 재지 못한다**: 이미 배포된 서드파티 plugin 이 그 25 개 중 하나를
  쓰고 있으면 **업그레이드 후 뜨지 않는다.** 매니페스트 검증에서 거절되므로 실패는
  요란하고(조용한 기능 저하가 아니라 명시적 에러) 원인이 메시지에 있다. 다만 그런
  plugin 이 **몇 개인지 잴 방법이 지금 없다** — 배포 레지스트리가 없어 설치 사례를
  집계할 채널 자체가 없다. "영향 없음" 이라고 쓰지 않는 이유가 이것이다. 재지 못하는
  것은 재지 못한다고 적는다.
  - 그 비용을 감수하는 근거는 **시점**이다. 지금은 번들 plugin 6 개만 namespace 를
    점유하고 있고 그중 이 결정에 걸리는 것은 0 이다(claimed: `agent_stream` ·
    `claude` · `codex` · `html` · `image` · `markdown` — 앞의 넷은 호스트 prefix 가
    아니고, 뒤의 둘은 예약 대상에서 뺐다). 레지스트리가 생긴 뒤에는 같은 결정의
    비용이 오르기만 한다.
- **운영 비용**: 목록이 손으로 유지된다. 그 부담을 가드가 대신 진다 — 어긋나면
  빨간불이지 리뷰어의 기억이 아니다. 새 호스트 prefix 를 만들 때 한 줄을 더해야 한다.
- 같은 prefix 를 두 plugin 이 점유하려 하면 두 번째 등록은 조용한 no-op 이다. 이
  결정은 그 형태를 바꾸지 않는다 — plugin 끼리의 충돌은 호스트 namespace 침범과 다른
  문제라 따로 다룬다.

## Alternatives Considered

- **집행을 본체로 옮기고 표에서 파생한다.** 목록이 사라져 어긋날 여지가 없어지므로
  가장 깔끔해 보인다. 기각한 이유는 도달 범위다 — 본체 측 검증은 설치 경로에서만 돌아
  수동 복사된 plugin 을 지나친다. **집행할 수 없는 자리에서 옳은 것보다 집행되는
  자리에서 고정된 것이 낫다.** 파생을 살리려면 의존 방향을 뒤집거나 메서드 표를 옮겨야
  하는데, 둘 다 이 결정보다 큰 변경이다.
- **골라서 예약한다** (`terminal` · `clipboard` · `settings` 처럼 사용자 대면 이름만).
  호환 비용이 작지만 기준을 적어야 하고, 그 기준이 다음 prefix 를 자동 분류하지 못하면
  손목록이 다시 자란다. 21 이 25 를 놓친 것과 같은 실패를 다시 만든다.
- **점유를 허용하되 알린다** (경고 + `plugin.list` 노출). 호환성은 안전하지만 조용히
  새는 형태가 그대로 남는다. 호출자는 여전히 자기가 호스트에 말하는 줄 안다.
- **동결만 하고 두기.** 직전 상태다. 가드가 "25 가 늘지 않는 것" 은 지켰지만 25 자체는
  열려 있었다. 시점 논거(위)가 이쪽을 기각한다.

## Reconsideration Triggers

- **배포 레지스트리가 생기면** — 그때는 영향 범위를 잴 수 있다. 이 ADR 이 못 잰다고
  적은 자리에 실측이 들어가고, 예약 범위를 좁힐 근거가 생길 수 있다.
- **서드파티 plugin 이 이 검증으로 죽었다는 보고가 오면** — 비용이 처음으로 관측되는
  순간이다. 유예 기간이나 이관 경로(예: 경고 후 다음 메이저에서 거절)를 다시 연다.
- **의존 방향이 바뀌거나 메서드 표가 옮겨지면** — 파생이 가능해지므로 위 대안 1 을
  다시 연다. 이 결정의 집합은 그대로 두고 유지 방식만 바뀐다.
- **plugin 끼리의 prefix 충돌을 다루게 되면** — 조용한 no-op 을 손볼 때 호스트 침범과
  같은 자리에서 판단할 수 있는지 본다.

## References

- [ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) — 모수를 열거하지
  않고 고정하는 형태. 이 결정의 유지 방식이 그 ③ 이다
- [plugin-development](../dev-guide/plugin-development.md) — `[[contributes.ipc_namespace]]`
  사용자 문서
- [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) — 같은 표를 두 벌 두면
  갈라진다는 같은 축의 판단
