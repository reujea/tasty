# 타입 있는 길이 (Typed length)

길이에는 `PhysicalPx` 또는 `LogicalPx`를 사용한다. 두 타입은 좌표계가 다른 값을 섞는 실수를 막는다. 원시 `f32`를 잘못 감싸거나 변환을 생략한 것까지 컴파일러가 확인해 주지는 않는다.

정의는 `crates/tasty-type-geometry/src/length.rs`, 프로젝트 규칙은 [CLAUDE.md](../../CLAUDE.md)의 길이 타입 절에 있다.

## 두 타입

| 타입 | 단위 | 사용하는 곳 |
|---|---|---|
| `PhysicalPx(pub f32)` | 실제 장치 픽셀 | winit 마우스 좌표, GPU 크기·뷰포트·시저, PhysicalRect |
| `LogicalPx(pub f32)` | DPI와 독립된 논리 픽셀 | egui 좌표·크기, Theme 치수, UI 조작 영역 |

두 타입은 `repr(transparent)`이며 내부 값은 f32다. 같은 타입끼리 더하고 빼거나, 무차원 배율로 곱하고 나눌 수 있다. `PhysicalPx + LogicalPx`는 컴파일 오류다. `max`·`min`·`floor`·`abs`와 대입 연산도 제공한다.

타입은 값의 용도로 고른다. 조작 영역과 디자인 시스템 치수는 논리 픽셀을 쓰고, 의도적으로 1 device pixel을 유지하는 hairline은 물리 픽셀을 쓴다. 예를 들어 pane 보더는 `LogicalPx(2.0)`, surface 보더는 `PhysicalPx(1.0)`, divider 조작 영역은 논리값이다. 배율 2에서 앞의 두 보더는 각각 물리 4px·1px여야 한다. 배율 1만 확인하면 이 차이를 검사할 수 없다.

### `const` 문맥에서는 트레이트 연산을 못 쓴다

현재 타입의 Add·Sub 등 구현은 const가 아니므로 상수 초기화식의 `FRAME_H - HEADER_H`는 컴파일되지 않는다. `.0`을 꺼내 계산하는 대신 두 타입이 제공하는 const 메서드를 쓴다.

```rust
const BODY_H: LogicalPx = FRAME_H.minus(HEADER_H);
const LIST_MIN: LogicalPx = ITEM_HEIGHT.scaled(4.0);
const INDENT: LogicalPx = LABEL_COL_WIDTH.plus(LogicalPx(12.0));
```

`plus`·`minus`·`scaled`는 런타임 연산자와 결과가 같다. 이름을 다르게 두어 같은 이름의 트레이트 메서드를 가리지 않는다. 계수가 앞에 있는 `4.0 * LEN`은 `LEN.scaled(4.0)`으로 쓴다.

## 변환 — scale factor 를 명시적으로 통과

변환 메서드에 현재 DPI scale factor를 전달한다. UI 배율과 DPI 배율은 별개다. UI 배율의 적용 위치는 [테마 가이드](../design/systems/theme.md#host-ui-zoom)를 따른다.

```rust
let physical: PhysicalPx = logical.to_physical(scale_factor);
let logical: LogicalPx = physical.to_logical(scale_factor);
let rect: LogicalRect = physical_rect.to_logical(scale_factor);
```

사각형은 네 필드를 각각 변환하지 않고 `PhysicalRect`·`LogicalRect`의 변환 메서드를 사용한다. `src/host_api/webview.rs`의 `WebViewBounds`·`PhysicalWebViewBounds`도 같은 방식이다. 짝 타입의 왕복 변환은 테스트로 확인한다.

튜플 생성자는 공개되어 있다. winit 좌표나 GPU 크기처럼 이미 물리 단위로 들어온 값과 상수를 만들 때 필요하다. `PhysicalPx(x)`는 x가 물리 단위라는 개발자의 표현이며, 그 단위를 검증하는 함수가 아니다. 생성자 이름만 `from_raw`로 바꾸어도 이 한계는 같다.

## 외부 API 경계에서만 `.value()`

외부 API가 f32를 요구할 때 마지막에 값을 꺼낸다.

```rust
egui::FontId::proportional(th.font_size_body.value());
```

내부 계산에서는 길이 타입을 유지한다. `.value()`를 꺼낸 뒤 scale factor를 곱하거나 나누는 대신 변환 API를 먼저 호출한다. 수동 산술이 맞는 변환 함수 본체 등은 DPI 가드의 `ALLOWED`에 이유와 건수를 기록한다.

## 집행은 셋으로 나뉜다

| 검사 | 확인하는 것 | 확인하지 못하는 것 |
|---|---|---|
| 컴파일러 | 서로 다른 길이 타입의 혼합 | raw 값에 잘못 붙인 타입 |
| `src/dpi_conversion_guard.rs` | 인식 가능한 scale factor 이름 주변의 수동 산술 | 모든 변환 누락, 여러 줄로 분리된 식, 생성된 코드의 의미 |
| `src/source_guards/length_constant_frontier.rs` | 길이로 보이는 f32·f64 const 선언 | 모든 변수와 함수 인자의 실제 단위 |

DPI 가드는 `src`·`crates`를 검사한다. `ALLOWED`는 변환 API 본체, WebView bounds 변환, 폰트 배율 계산을 사유·건수로 관리한다. `PENDING_PORT`는 아직 변환 API로 옮기지 못한 코드용이며 현재 비어 있다. 허용 파일 안에서 같은 건수를 유지한 채 식이 바뀌면 건수 검사만으로는 구분할 수 없다. 줄 번호 고정은 무관한 편집에도 실패하므로 쓰지 않으며, 정규화된 식 비교는 아직 도입하지 않았다.

선언 가드의 검사 범위는 `src/`, `crates/tasty-gallery/`, `crates/tasty-platform/`이다. 갤러리는 잔여 0을 요구한다. 현재 예외로 남은 선언은 `src/app/modal/shake.rs`와 `crates/tasty-platform/src/window_chrome.rs`의 f64 경계 값 각 하나다. winit의 f64 좌표와 직접 계산하는 곳이며, 상세 이유와 건수는 가드의 `FRONTIERS`가 관리한다. 기존 UI 디렉터리 전체를 예외로 허용하는 규칙은 없다.

잔여 건수는 범위별 상한을 넘을 수 없다. 줄어든 것은 실패가 아니며, 모두 전환한 범위는 `FRONTIERS`에서 지운다. 검사 목록에 없는 크레이트는 아직 확인하지 않은 범위이며 위반 0으로 보고하지 않는다. 경로 오타로 검사할 파일이 없어지는 경우도 별도 테스트가 잡는다. 테스트 코드, 배율·시간처럼 보이는 이름, 0보다 크고 1보다 작은 값, static·let 선언은 이 검사의 한계로 남는다.

검사 실행 범위는 [CI 가이드](../dev-guide/ci-gates.md)를 따른다. 테스트가 실행되지 않은 결과를 타입이 모든 DPI 오류를 막았다는 근거로 사용하지 않는다.

### 디자인 길이 검사의 예외

`on_scale_length_literal.rs`는 디자인 토큰이 바뀌면 함께 바뀌어야 하는 길이 리터럴을 검사한다. 다음 값은 다르게 취급한다.

- 빈 사각형을 막는 1px 하한과 1 물리 픽셀 미만의 비교는 값·clamp 또는 비교 형태로 구분한다. 같은 max 표현이라도 24px 같은 실제 치수 하한은 제외하지 않는다.
- UV의 0..1은 픽셀 길이가 아니다. `UNIT_SPACE_SITES`에 파일·호출 형태·상한·이유를 등록한다. 같은 pos2 호출이 화면 좌표에도 쓰이므로 숫자 1만 보고 일괄 제외하지 않는다.

예외도 스캔 결과에 표시해 건수가 상한을 넘지 않는지 확인한다. 새 UV 항목을 등록할 때는 실제로 정규화 좌표인지 검토한다. 명부에 맞춘 숫자만으로 의미가 검증되지는 않는다.

## `f32` 로 남는 값

비율·불투명도·scale factor·색 채널은 길이가 아니므로 f32로 둔다. 외부 API에 넘기기 직전 꺼낸 값도 해당한다.

## 새 코드 작성 시

1. 입력이 장치 픽셀인지 논리 UI 단위인지 확인해 타입을 고른다.
2. 내부 필드·계산·함수 경계에서 타입을 유지한다.
3. DPI 경계를 넘을 때 변환 메서드를 사용하고 외부 API 직전에만 값을 꺼낸다.

기존 상수를 전환할 때는 파일 안에서 끝나는 변경, 산술을 함께 바꿔야 하는 변경, 함수 시그니처까지 바뀌는 변경을 나눈다. 이름 개수보다 실제 영향 범위를 확인하고 한 부류를 마친 뒤 잔여 명부를 줄인다.

## 관련

- 좌표계와 변환 경계 결정
- [테마와 UI 배율](../design/systems/theme.md)
- [DPI 화면 검증](../ai-verification/dpi-scale-verification.md)
