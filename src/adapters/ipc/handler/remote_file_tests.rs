use crate::adapters::ipc::handler::{
    deferred::{MemoryReplies, ReplyTo},
    passkey, remote_profile,
};
use crate::app::{
    file_writer::settle::{HANG_LIMIT, settle},
    remote_file_commands::Failure,
    services::AppServices,
};
use serde_json::json;
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_utils::{batch_writer, file_replace};

type Core = AppServices;
fn core() -> Core {
    let writer = crate::app::file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .with_file_writer(writer)
        .build()
        .unwrap()
}
fn park(core: &Core, placeholder: JsonRpcResponse) -> mpsc::Receiver<JsonRpcResponse> {
    assert!(placeholder.error.is_some());
    let pending = core
        .memory_replies()
        .take_parked()
        .expect("original caller parked");
    let (tx, rx) = mpsc::sync_channel(2);
    core.defer_reply(pending, ReplyTo::Ipc(tx));
    rx
}
fn receive(core: &mut Core, rx: &mpsc::Receiver<JsonRpcResponse>) -> JsonRpcResponse {
    settle(&core.file_command_queue().unwrap());
    core.poll_file_writer();
    assert!(MemoryReplies::poll(core).is_empty());
    rx.try_recv()
        .expect("the file writer finished without answering the original remote caller")
}
/// The test holds the file lock, so the worker cannot finish the parked request however long this
/// waits: load cannot fail this check, and a worker that ignored the lock is caught finishing.
fn assert_blocked_by_held_lock(core: &Core) {
    let fence = core
        .file_command_queue()
        .unwrap()
        .flush(batch_writer::FlushPoint::START)
        .unwrap();
    assert!(
        fence.wait_timeout(Duration::from_millis(200)).is_none(),
        "the file writer finished its queue while the test held the file lock"
    );
    assert!(MemoryReplies::poll(core).is_empty());
}
fn drain(core: &mut Core) {
    core.profile_detections.begin_shutdown();
    let deadline = Instant::now() + HANG_LIMIT;
    while core.profile_detections.poll_shutdown() != 0 {
        core.poll_file_writer();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    while core.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(MemoryReplies::poll(core).is_empty());
}
fn root() -> std::path::PathBuf {
    tasty_utils::path::tasty_home().unwrap()
}
fn counts_after_bootstrap(core: &Core) -> crate::app::file_writer::FileWriteCounts {
    // Child-registry startup reads share this queue; settle them before measuring remote operations.
    core.registries.child_terminals.wait_bootstrap();
    let fence = core
        .file_command_queue()
        .unwrap()
        .flush(batch_writer::FlushPoint::START)
        .unwrap();
    fence
        .wait_timeout(HANG_LIMIT)
        .expect("bootstrap file fence completed")
        .expect("bootstrap file work succeeded");
    let (queue, counts) = core.file_writer_stats().unwrap();
    assert_eq!(queue.pending_items, 0);
    assert_eq!(queue.held_items, 0);
    counts
}
#[test]
fn held_inline_add_and_remove_answer_original_callers_once_with_actual_operations() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let mut core = core();
    let before = counts_after_bootstrap(&core);
    let path = root().join("passkeys.toml");
    let held = file_replace::lock(&path).unwrap();
    let added = park(
        &core,
        passkey::handle_add(
            &core,
            json!(871),
            &json!({"name":"owned","kind":"inline","value":"private-fixture"}),
        ),
    );
    let removed = park(
        &core,
        passkey::handle_remove(&core, json!(872), &json!({"name":"owned"})),
    );
    assert_blocked_by_held_lock(&core);
    assert!(added.try_recv().is_err());
    assert!(removed.try_recv().is_err());
    assert!(!root().join("passkeys/owned").exists());
    assert_eq!(
        core.file_writer_stats()
            .unwrap()
            .1
            .operations
            .replace
            .attempts
            - before.operations.replace.attempts,
        0
    );
    drop(held);
    let answer = receive(&mut core, &added);
    assert_eq!(answer.id, json!(871));
    assert_eq!(
        answer.result.unwrap(),
        json!({"saved":true,"name":"owned","kind":"inline","replaced":false})
    );
    let answer = receive(&mut core, &removed);
    assert_eq!(answer.id, json!(872));
    assert_eq!(
        answer.result.unwrap(),
        json!({"removed":true,"name":"owned"})
    );
    assert!(
        tasty_remote_profiles::Passkeys::load()
            .get("owned")
            .is_none()
    );
    assert!(!root().join("passkeys/owned").exists());
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(
        counts.operations.replace.succeeded - before.operations.replace.succeeded,
        3
    );
    assert_eq!(
        counts.operations.delete.succeeded - before.operations.delete.succeeded,
        1
    );
    assert_eq!(
        counts.commands.known_success - before.commands.known_success,
        2
    );
    assert_eq!(
        counts.commands.unclassified - before.commands.unclassified,
        0
    );
    drain(&mut core);
    assert!(added.try_recv().is_err());
    assert!(removed.try_recv().is_err());
}
#[test]
fn malformed_profile_keeps_secret_and_original_absent_until_safe_error_receipt() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let mut core = core();
    let path = root().join("remote-profiles.toml");
    let original = "private_field='do-not-log-fixture'\nbroken=[";
    std::fs::write(&path, original).unwrap();
    let held = file_replace::lock(&path).unwrap();
    let placeholder = remote_profile::handle_add(
        &mut core,
        json!(873),
        &json!({"name":"owned","host":"owned.invalid","identity_file":"private-path","shell":"auto"}),
    );
    let answer = park(&core, placeholder);
    assert!(answer.try_recv().is_err());
    assert!(!core.profile_detections.has_pending());
    drop(held);
    drain(&mut core);
    let response = answer.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(response.id, json!(873));
    let error = response.error.unwrap();
    assert!(!error.message.contains("do-not-log-fixture"));
    let data = error.data.unwrap();
    assert_eq!(data["saved"], false);
    assert_eq!(data["partial"], false);
    assert_eq!(data["secret_written"], false);
    assert_eq!(data["passkeys_saved"], false);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(!root().join("passkeys.toml").exists());
    assert!(!core.profile_detections.has_pending());
    assert_eq!(
        core.file_writer_stats()
            .unwrap()
            .1
            .operations
            .replace
            .attempts,
        0
    );
    assert_eq!(
        core.file_writer_stats().unwrap().1.commands.known_failure,
        1
    );
    assert!(answer.try_recv().is_err());
}
#[test]
fn closed_remote_queue_refuses_without_synchronous_mutation_or_detection() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let mut core = core();
    drain(&mut core);
    let response = passkey::handle_add(
        &core,
        json!(874),
        &json!({"name":"owned","kind":"inline","value":"private-fixture"}),
    );
    assert_eq!(response.id, json!(874));
    assert_eq!(response.error.unwrap().data.unwrap()["saved"], false);
    assert!(!root().join("passkeys.toml").exists());
    assert!(!root().join("passkeys/owned").exists());
    assert!(core.memory_replies().take_parked().is_none());
    assert!(!core.profile_detections.has_pending());
}
#[test]
fn unknown_failure_reports_no_guessed_save_partial_or_success() {
    let error = Failure::Transport(crate::app::file_writer::command::Failure {
        message: "outcome unknown".into(),
        unknown: true,
    });
    let response = error.response(json!(875));
    assert_eq!(response.id, json!(875));
    let data = response.error.unwrap().data.unwrap();
    assert!(data["saved"].is_null());
    assert!(data["partial"].is_null());
    assert_eq!(data["outcome_unknown"], true);
    assert!(!data.as_object().unwrap().contains_key("secret_written"));
}

#[test]
fn committed_list_and_failed_managed_cleanup_are_a_known_partial_error() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let store = tasty_remote_profiles::RemoteFileStore::at(root());
    store
        .passkey(tasty_remote_profiles::PasskeyCommand::Inline {
            name: "owned".into(),
            secret: "private-fixture".into(),
        })
        .unwrap();
    let secret = root().join("passkeys/owned");
    std::fs::remove_file(&secret).unwrap();
    std::fs::create_dir(&secret).unwrap();
    let mut core = core();
    let held = file_replace::lock(&root().join("passkeys.toml")).unwrap();
    let answer = park(
        &core,
        passkey::handle_remove(&core, json!(876), &json!({"name":"owned"})),
    );
    assert!(answer.try_recv().is_err());
    drop(held);
    let response = receive(&mut core, &answer);
    assert_eq!(response.id, json!(876));
    let data = response.error.unwrap().data.unwrap();
    assert_eq!(data["saved"], false);
    assert_eq!(data["partial"], true);
    assert_eq!(data["passkeys_saved"], true);
    assert_eq!(data["removed"], true);
    assert_eq!(data["cleanup_failed"], true);
    assert!(
        tasty_remote_profiles::Passkeys::load()
            .get("owned")
            .is_none()
    );
    assert!(secret.is_dir());
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.succeeded, 1);
    assert_eq!(counts.operations.delete.failed, 1);
    assert_eq!(counts.commands.known_failure, 1);
    assert_eq!(counts.commands.partial, 1);
    drain(&mut core);
    assert!(answer.try_recv().is_err());
}
#[cfg(unix)]
#[test]
fn inline_success_before_readonly_list_failure_preserves_partial_fields_and_private_file() {
    use std::os::unix::fs::PermissionsExt;
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let path = root().join("passkeys.toml");
    let original = "format=7\n";
    std::fs::write(&path, original).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    let mut core = core();
    let held = file_replace::lock(&path).unwrap();
    let answer = park(
        &core,
        passkey::handle_add(
            &core,
            json!(877),
            &json!({"name":"owned","kind":"inline","value":"private-fixture"}),
        ),
    );
    assert!(answer.try_recv().is_err());
    drop(held);
    let response = receive(&mut core, &answer);
    assert_eq!(response.id, json!(877));
    let error = response.error.unwrap();
    assert!(!error.message.contains("private-fixture"));
    let data = error.data.unwrap();
    assert_eq!(data["saved"], false);
    assert_eq!(data["partial"], true);
    assert_eq!(data["secret_written"], true);
    assert_eq!(data["passkeys_saved"], false);
    assert_eq!(data["stage"], "PasskeySave");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let secret = root().join("passkeys/owned");
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "private-fixture");
    assert_eq!(
        std::fs::metadata(secret).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.attempts, 2);
    assert_eq!(counts.operations.replace.succeeded, 1);
    assert_eq!(counts.operations.replace.failed, 1);
    assert_eq!(counts.commands.known_failure, 1);
    assert_eq!(counts.commands.partial, 1);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    drain(&mut core);
    assert!(answer.try_recv().is_err());
}
#[cfg(unix)]
#[test]
fn linked_key_commit_before_profile_failure_reports_partial_and_never_starts_detection() {
    use std::os::unix::fs::PermissionsExt;
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let path = root().join("remote-profiles.toml");
    let original = "format=7\n";
    std::fs::write(&path, original).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    let mut core = core();
    let held = file_replace::lock(&path).unwrap();
    let placeholder = remote_profile::handle_add(
        &mut core,
        json!(878),
        &json!({"name":"owned","host":"owned.invalid","identity_file":"private-path","shell":"auto"}),
    );
    let answer = park(&core, placeholder);
    assert!(answer.try_recv().is_err());
    assert!(!core.profile_detections.has_pending());
    drop(held);
    let response = receive(&mut core, &answer);
    assert_eq!(response.id, json!(878));
    let data = response.error.unwrap().data.unwrap();
    assert_eq!(data["saved"], false);
    assert_eq!(data["partial"], true);
    assert_eq!(data["secret_written"], false);
    assert_eq!(data["passkeys_saved"], true);
    assert_eq!(data["stage"], "ProfileSave");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(
        tasty_remote_profiles::Passkeys::load()
            .get("owned-key")
            .is_some()
    );
    assert!(!core.profile_detections.has_pending());
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.attempts, 2);
    assert_eq!(counts.operations.replace.succeeded, 1);
    assert_eq!(counts.operations.replace.failed, 1);
    assert_eq!(counts.commands.partial, 1);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    drain(&mut core);
    assert!(answer.try_recv().is_err());
}

#[test]
fn explicit_shell_upsert_clears_previous_failure_before_original_success_reply() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let original = tasty_remote_profiles::RemoteProfile::new("owned", "ssh")
        .with_field("host", "old.invalid")
        .with_field("shell", "auto")
        .with_field("detect_failed", "true");
    tasty_remote_profiles::RemoteFileStore::at(root())
        .profile(
            tasty_remote_profiles::ProfileCommand::Insert(original),
            None,
        )
        .unwrap();
    let mut core = core();
    let held = file_replace::lock(&root().join("remote-profiles.toml")).unwrap();
    let placeholder = remote_profile::handle_add(
        &mut core,
        json!(882),
        &json!({"name":"owned","host":"latest.invalid","shell":"bash"}),
    );
    let answer = park(&core, placeholder);
    assert_blocked_by_held_lock(&core);
    assert!(answer.try_recv().is_err());
    drop(held);
    let response = receive(&mut core, &answer);
    assert_eq!(response.id, json!(882));
    assert_eq!(response.result.unwrap()["saved"], true);
    let profiles = tasty_remote_profiles::RemoteProfiles::load();
    let profile = profiles.get("owned").unwrap();
    assert!(!profile.fields.contains_key("detect_failed"));
    assert_eq!(
        profile.fields.get("host").and_then(|value| value.as_str()),
        Some("latest.invalid")
    );
    assert!(!core.profile_detections.has_pending());
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.succeeded, 2);
    assert_eq!(counts.commands.known_success, 1);
    drain(&mut core);
    assert!(answer.try_recv().is_err());
}
