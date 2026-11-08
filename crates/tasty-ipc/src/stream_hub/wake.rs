//! 스트림 수신이 메인 루프를 깨우는 횟수를 처리 전 하나로 합친다.
//!
//! 연결의 읽기 스레드는 프레임마다 깨운다. 그 깨움을 그대로 사용자 이벤트로 보내면
//! 이벤트 루프가 사용자 이벤트를 비우는 동안 새 이벤트가 계속 쌓여 입력·렌더·IPC가
//! 차례를 받지 못한다. 그래서 처리 전인 깨움은 하나만 두고, 메인 루프가 회차를 열 때
//! 표시를 내린다. 동작과 이유는 docs/architecture/data-flows.md 의 스트림 회차 절에 있다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::server::IpcWaker;

/// 한 회차에 꺼내는 수신 메시지 상한. 남은 메시지는 다음 회차가 처리한다.
pub const PUMP_MESSAGE_BUDGET: usize = 256;

struct Inner {
    /// 메인 루프가 아직 처리하지 않은 깨움이 있다.
    pending: AtomicBool,
    send: OnceLock<IpcWaker>,
}

/// 읽기 스레드와 메인 루프가 공유하는 깨움 표시. 복제본은 같은 표시를 가리킨다.
#[derive(Clone)]
pub struct StreamWakeGate {
    inner: Arc<Inner>,
}

impl Default for StreamWakeGate {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamWakeGate {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                pending: AtomicBool::new(false),
                send: OnceLock::new(),
            }),
        }
    }

    /// 실제로 이벤트를 보내는 함수를 연결하고, 읽기 스레드에 넘길 waker를 돌려준다.
    /// 두 번째 연결은 무시하고 처음 연결한 함수를 계속 쓴다.
    pub fn install(&self, send: IpcWaker) -> IpcWaker {
        self.connect(send);
        self.waker()
    }

    /// 실제로 이벤트를 보내는 함수만 연결한다. waker는 필요한 곳에서 `waker`로 만든다.
    pub fn connect(&self, send: IpcWaker) {
        if self.inner.send.set(send).is_err() {
            tracing::warn!("stream wake gate: sender already installed; keeping the first one");
        }
    }

    /// 이미 연결한 함수를 쓰는 waker를 하나 더 만든다. 연결이 여러 개여도 표시는 하나다.
    pub fn waker(&self) -> IpcWaker {
        let gate = self.clone();
        Arc::new(move || gate.notify())
    }

    /// 처리 전인 깨움이 없을 때만 이벤트를 보낸다.
    /// 읽기 스레드는 채널에 메시지를 넣은 뒤 이 함수를 부른다.
    pub fn notify(&self) {
        if self.inner.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(send) = self.inner.send.get() {
            send();
        }
    }

    /// 회차를 시작한다. 채널을 읽기 전에 불러야 그 뒤에 들어온 메시지가 새 깨움을 만든다.
    pub fn open_round(&self) {
        self.inner.pending.swap(false, Ordering::AcqRel);
    }

    /// 아직 회차가 처리하지 않은 깨움이 있다.
    pub fn is_pending(&self) -> bool {
        self.inner.pending.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn counting_gate() -> (StreamWakeGate, IpcWaker, Arc<AtomicUsize>) {
        let gate = StreamWakeGate::new();
        let sent = Arc::new(AtomicUsize::new(0));
        let counter = sent.clone();
        let waker = gate.install(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        (gate, waker, sent)
    }

    #[test]
    fn many_frames_before_a_round_send_one_event() {
        let (gate, waker, sent) = counting_gate();
        for _ in 0..1000 {
            waker();
        }
        assert_eq!(sent.load(Ordering::SeqCst), 1);
        assert!(gate.is_pending());
    }

    #[test]
    fn a_frame_after_the_round_opens_sends_a_new_event() {
        let (gate, waker, sent) = counting_gate();
        waker();
        gate.open_round();
        assert!(!gate.is_pending());
        waker();
        waker();
        assert_eq!(sent.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn notify_without_a_sender_only_marks_pending() {
        let gate = StreamWakeGate::new();
        gate.notify();
        assert!(gate.is_pending());
    }

    #[test]
    fn budgeted_pump_leaves_the_rest_in_the_channel() {
        use crate::stream::{StreamFrame, StreamTag};
        let hub = super::super::StreamHub::new();
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..5 {
            tx.send(super::super::StreamInbound::Frame {
                client_id: 1,
                frame: StreamFrame::new(StreamTag::Ping, Vec::new()),
            })
            .unwrap();
        }
        let (_, cut) = hub.pump_inbound_budgeted(&rx, 3);
        assert!(cut);
        let (_, cut) = hub.pump_inbound_budgeted(&rx, 3);
        assert!(!cut, "two messages remained and the budget was not reached");
        assert!(rx.try_recv().is_err());
    }
}
