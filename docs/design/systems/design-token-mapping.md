# Design Token → tasty Theme 매핑

디자인 정본(tasty-page `vendor/`)의 `tokens/semantic.css`·`tokens/primitives.css`와 Tasty Theme의 대응표다. 색의 hex 값은 mocha 기준이며 테마마다 달라지므로 코드에서는 Theme 접근자를 사용한다.

아이콘 SVG는 `tasty-icons`에서 정의하며 사용하는 곳에서 도형을 다시 만들지 않는다. [아이콘 가이드](icons.md)를 따른다.

## 색

| 디자인 토큰 | tasty Theme | mocha hex | 비고 |
|---|---|---|---|
| `bg-sidebar` | `mantle` | `#181825` | 사이드바·**탭 바** 등 한 단계 더 어두운 면 |
| `bg-panel` | `base` | `#1e1e2e` | 패널형 팝업 본문 (remote_tool / port_scanner / file_handler_picker / file picker / 탐색기 Properties / 전송 진행·실패 / DAG 목록). DAG 목록의 검색 칸은 이 바탕 위 기본 바탕 Input 이다(시안 `DagWindow`). 실제 배경 선택은 다음 함수가 정한다: `popup::draw::popup_bg_fill` |
| `surface-raised` | `surface0` | `#313244` | 카드·입력·메뉴·command_palette 본문, secondary 버튼 fill |
| `border-default` | `surface0` | `#313244` | |
| `border-strong` | `surface1` | `#45475a` | 팝업 외곽선·강한 구분 |
| `separator` | 흰색 8% 알파 | — | **구역 bg 위 블렌드.** premultiplied `Theme.separator`를 `to_egui_premultiplied()`로 그린다. 예: bg-panel(base) 위 (48,48,62), 대비 1.264:1. 카드 안 구획선 전용이며 팝업 바깥 경계는 `border-strong`이다 |
| `text-primary` | `text` | `#cdd6f4` | 터미널 일반 찾기 매치 안의 기본 글자 셀도 이 값을 쓴다 |
| `text-secondary` | `subtext1` | `#bac2de` | |
| `text-muted` | `subtext0` | `#a6adc8` | |
| `text-disabled` | `overlay1` | `#7f849c` | **비활성 상태의 글자·아이콘 색.** 모든 disabled 라벨·글리프가 이 role 을 읽는다 |
| `state-disabled-fg` | `state_disabled_fg()` → `text-disabled` | `#7f849c` | disabled 컨트롤의 잉크(라벨·아이콘·체크·thumb). component `button-disabled-fg` 가 가리킨다 |
| `state-disabled-fill` | `state_disabled_fill()` → `surface-raised` | `#313244` | disabled 컨트롤의 박스 채움. 모든 Button 변형(ghost 제외)·solid IconButton·Input·Select 트리거·Checkbox 박스·Switch 트랙이 같은 채움을 쓴다. component `button-disabled-bg` |
| `state-disabled-border` | `state_disabled_border()` → `border-default` | — | disabled 컨트롤의 박스 테두리. component `button-disabled-border` |
| `glyph-dim` | `overlay0` | `#6c7086` | 덜 강조할 창 아이콘(사이드바 dim chevron·dim 아이콘·상태바 글리프)과 DAG 빈 상태 글리프. `text-placeholder` 와 값만 같고 role 이 다르다 — **disabled 용이 아니다** |
| `border-frame` | `surface2` | `#585b70` | 틀의 선 **넷**(popup 프레임 · titlebar 아래 선 · pane divider · GPU 비활성 보더). popup **내부** 구분선은 `border-strong` 에 남는다. 시안이 메뉴형으로 그리는 host 팝업(Tools 메뉴 · 레일 카테고리 메뉴 · 검색 바 · 마우스 캡처 배너 더보기 메뉴)의 셸 테두리도 `border-strong` 이다(`popup/draw.rs` 의 `popup_border`). `surface-active` 와 값만 같다 |
| `accent-decorative` | `peach` | `#fab387` | 헤더 장식 accent(Plugins 창 헤더 glyph). `accent-attention`(주의 환기)과 값만 같다 |
| `accent-primary` | `accent_primary()` | `#89b4fa` | primary 버튼·포커스·활성 탭 언더라인 |
| `accent-danger` | `accent_danger()` | — | |
| `accent-move` | `pink` | `#f5c2e7` | 이동 대기 대상 표시(`move-source-ring`·`move-source-glyph`). 상태나 알림 종류가 아닌 대기 중 동작 표시라 다른 accent와 겹치지 않는 색을 쓴다. Latte `#ea76cb` |
| `accent-data` | `teal` | `#94e2d5` | DAG 입력 binding 엣지(`dag-edge-binding`). 한 task 의 값이 다른 task 로 넘어간다는 데이터 흐름 표시이며 상태색이 아니다 |
| `accent-route` | `lavender` | `#b4befe` | DAG 전이 엣지(`dag-edge-transition`). 결과로 고른 경로 표시. `accent-attached`(`border_attached()`)와 primitive 가 같아도 역할이 다르다 |
| `os-macos-close` | `accent_macos_close()` | `#ec6a5e` | macOS 신호등 close (테마 불변 OS 리터럴, const `OS_MACOS_CLOSE`) |
| `os-macos-min` | `accent_macos_min()` | `#f4bf4f` | macOS 신호등 minimize (const `OS_MACOS_MIN`) |
| `os-macos-zoom` | `accent_macos_zoom()` | `#61c554` | macOS 신호등 zoom (const `OS_MACOS_ZOOM`) |

## 채움이 정해진 자리의 글자 역할

`tasty-type-appearance/src/state_ink.rs`의 수기 접근자가 Mocha·Latte의 값을 `band()`로 나누고 color-mix를 계산한다. 값의 정본은 `dtcg/tasty.tokens.json`이다.

| 역할 | 값 | 쓰는 자리 |
|---|---|---|
| `text-on-active` · `text-on-active-hover` | Mocha `text-primary`·white, Latte neutral-1100을 검정 쪽으로 혼합 | `surface-active`와 그 hover·press 위 글자와 모양 글리프. 터미널 선택 칸과 포인터 아래 링크의 기본 글자도 이 값이다 |
| `accent-primary-on-active` | blue와 white 혼합, Latte는 blue를 검정 쪽으로 혼합 | `surface-active` 위 선택 표지와 일반 강조 글리프, 쉬는 일치 구간 |
| `accent-warning-on-active` · `accent-success-on-active` · `accent-info-on-active` · `accent-remote-on-active` | Mocha 각 accent, Latte 각 hue를 검정 쪽으로 혼합. remote는 info를 따른다 | `surface-active` 위 경고·성공·이미지·원격 글리프 |
| `accent-danger-on-active` · `accent-agent-on-active` · `accent-attached-on-active` | hue와 white 혼합, Latte는 검정 쪽으로 혼합 | `surface-active` 위 오류·plugin·점유 글리프 |
| `accent-*-raised` | Mocha 각 accent, Latte 각 hue를 검정 쪽으로 혼합 | hue 글자. 모든 평상 바탕과 surface-raised |
| `accent-*-strong` | hue를 Mocha는 white, Latte는 검정 쪽으로 더 혼합 | surface-raised의 hover·press 위 hue 글자 |
| `text-muted-raised` · `text-secondary-raised` | Mocha 각 역할, Latte neutral-1100 | surface-raised와 mantle·base의 hover·press 위 글자 |
| `text-quiet` | Mocha neutral-700, Latte neutral-1000 | 조용한 글자 역할 |
| `glyph-dim` · `glyph-dim-raised` · `status-idle` | Mocha neutral-600·700·600, Latte neutral-900 | 약한 글리프와 idle 점 |
| `surface-field` | Mocha `surface-raised`, Latte neutral-200 | 입력 상자 채움 |
| `border-control` · `border-control-raised` | Mocha neutral-600·700, Latte neutral-800 혼합·neutral-900 | 컨트롤 테두리. 3:1 |
| `control-thumb` · `control-thumb-raised` | Mocha neutral-1100·white, Latte white | Switch thumb |
| `scrollbar-thumb` · `-raised` · `-hover` | `border-control` · `border-control-raised` · `text-muted-raised` | 스크롤바 thumb |
| `accent-success-fill` · `attention-needs-input-fill` · `tint-*-bg` | Mocha 원 역할, Latte 혼합. 틴트는 tint-fill 비율의 알파 | 틴트 상자와 채움 |
| `vi-cursor-fg` | Mocha neutral-0, Latte black | 터미널 vi 커서 칸의 글자 |
| `surface-on-raised` | Mocha `surface-hover`, Latte neutral-200 | `banner-button-bg` |

`text-muted`는 Latte에서 neutral-1000, Mocha에서 neutral-900이다. `text_muted()`는 분기가 있어 `state_ink.rs`에 수기로 쓴다.

선택·키보드 활성 행은 위젯별 토큰이 있으면 그 토큰을 읽는다.

| 위젯 | 글자 | 글리프 |
|---|---|---|
| TreeRow | `tree_row_fg_active()`, chevron·meta `tree_row_part_fg_active()` | `tree_row_icon_fg_active()` |
| Table | `table_row_fg_selected()` | 셀이 색을 명시하면 `table_cell_ink()`로 읽는다 |
| ListCtrl | `listctrl_label_fg_active()`, 설명 `listctrl_part_fg_selected()` | 아이콘·chevron `listctrl_part_fg_selected()` |
| MultiSelect | `multiselect_row_fg_active()` | 없음 |
| MenuItem | `menu_item_fg_hover()`, 단축키·danger `menu_item_part_fg_active()` | 아이콘 `menu_item_part_fg_active()`, 체크 `menu_item_check_fg()` |
| AutoComplete | `autocomplete_row_fg_active()`, 일치 구간 `autocomplete_part_fg_active()`에 `autocomplete_match_underline_width()` 밑줄 | 아이콘 `autocomplete_part_fg_active()` |
| Explorer 이름 | `text_on_active()`, 일치 구간 `explorer_match_fg_active()`에 같은 밑줄 | 파일 종류 글리프 `text_on_active()` |
| StatusDot | 라벨 `table_row_fg_selected()` | `status_dot_<idle, success, warning, danger, agent, needs_input, completion>_active()`, 점유 고리 `status_dot_attached_ring_active()` |
| 포트 즐겨찾기 별 | 없음 | 켜짐 `port_star_on_active()`, 꺼짐 `port_star_off_active()` |
| plugin 표지 | 없음 | `accent_agent_on_active()` |
| 사이드바 미러 | 없음 | `workspace_mirror_fg_active()` |
| Kbd | 키 글자는 키캡 채움 위라 그대로 | 사이 `+` `kbd_plus_fg_active()` |
| 수식키 안내 역할 행 | `modhint_role_text_fg()` | `modhint_role_fg()` |

Notification 행의 `notification_item_fg_selected()`·`notification_item_unread_dot_selected()`와 `button_ghost_active_fg()`는 접근자만 있다. 앱의 Notification 행에는 선택 상태가 없고, 활성 행 위에 놓이는 ghost 버튼도 없다.

Tag는 모든 상태 variant가 `tag_bg()` 위에 그리고 테두리는 accent 40%를 유지한다. 글자는 `tag_success_fg()`·`tag_warning_fg()`·`tag_danger_fg()`·`tag_info_fg()`·`tag_attached_fg()`이고 상태 점과 앞 글리프도 같은 색이다. 자체 채움이 있어 선택 행에서도 바뀌지 않는다. 원격 미러 Tag는 `info`, remote attach의 점유 표시는 `attached`, inactive 표시는 `warning` variant다. `tag_attention_fg()`는 대응 variant가 앱에 없어 읽는 곳이 없다.

## 불투명도

| 디자인 토큰 | tasty Theme | 값 | 비고 |
|---|---|---|---|
| `state-dim-opacity` | `state_dim_opacity()` | `0.5` | 흐리게 표시하는 **항목**(preset 편집 중 비활성 영역)의 디밍. const `OPACITY_DISABLED`. `preset-cfg-dim-opacity`(`preset_cfg_dim_opacity()`)가 이 값을 가리킨다. 사용자가 읽는 글자는 곱하지 않는다. 꺼진 hook 행과 plugin 행은 `text-muted`, 잘라내기 대기는 `cut-pending-fg`(→ `text-muted`)와 `cut-pending-font-style`(italic)을 쓴다. `cut-pending-opacity`와 `dag-edge-dim-opacity`는 DEPRECATED이며 읽는 코드가 없다. **disabled 컨트롤 신호가 아니다** — disabled 컨트롤은 아래 `state-disabled-*` 박스와 잉크로 그리고 opacity를 곱하지 않는다 |
| `opacity-dimmed` | `opacity_dimmed()` | `0.75` | 진행 중이라 물러난 내용(재로드 중인 배너 본문). const `OPACITY_DIMMED`. 흐려져도 4.5:1 대비를 유지한다 |
| `tint-fill-alpha` | `tint_fill_alpha()` | `0.12` | accent 로 옅게 깐 채움. const `TINT_FILL_ALPHA` |
| `tint-border-alpha` | `tint_border_alpha()` | `0.36` | 같은 accent 의 테두리. const `TINT_BORDER_ALPHA`. **짝으로 쓰는 것이 기본**이고 승인된 부분 사용 하나(채움만: warning callout·misc/remote/script 배지) |

## 치수 (px → LogicalPx)

| 디자인 토큰 | 값 | tasty |
|---|---|---|
| space xs / sm / md / lg | 4 / 8 / 12 / 16 | `spacing_xs/sm/md/lg` |
| control-height (버튼·입력) | 28 | |
| control-height-tab | 24 | |
| control-height-tree | 22 | |
| font-size body / caption / heading | 13 / 11 / 13(weight 600) | `font_size_body/caption/heading` |
| font-size micro | 10 | `font_size_micro` — Badge/Tag/Kbd·command_palette footer 힌트 |
| font-size prose-h1 | 20 | `font_size_prose_h1` — markdown 헤딩 사다리의 h1 앵커(`render.rs::heading_sizes_px` 가 h1↔`font-size-body`(h6) 사이를 CSS 로 5단계 선형보간, UI cap 면제). `prose-h2`·`line-height-prose` 는 [WebView 호스트 계약](webview.md) 이전 egui_commonmark 시절 라이브러리 제약으로 은퇴한 토큰이며, webview 전환(CSS) 이후로도 부활하지 않았다 — 헤딩 사다리는 이제 `--md-h1`..`--md-h6` CSS custom property 로 직접 표현되므로 별도 semantic 토큰이 필요 없다. 정본은 여전히 tokens/·vendor json·생성 const 모두에서 제거된 상태 |
| font-size term-sm / term / term-lg | 12 / 14 / 16 | `font_size_term_sm` / `font_size_term` / `font_size_term_lg` — 터미널 스케일 |
| icon-size xs | 12 | `icon_glyph_size_xs` |
| icon-size md | 16 | `icon_glyph_size_md` (기존) — Button leading/trailing·MenuItem 글리프, **spinner 지름**(폰트가 아니라 아이콘 가족) |
| icon-size lg | 24 | `icon_glyph_size_lg` — 빈 상태·오류 안내의 가운데 글리프. component `center-state-glyph-size` 가 이 값을 읽는다 |
| font-size brand-display | 30 | `font_size_brand_display` — UI 14px 상한의 브랜딩 예외 둘 중 하나. 지금 이 접근자를 읽는 화면은 없다(첫 실행 셸 설정 화면은 로딩 화면과 같은 락업 크기를 쓴다) |
| status-dot-size-compact | 6 | `status_dot_size_compact()` — 24px 크롬 안의 점(pane 탭 strip·상태바·접힌 rail). `tab_dot_size()`·`statusbar_dot_size()` 가 이 값의 별칭 |
| statusbar-theme-glyph / statusbar-glyph | → `glyph-dim` | 상태바 글리프의 **색** role 둘. 앞의 것은 우측 테마 토글의 `sun`/`theme` 글리프가(색 점은 2026-09-17 결정으로 없어졌다), 뒤의 것은 좌측 브랜치 글리프가 읽는다 |
| statusbar-glyph-size | → `icon-size-xs` (12) | 상태바 인라인 글리프의 **크기** role(2026-09-20 결정). 바에는 글리프 색 role 만 있고 크기 role 이 없어 위젯이 `icon-size-xs` 를 직접 읽고 있었다 — 그 한 자리를 이 role 로 바꿨고, 바의 글리프는 전부 거기서 크기를 받는다. 배율은 바의 다른 글자와 같이 탄다(`round(12 × s)` = 10 / 12 / 14). **바 높이 24 는 배율 밖**이고 이 role 과 무관하다. 테마 분기 없음 — 치수는 어느 테마가 켜졌는지에 의존하지 않는다 |
| toolbar-height | 32 | `toolbar_height` — 뷰 상단 도구 막대 높이(배율 적용). component `git-toolbar-height` 가 이 role 의 alias 다 |
| git-toolbar-height | 32 | `git_toolbar_height()` — git viewer diff 툴바. **`control-height`(28) 아님**(컨테이너 role) |
| port-process-col-min-width | 200 | `port_process_col_min_width()` — port scanner Process 컬럼 **최소폭** |
| port-addr-col-min-width | 140 | `port_addr_col_min_width()` — port scanner Address 컬럼 폭(하한 = 폭, 늘지 않음) |
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
| (공용) | disabled 박스 | fill · border · fg | `state_disabled_fill()` · `state_disabled_border()` · `state_disabled_fg()` — opacity 없음 |

## switch-number overlay (chrome)

`tokens/components.css`의 `CHROME · SWITCH-NUMBER OVERLAY`는 보조키를 누르는 동안 탭 아이콘·workspace 상태 점·접힌 레일 아바타를 숫자 키캡으로 바꾼다. 8개 토큰은 아래 생성 접근자로 읽는다. 비활성 키캡은 Kbd와 같고 현재 항목은 accent 채움과 전경색을 사용한다.

| 디자인 토큰 | 디자인 체인 | tasty Theme / 위젯 | 비고 |
|---|---|---|---|
| `--tasty-switch-overlay-size` | → `kbd-size` → `size-16` (16px) | `switch_overlay_size()` (생성) | 키캡 footprint = 아이콘/dot slot |
| `--tasty-switch-overlay-bg` | → `kbd-bg` → `surface-raised` | `switch_overlay_bg()` (생성) | 비active 키캡 fill |
| `--tasty-switch-overlay-fg` | → `kbd-fg` → `text-secondary-raised` | `switch_overlay_fg()` (생성) | 비active 키캡 숫자 |
| `--tasty-switch-overlay-border` | → `kbd-border` → `border-strong` | `switch_overlay_border()` (생성) | 키캡 외곽선 |
| `--tasty-switch-overlay-shadow-depth` | → `kbd-shadow-depth` → `size-2` (2px) | `switch_overlay_shadow_depth()` (생성) | 키캡 하단 3D edge |
| `--tasty-switch-overlay-active-bg` | → `accent-primary` → `color-blue` | `switch_overlay_active_bg()` (생성) | **현재 항목 = accent-filled 키캡 bg.** |
| `--tasty-switch-overlay-active-fg` | → `text-on-accent` → `color-neutral-0` | `switch_overlay_active_fg()` (생성) | accent fill 위 숫자 (`text_on_accent` 는 dark 테마면 `crust`, light 테마면 `#ffffff`(`TEXT_ON_ACCENT_LIGHT`) — button-primary-fg/checkbox-check 등과 공유) |
| `--tasty-switch-overlay-fade` | → `motion-ui-fast` → `duration-90` (90ms) | `switch_overlay_fade()` (생성, `Millis`) | 등장 90ms, release 0ms. `src/adapters/ui/switch_overlay.rs` 가 소비 |

> `kbd-fg`는 키캡 채움(`kbd-bg` → surface-raised) 위 글자이고, 키캡 사이 `+`는 쉬는 바탕에서 `kbd_plus_fg()`(→ `text-muted-raised`), 선택 행에서 `kbd_plus_fg_active()`(→ `text-on-active`)다.

> **결론**: switch-number overlay 8 토큰 모두 **생성 접근자 `switch_overlay_*()` 로 커버** → 신규 Theme 필드 없음. 키캡은 `tasty-ui-widgets` `chip.rs` 의 `paint_num_keycap` 한 벌이 그린다(radius·글자 크기는 `kbd-*`).

## preset split-zone (preset-editor 경계 hover-split)

디자인 `tokens/components.css:295-296` 의 `--tasty-preset-split-zone-*` 2종. 프리셋 편집기에서
surface 경계 30% 존을 hover 할 때 뜨는 밴드+분할선 색. accent-primary 는 **테마 가변색**이라
`from_rgba` 리터럴이 아니라 `HexColor::with_alpha` 로 rgb 를 보존하고 알파만 파생한다(scrim 은
고정 검정이라 `from_rgba` 지만, 이건 가변 accent 라 접근자 방식이 정답). dtcg 인제스트 완료.

| 디자인 토큰 | 디자인 체인 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-preset-split-zone-bg` | `color-mix(accent-primary 22%, transparent)` | `Theme::preset_split_zone_bg()` = `accent_primary().with_alpha(56)` | 존 밴드 채움(22%×255≈56). `PRESET_SPLIT_ZONE_BG_ALPHA` |
| `--tasty-preset-split-zone-border` | `color-mix(accent-primary 55%, transparent)` | `Theme::preset_split_zone_border()` = `accent_primary().with_alpha(140)` | 안쪽 변 2px 분할선(55%×255≈140). `PRESET_SPLIT_ZONE_BORDER_ALPHA`. 굵기는 `--tasty-preset-split-divider-width` → `Theme::preset_split_divider_width()`(= `selection_edge_width`, hairline, 배율 제외) |
| `--tasty-preset-leaf-selected-ring-width` | `focus-ring-width` | `Theme::preset_leaf_selected_ring_width()`(= `focus_ring_width`, 배율 적용) | 선택한 leaf 를 감싸는 2px accent 링 |

미리보기의 분할 영역 비율은 `SPLIT_ZONE_EDGE=0.3`(30%)이며 무차원 f32다. 길이인 `SPLIT_ZONE_MIN` 46px, 닫기 × 14px, 탭 추가 22px 등은 `demo_layout.rs`·`preset_editor.rs`의 LogicalPx 상수를 사용한다. egui에 넘기는 최종 좌표가 f32라는 이유로 내부 길이 타입 규칙에서 제외하지 않는다.

## explorer drop target 채움

`--tasty-explorer-drop-target-bg`는 accent-primary와 `transparent`를 tint-fill 비율로 섞는 식이라 생성기가 건너뛴다. preset split-zone과 같은 방식으로 accent의 rgb를 보존하고 알파만 낮추는 수기 접근자를 둔다.

| 디자인 토큰 | 디자인 체인 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-explorer-drop-target-bg` | `color-mix(accent-primary × tint-fill-alpha, transparent)` | `Theme::explorer_drop_target_bg()` = `accent_primary().with_alpha(31)` | 12%×255≈31. 행 fill과 잉크 아래에 칠한다(`DropTintSlot`, 링은 마지막). `EXPLORER_DROP_TARGET_BG_ALPHA`. 시험이 `TINT_FILL_ALPHA`의 반올림과 같은지 확인한다 |

## 창 파일 드롭 오버레이

`--tasty-file-drop-overlay-bg`·`--tasty-file-drop-overlay-border`도 accent-primary와 `transparent`를 섞는 식이라 생성기가 건너뛴다. 같은 방식으로 알파만 낮추는 수기 접근자를 둔다. 글자·글리프 색 `--tasty-file-drop-overlay-fg`(accent-primary)는 생성 접근자다. 세 토큰은 본체와 갤러리가 함께 부르는 `tasty_ui_widgets::file_drop_overlay` 가 쓴다 — 터미널 영역 안쪽 `spacing-sm` 상자에 채움과 1px 테두리, 가운데에 download 글리프(`icon-size-md`) + `spacing-sm` + 라벨(`font-size-body`).

| 디자인 토큰 | 디자인 체인 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-file-drop-overlay-bg` | `color-mix(accent-primary × tint-fill-alpha, transparent)` | `Theme::file_drop_overlay_bg()` = `accent_primary().with_alpha(31)` | 12%×255≈31. `FILE_DROP_OVERLAY_BG_ALPHA` |
| `--tasty-file-drop-overlay-border` | `color-mix(accent-primary × tint-border-alpha, transparent)` | `Theme::file_drop_overlay_border()` = `accent_primary().with_alpha(92)` | 36%×255≈92. `FILE_DROP_OVERLAY_BORDER_ALPHA`. 시험이 두 알파가 `TINT_FILL_ALPHA`·`TINT_BORDER_ALPHA`의 반올림과 같은지 확인한다 |

## explorer 영역 선택 사각형

`--tasty-explorer-marquee-bg`·`--tasty-explorer-marquee-border`도 accent-primary와 `transparent`를 섞는 식이라 생성기가 건너뛴다. 같은 방식으로 알파만 낮추는 수기 접근자를 `crates/tasty-type-appearance/src/explorer_marquee.rs` 에 둔다. 현재 항목 테두리(`explorer-cursor-ring`·`-width`)와 자동 스크롤 띠(`explorer-autoscroll-zone`)는 생성 접근자다. 동작은 [Explorer](../../surfaces/explorer/index.md#현재-항목--영역-선택)에 있다.

| 디자인 토큰 | 디자인 체인 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-explorer-marquee-bg` | `color-mix(accent-primary × tint-fill-alpha, transparent)` | `Theme::explorer_marquee_bg()` = `accent_primary().with_alpha(31)` | 12%×255≈31. `EXPLORER_MARQUEE_BG_ALPHA` |
| `--tasty-explorer-marquee-border` | `color-mix(accent-primary × tint-border-alpha, transparent)` | `Theme::explorer_marquee_border()` = `accent_primary().with_alpha(92)` | 36%×255≈92. `EXPLORER_MARQUEE_BORDER_ALPHA`. 시험이 두 알파가 `TINT_FILL_ALPHA`·`TINT_BORDER_ALPHA`의 반올림과 같은지 확인한다 |

## 승인 위험 선택지

`--tasty-approval-danger-bg`는 accent-danger를 tint-fill 비율로 bg-panel에 섞는 불투명 식이라 생성기가 건너뛴다. DAG 상태 채움과 같은 `mix_srgb`로 계산하는 수기 접근자를 `crates/tasty-type-appearance/src/approval_danger.rs` 에 둔다. 테두리·글자색은 생성 접근자다. 동작은 [Human handoff](../../features/human-handoff/index.md#severity-별-표시)에 있다.

| 디자인 토큰 | 디자인 체인 | tasty Theme | 비고 |
|---|---|---|---|
| `--tasty-approval-danger-bg` | `color-mix(accent-danger × tint-fill-alpha, bg-panel)` | `Theme::approval_danger_bg()` = `mix_srgb(accent_danger(), TINT_FILL_ALPHA, bg_panel())` | 불투명. Mocha (56,43,61) · Latte (236,214,222). 시험이 두 테마 값을 확인한다 |
| `--tasty-approval-danger-border` | `accent-danger` | `Theme::approval_danger_border()` | 생성 접근자. 1px |
| `--tasty-approval-danger-fg` | `text-primary` | `Theme::approval_danger_fg()` | 생성 접근자 |

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
| `--tasty-preset-leaf-label-font-size` | `font-size-micro` (10) | `Theme::font_size_micro` | 라벨 폰트(mono, 필드 키는 값이다). 기존 필드 재사용 |
| `--tasty-preset-leaf-value-font-size` | `font-size-caption` (11) | `Theme::font_size_caption` | 값 폰트(mono). 기존 필드 재사용 |
| `--tasty-preset-leaf-summary-gap` | `space-xs` (4) | `Theme::spacing_xs` | 행↔행·kind명↔요약·라벨↔값 gap. 기존 필드 재사용 |

요약을 숨기는 기준 96×72(`LEAF_SUMMARY_MIN_W`·`LEAF_SUMMARY_MIN_H`)와 아이콘만 표시하는 기준 46(`LEAF_ICON_ONLY_MIN`)도 LogicalPx 상수다. cwd·file의 앞자름과 startup·url의 뒤자름은 `PresetFieldSpec.input`의 Dir/FilePath 여부로 결정하며 kind 이름으로 고정하지 않는다.

## 토큰이 아닌 raw 값 주의

디자인 inline style 에는 토큰이 아닌 raw px 도 섞여 있다 (전사 시 그대로 옮기되 기록):
- remote_tool 헤더 `gap: 9` — 토큰 아님 (spacing_sm=8 과 1px 차).
- remote_tool 헤더 title `fontSize: 14` — 토큰 아님 (14 폰트 토큰은 UI 상한 `font_size_max` 뿐 → heading 13 사용, 1px 차).
- remote_tool TabBtn `height: 35` / 탭바 `padding 0 8` / 탭 `padding 0 13` / `gap 2` — raw.

## banner (banner-02 specimen / banner-03 본체)

배너의 `--tasty-banner-*` component 토큰과 `--tasty-opacity-recessed`는 아래 값으로 연결된다. 본체와 갤러리는 같은 Theme 의미를 사용한다. 아래 표는 현재 대응이며 앞으로 구현할 작업 목록이 아니다.

| 디자인 토큰 | 디자인 체인 | 갤러리 매핑 | 본체 |
|---|---|---|---|
| `--tasty-banner-bg` | → `surface-raised` → neutral-300 | `surface_raised()` | semantic 직접 매핑(신규 필드 불필요) |
| `--tasty-banner-fg` | → `text-primary` | `text_primary()` | 〃 |
| `--tasty-banner-border` | → `border-strong` | `border_strong()` | 〃 |
| `--tasty-banner-radius` | → `radius-8` (8px) | `corner_radius_lg`(8) | 기존 토큰 — 본체 배너 셸도 `corner_radius_lg` 를 쓴다(시스템 기본 4px 의 의도적 2배) |
| `--tasty-banner-shadow` | → `shadow-popover` | `shadow_popover()` | 기존 토큰 — 본체 배너 셸도 같은 접근자를 쓴다([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md) 그림자 선택 기준 중 popover) |
| `--tasty-banner-margin` | → `space-sm` → size-8 (8px) | `spacing_sm` | 기존 토큰 |
| `--tasty-banner-padding-x` | → `space-md` (12) | `spacing_md` | 기존 토큰 |
| `--tasty-banner-padding-y` | → `space-sm` (8) | `spacing_sm` | 기존 토큰 |
| `--tasty-banner-gap` | → `space-md` (12) | `spacing_md` | 기존 토큰 |
| `--tasty-banner-icon-fg` | → `text-muted` (default) | `text_muted()` (per-banner severity override) | 기존 접근자 |
| `--tasty-banner-title-font-size` | → `font-size-body` (13) | `font_size_body` | 기존 토큰 |
| `--tasty-banner-body-font-size` | → `font-size-caption` (11) | `font_size_caption` | 기존 토큰 |
| `--tasty-banner-body-fg` | → `text-muted-raised` (Mocha `text-muted`, Latte `text-primary`) | `banner_body_fg()` | 본문과 재로딩 라벨. `banner-bg` 위 Mocha 5.65:1, Latte 5.17:1 |
| `--tasty-banner-countdown-font` | → `font-mono` | `FontId::monospace` | 기존 |
| `--tasty-banner-countdown-font-size` | → `font-size-micro` (10) | `font_size_micro` | 기존 토큰 |
| `--tasty-banner-countdown-fg` | → `text-muted` | `text_muted()` | 기존 접근자 |
| `--tasty-banner-recessed-opacity` | → `opacity-recessed` (0.4) | `opacity_recessed()` + `gamma_multiply` | 기존 접근자 — 본체도 같은 접근자를 쓴다 |
| `--tasty-banner-fade` | → `motion-ui` → duration-120 (120ms) | `banner_fade()` (생성, `Millis`) — 소비처 없음 | 본체는 페이드 없이 end-state 로 그린다 |
| `--tasty-banner-more-reason-glyph-offset` | → `size-1` (1px) | `banner_more_reason_glyph_offset()` (생성) | 생성 접근자 — 본체·갤러리 공용 `banner_more_unknown` 이 ⋯ 메뉴 이름 미상 형태의 사유 줄 info 글리프 top offset 으로 쓴다 |

배너 반경·디밍·그림자는 각각 `corner_radius_lg`·`opacity_recessed()`·`shadow_popover()`를 사용한다. `banner-more-reason-glyph-offset` 은 배너 앞 글리프의 `banner-glyph-offset` 과 역할이 다른 별도 토큰이다.

## 메뉴 구분선 (navigation/MenuItem `.tasty-menu-sep`)

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 쓰는 곳 |
|---|---|---|---|
| `--tasty-menu-separator` | → `border-strong` → neutral-400 | `menu_separator()` (생성) | 모든 메뉴의 구분선: 공용 `menu_separator`(Tools 메뉴 · 컨텍스트 메뉴 견본 · 레일 카테고리 메뉴 갤러리 견본), 본체 레일 카테고리 메뉴, SearchableSelect 검색칸·고정 행 아래, 배너 ⋯ 메뉴 이름 미상 형태. 메뉴 구분선은 팝업 안의 구획이라 `border-strong` 이고, 카드·탭 스트립·표·목록 안의 hairline 인 `separator` 와 구분한다. MultiSelect 일괄 토글 아래 선은 별도 `multiselect-separator`(→ separator)다 |

## modifier-hint 오버레이 (modifier-hint-03 specimen + 본체)

modifier-hint는 보조키를 누르는 동안 유지되며 키보드 포커스 없이 마우스로 이동·크기를 조절하는 오버레이다. [통합 용어집](../../concepts/ubiquitous-language.md)의 구분을 따른다.

기본 토큰은 `tokens/components.css`의 `--tasty-modhint-*`, `tokens/primitives.css`의 `--tasty-size-180`·`--tasty-duration-200`·`--tasty-duration-500`, `tokens/semantic.css`의 모션 값이다. 길이는 LogicalPx, 시간은 아래 Millis 접근자로 읽는다.

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-motion-hold-reveal` | → `duration-500` (500ms) | `modhint_hold_delay()` (생성, `Millis`) | 홀드→표시 지연. 모션 아님 → reduced_motion 무관 유지 |
| (대응 토큰 없음) | 1200ms | `motion_hold_reveal_shift()` = `MOTION_HOLD_REVEAL_SHIFT_MS` (수기, [키 매핑 설계 (운영 상세)](../policies/key-mapping.md)) | **Shift 단독** 홀드만 이 지연. 타이핑 중 Shift 스침으로 팝업이 튀는 것 억제. 모션 아님 → reduced_motion 무관 |
| `--tasty-motion-ui-fade` | → `duration-200` (200ms) | `modhint_fade()` (생성, `Millis`) | 등장 페이드(opacity 0.2→1.0). reduced_motion 시 0ms |
| `--tasty-modhint-width` | → `size-180` (180px) | `modhint_width()` | 열린 사이드바 폭(기본 180)과 정렬 |
| `--tasty-modhint-height` | → 400px | `modhint_height()` | 기본 세로 높이 |
| `--tasty-modhint-min-width` | → 180px | `modhint_min_width()` | 리사이즈 최소 (= 기본 너비) |
| `--tasty-modhint-min-height` | → 240px | `modhint_min_height()` | 리사이즈 최소 |
| `--tasty-modhint-strip-height` | → `size-28` (= item-height-interactive) | `modhint_strip_height()` | 드래그 스트립 높이 |
| `--tasty-modhint-pad` | → 10px | `modhint_pad()` | 스크롤 리스트 안쪽 패딩 |
| `--tasty-modhint-section-gap` | → `space-md` (12) | `modhint_section_gap()` | 섹션 사이 |
| `--tasty-modhint-row-gap` | → `size-8` (8px) | `modhint_row_gap()` (생성) | 섹션 내부 행 사이 |
| `--tasty-modhint-row-font-size` | → `font-size-caption` (11) | `modhint_row_font_size()` (생성) | 키캡 행 라벨·역할 행 설명·빈 행 문구의 글자 크기 |
| `--tasty-modhint-row-min-height` | → `size-24` (24px) | `modhint_row_min_height()` (생성) | 키캡 행 최소 높이 |
| `--tasty-modhint-empty-row-gap` | → 3px | `modhint_empty_row_gap()` | 빈 조합 섹션 내부 간격(채워진 6px보다 좁게, §6-5). 디자인은 인라인 px(`.mh-section--empty{gap:3px}`) — 코드에서 토큰화 |
| `--tasty-modhint-empty-row-min-height` | → `size-20` (20px) | `modhint_empty_row_min_height()` (생성) | 빈 조합 플레이스홀더 행 최소 높이(키캡 행 24px보다 타이트) |
| `--tasty-modhint-grip-size` | → `icon-size-xs` (12) | `modhint_grip_size()` (생성) | 코너 리사이즈 그립 |
| `--tasty-modhint-grip-inset` | → `size-2` (2px) | `modhint_grip_inset()` (생성) | 그립 상자와 패널 오른쪽·아래 변 사이 여백. UI 배율을 따른다 |
| `--tasty-modhint-grip-fg` | → `glyph-dim` | `modhint_grip_fg()` | 코너 그립 대각선 두 획의 색. 본체와 갤러리가 `tasty_ui_widgets::modhint_resize_grip` 으로 시안 SVG(`M11 5 5 11M11 9 9 11`, 12 상자, 오른쪽·아래 `modhint-grip-inset` 안쪽)를 그린다 |
| `--tasty-modhint-bg` | → `bg-panel` (불투명) | `modhint_bg()` | 라이브 출력 위 불투명 셸 |
| `--tasty-modhint-border` | → `border-strong` | `modhint_border()` | 1px 셸 보더 |
| `--tasty-modhint-radius` | → `radius` (4) | `corner_radius` | 셸 코너 |
| `--tasty-modhint-shadow` | → `shadow-popover` | `shadow_popover()` | anchored + scrim-less lift(banner 와 공유). 디자인 체인이 한때 `shadow-modal` 을 가리켜 구현과 갈려 있었으나, [`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md) 의 그림자 선택 기준에 맞춰 디자인이 `components.css` 의 이 토큰을 `shadow-popover` 로 재타깃했다 — 발산은 남아 있지 않다 |
| `--tasty-modhint-header-bg` | → `bg-sidebar` | `modhint_header_bg()` (생성) | 드래그 스트립 배경 |
| `--tasty-modhint-separator` | → `separator` | `modhint_separator()` | 스트립/헤더 하단 구분선 |
| `--tasty-modhint-held-fg` | → `text-muted` | `modhint_held_fg()` | 스트립 "held" 라벨 |
| `--tasty-modhint-role-bg` | → `surface-active` | `modhint_role_bg()` | 특수 역할 행 washed 배경 |
| `--tasty-modhint-role-fg` | → `accent-primary-on-active` | `modhint_role_fg()` | 역할 행 leading 글리프 |
| `--tasty-modhint-role-text-fg` | → `text-on-active` | `modhint_role_text_fg()` | 역할 행 텍스트 |
| `--tasty-modhint-row-fg` | → `text-secondary` | `modhint_row_fg()` | 액션 행 텍스트 |
| `--tasty-modhint-empty-fg` | → `text-muted` | `modhint_empty_fg()` | 빈 조합 플레이스홀더("바인딩 없음") 텍스트 — row-fg(text-secondary)보다 한 단계 절제(§6-2) |
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
| `--tasty-tooltip-line-height` | → `line-height-ui` (1.4) | `line_height_ui: f32 = 1.4` (무차원 비율, zoom 무관) | UI 줄간격 배수 |
| `--tasty-tooltip-max-width` | → `size-240` (240px) | `tooltip_max_width: LogicalPx(240)` (zoom 적용 — `toast_max_width` 전례) | 초과 시 wrap |
| `--tasty-tooltip-offset` | → `space-xs` (4) | `spacing_xs` · `tooltip_offset()` | 앵커와 간격. 탭 스트립 규칙(`placement_clear_of_native`)에서는 스트립 안 후보의 칸과 간격, 창 가장자리 가로·세로 clamp 여백도 된다. 스트립 안 후보의 네이티브 영역 겹침 허용치는 `border-width`(1)다 |
| `--tasty-tooltip-delay` | → `motion-ui-med` → duration-150 (150ms) | `tooltip_delay()` (생성, `Millis`) | hover delay. 종전에는 위젯 상수 `HOVER_DELAY_SECONDS: f64 = 0.15` 였다. fade 는 immediate-mode snap 으로 생략 |
| `--tasty-help-hint-size` | → `icon-size-sm` (14) | `icon_glyph_size_sm` | (?) 글리프 14px |
| `--tasty-help-hint-gap` | → `space-xs` (4) | `spacing_xs` | 라벨과 gap |
| `--tasty-help-hint-color` | → `text-muted` | `text_muted()` | rest 색 |
| `--tasty-help-hint-color-hover` | → `text-secondary` | `text_secondary()` | hover/focus 색 |

### duration 접근자에서 값을 벗기는 자리 — 감시 대상

디자인 모션은 Theme에서 `Millis`로 반환한다. 표시 문자열이나 단위 없는 ms 계산이 필요한 경계에서는 `to_millis_f32()`로 변환한다([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md)). 변환 위치가 많아지면 전체 모션 접근자 사용과 함께 검토한다. 사용처가 늘어난 것인지 단위를 벗기는 비율만 늘어난 것인지 구분해야 한다.

```bash
rg 'to_millis_f32\(\)' src crates -g '*.rs'
rg '\.(banner_fade|modhint_fade|modhint_hold_delay|spinner_duration|status_dot_pulse_duration|switch_overlay_fade|tooltip_delay|motion_hold_reveal_shift)\(\)' src crates -g '*.rs'
```

결과에는 정의와 테스트도 포함될 수 있으므로 실제 호출 문맥을 확인한다.

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
| `--tasty-listctrl-label-fg-active` | → `text-on-active` | `listctrl_label_fg_active()` | hover/selected |
| `--tasty-listctrl-part-fg-selected` | → `text-on-active` | `listctrl_part_fg_selected()` | 선택 행의 설명·아이콘·chevron |
| `--tasty-listctrl-desc-fg` | → `text-muted` | `listctrl_desc_fg()` | |
| `--tasty-listctrl-desc-font-size` | → `font-size-caption` (11) | `listctrl_desc_font_size()` | |
| `--tasty-listctrl-icon-fg` | → `text-muted` | `listctrl_icon_fg()` | leading 글리프 (icon-size-md = `icon_glyph_size_md`) |
| `--tasty-listctrl-chevron-fg` | → `text-muted` | `listctrl_chevron_fg()` | drill-in chevron (icon-size-sm 슬롯, painter 폴리라인 — tree_row 전례) |
| `--tasty-listctrl-row-bg-hover` | → `overlay-hover` | `listctrl_row_bg_hover()` | premultiplied 워시 |
| `--tasty-listctrl-row-bg-selected` | → `surface-active` | `listctrl_row_bg_selected()` | |
| `--tasty-listctrl-selected-bar` | → `accent-primary` | `listctrl_selected_bar()` | 좌측 accent 바 |
| `--tasty-listctrl-selected-bar-width` | → `selection-edge-width` (2px) | `listctrl_selected_bar_width()` | `selection_edge_width` 반환 (hairline, `ui_zoom` 제외) |
| `--tasty-listctrl-divider` | → `separator` | `listctrl_divider()` | 행 사이 헤어라인 |

> desc 줄과 라벨 사이 1px 간격(디자인 `.tasty-listctrl__text { gap: 1px }`)은 spacing
> 스텝 밖 구조 간격 → `tasty-ui-widgets::tokens::STRUCT_GAP_1` (primitive size-1 대응 관례).

## Remote file transfer (progress/error 09)

디자인 `tokens/components.css` 의 `--tasty-transfer-popup-width` + `--tasty-transfer-*` 여백(6종) + `--tasty-progress-*`(4종).
진행·실패 팝업(`popup/transfer.rs`)의 폭·여백과 완료 비율을 표시하는 진행률 막대에 사용한다. 아래 값과 현재 호출 경로를 따른다. component 토큰이라는 이유만으로 Theme 접근자를 만들 수 없다고 일반화하지 않는다.

| 디자인 토큰 | 디자인 체인 | tasty Theme / 값 | 비고 |
|---|---|---|---|
| `--tasty-transfer-popup-width` | → `size-400` (400px) | `transfer_popup_width()` | 진행·실패 프레임 폭. UI 배율을 곱한다(본체와 갤러리 공용) |
| `--tasty-transfer-pad-x` | → `size-14` (14px) | `transfer_pad_x()` | 헤더·바디·푸터 가로 여백과 바디 사방 여백 |
| `--tasty-transfer-header-pad-y` | → `space-md` (12px) | `transfer_header_pad_y()` | 헤더 세로 여백 |
| `--tasty-transfer-body-gap` | → `size-10` (10px) | `transfer_body_gap()` | 바디 블록 사이·진행 행 안 간격 |
| `--tasty-transfer-footer-pad-y` | → `size-10` (10px) | `transfer_footer_pad_y()` | 푸터 세로 여백 |
| `--tasty-transfer-well-pad-y` | → `space-sm` (8px) | `transfer_well_pad_y()` | 실패 사유 well 세로 여백 |
| `--tasty-transfer-well-pad-x` | → `size-10` (10px) | `transfer_well_pad_x()` | 실패 사유 well 가로 여백 |
| `--tasty-progress-height` | → `size-4` (4px) | `progress_height()` | determinate bar 두께 |
| `--tasty-progress-radius` | → `radius-sm` (2px) | `Theme::corner_radius_sm` | bar 라운드 |
| `--tasty-progress-track-bg` | → `bg-app` | `Theme::bg_app()` | recessed track(패널보다 어둡게) |
| `--tasty-progress-fill-bg` | → `accent-primary` | `Theme::accent_primary()` | determinate fill(0ms, 폭=바이트) |

> **팝업의 구조 치수**: 폭과 위 여백 토큰은 모두 UI 배율을 따른다(본체와 갤러리 공용 접근자).
> 줄 높이는 토큰이 아니다. 각 줄은 글자 크기 × `line_height_ui`이고, 헤더 줄은 glyph md와 제목 줄 중
> 큰 값, 파일명 줄은 glyph md와 본문 줄 중 큰 값이다. 실패 팝업 본문 문단의 줄 높이는 디자인
> `lineHeight: 1.5`를 따른다. bar 는 `Spinner` 처럼 위젯화하지 않고
> painter 인라인(track `bg_app` + fill `accent_primary`).

## Popup 타이틀바 · 알림 popup 폭

디자인 `tokens/components.css` 의 `--tasty-popup-title-*` 와 `--tasty-notifications-popup-width`. 공용 popup
타이틀바(`src/adapters/ui/popup.rs`·`popup/draw.rs`, 갤러리 `catalog/popup_frame.rs`)의 버튼 칸과 제목 예약 폭,
전체화면 무대를 선언한 알림 popup 의 폭이다. 제목 예약 폭 = edge-inset + N × btn-size + (N − 1) × btn-gap +
text-gap(버튼 하나 32, 둘 60).

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-popup-title-btn-size` | → `icon-button-size-sm` → `size-24` (24px) | `popup_title_btn_size()` | 배율 적용. 닫기·전체화면 버튼 칸 |
| `--tasty-popup-title-btn-gap` | → `space-xs` (4px) | `popup_title_btn_gap()` | 버튼 사이 간격 |
| `--tasty-popup-title-edge-inset` | → `space-xs` (4px) | `popup_title_edge_inset()` | 오른쪽 끝과 닫기 버튼 사이 |
| `--tasty-popup-title-text-gap` | → `space-xs` (4px) | `popup_title_text_gap()` | 버튼 묶음과 제목 띠 사이. `popup_title_text_rect` 의 대칭 예약에 더한다 |
| `--tasty-notifications-popup-width` | → `size-352` (352px) | `notifications_popup_width()` | 배율 적용. 본체 `notification.rs` 의 sizer 와 갤러리 알림 패널이 읽는다 |

## Convert popup 폭

디자인 `tokens/components.css` 의 `--tasty-convert-popup-width`. surface 변환 팝업(`popup/convert.rs`)의
고정 폭 role 이다.

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-convert-popup-width` | → `size-240` (240px) | `convert_popup_width()` | 배율 적용. 본체 `popup/convert.rs` 의 sizer 와 갤러리 `components/convert.rs` 가 읽는다 |

<a id="attention-kind--needsinputcompletion-surface-highlight-docs/dev-guide/agent-runner.md"></a>

## Attention kind — NeedsInput/Completion (surface-highlight)

디자인 `tokens/semantic.css` + `tokens/components.css` + `components/core/Badge.jsx` +
`components/feedback/StatusDot.jsx`(원본: Design System "attention-visuals", 2026-08-10
확정). 색은 기존 semantic 색과 같은 Theme 필드를 쓴다 — **신규 Theme 필드 0**
([Theme 접근자 선택](design-parity-notes.md)).
생성기는 아래 토큰마다 같은 이름의 접근자를 만든다. `attention_needs_input()`·`attention_completion()` 은
`yellow`·`blue` 필드를, `-fg` 두 개는 `theme.rs` 의 수기 접근자로 `text_on_accent()` 를 돌려준다.
component 접근자(`badge_warning_bg()` 등)는 이 네 접근자를 거친다. 본체의 탭 제목·surface 테두리·collapsed rail 점은
component 접근자(`tab_fg_*()`·`surface_highlight_*_border()`·`status_dot_*()`)를 쓴다. 공용 StatusDot의 `NeedsInput`·`Completion` 종류도
`status_dot_needs_input()`·`status_dot_completion()`을 쓴다. 공용 Badge의 `Warning` variant는 `badge_warning_bg()`·`badge_warning_fg()`를 쓰고,
본체 워크스페이스 행의 개수 배지는 공용 Badge(`Primary`·`Warning`)로 그린다. 갤러리 칩은 토큰 이름의 접근자를 쓴다.
`--tasty-badge-group-gap` 도 `space-xs` 그대로 별칭이다 — 갤러리 Badge 예제의 BadgeGroup은 `badge_group_gap()` 을 쓰고,
본체 사이드바는 `Theme::spacing_xs` 를 직접 참조한다.

| 디자인 토큰 | 디자인 체인 | tasty Theme / 값 | 비고 |
|---|---|---|---|
| `--tasty-attention-needs-input` | → `accent-warning` | `attention_needs_input()` = `accent_warning()` | NeedsInput 색(노랑) |
| `--tasty-attention-completion` | → `accent-primary` | `attention_completion()` = `accent_primary()` | Completion 색(파랑) |
| `--tasty-attention-needs-input-fg` / `-completion-fg` | → `text-on-accent` | `attention_*_fg()` = `text_on_accent()` | 두 배지 공통 전경 |
| `--tasty-attention-rank-needs-input` | `30`(정수) | `AttentionLevel::NeedsInput`(derive `Ord`) | 재도출 금지 — 소스에 정수 값 주석으로 미러링 |
| `--tasty-attention-rank-completion` | `10`(정수) | `AttentionLevel::Completion` | 위와 동일 |
| `--tasty-badge-primary-bg`/`-fg` | = attention-completion(-fg) | `accent_primary()`/`text_on_accent()` | Completion 배지 |
| `--tasty-badge-warning-bg`/`-fg` | = attention-needs-input(-fg) | `badge_warning_bg()`/`badge_warning_fg()` (공용 Badge `Warning`) | NeedsInput 배지 |
| `--tasty-badge-group-gap` | → `space-xs`(4px) | `Theme::spacing_xs` | 두 배지 동시 표시 시 간격. 갤러리 BadgeGroup은 `badge_group_gap()`, 본체 사이드바는 `spacing_xs` 직접 참조 |
| `--tasty-tab-fg-needs-input`/`-completion` | = attention-* | `tab_fg_needs_input()`/`tab_fg_completion()` | 탭 제목 색 |
| `--tasty-surface-highlight-input-border` | = attention-needs-input | `surface_highlight_input_border()` | surface 테두리 NeedsInput |
| `--tasty-surface-highlight-input-width` | → `focus-ring-width`(2px) | `Theme::focus_ring_width` | Completion 테두리와 동일 굵기 |
| `--tasty-surface-highlight-done-border` | = attention-completion | `surface_highlight_done_border()` | 완료 테두리 |
| `--tasty-status-dot-needs-input`/`-completion` | = attention-* | `status_dot_needs_input()`/`status_dot_completion()` | collapsed rail dot, 공용 StatusDot의 attention 종류 |
| `--tasty-status-dot-ring` | → `bg-sidebar` | `status_dot_ring()` | collapsed rail dot의 배경 고리 색. 실행 중 점을 포함한 모든 점에 두른다 |
| `--tasty-status-dot-ring-width` | → `size-1-5`(1.5px) | `Theme::status_dot_ring_width` (배율 제외 hairline) | collapsed rail dot 고리 두께 |

예약(아직 미구현, 토큰만 존재): `error` rank 40 → `accent-danger`, `approval` rank 20 →
`accent-agent`. Catppuccin 매핑에 추가할 hue 없음 — Latte 는 기존 accent role 을 통해
자동 상속.

## MultiSelect 메뉴 크기 (forms/MultiSelect)

디자인 `tokens/components.css` 의 `--tasty-multiselect-*` Tier-3 블록 중 **메뉴 크기 두
건 + 그림자**. 세 건 다 **신규 값이 아니다** — 기존 primitive/component/semantic 종착이다.
나머지 `--tasty-multiselect-*`(트리거 치수·색·행 리듬)는 디자인 판정이 그대로
`--tasty-select-*` / `--tasty-menu-*` / `--tasty-checkbox-*` alias 다. 트리거는 select 토큰을
직접 읽는다. 메뉴 프레임의 채움·테두리·반경은 `with_popover_frame` 의 메뉴 토큰(`multiselect_menu_bg`·`multiselect_menu_border`·
`multiselect_menu_radius` 가 그 별칭)이고, 안쪽 여백 `multiselect_menu_padding`·그림자·트리거 간격 `multiselect_menu_gap` 은 egui 팝업 style 과 기준 rect 로 바꿔 그리고, 메뉴 행은 `multiselect_row_height`·`multiselect_row_padding_x`·`multiselect_row_bg_hover`·
`multiselect_row_bg_active`·`multiselect_all_fg` 접근자를 읽는다.

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 비고 |
|---|---|---|---|
| `--tasty-multiselect-menu-max-height` | → `autocomplete-max-height` → `size-220` (220px) | `multiselect_menu_max_height()` | AutoComplete 드롭다운과 **같은 값 공유**(디자인 판정). 초과 시 내부 스크롤 |
| `--tasty-multiselect-menu-max-width` | → `size-320` (320px) | `multiselect_menu_max_width()` | 메뉴 **상자 전체**의 상한. 체인이 primitive 로 직접 닿으므로 같은 320 인 `toast_max_width` 를 빌리지 않는다(토스트 폭 재조정이 메뉴를 끌고 가는 가짜 결합 회피) |
| `--tasty-multiselect-menu-shadow` | → `shadow-popover` | `shadow_popover()` | 위젯이 직접 읽지 않는다 — 메뉴가 egui `Frame::popup` 으로 그려지므로 `apply_theme_to_egui` 가 `visuals.popup_shadow` 를 이 값으로 매핑해 같은 경로의 `ComboBox`·`popup_below_widget` 전부에 한 번에 닿는다([theme.md](theme.md) "떠 있는 표면의 그림자") |

메뉴 폭 규칙은 `min-width: 트리거` + 내용에 맞춰 max 까지 확장이고, CSS 와 같이 min 이
max 를 이긴다(트리거가 320 보다 넓으면 트리거를 따른다). 행 라벨이 남는 폭을 넘으면
`checkbox` 가 말줄임한다(디자인 `.tasty-check__label { flex:1; min-width:0; ellipsis }`).

## SearchableSelect (forms/SearchableSelect)

디자인 `tokens/components.css` 의 `--tasty-select-search-*` 블록. 모두 기존 semantic·component 종착의 별칭이며 생성된 접근자 `select_search_*()` 로 읽는다. `--tasty-font-combo-list-max-height`·`--tasty-font-combo-search-inset` 은 이 블록의 `list-max-height`·`inset` 별칭이다(`font_combo_*()` 가 `select_search_*()` 를 돌려준다). 트리거 치수·테두리는 `select_*` 토큰을, 옵션 행은 `menu_item_*` 토큰을 그대로 쓴다.

| 디자인 토큰 | 디자인 체인 | Theme 접근자 | 쓰는 곳 |
|---|---|---|---|
| `--tasty-select-search-trigger-border-hover` | → `border-control-raised` | `select_search_trigger_border_hover()` | 호버한 트리거 테두리 |
| `--tasty-select-search-value-placeholder` | → `text-placeholder` | `select_search_value_placeholder()` | 값이 없을 때 트리거 문구 |
| `--tasty-select-search-menu-gap` | → `space-xs` (4) | `select_search_menu_gap()` | 트리거와 팝업 사이 |
| `--tasty-select-search-menu-max-width` | → `size-320` | `select_search_menu_max_width()` | 팝업 상자 전체 폭 상한(하한은 트리거 폭) |
| `--tasty-select-search-list-max-height` | → `size-300` | `select_search_list_max_height()` | 팝업 상자 전체 높이 상한. 옵션 목록만 남는 높이로 스크롤한다 |
| `--tasty-select-search-inset` | → `space-xs` (4) | `select_search_inset()` | 팝업 안쪽 여백(`with_popover_frame_ring` 둘레) |
| `--tasty-select-search-field-height`·`-padding-x`·`-gap` | → `input-height` 28 · `space-sm` 8 · `input-gap` 8 | `select_search_field_height()`·`_padding_x()`·`_gap()` | 검색칸 높이 · 좌우 여백 · 아이콘(14, `icon_glyph_size_sm`)과 글자 사이 |
| `--tasty-select-search-field-bg`·`-border`·`-border-focus`·`-fg` | → `input-raised-*` · `input-border-focus` · `input-fg` | `select_search_field_bg()`·`_border()`·`_border_focus()`·`_fg()` | 검색칸 상자. 포커스는 테두리 색만 바꾸고 ring 은 없다 |
| `--tasty-select-search-placeholder`·`-icon-fg` | → `input-placeholder` · `input-icon-fg` | `select_search_placeholder()`·`select_search_icon_fg()` | 검색칸 안내 문구 · 돋보기 |
| `--tasty-select-search-status-fg` | → `text-secondary-raised` | `select_search_status_fg()` | 결과 없음·로딩 상태 행 |

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
`prim_listctrl` · `git_viewer`)가 이미 이쪽을 쓴다. 같은 값이어도 역할이 다른 토큰으로 대체하지 않는다.
`listctrl-selected-bar-width`는 한쪽 변 선택 띠 `selection-edge-width`를 가리키므로 값은
`selection_edge_width`(2px, hairline, 배율 제외)와 같다. remote attach 목록 행(`remote_attach/rows`와
본체 `popup/remote_attach.rs`)과 preset 목록 행은 대응하는 component 토큰이 없어 `selection_edge_width`를
직접 읽는다.

이 치수에는 대응하는 역할의 토큰이 없다. `crates/tasty-ui-widgets/src/tokens.rs`의 `FH_*` 상수를 본체와 갤러리가 함께 사용한다. 4px 배수인지와 해당 역할의 토큰이 있는지는 별도로 확인한다.

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
> (= `surface-raised`) 라, 셸이 같은 값이면 그 칩이 배경과 구분되지 않는다(실측: Mocha 에서
> 두 색이 `#313244` 로 동일했다). `popup_shell_fill_keeps_the_default_tag_visible`
> 가 두 값이 갈린다는 것을 고정한다.

> **전사되지 않은 디자인 속성 하나**: 그룹 라벨의 자간. egui에는 해당 API가 있다
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

> **vendor 상태**: DTCG 토큰 파일은 `crates/tasty-design-tokens/dtcg/tasty.tokens.json` 으로 **vendor 되어 있다** (1504 토큰 = primitive 145 / semantic 249 / component 1110 — 수는 `crates/tasty-design-tokens/tests/freshness.rs` 가 고정). 치수 계열은 `crates/tasty-design-tokens/src/generated/` 에 const 로 생성되고 freshness·정합·색 드리프트 테스트가 CI 에서 일치를 강제한다. vendor 갱신 절차는 `crates/tasty-design-tokens/README.md`. **component tier(치수+색)는 `&Theme` 접근자로 생성돼**(`tasty-type-appearance/src/generated_component.rs`, [theme.md](theme.md) "Component tier 접근자") `tasty-ui-widgets` 위젯과 host chrome(`src/adapters/ui/`)이 소비 중.

### 구조 모델

```
DTCG:  primitive ─▶ semantic ─▶ component ─▶ UI        (3-tier)
Rust:  ThemeColors(평면 primitive) ─▶ Theme(펼친 필드 + 도출 overlay + 생성 tier 접근자) ─▶ UI
                    ▲ 색 저장은 평면 primitive, tier 는 &Theme 접근자로 재구성
```

- **색 저장은 평면 primitive.** `ThemeColors`(`crates/tasty-type-appearance/src/theme.rs`)는 catppuccin 평면 primitive(neutral ramp 12 + `placeholder` + accent hue 13 + 터미널 색 4 + ansi 16 + `surface_themes` map)만 저장한다.
- **tier 는 `&Theme` 접근자로 노출된다**: semantic 색 중 단순 primitive alias(`accent_primary()`/`surface_raised()`/`border_default()` 등)는 **생성 접근자**(`semantic_color_generated.rs` — DTCG semantic 색 토큰에서 생성), is_light 분기·도출 overlay·합성·리터럴은 수기(`text_on_accent()`/`text_disabled()`/`overlay_hover()`/`scrim()` 등). component tier(치수+색)는 생성 접근자(`generated_component.rs` — `button_primary_bg()`/`button_height_lg()` 등, [theme.md](theme.md) "Component tier 접근자"). 즉 저장은 평면이나 소비는 tier 를 경유한다.
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

터미널/마크다운의 focused/unfocused × bg/fg 색은 `ThemeColors.surface_themes: BTreeMap<String, SurfaceTheme>` 에 들어가 `theme.surface("terminal")` / `theme.surface("markdown")` 헬퍼로 읽는다.
Tasty가 바탕을 칠하는 kind의 포커스 바탕 쌍은 `surface-bed-focused`(Mocha `#000000`, Latte `#ffffff`)와 `surface-bed-unfocused`(Mocha base, Latte crust)다. 접근자는 `Theme::surface_bed_focused()`와 `Theme::surface_bed_unfocused()`이며 `crates/tasty-type-appearance/src/state_ink.rs`에 있다. `[surfaces.terminal]`과 `[surfaces.markdown]`의 bg 네 값은 내장 테마에서 이 쌍과 같다(`crates/tasty-design-tokens/tests/color_drift.rs`). 항목이 없는 kind(empty, explorer, dag_graph, image)는 `surface("<kind>")`가 `FALLBACK_SURFACE`를 돌려주므로 이 접근자를 읽어야 한다.

토큰 체인은 대칭이 아니다. 사용자 테마에는 어떤 값도 보정하지 않으며 결과는 다음과 같다.

| 사용자 테마가 고친 값 | `surface_bed_focused()` | `surface_bed_unfocused()` | 터미널 바탕 |
|---|---|---|---|
| `[surfaces.terminal].focused_bg` | 따라간다 | 그대로 | focused만 따라간다 |
| `[surfaces.terminal].unfocused_bg` | 그대로 | 그대로 | unfocused만 따라간다. 다른 바탕 kind와 달라진다 |
| 팔레트 `base`(다크)·`crust`(라이트) | 그대로 | 따라간다 | 그대로 |
| `[surfaces.markdown].*_bg` | 그대로 | 그대로 | 그대로. markdown 값만 달라진다 |

html과 플러그인 surface는 바탕을 호스트가 보장할 수 없어 위쪽 2px 띠로 분류한다. 본체는 아직 띠를 그리지 않아 이 surface에는 포커스 표시가 없다([포커스 표현](../policies/focus.md#surface-포커스-표현)). 띠의 토큰은 `surface-focus-edge-focused`, `-unfocused`, `-width`이고 공용 위젯은 `tasty_ui_widgets::SurfaceFocusEdge`다. 바탕 선택은 `tasty_ui_widgets::surface_bed`다.

markdown 플러그인의 문서 바탕 `md-doc-bg`(→ `surface-markdown-focused-bg` → `surface-bed-focused`)는 플러그인이 `ThemeWire.colors.surface_themes["markdown"].focused_bg`로 읽는다(`crates/tasty-plugin-markdown/src/render.rs`의 `md_doc_bg`). 현재 webview 경로에 포커스 값을 넘기는 수단이 없어 이 경로는 focused 값 하나만 쓴다. `surface_themes`를 싣지 않는 이전 호스트에서는 항목이 없으므로 `bg_app`으로 칠한다. 주소 입력은 킷의 공용 입력 칸과 같은 `input-bg`(`--md-input-bg`)로, 찾기 입력과 코드 복사 버튼은 `bg_app`(`--md-control-bg`)으로 칠한다. 같은 방식으로 `bg_app`을 바탕으로 쓰는 git-viewer diff 영역과 clipboard-viewer 영역은 디자인에 별도 토큰이 없어 `bg_app` 그대로다.

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

<a id="sizing-소비처-토큰-전환-전-필독--그-전환에는-픽셀-변경이-섞여-있다"></a>

#### SIZING을 토큰에 연결하기 전에 값과 배율을 비교한다

현재 화면의 값과 토큰값이 다르면 단순 코드 정리로 바꾸지 않는다. 실제 값과 지원 배율을 먼저 비교하고 차이가 있으면 디자인 결정을 받는다. 이유와 재검토 조건은 [테마 시스템 (운영 상세)](theme.md)에 있다.

접근자 호출 여부와 값 일치는 별개다. 접근자를 쓰지 않아도 기반 필드로 같은 값을 읽을 수 있고, 기반 필드를 사용해도 해당 컴포넌트가 다른 값을 그릴 수 있다. `GAP`·`POPUP_WIDTH`·`MIN_HEIGHT`처럼 흔한 이름만 비교하면 무관한 토큰이 연결될 수 있으므로 정의와 사용처를 읽는다.

앞서 확인한 다섯 차이는 2026-09-17 디자인 결정으로 정리됐다. tab 점은 `status-dot-size-compact` 6, toast 간격은 8, attached ring의 굵기·offset은 각각 2, 상태바 점은 6을 사용한다. 이것이 나머지 호출처까지 검증했다는 뜻은 아니다. 새 차이를 발견하면 값은 유지하고 상수 옆에 해당 역할·후보 토큰·차이를 기록한 뒤 디자인에서 결정한다.

### 관련

- [theme.md](theme.md) — Theme 2계층 모델 + UI 디자인 규칙
- `crates/tasty-design-tokens/` — vendor json + 치수 codegen + 드리프트 가드 (갱신 절차는 crate README)
- 코드: `crates/tasty-type-appearance/src/theme.rs` (필드) · `crates/tasty-themes/src/fallback.rs` (mocha 색값)
