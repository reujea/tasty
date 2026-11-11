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
    let mut context = Context::new(window, Some(events.owned_display_handle()))
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
    let a = Context::new(first, Some(events.owned_display_handle()))
        .unwrap()
        .unwrap();
    let b = Context::new(second, Some(events.owned_display_handle()))
        .unwrap()
        .unwrap();
    assert!(Rc::ptr_eq(&a.owner, &b.owner));
    assert_ne!(a.origin, b.origin);
    assert_eq!(
        a.owner.borrow().state.seats.len(),
        b.owner.borrow().state.seats.len()
    );
}
