use super::*;
use std::{
    fs as disk,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    sync::Arc,
};
pub(super) fn namespace() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("tasty-profile-proof-")
        .tempdir()
        .unwrap();
    disk::set_permissions(dir.path(), disk::Permissions::from_mode(0o700)).unwrap();
    dir
}
#[test]
fn private_permissions_unique_paths_and_memory_only_native_lifetime() {
    fn send<T: Send>() {}
    fn shared<T: Send + Sync>() {}
    send::<PreparedProfile>();
    shared::<NativeProfileLease>();
    let parent = namespace();
    let path = parent.path().to_path_buf();
    let prepared =
        std::thread::spawn(move || admitted(|| PreparedProfile::create(&path, 3)).unwrap())
            .join()
            .unwrap();
    let root = prepared.root().to_path_buf();
    let data = prepared.data_dir().to_path_buf();
    let cache = prepared.cache_dir().to_path_buf();
    assert_ne!(data, cache);
    for p in [&root, &data, &cache] {
        let m = disk::metadata(p).unwrap();
        assert_eq!(m.mode() & 0o7777, 0o700);
        assert_eq!(m.uid(), fs::uid());
    }
    let m = disk::metadata(root.join(record::NAME)).unwrap();
    assert_eq!(m.mode() & 0o7777, 0o600);
    let lease = std::thread::spawn(move || prepared.into_native())
        .join()
        .unwrap();
    let clone = lease.clone();
    assert_eq!(lease.root(), root);
    assert_eq!(lease.data_dir(), data);
    assert_eq!(lease.cache_dir(), cache);
    drop(lease);
    drop(clone);
    assert!(root.is_dir());
    assert!(root.join(record::NAME).is_file());
    let second = admitted(|| PreparedProfile::create(parent.path(), 3)).unwrap();
    assert_ne!(second.root(), root);
    discard(second).unwrap();
    let report = reclaim_positive(parent.path(), 3);
    assert_eq!(
        (report.inspected, report.kept_live, report.removed),
        (1, 1, 0)
    );
    assert!(root.is_dir());
}
#[test]
fn explicit_unpublished_discard_removes_only_owned_root() {
    let parent = namespace();
    let outside = namespace();
    disk::write(outside.path().join("sentinel"), b"outside").unwrap();
    let p = admitted(|| PreparedProfile::create(parent.path(), 2)).unwrap();
    let root = p.root().to_owned();
    disk::write(p.data_dir().join("file"), b"owned").unwrap();
    std::thread::spawn(move || discard(p).unwrap())
        .join()
        .unwrap();
    assert!(!root.exists());
    assert_eq!(
        disk::read(outside.path().join("sentinel")).unwrap(),
        b"outside"
    );
}
#[test]
fn capacity_counts_unclassified_entries_and_parallel_owners() {
    let parent = namespace();
    let warm = admitted(|| PreparedProfile::create(parent.path(), 1)).unwrap();
    discard(warm).unwrap();
    let start = Arc::new(std::sync::Barrier::new(9));
    let mut threads = vec![];
    for _ in 0..8 {
        let start = start.clone();
        let path = parent.path().to_owned();
        threads.push(std::thread::spawn(move || {
            start.wait();
            admitted(|| PreparedProfile::create(&path, 2))
        }));
    }
    start.wait();
    let roots: Vec<_> = threads
        .into_iter()
        .filter_map(|t| t.join().unwrap().ok())
        .collect();
    assert!(!roots.is_empty());
    assert!(roots.len() <= 2);
    let count = disk::read_dir(parent.path()).unwrap().count() - 1;
    assert_eq!(count, roots.len());
    for p in roots {
        discard(p).unwrap();
    }
    disk::write(parent.path().join("unknown"), b"unclassified").unwrap();
    assert!(matches!(
        admitted(|| PreparedProfile::create(parent.path(), 1)),
        Err(ProfileError::Capacity)
    ));
    assert!(matches!(
        PreparedProfile::create(parent.path(), 0),
        Err(ProfileError::Invalid)
    ));
    assert!(matches!(
        reclaim_dead(parent.path(), 0),
        Err(ProfileError::Invalid)
    ));
}

#[test]
fn rejects_missing_relative_symlink_and_unsafe_ancestry_without_creating_fallback() {
    let parent = namespace();
    let missing = parent.path().join("missing");
    assert!(
        matches!(PreparedProfile::create(&missing,1),Err(ProfileError::Io(e)) if e.kind()==std::io::ErrorKind::NotFound)
    );
    assert!(!missing.exists());
    assert!(matches!(
        PreparedProfile::create(std::path::Path::new("relative"), 1),
        Err(ProfileError::Invalid)
    ));
    let outside = namespace();
    let alias = parent.path().join("alias");
    std::os::unix::fs::symlink(outside.path(), &alias).unwrap();
    assert!(admitted(|| PreparedProfile::create(&alias, 1)).is_err());
    assert_eq!(disk::read_dir(outside.path()).unwrap().count(), 0);
    let unsafe_dir = parent.path().join("writable");
    disk::create_dir(&unsafe_dir).unwrap();
    disk::set_permissions(&unsafe_dir, disk::Permissions::from_mode(0o775)).unwrap();
    let leaf = unsafe_dir.join("private");
    disk::create_dir(&leaf).unwrap();
    disk::set_permissions(&leaf, disk::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        admitted(|| PreparedProfile::create(&leaf, 1)),
        Err(ProfileError::Ownership)
    ));
    assert_eq!(disk::read_dir(leaf).unwrap().count(), 0);
}
#[test]
fn non_utf8_namespace_is_recorded_exactly() {
    use std::os::unix::ffi::OsStringExt;
    let base = namespace();
    let parent = base
        .path()
        .join(std::ffi::OsString::from_vec(b"private-\xff".to_vec()));
    disk::create_dir(&parent).unwrap();
    disk::set_permissions(&parent, disk::Permissions::from_mode(0o700)).unwrap();
    let p = admitted(|| PreparedProfile::create(&parent, 1)).unwrap();
    let data: serde_json::Value =
        serde_json::from_slice(&disk::read(p.root().join(record::NAME)).unwrap()).unwrap();
    let bytes: Vec<u8> = serde_json::from_value(data["root_path"].clone()).unwrap();
    assert_eq!(bytes, p.root().as_os_str().as_bytes());
    discard(p).unwrap();
}
#[test]
fn root_inode_replacement_and_symlinks_never_delete_outside_sentinels() {
    let parent = namespace();
    let outside = namespace();
    let sentinel = outside.path().join("sentinel");
    disk::write(&sentinel, b"untouched").unwrap();
    let p = admitted(|| PreparedProfile::create(parent.path(), 3)).unwrap();
    let root = p.root().to_owned();
    let saved = parent.path().join("saved");
    disk::rename(&root, &saved).unwrap();
    disk::create_dir(&root).unwrap();
    disk::set_permissions(&root, disk::Permissions::from_mode(0o700)).unwrap();
    disk::write(root.join("sentinel"), b"replacement").unwrap();
    assert!(matches!(discard(p), Err(ProfileError::Ownership)));
    assert_eq!(disk::read(root.join("sentinel")).unwrap(), b"replacement");
    let r = reclaim_positive(parent.path(), 3);
    assert_eq!((r.inspected, r.removed, r.kept_uncertain), (2, 0, 2));
    let parent = namespace();
    let p = admitted(|| PreparedProfile::create(parent.path(), 1)).unwrap();
    let root = p.root().to_owned();
    drop(p);
    disk::rename(&root, parent.path().join("saved")).unwrap();
    std::os::unix::fs::symlink(outside.path(), &root).unwrap();
    let r = reclaim_positive(parent.path(), 4);
    assert_eq!(r.removed, 0);
    assert!(r.kept_uncertain >= 1);
    assert_eq!(disk::read(sentinel).unwrap(), b"untouched");
}
#[test]
fn prepared_discard_rejects_inner_symlink_before_any_deletion() {
    let parent = namespace();
    let outside = namespace();
    disk::write(outside.path().join("sentinel"), b"outside").unwrap();
    let p = admitted(|| PreparedProfile::create(parent.path(), 1)).unwrap();
    let root = p.root().to_owned();
    std::os::unix::fs::symlink(outside.path(), p.data_dir().join("escape")).unwrap();
    assert!(discard(p).is_err());
    assert!(root.join(record::NAME).exists());
    assert!(root.join("cache").exists());
    assert_eq!(
        disk::read(outside.path().join("sentinel")).unwrap(),
        b"outside"
    );
}
#[test]
fn malformed_foreign_record_inode_start_and_record_symlink_are_retained() {
    for kind in 0..6 {
        let parent = namespace();
        let p = admitted(|| PreparedProfile::create(parent.path(), 1)).unwrap();
        let root = p.root().to_owned();
        let path = root.join(record::NAME);
        drop(p);
        let mut value: serde_json::Value =
            serde_json::from_slice(&disk::read(&path).unwrap()).unwrap();
        match kind {
            0 => {
                disk::write(&path, b"malformed").unwrap();
            }
            1 => {
                value["owner"]["uid"] = serde_json::json!(fs::uid() + 1);
                disk::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            2 => {
                value["root"]["ino"] = serde_json::json!(0);
                disk::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            3 => {
                value["owner"]["start"] =
                    serde_json::json!(value["owner"]["start"].as_u64().unwrap() + 1);
                disk::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            4 => {
                value["version"] = serde_json::json!(2);
                disk::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            _ => {
                let saved = root.join("saved-record");
                disk::rename(&path, &saved).unwrap();
                std::os::unix::fs::symlink(saved, &path).unwrap();
            }
        }
        let r = reclaim_positive(parent.path(), 1);
        assert_eq!(
            (r.inspected, r.removed, r.kept_live, r.kept_uncertain),
            (1, 0, 0, 1)
        );
        assert!(root.is_dir());
    }
}
#[test]
fn bounded_sweep_retains_live_roots_and_reports_remaining_entries() {
    let parent = namespace();
    for _ in 0..3 {
        drop(admitted(|| PreparedProfile::create(parent.path(), 3)).unwrap());
    }
    let r = reclaim_positive(parent.path(), 2);
    assert_eq!((r.inspected, r.kept_live, r.removed), (2, 2, 0));
    assert!(r.limit_reached);
    let r = reclaim_positive(parent.path(), 3);
    assert_eq!((r.inspected, r.kept_live), (3, 3));
    assert!(!r.limit_reached);
}
#[test]
fn preflight_then_root_replacement_is_detected_before_removal() {
    let parent = namespace();
    let outside = namespace();
    disk::write(outside.path().join("sentinel"), b"untouched").unwrap();
    let p = admitted(|| PreparedProfile::create(parent.path(), 1)).unwrap();
    let root = p.root().to_owned();
    drop(p);
    let ns = fs::Namespace::open(parent.path()).unwrap();
    let name = root.file_name().unwrap();
    let original_record = record::Record::read(&ns.dir.open_dir(name).unwrap()).unwrap();
    let tree = reclaim::Tree::inspect(ns.dir.open_dir(name).unwrap()).unwrap();
    disk::rename(&root, parent.path().join("moved")).unwrap();
    std::os::unix::fs::symlink(outside.path(), &root).unwrap();
    assert!(tree.dispose(&ns, name, &original_record).is_err());
    assert_eq!(
        disk::read(outside.path().join("sentinel")).unwrap(),
        b"untouched"
    );
}

/// The line the owned child prints once its profile is prepared. The newline ends it, so the
/// parent never reads a root that is still being written.
const ROOT_LINE: &[u8] = b"owned-profile-root=";
pub(super) struct OwnedChild {
    child: Option<std::process::Child>,
    // Both pipes are read to the end on their own threads, so the child never blocks on a full pipe.
    stdout: Option<std::thread::JoinHandle<Vec<u8>>>,
    stderr: Option<std::thread::JoinHandle<Vec<u8>>>,
    pub(super) root: PathBuf,
    pid: u32,
}
impl OwnedChild {
    /// Preparation includes the lock file and owner record fsyncs, so slow storage has no deadline
    /// here. The child reports its root before it ends, so an end without one fails at once, and
    /// only a child that never reports reaches `HANG_LIMIT`.
    pub(super) fn start(parent: &Path, restrictive_umask: bool) -> Self {
        use crate::host_api::webview::http_endpoint::runtime::tests::HANG_LIMIT;
        use std::io::{BufRead, Read};
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        let module = module_path!().split_once("::").unwrap().1;
        let entry = format!("{module}::child_writer");
        let started = std::time::Instant::now();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                &entry,
                "--nocapture",
                "--test-threads=1",
            ])
            .env("TASTY_PROFILE_PROOF_PARENT", parent)
            .env(
                "TASTY_PROFILE_PROOF_UMASK",
                if restrictive_umask { "1" } else { "0" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (roots, root) = mpsc::channel();
        let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
        let stdout = std::thread::spawn(move || {
            let mut all = Vec::new();
            let mut line = Vec::new();
            loop {
                match stdout.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!("owned child stdout: {error}");
                        break;
                    }
                }
                if let Some(path) = line.strip_prefix(ROOT_LINE)
                    && let Some(path) = path.strip_suffix(b"\n")
                    && let Err(error) = roots.send(path.to_vec())
                {
                    tracing::warn!("owned child root has no reader: {error}");
                }
                all.append(&mut line);
            }
            all
        });
        let mut stderr_pipe = child.stderr.take().unwrap();
        let stderr = std::thread::spawn(move || {
            let mut all = Vec::new();
            if let Err(error) = stderr_pipe.read_to_end(&mut all) {
                tracing::warn!("owned child stderr: {error}");
            }
            all
        });
        let pid = child.id();
        let mut owned = Self {
            child: Some(child),
            stdout: Some(stdout),
            stderr: Some(stderr),
            root: PathBuf::new(),
            pid,
        };
        match root.recv_timeout(HANG_LIMIT) {
            Ok(path) => {
                use std::os::unix::ffi::OsStringExt;
                owned.root = std::ffi::OsString::from_vec(path).into();
                owned
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                owned.abort("ended its output without a profile root", started)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => owned.abort(
                &format!("hung, no profile root within {HANG_LIMIT:?}"),
                started,
            ),
        }
    }
    /// Reaps the child first, so the panic leaves nothing for `Drop` to wait on or judge.
    fn abort(&mut self, what: &str, started: std::time::Instant) -> ! {
        let mut child = self.child.take().unwrap();
        let alive = match child.try_wait() {
            Ok(None) => "alive".to_owned(),
            Ok(Some(status)) => format!("ended {status}"),
            Err(error) => format!("unknown ({error})"),
        };
        if let Err(error) = child.kill() {
            tracing::warn!("owned child kill: {error}");
        }
        let status = child.wait();
        let (stdout, stderr) = self.drained();
        panic!(
            "owned child pid={} {what}: after {:?}, child {alive}, reaped {status:?}\n\
             stderr:\n{stderr}\nstdout:\n{stdout}",
            self.pid,
            started.elapsed()
        );
    }
    fn drained(&mut self) -> (String, String) {
        let text = |pipe: Option<std::thread::JoinHandle<Vec<u8>>>| match pipe.map(|p| p.join()) {
            Some(Ok(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
            Some(Err(_)) => "<reader panicked>".to_owned(),
            None => String::new(),
        };
        (text(self.stdout.take()), text(self.stderr.take()))
    }
    /// Closes stdin so the child ends normally, then waits for it and both of its pipes.
    fn reaped(&mut self) -> Option<(std::process::ExitStatus, String)> {
        let mut child = self.child.take()?;
        drop(child.stdin.take());
        let status = child
            .wait()
            .unwrap_or_else(|error| panic!("owned child wait failed: {error}"));
        let (_, stderr) = self.drained();
        Some((status, stderr))
    }
    pub(super) fn finish(&mut self) {
        let (status, stderr) = self.reaped().unwrap();
        assert!(status.success(), "owned child failed: {stderr}");
        assert!(!Path::new(&format!("/proc/{}", self.pid)).exists());
        tracing::info!(
            "owned-profile-child pid={} normal_exit=0 absent=true",
            self.pid
        );
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let Some((status, stderr)) = self.reaped() else {
            return;
        };
        // A test that is already failing keeps its own message; a second panic would abort the binary.
        if std::thread::panicking() {
            if !status.success() {
                tracing::warn!("owned child failed during a failing test: {status} {stderr}");
            }
        } else {
            assert!(status.success(), "owned child failed: {stderr}");
        }
    }
}
#[test]
#[ignore = "owned subprocess entry, invoked only with isolated proof paths"]
fn child_writer() {
    use std::io::{Read, Write};
    let parent = std::env::var_os("TASTY_PROFILE_PROOF_PARENT").expect("owned test parent");
    if std::env::var_os("TASTY_PROFILE_PROOF_UMASK").as_deref() == Some(std::ffi::OsStr::new("1")) {
        // SAFETY: this is the dedicated owned child process; it changes no parent's umask.
        unsafe { libc::umask(0o777) };
    }
    let profile = admitted(|| PreparedProfile::create(Path::new(&parent), 1)).unwrap();
    let lease = profile.into_native();
    let root = lease.root().as_os_str().as_bytes();
    assert!(!root.contains(&b'\n'), "a root line holds no newline");
    let mut out = std::io::stdout().lock();
    // The harness has already printed this test's name without a newline.
    out.write_all(&[b"\n", ROOT_LINE, root, b"\n"].concat())
        .unwrap();
    out.flush().unwrap();
    drop(out);
    let mut byte = [0];
    let received = std::io::stdin().read(&mut byte).unwrap();
    assert_eq!(received, 0, "owned parent closes stdin for normal exit");
    drop(lease);
}
#[test]
fn restrictive_child_umask_still_creates_explicit_private_permissions() {
    let parent = namespace();
    let mut child = OwnedChild::start(parent.path(), true);
    for p in [
        child.root.clone(),
        child.root.join("data"),
        child.root.join("cache"),
    ] {
        assert_eq!(disk::metadata(p).unwrap().mode() & 0o7777, 0o700);
    }
    assert_eq!(
        disk::metadata(child.root.join(record::NAME))
            .unwrap()
            .mode()
            & 0o7777,
        0o600
    );
    let r = reclaim_positive(parent.path(), 1);
    assert_eq!((r.kept_live, r.removed), (1, 0));
    child.finish();
    assert!(child.root.is_dir());
}
/// The audit walks every thread of this process, so a thread of a parallel test that exits during
/// the walk would decide its outcome. The whole proof runs alone in an owned child process.
#[test]
fn actual_child_identity_live_retention_and_open_fd_consumer_are_observed() {
    let module = module_path!().split_once("::").unwrap().1;
    let entry = format!("{module}::child_identity_proof_alone");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            &entry,
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "owned proof child: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
#[ignore = "owned subprocess entry, invoked alone by its parent test"]
fn child_identity_proof_alone() {
    use std::io::Write;
    let parent = namespace();
    let mut child = OwnedChild::start(parent.path(), false);
    let ns = fs::Namespace::open(parent.path()).unwrap();
    let dir = ns.dir.open_dir(child.root.file_name().unwrap()).unwrap();
    let rec = record::Record::read(&dir).unwrap();
    assert_eq!(rec.owner.pid, child.pid);
    assert_eq!(rec.owner.uid, fs::uid());
    assert!(matches!(
        rec.owner.state(None).unwrap(),
        process::OwnerState::Live
    ));
    drop(dir);
    let r = reclaim_positive(parent.path(), 1);
    assert_eq!((r.kept_live, r.removed), (1, 0));
    let file_path = child.root.join("data/open");
    let mut file = disk::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&file_path)
        .unwrap();
    file.write_all(b"owned").unwrap();
    file.set_len(4096).unwrap();
    child.finish();
    assert!(matches!(
        rec.owner.state(None).unwrap(),
        process::OwnerState::Dead
    ));
    let tree =
        reclaim::Tree::inspect(ns.dir.open_dir(child.root.file_name().unwrap()).unwrap()).unwrap();
    let mut objects = std::collections::HashSet::new();
    tree.observations(&mut objects);
    drop(tree);
    fd_and_mapping_consumers(&child.root, &objects, file);
    public_dead_sweep(&child.root, parent.path());
}
/// Whatever the real process table of this machine denies or exhausts, a free root is removed.
fn public_dead_sweep(root: &Path, parent: &Path) {
    let r = swept_until_removed(parent, 1);
    assert_eq!((r.inspected, r.kept_live, r.failed), (1, 0, 0));
    assert_eq!((r.removed, r.kept_uncertain), (1, 0));
    assert!(!root.exists());
}

fn fd_and_mapping_consumers(
    root: &Path,
    objects: &std::collections::HashSet<(u64, u64)>,
    file: std::fs::File,
) {
    use std::os::fd::AsRawFd;
    let our_proc = PathBuf::from(format!("/proc/{}", std::process::id()));
    let audit = || process::walk([Ok(our_proc.clone())], root, objects);
    assert!(audit().unwrap());
    // SAFETY: file is an owned 4096-byte test file, descriptor is live and offset0 is aligned.
    let mapping = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            4096,
            libc::PROT_READ,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            0,
        )
    };
    assert_ne!(mapping, libc::MAP_FAILED);
    drop(file);
    let mapped_consumer = audit().unwrap();
    // SAFETY: mapping is exactly the successful mmap allocation above, released once.
    let unmapped = unsafe { libc::munmap(mapping, 4096) };
    assert_eq!(unmapped, 0);
    assert!(
        mapped_consumer,
        "maps must detect a consumer after its FD closes"
    );
    assert!(!audit().unwrap());
}

#[test]
fn deletion_error_retains_ownership_record_for_remaining_root() {
    let parent = namespace();
    let p = admitted(|| PreparedProfile::create(parent.path(), 1)).unwrap();
    let root = p.root().to_owned();
    let namespace = fs::Namespace::open(parent.path()).unwrap();
    let name = root.file_name().unwrap();
    let original_record = record::Record::read(&namespace.dir.open_dir(name).unwrap()).unwrap();
    let tree = reclaim::Tree::inspect(namespace.dir.open_dir(name).unwrap()).unwrap();
    disk::write(
        root.join("late-owned-file"),
        b"must remain after failed rmdir",
    )
    .unwrap();
    assert!(tree.dispose(&namespace, name, &original_record).is_err());
    assert!(
        root.join(record::NAME).exists(),
        "failed cleanup must retain the exact ownership record"
    );
    assert_eq!(
        disk::read(root.join("late-owned-file")).unwrap(),
        b"must remain after failed rmdir"
    );
    assert!(
        record::Record::read(&namespace.dir.open_dir(name).unwrap()).unwrap() == original_record
    );
    disk::remove_file(root.join("late-owned-file")).unwrap();
    discard(p).unwrap();
}

// Creating a process anywhere in this multi-threaded test binary copies every open
// descriptor into the child until it execs or closes them, so a namespace lock description
// one of these tests just closed can stay locked in a child for a moment. For create and the
// reclaim_dead sweep, production admission (preparation/admission.rs) retries exactly this
// typed Busy before a deadline; these tests call the nonblocking API directly, so they take
// the same bounded admission and then judge the first other result. The unpublished discard
// gets the same retry here as on the production worker (preparation.rs): Busy returns before
// anything is removed.
pub(super) fn admitted<T>(
    mut operation: impl FnMut() -> Result<T, ProfileError>,
) -> Result<T, ProfileError> {
    let deadline = std::time::Instant::now() + SWEEP_LIMIT;
    loop {
        match operation() {
            Err(ProfileError::Busy) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(2))
            }
            other => return other,
        }
    }
}
fn discard(profile: PreparedProfile) -> Result<(), ProfileError> {
    admitted(|| profile.remove_unpublished())
}
pub(super) fn reclaim_positive(parent: &Path, max: usize) -> CleanupReport {
    admitted(|| reclaim_dead(parent, max))
        .unwrap_or_else(|error| panic!("positive private profile sweep: {error}"))
}
/// A process another test forks keeps every description open here until it execs, so a file this
/// test just closed can still refuse a lease for a moment. Production leaves that to its next sweep.
#[track_caller]
pub(super) fn swept_until_removed(parent: &Path, max: usize) -> CleanupReport {
    until_removed(parent, || reclaim_positive(parent, max))
}
/// How long a retried sweep or admission may be refused. A child forked elsewhere in this binary
/// holds what it copied until it execs, and a loaded machine can leave it unscheduled for seconds.
/// Both loops return at their first success, so only something that keeps holding reaches this.
pub(super) const SWEEP_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);
/// Sweeps until one removes a root under `parent`. A root still retained at `SWEEP_LIMIT` fails
/// with what was holding it, since the last report alone cannot tell the causes apart.
#[track_caller]
pub(super) fn until_removed(
    parent: &Path,
    mut sweep: impl FnMut() -> CleanupReport,
) -> CleanupReport {
    let started = std::time::Instant::now();
    let mut sweeps = 0;
    loop {
        let report = sweep();
        sweeps += 1;
        if report.removed > 0 {
            return report;
        }
        if started.elapsed() >= SWEEP_LIMIT {
            panic!(
                "root still retained after {sweeps} sweeps in {:?}, last {report:?}\n{}",
                started.elapsed(),
                retention_diagnosis(parent)
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
/// What holds each root under `parent`: the owner pid of its record, a lease tried on each of its
/// regular files, and every process of this user that shows one of its objects or denies the look.
fn retention_diagnosis(parent: &Path) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let entries = match disk::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) => return format!("namespace {}: {error}", parent.display()),
    };
    for root in entries.filter_map(Result::ok).map(|e| e.path()) {
        if !root.is_dir() {
            continue;
        }
        writeln!(out, "root {}", root.display()).unwrap();
        match disk::read(root.join(record::NAME)) {
            Ok(text) => {
                let owner = serde_json::from_slice::<record::Record>(&text)
                    .ok()
                    .map(|r| r.owner.pid);
                let alive = owner.map(|pid| Path::new(&format!("/proc/{pid}/stat")).exists());
                writeln!(
                    out,
                    "  owner pid {owner:?}, /proc/<pid>/stat exists {alive:?}"
                )
                .unwrap();
            }
            Err(error) => writeln!(out, "  record: {error}").unwrap(),
        }
        let mut objects = std::collections::HashSet::new();
        let mut files = Vec::new();
        collect_objects(&root, &mut objects, &mut files);
        for file in &files {
            writeln!(out, "  lease {}: {}", file.display(), lease_attempt(file)).unwrap();
        }
        holding_processes(&root, &objects, &mut out);
    }
    out
}
fn collect_objects(
    path: &Path,
    objects: &mut std::collections::HashSet<(u64, u64)>,
    files: &mut Vec<PathBuf>,
) {
    let Ok(m) = disk::symlink_metadata(path) else {
        return;
    };
    objects.insert((m.dev(), m.ino()));
    if m.is_file() {
        files.push(path.to_owned());
    } else if m.is_dir() {
        for entry in disk::read_dir(path).into_iter().flatten().flatten() {
            collect_objects(&entry.path(), objects, files);
        }
    }
}
/// A write lease is refused while any other description of the file is open anywhere.
fn lease_attempt(file: &Path) -> String {
    use std::os::fd::AsRawFd;
    if let Err(error) = lease::survive_lease_breaks() {
        return format!("lease break handler: {error}");
    }
    let file = match disk::File::open(file) {
        Ok(file) => file,
        Err(error) => return format!("open: {error}"),
    };
    match lease::set_write(&file) {
        Ok(()) => {
            // SAFETY: fcntl on a descriptor this function owns.
            let released =
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLEASE, libc::F_UNLCK) };
            format!("granted (released {released})")
        }
        Err(error) => format!("refused: {error}"),
    }
}
/// Every process of this user whose descriptors, mappings, cwd, root or exe show an object of
/// `root`, or that refuses the look; a child of this test process that has not exec'd yet shows
/// here as a process whose parent is this one and whose exe is this binary.
fn holding_processes(
    root: &Path,
    objects: &std::collections::HashSet<(u64, u64)>,
    out: &mut String,
) {
    use std::fmt::Write;
    let own = std::process::id();
    let shows = |link: PathBuf| {
        disk::metadata(&link).is_ok_and(|m| objects.contains(&(m.dev(), m.ino())))
            || disk::read_link(&link).is_ok_and(|target| target.starts_with(root))
    };
    let mut found = 0;
    for entry in disk::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let process = entry.path();
        if pid == own || !disk::metadata(&process).is_ok_and(|m| m.uid() == fs::uid()) {
            continue;
        }
        let mut seen = Vec::new();
        let mut denied = Vec::new();
        match disk::read_dir(process.join("fd")) {
            Ok(fds) => {
                for fd in fds.flatten() {
                    if shows(fd.path()) {
                        seen.push(format!("fd {}", fd.file_name().to_string_lossy()));
                    }
                }
            }
            Err(error) => denied.push(format!("fd: {error}")),
        }
        match disk::read_to_string(process.join("maps")) {
            Ok(maps) => {
                let root_text = root.to_string_lossy();
                let mapped = maps.lines().filter(|line| {
                    let inode = line.split_whitespace().nth(4).and_then(|i| i.parse().ok());
                    line.contains(root_text.as_ref())
                        || inode.is_some_and(|i: u64| objects.iter().any(|o| o.1 == i))
                });
                seen.extend(mapped.map(|line| format!("map {line}")));
            }
            Err(error) => denied.push(format!("maps: {error}")),
        }
        for link in ["cwd", "root", "exe"] {
            if shows(process.join(link)) {
                seen.push(link.to_owned());
            }
        }
        if seen.is_empty() && denied.is_empty() {
            continue;
        }
        found += 1;
        let stat = disk::read_to_string(process.join("stat")).unwrap_or_default();
        // comm is parenthesized and may hold spaces, so the fields after it start at its last ')'.
        let (comm, rest) = stat.rsplit_once(')').unwrap_or((&stat, ""));
        let comm = comm.split_once('(').map_or("", |c| c.1);
        let mut rest = rest.split_whitespace();
        let state = rest.next().unwrap_or("?");
        let ppid = rest.next().unwrap_or("?");
        let exe = disk::read_link(process.join("exe"))
            .map_or_else(|e| format!("<{e}>"), |p| p.display().to_string());
        writeln!(
            out,
            "  pid {pid} ppid {ppid}{} comm {comm} state {state} exe {exe}: shows {seen:?} denied {denied:?}",
            if ppid == own.to_string() { " (this test process)" } else { "" }
        )
        .unwrap();
    }
    writeln!(out, "  {found} processes listed").unwrap();
}
