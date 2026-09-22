# ADR-0176: 모션 지속시간은 `Millis` 로 `Theme` 경계를 건넌다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: design-tokens, motion, typed-values, theme, code-generation
- **Group**: ui-theme-gallery

## Context

디자인 토큰의 `duration` 은 생성 파이프라인에 처음부터 들어와 있었다 — 생성기가
`$type: "duration"` 을 파싱해 세 tier 의 raw const 21 개를 낸다. 빠져 있던 것은
`&Theme` 접근자였고, 그 사유가 코드에 적혀 있었다: "테마 불변이라 raw const 로 이미
충분(zoom 무관)".

그 사유는 테마 축에서는 맞지만 **소비 축에서는 틀렸다.** 이 레포는 디자인 값의 소비를
`&Theme` 하나로 강제한다(위젯이 `generated::` const 를 직접 읽는 것을 금지). 그래서
접근자가 없으면 토큰이 있어도 아무도 못 쓴다. 실측 결과 **생성 duration const 21 개
전부 소비자 0** 이었고, 실제로 쓰이는 모션 값은 전부 손으로 쓴 중복이었다.

중복 자체보다 나쁜 것은 **단위**였다. 손으로 쓴 값들은 단위가 이름에만 있거나
(`SPIN_PERIOD: f64 = 0.9` — 초) 아예 없었다. 이 레포에서 모션 값을 세려고 만든
스캐너가 그 `0.9` 를 **0.9ms 로 환산했다**(실제 900ms). 값을 찾는 도구가 그 값에서
1000 배 틀렸다.

시간 값은 이 레포에서 세 형태로 흐른다 — 밀리초 `f32`(토큰), 초 `f32`/`f64`(egui
`animate_bool_with_time`), `Duration`(타이머). 셋 다 맨 실수라
섞여도 컴파일이 통과한다. `PhysicalPx`/`LogicalPx` 가 존재하는 이유와 같은 상황이다.

## Decision

component tier 의 `duration` 토큰에 `&Theme` 접근자를 생성하고, 그 반환 타입을
새 `Millis` 신형으로 한다. **이름이 아니라 타입이 단위를 진다.**

`Millis` 의 범위는 **`Theme` 경계 하나**다. egui·winit·`Duration` 이 맨 실수를 받는
가장자리는 그대로 두고 `to_secs_f32` / `to_secs_f64` / `to_duration` 이 경계에서
변환한다. 시간 값 전면 도입이 아니다.

무단위 접근자(`value()`)를 두지 않는다. 원시 밀리초가 필요하면
`to_millis_f32()` 로 나가는데, **이름이 단위를 지고 있어** 초로 오인될 수 없고 텍스트로
셀 수 있다.

생성 접근자는 치수와 두 가지가 다르다. **`ui_zoom` 을 곱하지 않는다**(배율은 길이
축이다). 그리고 **semantic 종착에도 `Theme` 필드가 없다** — duration 은 테마마다
달라지지 않아 굽지 않는다. 그래서 본문 형태가 체인/리터럴 둘뿐이다.

## Consequences

- **얻은 것**: 손으로 쓴 모션 값 6 개가 사라지고 토큰이 정본이 됐다. 그중 둘
  (`SPIN_PERIOD` · `HOVER_DELAY_SECONDS`)은 **초 단위 위젯 로컬 상수**였고, 단위 착오의
  진원지였다.
- **얻은 것**: 흔한 오용 형태가 컴파일 에러가 된다 — `theme.spinner_duration() / 1000.0`
  은 `Millis / f32` 라 타입이 없다. 길이 축과 달리 **두 번째 가드가 필요 없다**(아래).
- **잃은 것**: 순수 함수 경계에서 벗기는 자리가 생긴다. `hold_reveal_alpha` 처럼 단위
  없는 ms 산술을 하는 함수는 그대로 두고 호출부가 `to_millis_f32()` 로 넘긴다 —
  현재 그런 자리 5 곳.
- **운영 비용 / 유지 부담**: 대응 토큰이 없는 값은 여전히 손으로 남는다. 현재 넷
  (toast fade-in 80 · fade-out 160 · 모달 흔들기 300 · modifier-hint Shift 지연 1200).
  이것들은 코드가 값을 발명한 자리이고, 디자인 쪽에 토큰을 요청할 목록이다.

### 왜 길이 축의 두 번째 가드에 해당하는 것이 없는가

길이 축은 강제 수단이 둘이다 — 컴파일러가 혼합을 막고,
`src/dpi_conversion_guard.rs` 가 `.value()` 로 벗긴 뒤의 수동 산술을 막는다. 둘째가
필요했던 이유는 egui·wgpu 가 맨 `f32` 를 요구하는 자리가 수백 개라 **벗기는 것이
일상**이고, 벗긴 값이 어느 좌표계인지 타입이 사라진 뒤에는 알 수 없기 때문이다.

시간 축은 경계가 좁다. 필요한 변환이 초와 `Duration` 둘뿐이고 그 둘을 타입이
제공하므로, 벗길 일상적 이유가 없다. 남는 탈출구는 `to_millis_f32()` 하나이고 이름이
단위를 진다. 그래서 여기서는 **두 번째 판정을 만들지 않는다** — 필요해지는 조건은
아래 재검토 트리거에 적었다.

## Alternatives Considered

- **A: 맨 `f32` 로 두고 이름에 `_ms` 를 붙인다** — 지금까지의 방식이고, 실제로 실패했다.
  이름은 도구가 못 읽고 컴파일러도 안 본다.
- **B: `std::time::Duration` 을 그대로 쓴다** — 정수 나노초라 디자인 값(밀리초 실수)을
  왕복시키면 반올림이 끼고, `const` 문맥에서 쓰기 불편하며, 생성기가 내는 리터럴이
  읽히지 않는다(`Duration::from_nanos(900_000_000)`). 경계에서 `to_duration()` 으로
  나가는 것으로 충분하다.
- **C: 시간 전면에 신형을 도입한다**(폴링 간격·타임아웃·프레임 간격까지) — 그 값들은
  디자인 토큰 축이 아니고 이미 `Duration` 이라 단위가 있다. 넓히면 얻는 것 없이 diff 만
  커진다.
- **D: 접근자 없이 raw const 를 위젯이 직접 읽게 한다** — `&Theme` 경유 강제 원칙을
  깬다. 그 원칙이 있는 이유(배율·테마 재굽기)가 시간에는 안 걸리지만, 예외를 하나
  만들면 소비 규칙이 값 종류마다 갈린다.

## Reconsideration Triggers

- `to_millis_f32()` 를 쓰는 자리가 늘어난다 — 벗기는 것이 일상이 되면 길이 축과 같은
  상황이고, 그때가 두 번째 가드를 만들 자리다. 세는 명령은
  [`docs/design/systems/design-token-mapping.md`](../design/systems/design-token-mapping.md) 에 있다.
- semantic tier duration 에도 접근자가 필요해진다 — 지금은 component tier 만 낸다.
- 시간 값에 테마별 분기가 생긴다 — 그러면 `Theme` 필드로 구워야 하고 본문 형태가
  하나 는다.
- `Millis` 를 요구하는 자리가 `Theme` 경계 밖으로 번진다 — 범위 결정을 다시 봐야 한다.

## References

- [ADR-0174](0174-theme-carries-reduced-motion.md) — 모션 감소를 `Theme` 이 실어
  나른다. 이 ADR 과 같은 경계를 쓴다.
- [`docs/concepts/typed-length.md`](../concepts/typed-length.md) — 길이 축의 선례와
  "강제 수단이 둘" 의 근거.
- [`docs/design/systems/design-token-mapping.md`](../design/systems/design-token-mapping.md)
