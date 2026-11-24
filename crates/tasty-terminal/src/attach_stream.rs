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

/// 시퀀스 경계만 보려고 파서 상태를 돌린다. 동작은 termwiz 파서가 맡는다. `ended`는 시퀀스를 끝낸
/// 바이트에서 선다. 문자열을 끝내는 ESC도 그 문자열의 dispatch를 부르므로 여기서 끝이다.
#[derive(Default)]
struct SequenceEnd {
    ended: bool,
}
impl vtparse::VTActor for SequenceEnd {
    fn print(&mut self, _: char) {}
    fn execute_c0_or_c1(&mut self, _: u8) {}
    fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
    fn dcs_put(&mut self, _: u8) {}
    fn dcs_unhook(&mut self) {
        self.ended = true;
    }
    fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: u8) {
        self.ended = true;
    }
    fn csi_dispatch(&mut self, _: &[vtparse::CsiParam], _: bool, _: u8) {
        self.ended = true;
    }
    fn osc_dispatch(&mut self, _: &[&[u8]]) {
        self.ended = true;
    }
    fn apc_dispatch(&mut self, _: Vec<u8>) {
        self.ended = true;
    }
}

const ESC: u8 = 0x1b;

/// 효과 소유자에게 넘긴 바이트의 파서 상태. CSI·DCS·SOS·PM·APC를 ESC가 취소하면 vtparse 콜백이
/// 오지 않으므로, 열린 시퀀스의 도입자를 ESC 뒤 원시 바이트로 기억해 그 ESC를 시퀀스의 끝으로 본다.
pub(crate) struct EffectsScan {
    parser: vtparse::VTParser,
    /// 열린 시퀀스가 ESC에 끝나는 종류(CSI·DCS·SOS·PM·APC)다.
    ends_at_esc: bool,
    after_esc: bool,
}
impl EffectsScan {
    fn new() -> Self {
        Self {
            parser: vtparse::VTParser::new(),
            ends_at_esc: false,
            after_esc: false,
        }
    }

    /// 한 바이트를 넣는다. 열린 시퀀스가 이 바이트에서 끝났거나(dispatch·ESC 취소) ground면 true.
    fn step(&mut self, byte: u8) -> bool {
        let cancelled = byte == ESC && self.ends_at_esc;
        let mut end = SequenceEnd::default();
        self.parser.parse_byte(byte, &mut end);
        if self.after_esc {
            self.ends_at_esc = matches!(byte, b'[' | b'P' | b'X' | b'^' | b'_');
        }
        self.after_esc = byte == ESC;
        if self.after_esc || self.parser.is_ground() {
            self.ends_at_esc = false;
        }
        cancelled || end.ended || self.parser.is_ground()
    }
}

impl TerminalState {
    /// 큐가 넘쳐 바이트를 잃은 스트림도 수신자가 닫힐 때까지 효과를 맡는다(손실 뒤 재동기화가 화면만
    /// 복구한다).
    pub(crate) fn fan_out_attach_output(&mut self, bytes: &[u8]) {
        self.attach_streams
            .retain(|tap| tap.send(Some(bytes), 0, 0) || tap.keeps_effects());
    }
    /// 앞에서부터 효과 소유자 몫인 바이트 수. 소유자가 있으면 전부다. 소유자가 떠났으면 그가 받은
    /// 바이트로 시작해 아직 끝나지 않은 시퀀스를 끝내거나 취소하는 바이트까지다. 그래서 앞부분을
    /// client에 넘긴 시퀀스를 서버가 이어 받아 효과를 내지 않고, 그 끝 ESC로 시작한 다음 시퀀스는
    /// 서버 몫이다.
    pub(crate) fn effects_held_prefix(&mut self, bytes: &[u8], owned: bool) -> usize {
        if owned {
            let scan = self.effects_scan.get_or_insert_with(EffectsScan::new);
            for &byte in bytes {
                scan.step(byte);
            }
            return bytes.len();
        }
        let Some(scan) = self.effects_scan.as_mut() else {
            return 0;
        };
        if scan.parser.is_ground() {
            self.effects_scan = None;
            return 0;
        }
        let Some(last) = bytes.iter().position(|&byte| scan.step(byte)) else {
            return bytes.len();
        };
        self.effects_scan = None;
        last + 1
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
