//! The text clipboard on the shared owner's standing data device. It exists only on a native
//! Wayland start, where egui-winit (built without smithay-clipboard) has no system clipboard.
use super::selection::{Command, Paste, TEXT_MIMES, Target};
use super::{Owner, State, nonblocking, receive_as, shared_owner};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_data_source, wl_keyboard},
};
use winit::window::Window;

/// User data that tells our selection sources apart from the outbound drag source.
pub(super) struct SelectionSource;

/// One window's view of the shared selection. Pastes it asks for come back only to it.
pub(in crate::file::drag) struct Clipboard {
    pub(super) owner: Rc<RefCell<Owner>>,
    client: u64,
}

impl Clipboard {
    /// `None` unless `inbound` (the native Wayland start) and the window is a Wayland one. A
    /// screen without an owned display handle (its own short event loop that never polls this
    /// queue) keeps egui's in-process clipboard.
    pub(in crate::file::drag) fn new(
        window: &Window,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
        inbound: bool,
    ) -> Result<Option<Self>, String> {
        if !inbound || display_owner.is_none() {
            return Ok(None);
        }
        let Some((owner, _)) = shared_owner(window, display_owner, inbound)? else {
            return Ok(None);
        };
        let ready = {
            let owner = owner.borrow();
            owner.state.manager.is_some() && owner.state.seats.iter().any(|s| s.device.is_some())
        };
        if !ready {
            return Ok(None);
        }
        let client = {
            let mut owner = owner.borrow_mut();
            owner.state.next_client += 1;
            owner.state.next_client
        };
        Ok(Some(Self { owner, client }))
    }

    /// Makes `text` the selection, using the latest keyboard or pointer serial this client got.
    pub(in crate::file::drag) fn copy(&mut self, text: String) -> Result<(), String> {
        let mut owner = self.owner.borrow_mut();
        owner.poll()?;
        let Owner {
            state,
            queue,
            connection,
            ..
        } = &mut *owner;
        let (seat_id, serial) = state
            .serial
            .ok_or("no keyboard or pointer input has reached this client yet")?;
        let device = state
            .seats
            .iter()
            .find(|seat| seat.id == seat_id)
            .and_then(|seat| seat.device.as_ref())
            .ok_or("the input seat has no data device")?;
        let manager = state.manager.as_ref().ok_or("no data-device manager")?;
        let source = manager.create_data_source(&queue.handle(), SelectionSource);
        for mime in TEXT_MIMES {
            source.offer(mime.into());
        }
        device.set_selection(Some(&source), serial);
        let id = source.id().protocol_id();
        if let Some(old) = state.own_source.replace(source) {
            old.destroy();
        }
        state.clip.own(id, Arc::from(text));
        connection.flush().map_err(|e| e.to_string())
    }

    /// Starts a paste for `tag` (the caller's input target).
    pub(in crate::file::drag) fn request(&mut self, tag: u64) -> Result<Paste, String> {
        let mut owner = self.owner.borrow_mut();
        owner.poll()?;
        let target = Target {
            client: self.client,
            tag,
        };
        let paste = owner.state.clip.request(target, Instant::now());
        owner.poll()?;
        Ok(paste)
    }

    pub(in crate::file::drag) fn cancel(&mut self) {
        self.owner.borrow_mut().state.clip.cancel(self.client);
    }

    /// Runs the shared queue and returns this handle's finished pastes as `(tag, text)`.
    pub(in crate::file::drag) fn poll(&mut self) -> Result<Vec<(u64, String)>, String> {
        let mut owner = self.owner.borrow_mut();
        owner.poll()?;
        Ok(owner.state.clip.take_results(self.client))
    }

    pub(in crate::file::drag) fn busy(&self) -> bool {
        self.owner.borrow().state.clip.busy()
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        self.owner.borrow_mut().state.clip.forget(self.client);
    }
}

/// Sends the selection machine's requests to the real offers.
pub(super) fn apply_selection(s: &mut State) {
    for command in s.clip.take_commands() {
        match command {
            Command::Receive { offer, mime } => {
                let pipe = s
                    .offers
                    .get(&offer)
                    .ok_or_else(|| std::io::Error::other("offer already destroyed"))
                    .and_then(|proxy| receive_as(proxy, mime));
                s.clip.attach(offer, pipe);
            }
            Command::Destroy { offer } => {
                if let Some(proxy) = s.offers.remove(&offer) {
                    proxy.destroy();
                }
            }
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, SelectionSource> for State {
    fn event(
        s: &mut Self,
        source: &wl_data_source::WlDataSource,
        e: wl_data_source::Event,
        _: &SelectionSource,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = source.id().protocol_id();
        match e {
            wl_data_source::Event::Send { mime_type, fd } => match nonblocking(fd) {
                Ok(file) => s.clip.send(id, &mime_type, Box::new(file), Instant::now()),
                Err(error) => tracing::warn!(%error, "clipboard pipe setup failed"),
            },
            wl_data_source::Event::Cancelled => {
                if s.clip.cancelled(id)
                    && let Some(own) = s.own_source.take()
                {
                    own.destroy();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, u32> for State {
    fn event(
        s: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        e: wl_keyboard::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The keymap descriptor is closed when the event drops.
        match e {
            wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } => {
                s.serial = Some((*id, serial));
            }
            _ => {}
        }
    }
}
