# ADR-0497: 에이전트가 만든 창은 사용자의 포커스를 가져가지 않는다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: focus, window, multi-window, ipc, cli, user-agent-separation, identity, winit, x11, wayland, stacking
- **Group**: window-workspace-lifecycle

## Context

main 창을 등록하는 자리는 `App::register_window` 하나다. 이 결정 전에는 그 함수가 등록한 창을
**무조건** `focused_view_id` 로 세웠다. 창을 만드는 경로는 둘로 갈린다.

- 사용자 경로: 부팅 첫 창 · 단축키 · 명령 팔레트 · CSD 버튼 · 트레이 · macOS dock.
- 에이전트 경로: IPC `window.create` / `view.create`. CLI `tasty new window` 와 hook 의
  `ipc_sequence` 도 이 경로로 들어온다.

그래서 에이전트가 `tasty new window` 를 부르면 두 가지가 일어났다.

- `window.list` 의 `focused` 가 새 창으로 옮겨 갔다.
- 뒤이은 대상 없는 요청(`tasty new workspace` 등)이 사용자가 보던 창이 아니라 에이전트의 새 창에
  떨어졌다. IPC 라우터(`src/app/ipc/routing.rs`)가 대상이 없으면 `focused_view_id` 로 폴백하기
  때문이다.

그 폴백은 "대상 없는 요청은 사용자가 보는 창으로 간다" 는 약속이고, 그 창이 어디인지를 에이전트가
바꿔 버린 것이다. [정체성 원칙](../identity.md) 1(에이전트 행동의 부수효과가 사용자 포커스에 닿지
않는다)과 3(포커스는 사용자의 것이다)을 어긴다.

`create_new_window` 는 이미 `WindowRequestOrigin { User, Agent }` 를 받고 있었다. 창 생성 실패를
누구에게 알릴지 가르던 축이다([ADR-0117](0117-window-and-modal-creation-failure-policy.md) ·
[ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md)). 포커스를 옮기느냐도 같은
물음(이 창은 누가 원했나)이다.

OS 포커스는 별개 층이다. winit 0.30.13 의 `WindowAttributes::with_active(false)` 는 새 창이
포커스를 받지 않게 요청한다. 문서는 Android · iOS · X11 · Wayland · Orbital 에서 이것을
"Unsupported" 라 적는다. 실측(winit 0.30.13 소스)으로 X11 백엔드는 이 값을 읽지 않고, 창이
visible 이면 그대로 map 한다.

## Decision

**창을 만든 주체가 포커스를 정한다. 그 주체 값은 `WindowRequestOrigin` 하나다.**

- `register_window` 는 origin 을 받는다. 등록 뒤 `focused_view_id` 는 순수함수
  `focus_after_register` 가 정한다.
  - `User` 면 새 창으로 옮긴다. 부팅 첫 창도 `User` 다 — 사용자가 앱을 띄운 결과이고, 옮겨 갈
    이전 포커스도 없다.
  - `Agent` 면 옮기지 않는다. 단 가리키던 창이 없으면(main 창이 0 개였으면) 새 창을 잡는다.
    빼앗을 사용자 포커스가 없고, `None` 으로 두면 이후 대상 없는 IPC 가 전부 실패한다.
- 에이전트 창은 `with_active(false)` · `with_visible(false)` 로 만들고, 등록 뒤 사용자 창 뒤에
  보인다(아래 "보강"). 사용자 창은 `with_active(true)`(winit 기본값)에 보이는 채로 만든다 —
  이 결정 전과 같다. 속성 갈래는 순수함수 `origin_window_attributes` 하나다.
- 사용자가 에이전트 창을 직접 고르면 기존 `WindowEvent::Focused(true)` 추적 경로
  (`App::handle_window_focused`)가 `focused_view_id` 를 옮긴다. 이 결정은 그 경로를 바꾸지 않는다.

### 보강 — 에이전트 창은 사용자 창 뒤에 생긴다

처음 이 결정은 키 포커스와 `focused_view_id` 만 막았다. 그 뒤 winit 소스로 확인하니 macOS
(`orderFront`) · Windows(`SW_SHOWNOACTIVATE`)에서 그 창은 키 포커스 없이도 **사용자 창 위로
올라와 보였다.** 사용자 결정: **OS 마다 가능한 데까지 막는다** — 에이전트 창은 사용자가 보던 창
뒤(바로 아래)에 생기고 키 포커스도 안 가져간다. 사용자 창은 불변.

처음에는 X11 의 `_NET_WM_USER_TIME` 을 범위에서 뺐다(창 관리자가 있는 환경에서 잴 수 없어서).
openbox 실측으로 채택했다.

그 수단은 `tasty_platform::window_stacking::show_behind` 하나에 모았다. 기준 창(anchor)은
등록 시점의 focused main 창이고, 그것이 없으면 활성화 없이 보이기만 한다. winit 0.30.13 의
show 경로를 그대로 쓸 수 없는 자리가 있어 OS 마다 다르다.

- **macOS**: winit `set_visible(true)` 는 `makeKeyAndOrderFront` 다(키 창 + 맨 앞). 그래서 winit 을
  거치지 않고 `orderWindow:relativeTo:`(`NSWindowBelow`, 사용자 창의 `windowNumber`)로 보인다.
- **Windows**: winit 이 `WindowFlags::VISIBLE` 를 들고 있고, 그것이 꺼진 채 다른 플래그가 바뀌면
  `ShowWindow(SW_HIDE)` 를 부른다. 그래서 네이티브로 보이지 않는다. 보이기 전과 뒤에
  `SetWindowPos(hWndInsertAfter = 사용자 창, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE)` 를 걸고,
  보이는 것은 winit(`SW_SHOWNOACTIVATE`)으로 한다. 두 `SetWindowPos` 사이에 창이 맨 위에 보이는
  틈은 생기지 않을 것으로 본다 — winit 의 `set_visible` 은 이벤트 루프 스레드에서 동기로 돌고,
  `apply_diff` 는 `SW_SHOWNOACTIVATE` 만 부르며 z-order 를 올리는 호출이 없다. 실기 미측정이다.
- **X11**: map 전에 EWMH `_NET_WM_USER_TIME = 0`(초기 포커스를 주지 말라)을 건다. map 은
  winit 으로 한다(winit 이 보임 상태를 들고 있다). 그 뒤 `_NET_RESTACK_WINDOW`(detail `Below`,
  sibling = 사용자 창, source 2)로 사용자 창 아래를 **요청한다.** winit 의 Xlib 연결을 그대로 써
  map 요청 뒤에 순서대로 닿는다. 창 관리자가 받아들일지는 그쪽이 정한다.
  map 된 뒤에는 `_NET_WM_USER_TIME` 을 지운다(`clear_initial_focus_hint`). 값 0 이 남아 있으면
  나중에 사용자가 그 창을 고를 때 창 관리자가 "사용자 조작이 한 번도 없던 창" 으로 다룰 수 있다.
  지우는 시점은 그 창의 첫 `WindowEvent::Focused` 다 — winit 0.30.13 의 X11 백엔드는 MapNotify
  를 받는 자리(`event_processor` 의 map 처리)에서 그 이벤트를 내고, 앱이 받는 map 신호 중 가장
  이르다. map 전에 지우면 초기 포커스 힌트가 사라진다.
- **Wayland**: 클라이언트가 쌓임 순서나 포커스를 정할 프로토콜이 없다. xdg-activation 을
  요청하지 않는 것(winit `with_active(false)`)이 최선이라 보이기만 한다.

네이티브 호출이 실패하면 창 생성을 실패시키지 않는다. 경고를 남기고 winit 기본 경로
(`set_visible(true)`)로 보인다.

### 플랫폼별 결과

| 플랫폼 | tasty 의 `focused_view_id` | 키 포커스(OS) | 쌓임 순서 |
|---|---|---|---|
| macOS | 안 옮김 | 안 가져간다 — 키 창으로 만들지 않는다. 미측정 | 사용자 창 **바로 아래**에 둔다. 미측정 |
| Windows | 안 옮김 | 안 가져간다 — 활성화 없이 보인다. 미측정 | 사용자 창 **바로 아래**에 둔다. 두 `SetWindowPos` 사이에 위에 보이는 틈은 생기지 않을 것으로 본다(근거: winit `set_visible` 이 이벤트 루프 스레드에서 동기 · `apply_diff` 가 `SW_SHOWNOACTIVATE` 만 부름). 실기 미측정 |
| X11 | 안 옮김 | `_NET_WM_USER_TIME = 0` 으로 **요청한다.** openbox 3.6.1 에서 새 창은 포커스를 안 받았다(실측). map 뒤에는 그 속성을 지운다 | `_NET_RESTACK_WINDOW` 로 사용자 창 아래를 **요청한다.** openbox 3.6.1 에서는 사용자 창 **뒤**, 다만 바로 아래가 아니라 맨 아래였다(실측 — `Below` 의 sibling 을 안 쓰는 것으로 보인다) |
| Wayland | 안 옮김 | 컴포지터가 정한다(요청하지 않는다) | 컴포지터가 정한다 |

첫 열(tasty 가 대상 없는 요청을 보낼 창)은 모든 플랫폼에서 같다. 원칙 3 이 지키려는 것이 그
값이다. OS 포커스가 새 창으로 가도 사용자가 그 창을 실제로 조작하면(`Focused(true)`) 그때
옮겨 간다 — 사용자가 한 일이니 옳다.

X11 실측의 조건과 대조:

- 조건: Xvfb 위의 openbox 3.6.1, 격리 debug 인스턴스.
- 변이 대조:
  - user_time 설정을 빼면 새 창이 `_NET_ACTIVE_WINDOW` 를 가져갔다.
  - restack 요청을 빼면 새 창이 사용자 창 위에 쌓였다.
  - 즉 두 수단이 각각 포커스와 쌓임을 맡는다.
- user_time 삭제:
  - 새 창의 `_NET_WM_USER_TIME` 은 map 뒤 `xprop` 에서 없었다. 창은 여전히 사용자 창 뒤에
    쌓였고 `_NET_ACTIVE_WINDOW` 는 사용자 창에 남았다.
  - 그 뒤 debug `window.focus` 로 고르면 새 창이 맨 위로 오고 active 가 됐다.
  - 삭제를 빼는 변이에서는 map 뒤에도 `_NET_WM_USER_TIME = 0` 이 남았다. 쌓임 · active 는 같았다.
    user_time 설정을 빼면 포커스를 뺏긴다는 위 대조와 합치면, 그 속성은 map 시점에 있었고 삭제가
    그 뒤에 일어났다.
- 사용자 단축키로 만든 창은 맨 위에 쌓이고 포커스를 받았다.

### X11 에서 winit 이 주지 않는 것

winit 0.30.13 의 X11 확장(`WindowAttributesExtX11`)이 주는 것은 visual · screen · name ·
override-redirect · window type · base size · embed parent 다. 포커스 힌트
(`_NET_WM_USER_TIME` · `WM_HINTS.input` 류)를 주는 것은 없고, 백엔드도 `_NET_WM_USER_TIME` 을
쓰지 않는다. 그래서 위 X11 수단은 raw window/display handle 로 X 연결을 얻어 직접 쓴다.

가까워 보이지만 답이 아닌 것:

- `with_override_redirect(true)` 는 창 관리자를 통째로 우회한다. 장식 · 스태킹 · 포커스 관리를
  전부 잃는다.
- window type(utility · notification 등)은 창의 의미를 바꾼다.
- startup-notify 의 activation token 과 Wayland 의 xdg-activation 은 반대 방향이다. 포커스를
  **받으려는** 장치다.

## Consequences

- **얻은 것**:
  - 에이전트가 창을 만들어도 대상 없는 요청이 떨어지는 창이 바뀌지 않는다. 모든 플랫폼에서
    같다.
  - macOS · Windows 에서는 OS 포커스도 안 옮겨 가고, 창은 사용자 창 바로 아래에 생긴다.
  - X11 에서는 포커스를 주지 말라는 것과 사용자 창 아래에 두라는 것을 창 관리자에게 요청한다.
  - 포커스를 가르는 축이 실패 안내 축과 같은 값 하나라, 한쪽만 갈리는 사고가 안 난다.
- **잃은 것**:
  - `tasty new window` 직후 대상 없는 명령으로 새 창을 조작하던 스크립트는 이제 원래 창을
    조작한다. 새 창에 워크스페이스를 만들려면 그 창의 surface 를 `workspace.create` 의
    `surface_id`(CLI `tasty new workspace --surface`)로 지목한다 — `window_id` 는 창 자체를 다루는
    요청에만 쓴다([ADR-0514](0514-new-workspace-names-its-window-by-a-surface-and-keeps-no-env-default.md)).
    대상을 ID 로 지정하는 것은 원칙 3 이 원래 요구하던 형태다.
  - X11 은 요청일 뿐이라 창 관리자가 무시할 수 있다. openbox 처럼 `Below` 를 "맨 아래" 로 다루는
    창 관리자에서는 사용자 창 바로 아래가 아니라 맨 아래에 생긴다.
  - Wayland 는 컴포지터가 정한다. 그곳과 요청을 무시하는 X11 창 관리자에서는 OS 포커스와
    tasty 의 `focused_view_id` 가 잠시 어긋날 수 있다. winit 이 `Focused(true)` 를 전하면 추적
    경로가 따라가므로 어긋남은 그 이벤트 전까지다.
- **운영 비용 / 유지 부담**:
  - 창을 만드는 새 경로는 `WindowRequestOrigin` 을 정해 넘겨야 한다. `register_window` 의
    인자라 빠뜨리면 컴파일이 안 된다.
  - OS 별 네이티브 코드 세 벌(`window_stacking`)을 winit 을 올릴 때마다 winit 의 show 경로와
    대조해야 한다. 이 머신에서 실행을 잴 수 있는 것은 X11 뿐이다.
  - 에이전트 창이 결국 보이는지(X11 map state `IsViewable`)는 gui e2e
    `multi_window_owner_routing` 이 본다. `show_agent_window` 호출을 빼는 변이로 빨개지는 것을
    확인했다. 쌓임 순서와 OS 포커스는 창 관리자에 달려 있어 그 e2e 가 보지 않는다. CI 에서는
    `crossplatform-check` 의 관측용(비차단, `continue-on-error`) gui e2e 스텝이 돌린다 —
    `check-headless` 는 이 이름을 skip 한다.

## Alternatives Considered

- **(b) OS 동작을 받아들인다** — 새 창이 포커스를 가져가는 것을 플랫폼 관례로 두고 tasty 는
  손대지 않는다. 사용자가 기각했다. OS 포커스 쪽은 X11 · Wayland 에서 실제로 이 상태가 남지만,
  `focused_view_id` 까지 따라가게 두면 에이전트의 창 생성이 대상 없는 요청의 목적지를 바꾼다.
  그것은 OS 가 강제하지 않는 tasty 자신의 선택이다.
- **키 포커스만 막고 창은 앞에 보이게 둔다** — 처음 이 결정의 범위였다. 사용자가 "가능한
  데까지 막는다" 로 넓혔다. 앞에 올라온 창은 사용자가 보던 내용을 가린다.
- **Windows 에서 네이티브 `SWP_SHOWWINDOW` 로 한 번에 보인다** — 깜빡임 틈이 없다. 그러나 winit
  의 `VISIBLE` 플래그가 꺼진 채 남아, 뒤에 다른 창 플래그가 바뀌면 winit 이 `SW_HIDE` 로 창을
  숨긴다.
- **X11 `_NET_RESTACK_WINDOW` 에 source 1(스펙대로의 응용 값)을 쓴다** — openbox 3.6.1 이
  "invalid source indication 1" 로 버린다(실측). 그래서 source 2 를 쓴다. 다른 창 관리자는
  미측정이다.
- **macOS 에서 winit `set_visible(true)` 뒤에 `orderWindow` 로 내린다** — winit 의 show 가 이미
  키 창을 만든 뒤라 키 포커스를 뺏는다.
- **에이전트 창은 `focused_view_id` 가 `None` 이어도 안 잡는다** — 규칙은 더 단순하다. 그러나
  main 창이 0 개인 상태(macOS 에서 전부 파킹 · 트레이)에서 에이전트가 창을 만들면 이후 대상 없는
  IPC 가 전부 실패한다. 빼앗을 포커스가 없는 자리에서 원칙이 지키는 것이 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `Cargo.lock` 의 winit 이 0.30.13 이 아니게 됐다. 재는 법: 새 버전 소스에서 세 가지를 본다.
  - 각 OS 의 `set_visible(true)` 가 무엇을 부르는가 — macOS `makeKeyAndOrderFront` · Windows
    `apply_diff` 의 `VISIBLE` 처리 · X11 map + `ABOVE`.
  - `WindowAttributes::with_active` 문서의 플랫폼 절.
  - `platform/x11.rs` 의 `WindowAttributesExtX11` 목록.
- 창을 만드는 경로가 새로 생겼는데 사용자/에이전트 둘로 안 갈린다(예: plugin 이 IPC 를 거치지
  않고 창을 만든다).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 에이전트 창이 포커스를 가져가 타이핑이 새 창으로 샌다는 보고, 또는 사용자 창 위에 뜬다는
  보고가 온다(어느 OS 든). 재는 법: 그 OS 에서 한 창에 타이핑하는 중에 `tasty new window` 를
  부르고, 키 입력이 어느 창에 들어가는지와 새 창이 위에 보이는지를 본다. X11 이면
  `xprop -root _NET_CLIENT_LIST_STACKING` · `_NET_ACTIVE_WINDOW` 로 잰다.
- 반대 방향: 에이전트 창을 트레이 · 알림으로 불러도 앞으로 안 온다는 보고가 온다(X11). map 뒤
  `_NET_WM_USER_TIME` 삭제가 창 관리자가 map 시점에 캐시한 값을 못 바꾼 경우다. 재는 법: 그 창
  관리자 아래에서 `tasty new window` 로 만든 창을 트레이 · 알림으로 불러
  `xprop -root _NET_ACTIVE_WINDOW` 가 그 창으로 바뀌는지, `xprop -id <창>` 에
  `_NET_WM_USER_TIME` 이 없는지 본다.
- source 2 의 `_NET_RESTACK_WINDOW` 를 거부하거나, 응용이 2 를 보내는 것을 벌하는 창 관리자가
  보고된다. 재는 법: 그 창 관리자 아래에서 `tasty new window` 뒤
  `xprop -root _NET_CLIENT_LIST_STACKING` 으로 새 창이 사용자 창 아래인지 보고, 창 관리자 로그에
  source 관련 경고가 찍히는지 본다.

## References

- [포커스 정책](../design/policies/focus.md) — 에이전트 창 생성과 포커스
- [멀티 윈도우 아키텍처](../architecture/multi-window.md)
- [정체성](../identity.md) — 원칙 1 · 3
- [ADR-0117](0117-window-and-modal-creation-failure-policy.md) ·
  [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md) — 같은 `WindowRequestOrigin`
  축이 실패 안내 채널을 가른다
- 코드 근거(결정이 실현된 현재 위치): `App::register_window` · `App::create_new_window` ·
  `focus_after_register` · `origin_window_attributes` · `show_agent_window`
  (`src/app/window_lifecycle.rs`), `show_behind` · `clear_initial_focus_hint`
  (`crates/tasty-platform/src/window_stacking.rs`),
  `WindowRequestOrigin`(`src/app/event.rs`), `App::handle_window_focused`(`src/app/event_handler.rs`)
