# ADR-0406: dispatch 에 응답하기 전의 publish 는 호스트가 hop 을 올린다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: events, event-bus, plugin, hop, loop-prevention, compatibility, adr-0321
- **Group**: event-feed

## Context

사건 카탈로그는 `meta.hop` 을 "호스트 발화 시 `0`, plugin 재발화 시 `+1`. `hop > 16`
(MAX_HOP) 이면 dispatcher 차단" 이라고 적었다. 그런데 호스트는 hop 을 **고치지 않았다** —
`publish_from_plugin` 은 plugin 이 적어 보낸 값을 `MAX_HOP` 과 견주기만 했다. `+1` 은 plugin
의 관례였고, SDK 에는 그 관례를 지킬 헬퍼도 없다. `publish_fresh` 는 항상 hop 0 · 새 trace 를
만든다.

실측(격리 headless 인스턴스, `agent.*` 를 구독한 시험 plugin): 받은 사건을 hop 0 · trace
`forged-fresh` 로 재발화하자 호스트가 그대로 받았다. hop 17 만 `MAX_HOP` 초과로 거절됐다.

그래서 서로의 사건에 반응하는 두 plugin 은 — SDK 의 `publish_fresh` 로 반응하기만 해도 —
hop 이 영영 0 에 머물러 `MAX_HOP` 차단에 닿지 않는다. 루프 차단 장치가 있는 것처럼 문서에
적혀 있었지만 그 장치에 입력되는 값은 루프 당사자가 정했다.

호스트가 쓸 수 있는 사실은 순서뿐이다. 호스트는 `event.dispatch` 의 응답을 기다리지 않지만(pending 을
만들지 않는다), plugin 은 응답한다 — 그리고 이 결정 뒤로는 응답해야 한다(아래 Consequences). SDK 는 `on_event` 를 **마친 뒤에** 응답하므로
(`tasty-plugin-sdk` runtime 의 `event.dispatch` 갈래), 콜백 안에서 한 publish 는 같은 연결에서
그 응답보다 먼저 도착한다. 그리고 pump 는 한 tick 안에서 plugin 의 사건을 응답보다 먼저
처리한다.

## Decision

**호스트가 plugin 에게 보낸 `event.dispatch` 중 응답이 안 온 것이 있는 동안 그 plugin 이
publish 하면, hop 을 `max(보낸 값, 그 dispatch 들의 hop 최댓값 + 1)` 로 올린 뒤 `MAX_HOP`
과 견준다.**

- **기록 자리** — 버스가 plugin 별로 (request id, hop) 을 든다. 송신에 성공한 dispatch 만
  기록한다(받지 않은 사건에 반응할 수는 없다). 응답이 오면 지운다. 응답 처리 경로에서 이
  id 를 버스가 먼저 가져가고, 버스의 것이 아니면 예전처럼 늦은 응답으로 정리한다.
- **판정 시점** — publish 가 **도착한 순간**이다. hook 을 거치는 publish 는 fan-out 이 hook
  응답 뒤로 밀리므로, 그때 판정하면 그 사이 dispatch 응답이 와 하한이 사라진다.
- **올리기만 한다** — plugin 이 더 큰 hop 을 적었으면 그대로 둔다. trace_id 는 고치지 않는다.
- **상한** — 응답하지 않는 plugin 에 기록이 끝없이 쌓이지 않도록 plugin 당
  `EVENT_RING_CAPACITY`(1024) 건을 넘으면 가장 오래된 것부터 버린다. 버린 기록은 하한에서
  빠질 뿐이라, 이 상한이 루프를 여는 방향으로 작동하려면 plugin 이 1024 건을 응답 없이
  쌓아야 한다.
- **정리** — plugin 이 멈추거나 재시작하면(`clear_plugin`) 그 plugin 의 기록을 지운다.
  재시작한 프로세스는 옛 프로세스가 받던 dispatch 에 응답하지 않는다.
- 문서의 `+1` 은 이제 호스트가 정하는 값이다. 사건 카탈로그의 `meta.hop` 행이 이 규칙을
  적는다.

## Consequences

- **얻은 것**: 서로의 사건에 hop 0 · 새 trace 로 반응하는 두 plugin 이 `MAX_HOP` 번 안에
  끊긴다. 단위 시험이 그 루프를 돌려 16 번째 반응 뒤 `HopExceeded` 로 끊기는 것을 고정한다.
- **얻은 것**: 호스트 사건을 받아 콜백 안에서 재발화한 envelope 의 hop 은 plugin 이 뭐라고
  적든 1 이상이다. 카탈로그의 서술과 코드가 같은 것을 말한다.
- **얻은 것**: 응답한 뒤의 publish 는 건드리지 않는다. 주기적으로 발화하는 plugin 이 사건을
  받았다는 이유만으로 hop 이 쌓여 차단되는 오탐이 없다(아래 대안 A 가 그 오탐을 낸다).
- **잃은 것 — 빠져나가는 길**: dispatch 에 **먼저 응답하고** 나중에 publish 하는 plugin 은 이
  하한에 안 걸린다. 콜백에서 일을 다른 스레드로 넘겨 나중에 발화하는 plugin, SDK 를 안 쓰고
  응답부터 보내는 plugin 이 그렇다. 그 경로는 이 결정 전과 같다(plugin 이 적은 값 그대로).
  루프가 그 길로 돌면 차단되지 않는다.
- **잃은 것**: 한 dispatch 를 처리하는 동안 **무관한** publish 를 한 plugin 은 그 publish 의
  hop 도 올라간다. 호스트는 콜백 안의 publish 가 반응인지 무관한지 가를 수 없다. 올라간
  hop 은 `MAX_HOP` 에 가까운 chain 이 아니면 아무것도 막지 않는다.
- **잃은 것 — 응답하지 않는 plugin**: 기록은 응답이 와야 지워지므로, `event.dispatch` 에 응답하지 않는
  plugin 은 **받은 최대 hop + 1 이 재시작 전까지 영구 하한**이 된다(그 사이 1024 건이 더 쌓여 밀려나지
  않는 한). hop 16 사건을 한 번 받고 응답하지 않으면 그 뒤의 publish 는 무엇이든 hop 17 로 올라 `MAX_HOP`
  으로 거절된다. SDK 는 `on_event` 뒤에 응답하므로 SDK plugin 은 해당하지 않는다. 그래서 plugin 계약에
  "`event.dispatch` 에 응답하라" 를 적는다(`docs/dev-guide/plugin-development.md` · 사건 카탈로그의 `meta.hop` 행).
- **운영 비용**: 이 규칙은 SDK 의 응답 순서(콜백 뒤 응답)에 기댄다. 그 순서가 바뀌면 하한이
  말없이 사라진다 — 아래 재검토 조건.

## Alternatives Considered

- **A: 시간 창 — 최근 T 안에 받은 사건의 hop 으로 하한을 건다** — 먼저 응답하는 plugin 도
  잡는다. 안 고른 이유는 오탐이다. 서로의 namespace 를 구독하고 T 보다 짧은 주기로 각자
  발화하는 두 plugin 은 반응이 아닌데도 hop 이 한 번에 하나씩 쌓여 `MAX_HOP` 에서 둘 다
  차단된다. 지금 동작하는 plugin 을 깨는 쪽이라 호환이 가장 적게 보존된다. T 의 값도 근거
  없이 정해야 한다.
- **B: SDK 에 재발화 헬퍼를 두고 문서의 `+1` 을 plugin 계약으로 적는다** — 호스트 변경이
  없다. 안 고른 이유는 계약을 지키는 쪽이 루프 당사자라는 점이 그대로라는 것이다. 헬퍼를
  안 쓰는 plugin(지금의 `publish_fresh` 사용자 전부)이 루프를 만들면 여전히 안 끊긴다.
  그리고 SDK 를 고치면 번들 plugin 전부의 산출물이 바뀐다.
- **C: trace_id 로 인과를 잇는다 — 받은 trace 로 발화하면 그 hop + 1** — 정확하다. 안 고른
  이유는 새 trace 로 반응하면(바로 `publish_fresh`) 빠져나간다는 것이다. 막으려던 루프가 그
  모양이다.
- **D: 응답을 기다리는 dispatch 가 있는 동안의 publish 를 거절한다** — 더 강하다. 안 고른
  이유는 콜백 안의 정당한 재발화(카탈로그가 전제하는 사용법)를 전부 깬다는 것이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- SDK runtime 의 `event.dispatch` 갈래가 `on_event` 를 부르기 **전에** 응답하게 바뀐다(예:
  콜백을 별도 작업으로 넘기고 바로 `Null` 을 돌려준다). 그러면 SDK plugin 의 재발화가 전부
  이 하한을 빠져나간다.
- pump 가 한 tick 안에서 plugin 응답을 사건보다 먼저 처리하게 순서를 바꾼다. 같은 tick 에
  도착한 반응과 응답의 판정이 뒤집힌다.
- `event.dispatch` 가 fire-and-forget 이 아니게 된다(호스트가 응답을 pending 으로 기다린다).
  그러면 기록 자리가 그 pending 과 겹치므로 하나로 합친다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 먼저 응답하고 나중에 발화하는 plugin 끼리의 루프가 실제로 나는가. 재는 법: 호스트 로그의
  `publish ... rejected: ... exceeds MAX_HOP` 없이 같은 두 plugin 의 사건이 링을 채우는지
  `tasty events fetch` 로 본다.

## References

- [ADR-0321](0321-agent-domain-events-publish-only-at-the-funnel-that-already-exists.md) — 이 버스로 나가는 agent 사건
- [`docs/reference/event-catalog.md`](../reference/event-catalog.md) — `meta.hop` 행
- 코드 근거(결정이 실현된 현재 위치): `EventBus::apply_relay_floor` · `EventBus::note_dispatch_sent` ·
  `EventBus::note_dispatch_answered` · `MAX_INFLIGHT_DISPATCHES` · `PluginManager::send_event_dispatches` ·
  `PluginManager::route_plugin_event_publish` · `PluginManager::handle_plugin_response`
