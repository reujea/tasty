//! Prepare only the app-owned namespace leaf; never follow links or chmod shared ancestors.
//!
//! The namespace lives in the user runtime directory when XDG_RUNTIME_DIR is set, and in the app
//! data home otherwise. The choice is made once per process and a refused location is never
//! retried elsewhere (docs/dev-guide/html-http-bridge.md "Private profile namespace location").
use super::super::profile::ProfileError;
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, OsStr, OsString},
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};
fn uid() -> u32 {
    // SAFETY: geteuid has no arguments or memory preconditions.
    unsafe { libc::geteuid() }
}
fn open(parent: &File, name: &OsStr) -> Result<File, ProfileError> {
    let name = CString::new(name.as_bytes()).map_err(|_| ProfileError::Invalid)?;
    // SAFETY: live directory descriptor and single NUL-terminated relative path component.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: successful openat returned a fresh descriptor owned once by File.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn ancestor(dir: &File) -> Result<(), ProfileError> {
    let m = dir.metadata()?;
    if !m.is_dir()
        || (m.uid() != uid() && m.uid() != 0)
        || (m.mode() & 0o022 != 0 && !(m.uid() == 0 && m.mode() & 0o1000 != 0))
    {
        return Err(ProfileError::Ownership);
    }
    Ok(())
}
fn home(path: &Path) -> Result<File, ProfileError> {
    if !path.is_absolute() {
        return Err(ProfileError::Invalid);
    }
    let mut dir = File::open("/")?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                dir = open(&dir, name)?;
                ancestor(&dir)?;
            }
            _ => return Err(ProfileError::Invalid),
        }
    }
    if dir.metadata()?.uid() != uid() {
        return Err(ProfileError::Ownership);
    }
    Ok(dir)
}
const HOME_LEAF: &str = "native-http-profiles";

/// Where the private profile namespace is created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NamespaceRoot {
    /// `<app home>/native-http-profiles`.
    AppHome(PathBuf),
    /// `<runtime>/tasty-native-http-<SHA-256 of the app home path bytes>`.
    Runtime { runtime: PathBuf, app_home: PathBuf },
}

impl From<PathBuf> for NamespaceRoot {
    fn from(app_home: PathBuf) -> Self {
        Self::AppHome(app_home)
    }
}

impl NamespaceRoot {
    /// Reads XDG_RUNTIME_DIR once per process. A set value always selects the runtime directory,
    /// even when it is empty or relative; preparation then refuses it.
    pub(crate) fn for_process(app_home: PathBuf) -> Self {
        static RUNTIME: OnceLock<Option<OsString>> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| std::env::var_os("XDG_RUNTIME_DIR"));
        Self::select(runtime.as_deref(), app_home)
    }

    pub(crate) fn select(runtime: Option<&OsStr>, app_home: PathBuf) -> Self {
        match runtime {
            Some(runtime) => Self::Runtime {
                runtime: PathBuf::from(runtime),
                app_home,
            },
            None => Self::AppHome(app_home),
        }
    }

    /// The pre-runtime namespace of the same app home, swept but never created.
    pub(super) fn legacy(&self) -> Option<PathBuf> {
        match self {
            Self::AppHome(_) => None,
            Self::Runtime { app_home, .. } => existing(app_home, OsStr::new(HOME_LEAF)).ok(),
        }
    }
}

pub(super) fn runtime_leaf(app_home: &Path) -> OsString {
    let digest = Sha256::digest(app_home.as_os_str().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    OsString::from(format!("tasty-native-http-{hex}"))
}

pub(super) fn prepare(root: &NamespaceRoot) -> Result<PathBuf, ProfileError> {
    match root {
        NamespaceRoot::AppHome(app_home) => create(app_home, OsStr::new(HOME_LEAF), false),
        NamespaceRoot::Runtime { runtime, app_home } => {
            create(runtime, &runtime_leaf(app_home), true)
        }
    }
}

/// The parent of the runtime namespace must additionally be exactly 0700.
fn parent_dir(path: &Path, exact_private: bool) -> Result<File, ProfileError> {
    let dir = home(path)?;
    if exact_private && dir.metadata()?.mode() & 0o7777 != 0o700 {
        return Err(ProfileError::Ownership);
    }
    Ok(dir)
}

/// Opens an existing namespace that passes the same checks as a created one; creates nothing.
fn existing(path: &Path, name: &OsStr) -> Result<PathBuf, ProfileError> {
    let parent = home(path)?;
    let m = open(&parent, name)?.metadata()?;
    if m.uid() != uid() || m.mode() & 0o7777 != 0o700 {
        return Err(ProfileError::Ownership);
    }
    let again = open(&home(path)?, name)?.metadata()?;
    if (m.dev(), m.ino()) != (again.dev(), again.ino()) {
        return Err(ProfileError::Ownership);
    }
    Ok(path.join(name))
}

fn create(data_home: &Path, name: &OsStr, exact_private: bool) -> Result<PathBuf, ProfileError> {
    let parent = parent_dir(data_home, exact_private)?;
    let c = CString::new(name.as_bytes()).map_err(|_| ProfileError::Invalid)?;
    // SAFETY: parent is an owned directory fd, c is a fixed single relative name.
    let created = unsafe { libc::mkdirat(parent.as_raw_fd(), c.as_ptr(), 0o700) } == 0;
    if !created && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
        return Err(std::io::Error::last_os_error().into());
    }
    if created {
        // SAFETY: the parent fd is pinned and the fixed leaf cannot redirect through a symlink.
        // Set explicit permissions before opening: a restrictive umask can remove read access.
        if unsafe {
            libc::fchmodat(
                parent.as_raw_fd(),
                c.as_ptr(),
                0o700,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    let dir = open(&parent, name)?;
    let before = dir.metadata()?;
    if before.uid() != uid() {
        return Err(ProfileError::Ownership);
    }
    let m = dir.metadata()?;
    if m.mode() & 0o7777 != 0o700 {
        return Err(ProfileError::Ownership);
    }
    // Reopen every ancestor and leaf and compare actual objects before returning a pathname.
    let current = parent_dir(data_home, exact_private)?;
    let a = parent.metadata()?;
    let b = current.metadata()?;
    let current = open(&current, name)?.metadata()?;
    if (a.dev(), a.ino(), a.uid()) != (b.dev(), b.ino(), b.uid())
        || (m.dev(), m.ino(), m.uid()) != (current.dev(), current.ino(), current.uid())
    {
        return Err(ProfileError::Ownership);
    }
    Ok(data_home.join(name))
}
