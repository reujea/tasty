//! A refused setup names its error in the fixture panic, with the I/O cause when there is one.
use super::fault::{Fault, inject};
use super::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::AtomicBool,
};

/// A raw value with no meaning on any OS: only its delivery is checked, not what it stands for.
const RAW: i32 = 9999;

fn panic_message(run: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(run)).expect_err("the fixture did not panic");
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload
            .downcast::<&str>()
            .map(|message| (*message).to_owned())
            .unwrap_or_else(|_| panic!("the fixture panic carried no message")),
    }
}

#[test]
fn a_setup_retired_after_admission_names_retired_in_the_ready_panic() {
    let _serial = serial();
    let current = Arc::new(AtomicBool::new(true));
    let flag = current.clone();
    let mut setup = start(Arc::new(move || flag.load(Ordering::Acquire)));
    current.store(false, Ordering::Release);
    let message = panic_message(|| drop(ready(&mut setup)));
    assert!(message.contains("Retired"), "{message}");
    drop(setup);
    reaped();
}

#[test]
fn a_setup_thread_that_cannot_spawn_names_its_os_error_in_the_start_panic() {
    let _serial = serial();
    inject(Fault::Spawn(RAW));
    let message = panic_message(|| drop(start(Arc::new(|| true))));
    assert!(message.contains(&format!("os: Some({RAW})")), "{message}");
    // The refused spawn returned its owner permit.
    reaped();
}

#[test]
fn a_runtime_that_cannot_build_names_its_os_error_in_the_ready_panic() {
    let _serial = serial();
    inject(Fault::Build(RAW));
    let mut setup = start(Arc::new(|| true));
    let message = panic_message(|| drop(ready(&mut setup)));
    assert!(message.contains(&format!("os: Some({RAW})")), "{message}");
    drop(setup);
    reaped();
}

/// Collects WARN events from every thread. Only a `rerun_alone` child may install it: that process
/// runs this one test, so the process-wide subscriber reaches no other test.
fn global_warnings() -> Arc<std::sync::Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(
        crate::test_support::is_rerun_alone(),
        "a process-wide subscriber is installed only in a rerun_alone child"
    );
    let buffer = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Sink(buffer.clone());
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .without_time()
        .with_writer(move || sink.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("the only global subscriber");
    buffer
}

#[test]
fn a_runtime_build_that_panics_names_setup_panicked_and_warns_its_message() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_runtime_build_that_panics_names_setup_panicked_and_warns_its_message",
    ) {
        return;
    }
    let warnings = global_warnings();
    let _serial = serial();
    inject(Fault::BuildPanic);
    let mut setup = start(Arc::new(|| true));
    let message = panic_message(|| drop(ready(&mut setup)));
    assert!(message.contains("SetupPanicked"), "{message}");
    // The result is sent before wake, so the wake is observed on its own.
    let published = setup.published.clone();
    wait_for("setup wake after a build panic", || {
        published.load(Ordering::Acquire)
    });
    drop(setup);
    reaped();
    let log = String::from_utf8_lossy(
        &warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
    .into_owned();
    let panicked: Vec<_> = log
        .lines()
        .filter(|line| line.contains("native HTTP runtime setup panicked"))
        .collect();
    assert_eq!(panicked.len(), 1, "{log}");
    assert!(panicked[0].contains(super::fault::BUILD_PANIC), "{log}");
}

#[test]
fn a_build_panic_payload_whose_destructor_panics_still_delivers_setup_panicked_and_wakes() {
    let _serial = serial();
    inject(Fault::BuildPanicDropPanics);
    let mut setup = start(Arc::new(|| true));
    let message = panic_message(|| drop(ready(&mut setup)));
    assert!(message.contains("SetupPanicked"), "{message}");
    let published = setup.published.clone();
    wait_for("setup wake after a build panic", || {
        published.load(Ordering::Acquire)
    });
    drop(setup);
    reaped();
}
