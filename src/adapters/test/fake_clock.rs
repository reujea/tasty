//! FakeClock — test 시 시각을 생성 시점에 **고정**한다. deterministic.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::ports::clock::Clock;

pub struct FakeClock {
    base_instant: Instant,
    base_system: SystemTime,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            base_instant: Instant::now(),
            base_system: SystemTime::now(),
        }
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now_instant(&self) -> Instant {
        self.base_instant
    }

    fn now_system(&self) -> SystemTime {
        self.base_system
    }

    fn now_unix_millis(&self) -> i64 {
        let now = self.now_system();
        now.duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}
