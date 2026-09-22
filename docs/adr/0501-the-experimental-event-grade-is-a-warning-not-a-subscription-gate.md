# ADR-0501: 사건의 Experimental 등급은 경고이지 구독 게이트가 아니다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: events, event-bus, plugin, manifest, stability, compatibility, adr-0321

## Context

사건 카탈로그(`docs/reference/event-catalog.md`)는 첫 판부터 Experimental 등급을 "minor 마다
변경 가능, 매니페스트 `experimental_events = true` 필요" 로 정의했다. 그 뒤 ADR-0321 이
`agent.task_finished` · `agent.barrier_closed` 를 Experimental 로 내면서 같은 문장이 카탈로그의
Agent 절과 `CHANGELOG.md` 의 `[Unreleased]` 항목에 한 번씩 더 복제됐다.

그 게이트를 재는 좌변은 코드에 없다(2026-09-23, `rg -n 'experimental_events'` 로 셌을 때 세
자리 전부가 산문이었다).

- 매니페스트 스키마(`tasty-plugin-manifest` 의 `Manifest`)에 그 키가 없다. `Manifest` 에
  `deny_unknown_fields` 도 없어서, 문서대로 키를 적은 매니페스트는 **오류 없이 무시된다.**
- 호스트가 발화하는 사건의 등급은 값으로 존재하지 않는다. `EventStability` 는 plugin 이 **자기가
  발화할** 사건을 선언하는 자리에만 쓰인다.
- 구독 판정 두 자리 — `EventBus::subscribe_plugin` 의 등록 판정과 fan-out 의 발송 판정 — 는
  매니페스트 `event_subscribe` 패턴이 덮는가와 패턴이 키에 맞는가만 본다.

그래서 Experimental 키(`agent.*` 둘 · `ime.composition_*` · `hook.fired`)는 첫 판부터 지금까지
등급과 무관하게 구독돼 왔다. 문서와 코드 중 한쪽을 다른 쪽에 맞춰야 한다.

## Decision

**코드를 문서에 맞추지 않고 문서에서 게이트 약속을 거둔다.** Experimental 등급은 "minor 에서 키·
payload 가 바뀔 수 있다" 는 **경고**이고, 구독 조건은 등급과 무관하게 매니페스트 `event_subscribe`
패턴이 요청 패턴을 덮는가 하나다. 카탈로그의 등급 정의와 Agent 절, `CHANGELOG.md` 의
`[Unreleased]` 항목을 그 뜻으로 고친다. CHANGELOG 항목은 아직 어떤 태그에도 안 들어갔으므로
(`git tag --contains` 가 빈 답) 새 항목을 더하지 않고 그 문장을 고친다.

## Consequences

- **얻은 것**: 지금 Experimental 키를 받는 plugin 은 계속 받는다 — 외부 동작 변화가 없다.
  문서가 적은 구독 조건과 코드의 판정이 같아진다.
- **얻은 것**: 호스트 사건 등급의 두 번째 사본(코드 표)을 만들지 않는다. 등급의 정본은 카탈로그
  한 곳이다.
- **잃은 것**: plugin 작성자가 Experimental 키에 의존하겠다고 명시적으로 동의하는 자리가 없다.
  등급이 바뀌어 깨지는 것을 막는 것은 카탈로그를 읽는 것뿐이다.
- **운영 비용**: 이미 매니페스트에 `experimental_events = true` 를 적은 외부 plugin 은 그 키가
  계속 조용히 무시된다. 동작은 달라지지 않는다(원래도 무시됐다).

## Alternatives Considered

- **게이트를 배선한다** — `Manifest` 에 `experimental_events` 필드를 두고, 호스트 사건 키 →
  등급 표를 코드에 만들어 발송 시점에 거른다. 문장은 참이 되지만, 지금 그 키 없이 `agent.*` ·
  `ime.composition_*` · `hook.fired` 를 받는 plugin 이 **받지 못하게 되는** 동작 변경이고, 등급표가
  카탈로그와 두 벌이 되어 대조 판정기가 따라온다. 호환을 더 많이 보존하는 쪽을 골랐다.

## Reconsideration Triggers

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- Experimental 키의 변경이 그것에 의존하던 외부 plugin 을 깨뜨렸다는 보고가 나온다 — 그때는
  명시적 동의 자리(게이트)의 값이 비용을 넘는다. 재는 법: 이슈·사용자 보고. 외부 plugin 의
  구독 목록은 이 레포가 읽을 수 없다.

## References

- [`docs/reference/event-catalog.md`](../reference/event-catalog.md) — 등급 정의와 Agent 절
- [ADR-0321](0321-agent-domain-events-publish-only-at-the-funnel-that-already-exists.md) — `agent.*` 를 Experimental 로 낸 결정(게이트를 전제하지 않았다)
