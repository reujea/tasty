# ADR-0193: 폰트 resolve 는 글리프 유무만 해결한다 — RTL 어순은 범위 밖

- **Status**: Accepted
- **Date**: 2026-09-07
- **Tags**: i18n, font, rtl, egui, scope-boundary, adr-0139, adr-0114
- **Group**: i18n

## Context

언어팩 `[font]` resolve 는 팩이 선언한 폰트를 UI 폴백 체인 맨 뒤에 붙여, 내장 체인(라틴 + 시스템 CJK)이 커버하지 못하는 스크립트가 □(tofu)로 뜨던 것을 글리프 있는 렌더로 바꾼다. 이때 스크립트 둘 — 아랍·히브리 — 은 **오른쪽에서 왼쪽으로(RTL)** 쓰이고, 올바로 보이려면 글리프뿐 아니라 **양방향 재배열(BiDi, UAX #9)** 과 **문맥 결합(cursive shaping/joining)** 이 필요하다.

셸 UI 는 egui/epaint 로 그린다. epaint 의 텍스트 레이아웃은 런을 **논리 순서 그대로 왼쪽→오른쪽**으로 배치하고, BiDi 재배열도 아랍 결합도 하지 않는다. 그래서 팩이 아랍 글리프를 가진 폰트를 선언해도, 글자는 뜨지만 **어순이 뒤집힌 채**(아랍은 결합 없이 낱자로) 보인다. 물음은 하나다 — 폰트 resolve 가 RTL 표시까지 책임져야 하나.

## Decision

**아니다. `[font]` resolve 의 계약은 글리프 커버리지 하나다** — 팩 스크립트의 글리프를 가진 폰트를 공급해 □ 를 없애는 것까지다. 시각적 어순(BiDi)과 문맥 결합은 명시적으로 범위 밖이고, 폰트 resolve 가 아니라 **텍스트 레이아웃 엔진(egui)** 의 몫이다. tasty 는 스크립트별로 문자열을 재배열·결합·특수분기하지 않는다. RTL 언어의 팩도 `[font]` 를 선언해 글리프 커버리지를 받을 수 있으나, 그 텍스트는 레이아웃 엔진이 BiDi 를 갖추기 전까지 논리 순서(LTR 시각)로 렌더된다.

경계를 정직하게 그은 것이다 — "글리프는 붙는다, 어순은 아니다" 는 절반만 작동하는 RTL 을 조용히 내보내는 것보다 낫다. resolve 는 스크립트를 모르는 단일 메커니즘(체인 뒤에 append)으로 남고, 그 성질이 이 결정의 값이다.

## Consequences

- **얻은 것**: 폰트 resolve 가 스크립트 불가지의 단일 경로로 유지된다 — BiDi·shaping 코드도, 스크립트별 분기도 없다. 팩 제작자는 글리프가 뜨는 것을 보고, 어순 한계는 문서로 안다(절반 작동을 오해하지 않는다). 경계가 문서에 박혀 있어 "왜 아랍이 뒤집히나" 가 결함이 아니라 알려진 범위로 읽힌다.
- **잃은 것**: 아랍·히브리 팩은 어순이 뒤집혀 렌더된다 — 그 언어들은 레이아웃 엔진이 BiDi 를 갖추기 전까지 1급이 아니다. tasty 는 RTL 로케일 지원을 표방할 수 없다.
- **운영 비용 / 유지 부담**: `[font]` 를 설명하는 모든 자리에 이 한계를 함께 적어야 한다(현재 `docs/dev-guide/i18n.md` 의 "`[font]` resolve 와 UI 폴백 체인", `docs/features/language-packs/index.md` 의 비-목표). 레이아웃 엔진이 BiDi 를 얻으면 이 ADR 을 재검토한다.

## Alternatives Considered

- **A — tasty 가 egui 에 넘기기 전에 문자열을 재배열(UAX #9 BiDi 를 문자열 경계에서 구현)**: 범위가 과대하고 레이아웃 엔진이 소유해야 할 일을 중복 구현한다. `t()` 결과가 모든 호출처로 흐르므로 경계가 사방에 흩어지고, 아랍 결합(shaping)은 egui 가 배선하지 않는 별도 shaping 엔진(harfbuzz 류)을 또 요구한다 — 폰트 resolve 티켓에 비해 층이 틀리고 규모가 안 맞는다. 기각.
- **B — UI 텍스트 스택을 BiDi 지원 엔진으로 교체(예: UI 전반을 cosmic-text 로)**: cosmic-text 는 이미 터미널 그리드(`tasty-font`)에 쓰이지만 셸 UI 는 egui/epaint 다. epaint 의 텍스트 레이아웃을 갈아끼우는 것은 UI 렌더 경로의 재작성이라, 폰트 resolve 한 건과 규모가 안 맞는 훨씬 큰 별개 결정이다. 여기서는 채택하지 않는다(미래 트랙으로 닫지는 않는다).
- **C — RTL 팩을 로드에서 거부**: 제작자를 벌하고, 실제로 작동하는 글리프 커버리지를 숨긴다(히브리 독자는 아무것도 없는 것보다 읽히되 뒤집힌 쪽을 택할 수 있다). 정직한 경계는 "글리프 예, 어순 아니오" 지 "아니오" 가 아니다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- egui/epaint 가 양방향 텍스트 레이아웃(UAX #9)과 문맥 결합을 얻거나, tasty 가 그것을 하는 UI 텍스트 스택을 채택한다(대안 B).
- RTL 로케일을 1급으로 출하해야 하는 구체 요구가 생긴다.

## References

- [dev-guide/i18n](../dev-guide/i18n.md) — "`[font]` resolve 와 UI 폴백 체인"
- [features/language-packs](../features/language-packs/index.md) — 비-목표(RTL 어순)
- [ADR-0114](0114-language-pack-directory-shape-and-english-fallback.md) — 언어팩 형상·`[font]` 계약의 출처
- [ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md) — 절대값을 박지 않고 성질로 적는 규칙
