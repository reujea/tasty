//! Actual bounded profile admission and an inherited-flock counterexample.
use super::*;
use crate::host_api::webview::http_context::{
    preparation::PrepareError,
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

pub(super) fn attempt(
    runtime: &NativeRuntime,
    home: &Path,
) -> Result<PreparedProfile, PrepareError> {
    let (receipt, work) = Preparation::new(
        home.to_path_buf(),
        8,
        Duration::from_secs(1),
        runtime.current(),
        Arc::new(|| {}),
    );
    assert!(runtime.spawn_profile(work).is_ok());
    let mut result = None;
    wait_for(|| {
        result = receipt.poll();
        result.is_some()
    });
    result.unwrap()
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
        let fd = lock.as_raw_fd();
        // SAFETY: fd is an owned live regular file; flock does not access Rust memory.
        assert_eq!(unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) }, 0);
        // Rust opens with CLOEXEC: deliberately demonstrate that fork still inherits it.
        // SAFETY: F_GETFD only inspects this live descriptor.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0);
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        let (release, wait) = UnixStream::pair().unwrap();
        let parent_fd = release.as_raw_fd();
        let child_fd = wait.as_raw_fd();
        // SAFETY: the child only closes its inherited parent socket, polls/reads one byte,
        // and _exit's. It never calls allocation, Rust destructors, or a non-async-safe API.
        let pid = unsafe { libc::fork() };
        assert!(
            pid >= 0,
            "fork fixture: {}",
            std::io::Error::last_os_error()
        );
        if pid == 0 {
            // SAFETY: close only the inherited parent-side socket in this child.
            unsafe { libc::close(parent_fd) };
            let mut poll = libc::pollfd {
                fd: child_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one live inherited socket and valid stack pollfd; bounded async-safe wait.
            let ready = unsafe { libc::poll(&mut poll, 1, 3000) };
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
    let _serial = TEST.lock().unwrap();
    reaped();
    let mut setup =
        Setup::start(Arc::new(|| true), Arc::new(|| {})).unwrap_or_else(|_| panic!("setup"));
    let runtime = ready(&mut setup);
    {
        let home = private_home();
        let mut child = InheritedLock::hold(home.path());
        assert!(matches!(
            attempt(&runtime, home.path()),
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
    let _serial = TEST.lock().unwrap();
    reaped();
    let mut setup =
        Setup::start(Arc::new(|| true), Arc::new(|| {})).unwrap_or_else(|_| panic!("setup"));
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
