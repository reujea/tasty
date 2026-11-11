//! Incoming file drags on the standing data device, kept free of protocol objects so every event
//! order can be tested. The owner applies the returned commands to the real `wl_data_offer`s.
//!
//! A selection offer never takes part in a drag: its mime list is handed to the text clipboard
//! (`selection`), which never sends it accept or set_actions.
use crate::file::drag::InboundEvent;
use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) const URI_LIST: &str = "text/uri-list";
/// Same bound as an outbound transfer waiting for its receiver.
const READ_LIMIT: Duration = Duration::from_secs(30);

/// Surface-local logical position from `wl_data_device`.
pub(super) type Logical = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    /// `uri_list: false` is the NULL mime, which also forbids a later `finish`.
    Accept {
        offer: u32,
        serial: u32,
        uri_list: bool,
    },
    SetActions {
        offer: u32,
        copy: bool,
    },
    Receive {
        offer: u32,
    },
    Finish {
        offer: u32,
    },
    Destroy {
        offer: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    None,
    Copy,
    Other,
}

#[derive(Default)]
struct Offer {
    mimes: Vec<String>,
    action: Option<Action>,
}

/// What the device was told when the drag entered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Enter {
    pub(super) serial: u32,
    pub(super) surface: u32,
    pub(super) at: Logical,
    pub(super) offer: Option<u32>,
}

/// Facts outside this state machine that decide whether an entering drag is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Gate {
    /// The standing device exists (native Wayland start).
    pub(super) inbound: bool,
    /// The entered surface is one of this process's windows.
    pub(super) known_surface: bool,
    /// This process is dragging out; its own offer must never be taken.
    pub(super) outbound_active: bool,
}

struct Dnd {
    offer: u32,
    serial: u32,
    surface: u32,
    at: Logical,
    accepted: bool,
    pipe: Option<Box<dyn Read>>,
    bytes: Vec<u8>,
    deadline: Instant,
    paths: Option<Vec<PathBuf>>,
    hovered: bool,
    /// Fixed at the drop so later motion or a late read cannot move the target.
    dropped_at: Option<Logical>,
}

#[derive(Default)]
pub(super) struct Inbound {
    offers: HashMap<u32, Offer>,
    dnd: Option<Dnd>,
    commands: Vec<Command>,
    events: Vec<(u32, InboundEvent)>,
}

/// Whether an entering drag is taken: a file list from another client over one of our windows.
pub(super) fn takes(gate: Gate, mimes: &[String]) -> bool {
    gate.inbound
        && gate.known_surface
        && !gate.outbound_active
        && mimes.iter().any(|mime| mime == URI_LIST)
}

/// `finish` is valid only for an accepted mime and a final Copy action (wl_data_offer.finish).
pub(super) fn can_finish(accepted: bool, action: Option<Action>) -> bool {
    accepted && action == Some(Action::Copy)
}

impl Inbound {
    pub(super) fn data_offer(&mut self, offer: u32) {
        self.offers.insert(offer, Offer::default());
    }
    pub(super) fn offer_mime(&mut self, offer: u32, mime: String) {
        if let Some(entry) = self.offers.get_mut(&offer) {
            entry.mimes.push(mime);
        }
    }
    pub(super) fn offer_action(&mut self, offer: u32, action: Action) {
        if let Some(entry) = self.offers.get_mut(&offer) {
            entry.action = Some(action);
        }
    }

    pub(super) fn enter(&mut self, enter: Enter, gate: Gate, now: Instant) {
        self.end();
        let Some(offer) = enter.offer else {
            return;
        };
        let accepted = self
            .offers
            .get(&offer)
            .is_some_and(|entry| takes(gate, &entry.mimes));
        if accepted {
            self.commands.extend([
                Command::Accept {
                    offer,
                    serial: enter.serial,
                    uri_list: true,
                },
                Command::SetActions { offer, copy: true },
                Command::Receive { offer },
            ]);
        } else if gate.inbound || gate.outbound_active {
            self.reject(offer, enter.serial);
        }
        // Without the standing device and an outbound drag, the offer is left untouched.
        self.dnd = Some(Dnd {
            offer,
            serial: enter.serial,
            surface: enter.surface,
            at: enter.at,
            accepted,
            pipe: None,
            bytes: Vec::new(),
            deadline: now + READ_LIMIT,
            paths: None,
            hovered: false,
            dropped_at: None,
        });
    }

    /// Hands over the read end for a `Receive`. A result for an offer that is no longer entered
    /// belongs to an earlier enter and is dropped.
    pub(super) fn attach(&mut self, offer: u32, pipe: std::io::Result<Box<dyn Read>>) {
        let Some(dnd) = self.dnd.as_mut().filter(|dnd| dnd.offer == offer) else {
            return;
        };
        match pipe {
            Ok(pipe) => dnd.pipe = Some(pipe),
            Err(error) => self.fail(&error.to_string()),
        }
    }

    pub(super) fn motion(&mut self, at: Logical) {
        let Some(dnd) = self.dnd.as_mut().filter(|dnd| dnd.dropped_at.is_none()) else {
            return;
        };
        dnd.at = at;
        if dnd.hovered {
            self.events.push((dnd.surface, InboundEvent::Motion { at }));
        }
    }

    pub(super) fn leave(&mut self) {
        // Some compositors send leave after drop; a dropped offer is finished by `settle`.
        if self
            .dnd
            .as_ref()
            .is_some_and(|dnd| dnd.dropped_at.is_none())
        {
            self.end();
        }
    }

    pub(super) fn drop_here(&mut self, now: Instant) {
        let Some(dnd) = self.dnd.as_mut().filter(|dnd| dnd.dropped_at.is_none()) else {
            return;
        };
        dnd.dropped_at = Some(dnd.at);
        self.pump(now);
    }

    /// Hands a selection offer's mime list over to the clipboard and stops tracking it. `None` for
    /// the offer of the current drag, which is never a selection.
    pub(super) fn take_selection(&mut self, offer: u32) -> Option<Vec<String>> {
        if self.dnd.as_ref().is_some_and(|dnd| dnd.offer == offer) {
            return None;
        }
        Some(
            self.offers
                .remove(&offer)
                .map(|o| o.mimes)
                .unwrap_or_default(),
        )
    }

    /// Ends the current drag as if it left, e.g. when its seat disappears.
    pub(super) fn cancel(&mut self) {
        self.end();
    }

    /// Destroys every offer still held. Used when the outbound-only device is released.
    pub(super) fn clear(&mut self) {
        self.end();
        let offers: Vec<u32> = self.offers.drain().map(|(offer, _)| offer).collect();
        self.commands
            .extend(offers.into_iter().map(|offer| Command::Destroy { offer }));
    }

    /// Reads what the pipe has without blocking, then hovers or drops once the list is complete.
    pub(super) fn pump(&mut self, now: Instant) {
        let Some(dnd) = self.dnd.as_mut() else {
            return;
        };
        if let Some(pipe) = dnd.pipe.as_mut() {
            let mut buf = [0u8; 32 * 1024];
            let failure = loop {
                match pipe.read(&mut buf) {
                    Ok(0) => {
                        dnd.pipe = None;
                        match crate::file::drag::payload::parse_uri_list(&dnd.bytes) {
                            Ok(paths) => dnd.paths = Some(paths),
                            Err(error) => break Some(error),
                        }
                        break None;
                    }
                    Ok(n) => {
                        dnd.bytes.extend_from_slice(&buf[..n]);
                        if dnd.bytes.len() > crate::file::drag::payload::MAX_BYTES {
                            break Some("incoming file list exceeds 1 MiB".to_string());
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if now >= dnd.deadline {
                            break Some("incoming file list timed out".to_string());
                        }
                        break None;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => break Some(error.to_string()),
                }
            };
            if let Some(reason) = failure {
                self.fail(&reason);
            }
        }
        self.settle();
    }

    pub(super) fn busy(&self) -> bool {
        self.dnd.is_some()
    }

    pub(super) fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }
    pub(super) fn take_events(&mut self) -> Vec<(u32, InboundEvent)> {
        std::mem::take(&mut self.events)
    }

    fn settle(&mut self) {
        let Some(dnd) = self.dnd.as_mut() else {
            return;
        };
        let Some(at) = dnd.dropped_at else {
            if !dnd.hovered
                && let Some(paths) = dnd.paths.clone()
            {
                dnd.hovered = true;
                let at = dnd.at;
                self.events
                    .push((dnd.surface, InboundEvent::Hover { paths, at }));
            }
            return;
        };
        let action = self.offers.get(&dnd.offer).and_then(|offer| offer.action);
        if !can_finish(dnd.accepted, action) {
            self.end();
            return;
        }
        let Some(paths) = dnd.paths.take() else {
            return;
        };
        let Some(dnd) = self.dnd.take() else {
            return;
        };
        self.offers.remove(&dnd.offer);
        self.events
            .push((dnd.surface, InboundEvent::Drop { paths, at }));
        self.commands.extend([
            Command::Finish { offer: dnd.offer },
            Command::Destroy { offer: dnd.offer },
        ]);
    }

    /// Stops taking this drag. Before the drop the offer stays alive for the compositor's leave.
    fn fail(&mut self, reason: &str) {
        tracing::warn!(reason, "incoming Wayland file drag ignored");
        let Some(dnd) = self.dnd.as_mut() else {
            return;
        };
        if dnd.dropped_at.is_some() {
            self.end();
            return;
        }
        dnd.pipe = None;
        dnd.paths = None;
        if dnd.accepted {
            dnd.accepted = false;
            let (offer, serial) = (dnd.offer, dnd.serial);
            if std::mem::take(&mut dnd.hovered) {
                self.events.push((dnd.surface, InboundEvent::Leave));
            }
            self.reject(offer, serial);
        }
    }

    fn reject(&mut self, offer: u32, serial: u32) {
        self.commands.extend([
            Command::Accept {
                offer,
                serial,
                uri_list: false,
            },
            Command::SetActions { offer, copy: false },
        ]);
    }

    /// Forgets the drag without finishing: leave, refusal or failure.
    fn end(&mut self) {
        let Some(dnd) = self.dnd.take() else {
            return;
        };
        if dnd.hovered {
            self.events.push((dnd.surface, InboundEvent::Leave));
        }
        self.offers.remove(&dnd.offer);
        self.commands.push(Command::Destroy { offer: dnd.offer });
    }
}

#[cfg(test)]
#[path = "inbound_tests.rs"]
mod tests;
