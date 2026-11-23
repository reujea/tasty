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
impl ChildTerminalRegistry {
    #[cfg(all(unix, feature = "gui"))]
    pub(crate) fn flush_for_test(&mut self) {
        wait(self.submit().expect("real child file admission"))
            .expect("real child file replacement");
    }
}
