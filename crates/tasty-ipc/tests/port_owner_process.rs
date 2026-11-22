//! Gated real process participant fixture; custom harness so the child is not a fake passing test.
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tasty_ipc::port_file::publish_port_file_to;

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || path.exists()),
        "fixture marker timeout: {}",
        path.display()
    );
}

fn writer(root: &Path) {
    let path = root.join("tasty.port");
    let lock = tasty_utils::file_replace::lock(&path).unwrap();
    fs::write(root.join("locked"), b"ready").unwrap();
    wait_for(&root.join("release"));
    drop(lock);
    let latest = publish_port_file_to(4242, Some(&path)).unwrap();
    fs::write(root.join("published"), b"ready").unwrap();
    wait_for(&root.join("cleanup"));
    assert!(latest.cleanup_if_owned().unwrap());
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Err(error) = self.0.kill() {
            tracing::warn!(%error, "owned fixture child kill failed");
        }
        if let Err(error) = self.0.wait() {
            tracing::warn!(%error, "owned fixture child wait failed");
        }
    }
}

fn parent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = root.join("tasty.port");
    let old = publish_port_file_to(4242, Some(&path)).unwrap();
    let log = fs::File::create(root.join("child.log")).unwrap();
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .arg("writer")
            .arg(root)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    wait_for(&root.join("locked"));
    assert!(!old.cleanup_if_owned().unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"4242");
    fs::write(root.join("release"), b"go").unwrap();
    wait_for(&root.join("published"));
    assert!(!old.cleanup_if_owned().unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"4242");
    fs::write(root.join("cleanup"), b"go").unwrap();
    let status = child.0.wait().unwrap();
    assert!(
        status.success(),
        "{}",
        fs::read_to_string(root.join("child.log")).unwrap()
    );
    assert!(!path.exists());
    tracing::info!(
        child_pid = child.0.id(),
        ?status,
        "port owner process scenario passed"
    );
}

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("writer")) {
        writer(Path::new(
            &std::env::args_os().nth(2).expect("owned fixture root"),
        ));
    } else {
        parent();
    }
}
