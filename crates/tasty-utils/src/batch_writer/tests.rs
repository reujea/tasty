use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct Req {
    id: u32,
    bytes: usize,
    barrier: bool,
    key: Option<&'static str>,
    reject: bool,
}

fn req(id: u32) -> Req {
    Req {
        id,
        bytes: 10,
        barrier: false,
        key: None,
        reject: false,
    }
}

impl Request for Req {
    fn bytes(&self) -> usize {
        self.bytes
    }
    fn barrier(&self) -> bool {
        self.barrier
    }
    fn snapshot_key(&self) -> Option<&str> {
        self.key
    }
}

enum Script {
    Busy,
    Poison(usize),
    Fail,
    Unknown,
    Panic,
    /// 요청마다 따로 끝낸다. 'o' 확정, 'r' 거절, 'f' 실패, 'u' 결과 불명. 모자라면 결과를 덜 준다.
    Each(&'static str),
}

/// execute 호출을 허락한 수만큼만 통과시키는 시험 gate.
#[derive(Default)]
struct Gate {
    permits: Mutex<Option<usize>>,
    changed: Condvar,
    entered: AtomicUsize,
}

impl Gate {
    fn closed() -> Arc<Self> {
        let gate = Self::default();
        *gate.permits.lock().unwrap() = Some(0);
        Arc::new(gate)
    }
    fn allow(&self, n: usize) {
        if let Some(p) = self.permits.lock().unwrap().as_mut() {
            *p += n;
        }
        self.changed.notify_all();
    }
    fn open(&self) {
        *self.permits.lock().unwrap() = None;
        self.changed.notify_all();
    }
    fn pass(&self) {
        self.entered.fetch_add(1, Ordering::SeqCst);
        let mut permits = self.permits.lock().unwrap();
        loop {
            match permits.as_mut() {
                None => return,
                Some(n) if *n > 0 => {
                    *n -= 1;
                    return;
                }
                Some(_) => permits = self.changed.wait(permits).unwrap(),
            }
        }
    }
}

#[derive(Clone, Default)]
struct Probe {
    calls: Arc<Mutex<Vec<Vec<u32>>>>,
    committed: Arc<Mutex<Vec<u32>>>,
    script: Arc<Mutex<VecDeque<Script>>>,
    notified: Arc<AtomicUsize>,
}

impl Probe {
    fn calls(&self) -> Vec<Vec<u32>> {
        self.calls.lock().unwrap().clone()
    }
    fn committed(&self) -> Vec<u32> {
        self.committed.lock().unwrap().clone()
    }
    fn script(&self, s: Script) {
        self.script.lock().unwrap().push_back(s);
    }
}

struct Exec {
    probe: Probe,
    gate: Arc<Gate>,
}

impl Executor for Exec {
    type Request = Req;
    type Output = u32;
    type Error = String;

    fn execute(&mut self, batch: &mut [Req]) -> Execution<u32, String> {
        self.gate.pass();
        self.probe
            .calls
            .lock()
            .unwrap()
            .push(batch.iter().map(|r| r.id).collect());
        let script = self.probe.script.lock().unwrap().pop_front();
        match script {
            Some(Script::Busy) => return Execution::Busy("busy".into()),
            Some(Script::Poison(index)) => {
                return Execution::Poisoned {
                    index,
                    error: "poisoned".into(),
                };
            }
            Some(Script::Fail) => return Execution::Failed("full".into()),
            Some(Script::Unknown) => return Execution::Unknown("io at commit".into()),
            Some(Script::Panic) => panic!("executor panic for a test"),
            Some(Script::Each(marks)) => {
                let settled = batch
                    .iter()
                    .zip(marks.chars())
                    .map(|(r, mark)| match mark {
                        'o' => {
                            self.probe.committed.lock().unwrap().push(r.id);
                            Settled::Done(Ok(r.id))
                        }
                        'r' => Settled::Done(Err(format!("rejected {}", r.id))),
                        'f' => Settled::Failed(format!("write {} failed", r.id)),
                        _ => Settled::Unknown(format!("replace {} unknown", r.id)),
                    })
                    .collect();
                return Execution::Finished(settled);
            }
            None => {}
        }
        let results = batch
            .iter()
            .map(|r| {
                if r.reject {
                    Err(format!("rejected {}", r.id))
                } else {
                    self.probe.committed.lock().unwrap().push(r.id);
                    Ok(r.id)
                }
            })
            .collect();
        Execution::Committed(results)
    }

    fn output_bytes(_output: &u32) -> usize {
        4
    }
}

struct Rig {
    writer: Writer<Exec>,
    handle: Handle<Exec>,
    probe: Probe,
    gate: Arc<Gate>,
    clock: Arc<ManualClock>,
}

fn rig(limits: Limits, gate: Arc<Gate>) -> Rig {
    let probe = Probe::default();
    let clock = Arc::new(ManualClock::default());
    let notified = probe.notified.clone();
    let writer = Writer::spawn(
        "test-writer",
        Exec {
            probe: probe.clone(),
            gate: gate.clone(),
        },
        limits,
        clock.clone(),
        Arc::new(move || {
            notified.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .unwrap();
    let handle = writer.handle();
    Rig {
        writer,
        handle,
        probe,
        gate,
        clock,
    }
}

fn limits() -> Limits {
    Limits {
        max_delay: Duration::from_millis(2),
        max_items: 4,
        max_bytes: 100,
        queue_items: 16,
        queue_bytes: 1000,
        retry_for: Duration::from_millis(20),
        retry_backoff: Duration::from_millis(1),
        retry_backoff_max: Duration::from_millis(4),
    }
}

fn eventually(what: &str, cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    assert!(
        crate::test_wait::holds_by(deadline, cond),
        "timed out waiting for {what}"
    );
}

/// 실제 시간을 조금 흘려도 상태가 그대로인지 본다(worker는 1ms마다 다시 확인한다).
fn stays(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..30 {
        assert!(cond(), "{what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn done(c: Completion<u32, String>) -> Result<u32, String> {
    match c {
        Completion::Done(r) => r,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn finish(mut rig: Rig) {
    rig.gate.open();
    rig.writer.begin_shutdown();
    eventually("writer join", || rig.writer.poll_shutdown() == 0);
}

#[test]
fn submit_returns_before_the_gated_worker_commits_and_the_receipt_waits_for_the_commit() {
    let rig = rig(limits(), Gate::closed());
    let started = Instant::now();
    let receipts: Vec<_> = (1..=3)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        receipts.iter().map(Receipt::seq).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    rig.clock.advance(Duration::from_millis(2));
    eventually("worker reaches the gate", || {
        rig.gate.entered.load(Ordering::SeqCst) == 1
    });
    stays("no receipt completes before the commit", || {
        receipts.iter().all(|r| r.try_take().is_none())
    });
    assert!(rig.probe.committed().is_empty());
    rig.gate.allow(1);
    for (i, r) in receipts.iter().enumerate() {
        let got = r.wait_timeout(WAIT).expect("completes after the commit");
        assert_eq!(done(got), Ok(i as u32 + 1));
    }
    assert_eq!(rig.probe.calls(), [vec![1, 2, 3]]);
    finish(rig);
}

#[test]
fn a_lone_request_runs_alone_once_the_window_reaches_max_delay() {
    let rig = rig(limits(), Arc::new(Gate::default()));
    let r = rig.handle.submit(req(1)).unwrap();
    rig.clock.advance(Duration::from_micros(1999));
    stays("not before max_delay", || rig.probe.calls().is_empty());
    rig.clock.advance(Duration::from_micros(1));
    assert_eq!(done(r.wait_timeout(WAIT).unwrap()), Ok(1));
    assert_eq!(rig.probe.calls(), [vec![1]]);
    finish(rig);
}

#[test]
fn reaching_max_items_runs_the_batch_without_waiting_for_the_delay() {
    let rig = rig(limits(), Arc::new(Gate::default()));
    let receipts: Vec<_> = (1..=5)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    for r in &receipts[..4] {
        assert!(done(r.wait_timeout(WAIT).unwrap()).is_ok());
    }
    stays("the fifth waits for its own window", || {
        receipts[4].try_take().is_none()
    });
    assert_eq!(rig.probe.calls(), [vec![1, 2, 3, 4]]);
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(receipts[4].wait_timeout(WAIT).unwrap()).is_ok());
    assert_eq!(rig.probe.calls(), [vec![1, 2, 3, 4], vec![5]]);
    finish(rig);
}

#[test]
fn reaching_max_bytes_cuts_the_batch_and_an_oversized_request_runs_alone() {
    let rig = rig(limits(), Gate::closed());
    let sized = |id, bytes| Req { bytes, ..req(id) };
    let a = rig.handle.submit(sized(1, 60)).unwrap();
    let b = rig.handle.submit(sized(2, 50)).unwrap();
    let c = rig.handle.submit(sized(3, 150)).unwrap();
    let d = rig.handle.submit(sized(4, 10)).unwrap();
    rig.gate.allow(3);
    for r in [&a, &b, &c] {
        assert!(done(r.wait_timeout(WAIT).unwrap()).is_ok());
    }
    assert_eq!(rig.probe.calls(), [vec![1], vec![2], vec![3]]);
    stays("the small one after the oversized one waits", || {
        d.try_take().is_none()
    });
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(d.wait_timeout(WAIT).unwrap()).is_ok());
    finish(rig);
}

#[test]
fn a_barrier_closes_the_window_and_runs_alone_after_the_requests_before_it() {
    let rig = rig(limits(), Gate::closed());
    let read = Req {
        barrier: true,
        ..req(3)
    };
    let a = rig.handle.submit(req(1)).unwrap();
    let b = rig.handle.submit(req(2)).unwrap();
    let r = rig.handle.submit(read).unwrap();
    let c = rig.handle.submit(req(4)).unwrap();
    rig.gate.allow(2);
    for x in [&a, &b, &r] {
        assert!(done(x.wait_timeout(WAIT).unwrap()).is_ok());
    }
    assert_eq!(rig.probe.calls(), [vec![1, 2], vec![3]]);
    stays("a request after the barrier opens its own window", || {
        c.try_take().is_none()
    });
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(c.wait_timeout(WAIT).unwrap()).is_ok());
    finish(rig);
}

#[test]
fn a_flush_reports_its_cut_and_does_not_complete_requests_after_it() {
    let rig = rig(limits(), Gate::closed());
    let a = rig.handle.submit(req(1)).unwrap();
    let rejected = rig
        .handle
        .submit(Req {
            reject: true,
            ..req(2)
        })
        .unwrap();
    let flush = rig.handle.flush(FlushPoint::START).unwrap();
    let after = rig.handle.submit(req(3)).unwrap();
    rig.gate.allow(1);
    let first = flush.wait_timeout(WAIT).unwrap();
    let point = match first {
        Err(FlushFailure::NotClean {
            point,
            rejected: 1,
            failed: 0,
        }) => point,
        other => panic!("the rejected request must fail the flush: {other:?}"),
    };
    assert_eq!(point.seq, flush.seq());
    assert_eq!(flush.seq(), 3);
    assert!(done(a.try_take().unwrap()).is_ok());
    assert_eq!(done(rejected.try_take().unwrap()), Err("rejected 2".into()));
    stays("the request after the cut is still waiting", || {
        after.try_take().is_none()
    });
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    let second = rig.handle.flush(point).unwrap();
    assert_eq!(
        second.wait_timeout(WAIT).unwrap(),
        Ok(FlushPoint {
            seq: 5,
            rejected: 1,
            failed: 0
        }),
        "only the window after the previous cut counts"
    );
    assert!(done(after.try_take().unwrap()).is_ok());
    finish(rig);
}

#[test]
fn a_full_budget_refuses_at_once_and_unconsumed_results_keep_holding_it() {
    let mut l = limits();
    l.queue_items = 3;
    let rig = rig(l, Gate::closed());
    let held: Vec<_> = (1..=3)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    let started = Instant::now();
    assert_eq!(
        rig.handle.submit(req(4)).unwrap_err(),
        SubmitError::Saturated {
            items: 3,
            bytes: 30
        }
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    eventually("batch commits", || rig.probe.committed().len() == 3);
    eventually("results are counted", || {
        rig.handle.stats().held_bytes == 12
    });
    assert!(
        matches!(
            rig.handle.submit(req(5)),
            Err(SubmitError::Saturated { items: 3, .. })
        ),
        "a completed result nobody took still holds its place"
    );
    assert!(done(held[0].try_take().unwrap()).is_ok());
    let fifth = rig.handle.submit(req(6)).unwrap();
    drop(held);
    assert_eq!(rig.handle.stats().held_items, 1);
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(fifth.wait_timeout(WAIT).unwrap()).is_ok());
    let stats = rig.handle.stats();
    assert_eq!((stats.held_items, stats.held_bytes), (0, 0));
    assert_eq!(stats.rejected_saturated, 2);
    assert_eq!(stats.peak_items, 3);
    finish(rig);
}

#[test]
fn a_byte_budget_refuses_a_request_that_would_overflow_it() {
    let mut l = limits();
    l.queue_bytes = 25;
    let rig = rig(l, Gate::closed());
    let _a = rig.handle.submit(req(1)).unwrap();
    let _b = rig.handle.submit(req(2)).unwrap();
    assert_eq!(
        rig.handle.submit(req(3)).unwrap_err(),
        SubmitError::Saturated {
            items: 2,
            bytes: 20
        }
    );
    finish(rig);
}

#[test]
fn a_dropped_receipt_does_not_cancel_its_request_and_releases_the_budget_after_it_runs() {
    let rig = rig(limits(), Gate::closed());
    drop(rig.handle.submit(req(1)).unwrap());
    assert_eq!(rig.handle.stats().held_items, 1);
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    eventually("the abandoned request commits", || {
        rig.probe.committed() == [1]
    });
    eventually("its budget is released", || {
        rig.handle.stats().held_items == 0
    });
    finish(rig);
}

#[test]
fn busy_is_retried_in_place_and_later_requests_wait_behind_it() {
    let rig = rig(limits(), Gate::closed());
    rig.probe.script(Script::Busy);
    rig.probe.script(Script::Busy);
    let a = rig.handle.submit(req(1)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    rig.gate.allow(1);
    eventually("first attempt", || rig.probe.calls().len() == 1);
    let b = rig.handle.submit(req(2)).unwrap();
    rig.gate.open();
    assert!(done(a.wait_timeout(WAIT).unwrap()).is_ok());
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(b.wait_timeout(WAIT).unwrap()).is_ok());
    assert_eq!(rig.probe.calls(), [vec![1], vec![1], vec![1], vec![2]]);
    assert_eq!(rig.probe.committed(), [1, 2]);
    assert_eq!(rig.handle.stats().retries, 2);
    finish(rig);
}

#[test]
fn busy_past_the_retry_window_fails_the_batch_and_the_writer_goes_on() {
    let rig = rig(limits(), Arc::new(Gate::default()));
    for _ in 0..100 {
        rig.probe.script(Script::Busy);
    }
    let a = rig.handle.submit(req(1)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    match a.wait_timeout(WAIT).unwrap() {
        Completion::Failed(e) => assert_eq!(e, "busy"),
        other => panic!("{other:?}"),
    }
    // 시도 시각 0·1·3·7·11·15·19ms는 20ms 안이라 다시 실행하고, 23ms의 여덟 번째에서 멈춘다
    // (시계는 sleep만큼만 흐른다).
    assert_eq!(rig.probe.calls().len(), 8);
    assert_eq!(rig.handle.stats().retries, 7);
    rig.probe.script.lock().unwrap().clear();
    let b = rig.handle.submit(req(2)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(b.wait_timeout(WAIT).unwrap()).is_ok());
    finish(rig);
}

#[test]
fn a_poisoned_request_fails_alone_and_the_rest_runs_again() {
    let rig = rig(limits(), Gate::closed());
    rig.probe.script(Script::Poison(1));
    let rs: Vec<_> = (1..=3)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    assert!(matches!(rs[1].wait_timeout(WAIT).unwrap(), Completion::Failed(e) if e == "poisoned"));
    assert!(done(rs[0].wait_timeout(WAIT).unwrap()).is_ok());
    assert!(done(rs[2].wait_timeout(WAIT).unwrap()).is_ok());
    assert_eq!(rig.probe.calls(), [vec![1, 2, 3], vec![1, 3]]);
    finish(rig);
}

#[test]
fn failed_and_unknown_batches_end_every_request_and_are_not_run_again() {
    let rig = rig(limits(), Gate::closed());
    rig.probe.script(Script::Fail);
    let a = rig.handle.submit(req(1)).unwrap();
    rig.gate.allow(1);
    rig.clock.advance(Duration::from_millis(2));
    assert!(matches!(a.wait_timeout(WAIT).unwrap(), Completion::Failed(e) if e == "full"));
    rig.probe.script(Script::Unknown);
    let b = rig.handle.submit(req(2)).unwrap();
    let c = rig.handle.submit(req(3)).unwrap();
    rig.gate.allow(1);
    rig.clock.advance(Duration::from_millis(2));
    for r in [&b, &c] {
        assert!(matches!(
            r.wait_timeout(WAIT).unwrap(),
            Completion::Unknown(_)
        ));
    }
    assert_eq!(rig.probe.calls(), [vec![1], vec![2, 3]]);
    let flush = rig.handle.flush(FlushPoint::START).unwrap();
    rig.gate.open();
    assert!(matches!(
        flush.wait_timeout(WAIT).unwrap(),
        Err(FlushFailure::NotClean { failed: 3, .. })
    ));
    finish(rig);
}

#[test]
fn independent_results_keep_the_committed_requests_and_count_each_failure_once() {
    let rig = rig(limits(), Gate::closed());
    rig.probe.script(Script::Each("ofur"));
    let receipts: Vec<_> = (1..=4)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    rig.gate.allow(1);
    rig.clock.advance(Duration::from_millis(2));
    let results: Vec<_> = receipts
        .iter()
        .map(|r| r.wait_timeout(WAIT).unwrap())
        .collect();
    assert!(matches!(results[0], Completion::Done(Ok(1))));
    assert!(matches!(&results[1], Completion::Failed(e) if e == "write 2 failed"));
    assert!(matches!(results[2], Completion::Unknown(_)));
    assert!(matches!(&results[3], Completion::Done(Err(e)) if e == "rejected 4"));
    assert_eq!(rig.probe.calls(), [vec![1, 2, 3, 4]], "nothing ran again");
    assert_eq!(rig.probe.committed(), [1]);
    let flush = rig.handle.flush(FlushPoint::START).unwrap();
    rig.gate.open();
    assert!(matches!(
        flush.wait_timeout(WAIT).unwrap(),
        Err(FlushFailure::NotClean {
            rejected: 1,
            failed: 2,
            ..
        })
    ));
    finish(rig);
}

#[test]
fn independent_results_follow_the_replacement_and_a_missing_result_is_lost() {
    let rig = rig(limits(), Gate::closed());
    rig.probe.script(Script::Each("f"));
    let snap = |id| Req {
        key: Some("config.toml"),
        ..req(id)
    };
    let s1 = rig.handle.submit(snap(1)).unwrap();
    let other = rig.handle.submit(req(2)).unwrap();
    let s2 = rig.handle.submit(snap(3)).unwrap();
    rig.gate.allow(1);
    rig.clock.advance(Duration::from_millis(2));
    assert!(matches!(
        other.wait_timeout(WAIT).unwrap(),
        Completion::Failed(_)
    ));
    assert!(
        matches!(s2.wait_timeout(WAIT).unwrap(), Completion::Lost),
        "the executor gave one result for two requests"
    );
    assert!(matches!(
        s1.wait_timeout(WAIT).unwrap(),
        Completion::Superseded { outcome, .. } if matches!(*outcome, Completion::Lost)
    ));
    assert_eq!(rig.probe.calls(), [vec![2, 3]]);
    finish(rig);
}

#[test]
fn an_executor_panic_loses_its_batch_but_not_the_worker() {
    let rig = rig(limits(), Arc::new(Gate::default()));
    rig.probe.script(Script::Panic);
    let a = rig.handle.submit(req(1)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    assert!(matches!(a.wait_timeout(WAIT).unwrap(), Completion::Lost));
    let b = rig.handle.submit(req(2)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(b.wait_timeout(WAIT).unwrap()).is_ok());
    assert!(!rig.writer.worker_ended_unexpectedly());
    finish(rig);
}

#[test]
fn snapshots_merge_inside_one_window_only_and_the_replaced_one_takes_the_later_result() {
    let rig = rig(limits(), Gate::closed());
    let snap = |id, key| Req {
        key: Some(key),
        ..req(id)
    };
    let s1 = rig.handle.submit(snap(1, "settings")).unwrap();
    let other = rig.handle.submit(snap(2, "presets")).unwrap();
    let s2 = rig.handle.submit(snap(3, "settings")).unwrap();
    let barrier = rig
        .handle
        .submit(Req {
            barrier: true,
            ..req(4)
        })
        .unwrap();
    let s3 = rig.handle.submit(snap(5, "settings")).unwrap();
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    match s1.wait_timeout(WAIT).unwrap() {
        Completion::Superseded { by, outcome } => {
            assert_eq!(by, s2.seq());
            assert!(matches!(*outcome, Completion::Done(Ok(()))));
        }
        other => panic!("{other:?}"),
    }
    for r in [&other, &s2, &barrier] {
        assert!(done(r.wait_timeout(WAIT).unwrap()).is_ok());
    }
    rig.clock.advance(Duration::from_millis(2));
    assert!(done(s3.wait_timeout(WAIT).unwrap()).is_ok());
    assert_eq!(rig.probe.calls(), [vec![2, 3], vec![4], vec![5]]);
    let stats = rig.handle.stats();
    assert_eq!((stats.superseded, stats.batch_requests), (1, 5));
    let flush = rig.handle.flush(FlushPoint::START).unwrap();
    assert!(
        flush.wait_timeout(WAIT).unwrap().is_ok(),
        "a replaced request whose replacement committed is not a hole"
    );
    finish(rig);
}

#[test]
fn a_replaced_snapshot_shares_the_failure_of_its_replacement() {
    let rig = rig(limits(), Gate::closed());
    rig.probe.script(Script::Fail);
    let snap = |id| Req {
        key: Some("k"),
        ..req(id)
    };
    let s1 = rig.handle.submit(snap(1)).unwrap();
    let s2 = rig.handle.submit(snap(2)).unwrap();
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    assert!(matches!(
        s2.wait_timeout(WAIT).unwrap(),
        Completion::Failed(_)
    ));
    let replaced = s1.wait_timeout(WAIT).unwrap();
    assert!(!replaced.took_effect());
    assert!(matches!(
        replaced,
        Completion::Superseded { outcome, .. } if matches!(*outcome, Completion::Failed(_))
    ));
    finish(rig);
}

#[test]
fn shutdown_stops_admission_drains_accepted_requests_and_joins() {
    let mut rig = rig(limits(), Gate::closed());
    let accepted: Vec<_> = (1..=3)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    rig.writer.begin_shutdown();
    assert_eq!(rig.handle.submit(req(9)).unwrap_err(), SubmitError::Closed);
    let late_flush = rig.handle.flush(FlushPoint::START).unwrap();
    assert_eq!(rig.writer.poll_shutdown(), 1);
    rig.gate.open();
    eventually("join", || rig.writer.poll_shutdown() == 0);
    for r in &accepted {
        assert!(done(r.try_take().unwrap()).is_ok());
    }
    assert!(late_flush.try_take().unwrap().is_ok());
    assert_eq!(
        rig.handle.flush(FlushPoint::START).unwrap_err(),
        SubmitError::Closed
    );
    assert_eq!(rig.probe.committed(), [1, 2, 3]);
    assert_eq!(rig.handle.stats().rejected_closed, 1);
}

#[test]
fn close_stops_admission_but_keeps_running_and_flushing() {
    let rig = rig(limits(), Arc::new(Gate::default()));
    let a = rig.handle.submit(req(1)).unwrap();
    rig.writer.close();
    assert_eq!(rig.handle.submit(req(2)).unwrap_err(), SubmitError::Closed);
    let flush = rig.handle.flush(FlushPoint::START).unwrap();
    assert!(flush.wait_timeout(WAIT).unwrap().is_ok());
    assert!(done(a.try_take().unwrap()).is_ok());
    finish(rig);
}

#[test]
fn dropping_the_writer_does_not_block_on_a_stuck_worker() {
    let rig = rig(limits(), Gate::closed());
    let r = rig.handle.submit(req(1)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    eventually("worker is stuck in the executor", || {
        rig.gate.entered.load(Ordering::SeqCst) == 1
    });
    let Rig { writer, gate, .. } = rig;
    let started = Instant::now();
    drop(writer);
    assert!(started.elapsed() < Duration::from_secs(1));
    gate.open();
    assert!(done(r.wait_timeout(WAIT).unwrap()).is_ok());
}

#[test]
fn every_batch_wakes_the_owner_and_sizes_land_in_their_buckets() {
    let rig = rig(limits(), Gate::closed());
    let rs: Vec<_> = (1..=5)
        .map(|i| rig.handle.submit(req(i)).unwrap())
        .collect();
    rig.gate.open();
    rig.clock.advance(Duration::from_millis(2));
    for r in &rs {
        assert!(done(r.wait_timeout(WAIT).unwrap()).is_ok());
    }
    eventually("two wakes", || {
        rig.probe.notified.load(Ordering::SeqCst) == 2
    });
    let stats = rig.handle.stats();
    assert_eq!(stats.batches, 2);
    assert_eq!(stats.batch_sizes, [1, 1, 0, 0, 0, 0]);
    assert_eq!(stats.batch_size_max, 4);
    finish(rig);
}

mod shutdown_observation;

mod measurements;
