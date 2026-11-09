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
    pub fn state(&self) -> Result<OwnerState, ProfileError> {
        // Mismatched boot/start observations are not permission to delete. They can represent
        // PID reuse or an altered record; retain uncertainty instead of labeling that owner live.
        if self.boot != boot()? {
            return Ok(OwnerState::Uncertain);
        }
        match start(self.pid) {
            Ok(value) if value == self.start => Ok(OwnerState::Live),
            Ok(_) => Ok(OwnerState::Uncertain),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(OwnerState::Dead),
            Err(e) => Err(e.into()),
        }
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
/// Any inaccessible live process, exhausted bound or unstable observation leaves uncertainty.
pub(super) fn consumers(root: &Path, objects: &HashSet<(u64, u64)>) -> io::Result<bool> {
    let mut entries = 0;
    let mut budget = 65_536usize;
    for entry in std::fs::read_dir("/proc")? {
        entries += 1;
        if entries > 4096 {
            return Err(io::Error::other("process audit entry bound"));
        }
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_none_or(|s| s.parse::<u32>().is_err())
        {
            continue;
        }
        let path = entry.path();
        let m = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if vanished(&e) => continue,
            Err(e) => return Err(e),
        };
        if m.uid() != fs::uid() {
            continue;
        }
        match audit_process(&path, root, objects, &mut budget) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) if gone(&e, &path) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}
pub(super) fn audit_process(
    process: &Path,
    root: &Path,
    objects: &HashSet<(u64, u64)>,
    budget: &mut usize,
) -> io::Result<bool> {
    for task in std::fs::read_dir(process.join("task"))? {
        spend(budget)?;
        let task = task?.path();
        match audit_task(&task, root, objects, budget) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) if gone(&e, &task) => {}
            Err(e) => return Err(e),
        }
    }
    maps(&process.join("maps"), root, objects, budget)
}
fn audit_task(
    task: &Path,
    root: &Path,
    objects: &HashSet<(u64, u64)>,
    budget: &mut usize,
) -> io::Result<bool> {
    for name in ["cwd", "root", "exe"] {
        if reference(&task.join(name), root, objects)? {
            return Ok(true);
        }
    }
    for entry in std::fs::read_dir(task.join("fd"))? {
        spend(budget)?;
        if reference(&entry?.path(), root, objects)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn vanished(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound || e.raw_os_error() == Some(libc::ESRCH)
}
/// Whether an audit error means its process or task no longer exists, so it consumes nothing.
fn gone(e: &io::Error, subject: &Path) -> bool {
    gone_after(e, || read(&subject.join("stat"), 8192).map(drop))
}
/// A task that exits during the walk also answers its /proc entries with EACCES, the same answer as
/// a live task this UID may not inspect. Only observing the subject again tells them apart: it
/// counts as gone if that says it is gone, and a live subject or a failed observation stays denied.
pub(super) fn gone_after(e: &io::Error, reobserve: impl FnOnce() -> io::Result<()>) -> bool {
    vanished(e)
        || (e.raw_os_error() == Some(libc::EACCES)
            && reobserve().is_err_and(|again| vanished(&again)))
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
