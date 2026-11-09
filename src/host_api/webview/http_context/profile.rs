#![cfg(target_os = "linux")]
//! Linux private native-store ownership. Call filesystem entry points on an existing bounded
//! worker. Neither lease Drop performs filesystem work; exposed roots survive until a later
//! dead-owner sweep finds no consumer. This is not a native session teardown ACK.
mod fs;
mod lease;
mod process;
mod reclaim;
mod record;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
pub(crate) enum ProfileError {
    Invalid,
    Ownership,
    Capacity,
    Busy,
    Io(std::io::Error),
}
impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid => f.write_str("invalid private profile request"),
            Self::Ownership => f.write_str("private profile ownership could not be established"),
            Self::Busy => f.write_str("private profile namespace lock busy"),
            Self::Capacity => f.write_str("private profile capacity unavailable"),
            Self::Io(e) => write!(f, "private profile filesystem operation failed: {e}"),
        }
    }
}
impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}
impl From<std::io::Error> for ProfileError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
#[derive(Debug, Default)]
pub(crate) struct CleanupReport {
    pub(crate) inspected: usize,
    pub(crate) removed: usize,
    pub(crate) kept_live: usize,
    pub(crate) kept_uncertain: usize,
    pub(crate) failed: usize,
    pub(crate) limit_reached: bool,
}
struct Paths {
    root: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    record: record::Record,
}
/// Unpublished, uniquely movable ownership. Drop retains the recorded root without I/O.
pub(crate) struct PreparedProfile(Arc<Paths>);
/// Retain through every native store/view/probe/callback. Last Drop only frees memory.
#[derive(Clone)]
pub(crate) struct NativeProfileLease {
    _paths: Arc<Paths>,
}
impl PreparedProfile {
    /// Worker only. The caller prepares an absolute private namespace; never uses a fallback.
    pub(crate) fn create(parent: &Path, max_roots: usize) -> Result<Self, ProfileError> {
        if max_roots == 0 {
            return Err(ProfileError::Invalid);
        }
        let namespace = fs::Namespace::open(parent)?;
        let _lock = namespace.lock()?;
        let count = namespace.entries(max_roots)?.0.len();
        if count >= max_roots {
            tracing::warn!(count, max_roots, "private profile root capacity exhausted");
            return Err(ProfileError::Capacity);
        }
        let owner = process::Identity::current()?;
        let (name, dir) = namespace.create_root()?;
        let root = parent.join(&name);
        let record = record::Record::new(&namespace, &dir, root.clone(), owner);
        let cleanup_record = record.clone();
        let result = (|| {
            dir.create_dir(std::ffi::OsStr::new("data"))?;
            dir.create_dir(std::ffi::OsStr::new("cache"))?;
            record.write(&dir)?;
            namespace.validate()?;
            dir.check_link(&namespace.dir, &name)?;
            Ok(Self(Arc::new(Paths {
                data: root.join("data"),
                cache: root.join("cache"),
                root,
                record,
            })))
        })();
        if result.is_err()
            && let Err(error) =
                reclaim::remove_unpublished(&namespace, &name, &dir, &cleanup_record)
        {
            tracing::warn!(%error, root_entry = ?name, device = dir.id.dev, inode = dir.id.ino,
                "private profile initialization cleanup failed; owned root retained");
        }
        result
    }
    pub(crate) fn data_dir(&self) -> &Path {
        &self.0.data
    }
    pub(crate) fn cache_dir(&self) -> &Path {
        &self.0.cache
    }
    pub(crate) fn root(&self) -> &Path {
        &self.0.root
    }
    /// Memory-only move. Exposed ownership cannot return to unpublished deletion.
    pub(crate) fn into_native(self) -> NativeProfileLease {
        NativeProfileLease { _paths: self.0 }
    }
    /// Worker only. No native manager may have consumed this unpublished ownership.
    pub(crate) fn discard_unpublished(self) -> Result<(), ProfileError> {
        self.remove_unpublished()
    }
    /// [`Self::discard_unpublished`] without consuming. `Busy` returns before any removal.
    fn remove_unpublished(&self) -> Result<(), ProfileError> {
        let parent = self.0.root.parent().ok_or(ProfileError::Invalid)?;
        let name = self.0.root.file_name().ok_or(ProfileError::Invalid)?;
        let namespace = fs::Namespace::open(parent)?;
        let _lock = namespace.lock()?;
        let dir = namespace.dir.open_dir(name)?;
        self.0.record.validate(&namespace, &dir, &self.0.root)?;
        if self.0.record.owner != process::Identity::current()? {
            return Err(ProfileError::Ownership);
        }
        let disk = record::Record::read(&dir)?;
        if disk != self.0.record {
            return Err(ProfileError::Ownership);
        }
        reclaim::remove_unpublished(&namespace, name, &dir, &self.0.record)
    }
}
#[cfg(test)]
impl NativeProfileLease {
    pub(crate) fn data_dir(&self) -> &Path {
        &self._paths.data
    }
    pub(crate) fn cache_dir(&self) -> &Path {
        &self._paths.cache
    }
    pub(crate) fn root(&self) -> &Path {
        &self._paths.root
    }
}
/// Worker only. A dead owner's root is removed if nothing shows it in use: no process walked has it
/// open, and the kernel leases every regular file in it. A denied process or an exhausted walk bound
/// does not retain it, since the leases answer for those too. See the limits in html-http-bridge.md.
pub(crate) fn reclaim_dead(
    parent: &Path,
    max_entries: usize,
) -> Result<CleanupReport, ProfileError> {
    reclaim::run(parent, max_entries)
}
#[cfg(test)]
mod lease_tests;
#[cfg(test)]
mod tests;
