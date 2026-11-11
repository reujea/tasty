# Linux 디스플레이 백엔드 선택

- **Status**: Implemented. 네이티브 Wayland 경로는 `DISPLAY`가 없는 환경에만 쓰이며 아래 한계가 있다.
- **주체**: 로컬 사용자
- **코드**: 판정 `src/boot/display_backend.rs`, 호출 `src/boot/event_loop.rs`, 두 번째 실행이 보내는 값 `src/boot/single_instance/second.rs`, GTK 초기화 결과 `crates/tasty-platform/src/gtk_init.rs`, 안내 타일 `crates/tasty-ui-widgets/src/webview_page_tile.rs`

## 선택 규칙

창과 GDK는 같은 백엔드를 쓴다. 판정은 이벤트 루프를 만들기 전에 끝낸다. winit은 프로세스에서 이벤트 루프를 한 번만 만들 수 있어 X11로 만들다 실패한 뒤 Wayland로 다시 만들 수 없기 때문이다.

| `DISPLAY` | `WAYLAND_DISPLAY` | 판정 | 결과 |
|---|---|---|---|
| 있음 | 있음 | X 서버에 시험 연결(한도 10초) | 성공하면 X11. 실패하면 사유를 경고로 남기고 네이티브 Wayland로 내려간다 |
| 있음 | 없음 | 시험 연결 없음 | X11. 서버에 연결하지 못하면 시작 실패다 |
| 없음 | 있음 | 시험 연결 없음 | 네이티브 Wayland |
| 없음 | 없음 | 고정하지 않음 | winit 오류로 시작 실패다 |

- 시험 연결은 이벤트 루프를 만들 때까지 유지한다. 그 틈에 XWayland가 내려가지 않게 하기 위해서다.
- GDK는 `gdk_set_allowed_backends`로 맞춘다. 환경 변수를 쓰지 않으므로 터미널 자식과 플러그인이 받는 `DISPLAY`·`WAYLAND_DISPLAY`는 세션 값 그대로이고 `GDK_BACKEND`가 추가되지 않는다.
- 사용자가 Tasty 실행 환경에 x11을 허용하지 않는 `GDK_BACKEND`(`wayland`·`broadway` 등)를 직접 설정하면 창은 X11이고 GTK 초기화가 실패한다. 이때 트레이 아이콘과 WebView를 만들지 못하고 html·markdown surface가 원인과 해결책을 안내한다(아래 "표시 경로 안내"). 네이티브 메뉴는 화면 안내 없이 뜨지 않는다(`gtk::init` 실패 처리를 코드로 판단했고 실행은 미측정). Tasty는 이 값을 읽기만 하고 지우거나 덮지 않는다. 지우면 터미널 자식과 플러그인이 사용자가 정한 값과 다른 환경을 받기 때문이다.
- 막힌 TCP 주소(`DISPLAY=10.255.255.1:0` 같은)와 `WAYLAND_DISPLAY`가 함께 있으면 시험 연결이 한도까지 기다린 뒤 내려간다. 그 뒤 첫 프레임이 나오지 않는 현상이 있고, 선택 규칙을 넣기 전 바이너리에서도 같다. 원인은 확인하지 못했다.

## XWayland 경로의 배율

배율은 컴포지터가 X 클라이언트에 알리는 값을 따른다. 3200x2000 격리 GNOME Shell 46에서 잰 값이다. 물리 패널에서 본 값이 아니며 글자 판단은 컴포지터 캡처 기준이다.

| GNOME 설정 | X 화면 | `Xft.dpi` | Tasty 배율 | 창(물리 px) | 글자 |
|---|---|---|---|---|---|
| 기본값, 배율 2 | 3200x2000 | 192 | 2.0 | 2560x1440 | 선명하다 |
| `scale-monitor-framebuffer` 실험 기능, 배율 2 | 1600x1000 | 96 | 1.0 | 1280x720 | 컴포지터가 확대해 흐리다 |
| `scale-monitor-framebuffer` 실험 기능, 배율 1.5 | 2136x1335 | 96 | 1.0 | 1280x720 | 컴포지터가 확대한다 |

GNOME 기본값에서는 정수 배율만 고를 수 있다. 배율 1과 표의 세 경우에 X 입력(`xdotool`, XTEST)을 넣으면 메뉴가 클릭 지점에 뜨고 내부 끌기가 대상 행에 놓인다. WebView는 창 기하와 캡처에서 주소창 아래 표시 영역과 맞는다. XTEST는 X 좌표를 바로 넣으므로 컴포지터 포인터가 X 좌표로 옮겨지는 경로를 재지 않는다. 실제 포인터 입력으로 메뉴를 확인한 것은 배율 1뿐이고, 배율 2와 논리 레이아웃의 실제 포인터 경로는 미측정이다. 실험 기능을 켠 경우 네이티브 Wayland는 배율 2.0을 받아 선명하지만 이 환경에서도 `DISPLAY`가 있으면 X11을 고른다.

## 네이티브 Wayland의 범위

`DISPLAY`가 없는 환경에서 창·터미널·입력·내부 끌기는 동작하고 다음은 동작하지 않는다. 값은 X 서버가 있는 격리 세션에서 Tasty의 `DISPLAY`·`XAUTHORITY`를 빼고 잰 것이다. 이 선택 규칙을 넣은 빌드로 X 서버 자체가 없는 세션(`--no-x11`)은 재지 않았다.

| 기능 | 동작 | 사용자에게 보이는 것 |
|---|---|---|
| 터미널 복사·붙여넣기 | 외부 앱과 주고받지 못한다 | warning 토스트. 원인 1은 `toast.clipboard_wayland_unsupported`, 원인 1b는 `toast.clipboard_wayland_x_failed` |
| Explorer 파일 복사·붙여넣기 | 같은 Tasty 인스턴스 안의 목록에만 남고 그 목록을 붙여넣는다 | warning 토스트 `toast.clipboard_files_wayland_{copy,paste}`, 원인 1b는 `toast.clipboard_files_wayland_x_failed_{copy,paste}` |
| 네이티브 메뉴 | 뜨지 않는다 | 화면 안내 없음. `native context menu: not an X11 window` 경고 로그 |
| html·markdown WebView | 만들지 못한다 | 페이지 영역에 원인과 해결책 안내(`native-wayland`, 원인 1b는 `native-wayland-x`). `Not an X11 window` 경고 로그 |
| 원격 웹주소 | 사용 가능으로 알리지 않는다 | html mirror surface도 같은 안내다. [HTML surface](../../surfaces/html/index.md#원격-https-native-viewer) 참고 |
| 외부 파일 끌어오기 | 받지 못한다 | 화면 안내 없음 |

외부 앱으로 파일 끌어내기는 네이티브 Wayland에서도 파일이 복사된다.

## 표시 경로 안내

WebView를 만들 수 없는 표시 경로면 html·markdown·html mirror surface의 페이지 영역이 그 이유를 안내한다. 안내는 무엇이 꺼졌는지와 원인을 제목에, 해결책을 그 아래 줄에 둔다. 버튼·링크는 없다. 디자인은 tasty-page 정본 `HtmlPageState`의 표시 백엔드 kind이고 문구는 정본 `display_backend_copy.js`의 값이다.

판정은 세 값으로 한다. 시작 때 고른 백엔드(이벤트 루프를 만들 때 프로세스에 한 번 남긴다), 시작 환경의 `GDK_BACKEND`가 x11을 허용하는지, GTK 초기화 결과다. GTK 초기화는 트레이·네이티브 메뉴·WebView·원격 웹주소가 같은 함수로 묻고, 첫 결과를 프로세스 수명 동안 보관한다. 초기화 실패는 대개 시작 환경에서 오지만 기본 main context를 얻지 못한 경우처럼 실행 중 상황에서 오는 실패도 있다. 그래도 다시 시도하지 않는다. 실패를 본 surface는 WebView 생성을 영구 실패로 멈추고 원격 웹주소를 쓸 수 있는지도 창을 만들 때 정하므로, GTK만 나중에 살아나면 안내가 사라진 빈 화면이 남기 때문이다. 첫 결과를 고정해 안내와 surface 상태가 같은 사실을 가리키게 한다.

| kind | 조건 | 제목 / 해결책 키 |
|---|---|---|
| `native-wayland`(원인 1) | `DISPLAY`가 없어 네이티브 Wayland로 시작했다 | `webview.native_wayland` / `webview.native_wayland_sub` |
| `native-wayland-x`(원인 1b) | `DISPLAY`의 X 서버에 연결하지 못해 Wayland로 내려갔다 | `webview.native_wayland_x_failed` / `webview.native_wayland_x_failed_sub` |
| `gdk-backend`(원인 2) | 창은 X11이고 GTK 초기화가 실패했으며 `GDK_BACKEND`가 x11을 허용하지 않는다 | `webview.gdk_backend` / `webview.gdk_backend_sub` |
| `gtk-init` | 창은 X11이고 GTK 초기화가 실패했으며 `GDK_BACKEND`가 없거나 x11을 허용한다 | `webview.gtk_init_failed` / 없음 |

- Wayland 창이면 GTK 결과와 관계없이 원인 1·1b다.
- `GDK_BACKEND`는 GDK와 같게 읽는다. 쉼표로 나눈 이름 가운데 `x11`이나 `*`가 있거나 값이 `help`면 x11을 허용한다. 빈 값은 고를 백엔드가 없어 허용하지 않는다.
- 이 조건이 성립하면 이 프로세스에서는 어느 WebView도 만들어지지 않는다. 그래서 surface마다 실패를 기록하지 않고 프로세스의 판정으로 그린다. 판정이 성립하면 탐색 상태(빈 탭·로딩·실패)와 관계없이 안내다. html mirror는 받기가 끝난 뒤 안내를 보인다. 받는 중에는 받기 상태를 보인다.
- 원인 2와 `gtk-init`의 제목은 트레이 아이콘이 꺼졌다는 사실도 담는다. 트레이에는 따로 안내하지 않는다.
- 블록이 영역보다 크면 위에서 시작해 세로로 스크롤한다. 해결책은 잘리지 않는다. `gtk-init`은 해결책 줄 자리를 비워 둔다.
- 원인 2와 `gtk-init`에서 창은 X11이라 클립보드 안내는 없다(코드 경로 판단).
- 토스트 수명은 다른 토스트와 같은 전역 설정이다.

## 확인한 컴포지터

위 값은 격리 GNOME Shell 46 세션에서만 쟀다. KDE·Sway·Hyprland는 미측정이다.

## 완료 조건

- Given `DISPLAY`와 `WAYLAND_DISPLAY`가 모두 있고 X 서버에 연결된다, When 시작하면, Then 창과 GDK가 X11이다.
- Given `WAYLAND_DISPLAY`와 서버가 없는 `DISPLAY`가 있다, When 시작하면, Then 사유를 경고로 남기고 네이티브 Wayland로 뜬다.
- Given Tasty 터미널 안 셸, Then `DISPLAY`·`WAYLAND_DISPLAY`가 세션 값이고 `GDK_BACKEND`가 추가되지 않았다.
- Given 원인 1 또는 1b, When html·markdown 파일을 연다, Then 페이지 영역에 해당 kind의 제목과 해결책이 말줄임 없이 보인다.
- Given 원인 1 또는 1b, When 터미널이나 Explorer에서 복사·붙여넣기를 한다, Then 원인별 키의 warning 토스트가 보인다.
- Given X11에서 시스템 클립보드 쓰기·읽기가 실패했다, Then Explorer는 `toast.clipboard_files_internal_copy`/`_paste`를 info로 보인다.
- Given `DISPLAY`가 있어 X11로 뜨고 x11을 허용하지 않는 `GDK_BACKEND` 때문에 GTK 초기화가 실패했다, When html·markdown 파일을 연다, Then `gdk-backend` 안내가 보인다.
- Given 원인 2 환경의 Tasty 터미널 안 셸, Then `GDK_BACKEND`가 사용자가 설정한 값 그대로다.
- Given X11 창에서 `GDK_BACKEND`가 없거나 x11을 허용하는데 GTK 초기화가 실패했다, Then `gtk-init` 안내가 보이고 해결책 줄은 비어 있다.

측정 방법은 [GNOME 세션 검증](../../ai-verification/gnome-session-verification.md)에 있다.
