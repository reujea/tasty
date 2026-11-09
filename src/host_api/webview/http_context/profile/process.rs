use super::{
    ProfileError,
    fs::{self},
};
use std::{
    collections::HashSet,
    fs::File,
    io::{self, BufRead, BufReader, Read},
    os::unix::{ffi::OsStringExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub uid: u32,
    pub pid: u32,
    pub start: u64,
    pub boot: String,
}
pub(super) enum OwnerState {
    Live,
    Dead,
    Uncertain,
}
impl Identity {
    pub fn current() -> Result<Self, ProfileError> {
        let pid = std::process::id();
        let start = start(pid)?;
        let identity = Self {
            uid: fs::uid(),
            pid,
            start,
            boot: boot()?,
        };
        if !identity.valid() || std::fs::metadata(format!("/proc/{pid}"))?.uid() != identity.uid {
            return Err(ProfileError::Ownership);
        }
        Ok(identity)
    }
    pub fn valid(&self) -> bool {
        self.pid > 0
            && self.start > 0
            && self.boot.len() == 36
            && self.boot.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
    }
    /// `namespace_file_system` is the namespace's `f_type`, `None` when the kernel did not say.
    pub fn state(&self, namespace_file_system: Option<u32>) -> Result<OwnerState, ProfileError> {
        if self.boot != boot()? {
            return Ok(owner_of_another_boot(namespace_file_system));
        }
        match start(self.pid) {
            Ok(value) if value == self.start => Ok(OwnerState::Live),
            // PID reuse or an altered record. It clears by itself when that process ends.
            Ok(_) => Ok(OwnerState::Uncertain),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(OwnerState::Dead),
            Err(e) => Err(e.into()),
        }
    }
}
/// ext2/3/4, xfs, btrfs and tmpfs. Only ext4 and tmpfs were measured.
const LOCAL_FILE_SYSTEMS: [u32; 4] = [0xEF53, 0x5846_5342, 0x9123_683E, 0x0102_1994];
/// An owner of another boot cannot have survived it on a file system that one kernel serves. On any
/// other, a shared home can hold the root of a live owner on another host, which neither the walk
/// nor a lease here would see. The record is no proof of its boot: an altered one makes a live
/// owner's root a candidate too, and the file leases still decide.
pub(super) fn owner_of_another_boot(file_system: Option<u32>) -> OwnerState {
    match file_system {
        Some(kind) if LOCAL_FILE_SYSTEMS.contains(&kind) => OwnerState::Dead,
        _ => OwnerState::Uncertain,
    }
}
fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("process observation bound"));
    }
    Ok(bytes)
}
fn boot() -> io::Result<String> {
    String::from_utf8(read(Path::new("/proc/sys/kernel/random/boot_id"), 128)?)
        .map(|s| s.trim().to_owned())
        .map_err(io::Error::other)
}
fn start(pid: u32) -> io::Result<u64> {
    let bytes = read(Path::new(&format!("/proc/{pid}/stat")), 8192)?;
    let text = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
    parse_start(text).ok_or_else(|| io::Error::other("process start identity unavailable"))
}
fn parse_start(text: &str) -> Option<u64> {
    // comm can contain spaces and parentheses. Field22 follows the final closing parenthesis.
    text.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}
/// A bounded read-only same-UID FD/maps/cwd/root/exe observation, never a WebKit teardown ACK.
/// `Ok(true)` is positive evidence of a consumer and is the only answer that retains a root.
/// `Ok(false)` saw every process, `Err` could not: a denied process, an exhausted bound or an unstable
/// observation. Neither shows the root is free, which only the file leases of the reclaim decide.
pub(super) fn consumers(root: &Path, objects: &HashSet<(u64, u64)>) -> io::Result<bool> {
    consumers_in(Path::new("/proc"), root, objects)
}
/// The product walk over a given process table.
pub(super) fn consumers_in(
    processes: &Path,
    root: &Path,
    objects: &HashSet<(u64, u64)>,
) -> io::Result<bool> {
    let listed = std::fs::read_dir(processes)?
        .take(ENTRY_BOUND + 1)
        .enumerate()
        .filter_map(|(index, entry)| {
            if index == ENTRY_BOUND {
                return Some(Err(io::Error::other("process audit entry bound")));
            }
            match entry {
                Ok(entry) => entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.parse::<u32>().is_ok())
                    .then(|| Ok(entry.path())),
                Err(e) => Some(Err(e)),
            }
        });
    walk(listed, root, objects)
}
const ENTRY_BOUND: usize = 4096;
/// Visits every given process. One that cannot be inspected must not stop the walk: a process after
/// it can still hold a directory or cwd in the root, which no file lease would notice. Only a
/// consumer or the spent budget ends it early. The first thing left unseen is reported if none was found.
pub(super) fn walk(
    processes: impl IntoIterator<Item = io::Result<PathBuf>>,
    root: &Path,
    objects: &HashSet<(u64, u64)>,
) -> io::Result<bool> {
    let mut walk = Walk {
        root,
        objects,
        budget: 65_536,
        unseen: None,
    };
    for process in processes {
        let found = process.and_then(|process| walk.process(&process));
        match found {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) if walk.budget == 0 => return Err(e),
            Err(e) => walk.note(e),
        }
    }
    walk.unseen.map_or(Ok(false), Err)
}
struct Walk<'a> {
    root: &'a Path,
    objects: &'a HashSet<(u64, u64)>,
    budget: usize,
    unseen: Option<io::Error>,
}
impl Walk<'_> {
    fn note(&mut self, e: io::Error) {
        if !vanished(&e) && self.unseen.is_none() {
            self.unseen = Some(e);
        }
    }
    fn process(&mut self, process: &Path) -> io::Result<bool> {
        if std::fs::metadata(process)?.uid() != fs::uid() {
            return Ok(false);
        }
        // Denied tasks still leave the maps of the process, and the reverse.
        match self.tasks(process) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) if self.budget == 0 => return Err(e),
            Err(e) => self.note(e),
        }
        maps(
            &process.join("maps"),
            self.root,
            self.objects,
            &mut self.budget,
        )
    }
    fn tasks(&mut self, process: &Path) -> io::Result<bool> {
        for task in std::fs::read_dir(process.join("task"))? {
            spend(&mut self.budget)?;
            match task.and_then(|task| self.task(&task.path())) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(e) if self.budget == 0 => return Err(e),
                Err(e) => self.note(e),
            }
        }
        Ok(false)
    }
    fn task(&mut self, task: &Path) -> io::Result<bool> {
        for name in ["cwd", "root", "exe"] {
            match reference(&task.join(name), self.root, self.objects) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(e) => self.note(e),
            }
        }
        for entry in std::fs::read_dir(task.join("fd"))? {
            spend(&mut self.budget)?;
            match entry.and_then(|entry| reference(&entry.path(), self.root, self.objects)) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(e) => self.note(e),
            }
        }
        Ok(false)
    }
}
fn vanished(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound || e.raw_os_error() == Some(libc::ESRCH)
}
fn reference(path: &Path, root: &Path, objects: &HashSet<(u64, u64)>) -> io::Result<bool> {
    let target = match std::fs::read_link(path) {
        Ok(p) => p,
        Err(e) if vanished(&e) => return Ok(false),
        Err(e) => return Err(e),
    };
    if target.starts_with(root) {
        return Ok(true);
    }
    match std::fs::metadata(path) {
        Ok(m) => Ok(objects.contains(&(m.dev(), m.ino()))),
        Err(e) if vanished(&e) => Ok(false),
        Err(e) => Err(e),
    }
}
fn spend(budget: &mut usize) -> io::Result<()> {
    *budget = budget
        .checked_sub(1)
        .ok_or_else(|| io::Error::other("process audit observation bound"))?;
    Ok(())
}
fn maps(
    path: &Path,
    root: &Path,
    objects: &HashSet<(u64, u64)>,
    budget: &mut usize,
) -> io::Result<bool> {
    let mut reader = BufReader::new(File::open(path)?.take(16 * 1024 * 1024 + 1));
    let mut total = 0;
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? > 0 {
        spend(budget)?;
        total += line.len();
        if total > 16 * 1024 * 1024 {
            return Err(io::Error::other("process maps byte bound"));
        }
        if mapped(&line, root, objects)? {
            return Ok(true);
        }
        line.clear();
    }
    Ok(false)
}
fn mapped(line: &[u8], root: &Path, objects: &HashSet<(u64, u64)>) -> io::Result<bool> {
    // Address, permissions, offset, device and inode are whitespace-separated; the remaining
    // pathname can contain spaces and Linux octal escapes. Reject malformed observations.
    let mut fields = Vec::new();
    let mut offset = 0;
    for token in line
        .split(|b| b.is_ascii_whitespace())
        .filter(|x| !x.is_empty())
        .take(5)
    {
        let start = line[offset..]
            .windows(token.len())
            .position(|p| p == token)
            .ok_or_else(|| io::Error::other("maps fields"))?
            + offset;
        offset = start + token.len();
        fields.push(token);
    }
    if fields.len() != 5 {
        return Err(io::Error::other("maps fields"));
    }
    let dev = std::str::from_utf8(fields[3]).map_err(io::Error::other)?;
    let (major, minor) = dev
        .split_once(':')
        .ok_or_else(|| io::Error::other("maps device"))?;
    let major = u32::from_str_radix(major, 16).map_err(io::Error::other)?;
    let minor = u32::from_str_radix(minor, 16).map_err(io::Error::other)?;
    let ino = std::str::from_utf8(fields[4])
        .map_err(io::Error::other)?
        .parse::<u64>()
        .map_err(io::Error::other)?;
    if objects.contains(&(libc::makedev(major, minor), ino)) {
        return Ok(true);
    }
    let pathname = line[offset..].trim_ascii();
    if !pathname.starts_with(b"/") {
        return Ok(false);
    }
    Ok(PathBuf::from(std::ffi::OsString::from_vec(unescape(pathname)?)).starts_with(root))
}
fn unescape(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            let digits = bytes
                .get(i + 1..i + 4)
                .ok_or_else(|| io::Error::other("maps escape"))?;
            let text = std::str::from_utf8(digits).map_err(io::Error::other)?;
            out.push(u8::from_str_radix(text, 8).map_err(io::Error::other)?);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(out)
}
