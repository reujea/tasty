//! Actual worker preparation, cancellation receipts and namespace effects, without GTK input.
use super::*;
use crate::host_api::webview::http_endpoint::runtime::tests::HANG_LIMIT;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
pub(super) fn private_home() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
/// Waits for the worker's wake. Slow storage has no deadline here: the worker wakes before it ends,
/// so its end is read first, and an ended worker without a wake has lost it at once. Only work that
/// never ends reaches `HANG_LIMIT`, and that says `hung`.
pub(super) fn woken(wake: &mpsc::Receiver<()>, worker: &JoinHandle<()>) {
    let deadline = Instant::now() + HANG_LIMIT;
    loop {
        let ended = worker.is_finished();
        match wake.recv_timeout(Duration::from_millis(1)) {
            Ok(()) => return,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!("profile worker: lost, its wake is gone")
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        assert!(!ended, "profile worker: lost, it ended without a wake");
        assert!(
            Instant::now() < deadline,
            "profile worker: hung, no wake within {HANG_LIMIT:?}"
        );
    }
}
/// Joins a worker that must end, failing with `hung` instead of stopping the whole test run.
pub(super) fn joined(worker: JoinHandle<()>) {
    let deadline = Instant::now() + HANG_LIMIT;
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "profile worker: hung, not finished within {HANG_LIMIT:?}"
        );
        thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap();
}
pub(super) fn roots(home: &Path) -> Vec<PathBuf> {
    let namespace = home.join("native-http-profiles");
    if !namespace.exists() {
        return vec![];
    }
    fs::read_dir(namespace)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap() != ".profile-lock")
        .collect()
}
pub(super) fn request(
    home: &Path,
    current: Arc<AtomicBool>,
    timeout: Duration,
) -> (Preparation, ProfileWork, mpsc::Receiver<()>) {
    let (wake, rx) = mpsc::channel();
    let current = Arc::new(move || current.load(Ordering::Acquire));
    let (receipt, work) = Preparation::new(
        home.to_owned(),
        4,
        timeout,
        current,
        Arc::new(move || {
            if wake.send(()).is_err() {
                tracing::debug!("owned profile fixture receiver retired");
            }
        }),
    );
    (receipt, work, rx)
}
#[test]
fn owned_worker_claim_is_memory_only_and_exposed_paths_remain_private() {
    let home = private_home();
    let (receipt, work, wake) =
        request(home.path(), Arc::new(AtomicBool::new(true)), MAX_CLAIM_WAIT);
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let profile = receipt
        .poll()
        .unwrap()
        .unwrap_or_else(|e| panic!("profile: {e}"));
    let paths = (
        profile.root().to_owned(),
        profile.data_dir().to_owned(),
        profile.cache_dir().to_owned(),
    );
    let lease = profile.into_native();
    let clone = lease.clone();
    assert_eq!(lease.root(), paths.0);
    assert_eq!(lease.data_dir(), paths.1);
    assert_eq!(clone.cache_dir(), paths.2);
    drop(lease);
    drop(clone);
    drop(receipt);
    joined(worker);
    assert_eq!(roots(home.path()), vec![paths.0.clone()]);
    for p in [&paths.0, &paths.1, &paths.2] {
        let m = fs::metadata(p).unwrap();
        assert_eq!(m.mode() & 0o7777, 0o700);
    }
    assert_eq!(
        fs::metadata(paths.0.join("owner.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o600
    );
}
#[test]
fn dropped_ready_receipt_returns_unpublished_cleanup_to_the_same_worker() {
    let home = private_home();
    let (receipt, work, wake) =
        request(home.path(), Arc::new(AtomicBool::new(true)), MAX_CLAIM_WAIT);
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    assert_eq!(roots(home.path()).len(), 1);
    drop(receipt);
    joined(worker);
    assert!(roots(home.path()).is_empty());
}
#[test]
fn stale_native_poll_does_not_take_profile_and_worker_discards_it() {
    let home = private_home();
    let current = Arc::new(AtomicBool::new(true));
    let (receipt, work, wake) = request(home.path(), current.clone(), MAX_CLAIM_WAIT);
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    assert_eq!(roots(home.path()).len(), 1);
    current.store(false, Ordering::Release);
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Retired))));
    joined(worker);
    assert!(roots(home.path()).is_empty());
}
#[test]
fn retired_or_unrun_work_never_creates_a_namespace_or_leaks_receiving() {
    let home = private_home();
    let (receipt, work, _wake) = request(
        home.path(),
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(2),
    );
    let worker = thread::spawn(move || work.run());
    joined(worker);
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Retired))));
    assert!(!home.path().join("native-http-profiles").exists());
    let (receipt, work, _wake) = request(
        home.path(),
        Arc::new(AtomicBool::new(true)),
        Duration::from_secs(2),
    );
    drop(work);
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Worker))));
    assert!(!home.path().join("native-http-profiles").exists());
    let (receipt, work, _wake) = request(
        home.path(),
        Arc::new(AtomicBool::new(true)),
        Duration::from_secs(2),
    );
    drop(receipt);
    let worker = thread::spawn(move || work.run());
    joined(worker);
    assert!(
        !home.path().join("native-http-profiles").exists(),
        "cancel before execution must avoid namespace I/O"
    );
}
/// 20 ms is the claim deadline this test expects to expire, and also the namespace admission
/// budget. A lock contended past it, by a parallel test's child between fork and exec, ends the
/// attempt as `AdmissionTimeout` before any claim exists: that says nothing about the claim, so the
/// attempt starts over in a new home. Every other answer is judged, and no attempt may leave a root.
#[test]
fn abandoned_native_claim_times_out_and_cleans_only_unpublished_root() {
    let deadline = Instant::now() + HANG_LIMIT;
    loop {
        let home = private_home();
        let (receipt, work, wake) = request(
            home.path(),
            Arc::new(AtomicBool::new(true)),
            Duration::from_millis(20),
        );
        let worker = thread::spawn(move || work.run());
        woken(&wake, &worker);
        joined(worker);
        let answer = receipt.poll();
        assert!(roots(home.path()).is_empty());
        match answer {
            Some(Err(PrepareError::Timeout)) => return,
            Some(Err(PrepareError::AdmissionTimeout)) => assert!(
                Instant::now() < deadline,
                "no attempt reached the claim within {HANG_LIMIT:?}"
            ),
            other => panic!(
                "abandoned claim answered {:?}",
                other.map(|answer| answer.map(|_| "a profile"))
            ),
        }
    }
}
#[test]
fn namespace_refuses_symlink_and_unsafe_existing_leaf_without_chmod() {
    assert!(
        namespace::prepare(&NamespaceRoot::AppHome(
            Path::new("relative-native-home").to_path_buf()
        ))
        .is_err()
    );
    let home = private_home();
    let outside = private_home();
    let sentinel = outside.path().join("sentinel");
    fs::write(&sentinel, b"outside").unwrap();
    let leaf = home.path().join("native-http-profiles");
    std::os::unix::fs::symlink(outside.path(), &leaf).unwrap();
    assert!(namespace::prepare(&NamespaceRoot::AppHome(home.path().to_path_buf())).is_err());
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside");
    fs::remove_file(&leaf).unwrap();
    fs::create_dir(&leaf).unwrap();
    fs::set_permissions(&leaf, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(namespace::prepare(&NamespaceRoot::AppHome(home.path().to_path_buf())).is_err());
    assert_eq!(fs::metadata(&leaf).unwrap().mode() & 0o7777, 0o755);
}

#[test]
fn private_namespace_creation_honors_restrictive_umask_in_owned_child() {
    let home = private_home();
    let module = module_path!().split_once("::").unwrap().1;
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            format!("{module}::namespace_umask_child"),
            "--exact".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ])
        .env("TASTY_PROFILE_PREP_CHILD_HOME", home.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "owned child {pid} failed: {}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    let namespace = home.path().join("native-http-profiles");
    assert_eq!(fs::metadata(namespace).unwrap().mode() & 0o7777, 0o700);
}
#[test]
#[ignore = "owned subprocess entry isolates process-wide umask from parallel tests"]
fn namespace_umask_child() {
    let home =
        PathBuf::from(std::env::var_os("TASTY_PROFILE_PREP_CHILD_HOME").expect("owned child home"));
    // SAFETY: this is a separately spawned owned test process; no unrelated thread shares its umask.
    unsafe {
        libc::umask(0o777);
    }
    let (receipt, work, wake) = request(&home, Arc::new(AtomicBool::new(true)), MAX_CLAIM_WAIT);
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let profile = receipt
        .poll()
        .unwrap()
        .unwrap_or_else(|error| panic!("profile under restrictive umask: {error}"));
    let namespace = profile.root().parent().unwrap().to_owned();
    let lease = profile.into_native();
    for path in [
        lease.root(),
        lease.data_dir(),
        lease.cache_dir(),
        namespace.as_path(),
    ] {
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o7777, 0o700);
    }
    drop(lease);
    drop(receipt);
    joined(worker);
}
