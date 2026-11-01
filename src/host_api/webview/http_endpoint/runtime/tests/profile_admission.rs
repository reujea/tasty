//! Contended production profile work keeps all actual runtime job permits until cancellation.
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
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
        for _ in 0..8 {
            let tx = tx.clone();
            let current = runtime.current();
            let (receipt, work) = Preparation::new(
                home.path().to_path_buf(),
                8,
                Duration::from_secs(2),
                Arc::new(move || {
                    tx.send(()).unwrap();
                    current()
                }),
                Arc::new(|| {}),
            );
            assert!(runtime.spawn_profile(work).is_ok());
            receipts.push(receipt);
        }
        for _ in 0..8 {
            checks.recv_timeout(Duration::from_secs(2)).unwrap();
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
        wait_for(|| runtime.parts.jobs.available_permits() == 8);
        // The original lock is still held: no worker may have allocated or published a root.
        assert_eq!(fs::read_dir(namespace).unwrap().count(), 1);
        drop(lock);
    });
}
