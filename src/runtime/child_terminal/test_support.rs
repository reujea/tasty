//! Legacy synchronous assertions wait on the real adapter; runtime never uses these barriers.
use super::*;
use crate::app::file_writer;
use std::time::{Duration, Instant};
use tasty_utils::batch_writer::{Limits, SystemClock};

pub(super) fn wait(receipt: Ticket) -> Result<Option<u32>, String> {
    // receipt.poll 은 안에서 hub.poll 로 child cleanup 을 제출할 수 있는 진행이라 기한 뒤 관측 읽기에서 부를 수
    // 없다. 부르지 않고 볼 수 있는 저장 상태로는 결과가 나왔는지 알 수 없어 원래 루프를 둔다.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = receipt.poll() {
            return result;
        }
        assert!(Instant::now() < deadline, "child file receipt missing");
        std::thread::yield_now();
    }
}
impl Hub {
    pub(crate) fn test_attach_writer(&self) {
        let parts = file_writer::start(
            Arc::new(tasty_settings::FileSettingsStorage),
            Limits::default(),
            Arc::new(SystemClock),
            Arc::new(|| {}),
        )
        .unwrap();
        self.attach(parts.writer.handle());
        // ready 는 안에서 poll 로 child cleanup 을 제출할 수 있는 진행이라 기한 뒤 관측 읽기에서 부를 수 없다.
        // 부르지 않고 볼 수 있는 저장 상태로는 준비됐는지 알 수 없어 원래 루프를 둔다.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.ready() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        *self.fixture_writer.lock().unwrap() = Some(parts);
    }
}
/// `Hub::test_hold_writer`가 writer 큐 앞에 넣은 막힌 명령. `release`하기 전까지 뒤 명령은 실행되지 않는다.
pub(crate) struct WriterHold {
    release: std::sync::mpsc::SyncSender<()>,
    receipt: crate::app::file_writer::command::Ticket<()>,
}
impl WriterHold {
    pub(crate) fn release(self) {
        self.release.send(()).unwrap();
        // 영수증 poll은 이미 나온 결과만 꺼낸다. 원래 간격(yield)을 지킨다.
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.receipt.poll().is_none() {
            assert!(Instant::now() < deadline, "writer hold receipt missing");
            std::thread::yield_now();
        }
    }
}
impl Hub {
    /// `test_attach_writer`로 붙인 writer를 막는다. 막힌 명령이 실행을 시작한 뒤 돌아온다.
    pub(crate) fn test_hold_writer(&self) -> WriterHold {
        let (entered, ready) = std::sync::mpsc::sync_channel(1);
        let (release, wait) = std::sync::mpsc::sync_channel(1);
        let fixture = self.fixture_writer.lock().unwrap();
        let parts = fixture.as_ref().expect("test_attach_writer first");
        let receipt = crate::app::file_writer::command::submit(
            Some(&parts.writer.handle()),
            1,
            move || {
                entered.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(10)).unwrap();
            },
            |_| 0,
        )
        .expect("hold admitted");
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        WriterHold { release, receipt }
    }
}
impl ChildTerminalRegistry {
    /// 이 registry의 hub에 붙은 시험 writer를 막는다(`Hub::test_hold_writer`).
    pub(crate) fn test_hold_writer(&self) -> WriterHold {
        self.hub.test_hold_writer()
    }
    /// 다음에 접수되는 저장 명령 하나를 writer 안에서 실패(panic)하게 한다.
    pub(crate) fn test_fail_next_write(&self) {
        self.hub
            .panic_next
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    #[cfg(all(unix, feature = "gui"))]
    pub(crate) fn flush_for_test(&mut self) {
        wait(self.submit().expect("real child file admission"))
            .expect("real child file replacement");
    }
}
