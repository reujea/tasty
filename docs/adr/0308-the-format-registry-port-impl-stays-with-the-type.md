# ADR-0308: 형식 레지스트리의 port impl 은 타입을 소유한 크레이트에 남고, 그 의존이 layer 예외다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, crates, layering, plugin-protocol, orphan-rule, file-format, guards, adr-0089
- **Group**: architecture

## Context

파일 형식 식별(detector 레지스트리 + 규칙/Lua/구조 평가기)은 본 바이너리 안에 있었지만
호스트 상태를 하나도 안 봤다. 그래서 크레이트로 내렸고, `docs/architecture/index.md` 의
**도메인-IO** 절에 넣었다. 그 절의 규칙은 "이 절 + type-\*/primitive 절만 의존 가능" 이다.

그런데 그 크레이트 안에 `tasty_plugin_protocol` 을 부르는 자리가 정확히 하나 있다 —
`FileFormatRegistry` 에 대한 `tasty_plugin_protocol::host_port::FileFormatRegistryPort`
구현이다. plugin 이 형식 레지스트리를 조회하는 port trait 은 plugin wire 계약이라
`tasty-plugin-protocol` 이 정본이고, 그 크레이트는 **plugin protocol/SDK** 절에 있다.
Rust 고아 규칙상 trait 과 타입이 둘 다 남의 것이면 impl 을 쓸 수 없으므로, 그 impl 이
설 수 있는 자리는 trait 쪽 크레이트 아니면 타입 쪽 크레이트뿐이다.

`crates/tasty-doc-guards/tests/architecture_layer_order_holds.rs` 는 절 소속으로 의존
방향을 판정하므로, 아무것도 안 하면 이 한 줄이 그 가드를 빨갛게 만든다.

## Decision

impl 을 **타입을 소유한 `tasty-file-format` 에 둔다.** 그 크레이트가 `tasty-plugin-protocol`
에 의존하는 것을 도메인-IO 절의 **문서화된 예외**로 선언하고, 근거를 `docs/architecture/index.md`
의 그 절 본문과 위 가드의 `EXCEPTIONS` 항목 양쪽에 적는다. 예외의 범위는 **trait 구현 한
개**이고, 반대 방향 호출(`tasty-plugin-protocol` → `tasty-file-format`)은 없다.

이 절의 형제 예외는 `tasty-remote` → `tasty-ipc` 하나뿐이고 그것은 [ADR-0089](0089-crate-split-follows-dependency-direction.md)
의 결정이다. 같은 절에 예외가 둘이 되는 것을 받아들인다.

## Consequences

- **얻은 것**: 형식 식별이 본 바이너리 밖으로 나가 본체·plugin host·시험이 같은 코드를
  크레이트 경계로 공유한다. impl 이 타입 옆에 있어 레지스트리 내부 표현이 바뀔 때 한 자리만
  움직인다.
- **잃은 것**: 도메인-IO 절의 "위 두 절만 의존 가능" 이 이제 예외 둘을 달고 읽힌다. 절 규칙을
  기계적으로 믿고 읽는 사람이 한 번 더 확인해야 한다.
- **운영 비용 / 유지 부담**: 예외의 근거가 **두 자리**(문서 본문 · 가드의 `EXCEPTIONS` 상수)에
  적히므로 고칠 때 둘을 함께 움직여야 한다. 가드가 그 둘의 정합을 재지는 않는다 — 가드가 읽는
  것은 `EXCEPTIONS` 쪽뿐이다.

## Alternatives Considered

- **A: port trait 을 도메인 쪽 크레이트로 옮긴다** — 그러면 impl 이 예외 없이 선다. 안 고른
  이유는 방향이 그대로 뒤집히기 때문이다: 그 trait 은 plugin 이 부르는 wire 계약이라
  `tasty-plugin-protocol` 과 plugin SDK 가 같은 정의를 봐야 하고, 옮기면 protocol 크레이트가
  도메인-IO 크레이트에 의존하게 된다. 예외 하나가 사라지는 대신 더 무거운 역방향 하나가 생긴다.
- **B: 크레이트를 둘로 쪼갠다** — 순수 식별부와 port adapter 부. 예외는 사라지지만 워크스페이스
  크레이트가 하나 더 늘고, 그 수는 네 파일(루트 `CLAUDE.md` · 두 README 의 배지와 본문 ·
  `docs/architecture/index.md`)에서 lockstep 으로 움직이는 값이다. 새 크레이트의 내용이
  `impl` 한 개인 것에 비해 대가가 크다.
- **C: 예외를 안 만들고 형식 식별을 본 바이너리에 남긴다** — 이 이동의 목적 자체를 버린다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `tasty-file-format` 안에서 `tasty_plugin_protocol` 을 부르는 자리가 `FileFormatRegistryPort`
  구현 하나를 넘어선다. 이 ADR 이 "trait 구현 한 개" 를 예외의 범위로 적었으므로, 그 수가
  늘면 예외의 근거 자체가 바뀐다. **오늘 이 좌변을 읽는 가드는 없다** —
  `architecture_layer_order_holds` 가 읽는 것은 예외가 `EXCEPTIONS` 에 **등재됐는가**뿐이고
  그 범위가 아니다. 세는 법: `grep -rn tasty_plugin_protocol crates/tasty-file-format/src | wc -l` (오늘 1).
- 같은 절의 예외가 셋 이상이 된다. 좌변은 `EXCEPTIONS` 중 도메인-IO 절 소속 항목 수다. 절의
  규칙이 예외로 더 설명되는 상태가 되면 절 경계 자체를 다시 그어야 한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- plugin 이 형식 레지스트리를 조회할 필요가 없어진다(기능 결정). 그러면 port trait 이 통째로
  사라지고 예외도 함께 사라진다. 재는 법: `tasty-plugin-protocol` 의 `host_port` 에서
  `FileFormatRegistryPort` 선언이 없어졌는지 본다.

## References

- `docs/architecture/index.md` 의 도메인-IO 절 — 예외를 이름과 이유로 적는 자리. 지금 거기
  적힌 수는 **셋**이다. **이 ADR 본문(Decision·Consequences)의 "둘" 은 결정 시점의 수이고
  갱신하지 않는다**(템플릿의 좌표 예외) — 값이 둘에서 셋이 됐다는 것은 위 재검토 조건이
  발화했다는 뜻이고, 소진한 자리는 아래 ADR-0318 이다. (결정이 실현된 현재 위치)
- `crates/tasty-doc-guards/tests/architecture_layer_order_holds.rs` 의 `EXCEPTIONS` — 가드가 읽는 자리. (결정이 실현된 현재 위치)
- `crates/tasty-file-format/src/registry.rs` 의 `impl FileFormatRegistryPort for FileFormatRegistry` — 예외의 전부. (결정이 실현된 현재 위치)
- [ADR-0089](0089-crate-split-follows-dependency-direction.md) — 같은 절의 형제 예외 `tasty-remote` → `tasty-ipc`.
- [ADR-0318](0318-bundled-handler-defaults-become-a-crate-constant.md) — 같은 뿌리(고아 규칙)의
  셋째 예외 `tasty-file-handler` → `tasty-plugin-protocol` 을 더하면서 **위 재검토 조건을
  소진한 자리**. 절 경계를 다시 그리지 않기로 한 근거와, 문턱을 셋에서 넷으로 옮기는 결정이
  거기 있다.
- [ADR-0194](0194-code-citations-name-symbols-not-line-numbers.md) — 코드 인용은 심볼 이름으로.
