//! Owned subprocess termination after physical commit, partial ACK, and durable claim.
use super::*;
use std::process::{Child, Command, Stdio};

const TEST: &str = "runtime::journal_product::tests::group::crash::owned_group_crash_reopens_each_durable_cut_without_duplicate_effects";
const HOME_ENV: &str = "TASTY_JOURNAL_GROUP_CRASH_HOME";
const PHASE_ENV: &str = "TASTY_JOURNAL_GROUP_CRASH_PHASE";
const CHILD_LOG: &str = "owned-child.log";
/// The child starts its journal worker and then sleeps without ever writing the marker.
const STALL_PHASE: &str = "stalled-before-marker";

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            drop(self.0.kill());
            drop(self.0.wait());
        }
    }
}

fn child(home: &std::path::Path, phase: &str) -> ! {
    let fixture = fixture(home);
    if phase == STALL_PHASE {
        loop {
            std::thread::park();
        }
    }
    let first = first_publication(&fixture, home);
    let mut claimed = None;
    if phase != "before-first-ack" {
        let second = acknowledge_first(&fixture, first);
        if phase == "after-claim" {
            fixture.worker.acknowledge(second, Ok(())).unwrap();
            finished(&fixture.worker, 12).unwrap();
            let Completion::Publish { batch, .. } = receive(&fixture.worker) else {
                panic!("B3 publication")
            };
            fixture.worker.acknowledge(batch.batch_id, Ok(())).unwrap();
            for ticket in [13, 31] {
                finished(&fixture.worker, ticket).unwrap();
            }
            let conn = readonly(home);
            let b3 = command(&conn, "b3");
            let effect: String = conn
                .query_row(
                    "SELECT effect_id FROM effects WHERE command_id=?1",
                    [&b3.0],
                    |r| r.get(0),
                )
                .unwrap();
            let operation = OperationId(effect.strip_suffix("/prepare").unwrap().into());
            submit(
                &fixture.worker,
                99,
                Work::ClaimPreparation {
                    stream: "structure:slot-1".into(),
                    operation,
                },
            );
            let ResultValue::Claimed(receipt) = finished(&fixture.worker, 99).unwrap() else {
                panic!("durable claim")
            };
            assert_eq!(receipt.lease.attempt, 1);
            claimed = Some(receipt.lease);
        }
    }
    let conn = readonly(home);
    let marker = serde_json::json!({"phase":phase,"pid":std::process::id(),"first_cut":first,
        "b1":command(&conn,"b1"),"b2":command(&conn,"b2"),"b3":command(&conn,"b3"),"claim":claimed});
    std::fs::write(
        home.join("reached.tmp"),
        serde_json::to_vec_pretty(&marker).unwrap(),
    )
    .unwrap();
    std::fs::rename(home.join("reached.tmp"), home.join("reached.json")).unwrap();
    // Keep all resources and the ACK-blocked worker alive until this exact owned PID is killed.
    loop {
        std::thread::park();
    }
}

fn spawn_child(home: &std::path::Path, phase: &str) -> Owned {
    let out = std::fs::File::create(home.join(CHILD_LOG)).unwrap();
    Owned(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(HOME_ENV, home)
            .env(PHASE_ENV, phase)
            .stdin(Stdio::null())
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap(),
    )
}

/// Waits for the child's marker. The limit is the time every thread of the child slept.
fn wait_for_marker(child: &mut Owned, home: &std::path::Path, phase: &str) {
    let pid = child.0.id();
    let mut stall = StallBudget::for_process(Some(pid));
    while !home.join("reached.json").exists() {
        if let Some(status) = child.0.try_wait().unwrap() {
            panic!(
                "owned child {pid} exited before {phase}: {status}\n{}",
                std::fs::read_to_string(home.join(CHILD_LOG)).unwrap()
            );
        }
        stall.nap("owned journal group crash marker");
    }
}

#[test]
#[should_panic(expected = "owned journal group crash marker stalled: the watched process slept")]
fn a_child_whose_threads_all_sleep_without_a_marker_fails_as_stalled() {
    let home = tempfile::tempdir().unwrap();
    let mut child = spawn_child(home.path(), STALL_PHASE);
    wait_for_marker(&mut child, home.path(), STALL_PHASE);
}

#[test]
fn owned_group_crash_reopens_each_durable_cut_without_duplicate_effects() {
    if let Some(home) = std::env::var_os(HOME_ENV) {
        child(
            std::path::Path::new(&home),
            &std::env::var(PHASE_ENV).unwrap(),
        );
    }
    for phase in ["before-first-ack", "after-first-ack", "after-claim"] {
        let home = tempfile::tempdir().unwrap();
        let mut child = spawn_child(home.path(), phase);
        let pid = child.0.id();
        wait_for_marker(&mut child, home.path(), phase);
        let marker = std::fs::read_to_string(home.path().join("reached.json")).unwrap();
        child.0.kill().unwrap();
        let status = child.0.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9));
        verify_reopen_and_retries(home.path(), if phase == "after-claim" { 1 } else { 0 });
        verify_worker_recovery(home.path(), if phase == "after-claim" { 1 } else { 0 });
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .without_time()
            .with_writer(std::io::stderr)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(phase, pid, signal = 9, marker = %marker, "owned group crash verified");
        });
    }
}
