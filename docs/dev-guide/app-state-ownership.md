# AppState 필드 소유권

`AppState`(`src/state.rs`)는 창 하나의 상태다. 한 struct 안에 성질이 다른 세 가지가 함께
산다. 이 문서는 필드마다 그중 무엇인지, 얼마나 사는지, 누가 세우고 누가 비우는지, 그리고
headless 빌드에 그 필드가 있는지를 적는다.

- **도메인 사실** — 에이전트가 IPC 로 묻고 바꾸는 구조의 일부. 창이 없어도 뜻이 있다.
- **사용자 view 상태** — 로컬 사용자가 지금 무엇을 보고 어디를 누르는지. 포커스·선택·
  popup 입력·hover 가 여기다. 불가침 원칙 1·3(`docs/identity.md`)이 지키는 대상이다.
- **실행 자원** — 다른 소유자의 핸들 사본이나, 한 계층이 넣고 다른 계층이 비우는 큐.

소유권을 struct 두 개로 가르지 않고 **컴파일 경계와 모듈 경계**로 가른 결정과 그 근거는
[ADR-0355](../adr/0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md).
"어떤 정의를 gui 전용으로 가르는가" 의 판정 규칙 자체는 [헤드리스 정의 경계](headless-build-boundaries.md)
가 정본이고, 이 문서는 그 규칙을 `AppState` 에 적용한 결과표다.

## 열 읽는 법

- **수명**: `프레임`(매 egui 프레임 다시 채운다) · `열림`(popup·메뉴·드래그가 열려 있는 동안) ·
  `요청`(세운 쪽 다음 소비 한 번에 비워진다) · `세션`(창이 사는 동안) · `영속`(디스크에서
  로드된다).
- **headless**: `없음` 은 `cfg(feature = "gui")` 로 그 빌드에서 필드가 사라진 것이다.
  `읽힘` 은 headless 라이브러리 안에 그 필드를 읽는 자리가 있는 것이다. `③` 은 필드가
  headless 에도 컴파일되고 그 빌드가 값을 세우지만 읽는 자가 GUI 뿐이라 항목 단위
  `expect(dead_code)` 를 단 것이다. `②` 는 headless 라이브러리에는 없고 테스트 구성에만 있는 것이다.
  `debug 헤드리스만 읽힘` 은 게이트가 `cfg(any(feature = "gui", debug_assertions[, test]))` 라 debug
  헤드리스에서만 컴파일되고 읽히는(주로 debug `ui.state` 덤프) 것이다 — release 헤드리스에는 필드가 없다.

이 칸은 소스를 읽어 정한 값이 아니라 진단으로 잰 값이다. 재는 법은 아래 "재는 법".

## `dialogs` — GUI 소유 한 덩어리

`dialogs: DialogState` 는 필드 하나지만 그 안에 popup·메뉴·드래그의 입력 상태가 스무 개 넘게
있다. 자료형은 전부 `src/state/dialogs.rs` 한 파일에 있고, 그 모듈과 `dialogs` 필드는
`cfg(feature = "gui")` 다. **headless 빌드에는 이 상태가 아예 없다.**

전부 같은 칸에 든다: 사용자 view 상태 · 수명 `열림` 또는 `요청` · 세우는 쪽과 비우는 쪽이
모두 GUI(popup `draw_fn`·`on_close`, 사이드바·탭바, 메인 루프). 에이전트가 세우는 것처럼 보이는
자리도 비우는 쪽은 GUI 다:

| 필드 | 에이전트 쪽 입구 | 실제 도메인 사실이 사는 곳 |
|---|---|---|
| `pending_approval_ids` | `approval.request` · capability elevation 이 `enqueue_approval` 로 push | approval 레코드는 `Core` 의 approval 저장소가 갖는다. 이 큐는 그중 **popup 이 보여줄 순서**다 |
| `file_picker` | `file_picker.trigger`(그 핸들러 모듈이 gui 전용) | 결과는 plugin 에 이벤트로 나간다(ADR-0058) |
| `pending_open_preset_window` · `pending_preset_window_selection` | 사용자 origin 의 preset 저장만 세운다 | 저장된 preset 은 `PresetStore` 에 있다 |

그래서 headless 가 이 덩어리를 잃어도 도메인 사실은 하나도 안 잃는다. approval 을 묻는 IPC
(`approval.list` 류)는 저장소를 읽는다.

두 판정은 dialog 가 없어도 debug 빌드의 두 조합에 남는다(release 헤드리스에는 없다) —
`has_input_dialog_open` 은 debug 헤드리스에서 `false` 를 답하고, 그 값을 쓰는 `keyboard_overlay_open`
도 그대로다. `ui.state` debug 덤프가 두 값을 두 조합에서 같은 키로 찍기 때문이다.

## 나머지 필드

| 필드 | 분류 | 수명 | 세우는 쪽 → 비우는 쪽 | headless |
|---|---|---|---|---|
| `active_workspace` | 사용자 view 상태 | 세션 | 사용자 전환 → — | 읽힘 (대상 생략 시 기본값) |
| `category_last_active` | 사용자 view 상태 | 세션 | 사용자 전환 → — | debug 헤드리스만 읽힘(release 에는 필드 없음) |
| `settings_open_requested` · `plugins_open` | 사용자 view 상태 | 요청 | 사이드바 버튼 → 다음 프레임 `dispatch_pending_modal_opens` | 앞은 debug 헤드리스만 읽힘(`ui.state`, release 에는 필드 없음), 뒤는 없음 |
| `active_modal_id` · `active_modal_kind` | 사용자 view 상태 | 열림 | `App::open_modal` → 모달 닫힘 | debug 헤드리스만 읽힘(release 에는 필드 없음) |
| `sidebar_width` · `sidebar_visible` · `sidebar_collapsed` | 사용자 view 상태 | 세션 | 설정·사용자 토글 → — | 없음 |
| `pending_resize_cursor` · `switch_overlay` · `modifier_hint` · `tutorial` | 사용자 view 상태 | 프레임·열림 | GUI 입력 → GUI | 없음 |
| `dialogs` | 사용자 view 상태 | 열림·요청 | 위 절 | 없음 |
| `tab_bar_height` | 사용자 view 상태 | 프레임 | 탭바 실측 → — | 없음 (테스트 구성에는 있다 — ②) |
| `popups` · `toasts` · `banners` | 사용자 view 상태 | 열림 | GUI → GUI | 없음 |
| `fullscreen_stage` · `stage_closed_queue` · `stage_deferred_grid_resync` | 사용자 view 상태 | 열림 | GUI → GUI | 없음 |
| `search` | 사용자 view 상태 | 열림 | 검색 바 → 검색 바 | 없음 |
| `port_scan` · `port_favorites_scan` | 사용자 view 상태 | 열림 | port scanner popup → popup | 없음 |
| `command_palette` | 사용자 view 상태 | 열림 | 팔레트 → 팔레트 | 없음 |
| `recent_files` | 도메인 사실 | 영속 | 디스크 로드·파일 열기 → — | 읽힘 |
| `popup_hovered` · `banner_hovered` · `modifier_hint_hovered` · `resize_edge_widget_hovered` | 사용자 view 상태 | 프레임 | egui 패스 → 입력 라우팅 | 없음 |
| `plugin_popup_open` | 사용자 view 상태 | 프레임 | plugin popup 그리기 → 입력 라우팅 | debug 헤드리스만 읽힘(`ui.state`, release 에는 필드 없음) |
| `popup_layers` · `plugin_popup_layers` · `host_popup_hittest` · `popup_escape_owner` · `plugin_popup_hittest` · `banner_layer` · `modifier_hint_layer` | 사용자 view 상태 | 프레임 | egui 패스 → 입력 라우팅 | 없음 |
| `preset_store` · `memory` | 실행 자원 (Core 소유 Arc 의 사본) | 세션 | Core → — | `memory` 는 읽힘, `preset_store` 는 ③(사본을 받지만 읽는 자가 GUI 뿐 — `expect`) |
| `pending_lifecycle_events` | 실행 자원 (큐) | 요청 | close cascade → 메인 루프가 plugin 에 통지 | 읽힘 |
| `pending_host_events` | 실행 자원 (큐) | 요청 | `enqueue_host_event` → Event Bus 발화 | 읽힘 (headless drain) |
| `last_focused_surface_id` · `last_active_workspace_id` · `last_focused_tab` · `last_tab_locations` | 실행 자원 (변화 감지 기준값) | 세션 | GUI tick 의 감지 → 같은 자리 | 없음 |
| `explorer_views` · `dag_graph_views` | 사용자 view 상태 | 열림 (surface 수명) | surface 그리기 → surface 닫힘 | 없음 |
| `tool_registry` · `palette_plugin_commands` | 도메인 사실의 사본 (plugin 기여 목록) | 세션 | plugin 활성 → 재계산 | 없음 |
| `pending_plugin_command_invokes` · `pending_tool_events` · `pending_popup_opens` | 실행 자원 (큐) | 요청 | 도구 메뉴·팔레트 → 메인 루프 | 없음 |
| `pending_handler_ipc` | 실행 자원 (큐) | 요청 | 파일 핸들러 dispatch → 메인 루프 | 없음 (넣는 자리 `file::dispatch` 의 핸들러 실행도 GUI 다) |
| `drop_hover` · `pending_file_drops` | 사용자 view 상태 | 열림·요청 | OS drag&drop → frame end | 없음 |
| `plugin_popup_closes` · `plugin_popup_focus_bumps` · `plugin_banner_closes` | 실행 자원 (큐) | 요청 | egui 패스 → 메인 루프가 plugin 에 통지 | 없음 |
| `plugin_mesh_popup_regions` · `plugin_popup_ime_cursor_area` | 사용자 view 상태 | 프레임 | egui 패스 → 합성·IME | 없음 |
| `plugin_mesh_popup_forward` · `plugin_mesh_banner_forward` | 사용자 view 상태 | 열림 | egui 패스 → 합성 | 없음 |
| `plugin_mesh_banner_regions` | 사용자 view 상태 | 프레임 | egui 패스 → 합성 | 없음 |
| `plugin_mesh_popup_pending_repaint` · `plugin_mesh_banner_pending_repaint` | 사용자 view 상태 | 요청 | plugin repaint 요청 → 합성 | 없음 |
| `pending_intents` | 실행 자원 (큐) | 요청 | GUI 의 `dispatch_intent` · IPC 진입점이 옮기는 요청 출구 → `dispatch_pending_intents` / headless drain | 읽힘 |

## 모듈 단위 예외 없이 가른다

`state` 모듈에는 headless 진단을 덮는 모듈 단위 예외가 없다. 한때 `src/state.rs` 첫머리의
`cfg_attr(not(feature = "gui"), allow(dead_code, unused_imports))` 가 `state` 와 하위 모듈 전체를
덮었고, 그 아래에 headless 에 컴파일되지만 아무도 읽지 않는 필드 31 개가 있었다. 지금은
[헤드리스 정의 경계](headless-build-boundaries.md) 의 세 갈래를 항목마다 적용한다.

- **①** 필드 29 개와 그 필드만 쓰는 자료형(`PendingPopupOpen` · `DropHoverState`)은
  `cfg(feature = "gui")` 다 — headless 에서 그 필드를 세우는 것은 생성자의 초깃값뿐이었다.
  GUI 입력 경로만 부르는 하위 모듈(`detect` · `events` · `focus` · `search`)은 모듈 선언에,
  일부만 GUI 전용인 모듈(`layout` · `mouse` · `pane` · `tab` · `workspace` · `accessors`)은
  항목에 붙인다. `pending_handler_ipc` 도 ① 이다 — `file/dispatch.rs` 의 모듈 단위 예외가
  그 필드에 넣는 자리를 덮고 있어 늦게 드러났다.
- **②** headless 라이브러리에는 소비자가 없지만 **headless 테스트가 실제로 부르는** 정의
  (탭·pane·워크스페이스 조작 메서드, `layout` 모듈, `tab_bar_height`)는
  `cfg(any(feature = "gui", test))` 다. 그 시험들은 base 에서도 headless 구성에서 돌았고 지금도 돈다.
- **③** `preset_store` 와 `ModalKind` 의 variant 는 `expect(dead_code)` 다 — 앞은 headless 도
  `new` 로 사본을 받지만 읽는 자가 GUI 뿐이고, 뒤는 모달을 여는 자리가 GUI 뿐이라 headless 에서
  variant 가 만들어지지 않는다(열거와 `active_modal_kind` 는 `ui.state` 덤프가 debug 빌드의 두
  조합에서 읽는다 — release 헤드리스에는 `active_modal_kind` 필드도 덤프도 없다).
  `CoreState` 에도 같은 가름을 쓴다 — `readonly_views`(점유 surface 의 readonly mirror, gui 의
  render_pass 와 attach 폴링(`refresh_readonly_views`)만 읽는다)와 `input_simulation_enabled`(debug
  빌드에만 있는 필드, 읽는 자가 gui 의 입력 시뮬레이션 IPC 뿐)가 headless 에서 `expect(dead_code)` 다.

`state` 아래에는 모듈 단위 예외가 없다. 마지막이던 `state/command_palette.rs` 는 모듈 선언이
②(매칭 로직을 headless 시험이 부른다)이고 그 안의 `CommandPaletteState` 가 ①(필드가 gui 전용)이다.

## `state` 가 아니라 `core` 에 두는 것

`AppState` 필드를 읽지 않는 공용 동작은 `core` 에 둔다. `core` 가 부르는 동작이 `state` 에
있으면 도메인 계층이 view 상태 모듈을 거꾸로 보게 된다.

| 정의 | 자리 | 하는 일 |
|---|---|---|
| `SurfaceMessage` | `src/core/state/message.rs` | `CoreState` 의 surface 간 메시지 큐 항목 |
| `default_tab_name_for_kind` | `src/core/surface_registry.rs` | surface kind 선언과 params 로 탭 표시명을 도출 |
| `collect_close_targets` | `src/core/impl_close.rs` | 닫히는 탭의 surface 와 scrollback persist id 를 모은다 |
| `PendingHostEvent` · `PendingSurfaceClosed` | `src/core/host_event.rs` | 도메인 cascade 가 세우고 GUI 메인 루프가 비우는 호스트 이벤트 큐 항목. `crate::state` 가 재수출한다 |

`state` 쪽 호출자(탭·pane·preset 적용)도 같은 정의를 `core` 경로로 부른다.

`core` 는 `AppState` 를 이름으로 부르지 않는다. 구조 실행·cascade 가 필요로 하는 창 연산은
도메인이 선언한 포트 `core::cascade_window::CascadeWindow` 로 닿고, `AppState` 가
`src/state/cascade_window.rs` 에서 메서드마다 한 줄 위임으로 구현한다
([ADR-0440](../adr/0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md)).
그 포트의 메서드 목록이 "도메인 실행이 `AppState` 에서 무엇을 쓰는가" 의 답이다.

IPC 핸들러는 **창 상태를 읽을 때만** `AppState` 를 받는다
([ADR-0470](../adr/0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md)). 그리고 **엔진
핸들러는 창 상태를 받지 않는다** — 창에 닿아야 하는 일은 좁은 포트와 intent 출구로만 한다
([ADR-0471](../adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md)).

- **창 연산** — `IpcWindow`(`src/adapters/ipc/window_port.rs`)가 `CascadeWindow` 를 물려받아 엔진
  핸들러가 쓰는 창 연산을 선언하고, `AppState` 가 `src/state/ipc_window.rs` 에서 한 줄 위임으로
  구현한다. 대상 생략 시의 기본 워크스페이스(`active_workspace`)도 그 포트의
  `active_workspace_index` 로 읽는다. 그 메서드 목록이 "IPC 엔진 핸들러가 `AppState` 에서 무엇을
  쓰는가" 의 답이다.
- **intent** — 핸들러는 `pending_intents` 에 직접 넣지 않고 요청 하나의 `IntentOutbox` 에 넣는다.
  진입점(게이트 · 엔진 라우터 · `record_plugin_rss_samples`)이 요청 끝에 출구를 창 큐 끝으로 옮긴다.
- **`AppState` 를 받는 자리** — 창을 쥔 진입점(`handle_checked_request` · 헤드리스 `pump_ipc`)과,
  창 상태 자체가 대상인 GUI·debug 핸들러(파일 선택기 · popup · 배너 · 도구 메뉴 · debug 주입 ·
  `ui.state`)와 그 라우터(`route_window_handler` · `route_debug_handler`)뿐이다.
  `handle_checked_request` 는 받은 창을 곧바로 `EntryWindow`(`src/adapters/ipc/handler/entry_window.rs`)로
  감싸고, 그 아래 입구 본문(`route_checked_request` · `dispatch_routed`)은 `EntryWindow` 만 받는다.
  `EntryWindow` 의 필드는 모듈 밖에 안 보이고, 열린 문은 셋이다 — 엔진 라우터용 포트(`port`), gui
  창 라우터(`route_window`), debug 라우터(`route_debug`, 파일 전체가 `#![cfg(debug_assertions)]` 인
  `entry_window_debug.rs`). 그래서 입구 본문이 popup·`active_workspace` 를 이름으로 만지면 컴파일이
  깨진다. 헤드리스 `pump_ipc` 는 응답 전에 intent 적용(`drain_pending_intents`)에 창 전체를 넘겨야
  해서 이 봉인 밖이다(ADR-0471 Decision 5).

`AppState` 가 든 Core memory 핸들 사본(`memory`)은 IPC 핸들러가 읽지 않는다 — 같은 Arc 를 `Core`
로 읽는다.

## 재는 법

`없음`·`②`·`③` 칸은 headless 두 칸 검사로 닫혀 있다. 아래는 debug 프로필이고, release 두 칸
(`--release` 를 더한 것)도 함께 돌린다 — debug 핸들러만 읽는 필드는 release headless 에서만
dead 가 된다. 여덟 칸 전체는 [헤드리스 정의 경계](headless-build-boundaries.md) "재는 법":

```bash
cargo check -p tasty --no-default-features --lib
cargo check -p tasty --no-default-features --all-targets
```

`dead_code` 는 이 크레이트에서 error 라, 새 필드가 headless 에 컴파일되고 아무도 안 읽으면 앞
검사가 그 필드를 이름으로 찍고 실패한다. `③` 의 `expect` 는 거꾸로 — headless 에 읽는 자가 생겨
진단이 사라지면 `unfulfilled_lint_expectations` 가 그 자리를 **경고로** 가리킨다. 빌드·CI 를 막지는
않는다 — 그 lint 는 warn 이고 `Cargo.toml` 의 `[workspace.lints.rust]` deny 목록 밖이며, CI 는 `-D warnings` 를
안 쓴다. 그래서 이 칸의 변화는 경고 수로만 보인다. `②` 를 `cfg(feature =
"gui")` 로 좁히면 뒤 검사가 그 정의를 부르는 시험에서 실패한다.

`없음` 칸은 필드 선언 앞의 `#[cfg(feature = "gui")]` 로 읽는다.

## 관련

- [헤드리스 정의 경계](headless-build-boundaries.md) — gui 전용 판정 규칙 세 갈래와 여덟 칸
- [model-view-split](model-view-split.md) — Model 과 Host View 를 가르는 패턴
- [focus 정책](../design/policies/focus.md) — 사용자 view 상태 중 포커스의 운영 규칙
