//! Ordered, bounded attach output. One parser lock orders the snapshot, output and grid resize.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use crate::{Terminal, TerminalState};

/// Maximum queued events for one subscription; loss notification needs no queue slot.
pub const ATTACH_STREAM_MAX_EVENTS: usize = 256;
/// Includes queued output bytes and a fixed charge for every queued event.
pub const ATTACH_STREAM_MAX_BYTES: usize = 1024 * 1024;
const EVENT_CHARGE: usize = std::mem::size_of::<AttachEvent>();

#[derive(Debug)]
pub enum AttachEvent {
    Output(Vec<u8>),
    /// The grid changed at this point in the output sequence. This is not an OS resize ACK.
    Resize {
        cols: usize,
        rows: usize,
    },
    /// The prefix is complete, but its next event could not be retained. Resubscribe with a
    /// fresh snapshot; this subscription never resumes and never silently skips an event.
    Loss,
}
impl AttachEvent {
    fn weight(&self) -> usize {
        EVENT_CHARGE
            + match self {
                Self::Output(bytes) => bytes.len(),
                _ => 0,
            }
    }
}

#[derive(Default)]
struct QueueState {
    bytes: AtomicUsize,
    lost: AtomicBool,
    receiver_closed: AtomicBool,
}

/// Single consumer. Dropping it releases queued bytes and disables the producer without
/// acquiring the parser lock. A later ingest, resize or subscription removes the inert sender.
pub struct AttachEventReceiver {
    receiver: mpsc::Receiver<AttachEvent>,
    state: Arc<QueueState>,
    ended: bool,
}
impl AttachEventReceiver {
    /// Delivers retained events in order, then exactly one Loss on overflow, then Disconnected.
    /// A normal terminal/producer drop drains the prefix and reports Disconnected without Loss.
    pub fn try_recv(&mut self) -> Result<AttachEvent, mpsc::TryRecvError> {
        if self.ended {
            return Err(mpsc::TryRecvError::Disconnected);
        }
        match self.receiver.try_recv() {
            Ok(event) => {
                self.state.bytes.fetch_sub(event.weight(), Ordering::AcqRel);
                Ok(event)
            }
            Err(error) => {
                if self.state.lost.load(Ordering::Acquire) {
                    // The initial Empty may precede a final successful enqueue. After acquiring
                    // loss, the sole producer cannot append again: recheck that complete prefix.
                    // Keep loss sticky while returning it, so later polls drain every queued event.
                    match self.receiver.try_recv() {
                        Ok(event) => {
                            self.state.bytes.fetch_sub(event.weight(), Ordering::AcqRel);
                            Ok(event)
                        }
                        Err(_) => {
                            self.ended = true;
                            Ok(AttachEvent::Loss)
                        }
                    }
                } else {
                    if error == mpsc::TryRecvError::Disconnected {
                        self.ended = true;
                    }
                    Err(error)
                }
            }
        }
    }
}
impl Drop for AttachEventReceiver {
    fn drop(&mut self) {
        self.state.receiver_closed.store(true, Ordering::Release);
    }
}

pub struct AttachStreamSubscription {
    pub snapshot: Vec<u8>,
    /// Dimensions captured under the same parser lock as the snapshot and subscription.
    pub cols: usize,
    pub rows: usize,
    pub events: AttachEventReceiver,
}

pub(crate) struct AttachStreamTap {
    sender: mpsc::SyncSender<AttachEvent>,
    state: Arc<QueueState>,
}
impl AttachStreamTap {
    fn is_live(&self) -> bool {
        !self.state.receiver_closed.load(Ordering::Acquire)
    }

    /// Called only under the terminal's parser lock. There is one producer per subscription;
    /// the consumer can release bytes concurrently, but cannot insert events or resume a loss.
    fn send(&self, output: Option<&[u8]>, cols: usize, rows: usize) -> bool {
        if !self.is_live() {
            return false;
        }
        let Some(weight) = output.map_or(Some(EVENT_CHARGE), |bytes| {
            bytes.len().checked_add(EVENT_CHARGE)
        }) else {
            self.state.lost.store(true, Ordering::Release);
            return false;
        };
        if self
            .state
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(weight)
                    .filter(|total| *total <= ATTACH_STREAM_MAX_BYTES)
            })
            .is_err()
        {
            self.state.lost.store(true, Ordering::Release);
            return false;
        }
        // Reject oversized chunks before allocating a per-subscriber copy.
        let event = match output {
            Some(bytes) => AttachEvent::Output(bytes.to_vec()),
            None => AttachEvent::Resize { cols, rows },
        };
        match self.sender.try_send(event) {
            Ok(()) => true,
            Err(error) => {
                self.state.bytes.fetch_sub(weight, Ordering::AcqRel);
                if matches!(error, mpsc::TrySendError::Full(_)) {
                    self.state.lost.store(true, Ordering::Release);
                }
                false
            }
        }
    }
}

impl TerminalState {
    /// 살아 있는 스트림 하나라도 바이트를 받았으면 true다.
    pub(crate) fn fan_out_attach_output(&mut self, bytes: &[u8]) -> bool {
        let mut delivered = false;
        self.attach_streams.retain(|tap| {
            let sent = tap.send(Some(bytes), 0, 0);
            delivered |= sent;
            sent
        });
        delivered
    }
    pub(crate) fn fan_out_attach_resize(&mut self, cols: usize, rows: usize) {
        self.attach_streams.retain(|tap| tap.send(None, cols, rows));
    }
    fn snapshot_and_stream(&mut self) -> AttachStreamSubscription {
        self.attach_streams.retain(AttachStreamTap::is_live);
        let snapshot = self.snapshot_as_vt();
        let (sender, receiver) = mpsc::sync_channel(ATTACH_STREAM_MAX_EVENTS);
        let state = Arc::new(QueueState::default());
        self.attach_streams.push(AttachStreamTap {
            sender,
            state: state.clone(),
        });
        AttachStreamSubscription {
            snapshot,
            cols: self.cols,
            rows: self.rows,
            events: AttachEventReceiver {
                receiver,
                state,
                ended: false,
            },
        }
    }
}
impl Terminal {
    /// Snapshot and ordered output/resize subscription share one parser-lock cut. This is an
    /// execution capability: immutable terminal display access cannot create subscriptions.
    pub fn snapshot_and_stream(&mut self) -> AttachStreamSubscription {
        self.lock_state().snapshot_and_stream()
    }
}

#[cfg(test)]
#[path = "attach_stream_tests.rs"]
mod tests;
