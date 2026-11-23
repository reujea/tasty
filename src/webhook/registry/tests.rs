use super::*;
use crate::webhook::lifetime::{Limit, Persistence};
use std::time::{Duration, Instant};
use tasty_utils::{batch_writer, file_replace};
fn lifetime(persistent: bool, limit: Limit) -> Lifetime {
    Lifetime {
        persistence: if persistent {
            Persistence::Persistent
        } else {
            Persistence::Temporary
        },
        limit,
    }
}
fn registry() -> Registry {
    Registry::new(None, PathBuf::from("unused-temporary-webhook"))
}
fn await_result(r: &Registry, mut receipt: Receipt) -> Result<Completed, Failure> {
    let until = Instant::now() + Duration::from_secs(5);
    let mut value = None;
    // 원래 순서(r.poll → 읽기 → 기한 검사 → yield) 그대로, 기한 뒤 읽기는 r.poll 없이 영수증만 본다.
    let held = tasty_utils::test_wait::holds_by_every(until, Duration::ZERO, |last| {
        if !last {
            r.poll();
        }
        value = receipt.poll();
        value.is_some()
    });
    assert!(held, "webhook receipt deadline");
    value.unwrap()
}
fn register(r: &Registry, lt: Lifetime) -> RegisterOutcome {
    let result = await_result(r, r.register(vec!["POST".into()], None, vec![], lt, None)).unwrap();
    let Value::Registered(result) = result.value else {
        panic!("unexpected registration result");
    };
    result
}
fn matched(r: &Registry, id: &str, method: &str) -> MatchResult {
    r.match_request(id, method, 0, |_| true).0
}
fn remove(r: &Registry, id: &str) -> bool {
    let Value::Removed(value) = await_result(r, r.unregister(id)).unwrap().value else {
        panic!("unexpected removal result");
    };
    value
}
struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    registry: Arc<Registry>,
    writer: crate::app::file_writer::FileWriterParts,
}
impl Fixture {
    fn new() -> Self {
        Self::with_limits(Limits::default(), Limits::default())
    }
    fn with_limits(writer_limits: Limits, registry_limits: Limits) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("webhooks.toml");
        let writer = crate::app::file_writer::start(
            Arc::new(tasty_settings::FileSettingsStorage),
            writer_limits,
            Arc::new(batch_writer::SystemClock),
            Arc::new(|| {}),
        )
        .unwrap();
        let registry = Arc::new(Registry::with_limits(
            Some(Arc::new(writer.writer.handle())),
            path.clone(),
            registry_limits,
        ));
        Self {
            _dir: dir,
            path,
            registry,
            writer,
        }
    }
    fn drain(&mut self) {
        let until = Instant::now() + Duration::from_secs(5);
        // holds_by_every 로 바꾸지 않는다. 완료는 poll_shutdown 의 반환값으로만 보이고 그 poll 이 등록부 일을
        // 함께 처리해, 기한 뒤 진행 없이 관측할 저장 상태가 없다.
        while self.registry.poll_shutdown() != 0 {
            assert!(Instant::now() < until, "registry producer drain");
            std::thread::yield_now();
        }
        // writer 의 poll_shutdown 은 끝난 worker 를 join 만 하므로 기한 뒤 읽기에도 그대로 부른다.
        assert!(
            tasty_utils::test_wait::holds_by_every(until, Duration::ZERO, |_| self
                .writer
                .writer
                .poll_shutdown()
                == 0),
            "file writer drain"
        );
    }
}
#[test]
fn local_poison_recovery_does_not_use_a_global_registry() {
    let shared = Arc::new(Mutex::new(State::default()));
    let poison = shared.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = poison.lock().unwrap();
            panic!("owned poison");
        })
        .join()
        .is_err()
    );
    lock_state(&shared).bound = true;
    assert!(lock_state(&shared).bound);
    assert!(STATE_POISON_REPORTED.load(std::sync::atomic::Ordering::Relaxed));
}
#[test]
fn register_list_info_unregister_and_opaque_identity() {
    let r = registry();
    let a = register(&r, lifetime(false, Limit::Unlimited));
    let b = register(&r, lifetime(false, Limit::Unlimited));
    assert_eq!(a.id.len(), 16);
    assert_ne!(a.id, b.id);
    assert!(a.url.contains(&a.id));
    assert!(r.info(&a.id).is_some());
    assert_eq!(r.list().len(), 2);
    assert!(matches!(
        matched(&r, &a.id, "POST"),
        MatchResult::Matched { .. }
    ));
    assert!(matches!(
        matched(&r, &a.id, "GET"),
        MatchResult::MethodNotAllowed
    ));
    assert!(matches!(
        matched(&r, "absent", "POST"),
        MatchResult::NotFound
    ));
    assert!(remove(&r, &a.id));
    assert!(r.info(&a.id).is_none());
    assert!(!remove(&r, &a.id));
}
#[test]
fn temporary_count_and_time_keep_existing_policy() {
    let r = registry();
    let count = register(&r, lifetime(false, Limit::CountLimit { remaining: 2 }));
    assert!(matches!(
        matched(&r, &count.id, "GET"),
        MatchResult::MethodNotAllowed
    ));
    assert!(matches!(
        r.info(&count.id).unwrap().0.lifetime.limit,
        Limit::CountLimit { remaining: 2 }
    ));
    assert!(matches!(
        matched(&r, &count.id, "POST"),
        MatchResult::Matched { .. }
    ));
    assert!(matches!(
        r.info(&count.id).unwrap().0.lifetime.limit,
        Limit::CountLimit { remaining: 1 }
    ));
    assert!(matches!(
        matched(&r, &count.id, "POST"),
        MatchResult::Matched { .. }
    ));
    assert!(r.info(&count.id).is_none());
    let expired = register(&r, lifetime(false, Limit::TimeLimit { deadline_unix: 1 }));
    assert!(matches!(
        matched(&r, &expired.id, "POST"),
        MatchResult::Expired
    ));
    assert!(matches!(
        matched(&r, &expired.id, "POST"),
        MatchResult::NotFound
    ));
}
#[test]
fn sweep_is_ordered_and_reports_only_removed_ids() {
    let r = registry();
    let live = register(&r, lifetime(false, Limit::Unlimited));
    let expired = register(&r, lifetime(false, Limit::TimeLimit { deadline_unix: 1 }));
    let Value::Swept(ids) = await_result(&r, r.sweep()).unwrap().value else {
        panic!("sweep");
    };
    assert_eq!(ids, [expired.id]);
    assert!(r.info(&live.id).is_some());
    let Value::Swept(ids) = await_result(&r, r.sweep()).unwrap().value else {
        panic!("sweep");
    };
    assert!(ids.is_empty());
}
#[test]
fn failed_persistent_register_and_remove_leave_memory_and_original_unchanged() {
    let mut f = Fixture::new();
    let kept = register(&f.registry, lifetime(true, Limit::Unlimited));
    let broken = "format=2\nport=40123\n[[webhook]]\ninvalid toml\n";
    std::fs::write(&f.path, broken).unwrap();
    assert!(
        await_result(
            &f.registry,
            f.registry.register(
                vec!["POST".into()],
                None,
                vec![],
                lifetime(true, Limit::Unlimited),
                None
            )
        )
        .is_err()
    );
    assert_eq!(f.registry.list().len(), 1);
    assert!(await_result(&f.registry, f.registry.unregister(&kept.id)).is_err());
    assert!(f.registry.info(&kept.id).is_some());
    assert_eq!(std::fs::read_to_string(&f.path).unwrap(), broken);
    f.drain();
}
#[cfg(unix)]
#[test]
fn next_section_is_prepared_after_failure_and_never_republishes_failed_register() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    std::fs::write(&f.path, "format=2\nport=40123\n").unwrap();
    let held = file_replace::lock(&f.path).unwrap();
    let mut first = f.registry.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(true, Limit::Unlimited),
        None,
    );
    let second = f.registry.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(true, Limit::Unlimited),
        None,
    );
    assert!(first.poll().is_none());
    assert!(f.registry.list().is_empty());
    std::fs::set_permissions(&f.path, std::fs::Permissions::from_mode(0o400)).unwrap();
    drop(held);
    let until = Instant::now() + Duration::from_secs(5);
    assert!(tasty_utils::test_wait::holds_by(until, || f
        .writer
        .stats
        .snapshot()
        .operations
        .replace
        .failed
        != 0));
    // Repair before collector constructs the second section. It must exclude the first intent.
    std::fs::set_permissions(&f.path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(await_result(&f.registry, first).is_err());
    let Value::Registered(second) = await_result(&f.registry, second).unwrap().value else {
        panic!("second registration");
    };
    let table = tasty_settings::webhook_port_file::read_table(&f.path).unwrap();
    assert_eq!(table["port"].as_integer(), Some(40123));
    assert_eq!(table["webhook"].as_array().unwrap().len(), 1);
    assert_eq!(table["webhook"][0]["id"].as_str(), Some(second.id.as_str()));
    assert_eq!(f.registry.list().len(), 1);
    f.drain();
}
#[test]
fn secondary_receipt_count_and_payload_bytes_remain_bounded_until_consumed() {
    let limits = Limits {
        queue_items: 1,
        queue_bytes: 2048,
        ..Limits::default()
    };
    let r = Registry::with_limits(None, PathBuf::from("unused"), limits);
    let first = r.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(false, Limit::Unlimited),
        None,
    );
    assert_eq!(r.budget.usage().0, 1);
    let mut refused = r.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(false, Limit::Unlimited),
        None,
    );
    let Err(error) = refused.poll().unwrap() else {
        panic!("must refuse count saturation");
    };
    assert!(!error.unknown);
    drop(first);
    assert_eq!(r.budget.usage(), (0, 0));
    let mut large = r.register(
        vec!["POST".into()],
        None,
        vec![IpcCall {
            method: "notification.create".into(),
            params: serde_json::json!({"body":"x".repeat(4096)}),
        }],
        lifetime(false, Limit::Unlimited),
        None,
    );
    assert!(large.poll().unwrap().is_err());
    assert_eq!(r.budget.usage(), (0, 0));
}
#[test]
fn actual_post_replace_unknown_stops_later_sections_without_replay_or_cache_publication() {
    let mut f = Fixture::new();
    f.registry
        .panic_after_replace
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let first = f.registry.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(true, Limit::Unlimited),
        None,
    );
    let second = f.registry.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(true, Limit::Unlimited),
        None,
    );
    let Err(first) = await_result(&f.registry, first) else {
        panic!("unknown must not succeed");
    };
    assert!(first.unknown);
    assert!(f.registry.status().outcome_unknown);
    let Err(second) = await_result(&f.registry, second) else {
        panic!("unexecuted successor must fail");
    };
    assert!(!second.unknown);
    assert!(f.registry.list().is_empty());
    assert_eq!(
        tasty_settings::webhook_port_file::read_table(&f.path).unwrap()["webhook"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let counts = f.writer.stats.snapshot();
    assert_eq!(counts.operations.replace.succeeded, 1);
    assert_eq!(counts.commands.unknown, 1);
    assert!(await_result(&f.registry, f.registry.sweep()).is_err());
    assert_eq!(f.writer.stats.snapshot().operations.replace.attempts, 1);
    f.drain();
}
fn collect_http(
    r: &Registry,
    job: std::thread::JoinHandle<(MatchResult, Option<Lease>)>,
) -> (MatchResult, Option<Lease>) {
    let until = Instant::now() + Duration::from_secs(5);
    // 원래 순서(읽기 → r.poll → 기한 검사 → yield) 그대로, 기한 뒤 읽기는 r.poll 없이 본다.
    assert!(
        tasty_utils::test_wait::holds_by_every(until, Duration::ZERO, |last| job.is_finished()
            || {
                if !last {
                    r.poll();
                }
                false
            }),
        "HTTP receipt did not settle"
    );
    job.join().unwrap()
}
#[test]
fn known_http_save_failure_consumes_but_unadmitted_and_unknown_do_not_execute() {
    let mut f = Fixture::new();
    let kept = register(
        &f.registry,
        lifetime(true, Limit::CountLimit { remaining: 1 }),
    );
    std::fs::write(&f.path, "invalid toml [").unwrap();
    let r = f.registry.clone();
    let id = kept.id.clone();
    let job = std::thread::spawn(move || r.match_request(&id, "POST", 1024, |_| true));
    let (result, lease) = collect_http(&f.registry, job);
    assert!(matches!(result, MatchResult::Matched { .. }));
    assert!(f.registry.info(&kept.id).is_none());
    assert!(f.registry.status().dirty);
    drop(lease);
    assert_eq!(std::fs::read_to_string(&f.path).unwrap(), "invalid toml [");
    std::fs::remove_file(&f.path).unwrap();
    let kept = register(
        &f.registry,
        lifetime(true, Limit::CountLimit { remaining: 1 }),
    );
    assert!(!f.registry.status().dirty);
    f.registry
        .panic_after_replace
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let r = f.registry.clone();
    let id = kept.id.clone();
    let job = std::thread::spawn(move || r.match_request(&id, "POST", 1024, |_| true));
    let (result, lease) = collect_http(&f.registry, job);
    assert!(matches!(result, MatchResult::Unavailable));
    assert!(lease.is_some());
    drop(lease);
    assert!(matches!(
        f.registry.info(&kept.id).unwrap().0.lifetime.limit,
        Limit::CountLimit { remaining: 1 }
    ));
    assert!(matches!(
        matched(&f.registry, &kept.id, "POST"),
        MatchResult::Unavailable
    ));
    f.drain();
}
#[test]
fn held_write_leaves_reads_free_and_shutdown_retains_accepted_http_until_its_lease_drops() {
    let mut f = Fixture::new();
    let kept = register(
        &f.registry,
        lifetime(true, Limit::CountLimit { remaining: 2 }),
    );
    let held = file_replace::lock(&f.path).unwrap();
    let r = f.registry.clone();
    let id = kept.id.clone();
    let job = std::thread::spawn(move || r.match_request(&id, "POST", 1024, |_| true));
    let until = Instant::now() + Duration::from_secs(5);
    assert!(tasty_utils::test_wait::holds_by(until, || f
        .registry
        .budget
        .usage()
        .0
        != 0));
    assert!(!job.is_finished());
    assert_eq!(f.registry.list().len(), 1);
    assert!(f.registry.poll_shutdown() > 0);
    assert!(matches!(
        matched(&f.registry, &kept.id, "POST"),
        MatchResult::Unavailable
    ));
    drop(held);
    let (result, lease) = collect_http(&f.registry, job);
    assert!(matches!(result, MatchResult::Matched { .. }));
    assert!(
        f.registry.poll_shutdown() > 0,
        "ACK/execution context still owns a retained producer permit"
    );
    drop(lease);
    f.drain();
}
#[test]
fn a_shared_queue_refusal_keeps_http_count_and_returns_unavailable_without_disk_fallback() {
    let limits = Limits {
        queue_items: 1,
        ..Limits::default()
    };
    let mut f = Fixture::with_limits(limits, Limits::default());
    let registered = register(
        &f.registry,
        lifetime(true, Limit::CountLimit { remaining: 1 }),
    );
    let before = std::fs::read(&f.path).unwrap();
    let (started, begin) = std::sync::mpsc::sync_channel(1);
    let (release, held) = std::sync::mpsc::sync_channel(1);
    let gate = command::submit(
        f.registry.queue.as_deref(),
        128,
        move || {
            started.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
        },
        |_| 64,
    )
    .unwrap();
    begin.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        matched(&f.registry, &registered.id, "POST"),
        MatchResult::Unavailable
    ));
    assert!(matches!(
        f.registry.info(&registered.id).unwrap().0.lifetime.limit,
        Limit::CountLimit { remaining: 1 }
    ));
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    release.send(()).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    let mut result = None;
    assert!(tasty_utils::test_wait::holds_by(until, || {
        result = gate.poll();
        result.is_some()
    }));
    result.unwrap().unwrap();
    f.drain();
}
#[test]
fn restore_cleanup_is_a_receipt_barrier_before_admission_opens() {
    let mut f = Fixture::new();
    std::fs::write(&f.path,"format=2\n[[webhook]]\nid='live'\nmethods=['POST']\nlimit={kind='unlimited'}\n[[webhook]]\nid='expired'\nmethods=['POST']\nlimit={kind='time',deadline_unix=1}\n").unwrap();
    let held = file_replace::lock(&f.path).unwrap();
    f.registry.begin_restore();
    assert_eq!(
        crate::webhook::persist::restore_into_registry(&f.registry),
        1
    );
    let opened = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = opened.clone();
    f.registry
        .after_restore(move || flag.store(true, std::sync::atomic::Ordering::Release));
    assert!(!opened.load(std::sync::atomic::Ordering::Acquire));
    assert!(f.registry.info("live").is_some());
    assert!(f.registry.info("expired").is_none());
    assert!(matches!(
        matched(&f.registry, "live", "POST"),
        MatchResult::Unavailable
    ));
    drop(held);
    let until = Instant::now() + Duration::from_secs(5);
    // 원래 순서(읽기 → poll → 기한 검사 → yield) 그대로, 기한 뒤 읽기는 poll 없이 본다.
    assert!(tasty_utils::test_wait::holds_by_every(
        until,
        Duration::ZERO,
        |last| opened.load(std::sync::atomic::Ordering::Acquire) || {
            if !last {
                f.registry.poll();
            }
            false
        }
    ));
    let table = tasty_settings::webhook_port_file::read_table(&f.path).unwrap();
    assert_eq!(table["webhook"].as_array().unwrap().len(), 1);
    assert_eq!(table["webhook"][0]["id"].as_str(), Some("live"));
    assert_eq!(f.registry.budget.usage(), (0, 0));
    f.drain();
}
#[test]
fn a_large_retained_section_cannot_bypass_the_secondary_byte_budget() {
    let limits = Limits {
        queue_bytes: 4096,
        ..Limits::default()
    };
    let mut f = Fixture::with_limits(Limits::default(), limits);
    let entry = WebhookEntry {
        id: "large-restored".into(),
        methods: vec!["POST".into()],
        handler_id: None,
        calls: vec![IpcCall {
            method: "notification.create".into(),
            params: serde_json::json!({"body":"x".repeat(8192)}),
        }],
        lifetime: lifetime(true, Limit::CountLimit { remaining: 1 }),
        auth: None,
    };
    super::super::persist::WriteRequest::new(
        f.path.clone(),
        vec![super::super::persist::to_persisted(&entry)],
    )
    .execute()
    .unwrap();
    f.registry.restore_entry(entry);
    let original = std::fs::read(&f.path).unwrap();
    assert!(
        await_result(
            &f.registry,
            f.registry.register(
                vec!["POST".into()],
                None,
                vec![],
                lifetime(true, Limit::Unlimited),
                None
            )
        )
        .is_err()
    );
    assert!(matches!(
        matched(&f.registry, "large-restored", "POST"),
        MatchResult::Unavailable
    ));
    assert!(matches!(
        f.registry.info("large-restored").unwrap().0.lifetime.limit,
        Limit::CountLimit { remaining: 1 }
    ));
    assert_eq!(std::fs::read(&f.path).unwrap(), original);
    assert_eq!(f.writer.stats.snapshot().operations.replace.attempts, 0);
    assert_eq!(f.registry.budget.usage(), (0, 0));
    f.drain();
}
#[test]
fn stale_http_authorization_identity_cannot_consume_a_recreated_registration() {
    let r = registry();
    let first = register(&r, lifetime(false, Limit::CountLimit { remaining: 1 }));
    let identity = r.lock().entries[&first.id].identity.clone();
    assert!(remove(&r, &first.id));
    let entry = WebhookEntry {
        id: first.id.clone(),
        methods: vec!["POST".into()],
        handler_id: None,
        calls: vec![],
        lifetime: lifetime(false, Limit::CountLimit { remaining: 1 }),
        auth: None,
    };
    await_result(&r, r.submit(Operation::Register(entry), false, 0)).unwrap();
    let result = await_result(
        &r,
        r.submit(
            Operation::Match {
                id: first.id.clone(),
                method: "POST".into(),
                evidence: Some((identity, true)),
            },
            false,
            0,
        ),
    )
    .unwrap();
    assert!(matches!(
        result.value,
        Value::Matched(MatchResult::NotFound)
    ));
    assert!(matches!(
        r.info(&first.id).unwrap().0.lifetime.limit,
        Limit::CountLimit { remaining: 1 }
    ));
}
#[test]
fn status_counts_retained_receipts_and_never_contains_auth_or_payload_values() {
    let r = registry();
    let secret = "owned-credential-sentinel";
    let payload = "owned-payload-sentinel";
    let mut receipt = r.register(
        vec!["POST".into()],
        None,
        vec![IpcCall {
            method: "system.info".into(),
            params: serde_json::json!({"body":payload}),
        }],
        lifetime(false, Limit::Unlimited),
        Some(WebhookAuth {
            location: super::super::auth::AuthLocation::BearerHeader,
            token: secret.into(),
        }),
    );
    let status = r.status();
    assert_eq!(status.accepted, 1);
    assert_eq!(status.settled, 1);
    assert_eq!(status.retained_items, 1);
    assert!(status.retained_bytes > payload.len() + secret.len());
    let text = serde_json::to_string(&status).unwrap();
    assert!(!text.contains(secret));
    assert!(!text.contains(payload));
    assert!(receipt.poll().unwrap().is_ok());
    drop(receipt);
    assert_eq!(r.status().retained_items, 0);
    assert_eq!(r.status().retained_bytes, 0);
}
#[test]
fn webhook_producer_retirement_keeps_the_existing_preset_collector_progressing() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let parts = crate::app::file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .with_file_writer(parts)
        .build()
        .unwrap();
    let registry = core.webhooks.clone();
    let entry = register(&registry, lifetime(false, Limit::Unlimited));
    let (result, lease) = registry.match_request(&entry.id, "POST", 256, |_| true);
    assert!(matches!(result, MatchResult::Matched { .. }));
    let preset = core.preset_files.save(
        "owned-wreg-drain".into(),
        Some("owned-wreg-drain".into()),
        false,
        crate::app::preset_commands::tests::value("owned"),
    );
    let until = Instant::now() + Duration::from_secs(5);
    // holds_by_every 로 바꾸지 않는다. 성공을 정한 회차에도 종료 poll 로 producer 소유권을 단언해야 하는데
    // 그 poll 은 종료 진행이라, 기한 뒤 진행 없이 소유권을 관측할 경로가 없다.
    loop {
        assert!(
            core.poll_file_writer_shutdown() > 0,
            "HTTP producer lease still retains ownership"
        );
        if let Some(result) = preset.poll() {
            assert!(matches!(
                result,
                Ok(crate::intent::preset::SaveOutcome::Saved(_))
            ));
            break;
        }
        assert!(
            Instant::now() < until,
            "webhook wait starved the preset collector"
        );
        std::thread::yield_now();
    }
    drop(lease);
    // holds_by_every 로 바꾸지 않는다. 완료는 이 poll 의 반환값으로만 보이고 poll 은 종료 진행(재제출·회수)을
    // 함께 돌아, 기한 뒤 진행 없이 관측할 저장 상태가 없다.
    while core.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
}

#[test]
fn a_concurrent_poller_never_overtakes_publication_or_loses_an_admitted_successor() {
    let registry = Arc::new(registry());
    let (entered, started) = std::sync::mpsc::sync_channel(1);
    let (release, held) = std::sync::mpsc::sync_channel(1);
    let runner = registry.clone();
    let job = std::thread::spawn(move || {
        runner.after_restore(move || {
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
        })
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut next = registry.register(
        vec!["POST".into()],
        None,
        vec![],
        lifetime(false, Limit::Unlimited),
        None,
    );
    assert!(
        next.poll().is_none(),
        "later publication overtook the active notification owner"
    );
    assert!(
        registry.list().is_empty(),
        "short state reads must remain available"
    );
    release.send(()).unwrap();
    job.join().unwrap();
    assert!(
        next.poll().unwrap().is_ok(),
        "active owner lost the admission made during publication"
    );
    assert_eq!(registry.list().len(), 1);
}
