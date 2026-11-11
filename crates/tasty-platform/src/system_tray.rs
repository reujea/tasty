//! Cross-platform system tray / status item integration (best-effort).
//!
//! Creates a tray icon with menu items: Show Window, New Window, Quit. Menu
//! labels and the tooltip are user-facing strings and come from `t("tray.*")`
//! (`docs/dev-guide/i18n.md`) — the tray is a user surface on all three OSes.
//! Backed by the `tray-icon` crate:
//! - Windows: notification area (`Shell_NotifyIcon`)
//! - macOS: menu bar status item (`NSStatusItem`)
//! - Linux: StatusNotifierItem / AppIndicator (via GTK)
//!
//! Per docs/design/policies/system-tray.md#백그라운드-진입--복귀-os별 this is best-effort: when the platform has no usable tray
//! (minimal WM, missing AppIndicator host, etc.), [`create_tray_icon`] returns
//! `None` and the caller falls back to taskbar/dock minimize. Creation never
//! aborts the app.
//!
//! Threading constraints (from the `tray-icon` crate docs):
//! - macOS: the tray must be created on the main thread while the event loop is
//!   already running (earliest at `StartCause::Init`). Our creation site runs in
//!   the winit main-thread window setup, after the loop is up.
//! - Linux: a GTK event loop must be running on the creating thread, and the
//!   tray must be created on that thread. tasty pumps GTK iterations from the
//!   main loop (see the caller's polling site) rather than running a dedicated
//!   GTK main loop.

#![cfg(all(
    any(windows, target_os = "macos", target_os = "linux"),
    feature = "gui"
))]

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use tasty_i18n::t;

/// Menu item IDs for tray context menu.
pub struct TrayMenuIds {
    pub show_window: String,
    pub new_window: String,
    pub quit: String,
}

/// Creates the system tray icon with menu. Returns the TrayIcon (must be kept alive)
/// and the menu item IDs for event handling.
///
/// Returns `None` (graceful degradation) when the platform tray is unavailable —
/// the caller falls back to taskbar/dock minimize.
pub fn create_tray_icon() -> Option<(TrayIcon, TrayMenuIds)> {
    match create_tray_icon_inner() {
        Ok(result) => Some(result),
        Err(e) => {
            tracing::warn!("Failed to create tray icon: {e}");
            None
        }
    }
}

/// Append `item` to `menu`, logging (not propagating) any failure — a single
/// failed menu item shouldn't abort the whole tray creation. `label` is the
/// diagnostic name for the log line only (not a user-facing string).
fn append_or_warn(menu: &Menu, item: &MenuItem, label: &str) {
    if let Err(e) = menu.append(item) {
        tracing::warn!("Failed to append {label} menu item: {e}");
    }
}

/// Builds the tray context menu (Show Window / New Window / Quit) and the
/// item-id table the caller needs for event dispatch.
fn build_tray_menu() -> (Menu, TrayMenuIds) {
    let show_item = MenuItem::new(t("tray.show_window"), true, None);
    let new_window_item = MenuItem::new(t("tray.new_window"), true, None);
    let quit_item = MenuItem::new(t("tray.quit"), true, None);

    let ids = TrayMenuIds {
        show_window: show_item.id().0.clone(),
        new_window: new_window_item.id().0.clone(),
        quit: quit_item.id().0.clone(),
    };

    let menu = Menu::new();
    append_or_warn(&menu, &show_item, "Show Window");
    append_or_warn(&menu, &new_window_item, "New Window");
    append_or_warn(&menu, &quit_item, "Quit");

    (menu, ids)
}

fn create_tray_icon_inner() -> Result<(TrayIcon, TrayMenuIds), Box<dyn std::error::Error>> {
    // Linux: `tray-icon` (AppIndicator) assumes GTK is already initialized on the
    // calling thread and does not init it itself. Initialize lazily; if GTK is
    // unavailable (no display / no GTK), bail so the caller degrades gracefully.
    #[cfg(target_os = "linux")]
    crate::gtk_init::ensure().map_err(|e| format!("GTK init failed: {e}"))?;

    let icon =
        crate::app_icon::tray_icon().ok_or("failed to decode tray icon from embedded PNG")?;

    let (menu, ids) = build_tray_menu();

    // `with_icon_as_template(true)` is a no-op off macOS; on macOS it lets the
    // menu bar tint the icon for light/dark mode instead of showing raw colors.
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(t("tray.tooltip"))
        .with_icon(icon)
        .with_icon_as_template(true)
        .build()?;

    tracing::info!("System tray icon created");
    Ok((tray_icon, ids))
}

/// Poll for tray menu events. Returns the menu item ID if an event was received.
pub fn poll_menu_event() -> Option<String> {
    MenuEvent::receiver().try_recv().ok().map(|e| e.id.0)
}

/// GTK를 소유한 스레드에서 대기 중 이벤트를 처리한다. tray가 있는 동안 호스트 루프가 호출한다.
/// 새 이벤트를 기다리지는 않지만 개별 GTK 콜백의 실행 시간은 제한하지 않는다.
#[cfg(target_os = "linux")]
pub fn pump_gtk_events() {
    if gtk::is_initialized() {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
}
