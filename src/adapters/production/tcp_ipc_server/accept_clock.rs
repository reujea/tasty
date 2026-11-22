//! accept 큐의 실제 도착 시각 대신 마지막으로 빈 큐를 확인한 시각부터 잰다.
//! 그 뒤 꺼낸 연결은 이후에 도착했으므로 이 차이는 큐 대기 시간의 상한이다.
//! 연속 accept 중에는 기준이 바뀌지 않아 뒤의 연결일수록 상한이 커질 수 있다.

use std::time::{Duration, Instant};

pub(super) struct AcceptClock {
    last_empty: Instant,
}

impl AcceptClock {
    /// 리스너를 막 열었다 — 그 순간 큐는 비어 있었다.
    pub(super) fn new(now: Instant) -> Self {
        Self { last_empty: now }
    }

    /// `accept` 가 `WouldBlock` 을 돌려줬다 — 큐가 지금 비어 있다.
    pub(super) fn saw_empty(&mut self, now: Instant) {
        self.last_empty = now;
    }

    /// `now` 에 꺼낸 연결이 큐에서 기다렸을 수 있는 시간의 상한.
    pub(super) fn bound(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::AcceptClock;
    use std::time::{Duration, Instant};

    #[test]
    fn the_bound_runs_from_the_last_empty_queue() {
        let t0 = Instant::now();
        let mut clock = AcceptClock::new(t0);
        clock.saw_empty(t0 + Duration::from_millis(10));
        assert_eq!(
            clock.bound(t0 + Duration::from_millis(110)),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn a_burst_keeps_the_same_reference() {
        let t0 = Instant::now();
        let clock = AcceptClock::new(t0);
        let first = clock.bound(t0 + Duration::from_millis(5));
        let second = clock.bound(t0 + Duration::from_millis(6));
        assert!(second > first);
        assert_eq!(first, Duration::from_millis(5));
    }
}

#[cfg(test)]
mod loop_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use tasty_ipc::stream_hub::{StreamContext, StreamHub};
    use tasty_telemetry::ConnectionStats;

    use super::super::TcpIpcServer;

    // 수락·포화 거절을 포함해 꺼낸 연결마다 상한을 기록해야 한다.
    #[test]
    fn the_accept_loop_records_a_bound_for_every_connection_it_takes() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let dir = tempfile::tempdir().expect("tempdir");
        let port_file = dir.path().join("tasty.port");
        let (inbound_tx, _inbound_rx) = std::sync::mpsc::channel();
        let ctx = StreamContext {
            hub: StreamHub::new(),
            inbound_tx,
            waker: Arc::new(|| {}),
            heartbeat_timeout: Default::default(),
        };
        let stats = Arc::new(ConnectionStats::default());
        let server = TcpIpcServer::start_with_port_file(
            Some(port_file.to_string_lossy().into_owned()),
            None,
            ctx,
            stats.clone(),
        )
        .expect("server");
        let addr = format!("127.0.0.1:{}", server.port);
        let clients: Vec<_> = (0..2)
            .map(|_| std::net::TcpStream::connect(&addr).expect("connect"))
            .collect();

        // accept 루프는 대기 상한을 기록한 뒤에 수락·거절 계수를 올린다. 세 계수가 모두 선 스냅샷을
        // 기다리고, 기한에 닿으면 마지막으로 읽은 스냅샷으로 아래를 판정한다.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut s = stats.snapshot();
        tasty_utils::test_wait::holds_by(deadline, || {
            s = stats.snapshot();
            s.accept_waits >= 2 && s.accept_waits == s.accepted + s.refused_saturated
        });
        assert_eq!(
            s.accept_waits, 2,
            "연결 둘을 꺼냈는데 기록이 {}",
            s.accept_waits
        );
        assert_eq!(s.accept_waits, s.accepted + s.refused_saturated);
        assert!(
            s.accept_wait_bound_us_max < 5_000_000,
            "상한이 루프의 잠(100 ms) 규모를 한참 넘었다: {} µs",
            s.accept_wait_bound_us_max
        );
        drop(clients);
        drop(server);
    }
}
