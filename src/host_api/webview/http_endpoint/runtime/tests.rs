//! Actual runtime/worker ownership, not GTK destruction or App publication evidence.
#[cfg(target_os = "linux")]
mod profile_admission;
#[cfg(target_os = "linux")]
mod profile_fixture;
use super::*;
#[cfg(target_os = "linux")]
use crate::host_api::webview::http_context::preparation::Preparation;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
pub(super) static TEST: Mutex<()> = Mutex::new(());
pub(super) fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "bounded runtime fixture wait");
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn ready(setup: &mut Setup) -> NativeRuntime {
    let mut result = None;
    wait_for(|| {
        result = setup.poll();
        result.is_some()
    });
    result.unwrap().unwrap_or_else(|_| panic!("runtime setup"))
}
pub(super) fn reaped() {
    wait_for(|| {
        retirement::reap().unwrap();
        retirement::owners() == 0
    });
}
/// All fixtures using the process-wide runtime owner budget share TEST through actual teardown.
pub(in super::super) fn with_runtime(run: impl FnOnce(NativeRuntime)) {
    let _serial = TEST.lock().unwrap();
    reaped();
    let mut setup =
        Setup::start(Arc::new(|| true), Arc::new(|| {})).unwrap_or_else(|_| panic!("setup"));
    run(ready(&mut setup));
    drop(setup);
    reaped();
}
#[test]
fn setup_capacity_and_retired_workers_remain_charged_until_actual_job_termination() {
    let _serial = TEST.lock().unwrap();
    reaped();
    let mut setup =
        Setup::start(Arc::new(|| true), Arc::new(|| {})).unwrap_or_else(|_| panic!("setup"));
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
        started.recv_timeout(Duration::from_secs(1)).unwrap(),
        std::thread::current().id()
    );
    let began = Instant::now();
    drop(runtime);
    assert!(began.elapsed() < Duration::from_millis(100));
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
    let _serial = TEST.lock().unwrap();
    reaped();
    let active = Arc::new(AtomicBool::new(true));
    let source = active.clone();
    let mut setup = Setup::start(
        Arc::new(move || source.load(Ordering::Acquire)),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("setup"));
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
    let _serial = TEST.lock().unwrap();
    reaped();
    let home = private_home();
    let mut setup =
        Setup::start(Arc::new(|| true), Arc::new(|| {})).unwrap_or_else(|_| panic!("setup"));
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
    let _serial = TEST.lock().unwrap();
    reaped();
    let home = private_home();
    let mut setup =
        Setup::start(Arc::new(|| true), Arc::new(|| {})).unwrap_or_else(|_| panic!("setup"));
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
