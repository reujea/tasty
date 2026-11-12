# Linux 디스플레이 백엔드 선택

- **Status**: Implemented. 네이티브 Wayland 경로는 `DISPLAY`가 없는 환경에만 쓰이며 아래 한계가 있다.
- **주체**: 로컬 사용자
- **코드**: 판정 `src/boot/display_backend.rs`, 데이터 장치 `src/file/drag/wayland.rs`·`src/file/drag/wayland/`, egui 연결 `src/gfx/gpu/text_selection.rs`, 터미널 연결 `src/clipboard/terminal.rs`·`src/view/main/clipboard.rs`, 호출 `src/boot/event_loop.rs`, 두 번째 실행이 보내는 값 `src/boot/single_instance/second.rs`, GTK 초기화 결과 `crates/tasty-platform/src/gtk_init.rs`, 안내 타일 `crates/tasty-ui-widgets/src/webview_page_tile.rs`

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
| 터미널 primary selection(vi `"*` register) | 외부 앱에 닿지 않는다(`"+`는 닿는다) | warning 토스트. 원인 1은 `toast.clipboard_primary_wayland_unsupported`, 원인 1b는 `toast.clipboard_primary_wayland_x_failed`. 아래 데이터 장치가 없는 창이면 터미널 키가 우선한다 |
| Explorer 파일 복사·붙여넣기 | 같은 Tasty 인스턴스 안의 목록에만 남고 그 목록을 붙여넣는다 | warning 토스트 `toast.clipboard_files_wayland_{copy,paste}`, 원인 1b는 `toast.clipboard_files_wayland_x_failed_{copy,paste}` |
| 네이티브 메뉴 | 뜨지 않는다 | 화면 안내 없음. `native context menu: not an X11 window` 경고 로그 |
| html·markdown WebView | 만들지 못한다 | 페이지 영역에 원인과 해결책 안내(`native-wayland`, 원인 1b는 `native-wayland-x`). `Not an X11 window` 경고 로그 |
| 원격 웹주소 | 사용 가능으로 알리지 않는다 | html mirror surface도 같은 안내다. [HTML surface](../../surfaces/html/index.md#원격-https-native-viewer) 참고 |

외부 앱에서 파일을 끌어 오고 외부 앱으로 끌어내는 것, egui 입력칸(설정·이름 바꾸기·주소창 등)의 텍스트 복사·붙여넣기, 터미널 복사·붙여넣기(텍스트, 붙여넣기의 PNG 이미지)는 네이티브 Wayland에서도 외부 앱과 주고받는다. 아래 데이터 장치가 맡는다.

## 네이티브 Wayland의 데이터 장치

시작 판정이 네이티브 Wayland(원인 1·1b)면 Tasty는 winit과 같은 Wayland 연결에 소유자 하나를 두고, seat마다 `wl_data_device` 하나를 프로세스가 끝날 때까지 둔다. 모든 창이 이 장치를 같이 쓴다. 컴포지터는 끌기와 선택(selection)을 클라이언트의 장치 가운데 하나에 보내므로 한 클라이언트에 장치가 둘이면 끌어 온 파일이 다른 장치로 가서 버려진다. 그래서 장치는 하나만 둔다.

- egui-winit은 smithay-clipboard 없이 빌드한다. smithay-clipboard는 키보드가 생기면 같은 연결에 장치를 하나 더 만든다. egui-winit의 나머지 클립보드인 arboard는 X11만 쓰므로 원인 1·1b에서는 앱 안 문자열로 떨어진다. 그 자리를 이 장치가 채운다.
- **끌어오기**: 다른 클라이언트의 `text/uri-list` offer만 Copy로 받고 목록을 비차단으로 읽는다(30초·1 MiB 상한). enter·motion·leave·drop은 끌기가 들어온 창으로 보낸다. 놓은 자리는 drop 때 고정하고, 받은 목록과 Copy가 모두 확인된 뒤에만 `finish`를 보낸다. 이 프로세스가 끌어내는 동안에는 자기 offer를 받지 않는다. 처리는 [Explorer 드래그 앤 드롭](../../surfaces/explorer/file-operations.md#드래그-앤-드롭)과 X11 `DroppedFile` 경로와 같다.
- **egui 복사**: egui가 내놓은 복사 텍스트(`OutputCommand::CopyText`)를 egui-winit에 넘기기 전에 가져와 data source로 선택을 잡는다. mime은 `text/plain;charset=utf-8`·`UTF8_STRING`·`text/plain`이다. serial은 이 연결이 마지막으로 받은 키보드 입력이나 포인터 버튼의 것이다. 받는 쪽에는 비차단으로 쓰고(30초 상한) 끝나면 닫는다. 다른 앱이 선택을 가져가면 source를 버린다.
- **egui 붙여넣기**: 붙여넣기 키(egui-winit과 같은 판정)가 오면 Tasty가 잡은 선택은 바로, 다른 앱의 선택은 그때 읽기 시작해 다 읽은 뒤 다음 egui 프레임 입력에 붙여넣기로 넣는다. 선택 offer는 최신 하나만 보관하고 미리 읽지 않는다. 포커스를 옮길 수 있는 입력(포인터 버튼 누름, Tab·Shift+Tab, 방향키, Escape)이 들어온 프레임과 그다음 프레임에는 넣지 않는다. egui는 Shift+Tab의 포커스 이동을 다음 프레임 시작에 적용하므로 두 프레임이 지나야 포커스가 정해진다. 그런 입력이 없는 프레임이 이어져 포커스가 정해진 뒤, 요청 때 포커스였던 입력칸이 그때도 포커스면 넣고 아니면 버린다. 포커스 이동 입력이 계속 이어지면 보관하되 도착 뒤 3초가 지나면 버린다. 앱 코드가 프레임 안에서 직접 포커스를 옮기는 경우는 이 대조가 막지 못한다. 새 붙여넣기, 선택 교체, 3초 경과, 16 MiB 초과도 읽기를 버린다.
- **터미널 복사**: 선택·vi register(`"*` 제외)·링크·경로·서피스 ID 복사가 egui 복사와 같은 source·mime·serial로 선택을 잡는다. 창마다 egui 입력칸과 별개의 핸들이 있지만 선택은 하나다. 터미널에서 복사한 것을 입력칸에 붙여넣을 수 있고 반대도 된다. 잡지 못하면(아직 이 연결에 키보드·포인터 입력이 없어 serial이 없는 경우 등) `toast.clipboard_failed` error 토스트다.
- **터미널 붙여넣기**: Tasty가 잡은 선택은 바로 넣는다. 다른 앱의 선택은 텍스트 mime(egui와 같은 순서)을, 텍스트가 없으면 `image/png`를 그때 읽기 시작한다. 읽는 동안 winit 스레드는 기다리지 않는다. 다 읽으면 요청한 창이 그때도 키보드 포커스를 갖고, 요청 때 포커스였던 surface가 그때도 포커스이고 같은 세대일 때만 X11과 같은 경로로 넣는다(텍스트는 bracketed paste 포함, PNG는 RGBA로 풀어 이미지 붙여넣기). 아니면 결과와 오류를 모두 버린다. 3초 상한 초과, pipe 실패, 텍스트 16 MiB·PNG 64 MiB 초과, 읽는 동안 선택 교체, 풀리지 않는 PNG는 `toast.clipboard_failed`를 한 번 보이고 PTY에 아무것도 보내지 않는다. 다른 창이나 입력칸의 더 새 붙여넣기가 오면 읽던 것은 알리지 않고 버린다. 텍스트의 CRLF는 egui와 같이 LF로 바꾼다.
- **Core 쪽 쓰기**: 터미널 프로그램의 OSC 52 쓰기, IPC `clipboard.set_text`, 로컬 스크린샷과 원격 캡처 게시는 이벤트 루프 스레드에서 같은 소유자로 선택을 잡는다. 캡처 게시는 경로 텍스트와 `image/png`를 한 source로 함께 offer한다. 이 소유자는 이벤트 루프 스레드에만 있으므로 원격 캡처의 저장 worker는 게시를 하지 않고 이벤트 루프에 넘긴다(Windows와 같은 경로). worker가 이벤트 루프를 기다리는 일은 없다. serial은 마지막 키보드·포인터 입력의 것이라, 사용자 입력과 무관한 쓰기를 컴포지터가 받아들이는지는 컴포지터마다 다르다.
- **OSC 52 읽기**(`general.allow_clipboard_read`를 켰을 때): Tasty가 잡은 선택은 바로 답한다. 다른 앱의 선택은 텍스트 mime만 터미널 붙여넣기와 같은 상한으로 비차단으로 읽고, 다 읽었을 때 질의한 터미널이 같은 세대이고 설정이 그대로 켜져 있으면 답한다. 상한 초과, 읽기 실패, 다른 붙여넣기나 질의에 밀린 읽기는 응답하지 않는다(경고 로그). 읽는 칸은 하나라 터미널·입력칸 붙여넣기와 OSC 52 읽기 가운데 더 새 요청이 앞의 것을 대신한다.
- 창이 Wayland인데 컴포지터가 `wl_data_device_manager` v3나 seat를 주지 않아 장치가 없으면 터미널 클립보드는 arboard로 떨어진다. 이때는 `"*`를 포함한 모든 터미널 복사·붙여넣기가 원인별 터미널 키(원인 1 `toast.clipboard_wayland_unsupported`, 원인 1b `toast.clipboard_wayland_x_failed`)의 warning 토스트다. 이 판정이 위 표의 `"*` 키보다 먼저다.
- X11·XWayland 창에는 Wayland 연결이 없어 장치·키보드·source를 만들지 않는다. egui·터미널 클립보드는 arboard다.
- 갤러리(`tasty-gallery`)는 이 장치를 붙이지 않는다. 네이티브 Wayland에서 갤러리 입력칸은 외부 앱과 클립보드를 주고받지 않는다(앱 안 복사·붙여넣기만). smithay-clipboard는 갤러리에서도 끈다. 켜 두면 `--workspace` 빌드의 feature 통합으로 본체에도 다시 들어오기 때문이다.
- Explorer 파일 클립보드와 터미널 primary selection은 이 장치를 쓰지 않는다(위 표).

격리 GNOME Shell 46(`--no-x11`)에서 가상 키보드가 있는 채로 시작해 GNOME Files의 파일을 터미널로 10번 끌어 놓아 10번 모두 열렸고, Explorer 칸으로 놓으면 복사됐다. egui 입력칸과 GNOME Text Editor 사이의 텍스트 복사·붙여넣기를 양방향으로 확인했다. 원인 1b(`DISPLAY`의 X 서버 연결 실패)에서도 끌어오기와 양방향 클립보드를 한 번씩 확인했다. 물리 화면과 실제 사용자 세션은 재지 않았다([측정 한계](../../ai-verification/gnome-session-verification.md#정리와-한계)).

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
- Given 원인 1 또는 1b, When 터미널에서 복사하고 다른 앱에 붙여넣거나 다른 앱에서 복사한 텍스트를 터미널에 붙여넣는다, Then 같은 텍스트가 들어가고 클립보드 토스트는 복사 성공 토스트만 보인다.
- Given 원인 1 또는 1b, When 다른 앱이 PNG 이미지만 복사했고 터미널에 붙여넣는다, Then X11과 같은 이미지 붙여넣기 경로로 간다.
- Given 원인 1 또는 1b, When 다른 앱의 선택을 읽는 중에 포커스 surface를 바꾸거나 닫는다, Then 늦게 온 결과는 버려진다.
- Given 원인 1 또는 1b, When 다른 앱의 선택을 읽는 중에 키보드 포커스를 다른 창이나 앱으로 옮긴다, Then 늦게 온 결과와 실패 토스트는 요청한 창에 전달되지 않는다.
- Given 원인 1 또는 1b, When 다른 앱이 선택을 주고 3초 안에 보내지 않는다, Then `toast.clipboard_failed`가 한 번 보이고 PTY에는 아무 바이트도 가지 않는다.
- Given 원인 1 또는 1b이고 데이터 장치가 있다, When vi `"*` register로 복사한다, Then 원인별 `toast.clipboard_primary_wayland_*` warning 토스트가 보인다.
- Given 원인 1 또는 1b이고 데이터 장치가 없다, When 터미널에서 복사·붙여넣기를 한다(`"*` 포함), Then 원인별 `toast.clipboard_wayland_*` warning 토스트가 보인다.
- Given 원인 1 또는 1b, When Explorer에서 파일을 복사·붙여넣는다, Then 원인별 `toast.clipboard_files_wayland_*` warning 토스트가 보인다.
- Given X11에서 시스템 클립보드 쓰기·읽기가 실패했다, Then Explorer는 `toast.clipboard_files_internal_copy`/`_paste`를 info로 보인다.
- Given `DISPLAY`가 있어 X11로 뜨고 x11을 허용하지 않는 `GDK_BACKEND` 때문에 GTK 초기화가 실패했다, When html·markdown 파일을 연다, Then `gdk-backend` 안내가 보인다.
- Given 원인 2 환경의 Tasty 터미널 안 셸, Then `GDK_BACKEND`가 사용자가 설정한 값 그대로다.
- Given X11 창에서 `GDK_BACKEND`가 없거나 x11을 허용하는데 GTK 초기화가 실패했다, Then `gtk-init` 안내가 보이고 해결책 줄은 비어 있다.

측정 방법은 [GNOME 세션 검증](../../ai-verification/gnome-session-verification.md)에 있다.
