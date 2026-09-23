# 아이콘 시스템

tasty 의 라인/필 아이콘 세트 규칙. 지오메트리(SVG path)의 **단일 소스**와 소비 구조,
아이콘 추가 절차, 정합 보장 방식을 기술한다. 색·크기 토큰 규칙은 [theme.md](theme.md)
"UI 디자인 규칙" 과 함께 본다.

## 단일 소스 — `crates/tasty-icons`

아이콘 지오메트리는 오직 **`tasty-icons` 크레이트**가 소유한다(never-inline 계약 —
소비처는 `<path>` 를 절대 재정의·재인라인하지 않는다). 한 글리프는 `Icon` const 하나로
노출되고, 본체·갤러리·위젯이 모두 이 const 를 참조한다.

`tasty-icons` 는 **수기 전사**다. `crates/tasty-icons/src/lib.rs` 가 `stroke_icon!` /
`fill_icon!` 매크로로 `Icon` const 를 손으로 정의하며, 각 path 는 디자인 시스템 번들의
`icons.json` 매니페스트(글리프 machine-readable SoT, `components/core/Icon.jsx`
`ICON_PATHS` 와 동기)를 전사한 것이다. 레포에는 매니페스트 사본·코드 생성물·생성
스크립트가 없다 — const 자체가 정적 소스다.

## `Icon` 구조 + 2 소비 경로

```rust
pub struct Icon {
    pub svg: &'static str,   // 완성 <svg viewBox="0 0 24 24" …> 문서
    pub body: &'static str,  // inner 마크업만(<path>/<rect>/<circle> 시퀀스)
    pub uri: &'static str,   // egui 이미지 캐시 키(bytes://tasty_icon_<name>.svg)
    pub filled: bool,        // true=채운 글리프 / false=stroke-only
}
```

한 `Icon` 을 두 소비 경로가 공유한다. 두 경로가 **바이트 동일한 같은 `<svg>` 문자열**을
쓰는 것이 정합의 핵심이다.

1. **host / gallery 런타임** — `feature = "egui"` 를 켜고 [`Icon::image(size, tint)`] 로
   `egui::Image` 를 만든다. 실제 SVG 텍스처화는 앱이 설치한 `egui_extras` svg 로더
   (`gpu.rs` 의 `install_image_loaders`, 갤러리 동일)가 담당한다.
2. **plugin build.rs 빌드타임** — `[build-dependencies]` 로 이 크레이트를 **egui 없이**
   링크해 `Icon::svg` / `Icon::body` 를 usvg 에 먹여 베이크한다. egui optional·default off
   구조라 build-dependency 로 붙어도 egui 가 링크되지 않는다.

### 색: currentColor → white + tint

24×24 viewBox, 2px stroke, round cap/join. stroke 는 **white 로 고정**하고, 소비처가
`tint` 로 테마 색을 입혀 `currentColor` 를 재현한다 — 색을 글리프에 박지 않는다. egui/resvg
가 `currentColor` 를 직접 물지 못하기 때문에 white 고정 + tint 트릭으로 우회한다.

### fill 규약

`filled` bool 로 stroke/fill 을 분기한다. `stroke_icon!` = `fill="none" stroke="white"`
(stroke-only), `fill_icon!` = `fill="white" stroke="white"`(채운 글리프, 예: `STAR_FILL`).
채운 상태 표시자(StatusDot / Badge)는 아이콘이 아니다 — 아이콘에 fill 을 더해 상태를
표현하지 않는다.

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

새 글리프 추가는 **디자인 영역**이다. [디자인 변경 워크플로]를 따른다: 디자인 요청 →
갱신된 매니페스트 수령 → `tasty-icons` 에 `stroke_icon!` / `fill_icon!` const 를 **수기로
추가**. 소스에 임의 path 를 새로 인라인하지 않는다(never-inline 계약).

[디자인 변경 워크플로]: 디자인 자체를 바꾸는 변경(디자인에 없는 글리프 추가)은 소스부터
고치지 않고 디자인 측에 먼저 요청한다. 이미 디자인에 있는 글리프를 소스가 아직 전사하지
못한 경우(누락)만 소스에 const 를 더한다.

## 정합 보장 방식

자동 코드 생성은 **없다**(수기 전사이므로 생성 파이프라인 자체가 없다).
정합은 두 축으로 보장한다:

- **(a) 존재성은 컴파일러가 강제** — 없는 글리프를 참조하면 빌드가 실패한다. `icons::CLOSE`
  같은 심볼 참조가 핵심 사용법이므로, 오타·미정의 글리프는 컴파일 단계에서 걸린다.
- **(b) canonical ↔ const 정합은 가드가 대조한다** — `crates/tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs`
  가 레포 안의 canonical 사본(`site/vendor/`)과 `tasty-icons` const 를 대조한다(`doc-guards.yml`, main push · PR).
  원격 원본과 그 사본 사이의 차이는 이 가드 밖이다.

## 알려진 한계

- **루트 `assets/icons/chevron-{left,right}.svg`**: 코드 참조가 없는 pre-existing dead asset.
  정리 후보이나 이 문서 시점에서는 그대로 둔다.
