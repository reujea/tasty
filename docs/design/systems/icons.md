# 아이콘 시스템

tasty 의 라인/필 아이콘 세트 규칙. 지오메트리(SVG path)의 **단일 소스**와 소비 구조,
아이콘 추가 절차, 정합 보장 방식을 기술한다. 색·크기 토큰 규칙은 [theme.md](theme.md)
"UI 디자인 규칙" 과 함께 본다.

## 단일 소스 — `crates/tasty-icons`

아이콘의 SVG 도형은 `tasty-icons` 크레이트에서 정의한다. 사용하는 곳에서 `<path>`를 다시 정의하지 않는다. 한 글리프는 `Icon` const 하나로
노출되고, 본체·갤러리·위젯이 모두 이 const 를 참조한다.

`tasty-icons` 는 **수기 전사**다. `crates/tasty-icons/src/lib.rs` 가 `stroke_icon!` /
`fill_icon!` 매크로로 `Icon` const 를 손으로 정의하며, 각 path 는 디자인 시스템 번들의
`icons.json` 매니페스트에서 옮긴 것이다. 이 매니페스트는 디자인의 `components/core/Icon.jsx` 안 `ICON_PATHS`와 일치해야 한다. 레포에는 매니페스트 사본·코드 생성물·생성
스크립트가 없다 — const 자체가 정적 소스다.

## `Icon` 구조 + 2 소비 경로

```rust
pub struct Icon {
    pub svg: &'static str,   // 완성 <svg viewBox="0 0 24 24" …> 문서
    pub body: &'static str,  // inner 마크업만(<path>/<rect>/<circle> 시퀀스)
    pub uri: &'static str,   // egui 로더가 쓰는 uri(bytes://tasty_icon_<name>.svg)
    pub filled: bool,        // true=채운 글리프 / false=stroke-only
}
```

한 `Icon` 을 두 소비 경로가 공유한다. 두 경로가 **바이트 동일한 같은 `<svg>` 문자열**을
쓰는 것이 정합의 핵심이다.

1. **host / gallery 런타임** — `feature = "egui"` 를 켜고 [`Icon::image(size, tint)`] 로
   `egui::Image` 를 만든다. 앱은 두 로더를 설치한다(`gpu.rs`, 갤러리 `main.rs` 동일).
   - `tasty_icons::install_texture_loader`: 아이콘 SVG를 요청 크기로 래스터하고 텍스처를 캐시한다.
   - `egui_extras::install_image_loaders`: 아이콘이 아닌 이미지(SVG·PNG 등)를 읽는다.
2. **plugin build.rs 빌드타임** — `[build-dependencies]` 로 이 크레이트를 **egui 없이**
   링크해 `Icon::svg` / `Icon::body` 를 usvg로 읽어 선분 데이터로 바꾼다. egui optional·default off
   구조라 build-dependency 로 붙어도 egui 가 링크되지 않는다.

### 색: currentColor → white + tint

24×24 viewBox, 2px stroke, round cap/join. stroke 는 **white 로 고정**하고, 소비처가
`tint` 로 테마 색을 입혀 `currentColor` 를 재현한다 — 색을 글리프에 박지 않는다. egui/resvg
가 `currentColor`를 직접 처리하지 못해 흰색 원본에 tint를 적용한다.

### fill 규약

`filled` bool 로 stroke/fill 을 분기한다. `stroke_icon!` = `fill="none" stroke="white"`
(stroke-only), `fill_icon!` = `fill="white" stroke="white"`(채운 글리프, 예: `STAR_FILL`).
채운 상태 표시자(StatusDot / Badge)는 아이콘이 아니다 — 아이콘에 fill 을 더해 상태를
표현하지 않는다.

### 크기별 텍스처

egui 0.31의 기본 텍스처 로더는 uri와 `TextureOptions`만 캐시 키로 쓴다. 그래서 같은 아이콘을 여러 크기로 그리면 처음 래스터한 크기의 텍스처를 늘이거나 줄여 흐려진다.

`tasty_icons::install_texture_loader`가 설치하는 로더는 `bytes://tasty_icon_` uri만 맡는다. 키는 `(uri, TextureOptions, SizeHint)`이고, `SizeHint`는 논리 크기 × 배율의 물리 픽셀이다. 같은 아이콘이라도 크기와 배율마다 따로 래스터하며, 배율이 바뀌면 그 배율로 다시 래스터한다. 다른 uri는 기본 로더가 처리한다.

래스터도 이 로더가 resvg로 직접 한다. resvg(tiny-skia)의 픽셀은 알파를 미리 곱한(premultiplied) 값이라 egui에 premultiplied로 그대로 넘긴다. egui_extras 0.31의 SVG 로더는 이 값을 곱하지 않은 값으로 넘겨 가장자리 알파가 한 번 더 곱해지고, 작은 아이콘의 획이 흐려진다. 그래서 아이콘은 그 경로를 쓰지 않는다.

텍스처는 쓰인 (아이콘, 크기, 배율) 조합 수만큼 생기고 앱이 끝날 때까지 남는다. 조합은 테마의 아이콘 크기 토큰과 배율 수로 제한된다.

`texture_loader.rs` 단위 테스트가 다음을 검사한다.

- 크기마다 따로 텍스처를 만든다.
- 아이콘이 아닌 uri는 기본 로더에 넘긴다.
- 두 번 설치해도 로더는 하나다.
- 두 매크로가 만드는 uri가 이 로더의 접두사를 쓴다.
- 래스터한 모든 픽셀이 같은 SVG를 tiny-skia로 직접 그린 알파 a에 대해 (a, a, a, a)다. 곱하지 않은 값으로 넘기면 가장자리 픽셀에서 실패한다.

본체 창의 egui Context 생성 자리(`src/gfx/gpu.rs`)와 갤러리 `main.rs`는 같은 fn에서 이 로더를 설치해야 한다. source guard `icon_texture_loader_installed`가 Context를 받은 변수로 `install_texture_loader`를 호출하는지 텍스트로 확인한다. 호출이 실제로 실행되는지(조건 분기·이른 반환)는 판단하지 않는다. 두 파일 밖에서 만드는 egui Context는 보지 않는다.

## 크기 소유 — 호출측

`tasty-icons` 는 **크기를 소유하지 않는다**. `Icon::image(size, tint)` 의 `size` 는 호출측이
`Theme.icon_glyph_size_{xs,sm,md}`(`LogicalPx`) 로 전달한다. 4px 그리드·14px 폰트 상한 등
치수 규율은 [theme.md](theme.md) 를 따른다.

## 소비 구조

| 소비처 | 방식 | 지오메트리 정의 |
|---|---|---|
| 본체 `src/adapters/ui/icons.rs` | `pub use tasty_icons::*` shim + host 로컬 이름 별칭(`LAYOUT_DETAIL as DETAIL` 등) + `from_name` 이름→글리프 매핑 | 없음(재노출) |
| 갤러리 `crates/tasty-gallery/src/catalog/icons.rs` | `tasty_icons::*` 재노출 + 카탈로그 페이지 전시(글리프 전시 창구) | 없음(재노출) |
| 위젯 `crates/tasty-ui-widgets/` | `tasty_icons::CHEVRON_LEFT/RIGHT` · `PLUS` · `FUNNEL` · `SUN` · `THEME` · `GIT_BRANCH` 직접 참조 | 없음(직접 참조) |

세 소비처 모두 지오메트리를 재정의하지 않는다 — 정의는 오직 `tasty-icons`. `IconButton`
(`icon_button.rs`)은 `IconPainter` 클로저 주입 방식이라 아이콘 소스에 비의존이며, 이 설계는
그대로 유지된다.

## 아이콘 추가 절차

새 글리프는 [디자인 변경 절차](../../dev-guide/design-change-workflow.md)에 따라 요청한다. 갱신된 매니페스트를 받은 뒤 `tasty-icons`에 `stroke_icon!`·`fill_icon!` 상수를 추가한다. 이미 디자인에 있으나 코드에 빠진 글리프는 해당 상수를 추가하면 된다. 디자인에 없는 도형을 소스에서 임의로 만들지 않는다.

## 정합 보장 방식

코드는 자동 생성하지 않는다. 다음 두 가지를 검사한다.

- **심볼 존재 확인** — 없는 글리프를 참조하면 빌드가 실패한다. `icons::CLOSE`
  같은 심볼 참조가 핵심 사용법이므로, 오타·미정의 글리프는 컴파일 단계에서 걸린다.
- **디자인 정본과 상수 비교** — `crates/tasty-doc-guards/tests/design_source_icons_match_the_app_transcription.rs`
  가 `crates/tasty-design-tokens/dtcg/SOURCE`에 기록한 tasty-page 커밋의 `vendor/` 아이콘과 `tasty-icons` const 를
  대조한다(`doc-guards.yml`, develop·release·main push · PR). tasty-page 체크아웃 경로는 `TASTY_PAGE_DIR`로 넘기며 없으면 실패한다.
  앱 비교 기준은 tasty-page `vendor/components/core/Icon.jsx`이고, `vendor/icons/` SVG 파일과 `Icon.jsx`의 일치는
  tasty-page의 `npm run check-icons`가 맡는다.
  기록한 커밋 이후 tasty-page `main`에서 바뀐 아이콘은 이 가드 밖이다.

## 알려진 한계

- `assets/icons/chevron-{left,right}.svg`는 코드에서 사용하지 않는 파일이다.

### 플러그인에서 선 아이콘을 그릴 때

플러그인의 빌드 스크립트는 공용 SVG를 `usvg`로 읽고 곡선을 선분으로 바꿔 `OUT_DIR`에 좌표 배열을 만든다. 실행 중에는 SDK의 `baked_icon`으로 크기와 테마 색을 적용해 그린다. 좌표는 원본의 24×24 viewBox를 기준으로 하며, 별도의 SVG 복사본이나 Unicode 대체 문자를 만들지 않는다.

이 경로는 선 아이콘을 위한 것이다. 채워진 도형·여러 색 아이콘은 같은 선분 배열만으로 재현된다고 가정하지 않는다. 런타임 SVG 로더를 쓰는 본체 경로와 플러그인의 빌드 시점 변환은 같은 원본을 공유하되 서로 다른 렌더링 방식이다.
