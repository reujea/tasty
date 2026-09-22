# ADR-0128: DPI 변환 정합은 타입 봉인이 아니라 소스 스캔 가드로 지킨다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: typed-length, dpi, guard, scale-factor, geometry, drift-guard
- **Group**: typed-length-dpi

## Context

`PhysicalPx` / `LogicalPx` 는 두 좌표계를 **섞는** 것을 컴파일 에러로 만든다. 그러나
**변환을 빠뜨리는** 것은 막지 못한다:

```rust
PhysicalPx(content_rect.min.x * ppp)   // 올바름
PhysicalPx(content_rect.min.x)         // `* ppp` 를 빠뜨려도 그대로 컴파일된다
```

`docs/concepts/typed-length.md` 는 "DPI 혼동을 타입 시스템이 막는다" 고 서술해 왔지만,
그 서술은 두 실수 중 하나에만 참이었다. 나머지 절반은 강제 수단 없이 관례로만 존재했다.

실측이 그 구멍의 크기를 보여줬다. 워크스페이스 전역에서 수동 DPI 산술이 **78건**,
그중 정당한 자리(변환 API 본체·폰트 크기·길이 타입에 의존할 수 없는 plugin SDK)를 뺀
**65건**이 타입 API 밖에서 변환하고 있었다.

두 가지 후속 사실이 결정을 갈랐다.

**(1) 탈출의 지배적 형태가 생성자를 거치지 않는다.** 65건 중 35건이 아래 형태였다:

```rust
egui::pos2(r.x.value() / scale_factor, r.y.value() / scale_factor)
```

`.value()` 로 타입을 벗겨 외부 라이브러리 타입으로 직행하므로, `PhysicalPx` 의 공개
튜플 필드를 비공개로 바꿔도 **원리적으로 잡히지 않는다.**

**(2) 스캔 패턴의 과소계수가 잘못된 결론을 만들 뻔했다.** 이 축의 최초 감사는
`grep -E '\*\s*scale_factor'` 계열로 세어 변환 탈출을 4건으로 보고했다. 그 패턴은
수신자가 낀 형태를 못 잡는다:

```rust
let px = pos.x * self.scale_factor;        // `* scale_factor` 가 아니다
let w  = px as f32 / ctx.pixels_per_point(); // 연산자와 이름 사이에 `ctx.` 가 있다
```

낱말 경계 기준으로 다시 세자 4 → 78 이 됐고, 그 차이가 숨기고 있던 자리 중 하나가
egui hover 좌표를 물리 픽셀로 바꿔 맨 `f32` 로 넘기던 커서 판정 경로였다. **감사는
그 파일을 "깨끗함" 으로 넘겼다.**

## Decision

DPI 변환의 정합을 **타입 수준 봉인이 아니라, 소스 스캔 가드 + 짝 타입 + 왕복 상쇄
테스트**로 지킨다.

- **가드**: `src/dpi_conversion_guard.rs` 가 `src`·`crates` 전역에서 수동 DPI 산술을
  찾는다. 연산자가 인접한 식별자만 세므로 변환 API 에 인자로 넘기는 정상 형태
  (`to_logical(scale_factor)`)는 걸리지 않고, 수신자 경로를 건너뛰어 위 (2) 형태를
  잡는다. 산술이 정당한 자리는 `ALLOWED` 에 **사유와 건수**로 등재한다 — 파일 단위
  면제가 아니라 건수 고정이라, 등재된 파일이 새 위반을 들이면 그것도 잡힌다.
- **짝 타입**: 사각형은 `PhysicalRect` ↔ `LogicalRect` 로 네 변을 한 번에 변환한다.
  변을 하나씩 나누면 하나를 빠뜨려도 컴파일이 통과하지만, 변환이 한 번이면 빠뜨릴
  것이 없다. `WebViewBounds` ↔ `PhysicalWebViewBounds` 도 같은 형태다.
- **왕복 상쇄 테스트**(`physical_and_logical_round_trip_cancels`): 짝 타입마다 내렸다 올린 값이 원본과 같음을 고정한다. 손으로
  나누던 시절에는 이 성질을 확인할 자리 자체가 없었다.

## Consequences

- **얻은 것**: 정책의 나머지 절반에 실제 강제 수단이 생겼다. 수동 DPI 산술 65건이
  0건이 됐고, 재발은 테스트 실패로 드러난다. 감사가 놓쳤던 자리들이 함께 닫혔다.
- **잃은 것**: 컴파일러가 아니라 별도 테스트가 판정하므로, 그 테스트가 실행되지 않는
  경로에서는 보호가 없다. 그리고 판정이 어휘 수준이라 구문 트리를 보지 않는다 — 주석과
  문자열 리터럴은 문자 단위 상태 기계로 갈라내지만, 연산자와 식별자가 서로 다른 줄로
  갈리거나 매크로가 문자열에서 코드를 만들어 내는 형태는 못 본다. 이 사각지대들은
  가드 안에 **의도된 false negative 로 테스트에 붙박아** 두었으므로, 나중에 판정을
  넓히면 그 결정이 테스트 실패로 드러난다.
- **운영 비용 / 유지 부담**: `ALLOWED` 표가 실측과 어긋나면 실패하므로, 정당한 산술을
  늘리거나 줄일 때 표를 함께 갱신해야 한다. 이는 비용이자 의도된 마찰이다 — 표를
  건드리지 않고 면제 범위가 넓어지는 것을 막는다.

이 결정은 가드가 **실행되는 것**을 전제한다. 가드는 크레이트 안의 유닛 테스트로 두어
`--lib --bins` 를 도는 잡에서 실행된다. 어느 잡이 무엇을 언제 도는지는
[`../dev-guide/ci-gates.md`](../dev-guide/ci-gates.md) 가 정본이며, 여기 복제하지
않는다 — 복제하면 정본이 바뀔 때 이 문서만 낡는다.

## Alternatives Considered

- **A: 생성자 봉인** — `PhysicalPx(pub f32)` 의 튜플 필드를 비공개로 바꾸고 raw 생성을
  `from_raw()` 같은 명시적 이름으로만 허용한다. 오탐이 0(컴파일러가 판정)이라는 장점이
  있으나, **커버리지가 65건 중 4건**이었다. 지배적 형태 35건은 생성자를 아예 거치지
  않아 원리적으로 안 잡힌다. 비용은 반대로 컸다 — 생성자 호출 400여 곳과, 산술식 인자
  대부분을 차지하는 `LogicalPx((N * self.ui_zoom).round())`(**DPI 가 아니라 UI 줌**)를
  생산하는 코드 생성 템플릿까지 바꿔야 한다. 잡는 결함보다 만드는 소음이 훨씬 크다.
  이식을 끝낸 뒤 재측정한 결과 **DPI 변환이 생성자를 거치는 경우가 0건**이 되어, 봉인은
  막을 대상 자체가 사라졌다.
- **B: 두 newtype 만으로 충분하다(현상 유지)** — 타입이 이미 DPI 를 지킨다는 입장.
  실측이 이를 부정한다: 타입이 지키는 것은 혼합뿐이고, 위 Context 의 65건이 타입이
  존재하는 상태에서 그대로 통과하고 있었다. 문서가 주장하는 보호와 코드의 실제 보호가
  다르다는 것이 이 축의 결함이었으므로, 현상 유지는 그 결함을 그대로 두는 선택이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **DPI 변환이 다시 생성자를 거치는 형태가 지배적이 되면.** 봉인을 기각한 근거가
  "생성자를 거치는 변환이 0건" 이라는 실측이므로, 그 전제가 무너지면 A 를 다시 본다.
  관측 수단은 가드의 `ALLOWED` 추이와, 생성자 호출 중 산술식 인자를 DPI / UI 줌 /
  좌표계 내부 산술로 가르는 재분류다.
- **가드의 자동 실행 채널이 사라지면.** 이 결정은 타입 봉인을 가드로 대체한 것이므로,
  가드가 사람이 기억할 때만 도는 상태가 되면 대체가 성립하지 않는다. 그때는 채널을
  복구하거나, 봉인을 포함한 다른 강제 수단을 다시 저울질한다.
- 스캔이 구문 분석 없이는 가를 수 없는 오탐/누락을 반복해서 내면 — 강제 수단을
  린트(예: clippy custom lint)나 타입 수준으로 올리는 쪽을 재검토한다.

## References

- [`../concepts/typed-length.md`](../concepts/typed-length.md) — 두 강제 수단의 분담을 기술
- `src/dpi_conversion_guard.rs` — 가드 본체. 왜 `tests/` 가 아니라 크레이트 안에 있는지도 모듈 doc 에 있다
- `crates/tasty-type-geometry/src/rect.rs` — `PhysicalRect` / `LogicalRect` 짝과 왕복 상쇄 테스트
- `src/host_api/webview.rs` — 같은 형태의 선례(`WebViewBounds` / `PhysicalWebViewBounds`)
- [`../dev-guide/ci-gates.md`](../dev-guide/ci-gates.md) — 이 결정이 의존하는 실행 채널의 정본
- 중복 조항 통합: [0509](0509-length-constructor-sealing-clause-consolidated.md) (튜플 생성자 봉인 조항 — 이 ADR 의 다른 조항은 유효)
