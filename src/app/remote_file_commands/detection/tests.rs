//! Actual ordered file queue, including preparation behind an older admitted status patch.
use super::*;
use crate::app::file_writer::{
    self,
    settle::{HANG_LIMIT, settle},
};
use std::time::Instant;
use tasty_remote_profiles::RemoteProfile;
use tasty_utils::{batch_writer, file_replace};
struct Writer(file_writer::FileWriterParts);
impl Writer {
    fn start() -> Self {
        Self(
            file_writer::start(
                Arc::new(tasty_settings::FileSettingsStorage),
                batch_writer::Limits::default(),
                Arc::new(batch_writer::SystemClock),
                Arc::new(|| {}),
            )
            .unwrap(),
        )
    }
    fn sink(&self) -> Submit {
        sink(Some(self.0.writer.handle())).unwrap()
    }
    fn wait(&mut self, receipt: &mut dyn Receipt) -> Result<Value, Failure> {
        settle(&self.0.writer.handle());
        self.0.poll();
        receipt
            .poll()
            .expect("the file writer finished without completing this detection receipt")
    }
    fn finish(mut self) {
        let deadline = Instant::now() + HANG_LIMIT;
        while self.0.writer.poll_shutdown() != 0 {
            assert!(
                Instant::now() < deadline,
                "ordered detection writer did not drain"
            );
            std::thread::yield_now();
        }
    }
}
fn accepted(sink: &Submit, request: Request) -> Box<dyn Receipt> {
    match sink(request) {
        Ok(receipt) => receipt,
        Err(error) => panic!("admission: {}", error.message),
    }
}
fn success(result: Result<Value, Failure>) -> Value {
    match result {
        Ok(value) => value,
        Err(error) => panic!("completion: {}", error.message),
    }
}
fn original() -> RemoteProfile {
    RemoteProfile::new("owned", "ssh")
        .with_field("host", "owned.invalid")
        .with_field("shell", "auto")
}
#[test]
fn fresh_preparation_observes_earlier_accepted_status_before_next_expected_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    let original = original();
    RemoteFileStore::at(&root)
        .profile(ProfileCommand::Insert(original.clone()), None)
        .unwrap();
    let held = file_replace::lock(&root.join("remote-profiles.toml")).unwrap();
    let mut writer = Writer::start();
    let sink = writer.sink();
    let mut old = accepted(
        &sink,
        Request::Save {
            root: root.clone(),
            original: original.clone(),
            failed: true,
            cancel: tasty_ssh::SshCancel::new(),
        },
    );
    let mut fresh = accepted(
        &sink,
        Request::Load {
            root: root.clone(),
            name: original.name.clone(),
            expected: Some(original),
        },
    );
    assert!(old.poll().is_none());
    assert!(fresh.poll().is_none());
    drop(held);
    let Value::Loaded(input) = success(writer.wait(fresh.as_mut())) else {
        panic!("fresh input required");
    };
    assert_eq!(
        input
            .original
            .fields
            .get("detect_failed")
            .and_then(|value| value.as_str()),
        Some("true")
    );
    assert!(matches!(success(writer.wait(old.as_mut())), Value::Saved));
    assert!(old.poll().is_none());
    let mut next = accepted(
        &sink,
        Request::Save {
            root: input.root,
            original: input.original,
            failed: false,
            cancel: tasty_ssh::SshCancel::new(),
        },
    );
    assert!(matches!(success(writer.wait(next.as_mut())), Value::Saved));
    let current: RemoteProfiles =
        read(&root, "remote-profiles.toml").unwrap_or_else(|error| panic!("{}", error.message));
    assert!(
        !current
            .get("owned")
            .unwrap()
            .fields
            .contains_key("detect_failed")
    );
    let counts = writer.0.stats.snapshot();
    assert_eq!(counts.operations.replace.succeeded, 2);
    assert_eq!(counts.commands.known_success, 3);
    assert_eq!(counts.commands.unclassified, 0);
    writer.finish();
}
#[test]
fn edited_profile_refuses_old_status_patch_without_a_physical_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    let original = original();
    let store = RemoteFileStore::at(&root);
    store
        .profile(ProfileCommand::Insert(original.clone()), None)
        .unwrap();
    let path = root.join("remote-profiles.toml");
    let held = file_replace::lock(&path).unwrap();
    let mut writer = Writer::start();
    let sink = writer.sink();
    let mut old = accepted(
        &sink,
        Request::Save {
            root: root.clone(),
            original,
            failed: true,
            cancel: tasty_ssh::SshCancel::new(),
        },
    );
    assert!(old.poll().is_none());
    // Model an uncooperative external editor while the participating writer is still held.
    let latest = "[[profile]]\nname='owned'\nkind='ssh'\n[profile.fields]\nhost='latest.invalid'\nshell='auto'\n";
    std::fs::write(&path, latest).unwrap();
    drop(held);
    let failure = match writer.wait(old.as_mut()) {
        Err(error) => error,
        Ok(_) => panic!("changed expected profile must fail"),
    };
    assert!(!failure.unknown);
    assert!(failure.message.contains("changed"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), latest);
    let counts = writer.0.stats.snapshot();
    assert_eq!(counts.operations.replace.attempts, 0);
    assert_eq!(counts.commands.known_failure, 1);
    assert!(old.poll().is_none());
    writer.finish();
}
#[test]
fn malformed_preparation_returns_sanitized_failure_and_preserves_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("remote-profiles.toml");
    let original = "profile=[{name='synthetic-detection-private-marker'";
    std::fs::write(&path, original).unwrap();
    let mut writer = Writer::start();
    let mut receipt = accepted(
        &writer.sink(),
        Request::Load {
            root: dir.path().to_owned(),
            name: "owned".into(),
            expected: None,
        },
    );
    let failure = match writer.wait(receipt.as_mut()) {
        Err(error) => error,
        Ok(_) => panic!("malformed input must fail"),
    };
    assert!(!failure.unknown);
    assert!(failure.message.contains("remote-profiles.toml"));
    assert!(
        !failure
            .message
            .contains("synthetic-detection-private-marker")
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    assert_eq!(writer.0.stats.snapshot().operations.replace.attempts, 0);
    writer.finish();
}
