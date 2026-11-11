//! GTK 3 기반 X11 컨텍스트 메뉴. 사용자 선택을 기다리지 않고 Pending 핸들을 반환한다.
//! 호출자는 매 프레임 poll로 GTK 이벤트를 처리한다. winit 창의 XID를 GdkWindow로 감싸
//! popup_at_rect의 위치·grab 기준 창으로 사용한다.

#[path = "linux_icon.rs"]
mod icon;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use gtk::glib::Cast;
use gtk::prelude::*;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::{MenuItem, MenuOutcome};

/// 선택 완료 신호가 오지 않는 메뉴를 닫아 핸들의 후속 처리가 영구 대기하지 않게 한다.
const WATCHDOG: Duration = Duration::from_secs(30);

fn ensure_gtk() -> bool {
    match crate::gtk_init::ensure() {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("gtk::init failed: {e}");
            false
        }
    }
}

/// debug 훅: 워치독 타임아웃을 짧게 덮어써 grab 실패 경로를 실사용 대기 없이
/// 관찰한다 (`TASTY_DEBUG_NATIVE_MENU_TIMEOUT_MS`). release 미노출.
#[cfg(debug_assertions)]
fn watchdog_duration() -> Duration {
    std::env::var("TASTY_DEBUG_NATIVE_MENU_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(WATCHDOG, Duration::from_millis)
}

#[cfg(not(debug_assertions))]
fn watchdog_duration() -> Duration {
    WATCHDOG
}

/// debug에서 grab을 생략해 실패 후 dismiss·timeout 처리를 재현한다.
#[cfg(debug_assertions)]
fn force_grab_failure() -> bool {
    std::env::var_os("TASTY_DEBUG_NATIVE_MENU_FORCE_GRAB_FAIL").is_some()
}

#[cfg(not(debug_assertions))]
fn force_grab_failure() -> bool {
    false
}

/// 메뉴와 신호 상태·watchdog·X11 display를 메뉴가 닫힐 때까지 보유한다.
pub struct GtkMenuHandle {
    menu: gtk::Menu,
    display: gdkx11::X11Display,
    selected: Rc<Cell<Option<u32>>>,
    done: Rc<Cell<bool>>,
    timed_out: Rc<Cell<bool>>,
    grabbed: Rc<Cell<bool>>,
    timeout_id: Option<gtk::glib::SourceId>,
    watchdog: Duration,
    finished: bool,
}

impl GtkMenuHandle {
    /// GTK 이벤트가 빌 때까지 처리하고 닫힘 결과를 반환한다.
    /// 새 이벤트를 기다리지는 않지만 이벤트 처리 시간의 상한을 두지는 않는다.
    pub(super) fn poll(&mut self) -> Option<Option<u32>> {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if !self.done.get() {
            return None;
        }
        Some(self.finish())
    }

    /// Close the menu without a selection (outside click routed from winit).
    /// The cancel surfaces through the next `poll`, so completion keeps a
    /// single path.
    pub(super) fn dismiss(&mut self) {
        if self.done.get() {
            return;
        }
        self.menu.popdown();
        self.done.set(true);
    }

    fn finish(&mut self) -> Option<u32> {
        let was_grabbed = self.grabbed.get();
        self.release();
        self.finished = true;
        if self.timed_out.get() {
            tracing::warn!(
                "native context menu popup timed out after {:?} without selection-done (pointer grab was {}) — forcing close",
                self.watchdog,
                if was_grabbed {
                    "established"
                } else {
                    "NOT established"
                }
            );
            return None;
        }
        self.selected.get()
    }

    /// Watchdog 해제 + 대칭 ungrab. 완료 경로(`finish`)와 미완 상태로 버려지는
    /// 경로(`Drop`)가 공유한다.
    fn release(&mut self) {
        if let Some(id) = self.timeout_id.take() {
            // 이미 발화한 once 소스는 스스로 제거된다 — 재제거는 glib 경고.
            if !self.timed_out.get() {
                id.remove();
            }
        }
        if self.grabbed.replace(false)
            && let Some(seat) = self
                .display
                .upcast_ref::<gtk::gdk::Display>()
                .default_seat()
        {
            seat.ungrab();
        }
    }
}

impl Drop for GtkMenuHandle {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // 결과를 회수하지 않고 핸들이 버려지는 경로(창 종료 등) — 유령 메뉴와
        // 잡힌 채 남는 포인터 grab 을 남기지 않는다.
        self.menu.popdown();
        self.release();
    }
}

/// 이미 경고한 winit·GDK 배율 조합. 같은 조합은 중복 보고하지 않고 새 조합은 알린다.
static WARNED_ANCHOR_SCALES: Mutex<Vec<(u64, i32)>> = Mutex::new(Vec::new());

/// 위 경고-억제 셋 락의 poison 복구 공용 보고 좌표(첫-1 회). 복구는 안전하다(억제는 부가).
const WARNED_ANCHOR_SCALES_WHAT: &str = "native-menu anchor-scale warning set";
static WARNED_ANCHOR_SCALES_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// winit 논리 좌표를 GDK 논리 좌표로 그대로 전달하므로 두 배율이 다르면 위치가 어긋날 수 있다.
/// 각 라이브러리가 다른 설정을 읽어 이 일치가 보장되지 않으므로 실제 불일치를 경고한다.
/// 좌표 변환은 여기서 수정하지 않는다. 확인 절차는 docs/ai-verification/dpi-scale-verification.md를 따른다.
pub fn warn_if_menu_anchor_scale_premise_broken(winit_scale: f64) {
    if !ensure_gtk() {
        return;
    }
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    // GDK3/X11 의 배율은 디스플레이 전역(`GDK_SCALE`)이라 어느 모니터로 읽어도
    // 같다. primary 가 없는 구성만 0 번으로 물러선다.
    let Some(monitor) = display.primary_monitor().or_else(|| display.monitor(0)) else {
        return;
    };
    let gdk_scale = monitor.scale_factor();
    if winit_scale == f64::from(gdk_scale) {
        return;
    }

    let key = (winit_scale.to_bits(), gdk_scale);
    {
        // 경고 경로가 락 오염으로 침묵하면 안 된다 — 중복 억제는 부가 기능이다.
        let mut warned = tasty_utils::poison::recover_mutex(
            WARNED_ANCHOR_SCALES.lock(),
            WARNED_ANCHOR_SCALES_WHAT,
            &WARNED_ANCHOR_SCALES_POISONED,
        );
        if warned.contains(&key) {
            return;
        }
        warned.push(key);
    }

    tracing::warn!(
        "native context menu: winit scale_factor={winit_scale} vs GDK scale_factor={gdk_scale} \
         — 배율 전제가 깨졌다. 네이티브 메뉴 앵커가 어긋날 수 있다 \
         (메뉴가 클릭 지점에서 winit/GDK 배율비만큼 옮겨진 자리에 뜬다). \
         절차: docs/ai-verification/dpi-scale-verification.md"
    );
}

#[allow(clippy::cognitive_complexity)] // complexity-exempt: GTK/X11 grab 타이밍이 selected/done/grabbed/timed_out 4개 Rc<Cell<_>>를 공유하는 여러 클로저(activate/selection-done/button-press/idle/timeout)의 등록 순서 자체에 의미론이 있음(GDK Seat::grab을 idle 콜백에서 호출해야 하고 timeout이 없으면 유령 메뉴가 영원히 남음) — 클로저를 분리하면 각 함수가 4~5개 Rc<Cell<>> 핸들을 인자로 주고받아야 하고 실행 순서와 코드 위치가 물리적으로 분리되어 가독성이 오히려 나빠짐
pub fn show_context_menu(
    window: &impl HasWindowHandle,
    x: f64,
    y: f64,
    items: &[MenuItem],
) -> MenuOutcome {
    if !ensure_gtk() {
        return MenuOutcome::Ready(None);
    }

    let x11_window = match window.window_handle().ok().map(|h| h.as_raw()) {
        Some(RawWindowHandle::Xlib(w)) => w.window,
        _ => {
            tracing::warn!("native context menu: not an X11 window (Wayland is not supported)");
            return MenuOutcome::Ready(None);
        }
    };
    let gdk_display = match gtk::gdk::Display::default() {
        Some(d) => d,
        None => {
            tracing::warn!("native context menu: no GDK display");
            return MenuOutcome::Ready(None);
        }
    };
    let x11_gdk_display: gdkx11::X11Display = match gdk_display.downcast() {
        Ok(d) => d,
        Err(_) => {
            tracing::warn!("native context menu: GDK display is not X11");
            return MenuOutcome::Ready(None);
        }
    };
    // winit의 기존 창을 GdkWindow로 감싼다. 이미 파괴된 창 등으로 조회에 실패하면 메뉴를 열지 않는다.
    let rect_window = match crate::x11_gdk_window::foreign_gdk_window(&x11_gdk_display, x11_window)
    {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("native context menu: {e}");
            return MenuOutcome::Ready(None);
        }
    };

    let menu = gtk::Menu::new();
    let selected: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));

    for item in items {
        if item.is_separator() {
            let sep = gtk::SeparatorMenuItem::new();
            menu.append(&sep);
        } else {
            let mi = menu_item(item);
            mi.set_sensitive(item.enabled);
            if let Some(tooltip) = &item.tooltip {
                mi.set_tooltip_text(Some(tooltip));
            }
            if item.enabled {
                let id = item.id;
                let selected = Rc::clone(&selected);
                mi.connect_activate(move |_| {
                    selected.set(Some(id));
                });
            }
            menu.append(&mi);
        }
    }
    menu.show_all();

    let done: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    {
        let done = Rc::clone(&done);
        menu.connect_selection_done(move |_| {
            done.set(true);
        });
    }

    // 메뉴 밖으로 전달된 클릭은 직접 닫는다. grab이 실패해 GTK에 클릭이 오지 않으면
    // 호스트 winit 입력 경로가 MenuHandle::dismiss를 호출한다.
    {
        let done = Rc::clone(&done);
        menu.connect_button_press_event(move |menu_widget, event| {
            let (px, py) = event.position();
            let w = f64::from(menu_widget.allocated_width());
            let h = f64::from(menu_widget.allocated_height());
            if px < 0.0 || py < 0.0 || px >= w || py >= h {
                menu_widget.popdown();
                done.set(true);
            }
            gtk::glib::Propagation::Proceed
        });
    }

    // GDK 이벤트 경로로 클릭을 받으려고 Seat::grab을 사용한다.
    // popup_at_rect의 map이 처리된 뒤 grab하도록 idle 콜백에 등록한다.
    let grabbed: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    {
        let grabbed = Rc::clone(&grabbed);
        let x11_gdk_display = x11_gdk_display.clone();
        let menu_weak = menu.downgrade();
        gtk::glib::idle_add_local_once(move || {
            if force_grab_failure() {
                tracing::warn!(
                    "native context menu: grab skipped (TASTY_DEBUG_NATIVE_MENU_FORCE_GRAB_FAIL)"
                );
                return;
            }
            let Some(menu) = menu_weak.upgrade() else {
                return;
            };
            let Some(gdk_win) = menu.window() else {
                return;
            };
            let Some(seat) = x11_gdk_display
                .upcast_ref::<gtk::gdk::Display>()
                .default_seat()
            else {
                return;
            };
            // `popup_at_rect`'s own internal grab (established with no
            // trigger event) does succeed at the X11 level — release it
            // first so our explicit one below doesn't fail with
            // `AlreadyGrabbed`.
            seat.ungrab();
            let status = seat.grab(
                &gdk_win,
                gtk::gdk::SeatCapabilities::POINTER | gtk::gdk::SeatCapabilities::KEYBOARD,
                true, // owner_events: let clicks inside the menu (or any of
                // its own sub-windows) route normally so GTK's own
                // item hit-testing/activation keeps working; clicks
                // outside every owned window still land on the menu
                // (the grab window) and reach the handler above.
                None,
                None,
                None,
            );
            if status == gtk::gdk::GrabStatus::Success {
                grabbed.set(true);
            } else {
                tracing::warn!(
                    "native context menu: seat grab failed ({status:?}) — outside-click dismiss falls back to the winit press path"
                );
            }
        });
    }

    let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
    menu.popup_at_rect(
        &rect_window,
        &rect,
        gtk::gdk::Gravity::NorthWest,
        gtk::gdk::Gravity::NorthWest,
        None,
    );

    // grab이나 완료 신호가 실패해 메뉴가 남아 있을 때 watchdog으로 닫는다.
    let watchdog = watchdog_duration();
    let timed_out: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let timeout_id = {
        let done = Rc::clone(&done);
        let timed_out = Rc::clone(&timed_out);
        let menu_weak = menu.downgrade();
        gtk::glib::timeout_add_local_once(watchdog, move || {
            if done.get() {
                return;
            }
            timed_out.set(true);
            done.set(true);
            if let Some(menu) = menu_weak.upgrade() {
                menu.popdown();
            }
        })
    };

    MenuOutcome::Pending(super::MenuHandle::from_gtk(GtkMenuHandle {
        menu,
        display: x11_gdk_display,
        selected,
        done,
        timed_out,
        grabbed,
        timeout_id: Some(timeout_id),
        watchdog,
        finished: false,
    }))
}

/// 항목 하나. 단축키가 있으면 GTK 가속키 표시와 같은 모양으로 이름 오른쪽에 붙인다.
/// `AccelLabel` 은 키 이름을 GTK 가 정하므로 다른 OS 와 같은 표시 문자열을 직접 쓴다.
fn menu_item(item: &MenuItem) -> gtk::MenuItem {
    if item.shortcut.is_none() && item.icon.is_none() {
        return gtk::MenuItem::with_label(&item.label);
    }
    let mi = gtk::MenuItem::new();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let label = gtk::Label::new(Some(&item.label));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    if let Some(glyph) = item.icon {
        let image = icon::widget(glyph, &mi);
        row.pack_start(&image, false, false, 0);
    }
    row.pack_start(&label, true, true, 0);
    if let Some(shortcut) = &item.shortcut {
        let accel = gtk::Label::new(Some(&shortcut.display));
        accel.style_context().add_class("accelerator");
        row.pack_end(&accel, false, false, 0);
    }
    mi.add(&row);
    mi
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 테스트에서만 쓰는 X11 윈도우 핸들 어댑터 — GTK 가 만든 실제 창의 XID 를
    /// `show_context_menu` 가 요구하는 `HasWindowHandle` 로 감싼다.
    struct XlibWindow(std::os::raw::c_ulong);

    impl HasWindowHandle for XlibWindow {
        fn window_handle(
            &self,
        ) -> Result<winit::raw_window_handle::WindowHandle<'_>, winit::raw_window_handle::HandleError>
        {
            let h = winit::raw_window_handle::XlibWindowHandle::new(self.0);
            // SAFETY: 아래 호출부가 GTK 창을 이 핸들의 수명 동안 살려 둔다.
            Ok(unsafe {
                winit::raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::Xlib(h))
            })
        }
    }

    /// 행 안의 첫 widget(아이콘)과 그 뒤 라벨 사이 간격.
    fn icon_gap(row: &gtk::Widget) -> i32 {
        let item = row.clone().downcast::<gtk::MenuItem>().unwrap();
        let cells = item
            .child()
            .unwrap()
            .downcast::<gtk::Box>()
            .unwrap()
            .children();
        assert!(cells[0].is::<gtk::DrawingArea>(), "icon first");
        cells[0].margin_end()
    }

    /// 아이콘 행 둘(단축키 있음·비활성), 구분선, 아이콘 없는 행.
    fn assert_icon_rows(rows: &[gtk::Widget]) {
        let first = rows[0].clone().downcast::<gtk::MenuItem>().unwrap();
        let first_row = first.child().unwrap().downcast::<gtk::Box>().unwrap();
        assert_eq!(first_row.children().len(), 3, "icon + label + accelerator");
        assert_eq!(icon_gap(&rows[0]), super::icon::label_gap());
        assert_eq!(icon_gap(&rows[1]), super::icon::label_gap());
        assert!(!rows[1].is_sensitive());
        assert_eq!(rows[1].tooltip_text().as_deref(), Some("Unavailable"));
        assert!(
            rows[3]
                .clone()
                .downcast::<gtk::MenuItem>()
                .unwrap()
                .child()
                .unwrap()
                .is::<gtk::Label>()
        );
    }

    /// 아이콘과 라벨 사이는 시안 space-sm 이고, 행을 메뉴에 붙인 뒤에도 그대로다.
    /// 붙기 전 GTK style padding 은 0 이라 그 값을 읽으면 간격이 사라진다.
    /// 실행: cargo test -p tasty-platform --lib --features gui -- --ignored --test-threads=1 native_menu::linux
    #[test]
    #[ignore]
    fn icon_keeps_the_design_gap_before_its_label() {
        use crate::native_menu::MenuIcon;
        use tasty_type_appearance::theme::SIZING;
        assert!(ensure_gtk(), "이 테스트는 X11 디스플레이가 있어야 한다");
        assert_eq!(super::icon::label_gap(), SIZING.spacing_sm.value() as i32);
        let row = menu_item(
            &MenuItem::new(1, "Open in system").with_icon(MenuIcon::from_svg(
                tasty_icons::EXTERNAL.svg,
                SIZING.icon_glyph_size_md,
            )),
        );
        let menu = gtk::Menu::new();
        menu.append(&row);
        menu.show_all();
        assert_eq!(icon_gap(row.upcast_ref()), SIZING.spacing_sm.value() as i32);
    }

    /// 실제 메뉴 선택은 격리 X11 하네스의 pointer 입력으로 한다. 제품 IPC를 추가하지 않는다.
    #[test]
    #[ignore]
    fn icon_menu_keeps_async_selection_and_disabled_rows() {
        use crate::native_menu::{MenuIcon, MenuShortcut};
        use tasty_type_appearance::theme::SIZING;
        assert!(ensure_gtk());
        let win = gtk::Window::new(gtk::WindowType::Toplevel);
        win.set_title("Native menu icon verification");
        win.set_default_size(400, 240);
        win.show_all();
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        let xid = win
            .window()
            .unwrap()
            .downcast::<gdkx11::X11Window>()
            .unwrap()
            .xid();
        let items = [
            MenuItem::new(7, "Open in system")
                .with_icon(MenuIcon::from_svg(
                    tasty_icons::EXTERNAL.svg,
                    SIZING.icon_glyph_size_md,
                ))
                .with_shortcut(Some(MenuShortcut {
                    binding: "ctrl+o".into(),
                    display: "Ctrl+O".into(),
                })),
            MenuItem::disabled(8, "Show in File Manager")
                .with_tooltip("Unavailable")
                .with_icon(MenuIcon::from_svg(
                    tasty_icons::FOLDER_OPEN.svg,
                    SIZING.icon_glyph_size_md,
                )),
            MenuItem::separator(),
            MenuItem::new(9, "Without icon"),
        ];
        let MenuOutcome::Pending(mut handle) =
            show_context_menu(&XlibWindow(xid), 30.0, 30.0, &items)
        else {
            panic!("GTK must return Pending");
        };
        let super::super::HandleImpl::Gtk(gtk_handle) = &handle.0 else {
            panic!("GTK handle");
        };
        assert_icon_rows(&gtk_handle.menu.children());
        let menu_weak = gtk_handle.menu.downgrade();
        let menu_window = gtk_handle.menu.window().unwrap();
        let (_, x, y) = menu_window.origin();
        let _trace = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_test_writer()
                .with_ansi(false)
                .without_time()
                .finish(),
        );
        tracing::info!(
            "NATIVE_MENU_READY xid={xid} x={x} y={y} scale={} rows=4",
            gtk_handle.menu.scale_factor()
        );
        assert!(handle.poll().is_none());
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(result) = handle.poll() {
                assert_eq!(result, Some(7), "pointer must select enabled icon row");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no isolated pointer selection"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(handle);
        if let Some(menu) = menu_weak.upgrade() {
            assert!(!menu.is_visible(), "selection must close the native menu");
        }
        win.close();
    }

    /// X11의 실제 GTK 메뉴로 강제 grab 실패 뒤 반환·poll·watchdog 처리를 확인한다.
    /// 실행: cargo test -p tasty-platform --lib --features gui -- --ignored --test-threads=1 native_menu::linux
    /// 실제 메뉴가 잠시 뜨므로 격리 X11 디스플레이에서 실행한다. gui를 빼면 이 시험은 포함되지 않는다.
    #[test]
    #[ignore]
    fn forced_grab_failure_resolves_via_watchdog_without_blocking() {
        // 가드가 원값 복원까지 맡는다 — 아래 단언 중 하나가 패닉해도 env 오염이
        // 남지 않는다. 동시 경합은 `#[ignore]` + `--test-threads=1` 실행 조건이 막는다.
        let _force_fail =
            tasty_test_support::EnvVarGuard::set("TASTY_DEBUG_NATIVE_MENU_FORCE_GRAB_FAIL", "1");
        let _timeout =
            tasty_test_support::EnvVarGuard::set("TASTY_DEBUG_NATIVE_MENU_TIMEOUT_MS", "250");
        assert!(ensure_gtk(), "이 테스트는 X11 디스플레이가 있어야 한다");

        let win = gtk::Window::new(gtk::WindowType::Toplevel);
        win.set_default_size(200, 100);
        win.show();
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        let xid = win
            .window()
            .and_then(|w| w.downcast::<gdkx11::X11Window>().ok())
            .map(|w| w.xid())
            .expect("X11 GdkWindow");
        let anchor = XlibWindow(xid);

        let items = [MenuItem::new(1, "item")];
        let opened = std::time::Instant::now();
        let outcome = show_context_menu(&anchor, 10.0, 10.0, &items);
        assert!(
            opened.elapsed() < Duration::from_millis(500),
            "show_context_menu 는 메뉴가 닫히기를 기다리지 않는다"
        );
        let mut handle = match outcome {
            MenuOutcome::Pending(h) => h,
            MenuOutcome::Ready(_) => panic!("Linux 백엔드는 Pending 을 돌려줘야 한다"),
        };

        // 워치독이 걷어갈 때까지 폴링 — 각 폴링이 프레임 예산 안에 끝나야 한다.
        let mut polls = 0u32;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let result = loop {
            let tick = std::time::Instant::now();
            let r = handle.poll();
            assert!(
                tick.elapsed() < Duration::from_millis(100),
                "폴링 한 번이 {:?} 나 걸렸다 — 블로킹 대기가 되살아났다",
                tick.elapsed()
            );
            polls += 1;
            if let Some(r) = r {
                break r;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "250ms 워치독이 5초 안에 메뉴를 걷어가지 않았다"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(result, None, "워치독 강제 종료는 선택 없음으로 해소된다");
        assert!(
            polls > 1,
            "메뉴가 떠 있는 동안 호출자가 여러 번 제어를 돌려받아야 한다 (polls={polls})"
        );

        win.close();
    }
}
