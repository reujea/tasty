# App·Engine·View 상태 소유권

`AppState`(`src/app/state.rs`)는 프로세스의 시작·종료·요청 진행 값을 가진다. 서비스와 실제 실행 handle은 AppServices·App·EngineSession 같은 소유 객체에 남는다.

`MainViewState`(`src/state/main.rs`)는 창 하나의 navigation·표시·편집 상태와 실행을 요청하는 값 큐를 가진다. worker·socket·파일 저장 원본을 View 상태에 넣지 않는다.
Headless는 별도 `CommandContext`(`src/state/command.rs`)를 생성한다. 이 타입은 생략된 명령 대상을 해소하는 기본값과 요청 값만 가지며, GUI 상태 객체를 만들지 않는다.
공통 App adapter 코드의 `RequestContext`는 빌드에 맞는 구체 타입을 재노출하는 이름이다. Core는 이 타입을 참조하지 않는다.

GUI의 `navigation`은 사용자 선택 원본이고, headless의 같은 값 타입은 호환 명령의 대상 해소 문맥이다.
Workspace·Pane·Tab에는 사용자 선택이 없다. 명령은 해소된 ID로 실행하고 Core 결과의 ID를 App이 받아 선택 삭제 보정 또는 사용자 continuation을 적용한다.
IPC·저장·attach의 active/focused 값은 명시 read-only presentation으로 합성한다([모델·View 분리](model-view-split.md)).

창의 engine 소유자(`EngineSession`)는 MainViewState와 MainView 어디에도 없다. CoreState와 Terminal·hook·task·observer 자원은 Session이 각각 소유한다. GUI에서는 `App.engines`가 engine을 소유하고,
App이 창 ID로 찾아 View에 넘긴다([engine registry](../architecture/multi-window.md#engine-registry와-parked--pty-생존)).
CoreState만 필요한 함수에는 구조 참조를, 실행 adapter에는 `EngineMut`/`EngineRef`, View에는 `EngineRead`를 전달한다. View의 terminal·kind·surface 조회는 실행 owner를 반환하지 않는 읽기 API를 사용한다.
terminal tell·broadcast·respawn 실행 함수는 `EngineMut` 대신 `TerminalInputAccess`(`src/runtime/terminal_input_access.rs`)를 받는다. `EngineMut::terminal_input`이 조립하며 구조 읽기, 점유 읽기, 터미널 입력 쓰기(바이트·텍스트 즉시 쓰기와 ack 본문 쓰기뿐이며 resize·스크롤·연결 해제 같은 `Terminal`의 다른 조작은 열지 않는다)와 세대 확인, 제출 예약, child 등록의 idle·role·nickname 변경과 저장만 연다. hooks·task 범위·observer·remote·persistence에는 닿지 않는다. child 등록 저장은 기존 `ChildTerminalRegistry::save`의 병합 저장을 그대로 쓰며 tell·broadcast는 idle을 해제한 child가 있을 때 한 번 저장한다. respawn은 두 저장이 따로 있다. 실행 전 트리에 없는 child 등록을 정리해 바뀐 것이 있으면 한 번 저장하고, 입력을 쓴 뒤 role·nickname과 active 상태를 반영해 한 번 더 저장한다.
구조와 effect·kind 실행 객체의 나머지 분리는 이 문서를 따른다.

## 열 읽는 법

- **수명**: `프레임`(매 egui 프레임 다시 채운다) · `열림`(popup·메뉴·드래그가 열려 있는 동안) ·
  `요청`(설정한 쪽 다음 소비 한 번에 비워진다) · `세션`(창이 유지되는 동안) · `영속`(디스크에서
  로드된다).
- **headless**: `없음` 은 `cfg(feature = "gui")` 로 그 빌드에서 필드가 사라진 것이다.
  `읽힘` 은 headless 라이브러리 안에 그 필드를 읽는 자리가 있는 것이다. `③` 은 필드가
  headless 에도 컴파일되고 그 빌드가 값을 쓰지만 읽는 자가 GUI 뿐이라 항목 단위
  `expect(dead_code)` 를 단 것이다. `②` 는 headless 라이브러리에는 없고 테스트 구성에만 있는 것이다.
  `debug 헤드리스만 읽힘` 은 게이트가 `cfg(any(feature = "gui", debug_assertions[, test]))` 라 debug
  헤드리스에서만 컴파일되고 읽히는(주로 debug `ui.state` 덤프) 것이다 — release 헤드리스에는 필드가 없다.

빌드별 포함·사용 여부는 아래 "재는 법"의 컴파일 진단으로 확인한다.

## `dialogs` — GUI 소유 한 덩어리

`dialogs: DialogState` 는 필드 하나지만 그 안에 popup·메뉴·드래그의 입력 상태가 스무 개 넘게
있다. 자료형은 전부 `src/state/dialogs.rs` 한 파일에 있고, 그 모듈과 `dialogs` 필드는
`cfg(feature = "gui")` 다. **headless 빌드에는 이 상태가 아예 없다.**

모두 다음 분류를 따른다: 사용자 view 상태 · 수명 `열림` 또는 `요청` · 설정하는 쪽과 비우는 쪽이
모두 GUI(popup `draw_fn`·`on_close`, 사이드바·탭바, 메인 루프). 에이전트가 세우는 것처럼 보이는
자리도 비우는 쪽은 GUI 다:

| 필드 | 에이전트 쪽 입구 | 실제 도메인 데이터의 저장 위치 |
|---|---|---|
| `file_picker` | `file_picker.trigger`(그 핸들러 모듈이 gui 전용) | 결과는 plugin 에 이벤트로 나간다(docs/design/systems/popup.md) |
| `pending_open_preset_window` · `pending_preset_window_selection` | 사용자 origin 의 preset 저장만 세운다 | 저장된 preset 은 `PresetStore` 에 있다 |

승인 대기 목록 `approval_inbox` 는 `dialogs` 밖의 `MainViewState` 필드지만 같은 분류다. gui 전용이고,
`approval.request` · capability elevation 이 `enqueue_approval` 로 넣으며 App 이 승인 저장소가 바뀔 때마다
모든 창의 목록을 저장소와 맞춘다. 레코드는 `AppServices.approval_store` 가 갖고, 이 목록은 그중
**popup 이 보여줄 순서**다.

그래서 headless 가 이 상태를 제외해도 승인 등 도메인 데이터는 유지된다. approval 을 묻는 IPC
(`approval.list` 류)는 저장소를 읽는다.

두 판정은 dialog 가 없어도 debug 빌드의 두 조합에 남는다(release 헤드리스에는 없다) —
`has_input_dialog_open` 은 debug 헤드리스에서 `false` 를 답하고, 그 값을 쓰는 `keyboard_overlay_open`
도 그대로다. `ui.state` debug 덤프가 두 값을 두 조합에서 같은 키로 찍기 때문이다.

## 나머지 필드

| 필드 | 분류 | 수명 | 설정하는 쪽 → 비우는 쪽 | headless |
|---|---|---|---|---|
| `navigation` | 사용자 workspace/pane/tab/surface ID 선택·category 접힘·split 호환 hint | 세션 | 사용자 intent·구조 결과의 삭제 보정 → 표시·저장 projection | CommandContext는 생략 대상 해소 기본값으로 별도 소유 |
| `tab_bar_scroll` | pane ID별 사용자 탭바 스크롤 | 세션 | 사용자 입력·표시 보정 → pane 삭제 시 회수 | 없음 |
| `category_last_active` | 사용자 view 상태 | 세션 | 사용자 전환 → — | debug 헤드리스만 읽힘(release 에는 필드 없음) |
| `settings_open_requested` · `plugins_open` | 사용자 view 상태 | 요청 | 사이드바 버튼 → 다음 프레임 `dispatch_pending_modal_opens` | 없음 |
| `sidebar_width` · `sidebar_visible` · `sidebar_collapsed` | 사용자 view 상태 | 세션 | 폭은 매 렌더에 `ui_scale`·접힘·숨김에서 도출, 접힘·숨김은 사용자 토글 → — | 없음 |
| `pending_resize_cursor` · `switch_overlay` · `modifier_hint` · `tutorial` | 사용자 view 상태 | 프레임·열림 | GUI 입력 → GUI | 없음 |
| `dialogs` | 사용자 view 상태 | 열림·요청 | 위 절 | 없음 |
| `tab_bar_height` | 사용자 view 상태 | 프레임 | 탭바 실측 → — | 없음 (테스트 구성에는 있다 — ②) |
| `popups` · `toasts` · `banners` | 사용자 view 상태 | 열림 | GUI → GUI | 없음 |
| `fullscreen_stage` · `stage_closed_queue` · `stage_deferred_grid_resync` | 사용자 view 상태 | 열림 | GUI → GUI | 없음 |
| `search` | 사용자 view 상태 | 열림 | 검색 바 → 검색 바 | 없음 |
| `port_scan` · `port_favorites_scan` | 사용자 view 상태 | 열림 | port scanner popup → popup | 없음 |
| `command_palette` | 사용자 view 상태 | 열림 | 팔레트 → 팔레트 | 없음 |
| `recent_files` | 도메인 사실 | 영속 | 디스크 로드·파일 열기 → — | 읽힘 |
| `host_popup_covers` · `plugin_popup_covers` · `banner_hovered` · `resize_edge_widget_hovered` | 사용자 view 상태 | 프레임 | egui 패스 → 입력 라우팅 | 없음 |
| `plugin_popup_open` | 사용자 view 상태 | 프레임 | plugin popup 그리기 → 입력 라우팅 | 없음 |
| `popup_layers` · `plugin_popup_layers` · `host_popup_hittest` · `frame_key_owner` · `plugin_popup_frame_entries` · `plugin_popup_hittest` · `banner_layer` | 사용자 view 상태 | 프레임 | egui 패스 → 입력 라우팅 | 없음 |
| `preset_store` | 읽기 facade (`PresetCatalog`) | View 수명 | AppServices 저장소의 목록 query → popup 표시 | 없음 |
| `memory` | View 필드 아님. AppServices/EngineRuntime의 실행 저장소 | process/engine | App·runtime adapter → 저장 서비스 | EngineRuntime 사용 |
| `pending_lifecycle_events` | View 필드 아님. EngineRuntime 완료 큐 | engine 요청 | retirement 완료 → plugin 통지 | EngineRuntime 사용 |
| `pending_host_events` | View 필드 아님. EngineRuntime 및 AppState의 별도 큐 | engine/process 요청 | 확정 결과·표시 관측 → Event Bus, hook 완료 | EngineRuntime drain |
| `last_focused_surface_id` · `last_active_workspace_id` · `last_focused_tab` · `last_tab_locations` | 실행 자원 (변화 감지 기준값) | 세션 | GUI tick 의 감지 → 같은 자리 | 없음 |
| `explorer_views` · `dag_graph_views` | 사용자 view 상태 | 열림 (surface 수명) | surface 그리기 → surface 닫힘 | 없음 |
| `explorer_clipboard` | 사용자 view 상태. AppServices 공용 슬롯의 핸들로 모든 창이 같은 값을 본다 | 세션 (process) | 어느 창의 explorer 복사·잘라내기 → 잘라내기 붙여넣기 성공(붙여넣은 창이나 surface 가 그 사이 닫혔어도 App 이 공용 슬롯에서 정리) | 없음 |
| `branch_cache` | 사용자 view 상태 (상태바 표시 cache) | 세션 | busy tick 이 포커스 surface 로 갱신 → 다음 busy tick 이 덮어씀 | 없음 |
| `shell_integration_hint_shown` | 사용자 view 상태 (배너 표시 기록) | 열림 (surface 수명) | 셸 통합 안내 cascade → surface 닫힘(`release_surface_views`) | 없음 |
| `tool_registry` · `palette_plugin_commands` | 도메인 사실의 사본 (plugin 기여 목록) | 세션 | plugin 활성 → 재계산 | 없음 |
| `pending_plugin_command_invokes` · `pending_tool_events` · `pending_popup_opens` | 실행 자원 (큐) | 요청 | 도구 메뉴·팔레트 → 메인 루프 | 없음 |
| `pending_handler_ipc` | 실행 자원 (큐) | 요청 | 파일 핸들러 dispatch → 메인 루프 | 없음 (넣는 자리 `file::dispatch` 의 핸들러 실행도 GUI 다) |
| `file_handler_recent` | 사용자 선택 기록 | 영속 | 창 생성 시 디스크 로드·picker 선택 → — | 없음 |
| `drop_hover` · `pending_file_drops` | 사용자 view 상태 | 열림·요청 | OS drag&drop → frame end | 없음 |
| `plugin_popup_closes` · `plugin_popup_focus_bumps` · `plugin_banner_closes` | 실행 자원 (큐) | 요청 | egui 패스 → 메인 루프가 plugin 에 통지 | 없음 |
| `plugin_mesh_popup_regions` · `plugin_popup_ime_cursor_area` | 사용자 view 상태 | 프레임 | egui 패스 → 합성·IME | 없음 |
| `plugin_mesh_popup_forward` · `plugin_mesh_banner_forward` | 사용자 view 상태 | 열림 | egui 패스 → 합성 | 없음 |
| `plugin_mesh_banner_regions` | 사용자 view 상태 | 프레임 | egui 패스 → 합성 | 없음 |
| `plugin_mesh_popup_pending_repaint` · `plugin_mesh_banner_pending_repaint` | 사용자 view 상태 | 요청 | plugin repaint 요청 → 합성 | 없음 |
| `plugin_popup_user_activated` | 실행 자원 (사용자 행동 근거) | 열림 | `draw_plugin_popups` 입력 forward → popup 닫힘 | 없음 |
| navigation proof | View 필드 아님. AppServices의 `NavigationProofs` | 원 document·View 요청 | App webview 동기화 → bound 요청에서 1회 소비/만료 | 없음 |
| `pending_intents` | 실행 자원 (큐) | 요청 | GUI 의 `dispatch_intent` · IPC 진입점이 옮기는 요청 출구 → `dispatch_pending_intents` / headless drain | 읽힘 |

활성 모달의 ID·종류는 MainViewState에 없다. 모달은 앱 전체에 최대 1개라 `ViewRegistry`(`src/view/mod.rs`)가 유일한 원본으로 갖고, `App::open_modal`이 세우고 `App::close_active_modal`과 macOS의 `App::handle_minimize`가 비운다. 이 세 곳 밖에서는 바꾸지 않는다. debug `ui.state`의 `modal_open`·`active_modal_id`·`active_modal_kind`는 handler가 모달 없음으로 채운 뒤 GUI App이 응답을 보내기 전에 이 원본으로 덮어쓴다. 그래서 창과 parked 상태 어느 쪽이 응답해도 같은 값이고, 헤드리스는 늘 모달 없음이다.

## 윈도우가 함께 쓰는 영속 목록

윈도우마다 engine 이 있으므로 engine 에 영속 목록의 사본을 두고 변경마다 사본 전체를 저장하면, 다른 윈도우의 저장이 이 윈도우의 변경을 파일에서 지운다. 파일 하나에 통째로 저장하고 UI 가 그리는 작은 목록은 `src/core/shared_list.rs` 의 `SharedList<T>` 로 프로세스에 원본 하나를 둔다. 대상은 Explorer 즐겨찾기(`explorer-favorites.toml`)와 포트 즐겨찾기(`port-favorites.toml`)다.

- `RuntimeRegistries` 가 시작할 때 파일을 한 번 읽어 원본을 만들고 모든 engine 이 같은 `Arc` 를 받는다. 실행 중 저장은 시작 때 관측한 원본과 비교하고 외부 편집이 있으면 덮지 않고 실패로 남긴다.
- engine 은 그리기용 사본 `Replica<T>` 를 가진다. 읽기는 `Deref` 로만 열려 있고, 변경은 `Replica::change` 가 원본을 바꾸고 표시 revision을 올리고 불변 snapshot을 file writer에 접수한다. 원본 잠금 안에서는 접수 순서만 정하며 파일 I/O와 완료 대기는 하지 않는다. 변경 판단(예: 이미 있으면 해제)도 이 closure 안에서 원본 기준으로 한다.
- 표시 revision과 저장 완료 revision은 다르다. 완료 receipt가 성공한 revision까지만 확정하며, 오래된 완료·실패·접수 거절은 최신 dirty를 지우지 않는다. 종료는 최신 dirty에 실행 중인 요청이 없을 때 한 번 더 접수하고 writer를 drain한다. 자세한 경계는 [파일 실행기](../design/systems/storage-contract.md#파일-실행기)에 있다.
- 각 윈도우는 그리기 전에(`App::redraw_main_window`) `EngineRuntime::sync_shared_lists` 로 리비전을 비교해 사본을 다시 복사한다.
- App 은 다른 윈도우에 마지막으로 알린 리비전(`App::shared_lists_announced`)을 들고, 대기 intent 를 처리한 뒤(`dispatch_pending_intents`)와 각 그리기 끝에 원본 리비전과 비교한다. 달라졌으면 모든 메인 윈도우를 다시 그리게 한다(`App::redraw_windows_for_shared_lists`). 그리기 밖에서 적용되는 변경도 다른 윈도우에 바로 보인다.
- Given 윈도우 두 개 When 각 윈도우가 같은 목록에 다른 항목을 더한다 Then 재시작 뒤 둘 다 남는다(`engine_action.rs` 의 `favorites_added_in_two_windows_both_survive_a_restart`, `port_favorites_toggled_in_two_windows_both_survive_a_restart`).
- Given 목록이 바뀌었다 When 다음 확인 지점에 온다 Then 모든 메인 윈도우가 한 번 다시 그려진다(`view_frame.rs` 의 `a_shared_list_change_redraws_every_window_once`).

## 모듈 단위 예외 없이 가른다

MainViewState와 CommandContext는 별도 struct다. 공통 알고리즘과 값 타입은 재사용하되 GUI popup·hover·렌더 자료는 headless에 만들지 않는다. 실제 승인 레코드는 AppServices에 있고 popup의 pending ID 목록은 View 상태다. intent origin 검사는 사용자 선택·닫은 항목 기록 보호를 위해 계속 유지한다.

## Core 결과와 공통 App adapter

구조 요청은 App의 journal admission을 거쳐 worker가 decide/commit한다. 확정 batch만 CoreState projection에 적용하며 GUI/IPC/원격 응답을 위해 구조를 두 번 실행하지 않는다. raw input·파일 실행처럼 구조 event 밖의 작업은 원 대상 binding을 가진 실행 adapter로 전달한다.

닫힌 surface의 원 box·Terminal/Pty는 `ResourceRetirement`가 단독 소유하고 effect claim 뒤 정리한다. 원 PTY/plugin receipt와 metadata 의무가 끝나기 전에는 성공으로 응답하지 않는다. MainViewState는 화면 cache·선택 보정만 맡으며 필수 cleanup이 View 유무에 의존하지 않는다. 원 engine이 retiring 중이면 receipt까지 유지한다([닫기 순서](../architecture/close-sequence.md)).

공용 파라미터 검증과 권한·점유·origin 정책은 진입점에서 유지한다. RequestScope는 필요한 presentation을 빌리고 요청 값을 돌려줄 뿐 구조 writer나 전체 View 실행 포트가 아니다.

핸들러는 사용하는 상태만 인자로 받는다. 쓰지 않는 `_state: MainViewState` 인자를 공통 호출 모양에 맞추려고 남기지 않는다. memory와 대상 nickname 해석은 AppServices 또는 명시 Engine 실행 문맥에서 읽고, 출력 조회는 EngineRead의 관측값으로 수행한다. CoreState는 확정 구조 projection이며 실행 저장소나 터미널 객체를 소유하지 않는다. GUI·debug에서 창 상태 자체를 조작하는 핸들러는 창 전용 라우터에 둔다.

IPC engine 핸들러는 동기 `RequestScope`의 borrowed presentation과 권한 근거, 요청별 `IntentOutbox`를 사용한다. `IpcWindow`에는 실행 owner나 창 전체를 꺼내는 접근자가 없다. 요청이 끝나면 App이 `RequestOutputs`의 intent와 승인 표시 요청을 원 문맥에 적용하며, 확정 구조 명령의 완료는 journal publication 경계를 따른다.
창·debug 라우터는 별도 경로이며 각 창 메서드의 caller 정책을 선언·검증한다. 선택 변경은 originating View와 navigation generation을 확인하는 표시 continuation이며 도메인 필수 정리를 수행하지 않는다.
헤드리스 pump는 CommandContext를 소유하고 같은 App 실행 adapter로 결과를 적용한다. GUI 전용 요청의 기존 미지원 오류를 유지한다.

### 아직 남아 있는 동작 차이

구조 완료의 host/lifecycle 통지는 EngineRuntime 큐가 보관하고 App이 View 유무와 별개로 해소한다. GUI 표시 cache 정리는 별도 후처리이며 engine의 필수 통지·자원 회수 완료를 대신하지 않는다.

원격 pane split에서 전달된 params에 `target_pane`이 있고 서버가 `target_surface`도 채우면 두 대상 동시 지정 오류가 날 수 있다. 실행 함수 통합은 이 기존 동작을 바꾸지 않았다. 실패 문구의 경로 간 일치와 문구 자체의 호환은 별도로 검사한다.

## 재는 법

`없음`·`②`·`③` 칸은 headless 두 가지 검사로 확인한다. 아래는 debug 프로필이고, release 두 조합
(`--release` 를 더한 것)도 함께 돌린다 — debug 핸들러만 읽는 필드는 release headless 에서만
dead 가 된다. 여덟 조합 전체는 [헤드리스 정의 경계](headless-build-boundaries.md) "재는 법":

```bash
cargo check -p tasty --no-default-features --lib
cargo check -p tasty --no-default-features --all-targets
```

`dead_code` 는 이 크레이트에서 error 라, 새 필드가 headless 에 컴파일되고 아무도 안 읽으면 앞
검사가 그 필드를 이름으로 찍고 실패한다. `③` 의 `expect` 는 거꾸로 — headless 에 읽는 자가 생겨
진단이 사라지면 `unfulfilled_lint_expectations` 가 해당 항목을 **경고로** 알린다. 빌드·CI 를 막지는
않는다 — 그 lint 는 warn 이고 `Cargo.toml` 의 `[workspace.lints.rust]` deny 목록 밖이며, CI 는 `-D warnings` 를
안 쓴다. 그래서 이 칸의 변화는 경고 수로만 보인다. `②` 를 `cfg(feature =
"gui")` 로 좁히면 뒤 검사가 그 정의를 부르는 시험에서 실패한다.

`없음` 칸은 MainViewState에만 있는 필드 또는 GUI 조건으로 제외되는 정의다.

## 관련

- [헤드리스 정의 경계](headless-build-boundaries.md) — gui 전용 판정 규칙 세 갈래와 여덟 칸
- [model-view-split](model-view-split.md) — Model 과 Host View 를 가르는 패턴
- [focus 정책](../design/policies/focus.md) — 사용자 view 상태 중 포커스의 운영 규칙

## 실행 자원 대여의 검사 범위

`engine_resource_ownership` 문서 가드는 CoreState의 이동 대상 자원 타입 재유입, Session의 직접 소유,
EngineRef/EngineMut의 참조 필드와 Session의 암묵적 Deref를 검사한다. 이름을 가진 필드와 직접 경로를 읽는 검사이며
타입 별칭·전이 의존이나 CoreState 전체의 순수성을 증명하지 않는다. `domain_does_not_reach_up`은 `tasty-core`·`tasty-model`의 지정 상위 host·GUI 직접 참조를 검사한다. root 실행 adapter 전체를 순수 domain으로 취급하지 않는다. Session 단위 시험은 Terminal/task 원본 공유, observer 종료 flush,
교체된 Terminal 내용의 격리와 parked 자원의 보존을 검사한다. 실제 PTY 종료·reap과 resource generation의 계약은 별도 검증 대상이다.

전역 registry의 실제 Arc 소유자는 AppServices.registries이며 EngineRuntime과 PluginManager는 같은 인스턴스를 공유한다. 등록·철회와 설정 저장은 창이 없는 상태에서도 App에서 실행한다. 추가 창의 View 조립이 실패하면 pending Engine은 retiring 관계에서 이미 수락한 실행 의무와 필수 통지를 마친 뒤 해제된다. 이 실패는 저장된 slot의 사용자 폐기가 아니므로 기존 stream을 삭제하거나 실패한 View 선택으로 checkpoint를 덮지 않는다.

파일 detector·handler 사용자 문서와 디스크 revision은 `RuntimeRegistries.file_rules`가 소유한다. View는 편집 결과만 돌려주고 App이 두 registry의 사용자 export를 합쳐 file writer에 제출한다. 원본 관측은 부팅의 한 번 읽기와 공유하며, 완료·실패 수거는 writer wake acknowledge 뒤에 한다. registry 내부 effective 재계산용 dirty를 디스크 저장 완료로 사용하지 않는다. 명시적 reload는 pending 저장이 없을 때 같은 경로의 문서로 두 registry와 원본 관측을 함께 바꾼다. 저장 실패는 메모리 적용을 되돌리지 않고 설정 결과 토스트와 알림으로 남긴다.


### 레거시 복원 메타데이터 정리

슬롯의 레거시 레이아웃 이관이 성공했고 최초 incarnation을 명시적으로 복원할 때, worker는 이관 기록에서 정리 여부만 반환한다. App은 실행 자원 복원 전에 PTY 범위를 침범한 Surface scope를 기존 메모리 저장소에서 삭제하고 오류를 기록한다. 삭제는 해당 scope의 regular·secret 영역에 함께 적용된다. 같은 incarnation을 다시 복원하면 DB View checkpoint가 있어도 이 정리를 재시도한다. 새 incarnation, 복원 비활성, headless 새 시작, 이관 실패, 다른 저널의 명시적 가져오기는 이 레거시 정리를 실행하지 않는다. 메모리 쓰기 소유권은 App에 있고 worker에는 전달하지 않는다.

원격 구조 요청의 내부 durable correlation은 client·runtime epoch·op_id와 원 연결 registration을 보존한다. 128비트 registration은 JSON 숫자 범위로 잘리지 않도록 십진 문자열로 기록한다. StreamControl의 공개 wire와 원 연결의 Weak binding 검증은 이 표현 변경과 독립이며, correlation을 재시도 키로 사용하지 않는다.

실행 정리의 관측값은 수명과 구분한다. runner의 첫 stop 호출 결과를 사용하지 않아도 원 receipt는 EngineSession 또는 RunnerRegistry에 남아 후속 poll이 실제 join을 관측한다. activation의 OnceLock은 최초 Ready/Failed만 전달하며, 반복 publication이나 Drop이 이미 전달한 결과를 덮어쓰지 않는다. 점유 해제의 예상 밖 실패는 로그로 남긴다.

복원할 surface에 저장된 탭 제목이 없거나 복구 미확정 사유가 없는 경우의 표시 문구는 `surface.restore` 번역 키를 사용한다. 이미 기록된 제목과 원 오류 사유는 보존한다.

journal 완료 pump는 bootstrap, batch 적용, 요청별 결과를 private 함수로 나눠 처리한다. 모든 대상 Engine의 projection·실행 자원 설치가 끝난 뒤 batch ACK를 보내고, 그 뒤 원 View continuation과 관측값을 적용하는 순서는 유지한다. shutdown은 port scan·profile detection·screenshot의 기존 기한과 실제 미회수 관측을 각 함수에서 확인한 뒤 plugin 종료 단계로 진행한다.
