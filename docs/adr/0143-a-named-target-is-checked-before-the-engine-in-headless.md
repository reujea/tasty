# ADR-0143: 헤드리스도 지목한 대상을 확인한다 — 예약 prefix 에 한정해 engine handler 앞에서

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: headless, ipc, routing, plugin-namespace, identity-principle-3, adr-0140, adr-0136
- **Group**: architecture

## Context

gui 라우터는 요청이 지목한 대상을 아무도 안 가졌을 때 그 요청을 거절한다. 예전에는
포커스된 창으로 넘겼고, 그래서 **핸들러가 그 키를 안 읽는 메서드**에서 대상을 잘못 적은
요청이 다른 창에서 진짜로 성공했다.

**헤드리스는 그 판정이 없었다.** engine 이 하나뿐이라 라우팅할 곳이 없고, pump 는
engine handler 를 바로 부른다. 그래서 요청이 지목한 id 를 아무도 보지 않는다. 실측
(2026-09-05, 같은 요청을 두 조합에):

    workspace.create {workspace_id: 999999, name: "ghost"}
    gui   error -32602  (대상 없음)
    hl    result {"id": 2, …}      ← 만들어지고 성공이 돌아온다

증상은 gui 의 폴백과 같고 원인만 다르다. 그리고 에이전트의 주 경로가 헤드리스라,
안 닫힌 쪽이 하필 에이전트 쪽이었다.

곧바로 같은 검사를 넣지 못한 이유는 **순서**다. 헤드리스는 plugin namespace forward 를
engine handler 가 `-32601` 을 준 **뒤**에 둔다. 소유 검사를 handler 앞에 두면, plugin
prefix 의 메서드가 id 를 실었을 때(`image.open {surface_id: N}`) forward 되기 전에
잘린다. forward 가 뒤에 있는 것에는 이유가 있다 — namespace 표가 plugin spawn 시점에
채워져서, 부르기 전에는 그 메서드가 plugin 것인지 알 수 없다.

## Decision

**소유 검사를 engine handler 앞에 두되, 메서드의 prefix 가 호스트 예약일 때만 적용한다.**

예약 목록은 매니페스트 검증이 `[[contributes.ipc_namespace]]` 를 거절하는 데 쓰는 바로
그것이다([ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md)).
예약된 prefix 는 **어떤 plugin 도 점유할 수 없으므로**, 그 메서드가 forward 될 일이
없다. 그래서 그 범위에서는 순서 문제가 성립하지 않는다. 예약되지 않은 prefix(번들
plugin 이 점유한 `image` · `markdown`, 그리고 plugin 이 가질 수 있는 이름들)는 검사를
건너뛰고 기존 경로를 그대로 탄다.

즉 **이 결정은 ADR-0140 이 없었으면 성립하지 않는다.** 그전에는 호스트 prefix 45 개 중
25 개가 점유 가능해서 "예약이면 forward 되지 않는다" 가 참이 아니었다.

판정 자체(무엇을 지목했는가 · 이 engine 이 가졌는가 · 어떤 메시지로 거절하는가)는 두
조합이 **같은 코드**를 쓴다. `App` 에 안 매인 순수 부분을 `core/request_target` 으로
옮겨 gui 라우터와 헤드리스 pump 가 함께 부른다 — 두 벌로 두면 한쪽만 고쳐지는 순간
같은 요청이 조합에 따라 다르게 끝난다.

거절 메시지는 창을 말하지 않는다. 헤드리스에는 창이 없으므로 두 조합에서 참인 문장이어야
한다.

## Consequences

- **얻은 것**: 같은 요청이 두 조합에서 같은 판정을 받는다. 에이전트 경로에서 대상을
  잘못 적은 요청이 조용히 실행되지 않는다.
- **잃은 것**: 헤드리스에서 지금까지 통과하던 호출 중 "없는 대상을 지목한" 것이 에러가
  된다. 그 호출이 의도적으로 무시되기를 기대했다면 깨진다 — 다만 그 기대는 gui 에서
  이미 성립하지 않는다.
- **범위의 비대칭이 남는다**: 예약되지 않은 prefix(`image` · `markdown`)의 메서드는
  헤드리스에서 여전히 검사를 안 받는다. 그쪽은 plugin 이 답할 수 있어야 해서 자를 수
  없고, plugin 이 자기 id 공간을 어떻게 보는지는 호스트가 모른다.
- **운영 비용**: 없음. 판별은 상수 목록 조회 한 번이고, 매니페스트를 읽거나 plugin 을
  띄우지 않는다.

## Alternatives Considered

- **namespace 표를 매니페스트에서 미리 채운다(eager).** 표의 출처가 실행 중인 프로세스가
  아니라 매니페스트라 가능해 보였다. **측정이 기각했다** — 깨끗한 홈에서 첫 호출로
  `plugin.list`(메타데이터 층)를 부르면 `packages: 0` 이고 plugins 디렉터리 자체가 없다.
  번들 설치는 조회 경로에서 부르지 않기로 되어 있으므로
  ([ADR-0136](0136-a-query-does-not-create-what-it-observes.md)), 매니페스트 기반 표는
  **필요한 바로 그 순간에 비어 있다.** 이 대안을 살리려면 ADR-0136 의 경계를 다시 열어야
  한다.
- **검사를 두 자리로 나눈다** — 예약 prefix 는 앞에서, 나머지는 forward 실패 뒤에서.
  뒤쪽 자리에서는 "핸들러가 못 찾음을 성공으로 돌려준 경우" 를 일반적으로 감지할 수
  없어(그게 이 결함의 형태다) 뒤쪽 절반이 아무 일도 못 한다.
- **갈림을 문서화하고 둔다** — 비용이 가장 싸다. 기각한 이유는 그 갈림이 하필 에이전트
  경로를 열어 두기 때문이다. 원칙 3 은 조합별로 다르게 적용되는 종류의 것이 아니다.
- **핸들러마다 검사한다** — `require_surface_id` 류의 확장. 순서와 안 얽히지만 새
  핸들러마다 빠뜨릴 수 있어 가드가 따로 필요하고, 이미 있는 판정을 흩는다.

## Reconsideration Triggers

- **예약되지 않은 prefix 에도 판정이 필요해지면** — plugin 이 점유한 namespace 안에서
  호스트 리소스를 지목하는 요청이 실제로 문제를 일으키면, forward 순서 자체를 다시 연다.
- **[ADR-0136](0136-a-query-does-not-create-what-it-observes.md) 의 경계가 바뀌면** —
  번들 설치가 조회 경로로 들어오면 매니페스트 기반 eager 표가 가능해지고, 위 대안 1 이
  다시 열린다.
- **헤드리스에 engine 이 둘 이상 생기면** — "engine 이 하나라 라우팅이 없다" 는 전제가
  깨지고, 그때는 gui 와 같은 주인 찾기가 필요하다.
- **예약 목록이 좁아지면** — 이 결정의 적용 범위가 그만큼 줄어든다. ADR-0140 을 되돌리는
  변경은 이 ADR 도 함께 본다.

## References

- [ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md) — 이 결정이
  기대는 불변식(예약된 prefix 는 plugin 이 못 가진다)
- [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) — 조회가 관측 대상을 만들지
  않는다. eager 표를 기각한 근거
- [focus](../design/policies/focus.md) — 포커스 독립성의 운영 규칙
- [headless-ipc-surface](../dev-guide/headless-ipc-surface.md) — 헤드리스가 무엇에 답하는가
