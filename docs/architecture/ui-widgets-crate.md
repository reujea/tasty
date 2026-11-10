# `tasty-ui-widgets` — 본체·갤러리 공유 UI primitive

`crates/tasty-ui-widgets/`는 본체와 갤러리가 공유하는 egui 위젯과 배치 함수를 제공한다. 두 소비자가 같은 구현을 호출하므로 위젯 변경을 한곳에서 반영할 수 있다. 실제 화면은 전달한 Theme·배율·상태도 함께 확인한다.

## 위치와 의존 방향

```
tasty-egui-theme     egui 어댑터 + 색·폰트 토큰
       ↑
tasty-ui-widgets     layout / 위젯 primitive (본 문서)
       ↑                ↑
   tasty (본체)    tasty-gallery
```

- 의존: `egui`, `egui_extras`, `tasty-icons`, `tasty-egui-theme`, `tasty-type-appearance`(Theme schema), `tasty-type-geometry`(`LogicalPx`/`PhysicalPx`), `tasty-dag-layout`(공통 그래프 뷰의 레이어 배치), `icu_normalizer`·`unicode-segmentation`(표시용 NFC와 원문 강조 범위 대응).
- **본체(`tasty`) 미의존** — widgets crate 는 본체 state·plugin·전역 `theme()` 를 모른다. 그리기 함수는 `&Theme` 을 **명시적 인자**로 받는다.
- 갤러리는 본체 빌드와 분리 — `cargo build` 기본 타깃이 `tasty-gallery` 를 의존하지 않는다. `cargo run -p tasty-gallery` 로만 실행되는 standalone 데모.

## 레이아웃 idiom 카탈로그

아래 표는 화면 배치 함수만 다룬다. 버튼·입력창·표·토글·툴팁 등 공용 컴포넌트는 `crates/tasty-ui-widgets/src/`의 각 파일에 있으며, 전체 목록과 화면은 [갤러리](../dev-guide/gallery-first.md)에서 확인한다. 본체의 모든 UI 컴포넌트는 갤러리에 포함한다.

| 함수 | 역할 |
|------|------|
| `two_depth_layout(ui, theme, available_height, left, content)` | 좌측 sub-menu Frame + 우측 content idiom |
| `two_depth_layout_filtered(...)` | 위 + 좌측 섹션 필터 입력 포함 변형 |
| `horizontal_tab_bar_with_arrows(ui, id_salt, tabs, active)` | 가로 ScrollArea + chevron overlay(콘텐츠 폭 > viewport 시) |
| `tab_content_frame(ui, content)` | 탭 콘텐츠 4면 inner_margin wrapper |
| `DrillDown::show(ui, theme, list, detail, actions)` | master→detail content-swap idiom — controlled `view`, 디테일 back bar(←+제목+actions) + 내부 스크롤 |
| `ListCtrl::show(ui, theme, items, selected)` | 행 선택형 내비게이션 리스트 — DrillDown 과 짝 (`clicked` 인덱스 반환) |

사용 사이트: `DrillDown`/`ListCtrl` — 본체 settings Keybindings(프리셋·import/export)와 DAG 목록 popup, 갤러리 데모. 나머지 넷 — 갤러리 데모만.

## Layout 토큰 (`tokens` 모듈)

위젯이 쓰는 *layout-level* 상수(폭·패딩·corner 등 — 색·폰트 아님)를 단일 출처로 보관한다. SIZING 과 겹치는 값은 매직넘버로 재정의하지 않고 `tasty-type-appearance::theme::SIZING` 을 참조한다(이름은 "이 위치에서 어떤 토큰을 쓰는지" 의미론 보존). 상수 변경 시 본체·갤러리가 자동 동기화된다.

| 상수 | SIZING 출처 | 설명 |
|------|------|------|
| `SUB_TAB_PANEL_WIDTH` | `tab_width` | `two_depth_layout` 좌측 패널 고정 폭 |
| `PANEL_INNER_MARGIN` | `spacing_sm` | 좌측 패널 Frame symmetric inner margin |
| `PANEL_CORNER_RADIUS` | `corner_radius` | 좌측 패널 corner radius |
| `PANEL_STROKE_WIDTH` | `border_width` | 좌측 패널 stroke |
| `PANEL_SPACING` | `spacing_sm` | 좌·우 horizontal spacing |
| `TAB_CONTENT_PADDING` | `spacing_lg` | `tab_content_frame` inner margin |

### SIZING 에 대응이 없는 값

`tokens` 모듈은 두 부류를 담는다. 위 표는 **SIZING 을 참조하는 쪽**이고, 나머지는 대응
토큰이 아예 없어 여기가 단일 위치인 값들이다 — 4px 그리드 밖 미세 구조 간격
(`STRUCT_GAP_1..4` = DTCG `primitive.size-1..4`), 스케일 밖 코너 반경
(`*_CORNER_RADIUS`), 토스트 기하·클립보드 가운데 아이콘 등. 전수는 소스가 정본이다(여기
열거하면 다음 추가에서 바로 낡는다). 빈·로딩·오류 중앙 블록은 `CenterState` 위젯이
`center_state_*` 컴포넌트 토큰을 읽으므로 이 모듈에 상수가 없다.

이 부류에는 규칙이 둘 붙는다.

- **값이 토큰과 같으면 이쪽이 아니라 SIZING 쪽이다.** 값이 같은 const 는 토큰의
  사본이고, 사본은 `Theme::with_colors_and_zoom` 의 `zoomed()` 경로 밖이라 `ui_scale`
  을 타지 않는다 — zoom 1 에서만 같고 나머지 배율에서 갈라진다
  ([테마 시스템 (운영 상세)](../design/systems/theme.md)).
- **그래서 스케일 밖 const 에는 사유를 적는다.** 어느 토큰 근처인지, 왜 스냅하지
  않는지, 그리고 zoom 을 안 타는 대가를 doc 주석에 남긴다. 대가의 크기는 축마다
  다르다 — 반경 토큰은 zoom 을 타므로 실재하고, `border_width` 처럼 애초에 zoom 을
  안 타는 축은 대가가 없다.

## 원문을 보존하는 이름·경로 표시

`display_nfc`는 읽은 값을 바꾸지 않는 표시 변환이며 이미 NFC면 빌린다. `explorer_match_job`은 원문 일치 범위를 합성된 표시 범위로 옮긴다. 합성으로 바뀐 grapheme의 일부 일치는 그 묶음 전체를 강조하고, 변하지 않은 묶음은 원래 구간을 유지한다. 검색 비교에는 NFC를 적용하지 않는다.

`PathField::display_nfc(true)`는 비편집 글자만 NFC로 덧그린다. 내부 Input의 TextEdit에는 항상 원문 버퍼가 들어가며, 합성 표시에서 편집 포커스를 얻으면 원문 전체를 선택한다. Go·Enter·자동완성 확정과 Copy는 원문 값이다. 입력의 폭·색·폰트·테두리는 기존 Theme를 그대로 쓴다. 기본은 false여서 다른 소비자의 표시·클릭 동작을 바꾸지 않는다.

## 호출 idiom — borrow snapshot

`two_depth_layout` 처럼 *좌측 클릭 시 sub_tab 갱신 + 우측은 현재 sub_tab 분기* 인 경우, 좌·우 두 클로저가 `&mut sub_tab` 을 동시에 캡처할 수 없다. 호출자에서 snapshot 으로 푼다:

```rust
let current = sub_tab.clone();          // Copy 면 *sub_tab
let mut selected_new = None;
tasty_ui_widgets::two_depth_layout(ui, &theme, available_height,
    |ui| { /* 좌측: 클릭 시 selected_new = Some(tab) */ },
    |ui| match &current { /* 우측 분기 */ },
);
if let Some(new) = selected_new { *sub_tab = new; }
```

탭 선택은 다음 프레임에 반영되므로 약 16ms의 지연이 생긴다. 두 클로저의 가변 참조 충돌을 피하기 위한 방식이다.

`horizontal_tab_bar_with_arrows` 의 chevron 아이콘은 `tasty-icons` 의 `CHEVRON_LEFT`/`CHEVRON_RIGHT` 를 쓰며, 호출자가 `tasty_icons::install_texture_loader` 를 미리 호출했다고 가정한다.

## 확장 가이드

위젯을 새로 추가할 때:

1. **표·드롭다운·버튼처럼 고유 이름으로 식별되는 보편 컴포넌트**(`data/Table`, `forms/Select` 등)는 **단 한 곳에서만 쓰여도 무조건 공용 위젯으로 제작**한다 — 이 경우 사용처 수를 따지지 않는다(상세: `docs/architecture/ui-widgets-crate.md#무엇을-공용-위젯으로`). 그 외 *layout idiom* 류는 본체·갤러리 양쪽에 같은 형태가 ≥ 2 곳 있는지 확인하고, 1 곳뿐이면 단일 사용처용 abstraction 임을 인지한다.
2. **시그니처는 전역 의존 0** — `theme: &Theme` 인자로 받고 전역 `theme()` 직접 호출 금지.
3. **매직넘버는 `tokens` 모듈에** — 함수 본문에 `f32` 리터럴 직접 박지 않는다.
4. **borrow 충돌은 호출자에서 snapshot 으로** — widget 함수는 `impl FnOnce(&mut egui::Ui)` 클로저 1~2 개를 받는 단순 시그니처 유지.

## 무엇을 공용 위젯으로

> 위젯이 *어디에* 사는지(크레이트 구조·demo=main 동기화)는 이 문서의 앞 절들이다. 이 절은 *무엇을* 공용 위젯으로 만들어야 하는지의 판단 기준만 기술한다.

**이름이 곧 정체성인 보편 컴포넌트는, 지금 쓰는 곳이 단 한 곳뿐이어도 무조건 공용 위젯으로 만들어 가져다 쓴다.** 인라인으로 그리지 않는다.

### 핵심 규칙

표(Table)·드롭다운(Dropdown/Select)·버튼(Button)처럼 **고유한 이름을 대면 그게 어떤 컴포넌트인지 바로 알 수 있는** 보편 컴포넌트는 `crates/tasty-ui-widgets` 에 공용 위젯으로 정의하고 호출해서 쓴다. 현재 사용처가 1군데뿐이라는 사실은 인라인 작성의 근거가 되지 않는다.

### 근거

표·버튼·드롭다운 같은 공용 컴포넌트를 화면마다 따로 구현하면 토큰·간격·입력 동작이 달라지기 쉽다. 현재 사용처가 하나여도 공용 위젯으로 만들면 다른 화면과 갤러리가 같은 구현을 사용할 수 있다.

- **공유 구현**: 토큰·간격·상태 오버레이를 한 함수에서 관리한다([theme UI 디자인 규칙](../design/systems/theme.md#ui-디자인-규칙-필수)).
- **본체·갤러리 비교**: 같은 위젯 함수를 호출하므로 구현 사본의 차이를 줄일 수 있다. 입력 상태와 배율도 맞춰 확인한다.
- **재사용**: 다음 화면에서도 기존 컴포넌트를 호출한다.

### 적용 기준

| 구분 | 판단 | 처리 |
|------|------|------|
| **이름으로 식별되는 보편 컴포넌트** | "이건 표/버튼/드롭다운이다" 라고 한 단어로 부를 수 있다 | 사용처 수와 무관하게 **공용 위젯**(`tasty-ui-widgets`) |
| **화면 전용 1회성 합성 레이아웃** | 보편 이름이 없고, 특정 화면의 구조를 짜맞춘 것 | 그 화면에 **인라인** 허용 |

판단은 사용처 개수가 아니라 **"보편 이름이 붙는가"** 로 한다.

#### 공용 위젯으로 만들어야 하는 예 (이름이 곧 정체성)

표(Table) · 드롭다운/셀렉트(Dropdown/Select) · 콤보박스/자동완성(Combobox/Autocomplete) · 버튼(Button) · 입력 필드(Input/TextField) · 체크박스(Checkbox) · 스위치/토글(Switch) · 라디오(Radio) · 탭(Tab) · 태그(Tag) · 배지(Badge) · 슬라이더(Slider) · 툴팁(Tooltip) · 스피너(Spinner) · 프로그레스(Progress) 등.

#### 인라인이 허용되는 반례 (보편 이름 없음)

- 설정 모달의 "Appearance 탭 본문 레이아웃" — 그 화면 전용 배치이며 재사용 단위가 아니다.
- 특정 패널의 헤더에 아이콘·제목·액션을 한 줄로 짜맞춘 합성 — "헤더바" 같은 보편 컴포넌트로 추출할 가치가 분명해지기 전까지는 인라인.
- 한 화면에서만 의미를 갖는 일회성 빈 상태(empty-state) 일러스트 영역.

단, 반례라도 **그 안에 들어가는 버튼·드롭다운 등 보편 컴포넌트는 공용 위젯을 호출**한다. "1회성 레이아웃"은 배치만 인라인이고, 부품은 공용 위젯이다.

## 관련

- `crates/tasty-egui-theme/` — Theme → egui Visuals/Style 어댑터
- `crates/tasty-gallery/` — 호출 사이트 + 데모 카탈로그
- [design/systems/theme](../design/systems/theme.md#ui-디자인-규칙-필수) — 4px 그리드·1px 보더 등 UI 디자인 규칙 — 위젯 내부 색·간격·상태 오버레이가 따르는 시각 규칙
- [dev-guide/model-view-split](../dev-guide/model-view-split.md) — Model 에 `egui::*` 를 두지 않는 분리 원칙(위젯은 View 측 primitive).

## 행 선택 표의 클릭과 복사

`Table::selectable(true)`는 본문 셀에서 기본 라벨 텍스트 선택을 끈다. 파일 이름 글자 위에서도 행 선택·우클릭·더블클릭이 작동해야 하기 때문이다. 헤더의 명시적인 정렬 클릭은 유지한다. 행 선택을 사용하지 않는 `selectable(false)` 표는 기본 텍스트 선택을 유지한다.

Table 빌더의 길이 setter(`header_pad_x`·`header_pad_right`·`header_height`·`row_height`·`max_scroll_height`)는 `TableColumnWidth`와 같이 `LogicalPx`를 받으며, egui에 넘기는 경계에서만 숫자로 꺼낸다. 높이를 지정하지 않은 표는 헤더와 본문 행 모두 `table_cell_height`(28)를 쓴다. 표 안에서는 세로 `item_spacing`을 0으로 둔다. egui_extras는 행 사이에 부모의 세로 간격을 두고 선택·줄무늬 배경을 그 간격까지 넓히기 때문이다. 그래서 `row_height`·`header_height`로 준 값이 곧 보이는 행 높이다. 표 뒤의 위젯에는 부모의 간격이 그대로 적용된다. 선택 행 배경은 `table_row_bg_selected`(surface-active), 행 선택 표의 hover 행 배경은 `table_row_bg_hover`(overlay-hover)다. egui_extras는 선택 행을 텍스트 선택색으로, hover 행을 위젯 hover 색으로 칠하므로 표가 행에만 두 색을 넣고, 셀 안의 텍스트 선택색과 위젯 hover 색은 그대로 둔다. 회귀 검사는 `crates/tasty-ui-widgets/tests/table_row_click.rs`의 `unsized_rows_and_header_use_table_cell_height`, `selected_row_fill_is_table_row_bg_selected`, `hovered_row_fill_is_table_row_bg_hover`다. egui_extras는 선택·hover·줄무늬 띠를 가로로도 `item_spacing.x`의 절반씩 셀 밖으로 넓힌다. 표는 열 사이 간격은 그대로 두고, 표를 둔 영역의 좌우 끝을 넘는 부분만 clip으로 잘라 띠가 표 폭 안에 머물게 한다. 표는 셀의 기본 글자색(`override_text_color`)을 시안대로 정한다. 행에서는 `table_row_fg`(text-secondary), 선택 행과 행 선택 표의 hover 행에서는 `text_primary`다. egui_extras가 선택 행 셀에 거는 선택 테두리색(accent)도 이 값으로 덮는다. hover 행은 egui_extras처럼 앞 프레임에서 포인터가 있던 행이며, egui_extras가 그 값을 밖에 내주지 않아 표가 따로 기억한다. 색을 명시한 RichText는 영향을 받지 않는다. 사용처는 kit 이 따로 색을 정한 셀(mono·strong 열의 text-primary, 대시·메타의 text-muted)만 색을 명시하고 나머지 셀은 이 계약에 맡긴다. 본체 포트 스캐너의 Workspace·Tab 셀이 그런 셀이다. 탐색기 Detail 표는 kit `DetailRow` 가 hover 행에서도 이름을 table-row-fg 로 두므로 이름 색을 직접 정한다(쉬는 행·hover 행 table-row-fg, 선택 행 text-primary, 잘라내기 대기 행은 50% 디밍). 회귀 검사는 `crates/tasty-ui-widgets/tests/table_band_and_ink.rs`의 `the_selected_band_stays_inside_the_table_area`, `an_uncoloured_label_in_the_selected_row_uses_text_primary`, `an_uncoloured_label_uses_table_row_fg_and_text_primary_when_hovered`다. `header_pad_right`는 `TableAlign::Right` 헤더 셀에만 오른쪽 여백을 준다. 본문 셀이 오른쪽에 같은 여백을 두는 열(탐색기 Size)에서 제목 끝을 값 끝과 맞추는 데 쓴다. 회귀 검사는 `header_pad_right_moves_right_aligned_titles_in`이다.

행 선택 표에서 복사가 필요하면 행 메뉴에 경로·주소 등 필요한 값의 복사 기능을 제공한다. 이 규칙은 본문 라벨의 기본 sense를 바꾸는 것이며, 셀에 명시적으로 넣은 버튼 등 다른 위젯의 입력까지 무효로 만들지는 않는다. 회귀 검사는 `crates/tasty-ui-widgets/tests/table_row_click.rs`에 있다.

행 선택 표의 우클릭은 `secondary_clicked_row`로만 나오고 `clicked_row`와 섞이지 않는다. 글자 위 우클릭도 행에 가며, 헤더 우클릭은 행 우클릭도 정렬 클릭도 아니다. 우클릭 행 번호는 스크롤한 뒤에도 `rows` 기준이다. 현재 우클릭 결과를 쓰는 소비처는 탐색기 Detail 뿐이다. 회귀 검사는 `crates/tasty-ui-widgets/tests/table_secondary_click.rs`이며, 탐색기 Detail(바깥 세로 ScrollArea + `virtual_rows`), 같은 배치에서 모든 행을 그리는 표, 갤러리 표(본문 높이 제한), 포트 표(`ports_table`: 가로 스크롤 + 본문 높이 제한) 네 구성을 함께 본다.

## 표의 가변 열과 가로 스크롤

`Table::virtual_rows(true)`는 화면에 걸친 행만 그린다(egui_extras `TableBody::rows`). 모든 본문 행의 높이가 같다는 전제에서 안 그린 행의 자리를 같은 높이로 비워 두므로 스크롤 길이는 행 수 그대로다. egui_extras 는 본문 맨 위를 기준으로 보이는 범위를 정하므로 모든 행을 한 번에 넘긴다. 앞에 따로 그린 행이 있으면 그 수만큼 범위가 밀려 위쪽 끝 행이 빠진다. 화면 밖 행의 셀 함수는 불리지 않으므로, 화면 밖에서도 유지해야 할 위젯(탐색기 detail 의 이름 입력)은 호출자가 표 밖에서 그린다. 클릭·우클릭 결과의 행 번호는 `rows` 기준이다. 셀 내용 폭으로 넓어지는 열(clip 없는 Remainder)은 그린 행으로만 폭을 잰다. `scroll_to_row(Some(i))`는 그 프레임에 `i` 행을 가운데로 스크롤한다. 보이는 행만 그리는 표에서 화면 밖 행으로 갈 때 쓴다. 회귀 검사는 `crates/tasty-ui-widgets/tests/table_virtual_rows.rs`의 `only_the_rows_on_screen_are_drawn`, `a_far_row_is_reached_with_every_row_in_view_drawn_and_clicks_report_its_index`(행 경계와 맞지 않는 여러 스크롤 위치에서 화면에 걸친 행이 모두 그려지는지 포함), `the_skipped_rows_keep_their_space_down_to_the_last_row`다. `virtual_rows`를 켜지 않은 소비처(포트 스캐너 표, 갤러리 표)는 기존 경로대로 매 프레임 모든 행의 셀을 그린다. 이 구분은 `table_secondary_click.rs`의 `only_virtual_tables_skip_rows_off_screen`이 검사한다.

`TableColumnWidth::Flex { min_width }`는 시안 Table 열 옵션 `minWidth`에 대응한다. 열은 남는 폭을 받지만 `min_width` 아래로 줄지 않는다. 시안처럼 고정 열(`Exact`)은 그 폭이 곧 하한이다. 가로 스크롤 표(`horizontal_scroll(true)`)에서는 그리기 전에 `resolve_scroll_widths`가 Flex 열의 폭을 정한다. 열 하한과 열 사이 간격의 합(`fixed_total_width`)이 표 폭보다 작으면 남는 폭을 Flex 열에 똑같이 나누고, 크거나 같으면 Flex 열도 하한을 유지해 본문이 스크롤한다. 표 폭에서는 본문의 세로 스크롤바 폭을 미리 뺀다. 고정 폭 열 위에 바가 겹쳐 그려지기 때문이며 이 문서의 폭 예약 예외에 해당한다. Flex 열이 없는 표는 열 폭을 바꾸지 않는다. 가로 스크롤을 쓰지 않는 표에서 Flex 열은 egui_extras remainder 열(최소 폭·clip)이 된다. 긴 셀 내용의 말줄임은 셀을 그리는 쪽이 맡는다. 회귀 검사는 `table.rs`의 `flex_keeps_its_floor_when_the_table_is_narrower_than_the_floors`, `spare_width_is_shared_equally_by_flex_columns_only`, `tables_without_flex_columns_keep_their_widths`다.

표는 색을 명시하지 않은 셀 글자를 선택 행에서 `table_row_fg_selected()`로 그린다. 색을 명시하는 셀은 `table_cell_ink(ui, theme, rest)`로 글자색을 정하고, 글자 외 분기는 `in_selected_table_row(ui)`로 판단한다. `checkbox_with_label_color`는 채움이 정해진 행에 놓이는 체크박스의 라벨 색을 받는다.

포트 스캐너 표의 열 정의는 `ports_table.rs`의 `PortsColumn`이다. 열별 하한·가변 여부·정렬, 표 꾸밈(`ports_table`: 행 선택·가로 스크롤·헤더 배경·헤더 왼쪽 여백), Process 셀(`ports_process_cell`: 이름만 말줄임하고 PID Tag 는 유지)을 본체 popup 과 갤러리 예제가 함께 쓴다.

## 자동완성 후보의 포인터 확정

`AutoComplete`는 자기 드롭다운 안의 주 버튼 누름에서만 입력 포커스를 유지한다. 후보를 떼기 전에 TextEdit의 blur가 호출자에게 원복으로 전달되지 않게 하며, 행 안에서 버튼을 떼면 원문 후보를 `Pick`으로 반환하고 입력 포커스를 해제한다. 후보 밖에서 떼면 선택하지 않는다. 다른 영역을 누르는 blur와 Enter·Esc는 이 포커스 유지에서 제외한다. `PathField`는 기존 우선순위인 취소 → 후보/버퍼 확정 → Go → 확정 없는 blur 원복을 적용한다. 공개 API와 일반 `Input`의 포커스 동작은 같고, 단독 AutoComplete의 버퍼 갱신은 호출자 책임이다.

## 공통 그래프 뷰

`graph_view` 모듈(crate 루트 재수출)은 노드·간선 그래프의 캔버스, 줌 클러스터, 미니맵, LOD 칩, 상세 Drawer 틀, DAG 카드를 제공한다. 데이터는 `GraphData` 계약으로, 카드 내용과 간선 잉크는 `GraphPainter`로, 문구는 `GraphChromeLabels`·`DagCardView`로 받는다. 본체 DAG surface·popup과 갤러리 DAG 예제가 함께 쓴다. 계약과 책임 경계는 [공통 그래프 뷰](../design/systems/graph-view.md)에 있다.

## 폴더 선택 footer

`FolderPickerFooter`(`folder_picker` 모듈, crate 루트 재수출)는 갤러리와 호스트 파일 선택기가 함께 쓰는 3행 footer다.
호출자가 번역한 라벨·안내·버튼 문구와 선택 경로를 전달한다. 경로는 readonly 토큰으로 앞 말줄임하며,
전체 경로를 툴팁에 남긴다. 안내의 폴더 이름만 고정폭으로 그리며 좁은 폭에서는 안내를 줄바꿈한다.
`height`와 `show`가 같은 안내 galley를 계산해 마지막 버튼 행의 자리를 보존한다. 파일이나 설정은 변경하지 않는다.

## HTML address band primitive

`html_address_bar` is a host-chrome view shared with the gallery. It composes IconButton,
Spinner and the Input box underlying PathField, without the PathField completion list or Go
slot. The current PathField composite always reserves the Go width and has no invalid or
read-only variant; its implementation is unchanged. Input has opt-in LogicalPx font/icon
size overrides and an inner editor margin override, so this field uses input-font-size
(13), icon-size-sm (14) and only the Input box padding, while
existing Input and PathField defaults stay unchanged. The HTML field accepts injected labels
and emits Back, Forward, Reload, Open and Revert actions without doing I/O or navigation.
The host HTML surface calls it from `src/adapters/ui/surface/html_address.rs`.
`html_address_field_id` names the field's egui id inside a band id, so the host can give up
the field focus after an open or when the native page takes the keyboard.

All lengths use LogicalPx and existing HTML address/input/banner Theme tokens. The band
is 40 logical pixels high, with a 24-pixel error row when needed and an inside separator.
Forward is omitted below the 440-pixel breakpoint; the field retains its 96-pixel minimum.
Loading replaces Reload with a noninteractive Spinner. Tooltips open above the buttons.
The idle address text uses html-address-value-fg, editing uses input-fg and the mirror
read-only field keeps input-readonly-fg; placeholder and disabled keep their Input roles.
A mirror field is read-only with Reload alone; a mirror placeholder allocates no band.
`html_address_content_inset` combines measured banner heights in size-before-script order:
first top margin, then each banner and its following gap. With no banners it returns only
the band height. Hosts must use the same measured lengths for native WebView placement.

HtmlAddressState keeps an editing/rejected draft across external address updates. Enter
emits an Open action without discarding the draft; the caller accepts or rejects the result.
Typing, Esc and accepted navigation clear an input error. Esc restores the latest address
supplied by the caller. Native keyboard focus, committed versus attempted address selection,
asynchronous lookup generations and settings-driven shortcuts remain host responsibilities.

`tasty_utils::html_address::resolve_html_address` is a pure resolver: explicit file/HTTP(S)
URLs and absolute OS paths take precedence; relative input uses the current local document
folder or web URL, then surface cwd only for an empty tab. Without a base it fails. Home is
an explicit parameter. Whitespace is trimmed only at the ends; local filename characters are
URL-encoded, and unsupported schemes are rejected. AddressTarget keeps the normalized URL
and decoded local lookup path separate. validate_local requires a caller-supplied successful
file lookup before local navigation; it rejects missing files, directories and lookup errors.
This preflight does not guarantee that subsequent loading succeeds. Web targets do not use
local lookup. Display shortens a home component prefix to ~ and retains URL fragments.

The gallery contains eight address states, script/size banner placement mocks, narrow and
mirror variants, and a live HTTP(S) input example. It opens neither local files nor WebViews.
