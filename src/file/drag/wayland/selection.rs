//! Text clipboard (Wayland `selection`) on the standing data device, kept free of protocol objects
//! so every order can be tested. The owner applies the returned commands to the real offers.
//!
//! Only the latest foreign selection offer is kept; it is read only when a paste asks for it, never
//! ahead of time. Text this process copied is served from memory while its source is current.
//! egui text fields ask for text only; the terminal also takes a PNG when the offer has no text.
use std::{
    io::{Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};

/// Offered for our own text and accepted for a foreign one, in this order of preference.
pub(super) const TEXT_MIMES: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// A paste that has not arrived by then is dropped; the key press is not retried.
const READ_LIMIT: Duration = crate::file::drag::PASTE_LIMIT;
/// Same bound as an outbound drag transfer waiting for its receiver.
const WRITE_LIMIT: Duration = Duration::from_secs(30);
/// Larger clipboard text is refused rather than handed to a text field.
pub(super) const MAX_TEXT: usize = 16 * 1024 * 1024;
/// The image a terminal paste accepts when the offer carries no text.
pub(super) const PNG_MIME: &str = "image/png";
/// A screenshot PNG is larger than text; still refused before it can exhaust memory.
pub(super) const MAX_PNG: usize = 64 * 1024 * 1024;
/// Receivers served at once; a further request is closed unanswered.
const MAX_SENDS: usize = 8;

/// Who asked for a paste: the clipboard handle and the caller's tag for its input target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) client: u64,
    pub(super) tag: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Receive { offer: u32, mime: &'static str },
    Destroy { offer: u32 },
}

/// What a paste accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Want {
    /// egui text fields.
    Text,
    /// The terminal: text first, a PNG only when the offer has no text (as its X11 path does).
    TextOrPng,
}

/// A finished read of a foreign selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Received {
    Text(String),
    Png(Vec<u8>),
    /// Timed out, the pipe failed, it was too large, or the selection changed while reading.
    Failed(String),
}

/// The answer to a paste request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Paste {
    /// This process owns the selection; the text is known now.
    Ready(String),
    /// A foreign selection is being read; the text arrives through `take_results`.
    Pending,
    /// No text selection to paste.
    Empty,
}

struct Reading {
    offer: u32,
    target: Target,
    mime: &'static str,
    pipe: Option<Box<dyn Read>>,
    bytes: Vec<u8>,
    deadline: Instant,
}

/// What our source serves: its text, and a PNG when a capture published one with it.
struct Own {
    source: u32,
    text: Arc<str>,
    png: Option<Arc<[u8]>>,
}

#[derive(Clone)]
enum Served {
    Text(Arc<str>),
    Png(Arc<[u8]>),
}
impl Served {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Png(png) => png,
        }
    }
}

struct Sending {
    out: Box<dyn Write>,
    data: Served,
    offset: usize,
    deadline: Instant,
}

#[derive(Default)]
pub(super) struct Selection {
    /// The latest foreign selection offer and its mime list.
    offer: Option<(u32, Vec<String>)>,
    /// Our current source and what it serves.
    own: Option<Own>,
    reading: Option<Reading>,
    sending: Vec<Sending>,
    commands: Vec<Command>,
    results: Vec<(Target, Received)>,
}

/// The preferred text mime a foreign offer carries.
pub(super) fn text_mime(mimes: &[String]) -> Option<&'static str> {
    TEXT_MIMES
        .into_iter()
        .find(|wanted| mimes.iter().any(|mime| mime == wanted))
}

/// The mime a paste of `want` reads from a foreign offer.
fn wanted_mime(mimes: &[String], want: Want) -> Option<&'static str> {
    text_mime(mimes).or_else(|| {
        (want == Want::TextOrPng && mimes.iter().any(|mime| mime == PNG_MIME)).then_some(PNG_MIME)
    })
}

/// Clipboard bytes as text field input: invalid UTF-8 is replaced and CRLF becomes LF, as egui-winit
/// does for its own clipboard.
pub(super) fn paste_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

impl Selection {
    /// The compositor announced a new selection (or none). The previous offer is destroyed and a
    /// read still waiting for it is dropped, so a paste never mixes two selections.
    pub(super) fn offered(&mut self, offer: Option<(u32, Vec<String>)>) {
        let next = offer.as_ref().map(|(id, _)| *id);
        if let Some((old, _)) = self.offer.take()
            && Some(old) != next
        {
            self.commands.push(Command::Destroy { offer: old });
            if let Some(reading) = self.reading.take_if(|r| r.offer == old) {
                tracing::debug!("clipboard paste dropped: the selection changed while reading");
                self.fail(reading.target, "the selection changed while reading");
            }
        }
        self.offer = offer;
    }

    /// Our new source now holds the selection. Returns the replaced source for the owner to
    /// destroy; transfers it already started keep their text.
    #[cfg(test)]
    pub(super) fn own(&mut self, source: u32, text: Arc<str>) -> Option<u32> {
        self.own_with_png(source, text, None)
    }

    /// Like `own`, also serving `png` as `image/png`.
    pub(super) fn own_with_png(
        &mut self,
        source: u32,
        text: Arc<str>,
        png: Option<Arc<[u8]>>,
    ) -> Option<u32> {
        self.own
            .replace(Own { source, text, png })
            .map(|old| old.source)
    }

    /// The PNG our current source serves; `None` when the selection is another app's or ours
    /// carries text only.
    pub(super) fn own_png(&self) -> Option<Arc<[u8]>> {
        self.own.as_ref().and_then(|own| own.png.clone())
    }

    /// Our source lost the selection. True when it was the current one.
    pub(super) fn cancelled(&mut self, source: u32) -> bool {
        if self.own.as_ref().is_some_and(|own| own.source == source) {
            self.own = None;
            return true;
        }
        false
    }

    /// A receiver asked our source for `mime`. Unknown mimes, stale sources and excess requests
    /// are answered by closing `out` unwritten.
    pub(super) fn send(&mut self, source: u32, mime: &str, out: Box<dyn Write>, now: Instant) {
        let Some(own) = self.own.as_ref().filter(|own| own.source == source) else {
            return;
        };
        let data = if TEXT_MIMES.contains(&mime) {
            Served::Text(own.text.clone())
        } else if let Some(png) = own.png.as_ref().filter(|_| mime == PNG_MIME) {
            Served::Png(png.clone())
        } else {
            return;
        };
        if self.sending.len() >= MAX_SENDS {
            return;
        }
        self.sending.push(Sending {
            out,
            data,
            offset: 0,
            deadline: now + WRITE_LIMIT,
        });
    }

    /// A paste for `target`. A newer request replaces a read still waiting; the replaced target
    /// gets no result (see `waiting`).
    pub(super) fn request(&mut self, target: Target, want: Want, now: Instant) -> Paste {
        self.reading = None;
        if let Some(own) = self.own.as_ref() {
            return Paste::Ready(paste_text(own.text.as_bytes()));
        }
        let Some((offer, mime)) = self
            .offer
            .as_ref()
            .and_then(|(offer, mimes)| wanted_mime(mimes, want).map(|mime| (*offer, mime)))
        else {
            return Paste::Empty;
        };
        self.commands.push(Command::Receive { offer, mime });
        self.reading = Some(Reading {
            offer,
            target,
            mime,
            pipe: None,
            bytes: Vec::new(),
            deadline: now + READ_LIMIT,
        });
        Paste::Pending
    }

    /// Drops a waiting read for `client`, e.g. when its key press was rebound to another action.
    pub(super) fn cancel(&mut self, client: u64) {
        if self
            .reading
            .as_ref()
            .is_some_and(|r| r.target.client == client)
        {
            self.reading = None;
        }
    }

    /// A read for `client` is still waiting for its pipe or its data.
    pub(super) fn waiting(&self, client: u64) -> bool {
        self.reading
            .as_ref()
            .is_some_and(|r| r.target.client == client)
    }

    /// Hands over the read end for a `Receive`. A pipe for a read no longer waiting is closed.
    pub(super) fn attach(&mut self, offer: u32, pipe: std::io::Result<Box<dyn Read>>) {
        let Some(reading) = self.reading.as_mut().filter(|r| r.offer == offer) else {
            return;
        };
        match pipe {
            Ok(pipe) => reading.pipe = Some(pipe),
            Err(error) => {
                tracing::warn!(%error, "clipboard paste could not be read");
                let target = reading.target;
                self.reading = None;
                self.fail(target, &error.to_string());
            }
        }
    }

    fn fail(&mut self, target: Target, reason: &str) {
        self.results
            .push((target, Received::Failed(reason.to_string())));
    }

    /// Reads and writes what the pipes take without blocking.
    pub(super) fn pump(&mut self, now: Instant) {
        self.pump_read(now);
        self.sending.retain_mut(|send| {
            if now >= send.deadline {
                tracing::warn!("clipboard receiver did not take the copied text in time");
                return false;
            }
            loop {
                let rest = &send.data.bytes()[send.offset..];
                if rest.is_empty() {
                    return false;
                }
                match send.out.write(&rest[..rest.len().min(32 * 1024)]) {
                    Ok(0) => return false,
                    Ok(n) => send.offset += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        tracing::debug!(%error, "clipboard receiver closed early");
                        return false;
                    }
                }
            }
        });
    }

    fn pump_read(&mut self, now: Instant) {
        let Some(reading) = self.reading.as_mut() else {
            return;
        };
        // The limit comes first: text that arrived after it is not delivered either.
        if now >= reading.deadline {
            tracing::warn!("clipboard paste timed out");
            let target = reading.target;
            self.reading = None;
            self.fail(target, "the selection owner did not send its data in time");
            return;
        }
        let Some(pipe) = reading.pipe.as_mut() else {
            return;
        };
        let limit = if reading.mime == PNG_MIME {
            MAX_PNG
        } else {
            MAX_TEXT
        };
        let mut buf = [0u8; 32 * 1024];
        let failure = loop {
            match pipe.read(&mut buf) {
                Ok(0) => break None,
                Ok(n) => {
                    reading.bytes.extend_from_slice(&buf[..n]);
                    if reading.bytes.len() > limit {
                        break Some(format!("clipboard data exceeds {} MiB", limit >> 20));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => break Some(error.to_string()),
            }
        };
        let Some(reading) = self.reading.take() else {
            return;
        };
        if let Some(reason) = failure {
            tracing::warn!(reason, "clipboard paste dropped");
            self.fail(reading.target, &reason);
            return;
        }
        let received = if reading.mime == PNG_MIME {
            Received::Png(reading.bytes)
        } else {
            Received::Text(paste_text(&reading.bytes))
        };
        self.results.push((reading.target, received));
    }

    /// A read is waiting or a receiver is still being written to.
    pub(super) fn busy(&self) -> bool {
        self.reading.is_some() || !self.sending.is_empty()
    }

    pub(super) fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    /// Finished reads for `client`, failures included.
    pub(super) fn take_received(&mut self, client: u64) -> Vec<(u64, Received)> {
        let (taken, kept) = std::mem::take(&mut self.results)
            .into_iter()
            .partition(|(target, _)| target.client == client);
        self.results = kept;
        taken
            .into_iter()
            .map(|(target, received)| (target.tag, received))
            .collect()
    }

    /// Finished text pastes for `client`. Failures were logged and an empty text pastes nothing.
    pub(super) fn take_results(&mut self, client: u64) -> Vec<(u64, String)> {
        self.take_received(client)
            .into_iter()
            .filter_map(|(tag, received)| match received {
                Received::Text(text) if !text.is_empty() => Some((tag, text)),
                _ => None,
            })
            .collect()
    }

    /// Forgets results nobody will take, when the handle that asked is gone.
    pub(super) fn forget(&mut self, client: u64) {
        self.cancel(client);
        self.results.retain(|(target, _)| target.client != client);
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
