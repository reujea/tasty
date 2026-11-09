use super::ProfileError;
use rand::Rng;
use std::{
    ffi::{CString, OsStr, OsString},
    fs::{File, Metadata},
    io::{Read, Seek, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
};

pub(super) const LOCK: &str = ".profile-lock";
const LOCK_CONTENT: &[u8] = b"tasty-private-profile-lock-v1\n";
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Object {
    pub dev: u64,
    pub ino: u64,
    pub uid: u32,
}
impl Object {
    pub fn from(m: &Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            uid: m.uid(),
        }
    }
}
pub(super) fn uid() -> u32 {
    // SAFETY: geteuid has no arguments or memory preconditions.
    unsafe { libc::geteuid() }
}
pub(super) struct Dir {
    pub file: File,
    pub id: Object,
}
impl Dir {
    fn from(file: File) -> Result<Self, ProfileError> {
        let m = file.metadata()?;
        if !m.is_dir() {
            return Err(ProfileError::Ownership);
        }
        Ok(Self {
            id: Object::from(&m),
            file,
        })
    }
    /// The `f_type` of the file system this directory is on, `None` when the kernel did not say.
    pub fn file_system_type(&self) -> Option<u32> {
        // SAFETY: the all-zero statfs is a valid value for libc.
        let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: file owns a live descriptor and stat is live for the call.
        if unsafe { libc::fstatfs(self.file.as_raw_fd(), &mut stat) } < 0 {
            let error = std::io::Error::last_os_error();
            tracing::debug!(%error, "private profile file system type unavailable");
            return None;
        }
        // Every magic number fits 32 bits. The field's width and sign differ between targets.
        Some(stat.f_type as u32)
    }
    pub fn open_dir(&self, name: &OsStr) -> Result<Self, ProfileError> {
        Self::from(open(self, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?)
    }
    pub fn open_file(&self, name: &OsStr) -> Result<File, ProfileError> {
        let f = open(self, name, libc::O_RDONLY, 0)?;
        let m = f.metadata()?;
        if !m.is_file() || m.uid() != uid() || m.nlink() != 1 || m.mode() & 0o7777 != 0o600 {
            return Err(ProfileError::Ownership);
        }
        Ok(f)
    }
    /// The same no-follow, no-mount resolution as every open here, for any mode WebKit gave a file.
    /// The caller compares it with its inspected entry.
    pub fn open_for_lease(&self, name: &OsStr) -> Result<File, ProfileError> {
        open(self, name, libc::O_RDONLY, 0)
    }
    pub fn create_file(&self, name: &OsStr) -> Result<File, ProfileError> {
        let f = open(
            self,
            name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        Ok(f)
    }
    pub fn create_dir(&self, name: &OsStr) -> Result<Self, ProfileError> {
        let c = name_c(name)?;
        // SAFETY: self owns a directory fd and c is a live NUL-terminated single name.
        if unsafe { libc::mkdirat(self.file.as_raw_fd(), c.as_ptr(), 0o700) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let before = self.metadata(name)?;
        if !before.is_dir() || before.uid() != uid() {
            return Err(ProfileError::Ownership);
        }
        // SAFETY: relative single name, live parent fd; NOFOLLOW forbids chmod of a symlink target.
        if unsafe {
            libc::fchmodat(
                self.file.as_raw_fd(),
                c.as_ptr(),
                0o700,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let dir = self.open_dir(name)?;
        if dir.id != Object::from(&before) {
            return Err(ProfileError::Ownership);
        }
        dir.private()?;
        Ok(dir)
    }
    pub fn metadata(&self, name: &OsStr) -> Result<Metadata, ProfileError> {
        // This proc path is derived solely from an owned descriptor; the final name is lstat'd.
        Ok(std::fs::symlink_metadata(self.proc_path().join(name))?)
    }
    pub fn private(&self) -> Result<(), ProfileError> {
        let m = self.file.metadata()?;
        if m.uid() != uid() || m.mode() & 0o7777 != 0o700 {
            return Err(ProfileError::Ownership);
        }
        Ok(())
    }
    pub fn check_link(&self, parent: &Dir, name: &OsStr) -> Result<(), ProfileError> {
        let current = parent.open_dir(name)?;
        if current.id != self.id {
            return Err(ProfileError::Ownership);
        }
        Ok(())
    }
    pub fn proc_path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd()))
    }
    pub fn names(
        &self,
        max: usize,
        exclude_lock: bool,
    ) -> Result<(Vec<OsString>, bool), ProfileError> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(self.proc_path())? {
            let name = entry?.file_name();
            if exclude_lock && name == LOCK {
                continue;
            }
            if names.len() == max {
                return Ok((names, true));
            }
            names.push(name);
        }
        Ok((names, false))
    }
    pub fn unlink(&self, name: &OsStr, directory: bool) -> Result<(), ProfileError> {
        let name = name_c(name)?;
        // SAFETY: owned parent fd and a single relative name; unlinkat never follows the leaf.
        if unsafe {
            libc::unlinkat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                if directory { libc::AT_REMOVEDIR } else { 0 },
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}
fn name_c(name: &OsStr) -> Result<CString, ProfileError> {
    if name.is_empty() || name == "." || name == ".." || name.as_bytes().contains(&b'/') {
        return Err(ProfileError::Invalid);
    }
    CString::new(name.as_bytes()).map_err(|_| ProfileError::Invalid)
}
#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}
fn open(parent: &Dir, name: &OsStr, flags: i32, mode: u32) -> Result<File, ProfileError> {
    let name = name_c(name)?;
    // BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV also rejects same-device bind mounts.
    // Unsupported kernels fail closed; there is no path-following compatibility fallback.
    let how = OpenHow {
        flags: (flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK) as u64,
        mode: u64::from(mode),
        resolve: 0x08 | 0x04 | 0x02 | 0x01,
    };
    // SAFETY: parent owns fd, name/how are live, and size matches Linux open_how's three u64s.
    let raw = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            parent.file.as_raw_fd(),
            name.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: successful openat2 returned a fresh fd owned exactly once by File.
    Ok(unsafe { File::from_raw_fd(raw as i32) })
}
pub(super) struct Namespace {
    pub path: PathBuf,
    pub dir: Dir,
}
impl Namespace {
    pub fn open(path: &Path) -> Result<Self, ProfileError> {
        if !path.is_absolute() {
            return Err(ProfileError::Invalid);
        }
        let mut dir = Dir::from(File::open("/")?)?;
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    dir = ancestor(&dir, name)?;
                }
                _ => return Err(ProfileError::Invalid),
            }
        }
        dir.private()?;
        Ok(Self {
            path: path.to_path_buf(),
            dir,
        })
    }
    pub fn validate(&self) -> Result<(), ProfileError> {
        let current = Self::open(&self.path)?;
        if self.dir.id != current.dir.id {
            return Err(ProfileError::Ownership);
        }
        Ok(())
    }
    pub fn lock(&self) -> Result<File, ProfileError> {
        let name = OsStr::new(LOCK);
        let mut file = match self.dir.create_file(name) {
            Ok(mut f) => {
                f.write_all(LOCK_CONTENT)?;
                f.sync_all()?;
                f
            }
            Err(ProfileError::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                self.dir.open_file(name)?
            }
            Err(e) => return Err(e),
        };
        // SAFETY: file owns a live descriptor. Nonblocking contention is reported, never waited on.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
            let e = std::io::Error::last_os_error();
            return Err(if e.kind() == std::io::ErrorKind::WouldBlock {
                ProfileError::Busy
            } else {
                e.into()
            });
        }
        file.rewind()?;
        let mut content = Vec::new();
        (&mut file).take(64).read_to_end(&mut content)?;
        if content != LOCK_CONTENT {
            return Err(ProfileError::Ownership);
        }
        self.validate()?;
        if Object::from(&file.metadata()?) != Object::from(&self.dir.open_file(name)?.metadata()?) {
            return Err(ProfileError::Ownership);
        }
        Ok(file)
    }
    pub fn entries(&self, max: usize) -> Result<(Vec<OsString>, bool), ProfileError> {
        self.dir.names(max, true)
    }
    pub fn create_root(&self) -> Result<(OsString, Dir), ProfileError> {
        for _ in 0..16 {
            let random: [u8; 16] = rand::rng().random();
            let name = OsString::from(format!(
                "p-{}",
                random
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ));
            match self.dir.create_dir(&name) {
                Ok(dir) => return Ok((name, dir)),
                Err(ProfileError::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        tracing::warn!("private profile name creation exhausted after sixteen collisions");
        Err(ProfileError::Capacity)
    }
}
fn ancestor(parent: &Dir, name: &OsStr) -> Result<Dir, ProfileError> {
    let name = name_c(name)?;
    // Ancestry can cross real mount points, but never a symlink. Below the namespace openat2
    // additionally rejects mount traversal. Root-owned sticky temp ancestry is allowed.
    // SAFETY: live fd and NUL-terminated relative name; flags require a real directory.
    let raw = unsafe {
        libc::openat(
            parent.file.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_DIRECTORY,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: successful openat returned a fresh owned fd.
    let dir = Dir::from(unsafe { File::from_raw_fd(raw) })?;
    let m = dir.file.metadata()?;
    if (m.uid() != uid() && m.uid() != 0)
        || (m.mode() & 0o022 != 0 && !(m.uid() == 0 && m.mode() & 0o1000 != 0))
    {
        return Err(ProfileError::Ownership);
    }
    Ok(dir)
}
