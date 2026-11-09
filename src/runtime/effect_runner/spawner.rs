//! Spawns claimed terminal candidates off the App thread. Validation, adoption and kind factories
//! stay with the App owner; this worker only starts the OS child and fills its private terminal.
//!
//! One worker keeps the serial order that preparation had on the App thread. The queue is bounded
//! and a full queue refuses the job before any external execution, so the refusal is a known
//! failure. A result nobody receives is retired by this worker and kept until its child is reaped.
use super::PreparationFailure;
use crate::core::layout_persistence::import::surface_data::SurfaceData;
use crate::runtime::journal_product::EffectLease;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use tasty_terminal::{ColorPalette, Pty, PtyPhase, PtyRetirement, Terminal};

/// Jobs waiting behind the running one. A full queue is answered before anything executes.
pub(crate) const QUEUE_CAPACITY: usize = 16;
/// How often the worker checks abandoned children while it has no job.
const REAP_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// Owned inputs fixed by the App before execution. Nothing here borrows the engine.
pub(crate) struct TerminalJob {
    pub(super) lease: EffectLease,
    pub(super) surface_id: u32,
    pub(super) cols: usize,
    pub(super) rows: usize,
    pub(super) shell: Option<String>,
    pub(super) arguments: Vec<String>,
    pub(super) environment: Vec<(String, String)>,
    pub(super) working_dir: Option<std::path::PathBuf>,
    pub(super) initial_input: Option<String>,
    pub(super) waker: tasty_terminal::Waker,
    pub(super) palette: ColorPalette,
    pub(super) scrollback_lines: usize,
    pub(super) disk_scrollback: bool,
    pub(super) disk_scrollback_lines: Option<usize>,
    pub(super) capture: Option<SurfaceData>,
    pub(super) startup_command: String,
}

pub(crate) struct SpawnedTerminal {
    pub(crate) terminal: Terminal,
    pub(crate) pty: Pty,
    pub(crate) scrollback_persist_id: Option<String>,
}

/// A failure is uncertain when a child may have started; its owner, if any, was retired by the
/// worker. A failure before any child is known.
pub(crate) type SpawnOutcome = Result<SpawnedTerminal, PreparationFailure>;

/// The exact owner of one submitted job's result. Dropping it before the job starts skips the
/// execution; dropping it later leaves the result to the worker's retirement list.
pub(crate) struct PendingSpawn {
    lease: EffectLease,
    receiver: mpsc::Receiver<SpawnOutcome>,
    abandoned: Arc<AtomicBool>,
}

impl PendingSpawn {
    pub(crate) fn lease(&self) -> &EffectLease {
        &self.lease
    }
    /// `None` while the job runs. A worker that ended without a result is an uncertain failure.
    pub(crate) fn try_take(&self) -> Option<SpawnOutcome> {
        match self.receiver.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(PreparationFailure {
                reason: "terminal preparation worker ended without a result".into(),
                uncertain: true,
            })),
        }
    }
}

impl Drop for PendingSpawn {
    fn drop(&mut self) {
        self.abandoned.store(true, Ordering::Release);
    }
}

struct Submitted {
    job: TerminalJob,
    reply: mpsc::SyncSender<SpawnOutcome>,
    abandoned: Arc<AtomicBool>,
}

pub(crate) struct TerminalSpawner {
    sender: Option<mpsc::SyncSender<Submitted>>,
    /// Set before the queue closes. The App drops its creations after the spawner, so their
    /// results still have owners then; nothing they wait for may start a child anymore.
    closing: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    #[cfg(test)]
    gate: Arc<test_gate::Gate>,
    #[cfg(test)]
    probe: crate::runtime::journal_product::thread_probe::ThreadProbe,
}

impl TerminalSpawner {
    pub(crate) fn spawn(wake: Arc<dyn Fn() + Send + Sync>) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let closing = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        #[cfg(test)]
        let gate = Arc::new(test_gate::Gate::default());
        #[cfg(test)]
        let worker_gate = gate.clone();
        #[cfg(test)]
        let probe = crate::runtime::journal_product::thread_probe::ThreadProbe::default();
        #[cfg(test)]
        let bound = probe.clone();
        let thread = std::thread::Builder::new()
            .name("terminal-preparation".into())
            .spawn(move || {
                #[cfg(test)]
                bound.bind_current();
                Worker {
                    abandoned: Vec::new(),
                    closing: worker_closing,
                    #[cfg(test)]
                    gate: worker_gate,
                }
                .run(receiver, wake)
            })?;
        Ok(Self {
            sender: Some(sender),
            closing,
            thread: Some(thread),
            #[cfg(test)]
            gate,
            #[cfg(test)]
            probe,
        })
    }

    /// A refused job is returned untouched: nothing was executed for it.
    pub(crate) fn submit(&self, job: TerminalJob) -> Result<PendingSpawn, (TerminalJob, String)> {
        let Some(sender) = &self.sender else {
            return Err((job, "terminal preparation worker stopped".into()));
        };
        let (reply, receiver) = mpsc::sync_channel(1);
        let abandoned = Arc::new(AtomicBool::new(false));
        let lease = job.lease.clone();
        let submitted = Submitted {
            job,
            reply,
            abandoned: abandoned.clone(),
        };
        match sender.try_send(submitted) {
            Ok(()) => Ok(PendingSpawn {
                lease,
                receiver,
                abandoned,
            }),
            Err(mpsc::TrySendError::Full(submitted)) => Err((
                submitted.job,
                format!("terminal preparation queue is full ({QUEUE_CAPACITY} waiting)"),
            )),
            Err(mpsc::TrySendError::Disconnected(submitted)) => {
                Err((submitted.job, "terminal preparation worker stopped".into()))
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn hold(&self) -> test_gate::Held {
        self.gate.hold()
    }
    #[cfg(test)]
    pub(crate) fn executed(&self) -> usize {
        self.gate.executed()
    }
    /// Every later job fails after its child started, as a setup failure would.
    #[cfg(test)]
    pub(crate) fn fail_after_start(&self) {
        self.gate.fail_after_start();
    }
    /// The worker took a job and is held before running it.
    #[cfg(test)]
    pub(crate) fn parked(&self) -> bool {
        self.gate.parked()
    }
    #[cfg(test)]
    pub(crate) fn thread_probe(
        &self,
    ) -> crate::runtime::journal_product::thread_probe::ThreadProbe {
        self.probe.clone()
    }
}

impl Drop for TerminalSpawner {
    /// Closing the queue skips every job that has not started and lets the worker finish the
    /// running one and reap every abandoned child. The PTY retirement escalates to a kill after
    /// its grace period, so the join is bounded.
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        self.sender = None;
        #[cfg(test)]
        self.gate.release_all();
        // A test that judged this worker stalled must fail instead of joining it.
        #[cfg(test)]
        if self.probe.is_abandoned() {
            return;
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("terminal preparation worker panicked");
        }
    }
}

struct Worker {
    abandoned: Vec<PtyRetirement>,
    closing: Arc<AtomicBool>,
    #[cfg(test)]
    gate: Arc<test_gate::Gate>,
}

impl Worker {
    fn run(mut self, receiver: mpsc::Receiver<Submitted>, wake: Arc<dyn Fn() + Send + Sync>) {
        loop {
            let next = if self.abandoned.is_empty() {
                receiver
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
            } else {
                receiver.recv_timeout(REAP_POLL)
            };
            match next {
                Ok(submitted) => {
                    self.execute(submitted);
                    wake();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            self.forget_reaped();
        }
        while !self.abandoned.is_empty() {
            std::thread::sleep(REAP_POLL);
            self.forget_reaped();
        }
    }

    fn execute(&mut self, submitted: Submitted) {
        #[cfg(test)]
        self.gate.wait();
        if submitted.abandoned.load(Ordering::Acquire) || self.closing.load(Ordering::Acquire) {
            return;
        }
        #[cfg(test)]
        self.gate.count();
        let outcome = self.start(submitted.job);
        // A child that finished starting while the spawner closed stays here until its reap; its
        // receiver would otherwise drop it with the App without observing the reap.
        let outcome = match outcome {
            Ok(spawned) if self.closing.load(Ordering::Acquire) => {
                drop(spawned.terminal);
                self.abandoned.push(spawned.pty.retire());
                return;
            }
            outcome => outcome,
        };
        if let Err(mpsc::SendError(Ok(spawned))) = submitted.reply.send(outcome) {
            drop(spawned.terminal);
            self.abandoned.push(spawned.pty.retire());
        }
    }

    fn start(&mut self, job: TerminalJob) -> SpawnOutcome {
        let args: Vec<_> = job.arguments.iter().map(String::as_str).collect();
        let environment: Vec<_> = job
            .environment
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let (mut terminal, pty) = tasty_terminal::spawn_terminal(
            tasty_terminal::TerminalConfig {
                cols: job.cols,
                rows: job.rows,
                shell: job.shell.as_deref(),
                args: &args,
                extra_env: &environment,
                surface_id: job.surface_id,
                working_dir: job.working_dir.as_deref(),
                initial_input: job.initial_input.as_deref(),
            },
            job.waker,
        )
        .map_err(|error| PreparationFailure {
            reason: error.to_string(),
            uncertain: !error.is_before_child(),
        })?;
        #[cfg(test)]
        if self.gate.fails_after_start() {
            drop(terminal);
            self.abandoned.push(pty.retire());
            return Err(PreparationFailure {
                reason: "injected failure after the child started".into(),
                uncertain: true,
            });
        }
        terminal.set_color_palette(job.palette);
        terminal.set_scrollback_limit(job.scrollback_lines);
        if job.disk_scrollback {
            terminal.set_disk_scrollback_limit(job.disk_scrollback_lines);
            terminal.enable_disk_scrollback(job.surface_id);
        }
        let mut scrollback_persist_id = None;
        if let Err(error) = super::restore_terminal_capture(
            &mut terminal,
            job.surface_id,
            job.capture,
            &mut scrollback_persist_id,
        ) {
            drop(terminal);
            self.abandoned.push(pty.retire());
            return Err(PreparationFailure {
                reason: error.to_string(),
                uncertain: true,
            });
        }
        if !job.startup_command.trim().is_empty() {
            terminal.send_key(&format!("{}\n", job.startup_command.trim()));
        }
        Ok(SpawnedTerminal {
            terminal,
            pty,
            scrollback_persist_id,
        })
    }

    fn forget_reaped(&mut self) {
        self.abandoned.retain(|receipt| {
            let phase = receipt.observation().phase;
            if phase == PtyPhase::WaitFailed {
                tracing::warn!(
                    generation = receipt.generation().value(),
                    "abandoned terminal candidate could not confirm reap"
                );
            }
            !matches!(phase, PtyPhase::Reaped | PtyPhase::WaitFailed)
        });
    }
}

/// A job that only exits; tests use it to occupy the queue.
#[cfg(test)]
pub(crate) fn test_job(lease: EffectLease) -> TerminalJob {
    TerminalJob {
        lease,
        surface_id: u32::MAX,
        cols: 80,
        rows: 24,
        shell: Some("/bin/sh".into()),
        arguments: vec!["-c".into(), "exit 0".into()],
        environment: Vec::new(),
        working_dir: None,
        initial_input: None,
        waker: Arc::new(|| {}),
        palette: crate::runtime::terminal_store::current_terminal_palette(),
        scrollback_lines: 100,
        disk_scrollback: false,
        disk_scrollback_lines: None,
        capture: None,
        startup_command: String::new(),
    }
}

#[cfg(test)]
pub(crate) mod test_gate {
    use std::sync::{Condvar, Mutex};

    /// Holds every job of one spawner before it executes, so a test can observe the App while a
    /// preparation is still running.
    #[derive(Default)]
    pub(crate) struct Gate {
        state: Mutex<State>,
        changed: Condvar,
    }
    #[derive(Default)]
    struct State {
        holds: usize,
        executed: usize,
        /// Whether the worker has taken a job and waits at the gate.
        parked: bool,
        fail_after_start: bool,
    }
    pub(crate) struct Held(std::sync::Arc<Gate>);

    impl Gate {
        pub(super) fn hold(self: &std::sync::Arc<Self>) -> Held {
            self.state.lock().unwrap().holds += 1;
            Held(self.clone())
        }
        pub(super) fn wait(&self) {
            let mut state = self.state.lock().unwrap();
            state.parked = true;
            while state.holds > 0 {
                state = self.changed.wait(state).unwrap();
            }
            state.parked = false;
        }
        pub(super) fn parked(&self) -> bool {
            self.state.lock().unwrap().parked
        }
        pub(super) fn count(&self) {
            self.state.lock().unwrap().executed += 1;
        }
        pub(super) fn executed(&self) -> usize {
            self.state.lock().unwrap().executed
        }
        pub(super) fn fail_after_start(&self) {
            self.state.lock().unwrap().fail_after_start = true;
        }
        pub(super) fn fails_after_start(&self) -> bool {
            self.state.lock().unwrap().fail_after_start
        }
        pub(super) fn release_all(&self) {
            self.state.lock().unwrap().holds = 0;
            self.changed.notify_all();
        }
    }
    impl Drop for Held {
        fn drop(&mut self) {
            let mut state = self.0.state.lock().unwrap();
            state.holds = state.holds.saturating_sub(1);
            self.0.changed.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease() -> EffectLease {
        EffectLease {
            effect_id: "closing".into(),
            operation: tasty_core::OperationId("closing".into()),
            stream: "structure:slot-1".into(),
            runtime_epoch: 1,
            resource_generation: 1,
            attempt: 1,
        }
    }

    /// At App drop the spawner closes while the creations still own their pending results.
    /// Jobs that have not started by then never start a child.
    #[test]
    fn closing_the_spawner_never_starts_a_waiting_job() {
        let spawner = TerminalSpawner::spawn(Arc::new(|| {})).unwrap();
        let gate = spawner.gate.clone();
        let held = spawner.hold();
        let first = spawner
            .submit(test_job(lease()))
            .unwrap_or_else(|(_, reason)| panic!("{reason}"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !spawner.parked() {
            assert!(
                std::time::Instant::now() < deadline,
                "the worker takes a job"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let second = spawner
            .submit(test_job(lease()))
            .unwrap_or_else(|(_, reason)| panic!("{reason}"));
        drop(spawner);
        assert_eq!(
            gate.executed(),
            0,
            "no queued job ran after the queue closed"
        );
        drop((held, first, second));
    }

    /// 빈 실행값이나 허용하지 않는 셸은 COMSPEC(cmd.exe)으로 빠지지 않고 자식 없는 실패가 된다.
    #[cfg(windows)]
    #[test]
    fn a_windows_job_without_git_bash_fails_before_a_child_starts() {
        for shell in [None, Some("C:/Windows/System32/cmd.exe".to_string())] {
            let mut worker = Worker {
                abandoned: Vec::new(),
                closing: Arc::new(AtomicBool::new(false)),
                gate: Arc::new(test_gate::Gate::default()),
            };
            let mut job = test_job(lease());
            job.shell = shell;
            job.arguments = Vec::new();
            let Err(failure) = worker.start(job) else {
                panic!("the job must not start a shell");
            };
            assert!(!failure.uncertain, "{}", failure.reason);
            assert!(worker.abandoned.is_empty());
        }
    }
}
