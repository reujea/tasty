//! Contended production profile work keeps all actual runtime job permits until cancellation.
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    sync::{RwLock, atomic::AtomicUsize},
};
#[test]
fn busy_profile_work_retains_eight_job_permits_and_drop_returns_them() {
    with_runtime(|runtime| {
        let home = private_home();
        let namespace = home.path().join("native-http-profiles");
        fs::create_dir(&namespace).unwrap();
        fs::set_permissions(&namespace, fs::Permissions::from_mode(0o700)).unwrap();
        let mut lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(namespace.join(".profile-lock"))
            .unwrap();
        lock.write_all(b"tasty-private-profile-lock-v1\n").unwrap();
        assert_eq!(
            // SAFETY: fixture owns this descriptor through worker cancellation and permit return.
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let mut receipts = Vec::new();
        let (tx, checks) = mpsc::channel();
        // A worker checks current once before the namespace and again after each Busy attempt.
        // Parking it in that second check keeps it inside contention, and its admission deadline
        // unread, for as long as this test observes: the outcome cannot depend on elapsed time.
        let parked = Arc::new(RwLock::new(()));
        let parking = parked.write().unwrap();
        let observer = std::thread::current().id();
        for _ in 0..8 {
            let tx = tx.clone();
            let current = runtime.current();
            let parked = parked.clone();
            let checked = AtomicUsize::new(0);
            let (receipt, work) = Preparation::new(
                home.path().to_path_buf(),
                8,
                Duration::from_secs(2),
                Arc::new(move || {
                    if std::thread::current().id() != observer
                        && checked.fetch_add(1, Ordering::AcqRel) == 1
                    {
                        tx.send(()).unwrap();
                        drop(parked.read());
                    }
                    current()
                }),
                Arc::new(|| {}),
            );
            assert!(runtime.spawn_profile(work).is_ok());
            receipts.push(receipt);
        }
        for _ in 0..8 {
            checks.recv_timeout(HANG_LIMIT).unwrap();
        }
        assert_eq!(runtime.parts.jobs.available_permits(), 0);
        let (extra, work) = Preparation::new(
            home.path().to_path_buf(),
            8,
            Duration::from_secs(2),
            runtime.current(),
            Arc::new(|| {}),
        );
        let rejected = match runtime.spawn_profile(work) {
            Err(work) => work,
            Ok(()) => panic!("ninth job must refuse"),
        };
        drop(rejected);
        drop(extra);
        assert!(receipts.iter().all(|receipt| receipt.poll().is_none()));
        drop(receipts);
        drop(parking);
        wait_for("cancelled jobs returning permits", || {
            runtime.parts.jobs.available_permits() == 8
        });
        // The original lock is still held: no worker may have allocated or published a root.
        assert_eq!(fs::read_dir(namespace).unwrap().count(), 1);
        drop(lock);
    });
}

/// A cleanup waiting for the namespace lock keeps its job permit and the runtime's owner slot, and
/// retiring that runtime does not make its owner's thread wait: the setup worker does.
#[test]
fn unpublished_cleanup_stays_charged_without_blocking_the_runtime_owner() {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    let home = private_home();
    let published = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let wake = published.clone();
    let (receipt, work) = Preparation::new(
        home.path().to_path_buf(),
        8,
        crate::host_api::webview::http_context::preparation::MAX_CLAIM_WAIT,
        runtime.current(),
        Arc::new(move || wake.store(true, Ordering::Release)),
    );
    let attempts = work.discard_attempts();
    assert!(runtime.spawn_profile(work).is_ok());
    wait_for("profile work", || published.load(Ordering::Acquire));
    let namespace = home.path().join("native-http-profiles");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(namespace.join(".profile-lock"))
        .unwrap();
    wait_for("namespace lock", || {
        // SAFETY: fixture owns this descriptor until the cleanup is released below.
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
    });
    drop(receipt);
    wait_for("the cleanup's first Busy", || attempts.busy() > 0);
    let jobs = runtime.parts.jobs.clone();
    assert_eq!(jobs.available_permits(), 7);
    assert_eq!(retirement::owners(), 1);
    drop(runtime);
    drop(setup);
    assert_eq!(jobs.available_permits(), 7);
    assert_eq!(retirement::owners(), 1);
    assert_eq!(fs::read_dir(&namespace).unwrap().count(), 2);
    drop(lock);
    reaped();
    assert_eq!(jobs.available_permits(), 8);
    assert_eq!(fs::read_dir(&namespace).unwrap().count(), 1);
}
