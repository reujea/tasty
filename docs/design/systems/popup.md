# 내부 팝업 시스템

Popup 은 View 내부에 존재하는 가상 창이다 — 터미널과 공존하며 포커스를 독점하지 않는다. 모든 내부 팝업은 **`PopupManager` + `PopupDef`** 로 관리된다(`src/adapters/ui/popup.rs`). 용어 구분은 [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md), 팝업을 *추가하는 법* 은 [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md). 이 문서는 시스템 *동작 모델* 이다.

## 8대 규칙

1. **타이틀과 콘텐츠**: 높이 토큰을 따르는 상단 제목 영역과 하단 콘텐츠로 나눈다.
2. **타이틀바**: 제목은 `font-size-max` 크기로 가운데에, 닫기 버튼은 오른쪽에 둔다. 버튼은 IconButton sm(`close` 아이콘)과 같은 모양·호버·누름 표시를 쓴다. `PopupDef.fullscreen_stage`를 선언한 팝업만 닫기 버튼 왼쪽에 같은 형태의 `fit` 전체화면 버튼과 툴팁을 표시한다. 제목은 버튼 수에 맞는 예약 폭을 양쪽에 대칭으로 비운 스트립 가운데에 오고 그 폭에서 말줄임한다([구현 가이드](../../dev-guide/popup-implementation.md#타이틀-길이-처리-elide)). [무대](fullscreen-stage.md)를 열어도 원본 팝업은 열린 채 유지한다. 무대는 같은 형태의 별도 콘텐츠다.
3. **드래그 이동**: `drag_handle` 영역에서 이동한다. 기본은 타이틀바이며, 타이틀바 없는 `port_scanner`·`remote_tool`·`file_picker`는 실제로 그린 헤더 사각형을 매니저에 전달한다. 첫 프레임은 `DragHandle::Region`을 사용한다. 헤더 글자는 선택하지 않게 하고, 버튼·입력이 포인터를 사용 중이면 드래그를 시작하지 않는다(`is_using_pointer`).
4. **커서**: 드래그 핸들은 grab, 크기 조절 테두리는 해당 방향의 리사이즈 커서를 표시한다.
5. **배경**: 기본은 `surface_raised`, 패널형 팝업은 `bg_panel`이며 타이틀바는 `mantle`을 쓴다. 실제 선택은 `popup::draw::popup_bg_fill`이 담당한다. 터미널과 같은 배경색일 수 있으므로 색 하나만으로 팝업 경계를 판단하지 않는다.
6. **경계**: 팝업은 소속 범위 밖으로 나가지 않으며 크기 조절 때 자동 재배치한다.
7. **다중 팝업**: 나중에 열거나 클릭한 팝업을 앞에 둔다. 겹친 영역의 마우스 입력은 최상단 팝업만 받는다([호스트·plugin 순서](#host--plugin-popup-z-order)).
8. **크기 조절과 입력**: `resizable` 팝업은 8방향 테두리에서 크기를 바꾸고 `min_size`와 범위 경계를 지킨다. 우선순위는 egui 위젯 → 닫기·전체화면 버튼 → 크기 조절 테두리 → 드래그 핸들 → 콘텐츠다. 콘텐츠를 그린 뒤 `is_using_pointer()`로 위젯의 입력 사용 여부를 확인한다. 사용자가 직접 크기를 바꾸면 닫기 전까지 sizer가 덮어쓰지 않는다.

(모든 색·치수는 Theme 토큰 — [theme.md](theme.md).)

## 구조

- **`PopupDef`** — 정적·데이터 지향 정의(id, title_key/title_fn, default_size/sizer, default_scope, close_on_outside_click, headless, sticky_focus, drag_handle, resizable, min_size, fullscreen_stage, draw_fn). 전부 `src/adapters/ui/popup/defs.rs::all_defs()` 에 모은다. 필드 상세 → [popup-implementation](../../dev-guide/popup-implementation.md).
- **`PopupManager`** — 공통 동작(z-order, 드래그, 리사이즈, 타이틀바, clamp, 포커스) 중앙 관리. `register_def` / `open*` / `close` / `draw`.
- **`PopupState`** — 개별 인스턴스 상태(id, title, pos, size, open, focused, scope, dragging/resizing, size_user_overridden).
- **범용 render 루프** — `src/adapters/ui/popup/frame.rs::draw_popup_layer`(등록된 모든 `PopupDef` 순회 + close 경로별 `on_close` 훅 drain). toast/banner/modifier-hint/tutorial 오버레이 체인은 개념이 다르므로(docs/design/systems/popup.md) `src/adapters/ui/overlay.rs` 로 분리돼 있다. 진입점 `src/adapters/ui.rs::draw_popups` 가 둘을 z-order 순서로 호출한다.

등록된 팝업은 `all_defs()` 가 단일 출처다(예: `notifications`, `convert_surface`, `rename`, `search_bar`, `tools_menu` …). 새 팝업 = 테이블 항목 1개 + draw 함수 하나. 단, plugin 이 소유하는 팝업(예: markdown 파일열기·large-file 확인)은 host `PopupDef` 가 아니라 plugin 매니페스트 `[[contributes.popup]]`(egui-mesh) 로 등록된다.

## Host ↔ Plugin popup z-order

호스트와 플러그인 팝업은 `next_popup_z_seq()`에서 열기·클릭 순번을 받는다. 매 프레임 각 묶음의 가장 큰 순번을 비교해 앞에 그릴 묶음을 정한다.

| 대상 | 순서 적용 방법 |
|---|---|
| 셸 배경·제목·테두리 | raw layer는 호출 순서만으로 정렬되지 않아 `enforce_host_plugin_popup_z_order`가 set_sublayer로 순서를 지정한다. |
| 플러그인 GPU 콘텐츠 | `render_egui_pass_and_mesh_popups`가 같은 판정으로 host egui와 plugin mesh pass 순서를 정한다. |
| 자기 콘텐츠 보호 | plugin 셸 배경을 콘텐츠를 제외한 네 사각형으로 그려, 먼저 그린 자기 콘텐츠를 덮지 않는다. |
| 클릭·hover·드래그·닫힘 | `occlusion.rs::point_ownership`이 좌표마다 popup 쌍의 z_seq를 비교한다. |

입력 판정의 `Mine`만 클릭 승격을 허용하고 `OutsideAll`만 바깥 클릭 닫힘을 허용한다. 위 팝업 안을 눌렀을 때 아래 팝업이 닫히거나 앞으로 나오지 않아야 한다. 플러그인이 보는 host rect는 같은 프레임 값이며, host가 보는 plugin rect는 이전 프레임 값이라 방금 닫힌 팝업이 클릭 하나를 더 차단할 수 있다.

렌더링 보정은 host 묶음과 plugin 묶음의 비교다. 여러 plugin 콘텐츠는 z_seq 순서지만 셸끼리의 순서는 set_sublayer의 중첩 제한 때문에 완전히 보장하지 않는다. modifier hint 등 다른 레이어 관계와 중첩되는 경우도 별도 확인이 필요하다. 입력 소유 판정은 팝업별 비교이므로 이 두 묶음 렌더링 제한과 구분한다.

## 수명 계약 (open → close → 뒷정리)

`draw_fn`은 그리기 전의 `popup_keyboard_focus` 스냅샷으로 키보드 포커스를 읽는다. 그리기 중에는 관리자를 별도로 대여한다. `PopupAction::ReleaseFocus`는 팝업을 닫지 않고 관리자를 복원한 뒤 포커스만 해제한다. 검색 바의 find 단축키가 이 경로로 터미널에 입력을 돌려준다.

닫기 경로는 draw_fn의 `PopupAction::Close`, X·바깥 클릭, `UiIntent::ClosePopup`, 열린 팝업의 `UiIntent::TogglePopup`, App의 직접 `close()`, debug IPC의 여섯 가지다. 모두 `PopupManager::close()`를 거쳐 `PopupState.open`을 false로 바꾼다.

초안이나 대상 ID의 정리를 draw_fn에만 두면 다른 닫기 경로에서 실행되지 않는다. `PopupDef.on_close`를 등록하면 close가 `closed_queue`에 넣고 다음 `draw_popup_layer`에서 한 번 호출한다. 훅이 다른 팝업을 닫으면 같은 처리 중 이어서 정리하되, 상호 재열기 같은 오류를 막는 라운드 상한을 둔다. `on_close: None`에는 상태가 없거나 남겨도 되는 이유를 적는다. 예를 들어 notifications는 펼친 항목과 스크롤 위치를 다시 열 때 유지하고 tutorial_topics도 남겨도 되는 상태다. notifications fullscreen 무대의 스크롤 상태는 무대 종료 시 별도로 지운다.

절차·필드 상세는 [popup-implementation §닫힘 정리](../../dev-guide/popup-implementation.md#닫힘-정리).

### plugin popup ↔ host popup 부모-자식

plugin 이 `file_picker.trigger`로 host popup 을 열 때 `owner_popup_instance` 로 **자기 popup instance_id 를 전달**하면, host 는 그 값을 자식 쪽 요청자 기록에 보관해 두 popup 을 스택으로 다룬다(이 문서). 전달하지 않으면(예: popup 밖 surface 위젯에서의 호출, Tools 메뉴 진입) 지금까지처럼 관계 없는 단독 popup 이다.

관계가 성립하면:

- **범위 상속·숨김 보존** — host는 요청자와 부모 instance를 대조해 부모가 직전 frame 에 실제로 놓인 범위(좁은 surface 라 창 범위로 대체된 경우 포함)를, 아직 놓이지 않았으면 선언 종류+target의 유효 범위를 자식 파일 피커에 적용한다(이 문서). 부모가 숨으면 자식도 paint/hit/Esc/키 게이트에서 빠지고, 돌아오면 draft·선택·pending 요청을 그대로 이어간다. 숨김은 닫기로 처리하지 않는다. Window 부모와 owner 없는 피커는 창 범위다.
- **스택 유지** — 자식이 열려 있는 동안 부모는 outside-click dismiss 대상에서 빠진다. 부모를 모달로 잠그는 것이 아니라 dismiss 목록에서만 제외한다(popup은 포커스를 독점하지 않으므로).
- **Esc 소유권** — host/plugin 통틀어 그 프레임 최상단 popup **하나만** Esc 를 소비한다. Esc 를 한 번 누르면 최상단 팝업 하나가 닫힌다. host 쪽 판정은 `MainViewState.popup_escape_owner`(`popup::frame` 이 매 프레임 결정), plugin 쪽은 `popup_render` 가 같은 z 축으로 비교한다. **host popup 끼리의 Esc 중재는 범위 밖** — 각 view 가 자기 Esc 를 직접 소비하며, 현재 스택에 참여하는 `file_picker` 에만 게이트가 붙어 있다.
- **연쇄 정리** — 부모가 어떤 경로로 닫히든 자식 피커에 취소 결과가 채워져, 평소 result 경로 그대로 plugin 에 `cancelled: true` 가 전달되고 피커도 닫힌다. 부모 없는 피커나 결과 유실을 남기지 않는다. 사용자가 이미 확정한 결과는 덮지 않는다.

소유 관계는 자식(요청자 기록) 한 곳에만 있다 — 부모 쪽 사본이 없어 둘이 어긋날 수 없다. host는 특정 plugin 이름/kind로 분기하지 않는다. 범위 상속에서는 요청자 plugin과 부모 소유자가 같은지만 확인한다.

## 발화 정책 (CRITICAL)

팝업은 사용자 행동(키보드 단축키·마우스·메뉴)으로 연다. 사용자 조작 근거가 없는 시스템·에이전트·플러그인 요청은 키보드 포커스를 가져오는 팝업을 띄울 수 없다([사용자 통제와 데이터 원칙](../../principles/control-and-data.md), [toast.md](toast.md) 와 같은 기준). 금지 대상은 포커스 탈취다. 사용자 결정을 기다리는 요청만 아래 허용 범위에서 포커스 없는 passive 팝업으로 보일 수 있다.

- ✅ 단축키/마우스/메뉴 → `UiIntent::OpenPopup` 발화
- ✅ popup A 의 *사용자 액션* cascade → popup B (origin 전파)
- ❌ 사용자 조작 근거가 없는 IPC/CLI/Plugin·시스템 요청으로 키보드 포커스를 가져오는 팝업 열기
- ❌ 결정이 필요 없는 시스템 조건(PTY 종료·시간 경과)으로 자동 popup — 대신 *Domain Intent 로 데이터만 변경*(NotificationStore push 등)하고 UI에서 사용자가 확인
- ✅ debug 의 `debug.popup.*` — *사용자 입력 재현* 한정 ([debug-ipc](../../dev-guide/debug-ipc.md))
- ✅ 사용자 결정을 기다리는 요청의 **passive 팝업**(아래). 허용 범위는 [승인 팝업](../../features/human-handoff/index.md) 하나다 — `approval.request`, 권한 상승 요청, telemetry cap 의 `require_approval` 이 이 팝업을 쓴다. 다른 팝업을 사용자 조작 근거 없이 열려면 먼저 이 목록에 허용 범위로 추가한다.

### passive 팝업

origin 이 사용자가 아닌 `UiIntent::OpenPopup`·`TogglePopup` 은 `OpenPopupMode` 가 정한 위치로 열되 키보드 포커스를 주지 않는다(`intent::popup::open` → `PopupManager::mark_passive`). 빌드와 무관하다. 이 처리는 허용 범위 밖의 요청이 실수로 들어와도 포커스를 빼앗지 않게 하는 안전장치이며, 허용 범위를 넓히지 않는다. 이미 열린 팝업에 온 요청은 무시하므로 사용자가 준 포커스를 빼앗지도 않는다. `debug.host_popup.open` 은 사용자 조작 재현이라 사용자 출처로 열어 지금처럼 포커스를 준다.

- passive 팝업은 `has_focused()` 에 들지 않으므로 열려 있어도 `keyboard_overlay_open()` 이 거짓이다. 터미널 입력, explorer·dag_graph 처럼 egui 입력을 받는 surface의 입력, native WebView 의 입력이 그대로 이어진다. Esc 의 포커스 해제 대상(`focused_dismissal_target`)도 아니다.
- native WebView 는 모두 숨기지 않고 키보드 포커스도 회수하지 않는다. 토스트 카드와 같이 팝업 영역과 겹치는 WebView 만, 그 WebView 가 키 입력을 받지 않을 때 숨긴다(`PopupManager::visible_passive_rects`, [webview](webview.md#포커스--회수는-조건부다)). 키를 받는 WebView 와 겹치면 그 WebView 가 팝업을 덮는다. native 위에 egui 를 그리는 층이 없어서이며, 요청은 함께 만든 알림으로도 보인다. 일반 팝업이 떠 있어도 페이지를 보이는 모드는 [WebView §egui 오버레이와 native WebView](webview.md#egui-오버레이와-native-webview)에 있다. 모드는 backend 가 보고한다. Linux(X11)는 XFixes 2.0 과 SHAPE 확장이 있을 때 합성 관리자가 있으면 투명, 없으면 불투명이고, 확장이 없거나 적용이 실패하면 숨김이다. Windows 와 macOS 는 불투명이다. passive 팝업은 그 모드의 대상이 아니다.
- 팝업을 누르면 일반 팝업과 같이 포커스를 받고 passive 가 풀린다. 호스트가 확인한 사용자 입력도 같은 전이를 낸다: 파일 피커를 연 plugin 이 같은 surface 에서 증명이 있는 `file_picker.trigger` 를 다시 보내면 사용자 출처 `UiIntent::FocusPassivePopup` 이 보이는 다른 팝업의 포커스를 풀고 그 피커를 맨 앞으로 올린다(`PopupManager::focus_passive`). 증명 없는 요청과 다른 plugin·surface 의 요청은 busy 로 남는다. 그때부터 일반 팝업처럼 오버레이 모드의 대상이 되고(지원하지 않으면 모든 native WebView 를 숨긴다) 포커스를 회수한다. 닫으면 passive 도 풀린다.
- 비포커스 상태의 별도 시각 표현은 없다. 포커스가 없는 다른 팝업(알림 패널 등)과 같은 모양이다.
- 팝업 내용이 자기 포커스를 확인할 때는 `MainViewState.popup_keyboard_focus` 를 읽는다. draw 동안에는 매니저를 빌릴 수 없어 그리기 직전에 복사한다.

파일 피커의 범위 회귀시험(`src/adapters/ipc/handler/file_picker_scope_tests.rs`)은 plugin이 연 직후의
passive 상태와 실제 포인터 클릭으로 포커스를 받은 상태를 각각 숨겼다가 복원한다. 두 경우 모두
선택·요청·z 순서를 보존하고 숨은 동안 paint/hit/Esc/키 게이트에 참여하지 않는다. 복원은 포커스를
새로 주지 않으며, 클릭한 경우에만 기존 포커스 의도를 되살린다.

**타입 차원 강제**: Core/Domain 핸들러는 `UiIntent`(`OpenPopup`/`ClosePopup`/`TogglePopup`)를 요청하는 메서드를 갖지 않는다 — GUI adapter(단축키 핸들러·메뉴 콜백·popup draw)에서만 요청할 수 있다. `state.popups.open*` 직접 호출도 금지(Intent 경유). 디스패치 상세는 [`design/flows/action-dispatch.md`](../flows/action-dispatch.md).

## 포커스

팝업은 **포커스 상태**를 가진다. 보이는 범위에 포커스된 팝업이 있으면 키보드 입력이 터미널로 안 간다. 여러 팝업이 겹쳐 있을 때 Esc 를 누가 받는지는 위 [§수명 계약](#plugin-popup--host-popup-부모-자식)의 "Esc 소유권" 을 따른다. 클릭 → 포커스(다른 팝업 언포커스), 바깥 클릭 → 전체 언포커스(터미널 복귀), 닫기 → 자동 언포커스. `PopupManager::has_focused()`로 확인한다. 이 조회는 최신 draw의 범위 가시성을 포함한다. 숨은 popup의 포커스 의도는 보존하며 다른 화면의 클릭은 그 의도를 바꾸거나 popup을 dismiss하지 않는다. 숨김 때 드래그·리사이즈 캡처는 해제한다. native WebView를 가리는 overlay 판정도 열린 popup 중 보이는 것만 센다. 오버레이 모드가 숨김이 아니면 페이지 위 클릭도 이 바깥 클릭 규칙을 따르고 페이지로 가지 않는다([WebView §영역과 backend 계약](webview.md#영역과-backend-계약)).

Modal 의 전역 입력 독점과 다르다 — 팝업 포커스는 **키보드만** 차단하고, 마우스는 [입력 계층](../../architecture/input-layer.md)에 따라 팝업이 소비한다.

plugin의 egui-mesh 팝업도 키보드 입력을 차단한다. 호스트 PopupManager 소속이 아니므로 렌더 프레임이 `MainViewState.plugin_popup_open` 캐시를 갱신한다. PluginManager에 접근할 수 없는 winit 입력 핸들러는 이 값을 읽는다.

`MainViewState::keyboard_overlay_open()`은 egui에 키·IME를 전달할지, 터미널로 보내지 않을지를 함께 결정한다. 서로 다른 조건을 쓰면 양쪽이 모두 처리하거나 모두 버릴 수 있다. IME 라우팅과 plugin surface 단축키도 같은 조건을 쓴다.

`set_ime_allowed`만은 plugin 팝업을 차단 조건에서 제외한다. 이 팝업에는 호스트 egui 위젯이 없으므로 IME를 끄면 조합 입력을 할 수 없다. `collect_mesh_popup_input`이 이미 호스트 egui에 들어온 이벤트를 plugin에 보내고, 후보창 위치는 plugin이 반환한 값으로 정한다([egui-mesh 채널](../../dev-guide/egui-mesh-channel.md)).

캐시는 렌더 프레임에 갱신되므로 popup 이 열린 **직후 최대 1 프레임** 늦게 반영된다 — 짧은 지연이므로, 이 캐시를 읽는 입력 경로에서는 열린 직후의 프레임 차이를 고려한다.

### sticky_focus

`true` 면 바깥 클릭에도 키보드 포커스가 유지된다(닫기로만 해제). 마우스는 터미널에 정상 전달. *키보드는 항상 자기가 받되 터미널 마우스 조작(스크롤/선택)은 허용* 해야 하는 오버레이용. 지금 이 값을 쓰는 것은 명령 팔레트 하나이며, 팔레트는 `close_on_outside_click` 도 참이라 바깥 클릭에 닫힌다. 검색 바와 알림 패널은 `sticky_focus` 가 거짓이라 바깥 클릭에 포커스를 잃고 열린 채 남는다.

### close_on_outside_click

`false`(기본): 바깥 클릭 시 언포커스만(팝업 유지, 예: 알림 패널). `true`: 바깥 클릭 시 닫음(예: surface 타입 전환).

## 스코프

팝업은 소속 범위(`PopupScope`)를 가지며 가시성·경계가 결정된다. enum: `Window` / `Workspace(u32)` / `Pane(u32)` / `Tab(u32)` / `Surface(u32)`. 모든 대상은 ID다. 에이전트가 앞쪽 workspace를 닫거나 workspace·탭 순서를 바꿔도 팝업은 연 대상을 따라간다. 같은 인덱스로 밀려온 다른 대상에서 보이지 않는다. `Tab`의 경계는 그 탭이 활성인 pane에서 찾는다. 범위 대상 workspace·탭·surface가 닫혀 사라지면 매 프레임 팝업을 그리기 전에 그 팝업을 닫고 `on_close`를 실행한다. 열린 채 숨어 같은 팝업을 다시 열 수 없게 되는 일을 막는다. 비활성 탭에 있어 보이지 않을 뿐인 surface는 사라진 것으로 보지 않는다. surface 변환은 같은 surface ID를 유지하므로 이 정리로 팝업이 닫히지 않는다. 플러그인 팝업이 연 파일 피커가 이렇게 닫히면 요청한 플러그인은 취소 결과를 받는다. `Window`·`Pane` 범위는 이 정리 대상이 아니다.

| 스코프 | 가시성 | 경계 clamp | scrim 이 덮는 rect |
|--------|--------|-----------|--------------------|
| Window | 항상(워크스페이스 전환 무관) | 윈도우 | 윈도우 |
| Workspace | 해당 워크스페이스 활성 시 | 워크스페이스 영역 | 윈도우 |
| Pane | 해당 pane 보일 때 | pane | pane |
| Tab | 해당 탭이 활성 탭일 때 | 탭 소속 pane | 탭 소속 pane |
| Surface | 해당 surface 보일 때 | surface **안쪽 8pt** | surface |

`PopupManager::draw()` 가 `LayoutContext` 를 받아 스코프별 가시성 필터 + clamp rect 를 결정한다.

clamp 는 팝업 크기를 경계 크기 이하로 줄이고 위치를 경계 안으로 옮긴다. 경계가 제목줄과 콘텐츠 여백보다 낮으면 콘텐츠 자리는 넓이 0 의 빈 사각형이 된다. 콘텐츠는 그 사각형으로 잘려 보이지 않지만 팝업은 열린 채 범위를 유지하고, 경계가 다시 넓어지면 sizer 가 있는 팝업은 원래 크기로 돌아간다.

Windows 에서 장식 없는 Tasty 창은 최소화 중에도 작업 표시줄 아이콘 크기(측정 160x28)를 client 크기로 보고한다. 이 값은 창 경계가 아니므로 egui 화면으로 쓰지 않고 직전 화면을 유지한다(`keep_screen_while_minimized`). 최소화한 동안 팝업은 clamp 로 줄거나 옮겨지지 않고, 복원하면 같은 자리·크기로 보인다.

`Workspace` 스코프의 clamp 는 실제로는 윈도우 전체다 — 워크스페이스가 윈도우를 통째로
차지하므로 둘이 같은 사각형이다. 따라서 이 스코프가 실질적으로 결정하는 것은 **가시성**이다.

**스코프는 `PopupDef` 에 못 박히지 않는다.** `Workspace(u32)` / `Pane(u32)` 처럼 대상을
런타임에야 아는 스코프는 `default_scope` 에 안전한 기본만 두고, 여는 쪽이
`OpenPopupMode::WithScope(scope)` 로 실제 값을 주입한다. 현재 소비자:

| 팝업 | 스코프 | 여는 쪽이 주입하는 값 |
|------|--------|----------------------|
| [DAG 목록](../../features/agent-collaboration/screens/dag-list-popup.md) (`dag_list`) | Workspace | 여는 시점의 활성 workspace ID |
| 변환(`convert_surface`) · 검색바(`search_bar`) | Surface | 포커스 surface id |
| 부모가 있는 파일 피커(`file_picker`) | 부모의 유효 범위 | host가 첫 paint 전에 부모 선언 종류+target을 해석 |

`Workspace` 스코프 팝업은 워크스페이스를 옮기면 **그리지 않는다** — 상태는 그대로 남아 있어
돌아오면 보던 화면이 그대로 복원된다. 그 상태의 수명은 스코프가 아니라 `on_close` 가 정한다.

### scrim 의 범위

scrim은 팝업이 속한 범위를 덮는다(이 문서). Surface 범위는 해당 surface의 보더까지 포함하고 인접 surface·사이드바·탭바·상태바는 제외한다. Pane·Tab 범위는 `PopupManager::scope_rect`가 `LayoutContext::pane_rects`에서 찾은 사각형이다.

모서리 형태는 범위 대상에 맞춘다. 현재 surface의 반경은 0이며 코드는 범위와 관계없이 반경 0과 같은 `scrim()` 색을 사용한다. 창·workspace 범위, 그 프레임에서 범위를 찾지 못한 경우, surface 선언에 대상이 없는 호환 경로는 창 전체를 덮는다.

같은 범위의 팝업이 여럿이어도 scrim은 한 번 그린다. 창 scrim이 있으면 내부 surface scrim은 생략한다. 같은 알파를 중복 적용해 특정 영역만 더 어두워지는 것을 막는다.

**어느 팝업이 scrim 을 까는가는 범위가 아니라 id 로 정한다.** `search_bar` 는 `Surface`
범위를 쓰지만 anchored + scrim-less 갈래이므로 scrim 이 없다([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md)).
현재 scrim 을 까는 host 팝업: `remote_tool` · `remote_attach` · `command_palette` ·
`port_scanner` · `convert_surface` · 전송 진행/오류. plugin 팝업은 예외 없이 깐다.

어둡게 하는 것과 입력을 막는 것은 다른 일이다 — scrim 은 dim 만 하고, Esc 와 바깥 클릭의
순서는 위 §수명 계약이 정한다.

### plugin popup 의 스코프

plugin popup(`[[contributes.popup]]`)은 매니페스트 `scope` 로 범위의 **종류**만 선언한다 —
`window`(기본) 또는 `surface`. 대상 surface 는 popup 을 여는 host 진입점이 인스턴스에
바인딩한다(이 문서).
가시성·경계는 위 표와 같은 판정 함수로 같은 frame 의 `LayoutContext` 에서 정한다.

이벤트 trigger popup 의 대상은 host 가 확인한다. 매니저는 surface 트리를 모르므로 `surface`
선언 + 대상이 있는 이벤트 popup 을 대기열에 두고, 앱이 같은 frame 에 꺼내 대상 surface
(webview 또는 egui-mesh plugin surface)의 소유 plugin 을 대조한다. 발행자 대조는 매니저가
대상 유무와 관계없이 먼저 한다 — 다른 plugin 이 발행한 이벤트로는 `surface` 선언 popup 을
창 범위로도 열지 않는다. 이 발행자 검사는 `surface` 선언 popup 에만 적용한다. `window` 선언
popup 은 다른 plugin 이 발행한 이벤트로도 지금처럼 열린다 — 소유를 확인할 대상이 없고, 신뢰
경계는 surface 를 지목하는 popup 에 둔다. 거절하면 popup 을 열지 않고 경고 로그만 남긴다. 발행 자체는 성공한다. `window` 로 낮춰 열지 않는다 — 지목한 대상이 조용히
무시되고 창 scrim 이 다른 surface 를 덮기 때문이다.

| 진입점 | 바인딩되는 대상 |
|--------|----------------|
| 변환 입력 popup(`convert_input_popup`) | 제자리 변환이면 그 surface, 새 탭이면 여는 시점의 focus surface |
| 도구 메뉴 popup | 여는 시점의 focus surface |
| 이벤트 trigger popup | `surface` 범위 이벤트(`EventScope::Surface`)의 payload `surface_id`. 대상이 그 popup 을 선언한 plugin 의 surface 가 아니거나, 다른 plugin 이 발행한 이벤트면 열지 않는다 |
| plugin 이 IPC 로 연 popup, 대상 없는 이벤트 | 없음 — 선언이 `surface` 여도 `window` 로 뜬다. 대상 없는 이벤트도 다른 plugin 이 발행했으면 열지 않는다 |

`surface` 범위 popup은 앵커의 가운데 기준도 그 surface 영역이다. 범위가 안 보이는 frame 에는
셸·콘텐츠 합성·히트테스트 rect·Esc·바깥 클릭·키 게이트 어디에도 들어가지 않는다 — 보이지
않는 rect 가 클릭을 삼키지 않는다. 인스턴스는 살아 있어 범위가 다시 보이면 그대로 복원된다.
scrim 도 그 범위를 덮는다(위 §scrim 의 범위). 보이는 범위의 안쪽(테두리 inset 을 뺀 영역)이
선언 크기(`size_hint`)보다 좁거나 낮으면 그 frame 에는 `window` 범위로 연다 — 범위에 맞춰
줄이면 버튼이 잘려 확정할 수 없기 때문이다. 배치·scrim·히트테스트가 모두 이 실효 범위를 쓰고,
surface 가 다시 넓어지면 surface 범위로 돌아온다. 숨은 범위는 대체하지 않고 계속 숨긴다.
창보다 큰 셸은 창 크기로 줄어든다 — plugin 콘텐츠는 GPU 합성으로 올라가 egui 레이어 클립이
그것까지 잘라 주지 않기 때문이다. 자식 파일 피커는 부모가 직전 frame 에 실제로 놓인 범위를
상속하므로, 부모가 창 범위로 대체되면 피커도 창 범위로 뜬다. 부모가 아직 그려지지 않았거나
숨었으면 선언 종류와 대상으로 정한다.

현재 `scope = "surface"` 선언: markdown `file-open` · `large-file-confirm`.

## Modal 과의 차이

| 항목 | Popup | Modal |
|------|-------|-------|
| 입력 차단 | 키보드: 포커스 시 / 마우스: 입력 계층 | 전역 독점 |
| 동시 열기 | 여럿 | 최대 1 |
| 위치 | 자유 이동 | 중앙 고정 |
| 구현 | PopupManager | 별도 OS 윈도우(View) |

## 관련

- [toast.md](toast.md) — 휘발성 알림 (별도 시스템)
- [banner.md](banner.md) — parent 상단 info+action 오버레이 (별도 시스템)
- [fullscreen-stage.md](fullscreen-stage.md) — 타이틀바 전체화면 버튼이 여는 창 전체 무대 (별도 시스템)
- [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md) — 팝업 추가 절차
- [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md) — Window/Modal/Popup/Toast/Banner 구분

## 복원 오류 안내의 포커스

[복구 원칙](../../principles/feedback-and-recovery.md)의 심각한 복원 실패는 앱 우측 상단 부근에 포커스를 가져가지 않는 팝업으로 알리는 대상이다. 복원 자료를 읽지 못한 안내는 PopupDef 창이 아니라 작업 영역 오른쪽 위 안내 열의 RestoreNotice로 그린다(`src/adapters/ui/restore_notice.rs`, 위젯 `tasty_ui_widgets::restore_notice`). 포커스를 가져가지 않고 scrim이 없으며 ×·Continue·안쪽 컨트롤의 Esc가 같은 응답이다. 중앙 부팅 오류 화면은 홈 사용 중·첫 workspace 생성 실패·App 생성·첫 화면 조립처럼 계속할 수 없는 실패에만 남는다. 일반 알림 상세 팝업은 사용자가 열어 읽는 별도 화면이다.
