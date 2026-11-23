use super::persistence::{Command, Operation};
use super::*;
fn entry() -> ChildEntry {
    ChildEntry {
        child_surface_id: 100,
        index: 0,
        cwd: None,
        role: None,
        nickname: None,
    }
}
fn loaded(path: &std::path::Path) -> RegistryData {
    let mut r = read_file(path);
    r.path = Some(path.to_owned());
    r
}
#[test]
fn review_counter_max_refuses_without_advancing_or_replacing() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("r.json");
    for parent in [ParentKey::Surface(10), ParentKey::External("e".into())] {
        let mut r = RegistryData::default();
        r.next_index.insert(10, u32::MAX);
        r.external_next_index.insert("e".into(), u32::MAX);
        std::fs::write(&p, serde_json::to_vec(&r).unwrap()).unwrap();
        let old = std::fs::read(&p).unwrap();
        let mut r = loaded(&p);
        assert!(r.reserve_index(&parent).unwrap_err().contains("exhausted"));
        assert_eq!(std::fs::read(&p).unwrap(), old);
        assert_eq!(r.next_index.get(&10), Some(&u32::MAX));
        assert_eq!(r.external_next_index.get("e"), Some(&u32::MAX));
    }
}
#[test]
fn review_stale_numeric_deletion_preserves_a_newer_reserved_counter() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("r.json");
    let mut r = loaded(&p);
    r.reserve_index(&10.into()).unwrap();
    r.register_child(10, entry());
    r.persist(Operation::Merge).unwrap();
    let mut newer = loaded(&p);
    assert_eq!(newer.reserve_index(&10.into()), Ok(1));
    r.unregister_child_by_surface(100);
    r.reconcile_with_live_surfaces(&[10].into_iter().collect());
    r.persist(Operation::Merge).unwrap();
    assert_eq!(loaded(&p).next_index.get(&10), Some(&2));
    assert!(loaded(&p).parent_of_child(100).is_none());
}
#[test]
fn review_external_relation_deletion_vs_stale_owned_field_edit() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("r.json");
    let mut r = loaded(&p);
    r.register_child(10, entry());
    r.persist(Operation::Merge).unwrap();
    r.update_child(&10.into(), 0, |c| c.cwd = Some("new".into()));
    let pending = Command::new(p.clone(), r, Operation::Merge);
    let mut external = loaded(&p);
    external.unregister_child_by_surface(100);
    external.persist(Operation::Merge).unwrap();
    pending.execute().unwrap();
    let final_view = loaded(&p);
    tracing::info!(
        "after external deletion then pending cwd edit: relation_present={}, reverse_present={}",
        !final_view.list_children(&10.into()).is_empty(),
        final_view.parent_of_child(100).is_some()
    );
    assert_eq!(
        !final_view.list_children(&10.into()).is_empty(),
        final_view.parent_of_child(100).is_some(),
        "field-only edit must not resurrect a forward relation without its reverse identity"
    );
}
#[cfg(unix)]
#[test]
fn review_fifo_latest_read_does_not_block() {
    if std::env::var_os("REVIEW_FIFO_PATH").is_none() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("fifo.json");
        std::fs::write(&p, serde_json::to_vec(&RegistryData::default()).unwrap()).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!(
                    "{}::review_fifo_latest_read_does_not_block",
                    module_path!().split_once("::").unwrap().1
                ),
                "--nocapture",
            ])
            .env("REVIEW_FIFO_PATH", &p)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // try_wait 는 끝난 자식만 거두므로 기한 뒤 관측 읽기에서도 부른다.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut status = None;
        if tasty_utils::test_wait::holds_by_every(
            deadline,
            std::time::Duration::from_millis(10),
            |_| {
                status = child.try_wait().unwrap();
                status.is_some()
            },
        ) {
            assert!(
                status.expect("checked above").success(),
                "nonregular target must return error cleanly"
            );
            let output = child.wait_with_output().unwrap();
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("1 passed"),
                "FIFO subprocess must execute its exact test"
            );
            return;
        }
        use std::os::unix::fs::FileTypeExt;
        let is_fifo = std::fs::metadata(&p)
            .map(|m| m.file_type().is_fifo())
            .unwrap_or(false);
        let pid = child.id();
        let wchan = std::fs::read_to_string(format!("/proc/{pid}/wchan")).unwrap_or_default();
        child.kill().unwrap();
        let output = child.wait_with_output().unwrap();
        tracing::info!(
            "owned FIFO child pid={pid}, latest_is_fifo={is_fifo}, wchan={wchan}, rc={:?}, output={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout)
        );
        panic!(
            "valid queued command blocked on latest FIFO for 2 seconds instead of returning failure; exact owned child reaped"
        );
    }
    let p = std::path::PathBuf::from(std::env::var_os("REVIEW_FIFO_PATH").unwrap());
    // Valid initially loaded command, then replace the pathname with an owned FIFO.
    let mut r = loaded(&p);
    r.register_child(10, entry());
    let pending = Command::new(p.clone(), r, Operation::Merge);
    std::fs::remove_file(&p).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&p)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        pending.execute().is_err(),
        "nonregular latest target must be rejected"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn review_actual_command_preserves_acl_xattr_and_symlink() {
    use std::os::unix::fs::symlink;
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("actual.json");
    let p = d.path().join("link.json");
    std::fs::write(
        &target,
        serde_json::to_vec(&RegistryData::default()).unwrap(),
    )
    .unwrap();
    symlink(&target, &p).unwrap();
    let script = r#"import os,struct,sys
p=sys.argv[1]
os.chmod(p,0o600)
a=struct.pack('<I',2)
for t,v,i in [(1,6,0xffffffff),(2,0,os.getuid()+1),(4,0,0xffffffff),(16,0,0xffffffff),(32,0,0xffffffff)]:a+=struct.pack('<HHI',t,v,i)
os.setxattr(p,'system.posix_acl_access',a)
os.setxattr(p,'user.review',b'preserved')
"#;
    assert!(
        std::process::Command::new("python3")
            .args(["-c", script])
            .arg(&p)
            .status()
            .unwrap()
            .success()
    );
    let snapshot = r#"import os,sys
p=sys.argv[1];s=os.stat(p);print(s.st_uid,s.st_gid,s.st_mode,[(k,os.getxattr(p,k).hex()) for k in sorted(os.listxattr(p))])"#;
    let query = || {
        std::process::Command::new("python3")
            .args(["-c", snapshot])
            .arg(&p)
            .output()
            .unwrap()
            .stdout
    };
    let before = query();
    let mut r = loaded(&p);
    r.register_child(10, entry());
    r.persist(Operation::Merge).unwrap();
    assert_eq!(query(), before);
    assert!(std::fs::symlink_metadata(&p).unwrap().is_symlink());
    assert_eq!(loaded(&p).parent_of_child(100), Some(10.into()));
}

#[test]
fn review_explicit_active_report_must_override_latest_idle_even_if_cached_active() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("r.json");
    let mut r = loaded(&p);
    r.register_child(10, entry());
    r.set_idle(100, false);
    r.persist(Operation::Merge).unwrap();
    let mut external = loaded(&p);
    external.set_idle(100, true);
    external.persist(Operation::Merge).unwrap();
    assert_eq!(loaded(&p).state_of(100), "idle");
    // This is a new explicit active report, not an unrelated save of the stale cache.
    r.set_idle(100, false);
    r.persist(Operation::Merge).unwrap();
    tracing::info!(
        "explicit active report committed cache={}, disk={}",
        r.state_of(100),
        loaded(&p).state_of(100)
    );
    assert_eq!(
        r.state_of(100),
        "active",
        "the requested state must not be lost merely because it equals the cached baseline"
    );
    assert_eq!(loaded(&p).state_of(100), "active");
}

#[test]
fn explicit_active_clears_latest_needs_input_and_preserves_external_nickname() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("registry.json");
    let mut cache = loaded(&path);
    cache.register_child(10, entry());
    cache.set_idle(100, false);
    cache.persist(Operation::Merge).unwrap();
    let mut external = loaded(&path);
    external.set_needs_input(100, true);
    external.update_child(&10.into(), 0, |entry| {
        entry.nickname = Some("external".into())
    });
    external.persist(Operation::Merge).unwrap();
    cache.set_idle(100, false);
    cache.persist(Operation::Merge).unwrap();
    let disk = loaded(&path);
    assert_eq!(disk.state_of(100), "active");
    assert_eq!(
        disk.find_child(&10.into(), 0).unwrap().nickname.as_deref(),
        Some("external")
    );
}

#[test]
fn explicit_needs_input_report_does_not_replay_an_old_idle_assignment() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("registry.json");
    let mut cache = loaded(&path);
    cache.register_child(10, entry());
    cache.set_idle(100, false);
    cache.persist(Operation::Merge).unwrap();
    let mut external = loaded(&path);
    external.set_idle(100, true);
    external.persist(Operation::Merge).unwrap();
    cache.set_needs_input(100, false);
    cache.persist(Operation::Merge).unwrap();
    assert_eq!(loaded(&path).state_of(100), "idle");
}

#[cfg(unix)]
#[test]
fn initial_fifo_is_protected_without_reading_contents() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("registry.json");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    // A child deadline keeps a regression from hanging the test process while retaining the
    // real production loader. The latest-read regression already owns the subprocess driver.
    let executable = std::env::current_exe().unwrap();
    if std::env::var_os("INITIAL_REGISTRY_FIFO").is_some() {
        unreachable!();
    }
    let mut child = std::process::Command::new(executable)
        .args([
            "--exact",
            &format!(
                "{}::initial_fifo_child",
                module_path!().split_once("::").unwrap().1
            ),
            "--nocapture",
        ])
        .env("INITIAL_REGISTRY_FIFO", &path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // try_wait 는 끝난 자식만 거두므로 기한 뒤 관측 읽기에서도 부른다.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut status = None;
    if !tasty_utils::test_wait::holds_by_every(
        deadline,
        std::time::Duration::from_millis(10),
        |_| {
            status = child.try_wait().unwrap();
            status.is_some()
        },
    ) {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("initial FIFO read blocked");
    }
    assert!(status.expect("checked above").success());
    let output = child.wait_with_output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "initial FIFO subprocess must execute its exact test"
    );
}

#[cfg(unix)]
#[test]
fn initial_fifo_child() {
    let Some(path) = std::env::var_os("INITIAL_REGISTRY_FIFO") else {
        return;
    };
    assert!(read_file(std::path::Path::new(&path)).load_error.is_some());
}
