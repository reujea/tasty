# Design Token → tasty Theme 매핑

claude design(`Tasty Design System`)의 semantic 토큰을 tasty `Theme` 필드로 옮길 때 쓰는
확정 매핑표. `design-parity` 스킬이 참조한다. **hex 가 아니라 Theme 필드로 접근한다**
(테마가 바뀌면 hex 는 달라지므로).

출처: 디자인 루트의 `tokens/semantic.css` + `tokens/primitives.css` (mocha 기준).

> 색·치수 토큰이 `Theme` 로 수렴하듯, **아이콘 지오메트리(SVG path)는 `tasty-icons`
> 크레이트의 수기 전사가 소유**한다(소비처는 path 를 재인라인하지 않는다). 규칙 전문은
> [icons.md](icons.md).

## 색

| 디자인 토큰 | tasty Theme | mocha hex | 비고 |
|---|---|---|---|
| `bg-sidebar` | `mantle` | `#181825` | 사이드바·**탭 바** 등 한 단계 더 어두운 면 |
| `bg-panel` | `base` | `#1e1e2e` | 패널형 팝업 본문 (remote_tool / port_scanner / file_handler_picker / 전송 진행·실패). 목록이 아니라 **판정기**가 정본이다 — `popup::draw::popup_bg_fill` |
| `surface-raised` | `surface0` | `#313244` | 카드·입력·메뉴·command_palette 본문, secondary 버튼 fill |
| `border-default` | `surface0` | `#313244` | |
| `border-strong` | `surface1` | `#45475a` | 팝업 외곽선·강한 구분 |
| `separator` | 흰색 8% 알파 | — | **구역 bg 위 블렌드.** base 위 → ≈surface0, mantle 위 → ≈surface1 근사. egui 는 `ui.separator()`(비가시) 대신 surface1 `hline` 으로 그린다 |
| `text-primary` | `text` | `#cdd6f4` | |
| `text-secondary` | `subtext1` | `#bac2de` | |
| `text-muted` | `subtext0` | `#a6adc8` | |
| `text-disabled` | `overlay1` | `#7f849c` | **disabled 전용 잉크.** 모든 disabled 라벨·글리프가 이 role 을 읽는다 |
| `glyph-dim` | `overlay0` | `#6c7086` | 물러나야 하는 chrome glyph(사이드바 dim chevron·dim 아이콘·상태바 글리프). `text-placeholder` 와 값만 같고 role 이 다르다 — **disabled 용이 아니다** |
| `border-frame` | `surface2` | `#585b70` | 틀의 선 **넷**(popup 프레임 · titlebar 아래 선 · pane divider · GPU 비활성 보더). popup **내부** 구분선은 `border-strong` 에 남는다. `surface-active` 와 값만 같다 |
| `accent-decorative` | `peach` | `#fab387` | 헤더 장식 accent(Plugins 창 헤더 glyph). `accent-attention`(주의 환기)과 값만 같다 |
| `accent-primary` | `accent_primary()` | `#89b4fa` | primary 버튼·포커스·활성 탭 언더라인 |
| `accent-danger` | `accent_danger()` | — | |
| `os-macos-close` | `accent_macos_close()` | `#ec6a5e` | macOS 신호등 close (테마 불변 OS 리터럴, const `OS_MACOS_CLOSE`) |
| `os-macos-min` | `accent_macos_min()` | `#f4bf4f` | macOS 신호등 minimize (const `OS_MACOS_MIN`) |
| `os-macos-zoom` | `accent_macos_zoom()` | `#61c554` | macOS 신호등 zoom (const `OS_MACOS_ZOOM`) |

## 불투명도

| 디자인 토큰 | tasty Theme | 값 | 비고 |
|---|---|---|---|
| `opacity-disabled` | `opacity_disabled()` | `0.5` | disabled 컨트롤 공통 디밍. const `OPACITY_DISABLED`. 모든 위젯이 disabled 시 이 값으로 `gamma_multiply` |
| `tint-fill-alpha` | `tint_fill_alpha()` | `0.12` | accent 로 옅게 깐 채움. const `TINT_FILL_ALPHA` |
| `tint-border-alpha` | `tint_border_alpha()` | `0.36` | 같은 accent 의 테두리. const `TINT_BORDER_ALPHA`. **짝으로 쓰는 것이 기본**이고 승인된 부분 사용 둘(채움만: warning callout·misc/remote/script 배지 · 테두리만: chip remote 태그) |

## 치수 (px → LogicalPx)

| 디자인 토큰 | 값 | tasty |
|---|---|---|
| space xs / sm / md / lg | 4 / 8 / 12 / 16 | `spacing_xs/sm/md/lg` |
| control-height (버튼·입력) | 28 | |
| control-height-tab | 24 | |
| control-height-tree | 22 | |
| font-size body / caption / heading | 13 / 11 / 13(weight 600) | `font_size_body/caption/heading` |
| font-size micro | 10 | `font_size_micro` — Badge/Tag/Kbd·command_palette footer 힌트 |
| font-size prose-h1 | 20 | `font_size_prose_h1` — markdown 헤딩 사다리의 h1 앵커(`render.rs::heading_sizes_px` 가 h1↔`font-size-body`(h6) 사이를 CSS 로 5단계 선형보간, UI cap 면제). `prose-h2`·`line-height-prose` 는 [ADR-0065](../../adr/0065-markdown-webview-render-channel.md) 이전 egui_commonmark 시절 라이브러리 제약으로 은퇴한 토큰이며, webview 전환(CSS) 이후로도 부활하지 않았다 — 헤딩 사다리는 이제 `--md-h1`..`--md-h6` CSS custom property 로 직접 표현되므로 별도 semantic 토큰이 필요 없다. 정본은 여전히 tokens/·vendor json·생성 const 모두에서 제거된 상태 |
| font-size term-sm / term / term-lg | 12 / 14 / 16 | `font_size_term_sm` / `font_size_term` / `font_size_term_lg` — 터미널 스케일 |
| icon-size xs | 12 | `icon_glyph_size_xs` |
| icon-size md | 16 | `icon_glyph_size_md` (기존) — Button leading/trailing·MenuItem 글리프, **spinner 지름**(폰트가 아니라 아이콘 가족) |
| font-size brand-display | 30 | `font_size_brand_display` — 첫 실행 셸 설정 카드 브랜드 타이틀. UI 14px 상한의 브랜딩 예외 둘 중 하나 |
| status-dot-size-compact | 6 | `status_dot_size_compact()` — 24px 크롬 안의 점(pane 탭 strip·상태바·접힌 rail). `tab_dot_size()`·`statusbar_dot_size()` 가 이 값의 별칭 |
| statusbar-theme-glyph / statusbar-glyph | → `glyph-dim` | 상태바 글리프의 **색** role 둘. 앞의 것은 우측 테마 토글의 `sun`/`theme` 글리프가(색 점은 2026-09-17 결정으로 없어졌다), 뒤의 것은 좌측 브랜치 글리프가 읽는다 |
| statusbar-glyph-size | → `icon-size-xs` (12) | 상태바 인라인 글리프의 **크기** role(2026-09-20 결정). 바에는 글리프 색 role 만 있고 크기 role 이 없어 위젯이 `icon-size-xs` 를 직접 읽고 있었다 — 그 한 자리를 이 role 로 바꿨고, 바의 글리프는 전부 거기서 크기를 받는다. 배율은 바의 다른 글자와 같이 탄다(`round(12 × s)` = 10 / 12 / 14). **바 높이 24 는 배율 밖**이고 이 role 과 무관하다. 테마 분기 없음 — 치수는 어느 테마가 켜졌는지에 의존하지 않는다 |
| git-toolbar-height | 32 | `git_toolbar_height()` — git viewer diff 툴바. **`control-height`(28) 아님**(컨테이너 role) |
| port-process-col-min-width | 200 | `port_process_col_min_width()` — port scanner Process 컬럼 **최소폭** |
| radius / radius-sm | 4 / 2 | `corner_radius` / `corner_radius_sm` |
| border-width | 1 (항상) | `border_width` |

## token-policy 값 변동 (★)

디자인 token-policy 정합으로 다음 위젯 치수가 on-grid 값으로 확정됐다 (`crates/tasty-ui-widgets/`):

| 위젯 | 항목 | 값 | 비고 |
|---|---|---|---|
| Switch | track | 28×16 | on-grid (이전 32×18 의 off-grid 18 제거) |
| TreeRow | per-level indent | 12 | `--tasty-tree-row-indent` = space-md(12) → `theme.spacing_md` |
| Badge / Tag / Kbd | pill 높이 | 16 | size-16. Kbd min-width 도 16 |
| Badge / Tag / Kbd | 폰트 | micro(10) | `font_size_micro` |
| MenuItem | 아이콘 글리프 | 16 | icon-size-md (15 → 16 snap), `icon_glyph_size_md` |
| (공용) | disabled opacity | 0.5 | `opacity_disabled()` |

## switch-number overlay (chrome)

디자인 `tokens/components.css` 의 `CHROME · SWITCH-NUMBER OVERLAY` 블록(8 토큰). modifier 홀드 중
탭/워크스페이스의 leading indicator(탭 아이콘 / 워크스페이스 status dot / collapsed letter avatar)를
숫자 키캡으로 제자리 교체하는 패턴. **비active 키캡은 기존 `Kbd` 키캡 그대로** (디자인도
`var(--tasty-kbd-*)` 로 alias) → tasty 의 `kbd()`(`crates/tasty-ui-widgets/src/chip.rs`) 가 이미 커버.
**신규는 active(현재 탭/ws) 키캡의 accent-filled bg/fg 뿐**이고, 그조차 기존 semantic 접근자로
표현된다 → **신규 Theme 필드 0개** (component→semantic 직접 매핑, button-primary-bg→accent_primary() 와 동일 관습).

| 디자인 토큰 | 디자인 체인 | tasty Theme / 위젯 | 비고 |
|---|---|---|---|
| `--tasty-switch-overlay-size` | → `kbd-size` → `size-16` (16px) | `switch_overlay_size()` (생성) | 키캡 footprint = 아이콘/dot slot |
| `--tasty-switch-overlay-bg` | → `kbd-bg` → `surface-raised` | `switch_overlay_bg()` (생성) | 비active 키캡 fill |
| `--tasty-switch-overlay-fg` | → `kbd-fg` → `text-secondary` | `switch_overlay_fg()` (생성) | 비active 키캡 숫자 |
| `--tasty-switch-overlay-border` | → `kbd-border` → `border-strong` | `switch_overlay_border()` (생성) | 키캡 외곽선 |
| `--tasty-switch-overlay-shadow-depth` | → `kbd-shadow-depth` → `size-2` (2px) | `switch_overlay_shadow_depth()` (생성) | 키캡 하단 3D edge |
| `--tasty-switch-overlay-active-bg` | → `accent-primary` → `color-blue` | `switch_overlay_active_bg()` (생성) | **현재 항목 = accent-filled 키캡 bg.** |
| `--tasty-switch-overlay-active-fg` | → `text-on-accent` → `color-neutral-0` | `switch_overlay_active_fg()` (생성) | accent fill 위 숫자 (`text_on_accent` 는 dark 테마면 `crust`, light 테마면 `#ffffff`(`TEXT_ON_ACCENT_LIGHT`) — button-primary-fg/checkbox-check 등과 공유) |
| `--tasty-switch-overlay-fade` | → `motion-ui-fast` → `duration-90` (90ms) | `switch_overlay_fade()` (생성, `Millis`) | 등장 90ms, release 0ms. `src/adapters/ui/switch_overlay.rs` 가 소비 |

> **결론**: switch-number overlay 8 토큰 모두 **생성 접근자 `switch_overlay_*()` 로 커버** → 신규 Theme 필드 없음. 키캡은 `tasty-ui-widgets` `chip.rs` 의 `paint_num_keycap` 한 벌이 그린다(radius·글자 크기는 `kbd-*`).

## preset split-zone (preset-editor 경계 hover-split)

디자인 `tokens/components.css:295-296` 의 `--tasty-preset-split-zone-*` 2종. 프리셋 편집기에서
surface 경계 30% 존을 hover 할 때 뜨는 밴드+분할선 색. accent-primary 는 **테마 가변색**이라
`from_rgba` 리터럴이 아니라 `HexColor::with_alpha` 로 rgb 를 보존하고 알파만 파생한다(scrim 은
고정 검정이라 `from_rgba` 지만, 이건 가변 accent 라 접근자 방식이 정답). dtcg 인제스트 완료.

| 디자인 토큰 | 디자인 체인 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-preset-split-zone-bg` | `color-mix(accent-primary 22%, transparent)` | `Theme::preset_split_zone_bg()` = `accent_primary().with_alpha(56)` | 존 밴드 채움(22%×255≈56). `PRESET_SPLIT_ZONE_BG_ALPHA` |
| `--tasty-preset-split-zone-border` | `color-mix(accent-primary 55%, transparent)` | `Theme::preset_split_zone_border()` = `accent_primary().with_alpha(140)` | 안쪽 변 2px 분할선(55%×255≈140). `PRESET_SPLIT_ZONE_BORDER_ALPHA`. 2px 굵기는 `tab_indicator_width` 재사용 |

> **비raw 치수(preview 전용 egui 좌표)**: 존 밴드 30%(`SPLIT_ZONE_EDGE=0.3`)·degrade 임계
> 46px(`SPLIT_ZONE_MIN`)·close × 14px·add-tab 22px 는 `demo_layout.rs`/`preset_editor.rs` 의
> egui logical raw f32 관행(PANE_GAP 등과 동일)으로, typed-length(PhysicalPx/LogicalPx) 규칙
> 밖이다(egui `Rect` 좌표계, tasty `Rect` 아님) — 소스에 주석 명시.

## preset leaf value summary (preset-editor 미선택 leaf 값 요약)

디자인 `tokens/components.css:335-339`·`tokens/tasty.tokens.json:2007-2023` 의
`--tasty-preset-leaf-*` 5종. 미선택 leaf 미리보기가 kind 아이콘·kind명 아래에 설정값을
`키 값` 한 줄로 요약하는 색·치수. 색 2종은 신규 component 접근자로, 폰트/gap 3종은 기존
semantic 필드를 그대로 재사용한다(신규 필드 없음). 정본 소스 `gallery/preset_editor.jsx`
(`LeafSummary`/`summaryRows`/`FIELDS`).

| 디자인 토큰 | semantic 참조 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-preset-leaf-label-fg` | `text-muted` | `Theme::preset_leaf_label_fg()` = `text_muted()` | 요약 라벨(소문자 필드 키) 색. 신규 접근자 |
| `--tasty-preset-leaf-value-fg` | `text-secondary` | `Theme::preset_leaf_value_fg()` = `text_secondary()` | 요약 값 색. 신규 접근자 |
| `--tasty-preset-leaf-label-font-size` | `font-size-micro` (10) | `Theme::font_size_micro` | 라벨 폰트(mono). 기존 필드 재사용 |
| `--tasty-preset-leaf-value-font-size` | `font-size-caption` (11) | `Theme::font_size_caption` | 값 폰트(mono). 기존 필드 재사용 |
| `--tasty-preset-leaf-summary-gap` | `space-xs` (4) | `Theme::spacing_xs` | 행↔행·kind명↔요약·라벨↔값 gap. 기존 필드 재사용 |

> **degrade 임계(preview 전용 egui 좌표, 비토큰 구조 상수)**: 요약 숨김 `96×72`
> (`LEAF_SUMMARY_MIN_W`/`LEAF_SUMMARY_MIN_H`)·아이콘만 `46`(`LEAF_ICON_ONLY_MIN`, `SPLIT_ZONE_MIN`
> 동류) 는 위 split-zone 과 동일하게 egui raw f32 관행이다. 앞자름(cwd/file)/뒤자름(startup/url)
> 방향은 `PresetFieldSpec.input`(Dir/FilePath=앞자름)으로 판정 — kind 하드코딩 없음.

## 토큰이 아닌 raw 값 주의

디자인 inline style 에는 토큰이 아닌 raw px 도 섞여 있다 (전사 시 그대로 옮기되 기록):
- remote_tool 헤더 `gap: 9` — 토큰 아님 (spacing_sm=8 과 1px 차).
- remote_tool 헤더 title `fontSize: 14` — 토큰 아님 (14 폰트 토큰은 UI 상한 `font_size_max` 뿐 → heading 13 사용, 1px 차).
- remote_tool TabBtn `height: 35` / 탭바 `padding 0 8` / 탭 `padding 0 13` / `gap 2` — raw.

## banner (banner-02 specimen / banner-03 본체)

디자인 `tokens/components.css` 의 `--tasty-banner-*` Tier-3 블록 + `tokens/primitives.css`
의 `--tasty-opacity-recessed`. 갤러리 specimen(`widgets/banner.rs`)은 **본체 Theme struct
증설 없이** 아래 표의 매핑대로 기존 접근자로 대응시킨다. 아래 표의 "갤러리
매핑" 은 specimen 이 현재 쓰는 값, "banner-03" 은 본체 구현 때 토큰화할 항목.

| 디자인 토큰 | 디자인 체인 | 갤러리 매핑(specimen) | banner-03(본체) |
|---|---|---|---|
| `--tasty-banner-bg` | → `surface-raised` → neutral-300 | `surface_raised()` | semantic 직접 매핑(신규 필드 불필요) |
| `--tasty-banner-fg` | → `text-primary` | `text_primary()` | 〃 |
| `--tasty-banner-border` | → `border-strong` | `border_strong()` | 〃 |
| `--tasty-banner-radius` | → `radius-8` (8px) | `corner_radius_lg`(8) | 기존 토큰 — 본체 배너 셸도 `corner_radius_lg` 를 쓴다(시스템 기본 4px 의 의도적 2배) |
| `--tasty-banner-shadow` | → `shadow-popover` | `shadow_popover()` | 기존 토큰 — 본체 배너 셸도 같은 접근자를 쓴다([ADR-0254](../../adr/0254-floating-surface-shadow-scope-rule.md) SCOPE RULE 의 popover 갈래) |
| `--tasty-banner-margin` | → `space-sm` → size-8 (8px) | `spacing_sm` | 기존 토큰 |
| `--tasty-banner-padding-x` | → `space-md` (12) | `spacing_md` | 기존 토큰 |
| `--tasty-banner-padding-y` | → `space-sm` (8) | `spacing_sm` | 기존 토큰 |
| `--tasty-banner-gap` | → `space-md` (12) | `spacing_md` | 기존 토큰 |
| `--tasty-banner-icon-fg` | → `text-muted` (default) | `text_muted()` (per-banner severity override) | 기존 접근자 |
| `--tasty-banner-title-font-size` | → `font-size-body` (13) | `font_size_body` | 기존 토큰 |
| `--tasty-banner-body-font-size` | → `font-size-caption` (11) | `font_size_caption` | 기존 토큰 |
| `--tasty-banner-countdown-font` | → `font-mono` | `FontId::monospace` | 기존 |
| `--tasty-banner-countdown-font-size` | → `font-size-micro` (10) | `font_size_micro` | 기존 토큰 |
| `--tasty-banner-countdown-fg` | → `text-muted` | `text_muted()` | 기존 접근자 |
| `--tasty-banner-recessed-opacity` | → `opacity-recessed` (0.4) | `opacity_recessed()` + `gamma_multiply` | 기존 접근자 — 본체도 같은 접근자를 쓴다 |
| `--tasty-banner-fade` | → `motion-ui` → duration-120 (120ms) | `banner_fade()` (생성, `Millis`) — 소비처 없음 | 본체는 페이드 없이 end-state 로 그린다 |

> **banner-03(본체) 에 추가로 필요한 신규 Theme 항목은 없다.** 한때 신규로 적혔던 셋 —
> `--tasty-banner-radius`(`corner_radius_lg`) · `--tasty-opacity-recessed`
> (`opacity_recessed()`) · `--tasty-banner-shadow`(`shadow_popover()`) — 은 모두 `Theme`
> 에 있고, 나머지 `--tasty-banner-*` 도 기존 semantic 접근자로 커버된다.

## modifier-hint 오버레이 (modifier-hint-03 specimen + 본체)

디자인 `tokens/components.css` 의 `--tasty-modhint-*` Tier-3 블록 + `tokens/primitives.css`
의 `--tasty-size-180` / `--tasty-duration-200` / `--tasty-duration-500`, `tokens/semantic.css`
의 `--tasty-motion-ui-fade`(=200ms) / `--tasty-motion-hold-reveal`(=500ms). 4분류
(Popup/Toast/Banner/Modal) 밖의 신규 요소(키보드 포커스 없음 + 마우스 인터랙티브 + 홀드
수명)라 [`docs/concepts/ubiquitous-language.md`](../../concepts/ubiquitous-language.md) 에
정의를 추가했다. 지오메트리는 `LogicalPx`(DPI 자연대응), 색은 전부 기존 semantic 접근자 재사용.
접근자는 `crates/tasty-type-appearance/src/theme.rs` 의 `modhint_*()` / `motion_hold_reveal_shift()`.

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-motion-hold-reveal` | → `duration-500` (500ms) | `modhint_hold_delay()` (생성, `Millis`) | 홀드→표시 지연. 모션 아님 → reduced_motion 무관 유지 |
| (대응 토큰 없음) | 1200ms | `motion_hold_reveal_shift()` = `MOTION_HOLD_REVEAL_SHIFT_MS` (수기, [ADR-0035](../../adr/0035-modifier-hint-combo-narrowing-and-shift-delay.md)) | **Shift 단독** 홀드만 이 지연. 타이핑 중 Shift 스침으로 팝업이 튀는 것 억제. 모션 아님 → reduced_motion 무관 |
| `--tasty-motion-ui-fade` | → `duration-200` (200ms) | `modhint_fade()` (생성, `Millis`) | 등장 페이드(opacity 0.2→1.0). reduced_motion 시 0ms |
| `--tasty-modhint-width` | → `size-180` (180px) | `modhint_width()` | 열린 사이드바 폭(기본 180)과 정렬 |
| `--tasty-modhint-height` | → 400px | `modhint_height()` | 기본 세로 높이 |
| `--tasty-modhint-min-width` | → 180px | `modhint_min_width()` | 리사이즈 최소 (= 기본 너비) |
| `--tasty-modhint-min-height` | → 240px | `modhint_min_height()` | 리사이즈 최소 |
| `--tasty-modhint-strip-height` | → `size-28` (= item-height-interactive) | `modhint_strip_height()` | 드래그 스트립 높이 |
| `--tasty-modhint-pad` | → 10px | `modhint_pad()` | 스크롤 리스트 안쪽 패딩 |
| `--tasty-modhint-section-gap` | → `space-md` (12) | `modhint_section_gap()` | 섹션 사이 |
| `--tasty-modhint-row-gap` | → 6px | `modhint_row_gap()` | 섹션 내부 행 사이 |
| `--tasty-modhint-empty-row-gap` | → 3px | `modhint_empty_row_gap()` | **신규**. 빈 조합 섹션 내부 간격(채워진 6px보다 좁게, §6-5). 디자인은 인라인 px(`.mh-section--empty{gap:3px}`) — 코드에서 토큰화 |
| `--tasty-modhint-empty-row-min-height` | → 20px | `modhint_empty_row_min_height()` | **신규**. 빈 조합 플레이스홀더 행 최소 높이(키캡 행 24px보다 타이트). 디자인 인라인 px(`.mh-empty{min-height:20px}`) — 코드에서 토큰화 |
| `--tasty-modhint-grip-size` | → `icon-size-xs` (12) | `modhint_grip_size()` | 코너 리사이즈 그립 |
| `--tasty-modhint-bg` | → `bg-panel` (불투명) | `modhint_bg()` | 라이브 출력 위 불투명 셸 |
| `--tasty-modhint-border` | → `border-strong` | `modhint_border()` | 1px 셸 보더 |
| `--tasty-modhint-radius` | → `radius` (4) | `corner_radius` | 셸 코너 |
| `--tasty-modhint-shadow` | → `shadow-popover` | `shadow_popover()` | anchored + scrim-less lift(banner 와 공유). 디자인 체인이 한때 `shadow-modal` 을 가리켜 구현과 갈려 있었으나, [ADR-0254](../../adr/0254-floating-surface-shadow-scope-rule.md) 의 SCOPE RULE 이 확정되면서 디자인이 `components.css` 의 이 토큰을 `shadow-popover` 로 재타깃했다 — 발산은 남아 있지 않다 |
| `--tasty-modhint-strip-bg` | → `bg-sidebar` | `modhint_strip_bg()` | 드래그 스트립 배경 |
| `--tasty-modhint-separator` | → `separator` | `modhint_separator()` | 스트립/헤더 하단 구분선 |
| `--tasty-modhint-held-fg` | → `text-muted` | `modhint_held_fg()` | 스트립 "held" 라벨 |
| `--tasty-modhint-role-bg` | → `surface-active` | `modhint_role_bg()` | 특수 역할 행 washed 배경 |
| `--tasty-modhint-role-fg` | → `accent-primary` | `modhint_role_fg()` | 역할 행 leading 글리프 |
| `--tasty-modhint-row-fg` | → `text-secondary` | `modhint_row_fg()` | 액션/역할 행 텍스트 |
| `--tasty-modhint-empty-fg` | → `text-muted` | `modhint_empty_fg()` | **신규**. 빈 조합 플레이스홀더("바인딩 없음") 텍스트 — row-fg(text-secondary)보다 한 단계 절제(§6-2) |
| `--tasty-modhint-agent-dot` | → `accent-agent` | `modhint_agent_dot()` | plugin 행 leading agent dot |

## tooltip / help-hint (help-hint-01 위젯 + specimen)

디자인 `tokens/components.css` 의 `--tasty-tooltip-*`(12종) + `--tasty-help-hint-*`(4종)
Tier-3 블록. 위젯 `crates/tasty-ui-widgets/src/tooltip.rs`(`Tooltip`) + `help_hint.rs`(`HelpHint`).
색·폰트·보더·패딩·글리프 크기는 모두 기존 Theme 접근자로 커버되고, **신규 Theme 필드는 2종**
(`line_height_ui`, `tooltip_max_width`)만 추가했다 — 아래 표에서 **신규** 표기.

| 디자인 토큰 | 디자인 체인 | tasty Theme / 위젯 | 비고 |
|---|---|---|---|
| `--tasty-tooltip-bg` | → `surface-raised` | `surface_raised()` | 불투명 카드 fill |
| `--tasty-tooltip-border` | → `border-strong` | `border_strong()` + `border_width`(1) | 1px 보더 |
| `--tasty-tooltip-fg` | → `text-secondary` | `text_secondary()` | 버블 텍스트 |
| `--tasty-tooltip-radius` | → `radius` (4px) | `corner_radius` | 카드 코너 |
| `--tasty-tooltip-shadow` | → `shadow-popover` | `shadow_popover().to_egui()` | lift 그림자(재사용) |
| `--tasty-tooltip-padding-y` | → `space-xs` (4) | `spacing_xs` | 세로 패딩 |
| `--tasty-tooltip-padding-x` | → `space-sm` (8) | `spacing_sm` | 가로 패딩 |
| `--tasty-tooltip-font-size` | → `font-size-caption` (11) | `font_size_caption` | 11px 텍스트 |
| `--tasty-tooltip-line-height` | → `line-height-ui` (1.4) | **신규** `line_height_ui: f32 = 1.4` (무차원 비율, zoom 무관) | UI 줄간격 배수 |
| `--tasty-tooltip-max-width` | → `size-240` (240px) | **신규** `tooltip_max_width: LogicalPx(240)` (zoom 적용 — `toast_max_width` 전례) | 초과 시 wrap |
| `--tasty-tooltip-offset` | → `space-xs` (4) | `spacing_xs` | 앵커와 간격 |
| `--tasty-tooltip-delay` | → `motion-ui-med` → duration-150 (150ms) | `tooltip_delay()` (생성, `Millis`) | hover delay. 종전에는 위젯 상수 `HOVER_DELAY_SECONDS: f64 = 0.15` 였다. fade 는 immediate-mode snap 으로 생략 |

### duration 접근자에서 값을 벗기는 자리 — 감시 대상

`Millis` 는 `Theme` 경계까지다([ADR-0176](../../adr/0176-motion-durations-cross-the-theme-boundary-as-millis.md)).
경계 밖의 단위 없는 ms 산술(순수 함수·표시 문자열)에는 `to_millis_f32()` 로 벗겨
넘긴다. **그 자리가 늘어나면** 벗기는 것이 일상이 됐다는 뜻이고, 길이 축처럼 두 번째
가드가 필요해진다(그 조건이 ADR-0176 의 재검토 트리거다).

```bash
# 분자 = 벗기는 자리 / 모수 = duration 접근자를 쓰는 자리 전체
echo "$(grep -rn 'to_millis_f32()' --include='*.rs' src crates \
        | grep -vc 'crates/tasty-type-appearance/src/motion.rs') \
      / $(grep -rnE '\.(banner_fade|modhint_fade|modhint_hold_delay|spinner_duration|status_dot_pulse_duration|switch_overlay_fade|tooltip_delay|motion_hold_reveal_shift)\(\)' \
          --include='*.rs' src crates | grep -vc 'crates/tasty-type-appearance/src/')"
```

모수를 함께 세는 이유는 분자만으로는 그 수가 큰지 작은지 알 수 없기 때문이다 —
접근자 사용이 늘어 분자도 같이 느는 것과, 사용은 그대로인데 벗기기만 느는 것은
다른 일이다.
| `--tasty-help-hint-size` | → `icon-size-sm` (14) | `icon_glyph_size_sm` | (?) 글리프 14px |
| `--tasty-help-hint-gap` | → `space-xs` (4) | `spacing_xs` | 라벨과 gap |
| `--tasty-help-hint-color` | → `text-muted` | `text_muted()` | rest 색 |
| `--tasty-help-hint-color-hover` | → `text-secondary` | `text_secondary()` | hover/focus 색 |

> **결론**: tooltip/help-hint 16 토큰 중 14 종은 기존 Theme 접근자·위젯 상수로 커버되고,
> `line-height-ui`(1.4)·`max-width-240`(240px) 2 종만 신규 Theme 필드로 승격했다. delay(150ms)
> 는 생성 접근자 `tooltip_delay()`, fade 는 immediate-mode snap 처리.
> 글리프는 SVG 자산 주입 대신 painter 직접 드로잉(`status_dot`/`spinner` 전례).

## drilldown / listctrl (S13 settings-preset-drilldown 위젯 2종)

디자인 `tokens/components.css` 의 `--tasty-drilldown-*`(8종) + `--tasty-listctrl-*`(17종)
Tier-3 블록. 위젯
`crates/tasty-ui-widgets/src/drilldown.rs`(`DrillDown`) + `listctrl.rs`(`ListCtrl`).
vendor 한 DTCG export(`crates/tasty-design-tokens/dtcg/tasty.tokens.json`)에 들어 있어 아래 접근자
24 개는 전부 생성물(`crates/tasty-type-appearance/src/generated_component.rs`)이다 — 수기 접근자가
아니다. `font-weight` 하나만 대응 접근자가 없다(아래 표). **신규 primitive/hex 없음**, 전부 기존
semantic/primitive 종착.

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-drilldown-backbar-height` | → `size-36` (36px) | `drilldown_backbar_height()` | back bar 밴드 (`ui_zoom` 적용) |
| `--tasty-drilldown-backbar-padding-x` | → `space-sm` (8) | `drilldown_backbar_padding_x()` | ← 를 콘텐츠 좌단 정렬 |
| `--tasty-drilldown-backbar-padding-y` | → `space-xs` (4) | `drilldown_backbar_padding_y()` | |
| `--tasty-drilldown-backbar-gap` | → `space-sm` (8) | `drilldown_backbar_gap()` | ← ↔ 제목 ↔ actions |
| `--tasty-drilldown-backbar-border` | → `separator` | `drilldown_backbar_border()` | 하단 헤어라인 |
| `--tasty-drilldown-title-font-size` | → `font-size-body` (13) | `drilldown_title_font_size()` | 디테일 제목 |
| `--tasty-drilldown-title-font-weight` | → `font-weight-semibold` | (없음) | egui weight 한계 — 색 강조 관례(`button.rs` semibold 관례와 동일) |
| `--tasty-drilldown-title-fg` | → `text-primary` | `drilldown_title_fg()` | |
| `--tasty-listctrl-row-min-height` | → `size-36` (36px) | `listctrl_row_min_height()` | desc 있으면 내용만큼 확장 (`ui_zoom` 적용) |
| `--tasty-listctrl-row-padding-x` | → `space-md` (12) | `listctrl_row_padding_x()` | |
| `--tasty-listctrl-row-padding-y` | → `space-sm` (8) | `listctrl_row_padding_y()` | |
| `--tasty-listctrl-row-gap` | → `space-sm` (8) | `listctrl_row_gap()` | icon ↔ text ↔ trailing |
| `--tasty-listctrl-radius` | → `radius-sm` (2) | `listctrl_radius()` | divided 시 헤어라인 행 radius 0 |
| `--tasty-listctrl-font-size` | → `font-size-body` (13) | `listctrl_font_size()` | 라벨 |
| `--tasty-listctrl-label-fg` | → `text-secondary` | `listctrl_label_fg()` | |
| `--tasty-listctrl-label-fg-active` | → `text-primary` | `listctrl_label_fg_active()` | hover/selected |
| `--tasty-listctrl-desc-fg` | → `text-muted` | `listctrl_desc_fg()` | |
| `--tasty-listctrl-desc-font-size` | → `font-size-caption` (11) | `listctrl_desc_font_size()` | |
| `--tasty-listctrl-icon-fg` | → `text-muted` | `listctrl_icon_fg()` | leading 글리프 (icon-size-md = `icon_glyph_size_md`) |
| `--tasty-listctrl-chevron-fg` | → `text-muted` | `listctrl_chevron_fg()` | drill-in chevron (icon-size-sm 슬롯, painter 폴리라인 — tree_row 전례) |
| `--tasty-listctrl-row-bg-hover` | → `overlay-hover` | `listctrl_row_bg_hover()` | premultiplied 워시 |
| `--tasty-listctrl-row-bg-selected` | → `surface-active` | `listctrl_row_bg_selected()` | |
| `--tasty-listctrl-selected-bar` | → `accent-primary` | `listctrl_selected_bar()` | 좌측 accent 바 |
| `--tasty-listctrl-selected-bar-width` | → `size-2` (2px) | `listctrl_selected_bar_width()` | (`ui_zoom` 적용) |
| `--tasty-listctrl-divider` | → `separator` | `listctrl_divider()` | 행 사이 헤어라인 |

> desc 줄과 라벨 사이 1px 간격(디자인 `.tasty-listctrl__text { gap: 1px }`)은 spacing
> 스텝 밖 구조 간격 → `tasty-ui-widgets::tokens::STRUCT_GAP_1` (primitive size-1 대응 관례).

## Remote file transfer (progress/error 09)

디자인 `tokens/components.css` 의 `--tasty-transfer-popup-width` + `--tasty-progress-*`(4종).
09 진행/실패 팝업(`popup/transfer.rs`)의 프레임 폭 + **시스템 최초 determinate progress bar**.
switch-overlay/preset-leaf 와 동일하게 **전부 기존 semantic 접근자·primitive 로 종착 → 신규 Theme
필드 0**([design-parity-notes](design-parity-notes.md) "component-tier 토큰은 신규 필드 안 만듦").

| 디자인 토큰 | 디자인 체인 | tasty Theme / 값 | 비고 |
|---|---|---|---|
| `--tasty-transfer-popup-width` | → `size-400` (400px) | 화면 전용 popup `default_size.x` const 400 (token-policy §c) | 진행+실패 프레임 폭. popup 좌표라 typed-length 밖(egui `Vec2`) |
| `--tasty-progress-height` | → `size-4` (4px) | `Theme::spacing_xs`(=4) | determinate bar 두께. size-4 = space-xs 값 일치 → 기존 필드 재사용 |
| `--tasty-progress-radius` | → `radius-sm` (2px) | `Theme::corner_radius_sm` | bar 라운드 |
| `--tasty-progress-track-bg` | → `bg-app` | `Theme::bg_app()` | recessed track(패널보다 어둡게) |
| `--tasty-progress-fill-bg` | → `accent-primary` | `Theme::accent_primary()` | determinate fill(0ms, 폭=바이트) |

> **화면 전용 raw px(token-policy §c, egui popup 좌표)**: 헤더/푸터 패딩(14/12/10)·바디 패딩(14)·
> gap(10)·헤더 콘텐츠 높이(20)는 디자인 inline raw 로 `popup/transfer.rs`·specimen module const.
> reason well 패딩(8/10)은 디자인 `padding: 8px 10px` 그대로. bar 는 `Spinner` 처럼 위젯화하지 않고
> painter 인라인(track `bg_app` + fill `accent_primary`).

## attention kind — NeedsInput/Completion (surface-highlight, ADR-0062)

디자인 `tokens/semantic.css` + `tokens/components.css` + `components/core/Badge.jsx` +
`components/feedback/StatusDot.jsx`(원본: Claude Design "attention-visuals", 2026-08-10
확정). 색은 기존 semantic accessor 를 그대로 참조 — **신규 Theme 필드 0**
([design-parity-notes](design-parity-notes.md) "component-tier 토큰은 신규 필드 안 만듦").
`--tasty-badge-group-gap` 도 `space-xs` 그대로 별칭이다 — 생성 접근자 `badge_group_gap()` 은 있으나 소비처는 없고
`Theme::spacing_xs` 를 직접 참조한다.

| 디자인 토큰 | 디자인 체인 | tasty Theme / 값 | 비고 |
|---|---|---|---|
| `--tasty-attention-needs-input` | → `accent-warning` | `Theme::accent_warning()` | NeedsInput 색(노랑) |
| `--tasty-attention-completion` | → `accent-primary` | `Theme::accent_primary()` | Completion 색(파랑) |
| `--tasty-attention-needs-input-fg` / `-completion-fg` | → `text-on-accent` | `Theme::text_on_accent()` | 두 배지 공통 전경 |
| `--tasty-attention-rank-needs-input` | `30`(정수) | `AttentionLevel::NeedsInput`(derive `Ord`) | 재도출 금지 — 소스에 정수 값 주석으로 미러링 |
| `--tasty-attention-rank-completion` | `10`(정수) | `AttentionLevel::Completion` | 위와 동일 |
| `--tasty-badge-primary-bg`/`-fg` | = attention-completion(-fg) | `accent_primary()`/`text_on_accent()` | Completion 배지 |
| `--tasty-badge-warning-bg`/`-fg` | = attention-needs-input(-fg) | `accent_warning()`/`text_on_accent()` | NeedsInput 배지 |
| `--tasty-badge-group-gap` | → `space-xs`(4px) | `Theme::spacing_xs` | 두 배지 동시 표시 시 간격. `badge_group_gap()` 은 생성됐으나 미사용 — 직접 참조 |
| `--tasty-tab-fg-needs-input`/`-completion` | = attention-* | `accent_warning()`/`accent_primary()` | 탭 제목 색 |
| `--tasty-surface-highlight-input-border` | = attention-needs-input | `accent_warning()` | surface 테두리 NeedsInput |
| `--tasty-surface-highlight-input-width` | → `focus-ring-width`(2px) | `Theme::focus_ring_width` | Completion 테두리와 동일 굵기 |
| `--tasty-surface-highlight-done-border` | = attention-completion | `accent_primary()` | 기존 완료 테두리 — 렌더 색 불변, 경유만 변경 |
| `--tasty-status-dot-needs-input`/`-completion` | = attention-* | `accent_warning()`/`accent_primary()` | collapsed rail dot |

예약(이번엔 미구현, 토큰만 존재): `error` rank 40 → `accent-danger`, `approval` rank 20 →
`accent-agent`. Catppuccin 매핑에 추가할 hue 없음 — Latte 는 기존 accent role 을 통해
자동 상속.

## MultiSelect 메뉴 크기 (forms/MultiSelect)

디자인 `tokens/components.css` 의 `--tasty-multiselect-*` Tier-3 블록 중 **메뉴 크기 두
건 + 그림자**. 세 건 다 **신규 값이 아니다** — 기존 primitive/component/semantic 종착이다.
나머지 `--tasty-multiselect-*`(트리거 치수·색·행 리듬)는 디자인 판정이 그대로
`--tasty-select-*` / `--tasty-menu-*` / `--tasty-checkbox-*` alias 라 위젯이 그 토큰을
직접 읽는다(별도 접근자 불필요).

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-multiselect-menu-max-height` | → `autocomplete-max-height` → `size-220` (220px) | `multiselect_menu_max_height()` | AutoComplete 드롭다운과 **같은 값 공유**(디자인 판정). 초과 시 내부 스크롤 |
| `--tasty-multiselect-menu-max-width` | → `size-320` (320px) | `multiselect_menu_max_width()` | 메뉴 **상자 전체**의 상한. 체인이 primitive 로 직접 닿으므로 같은 320 인 `toast_max_width` 를 빌리지 않는다(토스트 폭 재조정이 메뉴를 끌고 가는 가짜 결합 회피) |
| `--tasty-multiselect-menu-shadow` | → `shadow-popover` | `shadow_popover()` | 위젯이 직접 읽지 않는다 — 메뉴가 egui `Frame::popup` 으로 그려지므로 `apply_theme_to_egui` 가 `visuals.popup_shadow` 를 이 값으로 매핑해 같은 경로의 `ComboBox`·`popup_below_widget` 전부에 한 번에 닿는다([theme.md](theme.md) "떠 있는 표면의 그림자") |

메뉴 폭 규칙은 `min-width: 트리거` + 내용에 맞춰 max 까지 확장이고, CSS 와 같이 min 이
max 를 이긴다(트리거가 320 보다 넓으면 트리거를 따른다). 행 라벨이 남는 폭을 넘으면
`checkbox` 가 말줄임한다(디자인 `.tasty-check__label { flex:1; min-width:0; ellipsis }`).

## File handler picker (overlays-dialogs §filehandler)

디자인 canonical `gallery/overlays-shared.jsx` 의 `FileHandlerFrame` / `FhRow` / `FhGroup` /
`FhHeader` / `FhFooter`. **신규 Theme 필드 0** — 색은 전부 기존 semantic 접근자다
(`bg_panel` · `surface_raised` · `surface_active` · `border_strong` · `separator` ·
`accent_primary` · `accent_agent` · `accent_attention` · `text_primary` · `text_secondary` ·
`text_muted` · `text_disabled` · `text_on_accent`). 폰트는 `font_size_max`(제목 14) ·
`font_size_body`(이름 13) · `font_size_caption`(경로·출처·id·그룹 라벨 11), 행 반경은
`corner_radius_sm`(2).

선택 행의 2px 좌측 바는 **`listctrl_selected_bar_width` · `listctrl_selected_bar`** 다 —
`tab_indicator_width` 가 아니다. 두 토큰은 값이 같지만(2) 가리키는 역할이 다르고, 이 자리는
목록 행이라 `listctrl` 계열이다. 같은 역할의 다른 자리(`tasty_ui_widgets::listctrl` · 갤러리
`prim_listctrl` · `git_viewer` · `remote_attach/rows`)가 이미 이쪽을 쓴다. **값이 같다고 그
축이 아니다** 는 이 문서의 논지가 여기에도 걸린다.

치수는 대부분 4px 그리드 스텝 밖이라 대응 semantic 이 없다 — **화면 전용 raw px
(token-policy §c)** 로 `crates/tasty-ui-widgets/src/tokens.rs` 에 `FH_*` 이름을 붙여 두고
본체 popup 과 갤러리 specimen 이 **같은 상수를 읽는다**(정의를 둘로 두면 갈린다).

| 디자인 inline 값 | 상수 | 자리 |
|---|---|---|
| `width: 420` | `FH_FRAME_WIDTH` | 프레임 폭 |
| `padding: "14px 14px 10px"` | `FH_EDGE_PAD_X` · `FH_HEADER_PAD_TOP` · `FH_HEADER_PAD_BOTTOM` | 헤더(좌우 14 · 위 14 · 아래 10). footer·fallback 띠·empty 블록의 좌우도 `FH_EDGE_PAD_X` |
| `gap: 6` | `FH_GAP_SM` | 헤더의 제목행↔경로, 그룹 라벨↔count |
| `padding: 6` | `FH_LIST_PAD` | 목록 영역 안쪽 |
| `padding: "8px 10px"` | `spacing_sm` + `FH_ROW_PAD_X` | 행·그룹 헤딩·그룹 구분선의 좌우 |
| `gap: 10` | `FH_ROW_GAP` | 글리프 ↔ 텍스트 블록 |
| `gap: 5` | `FH_ID_LINE_GAP` | 둘째 줄 조각 사이 |
| `maxHeight: 264` | `FH_LIST_MAX_HEIGHT` | long 상태 목록 상한 |
| `height: 20`(fade) | `FH_LIST_FADE_HEIGHT` | 목록 하단 페이드 |
| `padding: "32px 14px"` | `FH_EMPTY_PAD_Y` + `FH_EDGE_PAD_X` | empty 블록 |
| `id.length > 34 … slice(-33)` | `FH_ID_ELIDE_MAX` · `FH_ID_ELIDE_TAIL` | 행 id 앞자름 |
| `opacity: 0.8` | `FH_RECENT_DIM_OPACITY` | Recent 행 글리프(비-plugin만) |

> **셸 채움은 `bg-panel` 이다 — 그것이 헤더의 Tag 를 보이게 한다.** popup 셸의 기본은
> `surface-raised` 이고 host 는 헤더 + 리스트형만 `bg-panel` 로 가른다
> (`popup::draw::popup_bg_fill`). 이 화면이 그 갈래인 이유는 형상만이 아니다 —
> 형식을 못 알아봤을 때 헤더가 다는 Tag 가 default 변형이고 그 채움이 `tag-bg`
> (= `surface-raised`) 라, 셸이 같은 값이면 그 칩이 배경에 녹는다(실측: Mocha 에서
> 두 색이 `#313244` 로 동일했다). `popup_shell_fill_keeps_the_default_tag_visible`
> 가 두 값이 갈린다는 것을 고정한다.

> **전사되지 않은 디자인 속성 하나**: 그룹 라벨의 자간. egui 에는 채널이 **있다**
> (`RichText::extra_letter_spacing` · `TextFormat::extra_letter_spacing`) — 안 건 이유는
> 값 쪽이다. 이 화면의 canonical `FileHandlerFrame` 은 평평한 핸들러 목록이라 그룹 헤딩도
> `letterSpacing` 선언도 없다(그룹은 본체가 더한 것이다). 값이 시안에 들어오기 전까지
> 대문자 · 11px · 색까지만 전사한다.

> **`FH_EDGE_PAD_X`(14) · `FH_HEADER_PAD_TOP`(14) · `FH_EMPTY_PAD_Y`(32)는 값이 스케일의
> 어떤 토큰과 겹치지만 그 축이 아니다** — 14 는 `icon_glyph_size_sm`·`font_size_max` 와,
> 32 는 정사각 요소의 한 변(`PLUGIN_AVATAR_ROW_SIZE`)과 같다. 겹치는 토큰을 부르면
> "아이콘이 커지면 여백도 커진다" 는 없는 관계가 생긴다. spacing 스텝(4·8·12·16·24)에는
> 둘 다 없다. `src/source_guards/on_scale_length_literal.rs` 의 래칫이 이 셋을 세고 있고,
> 사유는 각 상수의 doc 주석에도 값 옆에 적혀 있다.

## Rust 필드 → 호출처 (토큰 크로스워크)

디자인 시스템의 DTCG 토큰과 Rust `Theme` 필드, 그리고 실제 `th.*`/`theme.*` 호출처를 잇는 매핑 참조. [theme.md](theme.md) 의 토큰 구조를 호출처 관점에서 보충한다.

> **vendor 상태**: DTCG 토큰 파일은 `crates/tasty-design-tokens/dtcg/tasty.tokens.json` 으로 **vendor 되어 있다** (832 토큰 = primitive 123 / semantic 143 / component 566 — 수는 `crates/tasty-design-tokens/tests/freshness.rs` 가 고정). 치수 계열은 `crates/tasty-design-tokens/src/generated/` 에 const 로 생성되고 freshness·정합·색 드리프트 테스트가 CI 에서 일치를 강제한다. vendor 갱신 절차는 `crates/tasty-design-tokens/README.md`. **component tier(치수+색)는 `&Theme` 접근자로 생성돼**(`tasty-type-appearance/src/generated_component.rs`, [theme.md](theme.md) "Component tier 접근자") `tasty-ui-widgets` 위젯과 host chrome(`src/adapters/ui/`)이 소비 중.

### 구조 모델

```
DTCG:  primitive ─▶ semantic ─▶ component ─▶ UI        (3-tier)
Rust:  ThemeColors(평면 primitive) ─▶ Theme(펼친 필드 + 도출 overlay + 생성 tier 접근자) ─▶ UI
                    ▲ 색 저장은 평면 primitive, tier 는 &Theme 접근자로 재구성
```

- **색 저장은 평면 primitive.** `ThemeColors`(`crates/tasty-type-appearance/src/theme.rs`)는 catppuccin 평면 primitive(neutral ramp 12 + `placeholder` + accent hue 13 + 터미널 색 4 + ansi 16 + `surface_themes` map)만 저장한다.
- **tier 는 `&Theme` 접근자로 노출된다**: semantic 색 중 단순 primitive alias(`accent_primary()`/`surface_raised()`/`border_default()` 등)는 **생성 접근자**(`semantic_color_generated.rs` — DTCG semantic 색 토큰에서 생성), is_light 분기·도출 overlay·합성·리터럴은 수기(`text_on_accent()`/`overlay_hover()`/`scrim()` 등). component tier(치수+색)는 생성 접근자(`generated_component.rs` — `button_primary_bg()`/`button_height_lg()` 등, [theme.md](theme.md) "Component tier 접근자"). 즉 저장은 평면이나 소비는 tier 를 경유한다.
- 아직 접근자로 못 옮긴 **primitive 직접 참조**(`th.<field>`)에서는 *"이 primitive 가 지금 어떤 의미(role)로 쓰이나"* 를 코드 호출처가 들고 있다 — 같은 필드가 여러 role 로 갈린다(아래 핫스팟).
- 반투명 의미색(`hover_overlay`/`active_overlay`/`separator`)만 `is_light` 에서 **도출**된다(`derive_overlays`), primitive 가 아니다.

### 다의성 핫스팟 (Rust 필드 → 겹치는 role)

한 primitive 필드가 여러 의미 role 을 겸한다. 의미 기반 접근자([theme.md](theme.md) "Semantic 접근자 우선")로 옮길 때, 호출처별로 어느 role 인지 가려야 하는 지점이다. (필드명은 `theme.rs`, 실제 색값은 `crates/tasty-themes/src/fallback.rs` 가 출처. 현재 호출처는 `rg '\bth\.<field>\b'` 로 확인.)

| Rust 필드 | 겹치는 role | 갈래 판단 포인트 |
|-----------|-------------|------------------|
| `blue` | accent-primary · border-focus · ansi-blue | selection·hyperlink=primary, focus ring stroke=border-focus, 터미널 팔레트=ansi |
| `yellow` | accent-warning · ansi-yellow · search-match | 경고=warning, 검색 하이라이트=search-match, 팔레트=ansi |
| `red` | accent-danger · ansi-red | error/danger 버튼=danger, 팔레트=ansi |
| `green` | accent-success · ansi-green | 성공 표시=success, 팔레트=ansi |
| `mauve` | accent-agent · ansi-magenta | 에이전트 강조=agent, 팔레트=ansi |
| `surface0` | surface-raised · border-default | 채움 배경 vs 1px 선 |
| `surface1` | surface-hover · border-strong · ansi-black | hover 배경 vs 강조 선 vs 팔레트 |
| `surface2` | surface-active · selection-bg | active 배경 vs 터미널 선택 (동일값) |
| `subtext0` | text-muted (+ caption 혼용) | muted 본문 vs 보조/caption 라벨 (최다 호출) |
| `overlay1` | text-disabled (+ recording 강조) | 비활성 텍스트 vs keybinding 녹화 강조 |
| `text` | text-primary · ansi-bright-white | UI 본문 vs 팔레트 |
| `subtext1` | text-secondary · ansi-white | 보조 텍스트 vs 팔레트 |

### ANSI 팔레트는 배열로 한 번에 전달

ANSI 16색은 개별 `th.*` 호출이 아니라 `theme.ansi_palette()` 배열로 GPU 렌더러(`src/gfx/gpu/render_pass.rs`)에 한 번에 넘어간다. 다수가 neutral/accent 필드와 **동일값**이지만 별도 필드다(`ansi_black`=`surface1`, `ansi_blue`=`blue` 등 — 위 핫스팟의 "ansi-*" role).

### surface kind 색은 `surface_themes` map

터미널/마크다운의 focused/unfocused × bg/fg 색은 `ThemeColors.surface_themes: BTreeMap<String, SurfaceTheme>` 에 들어가 `theme.surface("terminal")` / `theme.surface("markdown")` 헬퍼로 읽는다. `focused_bg` 만 black/white role-remap(light/dark).

### neutral ramp 12단 ↔ ThemeColors 필드

DTCG `primitive.color-neutral-*` 는 **elevation role 기준 넘버링**(0 = 최심 배경, 1100 = 최강 전경 — TOKENS.md)이며, catppuccin 평면 필드와 아래처럼 1:1 대응한다. 이 표가 색 드리프트 테스트(`crates/tasty-design-tokens/tests/color_drift.rs`)의 전거다. (`placeholder` 필드는 ramp 밖 — DTCG primitive 미대응.)

| DTCG primitive | ThemeColors 필드 | | DTCG primitive | ThemeColors 필드 |
|---|---|---|---|---|
| `color-neutral-0` | `crust` | | `color-neutral-600` | `overlay0` |
| `color-neutral-100` | `mantle` | | `color-neutral-700` | `overlay1` |
| `color-neutral-200` | `base` | | `color-neutral-800` | `overlay2` |
| `color-neutral-300` | `surface0` | | `color-neutral-900` | `subtext0` |
| `color-neutral-400` | `surface1` | | `color-neutral-1000` | `subtext1` |
| `color-neutral-500` | `surface2` | | `color-neutral-1100` | `text` |

accent hue 13종(`color-blue` … `color-rosewater`)은 동명 필드와 1:1 (테마당 hue 별 1값, ramp 없음).

### vendor 후 남은 것

vendor·치수 codegen·드리프트 테스트는 완료됐다 (`crates/tasty-design-tokens`). 남은 것:

- DTCG semantic **색** 토큰 ↔ Rust 접근자 전수표. 과거 이 문서가 "Rust 미대응"으로 꼽았던 것 중 `text-on-accent` → `Theme::text_on_accent()`, `radius-sm` → `SIZING.corner_radius_sm` 은 **이미 구현되어 있다** (stale 정정). `radius-pill`/`motion-*`(semantic — component 모션은 `banner_fade()` 등 생성 접근자로 있다)/`ui-scale-*`/`brand-*`(`brand-melon-flesh` → `brand_melon_flesh()` 만 있다) 등은 여전히 Theme 표면 부재.
- component tier(버튼/입력/탭/토스트…) ↔ 호출처 매핑, SIZING 소비처의 토큰 참조 전환.

#### ★ SIZING 소비처 토큰 전환 전 필독 — 그 전환에는 **픽셀 변경이 섞여 있다**

**이 전환은 "연결" 이 아니라 부분적으로 재디자인이다.** vendor 된 값과 그 자리가 지금
그리는 값이 다르면 연결하는 순간 화면이 바뀌고, 그건 전환이 아니라 **디자인 결정**이라
전환 커밋이 곁다리로 할 수 없다. **지금까지 다섯이 확인됐고(아래에서 모두 닫혔다) 나머지는 미측정이다** — 그 둘을
같이 읽어라. 다섯은 상한이 아니라 지금까지 연 자리의 수다.

**같은 형태를 이 레포는 이미 한 번 겪었다** — 폰트 축에서 "값이 바뀌는 치환은 하나도
없다" 는 주장과 함께 올라온 묶음에 실제로는 ±0.5~1.0 변경이 10 자리 섞여 있었고,
그 결과가 [ADR-0126](../../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md) 이다.
그 ADR 의 결론(스케일 밖 값은 스냅하지 않고 사유를 적은 명명 const 로 둔다)은 폰트 ·
코너 반경 · 점 치수 세 축에 적용돼 있다.

**전환 전에 값 대조부터 한다.** 접근자를 부르기 전에 그 자리가 지금 그리는 값을 재고,
토큰 값과 다르면 연결하지 말고 디자인 판단을 받는다.

실측(2026-09-05, 치수 접근자 208): 접근자 이름으로 불리는 것 125 · **base 필드 이름으로
불리는 것 53** · 자체 계산이면서 아무 이름으로도 안 불리는 것 30.

★ **"안 불리는 접근자" 를 세는 것으로는 이 위험을 못 잰다.** 접근자가 안 불려도 같은 값이
base 필드 이름으로 이미 불리고 있을 수 있고(예: `titlebar-caption-width` ↔ `caption_width`),
반대로 **base 이름으로 불리고 있어도 그 컴포넌트 자리는 다른 값을 그리고 있을 수 있다.**
**호출 여부와 값 일치는 독립이다.**

값이 어긋나는 것으로 **확인된** 자리는 **한 줄도 안 남았다** — 2026-09-17 디자인 결정
(D1~D7)이 다섯을 모두 닫았다.

닫힌 다섯은 이렇게 갈렸다 — `tab-dot-size` 는 토큰이 `status-dot-size-compact`(6)를
가리키도록 바뀌어 자리가 맞았고(D2), `toast-gap`(D1) · `status-dot-attached-ring-width`(D4) ·
`-offset`(D5) · 상태바 점(D3, 7→`statusbar-dot-size` 6)은 **자리가 토큰 값으로 옮겨 갔다**
(6→8 · 1.5→2 · 1.5→2 · 7→6, 전부 의도된 시각 변화).

**확인된 자리가 다 닫혔다는 것이 "어긋난 자리가 없다" 는 뜻은 아니다** — 열린 확인 자리가 없다는
뜻이다. 아래 "미측정" 문단이 그 차이를 든다. 새로 확인되면 그 자리의 상수 doc 이 같은
사실을 적어야 한다: 값이 그리드 스케일 밖이라는 것과, 겨냥하는 토큰이 실재하되 값이
어긋난다는 것은 다른 물음이라 갈라 적는다. 어느 쪽이 맞는지는 디자인이 정하고, 그때까지
상수는 값을 지키고 이름만 남긴다(ADR-0126).

나머지는 **미측정**이다 — 0 이 아니라 안 잰 것이다. 이름이 같은 const 를 기계로 대조해
봤지만 `GAP` · `POPUP_WIDTH` · `MIN_HEIGHT` 같은 흔한 이름이 무관한 토큰에 무더기로 걸려
(예: `component.transfer-popup-width` ↔ 파일 피커의 `POPUP_WIDTH`) 그 수는 위험의 크기가
아니라 이름 충돌의 크기였다. **이 축의 판정은 이름이 아니라 정의를 열어야 선다.**

**남은 줄은 소스에 기록돼 있다 — 그 상수의 doc 주석에.** 그런데 전환을 하는 쪽은 상수
주석이 아니라 이 문서를 읽는다. 그래서 여기 옮겨 적는다. 반대 방향도 성립한다 — 이 표에
줄을 더할 때 그 상수의 doc 도 같이 적어야 두 자리가 갈리지 않는다.

### 관련

- [theme.md](theme.md) — Theme 2계층 모델 + UI 디자인 규칙
- `crates/tasty-design-tokens/` — vendor json + 치수 codegen + 드리프트 가드 (갱신 절차는 crate README)
- 코드: `crates/tasty-type-appearance/src/theme.rs` (필드) · `crates/tasty-themes/src/fallback.rs` (mocha 색값)
