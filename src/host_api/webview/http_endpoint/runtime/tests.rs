//! Actual runtime/worker ownership, not GTK destruction or App publication evidence.
#[cfg(target_os = "linux")]
mod profile_admission;
#[cfg(target_os = "linux")]
mod profile_fixture;
use super::*;
#[cfg(target_os = "linux")]
use crate::host_api::webview::http_context::preparation::Preparation;
use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};
static TEST: Mutex<()> = Mutex::new(());
/// Limit for work that never ends. Slow work has no other deadline, and work that ended without
/// a result fails at once through `wait_result` or `ready`.
pub(in super::super) const HANG_LIMIT: Duration = Duration::from_secs(300);
/// Serializes the process-wide owner budget. A failed test is reported by its own panic: the next
/// test ignores the poison and starts only after the owners that test left have terminated.
pub(super) fn serial() -> MutexGuard<'static, ()> {
    let serial = TEST.lock().unwrap_or_else(PoisonError::into_inner);
    reaped();
    serial
}
/// Waits for a state with no result to lose: only a hang can fail it.
pub(super) fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + HANG_LIMIT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "{what}: hung, not finished within {HANG_LIMIT:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
/// Waits for a result of work that reports `ended`. Work publishes before it ends, so `ended` is
/// read first: if it was already true and there is still no result, the result was lost.
pub(super) fn wait_result<T>(
    what: &str,
    mut ended: impl FnMut() -> bool,
    mut poll: impl FnMut() -> Option<T>,
) -> T {
    let mut result = None;
    wait_for(what, || {
        let ended = ended();
        result = poll();
        assert!(
            result.is_some() || !ended,
            "{what}: lost, the work ended without a result"
        );
        result.is_some()
    });
    result.expect("wait_for returned after a result")
}
/// A setup whose worker calls `published` after it has delivered the result.
pub(super) struct Started {
    setup: Setup,
    published: Arc<AtomicBool>,
}
pub(super) fn start(current: Current) -> Started {
    let published = Arc::new(AtomicBool::new(false));
    let wake = published.clone();
    let setup = Setup::start(
        current,
        Arc::new(move || wake.store(true, Ordering::Release)),
    )
    .unwrap_or_else(|_| panic!("setup"));
    Started { setup, published }
}
pub(super) fn ready(started: &mut Started) -> NativeRuntime {
    let Started { setup, published } = started;
    wait_result(
        "runtime setup",
        || published.load(Ordering::Acquire),
        || setup.poll(),
    )
    .unwrap_or_else(|_| panic!("runtime setup"))
}
pub(super) fn reaped() {
    wait_for("retired runtime owners", || {
        retirement::reap().unwrap();
        retirement::owners() == 0
    });
}
/// All fixtures using the process-wide runtime owner budget share TEST through actual teardown.
pub(in super::super) fn with_runtime(run: impl FnOnce(NativeRuntime)) {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    run(ready(&mut setup));
    drop(setup);
    reaped();
}
#[test]
fn setup_capacity_and_retired_workers_remain_charged_until_actual_job_termination() {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    let current = runtime.current();
    let (done, started) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    assert!(
        runtime
            .submit((done, wait), |(done, wait)| {
                done.send(std::thread::current().id()).unwrap();
                wait.recv().unwrap();
            })
            .is_ok()
    );
    assert_ne!(
        started.recv_timeout(HANG_LIMIT).unwrap(),
        std::thread::current().id()
    );
    // The job stays parked until `release` below, so this Drop can only return without waiting
    // for it. Running it apart turns a Drop that waits into a failure instead of a stuck test.
    let dropped = std::thread::spawn(move || drop(runtime));
    wait_for("runtime Drop while its job runs", || dropped.is_finished());
    dropped.join().unwrap();
    assert!(!current());
    assert_eq!(retirement::owners(), 1);
    let mut pending = Vec::new();
    for _ in 0..7 {
        pending.push(
            Setup::start(Arc::new(|| true), Arc::new(|| {}))
                .unwrap_or_else(|_| panic!("bounded setup")),
        );
    }
    assert!(matches!(
        Setup::start(Arc::new(|| true), Arc::new(|| {})),
        Err(RuntimeError::Capacity)
    ));
    assert_eq!(retirement::owners(), 8);
    drop(pending);
    release.send(()).unwrap();
    drop(setup);
    reaped();
}
#[test]
fn bounded_blocking_admission_retains_exact_rejected_work_and_no_new_runtime_after_retirement() {
    let _serial = serial();
    let active = Arc::new(AtomicBool::new(true));
    let source = active.clone();
    let mut setup = start(Arc::new(move || source.load(Ordering::Acquire)));
    let runtime = ready(&mut setup);
    let mut release = Vec::new();
    for _ in 0..8 {
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(
            runtime
                .submit(rx, |rx| {
                    rx.recv().unwrap();
                })
                .is_ok()
        );
        release.push(tx);
    }
    assert_eq!(
        runtime.submit(73usize, |_| panic!("excess job executed")),
        Err(73)
    );
    active.store(false, Ordering::Release);
    assert_eq!(
        runtime.submit(74usize, |_| panic!("retired job executed")),
        Err(74)
    );
    assert!(matches!(
        Setup::start(Arc::new(|| false), Arc::new(|| {})),
        Err(RuntimeError::Retired)
    ));
    for tx in release {
        if tx.send(()).is_err() {
            tracing::debug!("retired queued job dropped before execution");
        }
    }
    drop(runtime);
    drop(setup);
    reaped();
}
#[cfg(target_os = "linux")]
#[test]
fn actual_private_profile_work_runs_on_bounded_pool_and_receipt_claim_keeps_lease() {
    let _serial = serial();
    let home = private_home();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    let handle = runtime.handle();
    let endpoints = runtime.endpoint_budget();
    assert_eq!(endpoints.active_owners(), 0);
    drop(handle);
    let profile = profile_fixture::attempt(&runtime, home.path())
        .unwrap_or_else(|error| panic!("private profile: {error}"));
    assert_eq!(
        std::fs::read_dir(home.path().join("native-http-profiles"))
            .unwrap()
            .count(),
        2, // Exactly one owned root and the namespace lock.
    );
    let root = profile.root().to_owned();
    assert!(root.is_dir());
    let lease = profile.into_native();
    drop(lease);
    assert!(root.is_dir());
    drop(runtime);
    drop(setup);
    reaped();
    assert!(root.is_dir());
    // No WebKit object was created. This fixture owns the temporary home and has joined every
    // profile worker before TempDir removes it; it is not native teardown reclamation proof.
}
#[cfg(target_os = "linux")]
#[test]
fn cancellation_before_profile_dispatch_performs_no_namespace_io() {
    let _serial = serial();
    let home = private_home();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    let (receipt, work) = Preparation::new(
        home.path().to_path_buf(),
        8,
        Duration::from_secs(1),
        runtime.current(),
        Arc::new(|| {}),
    );
    drop(receipt);
    assert!(runtime.spawn_profile(work).is_ok());
    drop(runtime);
    drop(setup);
    reaped();
    assert!(!home.path().join("native-http-profiles").exists());
}

#[cfg(target_os = "linux")]
fn private_home() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::Builder::new()
        .prefix("tasty-native-runtime-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    home
}
