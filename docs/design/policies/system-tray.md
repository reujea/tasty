# 시스템 트레이 정책 (운영 상세)

GUI 앱이 백그라운드로 들어가면 가능한 환경에서 트레이나 상태 영역을 사용한다. 트레이를 만들 수 없으면 앱을 중단하지 않고 태스크바·도크를 통해 돌아올 수 있게 한다. 트레이를 만들었지만 아이콘을 표시할 곳이 없는 환경은 별도 경우다([트레이가 없는 환경](#graceful-degradation)). 창과 앱의 수명은 구분한다. 트레이는 창을 닫은 뒤에도 세션과 앱에 접근할 수 있게 하며 종료 처리는 [종료 순서](../../architecture/shutdown-sequence.md)를 따른다.

구현은 `tray-icon` 0.22와 OS별 분기를 사용한다. 아래 표는 구현과 플랫폼 요구 조건을 설명한다. 메뉴가 실제로 표시되고 클릭되는지는 자동 테스트로 확인하지 않으므로 각 OS의 데스크톱에서 마지막 창 닫기와 트레이 복귀를 직접 확인해야 한다.

## 트레이 생성

- 부팅을 시작할 때(`begin_boot` 끝. 셸 설정 화면을 거치면 셸을 확인한 뒤) `tray_icon.is_none()`이면 생성을 시도하고, 만든 트레이는 앱 종료까지 유지한다. 같은 경로에 다시 들어와도 이미 있으면 만들지 않는다. macOS에서 창을 파기·재생성할 때도 트레이는 재사용한다. Windows 점프 목록도 같은 자리에서 등록한다.
- 생성 실패 시 `create_tray_icon()`은 경고를 기록하고 `None`을 반환한다. 이후 태스크바·도크 복귀 경로를 사용한다. `tray_icon`이 `None`이면 생성 실패와 생성 미시도를 모두 뜻한다.
- 메뉴는 **Show Window · New Window · Quit**이다. 라벨과 툴팁은 `t("tray.*")`의 `show_window`·`new_window`·`quit`·`tooltip`을 사용해 `general.language`를 따른다([국제화](../../dev-guide/i18n.md)).
- 아이콘은 포함된 PNG(`app_icon::tray_icon()`)를 사용한다. macOS의 `with_icon_as_template(true)`는 메뉴 바의 밝고 어두운 테마에 맞춰 OS가 색을 적용하게 한다. 다른 OS에서는 이 옵션이 동작하지 않는다.

## 백그라운드 진입 / 복귀 (OS별)

| OS | 백그라운드 진입 | 복귀("Show Window") |
|----|----------------|---------------------|
| Windows | 트레이가 있으면 `set_visible(false)`로 창을 숨기고, 없으면 `set_minimized(true)`로 최소화 | `TrayShowWindow` → `set_visible(true)` + `set_minimized(false)` |
| Linux | Windows와 같은 호출. X11에서는 `set_visible(false)`가 창을 unmap한다. 네이티브 Wayland에서는 winit의 `set_visible`이 동작하지 않아 트레이가 있으면 창이 그대로 남는다. 트레이가 없을 때의 `set_minimized(true)`는 컴포지터에 최소화를 요청한다 | Windows와 같은 호출. 네이티브 Wayland에서는 `set_visible(true)`와 `set_minimized(false)` 모두 효과가 없다 |
| macOS | 창을 파기하고 state를 parked 상태로 보관. 트레이 유무와 무관 | 살아 있는 main view가 있으면 `focused_view_id`가 가리키는 창, 없으면 첫 main view를 복원·포커스. 모든 창이 parked 상태일 때만 `CreateWindow`로 복원. dock reopen과 같은 경로 |

**New Window**는 `CreateWindow`, **Quit**은 `Shutdown`을 보낸다. 두 동작의 메뉴 분기는 세 OS가 공통으로 사용한다(`src/app/event_handler.rs`). 메뉴 클릭은 이벤트 루프마다 `poll_menu_event()`로 확인하며, 내부에서는 `MenuEvent::receiver().try_recv()`를 사용한다.

<a id="graceful-degradation"></a>

## 트레이가 없는 환경

트레이를 쓸 수 없는 경우는 두 가지다. 어느 쪽도 사용자 오류창을 띄우지 않는다.

1. **트레이 생성 실패**: GTK 초기화 실패, Linux의 AppIndicator 라이브러리 부재, 아이콘·빌더 오류 등으로 `create_tray_icon()`이 경고를 남기고 `None`을 반환한다. Windows·Linux의 백그라운드 진입은 창 최소화(`set_minimized(true)`)이고 macOS는 기존 dock 복귀 경로를 유지한다.
2. **Linux에서 트레이는 만들었지만 표시할 곳이 없음**: AppIndicator 라이브러리가 열리고 나머지 생성 단계도 성공하면 `tray_icon`은 `Some`이다. 이후 단계는 조건이 따로 있다.
   - SNI 등록: 세션 버스에 `org.kde.StatusNotifierWatcher`가 있고 `RegisterStatusNotifierItem` 요청이 성공해야 한다. watcher가 없거나 등록 요청이 오류를 반환하면 Ayatana AppIndicator는 GtkStatusIcon 대체 경로를 쓴다.
   - 아이콘 표시: 등록을 받아 그리는 SNI host, 또는 대체 경로가 쓸 XEmbed 트레이가 있어야 한다.
   - 둘 다 없으면 아이콘은 보이지 않지만 백그라운드 진입은 트레이가 있는 경우와 같은 `set_visible(false)` 호출이다. X11(XWayland 포함)에서는 창이 숨겨지고, 네이티브 Wayland에서는 이 호출이 효과가 없어 창이 그대로 남는다([OS별 표](#백그라운드-진입--복귀-os별)). X11·XWayland에서 숨긴 창을 다시 보이게 하는 방법은 [같은 홈으로 다시 실행했을 때](focus.md#같은-홈으로-다시-실행했을-때-release-단일-실행)의 활성화 증거가 있는 실행이다. 증거 없는 터미널 실행은 새 창 하나를 연다.
   - 실측 범위: Fedora 45 X11(Xvfb, 창 관리자 없음) 세션에서 release rpm으로 watcher가 없을 때와 모의 watcher가 있을 때 모두 트레이 생성 로그, 앱 수준 최소화 뒤 창 unmap, 활성화 증거를 붙인 두 번째 실행 뒤 같은 창의 복원을 확인했다. 모의 watcher에는 `RegisterStatusNotifierItem`이 들어왔다. AppIndicator 라이브러리를 가린 세션에서는 패닉 없이 창이 남고 dlopen 실패 경고 뒤 최소화 경로를 탔다. GNOME·KDE 실제 데스크톱에서 아이콘이 표시되는지는 측정하지 않았다.

## Linux 특이사항 (GTK)

`tray-icon`의 Linux 백엔드(StatusNotifierItem/AppIndicator)는 GTK 초기화와 같은 스레드의 이벤트 처리가 필요하다. Tasty는 별도 GTK 메인 루프를 만들지 않는다.

- 트레이 생성 직전에 `gtk_init::ensure()`를 호출한다. 이 함수는 첫 GTK 초기화 결과를 프로세스 수명 동안 보관한다. 실패하면 `None`을 반환하고 트레이 생성 실패의 동작을 따른다.
- 이어서 AppIndicator 라이브러리를 `libappindicator-sys`와 같은 이름 순서(`libayatana-appindicator3.so.1`, `libappindicator3.so.1`, `libayatana-appindicator3.so`, `libappindicator3.so`)로 `dlopen`해 본다. 하나도 열리지 않으면 `None`을 반환한다. `libappindicator-sys`는 라이브러리를 열지 못하면 패닉하므로 트레이를 만들기 전에 확인한다. 성공한 확인은 프로세스 수명 동안 보관한다. 라이브러리는 열리지만 필요한 심볼이 없는 경우는 확인하지 않으며 그때는 `libappindicator-sys`가 패닉한다.
- winit의 `about_to_wait`에서 `pump_gtk_events()`가 비차단 `gtk::main_iteration_do(false)`를 호출한다. 처리할 이벤트가 없으면 즉시 반환한다.
- 빌드에는 GTK 3 개발 패키지(`libgtk-3-dev` 등)가 필요하다. 런타임에는 `libgtk-3`이 필요하다. 트레이에는 AppIndicator 라이브러리(`libayatana-appindicator3` 또는 `libappindicator3`)가 필요하고, 없으면 트레이 없이 동작한다. AppIndicator는 링크하지 않고 실행 중에 연다. 그래서 자동 의존성에 나오지 않고, rpm은 `libayatana-appindicator3.so.1()(64bit)` capability를 Recommends로 둔다(Fedora에서는 `libayatana-appindicator-gtk3`가 제공한다).
- `tray-icon`의 기본 기능 `libxdo`는 끈다(본체와 `tasty-platform` 두 선언 모두). 그 기능은 Tasty가 쓰지 않는 muda 기본 메뉴 항목(복사·붙여넣기 등)의 키 입력에만 쓰이고, 켜면 배포본이 빌드 머신의 `libxdo` soname에 묶인다.

### DE 가용성 (best-effort 범위)

| 데스크톱 환경 | 트레이 지원 조건 |
|--------------|--------|
| KDE Plasma | 네이티브 SNI 사용 |
| GNOME / Ubuntu | AppIndicator 확장 필요 |
| XFCE / Cinnamon / MATE | 환경의 트레이 지원 사용 |
| 미니멀 WM / 트레이 없는 환경 | 생성 실패면 최소화. 트레이만 만들어졌으면 아이콘 없이 `set_visible(false)`를 부르며, X11·XWayland에서는 창이 숨겨지고 네이티브 Wayland에서는 창이 남는다([트레이가 없는 환경](#graceful-degradation)) |

## 스레드 제약

- **macOS**: 이벤트 루프가 시작된 뒤 메인 스레드에서 생성한다. Tasty는 부팅을 시작할 때 winit 메인 스레드에서 생성한다.
- **Linux**: GTK를 소유한 winit 메인 스레드에서 생성하고 이벤트도 처리한다.

## 코드 위치

- `crates/tasty-platform/src/system_tray.rs`: `create_tray_icon()`, `poll_menu_event()`, Linux의 `pump_gtk_events()`, `TrayMenuIds`. GUI 기능이 켜진 Windows·macOS·Linux에서 컴파일한다.
- `src/app/event_handler.rs`: 생성, OS별 백그라운드 진입과 복귀, 메뉴·GTK 이벤트 처리.
- `src/app/event.rs`: `TrayShowWindow` 이벤트.
- `crates/tasty-platform/src/app_icon.rs`: 트레이 아이콘.
