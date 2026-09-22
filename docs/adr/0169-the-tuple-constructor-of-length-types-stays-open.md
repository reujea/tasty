# ADR-0169: 길이 타입의 튜플 생성자는 봉인하지 않는다 — 탈출구가 곧 같은 단언이기 때문

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: typed-length, geometry, guards, dpi, sealing, census
- **Group**: typed-length-dpi

## Context

`PhysicalPx(pub f32)` / `LogicalPx(pub f32)` 는 튜플 필드가 공개돼 있다. 그래서 생성은 **검증이 아니라 단언**이다.

```rust
PhysicalPx(content_rect.min.x * ppp)   // 올바름
PhysicalPx(content_rect.min.x)         // `* ppp` 를 빠뜨려도 그대로 컴파일된다
```

이 형태를 봉인(튜플 필드를 비공개로) 하자는 제안이 남아 있었다. 다만 제안 당시의 전제는 이미 바뀌었다 — [`src/dpi_conversion_guard.rs`](../../src/dpi_conversion_guard.rs) 가 이 축을 집행하고 있고 위반 0 을 유지한다(`PENDING_PORT` 가 비어 있다). 그러므로 봉인의 근거는 "새는 것을 막는다" 가 아니라 **"오탐 0 인 컴파일러 판정으로 바꾼다"** 뿐이다.

즉 남은 물음은 하나다: **그 교체가 무엇을 지우고 무엇을 치르는가.**

## Decision

**봉인하지 않는다.** 튜플 필드는 공개로 두고, 이 축의 집행은 `dpi_conversion_guard` 에 맡긴다.

봉인이 성립하려면 "변환을 거치지 않고는 물리 값을 만들 수 없다" 가 참이어야 하는데, 실측하면 **물리 값의 대다수는 변환의 산물이 아니라 본래 물리인 입력**(winit 마우스 좌표, GPU surface 크기)**과 상수**다. 그것들에는 탈출구가 필요하고, **탈출구는 오늘의 튜플 생성자와 똑같이 검증이 아니라 단언**이다. 봉인은 834 자리의 단언을 이름만 바꿔 다시 단언하게 만들 뿐이다.

### 실측 (2026-09-05, `main`)

**모수는 하나다 — 프로덕션 코드의 생성자 호출.** 테스트 파일과 `#[cfg(test)]` 블록은 뺐다(각각 33 · 134 자리로 따로 셌다). 아래 모든 비율의 분모는 이 834 다.

| 재는 것 | 수 |
|---|---|
| 프로덕션 생성자 호출(`PhysicalPx(` · `LogicalPx(`) | **834** |
| 그중 `src` | 304 |
| 그중 크레이트 8 개 | 530 (gallery 281 · type-appearance 148 · design-tokens 52 · ui-widgets 17 · dag-layout 11 · type-geometry 10 · model 7 · settings 4) |
| 인자가 숫자 리터럴인 것 | **524 (62%)** |
| 그중 `PhysicalPx` 인 것 | 83 (리터럴 25 · 단순변수 34 · 기타식 20 · 타입벗김 4) |
| 변환 호출(`.to_physical(` · `.to_logical(`) | **50** |
| 인자가 배율 산술인 `PhysicalPx(...)` | **1** — `tasty-type-geometry/src/length.rs:80`, **변환 API 본체 그 자체** |

마지막 줄이 판단의 핵심이다. 봉인이 막으려는 형태(`PhysicalPx(x * ppp)` 에서 `* ppp` 를 빠뜨리는 것)는 **프로덕션에 그 자리가 하나뿐이고 그 하나가 변환 함수의 몸통**이다. 나머지 82 자리의 `PhysicalPx` 는 곱할 배율이 애초에 없는 값 — 장치가 물리로 주는 좌표·크기와 상수다.

**이 수는 움직인다. 그리고 움직이는 방향이 판단의 일부다.** 같은 술어로 재면 `428ab045` 시점에 589 였다. 늘어난 245 의 상당 부분이 **길이 상수를 `f32` 에서 `LogicalPx` 로 옮기는 작업 그 자체**다 — `const LEAF_GAP: LogicalPx = LogicalPx(6.0);` 는 그대로 생성자 호출 한 자리다. 즉 **이 축이 성공할수록 봉인 비용이 커진다.**

### 봉인이 지우는 사각의 정확한 크기

`dpi_conversion_guard` 의 알려진 사각은 하나다 — **면제표에 등재된 파일 안에서, 건수를 유지한 채 산술이 자리를 옮기는 것.** 표는 `(경로, 건수, 사유)` 라 건수만 대조하기 때문이다.

그 노출면은 `ALLOWED` **4 파일 · 산술 13 자리**가 전부다(`tasty-type-geometry/src/length.rs` 2 · `src/host_api/webview.rs` 8 · `tasty-settings/src/appearance.rs` 1 · `tasty-plugin-sdk/src/egui_surface.rs` 2). 그리고 그 넷은 전부 **산술이 있는 것이 정상인 자리**다(변환 API 본체, 짝 타입 변환 초크포인트, 폰트 크기 스케일, 타입 API 를 못 쓰는 별도 프로세스 SDK).

834 자리를 바꿔 13 자리짜리 사각 하나를 지우는 거래다.

### 봉인이 못 지우는 것도 함께 적는다

봉인해도 **"변환을 빠뜨린 것" 이 아니라 "애초에 타입이 틀린 것"** 은 그대로 통과한다. 실례가 둘 있었다 — `DIVIDER_HIT_THRESHOLD` 가 `f32` 로 선언돼 있던 것(타입을 아예 안 씀, 지금은 `LogicalPx`)과 `tab_bar_height: PhysicalPx(24.0)`(물리 타입에 논리 토큰과 같은 수). 뒤쪽은 `PhysicalPx::new(24.0)` 로 써도 똑같이 통과한다. **봉인의 이득에 이 부류를 세면 안 된다.**

## Consequences

- **얻은 것**: 이 제안이 다시 올라올 때 세어야 할 것이 정해졌다 — 생성자 자리 수, 변환 자리 수, 탈출구가 필요한 자리 수, 그리고 면제표의 노출면. 판단이 "느낌상 크다" 가 아니라 네 수의 비교가 된다.
- **잃은 것**: 생성은 계속 단언이다. 면제표 4 파일 안에서 건수를 유지한 채 산술이 이동하면 가드가 못 본다.
- **운영 비용 / 유지 부담**: 없다 — 현행 유지다. 다만 이 ADR 의 수치는 **커밋마다 움직이는 종류**이므로, 재검토할 때는 인용하지 말고 다시 재라. 재는 술어는 위 표의 괄호 안에 그대로 적혀 있다.

## Alternatives Considered

- **튜플 필드를 비공개로 봉인하고 `to_physical`/`to_logical` 만 남긴다** — 위 실측대로 기각. 본래 물리인 입력과 상수에 탈출구가 필요하고, 탈출구는 같은 단언이다. 834 자리를 개명해 13 자리를 지운다.
- **봉인하되 `PhysicalPx::from_device(f32)` 같은 이름의 탈출구를 둔다** — 검증력은 오늘과 같고, 대신 "이 자리는 변환이 아니라 장치 입력이다" 가 이름으로 드러난다. 그 자체는 값이 있지만 **그 값은 봉인이 아니라 이름이 만든다** — 봉인 없이 그 이름을 도입해도 같은 것을 얻는다. 따라서 봉인의 근거가 되지 못한다.
- **면제표가 건수 대신 줄 번호를 못박는다** — 사각을 정확히 지운다. 데이터도 이미 있다: `violations()` 가 이미 `Vec<usize>` 로 줄 번호를 돌려주고 `verdict()` 가 `lines.len()` 만 보고 버린다. 표의 타입과 대조 한 줄이면 된다. **그런데 줄 번호는 무관한 편집에 흔들린다** — 등재 파일 위쪽에 주석 한 줄만 넣어도 전부 어긋나고, 그중 `src/host_api/webview.rs` 는 살아 있는 파일이다. 드문 사각을 잦은 거짓 경보와 바꾸는 거래라 기각.
- **면제표가 산술 **식 자체**를 못박는다** — 줄 번호의 취약성이 없으면서 사각을 지운다. `violations()` 가 줄 번호와 함께 정규화한 식 문자열을 돌려주게 하는 변경이 필요하다. **기각이 아니라 미착수**다 — 사각의 크기(13 자리, 전부 정상인 자리)가 아직 이 작업을 정당화하지 않는다. 아래 재검토 트리거에 걸어둔다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **탈출구 없이 봉인 가능한 형태가 생기면** — 본래 물리인 입력(winit 좌표·GPU 크기)이 전부 타입을 달고 들어오게 되어 `PhysicalPx` 를 f32 에서 만들 이유가 사라지면, 위 기각 근거의 전제가 무너진다.
- **`ALLOWED` 가 커지면** — 지금은 4 파일 13 자리라 사각이 작다. 등재 파일이 늘거나 한 파일의 건수가 자라면 "건수만 본다" 의 대가가 커지므로, 식 자체를 못박는 대안을 다시 잰다.
- **생성자 자리 수가 줄기 시작하면** — 지금은 이 축의 진행과 함께 는다. 줄기 시작한다는 것은 리터럴 상수가 토큰으로 흡수되고 있다는 뜻이고, 그때는 봉인 비용의 크기가 달라진다.

## References

- [`docs/concepts/typed-length.md`](../concepts/typed-length.md) — 두 타입과 집행 셋의 분담표
- [`src/dpi_conversion_guard.rs`](../../src/dpi_conversion_guard.rs) — 이 축의 집행 지점. `ALLOWED`/`PENDING_PORT` 와 그 성격 차이가 모듈 주석에 있다
- [`docs/ai-verification/dpi-scale-verification.md`](../ai-verification/dpi-scale-verification.md) — 배율 2 재현 절차
- [ADR-0148](0148-physical-px-constants-are-split-by-what-they-are-for.md) — 물리 상수를 용도로 가른 결정
- [ADR-0161](0161-length-constant-conversion-is-ordered-by-path-length.md) — 길이 상수 전환의 순서와 전선 가드
- 중복 조항 통합: [0509](0509-length-constructor-sealing-clause-consolidated.md) (튜플 생성자 봉인 조항 — 이 ADR 의 다른 조항은 유효)
