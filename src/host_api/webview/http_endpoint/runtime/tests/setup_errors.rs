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
