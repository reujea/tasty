# 전체화면 무대(Fullscreen stage) 시스템

무대는 **창 전체를 독점하는 독립 표면**이다. 기존 Workspace/Pane/Tab/Surface 트리와
**병렬로** 존재하며, 기존 요소를 확대한 것이 아니다. 용어 구분은
[concepts/ubiquitous-language](../../concepts/ubiquitous-language.md), 결정의 근거·대안은
[ADR-0082](../../adr/0082-fullscreen-independent-stage.md). 이 문서는 시스템 *동작 모델* 이고, 끝의 [기능 명세](#기능-명세--상태--인터페이스--acceptance-criteria) 절이 상태 · 인터페이스 · Acceptance Criteria 를 담는다.

구현: `src/adapters/ui/fullscreen.rs`(무대 셸·닫힘 훅 drain) + `.../fullscreen/defs.rs`(정적
테이블) + `src/fullscreen_stages.rs`(gui 무관 메타) + `AppState`(상태) + `Gpu::render`(렌더 분기).

## 모델

1. **별개 데이터** — 무대 콘텐츠는 뒤의 tasty 개체와 내부 로직상 연관이 없다. "이 popup 을
   전체화면으로" 는 그 popup 을 확대하는 것이 아니라 **같은 형상의 별개 인스턴스**를 무대에
   구성하는 것이다.
2. **원본은 그대로** — 무대가 유지되는 동안 뒤는 가려져 있으므로 **redraw 하지 않는다.**
   나올 때 그때 화면에 보이는 것을 다시 그린다.
3. **창당 하나** — 무대 상태는 `AppState`(= `MainView` 당 하나)의 `Option` 필드다. 한 창에
   최대 1 개, 창이 여럿이면 창마다 독립적으로 가질 수 있다.
4. **선언된 것만** — 무대에 올릴 수 있는 것은 정적 테이블(`fullscreen::defs::all_defs()`)에
   등록된 `StageDef` 뿐이다.
5. **휘발성** — 영속화하지 않는다. `src/core/layout_persistence/` 는 무대를 모른다(재시작이
   전체화면 상태로 부팅되면 창을 조작할 수 없다).

## 구조 — popup 시스템과의 대칭

| popup | 무대 |
|-------|------|
| `PopupDef` + `popup::defs::all_defs()` | `StageDef` + `fullscreen::defs::all_defs()` |
| `PopupManager`(다중 + z-order) | `Option<StageState>`(하나뿐이라 관리자 불필요) |
| `PopupManager::close` → `closed_queue` → `on_close` | `AppState::close_fullscreen_stage` → `stage_closed_queue` → `drain_on_close_hooks` |
| `draw_popup_layer` | `draw_fullscreen_stage` |

`StageDef` 필드: `meta`(`&'static StageMeta` — `id`(진입 경로가 지정하는 이름) · `title_key`(i18n), `src/fullscreen_stages.rs`) · `draw_fn` · `on_close`.
무대 콘텐츠의 **자체 상태**는 popup 관례를 따른다 — 무대 id 로 키를 만든 egui temp memory 에
두고 `on_close` 에서 지운다. `StageState` 에는 무대 수명 그 자체에 속하는 것만 담는다.

## 셸이 그리는 것 / 콘텐츠가 그리는 것

셸(`draw_fullscreen_stage`)은 **창 전체 scrim + 상단 제목 + 종료 버튼**을 그리고, 그 아래를
`content_rect`(제목 띠 아래 · 바깥 여백 `space-xl`)로 잘라 `draw_fn` 에 child `Ui` 로 넘긴다.
콘텐츠는 chrome 위치를 알 필요도, 침범할 수도 없다.

**종료 버튼은 셸이 공통 제공한다** — 콘텐츠 구현자가 빠뜨릴 수 없어야 하기 때문이다. 무대
프레임은 host chrome 을 통째로 건너뛰어 창 닫기·최소화 버튼(CSD 타이틀바)까지 사라지므로,
종료 수단이 없는 무대는 창을 빠져나갈 방법이 없는 상태가 된다. `PopupManager` 가 X 버튼을
중앙 관리하는 것과 같은 구조다.

따라서 popup 형상을 옮겨오는 콘텐츠도 **popup 타이틀바는 다시 그리지 않는다** — 제목과 닫기는
셸 chrome 의 몫이라 역할이 겹친다. 재사용하는 "형상" 은 프레임(배경/보더)과 콘텐츠다.

## 진입 경로 — popup 타이틀바의 전체화면 버튼

무대를 선언한 popup(`PopupDef.fullscreen_stage = Some(<stage id>)`)은 타이틀바 X 왼쪽에
전체화면 버튼을 얻는다([popup.md 규칙 2·8](popup.md)). 누르면 그 무대가 뜨고 **원본 popup 은
열린 채 남는다** — 무대가 덮을 뿐이다. 두 정적 테이블(popup / 무대)의 id 정합은 단위 테스트가
강제한다.

첫 소비자는 **알림 무대**(`notifications`)다: popup 의 형상 함수
(`notification::draw_notification_content_inner`)가 `PopupState` 기하에 의존하지 않아 무대가
그대로 호출할 수 있고, 목록형이라 넓은 화면에서 실익이 있다. 무대의 자체 상태는 목록 스크롤
위치뿐이며(셸이 콘텐츠 Ui 를 무대 id 로 salt 하므로 popup 쪽과 다른 egui state 를 쓴다),
`on_close` 가 그것을 지운다.

## 디자인 소스 — 신규 시안 없이 만든 이유

무대 셸(제목 · 종료 버튼 · 콘텐츠 프레임)과 popup 타이틀바의 진입 버튼에는 대응하는
**디자인 시안이 없고, 디자인 요청도 발주하지 않았다.** 근거는 "앞선 트랙도 안 했으니
선례상 불필요" 가 **아니다** — 앞선 트랙들은 필요할 때 실제로 요청문서를 발주했다.
여기서 발주하지 않은 이유는 이 화면이 **새 시각 결정을 하나도 만들지 않았기** 때문이다:

- **글리프**: canonical `icons.json` 의 `close` / `fit` 를 그대로 쓴다(신규 글리프 없음).
- **위젯**: 종료 버튼은 기존 `IconButton`(ghost · md) 그대로.
- **색 · 치수 · 간격**: 전부 확정 토큰의 조합이다 — 바깥 여백 `space-xl`, 제목
  `font-size-heading`, 배경 `scrim`, 콘텐츠 프레임 `surface-raised` + 1px
  `border-strong`, painter 전사 글리프 굵기 `icon-stroke-width`.

배치 수치(제목 = 상단 + `space-xl`, 종료 버튼 = 우상단 `space-xl`, 콘텐츠 inset)는
디자인이 준 값이 아니라 **구현자가 위 토큰을 조합해 정한 것**이다. 토큰 밖의 값을 새로
만든 곳은 없다.

따라서 무대에 **토큰으로 표현되지 않는 시각 요소**(고유 레이아웃 그리드, 신규 글리프,
새 색 역할, 무대 전용 chrome 형태)가 필요해지는 순간에는 그때 요청을 발주해야 한다 —
절차는 [design-change-workflow](../../dev-guide/design-change-workflow.md).

## 진입 / 종료

- 진입 `AppState::open_fullscreen_stage(id)` — 테이블에 없는 id 는 `false` 로 거부. 다른 무대가
  올라와 있으면 **그 무대를 닫고**(훅 경유) 교체한다. 같은 id 재진입은 no-op(콘텐츠 상태가
  날아가지 않게).
- 종료 `AppState::close_fullscreen_stage()` — **닫는 경로 전부가 지나는 유일한 지점**
  ([ADR-0063](../../adr/0063-popup-close-hook-single-choke-point.md) 과 같은 패턴). 닫힌 id 를
  훅 대기열에 넣고, draw 경로가 `on_close` 를 정확히 1 회 발화한다.
- 훅 drain 은 **무대 프레임과 일반 프레임 양쪽**에서 돈다. 무대를 나오면 다음 프레임은 일반
  프레임이라 무대 draw 경로가 아예 돌지 않기 때문이다.
- 진입·종료 뒤에는 프레임이 필요하다. IPC 경로는 라우팅이 이미 `dirty` 를 세운다. **draw 중에
  일어나는 진입/종료**(popup 버튼 클릭 · 무대 종료 버튼)는 그 프레임의 dirty 를 이미 소비한
  뒤라 `ctx.request_repaint()` 로 다음 프레임을 직접 유도한다 — 빠뜨리면 다음 입력이 올
  때까지 화면이 바뀌지 않는다.

## 레이어

무대는 등록된 `egui::Area`(`Order::Foreground`)로 그린다 — 미등록 raw layer(그 tier 안에서
항상 최상단이 되는 [함정](../../architecture/input-layer.md))를 **의도적으로 쓰지 않는다.**
무대 프레임에는 경쟁할 레이어가 없다(host chrome·popup·오버레이가 이 프레임에 그려지지
않는다). "항상 최상단" 을 얻으려고 함정에 기댈 이유가 없고, 등록해 두면 `Areas::order` 추적과
입력 라우팅이 정상 경로를 탄다.

셸은 창 전체 scrim(`theme.scrim()`, 마커 오버레이와 같은 토큰) + 제목을 그리고, 그 안쪽을
`StageDef::draw_fn` 이 채운다.

## 렌더 파이프라인 — 무대 분기의 **위치가 계약이다**

무대 분기는 `Gpu::render` 안, **offscreen surface 스크린샷 처리 뒤 · 레이아웃/렌더 패스 앞**에
있다. 이것은 "조기 반환" 이 아니라 background live-frame 과 stage frame 의 **분리**다:

1. offscreen surface 캡처 — 무대와 무관하게 항상
2. 비시각 relay(egui-mesh forward · attach mesh relay) — 무대와 무관하게 항상
3. **분기** — 무대면 clear + 무대 콘텐츠만, 아니면 기존 합성 전부
4. window 스크린샷 캡처 + `present` — **양쪽 경로 모두**

| 잘못된 위치 | 죽는 것 |
|-------------|---------|
| `MainView::render_if_dirty` 조기 반환 | attach mesh relay. 로컬 전체화면이 원격 사용자 화면을 멈춘다(주체 간 비침범 위반) |
| `Gpu::render` 최상단 | `ui.screenshot --surface <id>`(offscreen)가 영구 대기 |
| capture+present 를 건너뜀 | `ui.screenshot`(window)이 영구 대기 — 무대 검증 수단 자체가 사라진다 |
| 레이아웃/`resize_all` 뒤 | 무대 중 PTY grid 재계산 → "원본 그대로" 계약 파괴 |

무대 중에도 **`dirty` 를 억제하지 않는다.** relay 전체가 로컬 `dirty` 프레임에 종속돼 있어,
"어차피 안 보이니 프레임을 아끼자" 는 최적화가 곧 원격 구독자 굶김이다. 이 네 제약은
`crates/tasty-doc-guards/tests/fullscreen_stage_render_gate.rs` 가 구조 가드로 고정한다.

**PTY drain 은 계속 돈다.** drain 은 `AppEvent::TerminalOutput` 핸들러 몫이고 redraw 경로와
분리돼 있어 무대가 건드리지 않는다 — 무대 중에도 스크롤백이 정상적으로 쌓인다.

## OS 창 전환

무대의 경계는 작업영역이 아니라 **OS 창까지**다 — 무대가 서면 창 자체가 모니터를 덮는다.
브라우저 Fullscreen API 와 같은 모델이다: `requestFullscreen()` 은 새 창을 만들지 않고 **같은
창**을 OS fullscreen 으로 전환한 뒤 크롬 UI 를 숨긴다. 무대도 새 `View`(별개 OS 창)를 만들지
않고 그 `MainView` 의 winit 창을 전환한다. 구현은 `src/view/main/fullscreen_window.rs`.

**리컨실러다, 상태 머신이 아니다.** 무대 상태의 단일 수렴점(`open_fullscreen_stage` /
`close_fullscreen_stage`)은 `AppState` 위에 있고 `AppState` 는 headless 빌드에도 있어 winit
핸들을 들고 있지 않다. 그래서 전환 호출을 그 두 함수에 박는 대신 `MainView` 가 매 프레임
`fullscreen_stage_active()` 를 창에 반영한다 — WebView 노출을 `has_egui_overlay_open()` 에
맞추는 `sync_webviews` 와 같은 관례다. 여닫는 경로가 몇 개로 늘어나도 각 경로가 전환을 기억할
필요가 없고, 상태와 창이 어긋나면 다음 프레임에 수렴한다. 호출 위치는 `handle_redraw` 의
**render 뒤**다 — 무대는 자기 draw 안에서 닫힐 수 있어(`StageAction::Close`), 앞에 두면 그
프레임에 닫힌 무대의 창 복원이 다음 프레임으로 밀린다.

**`Borderless` 를 쓴다.** `Exclusive(VideoMode)` 는 모니터 해상도 자체를 바꾸는 게임용 모드라
다른 창들의 배치를 흐트러뜨리고 복귀 시 원래 배치가 돌아오지 않는다. 터미널은 해상도를 바꿀
이유가 없다.

**모니터는 `current_monitor()` 로 명시 지정한다** — 창이 있는 그 모니터를 덮는다. `Borderless(None)`
의 "현재 모니터" 판정은 winit 이 플랫폼 백엔드에 위임하므로 DE/컴포지터마다 해석이 다를 수
있다. `current_monitor()` 가 `None` 을 돌려주면 그 값이 그대로 `Borderless(None)` = 백엔드 판정
폴백이 되므로 별도 분기가 없다.

### 창 상태 복원

진입 **직전**의 창 상태(`was_fullscreen` / `was_maximized`)를 기록해 두고 종료 시 그대로 되돌린다.
그 기록의 존재 자체가 "이 fullscreen 은 무대가 만든 것" 의 마커를 겸한다.

| 진입 시점 창 | 무대 중 | 종료 후 |
|---|---|---|
| 일반 창 | fullscreen | 일반 창(진입 전 크기·위치) |
| maximize | fullscreen | maximize |
| **이미 OS fullscreen** | 그대로(재설정하지 않음) | **fullscreen 유지** |

셋째 줄이 핵심이다. macOS 신호등의 풀스크린 버튼처럼 **사용자가 직접 만든** 창 상태를 무대가
해제하면 "무대를 한 번 열었다 닫았더니 내가 만든 전체화면이 풀렸다" 가 된다. 무대는 자기가
만든 전환만 되돌린다. 같은 이유로 이미 fullscreen 인 창에는 `set_fullscreen` 을 다시 걸지도
않는다 — macOS 는 fullscreen 전환이 별도 Space 이동 애니메이션이라 중복 호출이 눈에 보이는
깜빡임이 된다.

### 리사이즈 잠금은 maximize 만이 아니다

리사이즈 엣지 hit-test(`view/main/mouse.rs`)는 `is_maximized()` 가 아니라 "maximize **또는** OS
fullscreen" 을 본다(`fullscreen_window::window_size_is_locked`). 무대 중에는 입력 게이트가
막아주지만 **무대 없이 fullscreen 인 창**(macOS 신호등, 또는 WM 단축키)이 가능하므로 게이트에
기대지 않는다.

### grid 는 두 전환 모두에서 불변이다

fullscreen 전환은 창 크기를 바꾸므로 `WindowEvent::Resized` 를 두 번 발생시킨다(진입 시 커짐,
종료 시 작아짐). 위 "무대 중 동결되는 것" 의 grid 동결이 **이 전환에 대해서도** 성립해야
한다 — 안 그러면 무대 진입만으로 뒤 터미널이 전부 리플로우된다. 종료 쪽은 리컨실러가 무대
상태를 이미 지운 뒤에 창을 되돌리므로 그 `Resized` 는 정상 경로로 흘러 진입 전 크기 = 진입 전
grid 로 돌아온다.

### 발화 정책

`set_fullscreen` 은 사용자가 보는 창을 바꾸므로 `docs/identity.md` 원칙 1 의 사용자 상태다.
release IPC/CLI 에 창 전환 API 를 노출하지 않는다 — 전환은 무대 상태를 따라갈 뿐이고, 무대
진입 경로(사용자 버튼 / debug 전용 `debug.fullscreen.open`)만이 그 전환을 부른다. 터미널 이스케이프로 창을 조작하는
경로도 없다([ADR-0011](../../adr/0011-xtwinops-window-ops-unsupported.md) 의 거부 결정 그대로).

### 플랫폼별 확인 결과

| 플랫폼 | 상태 | 내용 |
|---|---|---|
| **Linux / X11 / GNOME** | 확인(1회 수동) | 진입·종료·maximize 복원·사용자 fullscreen 유지·창 2 개 독립성 전부 실측. 전환 시 검은 프레임·잔상 없음. 무대 없이 fullscreen 인 창에서 CSD 타이틀바/캡션 버튼이 정상 렌더. 진입 경로가 `debug.fullscreen.*` 로 상시화되어 같은 실측을 언제든 재현할 수 있으나, 살아 있는 GUI 가 필요해 자동 회귀 커버리지는 여전히 없다 |
| **Linux / Wayland** | **미확인** | 검증 환경이 X11 세션이라 컴포지터 차이를 재현할 수 없다 |
| **Windows** | **미확인** | undecorated + `undecorated_shadow` 조합의 섀도/보더 잔상, 작업 표시줄 가림 여부. 해당 OS 없음 |
| **macOS** | **미확인** | 신호등 풀스크린과의 상태 어긋남, 별도 Space 이동 애니메이션, fullscreen 창 위 모달 z-order. 해당 OS 없음. 코드는 `fullscreen()` 조회 분기로 대응해 두었고 위 표의 셋째 줄이 그 계약이다 |
| **멀티 모니터** | **미확인** | 단일 모니터 환경이라 "창이 있는 모니터를 덮는가"·"두 모니터를 동시에 덮는가" 를 재현할 수 없다. 판정 수단으로 `debug.fullscreen.state` 가 덮고 있는 모니터의 이름·위치·크기·배율을 실어 돌려준다 — 멀티 모니터 환경 사용자가 그 출력만 보내주면 판정할 수 있다 |
| **DPI 가 다른 모니터로 전환** | **미확인** | 위와 같은 이유. `resync_scale_factor` → `update_grid_size` 경로는 무대 중 보류하도록 이미 배선돼 있다(위 "무대 중 동결되는 것") |

**무대 중 사용자가 창 fullscreen 을 직접 해제하면**(macOS 신호등 등) 무대는 그대로 남고 창만
작아진다. 무대 rect 는 창 크기를 추종하므로 작아진 창을 그대로 채운다 — 의도한 동작이다.
무대는 "창을 덮는 표면" 이지 "창을 fullscreen 으로 붙잡아 두는 잠금" 이 아니다.

## 무대 중 동결되는 것 / 동결되지 않는 것

| 대상 | 무대 중 |
|------|---------|
| PTY grid(cols/rows) | **동결** — `handle_redraw` 의 `resize_all_terminals` 스킵. 창 크기가 바뀌어도 진입 시점 값 유지 |
| 신규 터미널 기본 grid | DPI 변경 시 갱신을 **보류**했다가 무대를 나온 첫 프레임에 1 회 적용 |
| swapchain 크기 | 따라간다(창을 안 따라가면 렌더가 깨진다) |
| PTY 출력 처리 | 계속 |
| attach mesh relay | 계속 |
| offscreen/window 스크린샷 | 계속 |
| WebView | **숨김** — 아래 |

### WebView 는 "스킵" 이 아니라 "숨김"

WebView 는 OS 네이티브 자식 뷰(macOS `WKWebView` / Windows WebView2 / Linux WebKitGTK)이고
wgpu 렌더 표면 **위**에 있다. 그리지 않아도 화면에 남으므로 반드시 `set_visible(false)` 가
필요하다. 무대는 `AppState::has_egui_overlay_open()` 에 참여하고, `MainView::sync_webviews` 가
그 값으로 reveal 을 결정한다 — popup 이 열렸을 때와 **같은 게이트**를 그대로 쓴다.

## 입력 계약

무대는 입력 계층의 **새 최상위 단**이다 — [input-layer.md](../../architecture/input-layer.md)
7 단 표의 1 단(모달/오버레이)보다 위. popup 과 달리 **좌표 hit-test 없이** 무조건 차단한다:
화면 전체를 덮으므로 "무대 위인가" 를 물을 이유가 없고, 뒤 위젯은 그려지지도 않은 상태라
그 좌표로 판정하는 것 자체가 유령 입력이다.

게이트는 방향이 다른 두 가지가 짝을 이룬다 — 하나만으로는 무대가 차단막이거나 유령이다.

| 방향 | 지점 | 하는 일 |
|------|------|---------|
| **무대로 준다** | `MainView::handle_event` 의 egui feed 게이트 | 무대 중 키/IME 를 egui 입력 시스템에 넣는다(오버레이와 같은 취급). 마우스는 원래 항상 egui 로 먼저 간다 |
| **뒤로는 안 준다** | `keyboard.rs` 0단계 게이트 · `mouse_overlay_open()` | 무대 콘텐츠가 소비하지 않은 잔여 입력이 뒤 세계로 새지 않게 막는다 |

### 키보드 — 0단계

`handle_keyboard_input` 의 **맨 앞**(double-tap 1~3단계보다 앞)에 무대 게이트가 있다.
4단계 앞이 아니라 맨 앞인 이유: 무대는 뒤 세계와 로직상 무관하므로 뒤 세계의 어떤
단축키도(double-tap 포함) 무대 중에 발화하면 안 된다.

- **종료 키(기본 ESC)는 무대만 닫는다.** 4단계(`try_consume_escape_key`, settings 모달·
  notifications 팝업 닫기)에 **도달하지 않는다** — 사용자 확정 계약이자 브라우저 Fullscreen
  API 와 같은 동작(UA 가 ESC 를 처리하고 페이지 `keydown` 으로 전파하지 않는다).
- 그 외 키는 전부 무대가 소비한다 — 단축키(6단계)·vi copy-mode(7단계)·터미널
  forward(9단계) 어디로도 내려가지 않는다.
- `record_typing` 도 돌지 않는다. 뒤 surface 에 타이핑이 기록되면 `tasty is-typing`
  판정이 오염된다.
- 종료 키 판정은 `stage_exit_key_matches` **한 곳**에만 있고, 값은
  `KeybindingSettings.fullscreen_stage_exit`(4 프리셋 공통 기본값 `escape`)에서 읽는다 —
  코드에 키 리터럴이 없다(CLAUDE.md 단축키 정책).
- **그 조회는 무대 게이트 안에만 있다.** `stage_key_decision` 이 무대 비활성이면 판정 전에
  `PassThrough` 로 빠지므로, modifier 없는 기본값 ESC 가 평상시 ESC 동작(settings 모달·
  notifications 팝업 닫기, 터미널 `\x1b` 전달)을 훔칠 경로가 구조적으로 없다. 조회를 게이트
  밖으로 옮기면 그 순간 회귀가 되며, `crates/tasty-doc-guards/tests/fullscreen_stage_input_gate.rs` 가 위치를 고정한다.
- 바인딩이 **빈 vec** 이면 항상 불일치라 모든 키가 `ConsumeForStage` 다 — 키보드 종료 수단만
  사라지고 다른 키가 뒤로 새지는 않는다. 탈출은 셸이 항상 그리는 종료 버튼이 맡는다.
- double-tap 검출기에는 press/release 를 계속 먹인다(물리 상태를 놓치면 무대를 나온 뒤
  판정이 어긋난다). 완성된 결과만 무대 게이트가 버려 유령 발화를 막는다.

### 마우스 — `mouse_overlay_open()`

세 핸들러(`handle_cursor_moved` · `handle_mouse_input` · `handle_mouse_wheel`)와
click-to-activate press 가드, OS 가장자리 리사이즈 양보, 링크 hover 계산이 **같은 판정**을
본다. 한 지점만 빠져도 그 경로로 입력이 새는 것은 modifier-hint 오버레이가 이미 겪은
문제다(4 지점 배선 기록, input-layer.md).

커서 아이콘은 무대 프레임의 egui `platform_output` 이 정한다. `winit_cursor_icon_at` 은
무대 중 조기 반환해 뒤의 ↔/I-beam 이 그것을 덮지 않게 한다.

### IME

진입 시 진행 중인 preedit 은 **버린다**(`clear_ime_preedit`, `flush` 아님) — 오버레이가
열릴 때의 기존 관례. 무대를 띄우는 조작으로 조합 중이던 문자가 뒤 PTY 에 흘러 들어가면
안 된다. `update_ime_cursor_area` 도 무대 중 조기 반환한다(보이지 않는 뒤 surface 의 셀
좌표로 후보창을 잡을 이유가 없다).

### 진입 시 정리 — 확정하지 않고 폐기한다

`MainView::sync_fullscreen_stage_transition`(`handle_redraw` 최상단)이 무대 활성 여부의
**상승 엣지**를 잡아 정리한다. 진입 API 는 `&mut AppState` 만 갖고 있어 뷰 상태에 손댈 수
없고, 진입 경로가 단축키든 IPC 든 프레임은 반드시 돌기 때문에 엣지 검출이 모든 진입
경로의 유일한 공통 수렴점이다.

| 대상 | 처리 | 이유 |
|------|------|------|
| IME preedit | 버림 | 조합 문자가 뒤 PTY 로 새면 안 됨 |
| divider 드래그 · 좌클릭 선택 게이트 · popup 이동/리사이즈 | 폐기 | 무대가 마우스 경로를 끊어 짝이 되는 release 를 영영 못 받는다 → sticky |
| hovered_link · pending_resize_cursor | 비움 | 뒤 좌표 기반 잔재. 무대 중 갱신도 안 된다 |
| 네이티브 컨텍스트 메뉴 | dismiss + 요청 폐기 | 아래 |
| 네이티브 파일 드래그 요청 | 폐기 | 아래 |
| **텍스트 선택 범위** | **그대로** | 진행 중 제스처가 아니라 진입 전에 확정된 사용자 상태 |
| **vi copy-mode** | **그대로** | 키보드 모드라 입력 차단으로 자연히 멈추고, 나오면 있던 그대로 이어진다 |

폐기하되 **release 로 확정하지 않는** 이유는 반대편이다 — 사용자가 놓은 적 없는 위치로
좌표/크기를 확정하면 무대를 나왔을 때 레이아웃이 임의로 바뀐 것처럼 보인다.

#### 정리하지 않는 것 — 알려진 경계 둘

**1. 마우스 트래킹 앱은 release 보고를 못 받는다.** 트래킹 모드(1002/1003) 앱 위에서 버튼을
누른 채 무대에 진입하면, 그 앱은 press 만 받고 release 를 영영 못 받는다 — 보고 경로가
`mouse_overlay_open()` 뒤에 있기 때문이다. **호스트 쪽 상태는 안전하다**: 물리적 release 의
`pop_report_button` 은 `handle_mouse_input` **맨 앞**(모든 게이트보다 앞)에 있어 무대 중에도
`report_buttons_down` 이 정상적으로 비고, 무대를 나온 뒤 hover 가 드래그로 오인되지 않는다.
남는 것은 앱 쪽 인식뿐이다.

무대 고유 문제가 아니다 — settings 모달이 드래그 중에 열려도 같은 일이 생긴다
(`mouse_overlay_open()` 의 다른 항). 합성 release 를 쏘지 않는 이유는 위 표의 "확정하지
않는다" 와 같다: 사용자가 놓은 적 없는 좌표로 release 를 보고하게 된다. 이 경계를 없애려면
게이트가 아니라 보고 계층에서 "press 를 보고한 앱에는 release 도 반드시 보고한다" 를
별도로 세워야 하고, 그건 무대와 독립된 작업이다.

**2. modifier-hint / switch-overlay 홀드는 무대 중에도 갱신된다.** `ModifiersChanged` 는
`MainView::handle_event` 의 자체 분기에서 처리되어 0단계 키보드 게이트를 지나지 않는다 —
무대 중에도 `update_switch_overlay` / `modifier_hint.update_hold` 가 계속 돈다. 화면에는
영향이 없다(렌더 게이트가 host chrome 을 통째로 뺀다). 실제로 보이는 결과는 하나뿐이다:
modifier 를 누른 채 무대를 나오면 500ms 홀드가 이미 채워져 있어 hint 가 즉시 뜬다. 값은
다음 `ModifiersChanged` 가 그대로 덮으므로 상태가 어긋난 채 남지는 않는다. modifier 상태는
"진행 중 제스처" 가 아니라 물리 키의 현재 값이라, double-tap 검출기에 press/release 를 계속
먹이는 것과 같은 이유로 **끊지 않는 쪽이 맞다** — 끊으면 무대를 나온 순간 물리 상태와
어긋난다.

### OS 레벨 UI 는 입력 게이트가 막아주지 않는다

OS 가 직접 띄우는 UI 는 wgpu 표면 **위**에 있어 무대가 덮지 못하고, 입력 라우팅과 무관한
경로로 발동한다. 넷을 각각 처리한다.

| 대상 | 처리 | 근거 |
|------|------|------|
| **OS 인터랙티브 스크린샷**(`screenshot_to_clipboard`) | **차단** — 등록 단축키라 0단계 게이트가 자동으로 막는다 | 이 단축키는 OS 네이티브 영역 선택 UI 를 띄우고 **사용자가 선택을 마칠 때까지 앱을 블록**한다. 무대 위에 뜨는 데다 그 UI 의 ESC 가 무대 종료 ESC 와 겹쳐 "무엇을 취소하는 ESC 인가" 가 사용자에게 모호해진다. 무대를 찍고 싶으면 `tasty screenshot`(`ui.screenshot`, swapchain 캡처)이 무대를 그대로 담는다 — 대체 수단이 있고 그쪽이 무대 검증의 정규 경로다 |
| **네이티브 컨텍스트 메뉴** | 진입 시 dismiss, 무대 중 신규 요청 폐기 | 요청을 슬롯에 남기면 무대를 나온 뒤 사용자가 우클릭한 적 없는 자리에 메뉴가 뜬다. **폴링(`poll_pending_native_menu`)은 계속 돈다** — dismiss 의 결과 회수가 그 경로라 끊으면 슬롯이 영원히 차 있게 된다 |
| **네이티브 파일 드래그** | 무대 중 `start_file_drag` 억제 + 요청 폐기 | 드래그는 마우스를 누른 채 시작하는 제스처인데 무대가 그 경로를 이미 끊었다. 큐에 남겼다 나중에 시작하면 아무도 누르고 있지 않은 드래그가 뜬다 |
| **별도 OS 모달 창**(Settings/Plugins/Quit) | **공존** — 무대 진입을 거절하지 않는다 | 아래 |

### 모달과 무대는 공존한다

모달이 열려 있어도 무대 진입을 거절하지 않는다. 거절이 더 안전해 보이지만 tasty 에서는
그쪽이 원칙 위반이다:

- 모달은 **전역**(`ViewRegistry::active_modal_id`)이고 사용자가 언제든 여닫는 상태다.
  에이전트가 부르는 무대 진입이 그 값에 따라 실패하면 **활성 상태 의존 동작**이 된다
  (`docs/identity.md` 불가침 원칙 3 · [focus.md](../policies/focus.md)).
- 갇히지 않는다. 모달이 활성인 동안 `MainView::handle_event` 가 메인 창 입력을 통째로
  버리므로 무대 ESC 는 안 먹지만, 모달을 닫으면 그 순간 정상 동작한다 — 회복 경로가
  사용자 손에 항상 있다.
- 거절을 구현하려면 창마다의 `AppState` 에 있는 모달 거울(`active_modal_id`)을 읽어야 하는데, 그
  거울이 어긋나면 **무대가 영영 안 열리는** 훨씬 나쁜 실패로 나타난다.

## 아직 없는 것 — 무대 위에 얹을 때 전제할 경계

- **무대 프레임에는 CSD 타이틀바도 없다.** 무대 프레임은 host chrome 을 통째로 건너뛰므로
  창 닫기·최소화 버튼까지 사라진다. 탈출 수단은 둘이다 — 마우스는 셸이 공통 제공하는 종료
  버튼, 키보드는 위 입력 계약의 종료 키(`fullscreen_stage_exit`, 기본 ESC). 키 쪽은 사용자가
  비울 수 있으므로 **마우스 종료 버튼이 최후의 탈출구**다. 셸이 그 버튼을 무조건 그리는 것이
  그래서 콘텐츠 구현자 재량이 아니며, 종료 수단을 건드리는 작업은 "창을 빠져나올 방법이
  없어지지 않는가" 를 반드시 함께 본다.

## headless

무대는 화면 투영이라 headless 에 대응 도메인이 없다(`docs/identity.md` §2.2). 상태 필드와
API 는 `#[cfg(feature = "gui")]` 안에 있고, `AppState::fullscreen_stage_active()` 는
`#[cfg(any(feature = "gui", debug_assertions))]` 라 debug headless 에도 있고 거기서는 항상 `false` 를 돌려준다(release headless 에는 없다).

## 기능 명세 — 상태 · 인터페이스 · Acceptance Criteria

- **Status**: Partial — 무대 코어(상태 · 정의 테이블 · 렌더 파이프라인 게이트) + OS 창 전체화면 전환 + 첫 콘텐츠(알림 무대) + 사용자 진입 경로(popup 타이틀바 전체화면 버튼) + 셸 종료 버튼 + 입력 라우팅 게이트 + 설정 가능한 종료 단축키(`fullscreen_stage_exit`, 기본 ESC) + 에이전트 진입/조회(debug 전용 IPC/CLI)까지 있다. release 표면은 없다.
- **주체**: 로컬 사용자
- **ADR**: [ADR-0082](../../adr/0082-fullscreen-independent-stage.md)
- **코드**: `src/adapters/ui/fullscreen.rs` · `src/adapters/ui/fullscreen/defs.rs` · `src/adapters/ui/fullscreen/notifications.rs` · `src/adapters/ui/popup/draw.rs`(진입 버튼) · `src/state.rs` · `src/gfx/gpu.rs` · `src/view/main/redraw.rs` · `src/view/main/keyboard.rs` · `src/view/main/mouse.rs` · `src/view/main/fullscreen_window.rs`(OS 창 전환 + 상태 덤프) · `src/app/ipc/debug_methods.rs`(debug IPC) · `crates/tasty-settings/src/keybindings.rs`(종료 바인딩)
- **화면**: 없음 — 무대 자체가 화면이다. 동작 모델은 이 문서 위 절들

### 목적

무언가를 창 전체로 크게 보여주기 위한 기반. tasty 에는 이 개념이 아예 없었다(winit
`set_fullscreen` 호출 0 건, View 안에서 요소가 작업영역을 독점하는 상태도 없음). 무대는 기존
레이아웃을 확대하는 대신 **창 전체를 쓰는 독립 표면**을 띄우고 뒤는 손대지 않는다 — 그래서
화면 rect 를 계산하는 기존 경로를 하나도 고치지 않는다(근거는 ADR-0082).

### 내부 동작

- **상태**: `AppState.fullscreen_stage: Option<StageState>` — 창(= `MainView`)당 최대 하나.
  창이 여럿이면 창마다 독립적으로 가질 수 있다. 영속화하지 않는다.
- **등록**: 무대에 올릴 수 있는 것은 `fullscreen::defs::all_defs()` 의 `StageDef` 뿐이다
  (`id` · `title_key` · `draw_fn` · `on_close`). 선언되지 않은 id 로 여는 시도는 거부된다.
  현재 등록된 무대: `blank`(셸 확인용 기준 무대) · `notifications`(알림 무대).
- **셸 chrome**: scrim + 상단 제목 + **종료 버튼**. 종료 버튼은 콘텐츠가 아니라 셸이 그린다 —
  무대 프레임에는 창 닫기 버튼(CSD 타이틀바)조차 없어, 콘텐츠 구현자가 빠뜨리면 창을 빠져나갈
  수 없는 상태가 되기 때문이다. 콘텐츠는 제목 띠 아래로 잘린 child `Ui` 만 받는다.
- **알림 무대**: popup 의 형상 함수(`notification::draw_notification_content_inner`)를 그대로
  호출한다 — popup 인스턴스가 아니라 같은 형상의 **별개 콘텐츠**이고, 목록 스크롤 위치는 무대
  자신의 것이라 popup 쪽과 섞이지 않는다(무대 종료 시 `on_close` 가 지운다).
- **진입/종료**: `AppState::open_fullscreen_stage(id)` / `close_fullscreen_stage()`. 다른 무대가
  올라와 있으면 그 무대를 닫고 교체하며, 같은 id 재진입은 no-op 이다. 종료는 **모든 닫힘
  경로가 지나는 유일한 지점**이고, 닫힌 id 가 훅 대기열을 거쳐 `on_close` 를 정확히 1 회
  발화시킨다.
- **렌더**: 무대가 켜져 있으면 프레임이 통째로 갈린다 — clear + 무대 콘텐츠만 그리고, 터미널
  글리프 · egui-mesh · attach mesh 합성 · host chrome(사이드바/탭바/상태바/popup/오버레이)은
  그리지 않는다.
- **무대 중에도 계속 도는 것**: PTY 출력 처리(스크롤백이 계속 쌓인다) · attach mesh relay ·
  offscreen surface 스크린샷 · window 스크린샷.
- **무대 중 동결되는 것**: 터미널 grid(cols/rows). 창 크기가 바뀌어도 진입 시점 값을 유지한다
  — 그래야 나올 때 원본이 리플로우되지 않는다. DPI 변경에 따른 신규 터미널 기본 grid 갱신은
  보류했다가 무대를 나온 첫 프레임에 1 회 적용한다.
- **WebView**: 네이티브 자식 뷰라 "안 그린다" 로는 사라지지 않는다. 무대는
  `has_egui_overlay_open()` 에 참여해 popup 과 같은 게이트로 `set_visible(false)` 를 받는다.
- **입력**: 무대는 입력 계층의 새 최상위 단이다 — 활성이면 뒤 세계의 키보드/마우스가 전부
  막힌다(좌표 판정 없이). ESC 는 무대만 닫고 **뒤로 전파되지 않는다**(settings 모달·
  notifications 팝업이 함께 열려 있어도 그것들은 닫히지 않는다). 무대 콘텐츠는 egui 위젯이라
  키/IME/클릭을 정상적으로 받는다. 진입 시 진행 중이던 IME 조합·드래그·네이티브 메뉴·파일
  드래그는 확정하지 않고 폐기한다. 계약 전체는
  위 [§ 입력 계약](#입력-계약).
- **종료 키**: `KeybindingSettings.fullscreen_stage_exit`(4 프리셋 공통 기본값 `escape`). 조회는
  0단계 무대 게이트 안에서만 일어나므로 무대가 없으면 이 바인딩은 아예 매칭되지 않는다 —
  평상시 ESC 동작을 가져가지 않는 것이 이 위치의 목적이다. 사용자가 다른 키로 바꾸면 그 키로
  닫히고 ESC 로는 닫히지 않는다. 빈 값이면 키보드 종료 수단만 사라지고 셸의 종료 버튼이
  남는다(셸이 항상 그리므로 탈출 불가 상태가 되지 않는다). 프리셋 표는
  [`features/keybindings/index.md`](../../features/keybindings/index.md#프리셋-바인딩--네-프리셋의-기본값).

### 인터페이스

- **AI Agent (IPC/CLI)**: **debug 빌드 전용**. 무대는 화면 투영이고 진입은 사용자 조작이라
  release 표면을 두지 않는다(불가침 원칙 1) — 대신 자기검증용으로
  `debug.fullscreen.{list,open,close,state}` 4 종이 `#[cfg(debug_assertions)]` 로 격리되어 있다
  (CLI: `tasty debug fullscreen {list,open,close,state}`). release 바이너리에서는 아예 없어
  `method_not_found` 로 떨어진다. 전부 `local_only` 라 plugin caller 는 호출할 수 없다.
  대상 창은 `window_id` 로 지목하고, 미지정 시 창이 하나면 폴백·여럿이면 에러다(포커스된 창으로
  조용히 폴백하지 않는다). 파라미터·응답 상세는
  [`dev-guide/debug-ipc.md`](../../dev-guide/debug-ipc.md).
- **사용자 트리거**: 무대를 선언한 popup(`PopupDef.fullscreen_stage`)의 타이틀바 전체화면
  버튼 — 현재 알림 popup. 누르면 무대가 뜨고 **원본 popup 은 열린 채 남는다**. 종료는 무대
  우측 상단의 종료 버튼 또는 `fullscreen_stage_exit` 단축키(기본 ESC). 진입 단축키는 아직
  없다 — 무대를 여는 것은 popup 타이틀바 버튼뿐이다.
- **설정 UI**: 설정 > 단축키 > 일반에 "전체화면 무대 종료" 엔트리로 노출된다. 발견성 때문에
  노출한다 — 무대에는 메뉴도 CSD 타이틀바도 없어 종료 버튼 외에는 이 엔트리가 종료 수단을
  알 수 있는 유일한 자리다. 다만 녹화 버튼에서 ESC 는 "슬롯 비우기" 로 예약돼 있어 ESC 를
  다시 지정할 수는 없다(엔트리 툴팁에 명시).

### 남은 미구현

이 기능 자체에 남은 미구현 항목은 없다 — 종료 키의 설정화와 에이전트(debug IPC) 진입이
모두 붙었다. release 에이전트 표면을 두지 않는 것은 미구현이 아니라 확정된 경계다(무대는
화면 투영이라 대응 도메인이 없다).

다만 무대 **위에 무언가를 얹을 때** 전제해야 하는 경계(무대 프레임에는 CSD 타이틀바조차
없다 등)는 그대로다 — 위
["아직 없는 것"](#아직-없는-것--무대-위에-얹을-때-전제할-경계) 절.

### OS 창 전환 — 요약

무대가 서면 **창 자체가 모니터를 덮는다** — 무대의 경계는 작업영역이 아니라 OS 창까지다
(브라우저 Fullscreen API 와 같은 모델: 새 창을 만들지 않고 같은 창을 전환한다). 창이 있는
그 모니터를 덮으며 primary 로 튀지 않는다. 종료하면 진입 직전 창 상태로 되돌아간다 —
maximize 였으면 maximize 로, **사용자가 직접 만든 전체화면이었으면 그대로 유지한다**(무대는
자기가 만든 전환만 되돌린다). 뒤 터미널의 grid 는 두 전환 모두에서 불변이다.

동작 모델·플랫폼별 확인 결과(Wayland/Windows/macOS/멀티 모니터는 **미확인**)는
위 [§OS 창 전환](#os-창-전환).

### 비-목표 (Out of scope)

- 기존 요소(pane/tab/surface)를 뷰포트 크기로 리레이아웃하는 것 — 무대는 확대가 아니다.
- 무대 콘텐츠와 원본의 자동 동기화 — 무대에는 별개 데이터가 들어간다.
- 영속화 — 재시작이 전체화면 상태로 부팅되지 않는다.
- headless 대응 — 화면 투영이라 대응 도메인이 없다.

### Acceptance Criteria

단위 테스트로 상시 검증되는 것:

- Given 정의 테이블에 없는 id When 무대 진입 Then 거부되고 무대가 서지 않는다
- Given 무대 활성 When `has_egui_overlay_open()` 조회 Then true (WebView 숨김 게이트)
- Given 무대 종료 When 다음 프레임 Then 닫힘 훅이 정확히 1 회 발화한다(무대/일반 프레임 양쪽)
- Given 무대 A 활성 When 무대 B 진입 Then A 의 닫힘 훅이 1 회 발화하고 B 만 남는다
- Given 진입 시점 창 상태 When 종료 시 복원 동작 결정 Then 일반/maximize 는 되돌리고 사용자 fullscreen 은 유지한다
- Given fullscreen 단독 When 리사이즈 엣지 판정 Then 잠긴 것으로 본다(maximize 만 보지 않는다)
- Given 무대를 선언한 popup When 타이틀바 전체화면 버튼 클릭 Then 그 무대가 뜨고 popup 은
      열린 채 남는다
- Given 무대를 선언하지 않은 popup When 같은 좌표 클릭 Then 아무 무대도 뜨지 않는다
- Given 알림 무대 활성 When 무대 종료 Then 무대 자체 상태(목록 스크롤)가 지워진다
- Given popup 이 선언한 무대 id When 무대 테이블 조회 Then 반드시 존재하고, 그 popup 은
      headless 가 아니다

**미검증 — 살아 있는 GUI 가 필요해 자동 회귀가 없다.** 아래는 실제 창·PTY·GPU 가 있어야
재현되므로 headless 테스트로 고정할 수 없다. 다만 `debug.fullscreen.*` 로 진입/조회가
상시화되어 실측 재현 자체는 언제든 가능하며, 표시된 항목은 실측으로 1 회 확인했다(회귀를
잡지는 못한다).

- Given 무대 활성 When PTY 로 출력 발생 Then 무대를 나온 뒤 그 출력이 스크롤백에 있다
- Given 무대 진입 전후 When 터미널 grid 조회 Then cols/rows 가 동일하다(무대 중 창
      크기가 바뀌어도)
- Given 무대 활성 When `ui.screenshot`(window) Then 응답하고 결과에 무대만 찍힌다
      <!-- 실측 확인: debug.fullscreen.open 후 window 캡처에 무대 셸(제목 띠 + 종료 버튼)과
           콘텐츠만 있고 사이드바/탭바/상태바가 없음 -->
- Given 무대 활성 When `ui.screenshot --surface <id>` Then 응답하고 그 surface 의 터미널
      내용이 찍힌다
- Given 일반 창에서 무대 진입 When 무대 종료 Then 진입 전 크기·위치의 일반 창으로 복귀
      <!-- 실측 확인: debug.fullscreen.state 의 inner_size 가 1280x720 → 1920x1080 →
           1280x720, os_fullscreen 이 false → true → false -->
- Given maximize 창에서 무대 진입 When 무대 종료 Then maximize 로 복귀
- Given 이미 OS fullscreen 인 창에서 무대 진입 When 무대 종료 Then fullscreen 유지
- Given 창 2 개가 각각 무대 활성 When 한쪽만 종료 Then 다른 쪽 전체화면 유지
      <!-- 실측 확인: 창 A=blank / B=notifications 로 각각 진입 후 A 만 close →
           A 는 stage_id null·os_fullscreen false, B 는 notifications·true 유지 -->
- Given 무대 활성 + notifications 팝업 열림 When ESC Then 무대만 닫히고 팝업은 남는다
      <!-- 실측 확인: X11 + xdotool 로 winit 키 경로 재현 — ESC 후 창이 무대 크기에서
           원래 크기로 돌아오고 `ui.state` 의 notification_panel_open 은 true 유지 -->
- Given `fullscreen_stage_exit` 를 다른 키로 변경 When 무대 활성 + ESC Then 닫히지 않고,
      그 새 키를 주입하면 닫힌다
      <!-- 실측 확인: debug.settings.apply 로 ctrl+alt+q 재바인딩 후 ESC 는 무대 유지,
           ctrl+alt+q 로 종료. 빈 vec 이면 둘 다 무효고 셸 종료 버튼으로만 닫힘 -->
- Given 무대 활성 When 문자 키 주입 Then 뒤 터미널에 그 문자가 들어가지 않는다
- Given 무대 활성 When 등록 단축키(사이드바 토글 등) 주입 Then 발화하지 않는다
- Given 무대 활성 When 뒤 surface 좌표 클릭 주입 Then 포커스가 이동하지 않는다
- Given 무대 활성 When `debug.pending_menu` 조회 Then 대기 중 네이티브 메뉴가 없다

**수동 확인 대상** — IPC 로 재현할 수 없다:

- IME 조합 중 무대 진입 시 조합 문자가 PTY 로 새지 않음(실제 IME 입력 필요)
- OS 인터랙티브 스크린샷 선택 UI 가 무대 중 뜨지 않음(OS 레벨 UI)
- 네이티브 파일 드래그가 무대 중 시작되지 않음(OS 레벨 드래그 세션)
