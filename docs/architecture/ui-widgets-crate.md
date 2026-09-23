# `tasty-ui-widgets` — 본체·갤러리 공유 UI primitive

`crates/tasty-ui-widgets/` 는 본체(`tasty`)와 갤러리(`tasty-gallery`)가 공유하는 *egui layout / 위젯 primitive* 다. 본체의 settings 모달 등 UI 코드와 갤러리 데모가 **동일 함수를 호출**해 시각 100% 동기화를 보장한다.

## 위치와 의존 방향

```
tasty-egui-theme     egui 어댑터 + 색·폰트 토큰
       ↑
tasty-ui-widgets     layout / 위젯 primitive (본 문서)
       ↑                ↑
   tasty (본체)    tasty-gallery
```

- 의존: `egui`, `egui_extras`, `tasty-icons`, `tasty-egui-theme`, `tasty-type-appearance`(Theme schema), `tasty-type-geometry`(`LogicalPx`/`PhysicalPx`).
- **본체(`tasty`) 미의존** — widgets crate 는 본체 state·plugin·전역 `theme()` 를 모른다. 모든 함수는 `&Theme` 을 **명시적 인자**로 받는다.
- 갤러리는 본체 빌드와 분리 — `cargo build` 기본 타깃이 `tasty-gallery` 를 의존하지 않는다. `cargo run -p tasty-gallery` 로만 실행되는 standalone 데모.

## 레이아웃 idiom 카탈로그

이 표는 **layout idiom**(화면 배치 패턴) 함수만 다룬다 — `button`/`select`/`multi_select`/`table`/`toggle`/`chip`/`tooltip`/`spinner`/`autocomplete`/`segmented`/`tree_row`/`path_field`/`menu_item`/`icon_button`/`input`/`status_dot`/`warning_callout`/`help_hint` 등 이름으로 식별되는 보편 컴포넌트(아래 "확장 가이드" 1번 카테고리)는 `crates/tasty-ui-widgets/src/`에 각자 파일로 존재하지만 여기 표에는 나열하지 않는다 — 전체 위젯 목록·시각은 [`tasty-gallery`](../dev-guide/gallery-first.md)가 단일 출처다(gallery-completeness 정책상 본체의 모든 컴포넌트가 갤러리에 노출된다).

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
(`*_CORNER_RADIUS`), 토스트·중앙 블록 기하 등. 전수는 소스가 정본이다(여기 열거하면
다음 추가에서 바로 낡는다).

이 부류에는 규칙이 둘 붙는다.

- **값이 토큰과 같으면 이쪽이 아니라 SIZING 쪽이다.** 값이 같은 const 는 토큰의
  사본이고, 사본은 `Theme::with_colors_and_zoom` 의 `zoomed()` 경로 밖이라 `ui_scale`
  을 타지 않는다 — zoom 1 에서만 같고 나머지 배율에서 갈라진다
  ([ADR-0126](../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md)).
- **그래서 스케일 밖 const 에는 사유를 적는다.** 어느 토큰 근처인지, 왜 스냅하지
  않는지, 그리고 zoom 을 안 타는 대가를 doc 주석에 남긴다. 대가의 크기는 축마다
  다르다 — 반경 토큰은 zoom 을 타므로 실재하고, `border_width` 처럼 애초에 zoom 을
  안 타는 축은 대가가 없다.

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

탭 전환에 1 프레임(~16ms) 지연이 생기지만 인지 불가 수준 — closure 모델의 자연스러운 비용.

`horizontal_tab_bar_with_arrows` 의 chevron 아이콘은 `tasty-icons` 의 `CHEVRON_LEFT`/`CHEVRON_RIGHT` 를 쓰며, 호출자가 `egui_extras::install_image_loaders` 를 미리 호출했다고 가정한다.

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

"재사용처가 1곳뿐이니 YAGNI" 논리는 **이 부류엔 적용하지 않는다.** 보편 컴포넌트는 개발을 계속하면 반드시 다른 화면에서 다시 쓰게 된다 — 표는 두 번째 목록 화면에서, 버튼은 모든 다이얼로그에서, 드롭다운은 다음 설정 항목에서. 그때 인라인 구현이 흩어져 있으면 각각 따로 손봐야 하고, 시각·동작이 화면마다 미묘하게 갈라진다. 처음부터 한 곳에 정의해두면:

- **시각 단일 출처**: 토큰·간격·상태 오버레이가 한 함수에 모여 화면 간 불일치가 원천 차단된다([theme UI 디자인 규칙](../design/systems/theme.md#ui-디자인-규칙-필수)).
- **demo=main 보장**: 본체와 갤러리가 같은 함수를 호출하므로 갤러리 specimen 이 곧 본체 모습이다(mirror 아님).
- **두 번째 사용처가 공짜**: 다음에 같은 컴포넌트가 필요할 때 새로 그릴 게 없다.

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
