# 윈도우 크롬 (Window chrome / CSD)

- **Status**: Implemented
- **주체**: 로컬 사용자 (GUI 전용 — 윈도우 조작)
- **ADR**: 없음 (CSD 데코 전략은 attach decision 과 무관, 원칙 4 크로스플랫폼)
- **코드**: `crates/tasty-platform/src/window_chrome.rs` (CSD 속성·`resize_direction_at`), `src/adapters/ui/titlebar/` (`mod.rs`/`view.rs`/`caption.rs`), `src/adapters/ui/sidebar/` (`view.rs`/`full.rs`/`collapsed.rs`, 리사이즈 위젯 우선권 적재), `src/view/main/mouse.rs` (통합 리사이즈 hit-test)
- **화면**: [아래 절](#화면)

## 목적

[MainView](../main-view/index.md) 윈도우의 **CSD(Client-Side Decorations) 타이틀바**와 OS별 데코레이션 전략. winit `with_default_menu(false)` 로 tasty 가 창틀을 직접 소유하므로, 타이틀바·캡션 버튼·리사이즈 보더를 OS별로 직접 처리한다. 원칙 4(Windows/macOS/Linux 모두 1급)의 집행 지점.

## 내부 동작 (headless-valid)

### OS별 데코 전략 (`apply_csd_attributes`)

윈도우 생성 시 OS별로 다른 winit 속성을 적용한다:

- **macOS**: `titlebar_transparent` + `fullsize_content_view` + `title_hidden`. 타이틀바를 투명화하고 콘텐츠를 y=0 까지 확장하되 **OS 신호등(close/min/zoom)은 그대로 유지** — 클릭/hover/풀스크린/접근성/다크모드 디밍은 OS 가 처리. (`with_decorations(false)` 는 신호등까지 없애므로 안 씀.)
- **Linux**: `with_decorations(false)`. WM/컴포지터 데코를 끄고 tasty 가 DE 가변 캡션 버튼을 직접 그린다. 가장자리 리사이즈는 `drag_resize_window`, 이동은 `drag_window`(winit 0.30 표준).
- **Windows**: `with_decorations(false)` + `with_undecorated_shadow(true)`. OS 캡션/보더 제거 후 드롭 섀도는 복원, tasty 가 우측 캡션 버튼(min/max/restore/close)을 직접 그린다. 가장자리 리사이즈는 Linux 와 동일한 단일 MainView 경로로 처리(아래 "리사이즈 엣지").

### 타이틀바 (`titlebar/`)

순수 view(`draw_titlebar_view`)가 `TopBottomPanel::top` 으로 full-width 상단 바를 그리고, 입력을 `TitlebarAction` 으로 보고 → wrapper 가 winit 조작으로 브리지한다.

- **드래그 이동** (`StartDrag` → `drag_window`) — 빈 타이틀바 영역 드래그.
- **더블클릭 maximize** (`ToggleMaximize` → `set_maximized`).
- **캡션 버튼** (Linux/Windows, tasty 가 그림): `Minimize`/`ToggleMaximize`/`Close`. macOS 는 OS 신호등이라 tasty 가 안 그림(`controls=None`).
- **Close** 는 네이티브 `CloseRequested` 와 동일 라이프사이클(`AppEvent::CloseWindow`)로 라우팅 — 사용자 클릭이라 IPC 비노출(원칙 1).
- macOS 만 좌측 신호등 폭(`MACOS_TRAFFIC_LIGHT_INSET`)만큼 드래그 hit 영역을 carve-out.

### 상단 inset

타이틀바는 항상 그려지며 `top_inset`(= `titlebar_height` 토큰)을 반환한다 — `compute_terminal_rect` 가 작업 영역을 그만큼 아래에서 시작시킨다(상태바 `bottom_inset` 과 대칭).

### 리사이즈 엣지 (위젯 우선순위 단일 모델)

데코 없는 Windows/Linux 창의 가장자리 리사이즈는 **단일 MainView 경로**로 통일된다(`src/view/main/mouse.rs`). 리사이즈는 **가장자리 실제 인터랙티브 위젯 위가 아님 AND 커서가 가장자리 margin 안 AND 비최대화 AND 오버레이/팝업/배너 없음**일 때만 발동한다.

- `egui_consumed`(egui-winit `wants_pointer_input()`)는 **패널/Area 전체의 bounding rect** 단위라 리사이즈 게이트에 쓰지 않는다 — 상시 렌더되는 타이틀바(36px)/상태바(24px) 스트립 전체가 `RESIZE_EDGE_MARGIN`(8px)보다 두꺼워, 그 값을 그대로 쓰면 버튼 없는 빈 여백에서도 리사이즈가 막힌다.
- 대신 `AppState.resize_edge_widget_hovered`(**위젯 단위**)로 게이트한다. 타이틀바 창 버튼(`titlebar/view.rs::draw_window_buttons`)·Windows 캡션 버튼(`titlebar/caption.rs`)·상태바 클릭 요소(`status_bar.rs`)·사이드바 클릭 요소(`sidebar/view.rs` — 헤더 접기 버튼, Tools/Plugins/Settings/New Workspace, 카테고리 헤더, 워크스페이스 행/아바타, rail 카테고리 버튼 전부)가 각자 `Response::hovered()` 를 매 프레임 이 필드에 적재(타이틀바가 프레임당 첫 draw 라 리셋, 이후는 OR 누적) — 실제 버튼/행 위에서만 리사이즈가 양보되고, 빈 여백은 항상 리사이즈로 동작한다. 사이드바의 배경 우클릭 캐처(`bg_resp`, 빈 영역에서 컨텍스트 메뉴만 여는 캐처)는 타이틀바 드래그 rect 와 동일한 이유로 의도적으로 미적재 — 빈 시각 공간은 리사이즈에 양보해야 한다.
- `resize_direction_at` 는 좌표가 가장자리 margin(`RESIZE_EDGE_MARGIN`) 안이면 8방향 `ResizeDirection` 을 돌려주는 순수 함수(OS 무관 컴파일·테스트).
- `handle_mouse_input` 이 좌클릭 press 에서 hit-test → `Some(dir)` 이면 `drag_resize_window(dir)` 후 early-return.
- `handle_cursor_moved` 가 hover 방향을 `AppState.pending_resize_cursor` 에 저장하고, egui 프레임(`run_egui_frame`)이 `set_cursor_icon` 으로 ↔ 커서를 적용한다(egui 가 매 프레임 winit 커서를 덮으므로 프레임 내 적용 필수). 이 경로는 `egui_consumed` 대신 `is_using_pointer()` 기반 hover 판정을 쓰므로(egui-winit `CursorMoved` 처리) 애초에 빈 여백에서 리사이즈 커서가 정상적으로 뜬다 — 클릭 게이트만 어긋나 있었다.
- **커서 우선순위**: 보더 호버 중(`pending_resize_cursor` 가 `Some`)에는 리사이즈 커서(↔ 등)가 surface 커서(터미널 I-beam)·링크 hover(PointingHand)보다 **우선**한다. terminal surface 가 윈도우 우측 끝까지 full-bleed 로 닿아 8px 보더 픽셀을 자기 영역으로 포함하므로(`compute_terminal_rect` 우측 inset 없음), 프레임 직후 커서 덮어쓰기(`src/gfx/gpu.rs`)를 `pending_resize_cursor.is_none()` 으로 게이트해 보더 위에서만 리사이즈 커서를 보존한다. 보더 밖에선 `None` 이라 surface/링크 커서 동작 무변경, macOS 는 이 필드가 항상 `None` 이라 무영향.
- **macOS 는 데코 있는 창**이라 OS 가 네이티브 보더에서 리사이즈를 처리한다 → 위 hit-test/커서 저장 블록은 `#[cfg(not(target_os = "macos"))]` 가드로 macOS 에서 컴파일·실행되지 않는다.

## 인터페이스

- **사용자 트리거**: 타이틀바 드래그(이동), 더블클릭(maximize), 캡션 버튼 클릭(min/max/close), 가장자리 드래그(리사이즈). macOS 신호등은 OS 처리.
- **AI Agent**: 없음 — 윈도우 데코/이동/리사이즈는 사용자 행동(원칙 1). 멀티 윈도우 *생성* 자체는 [main-view](../main-view/index.md).

## 비-목표 (Out of scope)

- **타이틀 텍스트 표시** — 현재 타이틀바는 드래그 영역 + OS별 컨트롤만(중앙 타이틀 없음).
- **윈도우 생성/생명주기** — [main-view](../main-view/index.md).
- **둥근 모서리/그림자 프레이밍**(Linux) — 윈도우 투명화 필요, 별도 후속.
- **단축키 정의** — `KeybindingSettings`.

## Acceptance Criteria

- Given 각 OS When 윈도우 생성 Then macOS=신호등 유지+콘텐츠 확장, Linux/Windows=OS 데코 제거 + tasty 캡션 버튼.
- Given 타이틀바 빈 영역 When 드래그 Then 윈도우가 이동한다.
- Given 타이틀바 When 더블클릭 Then maximize 토글된다.
- Given 캡션 close 버튼 When 클릭 Then 네이티브 닫기와 동일 경로로 윈도우가 닫힌다.
- Given 창 가장자리 When 드래그 Then 해당 방향으로 리사이즈된다.

> OS별 시각·동작이라 각 플랫폼 스크린샷 + 수동 윈도우 조작으로 검증. `resize_direction_at` 은 단위 테스트(OS 무관).

## 구현

- CSD 속성: `crates/tasty-platform/src/window_chrome.rs` (`apply_csd_attributes`, `resize_direction_at`, `RESIZE_EDGE_MARGIN`).
- 타이틀바: `src/adapters/ui/titlebar/mod.rs`(wrapper `draw_titlebar`/`top_inset`/`os_controls`/`resize_cursor`), `view.rs`(순수 view + `TitlebarAction`/`WindowButton`/`ControlSide`), `caption.rs`(Windows 캡션).
- 가장자리 리사이즈: `src/view/main/mouse.rs`(`handle_mouse_input`/`handle_cursor_moved` hit-test), `AppState.pending_resize_cursor`/`resize_edge_widget_hovered`(`src/state.rs`), 커서 적용 `src/gfx/gpu/egui_bridge.rs`, 커서 우선순위 게이트 `src/gfx/gpu.rs`(`pending_resize_cursor.is_none()`).
- 리사이즈 위젯 우선권 적재: `src/adapters/ui/titlebar/view.rs`(`TitlebarDrawResult::resize_priority_hovered`)·`caption.rs`(`CaptionDrawResult::hovered`, Windows)·`crates/tasty-ui-widgets/src/status_bar.rs`(`StatusBarDrawResult::resize_priority_hovered`, 본체 wrapper `src/adapters/ui/status_bar.rs` 가 적재)·`src/adapters/ui/sidebar/view.rs`(`SidebarFullDrawResult`/`SidebarCollapsedDrawResult::resize_priority_hovered`) → `titlebar::draw_titlebar`/`status_bar::draw_status_bar`/`sidebar::full::draw_full_sidebar`/`sidebar::collapsed::draw_collapsed_sidebar` 가 `AppState.resize_edge_widget_hovered` 에 적재(타이틀바만 리셋, 나머지는 OR).

## 화면

화면정의서 — **윈도우 크롬 화면 (CSD 타이틀바)**.

- **시각 소스**: `site/vendor/ui_kits/terminal/` (`chrome.jsx`, `titlebar/titlebar_linux.jsx`) — claude design

[MainView](../main-view/index.md#화면) 최상단의 OS별 타이틀바. 동작은 부모 기획, 여기선 OS별 시각 배치.

### 트리거

윈도우가 열리면 항상 최상단에 표시(`top_inset` 만큼 작업 영역을 밀어냄).

### UI 요소 인벤토리 (OS별)

```
macOS  :  ●●●                          (OS 신호등, 좌상단 고정 · 나머지 드래그 영역)
Linux  :                    [ _ ] [ ▢ ] [ ✕ ]   (DE 가변 캡션, tasty 가 그림 · 우측 기본)
Windows:                    [ _ ] [ ▢ ] [ ✕ ]   (캡션 버튼 tasty, OS 캡션 제거 + 드롭섀도)
```

- **드래그 영역** — 타이틀바 빈 공간(이동/더블클릭 maximize). macOS 는 좌측 신호등 폭만큼 carve-out.
- **OS 컨트롤**:
  - macOS — **OS 네이티브 신호등**(tasty 가 그리지 않음).
  - Linux — tasty 가 그리는 DE 가변 버튼(기본 우측 min·max·close).
  - Windows — tasty 가 그리는 캡션 버튼(min/max/restore/close).
- **하단 1px 보더**(`titlebar_border` 토큰).
- 중앙 타이틀 텍스트 **없음**.

### 상태별 시각

- **활성 / 비활성 창** — `titlebar_bg` vs `titlebar_bg_inactive`, 버튼 글리프도 `titlebar_fg`(inactive) 차이.
- **maximize 상태** — Windows/Linux maximize 버튼 글리프가 restore 형태로 바뀜.
- **hover** — 캡션 버튼 hover 배경(close 는 강조색).

### 시각 소스

`site/vendor/ui_kits/terminal/chrome.jsx`(공통)·`titlebar/titlebar_linux.jsx`(Linux 캡션) — 타이틀바 높이·색 토큰·버튼 배치의 단일 출처. macOS 신호등 geometry 는 OS 고정(테마 토큰 아님).

## 관련

- [architecture/boot-sequence.md](../../architecture/boot-sequence.md) "로딩 프레임" — 이 창이 표시되기 전, 부팅 상태 머신이 그리는 워드마크+스피너+phase 문구 로딩 화면.
- [architecture/shutdown-sequence.md](../../architecture/shutdown-sequence.md) "종료 화면" — 창이 사라지기 전, 종료 상태 머신이 같은 락업을 문구만 바꿔 그리는 화면.
</content>
