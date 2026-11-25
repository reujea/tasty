use super::*;

struct TimedExec {
    inner: Exec,
    clock: Arc<ManualClock>,
    costs: VecDeque<u64>,
}

impl Executor for TimedExec {
    type Request = Req;
    type Output = u32;
    type Error = String;
    fn execute(&mut self, requests: &mut [Req]) -> Execution<u32, String> {
        self.clock
            .advance(Duration::from_micros(self.costs.pop_front().unwrap()));
        self.inner.execute(requests)
    }
    fn output_bytes(_: &u32) -> usize {
        4
    }
}

#[test]
fn manual_clock_separates_batch_wait_executor_time_and_retry_backoff() {
    let probe = Probe::default();
    probe.script.lock().unwrap().push_back(Script::Busy);
    let gate = Gate::closed();
    let clock = Arc::new(ManualClock::default());
    let mut writer = Writer::spawn(
        "timed",
        TimedExec {
            inner: Exec {
                probe,
                gate: gate.clone(),
            },
            clock: clock.clone(),
            costs: [13, 17].into(),
        },
        Limits {
            max_items: 2,
            max_delay: Duration::from_secs(1),
            ..limits()
        },
        clock.clone(),
        Arc::new(|| {}),
    )
    .unwrap();
    let handle = writer.handle();
    let first = handle
        .submit(Req {
            key: Some("same"),
            ..req(1)
        })
        .unwrap();
    clock.advance(Duration::from_micros(20));
    let second = handle
        .submit(Req {
            key: Some("same"),
            ..req(2)
        })
        .unwrap();
    eventually("first execute gate", || {
        gate.entered.load(Ordering::SeqCst) == 1
    });
    let blocked = handle.stats();
    assert_eq!(
        (
            blocked.pending_items,
            blocked.pending_bytes,
            blocked.oldest_pending_age_us
        ),
        (2, 20, Some(33))
    );
    assert_eq!(
        (blocked.queued_items, blocked.queued_bytes),
        (0, 0),
        "both coalesced requests are in flight"
    );
    gate.open();
    // 분포 기록은 표본 수·합·최댓값·구간을 차례로 올리므로 단정할 필드가 모두 찰 때까지 기다린다.
    eventually("batch measured", || {
        let b = handle.stats().batch_execution;
        b.samples == 1 && b.sum_us != 0 && b.max_us != 0 && b.counts.iter().sum::<u64>() == 1
    });
    let stats = handle.stats();
    assert_eq!(
        (
            stats.admitted_bytes,
            stats.batch_bytes,
            stats.batch_bytes_max
        ),
        (20, 20, 20)
    );
    assert_eq!(
        (
            stats.batches,
            stats.batch_requests,
            stats.superseded,
            stats.retries
        ),
        (1, 2, 1, 1)
    );
    assert_eq!(
        (
            stats.batch_wait.samples,
            stats.batch_wait.sum_us,
            stats.batch_wait.max_us
        ),
        (1, 20, 20)
    );
    assert_eq!(stats.batch_wait.counts[1], 1);
    assert_eq!(
        (
            stats.executor_execution.samples,
            stats.executor_execution.sum_us,
            stats.executor_execution.max_us
        ),
        (2, 30, 17)
    );
    assert_eq!(stats.executor_execution.counts[1], 2);
    assert_eq!(
        (
            stats.batch_execution.samples,
            stats.batch_execution.sum_us,
            stats.batch_execution.max_us
        ),
        (1, 1030, 1030)
    );
    assert_eq!(stats.batch_execution.counts[5], 1);
    assert_eq!(
        (
            stats.completed_ok,
            stats.pending_items,
            stats.pending_bytes,
            stats.oldest_pending_age_us
        ),
        (2, 0, 0, None)
    );
    assert_eq!(
        (stats.held_items, stats.held_bytes),
        (2, 4),
        "unread results still occupy the existing budget"
    );
    assert!(matches!(
        first.try_take(),
        Some(Completion::Superseded { .. })
    ));
    assert!(matches!(second.try_take(), Some(Completion::Done(Ok(2)))));
    assert_eq!(handle.stats().held_items, 0);
    eventually("join", || writer.poll_shutdown() == 0);
    assert_eq!(
        handle.stats().batch_execution.samples,
        1,
        "flush is not a measured request batch"
    );
}

#[test]
fn abandoned_receipt_keeps_pending_age_until_terminal_unknown_outcome() {
    let mut rig = rig(limits(), Gate::closed());
    rig.probe.script.lock().unwrap().push_back(Script::Unknown);
    let receipt = rig.handle.submit(req(1)).unwrap();
    rig.clock.advance(Duration::from_millis(5));
    eventually("execute", || rig.gate.entered.load(Ordering::SeqCst) == 1);
    drop(receipt);
    let stats = rig.handle.stats();
    assert_eq!(
        (
            stats.pending_items,
            stats.pending_bytes,
            stats.oldest_pending_age_us
        ),
        (1, 10, Some(5000))
    );
    rig.gate.open();
    // 종결 수는 결과 칸이 예산을 돌려주기 전에 오르므로 held_items까지 기다린다.
    eventually("completion", || {
        let s = rig.handle.stats();
        s.completed_unknown == 1 && s.held_items == 0
    });
    let stats = rig.handle.stats();
    assert_eq!(
        (
            stats.pending_items,
            stats.pending_bytes,
            stats.oldest_pending_age_us,
            stats.held_items
        ),
        (0, 0, None, 0)
    );
    assert_eq!(
        stats.completed_ok, 0,
        "unknown is not durable success or permanent generic dirty state"
    );
    eventually("join", || rig.writer.poll_shutdown() == 0);
}

#[test]
fn independent_results_and_caught_executor_panic_have_distinct_terminal_counts() {
    let mut rig = rig(limits(), Gate::closed());
    rig.probe
        .script
        .lock()
        .unwrap()
        .push_back(Script::Each("orfu"));
    let receipts: Vec<_> = (1..=4)
        .map(|id| rig.handle.submit(req(id)).unwrap())
        .collect();
    rig.gate.open();
    eventually("mixed results", || rig.handle.stats().pending_items == 0);
    let s = rig.handle.stats();
    assert_eq!(
        (
            s.completed_ok,
            s.completed_rejected,
            s.completed_failed,
            s.completed_unknown,
            s.completed_lost
        ),
        (1, 1, 1, 1, 0)
    );
    rig.probe.script.lock().unwrap().push_back(Script::Panic);
    let lost = rig.handle.submit(req(5)).unwrap();
    rig.clock.advance(Duration::from_millis(2));
    // 종결 수는 결과 칸이 채워지기 전에 오르므로 결과를 먼저 기다린다.
    assert!(matches!(lost.wait_timeout(WAIT), Some(Completion::Lost)));
    assert_eq!(rig.handle.stats().completed_lost, 1);
    assert_eq!(rig.handle.stats().executor_execution.samples, 2);
    drop(receipts);
    eventually("join", || rig.writer.poll_shutdown() == 0);
}

#[test]
fn actual_worker_panic_accounts_queued_lost_work_even_when_receipts_are_abandoned() {
    let gate = Gate::closed();
    let clock = Arc::new(ManualClock::default());
    let mut writer = Writer::spawn(
        "owned-metric-panic",
        Exec {
            probe: Probe::default(),
            gate: gate.clone(),
        },
        Limits {
            max_items: 1,
            ..limits()
        },
        clock,
        Arc::new(|| panic!("owned notifier fault")),
    )
    .unwrap();
    let handle = writer.handle();
    let first = handle.submit(req(1)).unwrap();
    eventually("first started", || gate.entered.load(Ordering::SeqCst) == 1);
    let queued = handle.submit(req(2)).unwrap();
    drop((first, queued));
    assert_eq!(handle.stats().pending_items, 2);
    assert_eq!(
        (handle.stats().queued_items, handle.stats().queued_bytes),
        (1, 10)
    );
    gate.open();
    eventually("panic join", || writer.poll_shutdown() == 0);
    let s = handle.stats();
    assert_eq!(
        (
            s.completed_ok,
            s.completed_lost,
            s.pending_items,
            s.pending_bytes,
            s.held_items
        ),
        (1, 1, 0, 0, 0)
    );
    assert_eq!(s.oldest_pending_age_us, None);
    assert_eq!((s.queued_items, s.queued_bytes), (0, 0));
}

#[test]
fn rejected_admission_never_enters_pending_tracker_and_abandon_does_not_free_it_early() {
    let mut rig = rig(
        Limits {
            queue_items: 2,
            ..limits()
        },
        Gate::closed(),
    );
    let first = rig.handle.submit(req(1)).unwrap();
    let second = rig.handle.submit(req(2)).unwrap();
    assert_eq!(
        (
            rig.handle.stats().queued_items,
            rig.handle.stats().queued_bytes
        ),
        (2, 20)
    );
    assert!(matches!(
        rig.handle.submit(req(3)),
        Err(SubmitError::Saturated { .. })
    ));
    drop((first, second));
    assert_eq!(
        (
            rig.handle.stats().admitted_bytes,
            rig.handle.stats().pending_items
        ),
        (20, 2)
    );
    assert!(matches!(
        rig.handle.submit(req(4)),
        Err(SubmitError::Saturated { .. })
    ));
    rig.writer.begin_shutdown();
    assert!(matches!(
        rig.handle.submit(req(5)),
        Err(SubmitError::Closed)
    ));
    rig.gate.open();
    eventually("join", || rig.writer.poll_shutdown() == 0);
    let s = rig.handle.stats();
    assert_eq!(
        (
            s.admitted,
            s.completed_ok,
            s.pending_items,
            s.held_items,
            s.rejected_saturated,
            s.rejected_closed
        ),
        (2, 2, 0, 0, 2, 1)
    );
}

#[test]
fn histogram_inclusive_edges_submicrosecond_zero_and_overflow_are_bounded() {
    let histogram = super::super::measurement::DurationStats::default();
    histogram.record(Duration::from_nanos(999));
    for edge in WRITER_LATENCY_BOUNDS_US {
        histogram.record(Duration::from_micros(edge));
    }
    histogram.record(Duration::from_micros(1_000_001));
    let s = histogram.snapshot();
    assert_eq!(s.samples, 13);
    assert_eq!(s.counts[0], 2);
    assert!(s.counts[1..].iter().all(|&n| n == 1));
    assert_eq!(s.counts.iter().sum::<u64>(), s.samples);
    assert_eq!(s.max_us, 1_000_001);
    assert_eq!(
        super::super::measurement::DurationStats::default().snapshot(),
        Default::default()
    );
}
