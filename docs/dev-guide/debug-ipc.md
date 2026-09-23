# Debug 전용 IPC

[identity.md](../identity.md) 원칙 1 ②(사용자 입력 재현은 release 에 없다)에 따라, **사용자의 키/마우스/단축키 입력을 그대로 재현하거나 내부 렌더 상태를 덤프하는 IPC** 는 release 빌드에 노출되지 않는다. `#[cfg(debug_assertions)]` 로 격리되어 debug 빌드에서만 컴파일·등록된다.

판단 기준(원칙 1):

- **에이전트 기능 → release IPC**: surface/tab/workspace 생성·조회·닫기, 클립보드, 알림, 메타데이터 등 — 에이전트가 *자기 작업* 을 하려고 필요한 동작. 예: **`surface.completion`**(surface highlight 발동) — 에이전트가 "이 surface 확인 필요" 라고 *자기 작업 결과를 보고* 하는 것이라 PushNotification 과 동류 → **release 정식**. 반면 그 highlight 를 *포커스를 주입해* 해제하는 경로는 사용자 입력 재현이라 debug 격리 대상(상태 해제 자체는 `surface.attention.clear` 로 release 에 있다 — 포커스를 옮겨서 해제하는 방식만 debug 격리 대상이다).
- **디버그 기능 → debug 전용**: 사용자 단축키/마우스로 트리거되는 동작의 자동 재현(키/마우스 주입, 단축키로만 여는 popup/도구 메뉴의 IPC 트리거), 그리고 렌더러·파서 검증용 저수준 덤프.

## 라우팅

JSON-RPC 라우터는 공통 `check_request`의 권한·cap·rate·관측을 마친 요청만 `src/adapters/ipc/handler.rs::handle_checked_request`에 넘기고, 핸들러 탐색은 그 안의 `dispatch_routed` 가 한다:

```rust
// window: &mut EntryWindow — handle_checked_request 가 쥔 AppState 를 감싼 것
let mut out = IntentOutbox::default();   // 요청 하나의 intent 출구
let routed = route_engine_handler(core, window.port(), &mut out, engine, caller, request, id.clone());
window.port().enqueue_intents(out);      // 출구를 이 창의 큐 끝으로
if let Some(resp) = routed {
    return resp;                         // release+debug 공통 엔진 핸들러
}
#[cfg(feature = "gui")]
if let Some(resp) = window.route_window(engine, caller, request, id.clone()) {
    return resp;                         // 창 상태가 대상인 gui 전용 핸들러(route_window_handler)
}
#[cfg(debug_assertions)]
if let Some(resp) = window.route_debug(engine, request, id.clone()) {
    return resp;                         // debug 빌드 전용(route_debug_handler)
}
JsonRpcResponse::unrouted_for_external_caller(id, &request.method)
```

`EntryWindow` 의 debug 문(`route_debug`)은 별도 파일 `src/adapters/ipc/handler/entry_window_debug.rs`
에 있다. 그 파일은 `EntryWindow` 의 비공개 필드에 닿아야 해서 `entry_window` 의 자식 모듈이고, 그래서
cfg 를 선언이 아니라 파일 머리의 `#![cfg(debug_assertions)]` 로 건다 — 아래 격리 정책의 가드가 그 형태를
파일 단위 격리로 인정한다. release 에는 문 자체가 없다.

`route_debug_handler` 함수 자체가 `#[cfg(debug_assertions)]` 라 release 바이너리엔 분기 한 줄과 함수가 모두 사라진다. release 에서 debug 메서드를 부르면 `-32601`(`method_not_found`)로 떨어진다 — 위 블록 끝의 `unrouted_for_external_caller` 는 **등록된 이름**이면 `-32017`(이 빌드 조합에 dispatch 팔이 없음)을, plugin 전용이면 `-32016` 을 답하지만, debug 메서드 표(`DEBUG_METHODS`)가 release 에서는 빈 표라 debug 메서드는 등록된 이름이 아니다. 그래서 모르는 이름과 같은 답이 된다(release 헤드리스 인스턴스에서 `debug.info` · `debug.popup.open` · `surface.raw_key` 가 전부 `-32601`, 2026-09-22 실측). `-32017` 은 release 에도 등록된 이름이 이 조합에서 빠진 경우(gui 전용 메서드를 헤드리스에 부른 것 등)의 답이다.

### `AppState` 하나로는 부족한 debug 메서드는 App-level 에서 분기

`debug.event_bus.*` / `debug.extension.invoke_hook` / `debug.popup.*` 는 `route_debug_handler` 를 거치지 않는다 — `AppState` 가 `PluginManager` 를 들고 있지 않기 때문이다. 이들은 `App` 레벨의 `ipc_step_debug`(`src/app/ipc/debug_methods.rs`)에서 `plugin_manager` 를 직접 호출한다.

`debug.fullscreen.*` 도 같은 곳에서 처리되지만 이유는 또 다르다 — **창을 골라야 하기 때문**이다. `route_debug_handler` 는 `AppState`(=`MainView` 하나)만 받아 다른 창을 볼 수 없는데, 전체화면 무대는 창 단위 상태라([fullscreen-stage](../design/systems/fullscreen-stage.md)) `window_id` 로 대상을 지목하지 못하면 "창 2 개에 각각 무대를 띄운 뒤 한쪽만 닫는다" 같은 시나리오 자체가 구동 불가다. `self.view.views` 순회는 App 레벨에서만 가능하다. `window_id` 해석은 `ui.screenshot` 과 동형 — 지정하면 그 창, 미지정이고 창이 하나면 그 창, 여럿이면 `-32000` 에러다. 포커스된 창으로 조용히 폴백하지 않는다([focus](../design/policies/focus.md)).

`debug.settings.open` 도 같은 `ipc_step_debug` 에서 처리되지만 이유는 또 다르다 — 설정 모달은 `AppEvent::OpenSettings`(event-loop proxy → `open_settings_modal`) 로만 열리는 **별도 winit 윈도우**라, `AppState` 핸들러가 아니라 `App` 의 `self.view.proxy` 가 필요하다(`window.create` 와 동일 패턴). 사용자 단축키/버튼 클릭과 같은 진입점을 그대로 호출하므로, 정상 모달 동작과 100% 동일하다. `tab` 인자는 `App.pending_settings_tab`(debug 전용 필드)에 1회성으로 실려 `open_settings_modal` 이 소비한다.

## 메서드 목록

debug 메서드는 모두 `local_only()` — plugin caller 는 호출 불가, CLI/network IPC(로컬)만 가능.

| method | params | 설명 |
|--------|--------|------|
| `ui.state` | `{}` | 현재 UI 상태 덤프 — `settings_open_requested`(열기 **요청** 래치. 모달이 화면에 있는지가 아니다) · `modal_open` + `active_modal_kind`(모달이 실제로 떠 있는가와 **어느** 모달인가. 종류 없이 `modal_open` 만 보면 설정 창을 기다리는 쪽이 plugins·quit 창을 보고 통과한다) · popup · `active_workspace` · `workspace_count` · `pane_count` · `tab_count` · `active_tab`(포커스된 pane 의 활성 탭 인덱스. 수만으로는 **탭 전환이 안 보여서** 있는 축이다). parked 엔진(마지막 창이 닫혀 워크스페이스 0)에서도 답한다 — 그때 `pane_count` · `tab_count` · `active_tab` 은 null 이고 `active_workspace` 는 `system.info` 처럼 원래 인덱스다 |
| `debug.info` | `{}` | 실행 중 인스턴스 debug 정보 |
| `debug.cell_info` | `surface_id, row, col` | 셀 단위 렌더 속성(텍스트, fg/bg, bold/italic/underline …) |
| `debug.screen_attrs` | `surface_id, row` | 한 행 전체 셀 속성 |
| `debug.glyph_color` | `surface_id, row, col, bg_mode?` | GPU 렌더러가 실제로 push 하는 글리프 bg/fg RGBA (renderer 색 해석 검증) |
| `debug.feed_bytes` | `surface_id, bytes(hex)` 또는 `text` | VTE 바이트를 PTY 우회로 터미널에 직접 주입(파서/렌더 테스트) |
| `debug.gpu.stall` | `ms` | 다음 프레임의 `present` 직전을 `ms` 밀리초 블로킹(1 회). 이벤트 루프 stall 재현 — CLI 는 `tasty debug gpu-stall --ms N` |
| `debug.inject_mouse` | `surface_id, row, col, button?, event_type?` | SGR mouse(1006) 시퀀스로 마우스 이벤트 주입 † |
| `debug.inject_key` | `surface_id, bytes(hex)` 또는 `text` | 키 이벤트 주입 † |
| `debug.inject_window_mouse` | `surface_id?`, `fx?`/`fy?`(기본 0.5, 창 정규화 좌표), `event_type?`, `button?`, `scroll_dx?`/`scroll_dy?`, `unit?`(기본 `line`) | winit 레벨 마우스 이벤트 주입 — 포커스된 창에 작용한다. 스크롤 단위는 아래 [휠 주입의 단위](#휠-주입의-단위-unit) |
| `debug.inject_egui_mouse` | 위와 같음, `unit?` 기본 `point` | egui 레벨 마우스 이벤트 주입 — winit 환산 경로를 건너뛰고 egui 입력에 직접 넣는다 |
| `debug.inject_egui_key` | `key?`(기본 `Escape`), `pressed?`(기본 `true`) | egui 레벨 키 이벤트 주입 |
| `debug.inject_egui_text` | `text`(필수, 문자열) | egui 레벨 **문자** 이벤트 주입 — 포커스된 `TextEdit`(command palette 쿼리 등)에 글자를 넣는다. 아래 [문자 주입은 키 주입과 다른 채널이다](#문자-주입은-키-주입과-다른-채널이다) |
| `debug.selection` | `{}` | focused window 의 로컬 텍스트 선택 상태 read-only 덤프(`present`·`surface_id`·`mode`·`dragging`·`empty`·`anchor/cursor/start/end{col,row}`). 마우스 라우팅 회귀 net 의 관찰면 — 순수 관찰(사용자 상태 불변) |
| `debug.pending_menu` | `{}` | 대기 중 컨텍스트 메뉴 read-only 덤프(`present`·`kind`·`surface_id?`). live pending 우선, 없으면 주입 포획본(`debug_captured_menu`). 우클릭 라우팅 회귀 관찰용 |
| `debug.focused_surface` | `{}` | 현재 포커스된 surface id read-only 덤프(`surface_id`, 없으면 null). `surface.list` 가 노출 않는 view-layer 포커스를 관찰 — click-to-activate 라우팅 회귀 net 용 |
| `debug.switch_workspace` | `index` (0-based) | 활성 워크스페이스 전환 — 사용자 포커스 조작 재현. **`index` 는 포커스된 창 안의 순번이라 포커스 독립성을 만족하지 않는다** — 사용자가 보고 있는 창에서 전환하는 것이 이 메서드의 뜻이므로 그것이 정답이다(`surface.ime_*` 와 같은 부류) |
| `debug.close_workspace` | `index` (0-based) | 워크스페이스 컨텍스트 메뉴 "Close workspace" 재현 — 워크스페이스 안 **모든** surface + closed_item 스냅샷까지 한 번에 닫는다. release `surface.close` 는 cascade 특성상 마지막 한 surface 만 정리하므로 "탭 많은 워크스페이스 통째 close" 비용([close-sequence](../architecture/close-sequence.md) `path="gui"`)은 이 메서드로만 재현된다. 마지막 workspace 는 거절(0개가 되면 다음 redraw 가 패닉). **`index` 는 포커스된 창 안의 순번이라 포커스 독립성을 만족하지 않는다.** 셋 중 유일하게 상태를 **파괴**하므로, 창 둘에 각각 워크스페이스를 두고 한쪽만 닫는 시나리오가 필요해지면 `debug.fullscreen.*` 처럼 App 레벨로 올려 `window_id` 를 받아야 한다 — 지금은 그 시나리오가 없어 `route_debug_handler`(창 하나만 본다)에 남아 있다 |
| `debug.switch_tab` | `index` (0-based) | 포커스 pane 의 활성 탭 전환 — 사용자 탭 클릭 재현 (egui-mesh 탭 가시성 시나리오 검증 등). **대상이 포커스된 창의 포커스된 pane 이라 포커스 독립성을 만족하지 않는다** — 재현하려는 클릭 자체가 그 pane 에서 일어나므로 그것이 정답이다. 응답은 세 갈래다 — 옮겼으면 `switched:true`, **이미 그 탭을 보고 있었으면 성공에 `switched:false`**(정상이고 바뀔 것이 없었을 뿐이다), 그런 탭이 없으면 실패에 실제 탭 수를 실어 보낸다. 앞의 둘을 하나로 누르면 범위 안 인덱스가 "out of range" 로 나가고, 그 문구를 믿은 호출자가 대상을 의심하는 대신 없는 탭을 찾는다 |
| `debug.tool.list` | `{}` | 도구 메뉴 항목 전체를 표시 순서대로 |
| `debug.tool.invoke` | `key` (`<plugin_id>/<tool_id>`) | 도구 항목을 사용자 클릭과 동일하게 dispatch |
| `debug.popup.list` | `{}` | contribute 된 popup 정의 + 현재 열린 instance (각 instance 에 `z_seq` — host popup 과 공유하는 전역 시퀀스라 `debug.host_popup.list` 값과 직접 비교 가능 — 와 소속 범위 `scope`(매니페스트 선언) · `scope_surface`(host 진입점이 바인딩한 대상, 없으면 `null`)) |
| `debug.popup.open` | `plugin_id, popup_id, context?` | popup 인스턴스 강제 open (응답에 `instance_id`) |
| `debug.popup.close` | `instance_id` | popup 인스턴스 강제 close. release 경로(plugin 의 `popup.close`)와 **같은 close 큐**로 합류한다([ADR-0084](../adr/0084-plugin-triggered-host-popup-ownership.md) Decision 3) — 여기서 매니저를 직접 치면 부모-자식 연쇄 정리를 건너뛰어, 이 표면으로 하는 재현 검증이 실제 동작과 어긋난다 |
| `debug.host_popup.list` | `{}` | 호스트 빌트인 popup(`PopupDef`) 전체 목록 (id + title_key + `close_on_outside_click`). 열려 있는 항목은 `open:true` 와 함께 `z_seq`·`rect`(논리 pt) 를 노출한다 — 겹친 popup 의 마우스 소유권 판정을 좌표 실측 없이 검증하기 위한 관찰면 |
| `debug.host_popup.open` | `popup_id, workspace_scope?` | 호스트 빌트인 popup 을 focused window 중앙에 강제 open (사용자 클릭 경로 우회, 시각 검증용). `workspace_scope:true` 면 활성 workspace 스코프로 연다 — 런타임 스코프 주입(`OpenPopupMode::WithScope`)을 쓰는 popup(`dag_list`)의 가시성 게이트를 재현하려면 이 플래그가 필요하다 |
| `debug.host_popup.close` | `popup_id` | 호스트 빌트인 popup 강제 close |
| `debug.modifier_hint.hold` | `ctrl?`, `alt?`, `option?`, `shift?`, `elapsed_ms?` | modifier-hint 오버레이의 홀드 조합을 직접 세팅(생략 축=false, 모두 false 면 홀드 해제). `elapsed_ms` 는 홀드 타이머를 그만큼 과거로 백데이트해 표시 지연(500/1200ms) 게이트를 즉시 통과. 실 modifier 홀드 우회 force-state(사용자 홀드 경로 우회). 응답은 `state` 와 동일한 렌더 상태 덤프 |
| `debug.modifier_hint.state` | `{}` | 오버레이 렌더 상태를 draw 경로와 동일 로직으로 재평가해 덤프: `held{ctrl,alt,option,shift}\|null` · `hold_elapsed_ms` · `dismissed` · `reveal_delay_ms`(Shift 단독 2000, 그 외 500) · `visible` · `alpha` · `header_combo`(전체 조합 키캡) · `sections[{combo,rows,roles,empty}]`(눌린 조합으로 좁혀진 섹션 — `combo`=섹션 헤더의 조합 전체 키캡, `rows`=각 행의 **leaf 키캡만**(`Ctrl+K` 가 아니라 `K` — modifier 는 섹션 헤더가 담당), `roles`=역할 설명 키, `empty`=바인딩·역할 모두 없는 조합(draw 가 "바인딩 없음" 플레이스홀더로 렌더, ADR-0038)). 스크린샷 없이 좁힘·즉시갱신·지연·빈-플레이스홀더 자동 단정용 |
| `debug.settings.open` | `tab?`, `subtab?` | 설정 모달 강제 open (사용자 클릭/단축키 우회, 시각 검증용). `tab` = L1 `general`/`terminal`/`appearance`/`keybindings`/`file_handler`/`misc`/`plugins` (생략 시 `general`). `subtab` = 선택한 L1 의 L2 섹션 키(아래 표), 생략·미지정 키면 해당 L1 의 기본 L2 유지. `AppEvent::OpenSettings` 발화 → 별도 모달 윈도우 생성 |
| `debug.modal.close_request` | `{}` | 활성 모달에 **창 닫기 요청**을 흘린다 — 사용자가 창 닫기 버튼을 누른 것의 재현. 응답 `closed` 는 실제로 닫은 모달이 있었는지(없었으면 `false`). release `window.close` 가 main view 만 대상으로 두고 모달을 명시적으로 뺀 것과 같은 선이다. **WM 없는 Xvfb 에서는 이 경로가 유일하다** — 창은 `WM_DELETE_WINDOW` 를 광고하지만 `xdotool windowclose` 는 그것을 안 쓰고 `XDestroyWindow` 를 불러 winit 이 `GetGeometry` 에서 패닉하고, `wmctrl -i -c` 가 보내는 `_NET_CLOSE_WINDOW` 는 WM 이 없으면 아무도 처리하지 않는다(rc 0, 무효과) |
| `debug.settings.apply` | `settings` (object) | 부분(또는 전체) 설정 patch 를 **라이브 settings 직렬화 복사본** 위에 재귀 deep-merge 한 뒤 완성된 전체 `Settings` 로 `UpdateSettings` 를 dispatch — 설정 모달 저장과 **동일 경로**라 collapse·theme·`config.toml` save 까지 cascade 가 처리한다(모달/proxy 불요). 라이브를 pre-mutate 하지 않으므로 cascade 의 prev≠new 비교가 살아 collapse 분기가 정상 발화. **알 수 없는 키는 조용히 무시(no-op)** — `Settings` 가 `deny_unknown_fields` 가 아니라 `#[serde(default)]` 이므로 오타 키는 변화 없이 통과한다(검증자 혼동 주의). 타입 불일치/비-object 는 `-32602` 로 거부되고 라이브는 불변. gui 게이트 없이 headless 에서도 동작 |
| `debug.banner.list` | `{}` | 빌트인 배너 정의 + 현재 표시 중/큐 배너(스코프 token·남은초·`total_queued`) + **기하** — `shown[].rect`(셸, **논리**, `host_popup.list` 와 같은 키 모양) · `shown[].content_rect`(plugin egui-mesh 콘텐츠, **물리**, host 배너는 `null`) · 좌표계를 응답이 스스로 싣는 `coords`. `rect` 는 **한 프레임 늦다** — 배너는 popup 과 달리 좌표를 모델에 안 들고 있어 그린 뒤에야 확정되므로 뜬 직후엔 `null` |
| `debug.banner.show` | `banner_id, scope` | 배너 강제 발화 (def 의 ttl 따라 ttl/persistent, 응답에 push `outcome`) — 사용자 조작 우회, 시각 검증용 |
| `debug.banner.close` | `banner_id` | 표시 중/큐 배너 강제 close (표시 중이면 큐 head 승격) |
| `debug.banner.set_countdown` | `scope, seconds` | 표시 중 TTL 배너의 카운트다운 조절 |
| `debug.event_bus.list_subscribers` | `key` | 해당 키 구독 plugin 목록 |
| `debug.event_bus.publish` | `key, payload, scope` | 임의 키로 host envelope 발화 |
| `debug.event_bus.trace` | `trace_id` | 같은 trace_id envelope 들을 발화 순서로 |
| `debug.extension.invoke_hook` | `extension_id, kind, phase, mode, target, payload` | 매니페스트 매칭 우회로 extension hook 직접 호출 (fail-open/backoff 우회). 응답 상한은 매니페스트 hook 의 `timeout_ms` 상한(`HOOK_TIMEOUT_MS_MAX`, 1 s)과 같고, 넘기면 `-32004` 로 회신한다 — 선언이 없는 경로라 값이 어디선가 와야 하는데, 같은 extension 이 정상 경로에서 받는 상한과 같게 두는 것이 유일한 파생이다 |
| `debug.fullscreen.list` | `{}` | 등록된 전체화면 무대 정의 전체 — `{"stages":[{id,title_key}]}`. 제목은 i18n **키** 그대로라 로케일에 무관하게 단정 가능 |
| `debug.fullscreen.open` | `stage_id`, `window_id?` | 무대를 창에 강제로 올린다(popup 타이틀바 전체화면 버튼 우회, 시각 검증용). 창당 하나 계약 그대로 — 다른 무대가 올라와 있으면 **교체**(닫힘 훅 발화), 같은 id 면 no-op. 정의 테이블에 없는 `stage_id` 는 창을 고르기 전에 `-32602` 로 **거부**(조용한 no-op 아님). 응답: `window_id`·`stage_id`·`previous_stage_id`·`replaced` |
| `debug.fullscreen.close` | `window_id?` | 그 창의 활성 무대를 내린다. 응답 `closed` 는 실제로 내린 무대가 있었는지(없었으면 `false`), `stage_id` 는 내려간 무대 id |
| `debug.fullscreen.state` | `window_id?` | 활성 무대 id(없으면 `null`) + 창 상태 덤프: `stage_active`·`os_fullscreen`·`maximized`·`inner_size{width,height}`·`monitor{name,position,size,scale_factor}`. **무대 상태와 OS 창 전환은 별개** — `open` 직후 `stage_id` 는 즉시 서지만 `os_fullscreen` 은 다음 프레임의 `sync_window_fullscreen` 이 반영한다 |
| `debug.lua.eval` | `source` | Lua 스크립트를 워커에서 실행한다 — fire-and-forget 이라 응답은 `scheduled` 뿐이고 결과·부수효과는 로그로 관측한다. 임의 코드 실행이라 debug 격리가 유일한 경계다 |
| `debug.plugin_banner.open` | `banner_id`, `surface_id` | plugin 이 기여한 배너를 강제로 띄운다 (응답 `instance_id`). 위 `debug.banner.*` 는 빌트인 배너 쪽이다 |
| `debug.plugin_banner.close` | `instance_id` | plugin 배너 인스턴스 강제 close (응답 `closed`) |
| `system.shutdown` | `{}` | 프로세스 종료를 요청한다 — 응답 `shutdown` 을 먼저 보내고 `AppEvent::Shutdown` 을 발사한다. 사용자만 내리던 것을 에이전트가 내리므로 debug 전용이다 |
| `window.focus` / `view.focus` | — | 프로그래밍적 포커스 전환(사용자 단축키/마우스 영역이라 debug 전용) |
| `surface.raw_key` | `keycode`, `direction?`(press/release/click) | **macOS gui 빌드 전용** (다른 조합은 `-32015` ‡). `CGEventPost` 로 OS 이벤트 스트림에 키를 주입한다 — 대상 surface 를 받을 수단이 없어 **그 순간 OS 포커스를 가진 무엇이든** 받는다(tasty 창이 아닐 수도 있다). PTY 바이트 쓰기로는 구동되지 않는 macOS IME 파이프라인(`interpretKeyEvents` → `setMarkedText`/`insertText`) 자동 검증용. 손쉬운 사용(Accessibility) 권한 미승인이면 `-32001 permission_denied` ([macOS 권한](../features/macos-permissions/index.md)) † |
| `surface.switch_input_source` | `source_id` | **macOS gui 빌드 전용** (다른 조합은 `-32015` ‡). `TISSelectInputSource` 로 시스템 입력 소스(키보드 레이아웃·입력기)를 바꾼다 — 사용자가 입력기 메뉴로 하는 조작의 재현. 위 `raw_key` 로 한글/CJK 경로를 검증하기 전 입력기를 맞추는 데 쓴다 † |
| `surface.ime_enable` / `ime_disable` / `ime_preedit` / `ime_commit` / `ime_status` | `text`/`cursor`(preedit·commit) | 포커스된 창의 IME 조합 상태(`ime_active`/`ime_preedit`)를 강제로 세팅·조회한다 — 사용자 입력기 조합의 재현. 대상을 ID 로 받지 못하고 포커스된 창에 작용하므로 포커스 독립성도 만족하지 않는다. 개별 등재가 아니라 `PREFIX_RULES` 의 `surface.ime_` 로 해소되며, 그 규칙 자체가 `#[cfg(debug_assertions)]` 다. 사용법은 [ime-testing](../ai-verification/ime-testing.md) |

`debug.settings.open` 의 `subtab` 키(활성 `tab` 종속):

| `tab` | 유효 `subtab` 키 |
|-------|------------------|
| `general` | `general` · `notifications` · `accessibility` · `overlay` · `remote_transfer` · `display`(macOS 전용 UI — 키 자체는 크로스플랫폼으로 강제 선택 가능) · `macos_permissions` |
| `terminal` | `general` · `mouse_capture` · `tui` · `performance` |
| `appearance` | `theme` · `colors` · `general` · `display` · `tasty` · `terminal` · `explorer` |
| `keybindings` | `general` · `workspace` · `pane` · `tab` · `surface` · `clipboard` · `zoom` · `explorer` · `scripts` · `preset` · `plugins` · `import_export` |
| `file_handler` | `extension_mapping` · `detectors` · `handlers` · `hook_handlers` |
| `misc` | `scripts` · `tastyrc` (Windows 전용) |
| `plugins` | — (L2 가 plugin contribute page 라 정적 키 없음; 무시) |

> **이 표에는 자동 채널이 없다.** `DEBUG_METHODS` 와 이 표가 어긋나도 어떤 잡도 안 터진다 —
> 실제로 일곱 건이 빠져 있었고 손으로 채웠다. 판정기를 안 만든 이유는 이 표가 사이에 낀
> 다른 표에 끊겨 있고 한 칸에 메서드가 여럿인 행이 있어, 그것을 다루는 파서가 판정하려는
> 명제보다 커지기 때문이다. 메서드를 더하면 **여기도 같이 고쳐야 한다.** 판정 기준은
> [duplicated-sets](duplicated-sets.md).

† **런타임 추가 게이트** — `debug.inject_mouse` · `debug.inject_key` · `surface.raw_key` · `surface.switch_input_source` 는 `--enable-input-simulation` 으로 띄운 인스턴스에서만 동작한다(`engine.input_simulation_enabled`). 안 켜져 있으면 `-32001` 로 거부. 앞의 둘은 대상 surface 의 PTY 에, 뒤의 둘은 **tasty 프로세스 밖 OS 전역 입력 상태**에 작용해 cfg 격리만으로는 부족하다고 봤다. 반면 **게이트 없이 cfg 격리만 받는 입력 재현이 다섯 있다** — `surface.ime_*` 와 `debug.inject_window_mouse` · `debug.inject_egui_mouse` · `debug.inject_egui_key` · `debug.inject_egui_text`. 다섯 다 tasty 프로세스 **안**의 창 상태(IME 조합 / winit·egui 입력 큐)만 바꾸는 in-process 시뮬레이션이라, 인스턴스를 debug 로 띄운 사람 밖으로 효과가 나가지 않는다. 위 넷과 갈리는 기준이 PTY·OS 전역이냐 in-process 냐이므로 이쪽은 cfg 격리로 충분하다. 근거는 [ADR-0115](../adr/0115-input-reproduction-ipc-debug-isolation.md).

‡ **다른 플랫폼의 답은 `-32601` 이 아니다.** 위 둘은 등재(`DEBUG_METHODS`)와 CLI 서브커맨드에 플랫폼 조건이 없다 — 이 저장소에서 그 두 층은 플랫폼 균일하고(실측 2026-09-05: `crates/tasty-cli/src/` 와 `crates/tasty-ipc/src/method_meta.rs` 에 `target_os` 게이트 0 건) 차이는 dispatch 층에만 둔다. 그래서 macOS gui 가 아닌 조합에서는 상보 arm 이 **`-32015` 와 사유**로 답한다("이 플랫폼엔 `CGEventPost`/`TISSelectInputSource` 대응물이 없다"). `-32601`("그런 메서드 없음")로 답하면 이름이 틀렸다는 뜻이 되어 호출자가 오타를 의심하게 되는데, 이름은 맞고 표에도 있다 — 고칠 방향이 달라진다. 짝이 빠지지 않게 `src/source_guards/platform_gated_dispatch_complement.rs` 가 강제한다. 근거는 [ADR-0154](../adr/0154-a-platform-gated-dispatch-arm-answers-why-not-what.md).

### egui 프레임이 세우는 컨텍스트 메뉴 관찰 (`TASTY_DEBUG_SUPPRESS_NATIVE_MENU`)

`debug.inject_egui_mouse`(winit 우회, egui 입력 큐에 직접 주입 — `event_type` ∈ move/press/release, `button` 0/1/2; `surface_id` 지정 시 `(fx,fy)` 를 그 surface rect 안 정규화 좌표로 해석해 창 크기 무관하게 조준)는 explorer 그리드/컨텍스트 메뉴처럼 egui 위젯 `secondary_clicked` 로 생산되는 메뉴를 탄다. 이 메뉴는 `MainView::process_pending_native_menu` 가 실제 OS native 팝업으로 소비한다(macOS/Windows 는 **블로킹** 모달, Linux 는 비블로킹이지만 팝업이 실제로 뜨는 건 같다) — 어느 쪽이든 headless 관찰이 막힌다. `TASTY_DEBUG_SUPPRESS_NATIVE_MENU=1` 로 띄우면 그 지점에서 메뉴를 표시하지 않고 `debug_captured_menu` 로 포획만 해, `debug.pending_menu` 로 종류를 단언할 수 있다(winit 경로 `debug.inject_window_mouse` 는 핸들러가 즉시 세워 이미 포획됨 — 이 env 는 egui 경로용). GUI 테스트 하네스(`tests/gui_common`)가 이 env 를 켠다. debug 격리, release 미노출.

### OS 열기를 띄우지 않고 기록하기 (`TASTY_DEBUG_OS_OPEN_LOG`)

tasty 가 OS 기본 핸들러에 무언가를 넘기는 자리 — 링크·`System` 파일 핸들러(`directory-system` 등)·plugin 의 `webview.open_external`(markdown 외부 링크)이 부르는 `terminal_link::open_uri`, explorer "OS 기본 앱으로 열기" 의 `platform::reveal::open_path`, info modal 외부 링크, macOS 전체 디스크 접근 설정 패널 — 는 격리 `TASTY_HOME` 과 전용 디스플레이로 격리되지 않는다. 브라우저는 이미 떠 있는 자기 인스턴스에 URL 을 넘기는 원격 제어 채널(DBus · 소켓 · LaunchServices)을 가져서, 검증 인스턴스가 연 것이 **사용자 브라우저의 탭**으로 나타난다.

`TASTY_DEBUG_OS_OPEN_LOG=<파일>` 로 띄우면 그 자리들이 프로세스를 띄우지 않고 `<via>\t<대상>` 한 줄을 그 파일에 붙인다(`via` ∈ `open_uri` · `open_path` · `open_external` · `open_settings`). 무엇이 열리려 했는지는 그 줄로 판정한다. 변수가 **있기만 하면** 억제한다 — 값이 비었거나 파일에 못 쓰면 경고 로그만 남기고 여전히 열지 않는다. 호출부는 성공으로 돌아가므로 뒤따르는 동작(recent 기록 등)은 실제로 연 것과 같다. e2e 하네스 셋(`tests/common` · `tests/gui_common` · `tests/webhook_common`)이 격리 홈 아래 `os-open.log` 로 이것을 켠다.

덮지 **않는** 것: tasty 가 띄운 **다른 프로세스**가 스스로 여는 것 — PTY 안의 셸이 부른 `xdg-open`, plugin 프로세스가 스스로 부르는 OS 열기(번들 plugin 에는 없다 — markdown 외부 링크는 host 를 거친다, [ADR-0527](../adr/0527-a-plugin-opens-external-links-through-the-host.md)). 그쪽은 [self-verification](self-verification.md) 의 가짜 브라우저 절차로 막는다. e2e 하네스는 같은 파일에 기록하는 가짜 `BROWSER` 를 준다(Linux·BSD 한정, [e2e-tests](e2e-tests.md) §3). 구현은 `crates/tasty-platform/src/debug_os_open.rs`(모듈 선언에 `#[cfg(debug_assertions)]`), 근거는 [ADR-0511](../adr/0511-os-open-is-recorded-not-launched-under-a-debug-switch.md). debug 격리, release 미노출.

### 회차 시간 예산 줄이기 (`TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS`)

IPC dispatch 한 회차의 시간 예산(제품값 16 ms)을 밀리초로 **줄여** 띄운다 — 제품값 이상은 버린다.
`0` 이면 회차마다 첫 명령 하나만 꺼내므로, 동시에 든 나머지 명령은 잘린 회차가 루프를 다시 깨우는
갈래(headless ADR-0465 · gui ADR-0413)로만 진척한다. 기본 예산에서는 시험의 동시 요청이 회차를
안 잘라 그 갈래를 실행 파일째로 지날 길이 없어 둔 손잡이다. 쓰는 곳은
`tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered_when_every_round_is_cut`. 부팅 뒤
처음 회차에서 한 번 읽는다. debug 격리(`#[cfg(debug_assertions)]`), release 미노출 — release 의
회차 예산은 늘 제품값이다.

### 문자 주입은 키 주입과 다른 채널이다

egui 가 **문자**를 받는 경로는 `Event::Text(String)` 하나뿐이고, `Event::Key` 는 그 문자를
만들지 않는다. 그래서 `debug.inject_egui_key` 로는 `TextEdit` 에 글자가 한 자도 안 들어간다 —
키 주입만 있던 동안 WM 없는 Xvfb 에서 **쿼리를 넣어 목록이 줄어든 화면**을 잴 방법이 없었고,
`xdotool type` 은 그 환경에서 키보드 포커스가 없어(`XGetInputFocus` 가 `PointerRoot` 를
돌려준다) 대신이 안 된다. `debug.inject_egui_text` 가 그 칸이다.

- **문자열 전체가 한 이벤트로** 들어간다. winit 의 `text` 필드가 `char` 가 아니라 문자열이라
  죽은키 조합 같은 실입력도 여러 문자를 한 이벤트로 나른다 — 문자마다 쪼개면 실입력이 만들지
  않는 순서가 된다. 키를 하나씩 누른 사실이 필요하면 `debug.inject_egui_key` 를 그만큼 부른다.
- **제어문자와 private use area 문자는 거절한다**(`injected: false`). 두 끝이 각각 거절할
  이유를 갖는다 — 보내는 끝인 `egui-winit` 은 그런 문자열을 애초에 `Event::Text` 로 올리지
  않고, 받는 끝인 `TextEdit` 은 빈 문자열과 `"\n"`·`"\r"` 를 **조용히 버린다.** 거르지 않으면
  `injected: true` 를 받고도 화면이 안 바뀌어, 이 채널이 메우려던 사각(주입을 믿고 찍은
  스크린샷이 빈 쿼리다)이 그대로 돌아온다. Enter·Tab·Backspace 는 `debug.inject_egui_key` 쪽이다.
- 대상은 **지금 포커스를 가진 위젯**이다. 창을 ID 로 지목하지 않으므로 이 메서드는 다른 주입과
  같이 focused window step(`src/app/ipc/window_required.rs`)에 산다.

CLI 는 `tasty debug inject egui-text --text <s>` 다. command palette 를 열어 쿼리를 넣는
전체 절차는 이렇다(`host-popup open` 이 사용자 단축키 경로를 대신한다):

```bash
tasty debug host-popup open --popup-id command_palette
tasty debug inject egui-text --text split      # {"injected":true}
tasty screenshot --window <id> --path /tmp/palette-filtered.png
```

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

`scroll_dx`/`scroll_dy` 의 뜻이 단위를 따라간다: `line` 은 줄 수(휠 한 칸이 1.0), egui
레벨의 `point` 는 논리 포인트, **winit 레벨의 `point` 는 물리 픽셀**이다(`PixelDelta` 가
물리 px 이고 수신 측이 scale factor 로 나눈다).

모르는 `unit` 값은 기본값으로 삼키지 않고 `-32602` 로 거절한다 — 오타를 대신 재면 검증이
의도한 것과 다른 경로를 재고도 통과한다.

## CLI 노출

CLI 도 동일하게 debug 빌드에서만 등록된다 — `DebugCommands`(`crates/tasty-cli/src/commands/debug.rs`)가 모듈째 `#![cfg(debug_assertions)]` — 실행부는 `crates/tasty-cli/src/local/debug.rs`(같은 cfg). 서브커맨드(일부 — 전체는 `tasty debug --help`): `info` · `cell-info` · `screen-attrs` · `glyph-color` · `ime-*` · `switch-input-source` · `raw-key`(주입에 macOS 손쉬운 사용 권한 필요 — 미승인이면 `surface.raw_key` 가 `permission_denied` 에러를 돌려준다. **두 서브커맨드는 모든 플랫폼에서 도움말에 뜨고 요청을 받는다** — 숨기면 "그런 명령 없음" 이 되어 이름을 의심하게 만드는데 이름은 맞다. 대신 도움말 첫 줄이 macOS GUI 전용임을 말하고, 다른 조합에서는 위 `-32015` 가 사유와 함께 즉시 돌아온다(CLI 가 그 메시지를 그대로 출력하고 exit 1 이다 — 실측). CLI 층에 플랫폼 조건을 넣지 않는 것이 그 결정이며 `src/source_guards/platform_gated_dispatch_complement.rs` 가 그 전제를 지킨다. [macOS 권한](../features/macos-permissions/index.md). `ime-*`/`switch-input-source`/`raw-key` 는 IPC 쪽도 debug 전용이다 — [ADR-0115](../adr/0115-input-reproduction-ipc-debug-isolation.md)) · `event-bus` · `extension` · `tool` · `popup` · `host-popup` · `modifier-hint` · `banner` · `settings` · `stream-echo` · `attach`. (`settings open [--tab <name>] [--subtab <key>]` → `debug.settings.open`; `settings apply --json '<obj>'` 또는 `settings apply --file <path>` → `debug.settings.apply`. 예: `tasty debug settings apply --json '{"general":{"workspace_categories_enabled":false}}'`. JSON 파싱/파일 읽기 에러는 CLI 단에서 1차로 잡아 종료하고, 서버는 `params.get("settings")` 가 object 임을 기대한다.)

### `tasty debug attach` (JSON-RPC 메서드 아님)

로컬 loopback self-attach 의 CLI 진입점 — **debug 전용**(`local/debug/attach.rs`). 로컬 self-attach 는 *사용자가 직접 하는 mirror 조작* 의 자동 재현 성격이라 release 표면에 없다. 원격 attach 는 release `tasty remote attach`. 둘 다 attach 세션 머신은 공용으로 보존되고, 로컬 진입점만 debug 로 격리된다. 결정 근거는 [ADR-0007](../adr/0007-attach-targets-remote.md), 메커니즘은 [attach-behavior.md](attach-behavior.md).

- 표면: `tasty debug attach [SURFACE] [--workspace <id>] [--dump-after <ms>] [--send <str>] [--send-to <sid>] [--raw] [--force-detach]`. `--ssh`/`--profile`/`--into-gui` 같은 원격 옵션은 없다(loopback 전용).
- stream 핸드셰이크 + framed 교환이라 JSON-RPC "메서드 목록" 표/`DEBUG_METHODS` 에 없다. force-detach 자체는 release JSON-RPC(`attach.force_detach`).

### `tasty debug stream-echo` (JSON-RPC 메서드 아님)

스트리밍 채널(`stream.open` 승격)의 server→client push 경로를 end-to-end 검증하는 CLI 전용 명령. raw framed 교환이라 메서드 표에 없다. *입력 재현이 아니라 transport 인프라 검증* 이지만, 사용자 검증 보조라 debug 표면에 둔다.

## 메서드 메타 등록

debug 메서드의 메타(`local_only()`)는 `crates/tasty-ipc/src/method_meta.rs::DEBUG_METHODS` 에 등록한다 (`METHOD_TABLE` 아님). 이 상수는 `#[cfg(debug_assertions)]` 일 때만 항목을 갖고, release 에선 **빈 슬라이스**로 컴파일된다.

새 debug IPC 추가 절차:

1. 핸들러를 `#[cfg(debug_assertions)]` (필요시 `+ feature = "gui"`) 로 감싼다.
2. `route_debug_handler` match 에 분기 추가 (PluginManager 필요하면 `ipc_step_debug`).
3. `DEBUG_METHODS` 표에 `(method, local_only())` 등록. 빠뜨리면 `tests/ipc_router_table_parity.rs` 가 잡는다 — 미등재도 plugin 호출자에겐 거부지만(`UnknownMethod`), 표만 봐서는 정책인지 누락인지 구분이 안 되므로 등재는 선택이 아니다([api-conventions](api-conventions.md) "권한 표 등재").
4. CLI 진입이 필요하면 `DebugCommands` variant 추가.
5. 본 문서 표에 추가. **`docs/` 의 에이전트용 release 문서에는 쓰지 않는다.**

## 디버그 코드 격리 정책 (필수)

기준 한 줄: *"이 코드를 통째로 지우고 컴파일 에러 몇 줄만 정리하면 디버그 기능이 깨끗이 사라지는가?"* 그게 되면 격리 OK.

- **debug 핸들러는 별도 파일에 모은다** — `src/adapters/ipc/handler/` 의 `debug.rs`(inject/host-popup/modifier-hint/banner 등 gui 상태) · `debug_plugin.rs`(event_bus/extension) · `tool.rs` · `popup.rs` · `input_source.rs`(macOS raw_key/switch_input_source) · `ime.rs`(surface.ime_*) 가 각각 `#[cfg(...)]` 로 모듈 선언된다. **파일 이름에 `debug` 가 들어갈 필요는 없다** — 기준은 "그 파일이 debug 핸들러만 담고 모듈 선언에 cfg 가 붙어 있는가" 다. 일반 핸들러 파일(`pane.rs`, `surface.rs` 등) 중간에 `#[cfg(debug_assertions)] fn debug_xxx()` 를 끼우지 않는다.
  - **예외 — gui 게이트 없는 debug 핸들러**: `debug.rs` 모듈은 `#[cfg(all(debug_assertions, feature = "gui"))]` 로 선언돼 headless 빌드에서 통째로 사라진다(그 모듈의 핸들러 다수가 `state.popups` / `state.banners` / `state.modifier_hint` 처럼 gui 에만 존재하는 필드를 만진다). 따라서 **gui 무관하게 headless 에서도 동작해야 하는 비-gui debug 핸들러**는 `debug.rs` 에 두지 않는다 — `#[cfg(debug_assertions)]` 만 건 형제 모듈로 뺀다 — 지금 셋이다: `debug_state.rs`(`ui.state` 의 `handle_ui_state`, `debug.settings.apply` 의 `handle_debug_settings_apply`) · `debug_nav.rs`(워크스페이스/탭 전환 3 종) · `debug_terminal.rs`(터미널 그리드 4 종 — `cell_info` / `screen_attrs` / `feed_bytes` / `glyph_color`). **판정 기준은 핸들러 본체가 gui 게이트된 심볼을 실제로 만지는가**이지, 그 메서드가 사용자 조작 재현인가가 아니다 — 사용자 조작 재현 여부는 debug/release 축이고 이미 `debug_assertions` 가 가른다. 그리고 "만진다" 의 판정은 **심볼이 하는 일**이지 심볼이 놓인 자리가 아니다: `debug.glyph_color` 가 부르는 색 해석 함수는 `CellAttributes` 와 색 타입만 쓰는 순수 함수인데 한동안 `#[cfg(feature = "gui")] mod gfx;` 아래 있었을 뿐이라, 함수를 복제하지 않고 그 파일을 게이트 밖(`src/cell_palette.rs`)으로 올렸다 — 렌더러와 **같은 함수**를 부르는 것이 그 메서드의 정의라 복제는 답이 아니다. 삭제 가능성(핸들러 fn + route 한 줄 + `DEBUG_METHODS` 한 줄 + CLI variant)은 그대로 유지된다.
- **외부 표면에 남는 cfg 가드는 router 분기 한 줄** (위 라우팅 코드의 `#[cfg(debug_assertions)] route_debug_handler(...)`).
- **삭제 가능성 테스트**: debug 파일을 지웠을 때 cfg-guard 호출처 몇 줄 제거 외에 다른 변경이 필요하면 격리가 깨진 것이다.

### 마우스를 다룬다고 debug 가 아니다 — 가르는 것은 조작이냐 상태 읽기냐

`debug.inject_window_mouse` 는 debug 고 `surface.mouse_tracking` 은 release 다. 이름이 둘 다
마우스지만 축이 다르다 — 앞은 **사용자 조작을 재현**하고, 뒤는 **터미널이 지금 어떤 상태인가**를
읽는다. 원칙 1 의 물음("에이전트가 자기 작업에 필요한가 vs 사용자 조작을 재현하는가")에
`surface.mouse_tracking` 은 앞쪽으로 답한다: 안의 프로그램이 마우스를 잡았는지 모르면
에이전트는 마우스 시퀀스를 보낼지 텍스트를 보낼지 정할 수 없고, 드래그 선택이 왜 안 먹는지도
가릴 수 없다. `surface.foreground_process`(셸이 유휴인가)와 같은 자리다.

★ 그리고 **그 release 표면은 두 값을 낸다.** 터미널 레지스터가 무엇인지(`terminal_mode`)와
마우스 핸들러가 그것을 존중하는지(`effective_click_mode`)는 다른 물음이고, 사이에 격하가
하나 있다 — hard 점유이거나 전경 프로세스가 마우스 캡처 블랙리스트에 걸리면 핸들러는 실제
모드와 무관하게 `None` 으로 취급한다(`crate::state::mouse::effective_click_tracking_decision`).
한 값으로 뭉개면 **낱말 하나가 두 축을 덮고**, 두 축이 어긋나는 기계에서 관측면이 거꾸로
읽힌다("터미널이 all_motion 인데 보고 0" 을 제품 결함으로 읽지만 사실은 격하가 정상 동작한
것이다). 그 격하 판정은 소비자가 둘(라우팅·관측면)이라 gui 게이트 밖(`src/state/mouse.rs`)에
두고 **같은 함수를 부른다** — 복제하지 않는 것은 위 `debug.glyph_color`/`cell_palette` 와 같은 처방이다.

**이 판정은 자동으로 안 난다.** `tests/ipc_release_table_excludes_input_reproduction.rs` 의
이름 규칙(`inject`·`raw_key`·`switch_input_source`·`ime_`·`simulate`)은 이 이름을 안 잡는다 —
그 가드가 스스로 적어 둔 사각지대("이름에 단서가 없고 debug CLI 진입점도 없는 새 release
메서드의 의미 판단")가 정확히 이 자리다. 그래서 판정 근거를 여기 남긴다.

### 예외: 데이터 구조의 dev-only 필드

매니페스트나 빌트인 spec 처럼 **데이터 구조의 필드 하나만 dev 전용**인 경우는 분리 대상이 아니다. *디버그 동작* 이 아니라 *dev 빌드 데이터 차이* 라 같은 룰을 적용하면 구조가 찢어진다.

## 관련

- [identity.md](../identity.md) — 원칙 1 ②(사용자 입력 재현 격리), 포커스 독립성
- [ADR-0007](../adr/0007-attach-targets-remote.md) — 로컬 attach 를 debug 로 격리한 결정
- [ADR-0115](../adr/0115-input-reproduction-ipc-debug-isolation.md) — OS 전역 입력 조작(`raw_key`/`switch_input_source`/`ime_*`)을 debug 로 격리한 결정 + `tests/ipc_release_table_excludes_input_reproduction.rs` 회귀 가드
- [ADR-0154](../adr/0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) — 플랫폼 게이트가 걸린 dispatch arm 은 `-32601` 이 아니라 `-32015` 와 사유로 답한다 + `src/source_guards/platform_gated_dispatch_complement.rs` 짝 가드
- [attach-behavior.md](attach-behavior.md) — attach 메커니즘
- [self-verification 독립 검증](self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다) — debug IPC 를 쓴 자체 검증
