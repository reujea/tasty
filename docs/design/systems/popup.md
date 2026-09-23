# 내부 팝업 시스템

Popup 은 View 내부에 존재하는 가상 창이다 — 터미널과 공존하며 포커스를 독점하지 않는다. 모든 내부 팝업은 **`PopupManager` + `PopupDef`** 로 관리된다(`src/adapters/ui/popup.rs`). 용어 구분은 [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md), 팝업을 *추가하는 법* 은 [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md). 이 문서는 시스템 *동작 모델* 이다.

## 8대 규칙

1. **타이틀 + 콘텐츠** — 상단 타이틀 영역(높이 토큰 고정) + 하단 콘텐츠.
2. **타이틀바** — 제목 중앙 정렬 + 우측 버튼군. 버튼은 항상 X(닫기, 호버 시 빨강)이고, **전체화면 무대를 선언한 popup**(`PopupDef.fullscreen_stage`)에만 그 왼쪽에 전체화면 버튼(디자인 `fit` 글리프, 호버 시 tooltip)이 하나 더 붙는다 — 선언하지 않은 popup 의 타이틀바는 X 위치까지 그대로다. 제목이 버튼군과 겹칠 만큼 길면 **버튼군 좌변** 기준 가용 폭으로 말줄임(`…`) 처리한다(상세 → [popup-implementation §타이틀 길이 처리](../../dev-guide/popup-implementation.md#타이틀-길이-처리-elide)). 전체화면 버튼을 누르면 [무대](fullscreen-stage.md)가 뜨고 **원본 popup 은 열린 채 남는다** — 무대에 올라가는 것은 이 popup 인스턴스가 아니라 같은 형상의 별개 콘텐츠다.
3. **드래그 이동** — 팝업이 선언한 **드래그 핸들**(`drag_handle`) 영역을 드래그로 팝업 전체 이동. 타이틀바 팝업은 타이틀 영역이 핸들(기본). 타이틀바 없는 headless 패널 팝업(`port_scanner`·`remote_tool`)은 뷰가 렌더 시점의 **실측 헤더 rect(전체폭 × 실제 높이)** 를 매니저에 보고해 헤더 영역 전체를 이동 핸들로 쓴다(정적 `DragHandle::Region` 띠는 open 첫 프레임 폴백). 헤더 텍스트 라벨은 비선택(`selectable_labels=false`)이라 글자 위에서도 드래그가 텍스트 선택으로 새지 않는다. **위젯 우선 중재**(`is_using_pointer`)로 핸들이 위젯(검색 입력·버튼)과 겹쳐도 위젯 클릭이 항상 우선되어 충돌하지 않는다(8번 입력 우선순위 참조).
4. **커서** — 드래그 핸들 위에서 grab 커서. 리사이즈 가능 팝업의 테두리에서는 엣지별 리사이즈 커서.
5. **배경 구분** — 팝업 배경은 `surface0`, 타이틀바는 `mantle` — 터미널 focused(검정)/unfocused 배경과 달라 위에 떠 있음이 시각적으로 구분된다.
6. **경계 제한** — 팝업의 어떤 부분도 스코프 밖으로 못 나간다. 리사이즈 시 자동 재배치.
7. **다중 + z-order** — 여러 개 동시 가능. 나중에 열리거나 클릭된 것이 앞. 겹친 영역의 마우스 이벤트는 최상단 팝업만 받는다(판정: [§Host ↔ Plugin popup z-order](#host--plugin-popup-z-order) 의 "마우스 소유권").
8. **리사이즈 + 입력 우선순위** — `resizable` 팝업은 테두리 8방향 드래그로 크기 조절(`min_size` 하한 + 스코프 경계 클램프). 입력 우선순위는 **(egui)위젯 > close·전체화면 버튼 > 리사이즈 엣지 > 드래그 핸들 > 콘텐츠** — 두 타이틀바 버튼은 매니저가 직접 페인팅한 같은 층이고, 둘 다 드래그 핸들(타이틀바)과 겹치므로 핸들보다 먼저 판정해야 버튼을 눌러 끌어도 팝업이 따라오지 않는다. 이동/리사이즈 START 는 콘텐츠 렌더 뒤 `is_using_pointer()` 게이트로 판정해, 위젯이 프레스를 가져간 프레임에는 발동하지 않는다(이동이 사실상 최후순위). 사용자가 리사이즈한 뒤에는 `sizer` 가 크기를 되돌리지 않는다(close 시 리셋).

(모든 색·치수는 Theme 토큰 — [theme.md](theme.md).)

## 구조

- **`PopupDef`** — 정적·데이터 지향 정의(id, title_key/title_fn, default_size/sizer, default_scope, close_on_outside_click, headless, sticky_focus, drag_handle, resizable, min_size, fullscreen_stage, draw_fn). 전부 `src/adapters/ui/popup/defs.rs::all_defs()` 에 모은다. 필드 상세 → [popup-implementation](../../dev-guide/popup-implementation.md).
- **`PopupManager`** — 공통 동작(z-order, 드래그, 리사이즈, 타이틀바, clamp, 포커스) 중앙 관리. `register_def` / `open*` / `close` / `draw`.
- **`PopupState`** — 개별 인스턴스 상태(id, title, pos, size, open, focused, scope, dragging/resizing, size_user_overridden).
- **범용 render 루프** — `src/adapters/ui/popup/frame.rs::draw_popup_layer`(등록된 모든 `PopupDef` 순회 + close 경로별 `on_close` 훅 drain). toast/banner/modifier-hint/tutorial 오버레이 체인은 개념이 다르므로(ADR-0024) `src/adapters/ui/overlay.rs` 로 분리돼 있다. 진입점 `src/adapters/ui.rs::draw_popups` 가 둘을 z-order 순서로 호출한다.

등록된 팝업은 `all_defs()` 가 단일 출처다(예: `notifications`, `convert_surface`, `rename`, `search_bar`, `tools_menu` …). 새 팝업 = 테이블 항목 1개 + draw 함수 하나. 단, plugin 이 소유하는 팝업(예: markdown 파일열기·large-file 확인)은 host `PopupDef` 가 아니라 plugin 매니페스트 `[[contributes.popup]]`(egui-mesh) 로 등록된다.

## Host ↔ Plugin popup z-order

Plugin 이 egui-mesh 로 그리는 popup(예: markdown 파일열기, `src/plugin_bridge/popup_render.rs`)은 host `PopupManager` 소속이 아니지만 규칙 7 의 "나중에 열리거나 클릭된 것이 앞" 을 host popup 과 함께 지킨다. 양쪽 모두 자신의 open/click 시점에 공유 전역 시퀀스(`tasty_host_plugin::next_popup_z_seq()`)에서 번호를 받아 기록하고(host: `PopupState.z_seq`, plugin: `PopupInstance.z_seq`), 매 프레임 두 진영의 열린 popup 중 최댓값끼리 비교해 이긴 쪽을 위로 강제한다(`src/gfx/gpu/egui_bridge.rs::host_popup_should_render_on_top`).

- **Shell(배경/테두리/제목)**: 둘 다 raw `ctx.layer_painter()` 로 그려 `Areas::order` 에 자연 등록되지 않는다([input-layer.md (c)](../../architecture/input-layer.md) 참고) — `enforce_host_plugin_popup_z_order` 가 진 쪽을 이긴 쪽의 `ctx.set_sublayer()` 자식으로 강제 편입해 순서를 고정한다.
- **Content(plugin 전용 GPU mesh)**: plugin popup 콘텐츠는 별도 `wgpu::Renderer` pass(`render_egui_mesh_popups`)로 host egui pass(`render_egui_pass`)와 독립 합성된다 — `render_egui_pass_and_mesh_popups`(`src/gfx/gpu/render_pass.rs`)가 같은 승패 결과로 두 pass 의 호출 순서를 뒤바꾼다. plugin popup 은 콘텐츠가 shell 보다 먼저 그려지는 경우에도 자기 shell 이 자기 콘텐츠를 덮지 않도록, shell 배경을 콘텐츠 영역을 제외한 4분할 사각형으로 그린다(`paint_shell_background_excluding_content`).
- **마우스 소유권(규칙 7 후반)**: "겹친 영역의 마우스 이벤트는 최상단 팝업만 받는다" 는 렌더 순서와 별개로 `src/adapters/ui/popup/occlusion.rs::point_ownership` 가 판정한다. 한 좌표에 대해 `Mine`(내 rect 안 + 위에 아무도 없음) / `OccludedByHigher`(나보다 z 가 높은 popup 이 덮음) / `OutsideAll`(어떤 popup 에도 안 속함) 3-상태를 내고, host/plugin 양쪽이 같은 함수를 쓴다 — click-to-front 는 `Mine` 일 때만, outside-click dismiss 는 `OutsideAll` 일 때만 일어난다. 그래서 위에 열린 popup 안을 클릭해도 아래 popup 이 "바깥 클릭" 으로 닫히거나 앞으로 튀어나오지 않는다. host popup 은 hover/close 버튼/리사이즈/드래그 히트테스트 전체가, plugin popup 은 focus-bump·dismiss·포인터 이벤트 forward 가 이 판정을 거친다. 아래 2-그룹 제약과 달리 이 판정은 **popup 쌍마다 z_seq 를 직접 비교**하므로 host↔plugin·plugin↔plugin 을 모두 정확히 가른다.
- **판정 재료의 방향별 신선도**: plugin 판정이 보는 host rect 는 같은 프레임 값이다(`draw_popups` 가 `draw_plugin_popups` 보다 먼저 돈다). 반대로 host 판정이 보는 plugin 셸 rect(`AppState.plugin_popup_hittest`)는 **1 프레임 stale** 이다 — 방금 닫힌 plugin popup 이 바깥 클릭 한 번을 더 삼킬 수 있지만, 반대 방향 오판(가려진 popup 이 잘못 닫히는 것)보다 회복이 쉬운 쪽을 택한 결과다.
- **범위**: host popup 묶음 대 plugin popup 묶음의 2-그룹 비교만 지원한다. plugin popup 이 여러 개 열려 있을 때 그들끼리의 상대 순서는 z_seq 로 정렬돼 콘텐츠(GPU mesh push 순서)에는 반영되지만, shell 레이어끼리는 `set_sublayer` 의 1단 들여쓰기 제약(아래 [input-layer.md (d)](../../architecture/input-layer.md)) 때문에 서로 엮이지 않는다 — host 묶음과의 상대 위치만 보정 대상이다.

## 수명 계약 (open → close → 뒷정리)

팝업이 닫히는 경로는 6개다: draw_fn 이 `PopupAction::Close` 반환 / X 버튼·바깥 클릭(`PopupManager::draw` 내장 포인터 처리) / `UiIntent::ClosePopup` / 이미 열린 채로의 `UiIntent::TogglePopup` / App 계층의 직접 `close()` 호출 / debug IPC(`debug.host_popup.close`, 구조적으로 `ClosePopup` 과 동일). 이 6개 전부가 **`PopupManager::close()`** 로 수렴한다 — `PopupState.open` 을 `false` 로 세팅하는 유일한 지점이다.

상태를 가진 팝업(draft 버퍼, 대상 id 등)은 draw_fn 안에서만 정리하면 안 된다 — draw_fn 을 거치지 않는 나머지 5개 경로에서 정리가 샌다. 대신 **`PopupDef.on_close`** 훅을 선언한다: `close()` 가 대상을 `closed_queue` 에 쌓고, `popup::frame::draw_popup_layer` 가 다음 draw 시점에 이 큐를 drain 하며 등록된 훅을 정확히 한 번 호출한다(재진입 지원 — 훅이 다른 popup 을 닫으면 그 close 도 같은 drain 안에서 처리되고, 상호 재오픈 등 논리 오류에 대비해 라운드 상한을 둔다). 상태가 없거나(`notifications`) 남아도 무해하다고 판단했으면(`tutorial_topics`) `on_close: None` 옆에 근거를 남긴다.

절차·필드 상세는 [popup-implementation §닫힘 정리](../../dev-guide/popup-implementation.md#닫힘-정리).

### plugin popup ↔ host popup 부모-자식

plugin 이 `file_picker.trigger`([ADR-0058](../../adr/0058-plugin-triggered-host-popup-async-ack-push.md))로 host popup 을 열 때 `owner_popup_instance` 로 **자기 popup instance_id 를 자진 신고**하면, host 는 그 값을 자식 쪽 요청자 기록에 보관해 두 popup 을 스택으로 다룬다([ADR-0084](../../adr/0084-plugin-triggered-host-popup-ownership.md)). 신고하지 않으면(예: popup 밖 surface 위젯에서의 호출, Tools 메뉴 진입) 지금까지처럼 관계 없는 단독 popup 이다.

관계가 성립하면:

- **범위 상속·숨김 보존** — host는 요청자와 부모 instance를 대조해 선언 종류+target의 유효 범위를 자식 파일 피커에 적용한다([ADR-0278](../../adr/0278-child-file-picker-inherits-parent-scope-and-preserves-hidden-work.md)). 부모가 숨으면 자식도 paint/hit/Esc/키 게이트에서 빠지고, 돌아오면 draft·선택·pending 요청을 그대로 이어간다. 숨김은 close를 발화하지 않는다. Window 부모와 owner 없는 피커는 창 범위다.
- **스택 유지** — 자식이 열려 있는 동안 부모는 outside-click dismiss 대상에서 빠진다. 부모를 모달로 잠그는 것이 아니라 dismiss 목록에서만 제외한다(popup 은 포커스를 독점하지 않으므로).
- **Esc 소유권** — host/plugin 통틀어 그 프레임 최상단 popup **하나만** Esc 를 소비한다. Esc 를 한 번 누르면 스택이 한 단계 벗겨진다. host 쪽 판정은 `AppState.popup_escape_owner`(`popup::frame` 이 매 프레임 결정), plugin 쪽은 `popup_render` 가 같은 z 축으로 비교한다. **host popup 끼리의 Esc 중재는 범위 밖** — 각 view 가 자기 Esc 를 직접 소비하며, 현재 스택에 참여하는 `file_picker` 에만 게이트가 붙어 있다.
- **연쇄 정리** — 부모가 어떤 경로로 닫히든 자식 피커에 취소 결과가 채워져, 평소 result 경로 그대로 plugin 에 `cancelled: true` 가 전달되고 피커도 닫힌다. 고아 피커와 조용한 결과 유실이 생기지 않는다. 사용자가 이미 확정한 결과는 덮지 않는다.

소유 관계는 자식(요청자 기록) 한 곳에만 있다 — 부모 쪽 사본이 없어 둘이 어긋날 수 없다. host는 특정 plugin 이름/kind로 분기하지 않는다. 범위 상속에서는 요청자 plugin과 부모 소유자가 같은지만 확인한다.

## 발화 정책 (CRITICAL)

**Popup 은 사용자 행동(키보드 단축키 / 마우스 / 메뉴)에서만 발사된다.** release 의 시스템·에이전트·도메인 cascade 어느 경로도 popup 을 자동으로 띄울 수 없다([toast.md](toast.md) 와 동일 원칙, [identity](../../identity.md) 원칙 1).

- ✅ 단축키/마우스/메뉴 → `UiIntent::OpenPopup` 발화
- ✅ popup A 의 *사용자 액션* cascade → popup B (origin 전파)
- ❌ release 의 IPC/CLI/Plugin 에서 popup 발화
- ❌ 시스템 조건(PTY 종료·시간 경과)으로 자동 popup — 대신 *Domain Intent 로 데이터만 변경*(NotificationStore push 등)하고 UI 가 수동 표시
- ✅ debug 의 `debug.popup.*` — *사용자 입력 재현* 한정 ([debug-ipc](../../dev-guide/debug-ipc.md))

**타입 차원 강제**: Core/Domain 핸들러는 `UiIntent`(`OpenPopup`/`ClosePopup`/`TogglePopup`)를 발화하는 메서드를 갖지 않는다 — GUI adapter(단축키 핸들러·메뉴 콜백·popup draw)에서만 발화 가능. `state.popups.open*` 직접 호출도 금지(Intent 경유). 디스패치 상세는 [`design/flows/action-dispatch.md`](../flows/action-dispatch.md).

## 포커스

팝업은 **포커스 상태**를 가진다. 보이는 범위에 포커스된 팝업이 있으면 키보드 입력이 터미널로 안 간다. 여러 팝업이 겹쳐 있을 때 Esc 를 누가 받는지는 위 [§수명 계약](#plugin-popup--host-popup-부모-자식)의 "Esc 소유권" 을 따른다. 클릭 → 포커스(다른 팝업 언포커스), 바깥 클릭 → 전체 언포커스(터미널 복귀), 닫기 → 자동 언포커스. `PopupManager::has_focused()`로 확인한다. 이 조회는 최신 draw의 범위 가시성을 포함한다. 숨은 popup의 포커스 의도는 보존하며 다른 화면의 클릭은 그 의도를 바꾸거나 popup을 dismiss하지 않는다. 숨김 때 드래그·리사이즈 캡처는 해제한다. native WebView를 가리는 overlay 판정도 열린 popup 중 보이는 것만 센다.

Modal 의 전역 입력 독점과 다르다 — 팝업 포커스는 **키보드만** 차단하고, 마우스는 [입력 계층](../../architecture/input-layer.md)에 따라 팝업이 소비한다.

**plugin egui-mesh popup 도 같은 차단을 받는다.** 다만 그 popup 은 host `PopupManager` 소속이 아니라 `has_focused()` 로 잡히지 않으므로, 렌더 프레임이 `AppState.plugin_popup_open` 캐시를 채우고 게이트가 그것을 읽는다(키/IME 게이트가 있는 winit 핸들러는 `PluginManager` 에 접근할 수 없다). 판정은 `AppState::keyboard_overlay_open()` 하나로 모여 있다 — egui 로 키/IME 를 들여보내는 게이트와 터미널 포워딩을 막는 게이트가 같은 식을 각자 계산하면 이중 처리(양쪽 다 처리)나 입력 유실(양쪽 다 안 처리)이 생긴다. IME 라우팅과 plugin surface 단축키 게이트도 같은 술어를 쓴다. 예외는 `set_ime_allowed` 판정 하나로, plugin popup 은 host egui 위젯이 없어 IME 를 끄면 popup 안에서 조합 입력을 못 하게 되므로 제외한다. 그 조합을 popup 으로 나르는 것은 이 게이트가 아니라 `collect_mesh_popup_input` 이며(이벤트는 이미 host egui ctx 에 들어가 있다), 후보창 위치는 plugin 이 되돌려준 값으로 정한다 — 둘 다 [egui-mesh-channel](../../dev-guide/egui-mesh-channel.md).

캐시는 렌더 프레임에 갱신되므로 popup 이 열린 **직후 최대 1 프레임** 늦게 반영된다 — 그 프레임에 사용자가 이미 키를 누르고 있을 수는 없어 실사용 영향은 없다.

### sticky_focus

`true` 면 바깥 클릭에도 키보드 포커스가 유지된다(닫기로만 해제). 마우스는 터미널에 정상 전달. 검색 바처럼 *키보드는 항상 자기가 받되 터미널 마우스 조작(스크롤/선택)은 허용* 해야 하는 오버레이용.

### close_on_outside_click

`false`(기본): 바깥 클릭 시 언포커스만(팝업 유지, 예: 알림 패널). `true`: 바깥 클릭 시 닫음(예: surface 타입 전환).

## 스코프

팝업은 소속 범위(`PopupScope`)를 가지며 가시성·경계가 결정된다. enum: `Window` / `Workspace(usize)` / `Pane(u32)` / `Tab(u32, usize)` / `Surface(u32)`.

| 스코프 | 가시성 | 경계 clamp | scrim 이 덮는 rect |
|--------|--------|-----------|--------------------|
| Window | 항상(워크스페이스 전환 무관) | 윈도우 | 윈도우 |
| Workspace | 해당 워크스페이스 활성 시 | 워크스페이스 영역 | 윈도우 |
| Pane | 해당 pane 보일 때 | pane | pane |
| Tab | 해당 탭이 활성 탭일 때 | 탭 소속 pane | 탭 소속 pane |
| Surface | 해당 surface 보일 때 | surface **안쪽 8pt** | surface |

`PopupManager::draw()` 가 `LayoutContext` 를 받아 스코프별 가시성 필터 + clamp rect 를 결정한다.

`Workspace` 스코프의 clamp 는 실제로는 윈도우 전체다 — 워크스페이스가 윈도우를 통째로
차지하므로 둘이 같은 사각형이다. 따라서 이 스코프가 실질적으로 결정하는 것은 **가시성**이다.

**스코프는 `PopupDef` 에 못 박히지 않는다.** `Workspace(usize)` / `Pane(u32)` 처럼 대상을
런타임에야 아는 스코프는 `default_scope` 에 안전한 기본만 두고, 여는 쪽이
`OpenPopupMode::WithScope(scope)` 로 실제 값을 주입한다. 현재 소비자:

| 팝업 | 스코프 | 여는 쪽이 주입하는 값 |
|------|--------|----------------------|
| [DAG 목록](../../features/agent-collaboration/screens/dag-list-popup.md) (`dag_list`) | Workspace | 여는 시점의 활성 workspace 인덱스 |
| 변환(`convert_surface`) · 검색바(`search_bar`) | Surface | 포커스 surface id |
| 부모가 있는 파일 피커(`file_picker`) | 부모의 유효 범위 | host가 첫 paint 전에 부모 선언 종류+target을 해석 |

`Workspace` 스코프 팝업은 워크스페이스를 옮기면 **그리지 않는다** — 상태는 그대로 남아 있어
돌아오면 보던 화면이 그대로 복원된다. 그 상태의 수명은 스코프가 아니라 `on_close` 가 정한다.

### scrim 의 범위

scrim 이 덮는 rect 는 그 팝업이 소속된 범위의 rect 다([ADR-0300](../../adr/0300-the-scrim-covers-the-popups-scope-not-always-the-window.md)).
`Surface` 범위면 그 칸 하나이고, 경계는 그 surface 의 **보더를 포함**하며 인접 surface ·
사이드바 · pane 탭바 · 상태바는 **제외**한다. `Pane`·`Tab` 범위면 그 pane 의 rect 다 — 현재
동작이다: `src/adapters/ui/popup/draw.rs::PopupManager::scope_rect` 가 두 범위 모두
`LayoutContext::pane_rects` 에서 찾은 rect 를 내고, scrim 은 그 rect 에 깔린다. radius 는 범위
대상 자신의 radius 를 따른다 — 오늘의 셸에서 surface radius 는 0 이라 직각이고, 코드는 범위와
무관하게 radius 0 으로 칠한다. 알파는 한 벌이다(`--tasty-scrim-bg`) — 범위 갈래가 몇이든 값을
나누지 않는다(host·plugin 두 경로 모두 범위와 무관하게 같은 `scrim()` 한 값으로 칠한다).
바인딩이 없으면(창·워크스페이스 범위, 범위 rect 를 그 frame 에 못 찾은 경우, 그리고 선언이
`surface` 여도 대상이 없는 호환 경로) 창 전체다.

**scrim 은 범위당 한 번 깔린다.** 같은 범위에 팝업이 여럿 떠도(부모 팝업과 그것이 연 자식
파일 피커) 한 번이고, 창 scrim 이 있으면 그 안의 surface scrim 은 안 깐다 — 넓은 쪽이
이긴다. 알파가 한 벌이라 두 번 칠하면 그 자리만 두 배로 어두워지기 때문이다.

**어느 팝업이 scrim 을 까는가는 범위가 아니라 id 로 정한다.** `search_bar` 는 `Surface`
범위를 쓰지만 anchored + scrim-less 갈래이므로 scrim 이 없다([ADR-0254](../../adr/0254-floating-surface-shadow-scope-rule.md)).
현재 scrim 을 까는 host 팝업: `remote_tool` · `remote_attach` · `command_palette` ·
`port_scanner` · `convert_surface` · 전송 진행/오류. plugin 팝업은 예외 없이 깐다.

어둡게 하는 것과 입력을 막는 것은 다른 일이다 — scrim 은 dim 만 하고, Esc 와 바깥 클릭의
순서는 위 §수명 계약이 정한다.

### plugin popup 의 스코프

plugin popup(`[[contributes.popup]]`)은 매니페스트 `scope` 로 범위의 **종류**만 선언한다 —
`window`(기본) 또는 `surface`. 대상 surface 는 popup 을 여는 host 진입점이 인스턴스에
바인딩한다([ADR-0273](../../adr/0273-plugin-popup-declares-a-scope-kind-and-the-host-binds-the-target.md)).
가시성·경계는 위 표와 같은 판정 함수로 같은 frame 의 `LayoutContext` 에서 정한다.

| 진입점 | 바인딩되는 대상 |
|--------|----------------|
| 변환 입력 popup(`convert_input_popup`) | 제자리 변환이면 그 surface, 새 탭이면 여는 시점의 focus surface |
| 도구 메뉴 popup | 여는 시점의 focus surface |
| plugin 이 IPC·이벤트로 연 popup | 없음 — 선언이 `surface` 여도 `window` 로 뜬다 |

`surface` 범위 popup 은 앵커의 가운데 기준도 그 surface 영역이다. 범위가 안 보이는 frame 에는
셸·콘텐츠 합성·히트테스트 rect·Esc·바깥 클릭·키 게이트 어디에도 들어가지 않는다 — 보이지
않는 rect 가 클릭을 삼키지 않는다. 인스턴스는 살아 있어 범위가 다시 보이면 그대로 복원된다.
scrim 도 그 범위를 덮는다(위 §scrim 의 범위). 셸이 범위보다 크면 범위 크기로 줄어든다 —
plugin 콘텐츠는 GPU 합성으로 올라가 egui 레이어 클립이 그것까지 잘라 주지 않기 때문이다.

현재 `scope = "surface"` 선언: markdown `file-open`.

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
