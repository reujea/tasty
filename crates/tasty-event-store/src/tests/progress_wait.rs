//! 다른 스레드의 진행을 기다리는 시험의 정체 감지.
//!
//! 벽시계 기한은 부하로 느려졌을 뿐 계속 진행하는 스레드도 실패시킨다. 여기서는 진행이 없는 동안
//! 지켜보는 스레드가 잠들어 있던 시간만 정체로 세고, 진행이 보이면 다시 센다.
//! - 스레드가 실행 중(`R`)이거나 디스크 I/O 를 기다리는(`D`) 시간은 세지 않는다. 같은 디스크를
//!   쓰는 다른 프로세스 때문에 commit 의 fsync 가 몇 초씩 늦어져도 실패하지 않는다.
//! - 잠금을 기다리거나 멈춘 스레드는 잠들어 있으므로 [`ProgressWait::IDLE_LIMIT`] 안에 실패한다.
//! - 스레드 상태는 Linux 에서만 읽는다. 다른 OS 에서는 진행이 없는 시간을 모두 센다.
//! - 쉬지 않고 도는 스레드는 잠들지 않으므로 [`ProgressWait::CEILING`] 의 벽시계 상한으로 잡는다.
//!
//! 본체의 `StallBudget` 을 줄인 사본이라 이 크레이트의 시험에만 둔다. 두 번째 소비자가 생기면
//! `tasty-test-support` 로 옮긴다.

use std::fmt::Debug;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 지켜볼 스레드의 실행 상태를 읽는 자리.
pub(super) struct ThreadState {
    stat: Option<PathBuf>,
}

impl ThreadState {
    /// 지켜볼 스레드 안에서 부른다.
    pub(super) fn current() -> Self {
        #[cfg(target_os = "linux")]
        let stat = std::fs::read_link("/proc/thread-self")
            .ok()
            .map(|task| PathBuf::from("/proc").join(task).join("stat"));
        #[cfg(not(target_os = "linux"))]
        let stat = None;
        Self { stat }
    }

    /// 실행 중이거나 디스크 I/O 를 기다리는가. 읽지 못하면(끝난 스레드, 다른 OS) 아니다.
    fn is_working(&self) -> bool {
        let Some(path) = &self.stat else {
            return false;
        };
        let Ok(stat) = std::fs::read_to_string(path) else {
            return false;
        };
        // 상태는 스레드 이름(괄호 안, 공백 가능) 뒤의 첫 필드다.
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.trim_start().chars().next())
            .is_some_and(|state| matches!(state, 'R' | 'D'))
    }
}

pub(super) struct ProgressWait<T> {
    last: T,
    idle: Duration,
    polled: Instant,
    started: Instant,
    thread: ThreadState,
}

impl<T: PartialEq + Debug> ProgressWait<T> {
    /// 진행 없이 지켜보는 스레드가 잠들어 있던 시간의 한도.
    pub(super) const IDLE_LIMIT: Duration = Duration::from_secs(10);
    /// 스레드가 일하는 중이어도 넘지 않는 벽시계 상한. 끝없이 도는 스레드를 잡는다.
    pub(super) const CEILING: Duration = Duration::from_secs(300);

    pub(super) fn new(thread: ThreadState, initial: T) -> Self {
        let now = Instant::now();
        Self {
            last: initial,
            idle: Duration::ZERO,
            polled: now,
            started: now,
            thread,
        }
    }

    /// 지금 관측한 진행을 넘긴다. 이전 관측 뒤로 진행이 없고 스레드가 잠들어 있으면 그 사이
    /// 시간을 정체로 센다. 한도를 넘으면 실패한다.
    pub(super) fn observe(&mut self, progress: T) {
        let now = Instant::now();
        let since = now - self.polled;
        self.polled = now;
        if progress != self.last {
            self.last = progress;
            self.idle = Duration::ZERO;
        } else if !self.thread.is_working() {
            self.idle += since;
        }
        assert!(
            self.idle < Self::IDLE_LIMIT,
            "stalled at {:?}: the watched thread slept {:?} without progress",
            self.last,
            self.idle
        );
        // 기한 뒤 재읽기를 넣지 않는다. 판정은 관측한 진행 유무(IDLE_LIMIT)로 하고, CEILING 은 끝없이 도는
        // 스레드를 잡는 300초 벽시계 상한이라 거짓 읽기 하나로 넘을 거리가 아니다.
        assert!(
            self.started.elapsed() < Self::CEILING,
            "stalled at {:?}: still waiting after {:?} (progress too slow or the watched thread never slept)",
            self.last,
            self.started.elapsed()
        );
    }
}
