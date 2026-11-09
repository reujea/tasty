# Linux 디스플레이 백엔드 선택

- **Status**: Implemented. 네이티브 Wayland 경로는 `DISPLAY`가 없는 환경에만 쓰이며 아래 한계가 있다.
- **주체**: 로컬 사용자
- **코드**: 판정 `src/boot/display_backend.rs`, 호출 `src/boot/event_loop.rs`, 두 번째 실행이 보내는 값 `src/boot/single_instance/second.rs`

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
- 사용자가 Tasty 실행 환경에 `GDK_BACKEND=wayland`를 직접 설정하면 창은 X11이고 GTK 초기화가 실패한다. 이때 트레이 아이콘과 WebView를 만들지 못한다(실행으로 확인). 네이티브 메뉴도 화면 안내 없이 뜨지 않는다(`gtk::init` 실패 처리를 코드로 판단했고 실행은 미측정).
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
| 터미널 복사·붙여넣기 | 외부 앱과 주고받지 못한다 | 토스트 `toast.clipboard_wayland_unsupported` |
| Explorer 파일 복사 | 같은 Tasty 인스턴스 안의 목록에만 남는다 | 토스트로 알린다 |
| 네이티브 메뉴 | 뜨지 않는다 | 화면 안내 없음. `native context menu: not an X11 window` 경고 로그 |
| html·markdown WebView | 만들지 못한다 | 빈 화면. 화면 안내 없음. `Not an X11 window` 경고 로그 |
| 원격 웹주소 | 사용 가능으로 알리지 않는다 | [HTML surface](../../surfaces/html/index.md#원격-https-native-viewer) 참고 |
| 외부 파일 끌어오기 | 받지 못한다 | 화면 안내 없음 |

외부 앱으로 파일 끌어내기는 네이티브 Wayland에서도 파일이 복사된다.

## 확인한 컴포지터

위 값은 격리 GNOME Shell 46 세션에서만 쟀다. KDE·Sway·Hyprland는 미측정이다.

## 완료 조건

- Given `DISPLAY`와 `WAYLAND_DISPLAY`가 모두 있고 X 서버에 연결된다, When 시작하면, Then 창과 GDK가 X11이다.
- Given `WAYLAND_DISPLAY`와 서버가 없는 `DISPLAY`가 있다, When 시작하면, Then 사유를 경고로 남기고 네이티브 Wayland로 뜬다.
- Given Tasty 터미널 안 셸, Then `DISPLAY`·`WAYLAND_DISPLAY`가 세션 값이고 `GDK_BACKEND`가 추가되지 않았다.

측정 방법은 [GNOME 세션 검증](../../ai-verification/gnome-session-verification.md)에 있다.
