//! tasty 의 **OS 경계** — crash/hang 리포트, 네이티브 컨텍스트 메뉴, 트레이, Windows
//! jump list, CSD 윈도우 크롬, 화면 캡처, macOS 권한, 이벤트 루프 stall 워치독.
//!
//! 이 크레이트는 **OS 를 부르는 코드만** 담는다. 그 신호가 App 에서 무엇이 되는지는
//! 부르는 쪽이 정한다 — 그래서 여기에는 `AppEvent` 도 `AppState` 도 없고, 콜백
//! (`power_windows::OnResume` · `macos_delegate::DelegateActions`)을 받아 부를 뿐이다.
//! 그 경계의 근거는 `docs/adr/0331-the-platform-folder-holds-the-os-call-not-the-app-meaning.md`,
//! 폴더가 크레이트가 된 근거는 `docs/adr/0343-the-os-boundary-is-a-crate.md`.
//!
//! **본체와 잇는 이름은 그대로다** — 본 바이너리가 `use tasty_platform as platform;` 로
//! 별칭을 걸어서, 호출부의 `crate::platform::…` 는 한 줄도 안 바뀌었다.
//!
//! `gui` feature 는 본체의 같은 이름 feature 가 켠다. 그 뒤에 있는 것이 윈도잉·GTK·
//! AppKit·트레이를 드는 모듈 전부이고, headless 빌드가 쓰는 것은 [`crash_report`] 뿐이다.

#[cfg(feature = "gui")]
pub mod app_icon;
pub mod crash_report;
/// debug 격리 — OS 열기를 띄우지 않고 기록만 한다(`TASTY_DEBUG_OS_OPEN_LOG`, ADR-0511).
#[cfg(debug_assertions)]
pub mod debug_os_open;
#[cfg(all(windows, feature = "gui"))]
pub mod jump_list;
#[cfg(all(target_os = "macos", feature = "gui"))]
pub mod macos_delegate;
// 순수한 목록 결정 로직이 macOS 밖에서도 유닛테스트되도록 모듈 자체는 gui 빌드 전체에서
// 컴파일한다 — 실제 파일 접근부만 모듈 안에서 macOS 로 좁힌다.
#[cfg(feature = "gui")]
pub mod macos_permissions;
#[cfg(feature = "gui")]
pub mod native_menu;
#[cfg(all(windows, feature = "gui"))]
pub mod power_windows;
#[cfg(feature = "gui")]
pub mod reveal;
#[cfg(feature = "gui")]
pub mod screen_capture;
#[cfg(feature = "gui")]
pub mod stall_watchdog;
#[cfg(all(
    any(windows, target_os = "macos", target_os = "linux"),
    feature = "gui"
))]
pub mod system_tray;
#[cfg(feature = "gui")]
pub mod window_chrome;
/// 에이전트가 만든 창을 사용자 창 뒤에, 키 포커스 없이 보인다(ADR-0497).
#[cfg(feature = "gui")]
pub mod window_stacking;
/// X11 XID → `GdkWindow` 변환. GTK 백엔드가 X11 인 Linux 에서만 쓴다.
#[cfg(all(target_os = "linux", feature = "gui"))]
pub mod x11_gdk_window;
