//! Real winit/XDND fixture. Never run against the desktop display.
use super::*;
use std::sync::{Arc, mpsc};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    platform::run_on_demand::EventLoopExtRunOnDemand,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window as WinitWindow, WindowId},
};

struct Fixture {
    window: Option<Arc<WinitWindow>>,
    native: Option<Session>,
    bytes: Vec<u8>,
    held: bool,
    forwarded: usize,
    released: bool,
    outcome: Option<Outcome>,
    ready: mpsc::Sender<()>,
    deadline: Instant,
}
impl ApplicationHandler for Fixture {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        let window = events
            .create_window(
                WinitWindow::default_attributes()
                    .with_title("owned outbound source")
                    .with_inner_size(winit::dpi::PhysicalSize::new(300, 200))
                    .with_position(winit::dpi::PhysicalPosition::new(50, 50)),
            )
            .unwrap();
        self.window = Some(Arc::new(window));
        self.ready.send(()).unwrap();
    }
    fn window_event(&mut self, events: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let window = self.window.as_ref().unwrap();
        match event {
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.held = state == ElementState::Pressed;
                if !self.held
                    && let Some(native) = self.native.as_mut()
                {
                    self.released = true;
                    native.release().unwrap();
                }
            }
            WindowEvent::CursorMoved { position, .. } if self.held => {
                if position.x >= 300.0 && self.native.is_none() {
                    let RawWindowHandle::Xlib(handle) = window.window_handle().unwrap().as_raw()
                    else {
                        panic!("X11 required")
                    };
                    self.native =
                        Some(Session::start(handle.window as u32, self.bytes.clone()).unwrap());
                }
                if let Some(native) = self.native.as_mut() {
                    self.forwarded += 1;
                    native.motion(position.x as i16, position.y as i16).unwrap();
                }
            }
            _ => {}
        }
        self.poll(events);
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        self.poll(events);
    }
}
impl Fixture {
    fn poll(&mut self, events: &ActiveEventLoop) {
        if let Some(native) = self.native.as_mut()
            && let Some(outcome) = native.poll().unwrap()
        {
            self.outcome = Some(outcome);
            events.exit();
        }
        assert!(
            Instant::now() < self.deadline,
            "fixture deadline: held={} moves={} released={}",
            self.held,
            self.forwarded,
            self.released
        );
        events.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(5),
        ));
    }
}

fn receiver(
    bytes: Vec<u8>,
    mode: String,
    ready: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let (conn, screen) = x11rb::connect(None).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        let a = Atoms::new(&conn).unwrap().reply().unwrap();
        conn.create_window(
            x11rb::COPY_FROM_PARENT as u8,
            window,
            root,
            600,
            50,
            300,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .unwrap()
        .check()
        .unwrap();
        conn.change_property32(PropMode::REPLACE, window, a.XdndAware, AtomEnum::ATOM, &[5])
            .unwrap();
        if mode == "same_process" {
            conn.change_property32(
                PropMode::REPLACE,
                window,
                a._NET_WM_PID,
                AtomEnum::CARDINAL,
                &[std::process::id()],
            )
            .unwrap();
        }
        conn.map_window(window).unwrap();
        conn.flush().unwrap();
        ready.recv().unwrap();
        let injector = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let status = std::process::Command::new("xdotool")
                .args([
                    "mousemove",
                    "100",
                    "100",
                    "mousedown",
                    "1",
                    "sleep",
                    "0.1",
                    "mousemove",
                    "180",
                    "100",
                    "sleep",
                    "0.1",
                    "mousemove",
                    "650",
                    "100",
                    "sleep",
                    "0.3",
                    "mouseup",
                    "1",
                ])
                .status()
                .unwrap();
            assert!(status.success());
        });
        let deadline = Instant::now() + Duration::from_secs(13);
        let mut source = 0;
        let mut received = Vec::new();
        let mut incremental = false;
        while Instant::now() < deadline {
            let Some(event) = conn.poll_for_event().unwrap() else {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
            match event {
                Event::ClientMessage(e) if e.type_ == a.XdndEnter => {
                    source = e.data.as_data32()[0];
                }
                Event::ClientMessage(e) if e.type_ == a.XdndPosition => {
                    let accepted = u32::from(mode != "reject");
                    conn.send_event(
                        false,
                        source,
                        EventMask::NO_EVENT,
                        ClientMessageEvent::new(
                            32,
                            source,
                            a.XdndStatus,
                            [window, accepted | 2, 0, 0, a.XdndActionCopy],
                        ),
                    )
                    .unwrap();
                }
                Event::ClientMessage(e) if e.type_ == a.XdndDrop => {
                    conn.convert_selection(
                        window,
                        a.XdndSelection,
                        a.uri,
                        a.uri,
                        e.data.as_data32()[2],
                    )
                    .unwrap();
                }
                Event::ClientMessage(e) if e.type_ == a.XdndLeave => break,
                Event::SelectionNotify(e) => {
                    assert_ne!(e.property, 0);
                    let reply = conn
                        .get_property(true, window, a.uri, AtomEnum::ANY, 0, u32::MAX)
                        .unwrap()
                        .reply()
                        .unwrap();
                    if reply.type_ == a.INCR {
                        incremental = true;
                    } else {
                        received = reply.value;
                    }
                }
                Event::PropertyNotify(e)
                    if incremental && e.atom == a.uri && e.state == Property::NEW_VALUE =>
                {
                    let reply = conn
                        .get_property(true, window, a.uri, AtomEnum::ANY, 0, u32::MAX)
                        .unwrap()
                        .reply()
                        .unwrap();
                    if reply.value.is_empty() {
                        incremental = false;
                    } else {
                        received.extend(reply.value);
                    }
                }
                _ => {}
            }
            if !received.is_empty() && !incremental {
                assert_eq!(received, bytes);
                let peer = if mode == "wrong_peer" {
                    window + 1
                } else {
                    window
                };
                conn.send_event(
                    false,
                    source,
                    EventMask::NO_EVENT,
                    ClientMessageEvent::new(
                        32,
                        source,
                        a.XdndFinished,
                        [peer, 1, a.XdndActionCopy, 0, 0],
                    ),
                )
                .unwrap()
                .check()
                .unwrap();
                conn.flush().unwrap();
                break;
            }
            conn.flush().unwrap();
        }
        injector.join().unwrap();
        received
    })
}

#[test]
#[ignore = "requires an owned Xvfb, xdotool and TASTY_DRAG_FIXTURE_DISPLAY=DISPLAY"]
fn real_winit_gesture_drives_xdnd_copy() {
    assert_eq!(
        std::env::var("DISPLAY").unwrap(),
        std::env::var("TASTY_DRAG_FIXTURE_DISPLAY").expect("owned fixture opt-in")
    );
    let mode = std::env::var("TASTY_DRAG_FIXTURE_MODE").unwrap_or_else(|_| "copy".into());
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file # 한글.txt");
    std::fs::write(&file, b"original must survive Copy").unwrap();
    let mut paths = vec![file.clone()];
    if mode == "incr" {
        paths.extend((0..1000).map(|n| temp.path().join(format!("item {n} with a long name"))));
    }
    let bytes = super::super::payload::uri_list(&paths);
    let (tx, rx) = mpsc::channel();
    let receiver = receiver(bytes.clone(), mode.clone(), rx);
    let mut fixture = Fixture {
        window: None,
        native: None,
        bytes,
        held: false,
        forwarded: 0,
        released: false,
        outcome: None,
        ready: tx,
        deadline: Instant::now() + Duration::from_secs(15),
    };
    let mut event_loop = crate::x11_test_loop::x11_event_loop::<()>();
    event_loop.run_app_on_demand(&mut fixture).unwrap();
    let received = receiver.join().unwrap();
    assert!(
        fixture.forwarded > 0,
        "actual winit movement outside source"
    );
    assert!(fixture.released, "actual winit release outside source");
    let expected = match mode.as_str() {
        "reject" | "same_process" => Outcome::Cancelled,
        "wrong_peer" => Outcome::Unknown,
        _ => Outcome::Copied,
    };
    assert_eq!(fixture.outcome, Some(expected));
    assert_eq!(std::fs::read(&file).unwrap(), b"original must survive Copy");
    if expected == Outcome::Copied {
        assert!(!received.is_empty());
        let dest = temp.path().join("receiver-copy");
        std::fs::copy(&file, &dest).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), std::fs::read(file).unwrap());
    }
}
