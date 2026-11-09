//! The reclaim contract: a dead owner's root goes unless something shows it in use, and nothing
//! denied or unseen by the process walk can hide a holder of a regular file.
//!
//! Holders are real processes owned by the test. Walks run over a table of those processes, so that
//! no other process of this machine answers; two tests walk the real one on purpose. Tests where a
//! lease really breaks, or that change process state, run alone in a child process of their own.
use super::{
    tests::{
        OwnedChild, admitted, namespace, reclaim_positive, swept_until_removed, until_removed,
    },
    *,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    fs as disk,
    fs::File,
    io::{self, BufRead, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::ExitStatusExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const FIRST: &[u8] = b"first owned bytes";
const TARGET_BYTES: usize = 4096;
fn target_content() -> Vec<u8> {
    vec![b't'; TARGET_BYTES]
}
fn entry_of(function: &str) -> String {
    format!("{}::{function}", module_path!().split_once("::").unwrap().1)
}
fn harness_entry(function: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--ignored",
        "--exact",
        &entry_of(function),
        "--nocapture",
        "--test-threads=1",
    ]);
    command
}
fn run_alone(function: &str) {
    let output = harness_entry(function).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.signal(),
        None,
        "{function} died by a signal: {stdout}"
    );
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "{function}: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A root whose owner has really exited, with two more regular files than its record.
struct DeadRoot {
    parent: tempfile::TempDir,
    root: PathBuf,
}
impl DeadRoot {
    fn new() -> Self {
        Self::under(namespace())
    }
    fn under(parent: tempfile::TempDir) -> Self {
        let mut owner = OwnedChild::start(parent.path(), false);
        owner.finish();
        let root = owner.root.clone();
        disk::write(root.join("data/first"), FIRST).unwrap();
        disk::write(root.join("data/target"), target_content()).unwrap();
        Self { parent, root }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn assert_retained_intact(&self) {
        assert_eq!(disk::read(self.path("data/first")).unwrap(), FIRST);
        assert_eq!(
            disk::read(self.path("data/target")).unwrap(),
            target_content()
        );
        assert!(self.path(record::NAME).is_file());
    }
    /// A sweep whose walk answers `audit` and whose leases answer `ops`.
    fn sweep_with(&self, audit: Audit<'_>, ops: &lease::LeaseOps<'_>) -> (usize, usize) {
        let r = admitted(|| reclaim::run_audited(self.parent.path(), 1, audit, ops)).unwrap();
        assert_eq!((r.inspected, r.kept_live, r.failed), (1, 0, 0));
        (r.removed, r.kept_uncertain)
    }
    /// The next sweep finds the root free, after whatever retained it has ended.
    fn assert_removed_after(&self, audit: Audit<'_>) {
        let r = until_removed(|| {
            admitted(|| reclaim::run_audited(self.parent.path(), 1, audit, &lease::KERNEL)).unwrap()
        });
        assert_eq!((r.removed, r.kept_uncertain), (1, 0));
        assert!(!self.root.exists());
    }
}
type Audit<'a> = &'a dyn Fn(&Path, &HashSet<(u64, u64)>) -> io::Result<bool>;
fn nothing_found(_: &Path, _: &HashSet<(u64, u64)>) -> io::Result<bool> {
    Ok(false)
}
fn bound_exhausted(_: &Path, _: &HashSet<(u64, u64)>) -> io::Result<bool> {
    Err(io::Error::other("process audit observation bound"))
}

/// A real process that holds what it is told to, optionally denying all inspection.
struct Holder {
    child: Child,
    stdin: std::process::ChildStdin,
    // Stays open until the child has exited: its harness still reports to this pipe.
    stdout: io::BufReader<std::process::ChildStdout>,
    process: PathBuf,
}
impl Holder {
    fn start(denied: bool) -> Self {
        Self::spawn(harness_entry("holder_child"), denied)
    }
    /// `cwd` is the one hold given at spawn: changing it later would be a process-wide test mutation.
    fn holding(denied: bool, verb: &str, path: &Path) -> Self {
        if verb == "cwd" {
            let mut entry = harness_entry("holder_child");
            entry.current_dir(path);
            Self::spawn(entry, denied)
        } else {
            Self::start(denied).hold(verb, path)
        }
    }
    fn spawn(mut entry: Command, denied: bool) -> Self {
        let mut child = entry
            .env(
                "TASTY_PROFILE_HOLDER_DENIED",
                if denied { "1" } else { "0" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let process = PathBuf::from(format!("/proc/{}", child.id()));
        let stdin = child.stdin.take().unwrap();
        let stdout = io::BufReader::new(child.stdout.take().unwrap());
        let mut holder = Self {
            child,
            stdin,
            stdout,
            process,
        };
        holder.ack();
        holder
    }
    fn ack(&mut self) {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stdout.read_line(&mut line).unwrap();
            assert_ne!(read, 0, "holder exited before answering");
            if line.contains("holder-ack") {
                return;
            }
        }
    }
    fn send(&mut self, verb: &str, path: &Path) {
        writeln!(self.stdin, "{verb} {}", path.display()).unwrap();
        self.stdin.flush().unwrap();
    }
    fn hold(mut self, verb: &str, path: &Path) -> Self {
        self.send(verb, path);
        self.ack();
        self
    }
    /// A process table of just this child.
    fn table(&self) -> tempfile::TempDir {
        let table = namespace();
        let pid = self.process.file_name().unwrap();
        std::os::unix::fs::symlink(&self.process, table.path().join(pid)).unwrap();
        table
    }
    /// Reports whether it was alive when asked to finish, so what was seen was a live process.
    fn finish(mut self) -> bool {
        let alive = self.child.try_wait().unwrap().is_none();
        drop(self.stdin);
        let status = self.child.wait().unwrap();
        assert!(status.success(), "holder ended with {status}");
        alive
    }
}
#[test]
#[ignore = "owned subprocess entry, driven over its stdin by Holder"]
fn holder_child() {
    use std::os::fd::AsRawFd;
    if std::env::var_os("TASTY_PROFILE_HOLDER_DENIED").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        // SAFETY: this is the dedicated owned child process; prctl touches no Rust memory.
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) }, 0);
    }
    let ack = || {
        let mut stdout = io::stdout();
        writeln!(stdout, "holder-ack").unwrap();
        stdout.flush().unwrap();
    };
    let map = |path: &str, protection: i32, flags: i32| {
        let file = File::open(path).unwrap();
        // SAFETY: a fresh mapping of a live descriptor of a nonempty file, kept until exit.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                TARGET_BYTES,
                protection,
                flags,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(mapping, libc::MAP_FAILED);
    };
    let mut held = Vec::new();
    ack();
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let (verb, path) = line.split_once(' ').unwrap();
        match verb {
            "read" => held.push(File::open(path).unwrap()),
            "write" => held.push(
                disk::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .unwrap(),
            ),
            "map" => map(path, libc::PROT_READ, libc::MAP_SHARED),
            "map-exec" => map(path, libc::PROT_READ | libc::PROT_EXEC, libc::MAP_PRIVATE),
            "dir" => held.push(File::open(path).unwrap()),
            "o-path" => {
                use std::os::unix::fs::OpenOptionsExt;
                held.push(
                    disk::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_PATH)
                        .open(path)
                        .unwrap(),
                );
            }
            "release" => held.clear(),
            other => panic!("unknown holder verb {other}"),
        }
        ack();
    }
}

// T1
#[test]
fn a_denied_unrelated_process_does_not_retain_a_free_root() {
    let dead = DeadRoot::new();
    let holder = Holder::start(true);
    let table = holder.table();
    let walked = process::consumers_in(table.path(), &dead.root, &HashSet::new());
    let error = walked.expect_err("the walk over a denied process cannot claim to be complete");
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    dead.assert_removed_after(&|root, objects| process::consumers_in(table.path(), root, objects));
    assert!(holder.finish(), "the denied process must have been live");
}
/// The product sweep on this machine's real process table, with a denied process known to be in it.
#[test]
fn the_public_sweep_removes_a_free_root_whatever_the_real_table_denies() {
    let dead = DeadRoot::new();
    let holder = Holder::start(true);
    let r = swept_until_removed(dead.parent.path(), 1);
    assert_eq!((r.inspected, r.removed, r.kept_uncertain), (1, 1, 0));
    assert!(!dead.root.exists());
    assert!(holder.finish(), "the denied process must have been live");
}
// T2, with the held file both early and last in lease order, the record being always last.
#[test]
fn a_denied_process_holding_a_regular_file_retains_the_root_until_it_ends() {
    for verb in ["read", "write", "map"] {
        for held in ["data/target", record::NAME] {
            let dead = DeadRoot::new();
            let holder = Holder::start(true).hold(verb, &dead.path(held));
            let table = holder.table();
            let audit: Audit<'_> =
                &|root, objects| process::consumers_in(table.path(), root, objects);
            assert_eq!(
                dead.sweep_with(audit, &lease::KERNEL),
                (0, 1),
                "{verb} {held}"
            );
            dead.assert_retained_intact();
            assert!(holder.finish(), "the holder must have been live");
            dead.assert_removed_after(audit);
        }
    }
}
/// The same retention through the public sweep on the real process table.
#[test]
fn the_public_sweep_retains_a_root_a_denied_process_holds() {
    let dead = DeadRoot::new();
    let holder = Holder::start(true).hold("read", &dead.path("data/target"));
    let r = reclaim_positive(dead.parent.path(), 1);
    assert_eq!((r.inspected, r.removed, r.kept_uncertain), (1, 0, 1));
    dead.assert_retained_intact();
    assert!(holder.finish(), "the holder must have been live");
    let r = swept_until_removed(dead.parent.path(), 1);
    assert_eq!((r.removed, r.kept_uncertain), (1, 0));
    assert!(!dead.root.exists());
}
// T3
#[test]
fn every_lease_answer_but_a_confirmed_grant_retains_the_root() {
    for code in [
        libc::EAGAIN,
        libc::EINVAL,
        libc::EACCES,
        libc::EIO,
        libc::EMFILE,
        libc::ENOLCK,
    ] {
        let dead = DeadRoot::new();
        // One file answers this way and the rest are granted, so nothing but this answer retains.
        let answering = disk::metadata(dead.path("data/target")).unwrap().ino();
        let set = |file: &File| {
            if file.metadata()?.ino() == answering {
                Err(io::Error::from_raw_os_error(code))
            } else {
                lease::set_write(file)
            }
        };
        let ops = lease::LeaseOps {
            set: &set,
            get: &lease::get,
        };
        assert_eq!(dead.sweep_with(&nothing_found, &ops), (0, 1), "set {code}");
        dead.assert_retained_intact();
    }
    let answers: [fn() -> io::Result<i32>; 3] = [
        || Ok(libc::F_RDLCK),
        || Ok(libc::F_UNLCK),
        || Err(io::Error::from_raw_os_error(libc::EIO)),
    ];
    for answer in answers {
        let dead = DeadRoot::new();
        let get = |_: &File| answer();
        let ops = lease::LeaseOps {
            set: &lease::set_write,
            get: &get,
        };
        assert_eq!(dead.sweep_with(&nothing_found, &ops), (0, 1));
        dead.assert_retained_intact();
    }
    DeadRoot::new().assert_removed_after(&nothing_found);
}
// T4
#[test]
fn a_denial_does_not_hide_a_later_process_that_holds_the_root_another_way() {
    for (verb, held) in [("dir", "data"), ("cwd", "cache")] {
        let dead = DeadRoot::new();
        let denied = Holder::start(true);
        let visible = Holder::holding(false, verb, &dead.path(held));
        let order = [denied.process.clone(), visible.process.clone()];
        let audit: Audit<'_> = &|root, objects| process::walk(order.clone().map(Ok), root, objects);
        assert_eq!(dead.sweep_with(audit, &lease::KERNEL), (0, 1), "{verb}");
        dead.assert_retained_intact();
        assert!(denied.finish() && visible.finish(), "both must be live");
        dead.assert_removed_after(audit);
    }
}
// T5
#[test]
fn an_exhausted_walk_does_not_retain_a_free_root() {
    let dead = DeadRoot::new();
    dead.assert_removed_after(&bound_exhausted);
}
// T6
#[test]
fn an_exhausted_walk_still_retains_a_root_a_denied_process_holds() {
    let dead = DeadRoot::new();
    let holder = Holder::start(true).hold("read", &dead.path("data/target"));
    assert_eq!(dead.sweep_with(&bound_exhausted, &lease::KERNEL), (0, 1));
    dead.assert_retained_intact();
    assert!(holder.finish(), "the holder must have been live");
    dead.assert_removed_after(&bound_exhausted);
}
// T7. Replaced after the tree was inspected, which a lease step can only meet by opening.
#[test]
fn a_file_replaced_after_inspection_is_not_leased_or_removed() {
    let outside = namespace();
    let sentinel = outside.path().join("sentinel");
    disk::write(&sentinel, b"untouched").unwrap();
    let replacements: [&dyn Fn(&Path); 2] = [
        &|path| std::os::unix::fs::symlink(&sentinel, path).unwrap(),
        &|path| {
            disk::write(path, b"another inode").unwrap();
            disk::set_permissions(path, disk::Permissions::from_mode(0o600)).unwrap();
        },
    ];
    for replace in replacements {
        let dead = DeadRoot::new();
        let record = dead.path(record::NAME);
        let replaced = Cell::new(false);
        let set = |file: &File| {
            if !replaced.replace(true) {
                disk::rename(&record, dead.path("saved-record")).unwrap();
                replace(&record);
            }
            lease::set_write(file)
        };
        let ops = lease::LeaseOps {
            set: &set,
            get: &lease::get,
        };
        assert_eq!(dead.sweep_with(&nothing_found, &ops), (0, 1));
        assert!(replaced.get());
        assert_eq!(disk::read(&sentinel).unwrap(), b"untouched");
        assert_eq!(disk::read(dead.path("data/first")).unwrap(), FIRST);
        assert!(dead.path("saved-record").is_file());
    }
}
// T8
#[test]
fn a_file_that_cannot_be_opened_retains_the_root() {
    if fs::uid() == 0 {
        // The superuser opens any mode, so this input does not exist for it.
        return;
    }
    let dead = DeadRoot::new();
    let closed = dead.path("data/closed");
    disk::write(&closed, b"mode 000").unwrap();
    disk::set_permissions(&closed, disk::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(dead.sweep_with(&nothing_found, &lease::KERNEL), (0, 1));
    dead.assert_retained_intact();
    assert!(closed.exists());
    disk::set_permissions(&closed, disk::Permissions::from_mode(0o600)).unwrap();
    dead.assert_removed_after(&nothing_found);
}
// T9
#[test]
fn the_file_modes_webkit_creates_do_not_retain_the_root() {
    let dead = DeadRoot::new();
    for (name, mode) in [("a", 0o644), ("b", 0o664), ("c", 0o600), ("d", 0o400)] {
        let path = dead.path("cache").join(name);
        disk::write(&path, b"cache entry").unwrap();
        disk::set_permissions(&path, disk::Permissions::from_mode(mode)).unwrap();
    }
    dead.assert_removed_after(&nothing_found);
}
#[test]
fn a_second_link_of_a_file_in_the_root_does_not_break_its_own_lease() {
    let dead = DeadRoot::new();
    disk::hard_link(dead.path("data/target"), dead.path("data/link")).unwrap();
    dead.assert_removed_after(&nothing_found);
}
// T10
#[test]
fn an_open_during_the_leases_is_survived_and_retains_the_root() {
    run_alone("open_during_the_leases_alone");
}
#[test]
#[ignore = "owned subprocess entry: a lease breaks here and SIGIO reaches this process"]
fn open_during_the_leases_alone() {
    let dead = DeadRoot::new();
    let target = dead.path("data/target");
    let target_ino = disk::metadata(&target).unwrap().ino();
    let leased = Cell::new(0);
    let target_lease = RefCell::new(None);
    let opener = RefCell::new(None);
    let set = |file: &File| {
        lease::set_write(file)?;
        if file.metadata()?.ino() == target_ino {
            *target_lease.borrow_mut() = Some(file.try_clone()?);
        }
        leased.set(leased.get() + 1);
        // The record, first and target: all leases are held, none has been confirmed.
        if leased.get() == 3 {
            let child = Command::new("cat")
                .arg(&target)
                .stdout(Stdio::piped())
                .spawn()?;
            opener.borrow_mut().replace(child);
            let held = target_lease.borrow();
            wait_until_broken(held.as_ref().expect("target leased"));
        }
        Ok(())
    };
    let ops = lease::LeaseOps {
        set: &set,
        get: &lease::get,
    };
    assert_eq!(dead.sweep_with(&nothing_found, &ops), (0, 1));
    drop(target_lease);
    dead.assert_retained_intact();
    let started = Instant::now();
    let output = opener
        .take()
        .expect("the competing open ran")
        .wait_with_output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, target_content());
    assert!(started.elapsed() < Duration::from_secs(5), "opener waited");
}
fn wait_until_broken(lease_file: &File) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while lease::get(lease_file).unwrap() == libc::F_WRLCK {
        assert!(Instant::now() < deadline, "the lease never broke");
        std::thread::sleep(Duration::from_millis(1));
    }
}
// T11
#[test]
fn a_holder_that_alternates_between_files_retains_the_root() {
    run_alone("alternating_holder_alone");
}
#[test]
#[ignore = "owned subprocess entry: a lease breaks here and SIGIO reaches this process"]
fn alternating_holder_alone() {
    let dead = DeadRoot::new();
    let table_holder = Holder::start(true);
    let table = table_holder.table();
    let first = dead.path("data/first");
    let target = dead.path("data/target");
    let first_ino = disk::metadata(&first).unwrap().ino();
    let target_ino = disk::metadata(&target).unwrap().ino();
    let holder = RefCell::new(table_holder);
    let turned = Cell::new(false);
    let set = |file: &File| {
        let ino = file.metadata()?.ino();
        if (ino != first_ino && ino != target_ino) || turned.replace(true) {
            return lease::set_write(file);
        }
        // Whichever of the two comes first is free now and held right after its lease.
        let (this, other) = if ino == first_ino {
            (&first, &target)
        } else {
            (&target, &first)
        };
        let mut holder = holder.borrow_mut();
        holder.send("read", other);
        holder.ack();
        lease::set_write(file)?;
        holder.send("release", other);
        holder.ack();
        holder.send("read", this);
        // Its open stays blocked by this lease, if the lease is still held at all.
        let deadline = Instant::now() + Duration::from_secs(5);
        while lease::get(file)? == libc::F_WRLCK {
            assert!(Instant::now() < deadline, "the holder never tried the file");
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    };
    let ops = lease::LeaseOps {
        set: &set,
        get: &lease::get,
    };
    let audit: Audit<'_> = &|root, objects| process::consumers_in(table.path(), root, objects);
    assert_eq!(dead.sweep_with(audit, &ops), (0, 1));
    // A verdict reached before the leases would pass above and then wait for an answer forever.
    assert!(turned.get(), "the leases never met the holder");
    dead.assert_retained_intact();
    let mut holder = holder.into_inner();
    holder.ack();
    assert!(holder.finish(), "the holder must have been live");
    dead.assert_removed_after(audit);
}
// T12
#[test]
fn the_lease_break_handler_is_not_inherited_by_children() {
    run_alone("handler_inheritance_alone");
}
fn signal_set(status_of: &Path, field: &str) -> u64 {
    let status = disk::read_to_string(status_of.join("status")).unwrap();
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .unwrap_or_else(|| panic!("{field} in {status}"));
    u64::from_str_radix(value.trim(), 16).unwrap()
}
#[test]
#[ignore = "owned subprocess entry: the handler is process-wide and observed from a clean start"]
fn handler_inheritance_alone() {
    let sigio = 1u64 << (libc::SIGIO - 1);
    let own = Path::new("/proc/self");
    assert_eq!(signal_set(own, "SigCgt:") & sigio, 0, "already caught");
    assert_eq!(signal_set(own, "SigIgn:") & sigio, 0, "already ignored");
    let dead = DeadRoot::new();
    dead.assert_removed_after(&nothing_found);
    assert_ne!(signal_set(own, "SigCgt:") & sigio, 0, "not caught");
    assert_eq!(signal_set(own, "SigIgn:") & sigio, 0, "ignored");
    let child = Holder::start(false);
    let caught = signal_set(&child.process, "SigCgt:");
    let ignored = signal_set(&child.process, "SigIgn:");
    assert!(child.finish());
    assert_eq!(caught & sigio, 0, "the child inherited the handler");
    assert_eq!(ignored & sigio, 0, "the child inherited an ignore");
}
/// Some other code's choice for the signal stays, so the lease step cannot take it away.
#[test]
fn a_disposition_already_chosen_is_left_alone() {
    run_alone("chosen_disposition_alone");
}
#[test]
#[ignore = "owned subprocess entry: the handler is process-wide and observed from a clean start"]
fn chosen_disposition_alone() {
    // SAFETY: SIG_IGN is a valid disposition, and this owned child has nothing relying on SIGIO.
    unsafe { libc::signal(libc::SIGIO, libc::SIG_IGN) };
    lease::survive_lease_breaks().unwrap();
    // SAFETY: as above; this reads the disposition by swapping it back.
    let after = unsafe { libc::signal(libc::SIGIO, libc::SIG_DFL) };
    assert_eq!(after, libc::SIG_IGN);
}
// T13
#[test]
fn a_live_owner_is_kept_without_consulting_leases_or_walk() {
    let parent = namespace();
    let mut owner = OwnedChild::start(parent.path(), false);
    let denied = Holder::start(true);
    let set = |_: &File| -> io::Result<()> { panic!("a live owner's file was leased") };
    let get = |_: &File| -> io::Result<i32> { panic!("a live owner's file was leased") };
    let ops = lease::LeaseOps {
        set: &set,
        get: &get,
    };
    let audit: Audit<'_> = &|_, _| panic!("a live owner's root was walked");
    let r = admitted(|| reclaim::run_audited(parent.path(), 1, audit, &ops)).unwrap();
    assert_eq!((r.kept_live, r.removed, r.kept_uncertain), (1, 0, 0));
    assert!(owner.root.is_dir());
    assert!(denied.finish());
    owner.finish();
}
// T14
#[test]
fn a_retained_root_opens_without_delay_and_no_descriptor_stays() {
    run_alone("release_after_retention_alone");
}
#[test]
#[ignore = "owned subprocess entry: walks its own descriptors, which must be undisturbed"]
fn release_after_retention_alone() {
    let dead = DeadRoot::new();
    // The record is leased last. The holder refuses that lease with the others already granted.
    let holder = Holder::start(true).hold("read", &dead.path(record::NAME));
    // What a process forked meanwhile would do: keep the description past the close.
    let kept = RefCell::new(Vec::new());
    let set = |file: &File| {
        lease::set_write(file)?;
        kept.borrow_mut().push(file.try_clone()?);
        Ok(())
    };
    let ops = lease::LeaseOps {
        set: &set,
        get: &lease::get,
    };
    assert_eq!(dead.sweep_with(&nothing_found, &ops), (0, 1));
    assert_eq!(kept.borrow().len(), 2, "the two earlier leases were held");
    let started = Instant::now();
    let mut opener = Command::new("cat")
        .arg(dead.path("data/target"))
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = started + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = opener.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let released = status.is_some_and(|status| status.success());
    drop(kept);
    if status.is_none() {
        opener.wait().unwrap();
    }
    assert!(released, "a released lease must not make an opener wait");
    let own = PathBuf::from(format!("/proc/{}", std::process::id()));
    let objects = HashSet::new();
    assert!(!process::walk([Ok(own.clone())], &dead.root, &objects).unwrap());
    assert!(holder.finish(), "the holder must have been live");
    dead.assert_removed_after(&nothing_found);
    assert!(!process::walk([Ok(own)], &dead.root, &objects).unwrap());
}
// T15
#[test]
fn a_file_in_execution_or_mapped_executable_retains_the_root() {
    let dead = DeadRoot::new();
    let tool = dead.path("data/tool");
    disk::copy("/bin/sleep", &tool).unwrap();
    disk::set_permissions(&tool, disk::Permissions::from_mode(0o700)).unwrap();
    let mut running = loop {
        // A process another test forks can hold the copy's write descriptor for a moment.
        match Command::new(&tool).arg("30").spawn() {
            Ok(child) => break child,
            Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("execute in the root: {e}"),
        }
    };
    let verdict = dead.sweep_with(&nothing_found, &lease::KERNEL);
    running.kill().unwrap();
    running.wait().unwrap();
    assert_eq!(verdict, (0, 1), "executing");
    dead.assert_retained_intact();
    let holder = Holder::start(true).hold("map-exec", &dead.path("data/target"));
    assert_eq!(
        dead.sweep_with(&nothing_found, &lease::KERNEL),
        (0, 1),
        "mapped executable"
    );
    dead.assert_retained_intact();
    assert!(holder.finish(), "the holder must have been live");
    dead.assert_removed_after(&nothing_found);
}
/// Accepted limit, in the contract: a denied process that holds no regular file in the tree is
/// invisible to both the walk and the leases, and its root is removed. It loses no open file data.
#[test]
fn accepted_limit_a_denied_process_without_a_regular_file_does_not_retain_the_root() {
    for (verb, held) in [("dir", "data"), ("cwd", "cache"), ("o-path", "data/target")] {
        let dead = DeadRoot::new();
        let holder = Holder::holding(true, verb, &dead.path(held));
        let table = holder.table();
        dead.assert_removed_after(&|root, objects| {
            process::consumers_in(table.path(), root, objects)
        });
        assert!(holder.finish(), "{verb}: the holder must have been live");
    }
}
/// Accepted limit, in the contract: an unlinked file is no entry of the tree, so no lease covers it.
#[test]
fn accepted_limit_a_denied_process_holding_only_an_unlinked_file_does_not_retain_the_root() {
    let dead = DeadRoot::new();
    let target = dead.path("data/target");
    let holder = Holder::start(true).hold("read", &target);
    disk::remove_file(&target).unwrap();
    let table = holder.table();
    dead.assert_removed_after(&|root, objects| process::consumers_in(table.path(), root, objects));
    assert!(holder.finish(), "the holder must have been live");
}
/// A visible process, by contrast, keeps showing a deleted file below the root.
#[test]
fn a_visible_process_holding_an_unlinked_file_retains_the_root() {
    let dead = DeadRoot::new();
    let target = dead.path("data/target");
    let holder = Holder::start(false).hold("read", &target);
    disk::remove_file(&target).unwrap();
    let table = holder.table();
    let audit: Audit<'_> = &|root, objects| process::consumers_in(table.path(), root, objects);
    assert_eq!(dead.sweep_with(audit, &lease::KERNEL), (0, 1));
    assert!(holder.finish(), "the holder must have been live");
    dead.assert_removed_after(audit);
}
fn with_recorded_owner(root: &Path, change: impl FnOnce(&mut serde_json::Value)) {
    let path = root.join(record::NAME);
    let mut value: serde_json::Value = serde_json::from_slice(&disk::read(&path).unwrap()).unwrap();
    change(&mut value["owner"]);
    disk::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
}
const ANOTHER_BOOT: &str = "00000000-0000-0000-0000-000000000000";
/// ext2/3/4, xfs, btrfs and tmpfs, written apart from the list under test.
const ONE_KERNEL_FILE_SYSTEMS: [u32; 4] = [0xEF53, 0x5846_5342, 0x9123_683E, 0x0102_1994];
/// The verdict by file system type alone, including those no test here can mount.
#[test]
fn another_boot_ends_an_owner_only_on_a_file_system_one_kernel_serves() {
    for local in ONE_KERNEL_FILE_SYSTEMS {
        assert!(
            matches!(
                process::owner_of_another_boot(Some(local)),
                process::OwnerState::Dead
            ),
            "{local:#x}"
        );
    }
    // FUSE, NFS, overlay, CIFS, SMB2, an unknown type and a failed query.
    for other in [
        Some(0x6573_5546),
        Some(0x6969),
        Some(0x794C_7630),
        Some(0xFF53_4D42),
        Some(0xFE53_4D42),
        Some(0),
        None,
    ] {
        assert!(
            matches!(
                process::owner_of_another_boot(other),
                process::OwnerState::Uncertain
            ),
            "{other:?}"
        );
    }
}
/// A reboot ends every owner, and a namespace on disk would otherwise never recover. The record is
/// not proof of its boot: altering it makes even a live owner's root a candidate, as the contract says.
/// What each place expects comes from its measured type, so this holds wherever the tests run.
#[test]
fn an_owner_of_another_boot_follows_the_file_system_of_its_namespace() {
    let mut parents = vec![namespace()];
    if Path::new("/dev/shm").is_dir() {
        let parent = tempfile::Builder::new()
            .prefix("tasty-profile-proof-")
            .tempdir_in("/dev/shm")
            .unwrap();
        disk::set_permissions(parent.path(), disk::Permissions::from_mode(0o700)).unwrap();
        parents.push(parent);
    }
    for parent in parents {
        let dead = DeadRoot::under(parent);
        with_recorded_owner(&dead.root, |owner| {
            owner["boot"] = serde_json::json!(ANOTHER_BOOT);
            owner["pid"] = serde_json::json!(std::process::id());
        });
        let opened = fs::Namespace::open(dead.parent.path()).unwrap();
        let file_system = opened.dir.file_system_type();
        // From outside this code: an answer of None would pass everything below as "retained".
        let measured = Command::new("stat")
            .args(["-f", "-c", "%t"])
            .arg(dead.parent.path())
            .output()
            .unwrap();
        let measured = String::from_utf8(measured.stdout).unwrap();
        let measured = u32::from_str_radix(measured.trim(), 16).unwrap();
        assert_eq!(file_system, Some(measured));
        let root = opened.dir.open_dir(dead.root.file_name().unwrap()).unwrap();
        let record = record::Record::read(&root).unwrap();
        // The type does reach the verdict, on a real record of another boot.
        assert!(matches!(
            record.owner.state(Some(0x6573_5546)).unwrap(),
            process::OwnerState::Uncertain
        ));
        assert!(matches!(
            record.owner.state(None).unwrap(),
            process::OwnerState::Uncertain
        ));
        drop((root, opened));
        let holder = Holder::start(true).hold("read", &dead.path("data/target"));
        assert_eq!(dead.sweep_with(&nothing_found, &lease::KERNEL), (0, 1));
        dead.assert_retained_intact();
        assert!(holder.finish(), "the holder must have been live");
        if ONE_KERNEL_FILE_SYSTEMS.contains(&measured) {
            dead.assert_removed_after(&nothing_found);
        } else {
            assert_eq!(dead.sweep_with(&nothing_found, &lease::KERNEL), (0, 1));
            dead.assert_retained_intact();
        }
    }
}
/// The recorded PID now names another live process: uncertain, whatever the files say, until it ends.
#[test]
fn a_reused_pid_retains_the_root_until_that_process_ends() {
    let dead = DeadRoot::new();
    let reused = Holder::start(false);
    let pid: u32 = reused
        .process
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    with_recorded_owner(&dead.root, |owner| {
        owner["pid"] = serde_json::json!(pid);
    });
    let forbidden = |_: &File| -> io::Result<()> { panic!("an uncertain owner's file was leased") };
    let ops = lease::LeaseOps {
        set: &forbidden,
        get: &lease::get,
    };
    assert_eq!(dead.sweep_with(&nothing_found, &ops), (0, 1));
    dead.assert_retained_intact();
    assert!(reused.finish(), "the reusing process must have been live");
    dead.assert_removed_after(&nothing_found);
}
/// A process of this UID that denies inspection leaves the walk incomplete, and the walk says so.
#[test]
fn a_denied_process_leaves_the_walk_incomplete_rather_than_complete() {
    let holder = Holder::start(true);
    let table = holder.table();
    let root = Path::new("/nonexistent-tasty-profile-root");
    let objects = HashSet::new();
    let alone = process::consumers_in(table.path(), root, &objects);
    let given = process::walk([Ok(holder.process.clone())], root, &objects);
    assert!(holder.finish(), "the denied process must have been live");
    for walked in [alone, given] {
        let error = walked.expect_err("completed past a live denial");
        assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    }
    // A reaped process consumes nothing and leaves nothing unseen.
    let reaped = process::consumers_in(table.path(), root, &objects);
    assert!(matches!(reaped, Ok(false)), "{reaped:?}");
}
fn descriptor_limit() -> libc::rlimit {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: limit is a live rlimit for the call.
    let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) };
    assert_eq!(result, 0);
    limit
}
fn set_own_descriptor_limit(limit: &libc::rlimit) {
    // SAFETY: limit is a live rlimit for the call; only the owned child that runs this calls it.
    let result = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, limit) };
    assert_eq!(result, 0);
}
// EMFILE: a root too large for this process's descriptors stays, with no limit raised.
#[test]
fn a_root_beyond_the_descriptor_limit_is_retained_without_raising_the_limit() {
    run_alone("descriptor_limit_alone");
}
#[test]
#[ignore = "owned subprocess entry: lowers its own soft descriptor limit"]
fn descriptor_limit_alone() {
    let dead = DeadRoot::new();
    for index in 0..200 {
        disk::write(dead.path("cache").join(format!("{index}")), b"entry").unwrap();
    }
    let original = descriptor_limit();
    let open = disk::read_dir("/proc/self/fd").unwrap().count() as u64;
    let lowered = libc::rlimit {
        rlim_cur: open + 60,
        rlim_max: original.rlim_max,
    };
    set_own_descriptor_limit(&lowered);
    let tried = Cell::new(0);
    let set = |file: &File| {
        tried.set(tried.get() + 1);
        lease::set_write(file)
    };
    let ops = lease::LeaseOps {
        set: &set,
        get: &lease::get,
    };
    let verdict = dead.sweep_with(&nothing_found, &ops);
    assert_eq!(
        descriptor_limit().rlim_cur,
        lowered.rlim_cur,
        "the sweep moved the limit"
    );
    assert_eq!(verdict, (0, 1));
    dead.assert_retained_intact();
    // Running out instead of declining would have failed the other threads' opens meanwhile.
    assert_eq!(
        tried.get(),
        0,
        "the root was tried beyond the descriptor margin"
    );
    set_own_descriptor_limit(&original);
    dead.assert_removed_after(&nothing_found);
}
