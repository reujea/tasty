//! GUI form preparation and its actual existing worker route, without an inline-save fallback.
use super::*;
use crate::app::file_writer::settle::{HANG_LIMIT, settle};
use std::time::Instant;
use tasty_utils::{batch_writer, file_replace};
#[test]
fn new_gui_key_uses_worker_receipts_and_duplicate_failure_never_replaces_its_secret() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (_state, _session) = crate::state::tests::test_state();
    let root = tasty_utils::path::tasty_home().unwrap();
    let writer = crate::app::file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .with_file_writer(writer)
        .build()
        .unwrap();
    let held = file_replace::lock(&root.join("passkeys.toml")).unwrap();
    let form = PasskeyForm {
        name: "owned".into(),
        kind: "inline".into(),
        value: "first-private-fixture".into(),
        ..Default::default()
    };
    let request = prepare(FileAction::SavePasskey(form.clone())).unwrap();
    let ticket =
        files::submit(&core, request).unwrap_or_else(|error| panic!("{}", error.message()));
    assert!(files::poll(&ticket).is_none());
    assert!(!root.join("passkeys/owned").exists());
    drop(held);
    let output = receive(&mut core, &ticket).unwrap_or_else(|error| panic!("{}", error.message()));
    assert!(matches!(output.value, files::Value::Passkey(ref value) if !value.replaced));
    let mut duplicate = form;
    duplicate.value = "second-private-fixture".into();
    let request = prepare(FileAction::SavePasskey(duplicate)).unwrap();
    let ticket =
        files::submit(&core, request).unwrap_or_else(|error| panic!("{}", error.message()));
    let error = match receive(&mut core, &ticket) {
        Err(error) => error,
        Ok(_) => panic!("duplicate GUI name must fail"),
    };
    assert_eq!(
        error.gui_message(),
        crate::i18n::t("remote_tool.err_name_dup")
    );
    assert!(!error.partial());
    assert!(!error.unknown());
    assert_eq!(
        error.response(serde_json::json!(881)).error.unwrap().code,
        -32041
    );
    assert_eq!(
        std::fs::read_to_string(root.join("passkeys/owned")).unwrap(),
        "first-private-fixture"
    );
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.succeeded, 2);
    assert_eq!(counts.operations.replace.attempts, 2);
    assert_eq!(counts.commands.known_success, 1);
    assert_eq!(counts.commands.known_failure, 1);
    assert!(files::poll(&ticket).is_none());
    let deadline = Instant::now() + HANG_LIMIT;
    while core.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn receive(
    core: &mut crate::app::services::AppServices,
    ticket: &files::Ticket,
) -> Result<files::Output, files::Failure> {
    settle(&core.file_command_queue().unwrap());
    core.poll_file_writer();
    files::poll(ticket).expect("the file writer finished without a result for this ticket")
}
