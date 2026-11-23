use super::*;
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

#[test]
fn missing_neutral_port_rejects_without_executing_even_in_test_builds() {
    let result = submit::<()>(None, 1, || panic!("unadmitted work executed"), |_| 0);
    let failure = result.err().expect("missing port must reject");
    assert!(!failure.unknown);
}

#[test]
fn injected_port_and_app_handle_share_one_ordered_worker_and_consuming_receipts() {
    use crate::app::file_writer;
    use tasty_utils::batch_writer::{Limits, SystemClock};
    let mut writer = file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let handle = writer.writer.handle();
    let queue: Queue = Arc::new(handle.clone());
    let (entered, events) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let first = file_writer::command::submit(
        Some(&handle),
        128,
        move || {
            entered.send(std::thread::current().id()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            1_u32
        },
        |_| 4,
    )
    .unwrap();
    let worker_id = events.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(worker_id, std::thread::current().id());
    let executed = Arc::new(Mutex::new(None));
    let second_executed = executed.clone();
    let second = submit(
        Some(queue.as_ref()),
        128,
        move || {
            *second_executed.lock().unwrap() = Some(std::thread::current().id());
            2_u32
        },
        |_| 4,
    )
    .unwrap();
    assert!(second.poll().is_none());
    assert!(executed.lock().unwrap().is_none());
    release.send(()).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    assert!(
        tasty_utils::test_wait::holds_by(end, || writer.writer.poll_shutdown() == 0),
        "shared writer drain"
    );
    assert_eq!(first.poll().unwrap().unwrap(), 1);
    assert_eq!(second.poll().unwrap().unwrap(), 2);
    assert!(first.poll().is_none());
    assert!(second.poll().is_none());
    assert_eq!(*executed.lock().unwrap(), Some(worker_id));
    let rejected = submit(
        Some(queue.as_ref()),
        1,
        || panic!("closed worker executed"),
        |_| 0,
    );
    assert!(!rejected.err().expect("closed port must reject").unknown);
}
