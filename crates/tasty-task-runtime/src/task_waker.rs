//! 작업 상태를 기다리는 호출자에게 snapshot을 보내고 종결 상태는 사건 큐에도 기록한다.
//! 상태를 바꾸는 호출자가 fire를 호출해야 두 소비자가 통지를 받는다.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tasty_agent::{TaskId, TaskResult, TaskState};

#[derive(Debug, Clone)]
pub struct TerminalSnapshot {
    pub state: TaskState,
    pub result: Option<TaskResult>,
    /// v2 task 의 실행 회차 id. 회차가 없는 task 는 `None`.
    pub attempt_id: Option<String>,
    /// 건너뛴 이유. 이유가 기록된 v2 task 만 있다.
    pub skip: Option<tasty_agent::SkipReason>,
    /// 종결을 기록한 뒤 읽은 레코드 revision([`tasty_agent::TaskStore::revision`]).
    pub revision: Option<u64>,
}

impl TerminalSnapshot {
    pub fn of(task: &tasty_agent::Task, revision: Option<u64>) -> Self {
        Self {
            state: task.state.clone(),
            result: task.result.clone(),
            attempt_id: task.attempt.as_ref().map(|a| a.id.clone()),
            skip: task.skip.clone(),
            revision,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AwaitOutcome {
    Terminal(TerminalSnapshot),
    TimedOut,
    NotFound,
}

type WaiterKey = (u32, TaskId);
type Waiters = HashMap<WaiterKey, Vec<(u64, Sender<TerminalSnapshot>)>>;

static TASK_WAKER_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Captured inside the ordered storage read, before its output can be delayed.
/// Acknowledgement retires the registry entry, while old outputs retain this shared claim.
#[derive(Clone)]
pub(crate) struct DeliveryToken {
    id: String,
    delivered: Arc<Mutex<bool>>,
}

type DeliveryRegistry = HashMap<String, (u32, Arc<Mutex<bool>>)>;

#[derive(Default)]
pub struct TaskWakerHub {
    waiters: Mutex<Waiters>,
    delivered: Mutex<DeliveryRegistry>,
    /// 조회 후 즉시 반환하거나 시간 초과한 대기자만 골라 지우는 데 쓴다.
    next_waiter: std::sync::atomic::AtomicU64,
    /// engine과 같은 큐를 공유해야 메인 루프가 이 이벤트를 방송할 수 있다.
    feed: std::sync::Arc<crate::event_feed::AgentEventQueue>,
}

impl TaskWakerHub {
    #[cfg(test)]
    pub(crate) fn retained_delivery_count(&self) -> usize {
        self.delivered.lock().unwrap().len()
    }

    #[cfg(test)]
    pub(crate) fn waiter_count(&self, workspace_id: u32, task_id: &TaskId) -> usize {
        self.lock_recovering()
            .get(&(workspace_id, task_id.clone()))
            .map_or(0, Vec::len)
    }

    /// 독립 큐를 만들며 외부 소비자는 연결하지 않는다. 실제 engine은 with_feed를 사용한다.
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_feed(feed: std::sync::Arc<crate::event_feed::AgentEventQueue>) -> Self {
        Self {
            waiters: Mutex::new(HashMap::new()),
            delivered: Mutex::new(HashMap::new()),
            next_waiter: std::sync::atomic::AtomicU64::new(0),
            feed,
        }
    }

    /// poison을 알리고 waiter 맵을 계속 사용해 상태 전이 경로에서 다시 패닉하지 않게 한다.
    fn lock_recovering(&self) -> std::sync::MutexGuard<'_, Waiters> {
        tasty_utils::poison::recover_mutex(
            self.waiters.lock(),
            "task waker hub",
            &TASK_WAKER_POISONED,
        )
    }

    fn unregister(&self, key: &WaiterKey, waiter: u64) {
        let mut g = self.lock_recovering();
        if let Some(senders) = g.get_mut(key) {
            senders.retain(|(id, _)| *id != waiter);
            if senders.is_empty() {
                g.remove(key);
            }
        }
    }

    /// 대기자를 먼저 등록한 뒤 load_current로 현재 상태를 읽는다. 조회와 등록 사이에 지나간 fire를 놓치지 않는다.
    /// load_current가 None이면 NotFound, 종결 상태면 즉시 반환한다. None·Some(0)은 무기한 대기다.
    pub fn await_terminal(
        &self,
        workspace_id: u32,
        task_id: &TaskId,
        timeout_ms: Option<u64>,
        load_current: impl FnOnce() -> Option<TerminalSnapshot>,
    ) -> AwaitOutcome {
        let key = (workspace_id, task_id.clone());
        let waiter = self
            .next_waiter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = channel::<TerminalSnapshot>();
        self.lock_recovering()
            .entry(key.clone())
            .or_default()
            .push((waiter, tx));
        // fire가 lock을 잡으므로 조회는 lock 밖에서 한다.
        let current = match load_current() {
            None => {
                self.unregister(&key, waiter);
                return AwaitOutcome::NotFound;
            }
            Some(c) => c,
        };
        if current.state.is_terminal() {
            self.unregister(&key, waiter);
            return AwaitOutcome::Terminal(current);
        }
        let deadline = timeout_ms
            .filter(|ms| *ms != 0)
            .map(|ms| std::time::Instant::now() + Duration::from_millis(ms));
        let result = loop {
            let received = match deadline {
                None => rx.recv().ok(),
                Some(deadline) => rx
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .ok(),
            };
            let Some(snapshot) = received else {
                break AwaitOutcome::TimedOut;
            };
            // A read completion may be reaped after retry has started another round. Its old
            // event is still valid, but it cannot finish a waiter that already read the newer row.
            if current
                .revision
                .zip(snapshot.revision)
                .is_some_and(|(seen, ended)| ended < seen)
            {
                continue;
            }
            break AwaitOutcome::Terminal(snapshot);
        };
        self.unregister(&key, waiter);
        result
    }

    /// 대기자에게 snapshot을 보낸다. 대기자가 수신하거나 시간 초과하면 등록을 지운다.
    pub fn fire(&self, workspace_id: u32, task_id: &TaskId, snapshot: TerminalSnapshot) {
        let current = Some(snapshot.clone());
        self.fire_identified(workspace_id, task_id, snapshot, None, current);
    }

    /// Only call within the same ordered read that obtained the complete, validated pending list.
    /// Absence there also confirms an ack whose successful response may have been lost.
    pub(crate) fn capture_deliveries<'a>(
        &self,
        workspace: u32,
        ids: impl IntoIterator<Item = &'a str>,
    ) -> Vec<DeliveryToken> {
        let ids: Vec<_> = ids.into_iter().collect();
        let present: HashSet<_> = ids.iter().copied().collect();
        let mut registry = tasty_utils::poison::recover_mutex(
            self.delivered.lock(),
            "terminal delivery identities",
            &TASK_WAKER_POISONED,
        );
        registry.retain(|id, (ws, _)| *ws != workspace || present.contains(id.as_str()));
        ids.into_iter()
            .map(|id| DeliveryToken {
                id: id.into(),
                delivered: registry
                    .entry(id.into())
                    .or_insert_with(|| (workspace, Arc::new(Mutex::new(false))))
                    .1
                    .clone(),
            })
            .collect()
    }

    /// An exact successful (including idempotent) ack can retire only its own token.
    pub(crate) fn confirm_ack(&self, workspace: u32, token: &DeliveryToken) {
        let mut registry = tasty_utils::poison::recover_mutex(
            self.delivered.lock(),
            "terminal delivery identities",
            &TASK_WAKER_POISONED,
        );
        if registry
            .get(&token.id)
            .is_some_and(|(ws, claim)| *ws == workspace && Arc::ptr_eq(claim, &token.delivered))
        {
            registry.remove(&token.id);
        }
    }

    pub(crate) fn fire_once(
        &self,
        token: &DeliveryToken,
        workspace_id: u32,
        task_id: &TaskId,
        snapshot: TerminalSnapshot,
        wake: Option<TerminalSnapshot>,
    ) -> bool {
        let mut delivered = tasty_utils::poison::recover_mutex(
            token.delivered.lock(),
            "terminal delivery claim",
            &TASK_WAKER_POISONED,
        );
        if *delivered {
            return false;
        }
        *delivered = true;
        // Keep the claim locked through publication: a duplicate must not submit its ack
        // before the first publisher has finished notifying the feed and waiters.
        self.fire_identified(
            workspace_id,
            task_id,
            snapshot,
            Some(token.id.clone()),
            wake,
        );
        true
    }

    fn fire_identified(
        &self,
        workspace_id: u32,
        task_id: &TaskId,
        snapshot: TerminalSnapshot,
        terminal_id: Option<String>,
        wake: Option<TerminalSnapshot>,
    ) {
        if snapshot.state.is_terminal() {
            self.feed.push(crate::event_feed::AgentEvent::TaskFinished {
                workspace_id,
                task_id: task_id.clone(),
                state: snapshot.state.name(),
                attempt_id: snapshot.attempt_id.clone(),
                revision: snapshot.revision,
                terminal_id,
                skip: snapshot.skip.clone(),
            });
        }
        let Some(snapshot) = wake else {
            return;
        };
        let mut g = self.lock_recovering();
        let Some(senders) = g.get_mut(&(workspace_id, task_id.clone())) else {
            return;
        };
        // Keep the registration through delivery: an obsolete snapshot may be rejected by a
        // waiter whose initial read already saw a newer round. Sending never blocks the writer.
        senders.retain(|(_, tx)| tx.send(snapshot.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;
    use tasty_latency_control::ControlProbe;

    fn snap(state: TaskState) -> TerminalSnapshot {
        TerminalSnapshot {
            state,
            result: None,
            attempt_id: None,
            skip: None,
            revision: None,
        }
    }

    #[test]
    fn delayed_delivery_from_before_retry_cannot_finish_a_new_round_waiter() {
        let hub = Arc::new(TaskWakerHub::new());
        let worker = hub.clone();
        let (read_tx, read_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let thread = thread::spawn(move || {
            let outcome = worker.await_terminal(1, &"p".into(), Some(0), || {
                let mut old = snap(TaskState::Failed {
                    error: "old".into(),
                });
                old.revision = Some(2);
                worker.fire_once(
                    &worker.capture_deliveries(1, ["old"]).remove(0),
                    1,
                    &"p".into(),
                    old.clone(),
                    Some(old),
                );
                let mut running = snap(TaskState::Running);
                running.revision = Some(4);
                read_tx.send(()).unwrap();
                Some(running)
            });
            drop(done_tx.send(outcome));
        });
        read_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            done_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        let mut ended = snap(TaskState::Succeeded);
        ended.revision = Some(5);
        hub.fire_once(
            &hub.capture_deliveries(1, ["new"]).remove(0),
            1,
            &"p".into(),
            ended.clone(),
            Some(ended),
        );
        let outcome = done_rx.recv_timeout(Duration::from_secs(5));
        if outcome.is_err() {
            hub.fire(1, &"p".into(), snap(TaskState::Cancelled));
        }
        assert!(
            matches!(outcome, Ok(AwaitOutcome::Terminal(s)) if s.state == TaskState::Succeeded)
        );
        thread.join().unwrap();
    }

    /// 시간 초과 시 스케줄러 대조 측정을 진단에 함께 남긴다. 원인을 확정하는 검사는 아니다.
    #[test]
    fn await_returns_immediately_if_already_terminal() {
        const LIMIT: Duration = Duration::from_millis(50);
        let hub = TaskWakerHub::new();
        let mut control = ControlProbe::start("종료 상태의 await_terminal 즉시 반환");
        let start = Instant::now();
        let out = hub.await_terminal(1, &"t-1".to_string(), Some(5000), || {
            Some(snap(TaskState::Succeeded))
        });
        let elapsed = start.elapsed();
        assert!(elapsed < LIMIT, "{}", control.verdict(elapsed, LIMIT));
        match out {
            AwaitOutcome::Terminal(s) => assert!(matches!(s.state, TaskState::Succeeded)),
            other => panic!("expected Terminal, got {other:?}"),
        }
    }

    #[test]
    fn await_wakes_on_fire() {
        let hub = Arc::new(TaskWakerHub::new());
        let hub_w = hub.clone();
        let handle = thread::spawn(move || {
            hub_w.await_terminal(1, &"t-1".to_string(), Some(5000), || {
                Some(snap(TaskState::Running))
            })
        });
        // 등록할 시간을 주지만 등록 완료를 동기화하는 방식은 아니다.
        thread::sleep(Duration::from_millis(50));
        hub.fire(1, &"t-1".to_string(), snap(TaskState::Succeeded));
        let out = handle.join().unwrap();
        match out {
            AwaitOutcome::Terminal(s) => assert!(matches!(s.state, TaskState::Succeeded)),
            other => panic!("expected Terminal, got {other:?}"),
        }
    }

    #[test]
    fn await_times_out_after_short_duration() {
        const CEILING: Duration = Duration::from_millis(500);
        let hub = TaskWakerHub::new();
        let mut control = ControlProbe::start("50 ms 요청의 실제 대기 시간");
        let start = Instant::now();
        let out = hub.await_terminal(1, &"t-1".to_string(), Some(50), || {
            Some(snap(TaskState::Running))
        });
        let elapsed = start.elapsed();
        assert!(matches!(out, AwaitOutcome::TimedOut));
        // 하한은 조기 반환을, 상한은 과도한 지연을 검사한다. 상한 실패에는 대조 측정도 표시한다.
        assert!(elapsed >= Duration::from_millis(40), "elapsed={elapsed:?}");
        assert!(elapsed < CEILING, "{}", control.verdict(elapsed, CEILING));
    }

    /// 조회가 비종결을 본 직후의 완료도 이미 등록된 대기자에게 전달돼야 한다.
    #[test]
    fn a_fire_right_after_the_state_read_still_wakes_the_waiter() {
        let hub = TaskWakerHub::new();
        let out = hub.await_terminal(1, &"t-1".to_string(), Some(2_000), || {
            let current = snap(TaskState::Running);
            hub.fire(1, &"t-1".to_string(), snap(TaskState::Succeeded));
            Some(current)
        });
        match out {
            AwaitOutcome::Terminal(s) => assert!(matches!(s.state, TaskState::Succeeded)),
            other => panic!("조회 직후의 완료를 놓쳤다: {other:?}"),
        }
    }

    #[test]
    fn a_missing_task_is_not_found_and_leaves_no_waiter() {
        let hub = TaskWakerHub::new();
        let out = hub.await_terminal(1, &"t-1".to_string(), Some(2_000), || None);
        assert!(matches!(out, AwaitOutcome::NotFound), "{out:?}");
        assert!(hub.lock_recovering().is_empty());
    }

    #[test]
    fn immediate_and_timed_out_returns_leave_no_waiter() {
        let hub = TaskWakerHub::new();
        let id = "t-1".to_string();
        let out = hub.await_terminal(1, &id, Some(5_000), || Some(snap(TaskState::Cancelled)));
        assert!(matches!(out, AwaitOutcome::Terminal(_)), "{out:?}");
        let out = hub.await_terminal(1, &id, Some(20), || Some(snap(TaskState::Running)));
        assert!(matches!(out, AwaitOutcome::TimedOut), "{out:?}");
        assert!(hub.lock_recovering().is_empty());
    }

    #[test]
    fn fire_without_waiters_is_noop() {
        let hub = TaskWakerHub::new();
        hub.fire(1, &"t-none".to_string(), snap(TaskState::Succeeded));
    }

    #[test]
    fn a_terminal_task_reaches_the_feed_even_with_nobody_waiting() {
        use crate::event_feed::{AgentEvent, AgentEventQueue};
        let feed = Arc::new(AgentEventQueue::new());
        let hub = TaskWakerHub::with_feed(Arc::clone(&feed));
        hub.fire(7, &"t-9".to_string(), snap(TaskState::Cancelled));
        let (events, dropped) = feed.take_pending();
        assert_eq!(dropped, 0);
        assert_eq!(
            events,
            vec![AgentEvent::TaskFinished {
                workspace_id: 7,
                task_id: "t-9".to_string(),
                state: "cancelled",
                attempt_id: None,
                revision: None,
                terminal_id: None,
                skip: None,
            }]
        );
    }

    /// 사건은 회차·revision·skip 이유를 싣고 결과는 싣지 않는다.
    #[test]
    fn a_finished_event_carries_the_attempt_revision_and_skip_reason() {
        use crate::event_feed::{AgentEvent, AgentEventQueue};
        let feed = Arc::new(AgentEventQueue::new());
        let hub = TaskWakerHub::with_feed(Arc::clone(&feed));
        let skip = tasty_agent::SkipReason::UpstreamUnavailable {
            source: "a".to_string(),
            source_state: "failed".to_string(),
        };
        hub.fire(
            7,
            &"t-9".to_string(),
            TerminalSnapshot {
                attempt_id: Some("t-9#2".to_string()),
                skip: Some(skip.clone()),
                revision: Some(5),
                ..snap(TaskState::Skipped)
            },
        );
        assert_eq!(
            feed.take_pending().0,
            vec![AgentEvent::TaskFinished {
                workspace_id: 7,
                task_id: "t-9".to_string(),
                state: "skipped",
                attempt_id: Some("t-9#2".to_string()),
                revision: Some(5),
                terminal_id: None,
                skip: Some(skip),
            }]
        );
    }

    #[test]
    fn a_nonterminal_snapshot_leaves_the_feed_alone() {
        use crate::event_feed::AgentEventQueue;
        let feed = Arc::new(AgentEventQueue::new());
        let hub = TaskWakerHub::with_feed(Arc::clone(&feed));
        hub.fire(7, &"t-9".to_string(), snap(TaskState::Running));
        let (events, _) = feed.take_pending();
        assert!(events.is_empty(), "비종결이 실렸다: {events:?}");
    }
}

#[cfg(test)]
mod poison_tests {
    use super::*;
    use std::sync::Arc;

    /// poison 뒤에도 등록한 대기자에게 snapshot이 전달되는지 확인한다.
    #[test]
    fn a_poisoned_hub_still_wakes_pending_waiters() {
        let hub = Arc::new(TaskWakerHub::new());
        let held = Arc::clone(&hub);
        let joined = std::thread::spawn(move || {
            let _guard = held.waiters.lock().expect("fresh mutex");
            panic!("a thread dies while holding the waker hub");
        })
        .join();
        assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
        assert!(hub.waiters.lock().is_err(), "poison 됐어야 한다");

        let waiting = Arc::clone(&hub);
        let awaiting = std::thread::spawn(move || {
            waiting.await_terminal(1, &"t1".to_string(), Some(5_000), || {
                Some(TerminalSnapshot {
                    state: TaskState::Running,
                    result: None,
                    attempt_id: None,
                    skip: None,
                    revision: None,
                })
            })
        });

        let snapshot = TerminalSnapshot {
            state: TaskState::Succeeded,
            result: None,
            attempt_id: None,
            skip: None,
            revision: None,
        };
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let registered = !hub.lock_recovering().is_empty();
            if registered {
                break;
            }
        }
        hub.fire(1, &"t1".to_string(), snapshot);

        let outcome = awaiting.join().expect("await thread must not panic");
        assert!(
            matches!(outcome, AwaitOutcome::Terminal(s) if s.state == TaskState::Succeeded),
            "poison 이후에도 종결이 전달돼야 한다"
        );
    }
}
