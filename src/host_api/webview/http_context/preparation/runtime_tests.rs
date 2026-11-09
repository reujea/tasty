//! Namespace location: the user runtime directory when XDG_RUNTIME_DIR is set, the app home otherwise.
use super::tests::{joined, woken};
use super::*;
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc},
    thread,
};

fn dir(mode: u32) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(mode)).unwrap();
    d
}

fn runtime(runtime: &Path, app_home: &Path) -> NamespaceRoot {
    NamespaceRoot::select(Some(runtime.as_os_str()), app_home.to_path_buf())
}

fn home_leaf(app_home: &Path) -> PathBuf {
    app_home.join("native-http-profiles")
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let mut v: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    v.sort();
    v
}

#[test]
fn an_unset_variable_keeps_the_app_home_and_a_set_one_selects_the_runtime_directory() {
    let home = PathBuf::from("/h");
    assert_eq!(
        NamespaceRoot::select(None, home.clone()),
        NamespaceRoot::AppHome(home.clone())
    );
    assert_eq!(
        NamespaceRoot::select(Some(OsStr::new("")), home.clone()),
        NamespaceRoot::Runtime {
            runtime: PathBuf::new(),
            app_home: home.clone()
        }
    );
    let leaf = namespace::runtime_leaf(&home);
    let name = leaf.to_str().unwrap();
    let hex = name.strip_prefix("tasty-native-http-").unwrap();
    assert_eq!(hex.len(), 64);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert_eq!(leaf, namespace::runtime_leaf(Path::new("/h")));
    assert_ne!(leaf, namespace::runtime_leaf(Path::new("/h/")));
    assert_ne!(leaf, namespace::runtime_leaf(Path::new("/h2")));
}

#[test]
fn a_private_runtime_directory_holds_a_per_home_namespace_and_the_app_home_is_untouched() {
    let rt = dir(0o700);
    // A per-user-group umask 002 home: the runtime location never walks it.
    let home = dir(0o775);
    let a = namespace::prepare(&runtime(rt.path(), home.path())).unwrap();
    assert_eq!(a, rt.path().join(namespace::runtime_leaf(home.path())));
    assert_eq!(fs::metadata(&a).unwrap().mode() & 0o7777, 0o700);
    let other = dir(0o700);
    let b = namespace::prepare(&runtime(rt.path(), other.path())).unwrap();
    assert_ne!(a, b);
    assert_eq!(entries(rt.path()), {
        let mut v = vec![a.clone(), b];
        v.sort();
        v
    });
    assert!(entries(home.path()).is_empty());
    assert_eq!(fs::metadata(home.path()).unwrap().mode() & 0o7777, 0o775);
    assert_eq!(
        namespace::prepare(&runtime(rt.path(), home.path())).unwrap(),
        a
    );
}

#[test]
fn an_unusable_runtime_value_fails_without_trying_the_app_home() {
    let home = dir(0o700);
    let missing = dir(0o700);
    let gone = missing.path().join("gone");
    let target = dir(0o700);
    let link = missing.path().join("link");
    std::os::unix::fs::symlink(target.path(), &link).unwrap();
    let group_readable = dir(0o750);
    let group_writable = dir(0o770);
    let other_writable = dir(0o777);
    let cases: Vec<(&str, PathBuf)> = vec![
        ("empty", PathBuf::new()),
        ("relative", PathBuf::from("run/user")),
        ("missing", gone.clone()),
        ("symlink", link.clone()),
        ("0750", group_readable.path().to_path_buf()),
        ("0770", group_writable.path().to_path_buf()),
        ("0777", other_writable.path().to_path_buf()),
    ];
    for (name, rt) in cases {
        let before = fs::symlink_metadata(&rt).ok().map(|m| m.mode());
        assert!(
            namespace::prepare(&runtime(&rt, home.path())).is_err(),
            "{name}"
        );
        assert_eq!(
            fs::symlink_metadata(&rt).ok().map(|m| m.mode()),
            before,
            "{name}"
        );
        assert!(!home_leaf(home.path()).exists(), "{name}");
    }
    assert!(!gone.exists());
    assert!(entries(target.path()).is_empty());
    for d in [&group_readable, &group_writable, &other_writable] {
        assert!(entries(d.path()).is_empty());
    }
}

#[test]
fn only_a_strictly_valid_old_namespace_is_offered_for_the_dead_owner_sweep() {
    let rt = dir(0o700);
    let home = dir(0o700);
    assert_eq!(runtime(rt.path(), home.path()).legacy(), None);
    fs::create_dir(home_leaf(home.path())).unwrap();
    fs::set_permissions(home_leaf(home.path()), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        runtime(rt.path(), home.path()).legacy(),
        Some(home_leaf(home.path()))
    );
    assert_eq!(
        NamespaceRoot::AppHome(home.path().to_path_buf()).legacy(),
        None
    );
    fs::set_permissions(home_leaf(home.path()), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(runtime(rt.path(), home.path()).legacy(), None);
    fs::set_permissions(home_leaf(home.path()), fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o775)).unwrap();
    assert_eq!(runtime(rt.path(), home.path()).legacy(), None);
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(home_leaf(home.path())).unwrap();
    let outside = dir(0o700);
    std::os::unix::fs::symlink(outside.path(), home_leaf(home.path())).unwrap();
    assert_eq!(runtime(rt.path(), home.path()).legacy(), None);
}

fn prepare(root: NamespaceRoot) -> (Preparation, thread::JoinHandle<()>, mpsc::Receiver<()>) {
    let (wake, rx) = mpsc::channel();
    let current = Arc::new(AtomicBool::new(true));
    let (receipt, work) = Preparation::new(
        root,
        4,
        MAX_CLAIM_WAIT,
        Arc::new(move || current.load(Ordering::Acquire)),
        Arc::new(move || {
            if wake.send(()).is_err() {
                tracing::debug!("owned runtime fixture receiver retired");
            }
        }),
    );
    (receipt, thread::spawn(move || work.run()), rx)
}

/// The worker creates the profile under the runtime namespace and keeps a live root in the old
/// namespace: the sweep deletes only what the dead-owner rules prove dead.
#[test]
fn the_worker_prepares_in_the_runtime_namespace_and_keeps_live_old_roots() {
    let rt = dir(0o700);
    let home = dir(0o700);
    let (old_receipt, old_worker, old_wake) = prepare(home.path().to_path_buf().into());
    woken(&old_wake, &old_worker);
    let old = old_receipt.poll().unwrap().unwrap().into_native();
    joined(old_worker);
    let old_roots = entries(&home_leaf(home.path()));

    let (receipt, worker, wake) = prepare(runtime(rt.path(), home.path()));
    woken(&wake, &worker);
    let profile = receipt.poll().unwrap().unwrap_or_else(|e| panic!("{e}"));
    let namespace = rt.path().join(namespace::runtime_leaf(home.path()));
    assert_eq!(profile.root().parent(), Some(namespace.as_path()));
    let lease = profile.into_native();
    joined(worker);
    assert!(lease.data_dir().starts_with(&namespace));
    assert_eq!(entries(&home_leaf(home.path())), old_roots);
    assert!(old.root().exists());
}

#[test]
fn a_refused_runtime_directory_fails_the_preparation_terminally() {
    let rt = dir(0o770);
    let home = dir(0o700);
    let (receipt, worker, wake) = prepare(runtime(rt.path(), home.path()));
    woken(&wake, &worker);
    joined(worker);
    assert!(matches!(
        receipt.poll(),
        Some(Err(PrepareError::Storage(ProfileError::Ownership)))
    ));
    assert!(!home_leaf(home.path()).exists());
    assert!(entries(rt.path()).is_empty());
}
