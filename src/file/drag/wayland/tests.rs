//! Uses an explicitly owned compositor; never connects to the user's display.
use super::*;
use winit::{event_loop::EventLoop, platform::wayland::EventLoopBuilderExtWayland};

#[test]
#[ignore = "requires TASTY_TEST_WAYLAND_DISPLAY on an isolated compositor"]
fn idle_context_does_not_bind_a_data_device() {
    let display = std::env::var("TASTY_TEST_WAYLAND_DISPLAY").expect("owned display required");
    assert_eq!(std::env::var("WAYLAND_DISPLAY").unwrap(), display);
    assert!(std::env::var_os("DISPLAY").is_none());
    let events = EventLoop::builder()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    // reason: This fixture creates no event handler and only inspects the initial context.
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(Window::default_attributes()).unwrap());
    let mut context = Context::new(window, Some(events.owned_display_handle()), false)
        .unwrap()
        .unwrap();
    context.poll().unwrap();
    let owner = context.owner.borrow();
    assert!(owner.state.manager.is_some());
    assert!(!owner.state.seats.is_empty());
    assert!(owner.state.seats.iter().any(|seat| seat.pointer.is_some()));
    assert!(owner.state.seats.iter().all(|seat| seat.device.is_none()));
    assert!(owner.state.active.is_none());
}

#[test]
#[ignore = "requires TASTY_TEST_WAYLAND_DISPLAY on an isolated compositor"]
fn windows_on_one_connection_share_one_owner() {
    let display = std::env::var("TASTY_TEST_WAYLAND_DISPLAY").expect("owned display required");
    assert_eq!(std::env::var("WAYLAND_DISPLAY").unwrap(), display);
    let events = EventLoop::builder()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    // reason: This fixture creates no event handler and only inspects the initial contexts.
    #[allow(deprecated)]
    let first = Arc::new(events.create_window(Window::default_attributes()).unwrap());
    // reason: as above.
    #[allow(deprecated)]
    let second = Arc::new(events.create_window(Window::default_attributes()).unwrap());
    let a = Context::new(first, Some(events.owned_display_handle()), true)
        .unwrap()
        .unwrap();
    let b = Context::new(second, Some(events.owned_display_handle()), true)
        .unwrap()
        .unwrap();
    assert!(Rc::ptr_eq(&a.owner, &b.owner));
    assert_ne!(a.origin, b.origin);
    let owner = a.owner.borrow();
    assert!(owner.state.windows.contains_key(&a.origin));
    assert!(owner.state.windows.contains_key(&b.origin));
    drop(owner);
    drop(b);
    assert_eq!(a.owner.borrow().state.windows.len(), 1);
}

#[test]
#[ignore = "requires TASTY_TEST_WAYLAND_DISPLAY on an isolated compositor"]
fn native_wayland_context_keeps_one_device_per_seat() {
    let display = std::env::var("TASTY_TEST_WAYLAND_DISPLAY").expect("owned display required");
    assert_eq!(std::env::var("WAYLAND_DISPLAY").unwrap(), display);
    let events = EventLoop::builder()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    // reason: This fixture creates no event handler and only inspects the initial context.
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(Window::default_attributes()).unwrap());
    let mut context = Context::new(window, Some(events.owned_display_handle()), true)
        .unwrap()
        .unwrap();
    assert!(context.poll().unwrap().is_empty());
    let owner = context.owner.borrow();
    assert!(owner.state.inbound);
    assert!(!owner.state.seats.is_empty());
    assert!(owner.state.seats.iter().all(|seat| seat.device.is_some()));
    assert!(owner.state.active.is_none());
}

/// X11 and XWayland must never hold a standing data device: a device is created only by the
/// outbound start or by `bind_inbound_devices`, which returns unless the start judgment set
/// `inbound`, and that flag comes only from `wants_inbound`.
#[test]
fn standing_device_is_created_only_behind_the_start_judgment() {
    let source = include_str!("../wayland.rs");
    let calls: Vec<usize> = source
        .match_indices(".get_data_device(")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(calls.len(), 2, "data device creation sites changed");
    let function_of = |at: usize| {
        let head = &source[..at];
        let start = [head.rfind("\n    pub(super) fn "), head.rfind("\nfn ")]
            .into_iter()
            .flatten()
            .max()
            .unwrap();
        head[start..].lines().nth(1).unwrap().trim().to_string()
    };
    let owners: Vec<String> = calls.iter().map(|&at| function_of(at)).collect();
    assert!(owners[0].starts_with("pub(super) fn start("), "{owners:?}");
    assert!(
        owners[1].starts_with("fn bind_inbound_devices("),
        "{owners:?}"
    );
    let body = &source[source.find("fn bind_inbound_devices(").unwrap()..];
    assert_eq!(body.lines().nth(1).unwrap().trim(), "if !s.inbound {");
    // Outbound reuses a standing device only when `inbound` is set.
    assert!(source.contains("let standing = seat.device.clone().filter(|_| state.inbound);"));
    // The flag is written once, from the constructor argument.
    assert!(source.contains("let mut state = State {\n            inbound,\n"));
    assert!(
        !source.contains(".inbound ="),
        "inbound must not be reassigned"
    );
    let drag = include_str!("../../drag.rs");
    assert!(drag.contains("wants_inbound(crate::boot::display_backend::start()"));
    assert!(drag.contains("wayland::Context::new(window, display_owner, inbound)"));
    assert_eq!(drag.matches("wayland::Context::new(").count(), 1);
}

/// The text clipboard adds no device of its own and, like the standing device, nothing at all
/// outside a native Wayland start: no keyboard, no selection source, no handle.
#[test]
fn the_text_clipboard_lives_only_behind_the_start_judgment() {
    let owner = include_str!("../wayland.rs");
    let clip = include_str!("clip.rs");
    assert!(!clip.contains(".get_data_device("));
    assert_eq!(owner.matches(".get_keyboard(").count(), 1);
    assert!(owner.contains("if s.inbound && c.contains(wl_seat::Capability::Keyboard) {"));
    assert!(!owner.contains("create_data_source(&queue.handle(), SelectionSource)"));
    assert_eq!(clip.matches("create_data_source(").count(), 1);
    let new = &clip[clip.find("fn new(").unwrap()..];
    assert!(
        new.contains("if !inbound || display_owner.is_none() {\n            return Ok(None);"),
        "Clipboard::new must refuse before touching the connection"
    );
    let drag = include_str!("../../drag.rs");
    assert_eq!(drag.matches("wayland::Clipboard::new(").count(), 1);
    assert!(drag.contains("wayland::Clipboard::new(window, display_owner, inbound)"));
    assert_eq!(
        drag.matches("wants_inbound(crate::boot::display_backend::start()")
            .count(),
        2
    );
}

#[test]
#[ignore = "requires TASTY_TEST_WAYLAND_DISPLAY on an isolated compositor"]
fn a_text_clipboard_exists_only_on_a_native_wayland_start() {
    let display = std::env::var("TASTY_TEST_WAYLAND_DISPLAY").expect("owned display required");
    assert_eq!(std::env::var("WAYLAND_DISPLAY").unwrap(), display);
    let events = EventLoop::builder()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    // reason: This fixture creates no event handler and only inspects the initial handles.
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(Window::default_attributes()).unwrap());
    assert!(
        Clipboard::new(&window, Some(events.owned_display_handle()), false)
            .unwrap()
            .is_none()
    );
    assert!(Clipboard::new(&window, None, true).unwrap().is_none());
    let mut clipboard = Clipboard::new(&window, Some(events.owned_display_handle()), true)
        .unwrap()
        .unwrap();
    // Nothing was copied: no selection of ours to paste at once.
    assert!(!matches!(clipboard.request(1).unwrap(), Paste::Ready(_)));
    assert!(clipboard.poll().unwrap().is_empty());
    let owner = clipboard.owner.borrow();
    assert!(owner.state.own_source.is_none());
    assert!(owner.state.seats.iter().all(|seat| seat.device.is_some()));
}
