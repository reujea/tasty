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
