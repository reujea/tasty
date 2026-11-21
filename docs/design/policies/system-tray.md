# 시스템 트레이 정책 (운영 상세)

GUI 앱이 백그라운드로 들어가면 가능한 환경에서 트레이나 상태 영역을 사용한다. 트레이를 등록할 수 없으면 앱을 중단하지 않고 태스크바·도크를 통해 돌아올 수 있게 한다. 창과 앱의 수명은 구분한다. 트레이는 창을 닫은 뒤에도 세션과 앱에 접근할 수 있게 하며 종료 처리는 [종료 순서](../../architecture/shutdown-sequence.md)를 따른다.

구현은 `tray-icon` 0.22와 OS별 분기를 사용한다. 아래 표는 구현과 플랫폼 요구 조건을 설명한다. 메뉴가 실제로 표시되고 클릭되는지는 자동 테스트로 확인하지 않으므로 각 OS의 데스크톱에서 마지막 창 닫기와 트레이 복귀를 직접 확인해야 한다.

## 트레이 생성

- `tray_icon.is_none()`일 때 한 번 생성하고 앱 종료까지 유지한다. macOS에서 창을 파기·재생성할 때도 트레이는 재사용한다.
- 생성 실패 시 `create_tray_icon()`은 경고를 기록하고 `None`을 반환한다. 이후 태스크바·도크 복귀 경로를 사용한다.
- 메뉴는 **Show Window · New Window · Quit**이다. 라벨과 툴팁은 `t("tray.*")`의 `show_window`·`new_window`·`quit`·`tooltip`을 사용해 `general.language`를 따른다([국제화](../../dev-guide/i18n.md)).
- 아이콘은 포함된 PNG(`app_icon::tray_icon()`)를 사용한다. macOS의 `with_icon_as_template(true)`는 메뉴 바의 밝고 어두운 테마에 맞춰 OS가 색을 적용하게 한다. 다른 OS에서는 이 옵션이 동작하지 않는다.

## 백그라운드 진입 / 복귀 (OS별)

| OS | 백그라운드 진입 | 복귀("Show Window") |
|----|----------------|---------------------|
| Windows | 트레이가 있으면 `set_visible(false)`로 창을 숨기고, 없으면 `set_minimized(true)`로 최소화 | `TrayShowWindow` → `set_visible(true)` + `set_minimized(false)` |
| Linux | Windows와 동일 | Windows와 동일 |
| macOS | 창을 파기하고 state를 parked 상태로 보관. 트레이 유무와 무관 | 살아 있는 main view가 있으면 `focused_view_id`가 가리키는 창, 없으면 첫 main view를 복원·포커스. 모든 창이 parked 상태일 때만 `CreateWindow`로 복원. dock reopen과 같은 경로 |

**New Window**는 `CreateWindow`, **Quit**은 `Shutdown`을 보낸다. 두 동작의 메뉴 분기는 세 OS가 공통으로 사용한다(`src/app/event_handler.rs`). 메뉴 클릭은 이벤트 루프마다 `poll_menu_event()`로 확인하며, 내부에서는 `MenuEvent::receiver().try_recv()`를 사용한다.

<a id="graceful-degradation"></a>

## 트레이가 없는 환경

미니멀 WM, AppIndicator 호스트 부재, 디스플레이 부재 등으로 트레이를 등록하지 못하면 사용자 오류창을 띄우지 않는다. Windows·Linux는 창을 최소화하고 macOS는 기존 dock 복귀 경로를 유지한다.

## Linux 특이사항 (GTK)

`tray-icon`의 Linux 백엔드(StatusNotifierItem/AppIndicator)는 GTK 초기화와 같은 스레드의 이벤트 처리가 필요하다. Tasty는 별도 GTK 메인 루프를 만들지 않는다.

- 트레이 생성 직전에 `gtk::init()`을 호출한다. 실패하면 `None`을 반환하고 트레이 없는 환경의 동작을 따른다.
- winit의 `about_to_wait`에서 `pump_gtk_events()`가 비차단 `gtk::main_iteration_do(false)`를 호출한다. 처리할 이벤트가 없으면 즉시 반환한다.
- 빌드에는 GTK 3 개발 패키지(`libgtk-3-dev` 등)가 필요하다. 런타임에는 `libgtk-3`과 AppIndicator 라이브러리(`libayatana-appindicator3` 또는 `libappindicator3`)가 필요하다. AppIndicator는 링크하지 않고 실행 중에 연다. 그래서 자동 의존성에 나오지 않고, rpm은 `libayatana-appindicator3.so.1()(64bit)` capability를 Recommends로 둔다(Fedora에서는 `libayatana-appindicator-gtk3`가 제공한다). 라이브러리가 없으면 tray 생성 단계에서 libappindicator-sys가 패닉해 앱이 시작 직후 끝난다.
- `tray-icon`의 기본 기능 `libxdo`는 끈다(본체와 `tasty-platform` 두 선언 모두). 그 기능은 Tasty가 쓰지 않는 muda 기본 메뉴 항목(복사·붙여넣기 등)의 키 입력에만 쓰이고, 켜면 배포본이 빌드 머신의 `libxdo` soname에 묶인다.

### DE 가용성 (best-effort 범위)

| 데스크톱 환경 | 트레이 지원 조건 |
|--------------|--------|
| KDE Plasma | 네이티브 SNI 사용 |
| GNOME / Ubuntu | AppIndicator 확장 필요 |
| XFCE / Cinnamon / MATE | 환경의 트레이 지원 사용 |
| 미니멀 WM / 트레이 없는 환경 | 등록하지 않고 대체 복귀 경로 사용 |

## 스레드 제약

- **macOS**: 이벤트 루프가 시작된 뒤 메인 스레드에서 생성한다. Tasty는 winit 메인 스레드의 창 설정 단계에서 생성한다.
- **Linux**: GTK를 소유한 winit 메인 스레드에서 생성하고 이벤트도 처리한다.

## 코드 위치

- `crates/tasty-platform/src/system_tray.rs`: `create_tray_icon()`, `poll_menu_event()`, Linux의 `pump_gtk_events()`, `TrayMenuIds`. GUI 기능이 켜진 Windows·macOS·Linux에서 컴파일한다.
- `src/app/event_handler.rs`: 생성, OS별 백그라운드 진입과 복귀, 메뉴·GTK 이벤트 처리.
- `src/app/event.rs`: `TrayShowWindow` 이벤트.
- `crates/tasty-platform/src/app_icon.rs`: 트레이 아이콘.
