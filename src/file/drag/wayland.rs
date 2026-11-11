//! Independent proxies on winit's *same* libwayland client, as in smithay-clipboard.
//! The genuine pointer grab serial and originating surface are observed before handoff.
//! One owner per connection serves every window: the compositor addresses a client's data device,
//! so per-window devices on the same seat would compete for one drag.
mod inbound;
#[cfg(test)]
mod tests;

use super::{InboundEvent, Outcome};
use inbound::{Action, Command, Enter, Gate, Inbound, URI_LIST};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Read, Write},
    os::fd::{AsFd, AsRawFd, OwnedFd},
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
    inbox: Rc<RefCell<Vec<InboundEvent>>>,
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
    /// Native Wayland start: each seat keeps one data device for the owner's lifetime.
    inbound: bool,
    /// Every live offer, keyed by protocol id, so a selection never destroys the drag's offer.
    offers: HashMap<u32, wl_data_offer::WlDataOffer>,
    dnd: Inbound,
    /// Window surfaces of this connection and the events waiting for each.
    windows: HashMap<u32, Rc<RefCell<Vec<InboundEvent>>>>,
}

impl Context {
    pub(super) fn new(
        window: Arc<Window>,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
        inbound: bool,
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
                let owner = Rc::new(RefCell::new(Owner::new(display, display_owner, inbound)?));
                OWNERS.with_borrow_mut(|owners| owners.push((key, Rc::downgrade(&owner))));
                owner
            }
        };
        let origin = id.protocol_id();
        let inbox = Rc::new(RefCell::new(Vec::new()));
        owner
            .borrow_mut()
            .state
            .windows
            .insert(origin, inbox.clone());
        Ok(Some(Self {
            owner,
            origin,
            inbox,
            _window: window,
        }))
    }
    pub(super) fn poll(&mut self) -> Result<Vec<InboundEvent>, String> {
        self.owner.borrow_mut().poll()?;
        Ok(std::mem::take(&mut *self.inbox.borrow_mut()))
    }
    pub(super) fn inbound_pending(&self) -> bool {
        let owner = self.owner.borrow();
        owner.state.dnd.busy()
            || owner
                .state
                .windows
                .values()
                .any(|inbox| !inbox.borrow().is_empty())
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
        // The standing device is shared; otherwise this drag owns a device until it ends.
        let standing = seat.device.clone().filter(|_| state.inbound);
        let device = match standing {
            Some(device) => device,
            None => manager.get_data_device(&seat.seat, &queue.handle(), seat_id),
        };
        let source = manager.create_data_source(&queue.handle(), ());
        source.offer(URI_LIST.into());
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
            if !state.inbound {
                for seat in &mut state.seats {
                    if let Some(device) = seat.device.take() {
                        device.release();
                    }
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
        inbound: bool,
    ) -> Result<Self, String> {
        // SAFETY: OwnedDisplayHandle retains winit's connection even after its event loop has exited.
        let connection = Connection::from_backend(unsafe {
            Backend::from_foreign_display(display.display.as_ptr().cast())
        });
        let mut queue = connection.new_event_queue();
        let q = queue.handle();
        let mut state = State {
            inbound,
            ..State::default()
        };
        connection.display().get_registry(&q, ());
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        bind_inbound_devices(&mut state, &q);
        if inbound && (state.manager.is_none() || state.seats.is_empty()) {
            tracing::warn!(
                "Wayland compositor lacks wl_data_device_manager v3 or wl_seat v5; incoming file drags are off"
            );
        }
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
        self.state.dnd.pump(Instant::now());
        apply_inbound(&mut self.state);
        self.connection.flush().map_err(|e| e.to_string())
    }
}
/// The only path that creates a data device outside an outbound drag, and only on a native
/// Wayland start. X11 and XWayland never reach here with `inbound` set.
fn bind_inbound_devices(s: &mut State, q: &QueueHandle<State>) {
    if !s.inbound {
        return;
    }
    let Some(manager) = s.manager.as_ref() else {
        return;
    };
    for seat in s.seats.iter_mut().filter(|seat| seat.device.is_none()) {
        seat.device = Some(manager.get_data_device(&seat.seat, q, seat.id));
    }
}
/// Sends the state machine's requests to the real offers and hands its events to the windows.
fn apply_inbound(s: &mut State) {
    loop {
        let commands = s.dnd.take_commands();
        if commands.is_empty() {
            break;
        }
        for command in commands {
            match command {
                Command::Accept {
                    offer,
                    serial,
                    uri_list,
                } => {
                    if let Some(proxy) = s.offers.get(&offer) {
                        proxy.accept(serial, uri_list.then(|| URI_LIST.to_string()));
                    }
                }
                Command::SetActions { offer, copy } => {
                    if let Some(proxy) = s.offers.get(&offer) {
                        let action = if copy {
                            wl_data_device_manager::DndAction::Copy
                        } else {
                            wl_data_device_manager::DndAction::None
                        };
                        proxy.set_actions(action, action);
                    }
                }
                Command::Receive { offer } => {
                    let pipe = s
                        .offers
                        .get(&offer)
                        .ok_or_else(|| std::io::Error::other("offer already destroyed"))
                        .and_then(receive);
                    s.dnd.attach(offer, pipe);
                }
                Command::Finish { offer } => {
                    if let Some(proxy) = s.offers.get(&offer) {
                        proxy.finish();
                    }
                }
                Command::Destroy { offer } => {
                    if let Some(proxy) = s.offers.remove(&offer) {
                        proxy.destroy();
                    }
                }
            }
        }
    }
    for (surface, event) in s.dnd.take_events() {
        if let Some(inbox) = s.windows.get(&surface) {
            inbox.borrow_mut().push(event);
        }
    }
}
fn receive(offer: &wl_data_offer::WlDataOffer) -> std::io::Result<Box<dyn Read>> {
    let (reader, writer) = std::io::pipe()?;
    // libwayland duplicates the descriptor when marshalling, so our write end closes here.
    offer.receive(URI_LIST.into(), writer.as_fd());
    drop(writer);
    Ok(Box::new(nonblocking(reader.into())?))
}
impl Drop for Context {
    fn drop(&mut self) {
        let mut owner = self.owner.borrow_mut();
        let windows = &mut owner.state.windows;
        if windows
            .get(&self.origin)
            .is_some_and(|inbox| Rc::ptr_eq(inbox, &self.inbox))
        {
            windows.remove(&self.origin);
        }
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
        let state = &mut owner.state;
        if let Some(active) = state.active.take() {
            active.source.destroy();
        }
        for seat in &mut state.seats {
            seat.grab = None;
            seat.buttons.clear();
        }
        // The standing device and its offers outlive any one outbound drag.
        if !state.inbound {
            state.dnd.clear();
            for seat in &mut state.seats {
                if let Some(device) = seat.device.take() {
                    device.release();
                }
            }
        }
        apply_inbound(state);
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
        for (_, offer) in self.state.offers.drain() {
            offer.destroy();
        }
        for seat in &mut self.state.seats {
            if let Some(p) = seat.pointer.take() {
                p.release();
            }
            if let Some(d) = seat.device.take() {
                d.release();
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
                s.manager = Some(r.bind(name, 3, q, ()));
                bind_inbound_devices(s, q);
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
                    buttons: Default::default(),
                });
                bind_inbound_devices(s, q);
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
                    s.dnd.cancel();
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
        _: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            wl_data_device::Event::DataOffer { id } => {
                let offer = id.id().protocol_id();
                s.dnd.data_offer(offer);
                if let Some(old) = s.offers.insert(offer, id) {
                    old.destroy();
                }
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                let surface = surface.id().protocol_id();
                let gate = Gate {
                    inbound: s.inbound,
                    known_surface: s.windows.contains_key(&surface),
                    // Only an active outbound source rejects own-client targets.
                    outbound_active: s
                        .active
                        .as_ref()
                        .is_some_and(|active| !active.dropped && active.outcome.is_none()),
                };
                let enter = Enter {
                    serial,
                    surface,
                    at: (x, y),
                    offer: id.map(|offer| offer.id().protocol_id()),
                };
                s.dnd.enter(enter, gate, Instant::now());
            }
            wl_data_device::Event::Motion { x, y, .. } => s.dnd.motion((x, y)),
            wl_data_device::Event::Leave => s.dnd.leave(),
            wl_data_device::Event::Drop => s.dnd.drop_here(Instant::now()),
            wl_data_device::Event::Selection { id } => {
                s.dnd.selection(id.map(|offer| offer.id().protocol_id()));
            }
            _ => {}
        }
    }
    wayland_client::event_created_child!(State,wl_data_device::WlDataDevice,[0=>(wl_data_offer::WlDataOffer,())]);
}
impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        s: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        e: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = offer.id().protocol_id();
        match e {
            wl_data_offer::Event::Offer { mime_type } => s.dnd.offer_mime(id, mime_type),
            wl_data_offer::Event::Action { dnd_action } => s.dnd.offer_action(
                id,
                match dnd_action {
                    WEnum::Value(action) if action.is_empty() => Action::None,
                    WEnum::Value(wl_data_device_manager::DndAction::Copy) => Action::Copy,
                    _ => Action::Other,
                },
            ),
            _ => {}
        }
    }
}
wayland_client::delegate_noop!(State:ignore wl_data_device_manager::WlDataDeviceManager);
