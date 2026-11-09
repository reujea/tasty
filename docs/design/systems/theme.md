# 테마 시스템 (운영 상세)

UI의 색·글꼴 크기·간격은 `Theme`에서 읽는다. 이 문서는 테마 저장·적용 방식과 토큰 사용 규칙을 설명한다.

## 핵심 모델

```text
[ 디스크 ] ~/.tasty/themes/<id>.toml          ← partial TOML, 파일명 stem = id
                  │  (settings 에서 테마 선택)
[ tasty-themes ] apply_theme(settings, id)
                  ├── theme_base.apply_partial(file)   ← 누락 필드 보존(누적)
                  └── theme_overrides.clear()
                  │  (픽커로 색 변경 시) theme_overrides.field = Some(new)
              resolve(settings) = theme_base ▷ theme_overrides + is_light overlay + SIZING
                  │
[ tasty-themes ] 전역 Theme 인스턴스 (1개, RwLock)
                  │
              theme().bg_app() / theme().spacing_sm / theme().is_light   ← UI 코드
```

### 두 레이어

| 레이어 | 타입 | 의미 | 테마 변경 시 |
|--------|------|------|--------------|
| `theme_base` | `ThemeColors`(풀 세트) | 적용된 테마 파일의 누적 결과 (**앱 소유**) | partial 덮어쓰기로 누적 |
| `theme_overrides` | `PartialColors`(모든 필드 `Option`) | 사용자가 손댄 색 (**사용자 커스터마이징의 단일 출처**) | **클리어(의도된 설계)** |

화면 적용 색 = `theme_base ▷ theme_overrides`(override 의 `Some` 필드만 덮어쓰기). partial 테마(일부 색만 정의)를 적용하면 누락 필드는 이전 base 값을 유지한다(누적).

#### 커스터마이징 모델 (불변)

- **`theme_base` 는 앱 소유다.** 빌트인 테마 파일은 앱이 관리하고 부팅 때 임베드 원본과 동기화한다. 사용자 테마 파일은 별도 ID로 만들 수 있다. 선택한 테마 위에서 색을 조절할 때는 아래 override를 사용한다.
- **사용자 색 변경은 오직 `theme_overrides` 로만 들어간다.** settings 가 보관하는 partial 레이어로, base 위에 resolve 시점에 얹힌다. base(파일)를 어떻게 바꾸거나 동기화해도 사용자 override 는 보존된다 — 두 레이어가 분리돼 충돌이 없다.
- **override 를 기록하는 정식 경로 = Settings › Appearance › Colors picker.** 픽커는 flat `PartialColors` 47색(Surfaces·Overlays·Text·Accents·Terminal-specific·ANSI 16) 을 그룹별 collapsible 로 노출한다. Terminal-specific 5행은 `selection_bg`·`vi_cursor_bg`·`search_match_bg`·`search_match_active_bg`·`search_match_active_fg` 순서이며, 활성 매치 글자색은 그 바탕색 바로 뒤에 둔다. 각 행의 "Default" 체크 = 그 필드 `None`(프리셋 base 추종), 해제 = `Some(hex)`. base 값은 resolved `theme_base` 에서 읽어 시드한다(하드코딩 없음). 행/그룹/전체 3단계 reset 으로 `None` 복귀. "Default"가 체크된 행의 hex 칸은 disabled가 아니라 읽기 전용 `Input`(mono, `field-width-xs`)이다 — 사용 중인 base 값을 text-secondary로 보여 주고 포커스·선택·복사를 허용하며, 체크를 풀면 일반 Input이 된다. Tasty 색 행과 Terminal surface 배경 행도 같다. 저장 시 `theme_overrides`가 바뀌면 설정 적용 경로가 모든 창에 즉시 반영한다(아래 "라이브 갱신"). `surface_themes`(맵 구조)는 이 flat 픽커에서 분리돼 `Tasty`/`Terminal` 섹션의 curated shortcut 으로 남되 같은 `theme_overrides` 에 기록된다.
- **테마를 바꾸면 `theme_overrides` 를 비운다(설계).** `apply_theme` 의 `theme_overrides.clear()` 는 부수효과가 아니라 의도다 — 테마 전환 = 그 테마의 색을 깨끗하게 적용하고 이전 테마에 얹어둔 사용자 변경분은 폐기한다. 픽커가 채운 override 도 함께 비워진다.

### Crate 책임

| crate | 책임 | IO |
|-------|------|----|
| `tasty-type-appearance::color` | `HexColor`, `PremulColor`, `GpuRgba`/`GpuRgb` newtype | 없음 |
| `tasty-type-appearance::theme` | `Theme` · `ThemeColors` · `PartialColors` · `ThemeSizing`/`SIZING` · `SurfaceTheme`/`FALLBACK_SURFACE` · `derive_overlays` · `Theme::surface(id)` | 없음 |
| `tasty-themes` | 전역 `RwLock<Theme>` + `theme()/set_theme()` · `ThemeFile`(TOML) · mocha/latte 임베드 · scan/load/apply/resolve/install · `first_run_init`/`sync_builtin_themes` | `~/.tasty/themes/` |
| `tasty-settings::appearance` | `AppearanceSettings.{theme,theme_base,theme_overrides,theme_is_light,ui_scale}` | settings IO |
| `tasty-design-tokens` | 디자인 DTCG export vendor(`dtcg/tasty.tokens.json`, 1278 토큰 — 수는 `crates/tasty-design-tokens/tests/freshness.rs` 가 고정) + 치수 const 생성(`crates/tasty-design-tokens/src/generated/` — primitive 는 `pub(crate)` 로 3-tier 규율 강제) + **component tier 접근자 생성**(`tasty-type-appearance/src/generated_component.rs` 로 산출 — `&Theme` 경유 치수·색 접근자, 아래 "Component tier 접근자") + freshness/`SIZING` 정합/mocha·latte 색 드리프트 가드 테스트. 생성 const 는 초기값·정합용 — 런타임 소비는 `&Theme` 경유(zoom 우회 금지). vendor 갱신 절차는 crate README | 없음 |

의존 방향은 `type-geometry ← type-appearance ← tasty-themes ← tasty-settings`다. `tasty-design-tokens` 는 `type-geometry` 만 런타임 의존(정합 테스트만 dev-deps 로 type-appearance/themes 참조) — 본체·egui 미의존.

## 빌트인 테마 정책

빌트인 테마 파일은 **앱 소유**다. 부팅 시 `sync_builtin_themes()` 가 디스크 복사본을 임베드 정본과 맞춘다 — 빌트인 색/스키마가 바뀌면 이미 풀려있던 옛 파일도 자동 갱신된다. 사용자 색 변경은 파일이 아니라 `theme_overrides` 에 있으므로 동기화가 사용자 커스터마이징을 덮어쓰지 않는다.

- **mocha**: 메모리에서 항상 내장 fallback 보장. 임베드 `MOCHA_TOML_TEXT` + `mocha_fallback_colors()`. 부팅 시 sync 가 누락/파싱 실패/**내용 불일치** 면 임베드로 덮어쓴다. 읽기·UTF-8 실패 원본은 보호하고 메모리에서는 내장 색상을 쓴다. runtime 적용은 파일 복구를 호출하지 않는다. unit test 가 `parse(MOCHA_TOML_TEXT) == mocha_fallback_colors()` 강제.
- **latte**: first-run(themes 폴더에 TOML이 없는 경우)에 자동 풀림. 이후엔 **파일이 있으면 임베드와 동기화**, 사용자가 지우면 존중하고 다시 풀지 않음(fallback 없음).
  - `subtext0` 은 upstream catppuccin latte(`#6c6f85`)가 아니라 `#63667c` 다 — upstream 값은 `base`(`#eff1f5`) 위에서 4.37:1 이라 아래 "텍스트 대비" 4.5:1 규칙을 못 넘긴다. subtext0→subtext1 램프 위에서 내려 `base` 4.99:1 / `mantle` 4.64:1 을 확보한 값이다. mocha 는 같은 토큰이 이미 7.37:1 이라 손대지 않았다. 남은 미달 조합은 아래 "latte 중성 램프 대비 — 알려진 예외" 참고.
- **사용자 테마**: 자동 동기화/복구 없음. 로드 실패 시 mocha fallback.
- **"사용자 의도 존중" 의 범위 = 파일 삭제(부재)뿐.** 읽을 수 있는 빌트인 파일의 *내용* 은 존중 대상이 아니다 — 사용자가 손으로 고쳐도 다음 부팅에 정본으로 되돌아간다(편집 경로가 아님, 커스터마이징은 `theme_overrides`).

## ThemeFile TOML

`~/.tasty/themes/<id>.toml`(stem = id). **모든 색상 필드 optional** — partial 정상 동작.

```toml
label = "표시 이름"   # 선택
is_light = false      # 선택. 없으면 이전 is_light 보존

[palette]   crust = "#11111b"   # ...
[accent]    blue = "#89b4fa"    # ...
[terminal]  selection_bg = "#585b70"   search_match_bg = "#f9e2af4d"  # 8자리 hex = alpha
[ansi]      black = "#45475a"   # 16 키: black..white + bright_*
[surfaces.terminal]   focused_bg = "#000000"  focused_fg = "#cdd6f4"  unfocused_bg = "#1e1e2e"  unfocused_fg = "#a6adc8"
```

- 내장 테마의 `[surfaces.terminal]`과 `[surfaces.markdown]` bg 네 값은 포커스 바탕 쌍과 같다. focused는 Mocha `#000000`, Latte `#ffffff`이고 unfocused는 Mocha base `#1e1e2e`, Latte crust `#dce0e8`이다([토큰 대응](design-token-mapping.md#surface-kind-색은-surface_themes-map)).
- 내장 테마의 `[surfaces.*].unfocused_fg`는 토큰 `text-muted`와 같다. Mocha는 `subtext0` `#a6adc8`, Latte는 `subtext1` `#5c5f77`이다. Latte `subtext0` `#63667c`는 crust 위 4.26이라 글자로 쓰지 않는다.
- `[surfaces.<id>]` 의 `<id>` 는 자유 문자열 — plugin 이 등록한 surface kind 도 정의 가능. `theme().surface(id)` 가 없는 id 엔 `FALLBACK_SURFACE`(검은 배경 + Mocha 톤) 반환 → plugin 이 색을 안 줘도 안전.
- `hover_overlay`/`active_overlay`/`separator` 같은 반투명 의미색은 TOML 에 없다 — `is_light` 로부터 자동 도출(라이트=검정 +8%/+12%, 다크=흰색 +8%/+12%).
- UI 크기/간격(`spacing_*`/`border_width`/`item_height_*`/`font_size_*`)도 TOML 에 없다 — 모든 테마 공통 `SIZING` const.
- **HexColor**: `#RGB` / `#RRGGBB`(alpha=255) / `#RRGGBBAA`. 직렬화는 alpha=255 면 6자리, 아니면 8자리.
- `[terminal]` 강조색(`selection_bg` · `vi_cursor_bg` · `search_match_bg` · `search_match_active_bg`)의 alpha 는 **그 셀의 배경 위에 합성**된다 — 창 아래가 비치는 것이 아니다(이 문서). `search_match_active_fg` 는 불투명 글자색으로, 현재 검색 매치 칸에서만 셀 글자색을 덮어쓴다. 테마 파일에 키가 없으면 Mocha 기본값을 쓰고, 이 키가 없는 직렬화 색 집합(이 필드 이전 host 가 plugin 에 보낸 테마 스냅샷)은 Theme 을 만들 때 base·text 중 active 채움(base 위 합성) 위에서 대비가 큰 색으로 도출한다.
- **빌트인(`mocha`/`latte`) 파일은 직접 편집하지 말 것** — 앱 소유라 부팅 시 임베드 정본으로 되돌아간다. 커스텀 테마는 별도 id(예: `my-theme.toml`)로 만들고, 기존 테마 위 색 조정은 settings 의 `theme_overrides` 로 한다.

## 부팅 흐름 (`window_lifecycle.rs::boot_apply_theme`)

`first_run_init()`(TOML이 없는 폴더면 mocha+latte 시드) → `sync_builtin_themes()`(빌트인을 임베드 정본과 동기화) → `rescan()` → `apply_theme(요청 id)`(실패 시 mocha) → `install_global_with_runtime(&settings.appearance, settings.theme_runtime())`. 요청 id ≠ 적용 id 면 InfoModal 로 알림.

이 초기 준비는 첫 로딩 프레임 전 완료 barrier이며 같은 디렉터리 안전 교체와 관측한 원본/링크 비교를 사용한다. 읽을 수 없는 파일을 덮어쓰지 않는다. 실행 중 fallback은 캐시 또는 내장 Mocha만 사용하고 파일을 쓰거나 다시 스캔하지 않는다. 추가 창도 초기 seed/sync를 반복하지 않는다. 설정의 명시 rescan은 읽기 전용이다.

## UI 코드의 색상 접근

```rust
let th = crate::theme::theme();
ui.painter().rect_filled(rect, CornerRadius::ZERO, th.accent_primary());
let bg = th.bg_panel().to_float();
let pad = th.spacing_sm;
```

host UI와 공용 위젯은 semantic 접근자를 사용한다. 원시 팔레트 필드는 테마 내부·색상 픽커·터미널 ANSI 처리·갤러리의 원시 팔레트 전시에 한정한다.

직접 필드 접근은 [`design_token_adherence.rs`](../../../crates/tasty-doc-guards/tests/design_token_adherence.rs)가 소스를 읽어 검사한다. GUI 기능에 의존하지 않아 GUI·headless 빌드 조합과 관계없이 실행할 수 있다. CI의 `doc-guards.yml`은 경로 필터 없이 main push와 PR에서 이 검사를 실행하도록 설정돼 있다. `crossplatform-check.yml`의 Windows 문서 검사와 headless 전체 테스트에도 포함된다. 워크플로 실행 조건과 실제 결과 확인 방법은 [CI 가이드](../../dev-guide/ci-gates.md)를 따른다.

대응 역할이 없으면 원시 색으로 되돌리지 않고 가까운 역할의 임시 alias와 `divergence:` 사유를 남겨 디자인에 요청한다. 역할이 확정되면 임시 alias를 제거한다.

| 표현할 역할 | 사용할 접근자 |
|---|---|
| 비활성 라벨·글리프 | `text_disabled()` |
| 탭 스트립 스크롤 화살표 | `tab_scroll_arrow_fg()`(스크롤할 수 있음) · `tab_scroll_arrow_fg_disabled()`(끝에 닿음). 이동 대기 대상 탭 칸이 가려진 쪽은 `tab_scroll_arrow_move_fg()`. hover 채움 `tab_scroll_arrow_hover_bg()`는 disabled가 아닌 쪽에만 깐다 |
| surface-raised(메뉴·드롭다운 목록) 위의 보조 글자 | `text_secondary_raised()`. Mocha는 `text_secondary()`와 같고 Latte는 `text_primary()`와 같다 |
| 배너 본문 | `banner_body_fg()`(→ `text_muted_raised()`). Mocha는 `text_muted()`와 같고 Latte는 `text_primary()`와 같다 |
| 입력 전 안내 | `text_placeholder()`. 입력 상자 안의 일반 글자 등급이다 |
| 입력 상자 채움·테두리 | `surface_field()` · `border_control()`, raised 바탕은 `border_control_raised()`. Latte의 채움은 base와 같은 색이라 상자는 테두리로 보인다. hover는 채움을 바꾸지 않고 테두리만 바꾼다 |
| 스크롤바 thumb | `scrollbar_thumb()`, raised 바탕은 `scrollbar_thumb_raised()`, hover·drag는 `scrollbar_thumb_hover()`. egui `ScrollArea`는 상태별 `fg_stroke`를 읽으므로 전역 visuals에 넣고, raised 바탕은 `apply_raised_scrollbar()`가 호출 범위에서 바꾼다 |
| 약하게 표시하는 chrome 글리프 | `glyph_dim()` |
| popup 프레임·pane divider·GPU 비활성 보더 | `border_frame()` |
| popup 내부 구분선 | `border_strong()` |
| 메뉴 구분선(Tools 메뉴 · 컨텍스트 메뉴 견본 · 레일 카테고리 메뉴 · Select/SearchableSelect 목록 · 배너 ⋯ 메뉴) | `menu_separator()`(→ border-strong). 카드·탭 스트립·표·목록 안의 hairline 은 `separator` 로 남는다 |
| 장식 accent | `accent_decorative()` |
| 주의 환기 accent | `accent_attention()` |
| 이동 대기 대상 표시 | `accent_move()` (component `move_source_ring()`·`move_source_glyph()`) |
| DAG 입력 binding·전이 엣지 | `accent_data()`·`accent_route()` (component `dag_edge_binding()`·`dag_edge_transition()`) |

값이 같더라도 프레임 선과 선택 배경, 장식과 주의 환기를 같은 역할로 묶지 않는다. 선의 단계는 명도 순서가 아니라 배경과의 대비로 확인한다. Latte에서는 더 강한 선이 더 어두울 수 있다. 현재 타이틀바 아래 선은 `titlebar_border()`를 호출하며 이 접근자는 separator를 반환한다. 프레임 선 역할 설명과 실제 CSD 매핑을 혼동하지 않는다.

단순 primitive alias인 semantic 색 메서드는 `semantic_color_generated.rs`에서 생성한다. 테마 밝기 분기·overlay 도출·합성색·OS 또는 브랜드 값은 수기 접근자로 남는다. component 접근자도 이 경로를 호출한다.

`hover_overlay`·`active_overlay`·`separator`는 알파를 미리 곱한 바이트로 저장하며 타입이 `PremulColor`다. 이 세 필드에 alias로 닿는 접근자(`overlay_hover()`·`overlay_active()`·`titlebar_border()`·`modhint_separator()`와 생성된 component 접근자)도 `PremulColor`를 반환한다. `PremulColor`는 `to_egui_premultiplied()`만 제공하므로 알파를 한 번 더 곱하는 `to_egui()`·`Color32::from`·`into()`는 컴파일되지 않는다. CSS처럼 straight rgba를 받는 곳에는 `unpremultiplied()`로 straight `HexColor`를 얻어 넘긴다. `PremulColor`에는 GPU 변환이 없어 GPU 버퍼로 가는 경로가 없다. 직접 색 생성은 아래 정책을 따른다.

## 색 생성 정책

tasty 의 색 데이터는 **단 두 출처**(테마 파일 + 빌트인 fallback/const)에서만 만들어진다. 그 외 경로는 **컴파일(newtype) + clippy lint** 조합으로 차단된다. 테마 모델은 이 문서 위쪽 [핵심 모델](#핵심-모델), 필드↔role 매핑은 [design-token-mapping.md 크로스워크 절](design-token-mapping.md#rust-필드--호출처-토큰-크로스워크).

### 정상 출처

1. `~/.tasty/themes/<id>.toml` — 사용자/빌트인 테마 파일(palette/accent/terminal/ansi/`[surfaces.<id>]`)
2. `tasty-themes::fallback` — `mocha_fallback_colors()` (빌트인 surface_themes 포함)
3. `tasty-type-appearance::theme` 의 const/const fn — `derive_overlays`, `FALLBACK_SURFACE`

**외부 입력(예외, 명시 호출 필요)**: termwiz ANSI true-color escape, 디스크 이미지/클립보드 픽셀, 직렬화 scrollback, 테스트 더미. 이들은 `dangerously_force_from_array` + 사유 주석 + `#[allow]` 필수.

설정 › Appearance › Terminal에는 포커스·비포커스 배경 픽커가 있다. `draw_terminal_surface_colors`가 `theme_overrides.surface_themes["terminal"]`에 기록한다. 다른 surface 종류의 기본색은 사용자 테마 TOML의 `[surfaces.<id>]`로 정의할 수 있다.

### 컴파일 강제 — newtype

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

`PremulColor`도 같은 방식으로 변환을 제한한다. 생성자는 크레이트 안(`derive_overlays`)에서만 부를 수 있고, egui 변환은 `to_egui_premultiplied()` 하나다. 토큰 생성기는 alias 체인이 `overlay-hover`·`overlay-active`·`separator`에 닿는 component 색 접근자의 반환 타입을 `PremulColor`로 만든다(`PREMULTIPLIED_SEMANTIC_COLORS`). `HexColor`에는 `to_egui_premultiplied()`가 없어 straight 색을 premultiplied로 읽는 반대 실수도 컴파일되지 않는다.

### clippy 강제 — disallowed-methods

`clippy.toml` 의 `disallowed-methods` 가 색 생성 함수의 외부 호출을 차단한다:

| 차단 함수 | 대체 |
|-----------|------|
| `HexColor::from_rgb` / `from_rgba` | TOML 또는 const(`hex!`) |
| `egui::Color32::from_rgb` | `theme().X` 또는 `.with_alpha(N).to_egui()` |
| `egui::Color32::from_rgba_{unmultiplied,premultiplied}` | 외부 픽셀은 `#[allow]` + 주석 / premultiplied 테마 색은 `PremulColor::to_egui_premultiplied()` |
| `egui::Color32::from_gray` | theme 회색 톤 |

예외 위치는 본거지 모듈(`tasty-type-appearance::{color,theme}`, `tasty-themes::fallback`)의 항목 단위 `#[allow]` + `// reason:` 주석, 외부 입력/테스트는 라인별 `#[allow]` + 주석.

### 컴파일 타임 hex 검증 — `hex!`

```rust
use tasty_type_appearance::{color::HexColor, hex};
pub const BRAND: HexColor = hex!("#89b4fa");          // OK (alpha·3-digit shorthand 도)
// pub const BAD: HexColor = hex!("#zzz");            // ← compile error
```

`from_hex_const`(= `from_hex` 의 const fn 버전)으로 잘못된 hex 를 빌드 에러로 잡는다. 고정 DTCG 원시색도 이 경로를 쓴다. 예를 들어 `semantic.accent-success-ink`의 혼합 대상인 `primitive.color-black`은 `hex!("#000000")`이며 테마 팔레트의 `black`과 구별한다.

### 새 색 도입 시

1. **테마 색**: 빌트인은 `crates/tasty-themes/themes/*.toml`/`mocha_fallback_colors()`, 사용자는 `~/.tasty/themes/*.toml`. UI 는 `theme().X.into()`(egui) / `theme().X.to_gpu_rgba()`(GPU).
2. **alpha 변형**: `theme().X.with_alpha(N).to_egui()`. `to_egui()`는 sRGB 바이트에 알파를 곱해 premultiplied 색을 만든다. egui 렌더러가 sRGB 바이트 공간에서 합성하므로 결과는 디자인의 `color-mix(in srgb, C N%, transparent)`를 배경에 올린 값과 같다(Mocha accent 12% on base = (43,48,70)). `Color32::from_rgba_unmultiplied`와 `Color32::linear_multiply`는 선형 공간에서 곱해 이 합성과 맞지 않고 어두운 배경에서 색이 진해지므로 테마 색에 쓰지 않는다. 이미 만든 `Color32`를 더 옅게 할 때는 `gamma_multiply`를 쓴다. 승인 팝업의 위험 선택지 채움은 대응 토큰이 없어 기존 `linear_multiply(0.18)`이 남아 있다.
3. **외부 입력**: `dangerously_force_from_array` + 주석 + `#[allow]`.

host UI와 공용 위젯은 semantic 색 접근자를 사용한다. 원시 팔레트 예외와 소스 검사는 위 [UI 코드의 색상 접근](#ui-코드의-색상-접근)을 따른다.

### 추가 가드

GPU 버퍼 struct 필드·렌더러 함수 색 인자는 **항상 newtype**(`GpuRgba`/`GpuRgb`), raw `[f32; 4]` 금지. 새 GPU buffer/렌더 시그니처 추가 시 동일 적용. 길이도 같은 newtype 정책 — [typed-length](../../concepts/typed-length.md).

## Component tier 접근자

DTCG component tier(치수+색) 토큰은 `crates/tasty-type-appearance/src/generated_component.rs` 의 **생성된 `&Theme` 메서드**로 노출된다 (`tasty-design-tokens` 생성기가 산출, `DO NOT EDIT`). `generated::component` 의 raw const 를 위젯이 직접 읽으면 `with_colors_and_zoom` 의 zoom resolve/제외 정책을 우회하므로, 위젯은 **반드시 이 접근자를 경유**한다.

- **치수 접근자**(→ `LogicalPx`) 3형태: alias 체인이 (a) zoom 정책이 이미 박힌 `Theme` 필드에 닿으면 그 필드 반환, (b) 다른 component 접근자에 닿으면 그 접근자 호출, (c) primitive 에 직접 닿으면 `Theme.ui_zoom` 을 곱해 계산(`LogicalPx((v*ui_zoom).round())`). 예: `button_gap()`=`spacing_sm`, `button_height_lg()`=`(32*ui_zoom).round()`.
- **색 접근자**(→ `HexColor`): semantic 접근자 체인 또는 component→component 상호 호출. 예: `button_primary_bg()`=`accent_primary()`. `banner_*`/`titlebar_*` 색은 기존 수기 접근자와 이름 충돌이라 생성 제외(수기 유지).
- **자간 접근자**(→ `LogicalPx`): 자간 토큰(em 단위인 `semantic.letter-spacing-caps`·`semantic.letter-spacing-caps-wide`, `component.table-header-tracking`, `component.sidebar-section-heading-tracking`, 그리고 값이 단위 없는 0 인 `semantic.letter-spacing-ui`·`component.sidebar-wordmark-tracking`)은 `crates/tasty-type-appearance/src/generated_tracking.rs` 의 생성 메서드로 읽는다. 글자 크기를 받아 `em × 글자 크기`를 돌려주고, egui `TextFormat::extra_letter_spacing`에 `.value()`로 넘긴다. 글자 크기 필드가 이미 UI 배율을 반영하므로 배율을 다시 곱하지 않는다. 예: `letter_spacing_caps(font_size_micro)` = 0.4(배율 1). component 자간은 semantic 접근자를 호출한다. em 토큰은 `LogicalPx` 상수로는 생성하지 않는다(`Skip::EmUnit`). 자간 접근자를 만드는 component 토큰(값이 단위 없는 0 인 것 포함)은 같은 이름의 치수 접근자를 만들지 않는다. 자간이 아닌 px 값의 component 토큰(`component.loading-lockup-tracking` = −1px)은 일반 치수 접근자다.
- **소비처**: `tasty-ui-widgets` 위젯(button/chip/input/menu_item/select/toggle/tree_row/icon_button/status_dot/table 등)이 이 접근자를 소비. host chrome(`src/adapters/ui/`)도 이 접근자를 소비한다.
- **zoom 회귀/값불변 테스트**: `tasty-type-appearance` 에 zoom 1.0 값 불변(이식 전후 동일)·zoom 1.5 스케일 단위 테스트 존재.

## CSD 타이틀바 토큰

[윈도우 크롬](../../features/window-chrome/index.md)이 쓰는 전용 토큰. 색은 semantic 접근자 조합, 길이는 `ThemeSizing`.

| 접근자 | → semantic | 용도 |
|--------|-----------|------|
| `titlebar_bg()` / `titlebar_bg_inactive()` | `bg_app` / `bg_sidebar` | 타이틀바 배경(active/inactive) |
| `titlebar_border()` | `separator` | 하단 1px 보더 |
| `titlebar_fg()` / `titlebar_fg_inactive()` | `text_secondary` / `text_muted` | 전경(active/디밍) |
| `accent_window_close()` / `text_on_window_close()` | `#c42b1c` 리터럴 / white | close 버튼 hover — **테마 불변 OS 리터럴**(다크/라이트 동일, 테스트 고정) |

길이 토큰: `titlebar_height`(36px = `top_inset`) · `traffic_size`(12px, 예약) · `caption_width`(46px, Windows) · `window_button_size`(24px, Linux). 모두 **host UI zoom 제외**(OS 데코 관습상 고정 px).

## UI 디자인 규칙 (필수)

새 UI는 아래 토큰과 역할을 따른다. 시안을 구현할 때는 값뿐 아니라 [컴포넌트 구조](design-parity-notes.md)와 [갤러리 매핑](design-gallery-mapping.md)도 확인한다. 같은 색을 썼다고 같은 레이아웃이 되는 것은 아니다.

| 항목 | 기본 규칙 |
|---|---|
| 테마 | Mocha를 기본·폴백으로 제공하고 Latte는 처음 실행할 때 설치한다. |
| 간격 | 4px 그리드의 `spacing_xs/sm/md/lg/xl`을 사용한다. |
| UI 폰트 | micro 10, caption 11, body·heading 13, max 14. 역할이 있으면 component 접근자를 우선한다. |
| 라벨 글꼴 | 번역 문구를 그리는 작은 라벨은 `font-label`(`label_font`)을 읽는다. `ja`·`zh`는 UI sans, 그 밖은 mono다. 값(경로·ID·버전·크기·시각·숫자·키캡)은 모든 언어에서 mono이고 값을 담은 Tag는 `tag_mono`다. [i18n](../../dev-guide/i18n.md) |
| 폰트 상한 | UI는 14px. 콘텐츠 폰트는 별도이며 브랜드 워드마크 17·부트 락업 30은 승인된 예외다. |
| 보더 | `border_width` 1px. 접힌 레일의 카테고리 `---` 선과 카테고리 없는 레일의 맨 위 구분선은 상자 테두리가 아니라 아바타 글자 무게의 표지라서 `rail-category-line-width`(= focus-ring-width 2px)로 그린다. |
| 점선 테두리 | 1px 점선은 대시 `border_dash` 4 · 간격 `border_dash_gap` 4(배율 제외)다. 대시는 곧은 변에만 두고 각 변이 대시로 시작해 대시로 끝나도록 남는 길이를 간격에 고르게 나눈다. 모서리는 실선 호다. 공용 `tasty_ui_widgets::paint_dashed_outline` 으로 그린다. 쓰는 곳: Add plugin 빈 안내 상자, Scripts Add trigger… |
| 지목 링 | 대상을 감싸는 획은 `focus_ring_width` 2px. 색은 용도에 맞는 semantic 색을 고른다. |
| 한쪽 선택 바 | 선택 항목의 한쪽 변 띠는 하나의 역할 `selection_edge_width` 2px(hairline, 배율 제외)다. 탭 밑줄은 `tab_indicator_width()`, 활성 워크스페이스 행은 `workspace_row_active_bar_width()`, 목록 행 선택 바(ListCtrl · file handler picker · git viewer 커밋·파일 행)는 `listctrl_selected_bar_width()`로 읽고 모두 이 역할을 가리킨다. remote attach·preset 목록 행과 갤러리 switch overlay·file picker 선택 행은 `selection_edge_width`를 직접 읽는다. 토스트 바는 `toast_accent_width` 3px로 별도 역할이다. 선택 표시가 아닌 한쪽 변 2px 선은 각자의 토큰을 읽는다: preset 분할 영역의 분할선 `preset_split_divider_width()`, markdown 인용 막대 `md_quote_bar_width()`(둘 다 `selection_edge_width`를 가리킨다), tinted 강조 상자(갤러리 Do / Don't 콜아웃)의 앞쪽 변 `tint_edge_width`(hairline, 배율 제외). |
| painter 아이콘 | close X·chevron·트리 가지 등의 선은 `icon_stroke_width` 1.5px. |
| 반경 | `corner_radius_sm` 2, 기본 4, `_lg` 8. 반경이 없으면 `CornerRadius::ZERO`. |
| hover·active | `hover_overlay` 8%, `active_overlay` 12%. 밝은 테마는 검정, 어두운 테마는 흰색에서 만든다. |
| 텍스트 대비 | 최소 4.5:1. Latte의 기존 예외는 아래 대비 표를 따른다. disabled 잉크는 대비 목표를 두지 않고 순서·패리티 규칙을 따른다(아래 "disabled 잉크" 절). |

이 표의 수치를 호출부에 복사하지 말고 해당 Theme 필드·접근자를 사용한다. 승인 없이 새 값을 정하거나 비슷한 값의 다른 토큰으로 바꾸지 않는다. 보편적인 표·버튼·선택 위젯은 [공용 위젯](../../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로)으로 구현한다.

### API별 값 전달

| API | 전달할 값 |
|---|---|
| 간격·margin | `vspace`·`hspace`·`margin_all`·`margin_sym`에 LogicalPx를 전달한다. 미세 구조 간격은 `STRUCT_GAP_1/2/3/4`. |
| FontId·RichText 크기 | 해당 폰트 토큰의 `.value()`. `FontId::proportional/monospace/new`의 숫자 리터럴과 FontId 구조체 리터럴은 금지한다. |
| Stroke | 용도에 맞는 선 굵기 토큰. `Stroke::new` 숫자 리터럴과 Stroke 구조체 리터럴은 금지한다. |
| 반경 | 반경 토큰. `.corner_radius`와 `CornerRadius::same` 모두 숫자를 직접 쓰지 않는다. |
| 컨테이너 폭·높이 | Theme 접근자. set_min/max_width, set_min_height, max_height, exact_width/height, desired_width에도 같은 규칙을 적용한다. |
| 색 파생 | 역할이 일치하는 opacity 토큰 또는 사유가 있는 명명 상수. gamma_multiply·with_alpha에 익명 숫자를 넣지 않는다. |

명명 상수만으로 모든 규칙이 충족되지는 않는다. 토큰의 숫자를 복사한 상수는 배율·역할 연결을 끊는다. 단, `Spinner::size`는 폰트 크기가 아니라 위젯 지름이다.

### 토큰에 없는 값과 배율

값은 해당 용도의 스케일에서 비교한다. 구조 길이는 size, 폰트는 font-size, 반경은 radius, 아이콘은 icon_glyph_size를 본다. 다른 스케일에 같은 숫자가 있어도 대응 토큰으로 취급하지 않는다.

대응 역할이 없는 폰트·반경·점 크기·색 계수는 사유가 있는 명명 상수로 남기고 디자인 결정으로 해결한다. 임의 반올림은 픽셀 변경이다. 폰트 토큰은 배율 적용 후 반올림되므로 `.5` 값과 같은 값을 유지할 수 없다. 이전에 승인된 폰트 조정은 해당 대상의 결정이며 새 값에 대한 포괄 승인으로 쓰지 않는다.

반경 예외 `TAG_PILL_CORNER_RADIUS` 3은 현재 공용 tokens 모듈에 있다. 이 상수는 배율을 따라 커지지 않는다. 4·8 반경 토큰은 지원 배율에서 변하지만 1·2 같은 작은 정수는 반올림 결과가 같을 수 있으므로, 배율 적용 경로와 실제 값 변화를 구분한다. 지원 배율이 바뀌면 다시 비교한다.

본체 컨테이너는 내부 글꼴·간격과 함께 커져야 한다. 대응 토큰이 없으면 원래 값을 유지하는 `LogicalPx((N * self.ui_zoom).round())` 접근자를 두고, 디자인 토큰이 생기면 생성 접근자로 옮긴다. 값이 없는 이유로 상자만 고정하면 큰 배율에서 내용이 잘린다. 고정 크기 축소 그림과 그 아래 실제 폼은 같은 파일이어도 따로 판단한다.

갤러리는 egui 전역 zoom을 사용해 리터럴도 함께 커진다. 따라서 본체의 배율 누락 검사에서 제외하지만, 전시 공간 치수에는 여전히 [이유가 있는 명명 상수](../policies/gallery-completeness.md)가 필요하다.

### 생성 길이 상수의 Theme 경로

본체 UI는 `tasty_design_tokens::generated`의 LogicalPx 상수를 직접 읽지 않는다. 생성 상수는 초기값과 정합 검사에 사용하고, 실제 UI는 그 토큰 자신의 Theme 경로를 사용한다. 예를 들어 `component.fp-crumb-max-width`는 `th.fp_crumb_max_width()`다.

가드는 `SEMANTIC_DIM_TO_THEME_FIELD`에서 먼저 나오는 매핑을 확인하고 그 필드가 Theme에 실제로 있는지 본다. 매핑이 없으면 component 이름의 생성·수기 접근자를 찾는다. 경로 없는 토큰은 `PATHLESS_LENGTH_TOKENS`에 이유를 남긴다. 소비가 필요하면 매핑·Theme 필드·sizing_parity 검사를 먼저 만든다. 값이 같은 다른 이름은 대체 경로가 아니다.

배율 적용·제외는 Theme가 정한다. border_width·status_bar_height처럼 일부러 고정한 필드도 이 경로를 거친다. 불투명도·가중치 같은 무차원 값과 지속시간은 길이 검사 대상이 아니다.

### 상태 점과 tint

| 역할 | 접근자·치수 |
|---|---|
| 일반 상태 점 | `status_dot_size` 8. badge·tag도 같은 일반 크기. |
| 24px chrome 안의 점 | `status_dot_size_compact` 6. tab·statusbar 크기도 compact. |
| 활성 탭 위치 마커 | 4. 상태 종류를 표시하는 점과 다른 역할. |
| attached ring | 폭 2 + offset 2. offset은 점 바깥 경계부터 ring 안쪽 경계까지다. |

워크스페이스 행의 attached 점은 일반 점 8을 쓰므로 ring까지 포함한 전체 폭이 16이다. 행은 attached 여부와 무관하게 `workspace_dot_slot()` 슬롯을 모든 행에 예약하고 점을 슬롯 가운데에 둔다. 슬롯 폭은 반올림한 점 지름에 반올림한 ring 폭과 offset의 두 배를 더해 만든다. 슬롯을 따로 반올림하면 0.85에서 ring(15)이 슬롯(14)을 넘기 때문이다. 그래서 슬롯은 0.85·1·1.2에서 15·16·18이고, 생성 접근자가 아닌 `theme.rs`의 수기 접근자다. 슬롯 높이는 제목 라벨이 실제로 차지하는 한 줄 높이라 슬롯이 행을 키우지 않는다. 왼쪽 inset은 `workspace_row_padding_x()` 8, 슬롯과 본문 사이는 `workspace_dot_gap()` 4라 제목·REMOTE 줄·부제·설명이 배율 1에서 카드 끝으로부터 28에 시작한다. 활성 행의 accent bar는 `workspace_row_active_bar_width()` 2(hairline)라 배율과 무관하게 2다. compact 점과 ring의 전체 폭 14는 24px chrome에 맞춘 값이며 현재 소비처가 없다. Plugins Attention의 7, 색 override·튜토리얼의 5 같은 남은 별도 값은 임의로 맞추지 않는다. 4px 간격 그리드는 점 지름 규칙이 아니다.

accent 채움과 테두리를 함께 쓰는 표현은 `tint_fill_alpha()` 0.12와 `tint_border_alpha()` 0.36을 사용한다. 승인된 채움만 사용과 테두리만 사용도 같은 값을 쓴다. 다만 별도 역할의 경고 테두리처럼 이 조합이 아닌 값은 사유가 있는 상수다. 같은 숫자라도 역할이 다르면 opacity 토큰으로 바꾸지 않는다.

### 경고 알림 배지

`notify-warning-fill`과 `notify-warning-fg`는 미확인 개수 배지 전용 역할이다. Mocha는 기존 yellow와 text-on-accent를 사용한다. Latte는 yellow 50%와 white를 sRGB로 혼합한 채움에 neutral-1100 글자를 올린다. 숫자와 글리프가 같은 전경을 쓰며 기본 팔레트의 대비는 약 5.01:1이다. `notification_ink.rs`의 Theme 접근자가 갤러리와 본체의 공용 배지 위젯에 같은 색을 전달한다. 상세 목록의 warning 글리프와 다른 강조색의 전경 역할은 별개다.

`accent-warning-glyph`는 밝은 배경 위 경고 글리프 역할이다. Mocha는 yellow를 그대로 쓰고, Latte는 yellow 50%와 neutral-1100을 sRGB로 혼합한다. Plugins 창 신뢰 상자의 권한 변경 글리프(`plugin-trust-glyph-warning`)와 알림 목록의 warning 글리프(`notification-item-warning-glyph`)가 이 역할을 참조한다. 기본 Latte에서 목록 글리프의 지정색 대비는 base 위 약 3.99:1, hover 바이트 합성 배경 위 약 3.35:1이며 읽음 여부로 색이 바뀌지 않는다. Mocha는 기존 yellow를 유지한다. 접근자는 `notification_ink.rs`에 수기로 두며 생성기는 component 토큰 접근자만 만든다.

`accent-danger-raised`는 raised 바탕(surface-raised 카드) 위 위험 글자와 그 글리프 역할이다. Mocha는 accent-danger를 그대로 쓰고(surface0 위 5.43:1), Latte는 red 60%와 neutral-1100을 sRGB로 혼합한다(surface0 위 4.79:1). Latte의 accent-danger는 surface0 위 3.52:1이라 글자 기준에 못 미친다. 채움과 invalid 테두리는 비텍스트라 accent-danger를 유지한다. File Detectors·File Handlers 추가 카드의 오류 줄(`file-handler-form-error-fg`)이 이 역할을 참조한다. 접근자는 `accent-warning-glyph`와 같이 `notification_ink.rs`에 수기로 둔다.

`accent-success-ink`는 trusted 상자의 본문과 shield 전용 전경이다. Mocha는 기존 accent-success이고, Latte는 green 60%와 primitive black을 sRGB로 혼합한다. 본문은 `plugin-trust-body-trusted-fg`, 글리프는 `plugin-trust-glyph-success`로 이 역할을 읽는다. 전역 accent-success와 상자의 tint 채움·테두리는 그대로다. 기본 Latte에서 기존 tint를 mantle/base 위에 합성한 지정색 대비는 각각 약 5.55:1/5.94:1로 본문 4.5:1과 shield의 디자인 목표 3:1을 넘는다. `gamma_multiply(0.12)`는 RGB와 alpha를 각각 반올림하므로 RGB에 반올림된 alpha 31/255를 다시 곱하지 않는다. 지정색 계산은 안티앨리어싱 경계 픽셀 전체의 대비나 GPU framebuffer 실측을 뜻하지 않는다.

### 애니메이션과 스크롤

터미널 셀·스크롤에 추가 transition을 두지 않는다. UI 위젯의 입력 직후 피드백은 짧은 모션을 사용한다. 시간 단위와 모션 감소 설정은 아래 [모션 설정](#모션-설정과-시간-단위)을 따른다.

UI 입력란 캐럿은 깜박이지 않는다. 입력란과 창이 모두 포커스를 가진 동안 `border_width`(1px) 굵기의 `text_primary()` 막대로 계속 그리고, 창이 포커스를 잃으면 그리지 않는다. 터미널 커서와 같은 규칙이다. 호스트 egui 는 `apply_theme_to_egui` 가 `text_cursor` 의 깜박임과 획을 정한다. 플러그인 egui-mesh 컨텍스트는 호스트 테마의 visuals 를 받지 않으므로 SDK 가 깜박임만 끄고, 캐럿 색과 굵기는 egui 기본값(2px 하늘색)이다. 갤러리 예제는 Form controls › `text-caret` 이다.

프로그램으로 이동하는 `scroll_to_*`·`scroll_to_me`는 host와 egui-mesh 모두 `ScrollAnimation::none()`으로 즉시 이동한다. 휠 델타를 도착 프레임에 전량 반영하는 것은 별도 프로세스 왕복을 줄이기 위한 plugin SDK 경로다. host egui·갤러리의 휠은 기본 스무딩을 유지한다. 스크롤바·페이드와 드래그 패닝은 [스크롤 표시 규칙](#스크롤-여지를-보여-주는-방법)을 따른다.

### 검사가 확인하는 범위

- `design_token_adherence.rs`는 지정된 UI API의 원시 색·숫자 리터럴·일부 구조체 표현을 검사한다. 변수·매크로를 통한 값의 의미까지 추적하지 않는다.
- `design_token_guard.rs`는 폰트·반경 토큰을 복사한 상수, UI 폰트 상한, 익명 점 반지름·색 계수, 생성 길이 상수 직접 소비 등을 검사한다. 점 상수 검사는 DOT 이름에 의존해 다른 이름을 놓칠 수 있다. 색 계수는 갤러리·번들 plugin도 대상이며 길이 zoom 예외를 적용하지 않는다.
- `zoom_coverage_guard.rs`는 리터럴을 담은 Theme 길이 접근자의 배율 적용을 검사한다. 본체 호출부에 직접 적은 모든 컨테이너 길이를 검사하는 것은 아니다.
- 검사 실행 범위는 [CI 가이드](../../dev-guide/ci-gates.md)를 따른다. 역할 선택과 디자인 변경의 타당성은 숫자 검사로 대신하지 않는다.

### latte 중성 램프 대비 — 알려진 예외

Latte의 일부 어두운 배경과 글자 조합은 4.5:1 대비를 충족하지 못한다. 아래 표는 WCAG 상대휘도 공식(sRGB→선형, 0.2126/0.7152/0.0722 가중, `(L_hi+0.05)/(L_lo+0.05)`)으로 계산한 값이며 `*`는 통과를 뜻한다.

| 전경 \ 배경 | crust `#dce0e8` | mantle `#e6e9ef` | base `#eff1f5` | surface0 `#ccd0da` | surface1 `#bcc0cc` | surface2 `#acb0be` | `#ffffff` |
|---|---|---|---|---|---|---|---|
| `subtext0` (neutral-900) `#63667c` | 4.26 | 4.64\* | 4.99\* | 3.65 | 3.10 | 2.61 | 5.64\* |
| `subtext1` (text-secondary, Latte text-muted) `#5c5f77` | 4.73\* | 5.14\* | 5.53\* | 4.05 | 3.44 | 2.89 | 6.25\* |
| `text` (text-primary) `#4c4f69` | 6.04\* | 6.57\* | 7.06\* | 5.17\* | 4.39 | 3.69 | 7.99\* |

`text_muted()`는 Latte에서 neutral-1000(`subtext1`)을 읽는다. neutral-900은 `bg_app`(=crust) 위 상태바에서 4.26으로 미달하고, 4.5를 겨우 넘기는 혼합색은 neutral-1000과 눈으로 구분되지 않기 때문이다. 그래서 Latte의 text-muted와 text-secondary는 같은 색이고 크기와 굵기가 순서를 나타낸다. Mocha는 neutral-900 그대로다.

표의 미달 칸은 그 바탕 전용 역할로 해결한다. 역할과 바탕의 전체 표는 tasty-page `vendor/RULES.md`가 정본이다.

| 잉크 띠 | 버티는 바탕 | 예 |
|---|---|---|
| 기본 역할 | crust·mantle·base와 surface-field | `text_muted()` · `text_secondary()` · `glyph_dim()` |
| `-raised` | surface-raised와 그보다 낮은 바탕, mantle·base의 hover·press | `text_muted_raised()` · `accent_primary_raised()` · `glyph_dim_raised()` |
| `-strong` | surface-raised의 hover·press처럼 불투명 틴트가 깔린 바탕 | `accent_danger_strong()` |
| `text_on_active()` · `-on-active` | surface-active. 글자는 hue 없이 `text_on_active()`, 글리프는 3:1의 `-on-active` | `accent_primary_on_active()` |
| `text_on_active_hover()` | surface-active의 hover·press | 선택 행 라벨과 모양 글리프 |

자체 채움이 있는 자식은 그 채움이 바탕이고, 채움이 없는 자식은 호스트의 현재 바탕을 읽는다. 대비 시험은 내장 Mocha와 Latte만 잰다. 사용자 테마에는 제한·경고·보정이 없다.

메뉴 행 글자는 이 규칙을 역할로 반영한다. `menu_item_fg()`는 semantic `text-secondary-raised`를 가리키며, 이 역할은 Mocha에서 neutral-1000(subtext1), Latte에서 neutral-1100(text, surface0 위 5.17:1)이다. hover 글자 `menu_item_fg_hover()`는 `text_on_active()`이고 hover는 `menu_item_bg_hover()` 채움으로도 구분한다.

### 상태별 대비 기준

글자는 hover와 눌림을 포함해 4.5:1, 글리프·컨트롤 테두리·포커스 링·스크롤바 thumb 같은 비텍스트는 3:1을 지킨다. disabled, 장식, 구획선은 목표가 없다. 값은 채널별 half-up 반올림으로 합성한 뒤 8비트 색으로 잰다. 반투명 overlay의 알파도 n/255로 반올림한다.

앱의 화면 합성도 같은 축이다. 표면 포맷은 sRGB가 아닌 포맷을 고르고(`src/gfx/gpu.rs`), egui는 premultiplied `Color32`를 감마 공간에서 source-over로 합성해 8비트로 저장한다. 그래서 `to_egui()`의 straight 알파와 `to_egui_premultiplied()`의 overlay 모두 sRGB 바이트 합성으로 재면 된다. 불투명하게 미리 합성해야 하는 선택 행 hover·press 채움은 `RowState::fill()`이 같은 계산을 한다.

행의 상태는 `RowState`(`tasty-ui-widgets`)로 고른다. 우선순위는 disabled, 선택+눌림, 선택+hover, 선택, 눌림, hover, 평상이다. 채움과 라벨·메타·글리프 잉크를 한 값에서 읽으므로 호출자가 선택 분기를 따로 만들지 않는다.

선택·키보드 활성 행의 채움 `surface_active()`는 Latte에서 surface2(`#acb0be`)이고 text-primary도 3.69로 미달한다. 어떤 채움도 세 글자 단계를 함께 통과시키지 못하므로 그 위의 글자는 secondary·muted를 구분하지 않고 `text_on_active()` 하나로 합친다(Mocha 4.62, Latte 5.40). 글리프는 무엇이 뜻을 나르는지에 따라 나눈다. 색이 뜻을 나르면(즐겨찾기 별, plugin, 원격, 상태 점, 점유 고리, 이미지 파일, 깨진 링크) 자기 `-on-active` 역할로 색을 유지하고 3:1을 지킨다. 모양이 뜻을 나르면(폴더·파일, 꺼진 별) 행 잉크다. 선택 표지와 일반 강조 글리프는 `accent_primary_on_active()`(Mocha 4.17, Latte 4.20)다. 상태·출처 칩은 쉬는 행과 선택 행 모두 Tag 조립을 쓴다. 자체 채움이 있어 행 상태와 무관하게 대비를 지키므로 틴트 pill을 쓰지 않는다. 어떤 accent도 이 채움 위에서 4.5:1을 넘으면서 행 글자색과 달라지지 못하므로 일치 구간은 행 글자색에 1px 밑줄로 구분한다. 밑줄 위치는 egui 글자 서식을 따른다. 위젯별 토큰은 [디자인 토큰 매핑](design-token-mapping.md#채움이-정해진-자리의-글자-역할)에 있다.

- 일반 행은 호출자가 선택 분기에서 이 역할을 읽는다. 자체 채움이 있는 Tag·Badge·키캡의 글자는 바꾸지 않는다.
- 공용 `Table`은 표 기본 글자색을 선택 행에서 `table_row_fg_selected()`로 바꾼다. 색을 명시하는 셀은 기본 글자색을 받지 못하므로 `table_cell_ink()`나 `in_selected_table_row()`를 읽는다. `status_dot`과 키캡 `+`는 공용 표의 선택 행을 스스로 알아챈다. 표 밖의 행은 `Ground::Active`를 넘긴다.
- `kbd`·`kbd_parts`·`kbd_parts_at`·`status_dot`·`checkbox`·`tag_disabled`·`disabled_chip_scope`는 호스트 바탕 `Ground`를 받아 그 칸의 행렬 값을 읽는다. 체크박스 테두리는 Panel `checkbox-border`, Raised `border-control-raised`, Active `text-on-active`이고, disabled 테두리는 Panel `state-disabled-border`, Raised `border-strong`, Active `border-frame`이다. 유휴·대기 점은 Raised에서 `glyph-dim-raised`·`accent-warning-raised`로 바뀐다. 갤러리의 바탕 범위 CSS는 앱에 옮기지 않고 칸 값을 직접 읽는다.
- `surface_active()`를 칠하기만 하는 자리(스위치·슬라이더 트랙, 튜토리얼 진행 점, 뱃지 점)와 hover·눌림 일시 채움(preset 편집기 핸들, 누르는 동안의 홀드 버튼)은 이 규칙의 대상이 아니다.

**새 UI 를 그릴 때는 이 표를 근거로 잉크를 고른다** — muted 캡션을 얹을 배경은 `base`/`mantle`/`#ffffff` 로 한정하고, `surface0` 위에는 `text_primary` 나 `-raised` 역할을, `surface_active()` 위에는 `text_on_active()` 를 쓴다.

### disabled 잉크 — 대비 목표 없음

비활성 라벨·글리프(`text_disabled()`와 이를 가리키는 component 역할)에는 대비 목표를 두지 않는다. WCAG 1.4.3과 1.4.11은 비활성 UI 구성요소를 대비 요구에서 제외한다. 대신 두 규칙을 지킨다.

- **순서**: 잉크 강도는 `text_disabled()` < `text_placeholder()` ≤ `text_muted()`다. 잉크 단계가 라벨의 유일한 disabled 신호이므로 disabled 라벨에 opacity를 추가로 곱하지 않는다. 라벨과 글리프는 같은 disabled 잉크를 쓴다.
- **패리티**: Mocha의 text-disabled는 neutral-700(overlay1), Latte의 text-disabled는 neutral-800(overlay2)이다. Latte 램프의 밝은 끝은 neutral-600~800이 서로 가까워 같은 단계가 Mocha보다 흐리게 읽히므로 한 단계 위를 쓴다. 그래도 text-muted(neutral-900)보다는 약해 순서 규칙이 유지된다.

### disabled 컨트롤 — 중립 박스와 잉크

disabled 컨트롤은 opacity로 흐리게 그리지 않는다. 변형과 관계없이 같은 중립 박스와 disabled 잉크로 그린다.

- **박스**: 채움 `state_disabled_fill()`(= surface-raised), 테두리 `state_disabled_border()`(= border-default). Button의 모든 변형(Ghost는 박스 없음), solid IconButton, Input, Select·MultiSelect 트리거, Checkbox 박스, Switch 트랙이 이 박스를 쓴다. accent 채움은 disabled에서 빠진다. Button은 component 역할 `button_disabled_bg/border/fg()`로 읽는다.
- **Tag·Badge**: disabled Tag는 `tag_disabled_bg/border/fg()`(= state-disabled fill·border·fg), disabled Badge는 `badge_disabled_bg/fg()`를 쓴다. variant와 관계없이 accent 채움과 40% tint 테두리가 빠지고 Tag의 상태 점도 같은 잉크다. 조건을 알리는 문구("not installed", "off", "removed", "reverts on save", 대기 라벨)는 비활성 컨트롤이 아니므로 기본 Tag로 그린다. `tasty_ui_widgets::disabled_chip_scope` 안의 `tag`·`badge`는 호출부를 바꾸지 않아도 이 변형으로 그린다.
- **잉크**: 라벨·아이콘·체크·thumb·placeholder·addon·단축키 표시는 모두 `state_disabled_fg()`(= text-disabled)를 쓴다. MenuItem·ListCtrl·IconButton(ghost)은 박스 없이 잉크만 바꾼다. disabled ListCtrl 행은 chevron을 숨기고, trailing 표지(Tag·Badge)는 숨기지 않고 disabled 변형으로 그린다. 행은 egui 비활성 Ui(fade)를 쓰지 않고 trailing을 `disabled_chip_scope` 안에서 호출한다. TreeRow는 disabled 상태가 없다. 열 수 없는 항목(읽기 권한 없는 폴더, 연결이 끊긴 원격 즐겨찾기)도 일반 행으로 선택·펼치기를 받고, 열었을 때 탐색기 본문이 이유를 알린다. 그래서 `tree_row`에는 `enabled` 인자가 없다. 잘라내기 대기 항목은 disabled가 아니라 dim 항목이다.
- **상호작용**: hover·active 오버레이를 그리지 않는다. egui의 `add_enabled(false)`/`ui.disable()`은 도형을 fade 색 쪽으로 섞어 박스 색을 바꾸므로, 공용 위젯은 hover만 감지하는 sense(Button)나 `TextEdit::interactive(false)`(Input)로 입력을 막는다.
- **공용 위젯 밖의 자리**: 번들 플러그인처럼 공용 위젯을 쓰지 않는 컨트롤도 같은 박스와 잉크를 쓴다. markdown 플러그인의 WebView 검색 버튼은 테마 CSS의 `--md-disabled-fill`·`--md-disabled-border`·`--md-disabled-fg`로 `:disabled` 상태를 그리며 opacity를 쓰지 않는다.
- **읽기 전용 Input과 구분**: 지금은 고칠 수 없지만 값 자체가 정보인 칸(예: Appearance 색 행이 Default일 때의 기본 hex, 글꼴 override 행이 기본값일 때의 적용 값)은 disabled가 아니라 읽기 전용이다. `Input::read_only(true)`는 같은 중립 상자(`input_readonly_bg/border()` = state-disabled fill·border)에 값을 `input_readonly_fg()`(= text-secondary-raised)로 그린다. 값은 정보이므로 일반 글자 대비 4.5:1 이상이어야 한다. Latte의 text-secondary는 이 상자(surface0)에서 4.05:1이라 raised 역할을 쓰며, Mocha 7.10:1 · Latte 5.17:1이다. Latte에서는 편집 중 글자와 색이 같고, 편집은 focus 테두리·ring·캐럿으로 구분한다. 파일 피커 폴더 모드의 경로(`fp_folder_path_fg`)와 HTML·Markdown 주소의 idle 값(`html_address_value_fg`·`md_address_value_fg`)도 같은 잉크다. 포커스·선택·복사가 되며, 포커스는 1px focus 테두리만 두고 ring은 그리지 않는다. `enabled(false)`와 함께면 읽기 전용은 무시한다.
- **디밍과 구분**: `state_dim_opacity()`(0.5)는 preset 편집 중 비활성 영역(`preset_cfg_dim_opacity()`)처럼 **항목 전체를 물러나게** 할 때만 곱한다. disabled 컨트롤 신호로 쓰지 않으며, 사용자가 읽는 글자에도 쓰지 않는다. 꺼진 hook 행, 비활성 plugin 행, 다른 클라이언트가 쓰는 워크스페이스, 새 워크스페이스를 만드는 동안의 기존 목록은 이름을 `text-muted`로 그린다. 입력을 막을 때는 egui 비활성 Ui를 겹치지 않고 행의 sense로 막는다.
- **바쁜 버튼**: 버튼의 라벨과 폭은 바뀌지 않는다. 비활성 상태로 원래 라벨을 두고, 스피너와 진행 문구는 버튼 옆 caption `text-muted`로 그린다(`tasty_ui_widgets::busy_caption`).

갤러리 Components 페이지의 Spec "Disabled — ink, never opacity"가 Mocha·Latte에서 다섯 변형과 IconButton·Checkbox·Switch의 enabled/disabled를 나란히 보여 준다.

활성 상태의 컨트롤 글리프는 이 예외에 들지 않는다. 탭 스트립의 스크롤할 수 있는 화살표(`tab_scroll_arrow_fg()`)는 1.4.11의 3:1 대상이다. 화살표 칸 바탕 `surface_raised`(본체는 포커스된 pane 의 스트립 배경, 비포커스 pane 은 `bg_sidebar`) 위에서 WCAG 공식으로 계산한 평면색 비율은 다음과 같다.

| 테마 | disabled | placeholder·muted(활성 화살표) |
|---|---|---|
| Mocha (`#313244`) | 3.40 | 5.65 |
| Latte (`#ccd0da`) | 2.56 | 4.05 |

### Host UI zoom

`AppearanceSettings.ui_scale`(`small/medium/large` = `0.85/1.0/1.2`). `install_global_with_runtime`(`ThemeRuntime.ui_zoom`) 이 `Theme::with_colors_and_zoom` 으로 sizing 토큰 자체에 배율을 곱해 전역 `Theme` 재빌드 — UI 호출부에서는 다시 배율을 곱하지 않는다(`theme().spacing_*`에 이미 적용됨).

- **zoom 받음**: `spacing_*` · `font_size_*` · `corner_radius`(`_sm`/`_lg` 포함) · `focus_ring_width` · `item_height_*` · 사이드바 sizing 토큰들.
- **zoom 제외**: hairline(`border_width` 1px 정책 · `icon_stroke_width` — 이 굵기를 쓰는 타이틀바 버튼 기하가 고정 px 라 선만 굵어지면 글리프 형태가 달라진다 · `selection_edge_width` — 탭 밑줄 `tab_indicator_width()` 는 별도 필드 없이 이 필드를 읽는다 · `tint_edge_width` · 1px 점선 테두리의 무늬 `border_dash`·`border_dash_gap`(4 / 4) · 접힌 레일 상태 점의 배경 고리 `status_dot_ring_width` 1.5px) · 탭바 토큰(`tab_width`/`tab_bar_*`) · 상태바 토큰(`status_bar_height`) · CSD 타이틀바 토큰 · 렌더 콘텐츠 폰트(터미널 `font_size_term_*` 는 별도 `effective_terminal_font` 경로로 GPU 셰이더에 전달, markdown `font_size_prose_h1`).
  이 목록은 **요약이고 정본이 아니다** — 정본은 `crates/tasty-type-appearance` 의 zoom 면제 가드가 든 이름 집합이며, 소스와 이름 단위로 대조된다. 필드를 새로 면제하려면 그 목록에 사유 갈래와 함께 등록해야 하고, 등록 없이 `zoomed()` 를 빼면 그 검사가 해당 필드 이름을 표시하며 실패한다. 각 필드의 사유는 필드 doc 에도 붙어 있다.
- **4px 그리드 + zoom**: 비정수(`12×1.2=14.4`)는 `round_ui()`/`f32::round()` 로 정수 픽셀로 반올림.
- **라이브 갱신**: settings save / IPC update 는 `App::apply_settings_after_structure` 에서 새 설정의 전역 Theme 를 먼저 설치한다. 테마 ID·`theme_base`·`theme_overrides`·`theme_is_light`·`ui_scale` 중 하나가 바뀌었으면 모든 창의 `GpuState::refresh_theme` 와 모든 engine 의 `resync_terminal_palettes` 를 1회 호출한다(polling 아님). 그다음 같은 조건에서 plugin 에 `theme.changed` 를 한 번 발행한다. 이 판정은 `theme_order::appearance_changed` 하나이며 창 갱신과 발행이 함께 쓴다. `accessibility.reduced_motion` 은 색이 아니고 `theme.query` 응답에도 없어 판정에 넣지 않는다. 설치를 발행보다 먼저 하는 순서는 `src/app/dispatch_domain/theme_order.rs` 의 `install_theme_then` 이 정한다. 그 파일의 시험은 헬퍼 안의 순서를, 소스 가드 `src/source_guards/theme_changed_after_install.rs` 는 `src/` 의 `theme.changed` 발행과 `announce_settings_change` 호출이 그 헬퍼의 클로저 인자 안에만 있고, 발행이 `if appearance_changed {` 블록 안에 있다는 텍스트 형태를 확인한다. 가드는 클로저를 변수로 넘기는 형태, 상수·매크로로 감춘 키, `crates/` 의 발행, 클로저의 실제 실행 여부를 판정하지 않는다. 이 순서는 구독자가 수신 후 `theme.query` 로 새 테마를 읽는 계약([markdown Theme parity](../../surfaces/markdown/index.md))의 전제다.
- **불변식 — `set_theme`/`install_global*` 은 렌더 밖에서만**: 전역 `THEME` 는 std `RwLock`(재진입 불가)이라, egui 렌더 클로저는 `theme()`(=`THEME.read()`) read guard 를 보유한다. 렌더 도중 `set_theme`(=`THEME.write()`)을 호출하면 자기 read guard 때문에 자기 읽기 잠금을 기다리는 교착 상태가 된다. 따라서 테마 install 은 항상 인텐트 dispatch(`about_to_wait` / cascade) 단계에서만 수행하고, 렌더 핸들러(설정 모달 Save 등)는 `UpdateSettings` 인텐트만 큐잉한다(install 직접 호출 금지).

## 코드 위치

- schema: `crates/tasty-type-appearance/src/{color,theme}.rs`
- 전역/IO: `crates/tasty-themes/`(`theme()`, `apply_theme`, `resolve`, `install_global[_with_runtime]`, mocha/latte 임베드)
- settings: `crates/tasty-settings/src/appearance.rs`
- 부팅: `src/app/window_lifecycle.rs::boot_apply_theme`

## 떠 있는 표면의 그림자

그림자는 표면의 형태에 따라 고른다. scrim 유무만으로 modal 그림자를 고르지 않는다. scrim은 배경을 어둡게 하지만 카드 경계를 그리지 않으므로, 화면 중앙의 카드에는 더 큰 modal 그림자를 사용한다.

| 형태 | 토큰 | 현재 예 |
|---|---|---|
| 콘텐츠 위에 붙는 anchored·scrim 없는 표면 | `shadow_popover()` | 배너, tooltip, 자동완성·Select 메뉴, modifier hint, tutorial callout, tools menu·rail category·배너 더보기·search bar |
| 화면 중앙을 차지하는 표면 | `shadow_modal()` | host·plugin popup 셸, 부팅 셸 설정, 부팅 오류 카드 |
| 위 둘에 속하지 않음 | 없음 | 사용자가 옮기는 창 형태의 알림 패널 |
| 불투명 모드 WebView 페이지와 겹친 팝업·드롭다운 상자 | `shadow_none()` | [WebView 위 팝업](webview.md#egui-오버레이와-native-webview)의 불투명 모드. 모서리도 `webview_opaque_radius()`(0)로 바꾸고 1px 테두리만 남긴다 |

search bar는 범위 상단 중앙에 있어도 scrim 없이 콘텐츠 위에 놓이므로 popover에 속한다. 좌표를 계산한 방식만으로 종류를 정하지 않는다. 갤러리 공용 셸도 `frame_card`·`frame_card_popover`·`frame_card_flat`으로 같은 구분을 사용한다. Settings 창 셸, image surface의 pane 콘텐츠, 다른 표면 안의 섹션은 flat이다.

셸을 직접 그리는 갤러리 예제도 확인한다. 공용 셸 호출만 검색하면 DAG·remote attach·transfer·file picker·popup frame 등이 빠질 수 있다. popup 대응 목록은 `gallery_specimen_parity.rs`, 앱의 선택 함수는 `popup_shadow`다.

`Shadow {}` 생성은 `ShadowToken::to_egui()`로 모은다. 페이드가 필요하면 변환 결과의 color에만 opacity를 적용하고 offset·blur·spread는 바꾸지 않는다. egui 기본값도 `visuals.popup_shadow`는 popover, `visuals.window_shadow`는 modal에 연결한다. 직접 프레임을 넘긴 호출부는 자신의 shadow 설정을 사용한다.

버튼에 붙는 팝오버(드롭다운·필터·열 선택·숨은 경로 메뉴 등)의 틀은 메뉴 컨테이너 토큰 `menu_bg()`·`menu_border()`·`menu_radius()`와 `shadow_popover()`다. `menu_border()`는 border-strong 이라 채움(surface-raised) 위에서 테두리가 보인다. 본체가 직접 그리는 메뉴형 팝업(Tools 메뉴·레일 카테고리 팝업)도 같은 `menu_border()`를, 검색 바는 `search_bar_border()`(→ menu-border)를 쓴다. 앵커 메뉴 팝업의 안쪽 둘레는 `popup_content_margin()`(→ space-xs)이다. 본체 `popup::content_margin()`은 대화상자형 팝업과 함께 쓰는 여백이라 `spacing_xs`를 직접 읽으며 같은 값이다. egui 는 팝오버 틀과 기본 tooltip(`on_hover_text`) 틀을 같은 `visuals.window_fill`·`window_stroke`·`menu_corner_radius`로 그리되, 팝오버는 부모 `Ui` 스타일에서, tooltip 은 전역 스타일에서 읽는다. 그래서 `apply_theme_to_egui`는 전역 값에 tooltip 토큰 `tooltip_bg()`·`tooltip_border()`·`tooltip_radius()`와 안쪽 여백 `tooltip_padding_x()`·`tooltip_padding_y()`(전역 `spacing.menu_margin`, 가로 8·세로 4)를 넣고, 팝오버를 여는 호출부는 `tasty_egui_theme::with_popover_frame`으로 감싸 그 호출 동안만 부모 스타일을 메뉴 토큰으로 바꾼다. 래퍼는 egui 팝오버의 안쪽 여백(`spacing.menu_margin`, egui 기본 6)도 앵커 메뉴 공통 둘레 `popup_content_margin()`(→ space-xs 4)으로 바꾼다. 안쪽 둘레를 따로 정하는 팝오버(MultiSelect 의 `multiselect_menu_padding()`, SearchableSelect 의 `select_search_inset()`, 원격 도구 프로토콜 필터의 0)는 둘레를 인자로 받는 `with_popover_frame_ring`으로 연다. 이 래퍼는 자식 `Ui`를 만들지 않아 위젯·팝업 ID 가 그대로다. 새 팝오버 호출부도 이 래퍼로 감싼다. 감싸지 않으면 tooltip 틀로 그려진다. 루트 lib 소스 검사 `popover_frame_call_sites`가 `egui::popup_below_widget`·`egui::popup::popup_above_or_below_widget`·`egui::ComboBox::from_id_salt` 호출이 두 래퍼 중 하나의 클로저 첫 식인지 확인한다. 다른 이름으로 줄여 부른 호출과 `Frame::popup`을 직접 그리는 경로는 보지 않는다. 열린 ComboBox 목록의 행은 공용 옵션 행 `tasty_ui_widgets::menu_option`·`menu_option_value`로 그리고 행 사이 간격을 0 으로 둔다. 현재 값은 글자 `menu_item_selected_fg()`와 오른쪽 체크(`menu_item_check_size()`·`menu_item_check_fg()`)로만 구분하며 채움이 없다. 옵션 행은 라벨과 체크 자리를 줄이지 않는 폭만큼 목록의 최소 폭을 넓히므로 목록이 트리거보다 넓어질 수 있다. egui 기본 선택 행(`selectable_value`·`selectable_label`)은 현재 값을 accent 채움으로 칠하므로 쓰지 않는다. 같은 소스 검사가 `ComboBox::from_id_salt`의 `show_ui` 클로저 본문에 egui 기본 선택 행이 있는지 확인한다.

`ShadowToken.spread`는 음수를 표현하지만 egui의 u8 spread로 재현할 수 없다. 음수를 0으로 근사하지 않으며 해당 표면은 미구현으로 둔다. `to_egui()`는 debug에서 음수 spread를 단언으로 거부한다.

검사는 역할이 다르다. `shadow_policy_guard.rs`는 인식하는 리터럴 생성 위치, 같은 줄의 변수 선언에서 찾은 기하 재대입, 접근자 목록을 확인한다. 데이터 흐름 전체·egui 기본 매핑·표면별 선택은 확인하지 않는다. `shadow_parity.rs`는 수기로 옮긴 세 토큰(popover·modal·none)의 값과 vendor JSON, raw·alias 목록, 그림자가 아닌 kbd-shadow-depth 구분을 대조한다. 음수 spread 사용 검사도 토큰 목록 완전성 검사와 함께 유지한다. 실행 범위는 [CI 가이드](../../dev-guide/ci-gates.md)를 따른다.

## 스크롤 여지를 보여 주는 방법

새로 만들거나 수정하는 영역은 스크롤바를 숨기고, 더 스크롤할 수 있는 가장자리에 배경색에서 투명으로 이어지는 페이드를 그린다. 기준 구현은 `remote_tool.rs`의 `scroll_list_with_fade`다. 기존 영역의 일괄 전환을 요구하지는 않는다.

스크롤바 폭을 예약하는 예외는 두 조건을 모두 만족해야 한다. 그 폭이 콘텐츠 치수·가로 스크롤 판단에 쓰이고, 컨테이너가 바 표시 설정이나 뷰포트 rect를 제공하지 않아야 한다. 현재 port scanner는 Exact 열 너비를 가용폭에 맞추고 내부 `TableBuilder`의 세로 스크롤을 사용하므로 이 예외에 해당한다. 공용 Table API가 바뀌면 다시 판단한다.

페이드에는 스크롤 위치와 전체 분량 정보가 없다. 긴 목록에서 그 정보가 필요해지면 별도 표시를 설계한다. 클릭이 통과하는 스크롤바나 임의 여백 추가로 대체하지 않는다.

드래그 패닝은 별도 규칙이다. `ScrollArea`, `TableBuilder`, 스크롤 축을 켠 `egui::Window`에는 `drag_to_scroll`을 명시한다. 기본은 `false`이며 예외로 켤 때 이유를 코드에 남긴다. `ComboBox` 내부 스크롤은 호출부에 설정 API가 없어 현재 검사 대상이 아니다.

`drag_to_scroll_is_declared.rs`는 생성 이후 40줄 안의 빌더 호출을 보는 소스 검사다. 데이터 흐름을 추적하지 않아 호출이 멀면 놓치거나 무관한 호출을 잘못 연결할 수 있다. 선언 여부를 검사하는 것이므로 true가 허용된다는 사실과 제품 기본값이 false라는 사실을 구분한다.

## 모션 설정과 시간 단위

모션 위젯은 기본적으로 `Theme.reduced_motion`을 따른다. 설정값은 `Settings::theme_runtime()`가 UI 배율과 함께 만들고, 전역 Theme 설치 경로는 그 결과를 통째로 전달한다. 개별 위젯 override는 갤러리에서 동작·정지 예제를 함께 보여 줄 때 사용한다.

`Theme::with_colors*`로 직접 만든 Theme는 기본값이므로 호스트 설정이 자동으로 들어간다고 가정하지 않는다. 현재 `ThemeWire`는 `colors`·`is_light`·`ui_zoom`만 전달하며 `reduced_motion`은 전달하지 않는다. 따라서 플러그인의 egui-mesh 모션 위젯에 호스트의 모션 감소 설정이 자동 적용되지는 않는다. 해당 위젯을 추가할 때는 이 설정을 전달하는 방법도 함께 정해야 한다. OS의 모션 감소 설정은 현재 설정값을 대신하지 않는다.

component duration 접근자는 `Millis`를 반환한다. egui에는 `to_secs_f32()` 또는 `to_secs_f64()`, 타이머에는 `to_duration()`을 사용한다. 원시 밀리초가 필요한 함수에는 `to_millis_f32()`로 전달한다. 단위가 없는 `value()` 접근자는 없으며, duration에는 UI 배율을 적용하지 않는다. semantic duration은 현재 별도의 Theme 필드로 저장하지 않는다.

대응 토큰이 없는 시간값은 임의로 가까운 값에 맞추지 않는다. 원시 ms를 꺼내 계산하는 경로가 늘면 단위 오용 검사를 추가할 필요가 있는지 검토한다. 이 규칙은 디자인 모션의 Theme 경계에 적용하며 기존 Duration 기반 폴링·타임아웃을 바꾸지는 않는다.

## 터미널 셀 강조색 합성

선택·vi 커서·링크·검색 강조는 이 순서로 현재 셀 배경 위에 합성한다. `renderer/overlay.rs::composite_over`가 source-over 색을 계산하며 `fill_surface`와 `render_cell` 모두 이를 사용한다. 새 강조를 추가할 때 두 경로를 확인한다.

불투명 강조색은 원래 값과 같고, 불투명 셀 배경 위에 반투명 강조를 합성해도 결과는 불투명하다. alpha는 창 아래를 비추는 값이 아니다. 셀 기본색·SGR 배경·block 커서는 이 강조 합성의 대상이 아니며, 별도 quad인 IME preedit도 포함하지 않는다. GPU 셀 배경 파이프라인은 REPLACE를 유지한다.
