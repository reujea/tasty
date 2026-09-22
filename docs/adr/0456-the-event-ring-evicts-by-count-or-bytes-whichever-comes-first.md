# ADR-0456: 사건 링은 개수와 바이트 중 먼저 닿는 쪽으로 밀어낸다 — ADR-0322 의 용량 단위 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: events, event-bus, ring-buffer, retention, resource-bounds, plugin, adr-0322, adr-0360
- **Group**: event-feed

## Context

[ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) 는 사건 링의 용량을
**개수**(`EVENT_RING_CAPACITY` = 1024)로 정했다. 바이트로 두지 않은 근거는 둘이었다 — 바이트를
재려면 발화마다 직렬화해야 한다는 비용, 소비자가 쓰는 단위(`max`)가 개수라는 것. 그리고 링이
차지하는 바이트는 "코드에서 안 나오는 값" 으로 재검토 조건에 남겼다.

그 값이 처음 재어졌다. plugin 하나가 1 MB 사건 1100 건을 약 1.8 초에 발행하자 호스트 RSS 가
약 1 GB 늘었다(격리 debug 인스턴스, 발행 전 357 MB → 발행 뒤 1375 MB). 두 구독자는 모두 제때
소비했으므로 채널 큐가 아니라 **링 보존**이 쥔 몫이다. 링은 `1024 × 사건 크기` 를 쥘 수 있고
사건 크기에는 상한이 없다([ADR-0360](0360-plugin-channels-are-bounded-in-bytes-per-queue-and-in-total.md)
Consequences: "plugin 소켓에 줄 길이 상한이 없다").

ADR-0360 은 plugin 채널 큐를 바이트로 묶었지만 링은 그 장부 밖이다. 그래서 채널이 아무리 잘
묶여도 **빠른 발행자** 하나가 링을 통해 호스트 메모리를 사건 크기에 비례해 붙잡았다 — 느린
plugin 이 아니라 발행 속도가 만드는 몫이다.

제약: wire 를 바꾸지 않는다. 밀려난 사건을 알리는 새 신호를 만들지 않는다.

## Decision

**링에 바이트 상한(`EVENT_RING_BYTES_LIMIT` = 16 MiB)을 더하고, 개수 상한과 둘 중 먼저 닿는
쪽으로 가장 오래된 사건을 밀어낸다.** 개정하는 것은 ADR-0322 의 "용량의 단위는 개수다" 조항
하나다.

- **재는 것은 envelope 의 JSON 직렬화 길이다.** 발화마다 버퍼 없이 한 번 센다(쓰기 대상이
  길이만 더하는 writer). ADR-0360 의 채널 장부와 같은 단위 — 소켓에 실리는 줄의 바이트 — 라서
  두 상한이 같은 자로 잰 값을 말한다. 메모리 안 `serde_json::Value` 의 크기와 같지는 않지만
  그것에 비례한다. 세는 일은 버스 락 밖에서 한다.
- **가장 새 사건 하나는 크기와 무관하게 남긴다.** 상한보다 큰 사건을 받자마자 버리면 그
  사건은 위치만 받고 아무도 못 읽는다. 그래서 실제 상한은 `상한 + 사건 한 건` 이고, 그 한 건의
  크기는 줄 길이 상한의 몫이다 — ADR-0360 의 "빈 큐는 한 건을 늘 받는다" 와 같은 형태다.
- **밀려난 사건의 신호는 기존 것이다.** 링 앞이 움직이면 그 앞보다 오래된 위치를 묻는
  소비자는 개수로 밀려났을 때와 같이 `truncated: true` · `skipped: N` 을 받는다. 위치는
  되돌아가지 않는다. `events.fetch` 응답 모양도 plugin 구독(`event.dispatch`)도 바뀌지 않는다
  — 구독자에게 가는 fan-out 은 링과 무관하게 모든 사건을 보낸다.
- **값은 파생이 아니다.** 관측된 분포가 없다. 고른 근거는 관계 하나다 — plugin 채널 큐 하나의
  바이트 상한(`QUEUE_BYTES_LIMIT` = 16 MiB)과 같게 둬, 발행자 하나가 링을 통해 호스트에 붙잡아
  둘 수 있는 양이 채널 큐 하나가 붙잡는 양을 넘지 않게 했다. 평시 사건(수백 바이트)이면 개수
  상한이 먼저 닿는다 — 1024 × 1 KB 는 1 MiB 다.

**개정하지 않는 것**: 위치가 되돌아가지 않는다는 것 · 보존 밖 요청에 `truncated`/`skipped` 를
준다는 것 · 세대 표지(`epoch`) · debug 와 release 가 한 링을 쓴다는 것 · 사건을 디스크에 안
쓴다는 것 · 개수 상한 `EVENT_RING_CAPACITY` 의 값과 그 값을 한 곳에만 둔다는 것 — ADR-0322 의
나머지 전부다.

## Consequences

- **얻은 것**: 링이 쥐는 메모리에 처음으로 바이트 상한이 생겼다. 같은 재현(1 MB × 1100)에서
  RSS 증가가 약 1017 MB 에서 약 39 MB 로 줄었고, 링에는 16 건이 남았다(격리 debug 인스턴스
  A/B, 2026-09-21). 두 구독자는 두 경우 모두 1200 건을 drop 없이 받았다.
- **얻은 것**: 링이 밀어낸 사건은 소비자에게 `truncated`/`skipped` 로 드러난다 — 같은 재현에서
  `events fetch --offset 0` 이 `skipped: 1196` 을 냈다.
- **잃은 것**: 큰 사건이 오면 링이 **시간이 아니라 크기로** 빨리 돈다. 느린 `events.fetch`
  소비자는 1 MB 사건 열여섯 건 뒤에 `skipped` 를 본다. 개수만 볼 때보다 보존 창이 크기에 따라
  좁아진다.
- **잃은 것**: 발화마다 envelope 을 한 번 직렬화 길이로 센다. 호스트 사건은 자주 나지 않고,
  plugin 사건은 fan-out 이 어차피 구독자마다 직렬화한다.
- **운영 비용**: 링 한 칸에 `usize` 하나가 더 붙는다.

## Alternatives Considered

- **A: 개수 상한을 줄인다** — 사건 크기가 무한인 한 개수는 메모리를 묶지 못한다. 줄이는 만큼
  작은 사건의 보존 창만 좁아진다.
- **B: 바이트를 추정한다(`Value` 를 재귀로 센다)** — 직렬화 길이 세기와 거의 같은 일을 하면서
  결과는 근사이고, 채널 장부와 단위가 갈린다.
- **C: plugin 이 보낸 줄의 길이를 링까지 넘긴다** — plugin 발행에는 정확하지만 호스트 발행에는
  줄이 없다. 두 경로가 서로 다른 자로 재게 된다. 그리고 `publish_from_plugin` 의 서명이 바뀐다.
- **D: 채널 장부(`ChannelLedger`)의 합계에 링을 넣는다** — 링은 plugin 별 큐가 아니고 발행자가
  사라져도 남는다. 합계 상한이 걸리면 거절 대상이 없다(이미 fan-out 된 사건이다). 축이 다르다.
- **E: 상한보다 큰 사건은 링에 안 넣는다** — 그 사건이 위치만 받고 아무도 못 읽는다. 소비자가
  받는 `skipped` 에도 안 잡히는 구멍이 생긴다(위치는 있는데 사건이 없다).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 바이트 상한이 빠지거나 밀어낸 몫을 안 빼면 `event_bus::tests` 의
  `the_ring_evicts_by_bytes_and_says_so_like_it_does_by_count` ·
  `the_ring_counts_the_serialized_bytes_of_what_it_holds` 가 잡는다. 가장 새 사건을 안 남기면
  `an_event_larger_than_the_limit_is_kept_until_the_next_one` 이 잡는다(바이트 조건을 끄는 변이로
  셋이 죽는 것을 확인했다).
- 개수 상한이 바이트 상한에 가려지면 `the_count_limit_still_applies_to_small_events` 가 잡는다.
- plugin 소켓에 줄 길이 상한이 생기면 "가장 새 사건 하나" 의 크기가 묶인다. 위 "실제 상한은
  상한 + 사건 한 건" 문장을 고쳐 적는다.
- `QUEUE_BYTES_LIMIT` 가 바뀌면 이 값의 근거(같게 둔다)를 다시 판단한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 정상 사용에서 바이트 상한이 개수 상한보다 먼저 닿는가. 재는 법: `events fetch --offset 0`
  으로 물어 `stream_end − skipped` 를 센다 — 위치 0 부터 밀려난 수를 끝 위치에서 빼면 링에 남은
  칸 수다. 그 값이 1024 보다 작으면 바이트가 먼저 닿은 것이다. **링이 한 번이라도 돈 뒤에만
  의미가 있다** — 그 전에는 아무것도 밀려나지 않아 `skipped` 가 0 이고, 1024 보다 작은 값은 발화
  수가 아직 적다는 뜻일 뿐이다. `next_offset` 은 이 물음에 못 쓴다: 링을 끝까지 훑으면
  `stream_end` 와 같고, `max` 에 걸리면 `max` 를 따라 움직인다.

## References

- 개정 대상: [ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) (용량의 단위 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 관련 ADR: [ADR-0360](0360-plugin-channels-are-bounded-in-bytes-per-queue-and-in-total.md) — 채널 큐의
  바이트 상한. 이 ADR 은 그 장부 밖의 링에 같은 단위의 상한을 준다.
- 관련 reference: [`event-catalog.md`](../reference/event-catalog.md) "지나간 사건 — 위치로 읽는다"
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty-host-plugin` 의 `event_bus` 모듈 —
  `EVENT_RING_BYTES_LIMIT` · `serialized_len` · `EventBus::fan_out`.
