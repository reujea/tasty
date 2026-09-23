# 색 생성 정책

tasty 의 색 데이터는 **단 두 출처**(테마 파일 + 빌트인 fallback/const)에서만 만들어진다. 그 외 경로는 **컴파일(newtype) + clippy lint** 조합으로 차단된다. 테마 모델은 [theme.md](../design/systems/theme.md), 필드↔role 매핑은 [design-token-mapping.md 크로스워크 절](../design/systems/design-token-mapping.md#rust-필드--호출처-토큰-크로스워크).

## 정상 출처

1. `~/.tasty/themes/<id>.toml` — 사용자/빌트인 테마 파일(palette/accent/terminal/ansi/`[surfaces.<id>]`)
2. `tasty-themes::fallback` — `mocha_fallback_colors()` (빌트인 surface_themes 포함)
3. `tasty-type-appearance::theme` 의 const/const fn — `derive_overlays`, `FALLBACK_SURFACE`

**외부 입력(예외, 명시 호출 필요)**: termwiz ANSI true-color escape, 디스크 이미지/클립보드 픽셀, 직렬화 scrollback, 테스트 더미. 이들은 `dangerously_force_from_array` + 사유 주석 + `#[allow]` 필수.

> settings UI 의 surface 색 picker 는 제거됐다 — surface 색은 theme TOML 의 `[surfaces.<id>]` 직접 편집.

## 컴파일 강제 — newtype

```rust
// crates/tasty-type-appearance/src/color.rs
#[repr(transparent)]
pub struct GpuRgba([f32; 4]);  // private field
impl GpuRgba {
    pub const fn as_array(self) -> [f32; 4] { ... }                  // 꺼내기 OK
    pub const fn dangerously_force_from_array(arr: [f32; 4]) -> Self { ... }  // 외부 입력 전용
}
```

GPU 버퍼 struct(`BgInstance.bg_color: GpuRgba`)가 이 newtype 을 받으므로:

```rust
let bg: GpuRgba = [1.0, 0.0, 0.0, 1.0];        // ❌ array literal 로 못 만듦
let bg = GpuRgba([1.0, 0.0, 0.0, 1.0]);        // ❌ private field
let bg = theme().crust.to_gpu_rgba();          // ✅ theme 에서 변환
// 외부 입력 (termwiz true-color escape).
let bg = GpuRgba::dangerously_force_from_array([r, g, b, a]);  // ⚠ 명시 + 주석
```

`#[repr(transparent)]` + `bytemuck::Pod` 라 byte layout 은 raw `[f32; 4]` 와 동일 — wgpu vertex layout 무수정, 런타임 오버헤드 0.

## clippy 강제 — disallowed-methods

`clippy.toml` 의 `disallowed-methods` 가 색 생성 함수의 외부 호출을 차단한다:

| 차단 함수 | 대체 |
|-----------|------|
| `HexColor::from_rgb` / `from_rgba` | TOML 또는 const(`hex!`) |
| `egui::Color32::from_rgb` | `theme().X` 또는 `.with_alpha(N).to_egui()` |
| `egui::Color32::from_rgba_{unmultiplied,premultiplied}` | 외부 픽셀은 `#[allow]` + 주석 / premultiplied 는 `to_egui_premultiplied()` |
| `egui::Color32::from_gray` | theme 회색 톤 |

예외 위치는 본거지 모듈(`tasty-type-appearance::{color,theme}`, `tasty-themes::fallback`)의 항목 단위 `#[allow]` + `// reason:` 주석, 외부 입력/테스트는 라인별 `#[allow]` + 주석.

## 컴파일 타임 hex 검증 — `hex!`

```rust
use tasty_type_appearance::{color::HexColor, hex};
pub const BRAND: HexColor = hex!("#89b4fa");          // OK (alpha·3-digit shorthand 도)
// pub const BAD: HexColor = hex!("#zzz");            // ← compile error
```

`from_hex_const`(= `from_hex` 의 const fn 버전)으로 잘못된 hex 를 빌드 에러로 잡는다.

## 새 색 도입 시

1. **테마 색**: 빌트인은 `crates/tasty-themes/themes/*.toml`/`mocha_fallback_colors()`, 사용자는 `~/.tasty/themes/*.toml`. UI 는 `theme().X.into()`(egui) / `theme().X.to_gpu_rgba()`(GPU).
2. **alpha 변형**: `theme().X.with_alpha(N).to_egui()`.
3. **외부 입력**: `dangerously_force_from_array` + 주석 + `#[allow]`.

UI 색 읽기는 primitive 직접접근(`theme().blue`)보다 **semantic 접근자**(`accent_primary()`/`text_muted()`…)를 우선한다(다의성 구분). additive 라 픽셀 동일, 전수 이식 전까지 clippy 강제는 보류 — [theme.md](../design/systems/theme.md) "Semantic 접근자 우선" · [design-token-mapping.md 크로스워크 절](../design/systems/design-token-mapping.md#rust-필드--호출처-토큰-크로스워크).

## 추가 가드

GPU 버퍼 struct 필드·렌더러 함수 색 인자는 **항상 newtype**(`GpuRgba`/`GpuRgb`), raw `[f32; 4]` 금지. 새 GPU buffer/렌더 시그니처 추가 시 동일 적용. 길이도 같은 newtype 정책 — [typed-length](../concepts/typed-length.md).
