# WebView 호스트 계약

WebView는 winit 창 안에 놓이는 별도의 OS 자식 창이나 뷰다. Linux는 X11 child window, macOS는 WKWebView subview, Windows는 child HWND를 사용한다. 콘텐츠는 OS의 웹 엔진이 그리고 호스트는 위치·표시·수명·키보드 입력을 관리한다.

이 문서는 세 백엔드가 공통으로 제공할 동작을 설명한다. 선택 이유는 이 문서에 있다.

<a id="계약의-형태--trait-이-아니라-이름이다"></a>

## 공통 API와 플랫폼별 구현

PlatformWebView는 OS별 타입 중 cfg가 하나를 선택한다. 공통 호출부가 쓰는 메서드와 시그니처는
해당 OS 컴파일이 검사한다. 같은 빌드에서 런타임 다형성을 쓰지 않으므로 아래 표의 연산은 trait 없이
같은 이름의 메서드로 맞춘다.

예외는 오버레이 합성 확장(`WebViewOverlayBackend`)이다. 이 trait은 "지원 안 함" 기본 구현을 가진다.
구현하지 않은 OS도 그대로 컴파일되고, WebView 동기화는 시험용 backend로 같은 경로를 검사한다.
호출은 정적 디스패치이며 `dyn`을 쓰지 않는다. 계약은 아래 "egui 오버레이와 native WebView" 절에 있다.

컴파일 성공은 실제 크기·focus·navigation·종료 행동이 같다는 증거가 아니다.
백엔드 변경 때는 아래 계약으로 세 구현을 비교하고 해당 OS에서 동작을 확인한다.

<a id="표면--열둘--drop"></a>

## 제공하는 연산

| 연산 | 입력 | 출력 | 백엔드 차이 |
|------|------|------|-------------|
| `new` | 부모 창 handle · `WebViewBounds` · `scale_factor` · `surface_id` · `Rc<dyn WebViewKeySink>` · `Option<EntryResponseSource>` | `Result<Self, WebViewCreateError>` | Linux 만 `HasDisplayHandle` 을 추가로 요구한다(X11 display 포인터). macOS 는 실패 셋을 전부 `Permanent` 로 분류한다 |
| `set_bounds` | `WebViewBounds` · `scale_factor` | — | macOS 는 Cocoa 가 논리 좌표를 그대로 받아 물리 변환을 쓰지 않는다 |
| `set_visible` | `bool` | — | |
| `release_keyboard_focus` | — | — | 회수는 조건부다 — 아래 "포커스" |
| `load_url` | `&str` | — | Linux 는 로드가 끝난(`Done`) 상태에서 지금 문서의 `#fragment` 주소를 받으면 `Loading` 을 세우지 않는다. `Failed`·`Loading` 상태의 같은 경우와 표기 정규화 차이는 남은 한계다. WebKitGTK 가 이 이동을 load 신호 없이 처리해서다(측정, [html surface §문서 주소와 히스토리](../../surfaces/html/index.md#내부-동작)). macOS·Windows 는 이 경우를 따로 다루지 않으며 미측정이다 |
| `load_html` | `&str` | — | |
| `nav_state` | — | `NavState` | |
| `take_pending_navigations` | — | `Vec<PendingNavigation>` | `user_gesture` 는 Linux 가 `is_user_gesture`, Windows 가 `IsUserInitiated` 에서 옮긴다. macOS 와 Windows 의 같은 문서 fragment 이동은 그 뷰가 받은 OS 클릭으로 정한다 — 아래 "탐색" |
| `set_zoom` | `f64` | — | |
| `set_javascript_enabled` | `bool` | — | 스크립트 게이트가 붙은 webview 에서는 값을 전역 sandbox 설정으로 `gate.set_sandbox` 에 넘기고, 적용하는 값은 문서별 판정이다([HTML Viewer (`com.tasty.html`)](../../surfaces/html/index.md)). macOS 는 전역 preference 를 켜 두고 문서별 값을 navigation 정책의 webpage preferences 로 준다 |
| `attach_script_gate` | `ScriptGate` | — | 첫 로드 전에 부른다. Linux 만 page cache 를 꺼서 뒤로·앞으로 가기도 응답 단계를 거치게 한다([HTML Viewer (`com.tasty.html`)](../../surfaces/html/index.md)) |
| `reload` | — | — | |
| `go_back` · `go_forward` | — | — | 세션 히스토리로 이동한다. 사용자 탐색 큐와 에이전트 히스토리 큐가 부른다([html surface §사용자 탐색·에이전트 히스토리 이동](../../surfaces/html/index.md#내부-동작)) |
| `nav_info` | — | `WebViewNavInfo` | commit 주소의 출처는 Linux 가 WebKitGTK `load-changed` Committed, macOS 가 WKWebView `didCommitNavigation`, Windows 가 WebView2 `ContentLoading` 이다. 주소 선택은 공통 `committed_document_url`(로드가 끝났으면 엔진 현재 주소, 로드 중·실패면 마지막 commit 주소)이다. 이동 가능 여부는 WebKitGTK `can_go_back`·`can_go_forward`, WKWebView `canGoBack`·`canGoForward`, WebView2 `CanGoBack`·`CanGoForward` 다. Windows 는 조회 실패를 웹뷰별 첫 번째만 warn 으로 남긴다. macOS·Windows 는 실 기기 미측정이다([html surface §문서 주소와 히스토리](../../surfaces/html/index.md#내부-동작)) |
| `debug_stop` | — | — | debug 빌드 전용이다. 로드 중지를 재현한다. 사용자 진입점이 없는 조작이라 사용자 탐색 큐와 나눈다 |
| `set_color_scheme` | `ColorScheme` | — | |
| `set_remote_content_allowed` | `bool` | — | 전역 설정 값이다. 실제로 막을지는 이 값과 스크립트 게이트의 문서 단위 원격 허용을 OR 한 실효값으로 정한다(아래 "원격 리소스 차단") |
| `Drop` | — | — | 아래 "수명" |

## 생성 — 부모 handle 과 실패 분류

부모 창은 raw-window-handle로 전달한다. Linux는 Xlib만, macOS는 AppKit ns_view,
Windows는 Win32 hwnd를 받는다. 지원하지 않는 handle은 Permanent 오류다. Linux는 `DISPLAY`가 있으면 X11로 뜨므로([백엔드 선택](linux-display-backend.md)) Wayland 세션에서도 XWayland로 WebView를 표시한다. `DISPLAY`가 없는 네이티브 Wayland는 미지원이고 화면 안내 없이 빈 화면이다.

Linux는 winit Xlib 연결에서 자식창을 만든 뒤 XSync로 서버 처리를 기다리고 별도 GDK 연결에서 조회한다.
XFlush만으로는 연결 사이 순서가 보장되지 않는다.
foreign_gdk_window가 FFI NULL을 Err로 반환하며 panic하는 foreign_new_for_display 바인딩은 직접 쓰지 않는다.
실패하면 방금 만든 X 창을 정리한다. native menu의 XID 변환도 같은 helper를 사용한다.

WebViewCreateError의 Permanent는 즉시 중단하고 Transient는 최대 8 회 시도한다.
창 생성·파괴 이벤트가 다음 시도를 깨울 수 있어 무한 재시도를 하지 않는다.
소스 검사는 알려진 생성→동기화→조회 순서와 NULL 처리만 확인하며 모든 X 경합이나 자원 정리를 증명하지 않는다.

## 원격 파일 entry 응답 모드

`new`의 선택적 `EntryResponseSource`는 native context를 만들기 전에 정한다. `None`은 일반 WebView이고, `Some`은 원격 파일 entry의 완성된 수신 본문을 제공하는 모드다. source는 `EntryBodyStore`의 weak owner를 참조한다. 서버 파일 URL은 출처·주소 표시·스크립트 판정용이며 client 파일을 여는 명령으로 쓰지 않는다. 전송·크기·연결 계약은 [HTML 전송 계약](../../dev-guide/html-transfer.md), 문서별 스크립트 정책은 [HTML surface](../../surfaces/html/index.md)에 있다.

Linux는 이 모드마다 별도 ephemeral `WebContext`에 `tasty-html-entry` scheme을 등록한다. 하위 파일 협상이 없으면 callback은 현재 store의 정확한 entry URI(fragment 제외)에 대한 GET에만 수신·검사가 끝난 immutable 본문과 MIME을 응답한다. 협상한 경우 같은 문서 identity 아래 root 상대 URI의 GET을 bounded 비동기 대기에 연결하고, 완성된 immutable 응답만 제공한다. 가상 entry URL의 host는 connection epoch·surface·binding·document로 나누고 경로에도 request·transfer를 담는다. 다른 origin·identity·method는 실패시키며 client 파일·네트워크로 대체하지 않는다. navigation 정책은 인가 root 밖의 이동과 새 창 요청도 거절한다. 실제 main-resource identity로 분류한 하위 문서 이동은 이전 source를 retire하고 멈춘 뒤 새 전송·크기 판정·script scan으로 시작한다.

응답에는 `Content-Security-Policy`, `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`를 붙인다. entry 전용 CSP는 외부 script·CSS·HTTP(S)·상대 리소스·fetch·frame·worker 등을 허용하지 않는다. 하위 파일 모드의 CSP는 같은 origin의 script·CSS·이미지·font·media·fetch·frame을 허용하되 HTTP(S)·worker·object를 막는다. script 실행 여부는 client의 문서별 script gate가 결정한다. DNS prefetch·hyperlink auditing·WebRTC·media stream과 file URL의 파일/범용 접근도 끈다. `allow_remote_content=true`로 설정해도 이 모드의 문서 URI 경계와 CSP는 풀리지 않는다. 일반 WebView의 비동기 content filter 준비만으로 entry 격리를 구현하지 않는다.

완성 본문과 scan은 같은 수신 할당을 사용한다. script Allow 재로드는 그 본문을 재사용하고, 사용자·에이전트 Reload는 새 문서 세대로 다시 받는다. store 교체·close·연결 세대 교체 뒤의 응답은 이전 문서로 게시하지 않는다. disconnect는 완료 본문을 보존하되 새 Reload를 막는다. weak owner가 사라지거나 현재 문서의 URI 경계 밖이면 callback은 실패한다. 인가된 하위 파일의 미완 본문은 native 응답을 보류한다.

현재 attach entry viewer capability는 Linux·Windows·macOS GUI client가 광고한다. Windows는 전용 user-data folder·InPrivate context에서 응답 interception으로 entry와 같은 root의 하위 파일 GET에 답한다. 격리 debug 인스턴스 두 개의 실제 GUI mirror에서 entry·하위 파일 표시, 하위 문서 이동의 새 전송·scan, root 밖 참조 거절, 크기 질문 허용, script 허용과 Reload를 확인했다. macOS는 view마다 nonpersistent `WKWebsiteDataStore`를 두고 `WKURLSchemeHandler`로 entry와 같은 root의 하위 파일 GET에 답한다. headless 서버와 GUI client의 격리 debug 인스턴스에서 실제 GUI mirror의 entry·CSS·이미지 표시, root 밖 참조 거절, 하위 문서 이동의 새 전송·scan, 크기 질문 배너, script 허용과 Reload를 확인했다. 크기 질문의 허용·거절 클릭은 macOS 자동 입력을 쓰지 않아 측정하지 않았다. Reload로 새로 만든 WKWebView는 배율 2 화면의 창에서 정상 표시됐으나, 배율 1 외장 화면의 창에서는 문서 전달과 내부 snapshot이 정상인데도 창에 그려지지 않는 경우를 관측했고 원인은 확인하지 않았다. macOS handler는 main-thread task를 보유해 완료 wakeup에서 polling하고, stop에서 waiter를 제거한 뒤 후속 callback을 보내지 않는다. Windows는 apartment 안에 resource args·deferral·waiter를 보유하며 완료·실패·view close에서 응답을 결정하고 deferral을 끝낸다. 두 어댑터 모두 대기 요청을 256개로 제한하고 완료 직전 native owner를 재검사한다. main-frame 하위 문서 탐색은 cache 유무와 관계없이 새 전송·scan으로 넘긴다. 이 파일 entry 모드는 HTTP(S)를 다루지 않는다. 원격 HTTP(S)는 [별도 native backend](#원격-http-native-backend-가용성)가 맡는다.

## 원격 HTTP 직접 경로

원격 HTTP(S) 문서 view의 HTTP 요청은 모두 문서 endpoint를 거친다. proxy 설정 밖으로 나갈 수 있는 WebRTC(ICE UDP)와 DNS prefetch(client resolver 조회)는 OS별로 다음과 같이 다룬다. Linux는 HTTP view와 TLS 확인용 보조 view에 WebRTC·media stream·DNS prefetch 끄기를 명시한다. hyperlink auditing은 proxy를 거치는 요청이라 HTTP view에서는 끄지 않는다. 실측 범위는 [HTTP bridge](../../dev-guide/html-http-bridge.md#linux-native-context-component)에 있다. Windows(WebView2)와 macOS(WKWebView)는 두 기능을 끄는 설정이 없다. proxy를 지정한 두 view에서는 DNS prefetch 조회가 관측되지 않았다. WebRTC는 원격 HTTP view에만 문서 생성 시 모든 frame에서 WebRTC 생성자를 지우는 같은 스크립트(`src/host_api/webview/http_native/webrtc_mitigation.js`)를 넣어 줄인다. 측정한 경로에서만 확인한 완화이며 차단을 보장하지 않으므로 스크립트를 허용한 문서의 잔여 경로로 남는다. Windows는 등록이 거절되면 view 생성이 실패한다. 로컬·entry view는 이 스크립트를 받지 않는다. 근거와 측정 범위는 [Windows HTTP context](../../dev-guide/windows-http-context.md#direct-network-paths)와 [macOS HTTP context](../../dev-guide/macos-http-context.md#direct-network-paths)에 있다.

## macOS HTTP context

macOS 27 이상 GUI client는 원격 HTTP(S) 문서마다 비영속 `WKWebsiteDataStore`를 만들고 문서 endpoint를 HTTP CONNECT proxy로 지정한다. WebKit은 proxy를 지정해도 loopback 주소에 직접 연결하므로 원격 loopback 호스트는 `*.tasty.localhost` 별칭으로 열고, 직접 loopback 요청은 content rule로 막는다. 인증·별칭·인증서·Reload 계약과 실측은 [macOS HTTP context](../../dev-guide/macos-http-context.md)에 있다.

## 좌표 — 논리와 물리를 타입 이름에 남긴다

`WebViewBounds`(논리) 와 `PhysicalWebViewBounds`(물리)는 `to_physical` / `from_physical`
로만 오간다. 레이아웃과 플랫폼 창 API가 각자 `* scale_factor` ·
`/ scale_factor` 를 손으로 적으면 한쪽만 고쳤을 때 조용히 어긋나기 때문이다. 두 타입이
`f32` 가 아니라 `f64` 인 이유(플랫폼 API 와 `scale_factor` 가 `f64` · 소비자가 `as i32` 로
절단)는 타입 정의에 붙어 있다. 왕복은 단위 시험이 세 OS 모두에서 고정한다.

크기 변경은 container와 실제 렌더 target 양쪽에 도달해야 한다.

| 플랫폼 | 크기 전달 |
|--------|-----------|
| macOS | WKWebView setFrame |
| Windows | SetWindowPos와 controller.SetBounds |
| Linux | XMoveResizeWindow와 gtk_window.size_allocate |

Linux foreign GdkWindow 구성에서는 GTK resize·size request만으로 allocation이 갱신되지 않았다.
foreign bind와 직접 size_allocate는 함께 유지하거나 함께 제거한다.

native 창은 egui보다 위에서 마우스를 직접 받는다. Linux는 WebView를 leaf rect에 꽉 채우고, 분할선 hit 띠와 창 리사이즈 밴드가 겹치는 부분만 `set_native_holes`로 X 창의 input shape에서 뺀다. 구멍은 `state::webview_edges::{host_input_zones, webview_input_holes}`가 host 판정과 같은 정수 픽셀 구간으로 계산한다. 분할선은 `find_divider_at`의 열린 구간 `|p - d| < reach`, 창 가장자리는 `resize_direction_at`의 `x <= band`·`x >= 너비 - band`다. WebView 기준 좌표는 `set_bounds`와 같이 반올림한 X 창 사각형에서 잰다. Linux 백엔드는 이 구멍을 XFixes `SetWindowShapeRegion`으로 X 창에 물리 px 그대로 설정한다. GDK input shape는 GDK 논리 px 단위라 배율 2에서 홀수 경계를 나타내지 못한다. macOS·Windows는 이 처리가 없어 `webview_edge_inset`의 pane 외곽 여백을 쓰는 코드 경로가 남아 있고, 이 머신에서는 그 OS에서 측정하지 않았다. 같은 호출(`set_native_holes`)은 복원 안내 카드 영역(`notice_clip_holes`)도 받아 그림과 포인터에서 함께 뺀다. Linux는 그 영역을 `ShapeBounding`에서 빼고 `ShapeInput`에서는 host 입력 영역과 합쳐 뺀다. Windows·macOS는 안내 영역만 창 region·layer mask와 hit-test에서 뺀다([레이아웃 저장](../../features/layout-persistence/index.md)의 복원 안내 절).
X 창에는 물리 px를 주고, GTK resize·size_allocate에는 GDK 창 배율로 나눈 논리 px(올림)를 준다. GTK 크기 단위는 논리 px라 물리 px를 그대로 주면 GDK 배율이 2일 때 페이지 viewport가 X 창의 두 배가 된다. winit 배율과 GDK 배율은 출처가 달라(`Xft.dpi`·`WINIT_X11_SCALE_FACTOR` / `GDK_SCALE`·XSETTINGS) 서로 대신하지 않는다. 이 규칙은 `GDK_SCALE=2`에서 측정했고, XSETTINGS로 배율이 정해지는 GNOME 세션에서는 측정하지 않았다.
활성·비활성 탭 생성, 전환, 확대·축소, 분할에서 부모와 렌더 자식 크기를 비교한다.
원인을 ConfigureNotify 누락 하나로 확정하지 않는다.

<a id="스레드--셋-다-send-지만-강제의-세기가-다르다"></a>

## 스레드 제약과 추가 검사

세 타입 모두 `Rc` · raw pointer · COM 객체를 필드로 가져 **auto-trait 상 자연 `!Send`** 다.
안전한 Rust 로는 다른 스레드로 옮길 수 없다. 추가 검사는 백엔드마다 다르다.

| 백엔드 | 컴파일 시점 | 실행 시점 |
|--------|-------------|-----------|
| Linux | `!Send` | `assert_origin_thread()` 가 생성 스레드와 현재 스레드를 비교해 **release 에서도** panic 한다 |
| macOS | `!Send` + `MainThreadMarker::new()` 가 생성 자체를 main thread 로 제한 | 없음(생성이 이미 막는다) |
| Windows | `!Send` | 없음. `CoInitializeEx(COINIT_APARTMENTTHREADED)` 가 같은 스레드 가정을 세운다 |

Linux는 X11 핸들을 잘못된 스레드에서 쓰면 정의되지 않은 동작(UB)이 생길 수 있어 release에서도 생성 스레드를 검사한다.

<a id="수명--부모가-먼저-죽을-수-있다"></a>

## 수명 — 부모 창이 먼저 닫히는 경우

PlatformWebView의 Drop이 OS 자원을 정리하며 부모 winit 창이 먼저 사라진 경우도 처리한다.

- Linux: 정리 구간에 GDK error trap을 설치한다. webview.destroy → gtk_window.hide+GTK pump →
  gtk_window.close+pump → GDK 연결 sync → XDestroyWindow+XSync+pump 순서다. hide·close는 예약형이므로 pump가 실제 순서를 완성한다.
  X 창은 winit 연결로 파괴하고 GDK는 자기 연결로 요청을 낸다. X 서버는 연결 사이의 처리 순서를 보장하지 않으므로 파괴 전에 GDK 연결을 sync한다.
  trap은 push 이후의 요청만 잡는다. drop 직전 `set_visible(false)`의 hide처럼 trap 전에 낸 GDK 요청도 이 sync가 파괴보다 먼저 처리시킨다.
  남은 X 오류는 warning으로 기록한다. trap만 두고 순서를 생략하지 않는다.
- macOS: removeFromSuperview 뒤 ARC가 자원을 정리한다.
- Windows: controller.Close 뒤 DestroyWindow를 호출한다. 이미 닫힌 경우는 trace로 기록한다.

종료 회귀를 재현할 때는 실제 native webview가 생성된 것을 먼저 확인하고 탭 닫기와 창 닫기를 구별한다.
보이던 webview가 overlay 없이 곧바로 제거되는 경로(원격 attach mirror의 surface 변환처럼 구조 변경이 왕복 뒤에 도착하는 경우)도 따로 확인한다.

<a id="탐색-상태--소유는-도메인-모델이다"></a>

## 탐색 상태와 실행 소유

`PlatformWebView`와 native handle, geometry·표시 cache는 `MainView`가 소유한다.
[App의 동기화 adapter](../../../src/app/webview_sync.rs)는 원 Engine의 실행 차용과 그 View의 native handle을 함께 받아 gate 설치, host-load 표시, load·reload, 탐색 상태 반영을 실행한다. 첫 로드 전에 script gate와 sandbox 값을 설치한다. surface 실행 인스턴스가 바뀌면 기존 native binding과 navigation proof를 폐기한다.

NavState(Idle·Loading·Done·Failed)는 tasty-model의 OS 비의존 값이다.
App adapter가 native backend의 현재 상태를 읽어 EngineRuntime의 `RemoteSurface` 관측값에 반영하고, egui chrome은 `SurfaceRead::remote_webview()`의 조회 값으로 표시한다. CoreState의 확정 구조가 native handle이나 탐색 실행을 소유하지 않는다.
`nav_state`는 현재 상태를 읽고 `take_pending_navigations`는 쌓인 요청을 꺼내 비운다.
Done·Failed 전이는 마지막에 시작하거나 요청한 로드의 종료만 따른다. 앞 로드의 늦은 종료는 새 로드의 Loading을 바꾸지 않고, web process 종료는 세대와 관계없이 Failed다. OS별 세대 출처는 [HTML Viewer (`com.tasty.html`)](../../surfaces/html/index.md)에 있다(Linux는 신호 순서).

web process가 끝나면 backend는 그 사실을 따로 기록하고(`take_content_process_ended`), 동기화 adapter가 다음 동기화에서 꺼낸다. 호스트가 HTML 본문으로 실은 페이지(markdown처럼 주소가 아닌 본문을 `load_html`로 실은 경우)는 마지막으로 실은 본문 기록을 지워 같은 본문을 다시 싣는다. 엔진은 그 로드에서 web process를 새로 띄운다. 이런 페이지는 새로고침 수단이 문서 안에 있고, 플러그인이 같은 본문을 다시 보내도 adapter가 로드를 건너뛰므로 다른 복구 경로가 없다. 주소로 연 페이지와 attach mirror 문서는 사용자가 이동한 위치를 덮지 않도록 다시 싣지 않고 Failed로 둔다. Windows는 렌더러 process 종료만 기록한다. 브라우저 process가 끝난 WebView2는 같은 WebView로 다시 탐색할 수 없기 때문이다. 앱이 입력 없이 쉬고 있으면 다시 싣기는 다음 프레임에 일어난다. 탭 전환처럼 화면을 그리는 조작이 그 프레임을 만든다.

macOS에서 측정한 결과: WKWebView의 web content process를 SIGKILL로 끝내면 `webViewWebContentProcessDidTerminate:` 뒤 다음 동기화에서 같은 본문을 다시 싣고 새 web content process가 떠 로드를 마쳤다. Linux와 Windows의 같은 경로는 측정하지 않았다.

HTML 배너는 [값 snapshot과 요청](../../../src/runtime/html_script.rs)을 사용한다. 스크립트 안내의 Allow·Dismiss·Reshow와 원격 콘텐츠 안내·주소 칸 표지의 RemoteAllow·RemoteDismiss·RemoteReshow 요청은 표시 당시 `DocumentRecord`, `SurfaceBinding`, 원 `HtmlScriptState`의 weak identity에 묶이고 한 번만 소비된다. App은 현재 surface와 문서가 일치할 때만 요청을 적용한다. 프레임에서는 배너 렌더 뒤 요청을 적용하고 `finish_redraw` 뒤 같은 호출 흐름에서 native 동기화를 수행하므로, 허용 상태 갱신이 reload보다 먼저다. 문서별 허용 정책은 [HTML Viewer (`com.tasty.html`)](../../surfaces/html/index.md)을 유지한다.

사용자 navigation proof의 원본은 AppServices가 보유한 `NavigationProofs`다. App adapter는 plugin 통지 전에 원 View·surface·페이지 작성자·resource identity를 기록하고, 프레임 끝에 owner takeover와 사라진 surface를 반영한다. IPC `RequestScope`는 원 View의 weak identity와 현재 Engine 실행 문맥으로 proof를 한 번 소비한다. 닫힌 View나 교체된 resource의 기록은 새 대상에 사용할 수 없다.

PendingNavigation은 URL과 user_gesture를 포함한다. Linux는 WebKit is_user_gesture,
Windows는 IsUserInitiated를 사용하며 실패 시 warning과 false로 처리한다. 같은 문서 fragment 이동은 아래 별도 경로를 탄다.
macOS의 WKNavigationAction 공개 API에는 같은 값이 없다. `navigationType`·`buttonNumber`·`modifierFlags`는 스크립트가 만든 합성 이벤트에도 채워질 수 있어 근거로 쓰지 않는다.
대신 WebView가 OS에서 직접 받은 왼쪽 클릭을 근거로 한다(`click_gesture::ClickGesture`).
- `KeyWebView`의 `mouseDown:`이 클릭을 기록하고 `mouseUp:`이 시각을 뗀 때로 옮긴다. 이 경로는 포커스 알림(`note_focus`)과 같다. 페이지 스크립트의 `dispatchEvent`·`click()`·`location.hash` 변경은 NSResponder를 거치지 않으므로 클릭을 만들지 못한다.
- 클릭 뒤 1초 안에 탐색 결정에 들어온 main frame 탐색 하나만 `user_gesture`가 참이다. 1초는 WebKit이 사용자 제스처를 타이머로 넘겨주는 상한(`UserGestureToken::maximumIntervalForUserGestureForwarding`)과 같다. 클릭 처리기 안의 `location.hash` 변경(Markdown 폴더 버튼)이 여기에 해당한다.
- 서브프레임 탐색은 참이 되지 않고 클릭을 소비하지도 않는다. 시간이 지난 클릭은 버린다.
- macOS에서 키보드로 링크·버튼을 활성화한 탐색(Enter·Space)은 OS 클릭이 아니므로 참이 되지 않는다. Linux 엔진은 키 입력도 제스처로 보고하고, Windows의 같은 문서 이동은 아래의 키 증명을 쓴다. macOS에서 전체 키보드 접근 설정 없이 Tab이 버튼에 가는지는 측정하지 않았다.
- 사용 API의 최소 macOS(MacOSX SDK 26.2 헤더): `NSResponder` `mouseDown:`·`mouseUp:`은 가용성 표기가 없다. `WKNavigationAction`의 macOS용 `buttonNumber`·`modifierFlags`는 개별 표기 없이 클래스의 `API_AVAILABLE(macos(10.10))`을 따른다. `loadFileURL:allowingReadAccessToURL:`은 `macos(10.11)`이다. 모두 macOS 11 이전부터 있다. 실행은 macOS 27.2에서만 확인했다.
- 호스트가 시작한 `load_url`·`load_html`·`reload`·`go_back`·`go_forward`는 남은 클릭을 지운다. 에이전트·IPC 로드가 앞선 클릭을 이어받지 않게 하기 위해서다.
- 남은 한계: 클릭 뒤 1초 안에 페이지 스크립트가 시작한 다른 탐색도 참이 된다. Linux·Windows 엔진의 제스처 전달과 같은 성질이다. 호스트 로드 요청과 그 탐색 결정 사이에 사용자가 클릭하면 그 호스트 탐색이 클릭을 소비할 수 있다. 증명은 소유 plugin 작성 페이지·URL 일치·한 번 소비 규칙을 함께 통과해야 쓰인다.
- debug 로그(`WKNavigationAction input`)가 탐색마다 `navigationType`·`buttonNumber`·`modifierFlags`·main frame 여부를 남긴다. 판정에는 쓰지 않으며 실측 대조용이다.

Windows의 같은 문서 fragment 이동은 `NavigationStarting`으로 오지 않는다. Markdown 주소창·링크의 `location.hash` 신호가 여기에 해당한다(`same_document`).
- 신호는 `HistoryChanged`다. `SourceChanged`는 fragment가 없던 문서의 첫 이동에만 왔고, 이어지는 이동(`#X`→`#Y`)에는 `HistoryChanged`만 왔다(Windows 11 측정).
- 주소는 페이지에서 `ExecuteScript`로 읽는다. `load_html`(NavigateToString) 문서의 `Source`는 비어 있다. 문서 생성 스크립트가 문서의 첫 주소를 기억하고, 이동마다 `#` 앞이 같은 문서에서 바뀐 주소만 돌려준다. 그래서 새 문서의 기록 변경은 탐색으로 쌓이지 않는다. 페이지가 기억한 값을 바꿔도 돌려주는 주소는 `location` 값이다.
- `user_gesture`는 macOS와 같은 `ClickGesture` 규칙이다(1초, 한 입력에 한 탐색, 호스트 로드가 지움). 이벤트에 `IsUserInitiated`가 없어서다. host 입력 경로의 메시지 훅이 WebView로 가는 `WM_LBUTTONDOWN`·`WM_LBUTTONUP`과, 페이지로 통과시키는 Enter·Numpad Enter·Space를 기록한다. 페이지 스크립트가 만든 이벤트는 창 메시지가 아니라 증명을 만들지 못한다. host 입력 경로를 쓰지 못한 view는 입력을 볼 수 없어 거짓이다.
- 키 증명에서 빠지는 것: 수식키(Ctrl·Shift·Alt·Win)가 눌린 키, 훅이 소비한 키, IME 조합 키(`VK_PROCESSKEY`). 자동 반복은 증명에 넣는다. Chromium은 Enter 반복마다 버튼을 누르므로, 반복을 빼면 반복이 낸 거짓 탐색이 첫 누름의 기록을 지운다. Space는 뗄 때 click이 생기므로 뗌이 시각을 옮긴다. 뗌은 같은 키의 누름에만 대응하고 한 번만 옮기며, 이미 소비된 증명을 되살리지 않는다.
- 증명이 없는 Folder 버튼의 결과: host 입력 경로를 쓰지 못한 view, 수식키와 함께 누른 경우, macOS 키보드, 스크립트만으로 낸 이동에서는 `user_gesture`가 거짓이라 피커가 passive로 열린다. passive 피커는 키 입력을 받는 WebView를 숨기지 않으므로([passive 팝업](popup.md#passive-팝업)) 페이지 밑에 깔려 보이지 않는다. 그 뒤 같은 surface에서 증명이 있는 입력(마우스 Folder 등)이 오면 그 피커가 포커스를 받아 페이지 위에 보인다. 증명이 없으면 `file_picker.trigger`가 `-32000`(busy)으로 거절되고 plugin은 warn 로그만 남긴다. 터치·펜이 `WM_LBUTTON*`으로 오는지는 재지 않았다.
- 연타의 결과: 입력마다 탐색이 생기고 증명은 surface마다 마지막 하나만 남아, 첫 탐색의 `trigger`는 기록이 뒤 URL로 바뀌어 passive 피커를 연다. 마지막 탐색의 `trigger`가 증명을 소비해 그 피커에 포커스를 준다. 중간 탐색은 증명이 맞지 않아 busy다(Linux 임시 로그로 관찰). macOS는 공용 코드로 같은 판정을 타며 측정하지 않았다.
- 큐에 쌓으면 부모 창에 `RedrawWindow(RDW_INTERNALPAINT)`를 요청한다. Windows에는 WebView 폴링이 없어, 요청하지 않으면 다음 입력이나 타이머까지 큐를 꺼내지 않는다(측정 3~5초).
이 값과 webview.set_url의 페이지 작성자를 이용한 파일 열기 판정은
[파일 핸들러의 origin 규칙](../../features/file-handler/index.md#origin-소유권과-비동기-완료)을 따른다.
그 절에 기록의 한 번 소비·작성자 전이·늦게 도착한 navigation 한계를 함께 둔다.

## 키보드 — 별도 계약

macOS와 Windows의 native keycode·문자·modifier 변환은 각 플랫폼의 `keyboard` 모듈에 둔다. native callback과 기존 키 전달 계약은 같은 변환 함수를 사용한다.

native 자식 창의 키는 winit으로 자동 전달되지 않는다. 각 backend가 논리 키·PhysicalKey·modifier를
WebViewKeySink에 보내고 공통 bridge가 소비 여부를 동기 반환한다. 소비한 키는 페이지로 보내지 않으며
실제 host 액션은 다음 프레임에 큐를 비워 실행한다.

| 메서드 | 역할 |
|--------|------|
| capture_key | 키 소비 여부를 반환한다 |
| edit_action | 사용자 편집 바인딩이면 실행할 편집 명령을 반환한다 |
| note_focus | native 클릭·focus 획득을 알려 모델 focus를 맞춘다 |

backend는 Rc<dyn WebViewKeySink>만 받는다. 정책 교체와 큐 관리는 구체 bridge가 맡는다.
callback은 main thread에서 실행하므로 Rc·RefCell을 사용한다.
HostShortcutPolicy는 ShortcutSources의 host·page_reserved·plugin·address·refresh 목록을 받는다.
address(주소창 포커스 `explorer_focus_address`)는 bridge가 받은 주소 띠 surface 목록(mirror가 아닌 html)에서만 가져가고, markdown 같은 다른 WebView 페이지에는 남긴다.
설정 field·quick-switch·사용자 script·활성 plugin의 effective binding 생성은 webview_claims가 맡는다.

modifier 없는 키와 Shift 전용 키는 페이지 입력에 남긴다. 예외는 ShortcutSources의 refresh(공통 새로고침) 조합 중 F1~F24 기능키다. 문자를 만들지 않는 키라 입력·IME를 방해하지 않는다. host가 가져간 refresh 키는 모델 focus가 아니라 키를 받은 WebView surface를 대상으로 한다([공통 새로고침](../../features/keybindings/index.md#공통-새로고침--refresh)).
find·copy·cut·paste·select_all·page_undo·page_redo는 host 액션 field에서 제외하고 이 예약 조합과 동등한 plugin 조합도 제외한다.

### 편집 액션

`copy`·`cut`·`paste`·`select_all`·`page_undo`·`page_redo`의 현재 바인딩은 `ShortcutSources::edit`로 들어간다. backend는 `capture_key`보다
먼저 `edit_action`을 묻고, 편집 액션이면 키를 받은 그 WebView에 엔진 편집 명령을 callback 안에서 요청한 뒤
페이지 전달을 막는다. host 큐를 거치지 않고 그 native WebView 객체에 바로 요청하므로 나중에 다른 WebView나 닫힌
surface로 옮겨 가지 않는다. macOS는 요청이 callback 안에서 끝난다. Linux의 바인딩 키는 키를 WebKit 입력 큐에 넘기고,
web process 확장이 그 키를 입력 순서대로 받았다고 알린 뒤 실행한다(아래 순서 경로, 비동기). 순서 경로를 쓸 수 없을 때만
Copy·Cut이 아닌 명령을 callback 안에서 바로 실행한다. Windows의 DevTools 명령은 같은 WebView에 비동기로 이어진다.
모두 아래 규칙을 따른다. 대상 frame·편집 가능
여부는 엔진이 정한다: 포커스된 입력란·textarea·contenteditable·하위 frame이 대상이고, 입력란이 없으면 본문
선택·문서 전체 선택이다. 선택 없는 복사·잘라내기는 클립보드를 바꾸지 않고, 읽기 전용 입력과 본문은 잘라내기·붙여넣기로
바뀌지 않는다. 페이지 스크립트 허용과 무관하다. Windows에서는 페이지 keydown 취소가 명령을 막는다(아래).

| 플랫폼 | 실행 경로 | 엔진에 그대로 두는 키 |
|--------|-----------|------------------------|
| Linux (WebKitGTK) | `webkit_web_view_execute_editing_command`(Copy·Cut·Paste·SelectAll·Undo·Redo). Copy·Cut은 아래 선택 확인을 거친다 | Ctrl+V, Shift+Insert |
| Windows (WebView2) | DevTools `Input.dispatchKeyEvent`의 `commands`(copy·cut·paste·selectAll·undo·redo). 페이지는 키 정보가 없는 keydown을 한 번 받는다 | Ctrl+C/X/V/A, Ctrl+Insert, Shift+Insert, Shift+Delete, Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y |
| macOS (WKWebView) | NSResponder `copy:`·`cut:`·`paste:`·`selectAll:`·`undo:`·`redo:`. 뷰가 `undo:`·`redo:`에 응답하지 않으면 응답자 체인으로 보낸다 | 없음 |

WebKitGTK의 기본 Copy·Cut은 선택이 없어도 빈 내용으로 클립보드를 차지해 기존 값을 지운다. 또 이 임베딩의
WebKitGTK는 Ctrl+Z·Ctrl+Shift+Z를 스스로 실행하지 않는다. 그래서 Linux에서는 Ctrl+C·Ctrl+X·Ctrl+Insert·Shift+Delete·
Ctrl+Z 계열과 Ctrl+A도 바인딩에 있으면 Tasty가 받는다. 이 키의 keydown은 확장의 document capture 리스너에서 멈춘다.
페이지의 window capture 처리기와 document capture 처리기만 keydown을 받고, 요소·bubble 단계 처리기는 받지 않는다
(`copy`·`cut` 이벤트는 엔진 명령이 그대로 보낸다). 확장은 앞선 처리기의 `preventDefault()`를 보지 않으므로 Linux에서는
페이지가 keydown 취소로 이 명령을 막을 수 없으며, 페이지 keydown 취소가 명령을 막는 Windows와 다르다. 바인딩에서 지운
키는 엔진 기본 동작으로 돌아간다.

Linux의 편집 바인딩은 web process 확장을 거쳐 입력 순서대로 실행하고, Copy·Cut은 그 시점에 범위 선택이 있을 때만
실행한다. `EditorState`는 편집 불가 본문에서 선택이 없어도 복사 가능으로 답하고 최대 수백 ms 늦으므로 쓰지 않는다.
WebKitGTK의 기본 Copy는 선택이 없어도 빈 내용으로 클립보드를 차지한다(측정).

- 확장: `src/host_api/webview/linux/selection_extension.c`. build.rs가 `webkit2gtk-web-extension-4.1`로 빌드하고 본체가
  바이트로 내장한다. 실행 시 `<Tasty 홈>/webkit-extensions/<내용 해시>/`에 모듈 하나만 꺼낸다. 디렉터리는 이 사용자 소유
  0700이어야 하고 다른 항목이 있으면 쓰지 않는다(WebKit은 그 디렉터리의 모든 `.so`를 불러온다). 기본 WebContext는
  첫 WebView 전에, entry·HTTP(S) WebContext는 만든 직후에 지정한다. 별도 설치·패키징 단계는 없다. WebKit sandbox
  설정은 바꾸지 않는다.
- 순서: 키 입력과 포인터 입력은 WebKit UI process의 큐에서 한 번에 하나씩 web process로 가지만, UI가 보내는 메시지와
  편집 명령은 큐를 앞지른다. 그래서 키 경로는 이렇게 한다.
  - UI는 바인딩 키를 받으면 무장 메시지(nonce, WebKitGTK keyCode, Ctrl·Shift·Alt·Meta 비트)를 보내고 키를 그대로
    WebKit 큐에 넘긴다. 무장이 키보다 먼저 가도 무장만 하므로 순서와 무관하다. 무장한 조합은 확장이 소유하며, 그 조합이
    바인딩이 아닌 키로 다시 눌리면 UI가 소유를 풀고 그 키는 엔진 기본 동작으로 간다.
  - WebKitGTK는 GDK META_MASK만 DOM `metaKey`로 옮긴다. 그래서 UI는 WebKit 기본 처리 전에 Super·Hyper·MOD4가 든 키
    이벤트(누름·뗌)에 META_MASK를 더한다. 원래 비트는 지우지 않는다. 지우면 엔진의 Ctrl+C 바인딩이 Ctrl+Super+C에도
    맞는다(측정). 더하면 엔진 동작은 바뀌지 않았고 DOM에서 C·Super+C·Ctrl+C·Ctrl+Super+C가 구분됐다(측정, X11). UI
    분류는 META_MASK를 쓰지 않으므로 WebKit이 다시 보낸 키도 같은 키로 분류된다. Wayland와 별도 Meta 키가 있는
    keymap은 측정하지 않았다.
  - 확장은 문서마다 capture 단계 native `keydown` 리스너를 붙인다(WebKitDOM window 리스너는 keydown을 받지 못했다, 측정).
    주 문서는 `document-loaded`에서, iframe·frame 문서는 그 frame의 `load`(capture)에서 붙인다. 무장 메시지를 받을 때마다
    지금 있는 모든 frame 문서를 다시 순회해 빠진 문서에 붙인다. 무장이 키보다 먼저 도착하므로, load가 아직 끝나지 않은
    frame 문서도 키가 오기 전에 리스너를 가진다. 깊이 상한은 없다. 주 문서의 준비 알림은 주 문서만의 준비를 뜻한다. 리스너는 소유한 조합이면 기본 동작과 이후 전파를 막는다. 그래서 그 조합은 엔진의 빈 복사로
    가지 않고, 뒤의 페이지 처리기가 명령 전에 선택을 바꾸지 못한다. 그 뒤 무장 순서대로 키를 맞춰 그 문서의 범위 선택
    여부를 WebView로 보낸다. 무장은 입력 순서대로 답한다. UI가 무효로 만든 무장과 앞 문서에서 만든 무장도 남아서
    자기 키를 소비하고 UI가 그 답을 버리므로, 큐에 남은 옛 키가 더 새 무장을 가져가지 않는다. 맞은 무장보다 앞에
    있던 무장(페이지가 멈춘 키)은 버린다. 기다리는 무장은 64개까지다. 하나가 더 오면(웹 프로세스가 멈췄거나 페이지 처리기가 키를 계속 멈춰 답이 오지 않는 경우) 확장은
    기다리던 무장을 모두 버리고 그 page가 사는 동안 무장을 더 받지 않는다. 문서를 다시 읽어도 풀리지 않는다. 조합의
    소유는 남으므로 큐에 있던 키를 포함해 소유한 키는 모두 명령 없이 막히고, 어떤 키도 나중 명령을 가져가지 않는다.
    UI는 이 overflow 응답을 받으면 기다리던 무장을 모두 버리고 경고를 남기며, 그 page의 바인딩 편집 키를 명령 없이
    소비한다. 바인딩이 아닌 입력과 키를 거치지 않는 host 명령(팔레트·IPC)은 그대로 쓸 수 있고, 페이지를 자동으로 닫거나
    다시 읽지 않는다. 그 view를 닫고 다시 열면(새 page) 순서 경로가 돌아온다. 맞는 무장이 없으면 명령 없이 막기만 한다.
    UI는 실패한 무장 하나만 지우고, 키 답을 받았을 때만 그보다 앞선 무장을 버린다. UI 쪽 기록도 64개까지이며 넘으면
    가장 오래된 기록을 버려 그 명령은 실행되지 않는다.
  - UI는 그 메시지를 받으면 명령을 실행한다. web process는 keydown을 처리하는 동안(이벤트 ack 전) 메시지를 보내므로
    편집 명령은 다음 키보다 먼저 간다. 그래서 앞서 입력한 Shift 선택 키·Ctrl+A 뒤의 Copy와 Copy 뒤의
    Ctrl+V·사용자 Paste·Undo 바인딩이 키 입력 순서대로 실행되며, UI에서 키를 보류하지 않는다.
  - 이 순서는 키 사이의 순서다. WebKit의 키 큐와 포인터 큐는 따로라서 드래그와 키 사이의 순서는 보장하지 않는다.
    드래그를 끝낸 뒤 누른 Copy는 그 선택을 복사한다(측정). 드래그와 바인딩 Copy가 한 묶음으로 바로 이어 들어오면
    (측정: xdotool 한 번의 호출로 드래그 직후 Ctrl+C) 키가 드래그의 선택보다 먼저 처리되는 경우가 있고, 그때는 선택이
    없으므로 실행하지 않고 클립보드를 그대로 둔다. 같은 회차에 통과한 경우도 있어 시점에 따라 다르다. 같은 묶음에서
    바인딩하지 않은 엔진 Ctrl+C도 선택을 보지 못하고 빈 내용을 복사했으므로(측정) 엔진의 경계와 같다. 이 경계를 위한
    별도 동작은 두지 않는다.
- 판정: 활성 요소가 iframe·frame이면 포커스가 있는 문서까지 내려가고, 그 문서의 native DOM selection 종류가 `Range`일 때만
  선택이 있다. 입력란(email·number처럼 `selectionStart`가 없는 종류 포함)·읽기 전용 입력·textarea·이미지만 고른 범위도
  같다. 다른 출처 frame도 읽는다. 페이지 스크립트를 실행하지 않으므로 스크립트 허용 여부와 결과가 같다. WebKitDOM은
  2.40부터 deprecated지만, 스크립트가 꺼지면 `window-object-cleared`가 오지 않아 script world로 frame을 얻을 수 없다(측정).
- 메시지는 nonce·page id·keyCode·수정자 비트·선택 여부만 담는다. 선택 내용은 보내거나 기록하지 않고 클립보드를 읽지 않는다.
- 실패 시 닫힘: 새 문서가 commit되면 그 문서의 리스너 준비 알림이 올 때까지, 또는 키를 keyCode로 옮길 수 없으면 키 경로의
  Copy·Cut은 실행하지 않고 키를 소비한다(엔진의 빈 복사를 막는다). keyCode는 WebKitGTK가 keydown에 넣는 Windows 가상
  키 코드이며, UI는 바인딩으로 받을 수 있는 keyval을 WebKitGTK 2.50 `windowsKeyCodeForGdkKeyCode` 표와 같은 값으로 옮긴다.
  대상은 글자·숫자, Shift를 누른 숫자 기호(`!`→1 등)와 US 배치 기호 쌍(`;:`·`=+`·`,<`·`-_`·`.>`·`/?`·`` `~ ``·`[{`·`\|`·
  `]}`·`'"`), Tab·Enter·Escape·Space·Backspace·Insert·Delete·Home·End·PageUp·PageDown·방향키(keypad 이동 keyval 포함),
  keypad 숫자·연산자, F1~F24다. WebKit 표에 없는 키(F25 이상, keypad Space·`=` 등)는 keyCode가 0이므로 옮기지 않는다.
  WebKitGTK가 keyCode를 0으로 주는 키(키릴 문자 등 측정함)는 keyIdentifier `U+XXXX`가 실제 keyval을 대문자로 바꾼 Unicode와 같으므로, VK가 없고
  ASCII가 아닌 Unicode가 있으면 그 code point로 맞춘다. 바인딩 인식은 그대로 물리 US 위치로 한다. Unicode가 없는 키(dead
  key 등)는 확장이 DOM에서 같은 키로 알아볼 수 없으므로 순서 경로에 넣지 않는다. IME 조합·AltGr 단계·Wayland는 측정하지
  않았다. 다른 명령은 바로 실행한다. 확장이 없으면 준비
  알림이 오지 않으므로 Copy·Cut은 동작하지 않는다. 본문을 그대로 복사하는 대체 경로는 없다.
- 무효화(`EditGuard`): 응답 page가 다르거나, 무장 뒤 그 WebView에 클릭·포커스 이탈·숨김·호스트의 로드·재로드·뒤로·앞으로·
  문서 로드 시작·commit이 있었거나 WebView를 버렸으면 실행하지 않는다. 뒤의 키는 무효로 하지 않는다(큐가 정렬한다). 응답
  시점에 같은 키로 `blocks_edit_key`를 다시 물어 host 모달·overlay가 열렸으면 실행하지 않는다. 소유한 키는 확장이 기본
  동작을 막아 처리된 키가 되므로 WebKit이 UI로 다시 보내지 않는다. 같은 X 시각에 연달아 누른 키도 각각 무장한다.
- host가 키 없이 요청한 Copy·Cut(팔레트·IPC)은 포커스 frame의 선택을 직접 묻는다. nonce·page id가 맞고, 그 사이 새 키나
  위 무효화가 없을 때만 실행한다. 확장이 없거나(`Unavailable`) 1초 안에 답하지 않으면 실행하지 않고 warn 로그를 남긴다.
- 페이지 처리기(스크립트 허용 시): window capture 처리기와 Tasty보다 먼저 등록된 document capture 처리기는 Tasty보다
  먼저 keydown을 받는다. Tasty보다 나중에 등록된 document capture 처리기도 같은 단계에서 받는다(WebKitDOM에는
  `stopImmediatePropagation`이 없다). 요소·bubble 단계 처리기는 받지 않는다. 먼저 받은 처리기가 바꾼 선택은 판정에
  반영되고, 그 처리기의 `preventDefault()`는 명령을 막지 않는다. 바인딩 조합의 수정자 키(Ctrl 등) 자체의 keydown과
  바인딩이 아닌 키는 모든 페이지 처리기가 그대로 받으므로, 그 keydown에서 페이지가 지운 선택도 판정에 반영된다.
- 남은 범위: window capture 처리기가 `stopPropagation`이나 `stopImmediatePropagation`을 부르거나, 같은 document에서
  먼저 받은 처리기가 `stopImmediatePropagation`을 부르면 키가 Tasty 리스너에 닿지 않아 Tasty는 실행하지 않는다. 그
  처리기가 기본 동작도 막지 않았으면 다른 브라우저처럼 엔진 기본 동작이 실행된다. 무장과 그 키 사이에 main document나
  frame이 새 문서를 commit하면 그 문서에 리스너가 붙기 전이라 엔진 기본 Copy 키(Ctrl+C·Ctrl+Insert)를 엔진이 처리한다.

Windows의 DevTools 명령은 같은 WebView에 비동기로 전달되며, `controller.Close()` 뒤에는 호출이 실패한다.

엔진에 그대로 두는 키는 바인딩에 있어도 Tasty가 명령을 실행하지 않는다. 페이지 keydown과 취소 규칙이 엔진 기본
처리와 같게 유지되고 같은 입력이 두 번 실행되지 않는다. 바인딩에서 지워도 엔진 기본 편집 키는 계속 동작한다.
Tasty 바인딩 해제는 Tasty가 더한 키를 없앨 뿐이다. 그 밖의 바인딩(예: `alt+c`, `ctrl+shift+c`)은 페이지 keydown
대신 편집 명령으로 실행된다. 페이지 텍스트의 실행 취소·다시 실행은 `page_undo`·`page_redo`이며 Explorer 파일 이력의
`explorer_undo`·`explorer_redo`와 별개다. 두 쌍은 같은 조합을 써도 충돌로 보지 않는다(`SHARED_COMBO_PAIRS`).
macOS에는 엔진 기본 키가 없으므로 `page_undo`·`page_redo` 바인딩을 지우면 페이지 실행 취소 키도 없다.
문자·단어 이동과 확장 선택·삭제, Tab 이동은 엔진 기본 키를 쓰며 Tasty 액션을 두지 않는다.

host 창이 키를 받았거나(native 자식이 포커스를 갖지 않은 상태) 명령 팔레트에서 편집 액션을 실행하면 포커스
surface의 WebView에 같은 명령을 실행한다(`run_webview_edit`). 터미널 붙여넣기나 다른 surface의 남은 선택으로 넘기지 않는다.
포커스 surface에 WebView가 없으면 `copy`·`cut`·`paste`·`select_all`은 host 동작을 실행하고, `page_undo`·`page_redo`는 대상이 없어
아무것도 실행하지 않는다.

host 모달이 열려 있거나 키보드를 가져가는 host overlay(popup·dialog)나 전체화면 무대가 열린 동안에는 native 자식이 OS 포커스를
가졌어도 편집 바인딩 키와, 바인딩에 없더라도 엔진이 스스로 편집으로 처리하는 키(Windows·Linux의 Ctrl+C/X/V/A,
Ctrl/Shift+Insert, Shift+Delete, Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y와 엔진 키 표의 Ctrl+Shift+V 서식 없는 붙여넣기,
Windows의 Alt+Backspace·Alt+Shift+Backspace 실행 취소·다시 실행, Linux의 Ctrl+/ 전체 선택)를 실행하지도 페이지에
보내지도 않는다(`WebViewKeySink::blocks_edit_key`). 이 추가 키는 막는 동안에만 쓰며 평소에는 엔진 처리에 그대로 둔다. macOS WKWebView는 Edit 메뉴 없이 Cmd 편집 키를 실행하지 않으므로 바인딩만 막는다.
모달 상태는 모달을 열고 닫을 때(`set_active_modal`·`take_active_modal`), overlay 상태는 프레임 동기화에서
bridge에 알린다. 편집이 아닌 키는 이 판정과 무관하다.

같은 조합이 여러 곳에 묶였을 때의 순서는 두 경로가 같다: 포커스 WebView의 편집 바인딩 > plugin 명령 > 그 밖의
host 액션. native 경로는 `edit_action`을 `capture_key`보다 먼저 묻고 페이지 예약 조합과 같은 plugin 조합을 정책에서
뺀다. host 창 경로는 `dispatch_plugin_shortcut_key`가 `webview_edit_for_key`에 걸리는 키를 plugin에 넘기지 않는다.

Windows의 DevTools 명령은 페이지에 키 정보가 없는 keydown을 한 번 보낸다. 그 keydown에서 페이지가
`preventDefault()`를 부르면 엔진이 명령을 실행하지 않는다. 이는 실제 키를 눌렀을 때 Chromium의 취소 규칙과 같으며,
Tasty는 이 취소를 덮어쓰지 않는다. 취소는 keydown을 받은 문서 기준이라 상위 문서의 처리기는 포커스된 하위 frame의
명령을 막지 않는다. WebView2 runtime 156에서 이 명령 경로로 확인한 범위: 포커스된 입력·textarea·contenteditable·
하위 frame 대상, 읽기 전용 유지, 선택 없는 복사의 클립보드 유지, 입력란 범위의 전체 선택, 실행 취소·다시 실행,
페이지 스크립트 차단과 무관함. Tasty 바인딩 키가 accelerator 경로로 이 명령에 이르는 실제 키보드 입력과 macOS는
실제 실행으로 확인하지 않았다.
예약 조합은 사용자의 현재 바인딩이다. markdown 페이지는 `find` 바인딩을 `settings.get_keybinding`으로 읽어 문서에 넣고
`command.host_shortcut_changed`를 받으면 문서를 다시 만든다. html 표면의 사용자 페이지는 이 정보를 받지 않는다.
host가 페이지에 남긴 modifier 없는 키도 `find` 바인딩이면 markdown 페이지가 찾기로 처리한다(`F3` 등). 글자 입력과 겹치는 조합은 입력칸·IME 조합 중에는 처리하지 않는다.
대소문자와 modifier 순서는 실제 binding parser로 비교한다.
다른 host 액션에 사용자가 같은 조합을 준 경우까지 추가 제외하지는 않는다.
plugin scope는 bridge에서 미리 거르지 않으므로 소비된 뒤 현재 host focus 규칙에 따라 실행되지 않는 키가 있을 수 있다.
비활성 plugin은 제외한다.

설정과 PluginCommandRegistry·PluginsConfig의 전역 revision이 바뀔 때만 정책 스냅숏을 다시 만든다.
modifier가 있으면 winit과 같은 US physical-key 변환을 쓰고 표 밖 키는 논리 키로 돌아간다.
key-up은 전달하지 않고 host의 modifier 저장값도 바꾸지 않는다.
macOS·Windows의 자동 반복은 다시 실행하거나 큐에 넣지 않는다. 첫 누름을 입력 차단으로 막았거나, 편집 명령으로
실행했거나, host가 가져갔을 조합이면 반복도 페이지·엔진에 보내지 않는다(`WebViewKeySink::consumes_repeat`).
엔진에 그대로 두는 키와 가져가지 않는 키의 반복은 엔진·페이지로 간다. Linux/GDK의 key press에는 repeat 표시가 없어
정상 press 오판을 피하려고 반복도 일반 press로 처리한다.
Windows WebView2는 `AcceleratorKeyPressed`를 Ctrl이나 Alt를 누르고 있거나 글자로 바뀌지 않는 키에서만 올린다
([ICoreWebView2Controller](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller)).
그래서 Windows는 controller를 `AllowHostInputProcessing`
([ICoreWebView2ControllerOptions4](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controlleroptions4))
을 켜서 만든다. 이 옵션의 키보드 포커스 창은 host GUI 스레드에 있으므로 페이지로 갈 key-down이 winit 메시지 루프를 먼저 지난다.
winit 메시지 훅(`webview/windows/host_input.rs`)이 Translate·Dispatch 전에 그 메시지의 창이 어느 WebView 자식 창 아래인지 찾고,
그 WebView의 `WebViewKeySink`로 위와 같은 순서(반복 → 입력 차단 → 편집 명령 → host 가져가기)를 판정한다.
소비한 key-down은 보내지 않으므로 WM_CHAR도 생기지 않는다. key-up, IME가 처리하는 키(`VK_PROCESSKEY`), 변환표 밖 키,
WebView 밖으로 가는 메시지는 판정하지 않고 그대로 보낸다. 그래서 Ctrl·Alt 없이 누르는 글자·숫자·기호 키(Shift 조합 포함)에 준
편집 바인딩도 웹 화면에서 받는다. host 액션은 modifier가 있는 조합만 가져가는 정책이 다른 OS와 같다.

이 view에서도 `AcceleratorKeyPressed`는 같은 판정(`keys::route_key_down`)을 한다. 훅이 소비한 key-down은 Dispatch되지 않으므로
WebView2가 그 키로 이 이벤트를 올리지 않는다. 훅이 통과시킨 키는 이 이벤트에서 한 번 더 판정하지만, 판정은 소비하지 않을 때 상태를
바꾸지 않아 같은 결과(통과)가 된다. 그래서 바인딩이 두 번 실행되지 않는다. 훅을 지나지 않은 키는 이 이벤트가 판정한다.
controller 생성을 기다리는 `wait_with_pump` 같은 중첩 메시지 루프, 창 이동·크기 조절 modal loop, `DoDragDrop`,
다른 스레드가 `SendMessage`로 보낸 키가 여기에 해당한다. 이 경우 Ctrl·Alt 조합과 글자가 아닌 키는 기존처럼 받지만,
이 이벤트가 오지 않는 Ctrl·Alt 없는 글자 키 바인딩은 그동안 페이지에 입력된다.

훅 경로에서 Ctrl·Alt·Super 없이 누른 글자 키는 다른 OS와 같이 현재 배열의 논리 문자로 비교한다(`windows/keyboard.rs`의
`layout_character_key`). GUI 스레드의 입력 언어(`GetKeyboardLayout(0)`)와 키 상태(Shift·CapsLock)로 `ToUnicodeEx`를
부르며, 0x4 플래그로 dead key 상태를 바꾸지 않아 뒤따르는 메시지의 조합이 그대로 남는다. 이 플래그는 Windows 10 1607부터
지원되므로 그보다 오래된 Windows에서는 dead key 조합이 깨질 수 있다. dead key는 어떤 바인딩과도 맞지 않는 `Key::Dead`가
되어 페이지로 간다. 배열이 문자를 만들지 않거나 제어 문자를 만들면 VK의 US 배열 문자(`vk_to_winit_key`)를 쓴다.
Ctrl·Alt·Super 조합은 계약대로 물리 키의 US 배열 문자로 비교한다(`tasty_key_match::shortcut_lookup_key`). AltGr는
`GetKeyState`가 Ctrl+Alt로 읽으므로 이 규칙을 따른다. `AcceleratorKeyPressed` 경로는 Ctrl·Alt 조합과 글자가 아닌 키만 받으므로
VK 변환값을 그대로 쓴다.

옵션 인터페이스(`ICoreWebView2Environment10`, `ICoreWebView2ControllerOptions4`)가 없는 옛 runtime이거나 이 옵션으로
만든 controller 생성이 실패하면 경고를 남기고 옵션 없이 다시 만든다. 그 view는 `AcceleratorKeyPressed`로 판정하므로
Ctrl·Alt 없는 글자 키 바인딩은 페이지에 입력된다. 사용자 세션의 runtime 156에서는 옵션을 켠 생성이 성공했고,
session 0에서 실행한 하네스에서는 같은 생성이 `REGDB_E_CLASSNOTREG`로 실패했다. 이 실패 경로의 Tasty 실제 실행은 측정하지 않았다.
폴백은 그 view가 닫힐 때까지 유지되며 warn 로그 외의 표시는 없다.

키 도착만으로 모델 focus를 바꾸지 않는다. X11에서는 포인터가 자식 위에 있다는 이유로 키가 올 수 있다.
Linux만 보이는 webview와 활성 창이 있을 때 16ms GTK poll을 사용하고 숨김·비활성 때 취소한다.
macOS의 performKeyEquivalent도 실제 first responder가 자기 뷰 안에 있을 때만 가로챈다.

## 포커스 — 회수는 **조건부**다

host가 webview를 가릴 때 host 창이 활성이고 실제 focus가 해당 자식 안에 있을 때만
부모 winit 창으로 회수한다. 가리는 경로는 두 가지다.

- overlay를 열 때: 모든 webview를 대상으로 개폐마다 한 번 회수한다.
- 탭·workspace 전환으로 surface가 활성 탭에서 빠질 때: 그 surface만 전이마다 한 번 회수한다.
  숨기기 전에 회수한다. 숨긴 뒤에는 backend가 focus가 이미 자식 밖에 있다고 판정할 수 있다.
  창이 비활성이면 창이 활성이 될 때까지 미루고, 그 사이 다시 활성 탭에 들어오거나 닫힌 surface는 뺀다.

overlay 판정(`has_egui_overlay_open`)은 [passive 팝업](popup.md#passive-팝업)을 세지 않는다.
passive 팝업은 토스트 카드와 같이 겹치는 webview만 숨긴다.

탐색이 끝나지 않아 활성 탭의 webview를 숨기는 경우, 토스트 카드나 passive 팝업과 겹쳐 그 webview만 숨기는 경우([토스트](toast.md#native-webview-위의-토스트), 키를 받는 webview는 숨기지 않는다), surface를 닫는 경우는 회수하지 않는다.
다시 보이거나 overlay가 닫혀도 native 자식 focus를 자동 복원하지 않는다.

| 플랫폼 | 자식 focus 확인 | 회수 대상 |
|--------|----------------|-----------|
| Linux | XGetInputFocus에서 부모 체인을 따라 자기 X 창인지 확인. None·PointerRoot 제외 | 부모 X 창 |
| macOS | firstResponder가 자기 뷰 또는 하위 뷰인지 확인 | contentView(winit 뷰), nil 사용 금지 |
| Windows | GetFocus가 자기 HWND 또는 IsChild인지 확인 | 부모 HWND |

창 활성 판정은 `host_window_has_os_focus`가 맡는다. base.focused가 참이거나, 어느 webview의
holds_keyboard_focus가 참이면 창이 활성이다. base.focused는 winit의 Focused 이벤트를 따르는데,
Linux/X11에서는 자식 창이 focus를 가져가면 winit이 부모 창에 Focused(false)를 보낸다(실측).
Windows의 winit도 WM_KILLFOCUS를 Focused(false)로 바꾼다(winit 소스 확인).
그래서 base.focused만으로는 회수가 필요한 바로 그 상태에서 회수가 막힌다.

holds_keyboard_focus는 위 표의 자식 focus 확인을 다시 쓰며 다른 앱이 OS focus를 가진 동안에는 거짓이어야 한다.

| 플랫폼 | 다른 앱이 focus를 가질 때 거짓인 이유 |
|--------|--------------------------------------|
| Linux | X focus는 서버 전역이다 |
| Windows | GetFocus는 호출 스레드의 큐가 비활성이면 널을 돌려준다 |
| macOS | first responder는 비활성 창에도 남으므로 key window 여부를 함께 확인한다 |

Linux의 GTK 펌프 폴링(`webview_poll_needed`)도 같은 판정을 쓴다. base.focused만 보면 자식이 X focus를 쥔 동안
GDK가 받은 클릭과 키가 다음 루프 깨움까지 처리되지 않는다(아래 "Linux 클릭 반영 지연").

OS 조회는 base.focused가 거짓이고 회수할 대상이 있을 때만 한다. 탭 전환 경로는 회수할 surface가 없는 프레임에 조회하지 않는다.
창 활성 판정 없이 backend의 회수 조건(자식 focus 확인)만 쓰면 macOS에서 IPC로 overlay를 열거나 탭을 바꿀 때 비활성 창의 first responder가 바뀐다.
컴파일만으로 이 동작을 확인할 수 없으므로 활성·비활성 창에서 각각 재현한다.
Linux/X11(Xvfb)에서는 X focus를 자식 창에 직접 넣은 상태와 다른 X 클라이언트가 focus를 가진 상태를 확인했다.
실제 클릭은 X focus를 부모 창에 남겨 앞의 상태를 만들지 않았다. Windows·macOS는 실행 환경에서 확인하지 않았다.

## egui 오버레이와 native WebView

### 모드와 현재 상태

대상 팝업이 보이는 동안 native WebView를 다루는 방식은 세 모드 중 하나다. 모드는 사용자 설정이 아니라 환경이 정한다.

| 모드 | 조건 | 보이는 것 |
|---|---|---|
| 투명 | backend가 투명 합성을 보고함 | 페이지 위에 팝업 셸·scrim·그림자·둥근 모서리가 지금 디자인 그대로 합성된다 |
| 불투명 | backend가 팝업 영역을 불투명하게 대신 보일 수단만 보고함 | 팝업 상자와 드롭다운 상자의 사각형 안만 호스트 그림이다. 그 밖의 페이지는 바뀌지 않는다 |
| 숨김 | backend가 지원하지 않음, 적용 실패, 전체화면 무대·튜토리얼 | 모든 native WebView를 숨기고 그 자리에 html surface의 boundary 타일이 보인다([html surface](../../surfaces/html/index.md)) |

공통 판정·영역 계산·표현·GPU 분리·창 단위 오버레이 host 연결은 구현돼 있다. OS별 현재 상태는 다음과 같다.

| OS | 보고하는 모드 | 수단 |
|---|---|---|
| Linux(X11) | XFixes 2.0과 SHAPE 확장이 있으면 투명을 보고하고, 창 단위 host 상한으로 낮춘다. 합성 관리자가 있으면 투명, 없거나 창 관리자가 오버레이 창 위치를 바꾸면 불투명이다. XFixes·SHAPE가 없으면 숨김 | 투명은 창 단위 host(주 창의 transient인 ARGB 관리 창)가 페이지 위에 그린다(아래 "Linux 투명 구현"). 불투명은 WebView X 창 shape에 구멍을 내 주 표면을 보인다(아래 "Linux 불투명 구현") |
| macOS | 불투명 | WKWebView layer mask와 입력 제외(아래 "macOS 측정 사실과 범위") |
| Windows | 불투명 | HWND region과 입력 차단(아래 "Windows 불투명 수단") |

macOS·Windows는 투명 모드를 보고하지 않고 주 표면의 불투명 경로를 쓴다. 창 단위 host는 Linux와 macOS에 있으며, macOS는 debug의 강제 투명 모드에서만 쓴다. Windows host는 `Unsupported`다. 각 OS의 측정 범위와 제약은 아래에 적는다.

숨김이 필요한 이유는 z-order다. WebView는 OS 자식 창(Linux X 창, Windows child HWND, macOS subview)이라 wgpu 주 표면보다 항상 위에 있고, egui는 주 표면 하나에만 그린다. 숨기지 않으려면 WebView에서 팝업 영역을 잘라 내거나(clip) 팝업을 별도 OS 층에 다시 그려야 한다.

복원 안내 카드에는 잘라 내는 경로가 이미 있다. `set_native_holes`가 받은 영역을 WebView의 그림과 포인터에서 함께 뺀다. Linux는 XFixes `ShapeBounding`·`ShapeInput`, Windows는 child HWND 창 region, macOS는 `KeyWebView` layer mask와 `hitTest:` 제외다. Linux 불투명 모드는 같은 X 창 shape에, Windows 불투명 모드는 같은 child HWND 창 region에, macOS 불투명 모드는 같은 layer mask에 팝업 상자를 합쳐 뺀다(아래 플랫폼별 구현). Linux와 Windows는 격리 debug 인스턴스에서 안내 clip 단독의 동작을 확인했다. macOS는 같은 mask 계산을 입력 없는 소유 probe로 확인했으며, Tasty 안내 clip 단독의 실제 GUI 동작은 측정하지 않았다.

### 모드 결정

`has_egui_overlay_open`의 의미와 소비자는 바꾸지 않는다. 그 값이 참이면 WebView 동기화(`settle_overlay`)가 별도로 모드를 정한다.

- 활성 WebView마다 `overlay_support()`를 묻고 가장 낮은 값을 쓴다. 하나라도 지원하지 않으면 숨김이다.
- 전체화면 무대나 튜토리얼이 보이면 숨김이다. 창 전체를 덮는 층이라 페이지를 보일 이유가 없다.
- 숨김이 아니면 활성 WebView마다 `apply_overlay`를 부른다. 하나라도 실패하면 그 프레임부터 숨김이고 `tracing::warn!`을 남긴다. 실패는 대상 팝업이 모두 닫힐 때까지 유지한다. 그동안 다시 시도하지 않고 경고도 한 번만 남긴다. 오버레이 창의 surface를 잃고 다시 설정해도 얻지 못한 경우도 같은 실패로 다룬다.
- 숨김으로 돌아가거나 팝업이 닫히면 적용했던 WebView에 `clear_overlay`를 부른다. 한 프레임에서 앞의 WebView 적용이 성공하고 뒤의 WebView가 실패하면, 첫 적용이어도 그 프레임에 모든 WebView에 `clear_overlay`를 불러 앞의 입력 차단·그림 영역을 되돌린다.
- 고른 모드의 그림 위치가 오버레이 창이면 창 단위 host가 낼 수 있는 상한(아래 "창 단위 오버레이 host")보다 높지 않게 한 단계씩 낮춘다(투명 → 불투명 → 숨김). 낮출 때마다 그 모드의 그림 위치를 다시 묻고, 주 표면이면 host와 관계없이 그 모드를 쓴다. host를 아직 만들지 않았으면 상한을 모르는 것이라 낮추지 않는다. 만들지 못했거나 이 OS가 만들지 않으면 상한은 숨김이다.
- 매 프레임 다시 정하므로 환경이 바뀌면(예: Linux 컴포지팅 매니저 종료) backend가 낮춘 보고를 다음 프레임부터 따른다.
- 표시 판정은 `reveal_wanted(mode, overlay_open, active, under_toast)`다. 숨김이 아니면 대상 팝업이 떠 있어도 활성 WebView를 보인다. 비활성 탭과 토스트·passive 팝업 규칙은 모드와 관계없이 그대로다.
- 모드와 함께 그 모드의 그림 위치(`OverlayPaintSource`)를 정한다. 활성 WebView가 모두 `overlay_paint_source(mode)`로 주 표면을 보고하면 주 표면이고, 하나라도 오버레이 창이면 오버레이 창이다. 주 표면은 backend가 페이지의 대상 영역에 구멍을 내 주 pass가 그린 팝업을 보이는 방식이다. 오버레이 창은 주 pass 뒤 별도 창에 그리는 방식이다. 같은 모드도 backend에 따라 위치가 다를 수 있다.
- 대상 팝업이 떠 있고 숨김이 아니면, 같은 프레임에 같은 모드·그림 위치·페이지 사각형으로 만든 그림이 지금과 같은 크기의 주 표면 프레임과 함께 present된 뒤에만 페이지를 드러낸다(`overlay_reveal_ready`). 주 표면 위치는 주 pass present가, 오버레이 창 위치는 오버레이 창 present가 그 증거다. job을 만든 뒤 표면 크기가 바뀐 present나 다른 그림 위치의 기록은 쓰지 않는다. 그림은 직전 동기화가 정한 모드·그림 위치·페이지(`OverlayFrame::planned_pages`)로 만들기 때문에, 팝업이 처음 뜬 프레임이나 페이지 사각형이 바뀐 프레임에는 그림이 없다. 그 프레임에는 페이지를 숨긴 채 다음 프레임에 넘기고 다시 그리기를 요청한다. 그래서 팝업 그림보다 페이지가 먼저 보이는 프레임은 없고, 대신 한 프레임 동안 숨김 모습이 보인다. 그림 위치가 오버레이 창인데 창이 없으면 그림이 올라가지 않으므로 팝업이 떠 있는 동안 페이지는 숨김 그대로다.

### 대상 층

대상 종류는 `TargetKind` 하나에 정의한다. 대상은 egui 프레임이 그리면서 `OverlayFrame::targets`에 기록한다. 종류와 기록 위치의 일치는 `source_guards::webview_overlay_targets`가 검사한다.

| 종류 | 기록 위치 | 상자 |
|---|---|---|
| `HostPopup` | `PopupManager::draw` | 보이는 non-passive host 팝업. 텍스트 입력 다이얼로그(이름 바꾸기)도 host 팝업이다 |
| `PluginPopup` | `draw_plugin_popups` | plugin 팝업 셸. 호스트가 직전 프레임 영역으로 입력을 판정하므로 같은 레이어의 직전·현재 영역을 합친다 |
| `Dropdown` | 프레임 끝 `webview_overlay::end_frame` | 열린 egui 팝업 레이어(`popup_below_widget`·`ComboBox` 등)다. 팝오버 래퍼(`with_popover_frame`)가 연 드롭다운은 부모 레이어와 가장자리를 함께 기록한다 |

| 층 | 모드 적용 |
|---|---|
| 위 세 종류와 plugin mesh 팝업 콘텐츠 | 적용 |
| 전체화면 무대, 튜토리얼 | 적용하지 않고 숨김 |
| 토스트 카드, passive 팝업 | 범위 밖이다. 겹친 WebView만 숨기는 규칙을 그대로 쓴다. 이 둘은 포커스를 받지 않는 층이라 페이지 입력을 가로채지 않는다 |

### 불투명 모드 표현

디자인 결정은 2026-10-21에 받았다. 사용자 설정·표시기·새 문자열은 없다. 갤러리 Overlays의 "Over a web page — opaque mode" 예제가 같은 판정 함수로 그린다.

- 모서리는 각지게 한다(`webview-opaque-radius` → radius-none → 0). 그림자는 없다(`webview-opaque-shadow` → shadow-none). 기존 1px 테두리는 유지한다.
- 판정은 상자 단위다. 상자 사각형이 보이는 불투명 모드 페이지와 면적을 공유하면 상자 전체에 적용한다. 변이 맞닿기만 하면 겹침이 아니다. 사이드바와 페이지에 걸친 메뉴도 상자 전체가 각진다.
- 드롭다운은 자기 사각형이 페이지와 겹치거나 부모 셸이 불투명이면 불투명이다. host 팝업은 콘텐츠를 그리는 동안 `with_opaque_parent`로 자기 판정을 알린다.
- scrim은 범위(surface 또는 창) 안에 불투명 모드 페이지가 하나라도 보이면 그리지 않는다. 페이지 밖만 어두워지는 얼룩을 피하기 위해서다. 페이지가 없거나 투명 모드면 지금처럼 그린다.
- 판정 기준인 페이지 영역은 직전 동기화가 불투명 모드로 드러내려는 WebView 사각형(`OverlayFrame::pages`)이다. 팝업이 처음 뜬 프레임에는 비어 있어 원래 모양으로 그리지만, 위 표시 판정 때문에 그 프레임에는 페이지가 보이지 않는다. 다음 프레임부터 각진 모양으로 그린 그림이 올라간 뒤 페이지가 보인다.
- 불투명 영역은 상자와 그 바깥(`StrokeKind::Outside`)에 그리는 1px 테두리다(`OverlayTarget::opaque_rect`, `outlined_box`). 소속 범위 밖 테두리는 넣지 않는다. 입력 판정과 각진 모서리 판정은 테두리를 넣지 않은 상자(`rect`)로 한다. egui 드롭다운은 테두리를 상자 안쪽에 그리므로 상자 그대로다.
- 불투명 영역은 egui가 실제로 칠하는 픽셀과 같아야 한다. 넓으면 페이지와 테두리 사이에 host 배경 한 줄이 보이고, 좁으면 페이지가 테두리를 덮는다. egui 테셀레이터는 테두리 종류와 관계없이 상자 네 변을 반올림해 픽셀에 맞추고(`round_rects_to_pixels`), egui-wgpu는 clip을 같은 반올림으로 scissor로 바꾼다. 그래서 상자와 소속 범위를 같은 배율(`pixels_per_point`)로 반올림한 뒤 테두리를 더한다(`painted_box`). 이 배율은 창 배율과 같다(egui zoom은 1로 고정, UI 확대는 Theme 크기에만 들어간다).
- 배율이 정수가 아니면 1px 테두리의 바깥 끝이 픽셀 중간에 온다. 그 행은 계조로 칠해지고 아래 바깥 넓힘이 구멍에 넣는다. 그래서 그 행에는 테두리 계조가 주 표면 배경 위에 섞여 보이고 페이지는 보이지 않는다(피할 수 없는 한계).

### 영역과 backend 계약

`WebViewOverlayBackend`(`src/host_api/webview/overlay.rs`)의 네 메서드다. 기본 구현은 지원 안 함이다.

| 메서드 | 계약 |
|---|---|
| `overlay_support() -> OverlayMode` | 지금 낼 수 있는 가장 좋은 모드. 매 프레임 묻는다. 실제로 측정해 확인한 모드만 보고한다 |
| `overlay_paint_source(mode) -> OverlayPaintSource` | 지원하는 모드의 그림 위치. 페이지에 구멍을 내 주 표면을 보이면 `MainSurface`, 별도 오버레이 창에 그리면 `OverlaySurface`다. 기본값은 `OverlaySurface`다 |
| `apply_overlay(&OverlayRequest) -> Result<(), OverlayApplyError>` | 대상 팝업이 보이는 프레임마다 부른다. 실패하면 `Unsupported` 또는 `Failed(이유)`를 돌려준다 |
| `clear_overlay()` | 오버레이를 끄고 페이지의 입력과 그림을 원래대로 돌린다. `set_native_holes`의 분할선·안내 카드 구멍은 이 호출과 별개로 유지한다 |

`OverlayRequest`:

- `mode`: 불투명 또는 투명이다. 숨김은 요청하지 않는다.
- `regions`: 호스트가 칠할 영역이다. WebView 창(`set_bounds`와 같이 반올림한 정수 사각형) 기준 물리 픽셀이고, 바깥쪽 정수 픽셀로 넓힌 뒤 창 밖은 자른다. 불투명 영역은 픽셀 경계에 맞춘 값이라, 물리로 되돌릴 때 생기는 f32 곱셈 오차(1/256 px 이하)만 정수로 붙인 뒤 넓힌다. 불투명 모드는 팝업·드롭다운 상자와 그 바깥 1px 테두리만 넣는다. 투명 모드는 그림자까지 넓힌 상자와 scrim 범위를 넣는다. 매 프레임 현재 배율로 다시 계산한다.
- `host_takes_pointer`: 참이면 WebView 영역 전체의 클릭·휠을 호스트가 받아야 한다. 대상 팝업이 보이는 동안 늘 참이다.
- `scale_factor`: 그 프레임의 배율이다.

입력·키보드·포커스·캡처 계약:

- **포인터**: 대상 층이 보이는 동안 WebView 영역 전체의 클릭·휠은 호스트로 온다(`host_takes_pointer`). 입력 의미는 숨긴 상태와 같다. 페이지 클릭은 egui의 바깥 클릭으로 도착하고, 그 처리는 팝업 정의를 그대로 따른다. `close_on_outside_click`(plugin은 `dismiss_on_outside_click`)이 참이면 닫고, 거짓이면 닫지 않고 언포커스만 하며, `sticky_focus`면 키보드 포커스를 유지한다([팝업 §포커스](popup.md#포커스)). 어느 경우에도 그 클릭은 페이지로 가지 않는다. 대상 층이 사라지면 다음 프레임부터 포인터가 페이지로 간다.
- **키보드·IME**: 대상 층이 보이는 동안 키와 IME는 페이지로 가지 않는다. 키는 지금처럼 포커스된 팝업이 받고, 포커스된 팝업이 없으면 호스트의 기존 규칙(터미널 등)을 따른다. 열 때의 포커스 회수(위 "포커스" 절)는 모드와 관계없이 유지한다. Linux에서 포인터가 페이지 위에 있으면 X 서버가 키를 WebView로 보내므로, 포인터를 호스트로 돌리는 처리가 이 라우팅도 막아야 한다. 오버레이 중 WebView가 전달한 키는 지금처럼 `webview_keys`에서 버린다. 닫을 때 native 자식 포커스를 자동 복원하지 않는다.
- **포커스**: 새 OS 층(오버레이 창 등)은 OS 포커스나 창 활성화를 가져가지 않는다. 창이 비활성일 때 first responder·포커스를 바꾸지 않는 판정(`host_window_has_os_focus`)을 그대로 따른다.
- **그리기**: 팝업은 주 표면에도 계속 그린다. 오버레이 창은 페이지 위 영역만 칠하므로 한 픽셀은 scrim과 정확히 한 번 합성된다(아래 GPU 분리).
- **캡처**: `ui.screenshot`은 주 표면을 읽으므로 팝업은 지금처럼 캡처에 남고 WebView 픽셀은 지금처럼 없다. 페이지와 팝업이 함께 보이는지는 OS 화면 캡처로 확인한다. 원격 attach mesh 중계는 host 팝업 픽셀을 보내지 않아 영향이 없다.

### GPU 분리 — 오버레이 paint job

직전 동기화의 모드가 숨김이 아니면 프레임 끝(`end_pass` 전)에 그 모드·그림 위치·`planned_pages`로 paint job을 만든다(`prepare_paint_job`). 아직 페이지를 드러내기 전이다. 그림 위치가 오버레이 창이면 그 창이 있을 때만 만들고, 대상 레이어의 shape를 복제해 담는다([GPU 렌더링 §WebView 오버레이 pass](../../dev-guide/gpu-rendering.md#webview-오버레이-pass)). 그림 위치가 주 표면이면 창 없이 shape 없는 job을 만들고, 렌더가 주 pass present 뒤에 그 기준을 기록한다. 주 표면에는 어느 경우에도 원래대로 모두 그린다.

- host·plugin 팝업 그룹 순서는 주 pass와 같은 `host_popup_should_render_on_top` 결정을 쓰고, 드롭다운은 맨 위다. plugin mesh 팝업 콘텐츠도 같은 그룹 규칙으로 오버레이 창에 합성한다.
- 각 shape와 plugin mesh primitive의 clip을 드러낼 페이지 사각형과 교차한다. 좌표는 옮기지 않는다. 페이지 밖의 scrim·그림자·mesh 콘텐츠는 주 표면에만 있어 두 번 합성되지 않는다.
- 오버레이 창은 주 창 클라이언트 영역과 원점·물리 크기가 같아야 한다. 크기가 다르면 그 프레임을 그리지 않고 오버레이 실패로 다룬다.
- shape가 없어도 오버레이 창을 투명으로 지운 프레임을 올린다. 이것도 그 모드·페이지로 준비된 그림이며, 지난 프레임 팝업이 오버레이 창에 남지 않는다.
- egui 텍스처 해제는 주 pass와 오버레이 pass가 모두 끝난 뒤에 한다.
- 오버레이 창은 아래 창 단위 host가 붙인다. 창이 없거나 host를 숨겨 두었으면 복제한 그림을 그리지 않고 기록도 남기지 않는다. 숨긴 채 준비 중인 host에 그린 그림은 화면에 없으므로 페이지를 드러내는 기록이 아니라 host를 보일 기록(`OverlayFrame::prepared`)으로 남긴다. 구멍 방식으로 주 표면을 보고하는 backend는 오버레이 창이 필요 없다.

### 창 단위 오버레이 host

페이지 단위 계약(`WebViewOverlayBackend`)과 나눈다. 오버레이 창은 주 창 클라이언트 영역과 같아야 하고 WebView는 창마다 여럿이라, 창 하나에 host 하나를 둔다. 둘은 `OverlayFrame`의 모드·그림 위치로만 만난다.

- 계약(`src/host_api/webview/overlay_host.rs`의 `OverlayNativeHost`): `surface_target`(wgpu surface 대상, host 수명 동안 유효), `composite`(합성 계약), `follow`(보유한 부모 창의 현재 위치·크기에 맞춤, 같으면 OS 호출 없음), `set_shown`, `raise`(그 창의 WebView native 창 위로). 포커스·창 활성화·포인터를 받지 않고 이벤트 루프 스레드에서만 쓴다. 숨김 중에는 그림이 보이지 않아야 하고, 숨긴 동안에도 surface에 present할 수 있어야 한다. 보인 직후에는 지난 구간 그림이 없어야 하며, 마지막 준비 그림 또는 완전히 투명한 빈 그림을 허용한다. 페이지는 보이는 host에 이번 그림을 present한 뒤에만 드러낸다. 공통 훅은 이번 구간 그림을 숨긴 채 present한 뒤에만 `set_shown(true)`를 부른다(아래 "순서"). 주 창 최소화·작업 공간·다른 앱과의 순서는 host가 주 창에 묶여(managed transient 또는 같은 창 안 자식) OS가 함께 처리한다. 공통 훅은 이를 묻지 않는다.
- 만들기는 `create_overlay_host(&Arc<Window>)`다. OS별 구현은 `overlay_host/{linux,macos,windows}.rs`의 `create`다. Linux는 ARGB 관리 창(아래 "Linux 투명 구현"), macOS는 winit 뷰 안의 형제 NSView(`SwapchainAlpha`)를 만들고 Windows는 `Unsupported`를 돌려준다.
- 수명: host는 부모 창의 `Arc<Window>`를 보유한다. `OverlaySurfaceParts`는 surface를 host보다 먼저 선언해 먼저 drop한다. 그래서 unsafe로 만든 surface가 사는 동안 부모 창(과 그 display 연결)이 살아 있다.
- 붙이기(`GpuState::attach_overlay_surface`): App의 Instance·Adapter와 창의 Device를 그대로 쓴다. 어댑터가 그 surface에 present할 수 없거나 surface 형식 목록에 주 surface 형식이 없으면 붙이지 않는다. 크기는 주 surface와 같고 present mode는 주 surface와 같은 규칙(Mailbox가 있으면 Mailbox, 없으면 Fifo)이다. alpha 설정은 caps에 있는 값 중 PreMultiplied, PostMultiplied, 첫 값 순서로 고른다. 이것은 그리는 규칙이며 합성 판정이 아니다.
- 상한(`overlay_surface_ceiling`)은 host 합성 계약과 alpha 설정으로 정한다.

| `composite()` | alpha 설정 | 상한 |
|---|---|---|
| `ExternalArgb`(외부 합성기가 ARGB 알파 반영, 그 OS·조건에서 측정한 host만) | 무엇이든 | 투명 |
| `SwapchainAlpha` | PreMultiplied·PostMultiplied | 투명 |
| `SwapchainAlpha` | 그 밖 | 불투명 |
| `OpaqueOnly` | 무엇이든 | 불투명 |
| `Unavailable` | - | 숨김 |

- 상태(`OverlayHostSlot`)와 훅: 같은 프레임의 WebView 동기화 바로 뒤에 `settle_overlay_host` 하나가 맞춘다. 필요하다는 것은 동기화한 모드가 숨김이 아니고 그림 위치가 오버레이 창인 것이다.

| 상태 | 조건 | 동작 |
|---|---|---|
| 아무 상태 | 창에 WebView가 없음 | 붙어 있으면 버리고 `Absent`. 그 밖에는 OS를 부르지 않는다 |
| `Absent` | 필요함 | 만들어 붙이고 숨긴 채 따라가며 준비한다. 성공이면 `Attached`, `Unsupported`면 `Unavailable`, 실패면 `FailedThisEpisode`와 경고 1회 |
| `Attached`(보이는 중) | 필요함 | 따라간다. 직전에 보이지 않던 페이지를 드러냈으면 다시 올린다 |
| `Attached`(숨김) | 필요함, 이번 프레임에 숨긴 채 지금 모드·그림 위치의 그림을 present함 | 따라간 뒤 보이고 올린다 |
| `Attached`(숨김) | 필요함, 그 밖 | 숨긴 채 따라가며 준비한다. 다음 오버레이 pass가 이 host에 그린다 |
| `Attached` | 필요 없음 | 숨기고 준비도 멈춘다. 버리지 않는다 |
| `Attached` | surface를 잃음 | `FailedThisEpisode`. 그 실패는 오버레이 pass가 기록했다 |
| `FailedThisEpisode` | 대상 팝업이 닫힘 | `Absent`. 다음 구간에 한 번 다시 시도한다 |
| `Unavailable` | - | 창 수명 동안 다시 시도하지 않는다 |

- 순서: 팝업이 처음 뜬 동기화는 host가 없어 모드를 낮추지 않고 페이지를 숨긴 채 다시 그리기를 요청한다. 같은 프레임 훅이 host를 만들어 숨긴 채 준비한다. 다음 프레임 렌더가 숨긴 host에 지운 뒤 현재 그림을 그려 present하고, 그 훅이 기록의 모드·그림 위치가 지금과 같으면 host를 보이고 올린다. 그다음 프레임 렌더가 보이는 host에 present한 뒤 동기화가 페이지를 드러낸다. 그러므로 숨긴 채 그리기 → 보이기 → 그리기 → 드러내기 순서이고 GPU나 OS를 기다리지 않는다. 숨겨 두었던 host를 다시 쓸 때도 같은 순서다. 숨기기 전 그림이 OS 층에 남아 있어도 보인 직후 지난 그림이 노출되지 않아야 한다. 마지막 준비 그림 또는 완전히 투명한 빈 그림을 허용한다. 모드가 바뀌어 기록이 맞지 않으면 보이지 않고 한 번 더 준비한다. 숨긴 채 그리다 실패하면 보이지 않고 오버레이 pass의 실패 기록으로 숨김에 내려간다. 만들지 못하면 다음 동기화가 상한 숨김으로 모드를 낮추고, 구멍 방식 불투명은 그대로 쓴다.
- 숨긴 채 present한 그림이 보이기 전에 OS 합성에 반영되는지는 앱이 알 수 없다. present 반환은 GPU 제출이고 화면 반영 확인이 아니다. 숨긴 상태의 present 가능 여부와 보이기 직후 첫 화면은 OS host마다 측정해야 하며, 측정하지 않은 OS host는 투명 합성을 보고하지 않는다. 2026-10-22 macOS 실험(NSView + CAMetalLayer, 입력 없음)에서 숨겼다 다시 보인 layer는 다음 present 전에 숨기기 전 그림을 보였고, 숨길 때 layer contents를 비우면 그 뒤 present도 보이지 않았다. 추가 실험에서는 숨긴 상태의 acquire·present 33회가 모두 반환했지만, 빠르게 다시 보인 5회 모두 지난 그림이 관측됐다. GPU 작업 완료 직후 보인 경우도 5회 중 2회 지난 그림이 남았다. 따라서 현재 macOS는 불투명을 유지하며, GPU 작업 완료만으로 새 그림의 화면 반영을 보장하지 않는다.
- 새로 드러낸 페이지 위로 host를 올리는 것은 Linux GTK `show_all`과 macOS 새 WKWebView가 위로 올라오기 때문이다. 올린 프레임에는 한 번 더 그린다.
- 주 창 `Resized`·`ScaleFactorChanged`는 `GpuState::resize`가 오버레이 surface도 같은 크기로 다시 설정하고 host를 따라가게 한다. `Moved`는 붙은 host만 따라가게 한다. 종료 때 WebView를 숨기는 경로가 host도 버린다.
- 비용: WebView가 없는 창은 OS 호출이 없다. 합성 계약은 대상 팝업이 WebView 위에 떠 있고 그림 위치가 오버레이 창일 때만 묻는다.

### debug 재현

debug 빌드의 `debug.webview.overlay`(CLI `tasty debug webview-overlay [--force off|hide|opaque|transparent]`)는 마지막 동기화의 모드·그림 위치·host 상태(`host`)·대상·페이지와 present한 그림의 기준(모드·그림 위치·페이지·표면 크기)을 돌려준다. `--force`는 backend의 지원 보고를 덮어쓴다. 지원하지 않는 backend에 `opaque`를 강제하면 적용이 `Unsupported`로 실패하고 숨김과 경고 경로를 그대로 지난다([debug IPC](../../dev-guide/debug-ipc.md)).

### 기각한 대안

- **스냅샷으로 대체**(WebKitGTK `get_snapshot`, WKWebView `takeSnapshot`, WebView2 `CapturePreviewAsync`): 정지 화면이고 비동기 왕복 동안 첫 프레임이 비어 보인다.
- **겹치는 WebView만 숨기기**(토스트·passive 팝업 방식): 서피스 타입 전환 팝업은 대상 surface 가운데에 떠서 정작 그 WebView가 숨는다.
- **offscreen 엔진 교체**(CEF OSR, WPE): 번들 크기·업데이트 부담과 입력·IME 중계 비용이 크다.

### Linux X11 측정 사실과 범위

2026-10-16 측정이다. 환경은 xrdp 세션(GNOME Shell, 컴포지팅 매니저 있음)과 Xvfb(창 관리자·컴포지팅 매니저 없음), GPU는 NVIDIA Vulkan과 비교용 lavapipe·llvmpipe GL이다. 실험 프로그램은 Tasty와 같은 방식으로 WebView를 만들었고 제품 코드에서 측정한 값이 아니다.

- 부모 창 아래의 ARGB 자식 창은 컴포지팅 매니저 유무와 관계없이 알파가 합성되지 않았다. GPU 없이 X 픽셀을 직접 넣어도 같아 원인은 X 서버 쪽이다. 자식 창 방식으로는 불투명 사각형만 된다.
- 투명 합성은 최상위 override-redirect ARGB 창이고 컴포지팅 매니저(`_NET_WM_CM_S0` 소유자)가 있을 때만 됐다. 이 방식은 빈 입력 shape가 필수다. 2026-10-22 소유 Xvfb와 격리 GNOME Shell(mutter 46, llvmpipe 합성) 실험에서 override-redirect 창은 부모 창 이동을 따라가지 않았고, 부모 창 최소화·다른 작업 공간 전환·다른 앱 활성화 뒤에도 화면에 남았다. 부모 창을 `WM_TRANSIENT_FOR`로 둔 관리 창(utility·dialog 종류, 입력 힌트 거짓, 작업 표시줄·페이저 제외, 장식 없음)은 최소화·작업 공간·다른 앱과의 순서를 부모 창과 함께 따랐고 만들 때 활성 창을 바꾸지 않았다. 이동은 두 방식 모두 앱이 옮겨야 한다. 다른 창 관리자는 측정하지 않았다.
- 오버레이 wgpu 표면은 주 표면과 같은 `Device`로 configure됐다. NVIDIA Vulkan과 GL은 `Opaque`만 보고했는데도 컴포지팅 매니저가 알파를 반영했다. 드라이버 동작에 기대는 결과다.
- GTK `show_all`이 WebView X 창을 맨 위로 올린다. `set_visible(true)`가 `show_all`을 부르므로 WebView를 보일 때마다 오버레이를 다시 올리고 같은 프레임에 다시 그려야 한다. 이 실험의 override-redirect 창에서는 배경 None으로 리사이즈 중 검정 프레임을 피했다. 현재 관리 창은 아래 투명 구현의 알파 0 배경을 쓴다.
- 입력: WebView에 팝업과 같은 사각형 input 구멍을 내면 그 클릭은 winit으로 온다. WebView 전체를 input 구멍으로 하면 본문 클릭·휠·키가 winit으로 와 바깥 클릭 dismiss가 가능하다. 구멍이 없으면 포인터가 본문 위일 때 키가 WebView로 간다. 키는 XTEST로만 넣었고 egui 텍스트 입력 경로는 확인하지 않았다.
- 불투명 방식에서는 본문이 scrim으로 어두워지지 않아 WebView 밖만 어두운 얼룩이 생기고, 상자 밖 그림자와 둥근 모서리 바깥은 표현할 수 없었다. 투명 방식에서 두 표면에 같은 scrim을 그리면 WebView 밖 영역이 두 번 어두워졌다.
- 이동 중 오버레이 가장자리 몇 px의 1프레임 잔상을 관측했다. tearing은 측정하지 않았다.

### Linux 불투명 구현

`src/host_api/webview/linux.rs`의 `WebViewOverlayBackend` 구현과 shape 합성(`src/host_api/webview/linux_shape.rs`)이다.

- `overlay_support`(`linux/overlay_backend.rs`): XFixes 2.0과 서버 SHAPE 확장을 생성 때 확인해 둘 다 있으면 투명, 아니면 숨김이다. 둘 중 하나가 없으면 분할선·안내 카드 구멍도 두지 않는다. 투명을 낼 수 있는지는 창 단위 host 상한이 정하고, 낼 수 없으면 공통 동기화가 불투명으로 낮춘다.
- `overlay_paint_source`: 불투명은 주 표면이라 오버레이 창을 만들지 않는다. 투명은 오버레이 창이다.
- `apply_overlay`: 숨김이면 `Unsupported`다. 요청 영역과 `host_takes_pointer`를 저장하고 shape를 다시 맞춘다. `clear_overlay`는 저장을 지우고 다시 맞춘다.
- shape 합성: X 창의 shape는 하나라서 `set_native_holes`의 입력 구멍·그림 구멍과 오버레이 영역을 한 함수(`linux_shape::compose`)로 합친다. 그림(`ShapeBounding`)에서는 안내 카드와 불투명 모드의 팝업 상자를 뺀다. 투명 모드는 페이지 그림을 그대로 두고 입력만 가져간다. 입력(`ShapeInput`)은 `host_takes_pointer`면 빈 region이고, 아니면 모든 구멍을 뺀다. 마지막 적용값과 같으면 X 요청을 보내지 않는다. 동기화는 `apply_overlay`를 `set_bounds`보다 먼저 부르므로 창 크기가 바뀐 프레임은 뒤의 `set_native_holes`가 새 크기로 다시 맞춘다. 오버레이를 지우면 native 구멍만 남은 shape로 돌아간다.
- 키: 입력 shape가 비면 포인터가 페이지 위에 있어도 X 서버가 키를 부모 창으로 보낸다. 그래서 오버레이가 포인터를 가져간 동안 `receives_keyboard_input`은 X 포커스가 WebView 안에 있는지만 본다. 열 때의 포커스 회수와 닫을 때 복원하지 않는 규칙은 공통 동기화가 그대로 맡는다. 이 backend는 포커스를 옮기지 않는다.
- 표시: GTK `show_all`이 매 프레임 불려도 X 창 shape는 유지됐다(아래 측정).

2026-10-22 격리 debug 인스턴스(Xvfb, 창 관리자 없음)에서 확인한 범위다. 배율은 1, 그리고 `WINIT_X11_SCALE_FACTOR`로 준 2와 1.25다. 첫 불투명 측정의 배율 2는 `GDK_SCALE=2`를 함께 줬다. 아래 픽셀 경계 측정의 배율 2·1.25는 `GDK_SCALE`을 주지 않았다(배율 1.25 실행 로그의 GDK 배율은 1).

- 명령 팔레트·서피스 종류 전환 팝업·Tools 메뉴에서 모드는 불투명, 그림 위치는 주 표면이고 페이지가 보였다. 팝업 상자와 1px 테두리 밖의 페이지 픽셀은 열기 전과 같았다. 상자는 각지고 그림자가 없었다.
- X 창 shape를 직접 읽어 확인했다. 그림 구멍은 요청 영역과 같았고 입력 shape는 비었다. 팝업이 닫히면 native 구멍만 남은 shape(분할선 입력 띠)로 돌아갔다.
- 팝업이 떠 있는 동안 페이지 링크 클릭은 팝업을 닫기만 했고 페이지 탐색이 없었다. 휠과 `End` 키는 페이지를 움직이지 않았다. 포인터를 페이지 위에 둔 채 친 글자는 팔레트 입력란에 들어갔다. 같은 클릭·휠·키는 팝업이 없을 때 페이지에 갔다.
- 창 크기 변경, 좌우로 나눈 두 WebView, 배율 2에서 구멍과 그림이 따라왔다. 다른 X 클라이언트가 포커스를 가진 동안 debug 경로로 팝업을 열고 닫아도 X 포커스는 그대로였다.
- 불투명 영역을 egui 픽셀 경계에 맞춘 코드(위 불투명 영역 규칙, `painted_box`)로 명령 팔레트와 서피스 종류 전환 팝업을 배율 1·2·1.25에서 측정했다. 두 팝업 모두 상자의 y가 0.5 논리 px에 놓인 경우다. 테두리 바깥 1px 링의 네 변 픽셀은 열기 전 페이지와 모두 같았고, 테두리 맨 바깥 행·열에 주 표면 배경색은 없었다. X 창 shape의 구멍은 egui가 그린 테두리 바깥 끝과 같았다. 배율 1에서 상자 위 변이 150.5면 구멍은 테두리가 시작하는 150부터이고 그 위 149행은 페이지다. 배율 1.25의 테두리 바깥 계조 행은 테두리 색이 주 표면 배경 위에 섞인 색이다(위 비정수 배율 한계). egui 드롭다운·plugin 팝업과 배율 1.1·1.2·1.75는 실제 실행하지 않았고 epaint 테셀레이터와 대조하는 단위 시험만 있다.
- 창 관리자가 있는 세션, IME, X 포커스가 WebView 자식 안에 있는 상태(이 Xvfb에서는 페이지를 눌러도 포커스가 부모 창에 남았다)는 이 실행에서 확인하지 않았다.

### Linux 투명 구현

창 단위 host(`src/host_api/webview/overlay_host/linux.rs`)다. 주 창마다 오버레이 창 하나를 둔다.

- 만들기: host가 자기 X 연결을 열어 합성 관리자 선택(`_NET_WM_CM_S<화면>`) 소유자, XFixes, SHAPE, 32bit TrueColor visual을 확인한다. 하나라도 없으면 `Unsupported`라 그 창은 창 수명 동안 불투명(주 표면)을 쓴다. 합성 관리자가 나중에 생겨도 그 창에서는 다시 만들지 않는다.
- 창: root의 자식인 32bit ARGB 관리 창이다. 배경 픽셀은 알파 0이다. 서버가 map·노출 때 투명하게 칠하므로 이전 구간 그림이나 아래 화면 사본을 노출하지 않는다. `WM_TRANSIENT_FOR`는 주 창, 입력 힌트는 거짓, `_NET_WM_USER_TIME`은 0이다. `_NET_WM_STATE`는 작업 표시줄·페이저 제외, 종류는 utility, 장식은 없고 입력 shape는 비었다. 전역 위(ABOVE) 상태는 두지 않는다. 창 관리자가 주 창과 함께 최소화·작업 공간 이동·다른 앱과의 순서를 처리한다.
- 합성(`composite`): 합성 관리자 소유자가 있으면 `ExternalArgb`다. 오버레이 surface의 alpha 설정이 `Opaque`여도 합성 관리자가 알파를 반영한다. 소유자 변화는 만들 때 신청한 XFixes 선택 이벤트로 알고, 사라지면 `Unavailable`과 `tracing::warn!`, 돌아오면 info다. 서버 왕복 요청은 쓰지 않는다.
- 따라가기(`follow`): 주 창 configure 이벤트가 온 뒤에만 주 창 위치를 다시 읽고(서버 왕복 두 번) 오버레이 창을 옮긴다. 숨긴 동안의 준비·크기 변경에서도 맞추므로 숨긴 채 present하는 표면과 X 창의 크기가 같다. 이동·크기는 창 관리자가 옮기지 않으므로 host가 맞춘다.
- 창 관리자 거절: 창 관리자는 프로그램이 요청한 관리 창 위치를 바꿀 수 있다. 오버레이 창 configure가 응답을 기다리는 요청과 다르면 그 주 창 위치에서는 `Unavailable`과 경고다. 주 창이 다른 위치로 움직이면 다시 시도한다(`overlay_host/linux/fit.rs`). 거절된 위치로 보인 첫 프레임은 어긋난 그림이 한 번 보일 수 있다.
- 표시: 보이기는 map이며 별도 채우기를 보내지 않는다. 숨기기는 ICCCM withdraw(unmap과 root에 보내는 합성 UnmapNotify)다. 관리자가 이전 map 요청을 뒤늦게 처리한 MapNotify를 숨김 상태에서 받으면 다시 withdraw한다. 배경 None과 단순 unmap 조합에서는 뒤늦게 map된 창에 아래 화면의 불투명 사본이 남는 것을 관측했다.
- 올리기(`raise`): 아무것도 하지 않는다. WebView X 창은 주 창의 자식이라 최상위 오버레이 창 위로 오지 못한다. `XRaiseWindow`를 보내면 창 관리자가 주 창 무리를 다른 앱 위로 올릴 수 있다.
- 수명: host가 X 연결을 소유하고 Drop에서 창·colormap을 지운 뒤 연결을 닫는다. surface가 host보다 먼저 drop되는 공통 순서를 따른다.
- 입력: WebView X 창 입력 shape는 불투명과 같이 비고, 오버레이 창 입력 shape도 비어 있어 포인터와 포인터로 정해지는 키는 부모(winit) 창으로 간다.

2026-10-22 소유 Xvfb와 격리 GNOME Shell(mutter 46, 합성 관리자 있음, NVIDIA Vulkan, 오버레이 surface alpha `Opaque`)에서 debug 인스턴스로 확인한 범위다. 배율은 1과 `WINIT_X11_SCALE_FACTOR=1.25`다.

- 명령 팔레트·서피스 종류 전환(surface scope)·이름 바꾸기 다이얼로그·파일 피커와 경로 막대 드롭다운·plugin mesh 팝업(egui-mesh 데모)에서 모드는 투명, 그림 위치는 오버레이 창이었다. 페이지가 보였고 둥근 모서리와 그림자가 페이지 위에 그려졌다.
- scrim은 열기 전후 OS 캡처로 판정했다. 페이지와 WebView 밖 부모 영역 모두 열기 전 값의 절반(a 0.5 검정 한 번)이었고 두 번 어두워진 픽셀은 0이었다. 좌우로 나눈 두 WebView와 배율 1.25, 합성 관리자가 돌아온 뒤도 같았다. surface scope 팝업은 그 surface 페이지만 어두워졌다.
- 팝업이 떠 있는 동안 페이지 링크 클릭은 팝업만 닫았고 페이지 탐색은 없었다. 휠과 `End`는 페이지를 움직이지 않았고, 포인터를 페이지 위에 둔 채 친 글자는 팔레트·다이얼로그 입력란에 들어갔다. mesh 팝업 버튼 클릭은 plugin으로 갔다. 닫은 뒤에는 같은 링크 클릭이 페이지로 갔다.
- 오버레이 창 생성·표시 뒤에도 활성 창은 주 창 그대로였다. 주 창 이동·크기 변경을 따라갔다. 주 창 최소화와 다른 작업 공간 전환 때 함께 숨었고 복원 때 다시 보였다. 다른 앱이 활성이면 그 앱 아래, 주 창이 다시 활성이면 주 창 바로 위였다.
- 팝업을 띄운 채 새 WebView 탭을 만들거나 탭을 바꿔 다른 WebView를 보여도 오버레이 창은 위에 남았고 새 페이지 위에 팝업이 그려졌다.
- 전체화면 주 창과 사용자가 일부를 화면 밖으로 끌어낸 주 창에서는 mutter가 오버레이 창을 작업 영역 안(위 패널 아래, 화면 안)으로 옮겼다. 위 거절 처리로 불투명으로 내려갔고 전체화면을 풀자 투명으로 돌아왔다.
- 합성 관리자(GNOME Shell)를 끄면 경고와 함께 불투명으로 내려갔고, 다시 띄우자 투명으로 돌아왔다. 합성 관리자가 없는 Xvfb에서는 오버레이 창을 만들지 않고 불투명이었다.
- 숨긴 채 준비 present한 뒤 map 직전·직후에서 제품 프로세스만 멈추고 합성 관리자의 화면을 캡처했다. 처음 표시·다시 열기·숨김 중 1280×720에서 1400×850으로 크기 변경 모두 map 직후 창의 모든 픽셀이 알파 0이었다. 준비 그림도 남지 않았으며 다음 보이는 present에서 팝업이 나타났다. 별도 빨강 픽셀을 넣은 대조군은 화면에 나타났으므로 캡처가 창 전체를 누락한 것은 아니다. 이는 mutter에서 정지시킨 경계의 관측이며 모든 드라이버·합성 관리자의 순간적인 프레임을 보장하지 않는다.
- 새 숨김 준비 순서에서 NVIDIA Vulkan과 lavapipe Vulkan을 각각 강제해 처음 만든 창·다시 표시하는 창·숨김 resize의 acquire/present 3회씩을 계측했다. 모두 반환했고 숨긴 acquire는 NVIDIA 0.2~0.6ms, lavapipe 5.3~7.6ms였다. present 호출은 각각 1.5~1.7ms와 2.7~3.0ms였으며 GPU 완료나 화면 반영 시간은 아니다. 디버거 계측이 포함된 표본이다. 두 드라이버 모두 전체화면 거절 때 불투명, 복귀 뒤 투명이며 resize 뒤 실패 상태가 없었다.
- `tasty screenshot --window`는 팝업을 담고 WebView 픽셀은 없었다. 팝업을 띄운 채 창 닫기 요청으로 종료했을 때 경고·X 오류 없이 끝났다.
- 다른 창 관리자·합성 관리자(KWin, picom 등), 실행 중 배율 변경, 다중 모니터, IME, 사용자 데스크톱 세션은 확인하지 않았다. 창 관리자 거절 감지는 mutter에서만 실제로 일으켰다.

Windows는 지금의 그림 경로(wgpu surface)로 투명을 낼 수단을 찾지 못했다. Windows의 불투명 구현과 측정한 투명 후보의 제약, macOS에서 확인한 알파 합성과 표시 순서의 한계는 아래에 적는다.

### Windows 불투명 수단

Windows backend(`src/host_api/webview/windows.rs`)는 불투명 모드만 적용한다. 그림 위치는 주 표면이다.

- 그림: 대상 팝업 영역을 WebView child HWND의 창 region에서 뺀다. 안내 카드 구멍(`set_native_holes`)과 같은 region 하나를 쓰므로 두 입력을 함께 보관해 전체 사각형에서 둘을 모두 뺀다(`window_region.rs`). 한쪽 갱신이 다른 쪽 구멍을 지우지 않는다. `clear_overlay`는 안내 카드 구멍만 남긴 region으로 돌리고, 구멍이 없으면 region을 지운다. HWND 크기나 배율이 바뀐 프레임에는 새 region을 적용하고, 같으면 다시 설정하지 않는다.
- 포인터: 대상 팝업이 보이는 동안 `EnableWindow(false)`로 WebView child HWND를 끈다. 꺼진 창과 그 자식 WebView2 창은 포인터를 받지 않으므로 그 자리의 클릭·휠은 부모(winit)로 간다. 끈 뒤 `IsWindowEnabled`로 다시 읽어 바뀌지 않았으면 적용 실패다. `clear_overlay`가 다시 켠다. 포커스는 옮기지 않는다.
- 실패: region이나 창 상태를 적용하지 못하면 `Failed`를 돌려주고, 공통 경로가 숨김으로 돌아가며 `clear_overlay`로 되돌린다.
- 확인 범위: Windows 11 기기의 사용자 데스크톱 세션에서 격리 debug 인스턴스로 html·markdown 페이지 위에 host 팝업을 열어 확인했다. 창은 활성화하지 않았고 전면 창은 측정 내내 다른 앱 그대로였다. 팝업이 열린 동안 WebView child HWND는 꺼졌고, 창 region은 팝업 상자만 뺀 사각형이었다. 팝업 바깥 페이지 위 점과 팝업 안 점의 `WindowFromPoint`는 모두 winit 창이었다. 닫은 뒤에는 region이 지워지고 창이 다시 켜졌으며 같은 점이 WebView2 렌더 창으로 돌아왔다. 창 크기를 바꾸면 새 크기의 region이 적용됐고 이동만 하면 그대로였다. 화면 캡처에서 팝업 바깥은 페이지, 팝업 상자 안은 주 표면이 그린 팝업이었다.
- markdown 페이지 위 서피스 종류 전환 팝업: 첫 행(터미널)을 누르면(debug 입력 주입) 팝업이 닫히고 `tasty list surfaces`에서 그 surface가 Terminal로 바뀌었으며 WebView child HWND가 사라졌다.
- 키 경로: 앱이 전면이 아니어서 실제 장치 키는 이 앱에 오지 않는다. 그래서 OS가 키를 보내는 자리인 GUI 스레드 포커스 창에 키 메시지를 직접 넣어 확인했다. 페이지 렌더 창에 클릭 메시지를 넣으면 포커스가 WebView2로 갔고 PageDown이 페이지를 움직였다. 팝업을 열자 포커스가 winit 창으로 돌아왔다. 그 상태의 PageDown은 페이지를 움직이지 않았고 Esc는 팝업을 닫았다. 닫은 뒤에도 포커스는 winit 창에 남았다(자동 복원 없음). 페이지를 다시 누르자 포커스가 WebView2로 돌아왔고 PageDown이 페이지를 움직였다.
- 측정하지 않은 것: 실제 마우스·키보드 장치 입력(사용자 커서와 전면 창을 바꾸지 않으려고 OS hit test, 포커스 창 메시지, debug 입력 주입으로 나누어 확인했다), IME 조합(OS가 조합을 보내는 포커스 창이 팝업 동안 winit 창인 것만 확인했다), 배율 변경, 안내 카드와 팝업 구멍이 동시에 있는 실제 화면, 팝업이 열린 채 최소화·복원.
- 투명 모드는 `Unsupported`다. 같은 기기(NVIDIA RTX 5090, 드라이버 616.56, WebView2 155)에서 WebView2 위에 빨간 불투명 상자·50% 검정 띠·완전 투명 영역을 그리는 측정 프로그램으로 두 후보를 쟀다.
  - 후보 1(WebView2 형제 위 자식 창에 wgpu surface): Vulkan surface는 alpha 모드로 `PreMultiplied`를 보고하고 그 설정으로 그렸지만 화면에서는 투명 영역이 검정이었다. 이 경로에서는 surface caps의 알파 모드가 실제 합성을 뜻하지 않는다. DX12는 `Opaque`만 보고하고 검정이었다. `WS_EX_LAYERED`를 붙이거나 `LWA_ALPHA`를 걸어도 같았다. GL은 layered 자식 창에 그림이 보이지 않았다.
  - 같은 그림을 wgpu로 그려 CPU로 읽고 `UpdateLayeredWindow`(픽셀 알파)로 layered 자식 창에 올리면 띠가 페이지와 반씩 섞이고 투명 영역에 페이지가 보였다. 다만 wgpu surface가 아니라 프레임마다 GPU에서 CPU로 읽는 별도 그림 경로다. layered 자식 창은 Windows 8 이상을 선언한 manifest가 있어야 만들어지고, manifest가 없는 실행 파일(지금 Tasty와 같다)에서는 창 생성이 `0x800700CB`로 실패했다. 또 `HTTRANSPARENT`로 통과한 hit test는 winit 부모가 아니라 형제 WebView2 렌더 창으로 갔다(같은 스레드 `WindowFromPoint`). 그래서 포인터를 호스트로 보내려면 지금처럼 WebView child HWND를 끄는 처리가 계속 필요하다.
  - 후보 2(CompositionController): 이 기기에서 만들어졌다. 같은 DirectComposition 트리에 wgpu DX12 비주얼을 올리면 `Opaque`라 투명 영역이 검정이었다. 미리 곱한 알파의 D3D11 composition swapchain을 올리면 띠가 섞이고 투명 영역에 페이지가 보였다. 이 경우 WebView2는 HWND 없이 그려지므로 모든 포인터가 호스트 창으로 오고 페이지 입력은 호스트가 전달해야 한다.
  - 두 후보 모두 투명을 내려면 wgpu surface가 아닌 새 그림 경로나 WebView2 호스팅 방식 변경이 필요하다. 그래서 Windows 기본은 불투명이다. 측정은 이 기기 한 대, 배율 1, 한 GPU 드라이버 조합이다.

### macOS 측정 사실과 범위

2026-10-22 측정이다. 환경은 Apple M2 Pro, macOS 27.0.1, 배율 2 디스플레이, wgpu Metal이다. 측정은 Tasty가 아니라 입력 없는 소유 probe가 했다. probe는 제품 host(`overlay_host/macos.rs`)와 mask 계산(`page_mask.rs`)을 그대로 가져다 쓰고, WKWebView는 Tasty와 같은 방식(winit 뷰에 `addSubview`)으로 붙였다. 앱은 Accessory 정책으로 띄웠고 activate하지 않았으며 창도 비활성으로 열었다. 받은 창 이벤트는 `Focused(false)`뿐이었다. 픽셀은 probe가 자기 창을 `CGWindowListCreateImage`로 읽었다. 화면 기록 권한이 없어 `screencapture`는 실패했고, 권한은 요청하지 않았다.

- **불투명(구멍)**: WKWebView layer에 구멍을 뺀 겹치지 않는 사각형들로 `CAShapeLayer` mask를 걸면 구멍 안에 주 표면 픽셀이 보이고 나머지는 페이지다. mask는 `hitTest:`를 바꾸지 않는다. 그래서 포인터 제외는 따로 한다.
- **포인터**: `hitTest:`가 nil인 위쪽 뷰는 건너뛰어지고 아래 형제나 winit 뷰가 결과가 된다. 판정은 `hitTest:` 조회로 확인했다. 이벤트를 만들지 않았으므로 실제 클릭·휠 전달은 측정하지 않았다. 대상 팝업이 보이는 동안 `KeyWebView`는 뷰 전체에 nil을 돌려준다.
- **형제 host**: wgpu가 host 뷰에 `CAMetalLayer` 하위 layer를 만든다. caps alpha는 `Opaque`·`PostMultiplied`이고, `PostMultiplied`로 설정하면 layer가 opaque=false가 된다. alpha 0으로 지우면 아래가 그대로 보였다. 반투명 색은 premultiplied source-over로 디스플레이 색공간에서 합성됐다(빨강 α0.5·파랑 α0.25의 측정값이 premultiplied 계산과 맞고 straight 계산과 맞지 않았다).
- **순서·크기**: host를 붙인 뒤 `addSubview`한 WKWebView는 host 위에 왔고, `raise` 뒤에는 다시 host가 위였다. 창 크기를 바꾸면 host 뷰와 Metal layer가 새 크기를 따라갔고 contentsScale은 2였다. 숨기면 합성에서 빠졌고, drop하면 superview에서 빠졌다.
- **한계**: host를 숨겼다가 다시 보이면 첫 present 전까지 지난 present가 보였다. Metal layer의 contents를 비우면 그 뒤 present도 보이지 않았다. host만으로는 "보인 직후 첫 present 전에는 지난 그림이 보이지 않아야 한다"를 지킬 수 없으므로 macOS는 투명 모드를 보고하지 않는다. `--force transparent`로만 이 경로를 탄다.

macOS가 보고하는 모드는 불투명이고 그림 위치는 주 표면이라 창 단위 host를 쓰지 않는다. 적용은 팝업 상자를 안내 카드 구멍과 합쳐 한 mask로 만든다. layer가 없어 적용하지 못하면 `Failed`를 돌려주고 공통 경로가 숨김과 경고로 처리한다. 측정하지 않은 항목은 다음과 같다.

- Tasty 실행 상태의 동작(제품 경로 `debug webview-overlay`·팝업 열기). 기존 debug 기동은 시작 창을 활성으로 열고 winit 기본값이 앱을 activate해 포커스를 가져갈 수 있다. 활성화 없이 띄우는 기존 방법이 없어 자동 기동하지 않았다.
- 실제 클릭·휠·키보드·IME, 팝업을 닫은 뒤 포인터 복귀.
- 배율이 다른 디스플레이 사이 이동, 전체화면, 최소화.

<a id="도메인-라이브러리로는-아무것도-새지-않는다"></a>

## 도메인과 OS 구현의 경계

native WebKit·AppKit·WebView2 타입은 호스트 OS adapter에 둔다.
도메인에서는 NavState 같은 OS 비의존 값만 사용한다.
의존성은 crates/*/Cargo.toml과 webview 타입의 실제 사용처를 확인한다. 주석에 이름이 등장하는 것과 타입 의존은 구별한다.
headless가 사용하지 않는 GUI 의존성은 [빌드 경계](../../dev-guide/headless-build-boundaries.md)를 따른다.

<a id="이-문서가-못-말하는-것"></a>

## 플랫폼별 검증 한계

각 OS의 컴파일은 공유 호출부와 타입을 검사한다. 실제 클릭·단축키·IME·크기·종료·리소스 차단까지 검사한 것은 아니다.
기존 근거에서 Linux/X11의 실행 확인과 macOS·Windows의 소스·컴파일 확인은 구별돼 있다.
macOS의 실제 클릭 제스처 판정, macOS 메뉴바와 키 중복 처리는 실제 환경에서 확인해야 하는 항목이다.
Windows의 같은 문서 fragment 이동과 클릭 증명은 Windows 11 사용자 세션에서 OS 합성 클릭(`mouse_event`)으로 확인했다. 대상은 로컬 Markdown의 폴더 버튼·주소 Go, mirror의 폴더 버튼·새로고침이다. mirror는 같은 머신의 격리 인스턴스와 SSH 터널로 붙인 Linux 격리 인스턴스로 쟀고, 새로고침은 같은 머신에서만 쟀다. 사람 손의 클릭과 Markdown 문서 안 링크 클릭은 측정하지 않았다.
이 문서의 소스 설명만으로 해당 플랫폼의 실행 검증을 대신하지 않는다.

## 설정 출처 — kind 를 등록한 plugin

webview surface 의 zoom·JS·원격 콘텐츠·색 구성표는 그 kind 를 등록한 plugin 의 설정에서 읽는다.
번들·커스텀 plugin 을 구분하지 않으며 kind 이름으로 고정한 대응표가 없다. 등록할 때
`webview_kind::register_webview_kind` 가 kind 마다 설정 소유 plugin 과 기본값을 기록하고,
`HtmlWebViewSettings::resolve` 가 생성 때와 매 동기화 때 값을 정한다. 같은 kind 를 다른 plugin 이
다시 등록하면 surface 등록부처럼 나중 등록이 설정 출처가 된다.

| 저장 키 (`plugin_settings.<plugin_id>.*`) | 값 | 호스트 기본값 | 요청하는 연산 |
|---|---|---|---|
| `zoom` | 수치(%), 25..=500 | 100 | `set_zoom` |
| `sandbox_scripts` | bool, 켜면 JS 끔 | 켬(JS 끔) | `set_javascript_enabled` |
| `allow_remote_content` | bool | 끔 | `set_remote_content_allowed` |
| `color_scheme` | `follow`·`light`·`dark` | `follow` | `set_color_scheme` |

값은 위에서부터 먼저 있는 것을 쓴다.

1. 사용자가 저장한 값. 저장된 값의 타입이 다르면 없는 것으로 본다.
2. 그 plugin 의 설정 페이지(`[[contributes.settings_pages.items]]`)에서 같은 `storage_key` 를 쓰는 항목의 `default`. 설정 창이 보여 주는 기본값과 적용값이 갈리지 않게 먼저 본다.
3. kind 선언의 `[surface_kinds.webview_defaults]`. 설정 창에 노출하지 않을 기본값을 정할 때 쓴다. markdown 은 `sandbox_scripts = false` 로 JS 를 기본으로 켠다.
4. 호스트 기본값.

zoom 단축키는 kind 가 `zoomable = true` 이고 webview kind 로 등록돼 있으면 그 설정 소유 plugin 의 `zoom` 을 10 씩 바꾸고, 초기화는 위 순서의 2~4 로 정한 기본값으로 돌린다. html 은 `zoomable` 을 선언하지 않아 단축키 대상이 아니다.

html 은 이 값을 전역 sandbox 로 받고 문서마다 스크립트를 판정하는 게이트(`attach_script_gate`)를 따로 붙인다. 게이트는 html kind 에만 붙으므로 커스텀 webview kind 의 `sandbox_scripts` 는 문서 판정 없이 그대로 JS 켜기·끄기로 적용된다.

## 원격 리소스 차단

일반 WebView의 allow_remote_content=false는 navigation과 페이지 내부 HTTP·HTTPS 요청을 함께 제한한다.
원격 파일 entry 모드는 [별도 응답 경계](#원격-파일-entry-응답-모드)를 사용하며 이 설정으로 하위 파일이나 HTTP(S)를 활성화하지 않는다.
Linux는 `^https?://` WebKit content filter를 홈의 webkit-content-filters 저장소에 비동기 컴파일한다.
완료 callback은 그때의 차단 상태를 다시 읽고, 상태가 바뀌면 기존 필터를 제거한 뒤 필요하면 붙인다.
macOS는 WKContentRuleList, Windows는 WebResourceRequested 거절을 사용한다. data URI는 원격 규칙에 해당하지 않는다.

막을지는 `전역 설정 || 문서 단위 원격 허용` 실효값 하나로 정한다. 문서 단위 값은 스크립트 게이트가 응답 결정에서 정하며, backend의 `remote_block` 모듈이 게이트 전이마다 다시 읽어 적용한다. Windows는 요청마다 게이트를 읽는다. entry mirror는 늘 막고 HTTP view는 endpoint가 정한다. 규칙과 수명은 [html surface의 원격 콘텐츠 차단](../../surfaces/html/index.md#원격-콘텐츠-차단)에 있다.

Linux는 필터 컴파일이 끝나기 전까지 차단하지 못하는 시간이 있으며 실패하면 경고를 남긴다. 첫 로드에서 원격 리소스가 차단되는지는 실제 화면·요청으로 확인한다.
안전 바인딩에 없는 API만 FFI로 호출하고 boxed filter handle은 Drop에서 unref한다.
상류가 해당 API를 제공하면 직접 FFI를 대체한다.

## Linux 클릭 반영 지연 — 첫 클릭은 소비되지 않는다

native WebView는 winit 창의 X 자식이라 포인터는 X 서버가 자식 창에 바로 보낸다. host가 포인터를
넘기는 경로는 없고, GDK가 자기 X 연결로 받은 이벤트를 host가 `pump_gtk_events`로 처리할 때 페이지에
도달한다. 그래서 클릭이 늦게 보이면 원인은 펌프 시점이다.

측정(Xvfb·창 관리자 없음, 2026-10-09). TOC 접기 버튼을 xdotool로 한 번 누르고, 루트 창 픽셀을 10ms마다
읽어 펼침 상태가 바뀐 시각을 잰다. 클릭 전 X focus를 바꿔 가며 조건마다 4회 측정했다.

| 클릭 전 X focus | 반영까지(수정 전) | 반영까지(현재) |
|---|---|---|
| PointerRoot | 0.10~0.39초 | 0.10초 |
| host 창 | 0.10~0.38초 | 0.10초 |
| WebView 자식 | 1.46~1.95초 | 0.10초 |
| 다른 창 | 1.46~1.48초 | 0.46~1.47초 |

- 측정한 모든 클릭(수정 전 48회, 현재 16회)이 페이지에 반영됐다. 소비되어 사라진 클릭은 없었다. 늦게 반영되는
  동안 다시 누르면 두 번째 클릭이 첫 클릭의 결과를 되돌리거나(토글), 열린 팝업의 바깥 클릭이 되어
  "첫 클릭이 먹힌다"처럼 보인다.
- 주소창·본문을 눌러도 X focus는 host 창이나 PointerRoot에 그대로 남았다. WebView 자식이 focus를 쥐는
  상태는 측정에서 `XSetInputFocus`로 만들었다.
- 다른 창이 focus를 가진 줄은 창 관리자가 없는 Xvfb에서만 생긴다. 창 관리자가 있으면 클릭이 창을 활성화해
  Focused(true)가 루프를 깨운다. 실제 데스크톱 세션에서는 측정하지 않았다.
- `debug inject window-mouse`·`egui-mouse`는 host 창의 입력 경로에 들어가므로 WebView 자식에 닿지 않는다
  (`injected:true` 를 돌려주지만 페이지는 바뀌지 않음). Xvfb에서 WebView에 클릭을 넣는 수단은 XTest(xdotool)다.


## 원격 HTTP native backend 가용성

원격 HTTP(S) viewer는 Linux X11·Windows·macOS 27 이상 GUI client에서 사용한다. Wayland 세션의 XWayland도 X11 경로다. 격리 GNOME 세션에서 가용 판정이 참인 것까지 확인했고 XWayland의 실제 attach는 미측정이다. Linux는 native factory가 받는 실제 Xlib 창·display와 X11 GDK 준비를 확인한다. Windows는 WebView2 factory가 있는 GUI 창이면 선언한다. macOS는 실측한 27 이상에서 `WKWebsiteDataStore`의 proxy 설정과 Network proxy 함수가 있을 때 선언한다. 등록한 창에서 판정하여 engine 수명에 보관하고, parked 상태에서는 해제하며 복원 때 다시 판정한다. 서버 HTTP/HTTPS/Reload protocol 지원은 GUI·headless 공통이고 client renderer 가용성과 구별한다. native Wayland와 macOS 11–26 client는 이 capability를 선언하지 않는다. 인증 proxy 앞단·인증서 확인·스크립트 정책은 [Windows HTTP context](../../dev-guide/windows-http-context.md)와 [macOS HTTP context](../../dev-guide/macos-http-context.md)에 있다. 실제 전송과 script/size consent는 기존 수신·배너 widgets를 사용하며 신규 UI나 포커스 이동은 없다. [원격 HTTP viewer](../../surfaces/html/index.md#원격-https-native-viewer)의 설정·trust boundary를 따른다.
