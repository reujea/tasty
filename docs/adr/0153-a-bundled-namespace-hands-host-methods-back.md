# ADR-0153: 번들 plugin 이 점유한 namespace 아래의 host 메서드는 그 plugin 이 되돌려 준다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: plugin, ipc, namespace, routing, guards, identity-principle-2, adr-0140, adr-0143
- **Group**: plugin-system

## Context

번들 plugin 은 매니페스트의 `[[contributes.ipc_namespace]]` 로 최상위 prefix 를 점유한다.
그 prefix 아래 이름은 host 가 모르는 것으로 취급되어 plugin 으로 forward 된다. 그런데
**host 가 같은 이름을 구현하고 있는 경우**가 실제로 있다 — surface 를 어느 창이 열고
있는지 같은 것은 host 만 안다.

[ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md) 은 host
prefix 를 예약하되 **번들 plugin 이 이미 점유한 둘(`image` · `markdown`)은 예외**로 뒀다.
예약하면 그 매니페스트가 거절돼 plugin 이 뜨지 못하기 때문이다. 이 ADR 은 그 예외가
남긴 자리를 다룬다.

### 실행 census 로 관측한 것

메서드 이름 361 건(등재 `METHOD_TABLE` 276 + `DEBUG_METHODS` 50 ∪ 핸들러 트리에서 뽑은
메서드 모양 리터럴)을 **세 세계에 실제로 호출**해 응답 코드를 받았다. Linux, debug 빌드,
2026-09-05 실측.

| 세계 | 무엇 |
|------|------|
| H | headless (`--no-default-features`) |
| A | gui, 번들 plugin 없음 |
| B | gui, 번들 plugin 설치·기동 |

`markdown.navigate` 의 응답이 세계마다 달랐다.

| 세계 | 응답 |
|------|------|
| A | `-32602 invalid params: missing field 'surface_id'` — **host arm 이 답했다** |
| B | `-32601 method 'markdown.navigate' not found` — **plugin 이 답했다** |
| H | `-32601` — host arm 자체가 `gui` 게이트 뒤에 있다 |

즉 host 의 구현(`src/adapters/ipc/handler.rs` 의 `markdown.navigate` arm)은 **plugin 이
설치돼 있는 동안에만 외부에서 안 닿는다.** plugin 을 빼면 같은 호출이 그 arm 에 닿는다.
어느 쪽도 오류를 내지 않으므로 이 차이는 조용하다.

같은 형태인 `image.open` · `image.list` 는 어느 세계에서도 host arm 에 닿았다. image
plugin 이 그 둘을 **self-call trampoline** 으로 host 에 되돌려 주기 때문이다(그 소스의
주석이 "Surface conversion + host enumeration stay host-owned" 라고 적어 뒀다).

그래서 관례가 둘이었던 것이 아니라 **하나였고 이탈이 하나였다.** markdown plugin 도 같은
관례를 이미 쓰고 있다 — `markdown.recent` 를 host 의 `recent.query` 로 trampoline 하고,
그 주석이 "image.open/list 와 동형 host-adapter" 라고 스스로 밝힌다. `markdown.navigate`
한 건만 빠져 있었다.

### 왜 이것이 위험한가

가려짐 자체는 지금 해를 끼치지 않는다 — 외부 호출자가 `-32601` 을 받을 뿐이다. 위험한
것은 **앞의 차단이 사라지는 순간 조용히 열린다**는 것이고, 여기서는 그 "차단" 이
plugin 이 설치돼 있다는 사실이다. 그것은 사용자가 언제든 바꿀 수 있는 상태다
(`plugin.disable` · `plugin.remove`). 즉 이 표면의 존재 여부가 **설치 상태에 따라
흔들린다.** [`docs/identity.md`](../identity.md) 원칙 2 가 요구하는 "에이전트 기능은
IPC + CLI 양면으로 동작한다" 는 그 상태에 의존해서는 성립하지 않는다.

## Decision

**번들 plugin 이 점유한 namespace 아래에서 host 가 구현한 메서드는, 그 plugin 의 inbound
dispatch 가 self-call trampoline 으로 host 에 되돌려 준다.**

- 적용: `markdown.navigate` 에 trampoline arm 을 추가한다. `image.open` · `image.list`
  는 이미 그렇다.
- 강제: `src/source_guards/bundled_plugin_namespace_coverage.rs` 가 매니페스트의
  `ipc_namespace` prefix 마다 `METHOD_TABLE` 의 그 prefix 아래 메서드가 전부 그 plugin 의
  `handle_ipc_method` 본문에 있는지 본다. 매니페스트는 실제 파서로 읽고, 메서드 표는
  상수를 링크해 값으로 쓴다.

### 판정은 dispatch 함수 **본문**만 본다

이 결정에서 가장 쉽게 잘못 만들 수 있는 부분이라 여기 적는다. 이탈하던 시점에도
`"markdown.navigate"` 라는 문자열은 그 plugin 소스에 **있었다** — plugin 이 host 로
*거는* `host.call("markdown.navigate", …)` 자리다. 방향이 반대인 두 자리가 같은 문자열을
쓰므로, 파일 전체에서 리터럴을 세는 판정은 이탈 상태에서도 초록이 된다. 그래서 가드는
`fn handle_ipc_method` 의 본문을 중괄호 균형으로 잘라 그 안만 본다.

변이 대조에서 이 성질이 확인됐다 — trampoline arm 만 지우면 파일에 그 리터럴이 하나
남은 채로도 가드가 빨개진다.

### host 가 그 이름을 아예 구현하지 않는 것도 정답이다

가드는 "host 구현이 있으면 plugin 이 받는다" 만 요구한다. host 가 그 prefix 아래
아무것도 구현하지 않으면 가려질 것이 없으므로 아무 요구도 하지 않는다.

## Consequences

- 같은 외부 호출이 plugin 설치 여부와 무관하게 같은 곳에 닿는다. `markdown.navigate` 는
  이제 세 세계 중 gui 두 곳에서 host arm 에 닿는다(headless 는 host arm 이 `gui` 게이트
  뒤라 별개 축이다).
- plugin 을 거쳐 가므로 응답이 한 겹 감싸진다 — `-32000 host call 'call#N' failed: …`.
  `image.open` 이 이미 그 모양이었고 이제 `markdown.navigate` 도 같다. 호출자가 보는
  코드가 바뀌므로 **동작 변경**이다.
- 번들 plugin 에 새 namespace 를 더하거나 host 가 그 prefix 아래 메서드를 새로 구현하면
  가드가 그 자리에서 막는다.
- 서드파티 plugin 은 이 가드의 대상이 아니다. 그쪽은 예약 prefix 를 못 가져가므로
  (ADR-0140) host 이름과 겹칠 수 없다. 겹칠 수 있는 것은 예외로 남긴 둘뿐이다.

## Alternatives Considered

- **host arm 을 지운다.** 가려진 구현을 없애면 문제가 사라진다. 그러나 그 arm 은 죽은
  코드가 아니다 — `src/core/attach_runtime.rs` 와 plugin 자신이 부른다. 지우면 그
  호출자들이 깨진다.
- **forward 대상에서 host 가 구현한 이름을 뺀다.** 라우터가 "이 이름은 host 가 안다" 를
  먼저 보게 하는 것. 판정 자리가 한 곳이라 매력적이지만, 그러면 plugin 이 자기 namespace
  아래 이름을 host 와 겹치게 정의할 때 **plugin 쪽이 조용히 죽는다** — 방향만 바뀐 같은
  형태의 가려짐이다. 어느 쪽이 이길지를 라우터가 정하는 대신, 그 이름을 가진 plugin 이
  명시적으로 넘기게 하는 편이 관계가 소스에 드러난다.
- **`image`/`markdown` prefix 도 예약한다.** ADR-0140 이 이미 기각한 자리다 — 예약하면
  번들 plugin 의 매니페스트가 거절돼 그 plugin 이 뜨지 못한다.
- **문서만 남긴다.** 이 저장소는 같은 형태(관례가 하나인데 이탈이 하나)를 이미 겪었고
  ([ADR-0136](0136-a-query-does-not-create-what-it-observes.md) 의 `handle_list`),
  그때의 교훈이 "적어 두는 것으로는 안 잡힌다" 였다.

## Reconsideration Triggers

- **번들 plugin 이 세 번째 namespace 를 점유하면** — 가드가 자동으로 그 prefix 를
  포함하지만, trampoline 이 그 도메인에도 맞는 답인지는 다시 본다.
- **`App` 이분이 착수되면** — headless 에서 `markdown.navigate` host arm 이 열릴 수
  있고, 그러면 세 세계가 같은 답을 내는지를 다시 잰다.
- **서드파티 plugin 에 namespace 예외를 열게 되면** — 이 가드는 번들만 본다. 예외가
  생기면 강제 자리를 매니페스트 검증 쪽으로 옮겨야 한다(ADR-0140 과 같은 판단).

## References

- 부분 개정: [0171](0171-a-host-error-code-survives-the-plugin-boundary.md) (Consequences 의 "한 겹 감싸짐" 조항 개정 — 감싸짐 자체는 그대로이고 코드가 뭉개지는 것만 고쳤다)
- [ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md) — 이
  예외를 남긴 결정
- [ADR-0143](0143-a-named-target-is-checked-before-the-engine-in-headless.md) — 같은
  라우팅 순서 차이를 다룬 직전 결정
- [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) — "관례가 하나였고
  이탈이 하나였다" 의 선례
- [plugin-development](../dev-guide/plugin-development.md) — namespace 기여 절차
