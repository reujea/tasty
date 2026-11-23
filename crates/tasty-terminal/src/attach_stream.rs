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
    /// The subscriber has taken over the user-facing effects of the bytes it receives.
    effects_owner: AtomicBool,
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
impl AttachEventReceiver {
    /// Records whether the subscriber handles the user-facing effects (clipboard, sound) of
    /// the bytes this subscription carries. The terminal reads it at each ingest.
    pub fn set_effects_owner(&self, owner: bool) {
        self.state.effects_owner.store(owner, Ordering::Release);
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

    /// A lost subscription still holds the effects of the bytes it missed until its receiver
    /// closes, so a split sequence finishing in a later chunk does not fall back to the server.
    fn keeps_effects(&self) -> bool {
        self.state.lost.load(Ordering::Acquire) && self.owns_effects()
    }

    fn owns_effects(&self) -> bool {
        self.is_live() && self.state.effects_owner.load(Ordering::Acquire)
    }

    /// Called only under the terminal's parser lock. There is one producer per subscription;
    /// the consumer can release bytes concurrently, but cannot insert events or resume a loss.
    fn send(&self, output: Option<&[u8]>, cols: usize, rows: usize) -> bool {
        if !self.is_live() || self.state.lost.load(Ordering::Acquire) {
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

/// 시퀀스 경계만 보려고 파서 상태를 돌린다. 동작은 termwiz 파서가 맡는다.
struct StateOnly;
impl vtparse::VTActor for StateOnly {
    fn print(&mut self, _: char) {}
    fn execute_c0_or_c1(&mut self, _: u8) {}
    fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
    fn dcs_put(&mut self, _: u8) {}
    fn dcs_unhook(&mut self) {}
    fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: u8) {}
    fn csi_dispatch(&mut self, _: &[vtparse::CsiParam], _: bool, _: u8) {}
    fn osc_dispatch(&mut self, _: &[&[u8]]) {}
    fn apc_dispatch(&mut self, _: Vec<u8>) {}
}

impl TerminalState {
    /// 큐가 넘쳐 바이트를 잃은 스트림도 수신자가 닫힐 때까지 효과를 맡는다(손실 뒤 재동기화가 화면만
    /// 복구한다).
    pub(crate) fn fan_out_attach_output(&mut self, bytes: &[u8]) {
        self.attach_streams
            .retain(|tap| tap.send(Some(bytes), 0, 0) || tap.keeps_effects());
    }
    /// 앞에서부터 효과 소유자 몫인 바이트 수. 소유자가 있으면 전부다. 소유자가 떠났으면 그가 받은
    /// 바이트로 시작해 아직 끝나지 않은 시퀀스의 끝(그 바이트 포함)까지다. 그래서 앞부분을 client에
    /// 넘긴 시퀀스를 서버가 이어 받아 효과를 내지 않는다.
    pub(crate) fn effects_held_prefix(&mut self, bytes: &[u8], owned: bool) -> usize {
        if owned {
            let scan = self.effects_scan.get_or_insert_with(vtparse::VTParser::new);
            scan.parse(bytes, &mut StateOnly);
            return bytes.len();
        }
        let Some(scan) = self.effects_scan.as_mut() else {
            return 0;
        };
        let held = bytes
            .iter()
            .position(|&byte| {
                let ended = scan.is_ground();
                if !ended {
                    scan.parse_byte(byte, &mut StateOnly);
                }
                ended
            })
            .unwrap_or(bytes.len());
        if scan.is_ground() {
            self.effects_scan = None;
        }
        held
    }
    pub(crate) fn has_effects_owner_stream(&self) -> bool {
        self.attach_streams
            .iter()
            .any(AttachStreamTap::owns_effects)
    }
    pub(crate) fn fan_out_attach_resize(&mut self, cols: usize, rows: usize) {
        self.attach_streams
            .retain(|tap| tap.send(None, cols, rows) || tap.keeps_effects());
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

    /// A subscriber that declared it handles terminal effects is attached now.
    pub fn has_effects_owner_stream(&self) -> bool {
        self.lock_state().has_effects_owner_stream()
    }
}

#[cfg(test)]
#[path = "attach_stream_tests.rs"]
mod tests;
