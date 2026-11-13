# Debug 전용 IPC

## 원칙과 현재 제공 범위

[주체별 원칙](../principles/control-and-data.md)은 release에서 에이전트가 Tasty 포커스를 빼앗는 동작을 제한한다. 아래 debug 전용 목록은 현재 구현의 제공 범위다. 포커스와 무관한 HTML 스크립트 허용·관찰 기능까지 보안 이유로 제한하는 부분은 원칙의 미충족이며, 입력 재현과 별개로 정합한다. debug 전용 모듈의 cfg 분리와 격리된 검증 절차는 유지한다.


현재 구현은 사용자 키·마우스·단축키 입력 재현과 내부 렌더 덤프를 debug 전용으로 제공한다. 포커스와 무관한 기능까지 같은 이유로 제한해야 한다는 제품 원칙은 아니다. 해당 모듈은 cfg(debug_assertions)로 분리되어 있다.

현재 구현의 경로 구분:

- **에이전트 기능 → release IPC**: surface/tab/workspace 생성·조회·닫기, 클립보드, 알림, 메타데이터 등 — 에이전트가 *자기 작업* 을 하려고 필요한 동작. 예: **`surface.completion`**(surface highlight 발동) — 에이전트가 "이 surface 확인 필요" 라고 *자기 작업 결과를 보고* 하는 것이라 PushNotification 과 동류 → **release 정식**. 반면 그 highlight 를 *포커스를 주입해* 해제하는 경로는 사용자 입력 재현이라 debug 격리 대상(상태 해제 자체는 `surface.attention.clear` 로 release 에 있다 — 포커스를 옮겨서 해제하는 방식만 debug 격리 대상이다).
- **디버그 기능 → debug 전용**: 사용자 단축키/마우스로 트리거되는 동작의 자동 재현(키/마우스 주입, 단축키로만 여는 popup/도구 메뉴의 IPC 트리거), 그리고 렌더러·파서 검증용 저수준 덤프.

## 라우팅

JSON-RPC 라우터는 공통 요청 게이트(`src/adapters/ipc/handler/gate.rs`)의 권한·cap·rate 검사와 사용량 집계를 마친 요청만 `src/adapters/ipc/handler.rs::handle_checked_request`에 넘기고, 핸들러 탐색은 그 안의 `dispatch_routed` 가 한다:

`dispatch_routed`는 `RequestScope`와 `IntentOutbox`로 요청별 결과를 모아 적용하고,
공통 엔진 핸들러가 처리하지 않은 요청을 GUI의 `route_window_handler`, debug의
`route_debug_handler` 순서로 전달한다. 두 분기는 각각 feature/debug 조건으로 제한한다.

엔진 debug 진입점은 `src/adapters/ipc/handler.rs`의 `route_debug_handler`다.
입력 재현 구현인 `src/adapters/ipc/handler/debug.rs`는 파일 단위 debug 조건을 사용한다.
창·플러그인 실행이 필요한 요청은 아래 App-level debug adapter가 처리한다.

`route_debug_handler` 함수 자체가 `#[cfg(debug_assertions)]` 라 release 바이너리엔 분기 한 줄과 함수가 모두 사라진다. release 에서 debug 메서드를 부르면 `-32601`(`method_not_found`)로 떨어진다 — `src/adapters/ipc/handler.rs`의 `unrouted_for_external_caller` 는 **등록된 이름**이면 `-32017`(이 빌드 조합에 dispatch 팔이 없음)을, plugin 전용이면 `-32016` 을 답하지만, debug 메서드 표(`DEBUG_METHODS`)가 release 에서는 빈 표라 debug 메서드는 등록된 이름이 아니다. 그래서 모르는 이름과 같은 답이 된다(release 헤드리스 인스턴스에서 `debug.info` · `debug.popup.open` · `surface.raw_key` 가 전부 `-32601`, 2026-09-22 실측). `-32017` 은 release 에도 등록된 이름이 이 조합에서 빠진 경우(gui 전용 메서드를 헤드리스에 부른 것 등)의 답이다.

### `MainViewState` 하나로는 부족한 debug 메서드는 App-level 에서 분기

`debug.event_bus.*` / `debug.extension.invoke_hook` / `debug.popup.*` 는 `route_debug_handler` 를 거치지 않는다 — `MainViewState` 가 `PluginManager` 를 들고 있지 않기 때문이다. 이들은 `App` 레벨의 `ipc_step_debug`(`src/app/ipc/debug_methods.rs`)에서 `plugin_manager` 를 직접 호출한다.

`debug.fullscreen.*` 도 같은 곳에서 처리되지만 이유는 또 다르다 — **창을 골라야 하기 때문**이다. `route_debug_handler` 는 `MainViewState`(=`MainView` 하나)만 받아 다른 창을 볼 수 없는데, 전체화면 무대는 창 단위 상태라([fullscreen-stage](../design/systems/fullscreen-stage.md)) `window_id` 로 대상을 지목하지 못하면 "창 2 개에 각각 무대를 띄운 뒤 한쪽만 닫는다" 같은 시나리오 자체가 구동 불가다. `self.view.views` 순회는 App 레벨에서만 가능하다. `window_id` 해석은 `ui.screenshot` 과 동형 — 지정하면 그 창, 미지정이고 창이 하나면 그 창, 여럿이면 `-32000` 에러다. 포커스된 창으로 조용히 폴백하지 않는다([focus](../design/policies/focus.md)).

`debug.settings.open` 도 같은 `ipc_step_debug` 에서 처리되지만 이유는 또 다르다 — 설정 모달은 `AppEvent::OpenSettings`(event-loop proxy → `open_settings_modal`) 로만 열리는 **별도 winit 윈도우**라, `MainViewState` 핸들러가 아니라 `App` 의 `self.view.proxy` 가 필요하다(`window.create` 와 동일 패턴). 사용자 단축키/버튼 클릭과 같은 진입점을 그대로 호출하므로, 모달 생성 이후에는 일반 동작과 같은 경로를 따른다. `tab`·`subtab` 인자는 설정 창이 없을 때만 `App.pending_settings_tab`·`pending_settings_subtab`(debug 전용 필드)에 1회성으로 실려 `open_settings_modal` 이 소비한다. 설정 창이 이미 열려 있으면 예약하지 않고 그 창의 상태에 바로 적용하며, 다른 모달이 열려 있으면 거절해 소비되지 않을 예약을 남기지 않는다. 두 경로는 같은 적용 함수(상위 탭 다음 하위 탭)를 쓴다. 처리는 `src/app/ipc/debug_settings.rs` 에 있다.

## 메서드 목록

debug 메서드는 모두 `local_only()` — plugin caller 는 호출 불가, CLI/network IPC(로컬)만 가능.

| method | params | 설명 |
|--------|--------|------|
| `ui.state` | `{}` | 현재 UI 상태. 요청과 실제 모달 표시를 구분하는 필드, 워크스페이스·탭 정보는 아래 [응답 해석](#uistate-응답-해석)을 참고한다 |
| `debug.info` | `{}` | 실행 중 인스턴스 debug 정보 |
| `debug.cell_info` | `surface_id, row, col` | 셀에 저장된 속성(텍스트, fg/bg, bold/italic/underline …). 그려진 결과가 아니다 |
| `debug.screen_attrs` | `surface_id, row` | 한 행 전체 셀 속성 |
| `debug.glyph_color` | `surface_id, row, col, bg_mode?` | GPU 렌더러가 실제로 push 하는 글리프 bg/fg RGBA (renderer 색 해석 검증) |
| `debug.feed_bytes` | `surface_id, bytes(hex)` 또는 `text` | VTE 바이트를 PTY 우회로 터미널에 직접 주입(파서/렌더 테스트) |
| `debug.gpu.stall` | `ms` | 다음 프레임의 `present` 직전을 `ms` 밀리초 블로킹(1 회). 이벤트 루프 stall 재현 — CLI 는 `tasty debug gpu-stall --ms N` |
| `debug.inject_mouse` | `surface_id, row, col, button?, event_type?` | SGR mouse(1006) 시퀀스로 마우스 이벤트 주입 † |
| `debug.inject_key` | `surface_id, bytes(hex)` 또는 `text` | 키 이벤트 주입 † |
| `debug.inject_window_mouse` | `surface_id?`, `fx?`/`fy?`(기본 0.5, 창 정규화 좌표), `event_type?`, `button?`, `scroll_dx?`/`scroll_dy?`, `unit?`(기본 `line`) | winit 레벨 마우스 이벤트 주입 — 포커스된 창에 작용한다. 스크롤 단위는 아래 [휠 주입의 단위](#휠-주입의-단위-unit) |
| `debug.inject_egui_mouse` | 위와 같음, `unit?` 기본 `point`, `modifiers?`(`shift`·`ctrl`·`alt`·`command` 배열) | egui 레벨 마우스 이벤트 주입 — winit 환산 경로를 건너뛰고 egui 입력에 직접 넣는다. `modifiers` 를 주면 egui 가 누른 것으로 보는 수식 키를 그 값으로 바꾸고 다음 `modifiers` 주입까지 유지한다(빈 배열은 모두 뗀다, 생략하면 그대로 둔다). CLI 는 `--modifiers shift,ctrl` · `--modifiers none`. `window_id?` 를 주면 그 창(메인 또는 설정·Preset·Plugins)의 egui 입력에 넣고, 좌표는 **그 창 전체에 대한 정규화 좌표**(`fx`/`fy`)만 받는다. 보조 창에는 surface 가 없으므로 `surface_id` 와 함께 주면 `-32602` 로 거절한다(CLI 는 `--window-id` 와 `--surface` 를 함께 받지 않고, `TASTY_SURFACE_ID` 로 채우지도 않는다). 응답에 `window_id` 를 싣고, 없는 창은 `-32602` |
| `debug.inject_egui_key` | `key?`(기본 `Escape`), `pressed?`(기본 `true`, CLI `--pressed false` 로 뗌), `window_id?` | egui 레벨 키 이벤트 주입. 매핑할 수 없는 키 이름이면 `injected:false`. `window_id` 는 `debug.inject_egui_text` 와 같은 규칙이다(그 창의 egui 입력, 응답에 `window_id`, 없는 창 `-32602`, 생략하면 포커스된 메인 창) |
| `debug.inject_egui_text` | `text`(필수, 문자열), `window_id?` | egui 레벨 **문자** 이벤트 주입 — 포커스된 `TextEdit`(command palette 쿼리 등)에 글자를 넣는다. `window_id` 를 주면 그 창(메인 또는 설정·Preset·Plugins 같은 보조 창)의 egui 입력에 넣고 응답에 `window_id` 를 싣는다. 없는 창이면 `-32602`. 생략하면 포커스된 메인 창이다. 아래 [문자 주입은 키 주입과 다른 채널이다](#문자-주입은-키-주입과-다른-채널이다) |
| `debug.selection` | `{}` | focused window 의 로컬 텍스트 선택 상태 read-only 덤프(`present`·`surface_id`·`mode`·`dragging`·`empty`·`anchor/cursor/start/end{col,row}`). 마우스 라우팅 회귀 net 의 관찰면 — 순수 관찰(사용자 상태 불변) |
| `debug.pending_menu` | `{}` | 대기 중 컨텍스트 메뉴 read-only 덤프(`present`·`kind`·`surface_id?`). live pending 우선, 없으면 주입 포획본(`debug_captured_menu`). 응답(`debug.native_menu.answer`)으로 끝난 메뉴는 남지 않아 `present:false`다. 우클릭 라우팅 회귀 관찰용 |
| `debug.native_menu.answer` | `menu`(필수) + `item` · `label` · `dismiss: true` 중 하나 | 포커스된 창이 **다음에 여는 `menu` 종류의** native 메뉴 하나를 OS 팝업 없이 지정 항목을 고른 것으로 끝낸다(`menu`는 `debug.pending_menu`의 `kind`와 같은 이름, `item`은 항목 id, `label`은 표시 문구, `dismiss`는 고르지 않고 닫기). 다른 종류의 메뉴에는 쓰지 않는다. 응답은 그 종류의 메뉴가 열릴 때까지 남아 있고 새 응답이 덮어쓴다. 결과는 `armed: true`와, 덮어쓴 응답이 있었으면 그 종류(`replaced`, 없으면 `null`)다. 아래 [native 메뉴 항목 선택 재현](#native-메뉴-항목-선택-재현-debugnative_menuanswer) |
| `debug.pending_move` | `{}` | 포커스된 창의 이동 대기 슬롯 read-only 덤프(`present`·`kind` = `surface`/`tab`/`pane`·`id`) |
| `debug.window.list` | `{}` | 메인 창과 보조 창(설정·Preset·Plugins·종료 확인)을 모두 나열한다. release `window.list` 는 메인 창만 다룬다([스크린샷 방법](../ai-verification/screenshot-methods.md)). 응답 `windows[]` 는 `window_id`·`kind`(`main`·`settings`·`preset`·`plugins`·`quit`)·`title`(winit 이 제목을 돌려주는 플랫폼에서만, X11·Wayland 는 `null`)·`inner_size`(물리 px `width`·`height`)·`inner_position`(창 내용 영역의 화면 좌표, 물리 px `x`·`y`. 플랫폼이 알려 주지 않으면 `null` — Wayland)·`scale_factor`·`focused`·`modal` 이며 `window_id` 오름차순이다. `inner_position` 은 native WebView 처럼 주입 경로가 닿지 않는 곳을 OS 수준 입력으로 누를 때 화면 좌표를 계산하는 데 쓴다. 보조 창의 id 는 `window_id` 를 받는 debug 메서드(`debug.inject_egui_*`·`debug.toast`)와 `ui.screenshot` 에 넘기는 값이다. 조회만 한다. CLI 는 `tasty debug windows`. gui 빌드 전용 |
| `debug.focused_surface` | `{}` | 현재 포커스된 surface id read-only 덤프(`surface_id`, 없으면 null). `surface.list` 가 노출 않는 view-layer 포커스를 관찰 — click-to-activate 라우팅 회귀 net 용 |
| `debug.surface_rect` | `surface_id` | 활성 탭에 있는 surface 의 화면 사각형 read-only 덤프(`rect` = 물리 px `x`·`y`·`width`·`height`, 화면에 없으면 null, `scale_factor`). 터미널이 아닌 surface 도 같은 leaf rect 다. native WebView 창이 surface 를 채우는지 재는 기준값 |
| `debug.switch_workspace` | `index` (0-based) | 활성 워크스페이스 전환 — 사용자 포커스 조작 재현. **`index` 는 포커스된 창 안의 순번이라 포커스 독립성을 만족하지 않는다** — 사용자가 보고 있는 창에서 전환하는 것이 이 메서드의 뜻이므로 그것이 정답이다(`surface.ime_*` 와 같은 부류) |
| `debug.close_workspace` | `index` (0-based) | 워크스페이스 컨텍스트 메뉴 "Close workspace" 재현 — 워크스페이스 안 **모든** surface + closed_item 스냅샷까지 한 번에 닫는다. release `surface.close` 는 cascade 특성상 마지막 한 surface 만 정리하므로 "탭 많은 워크스페이스 통째 close" 비용([close-sequence](../architecture/close-sequence.md) `path="gui"`)은 이 메서드로만 재현된다. 마지막 workspace 는 거절(0개가 되면 다음 redraw 가 패닉). **`index` 는 포커스된 창 안의 순번이라 포커스 독립성을 만족하지 않는다.** 셋 중 유일하게 상태를 **파괴**하므로, 창 둘에 각각 워크스페이스를 두고 한쪽만 닫는 시나리오가 필요해지면 `debug.fullscreen.*` 처럼 App 레벨로 올려 `window_id` 를 받아야 한다 — 지금은 그 시나리오가 없어 `route_debug_handler`(창 하나만 본다)에 남아 있다 |
| `debug.attach.force_detach` | `surface_id` | 점유 테두리의 **강제 끊기** 버튼 재현 — surface 의 hard 점유를 풀고 holder 에 `force_detached` + `Detach` 를 보낸다. 응답은 `force_detached`(점유가 있었는가)·`surface_id`·`holder`. release 에는 남의 점유를 푸는 IPC 가 없고 로컬 사용자는 GUI 로 끊는다([사용자 통제와 데이터](../principles/control-and-data.md)) |
| `debug.attach.force_detach_workspace` | `workspace_id` | 사이드바 우클릭 **강제 끊기** 확인 재현 — workspace 점유와 멤버 제한을 풀고 holder 에 알린다. 응답은 `force_detached`·`workspace_id`·`holder` |
| `debug.switch_tab` | `index` (0-based) | 포커스 pane 의 활성 탭 전환 — 사용자 탭 클릭 재현 (egui-mesh 탭 가시성 시나리오 검증 등). **대상이 포커스된 창의 포커스된 pane 이라 포커스 독립성을 만족하지 않는다** — 재현하려는 클릭 자체가 그 pane 에서 일어나므로 그것이 정답이다. 응답은 세 갈래다 — 옮겼으면 `switched:true`, **이미 그 탭을 보고 있었으면 성공에 `switched:false`**(정상이고 바뀔 것이 없었을 뿐이다), 그런 탭이 없으면 실패에 실제 탭 수를 실어 보낸다. 앞의 둘을 하나로 누르면 범위 안 인덱스가 "out of range" 로 나가고, 그 문구를 믿은 호출자가 대상을 의심하는 대신 없는 탭을 찾는다 |
| `debug.tool.list` | `{}` | 도구 메뉴 항목 전체를 표시 순서대로 |
| `debug.tool.invoke` | `key` (`<plugin_id>/<tool_id>`) | 도구 항목을 사용자 클릭과 동일하게 dispatch |
| `debug.popup.list` | `{}` | contribute 된 popup 정의 + 현재 열린 instance (각 instance 에 `z_seq` — host popup 과 공유하는 전역 시퀀스라 `debug.host_popup.list` 값과 직접 비교 가능 — 와 소속 범위 `scope`(매니페스트 선언) · `scope_surface`(host 진입점이 바인딩한 대상, 없으면 `null`)) |
| `debug.popup.open` | `plugin_id, popup_id, context?` | popup 인스턴스 강제 open (응답에 `instance_id`) |
| `debug.popup.close` | `instance_id` | 팝업 인스턴스를 닫는다. release의 플러그인 `popup.close`와 같은 큐를 사용해 부모·자식 팝업 정리까지 같은 경로로 검증한다([내부 팝업 시스템](../design/systems/popup.md)) |
| `debug.host_popup.list` | `{}` | 호스트 빌트인 popup(`PopupDef`) 전체 목록 (id + title_key + `close_on_outside_click`). 열려 있는 항목은 `open:true` 와 함께 `z_seq`·`rect`(논리 pt) 를 노출한다 — 겹친 popup 의 마우스 소유권 판정을 좌표 실측 없이 검증하기 위한 관찰면 |
| `debug.host_popup.open` | `popup_id, workspace_scope?` | 호스트 빌트인 popup 을 focused window 중앙에 강제 open (사용자 클릭 경로 우회, 시각 검증용). 사용자 조작 재현이므로 사용자 출처로 열어 키보드 포커스를 준다. `workspace_scope:true` 면 활성 workspace 스코프로 연다 — 런타임 스코프 주입(`OpenPopupMode::WithScope`)을 쓰는 popup(`dag_list`)의 가시성 게이트를 재현하려면 이 플래그가 필요하다 |
| `debug.host_popup.close` | `popup_id` | 호스트 빌트인 popup 강제 close |
| `debug.modifier_hint.hold` | `ctrl?`, `alt?`, `option?`, `shift?`, `elapsed_ms?` | modifier-hint 오버레이의 홀드 조합을 직접 세팅(생략 축=false, 모두 false 면 홀드 해제). `elapsed_ms` 는 홀드 타이머를 그만큼 과거로 백데이트해 표시 지연(500/1200ms) 게이트를 즉시 통과. 실 modifier 홀드 우회 force-state(사용자 홀드 경로 우회). 응답은 `state` 와 동일한 렌더 상태 덤프 |
| `debug.modifier_hint.state` | `{}` | 오버레이 렌더 상태를 draw 경로와 동일 로직으로 재평가해 덤프: `held{ctrl,alt,option,shift}\|null` · `hold_elapsed_ms` · `dismissed` · `reveal_delay_ms`(기본값: Shift 단독 1200ms, 그 외 500ms) · `visible` · `alpha` · `header_combo`(전체 조합 키캡) · `sections[{combo,rows,roles,empty}]`(눌린 조합으로 좁혀진 섹션 — `combo`=섹션 헤더의 조합 전체 키캡, `rows`=각 행의 **leaf 키캡만**(`Ctrl+K` 가 아니라 `K` — modifier 는 섹션 헤더가 담당), `roles`=역할 설명 키, `empty`=바인딩·역할 모두 없는 조합(draw 가 "바인딩 없음" 플레이스홀더로 렌더, docs/design/policies/key-mapping.md)). 스크린샷 없이 좁힘·즉시갱신·지연·빈-플레이스홀더 자동 단정용 |
| `debug.settings.open` | `tab?`, `subtab?` | 설정 모달 강제 open (사용자 클릭/단축키 우회, 시각 검증용). `tab` = L1 `general`/`terminal`/`appearance`/`keybindings`/`file_handler`/`misc`/`plugins` (생략 시 `general`). `subtab` = 선택한 L1 의 L2 섹션 키(아래 표), 생략·미지정 키면 해당 L1 의 기본 L2 유지. `AppEvent::OpenSettings` 발행 → 별도 모달 윈도우 생성. 응답 `{scheduled:true, tab, subtab}`. **설정 창이 이미 열려 있으면** 새로 열지 않고 그 창의 탭·하위 탭을 바꾼다 — 응답 `{scheduled:false, window_id, tab, subtab, tab_applied, subtab_applied}`(지정하지 않은 쪽 `null`, 알 수 없는 키 `false` 이고 그 단계는 현재 선택 유지). `subtab` 만 주면 지금 보고 있는 L1 안에서 바꾼다. 다른 모달(Plugins·종료 확인)이 열려 있으면 `-32000` 으로 거절한다 |
| `debug.settings.close` | `{}` | 설정 모달만 닫는다. 바닥글 Save 를 누르지 않은 상태로 닫으므로 편집은 저장하지 않는다(Cancel 과 같다). 응답 `closed` — 닫았으면 `true` 와 `window_id`, 설정 창이 없으면 `false`, 다른 모달이 열려 있으면 그 모달은 두고 `false` 와 `active_modal_kind`. CLI `tasty debug settings close`. gui 빌드 전용 |
| `debug.modal.close_request` | `{}` | 활성 모달에 **창 닫기 요청**을 흘린다 — 사용자가 창 닫기 버튼을 누른 것의 재현. 응답 `closed` 는 실제로 닫은 모달이 있었는지(없었으면 `false`). release `window.close` 가 main view 만 대상으로 두고 모달을 명시적으로 뺀 것과 같은 범위를 따른다. **WM 없는 Xvfb 에서는 이 경로가 유일하다** — 창은 `WM_DELETE_WINDOW` 를 광고하지만 `xdotool windowclose` 는 그것을 안 쓰고 `XDestroyWindow` 를 불러 winit 이 `GetGeometry` 에서 패닉하고, `wmctrl -i -c` 가 보내는 `_NET_CLOSE_WINDOW` 는 WM 이 없으면 아무도 처리하지 않는다(rc 0, 무효과) |
| `debug.settings.apply` | `settings` (object) | 부분(또는 전체) 설정 patch 를 **라이브 settings 직렬화 복사본** 위에 재귀 deep-merge 한 뒤 완성된 전체 `Settings` 로 교체하는 `ChangeSettings` 를 dispatch 하고 적용·저장이 끝난 뒤 응답한다 — 설정 모달 저장과 **동일 적용 경로**라 collapse·theme·`config.toml` save 까지 cascade 가 처리한다(모달/proxy 불요). 라이브를 pre-mutate 하지 않으므로 cascade 의 prev≠new 비교가 살아 collapse 분기가 정상 발화. `appearance.theme` 이 바뀌면 dispatch 전에 설정 모달·상태바 토글과 같은 `tasty_themes::apply_theme` 으로 그 테마의 색 세트와 밝기를 적용하고 색 override 를 비운다 — id 만 바꾸면 이전 색 세트로 Theme 가 설치돼 화면이 그대로다. 그래서 한 patch 에서 `theme` 과 `theme_overrides`·`theme_is_light` 를 함께 바꾸면 theme 적용이 뒤의 두 값을 덮는다 — theme 을 먼저 apply 하고 override·밝기는 다음 apply 로 나눠 보낸다. **알 수 없는 키는 조용히 무시(no-op)** — `Settings` 가 `deny_unknown_fields` 가 아니라 `#[serde(default)]` 이므로 오타 키는 변화 없이 통과한다(검증자 혼동 주의). 타입 불일치/비-object 는 `-32602` 로 거부되고 라이브는 불변. gui 게이트 없이 headless 에서도 동작 |
| `debug.remote_download.start` | `workspace_id`(mirror 의 로컬 workspace id), `path`(원격 호스트 경로) | 원격 파일 하나의 "다운로드 후 실행" 을 대화상자 없이 시작한다. 창의 대화상자·확인·진행 팝업을 거치지 않으므로 확인 대기는 이 IPC 의 `confirm` 으로 답한다. 응답 `download_id`, 시작 단계 실패면 `refused`(`Unsupported`·`Disconnected`). mirror 가 아닌 workspace 는 `-32602`. 진행·결과는 로그(`remote file download …`)로 보고, 다 받으면 로컬 핸들러로 연다. CLI 는 `tasty debug remote-download start --workspace <id> --path <원격 경로>`. gui 빌드 전용 |
| `debug.remote_download.confirm` | `download_id` | 1 MiB 를 넘어 확인을 기다리는 다운로드를 이어 받는다. 확인 대기가 아니면 아무것도 하지 않는다. CLI 는 `tasty debug remote-download confirm --id <id>` |
| `debug.remote_download.cancel` | `download_id` | 다운로드를 멈추고 받은 부분과 실행별 폴더를 지운다. CLI 는 `tasty debug remote-download cancel --id <id>` |
| `debug.toast` | `message`, `scope`(`window`·`workspace`·`pane`·`surface`), `target_id?`, `window_id?`, `kind?`(`info` 기본·`success`·`warning`·`error`), `hint?`(키캡 문자열 배열) | 지정한 창의 토스트 스택에 토스트를 넣는다(사용자 조작 없이 위치 실측용). `window_id` 는 메인 창이나 보조 창(Settings·Preset·Plugins·종료 확인) 어느 쪽이든 받으며, 생략하면 메인 창이 하나일 때만 그 창이다. 보조 창은 `window` 범위만 받는다. `workspace` 의 `target_id` 는 워크스페이스 **ID** 이고 표시 순번으로 바꿔 넣는다. 대상이 그 창의 engine 에 없으면 `-32602`. 응답 `window_id`·`view`(`main`·`settings`·`preset`·`plugins`·`quit`). 수명은 설정의 토스트 표시 시간을 따른다 |
| `debug.banner.list` | `{}` | 빌트인 배너 정의 + 현재 표시 중/큐 배너(스코프 token·남은초·`total_queued`) + **기하** — `shown[].rect`(셸, **논리**, `host_popup.list` 와 같은 키 모양) · `shown[].content_rect`(plugin egui-mesh 콘텐츠, **물리**, host 배너는 `null`) · 좌표계를 응답이 스스로 싣는 `coords`. `rect` 는 **한 프레임 늦다** — 배너는 popup 과 달리 좌표를 모델에 안 들고 있어 그린 뒤에야 확정되므로 뜬 직후엔 `null` |
| `debug.banner.show` | `banner_id, scope` | 배너 강제 표시 (def 의 ttl 따라 ttl/persistent, 응답에 push `outcome`) — 사용자 조작 우회, 시각 검증용 |
| `debug.banner.close` | `banner_id` | 표시 중/큐 배너 강제 close (표시 중이면 큐 head 승격) |
| `debug.banner.set_countdown` | `scope, seconds` | 표시 중 TTL 배너의 카운트다운 조절 |
| `debug.event_bus.list_subscribers` | `key` | 해당 키 구독 plugin 목록 |
| `debug.event_bus.publish` | `key, payload, scope` | 임의 키로 host 이벤트 발행 |
| `debug.event_bus.trace` | `trace_id` | 같은 trace_id envelope 들을 발행 순서로 |
| `debug.extension.invoke_hook` | `extension_id, kind, phase, mode, target, payload` | 매니페스트 매칭 우회로 extension hook 직접 호출 (fail-open/backoff 우회). 응답 상한은 매니페스트 hook 의 `timeout_ms` 상한(`HOOK_TIMEOUT_MS_MAX`, 1 s)과 같고, 넘기면 `-32004` 로 회신한다 — 선언이 없는 경로라 값이 어디선가 와야 하는데, 같은 extension 이 정상 경로에서 받는 상한과 같게 두는 것이 유일한 파생이다 |
| `debug.fullscreen.list` | `{}` | 등록된 전체화면 무대 정의 전체 — `{"stages":[{id,title_key}]}`. 제목은 i18n **키** 그대로라 로케일에 무관하게 단정 가능 |
| `debug.fullscreen.open` | `stage_id`, `window_id?` | 무대를 창에 강제로 올린다(popup 타이틀바 전체화면 버튼 우회, 시각 검증용). 창당 하나 계약 그대로 — 다른 무대가 올라와 있으면 **교체**(닫힘 훅 실행), 같은 id 면 no-op. 정의 테이블에 없는 `stage_id` 는 창을 고르기 전에 `-32602` 로 **거부**(조용한 no-op 아님). 응답: `window_id`·`stage_id`·`previous_stage_id`·`replaced` |
| `debug.fullscreen.close` | `window_id?` | 그 창의 활성 무대를 내린다. 응답 `closed` 는 실제로 내린 무대가 있었는지(없었으면 `false`), `stage_id` 는 내려간 무대 id |
| `debug.fullscreen.state` | `window_id?` | 활성 무대 id(없으면 `null`) + 창 상태 덤프: `stage_active`·`os_fullscreen`·`maximized`·`inner_size{width,height}`·`monitor{name,position,size,scale_factor}`. **무대 상태와 OS 창 전환은 별개** — `open` 직후 `stage_id` 는 즉시 서지만 `os_fullscreen` 은 다음 프레임의 `sync_window_fullscreen` 이 반영한다 |
| `debug.lua.eval` | `source` | Lua 스크립트를 워커에서 실행한다 — fire-and-forget 이라 응답은 `scheduled` 뿐이고 결과·부수효과는 로그로 관측한다. 임의 코드 실행이라 debug 격리가 유일한 경계다 |
| `debug.plugin_banner.open` | `banner_id`, `surface_id` | plugin 이 기여한 배너를 강제로 띄운다 (응답 `instance_id`). 위 `debug.banner.*` 는 빌트인 배너 쪽이다 |
| `debug.plugin_banner.close` | `instance_id` | plugin 배너 인스턴스 강제 close (응답 `closed`) |
| `debug.webview.history` | `surface_id`, `action`(open/back/forward/reload/stop), `url`(open일 때 필수, `file://`·`http(s)://`) | html surface의 사용자 탐색을 재현한다. open은 주소 표시줄 입력, back/forward/reload는 같은 이름의 버튼이다. 이 넷은 release 사용자 경로(사용자 출처 로드, [html surface](../surfaces/html/index.md))를 그대로 지나며, 히스토리 끝의 back/forward와 문서 없는 reload는 아무것도 하지 않는다. stop은 사용자 진입점이 없어 debug에만 있다. 요청을 받은 창의 다음 redraw에서 적용하며, 그 창에 해당 webview가 없으면 경고 로그만 남긴다(응답 `queued`). CLI `tasty debug webview-history --action <a> [--url <u>]` |
| `debug.webview.overlay` | `force`(선택: off/hide/opaque/transparent) | 마지막 WebView 동기화의 오버레이 모드·적용 여부·실패 여부·대상(종류·상자·scrim)·불투명 모드 페이지, 드러내려는 페이지(`planned_pages`), 그 프레임 오버레이 창에 올린 그림의 모드·페이지(`painted`, 없으면 null)를 돌려준다. `force`를 주면 다음 redraw부터 backend의 지원 보고를 덮어쓴다(`off`는 해제). 지원하지 않는 backend에 opaque·transparent를 강제하면 적용이 실패해 숨김과 `tracing::warn!` 경로를 그대로 지난다([WebView](../design/systems/webview.md#debug-재현)). 응답 뒤 라우터가 창을 dirty로 표시한다. CLI `tasty debug webview-overlay [--force <m>]` |
| `system.shutdown` | `{}` | 프로세스 종료를 요청한다 — 응답 `shutdown`을 먼저 보내고 `AppEvent::Shutdown`을 발행한다. 사용자만 내리던 것을 에이전트가 내리므로 debug 전용이다 |
| `window.focus` / `view.focus` | — | 프로그래밍적 포커스 전환(사용자 단축키/마우스 영역이라 debug 전용) |
| `surface.raw_key` | `keycode`, `direction?`(press/release/click) | **macOS gui 빌드 전용** (다른 조합은 `-32015` ‡). `CGEventPost` 로 OS 이벤트 스트림에 키를 주입한다 — 대상 surface 를 받을 수단이 없어 **그 순간 OS 포커스를 가진 무엇이든** 받는다(tasty 창이 아닐 수도 있다). PTY 바이트 쓰기로는 구동되지 않는 macOS IME 파이프라인(`interpretKeyEvents` → `setMarkedText`/`insertText`) 자동 검증용. 손쉬운 사용(Accessibility) 권한 미승인이면 `-32001 permission_denied` ([macOS 권한](../features/macos-permissions/index.md)) † |
| `surface.switch_input_source` | `source_id` | **macOS gui 빌드 전용** (다른 조합은 `-32015` ‡). `TISSelectInputSource` 로 시스템 입력 소스(키보드 레이아웃·입력기)를 바꾼다 — 사용자가 입력기 메뉴로 하는 조작의 재현. 위 `raw_key` 로 한글/CJK 경로를 검증하기 전 입력기를 맞추는 데 쓴다 † |
| `surface.ime_enable` / `ime_disable` / `ime_preedit` / `ime_commit` / `ime_status` | `text`/`cursor`(preedit·commit) | 포커스된 창의 IME 조합 상태(`ime_active`/`ime_preedit`)를 강제로 세팅·조회한다 — 사용자 입력기 조합의 재현. 대상을 ID 로 받지 못하고 포커스된 창에 작용하므로 포커스 독립성도 만족하지 않는다. 개별 등재가 아니라 `PREFIX_RULES` 의 `surface.ime_` 로 해소되며, 그 규칙 자체가 `#[cfg(debug_assertions)]` 다. 사용법은 [ime-testing](../ai-verification/ime-testing.md) |

### `ui.state` 응답 해석

`attach_pending`는 GUI의 선행 배치 선택, 실제 입력 대상, 미해결 구조 요청의 op·시각·Result 관측, workspace별 입력/조회/resize 대기 수를 읽는다. `draft`는 View 전용 ID이며 공개 surface ID가 아니다. 조회 자체로 redraw하지 않는다. 헤드리스 값은 null이다.


| 필드 | 의미 |
|---|---|
| `settings_open_requested` | 설정 열기 요청이 있는지 나타낸다. 모달이 실제로 표시됐다는 뜻은 아니다 |
| `modal_open`, `active_modal_kind` | 모달 표시 여부와 종류. 설정 창을 기다리는 검증은 plugins·quit 창과 구분하도록 종류도 확인한다 |
| `gate_host_popup_focused`, `gate_plugin_popup_open`, `notification_panel_open` | 호스트 팝업 포커스, 플러그인 팝업 표시, 알림 패널 표시 여부 |
| `gate_restore_notice_focused` | 오른쪽 위 복원 안내 안쪽 컨트롤이 직전 프레임에 키보드 포커스를 가졌는지. 참이면 키가 egui(안내)로 간다 |
| `active_workspace`, `workspace_count` | 활성 워크스페이스 인덱스와 워크스페이스 수 |
| `pane_count`, `tab_count` | 활성 워크스페이스의 pane 수와 포커스된 pane의 탭 수 |
| `active_tab` | 포커스된 pane의 활성 탭 인덱스. 탭 수가 그대로여도 전환 여부를 확인할 수 있다 |

창이 없는 parked 엔진에서도 응답한다. 워크스페이스가 0개이면 `pane_count`, `tab_count`, `active_tab`은 null이며,
`active_workspace`는 `system.info`와 같이 원래 인덱스를 반환한다.

`debug.settings.open` 의 `subtab` 키(활성 `tab` 종속):

| `tab` | 유효 `subtab` 키 |
|-------|------------------|
| `general` | `general` · `notifications` · `accessibility` · `overlay` · `remote_transfer` · `remote_attach` · `display`(macOS 전용 UI — 키 자체는 크로스플랫폼으로 강제 선택 가능) · `macos_permissions` |
| `terminal` | `general` · `mouse_capture` · `tui` · `performance` |
| `appearance` | `theme` · `colors` · `general` · `display` · `tasty` · `terminal` · `explorer` |
| `keybindings` | `general` · `workspace` · `pane` · `tab` · `surface` · `clipboard` · `zoom` · `explorer` · `scripts` · `preset` · `plugins` · `import_export` |
| `file_handler` | `extension_mapping` · `detectors` · `handlers` · `hook_handlers` |
| `misc` | `scripts` · `task_pipeline` / `task-pipeline` · `anomaly_detection` / `anomaly-detection` |
| `plugins` | — (L2 가 plugin contribute page 라 정적 키 없음; 무시) |

> 이 메서드 표와 `DEBUG_METHODS`의 일치를 확인하는 자동 검사는 없다.
> 새 메서드를 추가하면 이 표도 직접 갱신한다. 여러 메서드를 한 칸에 묶거나 중간에
> 다른 표를 넣는 형식까지 검사하려면 별도 파서가 필요하다. 검사 도입 기준은
> [duplicated-sets](duplicated-sets.md)를 따른다.

† **런타임 추가 게이트** — `debug.inject_mouse` · `debug.inject_key` · `surface.raw_key` · `surface.switch_input_source` 는 `--enable-input-simulation` 으로 띄운 인스턴스에서만 동작한다(`engine.input_simulation_enabled`). 안 켜져 있으면 `-32001` 로 거부. 앞의 둘은 대상 surface 의 PTY 에, 뒤의 둘은 **tasty 프로세스 밖 OS 전역 입력 상태**에 작용해 cfg 격리만으로는 부족하다고 봤다. 반면 **게이트 없이 cfg 격리만 받는 입력 재현이 다섯 있다** — `surface.ime_*` 와 `debug.inject_window_mouse` · `debug.inject_egui_mouse` · `debug.inject_egui_key` · `debug.inject_egui_text`. 다섯 다 tasty 프로세스 **안**의 창 상태(IME 조합 / winit·egui 입력 큐)만 바꾸는 in-process 시뮬레이션이라, 인스턴스를 debug 로 띄운 사람 밖으로 효과가 나가지 않는다. 위 넷과 갈리는 기준이 PTY·OS 전역이냐 in-process 냐이므로 이쪽은 cfg 격리로 충분하다. 근거는 [플러그인 권한 모델](plugin-permissions.md).

‡ **다른 플랫폼의 답은 `-32601` 이 아니다.** 위 둘은 등재(`DEBUG_METHODS`)와 CLI 서브커맨드에 플랫폼 조건이 없다 — 이 저장소에서 그 두 층은 플랫폼 균일하고(실측 2026-09-05: `crates/tasty-cli/src/` 와 `crates/tasty-ipc/src/method_meta.rs` 에 `target_os` 게이트 0 건) 차이는 dispatch 층에만 둔다. 그래서 macOS gui 가 아닌 조합에서는 상보 arm 이 **`-32015` 와 사유**로 답한다("이 플랫폼엔 `CGEventPost`/`TISSelectInputSource` 대응물이 없다"). `-32601`("그런 메서드 없음")로 답하면 이름이 틀렸다는 뜻이 되어 호출자가 오타를 의심하게 되는데, 이름은 맞고 표에도 있다 — 고칠 방향이 달라진다. 짝이 빠지지 않게 `src/source_guards/platform_gated_dispatch_complement.rs` 가 강제한다. 근거는 [CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md).

### egui 프레임이 세우는 컨텍스트 메뉴 관찰 (`TASTY_DEBUG_SUPPRESS_NATIVE_MENU`)

`debug.inject_egui_mouse`(winit 우회, egui 입력 큐에 직접 주입 — `event_type` ∈ move/press/release, `button` 0/1/2; `surface_id` 지정 시 `(fx,fy)` 를 그 surface rect 안 정규화 좌표로 해석해 창 크기 무관하게 조준)는 explorer 그리드/컨텍스트 메뉴처럼 egui 위젯 `secondary_clicked` 로 생산되는 메뉴를 탄다. 이 메뉴는 `MainView::process_pending_native_menu` 가 실제 OS native 팝업으로 소비한다(macOS/Windows 는 **블로킹** 모달, Linux 는 비블로킹이지만 팝업이 실제로 뜨는 건 같다) — 어느 쪽이든 headless 관찰이 막힌다. `TASTY_DEBUG_SUPPRESS_NATIVE_MENU=1` 로 띄우면 그 지점에서 메뉴를 표시하지 않고 `debug_captured_menu` 로 포획만 해, `debug.pending_menu` 로 종류를 단언할 수 있다(winit 경로 `debug.inject_window_mouse` 는 env 와 관계없이 주입 직후 요청을 포획한다. 같은 종류의 응답이 대기 중일 때만 요청을 남겨 응답 경로로 보낸다 — 아래 절). GUI 테스트 하네스(`tests/gui_common`)가 이 env 를 켠다. debug 격리, release 미노출.

### native 메뉴 항목 선택 재현 (`debug.native_menu.answer`)

이동 대기(pending move)처럼 native 메뉴의 항목을 골라야 생기는 상태는 OS 팝업을 열 수 없는 환경(Xvfb의 GTK 메뉴는 `no trigger event for menu popup`으로 뜨지 않는다)에서 만들 수 없다. `debug.native_menu.answer`로 메뉴 종류와 응답을 먼저 둔 뒤 사용자처럼 우클릭하면, `process_pending_native_menu`가 여는 메뉴의 종류를 기록하고 `MainView::open_native_menu`가 메뉴 항목을 만든 다음, 종류가 같을 때만 OS 팝업 대신 그 응답을 선택 결과로 넘긴다. 메뉴 항목 구성과 선택 뒤 처리(`apply_tab_move_selection` 등)는 사용자 우클릭과 같은 경로다. macOS·Windows의 즉시 완료 경로와 같은 순서이며, Linux의 비동기 폴링(`poll_pending_native_menu`)은 거치지 않는다.

- 고를 수 없는 항목(구분선·비활성·없는 id나 문구)을 지목하면 메뉴를 닫은 것으로 끝내고 warn 로그를 남긴다.
- 응답은 지정한 종류의 다음 메뉴 하나에만 쓰인다. 다른 종류의 메뉴는 응답을 그대로 둔 채 실제로 연다. 그 종류의 메뉴가 열리지 않으면 응답이 남아 있다가 나중에 열리는 같은 종류의 메뉴에 쓰인다.
- 메뉴를 여는 입력: 탭·사이드바처럼 egui가 만드는 메뉴는 `debug.inject_egui_mouse`(`button` 2, press·release)로 연다. 터미널 메뉴(`TerminalSurface`, 링크 위의 `TerminalLink`)는 winit 우클릭 경로라 `debug.inject_window_mouse`(`button` 2, press·release)로 연다. 링크 메뉴는 먼저 `event_type` `move`로 포인터를 링크 위에 둔다.
- `debug.inject_window_mouse`는 주입 직후 메뉴 요청을 나눈다. 같은 종류의 응답이 대기 중이면 요청을 슬롯에 남겨 다음 프레임의 `process_pending_native_menu`가 위 경로로 응답을 쓴다. 응답이 없거나 다른 종류면 요청을 `debug_captured_menu`로 포획한다. 이때 OS 팝업은 뜨지 않고 다른 종류의 응답은 그대로 남는다.
- `TASTY_DEBUG_SUPPRESS_NATIVE_MENU`가 켜져 있어도 같은 종류의 응답이 대기 중인 메뉴는 포획하지 않고 응답으로 끝낸다. 응답이 OS 팝업 없이 선택을 끝내므로 포획 env의 목적(OS 팝업을 띄우지 않기)과 충돌하지 않는다. 응답이 없거나 종류가 다른 메뉴는 포획한다.

```bash
tasty debug menu-answer --menu Tab --label "Move Tab"   # 또는 --item 7, --dismiss
tasty debug inject egui-mouse --fx 0.34 --fy 0.067 --event-type press --button 2
tasty debug inject egui-mouse --fx 0.34 --fy 0.067 --event-type release --button 2
tasty debug pending-move                           # {"present":true,"kind":"tab","id":...}

# 터미널 메뉴: 선택 영역을 만든 뒤 "Copy"를 고른다(--item 2와 같다)
tasty debug menu-answer --menu TerminalSurface --label "Copy"
tasty debug inject window-mouse --surface <SID> --fx 0.5 --fy 0.5 --event-type press --button 2
tasty debug inject window-mouse --surface <SID> --fx 0.5 --fy 0.5 --event-type release --button 2
```

### OS 열기를 띄우지 않고 기록하기 (`TASTY_DEBUG_OS_OPEN_LOG`)

tasty 가 OS 기본 핸들러에 무언가를 넘기는 자리 — 링크·`System` 파일 핸들러(`directory-system` 등)·plugin 의 `webview.open_external`(markdown 외부 링크)이 부르는 `terminal_link::open_uri`, explorer "OS 기본 앱으로 열기" 의 `platform::reveal::open_path`, macOS 전체 디스크 접근 설정 패널 — 는 격리 `TASTY_HOME` 과 전용 디스플레이로 격리되지 않는다. 브라우저는 이미 떠 있는 자기 인스턴스에 URL 을 넘기는 원격 제어 채널(DBus · 소켓 · LaunchServices)을 가져서, 검증 인스턴스가 연 것이 **사용자 브라우저의 탭**으로 나타난다.

`TASTY_DEBUG_OS_OPEN_LOG=<파일>` 로 띄우면 그 자리들이 프로세스를 띄우지 않고 `<via>\t<대상>` 한 줄을 그 파일에 붙인다(`via` ∈ `open_uri` · `open_path` · `open_settings`). 무엇이 열리려 했는지는 그 줄로 판정한다. 변수가 **있기만 하면** 억제한다 — 값이 비었거나 파일에 못 쓰면 경고 로그만 남기고 여전히 열지 않는다. 호출부는 성공으로 돌아가므로 뒤따르는 동작(recent 기록 등)은 실제로 연 것과 같다. e2e 하네스 셋(`tests/common` · `tests/gui_common` · `tests/webhook_common`)이 격리 홈 아래 `os-open.log` 로 이것을 켠다.

덮지 **않는** 것: tasty 가 띄운 **다른 프로세스**가 스스로 여는 것 — PTY 안의 셸이 부른 `xdg-open`, plugin 프로세스가 스스로 부르는 OS 열기(번들 plugin 에는 없다 — markdown 외부 링크는 host 를 거친다, [플러그인 제작 가이드](plugin-development.md)). 그쪽은 [self-verification](self-verification.md) 의 가짜 브라우저 절차로 막는다. e2e 하네스는 같은 파일에 기록하는 가짜 `BROWSER` 를 준다(Linux·BSD 한정, [e2e-tests](e2e-tests.md) §3). 구현은 `crates/tasty-platform/src/debug_os_open.rs`(모듈 선언에 `#[cfg(debug_assertions)]`), 근거는 [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md). debug 격리, release 미노출.

### 배경 실행 (`TASTY_DEBUG_BACKGROUND`)

macOS에서 `open -g`로 띄워도 winit은 실행이 끝날 때 기본값으로 앱을 활성화하고, 첫 창을 보이는
`set_visible(true)`는 그 창을 키 창으로 앞에 올린다. 화면 잠금이 풀린 Mac에서 격리 인스턴스를 띄우면
사용자가 쓰던 앱의 앞 창과 키 입력을 가져간다.

`TASTY_DEBUG_BACKGROUND=1`로 띄우면 macOS debug 빌드가 실행 완료 때의 활성화를 끄고
(`with_activate_ignoring_other_apps(false)`), 숨겨 만든 첫 창을 키 창으로 만들지 않은 채 모든 창
뒤에 둔다(`orderBack`). 정상 부팅뿐 아니라 셸 설정·부팅 오류·home 사용 중 화면의 첫 창도
같다. 뒤로 보내지 못하면 앞 창을 가져가지 않도록 창을 숨긴 채 두고 오류 로그를 남긴다. 정상
부팅이면 숨긴 채로도 IPC를 받는다. 변수가 없는 실행은 기존처럼 보인다. 값이 정확히 `1`일 때만
켜지며, 다른 OS에서는 값과 관계없이 기존 동작이다. 이후 사용자 출처로 여는 창·모달은 이
변수와 무관하게 기존 정책을 따른다. debug·에이전트 명령이 여는 OS 창도 활성화할 수 있다:
`window.focus`와 settings·preset·plugins·quit 모달은 앱을 활성화하고 창을 키 창으로 올리므로 배경
실행 검증에서는 부르지 않는다. 변수는 프로세스 환경에 남아 PTY 셸과 plugin 자식에게 상속되므로 그 안에서
띄운 debug tasty도 배경 실행이 된다. 부팅 로그 `T2.9 window_visible`은 창을 뒤로 보냈거나 숨긴 채 둔 경우에도
찍히므로 표시 판정에 쓰지 않는다. 가려진 창의 WebView는 그리기를 미룰 수 있으므로 화면
픽셀 판정에는 쓰지 않고 IPC·로그로 판정할 때 쓴다. 구현은 `src/boot/debug_background.rs`(모듈
선언에 `#[cfg(debug_assertions)]`), debug 격리, release 미노출.

### 회차 시간 예산 줄이기 (`TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS`)

IPC dispatch 한 회차의 시간 예산(제품값 16 ms)을 밀리초로 **줄여** 띄운다 — 제품값 이상은 버린다.
`0` 이면 회차마다 첫 명령 하나만 꺼내므로, 동시에 든 나머지 명령은 잘린 회차가 루프를 다시 깨우는
갈래(headless와 GUI, docs/architecture/ipc-server.md)로만 진척한다. 기본 예산에서는 시험의 동시 요청이 회차를
안 잘라 그 갈래를 실행 파일째로 지날 길이 없어 둔 손잡이다. 쓰는 곳은
`tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered_when_every_round_is_cut`. 부팅 뒤
처음 회차에서 한 번 읽는다. debug 격리(`#[cfg(debug_assertions)]`), release 미노출 — release 의
회차 예산은 늘 제품값이다.

### 문자 주입은 키 주입과 다른 채널이다

egui의 `Event::Text(String)`은 문자를 입력하고 `Event::Key`는 키 동작을 전달한다.
`debug.inject_egui_key`만으로는 `TextEdit`에 문자가 들어가지 않으므로
`debug.inject_egui_text`를 사용한다. WM 없는 Xvfb에서 `xdotool type`은 main 창에
`xdotool windowfocus --sync`로 X 포커스를 준 뒤에만 들어간다. 포커스가 없으면(`XGetInputFocus`가
`PointerRoot`) rc=0으로 끝나고 아무것도 입력되지 않는다([스크린샷 방법](../ai-verification/screenshot-methods.md) 5번).

- **문자열 전체가 한 이벤트로** 들어간다. winit 의 `text` 필드가 `char` 가 아니라 문자열이라
  죽은키 조합 같은 실입력도 여러 문자를 한 이벤트로 나른다 — 문자마다 쪼개면 실입력이 만들지
  않는 순서가 된다. 키를 하나씩 누른 사실이 필요하면 `debug.inject_egui_key` 를 그만큼 부른다.
- **제어문자와 private use area 문자는 거절한다**(`injected: false`). 두 끝이 각각 거절할
  이유를 갖는다 — 보내는 끝인 `egui-winit` 은 그런 문자열을 애초에 `Event::Text` 로 올리지
  않고, 받는 끝인 `TextEdit` 은 빈 문자열과 `"\n"`·`"\r"` 를 **조용히 버린다.** 거르지 않으면
  `injected: true` 를 받고도 화면이 안 바뀌어, 이 채널이 메우려던 사각(주입을 믿고 찍은
  스크린샷이 빈 쿼리다)이 그대로 돌아온다. Enter·Tab·Backspace 는 `debug.inject_egui_key` 쪽이다.
- 대상은 그 창에서 **지금 egui 포커스를 가진 위젯**이다. 창은 두 갈래로 고른다.
  - `window_id` 생략: 포커스된 메인 창. 다른 주입과 같이 focused window step(`src/app/ipc/window_required.rs`)에서 처리한다.
  - `window_id` 지정: 그 창. 설정처럼 별도 winit 창으로 뜨는 화면은 메인 창의 egui 입력과 따로 있어 생략한 경로로는 닿지 않는다. App 단계(`src/app/ipc/debug_egui_window.rs`)가 창 ID로 찾아 넣고 포커스는 바꾸지 않는다. `debug.inject_egui_key`·`debug.inject_egui_mouse` 도 같은 두 갈래다. 창 ID는 `debug.window.list`(`tasty debug windows`)로 얻는다([스크린샷 방법](../ai-verification/screenshot-methods.md#모달-창의-id-를-얻는-법)의 "모달 창의 ID 를 얻는 법").
- 이 메서드는 입력칸에 egui 포커스를 주지 않는다. 설정 창 입력칸은 먼저 `debug.inject_egui_mouse` 에 같은 `window_id` 를 주어 그 칸을 클릭한다(`move`·`press`·`release`). 포커스부터 입력·확인까지 debug IPC 만으로 끝나서 OS 입력 도구가 필요 없다.

CLI 는 `tasty debug inject egui-text --text <s>` 다. command palette 를 열어 쿼리를 넣는
전체 절차는 이렇다(`host-popup open` 이 사용자 단축키 경로를 대신한다):

```bash
tasty debug host-popup open --popup-id command_palette
tasty debug inject egui-text --text split      # {"injected":true}
tasty screenshot --window <id> --path /tmp/palette-filtered.png
```

설정 창의 입력칸에 넣는 절차는 이렇다:

```bash
tasty debug settings open --tab general
# 창 생성은 예약만 된다 — 나올 때까지 최대 20회 × 100ms 다시 조회하고, 없으면 멈춘다
S=""
for _ in $(seq 1 20); do
  S=$(tasty debug windows | python3 -c "import json,sys; print(next((w['window_id'] for w in json.load(sys.stdin)['windows'] if w['kind']=='settings'), ''))")
  [ -n "$S" ] && break
  sleep 0.1
done
[ -n "$S" ] || { echo "settings window did not appear in debug windows" >&2; exit 1; }
# Filter sections 입력칸(1100x700 창의 100,66 → 창 정규화 0.0909,0.0943)을 클릭해 egui 포커스를 준다
for e in move press release; do
  tasty debug inject egui-mouse --window-id $S --fx 0.0909 --fy 0.0943 --event-type $e
done
tasty debug inject egui-text --text notif --window-id $S   # {"injected":true,"window_id":<S>}
tasty debug inject egui-key --key Backspace --window-id $S # 마지막 글자를 지운다
tasty screenshot --window $S --path /tmp/settings-filtered.png
```

토글·버튼(Save 등)도 같은 `egui-mouse` 세 번으로 누른다. `Enter` 는 한 줄 입력칸의 포커스를 놓게 하므로 그 뒤의 `egui-text` 는 칸에 붙지 않는다.

### 휠 주입의 단위 (`unit`)

두 마우스 주입 메서드는 `event_type: "scroll"` 일 때 `unit` 을 받는다 — `"line"` ·
`"point"` · `"page"`. 실제 입력에서 데스크톱 마우스 휠은 winit `LineDelta` → egui `Line`
로 오고 트랙패드 같은 픽셀 장치는 `PixelDelta` → `Point` 로 오는데, 이 둘은 논리 포인트로
가는 배율이 다르다(`src/plugin_bridge/wire_scroll.rs`). 단위를 고를 수 없으면 **가장 흔한
입력인 마우스 휠의 환산 경로가 주입으로 재현되지 않는다.**

기본값은 각 메서드가 종전에 합성하던 것이다 — 단위를 넘기지 않던 기존 호출자는 동작이
바뀌지 않는다.

| 메서드 | 기본 | 넣을 수 있는 단위 |
|--------|------|-------------------|
| `debug.inject_window_mouse` (winit 레벨) | `line` | `line` → `LineDelta`, `point` → `PixelDelta`. **`page` 는 거절된다** — winit 에 대응 델타가 없어, 줄로 접어 넣으면 주입은 성공했는데 다른 단위가 흐른다 |
| `debug.inject_egui_mouse` (egui 레벨) | `point` | 셋 다. egui 는 `Page` 를 다루므로 그 갈래까지 재현할 수 있다 |

CLI(`tasty debug inject egui-mouse|window-mouse --unit`)는 위 표에서 그 메서드가 받는 이름만
통과시키고 나머지(`pixel` 등)는 요청을 보내기 전에 거절한다. CLI 의 기본값은 두 명령 모두 `line`
이다(IPC 를 직접 부를 때의 egui 레벨 기본값 `point` 와 다르다).

`scroll_dx`/`scroll_dy` 의 뜻이 단위를 따라간다: `line` 은 줄 수(휠 한 칸이 1.0), egui
레벨의 `point` 는 논리 포인트, **winit 레벨의 `point` 는 물리 픽셀**이다(`PixelDelta` 가
물리 px 이고 수신 측이 scale factor 로 나눈다).

알 수 없는 `unit` 값은 `-32602`로 거절한다. 기본값으로 대신 처리하면
오타가 있는 검증이 의도와 다른 단위로 실행되고도 통과할 수 있다.

## CLI 노출

CLI 선언(`crates/tasty-cli/src/commands/debug.rs`)과 실행
(`crates/tasty-cli/src/local/debug.rs`)은 모두 모듈 단위 `#![cfg(debug_assertions)]`로
제한한다. 전체 하위 명령은 `tasty debug --help`로 확인한다.

`raw-key`와 `switch-input-source`는 모든 플랫폼의 도움말에 표시한다. 실제 실행에는
macOS GUI가 필요하며 다른 조합에서는 `-32015`와 사유를 돌려준다. `raw-key`는
[손쉬운 사용 권한](../features/macos-permissions/index.md)도 필요하다. CLI에 플랫폼 조건을
넣지 않는 규칙은 `src/source_guards/platform_gated_dispatch_complement.rs`가 검사한다.

설정 예:

```bash
tasty debug settings open --tab general --subtab general
tasty debug settings open --tab terminal --subtab tui   # 열린 창의 탭을 바꾼다
tasty debug settings close                              # 저장하지 않고 닫는다
tasty debug settings apply --json '{"general":{"workspace_categories_enabled":false}}'
```

`settings apply`는 `--file <path>`도 받는다. JSON·파일 오류는 CLI가 먼저 거절하고,
서버는 `settings`가 object인지 확인한다.
적용은 설정 창 저장과 같은 `UpdateSettings` 경로를 거친다. 그래서 `general.language`를 바꾸면
설정은 저장되지만 실행 중인 GUI의 라벨은 재시작 뒤에 바뀐다. 검증 순서는
[IPC 함정 3](../ai-verification/ipc-usage.md#함정-3--언어-전환은-재시작-뒤-화면에-반영된다)을 따른다.

### `tasty debug attach` (JSON-RPC 메서드 아님)

로컬 loopback self-attach를 실행하는 **debug 전용** CLI다(`local/debug/attach.rs`).
사용자의 mirror 조작을 재현하는 로컬 CLI 진입점만 debug로 제한하며, 원격 attach는
release에서도 `tasty remote attach`로 실행한다. 같은 머신의 다른 인스턴스에 loopback 주소(`127.0.0.1:PORT`·`localhost:PORT`·`[::1]:PORT`)로
붙는 GUI 원격 연결 팝업·IPC `remote.attach`·자동 attach도 release에 있다. 두 명령은 같은 attach 세션 구현을 쓴다.
결정 근거는 [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md),
동작은 [attach-behavior.md](attach-behavior.md)를 참고한다.

- 명령: `tasty debug attach [SURFACE] [--workspace <id>] [--dump-after <ms>] [--send <str>] [--send-to <sid>] [--raw] [--force-detach]`. `--ssh`/`--profile`/`--into-gui` 같은 원격 옵션은 없다(loopback 전용).
- stream 핸드셰이크 + framed 교환이라 JSON-RPC "메서드 목록" 표/`DEBUG_METHODS` 에 없다. `--force-detach` 는 attach 하지 않고 debug JSON-RPC `debug.attach.force_detach`(`--workspace` 면 `debug.attach.force_detach_workspace`)를 보낸다 — 로컬 사용자의 GUI 강제 끊기 재현이며 release 에는 대응 IPC·CLI 가 없다.

### `tasty debug stream-echo` (JSON-RPC 메서드 아님)

스트리밍 채널(`stream.open` 승격)에서 서버가 클라이언트로 보내는 push를 검증하는
CLI 전용 명령이다. JSON-RPC 호출 대신 프레임을 직접 교환하므로 메서드 표에는 없다.
입력을 재현하는 명령은 아니지만, 전송 경로 검증에 쓰므로 debug 빌드에서만 제공한다.

## 메서드 메타 등록

debug 메서드의 메타(`local_only()`)는 `crates/tasty-ipc/src/method_meta.rs::DEBUG_METHODS` 에 등록한다 (`METHOD_TABLE` 아님). 이 상수는 `#[cfg(debug_assertions)]` 일 때만 항목을 갖고, release 에선 **빈 슬라이스**로 컴파일된다.

새 debug IPC 추가 절차:

1. 핸들러를 `#[cfg(debug_assertions)]` (필요시 `+ feature = "gui"`) 로 감싼다.
2. `route_debug_handler` match 에 분기 추가 (PluginManager 필요하면 `ipc_step_debug`).
3. `DEBUG_METHODS` 표에 `(method, local_only())` 등록. 빠뜨리면 `tests/ipc_router_table_parity.rs` 가 잡는다 — 미등재도 plugin 호출자에겐 거부지만(`UnknownMethod`), 표만 봐서는 정책인지 누락인지 구분이 안 되므로 등재는 선택이 아니다([api-conventions](api-conventions.md) "권한 표 등재").
4. CLI 진입이 필요하면 `DebugCommands` variant 추가.
5. 본 문서 표에 추가. **`docs/` 의 에이전트용 release 문서에는 쓰지 않는다.**

## 디버그 코드 격리 정책 (필수)

기준 한 줄: *"이 코드를 통째로 지우고 컴파일 에러 몇 줄만 정리하면 디버그 기능이 깨끗이 사라지는가?"* 이를 만족해야 격리된 것으로 본다.

- **debug 핸들러는 별도 파일에 모은다** — `src/adapters/ipc/handler/` 의 `debug.rs`(inject/host-popup/modifier-hint/banner 등 gui 상태) · `debug_plugin.rs`(event_bus/extension) · `tool.rs` · `popup.rs` · `input_source.rs`(macOS raw_key/switch_input_source) · `ime.rs`(surface.ime_*) 가 각각 `#[cfg(...)]` 로 모듈 선언된다. **파일 이름에 `debug` 가 들어갈 필요는 없다** — 기준은 "그 파일이 debug 핸들러만 담고 모듈 선언에 cfg 가 붙어 있는가" 다. 일반 핸들러 파일(`pane.rs`, `surface.rs` 등) 중간에 `#[cfg(debug_assertions)] fn debug_xxx()` 를 끼우지 않는다.
  - **GUI 없이 실행하는 debug 핸들러**는 `#[cfg(debug_assertions)]`만 붙인 형제 모듈에 둔다.
    `debug.rs`는 `#[cfg(all(debug_assertions, feature = "gui"))]`로 제한하며,
    `state.popups` / `state.banners` / `state.modifier_hint` 등 GUI 전용 필드를 사용한다.
    현재 GUI 없이 실행하는 핸들러는 다음 세 모듈에 있다.

    | 모듈 | 핸들러 |
    |---|---|
    | `debug_state.rs` | `ui.state`의 `handle_ui_state`, `debug.settings.apply`의 `handle_debug_settings_apply` |
    | `debug_nav.rs` | 워크스페이스·탭 전환 3종 |
    | `debug_terminal.rs` | 터미널 그리드 4종: `cell_info` / `screen_attrs` / `feed_bytes` / `glyph_color` |

    GUI 제한 여부는 핸들러가 실제로 사용하는 타입과 상태로 판단한다. 사용자 조작을
    재현하는지 여부는 debug/release 구분이며, `debug_assertions`가 제한한다.
    예를 들어 `debug.glyph_color`는 `CellAttributes`와 색 타입만 사용하는
    `src/cell_palette.rs`의 순수 함수를 호출한다. 검증 대상인 렌더러와 같은 함수를
    써야 하므로, 검증용 복사본을 만들지 않는다.
- 일반 라우터에는 `#[cfg(debug_assertions)] route_debug_handler(...)` 호출만 둔다.
  debug 핸들러와 함께 지울 항목은 해당 경로의 라우터 분기, `DEBUG_METHODS` 행, CLI variant다.
- **삭제 가능성 테스트**: debug 파일을 지웠을 때 cfg-guard 호출처 몇 줄 제거 외에 다른 변경이 필요하면 격리가 깨진 것이다.

### 마우스를 다룬다고 debug 가 아니다 — 가르는 것은 조작이냐 상태 읽기냐

`debug.inject_window_mouse` 는 debug 고 `surface.mouse_tracking` 은 release 다. 이름이 둘 다
마우스지만 축이 다르다 — 앞은 **사용자 조작을 재현**하고, 뒤는 **터미널이 지금 어떤 상태인가**를
읽는다. [사용자 통제와 데이터 원칙](../principles/control-and-data.md)의 물음("에이전트가 자기 작업에 필요한가 vs 사용자 조작을 재현하는가")에
`surface.mouse_tracking` 은 앞쪽으로 답한다: 안의 프로그램이 마우스를 잡았는지 모르면
에이전트는 마우스 시퀀스를 보낼지 텍스트를 보낼지 정할 수 없고, 드래그 선택이 왜 안 먹는지도
가릴 수 없다. `surface.foreground_process`(셸이 유휴인가)와 같은 자리다.

`surface.mouse_tracking`은 터미널의 `terminal_mode`와 실제 마우스 처리 모드인
`effective_click_mode`를 구분한다. hard 점유 중이거나 전경 프로세스가 캡처 블랙리스트에
있으면 실제 처리 모드는 `None`으로 내려간다. 따라서 terminal_mode가 all_motion인데도
마우스 보고가 없는 상태가 정상일 수 있다.
입력 라우팅과 조회는 GUI 조건 밖의 같은 함수
`crate::state::mouse::effective_click_tracking_decision`을 사용한다.

**이 판정은 자동으로 안 난다.** `tests/ipc_release_table_excludes_input_reproduction.rs` 의
이름 규칙(`inject`·`raw_key`·`switch_input_source`·`ime_`·`simulate`)은 이 이름을 안 잡는다 —
그 가드가 스스로 적어 둔 사각지대("이름에 단서가 없고 debug CLI 진입점도 없는 새 release
메서드의 의미 판단")가 정확히 이 자리다. 그래서 판정 근거를 여기 남긴다.

### 예외: 데이터 구조의 dev-only 필드

매니페스트나 빌트인 spec 처럼 **데이터 구조의 필드 하나만 dev 전용**인 경우는 분리 대상이 아니다. *디버그 동작* 이 아니라 *dev 빌드 데이터 차이* 라 같은 룰을 적용하면 불필요하게 데이터 구조를 나누게 된다.

## 관련

- [주체별 원칙](../principles/control-and-data.md) — 포커스와 debug 조작 범위
- [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md) — 로컬 attach 를 debug 로 격리한 결정
- [플러그인 권한 모델](plugin-permissions.md) — OS 전역 입력 조작(`raw_key`/`switch_input_source`/`ime_*`)을 debug 로 격리한 결정 + `tests/ipc_release_table_excludes_input_reproduction.rs` 회귀 가드
- [CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md) — 플랫폼 게이트가 걸린 dispatch arm 은 `-32601` 이 아니라 `-32015` 와 사유로 답한다 + `src/source_guards/platform_gated_dispatch_complement.rs` 짝 가드
- [attach-behavior.md](attach-behavior.md) — attach 메커니즘
- [self-verification 독립 검증](self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다) — debug IPC 를 쓴 자체 검증


## 기존 콜백의 숫자 관측

Debug GUI의 `TASTY_DEBUG_LOOP_SAMPLES` 환경변수는 IPC 메서드를 추가하지 않고 기존 콜백을 bounded buffer로 관측한다. 출력 경로와 실행 환경은 자신의 격리 인스턴스에만 지정한다. raw 형식·8192개 상한·overflow·종료 export 및 시간 모집단의 한계는 [종료 계측](../architecture/shutdown-sequence.md#writer-종료-시간과-콜백-원시-표본)을 따른다. 입력 주입이나 추가 heartbeat가 아니며 release에는 포함되지 않는다.
