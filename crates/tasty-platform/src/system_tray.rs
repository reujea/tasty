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
//! Per docs/design/policies/system-tray.md#graceful-degradation this is best-effort: when the tray
//! cannot be created (GTK init failure, no AppIndicator library on Linux, a
//! builder error), [`create_tray_icon`] returns `None` and the caller falls back
//! to the per-platform minimize in the policy's OS table; such failures never abort the app. On Linux the
//! AppIndicator library is probed with `dlopen` before building, because
//! `libappindicator-sys` panics when it cannot load it; the first library that
//! loads is also checked for the functions that crate calls, which panics too
//! when one is missing.
//!
//! A missing tray host is a different case: with the library present but no
//! StatusNotifierWatcher (or a failed registration), creation still succeeds and
//! the icon may simply not be shown anywhere.
//!
//! Threading constraints (from the `tray-icon` crate docs):
//! - macOS: the tray must be created on the main thread while the event loop is
//!   already running (earliest at `StartCause::Init`). Our creation site runs on
//!   the winit main thread when boot starts, after the loop is up.
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
/// Returns `None` (graceful degradation) when the tray cannot be created —
/// the caller falls back to the per-platform minimize in the policy's OS table
/// (docs/design/policies/system-tray.md). A created tray does not
/// guarantee that a host shows the icon.
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

    // Linux: building the tray makes libappindicator-sys load AppIndicator and panic
    // if it cannot, or if the library it loaded lacks a function it calls. Check
    // both first so either case takes the same `None` path.
    #[cfg(target_os = "linux")]
    appindicator_library().map_err(|failure| match failure {
        ProbeFailure::NotLoadable => format!(
            "dlopen failed for every AppIndicator library name ({})",
            APPINDICATOR_LIBRARIES.join(", ")
        ),
        ProbeFailure::MissingSymbols { library, missing } => format!(
            "AppIndicator library {library} has missing or null required symbols ({})",
            missing.join(", ")
        ),
    })?;

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

/// The names `libappindicator-sys` 0.9 tries, in its order. The last two are
/// tried only with its `backcompat` feature, which `tray-icon` 0.22 enables
/// through `libappindicator`'s default features. The tests pin the upstream
/// versions this list was checked against, so an upgrade fails until it is
/// checked again.
#[cfg(target_os = "linux")]
const APPINDICATOR_LIBRARIES: [&str; 4] = [
    "libayatana-appindicator3.so.1",
    "libappindicator3.so.1",
    "libayatana-appindicator3.so",
    "libappindicator3.so",
];

/// The functions the `tray-icon` 0.22 GTK backend calls through
/// `libappindicator` 0.9 for `TrayIcon` creation, icon, menu, title, visibility
/// and drop. `libappindicator-sys` looks each one up when it is called and panics
/// if the lookup fails. Tasty sets no title today, so `app_indicator_set_label`
/// is not called yet; it is required anyway so that setting one later cannot
/// reopen the panic path.
#[cfg(target_os = "linux")]
const APPINDICATOR_REQUIRED_SYMBOLS: [&str; 6] = [
    "app_indicator_new",
    "app_indicator_set_status",
    "app_indicator_set_icon_theme_path",
    "app_indicator_set_icon_full",
    "app_indicator_set_menu",
    "app_indicator_set_label",
];

/// Why the AppIndicator check failed.
#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq, Clone)]
enum ProbeFailure {
    /// No name in the list loads.
    NotLoadable,
    /// The first name that loads, which `libappindicator-sys` would use, lacks
    /// these functions or resolves them to a null address.
    MissingSymbols {
        library: &'static str,
        missing: Vec<&'static str>,
    },
}

/// The AppIndicator library `libappindicator-sys` will use, checked for the
/// required functions. A success and a missing-symbols failure are kept for the
/// process lifetime: both leave a handle open, so probing again would only add
/// references. A load failure is not kept, so the next tray attempt probes again.
#[cfg(target_os = "linux")]
fn appindicator_library() -> Result<&'static str, &'static ProbeFailure> {
    static RESULT: std::sync::OnceLock<Result<&'static str, ProbeFailure>> =
        std::sync::OnceLock::new();
    cached_probe(&RESULT, || {
        probe(&APPINDICATOR_LIBRARIES, &APPINDICATOR_REQUIRED_SYMBOLS)
    })
}

/// Returns the kept result, or runs `run` and keeps its result unless it is
/// `NotLoadable`.
#[cfg(target_os = "linux")]
fn cached_probe<'a>(
    cell: &'a std::sync::OnceLock<Result<&'static str, ProbeFailure>>,
    run: impl FnOnce() -> Result<&'static str, ProbeFailure>,
) -> Result<&'static str, &'a ProbeFailure> {
    static NOT_LOADABLE: ProbeFailure = ProbeFailure::NotLoadable;
    if let Some(kept) = cell.get() {
        return kept.as_ref().copied();
    }
    match run() {
        Err(ProbeFailure::NotLoadable) => Err(&NOT_LOADABLE),
        result => cell.get_or_init(|| result).as_ref().copied(),
    }
}

/// Loads the first name that opens and checks it for `symbols`. Later names are
/// not tried when the first one lacks a symbol, because `libappindicator-sys`
/// keeps the first library that loads.
#[cfg(target_os = "linux")]
fn probe(names: &[&'static str], symbols: &[&'static str]) -> Result<&'static str, ProbeFailure> {
    let (library, handle) = first_loadable(names).ok_or(ProbeFailure::NotLoadable)?;
    let missing = missing_symbols(symbols, |symbol| {
        // SAFETY: handle came from dlopen in `first_loadable` just above and is never
        // passed to dlclose, so it stays valid for every lookup.
        unsafe { lookup_symbol(handle, symbol) }
    });
    if missing.is_empty() {
        Ok(library)
    } else {
        Err(ProbeFailure::MissingSymbols { library, missing })
    }
}

/// The names in `symbols` whose address `lookup` reports as null. This is the
/// only place that decides whether a symbol is usable: a null address fails even
/// when `dlsym` reported no error (a GNU IFUNC resolver may return null), because
/// `libappindicator-sys` calls the address it gets.
#[cfg(target_os = "linux")]
fn missing_symbols(
    symbols: &[&'static str],
    mut lookup: impl FnMut(&str) -> *mut libc::c_void,
) -> Vec<&'static str> {
    symbols
        .iter()
        .copied()
        .filter(|symbol| lookup(symbol).is_null())
        .collect()
}

/// The raw `dlsym` address of `symbol` in `handle`, or null when the name has an
/// interior NUL. It does not interpret `dlerror`; [`missing_symbols`] decides.
///
/// # Safety
///
/// `handle` must be a non-null handle returned by `dlopen` that has not been
/// passed to `dlclose`.
#[cfg(target_os = "linux")]
unsafe fn lookup_symbol(handle: *mut libc::c_void, symbol: &str) -> *mut libc::c_void {
    let Ok(c_symbol) = std::ffi::CString::new(symbol) else {
        return std::ptr::null_mut();
    };
    // SAFETY: dlerror only clears this thread's pending dynamic-linker error here.
    // - Arguments: none; the returned message pointer is not read.
    // - Threads: glibc keeps the dlerror state per thread; libloading 0.7.4 lists
    //   Linux among the targets whose dlerror is thread-safe
    //   (libloading-0.7.4/src/os/unix/mod.rs, `Library::get`).
    unsafe { libc::dlerror() };
    // SAFETY: the lookup only reads the symbol table of a library this module opened.
    // - Pointer: the caller guarantees handle is a valid dlopen handle that has not been
    //   passed to dlclose (function-level Safety doc); c_symbol is a NUL-terminated string that lives
    //   until the call returns.
    // - Resolver code: for a GNU IFUNC symbol dlsym runs the library's resolver. In
    //   the product call this runs in the same thread context as the dlopen above
    //   (after GTK init, on the GTK-owning winit main thread) but earlier than
    //   libappindicator-sys would look the symbols up: all six are looked up before
    //   `app_indicator_new` creates any object, and `app_indicator_set_label` is
    //   looked up although upstream does not call it today. This relies on
    //   AppIndicator being an ordinary shared library whose functions can be looked
    //   up before any object exists (the Ayatana build checked here exports all six
    //   as plain FUNC symbols). The unit tests look up only `cos` and the six
    //   AppIndicator names in libc.so.6 and libm.so.6.
    // - Address use: the result is only compared with null; it is never called or
    //   dereferenced, so no function type is assumed.
    // - Threads: dlsym is not in the POSIX.1-2017 §2.9.1 list of functions that need
    //   not be thread-safe.
    unsafe { libc::dlsym(handle, c_symbol.as_ptr()) }
}

/// Opens each name with the flags `libloading` uses (`RTLD_LAZY | RTLD_LOCAL`)
/// and returns the first that loads with its handle. The handle is never closed:
/// `libappindicator-sys` may load the same library again once the builder is
/// reached (it is not reached when icon decoding fails or a symbol is missing),
/// and closing would run the unload routines of a GObject library.
#[cfg(target_os = "linux")]
fn first_loadable(names: &[&'static str]) -> Option<(&'static str, *mut libc::c_void)> {
    names.iter().copied().find_map(|name| {
        let Ok(c_name) = std::ffi::CString::new(name) else {
            return None;
        };
        // SAFETY: in the product call the probe opens only the AppIndicator names the tray builder opens, in the same context.
        // - Pointer: c_name is a NUL-terminated string that lives until the call returns;
        //   dlopen copies what it needs and keeps no pointer to it.
        // - Initialization routines: dlopen runs the library's initializers, which
        //   libloading 0.7.4 treats as calling an unknown foreign function. The product
        //   call is `appindicator_library`, which passes `APPINDICATOR_LIBRARIES`: the
        //   names and order that libappindicator-sys 0.9 (pulled in by tray-icon 0.22)
        //   opens with the same RTLD_LAZY | RTLD_LOCAL flags once `TrayIconBuilder::build`
        //   is reached. Reaching it is not guaranteed (icon decoding comes between and
        //   may fail), so this may load the library earlier than, or without,
        //   the build. In the product call the probe itself runs inside
        //   `create_tray_icon_inner` after `gtk_init::ensure()` succeeded, on the thread
        //   that owns GTK (the winit main thread when boot starts: `begin_boot`, reached from
        //   `resumed` or from the shell setup confirmation), the same context in which
        //   the build would load it. The unit tests call this function directly with only
        //   libc.so.6, libm.so.6 (already loaded, so no new initializer runs) and a name
        //   that does not exist; the GTK conditions do not apply to them.
        // - Termination routines: the handle is never passed to dlclose, so this code
        //   never triggers the library's unload routines.
        // - Threads: POSIX.1-2017 volume 2 §2.9.1 requires every function it defines to be
        //   thread-safe except those it lists, and dlopen is not listed:
        //   https://pubs.opengroup.org/onlinepubs/9699919799/functions/V2_chap02.html#tag_15_09_01
        //   (the reading libloading 0.7.4 records in libloading-0.7.4/src/os/unix/mod.rs for
        //   `Library: Sync`).
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        (!handle.is_null()).then_some((name, handle))
    })
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{
        APPINDICATOR_LIBRARIES, APPINDICATOR_REQUIRED_SYMBOLS, ProbeFailure, cached_probe,
        first_loadable, missing_symbols, probe,
    };
    use std::cell::Cell;
    use std::sync::OnceLock;

    #[test]
    fn a_missing_library_name_is_not_loadable() {
        assert_eq!(first_loadable(&["libtasty-does-not-exist.so.0"]), None);
        assert_eq!(
            probe(&["libtasty-does-not-exist.so.0"], &[]),
            Err(ProbeFailure::NotLoadable)
        );
    }

    #[test]
    fn the_first_loadable_name_wins() {
        assert_eq!(
            first_loadable(&["libtasty-does-not-exist.so.0", "libc.so.6", "libm.so.6"])
                .map(|(name, _)| name),
            Some("libc.so.6")
        );
    }

    #[test]
    fn a_library_without_the_required_symbols_is_rejected() {
        // libc.so.6 loads but has none of the AppIndicator functions.
        assert_eq!(
            probe(&["libc.so.6"], &APPINDICATOR_REQUIRED_SYMBOLS),
            Err(ProbeFailure::MissingSymbols {
                library: "libc.so.6",
                missing: APPINDICATOR_REQUIRED_SYMBOLS.to_vec(),
            })
        );
    }

    #[test]
    fn only_the_first_loadable_name_is_checked_for_symbols() {
        // libm.so.6 has `cos` and libc.so.6 does not, so stopping at libc.so.6 is
        // the behavior under test, not an environment accident.
        assert_eq!(probe(&["libm.so.6"], &["cos"]), Ok("libm.so.6"));
        assert_eq!(
            probe(&["libc.so.6", "libm.so.6"], &["cos"]),
            Err(ProbeFailure::MissingSymbols {
                library: "libc.so.6",
                missing: vec!["cos"],
            })
        );
    }

    #[test]
    fn a_resolved_null_address_counts_as_missing() {
        let mut fake = 0u8;
        let present: *mut libc::c_void = (&raw mut fake).cast();
        let missing = missing_symbols(&["resolves_to_null", "present"], |symbol| {
            if symbol == "present" {
                present
            } else {
                std::ptr::null_mut()
            }
        });
        assert_eq!(missing, vec!["resolves_to_null"]);
    }

    fn count_runs(
        cell: &OnceLock<Result<&'static str, ProbeFailure>>,
        result: impl Fn() -> Result<&'static str, ProbeFailure>,
    ) -> (usize, Vec<Result<&'static str, ProbeFailure>>) {
        let runs = Cell::new(0);
        let outcomes = (0..2)
            .map(|_| {
                cached_probe(cell, || {
                    runs.set(runs.get() + 1);
                    result()
                })
                .map_err(ProbeFailure::clone)
            })
            .collect();
        (runs.get(), outcomes)
    }

    #[test]
    fn a_not_loadable_result_is_probed_again() {
        let cell = OnceLock::new();
        let (runs, outcomes) = count_runs(&cell, || Err(ProbeFailure::NotLoadable));
        assert_eq!(runs, 2);
        assert!(
            outcomes
                .iter()
                .all(|o| *o == Err(ProbeFailure::NotLoadable))
        );
    }

    #[test]
    fn a_missing_symbols_result_is_kept() {
        let failure = || ProbeFailure::MissingSymbols {
            library: "libexample.so.1",
            missing: vec!["app_indicator_new"],
        };
        let cell = OnceLock::new();
        let (runs, outcomes) = count_runs(&cell, || Err(failure()));
        assert_eq!(runs, 1);
        assert!(outcomes.iter().all(|o| *o == Err(failure())));
    }

    #[test]
    fn a_success_is_kept() {
        let cell = OnceLock::new();
        let (runs, outcomes) = count_runs(&cell, || Ok("libexample.so.1"));
        assert_eq!(runs, 1);
        assert!(outcomes.iter().all(|o| *o == Ok("libexample.so.1")));
    }

    /// The upstream versions `APPINDICATOR_LIBRARIES` and
    /// `APPINDICATOR_REQUIRED_SYMBOLS` were last checked against.
    const UPSTREAM_CHECKED: [(&str, &str); 3] = [
        ("tray-icon", "0.22.0"),
        ("libappindicator", "0.9.0"),
        ("libappindicator-sys", "0.9.0"),
    ];

    /// Every version of package `name` in Cargo.lock text (a `name = "x"` line
    /// followed by its `version = "y"` line).
    fn lock_versions<'a>(lock: &'a str, name: &str) -> Vec<&'a str> {
        let target = format!("name = \"{name}\"");
        let mut versions = Vec::new();
        let mut lines = lock.lines();
        while let Some(line) = lines.next() {
            if line.trim() == target
                && let Some(next) = lines.next()
                && let Some(v) = next.trim().strip_prefix("version = ")
            {
                versions.push(v.trim_matches('"'));
            }
        }
        versions
    }

    fn check_upstream_versions(lock: &str) -> Result<(), String> {
        for (name, expected) in UPSTREAM_CHECKED {
            let found = lock_versions(lock, name);
            if found != [expected] {
                return Err(format!(
                    "Cargo.lock has {name} {found:?}, but the AppIndicator pre-check was checked \
                     against {name} {expected} only. Re-check APPINDICATOR_LIBRARIES against the \
                     names and order of `LIB` in libappindicator-sys <version>/src/lib.rs, and \
                     APPINDICATOR_REQUIRED_SYMBOLS against the functions libappindicator \
                     <version>/src/lib.rs and tray-icon <version>/src/platform_impl/gtk/mod.rs \
                     call. If the libloading version libappindicator-sys uses changed, re-check \
                     the load flags and the null handling too. Confirm backcompat is still on \
                     with `cargo tree -p tasty-platform --features gui -e features,no-dev -i \
                     libappindicator-sys` and `cargo tree -p tasty -e features,no-dev -i \
                     libappindicator-sys --target x86_64-unknown-linux-gnu`. Then update \
                     UPSTREAM_CHECKED."
                ));
            }
        }
        Ok(())
    }

    fn workspace_lock() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    #[test]
    fn upstream_versions_match_the_last_checked_ones() {
        if let Err(message) = check_upstream_versions(&workspace_lock()) {
            panic!("{message}");
        }
    }

    #[test]
    fn a_different_upstream_version_asks_for_a_recheck() {
        let lock = workspace_lock();
        for (name, expected) in UPSTREAM_CHECKED {
            let entry = format!("name = \"{name}\"\nversion = \"{expected}\"");
            assert!(lock.contains(&entry), "Cargo.lock lacks {entry:?}");

            let changed =
                lock.replace(&entry, &format!("name = \"{name}\"\nversion = \"999.0.0\""));
            let message =
                check_upstream_versions(&changed).expect_err("a changed version must fail");
            for part in [name, expected, "999.0.0", "src/lib.rs"] {
                assert!(message.contains(part), "{message:?} lacks {part:?}");
            }

            let doubled = lock.replace(
                &entry,
                &format!("{entry}\n\n[[package]]\nname = \"{name}\"\nversion = \"999.0.0\""),
            );
            assert!(
                check_upstream_versions(&doubled).is_err(),
                "a second {name} version must fail"
            );
        }
    }

    #[test]
    fn the_library_list_matches_upstream_order() {
        // libappindicator-sys-0.9.0/src/lib.rs `LIB`, backcompat names last.
        assert_eq!(
            APPINDICATOR_LIBRARIES,
            [
                "libayatana-appindicator3.so.1",
                "libappindicator3.so.1",
                "libayatana-appindicator3.so",
                "libappindicator3.so",
            ]
        );
    }

    #[test]
    fn the_required_symbols_match_the_checked_set() {
        // tray-icon-0.22.0/src/platform_impl/gtk/mod.rs through libappindicator-0.9.0/src/lib.rs.
        assert_eq!(
            APPINDICATOR_REQUIRED_SYMBOLS,
            [
                "app_indicator_new",
                "app_indicator_set_status",
                "app_indicator_set_icon_theme_path",
                "app_indicator_set_icon_full",
                "app_indicator_set_menu",
                "app_indicator_set_label",
            ]
        );
    }
}
