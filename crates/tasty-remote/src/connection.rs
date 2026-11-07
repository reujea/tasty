//! A bounded queued frame belongs to exactly one connection, never a replaceable sender slot.
use super::client_session::OutFrame;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
// Only the observer value is replaced/cloned under this lock; callbacks run after unlocking.
static INPUT_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static FAILURE_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct ConnectionEpoch(Arc<AtomicBool>);
impl ConnectionEpoch {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    pub fn is_active(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn retire(&self) {
        self.0.store(false, Ordering::Release);
    }
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
pub struct QueuedFrame {
    pub epoch: ConnectionEpoch,
    pub frame: OutFrame,
    _bytes: QueuedBytes,
    current: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}
impl QueuedFrame {
    pub(crate) fn owner_current(&self) -> bool {
        self.current.as_ref().is_none_or(|current| current())
    }
}
struct QueuedBytes {
    counter: Arc<AtomicUsize>,
    bytes: usize,
}
impl Drop for QueuedBytes {
    fn drop(&mut self) {
        self.counter.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
/// 연결 실패를 알릴 상대 — 끊김 플래그와 깨우기 콜백. `bind_failure` 가 채우고
/// `fail` 이 읽는다.
type FailureObserver = (Arc<AtomicBool>, Arc<dyn Fn() + Send + Sync>);
pub struct ConnectionSender {
    began: std::time::Instant,
    epoch: ConnectionEpoch,
    sender: mpsc::SyncSender<QueuedFrame>,
    bytes: Arc<AtomicUsize>,
    failed: AtomicBool,
    failure: std::sync::Mutex<Option<FailureObserver>>,
    input: std::sync::Mutex<super::input_receipts::InputReceipts>,
    /// 서버 PTY가 Windows 콘솔 입력을 받는다. mirror 키 인코딩을 이 값으로 정한다.
    win32_console_input: AtomicBool,
}
impl ConnectionSender {
    pub fn set_win32_console_input(&self, enabled: bool) {
        self.win32_console_input.store(enabled, Ordering::Release);
    }
    pub fn win32_console_input(&self) -> bool {
        self.win32_console_input.load(Ordering::Acquire)
    }
    fn input(&self) -> std::sync::MutexGuard<'_, super::input_receipts::InputReceipts> {
        tasty_utils::poison::recover_mutex(
            self.input.lock(),
            "remote input receipts",
            &INPUT_POISON_REPORTED,
        )
    }
    pub fn enable_input_receipts(&self, enabled: bool) {
        self.input().enabled = enabled;
    }
    pub fn oldest_unprocessed_input(&self) -> Option<std::time::Instant> {
        self.epoch
            .is_active()
            .then(|| self.input().oldest())
            .flatten()
    }
    /// Reader-side receipt observation cannot wait behind a deferred structural Delta.
    pub fn input_processed(&self, sequence: u64) {
        if !self.epoch.is_active() {
            return;
        }
        let mut input = self.input();
        if let Some(next) = input.received(sequence) {
            self.send_checkpoint(next);
        }
    }
    fn send_checkpoint(&self, sequence: u64) {
        let payload =
            serde_json::to_vec(&tasty_ipc::stream::StreamControl::InputCheckpoint { sequence })
                .expect("input checkpoint contains only an integer");
        // The input was already accepted. Failure retires this exact epoch and is
        // reported by the existing connection failure observer; never replay it.
        if self
            .send_frame(OutFrame {
                tag: tasty_ipc::stream::StreamTag::Control,
                payload,
            })
            .is_err()
        {
            tracing::debug!("input checkpoint rejected; connection failure observer owns cleanup");
        }
    }
    pub fn bind_failure(&self, disconnected: Arc<AtomicBool>, wake: Arc<dyn Fn() + Send + Sync>) {
        let mut observer = self.failure.lock().unwrap_or_else(|poison| {
            tasty_utils::poison::recover_poisoned(
                poison,
                "remote connection failure observer",
                &FAILURE_POISON_REPORTED,
            )
        });
        *observer = Some((disconnected.clone(), wake.clone()));
        drop(observer);
        if self.failed.load(Ordering::Acquire) {
            disconnected.store(true, Ordering::Release);
            wake();
        }
    }
    fn fail(&self, reason: &str) {
        self.epoch.retire();
        if !self.failed.swap(true, Ordering::AcqRel) {
            tracing::warn!(
                reason,
                "remote output rejected; connection retired and queued input will not replay"
            );
        }
        let observer = self
            .failure
            .lock()
            .unwrap_or_else(|poison| {
                tasty_utils::poison::recover_poisoned(
                    poison,
                    "remote connection failure observer",
                    &FAILURE_POISON_REPORTED,
                )
            })
            .clone();
        if let Some((disconnected, wake)) = observer {
            disconnected.store(true, Ordering::Release);
            wake();
        }
    }

    pub fn began_at(&self) -> std::time::Instant {
        self.began
    }
    pub fn epoch(&self) -> ConnectionEpoch {
        self.epoch.clone()
    }
    pub fn retire(&self) {
        self.epoch.retire();
    }
    /// Failure means the frame was not accepted. Callers never retry old user input on a new epoch.
    pub fn send(&self, frame: OutFrame) -> Result<(), mpsc::SendError<OutFrame>> {
        let _input = self.input();
        self.send_frame(frame)
    }
    pub(crate) fn send_guarded(
        &self,
        frame: OutFrame,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), mpsc::SendError<OutFrame>> {
        let _input = self.input();
        self.enqueue_frame(frame, Some(current))
    }
    /// Called only after explicit user input was dispatched to this connection.
    /// Agent writes and terminal-generated replies use `send` without a receipt.
    pub fn user_input_dispatched(&self, admitted: std::time::Instant) {
        if !self.epoch.is_active() {
            return;
        }
        let mut input = self.input();
        if let Some(sequence) = input.input(admitted) {
            self.send_checkpoint(sequence);
        }
    }
    fn send_frame(&self, frame: OutFrame) -> Result<(), mpsc::SendError<OutFrame>> {
        self.enqueue_frame(frame, None)
    }
    fn enqueue_frame(
        &self,
        frame: OutFrame,
        current: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> Result<(), mpsc::SendError<OutFrame>> {
        if !self.epoch.is_active() || current.as_ref().is_some_and(|current| !current()) {
            return Err(mpsc::SendError(frame));
        }
        let weight = frame
            .payload
            .capacity()
            .saturating_add(std::mem::size_of::<QueuedFrame>());
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                held.checked_add(weight)
                    .filter(|next| *next <= tasty_ipc::admission::QUEUED_BYTES_LIMIT)
            })
            .is_err()
        {
            self.fail("outgoing byte capacity");
            return Err(mpsc::SendError(frame));
        }
        let queued = QueuedFrame {
            epoch: self.epoch.clone(),
            frame,
            current,
            _bytes: QueuedBytes {
                counter: self.bytes.clone(),
                bytes: weight,
            },
        };
        self.sender.try_send(queued).map_err(|error| {
            let queued = match error {
                mpsc::TrySendError::Full(queued) => {
                    self.fail("outgoing frame capacity");
                    queued
                }
                mpsc::TrySendError::Disconnected(queued) => {
                    self.fail("writer disconnected");
                    queued
                }
            };
            mpsc::SendError(queued.frame)
        })
    }
}
pub fn channel() -> (Arc<ConnectionSender>, mpsc::Receiver<QueuedFrame>) {
    let (sender, receiver) = mpsc::sync_channel(256);
    (
        Arc::new(ConnectionSender {
            began: std::time::Instant::now(),
            epoch: ConnectionEpoch::new(),
            sender,
            bytes: Arc::new(AtomicUsize::new(0)),
            failed: AtomicBool::new(false),
            failure: std::sync::Mutex::new(None),
            input: std::sync::Mutex::new(Default::default()),
            win32_console_input: AtomicBool::new(false),
        }),
        receiver,
    )
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use tasty_ipc::stream::{StreamControl, StreamTag};
    fn data(value: u8) -> OutFrame {
        OutFrame {
            tag: StreamTag::Data,
            payload: vec![value],
        }
    }
    fn checkpoint(frame: QueuedFrame) -> u64 {
        let StreamControl::InputCheckpoint { sequence } =
            serde_json::from_slice(&frame.frame.payload).unwrap()
        else {
            panic!("checkpoint")
        };
        sequence
    }
    #[test]
    fn receipt_is_between_input_prefix_and_successor_and_never_tracks_agent_bytes() {
        let (sender, queue) = channel();
        sender.enable_input_receipts(true);
        let t = std::time::Instant::now();
        sender.send(data(b'a')).unwrap();
        assert!(
            sender.oldest_unprocessed_input().is_none(),
            "plain/agent writes are not user requests"
        );
        sender.user_input_dispatched(t);
        sender.send(data(b'b')).unwrap();
        sender.user_input_dispatched(t + std::time::Duration::from_secs(2));
        sender.send(data(b'c')).unwrap();
        sender.user_input_dispatched(t + std::time::Duration::from_secs(3));
        assert_eq!(queue.try_recv().unwrap().frame.payload, b"a");
        assert_eq!(checkpoint(queue.try_recv().unwrap()), 1);
        assert_eq!(queue.try_recv().unwrap().frame.payload, b"b");
        assert_eq!(queue.try_recv().unwrap().frame.payload, b"c");
        assert!(queue.try_recv().is_err());
        sender.input_processed(1);
        assert_eq!(checkpoint(queue.try_recv().unwrap()), 2);
        assert_eq!(
            sender.oldest_unprocessed_input(),
            Some(t + std::time::Duration::from_secs(2))
        );
        sender.input_processed(1);
        assert!(queue.try_recv().is_err());
        sender.retire();
        sender.input_processed(2);
        assert!(sender.oldest_unprocessed_input().is_none());
        assert!(queue.try_recv().is_err());
    }
    #[test]
    fn guarded_frame_retains_byte_lease_until_actual_queue_drop() {
        let (sender, queue) = channel();
        let current = Arc::new(AtomicBool::new(true));
        let captured = current.clone();
        sender
            .send_guarded(data(7), Arc::new(move || captured.load(Ordering::Acquire)))
            .unwrap();
        let used = sender.bytes.load(Ordering::Acquire);
        assert!(used > 0);
        let frame = queue.try_recv().unwrap();
        current.store(false, Ordering::Release);
        assert!(!frame.owner_current());
        assert_eq!(sender.bytes.load(Ordering::Acquire), used);
        drop(frame);
        assert_eq!(sender.bytes.load(Ordering::Acquire), 0);
        assert!(sender.epoch().is_active());
        assert!(sender.send_guarded(data(8), Arc::new(|| false)).is_err());
        assert_eq!(sender.bytes.load(Ordering::Acquire), 0);
        assert!(sender.epoch().is_active());
    }
}
