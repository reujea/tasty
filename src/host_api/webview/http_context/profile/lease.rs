//! The kernel's answer to "does anything hold this file open", which needs no permission over the
//! holder. `F_SETLEASE(F_WRLCK)` was measured on ext4 and tmpfs to be refused while another open or
//! mapping of the inode exists, for a process of any UID or PID namespace that `/proc` would not show.
//!
//! While a lease is held, another process opening the file blocks until the lease is released or
//! `lease-break-time` passes, and this process receives `SIGIO`, which by default kills it.
use super::{ProfileError, fs::Dir};
use std::{ffi::OsStr, fs::File, io, os::fd::AsRawFd, sync::OnceLock};

/// The two kernel answers. Only tests pass anything but [`KERNEL`], to answer what a real file
/// system of this machine cannot be made to answer, or to act while the leases are held.
pub(super) struct LeaseOps<'a> {
    pub set: &'a dyn Fn(&File) -> io::Result<()>,
    pub get: &'a dyn Fn(&File) -> io::Result<i32>,
}
pub(super) const KERNEL: LeaseOps<'static> = LeaseOps {
    set: &set_write,
    get: &get,
};
pub(super) fn set_write(file: &File) -> io::Result<()> {
    set(file, libc::F_WRLCK)
}
pub(super) fn get(file: &File) -> io::Result<i32> {
    // SAFETY: file owns a live descriptor; F_GETLEASE takes no pointer.
    let held = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLEASE) };
    if held < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(held)
}
fn set(file: &File, kind: i32) -> io::Result<()> {
    // SAFETY: file owns a live descriptor; F_SETLEASE takes an integer.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLEASE, kind) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// Held until after the deletion, so that nothing can take turns opening the files one at a time.
#[derive(Default)]
pub(super) struct Leases(Vec<File>);
impl Leases {
    /// Opens an entry of an inspected tree and leases it, if it is still that entry. An open in this
    /// process would break a lease it already holds, so a second link of an inode has to be skipped.
    pub fn take(
        &mut self,
        dir: &Dir,
        name: &OsStr,
        id: super::fs::Object,
        ops: &LeaseOps<'_>,
    ) -> Result<(), ProfileError> {
        let file = dir.open_for_lease(name)?;
        let m = file.metadata()?;
        if !m.is_file() || super::fs::Object::from(&m) != id {
            return Err(ProfileError::Ownership);
        }
        (ops.set)(&file)?;
        self.0.push(file);
        Ok(())
    }
    /// An open that arrived while the leases were held shows here, as a lease on its way down.
    pub fn confirm(&self, ops: &LeaseOps<'_>) -> Result<(), ProfileError> {
        if self.0.is_empty() {
            return Err(ProfileError::Ownership);
        }
        for file in &self.0 {
            let held = (ops.get)(file)?;
            if held != libc::F_WRLCK {
                return Err(io::Error::from_raw_os_error(libc::EAGAIN).into());
            }
        }
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
impl Drop for Leases {
    /// A blocked opener proceeds at once. A process forked meanwhile can keep the description open
    /// until it execs, so closing alone would not release the lease.
    fn drop(&mut self) {
        for file in &self.0 {
            // EAGAIN is a lease already gone: lease-break-time ran out for a blocked opener.
            if let Err(error) = set(file, libc::F_UNLCK)
                && error.raw_os_error() != Some(libc::EAGAIN)
            {
                tracing::warn!(%error, "private profile file lease release failed");
            }
        }
    }
}
extern "C" fn ignore_lease_break(_: libc::c_int) {}
/// Survives `SIGIO` from here on. A signal can still arrive after the leases are gone, so this stays
/// installed. A handler resets to the default on exec, unlike `SIG_IGN`, which children would inherit.
/// A disposition that some other code chose already is not the default action and is left alone.
pub(super) fn survive_lease_breaks() -> Result<(), ProfileError> {
    static INSTALLED: OnceLock<Option<i32>> = OnceLock::new();
    let failed = *INSTALLED.get_or_init(|| {
        let installed = sigio_disposition().and_then(|current| match current {
            libc::SIG_DFL => install_lease_break_handler(),
            _ => Ok(()),
        });
        installed.err().and_then(|error| error.raw_os_error())
    });
    match failed {
        Some(code) => Err(io::Error::from_raw_os_error(code).into()),
        None => Ok(()),
    }
}
fn sigio_disposition() -> io::Result<usize> {
    // SAFETY: the all-zero sigaction is a valid value for libc.
    let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: current is live for the call, and the null new action only reads the disposition.
    if unsafe { libc::sigaction(libc::SIGIO, std::ptr::null(), &mut current) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(current.sa_sigaction)
}
fn install_lease_break_handler() -> io::Result<()> {
    // SAFETY: the all-zero sigaction is a valid value for libc.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = ignore_lease_break as *const () as usize;
    action.sa_flags = libc::SA_RESTART;
    // SAFETY: sa_mask is a live field of action for the call.
    if unsafe { libc::sigemptyset(&mut action.sa_mask) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: action is live for the call, and the handler does nothing, so it is async-signal-safe.
    if unsafe { libc::sigaction(libc::SIGIO, &action, std::ptr::null_mut()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// Whether `files` more descriptors fit under the soft limit and leave a quarter of it to the other
/// threads, which would otherwise fail their own opens for as long as this holds them. The limit
/// is never raised: it is process-wide state that other code and every child inherits.
pub(super) fn descriptors_available(files: usize) -> io::Result<bool> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: limit is a live rlimit for the duration of the call.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if limit.rlim_cur == libc::RLIM_INFINITY {
        return Ok(true);
    }
    let open = std::fs::read_dir("/proc/self/fd")?.count() as u64;
    Ok(open + files as u64 + limit.rlim_cur / 4 <= limit.rlim_cur)
}
/// Whether a lease error only means something holds the file, as opposed to an environment or a
/// file that will keep answering this way.
pub(super) fn held_elsewhere(error: &ProfileError) -> bool {
    matches!(error, ProfileError::Io(e) if e.raw_os_error() == Some(libc::EAGAIN))
}
