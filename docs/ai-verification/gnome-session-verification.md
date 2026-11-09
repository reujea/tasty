# GNOME 세션 검증

X 서버와 Wayland가 함께 있는 일반 GNOME 경로를 사용자 세션을 건드리지 않고 재는 방법이다. 선택 규칙은 [Linux 디스플레이 백엔드](../design/systems/linux-display-backend.md)에 있다.

## 격리 세션

전용 `XDG_RUNTIME_DIR`와 `HOME`을 만들고 `dbus-run-session` 안에서 다음을 실행한다.

```bash
gnome-shell --headless --wayland --wayland-display <이름> --mode=gnome --virtual-monitor 1920x1200
```

XWayland까지 쓰려면 두 가지가 필요하다.

- **systemd 대역**: gnome-shell은 X 클라이언트가 처음 연결될 때 `org.freedesktop.systemd1.Manager.StartUnit`으로 `gnome-session-x11-services-ready.target`을 시작한다. 격리 버스에 systemd가 없으면 `Error starting X11 services`로 실패하고 X 클라이언트가 멈춘다. 격리 버스에 `org.freedesktop.systemd1` 이름을 잡고 `StartUnit`에 성공으로 답하는 작은 스크립트를 둔다.
- **`XWAYLAND_NO_GLAMOR=1`**: NVIDIA EGLStream 경로의 Xwayland에서 X11 창이 뜰 때 gnome-shell이 죽는 환경이 있다. 일반 GTK X11 앱에서도 재현되면 측정 환경 문제이므로 이 값으로 피한다. 이 값을 쓴 측정은 조건에 적는다.

네이티브 Wayland 경로는 Tasty 실행 환경에서 `DISPLAY`와 `XAUTHORITY`를 빼서 잰다. winit은 `WINIT_UNIX_BACKEND`를 읽지 않는다. X 서버 자체가 없는 세션은 gnome-shell에 `--no-x11`을 준다.

## 입력과 관찰

- **입력**: `org.gnome.Mutter.RemoteDesktop` 세션 하나를 유지한다. 호출마다 세션을 새로 만들면 X 포인터 장치가 floating이 되어 클릭이 X 창에 가지 않는다. `ApplyMonitorsConfig` 뒤에는 입력 세션을 다시 만든다. 그래도 좌표가 맞지 않으면 X 창에 한해 `xdotool`을 쓰고 입력 수단을 결과에 적는다.
- **창 종류**: `org.gnome.Shell.Eval`에서 `meta_window.get_client_type()`을 읽는다. 0이 Wayland, 1이 X11이다.
- **화면**: `org.gnome.Shell.Screenshot`으로 컴포지터 합성 결과를 찍는다. native WebView와 메뉴 창이 함께 나온다.
- **배율**: `org.gnome.Mutter.DisplayConfig.ApplyMonitorsConfig`로 바꾸고 `GetCurrentState`로 적용 값을 확인한다. 비정수 배율은 `experimental-features`에 `scale-monitor-framebuffer`를 넣은 뒤 나온다. 이 값은 세션 시작 전에 설정한다. 적용이 실패하면 이전 배율로 잰 값이 되므로 Tasty의 `scale_factor`와 함께 확인한다.
- **외부 앱**: 파일 왕복은 실제 GNOME Files로, 텍스트 왕복은 `GDK_BACKEND`로 고른 작은 GTK 앱으로 잰다.
- **한글**: `ibus-daemon`과 `XMODIFIERS=@im=ibus`로 실제 조합을 만든다. 조합이 일어났는지 화면으로 먼저 확인한다. 조합이 없으면 입력 키가 그대로 들어가 Delete 같은 키가 실제 파일을 지울 수 있으므로 버려도 되는 fixture로 잰다.

## 끌기 측정

mutter는 `XdndSelection` 소유자 변경을 볼 때 포인터가 X 표면 위에 있어야 XWayland와 Wayland 사이의 끌기를 시작한다. X 끌기 소스는 포인터가 창을 벗어나기 전에 선택을 소유해야 한다. 끌기 소스를 바꿨다면 대상이 Wayland 앱인 경우와 X11 앱인 경우를 모두 잰다.

## 정리와 한계

프로세스는 저장한 PID와 `/proc/<pid>/environ`의 격리 `XDG_RUNTIME_DIR`로 소유를 확인한 뒤 종료한다. 이 방법은 물리 화면을 재지 않는다. 글자 선명도는 컴포지터 캡처 기준이고, 실제 사용자 세션의 systemd 활성화와 GPU 경로는 대신하지 못한다.
