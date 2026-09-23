# WebView 호스트 계약

native webview 는 winit 창 **안**에 얹히는 별개의 OS 자식 창/뷰다 — X11 child window ·
`WKWebView` subview · child `HWND`. 그래서 tasty 의 다른 UI 와 달리 **우리가 그리지 않는
픽셀**이 창 안에 있고, 그 픽셀의 위치·가시성·수명·키보드를 호스트가 밖에서 조종한다.

이 문서는 그 조종의 **입출력 계약**을 한 자리에 적는다. 결정의 근거는 ADR 에 있고 여기는
현재 운영 상태만 기술한다.

## 계약의 형태 — trait 이 아니라 이름이다

`PlatformWebView` 는 **세 개의 서로 다른 타입**이고, `src/host_api/webview.rs` 의 `cfg` 가
빌드마다 하나를 고른다. 공통 trait 은 없다. 세 타입은 같은 이름·같은 시그니처의 메서드
열둘과 `Drop` 을 노출하고, 호출부는 어느 것이 골라졌는지 모른 채 그 이름으로 부른다.

**이름 수준의 일치는 강제된다** — 호출부가 셋 모두에 공유되므로, 한 백엔드에 메서드가
없거나 시그니처가 다르면 그 OS 의 컴파일이 깨진다. 그 컴파일은 `crossplatform-check` 의
`check-macos` · `check-windows` · `check-headless` 세 잡이 main push · PR 마다 본다.

trait 을 두지 않기로 한 근거·대안·재검토 조건은 [ADR-0320](../../adr/0320-the-webview-backends-are-held-together-by-shared-call-sites-not-a-trait.md) 에 있다.

**뜻 수준의 일치는 강제되지 않는다.** "`set_visible(false)` 가 무엇을 하는가" 가 백엔드마다
갈라져도 셋 다 컴파일된다. 아래 표의 "백엔드 차이" 열이 지금 알려진 갈라짐 전부이고, 그
열이 맞는지는 사람이 읽어서 판정한다 — 재는 법은 각 백엔드의 해당 메서드 본문을 셋 다 열어
대조하는 것이고, 그것을 대신해 주는 채널은 없다.

## 표면 — 열둘 + `Drop`

| 연산 | 입력 | 출력 | 백엔드 차이 |
|------|------|------|-------------|
| `new` | 부모 창 handle · `WebViewBounds` · `scale_factor` · `surface_id` · `Rc<dyn WebViewKeySink>` | `Result<Self, WebViewCreateError>` | Linux 만 `HasDisplayHandle` 을 추가로 요구한다(X11 display 포인터). macOS 는 실패 셋을 전부 `Permanent` 로 분류한다 |
| `set_bounds` | `WebViewBounds` · `scale_factor` | — | macOS 는 Cocoa 가 논리 좌표를 그대로 받아 물리 변환을 쓰지 않는다 |
| `set_visible` | `bool` | — | |
| `release_keyboard_focus` | — | — | 회수는 조건부다 — 아래 "포커스" |
| `load_url` | `&str` | — | |
| `load_html` | `&str` | — | |
| `nav_state` | — | `NavState` | |
| `take_pending_navigations` | — | `Vec<PendingNavigation>` | `user_gesture` 는 Linux 가 `is_user_gesture`, Windows 가 `IsUserInitiated` 에서 옮긴다. macOS 는 늘 `false` 다 — 아래 "탐색" |
| `set_zoom` | `f64` | — | |
| `set_javascript_enabled` | `bool` | — | |
| `set_color_scheme` | `ColorScheme` | — | |
| `set_remote_content_allowed` | `bool` | — | Linux 는 WebKit content filter 로 막는다([ADR-0250](../../adr/0250-linux-blocks-remote-subresources-with-a-webkit-content-filter.md)) |
| `Drop` | — | — | 아래 "수명" |

`src/host_api/webview.rs` 의 머리 주석이 한때 이 표면을 "6 operations" 라고 적었다. 그 수는
lifecycle/geometry 만 센 것이고 키보드·탐색·페이지 설정 일곱이 빠져 있었다 — 그 자리는
이 표를 가리키도록 고쳤다.

## 생성 — 부모 handle 과 실패 분류

부모 창은 `raw-window-handle` 로 받는다. 백엔드마다 받아들이는 종류가 하나뿐이고, 다른
종류가 오면 **영구 실패**다.

- Linux: `RawWindowHandle::Xlib` 만. Wayland 는 지원하지 않는다.
- macOS: `RawWindowHandle::AppKit` 의 `ns_view`.
- Windows: `RawWindowHandle::Win32` 의 `hwnd`.

실패는 `WebViewCreateError` 가 **다시 시도할 가치가 있는가**로 가른다(`Transient` /
`Permanent`). 이 구분은 취향이 아니라 필수다 — 실패 경로가 X 창을 만들었다 지우면 그 X
이벤트가 이벤트 루프를 깨워 다음 시도를 스스로 부른다. 근거·실측은
[ADR-0159](../../adr/0159-a-null-gdk-window-is-a-value-not-a-crash.md) 와 그 타입에 붙은
주석에 있다.

## 좌표 — 논리와 물리를 타입 이름에 남긴다

`WebViewBounds`(논리) 와 `PhysicalWebViewBounds`(물리)는 `to_physical` / `from_physical`
로만 오간다. 생산자(레이아웃)와 소비자(플랫폼 창 API)가 각자 `* scale_factor` ·
`/ scale_factor` 를 손으로 적으면 한쪽만 고쳤을 때 조용히 어긋나기 때문이다. 두 타입이
`f32` 가 아니라 `f64` 인 이유(플랫폼 API 와 `scale_factor` 가 `f64` · 소비자가 `as i32` 로
절단)는 타입 정의에 붙어 있다. 왕복은 단위 시험이 세 OS 모두에서 고정한다.

## 스레드 — 셋 다 `!Send` 지만 강제의 세기가 다르다

세 타입 모두 `Rc` · raw pointer · COM 객체를 필드로 가져 **auto-trait 상 자연 `!Send`** 다.
안전한 Rust 로는 다른 스레드로 옮길 수 없다. 그 위에 얹힌 것이 백엔드마다 다르다.

| 백엔드 | 컴파일 시점 | 실행 시점 |
|--------|-------------|-----------|
| Linux | `!Send` | `assert_origin_thread()` 가 생성 스레드와 현재 스레드를 비교해 **release 에서도** panic 한다 |
| macOS | `!Send` + `MainThreadMarker::new()` 가 생성 자체를 main thread 로 제한 | 없음(생성이 이미 막는다) |
| Windows | `!Send` | 없음. `CoInitializeEx(COINIT_APARTMENTTHREADED)` 가 같은 스레드 가정을 세운다 |

Linux 만 실행 시점 그물을 더 가진 이유는 X11 핸들 오용이 UB 라서다 — debug 에서만 잡으면
release 에서 조용히 UB 가 난다.

## 수명 — 부모가 먼저 죽을 수 있다

자식 창의 수명은 `PlatformWebView` 값의 수명이고, `Drop` 이 OS 자원을 푼다. 부모 winit
창이 **먼저** 사라지는 경우가 실제로 있으므로 teardown 은 그 상태에서도 호스트가 더 할 일이
없게 끝나야 한다.

- Linux: GDK 에러 트랩을 걸고 정해진 순서로 푼다. 트랩은 abort 를 값으로 바꿔 X 에러를
  로그로 남긴다 — **순서 수정 뒤의 마지막 그물이지 순서 대신이 아니다**.
  [ADR-0248](../../adr/0248-webview-teardown-lets-gdk-finish-before-the-x-window-is-destroyed.md).
- macOS: `removeFromSuperview()` 하나. 나머지는 ARC 가 푼다.
- Windows: `controller.Close()` 후 `DestroyWindow`. 둘 다 이미 닫힌 경우를 `trace` 로만
  남기고 넘어간다.

## 탐색 상태 — 소유는 도메인 모델이다

`NavState`(`Idle` / `Loading` / `Done` / `Failed`)는 `tasty-model` 이 정의하고
(`tasty_model::NavState`), `host_api::webview` 가 재수출한다. 소비자가 셋이다 — 항상
컴파일되는 `RemoteSurface`(비-gui 빌드에도 있다)가 mirror 로 담고, gui 뒤의 native 백엔드가
쓰고, egui chrome 이 읽는다. 셋 중 어느 한쪽 파일에 두면 나머지가 그쪽 경로를 역참조하게
되므로 셋 모두의 아래인 도메인 모델에 둔다. OS·webview·egui 타입을 담지 않는 네 값짜리
enum 이다. `Copy` + `Default = Idle` 이라 native 백엔드의 `Rc<Cell<NavState>>` 에 그대로
들어간다. 근거는 [ADR-0385](../../adr/0385-webview-backends-receive-their-host-contract-by-injection.md).

호스트는 `nav_state()` 로 지금 상태를 읽고, `take_pending_navigations()` 로 페이지가
요청한 이동을 **소비**한다(읽으면 비워진다).

시도 하나(`PendingNavigation`)는 URL 과 **엔진이 그 시도를 사용자 제스처로 봤는가**
(`user_gesture`)를 함께 싣는다. 호스트는 시도를 소유 plugin 에 `webview.navigation_attempt` 로
통지하는 자리에서 시도마다 그 surface 의 기록을 다시 정한다 — 이 값이 참이고 그 surface 의 지금
페이지를 소유 plugin 이 썼으면(`webview.set_url` 호출자를 surface 가 기억한다) 그 plugin 에 묶어
기록하고, 아니면 그 surface 의 기록을 지운다. 작성자는 클릭 시점이 아니라 호스트가 시도를 drain 하는
시점에 읽으므로, 직전 drain 이후 작성자가 소유 plugin 이 아닌 쪽에서 소유 plugin 으로 바뀐 surface 는 그
프레임의 기록을 버린다(`RemoteSurface::take_webview_owner_takeover`). plugin 이 그 URL 을 `file_handler.dispatch` 의
`user_navigation_url` 로 되대면 그 호출 한 번이 사용자 행동이 된다 — 근거는 plugin 의 주장이 아니라
host 가 본 두 사실(엔진의 보고 · 페이지 작성자)이다. `webview.set_url` 은 에이전트에게도 열려 있어,
에이전트가 쓴 페이지 위의 클릭과 그 페이지의 스크립트가 낸 시도는 근거가 되지 않는다(재지 않은 예외
하나: 그 클릭의 시도가 소유 plugin 이 되찾은 프레임의 drain 뒤에야 도착하는 순서 — ADR "잃은 것"). macOS 는 공개 API 에 같은
뜻의 값이 없어 늘 `false` 이고, 그래서 macOS 의 webview 링크 클릭은 에이전트로 도착한다. 근거는
[ADR-0568](../../adr/0568-a-user-gesture-on-a-page-the-owning-plugin-wrote-makes-its-webview-file-dispatch-a-user-action.md).

## 키보드 — 별도 계약

자식 창이 OS 키보드 포커스를 잡으면 winit 은 `WindowEvent::KeyboardInput` 을 받지 못하고
호스트 단축키 경로가 통째로 도달 불가능해진다. 그 구멍은 한 곳에서만 메운다 — 세 백엔드는
자기 native 키 이벤트를 정규화해 `WebViewKeySink` 계약으로 올리고, 우선순위 판정은 그 호스트
구현 `WebViewKeyBridge` 에서만 한다. 판정은 동기, 실행은 다음 프레임이다. 전체 규칙은 그
모듈의 머리 주석과 [ADR-0102](../../adr/0102-webview-key-forwarding.md) 에 있다.

백엔드가 호스트에 대해 아는 것은 **두 메서드뿐이다.**

| 계약 | 입력 | 출력 | 누가 부르나 |
|------|------|------|-------------|
| `WebViewKeySink::capture_key` | `surface_id` · 레이아웃 문자 `Key` · 물리 위치 `PhysicalKey` · `ModifiersState` | `bool` — `true` 면 백엔드가 페이지 전파를 그 자리에서 막는다 | 백엔드, press·비repeat 에서만 |
| `WebViewKeySink::note_focus` | `surface_id` | — | 백엔드, native 클릭/포커스 획득에서 |

정책 교체(`set_policy`)와 큐 비우기(`take_pending` · `take_focus_requests`)는 브리지의 고유
메서드라 백엔드가 볼 수 없다. 정책(`HostShortcutPolicy`)은 **콤보 목록을 주입받는다** —
`ShortcutSources { host, page_reserved, plugin }` 셋이고, 키 모듈은 modifier 필터와 "페이지
예약과 동등한 plugin 콤보 제외" 두 축만 판정한다. 어느 설정 필드가 host 액션이고 어느 것이
페이지 예약인지는 단축키 계층(`adapters/ui/input/shortcuts/webview_claims.rs`)이
`KeybindingSettings` 에서 도출한다. 근거는
[ADR-0385](../../adr/0385-webview-backends-receive-their-host-contract-by-injection.md).

## 포커스 — 회수는 **조건부**다

`release_keyboard_focus` 는 host 가 egui overlay 를 열어 webview 를 가릴 때 부른다.
숨기는 것과 키보드 포커스를 놓는 것은 세 OS 모두에서 **별개**라, 회수하지 않으면 방금 연
popup 이 키를 못 받는다.

**무조건 회수하지 않는다.** 세 백엔드가 같은 두 단계를 각자의 OS API 로 구현한다.

1. **지금 포커스가 이 webview(또는 그 하위 창) 안에 있는가**를 먼저 묻는다. 아니면 아무것도
   안 하고 돌아온다.
2. 있으면 **부모 winit 창으로** 되돌린다. 다른 곳으로 옮기지 않는다.

| 백엔드 | 1 을 묻는 법 | 2 를 하는 법 |
|--------|--------------|--------------|
| Linux | `XGetInputFocus` — `None`(0)·`PointerRoot`(1) 은 특정 창이 아니라 회수 대상이 없다 | `XSetInputFocus(parent, RevertToParent)` + `XFlush` |
| macOS | `view_holds_first_responder(webview)` | `window.makeFirstResponder(contentView)` — 실패는 `warn` |
| Windows | `GetFocus()` 가 이 `HWND` 이거나 `IsChild` | `SetFocus(parent_hwnd)` — 실패는 창이 사라지는 중이라는 뜻이라 로그도 안 남긴다 |

1 단계가 **정책이지 최적화가 아니다.** 빼면 IPC 로 popup 하나를 여는 것만으로 다른 앱이
쥐고 있던 OS 키보드 포커스를 tasty 가 빼앗는다 — 에이전트 행동이 사용자 상태에 닿는 것이라
[불가침 원칙 1](../../identity.md) 위반이다. 창 자체가 활성인지는 호출부(`sync_webviews`)가
`base.focused` 로 한 번 더 건다. 즉 그물이 둘이고, 이 표의 1 단계가 안쪽 그물이다.

**같은 규칙이 세 벌로 적혀 있다.** 공유할 수 있는 것이 규칙 문장뿐이고 코드가 아니어서다
(AppKit first responder · X11 input focus · Win32 focus 는 서로 다른 API 다). 그래서 셋이
조용히 갈라질 수 있고, 그것을 잡아 주는 자동 채널은 없다 — 이 절이 그 세 벌의 정본이다.
재는 법: 세 백엔드의 `release_keyboard_focus` 와 그 짝인 포커스 조회 함수를 함께 열어
위 표의 두 단계가 다 있는지 본다.

## 도메인 라이브러리로는 아무것도 새지 않는다

`crates/` 의 어떤 크레이트도 webview 백엔드 의존(`wry` · `webkit2gtk` · `webview2-com`)을
선언하지 않고, `WKWebView`·`ICoreWebView2`·`webkit2gtk` 라는 낱말이 `crates/` 안에 나오는
자리는 전부 doc 주석 산문이다(2026-09-20 실측). 재는 법:

```bash
grep -lE '^(wry|webkit2gtk|webview2-com)' crates/*/Cargo.toml   # 빈 출력이어야 한다
git grep -nE 'webkit2gtk|WKWebView|ICoreWebView2|wry::' -- crates/
```

두 번째 명령은 산문 언급까지 잡으므로 결과를 눈으로 갈라 읽는다 — 타입 사용이 하나라도
있으면 계약이 깨진 것이다. 이 불변식을 지키는 자동 채널은 없다.

**egui 와 OS 창 타입은 따로 재야 한다** — 위 두 명령은 webview 백엔드만 본다. 2026-09-20
재측정:

```bash
grep -lE '^egui' crates/*/Cargo.toml                          # 13
grep -lE '^(winit|raw-window-handle)' crates/*/Cargo.toml     # 3
```

egui 열셋은 전부 UI 크레이트이거나 그 뒤가 feature 다(본체는 `gui` feature 에서만 켠다).
winit 셋은 `tasty-gallery`(GUI 바이너리) · `tasty-key-match` · `tasty-platform` 이다. 뒤엣것은 **winit 키
이벤트를 바인딩 문자열과 맞추는 것이 그 크레이트의 일**이라 의존이 우연이 아니고, 소비자가
전부 `gui` 뒤에 있어 본체가 optional 로 잡는다 — 헤드리스 그래프에 윈도잉 스택이 안 들어온다. `tasty-platform` 은 OS 경계 크레이트이고 winit 을 `gui` feature 뒤 optional 로만 켠다.
그래서 이 셋 중 **도메인 라이브러리에 해당하는 크레이트는 하나도 없다.** 그 판정은 사람이
한다. 세는 명령만 자동이고, "이 크레이트가 도메인 라이브러리인가" 를 답하는 채널은 없다.

## 이 문서가 못 말하는 것

- 세 백엔드의 **뜻**이 같은지. 위 표의 "백엔드 차이" 열은 소스를 읽어 채운 것이고, 셋이
  조용히 갈라지는 것을 잡아 주는 채널은 없다.
- macOS 백엔드의 동작. 이 레포의 Linux 개발 머신에서는 macOS 타깃이 컴파일되지 않으므로,
  여기 적힌 macOS 서술은 전부 **소스를 읽어** 쓴 것이다. 그 조합을 실제로 컴파일·실행하는
  것은 `crossplatform-check` 의 `check-macos` 잡이다.
