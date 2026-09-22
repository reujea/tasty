# 타입 있는 길이 (Typed length)

tasty 내부 소스는 길이 값을 **`f32` 그대로 다루지 않는다.** DPI scale factor 가 개입하는 코드에서 *물리 픽셀* 과 *논리 픽셀* 을 헷갈리면 런타임에야 드러나는 위치/크기 버그가 난다. 그래서 길이는 두 newtype 으로 나뉜다.

**무엇을 무엇이 막는지 구분한다 — 이 정책은 강제 수단이 둘이다.**

| 실수 | 예 | 막는 것 |
|---|---|---|
| 두 좌표계를 **섞는다** | `PhysicalPx + LogicalPx` | **컴파일러** — 타입 에러다 |
| 변환을 **빠뜨린다** | `PhysicalPx(x * ppp)` 에서 `* ppp` 누락 | **가드** — `src/dpi_conversion_guard.rs` 의 소스 스캔 |

두 번째는 타입이 못 막는다. `PhysicalPx(pub f32)` 는 튜플 필드가 공개돼 있어 **단언이지 검증이 아니고**, 애초에 산술이 타입 밖에서 끝난 뒤 결과만 감싸는 형태(`r.x.value() / scale_factor` → egui `f32`)는 생성자를 거치지도 않는다. 그래서 수동 DPI 산술을 찾아내는 가드가 따로 있다.

- 정의: `crates/tasty-type-geometry/src/length.rs`
- 강제 정책(필수): [`../../CLAUDE.md`](../../CLAUDE.md) "길이 타입" 섹션 — 새 길이 값은 반드시 둘 중 하나.

## 두 타입

| 타입 | 의미 | 쓰는 곳 |
|------|------|---------|
| `PhysicalPx(pub f32)` | 실제 디바이스 픽셀 (scale factor 적용 후) | GPU/wgpu, winit 마우스 좌표, `Rect` 필드, GPU 뷰포트·시저 |
| `LogicalPx(pub f32)` | DPI 독립 논리 픽셀 | egui UI, `Theme` 상수, 사이드바 너비, 모든 egui 좌표/크기 |

둘 다 `#[repr(transparent)]` 이라 런타임 오버헤드가 없다 (제로 코스트). `Add`/`Sub`/`Mul<f32>`/`Div<f32>`/`Neg`/`*Assign` 과 `max`/`min`/`floor`/`abs` 가 **같은 타입끼리만** 정의돼 있어, `PhysicalPx + LogicalPx` 같은 식은 타입 에러다.

### `const` 문맥에서는 트레이트 연산을 못 쓴다

위 연산자는 전부 트레이트 impl 이고 트레이트 impl 은 `const` 가 아니다. 그래서 상수 초기화식에서
부르면 컴파일이 막힌다:

```rust
const BODY_H: LogicalPx = FRAME_H - HEADER_H;   // error[E0015]: cannot call non-const operator
```

`LogicalPx(FRAME_H.0 - HEADER_H.0)` 으로 필드를 벗기면 컴파일은 되지만 그 자리에서 타입이
사라진다 — 위 표의 "단언이지 검증이 아니다" 로 되돌아가는 형태고, DPI 가드는 이걸 잡지 않는다
(그 가드가 겨냥하는 것은 scale factor 산술이다). 그래서 벗기지 않는 통로를 인허런트
`const fn` 으로 둔다. 두 타입 모두에 있다:

```rust
const BODY_H: LogicalPx = FRAME_H.minus(HEADER_H);
const LIST_MIN: LogicalPx = ITEM_HEIGHT.scaled(4.0);
const INDENT: LogicalPx = LABEL_COL_WIDTH.plus(LogicalPx(12.0));
```

`plus`/`minus`/`scaled` 는 대응 트레이트 연산과 결과가 같다 — 이름이 다른 것은 인허런트 메서드가
같은 이름의 트레이트 메서드를 조용히 가리는 것을 피하기 위해서다. 계수가 좌변인 형태
(`4.0 * LEN`)는 `Mul<f32>` 도 `scaled` 도 지원하지 않으므로 `LEN.scaled(4.0)` 으로 쓴다 —
곱셈 교환이라 값이 보존된다.

## 변환 — scale factor 를 명시적으로 통과

두 타입 간 직접 대입은 불가능하다. 반드시 변환 함수를 거치고, 그때 scale factor 를 넘긴다:

```rust
let physical: PhysicalPx = logical.to_physical(scale_factor); // 논리 → 물리 (× sf)
let logical:  LogicalPx  = physical.to_logical(scale_factor); // 물리 → 논리 (÷ sf)
```

변환에 scale factor 가 *강제 인자* 라는 점이 핵심이다 — "어느 좌표계인지" 를 매번 의식하게 만든다.

사각형은 네 변이 함께 넘어가므로 짝 타입으로 한 번에 변환한다. 변을 하나씩 나누면 **하나를 빠뜨려도 컴파일이 통과**한다:

```rust
let logical: LogicalRect = physical_rect.to_logical(scale_factor);
let physical: PhysicalRect = logical.to_physical(scale_factor);
```

`src/host_api/webview.rs` 의 `WebViewBounds` / `PhysicalWebViewBounds` 도 같은 형태다 — 플랫폼 창 API 로 나가는 사각형이라 타입이 다를 뿐, 왕복이 상쇄된다는 것을 테스트로 고정하는 구조가 같다.

## 외부 API 경계에서만 `.value()`

egui·wgpu 등 외부 라이브러리는 `f32` 를 받는다. 그 **경계에서만** `.value()` 로 raw `f32` 를 꺼낸다:

```rust
egui::FontId::proportional(th.font_size_body.value());
```

내부 로직 중간에서 `.value()` 로 빠져나와 `f32` 산술을 하는 것은 안티패턴 — 타입 보호를 스스로 버리는 셈이다.

**특히 `.value()` 로 벗긴 뒤 scale factor 를 곱하거나 나누는 것**은 위 표의 두 번째 실수 그 자체다. 그 형태는 `src/dpi_conversion_guard.rs` 가 잡는다. 산술이 정당한 자리(변환 API 본체, 길이 타입에 의존할 수 없는 plugin SDK 등)는 그 가드의 `ALLOWED` 에 **사유와 함께** 등재한다 — 파일 단위가 아니라 건수까지 고정하므로, 등재된 파일이 새 위반을 들이면 그것도 잡힌다.

## 집행은 셋으로 나뉜다

| 무엇을 막는가 | 누가 막는가 |
|---|---|
| 두 좌표계를 섞는 것 (`PhysicalPx + LogicalPx`) | 컴파일러 |
| 변환을 빠뜨리는 것 (`.value()` 뒤의 수동 scale factor 산술) | `src/dpi_conversion_guard.rs` |
| 애초에 타입을 안 쓰고 선언하는 것 (`const W: f32 = 96.0;`) | `src/source_guards/length_constant_frontier.rs` |

셋째가 따로 필요한 이유는 앞의 둘이 **이미 타입이 붙은 값**에만 걸리기 때문이다. 처음부터 `f32` 인 길이는 섞일 두 타입도 벗길 `.value()` 도 없어 둘 다 조용히 통과한다.

셋째 가드의 술어는 두 겹이다. **어디를 보는가**는 스캔 목록이 정하고(지금은 `src` 와 `crates/tasty-gallery`), 그 안에서 **"전환 전선 밖에 없다"** 를 요구한다. 본체 전환이 진행 중이라 아직 남은 영역이 있고, 그 영역은 면제 목록이 아니라 **경로 한 줄과 건수**로 이름 지어져 있다 — 건수가 상한이라 전선은 줄어들 수만 있다. 갤러리는 전환을 끝내 전선 없이 0 을 요구받는다. **목록에 없는 크레이트는 0 이 아니라 미측정이다** — 목록이 조용히 비는 것(접두사 오타 등)은 별도 테스트가 막는다. 그 가드가 설계상 못 잡는 형태(테스트 코드, 배율성 이름, 0~1 값, `static`·`let`)는 그 파일의 모듈 주석이 열거하고, 각 형태를 겨냥한 테스트가 사각의 모양을 못박는다.

## `f32` 로 남는 값

길이가 아닌 값은 그대로 `f32` 다 — 비율(ratio/opacity/scale_factor), 색 채널, 외부 API 로 넘길 직전 추출값.

## 새 코드 작성 시

1. 길이를 나타내는 맨 `f32` 필드/변수를 만들지 않는다.
2. **scale factor 에 따라 값이 달라지면 `PhysicalPx`, 아니면 `LogicalPx`.**
3. 외부 API 경계에서만 `.value()` 로 `f32` 추출.

## 관련

- [`../adr/0509-length-constructor-sealing-clause-consolidated.md`](../adr/0509-length-constructor-sealing-clause-consolidated.md) — 왜 생성자를 봉인하지 않는지, 기각한 대안과 재검토 조건
- [`../adr/0128-dpi-conversion-guarded-by-source-scan-not-sealed-types.md`](../adr/0128-dpi-conversion-guarded-by-source-scan-not-sealed-types.md) — 봉인 대신 쓰는 집행 수단 셋(스캔 가드 · 짝 타입 · 왕복 상쇄 시험)
- `src/dpi_conversion_guard.rs` — 수동 DPI 산술을 잡는 가드. 왜 `tests/` 가 아니라 크레이트 안에 있는지도 그 모듈 doc 에 적혀 있다
- [`../design/systems/theme.md`](../design/systems/theme.md) — Theme 상수는 모두 `LogicalPx`
- 색은 길이와 같은 newtype 정책을 따른다 (`GpuRgba` 등) — theme.md "색 생성 경로 단일화"
