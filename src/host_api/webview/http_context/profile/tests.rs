use super::*;
use std::{
    fs as disk,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    sync::Arc,
};
fn namespace() -> tempfile::TempDir {
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
fn malformed_foreign_record_inode_start_boot_and_record_symlink_are_retained() {
    for kind in 0..7 {
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
                value["owner"]["boot"] = serde_json::json!("00000000-0000-0000-0000-000000000000");
                disk::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            5 => {
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

struct OwnedChild {
    child: Option<std::process::Child>,
    receipt: tempfile::NamedTempFile,
    root: PathBuf,
    pid: u32,
}
impl OwnedChild {
    fn start(parent: &Path, restrictive_umask: bool) -> Self {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let receipt = tempfile::NamedTempFile::new().unwrap();
        disk::set_permissions(receipt.path(), disk::Permissions::from_mode(0o600)).unwrap();
        let module = module_path!().split_once("::").unwrap().1;
        let entry = format!("{module}::child_writer");
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                &entry,
                "--nocapture",
                "--test-threads=1",
            ])
            .env("TASTY_PROFILE_PROOF_PARENT", parent)
            .env("TASTY_PROFILE_PROOF_RECEIPT", receipt.path())
            .env(
                "TASTY_PROFILE_PROOF_UMASK",
                if restrictive_umask { "1" } else { "0" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut owned = Self {
            child: Some(child),
            receipt,
            root: PathBuf::new(),
            pid,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let bytes = disk::read(owned.receipt.path()).unwrap();
            if !bytes.is_empty() {
                use std::os::unix::ffi::OsStringExt;
                owned.root = std::ffi::OsString::from_vec(bytes).into();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned child profile preparation timed out"
            );
            assert!(
                owned.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "owned child exited before receipt"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        owned
    }
    fn finish(&mut self) {
        let mut child = self.child.take().unwrap();
        drop(child.stdin.take());
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "owned child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!Path::new(&format!("/proc/{}", self.pid)).exists());
        tracing::info!(
            "owned-profile-child pid={} normal_exit=0 absent=true",
            self.pid
        );
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.stdin.take());
            match child.wait() {
                Ok(status) => assert!(status.success()),
                Err(e) => panic!("owned child wait failed: {e}"),
            }
        }
    }
}
#[test]
#[ignore = "owned subprocess entry, invoked only with isolated proof paths"]
fn child_writer() {
    use std::io::Read;
    let parent = std::env::var_os("TASTY_PROFILE_PROOF_PARENT").expect("owned test parent");
    let receipt = std::env::var_os("TASTY_PROFILE_PROOF_RECEIPT").expect("owned test receipt");
    if std::env::var_os("TASTY_PROFILE_PROOF_UMASK").as_deref() == Some(std::ffi::OsStr::new("1")) {
        // SAFETY: this is the dedicated owned child process; it changes no parent's umask.
        unsafe { libc::umask(0o777) };
    }
    let profile = admitted(|| PreparedProfile::create(Path::new(&parent), 1)).unwrap();
    let lease = profile.into_native();
    disk::write(receipt, lease.root().as_os_str().as_bytes()).unwrap();
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
        rec.owner.state().unwrap(),
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
        rec.owner.state().unwrap(),
        process::OwnerState::Dead
    ));
    let tree =
        reclaim::Tree::inspect(ns.dir.open_dir(child.root.file_name().unwrap()).unwrap()).unwrap();
    let mut objects = std::collections::HashSet::new();
    tree.observations(&mut objects);
    drop(tree);
    fd_and_mapping_consumers(&child.root, &objects, file);
    public_dead_sweep(&child.root, parent.path(), &objects);
}
fn public_dead_sweep(root: &Path, parent: &Path, objects: &std::collections::HashSet<(u64, u64)>) {
    let audit = process::consumers(root, objects);
    let r = reclaim_positive(parent, 1);
    assert_eq!(r.inspected, 1);
    assert_eq!(r.kept_live, 0);
    assert_eq!(r.failed, 0);
    match audit {
        Ok(false) => assert_eq!(r.removed, 1),
        Ok(true) | Err(_) => {
            assert_eq!(r.removed, 0);
            assert_eq!(r.kept_uncertain, 1);
        }
    }
    tracing::info!(
        "public-dead-child-sweep removed={} uncertain={} failed={}; global_visibility_complete={}",
        r.removed,
        r.kept_uncertain,
        r.failed,
        matches!(audit, Ok(false))
    );
}

fn fd_and_mapping_consumers(
    root: &Path,
    objects: &std::collections::HashSet<(u64, u64)>,
    file: std::fs::File,
) {
    use std::os::fd::AsRawFd;
    let our_proc = PathBuf::from(format!("/proc/{}", std::process::id()));
    let audit =
        || process::audit_process(&our_proc, root, objects, &mut 65_536, &process::stat_again);
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

fn os_error(code: i32) -> std::io::Error {
    std::io::Error::from_raw_os_error(code)
}
#[test]
fn only_a_subject_observed_gone_turns_a_denial_into_gone() {
    let denied = os_error(libc::EACCES);
    assert!(!process::gone_after(&denied, || Ok(())));
    assert!(process::gone_after(&denied, || Err(os_error(libc::ENOENT))));
    assert!(process::gone_after(&denied, || Err(os_error(libc::ESRCH))));
    assert!(!process::gone_after(&denied, || Err(os_error(libc::EIO))));
    assert!(!process::gone_after(&denied, || Err(os_error(
        libc::EACCES
    ))));
    // Errors that already decide their meaning are not observed again.
    let unexpected = || panic!("no second observation");
    assert!(process::gone_after(&os_error(libc::ENOENT), unexpected));
    assert!(process::gone_after(&os_error(libc::ESRCH), unexpected));
    assert!(!process::gone_after(&os_error(libc::EIO), unexpected));
}
/// The audit answer for a dead owner's root decides between removing it and retaining it.
#[test]
fn a_dead_root_is_removed_only_when_the_denied_subject_was_observed_gone() {
    let sweep = |reobserved: fn() -> std::io::Result<()>| {
        let parent = namespace();
        let mut child = OwnedChild::start(parent.path(), false);
        child.finish();
        let audit = |_: &Path, _: &std::collections::HashSet<(u64, u64)>| {
            let denied = os_error(libc::EACCES);
            if process::gone_after(&denied, reobserved) {
                Ok(false)
            } else {
                Err(denied)
            }
        };
        let report = admitted(|| reclaim::run_with_audit(parent.path(), &audit)).unwrap();
        (report.removed, report.kept_uncertain, child.root.exists())
    };
    assert_eq!(sweep(|| Err(os_error(libc::ENOENT))), (1, 0, false));
    assert_eq!(sweep(|| Ok(())), (0, 1, true));
    assert_eq!(sweep(|| Err(os_error(libc::EIO))), (0, 1, true));
}
/// A real live process of this UID whose tasks and maps deny inspection.
struct UndumpableChild {
    child: std::process::Child,
    // Stays open until the child has exited: its harness still reports to this pipe.
    _stdout: std::io::BufReader<std::process::ChildStdout>,
    process: PathBuf,
}
impl UndumpableChild {
    fn start() -> Self {
        use std::{
            io::{BufRead, Read},
            process::{Command, Stdio},
        };
        let module = module_path!().split_once("::").unwrap().1;
        let entry = format!("{module}::undumpable_child");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                &entry,
                "--nocapture",
                "--test-threads=1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
        assert!(
            stdout
                .by_ref()
                .lines()
                .map_while(Result::ok)
                .any(|line| line.contains("undumpable-ready")),
            "owned undumpable child never reported ready"
        );
        let process = PathBuf::from(format!("/proc/{}", child.id()));
        Self {
            child,
            _stdout: stdout,
            process,
        }
    }
    /// A process table of just this child, so that no other process decides the walk.
    fn table(&self) -> tempfile::TempDir {
        let table = namespace();
        let pid = self.process.file_name().unwrap();
        std::os::unix::fs::symlink(&self.process, table.path().join(pid)).unwrap();
        table
    }
    /// Reports whether it was still alive when asked to finish, so what was seen was a live denial.
    fn finish(mut self) -> bool {
        let alive = self.child.try_wait().unwrap().is_none();
        drop(self.child.stdin.take());
        assert!(self.child.wait().unwrap().success());
        alive
    }
}
const NO_ROOT: &str = "/nonexistent-tasty-profile-root";
/// A real live task of this UID that denies inspection: the audit must not complete past it.
#[test]
fn a_live_task_that_denies_inspection_keeps_the_audit_incomplete() {
    let child = UndumpableChild::start();
    let process = child.process.clone();
    let root = Path::new(NO_ROOT);
    let objects = Default::default();
    let real = &process::stat_again;
    // The tasks alone: maps of this process denies too and would hide a task taken as gone.
    let tasks = process::audit_tasks(&process, root, &objects, &mut 65_536, real);
    let whole = process::audit_process(&process, root, &objects, &mut 65_536, real);
    let denied = os_error(libc::EACCES);
    let live_subject_gone = process::gone(&denied, &process, real);
    assert!(
        child.finish(),
        "the denied task must be live during the audit"
    );
    for audit in [tasks, whole] {
        let error = audit.expect_err("a denied live task leaves the audit incomplete");
        assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    }
    assert!(!live_subject_gone, "a live subject stays denied");
    assert!(
        process::gone(&denied, &process, real),
        "the same subject counts as gone once it has been reaped"
    );
}
/// The product audit, whose second observation decides whether a root can be deleted, must not
/// report completion while a denied process lives. The real table can answer with another process's
/// denial or the observation bound, so only the table of this child pins the denial itself.
#[test]
fn the_product_audit_stays_incomplete_while_a_denied_process_lives() {
    let child = UndumpableChild::start();
    let table = child.table();
    let root = Path::new(NO_ROOT);
    let objects = Default::default();
    let alone = process::consumers_in(table.path(), root, &objects);
    let everything = process::consumers(root, &objects);
    assert!(
        child.finish(),
        "the denied task must be live during the audit"
    );
    let error = alone.expect_err("completed past a live denial");
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    assert!(everything.is_err(), "completed: {everything:?}");
}
/// Both call sites of the classification, on real denials: the injected answer stands in for the
/// exit race, which no test can make happen on demand between a denial and its second observation.
#[test]
fn each_audit_call_site_skips_a_denied_subject_only_when_observed_gone() {
    type Answer = fn() -> std::io::Result<()>;
    let exists: Answer = || Ok(());
    let vanished: Answer = || Err(os_error(libc::ENOENT));
    let answers = |task: Answer, process: Answer| {
        move |subject: &Path| {
            if subject
                .parent()
                .is_some_and(|parent| parent.ends_with("task"))
            {
                task()
            } else {
                process()
            }
        }
    };
    let child = UndumpableChild::start();
    let table = child.table();
    let root = Path::new(NO_ROOT);
    let objects = Default::default();
    let tasks = |reobserve: process::Reobserve<'_>| {
        process::audit_tasks(&child.process, root, &objects, &mut 65_536, reobserve)
    };
    let consumers = |reobserve: process::Reobserve<'_>| {
        process::consumers_under(table.path(), root, &objects, reobserve)
    };
    let results = [
        ("tasks: task gone", tasks(&answers(vanished, exists)), None),
        (
            "tasks: task exists",
            tasks(&answers(exists, exists)),
            Some(libc::EACCES),
        ),
        (
            "tasks: real",
            tasks(&process::stat_again),
            Some(libc::EACCES),
        ),
        // Tasks taken as gone, so the denied maps reaches the consumers call site.
        (
            "consumers: all gone",
            consumers(&answers(vanished, vanished)),
            None,
        ),
        // A task denial carried up to the consumers call site, whose own subject is gone.
        (
            "consumers: process gone",
            consumers(&answers(exists, vanished)),
            None,
        ),
        // The consumers call site must not skip a process that is still there.
        (
            "consumers: process exists",
            consumers(&answers(vanished, exists)),
            Some(libc::EACCES),
        ),
        (
            "consumers: real",
            consumers(&process::stat_again),
            Some(libc::EACCES),
        ),
    ];
    assert!(
        child.finish(),
        "the denied task must be live during the audit"
    );
    for (case, result, denied) in results {
        match denied {
            None => assert!(matches!(result, Ok(false)), "{case}: {result:?}"),
            Some(code) => assert_eq!(
                result.as_ref().map_err(|error| error.raw_os_error()),
                Err(Some(code)),
                "{case}"
            ),
        }
    }
}
#[test]
#[ignore = "owned subprocess entry, invoked only by its parent test"]
fn undumpable_child() {
    use std::io::{Read, Write};
    // SAFETY: this is the dedicated owned child process; prctl touches no Rust memory.
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) }, 0);
    // The handshake with the parent over its pipe, not a log line.
    let mut stdout = std::io::stdout();
    writeln!(stdout, "undumpable-ready").unwrap();
    stdout.flush().unwrap();
    let mut byte = [0];
    let received = std::io::stdin().read(&mut byte).unwrap();
    assert_eq!(received, 0, "owned parent closes stdin for normal exit");
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
// the same bounded admission and then judge the first other result. Production does not
// retry the unpublished discard. The tests apply the same retry to it only because Busy
// returns before anything is removed.
fn admitted<T>(mut operation: impl FnMut() -> Result<T, ProfileError>) -> Result<T, ProfileError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
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
fn reclaim_positive(parent: &Path, max: usize) -> CleanupReport {
    admitted(|| reclaim_dead(parent, max))
        .unwrap_or_else(|error| panic!("positive private profile sweep: {error}"))
}
