use super::*;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

type ResultValue = Result<persistence::Value, persistence::Failure>;
struct Gate(mpsc::Receiver<ResultValue>);
impl persistence::Receipt for Gate {
    fn poll(&mut self) -> Option<ResultValue> {
        self.0.try_recv().ok()
    }
}
fn gate() -> (mpsc::Sender<ResultValue>, Box<dyn persistence::Receipt>) {
    let (send, receive) = mpsc::channel();
    (send, Box::new(Gate(receive)))
}
fn pending(progress: Progress) -> Job {
    match progress {
        Progress::Pending(job) => job,
        Progress::Done(_) => panic!("receipt was not ready"),
    }
}
fn done(progress: Progress) -> DetectionUpdate {
    match progress {
        Progress::Done(update) => update,
        Progress::Pending(_) => panic!("terminal receipt was ready"),
    }
}
fn probed(result: Result<String, String>) -> Job {
    let worker = std::thread::spawn(move || result);
    let deadline = Instant::now() + Duration::from_secs(2);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || worker.is_finished()),
        "owned scripted probe did not finish"
    );
    Job {
        id: DetectionId(7),
        name: "original".into(),
        cancel: tasty_ssh::SshCancel::new(),
        stage: Stage::Probing {
            original: RemoteProfile::new("original", "ssh"),
            root: PathBuf::from("owned-root"),
            worker,
        },
    }
}
fn sink(receipt: Box<dyn persistence::Receipt>) -> (persistence::Submit, Arc<Mutex<Vec<bool>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let receipt = Mutex::new(Some(receipt));
    (
        Arc::new(move |request| {
            let persistence::Request::Save {
                root,
                original,
                failed,
                ..
            } = request
            else {
                panic!("unexpected load");
            };
            assert_eq!(root, PathBuf::from("owned-root"));
            assert_eq!(original.name, "original");
            seen.lock().unwrap().push(failed);
            Ok(receipt.lock().unwrap().take().expect("save submitted once"))
        }),
        calls,
    )
}
#[test]
fn successful_probe_remains_pending_until_status_save_receipt() {
    let (send, receipt) = gate();
    let (sink, calls) = sink(receipt);
    let job = pending(probed(Ok("bash".into())).advance(false, Some(&sink)));
    let job = pending(job.advance(false, Some(&sink)));
    assert_eq!(*calls.lock().unwrap(), vec![false]);
    assert!(send.send(Ok(persistence::Value::Saved)).is_ok());
    let update = done(job.advance(false, Some(&sink)));
    assert_eq!(update.id, DetectionId(7));
    assert_eq!(update.name, "original");
    assert_eq!(update.result, Ok("bash".into()));
    assert_eq!(calls.lock().unwrap().len(), 1);
}
#[test]
fn unknown_status_save_is_terminal_error_and_never_replayed() {
    let (send, receipt) = gate();
    let (sink, calls) = sink(receipt);
    let job = pending(probed(Ok("bash".into())).advance(false, Some(&sink)));
    assert!(
        send.send(Err(persistence::Failure {
            message: "outcome unknown".into(),
            unknown: true
        }))
        .is_ok()
    );
    assert_eq!(
        done(job.advance(false, Some(&sink))).result,
        Err("outcome unknown".into())
    );
    assert_eq!(*calls.lock().unwrap(), vec![false]);
}
#[test]
fn cancelled_finished_probe_does_not_admit_status_write() {
    let (_send, receipt) = gate();
    let (sink, calls) = sink(receipt);
    let job = probed(Ok("bash".into()));
    job.cancel.request_cancel();
    assert!(done(job.advance(false, Some(&sink))).result.is_err());
    assert!(calls.lock().unwrap().is_empty());
}
#[test]
fn shutdown_retains_already_accepted_save_until_terminal_receipt() {
    let (send, receipt) = gate();
    let (sink, calls) = sink(receipt);
    let job = pending(probed(Ok("bash".into())).advance(false, Some(&sink)));
    job.cancel.request_cancel();
    let job = pending(job.advance(true, Some(&sink)));
    assert!(send.send(Ok(persistence::Value::Saved)).is_ok());
    assert_eq!(
        done(job.advance(true, Some(&sink))).result,
        Ok("bash".into())
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
}
#[test]
fn failed_probe_publishes_original_failure_only_after_failed_status_receipt() {
    let (send, receipt) = gate();
    let (sink, calls) = sink(receipt);
    let job = pending(probed(Err("probe failed".into())).advance(false, Some(&sink)));
    let job = pending(job.advance(false, Some(&sink)));
    assert_eq!(*calls.lock().unwrap(), vec![true]);
    assert!(send.send(Ok(persistence::Value::Saved)).is_ok());
    assert_eq!(
        done(job.advance(false, Some(&sink))).result,
        Err("probe failed".into())
    );
}
#[test]
fn shutdown_waits_for_preparation_without_starting_ssh() {
    let (send, receipt) = gate();
    let job = Job::preparing(DetectionId(9), "original".into(), receipt);
    let job = pending(job.advance(true, None));
    assert!(
        send.send(Ok(persistence::Value::Loaded(persistence::Input {
            original: RemoteProfile::new("original", "ssh"),
            passkeys: Default::default(),
            root: PathBuf::from("owned-root")
        })))
        .is_ok()
    );
    assert!(done(job.advance(true, None)).result.is_err());
}
