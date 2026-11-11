//! Independent proxies on winit's *same* libwayland client, as in smithay-clipboard.
//! The genuine pointer grab serial and originating surface are observed before handoff.
//! One owner per connection serves every window: the compositor addresses a client's data device,
//! so per-window devices on the same seat would compete for one drag.
#[cfg(test)]
mod tests;

use super::Outcome;
use std::{
    cell::RefCell,
    io::Write,
    os::fd::{AsRawFd, OwnedFd},
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
    backend::Backend,
    protocol::{
        wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_pointer,
        wl_registry, wl_seat, wl_surface,
    },
};
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
    window::Window,
};

pub(super) struct Context {
    owner: Rc<RefCell<Owner>>,
    /// Protocol id of this window's `wl_surface`.
    origin: u32,
    _window: Arc<Window>,
}
struct Owner {
    queue: EventQueue<State>,
    state: State,
    connection: Connection,
    // Drops after the queue/connection: foreign proxies must not outlive the display owner.
    _display_owner: winit::event_loop::OwnedDisplayHandle,
}
thread_local! {
    /// Live owners keyed by libwayland display pointer. winit runs every window on this thread.
    static OWNERS: RefCell<Vec<(usize, Weak<RefCell<Owner>>)>> = const { RefCell::new(Vec::new()) };
}
struct Seat {
    id: u32,
    seat: wl_seat::WlSeat,
    pointer: Option<wl_pointer::WlPointer>,
    device: Option<wl_data_device::WlDataDevice>,
    focus: Option<wl_surface::WlSurface>,
    grab: Option<(u32, wl_surface::WlSurface)>,
    offer: Option<wl_data_offer::WlDataOffer>,
    buttons: std::collections::HashSet<u32>,
}
struct Transfer {
    file: std::fs::File,
    offset: usize,
    deadline: Instant,
}
struct Active {
    source: wl_data_source::WlDataSource,
    bytes: Vec<u8>,
    outcome: Option<Outcome>,
    accepted: bool,
    dropped: bool,
    deadline: Option<Instant>,
    transfers: Vec<Transfer>,
}
#[derive(Default)]
struct State {
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    seats: Vec<Seat>,
    active: Option<Active>,
}

impl Context {
    pub(super) fn new(
        window: Arc<Window>,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Result<Option<Self>, String> {
        let RawDisplayHandle::Wayland(display) =
            window.display_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Ok(None);
        };
        let RawWindowHandle::Wayland(handle) =
            window.window_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Ok(None);
        };
        let display_owner = display_owner.ok_or("Wayland drag requires an owned display handle")?;
        if display_owner
            .display_handle()
            .map_err(|e| e.to_string())?
            .as_raw()
            != window.display_handle().map_err(|e| e.to_string())?.as_raw()
        {
            return Err("Wayland drag display owner mismatch".into());
        }
        // SAFETY: Borrow the surface's identity without installing a listener or destroying winit's proxy.
        let id = unsafe {
            wayland_client::backend::ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                handle.surface.as_ptr().cast(),
            )
        }
        .map_err(|e| e.to_string())?;
        let key = display.display.as_ptr() as usize;
        let shared = OWNERS.with_borrow_mut(|owners| {
            owners.retain(|(_, owner)| owner.strong_count() > 0);
            owners
                .iter()
                .find(|(display, _)| *display == key)
                .and_then(|(_, owner)| owner.upgrade())
        });
        let owner = match shared {
            Some(owner) => owner,
            None => {
                let owner = Rc::new(RefCell::new(Owner::new(display, display_owner)?));
                OWNERS.with_borrow_mut(|owners| owners.push((key, Rc::downgrade(&owner))));
                owner
            }
        };
        Ok(Some(Self {
            owner,
            origin: id.protocol_id(),
            _window: window,
        }))
    }
    pub(super) fn poll(&mut self) -> Result<(), String> {
        self.owner.borrow_mut().poll()
    }
    pub(super) fn start(&mut self, bytes: Vec<u8>) -> Result<Session, String> {
        let origin_id = self.origin;
        let mut owner = self.owner.borrow_mut();
        owner.poll()?;
        let Owner {
            state,
            queue,
            connection,
            ..
        } = &mut *owner;
        if state.active.is_some() {
            return Err("Wayland drag already active".into());
        }
        let manager = state
            .manager
            .as_ref()
            .ok_or("Wayland compositor has no data-device manager v3")?;
        let matches: Vec<_> = state
            .seats
            .iter()
            .filter_map(|seat| {
                seat.grab
                    .as_ref()
                    .filter(|(_, surface)| surface.id().protocol_id() == origin_id)
                    .map(|grab| (seat, grab))
            })
            .collect();
        if matches.len() != 1 {
            return Err("Wayland drag requires one live pointer grab on the source surface".into());
        }
        let (seat, (serial, origin)) = matches[0];
        let seat_id = seat.id;
        let serial = *serial;
        let origin = origin.clone();
        let device = manager.get_data_device(&seat.seat, &queue.handle(), seat_id);
        let source = manager.create_data_source(&queue.handle(), ());
        source.offer("text/uri-list".into());
        source.set_actions(wl_data_device_manager::DndAction::Copy);
        device.start_drag(Some(&source), &origin, None, serial);
        state
            .seats
            .iter_mut()
            .find(|seat| seat.id == seat_id)
            .expect("source seat")
            .device = Some(device);
        state.active = Some(Active {
            source,
            bytes,
            outcome: None,
            accepted: false,
            dropped: false,
            deadline: None,
            transfers: Vec::new(),
        });
        if let Err(error) = connection.flush() {
            if let Some(active) = state.active.take() {
                active.source.destroy();
            }
            for seat in &mut state.seats {
                if let Some(device) = seat.device.take() {
                    device.release();
                }
            }
            return Err(error.to_string());
        }
        Ok(Session(self.owner.clone()))
    }
}
impl Owner {
    fn new(
        display: winit::raw_window_handle::WaylandDisplayHandle,
        display_owner: winit::event_loop::OwnedDisplayHandle,
    ) -> Result<Self, String> {
        // SAFETY: OwnedDisplayHandle retains winit's connection even after its event loop has exited.
        let connection = Connection::from_backend(unsafe {
            Backend::from_foreign_display(display.display.as_ptr().cast())
        });
        let mut queue = connection.new_event_queue();
        let q = queue.handle();
        let mut state = State::default();
        connection.display().get_registry(&q, ());
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        connection.flush().map_err(|e| e.to_string())?;
        Ok(Self {
            queue,
            state,
            connection,
            _display_owner: display_owner,
        })
    }
    fn poll(&mut self) -> Result<(), String> {
        // winit (or another shared-display reader) reads the socket; dispatch only our queue.
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| e.to_string())?;
        if let Some(active) = self.state.active.as_mut() {
            let mut index = 0;
            while index < active.transfers.len() {
                let transfer = &mut active.transfers[index];
                if Instant::now() >= transfer.deadline {
                    active.transfers.swap_remove(index);
                    active.outcome = Some(Outcome::Unknown);
                    continue;
                }
                let end = (transfer.offset + 32 * 1024).min(active.bytes.len());
                match transfer.file.write(&active.bytes[transfer.offset..end]) {
                    Ok(0) => {
                        active.transfers.swap_remove(index);
                    }
                    Ok(n) => {
                        transfer.offset += n;
                        if transfer.offset == active.bytes.len() {
                            active.transfers.swap_remove(index);
                        } else {
                            index += 1;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => index += 1,
                    Err(e) => {
                        tracing::warn!(%e,"Wayland drag data pipe failed");
                        active.transfers.swap_remove(index);
                        active.outcome = Some(Outcome::Unknown);
                    }
                }
            }
            if active.deadline.is_some_and(|d| Instant::now() >= d) && active.outcome.is_none() {
                active.outcome = Some(Outcome::Unknown);
            }
        }
        self.connection.flush().map_err(|e| e.to_string())
    }
}
pub(super) struct Session(Rc<RefCell<Owner>>);
impl Session {
    pub(super) fn release(&mut self) {
        if let Some(active) = self.0.borrow_mut().state.active.as_mut() {
            active.deadline = Some(Instant::now() + Duration::from_secs(10));
        }
    }
    pub(super) fn poll(&mut self) -> Result<Option<Outcome>, String> {
        let mut owner = self.0.borrow_mut();
        owner.poll()?;
        Ok(owner.state.active.as_ref().and_then(|a| a.outcome))
    }
    pub(super) fn accepted(&self) -> bool {
        self.0
            .borrow()
            .state
            .active
            .as_ref()
            .is_some_and(|a| a.accepted)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let mut owner = self.0.borrow_mut();
        if let Some(active) = owner.state.active.take() {
            active.source.destroy();
        }
        for seat in &mut owner.state.seats {
            seat.grab = None;
            seat.buttons.clear();
            if let Some(offer) = seat.offer.take() {
                offer.destroy();
            }
            if let Some(device) = seat.device.take() {
                device.release();
            }
        }
        if let Err(e) = owner.connection.flush() {
            tracing::debug!(%e,"Wayland drag cleanup flush");
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(active) = self.state.active.take() {
            active.source.destroy();
        }
        for seat in &mut self.state.seats {
            if let Some(p) = seat.pointer.take() {
                p.release();
            }
            if let Some(d) = seat.device.take() {
                d.release();
            }
            if let Some(offer) = seat.offer.take() {
                offer.destroy();
            }
            seat.seat.release();
        }
        if let Err(e) = self.connection.flush() {
            tracing::debug!(%e,"Wayland drag context cleanup");
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        match e {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_data_device_manager" && version >= 3 => {
                s.manager = Some(r.bind(name, 3, q, ()))
            }
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_seat" && version >= 5 => {
                let seat = r.bind(name, version.min(7), q, name);

                s.seats.push(Seat {
                    id: name,
                    seat,
                    pointer: None,
                    device: None,
                    focus: None,
                    grab: None,
                    offer: None,
                    buttons: Default::default(),
                });
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(index) = s.seats.iter().position(|seat| seat.id == name) {
                    let mut seat = s.seats.remove(index);
                    if let Some(p) = seat.pointer.take() {
                        p.release();
                    }
                    if let Some(d) = seat.device.take() {
                        d.release();
                    }
                    if let Some(offer) = seat.offer.take() {
                        offer.destroy();
                    }
                    seat.seat.release();
                    if let Some(a) = s.active.as_mut() {
                        a.outcome = Some(Outcome::Cancelled);
                    }
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_seat::WlSeat, u32> for State {
    fn event(
        s: &mut Self,
        seat: &wl_seat::WlSeat,
        e: wl_seat::Event,
        id: &u32,
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let Some(local) = s.seats.iter_mut().find(|seat| seat.id == *id)
            && let wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(c),
            } = e
        {
            if c.contains(wl_seat::Capability::Pointer) {
                if local.pointer.is_none() {
                    local.pointer = Some(seat.get_pointer(q, *id));
                }
            } else {
                local.grab = None;
                local.focus = None;
                if let Some(p) = local.pointer.take() {
                    p.release();
                }
                if let Some(a) = s.active.as_mut() {
                    a.outcome = Some(Outcome::Cancelled);
                }
            }
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, u32> for State {
    fn event(
        s: &mut Self,
        _: &wl_pointer::WlPointer,
        e: wl_pointer::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(seat) = s.seats.iter_mut().find(|seat| seat.id == *id) else {
            return;
        };
        match e {
            wl_pointer::Event::Enter { surface, .. } => seat.focus = Some(surface),
            wl_pointer::Event::Leave { .. } => {
                seat.focus = None;
                seat.grab = None;
                seat.buttons.clear();
            }
            wl_pointer::Event::Button {
                serial,
                button,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => {
                // Only the first button owns the implicit grab serial. A later left press cannot replace it.
                if seat.buttons.is_empty() && button == 272 {
                    seat.grab = seat.focus.clone().map(|surface| (serial, surface));
                }
                seat.buttons.insert(button);
            }
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(wl_pointer::ButtonState::Released),
                ..
            } => {
                seat.buttons.remove(&button);
                if button == 272 {
                    seat.grab = None;
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        s: &mut Self,
        source: &wl_data_source::WlDataSource,
        e: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(a) = s.active.as_mut().filter(|a| a.source == *source) else {
            return;
        };
        match e {
            wl_data_source::Event::Target { mime_type } => {
                a.accepted = mime_type.as_deref() == Some("text/uri-list")
            }
            wl_data_source::Event::Action { dnd_action } => {
                a.accepted = matches!(
                    dnd_action,
                    WEnum::Value(wl_data_device_manager::DndAction::Copy)
                )
            }
            wl_data_source::Event::Cancelled => a.outcome = Some(Outcome::Cancelled),
            wl_data_source::Event::DndDropPerformed => {
                a.dropped = true;
                a.deadline = Some(Instant::now() + Duration::from_secs(30));
            }
            wl_data_source::Event::DndFinished if a.outcome.is_none() => {
                a.outcome = Some(if a.dropped && a.accepted && a.transfers.is_empty() {
                    Outcome::Copied
                } else {
                    Outcome::Unknown
                })
            }
            wl_data_source::Event::Send { mime_type, fd }
                if mime_type == "text/uri-list" && a.transfers.len() < 8 =>
            {
                match nonblocking(fd) {
                    Ok(file) => a.transfers.push(Transfer {
                        file,
                        offset: 0,
                        deadline: Instant::now() + Duration::from_secs(30),
                    }),
                    Err(e) => {
                        tracing::warn!(%e,"Wayland drag pipe setup failed");
                        a.outcome = Some(Outcome::Unknown);
                    }
                }
            }
            _ => {}
        }
    }
}
fn nonblocking(fd: OwnedFd) -> std::io::Result<std::fs::File> {
    // SAFETY: fd owns a live descriptor; F_GETFL takes no additional argument.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        // SAFETY: the owned descriptor is live and flags came from F_GETFL.
        || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(fd.into())
}
impl Dispatch<wl_data_device::WlDataDevice, u32> for State {
    fn event(
        s: &mut Self,
        _: &wl_data_device::WlDataDevice,
        e: wl_data_device::Event,
        seat_id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(seat) = s.seats.iter_mut().find(|seat| seat.id == *seat_id) else {
            return;
        };
        // Only an active outbound source rejects own-client targets. Inactive offers
        // must not change another data device's incoming drag or clipboard negotiation.
        match e {
            wl_data_device::Event::DataOffer { id } => {
                if let Some(old) = seat.offer.replace(id) {
                    old.destroy();
                }
            }
            wl_data_device::Event::Enter {
                serial,
                id: Some(offer),
                ..
            } if s
                .active
                .as_ref()
                .is_some_and(|active| !active.dropped && active.outcome.is_none()) =>
            {
                offer.accept(serial, None);
                offer.set_actions(
                    wl_data_device_manager::DndAction::None,
                    wl_data_device_manager::DndAction::None,
                );
            }
            wl_data_device::Event::Leave | wl_data_device::Event::Selection { .. } => {
                if let Some(offer) = seat.offer.take() {
                    offer.destroy();
                }
            }
            _ => {}
        }
    }
    wayland_client::event_created_child!(State,wl_data_device::WlDataDevice,[0=>(wl_data_offer::WlDataOffer,())]);
}
wayland_client::delegate_noop!(State:ignore wl_data_device_manager::WlDataDeviceManager);
wayland_client::delegate_noop!(State:ignore wl_data_offer::WlDataOffer);
