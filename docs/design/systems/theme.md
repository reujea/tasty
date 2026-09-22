# 테마 시스템 (운영 상세)

색상·타이포·간격의 단일 출처인 `Theme` 와 그 위의 **UI 디자인 규칙**. 모든 UI 는 색/크기/간격을 `Theme` 에서 가져온다(하드코딩 금지).

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
              theme().crust / theme().spacing_sm / theme().is_light   ← UI 코드
```

### 두 레이어

| 레이어 | 타입 | 의미 | 테마 변경 시 |
|--------|------|------|--------------|
| `theme_base` | `ThemeColors`(풀 세트) | 적용된 테마 파일의 누적 결과 (**앱 소유**) | partial 덮어쓰기로 누적 |
| `theme_overrides` | `PartialColors`(모든 필드 `Option`) | 사용자가 손댄 색 (**사용자 커스터마이징의 단일 출처**) | **클리어(의도된 설계)** |

화면 적용 색 = `theme_base ▷ theme_overrides`(override 의 `Some` 필드만 덮어쓰기). partial 테마(일부 색만 정의)를 적용하면 누락 필드는 이전 base 값을 유지한다(누적).

#### 커스터마이징 모델 (불변)

- **`theme_base` 는 앱 소유다.** 테마 파일(`<id>.toml`)의 내용은 앱이 관리하며, 사용자가 직접 편집하는 경로가 아니다(빌트인은 부팅 시 임베드 정본으로 동기화됨 → "빌트인 테마 정책" 참고).
- **사용자 색 변경은 오직 `theme_overrides` 로만 들어간다.** settings 가 보관하는 partial 레이어로, base 위에 resolve 시점에 얹힌다. base(파일)를 어떻게 바꾸거나 동기화해도 사용자 override 는 보존된다 — 두 레이어가 분리돼 충돌이 없다.
- **override 를 기록하는 정식 경로 = Settings › Appearance › Colors picker.** 픽커는 flat `PartialColors` 46색(Surfaces·Overlays·Text·Accents·Terminal-specific·ANSI 16) 을 그룹별 collapsible 로 노출한다. 각 행의 "Default" 체크 = 그 필드 `None`(프리셋 base 추종), 해제 = `Some(hex)`. base 값은 resolved `theme_base` 에서 읽어 시드한다(하드코딩 없음). 행/그룹/전체 3단계 reset 으로 `None` 복귀. 저장 시 `theme_overrides` 변화가 감지되면 `AppearanceChanged` 가 발화돼 전 윈도우에 라이브 반영된다. `surface_themes`(맵 구조)는 이 flat 픽커에서 분리돼 `Tasty`/`Terminal` 섹션의 curated shortcut 으로 남되 같은 `theme_overrides` 에 기록된다.
- **테마를 바꾸면 `theme_overrides` 를 비운다(설계).** `apply_theme` 의 `theme_overrides.clear()` 는 부수효과가 아니라 의도다 — 테마 전환 = 그 테마의 색을 깨끗하게 적용하고 이전 테마에 얹어둔 사용자 변경분은 폐기한다. 픽커가 채운 override 도 함께 비워진다.

### Crate 책임

| crate | 책임 | IO |
|-------|------|----|
| `tasty-type-appearance::color` | `HexColor`, `GpuRgba`/`GpuRgb` newtype | 없음 |
| `tasty-type-appearance::theme` | `Theme` · `ThemeColors` · `PartialColors` · `ThemeSizing`/`SIZING` · `SurfaceTheme`/`FALLBACK_SURFACE` · `derive_overlays` · `Theme::surface(id)` | 없음 |
| `tasty-themes` | 전역 `RwLock<Theme>` + `theme()/set_theme()` · `ThemeFile`(TOML) · mocha/latte 임베드 · scan/load/apply/resolve/install · `first_run_init`/`sync_builtin_themes` | `~/.tasty/themes/` |
| `tasty-settings::appearance` | `AppearanceSettings.{theme,theme_base,theme_overrides,theme_is_light,ui_scale}` | settings IO |
| `tasty-design-tokens` | 디자인 DTCG export vendor(`dtcg/tasty.tokens.json`, 492 토큰) + 치수 const 생성(`crates/tasty-design-tokens/src/generated/` — primitive 는 `pub(crate)` 로 3-tier 규율 강제) + **component tier 접근자 생성**(`tasty-type-appearance/src/generated_component.rs` 로 산출 — `&Theme` 경유 치수·색 접근자, 아래 "Component tier 접근자") + freshness/`SIZING` 정합/mocha·latte 색 드리프트 가드 테스트. 생성 const 는 초기값·정합용 — 런타임 소비는 `&Theme` 경유(zoom 우회 금지). vendor 갱신 절차는 crate README | 없음 |

의존: `type-geometry ← type-appearance ← tasty-themes ← tasty-settings`. 순환 없음 — `tasty-core` 는 시각 schema 를 모른다(GUI-free). `tasty-design-tokens` 는 `type-geometry` 만 런타임 의존(정합 테스트만 dev-deps 로 type-appearance/themes 참조) — 본체·egui 미의존.

## 빌트인 테마 정책

빌트인 테마 파일은 **앱 소유**다. 부팅 시 `sync_builtin_themes()` 가 디스크 복사본을 임베드 정본과 맞춘다 — 빌트인 색/스키마가 바뀌면 이미 풀려있던 옛 파일도 자동 갱신된다. 사용자 색 변경은 파일이 아니라 `theme_overrides` 에 있으므로 동기화가 사용자 커스터마이징을 덮어쓰지 않는다.

- **mocha**: 항상 정본 보장. 임베드 `MOCHA_TOML_TEXT` + `MOCHA_FALLBACK_COLORS` const. 부팅 시 sync 가 누락/파싱 실패/**내용 불일치** 면 임베드로 덮어쓴다. 로드 실패해도 const 가 fallback. unit test 가 `parse(MOCHA_TOML_TEXT) == MOCHA_FALLBACK_COLORS` 강제.
- **latte**: first-run(themes 폴더가 완전히 빈 경우)에 자동 풀림. 이후엔 **파일이 있으면 임베드와 동기화**, 사용자가 지우면 존중하고 다시 풀지 않음(fallback 없음).
  - `subtext0` 은 upstream catppuccin latte(`#6c6f85`)가 아니라 `#63667c` 다 — upstream 값은 `base`(`#eff1f5`) 위에서 4.37:1 이라 아래 "텍스트 대비" 4.5:1 규칙을 못 넘긴다. subtext0→subtext1 램프 위에서 내려 `base` 4.99:1 / `mantle` 4.64:1 을 확보한 값이다. mocha 는 같은 토큰이 이미 7.37:1 이라 손대지 않았다. 남은 미달 조합은 아래 "latte 중성 램프 대비 — 알려진 예외" 참고.
- **사용자 테마**: 자동 동기화/복구 없음. 로드 실패 시 mocha fallback.
- **"사용자 의도 존중" 의 범위 = 파일 삭제(부재)뿐.** 빌트인 파일의 *내용* 은 존중 대상이 아니다 — 사용자가 손으로 고쳐도 다음 부팅에 정본으로 되돌아간다(편집 경로가 아님, 커스터마이징은 `theme_overrides`).

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

- `[surfaces.<id>]` 의 `<id>` 는 자유 문자열 — plugin 이 등록한 surface kind 도 정의 가능. `theme().surface(id)` 가 없는 id 엔 `FALLBACK_SURFACE`(검은 배경 + Mocha 톤) 반환 → plugin 이 색을 안 줘도 안전.
- `hover_overlay`/`active_overlay`/`separator` 같은 반투명 의미색은 TOML 에 없다 — `is_light` 로부터 자동 도출(라이트=검정 +8%/+12%, 다크=흰색 +8%/+12%).
- UI 크기/간격(`spacing_*`/`border_width`/`item_height_*`/`font_size_*`)도 TOML 에 없다 — 모든 테마 공통 `SIZING` const.
- **HexColor**: `#RGB` / `#RRGGBB`(alpha=255) / `#RRGGBBAA`. 직렬화는 alpha=255 면 6자리, 아니면 8자리.
- `[terminal]` 강조색(`selection_bg` · `vi_cursor_bg` · `search_match_bg` · `search_match_active_bg`)의 alpha 는 **그 셀의 배경 위에 합성**된다 — 창 아래가 비치는 것이 아니다([ADR-0460](../../adr/0460-cell-highlight-alpha-is-composited-on-the-cpu-over-the-cell-bg.md)).
- **빌트인(`mocha`/`latte`) 파일은 직접 편집하지 말 것** — 앱 소유라 부팅 시 임베드 정본으로 되돌아간다. 커스텀 테마는 별도 id(예: `my-theme.toml`)로 만들고, 기존 테마 위 색 조정은 settings 의 `theme_overrides` 로 한다.

## 부팅 흐름 (`window_lifecycle.rs::boot_apply_theme`)

`first_run_init()`(빈 폴더면 mocha+latte 시드) → `sync_builtin_themes()`(빌트인을 임베드 정본과 동기화) → `rescan()` → `apply_theme(요청 id)`(실패 시 mocha) → `install_global(resolve())`. 요청 id ≠ 적용 id 면 InfoModal 로 알림.

## UI 코드의 색상 접근

```rust
let th = crate::theme::theme();                       // = tasty_themes::theme()
ui.painter().rect_filled(rect, 0.0, th.blue);         // HexColor → Color32
let bg  = th.terminal_bg.to_float();                  // GPU 셰이더
let pad = th.spacing_sm;                               // sizing 동일 방식
// ❌ egui::Color32::from_rgb(80,140,255)             // 하드코딩 금지 (clippy 차단)
```

- **색 생성 경로 단일화**: GPU 버퍼 struct 는 newtype(`GpuRgba` 등)을 받아 `[f32;4]` 대입이 컴파일 에러. `from_rgb` 직접 호출은 clippy 차단. 상세 [`dev-guide/color-policy`](../../dev-guide/color-policy.md).
- **premultiplied 주의**: `hover_overlay`/`active_overlay`/`separator` 는 premultiplied 바이트라 `to_egui_premultiplied()` 를 써야 한다. `to_egui()` 를 쓰면 sRGB-aware premultiplication 이 한 번 더 적용돼 색이 어긋난다.
- **Semantic 접근자 우선**: 평면 primitive(`th.blue`) 외에 의미 기반 접근자(`accent_primary()`/`surface_raised()`/`text_muted()`)를 제공. 신규/수정 UI 는 의미가 드러나는 접근자를 우선(같은 primitive 가 여러 role 로 갈리는 다의성 표현). **host UI 계층(`src/view`·`src/adapters/ui`·shell_setup) + 위젯 크레이트(`tasty-ui-widgets`)는 전수 이식 완료 → `crates/tasty-doc-guards/tests/design_token_adherence.rs` 가 `th.<primitive>`/`theme.<primitive>` 직접접근을 금지**한다(`doc-guards.yml` 이 main push · PR 마다 **경로 필터 없이** 자동으로 돌린다 — 그 크레이트로 옮겨 오면서 두 조합 모두에서 도는 자리가 됐다, [ci-gates](../../dev-guide/ci-gates.md))(semantic 접근자로 강제 유도). 위젯도 primitive 절대 불가 — 근거·제외 목록(테마 내부·픽커·ANSI·갤러리 팔레트 데모)·집행 채널(clippy 불가·가드 테스트 전담)은 [ADR-0033](../../adr/0033-ui-color-semantic-role-only.md). 대응 role 부재 use 는 primitive 로 되돌리지 말고 가장 가까운 role 로 alias + `// divergence:` 주석. 매핑·다의성 핫스팟은 [`token-crosswalk`](token-crosswalk.md).
- **잉크 role 은 값이 아니라 의미로 고른다** (2026-09-17 결정): **disabled 는 고유 잉크**다 — 모든 disabled 라벨·글리프는 `text_disabled()`(neutral-700)를 읽고, `text_placeholder()`(입력 전 안내)도 border role 도 쓰지 않는다. **`glyph_dim()`**(neutral-600)은 *물러나야 하는 chrome* 전용이지 disabled 용이 아니다(사이드바 dim chevron·dim 아이콘·상태바 글리프). **`border_frame()`**(neutral-500)은 틀의 선이고 `surface_active()`(선택된 표면)와 값만 같다. **그 선은 넷이다** — popup 프레임·titlebar 아래 선·pane divider·GPU 비활성 보더. popup **내부** 구분선은 이 role 이 아니라 `border_strong()`(neutral-400)에 남는다 — 틀과 칸막이는 다른 역할이라, 한 role 로 묶으면 틀만 올리는 것이 불가능해진다. 단계는 **명도가 아니라 대비로** 고른다: latte 에서는 neutral-500 이 neutral-400 보다 *어두워서* 명도 부등호가 두 테마에서 뒤집힌다([ADR-0290](../../adr/0290-settled-role-gaps-close-the-divergence-and-off-scale-sets.md), 값은 `crates/tasty-design-tokens/tests/color_drift.rs` 의 `the_frame_line_outranks_the_strong_border_by_contrast_not_lightness` 가 고정한다). **`accent_decorative()`**(peach)는 헤더 장식이고 `accent_attention()`(주의 환기)과 값만 같다.
- **tinted 채움/테두리는 한 짝이다**: accent 를 옅게 깔고 같은 accent 로 두르는 관용구는 **채움 `tint_fill_alpha()`(0.12) / 테두리 `tint_border_alpha()`(0.36)** 한 짝만 쓴다. 승인된 부분 사용 둘 — **채움만**(warning callout · misc/remote/script 배지)과 **테두리만**(chip remote 태그). 은퇴한 짝: 0.14/0.45 · 0.12/0.35 · 0.11/0.36.
- **Semantic 색 접근자 생성**: bg-*/surface-*/text-*(placeholder 까지)/accent-*/border-* 의 **단순 primitive 필드 alias** semantic 색 접근자는 `crates/tasty-type-appearance/src/semantic_color_generated.rs` 의 **생성된 `&Theme` 메서드**(`tasty-design-tokens` 생성기 산출, `DO NOT EDIT`) 로 노출된다 — DTCG semantic 색 토큰이 SSoT. is_light 분기(`text_on_accent()`)·derive_overlays 도출(`overlay_hover()`/`overlay_active()`)·합성색(`scrim()`)·OS/brand 리터럴 접근자만 `theme.rs` 에 수기로 남는다. component 색 접근자(아래)가 이 semantic 접근자를 호출하므로 inherent method 이름은 불변.

## Component tier 접근자

DTCG component tier(치수+색) 토큰은 `crates/tasty-type-appearance/src/generated_component.rs` 의 **생성된 `&Theme` 메서드**로 노출된다 (`tasty-design-tokens` 생성기가 산출, `DO NOT EDIT`). `generated::component` 의 raw const 를 위젯이 직접 읽으면 `with_colors_and_zoom` 의 zoom resolve/제외 정책을 우회하므로, 위젯은 **반드시 이 접근자를 경유**한다.

- **치수 접근자**(→ `LogicalPx`) 3형태: alias 체인이 (a) zoom 정책이 이미 박힌 `Theme` 필드에 닿으면 그 필드 반환, (b) 다른 component 접근자에 닿으면 그 접근자 호출, (c) primitive 에 직접 닿으면 `Theme.ui_zoom` 을 곱해 계산(`LogicalPx((v*ui_zoom).round())`). 예: `button_gap()`=`spacing_sm`, `button_height_lg()`=`(32*ui_zoom).round()`.
- **색 접근자**(→ `HexColor`): semantic 접근자 체인 또는 component→component 상호 호출. 예: `button_primary_bg()`=`accent_primary()`. `banner_*`/`titlebar_*` 색은 기존 수기 접근자와 이름 충돌이라 생성 제외(수기 유지).
- **소비처**: `tasty-ui-widgets` 위젯(button/chip/input/menu_item/select/toggle/tree_row/icon_button/status_dot/table 등)이 이 접근자를 소비. host chrome(sidebar/titlebar, `src/adapters/ui/`)은 후속 시리즈에서 전환.
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

테마 위에 얹히는 **시각 정책**(=토큰 축). 새 UI 를 그릴 때 따른다. 디자인을 구현에 정합시키는
작업이라면 이 토큰 축만으로 끝나지 않는다 — 레이아웃을 컴포넌트·소스 단위로 1:1 전사하는 **구조
축**([design-parity-notes.md](design-parity-notes.md) · [design-gallery-mapping.md](design-gallery-mapping.md))을
함께 충족한다(두 축은 독립적으로 어긋날 수 있다).

| 항목 | 규칙 |
|------|------|
| 기본 테마 | Mocha(fallback 보장) + Latte(first-run 자동) |
| 색 팔레트 | Catppuccin Mocha 톤 기준 |
| 간격 | **4px 그리드** — `spacing_xs/sm/md/lg/xl` 만 |
| 간격 API | **`add_space`/`inner_margin`/`Margin::same\|symmetric` 에 숫자 리터럴 직접 전달 금지** — `tasty-ui-widgets` 의 typed 헬퍼(`vspace`/`hspace`/`margin_all`/`margin_sym`)에 `th.spacing_*`(LogicalPx)를 넘긴다. 그리드 밖 미세 구조 간격(1~4px)은 `tasty_ui_widgets::tokens::STRUCT_GAP_1/2/3/4` (DTCG `primitive.size-1/2/3/4` 대응). **`crates/tasty-doc-guards/tests/design_token_adherence.rs` 가드가 인라인 리터럴 재유입을 막는다**(`doc-guards.yml` 이 main push · PR 마다 **경로 필터 없이** 자동으로 돌린다 — 그 크레이트로 옮겨 오면서 두 조합 모두에서 도는 자리가 됐다, [ci-gates](../../dev-guide/ci-gates.md)). 명명 구조 상수(`const NAME: LogicalPx = LogicalPx(N)`, 예: 사이드바 폭·카드 크기·control nudge)는 스코프 밖(권장 해결책이라 금지 아님) |
| UI 폰트 스케일 | **`font_size_micro`(10) · `caption`(11) · `body`(13) · `heading`(13) · `max`(14) 만.** 이 다섯이 UI 스케일 전부이고 `ui_scale` zoom 을 받는다. 역할이 이름에 있으면 component 접근자(`badge_font_size()` · `tag_font_size()` · `kbd_font_size()` 등)를 우선한다. `font_size_term*`/`prose_h1` 은 **콘텐츠** 폰트라 UI 에 쓰지 않는다 |
| UI 폰트 최대 | **14px**(`font_size_max`). **범위는 UI 폰트다** — `font_size_term*`·`prose_h1` 은 위 행이 말하는 대로 **콘텐츠** 폰트라 이 상한의 대상이 아니다. **승인된 예외 하나**: 브랜드 워드마크(부트 락업 30 = `font_size_brand_display` · 사이드바 헤더 17 = `font-size-brand-wordmark`)는 브랜드 자산의 verbatim 전사라 UI 텍스트 스케일 밖이고, 그 승인은 [boot-sequence](../../architecture/boot-sequence.md) 에 적혀 있다. 2026-09-17 결정이 30 에 semantic 이름을 붙였고 **브랜딩 예외는 그 둘이 마지막**이다. **채널**: `src/design_token_guard.rs` 의 `no_named_font_const_exceeds_the_ui_font_size_cap` 이 폰트 자리의 명명 const 값을 본다 — 이 행의 가드는 `src/lib.rs` 아래의 **lib 유닛**이라 `--lib` 가 그것을 담는다(루트 패키지의 단위시험은 전부 lib 타깃에 살아서 `--bins` 만으로는 하나도 안 담긴다). 실행 채널이 **둘**이다: `check-windows` 의 `cargo test --workspace --lib --bins`(기본 feature)와 `check-headless` 의 전체 스위트(`--no-default-features`) 양쪽에서 돈다 — 이 파일이 feature 게이트 뒤가 아니라 두 조합 모두에서 컴파일되기 때문이다. 상한 값은 손으로 옮겨 적지 않고 `SIZING.font_size_max` 를 링크한다 — 복사본을 두면 토큰만 바뀌었을 때 가드가 옛 값을 계속 강제하면서 초록으로 남는다(실측).(위 리터럴 행들의 통합 테스트와 채널이 다르다.) 승인 없이 상한을 넘는 자리는 그 가드의 한시 목록이 갖는다 — 목록은 줄어들기만 하고, 정책 예외 목록과 섞지 않는다(사유가 달라 역방향 검사가 한쪽에만 성립한다) |
| 폰트 크기 API | **`FontId::proportional`/`monospace`/`new` 와 `RichText::size` 에 숫자 리터럴 직접 전달 금지** — 위 토큰의 `.value()` 를 넘긴다. `proportional`/`monospace` 는 `FontId::new` 의 얇은 래퍼라 셋을 함께 막아야 한다(둘만 막으면 `new` 로 그대로 재유입된다). `egui::FontId { size: .. }` **구조체 리터럴 형태도 값과 무관하게 금지**다 — 필드명이 먼저 와서 숫자 인자 검사를 빠져나가기 때문이며, `Stroke {` 와 같은 이유·같은 규칙으로 막는다. 전역 폰트 치환 경로(`Style::text_styles.insert(..)`)는 결국 `FontId` 를 만들어야 하므로 위 넷을 막으면 함께 닫힌다. **`crates/tasty-doc-guards/tests/design_token_adherence.rs` 가드가 전부 막는다**(`doc-guards.yml` 이 main push · PR 마다 **경로 필터 없이** 자동으로 돌린다 — 그 크레이트로 옮겨 오면서 두 조합 모두에서 도는 자리가 됐다, [ci-gates](../../dev-guide/ci-gates.md)). `Spinner::size()` 는 이름이 같지만 위젯 지름이라 폰트 축이 아니다. 가드가 볼 수 없는 나머지(명명 const·변수·매크로를 거쳐 들어오는 값)는 그 파일 모듈 doc 의 "가드가 막지 못하는 것" 목록에 적혀 있다 |
| 스케일 밖 폰트 값 | DTCG primitive 는 10·11·12·13·14·16·17·20 이고 그중 semantic 이 붙은 것만 `Theme` 필드가 된다. **어느 tier 에도 없는 값(13.5 · 12.5 · 11.5 · 10.5 · 9.5 등)은 토큰으로 조용히 반올림하지 않는다** — 픽셀이 실제로 바뀌기 때문이다. 사유를 적은 명명 const 로 올려 드리프트를 눈에 보이게 두고, 어느 토큰으로 스냅할지는 디자인 판단으로 넘긴다. semantic 이 없는 primitive(12·16)를 쓰는 자리도 같다 — const 이름에 primitive 임을 남긴다. **명명 const 는 `ui_scale` 줌을 타지 않는다**(토큰만 `zoomed()` 경로에 있다). **그래서 이 허용은 값이 스케일 *밖*일 때만이다** — 값이 UI 폰트 토큰과 같은 const 는 토큰의 복사본이라 zoom 1 에서만 같고 나머지 배율에서 갈라진다. 그 형태는 `src/design_token_guard.rs` 의 `no_named_const_copies_a_ui_font_token` 이 잡는다(판별 축은 이름이 아니라 **값 + 위치**) |
| **`.5` 값은 토큰이 될 수 없다** | 위 규칙의 특수한 형태이지만 결론이 더 강해서 따로 적는다. 폰트 토큰은 `Theme::with_colors_and_zoom` 의 `zoomed = \|px\| LogicalPx((px.value() * ui_zoom).round())` 를 거치므로 **어떤 `ui_scale` 에서도 정수**다. 따라서 9.5 · 10.5 · 11.5 · 12.5 · 13.5 자리에 토큰을 넣는 것은 "zoom 1 에서만 0.5 다" 가 아니라 **전 배율에서 값이 다르다** — 값 보존 치환이 원리적으로 불가능하다. 그런 자리는 반드시 명명 const 로 두고, 스냅할지는 디자인이 정한다. 전제(폰트 토큰이 늘 정수)는 `crates/tasty-type-appearance/src/theme.rs` 의 `ui_font_size_tokens_are_integers_at_every_zoom` 가 잡는다. 이 행의 가드만 채널이 다르다 — 그것은 `src/` 안의 **lib 유닛 테스트**라 `cargo test --workspace --lib --bins`(`crossplatform-check.yml`, main push·PR)로 **자동으로 돈다**. 위 세 행의 `crates/tasty-doc-guards/tests/design_token_adherence.rs` 도 자동으로 돈다 — 채널이 다를 뿐이다: 그쪽은 `doc-guards.yml` 이 **경로 필터 없이** main push · PR 마다 그 크레이트를 통째로 돌린다([ci-gates](../../dev-guide/ci-gates.md)). 근거·대안·재검토 조건은 [ADR-0126](../../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md) |
| 보더 | 항상 **1px**(`border_width`) |
| 지목 링 | 2px(`focus_ring_width`) — 키보드 포커스뿐 아니라 **대상을 감싸 지목하는 2px 링 전반**의 굵기 토큰이다(우클릭 대상 표시, 드롭 대상 표시, 튜토리얼 마커, 선택 카드 테두리). 색은 별개 축이라 `accent_success` 등 다른 semantic 색과 조합해도 이 토큰을 쓴다. **링이지 바가 아니다** — 대상을 *감싸는* 획에만 쓴다 |
| accent 바 · 인디케이터 | 2px(`tab_indicator_width`) — 대상을 감싸지 않고 **한쪽 변에 붙는 띠**: 활성 행의 좌측 바, 탭 밑줄, 선택 리스트 행의 강조 바. 값은 `focus_ring_width` 와 같은 2 지만 **다른 토큰이고 zoom 거동이 다르다** — 링은 `zoomed()` 를 타고 이 토큰은 안 탄다. 값이 같다고 링 토큰을 바에 쓰지 않는다(그 오용이 갤러리 9자리에 있었다). 토스트 좌측 바만 3px(`toast_accent_width`)로 따로 있다 |
| painter 전사 글리프 | 선 굵기는 **`icon_stroke_width`**(1.5px) — `Ui` 가 없어 SVG 아이콘 대신 `Painter::line_segment` 로 형상을 옮기는 구간(popup 타이틀바의 close X · 전체화면 브래킷 · chevron · tree 가지) 전용. `border_width`(1)/`focus_ring_width`(2) 어느 쪽도 아니라 별도 필드이고, DTCG dim 토큰에 대응은 없다 |
| 선 굵기 API | **`Stroke::new` 에 숫자 리터럴 직접 전달 금지** — 위 세 필드의 `.value()` 를 넘긴다. `egui::Stroke { width: .. }` **구조체 리터럴 형태도 금지**다(값과 무관하게) — 필드명이 먼저 와서 숫자 인자 검사를 그대로 빠져나가기 때문이며, 같은 가드가 그 형태를 따로 막는다. 세 필드 어디에도 해당하지 않는 값(예: 체크마크 꺾은선, attached outline)은 명명 const 로 승격하고 사유를 주석에 남긴다 |
| 코너 반경 | **`corner_radius_sm`(2) · `corner_radius`(4) · `corner_radius_lg`(8) 만.** DTCG 도 `radius-2/4/8/full` 뿐이라 이 셋이 스케일 전부다. 떠 있는 패널(배너·부팅 카드)은 `_lg`, 작은 inner element(키캡·배지)는 `_sm`, 나머지는 기본. 셋 다 `ui_scale` zoom 을 받는다 — **굵기 축(`border_width`·`icon_stroke_width`)이 zoom 을 안 받는 것과 반대다** |
| 코너 반경 API | **`.corner_radius(<숫자>)` 와 `CornerRadius::same(<숫자>)` 둘 다 금지** — 위 토큰의 `.value()` 를 넘긴다. **접두 둘을 함께 막아야 한다**: 다수가 `.corner_radius(egui::CornerRadius::same(12))` 로 한 겹 감싸여 `.corner_radius(` 뒤에 숫자가 오지 않는다(`Margin::same(` 을 `inner_margin(` 과 따로 막는 것과 같은 이유). 반경이 없는 자리는 `0.0` 대신 **`CornerRadius::ZERO`** — 0 은 그리드의 원점이라 규칙 안에 있고, 전부 0 이면 이름을 쓴다(`Margin::ZERO` 와 같은 관례). 스케일 밖 값(3 · 6 · 12)은 스냅하지 말고 사유를 적은 명명 const 로 둔다 — 현재 `tasty_ui_widgets::tokens` 의 `BOOT_CHROME_CORNER_RADIUS`(6) · `BOOT_CARD_CORNER_RADIUS`(12) · `TAG_PILL_CORNER_RADIUS`(3) 셋이다. **폰트 축과 결론은 같고 대가는 더 크다**([ADR-0126](../../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md) 의 논리): 명명 const 는 `zoomed()` 밖인데 반경 토큰은 zoom 을 타므로, 그 자리들만 배율 0.85 / 1.2 에서 고정 반경으로 남는다. 집행은 토큰 가드의 `no_inline_visual_token_literals` 가 두 접두를 함께 막고, 모수가 갈리지 않는지는 `the_two_sister_guards_scan_the_same_roots` 가 본다. **실행 채널은 [ci-gates](../../dev-guide/ci-gates.md) 가 정본이다** — 이 행이 위 세 행처럼 채널을 단정하지 않는 것은 의도다(위 세 행의 단정은 헤드리스 잡이 워크스페이스 전체를 돌게 된 뒤로 낡았고, 그 축은 별도로 정리 중이다) |
| 상태 점 지름 | **점 가족은 셋이다** (2026-09-17 결정): 일반 `status_dot_size`(8, 배지·태그·리스트 행) · **compact `status_dot_size_compact`(6)** — 24px 크롬 안(pane 탭 strip · 상태바 · 접힌 rail) · 활성 탭 마커 4(위치 표시라 상태 점이 아닌 다른 role). `badge_dot_size`·`tag_dot_size` 는 일반 8 의 별칭이고 `tab_dot_size`·`statusbar_dot_size` 는 compact 6 의 별칭이다. attached ring 은 굵기 2(`status_dot_attached_ring_width`) + offset 2(`status_dot_attached_ring_offset`, **점 바깥 edge → ring 안쪽 edge** 기준)라 총 bbox 는 6 + 2×(2+2) = 14 로 24px 바 안에 남는다. 남은 스케일 밖 값은 둘이다 — Plugins Attention 의 severity 점 7(`src/view/plugins/ui/attention.rs`)과 색 override·튜토리얼 rail 의 5. 스냅하지 말고 사유를 적은 명명 const 로 둔다 ([ADR-0126](../../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md) 의 점 치수 축 절) — **스냅이 배율 1 에서 이미 픽셀을 바꾸기 때문**이다. 이 축에서는 **수렴이 드리프트의 반대 증거**였다 — 무관한 크레이트·화면이 같은 수로 모였고(6 이 둘), 그것이 실수가 아니라 이름 없는 역할이었음을 compact role 이 확인했다. **4px 그리드는 이 축에 안 걸린다** — 그리드 행은 간격을 말하고 점 지름은 `primitive.size-*` 를 직접 가리키는 leaf 다. **채널 둘**: `src/design_token_guard.rs` 의 `no_dot_radius_is_an_anonymous_literal`(`circle_filled(`·`circle_stroke(`·`circle(` 의 반지름 리터럴 금지)과 `no_dot_named_const_copies_the_status_dot_token`(값 8 인 점 이름 const 금지). 둘 다 lib/bin 유닛이라 `--lib --bins` 두 잡에서 자동으로 돈다. **뒤쪽은 사거리가 좁다** — 이 축에는 `.size(`·`.corner_radius(` 같은 문법적 자리가 없어(점 const 는 `.value() * 0.5`·`vec2(` 로 소비된다) 자매 둘의 `값 × 자리` 를 쓸 수 없고, 이름 축으로 세웠다. 이름에 `DOT` 이 없는 상수를 점에 쓰면 통과한다. **갤러리는 모수 밖이다**(전역 zoom, ADR-0135) |
| 컨테이너 길이 (폭·높이) | **`set_min_width`/`set_max_width`/`set_min_height`/`max_height`/`exact_width`/`exact_height`/`desired_width` 에 숫자 리터럴 직접 전달 금지** — `Theme` 접근자의 `.value()` 를 넘긴다. 대응 디자인 토큰이 있으면 그것(`field_width_xs/color/md/lg` · `input_height()` · `autocomplete_max_height()` …)을 쓰고, **없으면 접근자를 새로 만든다** — 값은 그대로 두고 본문만 `LogicalPx((N * self.ui_zoom).round())` 형태로 적는다(`modhint_*` · `multiselect_*` 와 같은 형태이고, 디자인 export 가 갱신되면 생성물로 넘어간다). **이 축에서 리터럴이 금지인 이유는 값 통일이 아니라 배율이다**: 본체는 egui `zoom_factor` 를 1.0 으로 고정하고 `ui_scale` 을 `zoomed()` 로만 적용하므로 호출부 리터럴은 배율을 안 탄다. 상자만 고정인데 안의 폰트·간격·글리프는 전부 토큰이라 커지므로 1.2 에서 내용이 잘리고 0.85 에서 빈 공간이 남는다 — 값 3~17 인 폰트 축과 달리 이 축의 값은 26~340 이라 대가가 크고 형태도 다르다. **갤러리는 예외다** — `ctx.set_zoom_factor(ui_scale)` 로 egui 전역에 배율을 걸어 리터럴도 함께 커진다. 그래서 갤러리의 같은 형태는 결함이 아니고, 이 축의 가드 모수에 넣지 않는다. 그 예외를 갤러리 쪽에서 읽는 자리는 [gallery-completeness](../policies/gallery-completeness.md) 의 무대 치수 절이다 — 액자에도 사유를 적은 명명 const 는 요구한다는 조건이 거기 있다. 스케일 밖 값은 폰트·반경 축과 같게 토큰으로 스냅하지 않는다. 접근자 쪽 집행은 `crates/tasty-type-appearance/src/zoom_coverage_guard.rs` 의 `every_literal_bearing_length_accessor_follows_ui_zoom`(lib 유닛이라 `--lib --bins` 두 잡에서 자동 실행)이 맡고, **호출부 리터럴을 막는 접두 가드는 아직 없다.** 근거·대안·재검토 조건은 [ADR-0135](../../adr/0135-ui-length-literals-do-not-follow-ui-scale-in-the-app.md) |
| 생성 토큰 상수 직접 소비 | **UI 계층(`src/`)은 `tasty_design_tokens::generated` 의 `LogicalPx` 상수를 직접 소비하지 않는다** — **그 토큰 자신의** `&Theme` 경로를 경유한다(semantic 은 `SEMANTIC_DIM_TO_THEME_FIELD` 가 잇는 `Theme` 필드, component 는 토큰 이름의 접근자 — `component.fp-crumb-max-width` → `th.fp_crumb_max_width()`). **값이 같은 다른 이름은 경로가 아니다** — 픽셀은 같고 다른 토큰에 묶인다. `Theme` 경로가 없는 길이 토큰은 가드의 `PATHLESS_LENGTH_TOKENS` 명부가 사유와 함께 들고, 그 토큰을 쓰려면 경로(표 항목 · `Theme` 필드 · `sizing_parity` arm)를 먼저 만든다. 실패문은 자리마다 그 경로 이름 또는 "경로 없음" 을 댄다([ADR-0556](../../adr/0556-the-generated-length-const-guard-prescribes-the-tokens-own-theme-path.md)). 규칙 원문은 그 크레이트의 `lib.rs` `zoom 우회 금지 (필수)` 에 있다: 생성 상수의 역할은 `SIZING` 초기값 공급과 정합 테스트까지다. 이유는 값이 아니라 **경로**다 — 생성 상수는 컴파일 타임 상수라 `with_colors_and_zoom` 의 `zoomed()` 밖이다. 어느 필드에 배율을 걸고 어느 필드에서 뺄지 — **zoom 적용·제외 정책은 `Theme` 가 소유한다**(`border_width` · `status_bar_height` 처럼 일부러 배율을 안 타는 필드도 있다) — 상수를 직접 읽으면 그 정책을 건너뛴다. 배율을 타는 토큰이면 zoom 1 에서만 같고 0.85 / 1.2 에서 갈라진다. **토큰을 썼으니 됐다고 믿게 만들어 리터럴보다 나쁘다.** 무차원 상수(`f32` — 불투명도·가중치·지속시간)는 배율 축이 아니라 대상이 아니다. 집행은 `src/design_token_guard.rs` 의 `ui_does_not_consume_generated_length_consts_directly`(lib/bin 유닛이라 `--lib --bins` 두 잡에서 자동 실행). 갤러리는 모수 밖이다 — egui 전역 zoom 을 쓰므로 상수도 함께 커진다([ADR-0135](../../adr/0135-ui-length-literals-do-not-follow-ui-scale-in-the-app.md)) |
| 호버 오버레이 | `hover_overlay`(라이트 검정 8% / 다크 흰색 8% 자동 도출) — 직접 값 금지 |
| 활성 오버레이 | `active_overlay`(12%) — 선택/active 행, hover(8%)와 구분 |
| 색 파생 계수 | **`.gamma_multiply(<숫자>)` · `.with_alpha(<숫자>)` 에 숫자 리터럴 직접 전달 금지** — 사유를 적은 명명 const 로 올린다. 값 공간은 다르지만(배율 0~1 · 알파 0~255) 의도가 같아 한 축으로 본다. **이 축은 "토큰을 써라" 가 아니다** — DTCG 의 opacity 는 `disabled`(0.5) · `recessed`(0.4) · `dimmed`(0.75) 셋뿐인데 실제 값은 0.09~0.92 로 훨씬 넓다. 값이 토큰과 같아도 **역할이 같을 때만** 토큰으로 보낸다(실측: 61 자리 중 역할까지 맞은 것은 `select` 의 disabled 디밍 하나뿐이었다 — chip 의 accent 테두리 0.4 는 `opacity_recessed` 의 "상위 스코프 배너 뒤 디밍" 이 아니다). **수렴은 이 규칙 밖이다** — 같은 idiom(채움+테두리 짝)이 한때 네 곳에 서로 다른 값으로 있었고(0.14/0.45 · 0.12/0.40 · 0.12/0.35 · 0.11/0.36), 어느 값으로 모을지는 픽셀이 바뀌는 디자인 결정이었다. 2026-09-17 결정이 그 짝에 `tint-fill-alpha`(0.12) · `tint-border-alpha`(0.36) 를 주어 **짝으로 쓰는 자리는 전부 그 둘을 읽는다** — 새로 계수를 짓지 않는다. 채움만 쓰거나 테두리 계수가 그 짝의 뜻이 아닌 자리(예: 경고 배지 테두리 0.4)는 여전히 명명 const 다. 이름을 붙이는 것과 값을 모으는 것을 가르는 근거는 [ADR-0126](../../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md)(축 중립). **갤러리와 번들 plugin 도 모수 안이다** — 계수는 배율 축이 아니라 갤러리 예외([ADR-0135](../../adr/0135-ui-length-literals-do-not-follow-ui-scale-in-the-app.md))가 걸리지 않고, specimen 의 값은 본체 값의 사본이라 빼면 그쪽이 조용히 갈린다. **채널**: `src/design_token_guard.rs` 의 `no_color_derivation_coefficient_is_an_anonymous_literal`(lib/bin 유닛이라 `--lib --bins` 두 잡에서 자동 실행). **면제 목록은 없다** — 오늘 위반 0 이라 필요가 없고, 첫 예외는 항목 추가가 아니라 부류의 창설이다 |
| 텍스트 대비 | 최소 **4.5:1**. 위반 시 [`ai-verification/visual-verification`](../../ai-verification/visual-verification.md) 체크리스트 |
| 터미널 콘텐츠 애니메이션 | **0ms** — 셀/스크롤엔 어떤 transition 도 금지(입력 응답성 우선) |
| UI 위젯 애니메이션 | 짧게(보통 100–150ms), 입력 직후 피드백 한정 |
| 스크롤 애니메이션 | **0ms** — 스크롤은 입력 직후 피드백이 아니라 콘텐츠 이송이라 위 "터미널 콘텐츠" 쪽 규칙을 따른다. **프로그램적 스크롤**(`scroll_to_*` / `scroll_to_me`)은 host egui 와 모든 egui-mesh Context 양쪽에서 `ScrollAnimation::none()` 으로 즉시 점프한다. **휠 델타를 도착 프레임에서 전량 반영하는 것은 egui-mesh(plugin SDK) 경로에 한한다** — plugin 은 별도 프로세스라 애니메이션 프레임 하나가 곧 프로세스 간 왕복이기 때문이다. host egui 의 휠은 egui-winit 이 `MouseWheelUnit::Line` 으로 넣고 egui 가 이를 항상 다중 프레임으로 소진하므로, 설정창·팔레트·host popup·갤러리의 `ScrollArea` 는 egui 기본 스무딩을 그대로 쓴다([ADR-0108](../../adr/0108-egui-mesh-scroll-delivered-in-one-pass.md)) |
| 드래그 패닝 | **스크롤 영역은 드래그 패닝 여부를 선언한다.** 기본 정책은 **끈다**(`drag_to_scroll(false)`) — 데스크톱 마우스에서 누른 채 끄는 것은 텍스트 선택 · 행 선택 · 파일 드래그의 의도이고, 그때 내용이 포인터를 따라 미끄러지면 그 의도와 충돌한다(손을 뗀 뒤 관성까지 붙는다). `egui` 기본값이 `true` 라 **안 적으면 켜진 채로 태어난다.** 남기기로 한 자리는 `drag_to_scroll(true)` 를 **명시**하고 이유를 그 자리에 적는다 — 가드는 값이 아니라 **말했는가**를 본다. 진입점 셋: `egui::ScrollArea` · `egui_extras::TableBuilder`(자기 안에서 `ScrollArea` 를 만들어 넘긴다) · **축을 켠**(`.scroll(`/`.scroll2(`/`.hscroll(`/`.vscroll(`) `egui::Window`. 축을 안 켠 `Window` 는 내부가 `ScrollArea::neither()` 라 스크롤 자체가 없어 대상이 아니다. `egui::ComboBox` 의 드롭다운은 egui 가 내부에서 만들고 `drag_to_scroll` 을 호출부에 노출하지 않아 **레포 소스로는 못 닫는다** — 잔여로 세지 않는다. **채널**: `crates/tasty-doc-guards/tests/drag_to_scroll_is_declared.rs`(`doc-guards.yml` 이 main push · PR 마다 **경로 필터 없이** 돌린다 — [ci-gates](../../dev-guide/ci-gates.md)). 이 규칙은 on/off 정책이라 `Theme` 필드가 없다(아래 문단의 관례). 근거 · 대안 · 재검토 조건은 [ADR-0299](../../adr/0299-scroll-areas-declare-whether-they-pan-on-drag.md) |

코드에 하드코딩이 보이면 `Theme` 필드로 옮긴다. 새 시각 규칙은 이 표에 추가 후 `Theme` 에 필드 신설 — 단 **on/off 정책은 제외한다.** 조절 가능한 수치가 아니라 "끈다"는 결정은 테마마다 달라지지 않는 정책 상수이므로 `Theme`/`ThemeWire` 를 넓히지 않고 이 표와 ADR 을 단일 출처로 둔다(위 애니메이션 3행이 그 예 — `crates/tasty-type-appearance` 에 대응 필드가 없다).

표·드롭다운·버튼처럼 이름이 곧 정체성인 보편 컴포넌트는 인라인으로 그리지 말고 공용 위젯으로 추출한다 — [공용 위젯 제작 정책](../policies/shared-widgets.md).

### latte 중성 램프 대비 — 알려진 예외

위 "텍스트 대비" 4.5:1 규칙에 대해 **latte 는 어두운 배경 토큰 위에서 구조적으로 미달**한다. 같은 계산을 반복하지 않도록 WCAG 상대휘도 공식(sRGB→선형, 0.2126/0.7152/0.0722 가중, `(L_hi+0.05)/(L_lo+0.05)`)으로 구한 전 조합을 박아 둔다. `*` 가 통과.

| 전경 \ 배경 | crust `#dce0e8` | mantle `#e6e9ef` | base `#eff1f5` | surface0 `#ccd0da` | surface1 `#bcc0cc` | surface2 `#acb0be` | `#ffffff` |
|---|---|---|---|---|---|---|---|
| `subtext0` (text-muted) `#63667c` | 4.26 | 4.64\* | 4.99\* | 3.65 | 3.10 | 2.61 | 5.64\* |
| `subtext1` (text-secondary) `#5c5f77` | 4.73\* | 5.14\* | 5.53\* | 4.05 | 3.44 | 2.89 | 6.25\* |
| `text` (text-primary) `#4c4f69` | 6.04\* | 6.57\* | 7.06\* | 5.17\* | 4.39 | 3.69 | 7.99\* |

실제로 미달 조합을 그리는 상시 노출 화면은 둘이다.

- **상태바** — `bg_app`(=crust) 위 `text_muted` = **4.26:1**.
- **탭바** — 포커스된 pane 의 탭 스트립이 `surface_raised`(=surface0), 비활성 탭 제목이 `text_muted` = **3.65:1**.

**팔레트로는 고칠 수 없다.** 두 경로 모두 막혀 있다.

- `subtext0` 을 crust 통과선(`#5f6279`, 4.52:1)까지 더 내리면 `subtext1`(`#5c5f77`)과의 차가 `(3,3,2)` 로 줄어 text-muted 와 text-secondary 가 사실상 같은 색이 된다 — 3단 텍스트 위계가 latte 에서만 2단으로 붕괴한다. 그러고도 surface0 는 여전히 미달(3.88)이다.
- surface0 를 통과시키려면 `subtext0` 이 `#555870` 근처여야 하는데 이는 `subtext1` 보다 **어둡다** — 램프 순서가 뒤집힌다.

즉 surface0 위에서 AA 를 넘는 중성 전경은 `text` 하나뿐이고, surface1/surface2 는 `text` 조차 미달이다. 이는 catppuccin latte 의 raised/hover 배경단이 라이트 테마치고 어둡기 때문이며, 고치려면 팔레트 중성 램프 전체를 다시 뜨는 디자인 결정이 필요하다(vendored 팔레트 정체성 + DTCG export 를 함께 갈아야 한다). 컴포넌트별 회피(해당 화면만 `text_secondary`/`text_primary` 로 승격)는 가능하지만 상태바·탭바의 확정 시안을 바꾸는 일이라 디자인 요청 없이 진행하지 않는다.

**새 UI 를 그릴 때는 이 표를 근거로 배경을 고른다** — muted 캡션을 얹을 배경은 `base`/`mantle`/`#ffffff` 로 한정하고, `surface0` 이상 어두운 배경 위에는 `text_primary` 를 쓴다.

### Host UI zoom

`AppearanceSettings.ui_scale`(`small/medium/large` = `0.85/1.0/1.2`). `install_global_with_zoom` 이 sizing 토큰 자체에 배율을 곱해 전역 `Theme` 재빌드 — UI 코드는 곱셈 무지(`theme().spacing_*` 가 이미 zoomed).

- **zoom 받음**: `spacing_*` · `font_size_*` · `corner_radius`(`_sm`/`_lg` 포함) · `focus_ring_width` · `item_height_*` · 사이드바 sizing 토큰들.
- **zoom 제외**: hairline(`border_width` 1px 정책 · `icon_stroke_width` — 이 굵기를 쓰는 타이틀바 버튼 기하가 고정 px 라 선만 굵어지면 글리프가 뭉갠다 · `tab_indicator_width`) · 탭바 토큰(`tab_width`/`tab_bar_*`) · 상태바 토큰(`status_bar_height`) · CSD 타이틀바 토큰 · 렌더 콘텐츠 폰트(터미널 `font_size_term_*` 는 별도 `effective_terminal_font` 경로로 GPU 셰이더에 전달, markdown `font_size_prose_h1`).
  이 목록은 **요약이고 정본이 아니다** — 정본은 `crates/tasty-type-appearance` 의 zoom 면제 가드가 든 이름 집합이며, 소스와 이름 단위로 대조된다. 필드를 새로 면제하려면 그 목록에 사유 갈래와 함께 등록해야 하고, 등록 없이 `zoomed()` 를 빼면 그 가드가 이름을 대며 빨개진다. 각 필드의 사유는 필드 doc 에도 붙어 있다.
- **4px 그리드 + zoom**: 비정수(`12×1.2=14.4`)는 `round_ui()`/`f32::round()` 로 GPU 픽셀 정수 흡수.
- **라이브 갱신**: settings save / IPC update 시 `UiIntent::AppearanceChanged` 발화 → `cascade_appearance_changed` 가 전 윈도우 GpuState 에 broadcast(polling 아님, 변경 시 1회).
- **불변식 — `set_theme`/`install_global*` 은 렌더 밖에서만**: 전역 `THEME` 는 std `RwLock`(재진입 불가)이라, egui 렌더 클로저는 `theme()`(=`THEME.read()`) read guard 를 보유한다. 렌더 도중 `set_theme`(=`THEME.write()`)을 호출하면 자기 read guard 때문에 self-deadlock 으로 hang 한다. 따라서 테마 install 은 항상 인텐트 dispatch(`about_to_wait` / cascade) 단계에서만 수행하고, 렌더 핸들러(설정 모달 Save 등)는 `UpdateSettings` 인텐트만 큐잉한다(install 직접 호출 금지).

## 코드 위치

- schema: `crates/tasty-type-appearance/src/{color,theme}.rs`
- 전역/IO: `crates/tasty-themes/`(`theme()`, `apply_theme`, `resolve`, `install_global[_with_runtime]`, mocha/latte 임베드)
- settings: `crates/tasty-settings/src/appearance.rs`
- 부팅: `src/app/window_lifecycle.rs::boot_apply_theme`
</content>

## 떠 있는 표면의 그림자

떠 있는 표면의 lift 그림자는 정본 토큰 **둘**이다 — `SHADOW_POPOVER`(design `--tasty-shadow-popover`, `theme.shadow_popover()`)와 `SHADOW_MODAL`(design `--tasty-shadow-modal`, `theme.shadow_modal()`). 어느 표면이 어느 쪽을 쓰는지는 SCOPE RULE 이 정한다 — anchored + scrim-less 는 popover, 뷰포트를 점유하는(centered) 표면은 modal, 두 형태 어디에도 안 들어가는 표면은 그림자가 없다. **scrim 유무는 술어가 아니다** — modal 을 받는 표면 중 실제로 scrim 이 깔리는 것은 일부이고, scrim 은 그 값을 더 크게 잡은 근거일 뿐이다. 그 근거(scrim 은 바닥을 어둡게 하지만 엣지를 그리지 않는다)·대안·재검토 조건은 [ADR-0254](../../adr/0254-floating-surface-shadow-scope-rule.md).

세 갈래의 현재 소비처:

- **popover** — 배너 셸, tooltip, autocomplete 드롭다운, MultiSelect/Select·ComboBox 메뉴(egui `Frame::popup` 경로 — 아래 참조), modifier-hint 오버레이, tutorial callout, 그리고 anchored popup(tools menu · rail category · 마우스 캡처 배너 더보기 메뉴 · search bar). 앞의 셋은 트리거 rect 로 좌표를 계산해 열고, `search bar` 만 scope 상단 가로중앙에 뜬다 — 좌표의 출처가 아니라 형태(scrim 없이 살아 있는 콘텐츠 위)가 갈래를 정한다.
- **modal** — 호스트 popup 셸(`src/adapters/ui/popup/draw.rs`), plugin popup 셸(`src/plugin_bridge/popup_render.rs`), 부팅 셸 설정 다이얼로그(`src/gfx/gpu/shell_setup.rs`), 부팅 실패 카드(`src/gfx/gpu/boot_error.rs`).
- **없음** — 알림 패널(타이틀바를 갖고 사용자가 옮기는 창처럼 동작해 두 형태 어디에도 안 들어간다).

갤러리도 같은 세 갈래를 노출한다 — 공유 셸 키트(`crates/tasty-gallery/src/catalog/widgets/dialog.rs`)의 `frame_card`(modal) / `frame_card_popover` / `frame_card_flat`(없음). 갈래는 셸이 아니라 **호출부가 본체에서 무엇인가**가 정하므로, 창 셸(Settings)·pane 콘텐츠(image surface)·다른 표면 안에 얹히는 섹션은 `frame_card_flat` 을 쓴다. 그래야 Foundations 의 elevation 전시("UI 표면의 깊이는 tint 로 읽는다")와 카드의 실제가 갈리지 않는다.

**그 키트가 갤러리의 전부는 아니다.** 셸을 직접 그리는 specimen 이 여럿 있고(예: `components/dag/window.rs`·`components/remote_attach.rs`·`components/transfer.rs`·`components/file_picker.rs`·`catalog/popup_frame.rs`), 그것들은 같은 판정을 **그 자리에서** 얹는다. 그래서 갈래를 감사할 때 키트 호출부만 훑으면 그 specimen 들이 모수 밖으로 빠진다 — popup specimen 의 모수는 `src/source_guards/gallery_specimen_parity.rs` 의 id↔specimen 표이고, 그 표의 각 id 를 `src/adapters/ui/popup/draw.rs::popup_shadow` 에 물어 대조한다.

값을 새로 만들지 않고 이 둘만 쓴다 — `Shadow {}` 를 직접 만드는 코드는 `crates/tasty-type-appearance/src/theme.rs` 의 `ShadowToken::to_egui()` 한 곳뿐이어야 하고, 그 밖의 생성은 접근자(`shadow_popover()` / `shadow_modal()`)의 `to_egui()` 로 라우팅한다. 페이드가 필요하면 그 결과의 `color` 에만 opacity 를 곱하고 기하(`offset`/`blur`/`spread`)는 바꾸지 않는다.

`crates/tasty-type-appearance/src/shadow_policy_guard.rs`(lib 유닛 테스트)의 검사는 이 정책보다 좁다. 인식하는 `Shadow` 리터럴의 생성 위치, 같은 줄의 `let mut` 와 접근자 이름으로 찾은 변수의 기하 재대입, 접근자 명부의 정합을 본다. 줄 단위 텍스트 검사라 별칭이나 다른 생성·대입 형태의 데이터 흐름까지 추적하지 않는다. **egui 기본 그림자 두 필드의 매핑 유지 여부와 표면별 토큰 선택은 검사하지 않는다.** 아래 매핑이 구현돼 있다는 사실과 이 가드가 그것을 검증한다는 주장은 구분한다.

두 상수가 디자인 정본을 제대로 옮겨 적었는지는 `crates/tasty-design-tokens/tests/shadow_parity.rs` 가 vendor json 과 필드 단위로 대조한다 — 그림자는 `$type: shadow` 라 생성기가 건너뛰고 값이 **손으로 옮겨지므로**, 이 대조가 없으면 전사 오차가 조용히 남는다(실제로 남아 있었다). 같은 파일이 정본 쪽 모수도 잠근다: raw 그림자 토큰 명부의 완전성, alias 토큰이 명부 안의 값으로 귀착하는지, `-shadow` 로 끝나는 이름이 떠 있는 그림자가 아닌 치수 축(`kbd-shadow-depth`)과 섞이지 않는지. 통합 테스트라 자동 실행은 헤드리스 잡뿐이다(`docs/dev-guide/ci-gates.md`).

`ShadowToken.spread` 는 음수를 **표현**하지만(CSS `box-shadow` 와 같은 의미) 그것을 쓰는 표면은 **미구현으로 둔다** — egui 가 음수를 못 담아 `to_egui()` 가 debug 단언으로 터진다. 근거·대안은 [ADR-0254](../../adr/0254-floating-surface-shadow-scope-rule.md).

egui 가 스스로 그리는 그림자 둘(`visuals.popup_shadow` · `visuals.window_shadow`)도 같은 두 토큰으로 매핑한다(`crates/tasty-egui-theme/src/lib.rs`). 매핑하지 않으면 `Visuals::dark()`/`light()` 의 기본값이 남아 정본 아닌 **세 번째 그림자**가 뜨고, 그 기본값은 테마마다 알파가 갈려(dark α96 / light α25) 같은 화면의 tasty 그림자와 값이 달라진다. 갈래는 형태가 정한다 — `Frame::popup`(= `egui::popup_below_widget` · `ComboBox` · MultiSelect/Select 위젯 메뉴)은 트리거 아래 붙는 anchored 표면이라 popover, `egui::Window` 는 뷰포트를 점유하므로 modal 이다. 프레임을 직접 넘기는 호출부는 이 기본값 대신 자기 프레임의 `.shadow(...)` 를 쓴다.
