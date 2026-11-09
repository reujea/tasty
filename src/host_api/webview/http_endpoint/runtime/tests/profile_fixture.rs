//! Actual bounded profile admission and an inherited-flock counterexample.
use super::*;
use crate::host_api::webview::http_context::{
    preparation::{MAX_CLAIM_WAIT, PrepareError},
    profile::{PreparedProfile, ProfileError},
};
use std::{
    fs::OpenOptions,
    io::Write,
    os::{
        fd::AsRawFd,
        unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            net::UnixStream,
        },
    },
    path::Path,
};

/// Runs real profile work to its result. `budget` is the product's admission and claim wait: only
/// a held namespace lock spends it to expiry, and a fixture holding that lock cannot then succeed.
fn attempt_within(
    runtime: &NativeRuntime,
    home: &Path,
    budget: Duration,
) -> Result<PreparedProfile, PrepareError> {
    let (receipt, work) = Preparation::new(
        home.to_path_buf(),
        8,
        budget,
        runtime.current(),
        Arc::new(|| {}),
    );
    assert!(runtime.spawn_profile(work).is_ok());
    // submit charged this job before returning, so all permits back means this job has ended too.
    wait_result(
        "profile work",
        || runtime.parts.jobs.available_permits() == 8,
        || receipt.poll(),
    )
}
/// Nothing holds the namespace lock long here, so the largest product budget leaves the outcome
/// independent of how long the work takes.
pub(super) fn attempt(
    runtime: &NativeRuntime,
    home: &Path,
) -> Result<PreparedProfile, PrepareError> {
    attempt_within(runtime, home, MAX_CLAIM_WAIT)
}

// The child uses only async-signal-safe syscalls after fork, never Rust unwinding or locks.
// Sending one byte releases this exact owned PID even when the assertion unwinds.
struct InheritedLock {
    release: Option<UnixStream>,
    pid: libc::pid_t,
}
impl InheritedLock {
    fn hold(home: &Path) -> Self {
        let namespace = home.join("native-http-profiles");
        std::fs::create_dir(&namespace).unwrap();
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(namespace.join(".profile-lock"))
            .unwrap();
        lock.set_permissions(std::fs::Permissions::from_mode(0o600))
            .unwrap();
        lock.write_all(b"tasty-private-profile-lock-v1\n").unwrap();
        // SAFETY: the descriptor is owned and live; flock does not access Rust memory.
        let locked = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        assert_eq!(locked, 0);
        Self::inherit(lock)
    }
    /// Takes the existing namespace lock of a prepared `home` into a forked child. A parallel
    /// test's child can hold that description for a moment, so only that refusal is retried.
    fn take(home: &Path) -> Self {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(home.join("native-http-profiles/.profile-lock"))
            .unwrap();
        wait_for("namespace lock for the owned child", || {
            // SAFETY: the descriptor is owned and live; flock does not access Rust memory.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return true;
            }
            let error = std::io::Error::last_os_error();
            assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock, "{error}");
            false
        });
        Self::inherit(lock)
    }
    /// Leaves the locked description of `lock` only in a forked child.
    fn inherit(lock: std::fs::File) -> Self {
        let fd = lock.as_raw_fd();
        // Rust opens with CLOEXEC: deliberately demonstrate that fork still inherits it.
        // SAFETY: F_GETFD only inspects this live descriptor.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0);
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        let (release, wait) = UnixStream::pair().unwrap();
        let child_fd = wait.as_raw_fd();
        // fork copies every descriptor of this multi-threaded test process, including lock
        // descriptions that parallel tests hold only briefly. The child keeps just this
        // fixture's lock and its own socket so that it never holds another test's namespace
        // lock for its whole lifetime.
        let (low, high) = (fd.min(child_fd) as u32, fd.max(child_fd) as u32);
        // SAFETY: the child only closes inherited descriptors, polls/reads one byte, and
        // _exit's. It never calls allocation, Rust destructors, or a non-async-safe API.
        let pid = unsafe { libc::fork() };
        assert!(
            pid >= 0,
            "fork fixture: {}",
            std::io::Error::last_os_error()
        );
        if pid == 0 {
            // close_range takes inclusive bounds and rejects an empty range; skip those.
            for (first, last) in [
                (3, low.wrapping_sub(1)),
                (low + 1, high - 1),
                (high + 1, u32::MAX),
            ] {
                if first <= last {
                    // SAFETY: closes only inherited descriptors in this child; async-signal-safe.
                    unsafe { libc::syscall(libc::SYS_close_range, first, last, 0u32) };
                }
            }
            let mut poll = libc::pollfd {
                fd: child_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // The fixture relies on this child holding the lock until release, however slow the
            // test is. Parent exit closes the socket, so the wait still ends without a release.
            // SAFETY: one live inherited socket and valid stack pollfd; async-safe wait.
            let ready = unsafe { libc::poll(&mut poll, 1, -1) };
            let mut byte = 0u8;
            let read = if ready > 0 {
                // SAFETY: live socket and valid stack byte; read is async-signal-safe after fork.
                unsafe { libc::read(child_fd, (&mut byte as *mut u8).cast(), 1) }
            } else {
                -1
            };
            // SAFETY: terminate only this owned child without Rust destructors or unwinding.
            unsafe { libc::_exit(if read == 1 && byte == 1 { 0 } else { 10 }) };
        }
        drop(wait);
        drop(lock); // The parent no longer owns the locked open-file description.
        Self {
            release: Some(release),
            pid,
        }
    }
    fn release(&mut self) {
        if let Some(mut release) = self.release.take() {
            let sent = release.write_all(&[1]);
            drop(release);
            let mut status = 0;
            let waited = loop {
                // SAFETY: exact owned PID, valid status pointer; retry only syscall interruption.
                let result = unsafe { libc::waitpid(self.pid, &mut status, 0) };
                if result >= 0
                    || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                {
                    break result;
                }
            };
            assert!(sent.is_ok(), "owned child release: {sent:?}");
            assert_eq!(waited, self.pid);
            assert_eq!(status, 0);
        }
    }
}
impl Drop for InheritedLock {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn inherited_cloexec_lock_refuses_real_profile_work_until_owned_child_exits() {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    {
        let home = private_home();
        let mut child = InheritedLock::hold(home.path());
        assert!(matches!(
            attempt_within(&runtime, home.path(), Duration::from_secs(1)),
            Err(PrepareError::AdmissionTimeout)
        ));
        assert_eq!(
            std::fs::read_dir(home.path().join("native-http-profiles"))
                .unwrap()
                .count(),
            1
        );
        child.release();
        let profile = attempt(&runtime, home.path()).unwrap();
        tracing::info!(pid = child.pid, "owned inherited-lock child reaped");
        assert_eq!(
            std::fs::read_dir(home.path().join("native-http-profiles"))
                .unwrap()
                .count(),
            2
        );
        assert!(profile.root().is_dir());
        drop(profile.into_native());
        drop(runtime);
        drop(setup);
        reaped();
    }
}

#[test]
fn populated_namespace_capacity_remains_terminal_in_real_worker() {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    {
        let home = private_home();
        let namespace = home.path().join("native-http-profiles");
        std::fs::create_dir(&namespace).unwrap();
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
        for i in 0..8 {
            std::fs::create_dir(namespace.join(format!("unknown-{i}"))).unwrap();
        }
        assert!(matches!(
            attempt(&runtime, home.path()),
            Err(PrepareError::Storage(ProfileError::Capacity))
        ));
        assert_eq!(std::fs::read_dir(namespace).unwrap().count(), 9);
        drop(runtime);
        drop(setup);
        reaped();
    }
}

/// The first Busy of the cleanup comes from a real child keeping the lock description past the
/// parent's close, the state a fork leaves until exec. The worker outlasts it and removes the root.
#[test]
fn unpublished_cleanup_outlasts_a_lock_description_kept_by_a_forked_child() {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    {
        let home = private_home();
        let published = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wake = published.clone();
        let (receipt, work) = Preparation::new(
            home.path().to_path_buf(),
            8,
            MAX_CLAIM_WAIT,
            runtime.current(),
            Arc::new(move || wake.store(true, Ordering::Release)),
        );
        let attempts = work.discard_attempts();
        assert!(runtime.spawn_profile(work).is_ok());
        wait_for("profile work", || published.load(Ordering::Acquire));
        let mut child = InheritedLock::take(home.path());
        drop(receipt);
        // Released only after a real Busy, or after the job gave up: a release before the first
        // attempt would pass even with no retry.
        wait_for("the cleanup's first Busy", || {
            attempts.busy() > 0 || runtime.parts.jobs.available_permits() == 8
        });
        child.release();
        wait_for("the job's permit", || {
            runtime.parts.jobs.available_permits() == 8
        });
        assert_eq!(
            std::fs::read_dir(home.path().join("native-http-profiles"))
                .unwrap()
                .count(),
            1,
            "the unpublished root must be gone, only the lock file stays"
        );
        drop(runtime);
        drop(setup);
        reaped();
    }
}
