# 탭 스트립 (Workspace tabs)

- **Status**: Implemented
- **주체**: 로컬 사용자 (탭 도메인 조작은 AI Agent 도 — [work-area](../work-area/index.md))
- **설계**: 없음
- **코드**: `src/adapters/ui/tab_bar.rs` (`PaneTabBarView`/`TabBarAction`/`draw_pane_tab_bars_view`), drag 상태 `src/state/dialogs.rs` `TabDragState`
- **화면**: [아래 절](#화면)

## 목적

각 [Pane](../work-area/index.md#pane--상위-레이아웃-탭-무관) 상단의 탭 바. Pane 의 `tabs` 도메인과 View 가 pane 마다 기억하는 선택 탭을 사용자가 보고 조작하는 GUI 표면이다. **Pane 마다 독립 탭 스트립**을 가진다(상위 레이아웃이 탭과 무관하므로). 도메인 자체(탭 생성/닫기/이동 의미)는 [work-area](../work-area/index.md), 여기선 그 스트립의 *시각 + 입력*.

## 내부 동작 (headless-valid)

탭 스트립은 순수 view(`draw_pane_tab_bars_view`)가 그리고, 사용자 입력을 `TabBarAction` 으로 보고하면 wrapper 가 work-area 도메인에 반영한다. view 는 MainViewState/CoreState 비의존(데이터 props 만 받음).

### 입력 → 액션 (`TabBarAction`)

- **SwitchTab** — 탭 클릭 → 그 pane 의 선택 탭 전환.
- **CloseTab** — 탭의 close 버튼(활성 탭 또는 hover 시 노출) → 탭 닫기. (마지막 탭은 work-area 규칙상 안 닫힘.)
- **AddTab** — 우측 `+` → 새 탭. `+` 우클릭은 **OpenNewTabButtonContextMenu**(프리셋으로 탭/페인 생성).
- **RequestSplit** / **OpenSearch** — 스트립 우측 split·search 아이콘 → 해당 Pane 분할 / 활성 surface 검색.
- **ScrollLeft / ScrollRight** — 탭이 넘치면 좌우 스크롤 화살표(가로 스크롤 `scroll_offset`).
- **FocusPane** — 탭이 없는 빈 영역 primary click → 탭 전환 없이 그 Pane 으로 focus 만 이동.
- **DragStart / DragUpdate / DragEnd** — 탭 드래그로 순서 변경(`TabDragState`, drop 위치는 `compute_drop_index`). UI 전용 상태(영속 안 함).
- **OpenContextMenu** / **OpenPaneContextMenu** — 탭 우클릭 / Pane 우클릭 컨텍스트 메뉴.

### 클릭 → Pane focus 이동

**비-focused Pane 의 탭 스트립을 primary click 하면(탭 본체·빈 영역·스크롤 화살표·+/split/search 버튼) 그 Pane 으로 focus 가 이동한다** — 콘텐츠 영역 클릭과 대칭. 탭 전환(`SwitchTab`)과 focus 이동은 독립적이라, 빈 영역 클릭은 focus 만 옮기고 선택 탭은 그대로 둔다. 우클릭 컨텍스트 메뉴 3종(`OpenContextMenu`/`OpenPaneContextMenu`/`OpenNewTabButtonContextMenu`)은 대상 `pane_id`/`tab_index` 를 메뉴 항목에 직접 전달하므로 focus 이동이 없다. 사용자 마우스 클릭에 의한 이동이라 [focus 정책](../../design/policies/focus.md)의 "CLI/IPC 포커스 독립 원칙"과 충돌하지 않는다(그 원칙은 IPC/CLI/에이전트 유래 focus 강제를 막는 것). 구현: `TabBarAction::focus_target_pane` + `apply_tab_bar_actions`(`src/adapters/ui/tab_bar/apply.rs`).

### 탭 표시

각 탭은 표시명(work-area 우선순위로 결정) + 상태 표지를 보인다:

- **leading 아이콘** — surface kind 별(terminal/markdown/…). 아이콘은 registry `SurfaceKindDef.icon`(매니페스트 `icon` 이름)을 `icons::from_name` 으로 해석한다 — host 가 kind 를 하드코딩 분기하지 않는다.
- **알림 표지** — attention kind 에 따른 라벨(`tab_attention_kind`: `NeedsInput`=노랑, `Completion`=파랑, 없으면 평상시 색).
- **busy 점** — 녹색 점(`tab_is_busy`). 판정은 [busy indicator 정책](../../design/policies/busy-indicator.md) — 셸이 아닌 프로그램이 최근 출력을 냈을 때이고, 이미 busy 인 탭은 타이핑해도 꺼지지 않는다. 터미널 제목만 바꾸는 출력(입력을 기다리는 Codex 의 제목 깜빡임 등)은 최근 출력으로 치지 않는다.
- **활성/포커스** — active 탭 강조, 포커스된 Pane 인지에 따라 스트립 배경이 달라짐.

탭 1개 너비·라벨 폰트 크기는 **사용자 옵션**(`tab_width`/`tab_font_size`).

## 인터페이스

- **사용자 트리거**: 탭 클릭(전환), close 버튼, `+`(추가, 우클릭=프리셋 메뉴), split/search 아이콘, 좌우 스크롤 화살표, 드래그(순서 변경), 우클릭(컨텍스트 메뉴). 단축키 경유 동작은 work-area/단축키.
- **AI Agent**: 탭 *도메인* 조작은 [work-area](../work-area/index.md) CLI/IPC (`tasty new tab` / `close tab` / `list tabs`). 탭 *스트립 위젯* 은 GUI 전용.

## 비-목표 (Out of scope)

- **탭/Pane 도메인 동작 정의**(생성·닫기·이동·분할의 의미·규칙) — [work-area](../work-area/index.md).
- **탭 표시명 우선순위 규칙** — work-area Tab.
- **컨텍스트 메뉴 각 항목의 동작** — 해당 기능(프리셋 등).

## Acceptance Criteria

- Given Pane 에 탭 여럿 When 탭 클릭 Then 그 탭으로 전환되고 하위 레이아웃이 바뀐다.
- Given 활성 탭 When close 버튼 클릭 Then 탭이 닫힌다(마지막 탭이면 안 닫힘) — 시야는 그 자리로 밀려 들어온 탭(마지막이었으면 직전 탭)으로 간다.
- Given 사용자가 보고 있지 않은 탭 When 그 탭이 닫힌다(에이전트 `surface.close` 포함) Then 보던 탭이 그대로 유지된다 — 닫힌 탭이 앞쪽이어서 인덱스가 밀려도 마찬가지다 ([focus 정책](../../design/policies/focus.md) "삭제로 인한 인덱스 이동").
- Given 탭이 스트립 폭을 넘침 When 스크롤 화살표 Then 가로 스크롤된다.
- Given 탭 드래그 Then drop 위치(`compute_drop_index`)대로 순서가 바뀐다.
- Given 탭 이름 변경 팝업이 열림 When 에이전트가 탭을 옮기거나 다른 탭을 닫아 인덱스가 바뀐다 Then 저장한 이름은 팝업을 연 탭에 붙는다. 그 탭이 닫히면 팝업이 닫힌다.
- Given 탭 우클릭 메뉴가 열림 When 에이전트가 탭 순서를 바꾸거나 다른 탭을 닫는다 Then 고른 항목(이름 변경·닫기·좌우 이동·프리셋 저장)은 메뉴를 연 탭에 적용된다. 그 탭이 먼저 닫혔으면 아무것도 하지 않는다.
- Given busy/알림 상태 Then 녹색 점 / 노란 라벨이 표시된다.
- Given 비-focused Pane When 그 Pane 의 탭/빈 영역/스크롤 화살표를 클릭 Then 그 Pane 으로 focus 가 이동한다(빈 영역 클릭은 선택 탭 불변).
- Given 비-focused Pane When 그 Pane 의 탭/빈 영역 우클릭 Then focus 는 이동하지 않는다(컨텍스트 메뉴만 열림).

> GUI 위젯이라 시각은 스크린샷, 결과(탭 전환/닫기/순서)는 work-area `tasty list tabs` 로 교차 확인.

## 구현

- view: `src/adapters/ui/tab_bar/view.rs` — `draw_pane_tab_bars_view`(props→`PaneTabBarsOutput{actions, measured_height_physical}`), `compute_drop_index`(드래그 drop 위치).
- 탭 한 칸 렌더링: `src/adapters/ui/tab_bar/tab.rs` — `TabRenderContext`/`draw_tab`(표시·클립·클릭·드래그). strip 조립과 drag overlay는 `view.rs`의 `strip_geometry`를 함께 사용한다.
- 공개 진입점과 재수출: `src/adapters/ui/tab_bar.rs`.
- props: `PaneTabBarView`(pane별 탭명/kind/알림/busy/active/focus/scroll), `PaneTabBarsProps`(테마/탭폭/폰트/drag).
- 액션 반영: `apply_tab_bar_actions`(`src/adapters/ui/tab_bar/apply.rs`) — `TabBarAction::focus_target_pane` 로 primary-click 계열 액션 처리 전 focus 를 선-이동한다.
- drag 상태: `src/state/dialogs.rs` `TabDragState`(UI 전용, 비영속).

## 화면

화면정의서 — **탭 스트립 화면**.

- **시각 소스**: tasty-page `vendor/ui_kits/terminal/work.jsx` (탭 바 부분)

[작업 영역](../work-area/index.md#화면) 안, 각 Pane 머리의 탭 바. 동작은 부모 기획, 여기선 시각.

### 트리거

Pane 이 존재하면 항상 그 위에 표시(Pane 마다 하나).

### UI 요소 인벤토리

```
┌ 탭 스트립 (Pane 하나) ─────────────────────────────────┐
│ [◀] [⬡ tab1 ●][⬡ tab2  ✕][⬡ tab3 ⚠] … [+] │ [⊟][🔍] [▶]│
└────────────────────────────────────────────────────────┘
  ◀▶ 스크롤   ⬡ kind 아이콘  ● busy  ✕ close  ⚠ 알림  + 추가  ⊟ split  🔍 search
```

- **탭** — leading **kind 아이콘** + 표시명. 상태 표지: **busy 녹색 점**, **알림 노란 라벨**. **활성 탭** 강조, **포커스 Pane** 여부로 스트립 배경(surface0 vs mantle) 구분.
- **close 버튼**(✕) — 활성 탭 또는 hover 시 우측에 노출. 숨어 있을 때도 칸(`tab_close_size`)을 지켜 hover 때 제목이 밀리지 않는다. lock 표지 위에 포인터가 있어도 탭 hover라 close가 보인다.
- **오른쪽 상태 묶음** — 탭 칸 오른쪽 끝(`space-xs` 안쪽)에 고정 묶음 하나를 둔다. 왼쪽에서 오른쪽으로 html 스크립트 표지 · move 글리프 · busy 점 · close 순서이고, 항목 간격은 `tab_status_gap`(4), 제목과 묶음 사이는 `tab_gap`(8)이다. 묶음은 줄지 않아 탭이 좁으면 제목이 먼저 말줄임된다. 배치는 `tab_bar/tab.rs::status_cluster`다.
- **`+` 추가 버튼** — 새 탭. 우클릭 = 프리셋 생성 메뉴.
- **스크롤 화살표**(◀▶) — 탭이 폭을 넘칠 때만. chevron 아이콘(`tab_scroll_arrow_glyph_size`)을 스트립 높이와 같은 정사각 칸에 그리고, 칸은 채우지 않아 스트립 바탕이 그대로 보인다. 끝에 닿은 쪽은 disabled 잉크이며 hover 채움과 클릭 응답이 없다. 스크롤할 수 있는 쪽에만 hover 채움(`tab_scroll_arrow_hover_bg`)을 깐다. 공용 위젯 `horizontal_tab_bar_with_arrows`도 같은 painter(`tasty_ui_widgets::paint_tab_scroll_arrow`)를 쓴다. 본체 탭 스트립의 화살표 칸 폭은 스트립 높이와 같은 zoom 면제 값 `tab_bar_height`다. 같은 토큰의 `tab_scroll_arrow_width()`는 UI zoom을 따라 zoom 면제 스트립에서 정사각이 깨지므로 공용 위젯만 이 접근자를 쓴다. 이동 대기 대상인 탭, 또는 잘라 둔 서피스를 담은 탭이 활성 탭이 아닐 때 그 탭 칸이 스크롤에 가려지면 그쪽 화살표가 move 분홍 잉크로 바뀐다. 활성 탭 안의 서피스는 콘텐츠 영역의 링으로 보이므로 화살표 표시가 없다([이동 대기 표시](../surface-move/index.md#이동-대기-표시)).
- **우측 액션** — split(⊟) / search(🔍) 아이콘 → 해당 Pane 분할 / 활성 surface 검색.
- 탭 너비·라벨 폰트 크기는 **사용자 옵션**.

### 상태별 시각

- **활성 vs 비활성 탭** / **포커스 Pane vs 비포커스** — 배경·강조 차이.
- **busy / 알림** — 녹색 점 / 노란 라벨.
- **오버플로** — 스크롤 화살표 노출 + 가로 스크롤.
- **드래그 중** — 드래그 탭 overlay + drop 위치 표시.
- **close 노출** — 활성 또는 hover 시에만.

### 시각 소스

tasty-page `vendor/ui_kits/terminal/work.jsx` 의 탭 바 — 탭 치수·아이콘·표지·간격의 단일 출처.
