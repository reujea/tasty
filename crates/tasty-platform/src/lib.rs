//! OS 메뉴·창·캡처·권한과 crash/hang 진단. 앱 동작은 호출자가 전달한 콜백으로 처리한다.
//! 윈도잉 모듈은 gui feature에서만 사용하며 crash_report는 CLI·headless에서도 사용한다.
//! debug_os_open은 debug 빌드에서 OS 열기를 기록으로 대신하는 격리 기능이다.

#[cfg(feature = "gui")]
pub mod app_icon;
pub mod crash_report;
/// debug 격리 — OS 열기를 띄우지 않고 기록만 한다(`TASTY_DEBUG_OS_OPEN_LOG`, docs/dev-guide/self-verification.md#os-열기와-지연-주입).
#[cfg(debug_assertions)]
pub mod debug_os_open;
#[cfg(all(target_os = "linux", feature = "gui"))]
pub mod gtk_init;
#[cfg(all(windows, feature = "gui"))]
pub mod jump_list;
#[cfg(all(target_os = "macos", feature = "gui"))]
pub mod macos_delegate;
// macOS 메뉴 key equivalent 변환표. OS 호출이 없는 순수부라 모든 OS 에서 컴파일하고 시험한다.
pub mod menu_key_equivalent;
// 부팅 권한 안내 문단 조립. OS 접근이 없어 gui 없이도 컴파일한다(갤러리가 쓴다).
pub mod macos_permission_notice;
// 순수한 목록 결정 로직이 macOS 밖에서도 유닛테스트되도록 모듈 자체는 gui 빌드 전체에서
// 컴파일한다 — 실제 파일 접근부만 모듈 안에서 macOS 로 좁힌다.
#[cfg(feature = "gui")]
pub mod macos_permissions;
// FDA 보유 기록과 안내 갈래 판정. 판정은 순수부라 macos_permissions 와 같은 범위에서 컴파일한다.
#[cfg(feature = "gui")]
mod macos_fda_history;
#[cfg(feature = "gui")]
pub mod native_menu;
#[cfg(all(target_os = "macos", feature = "gui"))]
pub mod pasteboard;
#[cfg(all(windows, feature = "gui"))]
pub mod power_windows;
#[cfg(feature = "gui")]
pub mod reveal;
#[cfg(feature = "gui")]
pub mod screen_capture;
/// Windows 단일 실행: MainView 창 탐색·등록 활성화 메시지·포그라운드 권한 위임.
pub mod single_instance_windows;
#[cfg(feature = "gui")]
pub mod stall_watchdog;
#[cfg(all(
    any(windows, target_os = "macos", target_os = "linux"),
    feature = "gui"
))]
pub mod system_tray;
/// 사용자가 다시 실행했을 때 OS 활성화 증거로 기존 창의 활성화를 OS에 요청한다.
#[cfg(feature = "gui")]
pub mod window_activation;
#[cfg(feature = "gui")]
pub mod window_chrome;
/// 에이전트가 만든 창을 사용자 창 뒤에, 키 포커스 없이 보인다(docs/features/window-chrome/index.md#에이전트-창의-os-표시).
#[cfg(feature = "gui")]
pub mod window_stacking;
/// X11 XID → `GdkWindow` 변환. GTK 백엔드가 X11 인 Linux 에서만 쓴다.
#[cfg(all(target_os = "linux", feature = "gui"))]
pub mod x11_gdk_window;
