# ADR-0102: webview 자식 창이 잡은 키는 host 로 포워딩한다 — 페이지 소유 범위는 `KeybindingSettings` + plugin 명령 레지스트리에서 도출한다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: webview, keyboard, shortcuts, keybindings, focus, cross-platform, markdown, html
- **Group**: plugin-render

## Context

`rendering = "webview"` 를 선언한 surface kind(`markdown` = `com.tasty.markdown`,
`html` = `com.tasty.html`)는 세 OS 모두 **winit 창과 별개의 OS 자식 창/뷰** 위에 그려진다
(X11 child window + WebKitGTK / WKWebView subview / child HWND + WebView2). 그 자식이
키보드 입력을 받으면 winit 최상위 창은 `WindowEvent::KeyboardInput` 을 받지 못한다.

> **출처 — OS 마다 다르다.** 아래 셋은 접히지 않는다.
>
> - **Linux — 실측이다.** 2026-09-07, Xvfb + WebKitGTK 4.1 에서 같은 창을 두 채널로 동시에
>   찍어 갈랐다: wgpu 스왑체인 readback(`ui.screenshot`)에는 host 가 그린 경계 배경
>   ("WebView region" + URL)만 담기고 OS 화면 캡처(`scrot`)에만 렌더된 페이지가 나왔다.
>   **스왑체인 밖의 별개 OS 창이라는 것이 그 차이다**(채널별 대상 표는
>   [`ai-verification/screenshot-methods`](../ai-verification/screenshot-methods.md)).
> - **macOS — 안 쟀다.** 이 문장을 참으로 만드는 것은 백엔드가 따로 있다는 **소스 모양**
>   (`src/host_api/webview/macos.rs` 의 WKWebView subview)과, 그 조합이 컴파일된다는 것
>   (`.github/workflows/crossplatform-check.yml` 의 `check-macos`)뿐이다. 그 잡은 컴파일까지만
>   본다 — 자식 뷰가 실제로 별개 창으로 그려지는지는 **그 채널 밖이다**.
> - **Windows — 안 쟀다.** 같은 모양이다(`.../windows.rs` 의 child HWND + WebView2 ·
>   `check-windows`). 컴파일까지만이라는 한계도 같다.
>
> **다시 재려면**: macOS·Windows 각각에서 GUI 를 띄우고 webview surface 를 연 뒤 같은 두 채널로
> 동시에 찍어 대조한다 — Linux 에서 한 것과 같은 절차다.

그 결과 tasty 의 전역 단축키 경로(`view/main/keyboard.rs` → `adapters/ui/input/shortcuts/`)가
**통째로 도달 불가능**해진다. Linux/X11 debug 인스턴스(Xvfb, WM 없음)에서 실측한 경계는
다음과 같았다.

- 터미널 탭(대조군): split / workspace 전환 / zoom 3 종 모두 동작.
- markdown 탭 + 포인터가 사이드바 위(webview 밖): 3 종 모두 동작.
- markdown 탭 + **포인터가 webview 위**(클릭하지 않아도): `handle_keyboard_input` 진입
  로그가 **한 줄도 찍히지 않고** 3 종 모두 무반응. X11 은 포인터가 포커스 창의 자손 창
  위에 있으면 키를 그 자손 창에 넣기 때문이다 — 즉 경계는 "문서 클릭 여부" 가 아니라
  **포인터가 webview 위인지**다.
- 문서를 클릭한 뒤: 같은 무반응에 더해, 클릭이 winit 에 도달하지 않아
  `try_click_to_activate` 자체가 실행되지 않고 모델 포커스(`focused_surface`)가 이전
  surface 에 남았다.
- 이 동안 `base.focused` 는 계속 `true` 였다 — 최상위 창은 활성인 채 `KeyboardInput` 만
  끊긴다. `WindowEvent::Focused` 는 이 문제의 진단 근거가 되지 못한다.

ADR-0065(markdown 의 webview 전환)와 ADR-0067(Stage B 스코프 정정)은 이 항목을 다루지
않았다 — 의도적으로 수용한 트레이드오프가 아니라 결정되지 않은 채 남은 공백이었다.
한편 CLAUDE.md 의 단축키 정책("모든 단축키는 `KeybindingSettings` 로 노출되며 코드에
하드코딩되지 않는다")이 전제하는 것은 *등록된 단축키가 실제로 동작한다* 는 것이다.
surface kind 에 따라 사용자가 설정한 단축키의 대부분이 조용히 죽는 상태는 설정 UI 가
약속한 것과 어긋난다.

## Decision

**세 백엔드가 native 키 이벤트를 가로채 host 로 포워딩한다.** 백엔드는 자기 native 키
표현을 공통 계약 `WebViewKeyEvent { surface_id, key: winit::keyboard::Key, mods:
ModifiersState }` 로 정규화해 창마다 하나뿐인 `WebViewKeyBridge` 에만 올리고,
**"host 가 가져가는가" 판정은 그 브리지 한 곳에서만** 한다(백엔드별로 우선순위 규칙이
갈라지지 않게). 판정은 콜백 안에서 **동기적**이라 백엔드는 그 자리에서 페이지 전파를
막을지 정하고, 실제 액션 실행만 host 가 다음 프레임에 큐를 비우며 수행한다 — 페이지와
host 가 같은 키를 이중 처리하는 경로가 없다.

**host 가 가져갈 콤보 집합은 `KeybindingSettings` 와 plugin 명령 레지스트리에서 전량
도출한다.** 고정 액션 필드(`GENERAL_BINDING_FIELDS`) + quick-switch 3 축(축 modifier 와
슬롯/다음/이전 raw 키의 합성, `INDIVIDUAL_SWITCH_MODIFIER` sentinel 축은 완성 콤보 그대로)
+ 사용자 스크립트 바인딩 + **활성** plugin 명령의 effective binding(매니페스트
`default_keybinding` / 사용자 override / host 액션 상속). 포워딩 계층에 키 콤보 리터럴은
하나도 없다. 그 집합을 두 축으로 거른다.

1. **modifier 를 가진 콤보만** 가져간다(`ctrl` / `alt` / `option` 중 하나 이상). 수식
   없는 키와 `shift` 만 붙은 키는 페이지 소유로 남는다 — 문서 안 텍스트 입력(주소창
   `#tasty-addr-input`, find 바)의 타이핑, IME 조합, 폼 내비게이션(Tab/Enter/화살표),
   페이지의 Esc 처리가 그대로 살아있어야 하기 때문이다.
2. **페이지 예약 액션은 제외**한다 — `find` / `copy` / `cut` / `paste` / `select_all`.
   페이지가 자체로 같은 의미를 구현하는 액션은 브라우저와 동일하게 페이지가 갖는다.
   제외 목록은 콤보가 아니라 **액션 field id** 라, 사용자가 그 액션의 콤보를 바꿔도
   규칙이 따라간다. plugin 이 같은 콤보를 바인딩해도 그 콤보는 페이지가 갖는다 — 예약은
   콤보의 출처가 아니라 **콤보 자체**에 걸린다. 이 대조는 **콤보 동등성**으로 한다:
   plugin 매니페스트는 raw 문자열(`"Ctrl+F"`)이라 사용자 설정(`"ctrl+f"`)과 대소문자·
   modifier 순서가 다를 수 있으므로, 실제 매칭(`matches_binding`)과 **같은 파싱 경로**
   (`shortcuts::bindings_equivalent` → `parse_binding`)로 정규화해 비교한다 — 원시 문자열
   비교였다면 `"Ctrl+F"` 가 필터를 통과해 페이지의 find-in-page 를 죽였을 것이다.

**plugin 바인딩은 scope 로 미리 거르지 않되, 비활성 plugin 은 제외한다.** 스냅샷은 **활성**
plugin 의 모든 명령을 담는 **상위집합**이다(scope 무관). 포워딩된 키가 host 에 도착하는
시점의 모델 포커스가 그 webview 자신일 수도(→ 그 plugin 의 Surface scope 명령이 후보) 다른
surface 일 수도(→ 모든 plugin 의 Global scope 명령이 후보) 있어, 브리지가 claim 하는 시점에는
어느 쪽인지 확정할 수 없다 — 그래서 **scope 는** 미리 거르지 않는다. 반면 **비활성 plugin 의
명령은 발화 자체가 불가능**하므로 claim 하면 그 키가 페이지에도 host 에도 가지 않고 사라진다
— registry 가 비활성 plugin 명령까지 담는 것은 설정 UI **표시**의 요구지(사용자가 미리 키를
잡아둘 수 있게) claim 의 근거가 아니라, `all_command_bindings` 쪽만 `is_disabled` 로 거른다.
최종 우선순위 판정은 기존 host 소비 경로(`dispatch_plugin_shortcut_key`,
[`key-mapping.md`](../design/policies/key-mapping.md))가 winit 경로와 동일하게 한다 —
포워딩은 키를 **도달시키기만** 하고 우선순위 규칙을 따로 갖지 않는다.

**정책 스냅샷은 매 프레임 재생성하지 않는다.** `sync_webviews` 는 `KeybindingSettings` 값과
**두 개의 epoch**(`PluginCommandRegistry::revision()` / `PluginsConfig::shortcut_revision()`)를
직전 스냅샷과 비교해, 달라진 프레임에만 다시 만든다. epoch 는 **프로세스 전역 단조 증가**
(`AtomicU64`)다 — plugin 재적재 시 `PluginCommandRegistry` 가 통째로 교체되므로 인스턴스
지역 카운터였다면 0 으로 되돌아가 stale 스냅샷이 갱신되지 않은 채 남는다.

**비라틴 레이아웃은 physical key 폴백으로 맞춘다.** 백엔드는 정규화한 논리 키와 함께
**하드웨어 키코드**를 `PhysicalKey` 로 실어 올린다(Linux/X11 은 `hardware_keycode − 8` =
evdev, Windows 는 확장 비트를 합친 16 비트 스캔코드, macOS 는 `kVK_*` virtual keycode —
셋 다 winit 의 `PhysicalKeyExtScancode::from_scancode` 가 받는 표현이다). 브리지는 winit
경로(`MainView::shortcut_lookup_key`)와 **같은 규칙**을 적용한다: `ctrl`/`super`/`alt` 중
하나라도 눌렸으면 US 배열 기준 문자로 치환하고, 아니면 레이아웃이 낸 논리 키를 그대로
쓴다. 수식 없는 키에는 적용하지 않으므로 페이지의 텍스트 입력·IME 조합은 영향을 받지
않는다.

**key-up 은 어느 백엔드도 올리지 않는다.** host 는 modifier 상태를 이벤트에 실려온
값으로만 읽고 자기 `base.modifiers` 를 갱신하지 않으므로, "modifier down 은 webview /
up 은 host" 경계에서 상태가 눌린 채 남는 stuck 이 생기지 않는다.

**auto-repeat 필터는 플랫폼이 갈린다.** macOS(`NSEvent.isARepeat`)와
Windows(`COREWEBVIEW2_PHYSICAL_KEY_STATUS.WasKeyDown`)는 press 이벤트에 repeat 표시가
있어 걸러낸다. **Linux/GDK 는 걸러내지 않는다** — GTK 의 `key-press-event` 는 auto-repeat
을 일반 press 와 구분할 플래그를 주지 않고, press/release 쌍을 직접 세는 대체 판정은
X 서버의 detectable auto-repeat 설정에 따라 결과가 달라져 **정상 press 를 repeat 로
오분류해 삼킬 위험**이 있다(누락은 중복 발화보다 나쁘다). host 단축키는 전부 edge 동작
이라 반복 발화가 사용자가 키를 누르고 있는 동안의 의도와 어긋나지 않고, winit 터미널
경로도 repeat 를 걸러내지 않으므로 tasty 내 다른 경로와도 일관된다.

**모델 포커스 동기화는 클릭에만 붙인다.** 백엔드가 native 클릭/포커스 획득을 관측하면
`surface_id` 를 브리지에 통지하고 host 가 `focused_pane`/`focused_surface` 를 맞춘다.
키 도착은 포커스 이동 근거로 쓰지 않는다 — X11 은 포인터가 자식 창 위에 있기만 해도
키를 넣으므로, 키를 근거로 삼으면 tasty 에 없는 focus-follows-mouse 가 생긴다.

**overlay 개폐 시 포커스는 회수만 하고 복원하지 않는다.** egui overlay 가 열려
webview 를 숨길 때 백엔드가 키보드 포커스를 host 창으로 되돌린다(숨기는 것과 포커스를
놓는 것은 세 OS 모두 별개다). 닫힐 때 자동 복원은 하지 않는다 — host 가 native 자식으로
키보드 포커스를 밀어넣는 것은 사용자 포커스 조작의 재현이고
([`focus.md`](../design/policies/focus.md)), 키 포워딩이 있는 지금은 문서를 다시
클릭하지 않아도 단축키가 그대로 동작한다.

**회수는 무조건이 아니라 두 겹의 조건 게이트를 통과할 때만 한다.** overlay 는 IPC 로도
열릴 수 있어(에이전트 행동), 무조건 회수하면 tasty 가 활성이 아닌 상황에서 다른 앱이
쥔 OS 키보드 포커스를 빼앗는다 — 에이전트 행동이 사용자 포커스에 닿는 것이라 불가침
원칙 1 위반이다([`identity.md`](../identity.md)).

1. **창 단위** — 호출부(`sync_webviews`)가 `base.focused == true` 일 때만 회수를 시도한다.
2. **뷰 단위** — 백엔드가 "포커스가 실제로 내 자식 안에 있는가" 를 각자 확인한다.
   Linux 는 `XGetInputFocus` 결과에서 부모 체인을 거슬러 자기 `x11_window` 를 만나는지,
   Windows 는 `GetFocus()` 가 자기 HWND 이거나 `IsChild` 인지, macOS 는 창의
   `firstResponder` 가 자기 자신이거나 하위 뷰인지(`isDescendantOf:`)를 본다.

macOS 의 회수 대상은 `nil` 이 아니라 **창의 `contentView`(= winit 뷰)** 다. `nil` 을
넘기면 창 자신이 first responder 가 되어 winit 뷰가 응답자 체인에서 빠지고 키보드가
통째로 죽는다.

## Consequences

- **얻은 것**: 사용자가 설정한 단축키가 surface kind 와 무관하게 동작한다. 규칙이
  백엔드가 아니라 계약 한 곳에 있어 세 OS 가 갈라지지 않는다. 새 webview kind 가
  추가돼도 자동으로 적용된다. 클릭이 모델 포커스에 반영되어, 단축키가 사용자가 보고
  있는 surface 를 대상으로 동작한다.
- **잃은 것**: 페이지가 자기 것으로 쓰던 modifier 콤보 중 host 바인딩과 겹치는 것은
  이제 페이지에 도달하지 않는다(예약 5 종 제외). host 가 콤보를 claim 했는데 그 프레임의
  게이트(모달/무대/kind 게이트)가 거절하면 그 키는 페이지에도 가지 않고 소실된다.
- **plugin 명령 단축키도 같은 경로를 탄다**: 매니페스트 `default_keybinding` 과 사용자
  override 가 정책 스냅샷에 합류하므로 webview 에 포커스가 있어도 도달한다. 다만 도달
  이후의 우선순위는 winit 경로와 **같다** — 모델 포커스가 어떤 plugin surface 에 있으면
  그 plugin 의 명령만 후보라, 다른 plugin 의 Global 명령 콤보는 스냅샷이 claim 하지만
  발화하지 않고 소실된다. 이는 포워딩이 새로 만든 손실이 아니라
  [`key-mapping.md`](../design/policies/key-mapping.md) 의 기존 우선순위 규칙이 그대로
  드러난 것이다(같은 상황을 wgpu 경로에서 재현해도 결과가 같다).
- **비라틴 레이아웃은 physical key 폴백으로 커버된다**: 세 백엔드 모두 하드웨어 키코드를
  함께 올리고 브리지가 winit 경로와 같은 폴백을 적용한다. 폴백 테이블은 winit 경로가 쓰는
  것과 **같은 함수**(`shortcuts::physical_key_to_logical`)라 두 경로가 갈라지지 않는다.
  US 배열 기준 문자 테이블에 없는 키(테이블 밖 `KeyCode`)는 폴백이 `None` 을 내고 논리
  키로 되돌아가므로, 그 범위에서는 여전히 레이아웃 의존이다.
- **운영 비용 / 유지 부담**: **Linux 에서만**, 그리고 **드러난 webview 가 있고 창이
  활성일 때만** 16ms 주기 폴링 tick 이 하나 걸린다(GDK 가 winit 과 다른 X 연결로
  이벤트를 받아 winit 루프를 깨우지 못하므로 GTK 를 그 tick 에서 non-blocking 펌프한다).
  webview 가 숨겨지거나 창이 최소화/비활성이 되면 tick 은 취소된다 — 배경 인스턴스가
  상시로 깨지 않게 하는 것이 조건의 목적이다. **Linux 한정은 조건부 컴파일이 아니라
  런타임 arm 분기다**: `Tick::WebviewKeyPoll` 변형과 `WEBVIEW_KEY_POLL_INTERVAL` 은
  `#[cfg(feature = "gui")]` 로만 게이트돼 세 OS 모두 컴파일되고 (출처: 세 OS **컴파일** 채널 — `.github/workflows/crossplatform-check.yml` 의 `check-macos`·`check-windows`·`check-headless`. **런타임 동작은 그 채널 밖이다** — integration·e2e 는 Linux 뿐이다), `reschedule_webview_key_poll`
  안의 `let arm = needs_poll && cfg!(target_os = "linux")` 가 Linux 에서만 tick 을 세운다
  (non-Linux 는 조건과 무관하게 `hub.cancel` 만 탄다). `cfg!` 는 컴파일타임 상수라
  최적화 단계에서 접히므로 macOS/Windows 의 폴링 비용은 실질 0 이고, native 키 콜백이
  winit 과 같은 OS 이벤트 루프에서 발화해 애초에 폴링이 필요하지도 않다. 플랫폼 조건부
  컴파일(`#[cfg(target_os = "linux")]`)이 걸린 곳은 GTK 펌프 호출
  (`app/webview_keys.rs` 의 `pump_gtk_events()`) 하나뿐이다. 백엔드 3 벌의 키 표현 변환
  테이블(GDK keyval / NSEvent charactersIgnoringModifiers / Win32 VK)을 유지해야 한다.

### 검증 범위

이 세션에서 실행 검증이 가능한 것은 **Linux/X11 뿐**이다. macOS/Windows 는 구현했으나
실기 미검증이다 — Windows 는 `cargo check --target x86_64-pc-windows-gnu` 로 타입
검증만, macOS 는 로컬에 크로스 툴체인이 없어 컴파일 검증도 하지 못했다. "코드가 같으니
동작할 것" 으로 검증 완료 처리하지 않는다.

physical key 폴백도 같은 경계를 따른다. **스캔코드 → `PhysicalKey` 변환의 실측은 Linux
뿐**이고, Windows(확장 비트 합성)·macOS(`kVK_*` 직접 전달)는 winit 의
`PhysicalKeyExtScancode` 구현을 읽어 표현이 일치함을 확인한 텍스트 검토 수준이다.
Linux/X11 에서는 `setxkbmap ru` 로 실제 레이아웃을 바꾼 뒤 webview 위에서 host 바인딩과
plugin 바인딩이 모두 매칭되는 것을 확인했다(키캡 위치는 같고 keysym 만 Cyrillic 인 상태).
같은 확인을 macOS/Windows 에서는 하지 못했다.

macOS 실기 검증 시 확인 순서는 다음과 같다.

1. **`release_keyboard_focus` 의 포커스 복구**(최우선). overlay 를 열고 닫는 동안 키보드가
   계속 살아 있는지, 그리고 tasty 가 비활성일 때 overlay 를 IPC 로 열어도 다른 앱의
   포커스를 빼앗지 않는지. 회수 대상이 `contentView` 인 것과 first-responder 게이트가
   이 항목의 핵심이다.
2. **`performKeyEquivalent:` 의 범위**. 이 메서드는 first responder 와 무관하게 창 전체
   뷰 트리로 내려오므로, 게이트가 없으면 터미널 입력 중의 Command 조합까지 webview 가
   먼저 집어 winit 경로와 이중 디스패치된다. 그래서 회수와 **같은 판정**
   (`firstResponder` 가 자기 자신이거나 하위 뷰)을 통과할 때만 가로챈다 — 통과하지 못하는
   상황은 키가 winit 뷰로 정상 도달하는 상황이라 포워딩이 필요하지도 않다.
3. **NSMenu 메뉴바와의 이중 발화**. macOS 는 NSMenu(`platform/macos_delegate.rs`)라는
   두 번째 입력 경로가 있어 `quit`/`new_window`/`minimize_window`/`maximize_window`/
   `close_window` 5 개 항목이 AppKit 의 key equivalent 로 먼저 디스패치될 수 있다. 그
   5 개는 AppKit 이 소비하므로 `performKeyEquivalent:` 오버라이드가 애초에 호출되지 않아
   이중 발화가 성립하지 않으리라 보지만, 실기로 확인해야 한다.

## Alternatives Considered

- **페이지 JS 가 키를 host 로 중계한다** — 기존 nav-fragment 채널
  (`location.hash = 'tasty-nav:...'`) 재사용은 부적절하다: URL/history/navigation 정책을
  건드리는 비동기 경로이고, `sandbox_scripts` 로 JS 가 꺼진 설정이나 원격 문서에는 적용되지
  않으며, markdown plugin 전용이라 `html` 등 다른 webview kind 를 커버하지 못한다. 전용
  script-message bridge 로 만들어도 신뢰 경계(페이지가 host 단축키를 위조 발화)와 비동기
  타이밍 때문에 열등하다.
- **현 동작을 의도된 것으로 확정하고 탈출 단축키 하나만 둔다** — "webview 콘텐츠에
  포커스가 있으면 그 페이지가 키를 소유한다(브라우저와 동일)" 를 정책으로 박는 안.
  그 탈출 키도 결국 백엔드 레벨 가로채기가 필요해 이 결정의 최소 부분집합을 구현하게
  되는데, 그러고도 사용자는 단축키를 쓸 때마다 탈출을 먼저 눌러야 한다. tasty 의 webview
  는 브라우저 탭이 아니라 **문서 뷰어 surface** 이므로 브라우저 관습을 그대로 가져올
  이유가 없다.
- **modifier 없는 콤보까지 전부 가져간다** — 사용자가 `F5` 같은 무수식 바인딩을 걸었을
  때도 동작하게 되지만, 페이지 텍스트 입력과 IME 조합, find 바의 Esc/Enter 를 host 가
  가로챌 위험이 생긴다. 안전 쪽을 골랐다.
- **키 도착으로도 모델 포커스를 옮긴다** — 분할 탭에서 비포커스 webview leaf 를
  hover 만 해도 포커스가 따라 움직여, tasty 에 없는 focus-follows-mouse 가 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 사용자가 modifier 없는(또는 shift 전용) 콤보를 webview surface 에서 쓰고 싶다는
  요구가 나온다 — 그때는 "페이지 입력 요소가 포커스인가" 를 백엔드가 알 수 있어야
  하므로, 페이지 상태 질의 경로(document-start 주입 bridge 등)를 다시 검토해야 한다.
- 페이지 예약 5 종 외에 페이지가 소유해야 할 액션이 생긴다(예: 페이지 자체 실행취소).
- plugin 명령을 **scope 별로** 미리 걸러야 할 이유가 생긴다 — 지금은 상위집합을 claim
  하므로, 포커스된 plugin 게이트에 걸려 소실되는 콤보가 페이지에도 가지 않는다. 이것이
  실사용에서 문제로 보고되면 브리지가 claim 시점에 모델 포커스를 참조하도록(계약에
  포커스 상태를 주입) 바꾼다.
- `physical_key_to_logical` 의 US 배열 테이블 밖 키에서 비라틴 레이아웃 미매칭이 보고된다
  — 그때는 테이블을 넓히거나, OS 레이아웃 질의(xkb / `UCKeyTranslate` / `ToUnicodeEx`)로
  런타임 매핑하는 방식을 검토한다. 이는 winit 경로와 공유하는 함수이므로 두 경로에 동시에
  적용된다.
- plugin 이 매우 많아 스냅샷 재생성 비용이 관측된다 — 지금은 두 epoch 가 바뀐 프레임에만
  전체를 다시 만든다. 그때는 증분 갱신(plugin 단위 delta)을 검토한다.
- macOS/Windows 실기 검증에서 NSMenu 이중 발화나 `AcceleratorKeyPressed` 미발화 등
  플랫폼 고유 어긋남이 확인된다.
- Linux 의 webview 폴링 tick(16ms)이 조건 게이트에도 불구하고 idle 전력/CPU 에서 문제로
  관측된다 — 그때는 GDK 키 콜백이 winit `EventLoopProxy` 로 루프를 직접 깨우는 방식으로
  바꾼다(폴링 제거).

## References

- [`docs/adr/0065-markdown-webview-render-channel.md`](0065-markdown-webview-render-channel.md) — markdown 의 webview 전환(Stage B). 키보드 항목 없음.
- [`docs/adr/0067-markdown-webview-stage-b-scope-correction.md`](0067-markdown-webview-stage-b-scope-correction.md) — Stage B 스코프 정정. 키보드 항목 없음.
- [`docs/design/policies/key-mapping.md`](../design/policies/key-mapping.md) — modifier 매핑의 위치 기반 추상화(macOS `alt`→Command / `option`→Option).
- [`docs/design/policies/focus.md`](../design/policies/focus.md) — 포커스 독립성 원칙.
- [`docs/identity.md`](../identity.md) — 불가침 원칙 1(사용자 행동 ↔ 에이전트 행동 분리). 포커스 회수 게이트의 근거.
- [`docs/plugins/markdown/screens/markdown.md`](../plugins/markdown/screens/markdown.md) — find-in-page 와 `kb.find` 게이트.
- [`docs/features/keybindings/index.md`](../features/keybindings/index.md) — 현재 동작 사양("webview surface 에서의 단축키" 절).
- [ADR-0385](0385-webview-backends-receive-their-host-contract-by-injection.md) — 이 결정이 정한 정책의 **입력 경계**. 정책은 콤보 목록을 주입받고, 어느 설정 필드가 host 액션·페이지 예약인지는 단축키 계층이 도출한다. 백엔드는 구체 브리지가 아니라 `WebViewKeySink` 를 받는다.
