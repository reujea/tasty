//! PTY 입력·터미널 응답 송신과 화면 변경 적용. TerminalState 락 안에서 처리한다.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use termwiz::cell::CellAttributes;
use termwiz::surface::Change;

use crate::sink::{self, OutputSink, WriteProgress};
use crate::{Terminal, TerminalState};

/// PTY writer의 write_all+flush 완료를 기다리는 핸들.
/// wait는 블로킹하므로 메인 스레드 밖에서 호출한다.
pub struct WriteAck {
    progress: WriteProgress,
    target: u64,
}

impl WriteAck {
    /// Nonblocking flush observation for event-loop-owned input continuations.
    pub fn is_complete(&self) -> bool {
        *sink::lock_write_progress(&self.progress.0) >= self.target
    }

    /// writer의 완료 횟수가 target에 도달하면 true를 반환한다.
    /// detached 터미널처럼 writer가 없거나 대기 제한 안에 완료하지 못하면 false다.
    ///
    /// 깨어날 때마다 카운터로 판정한다. std `wait_timeout_while`은 재획득에서 poison을 만나면 조건을
    /// 다시 보지 않고 그 한 번의 대기가 시간 초과였는지만 돌려준다. 그래서 poison 뒤 가짜 깨어남이
    /// 카운터가 안 올랐는데도 완료로 보이거나, 남은 시간을 기다리지 않고 끝날 수 있다.
    pub fn wait(&self, timeout: Duration) -> bool {
        let (lock, cvar) = &*self.progress;
        let deadline = Instant::now().checked_add(timeout);
        let mut guard = sink::lock_write_progress(lock);
        loop {
            if *guard >= self.target {
                return true;
            }
            let remaining = match deadline {
                Some(deadline) => deadline.saturating_duration_since(Instant::now()),
                None => timeout,
            };
            if remaining.is_zero() {
                return false;
            }
            guard = recover_wait(
                cvar.wait_timeout(guard, remaining),
                sink::WRITE_PROGRESS_WHAT,
                &sink::WRITE_PROGRESS_POISON_REPORTED,
            );
        }
    }
}

/// Condvar 재획득에서 발생한 poison도 복구하고 처음 한 번 보고한다. 깨어난 이유는 버린다.
/// 시험에서 다른 호출의 보고와 혼동하지 않도록 보고 플래그를 인자로 받는다.
fn recover_wait<T>(
    outcome: std::sync::LockResult<(T, std::sync::WaitTimeoutResult)>,
    what: &str,
    reported: &std::sync::atomic::AtomicBool,
) -> T {
    match outcome {
        Ok((guard, _)) => guard,
        Err(poisoned) => tasty_utils::poison::recover_poisoned(poisoned, what, reported).0,
    }
}

impl TerminalState {
    /// Route user/agent input bytes to the PTY writer (or the detached input
    /// sink). Records the input timestamp so PTY echo within
    /// `INPUT_ECHO_WINDOW` is not counted toward busy state.
    ///
    /// Only *externally originated* writes (keyboard, `send_bytes`, mouse
    /// reports) may take this path — the echo-suppression window exists to
    /// discount a program echoing back what the user just typed. Bytes the
    /// terminal itself generates are not input and must use
    /// [`send_terminal_response`](Self::send_terminal_response) instead, which
    /// enqueues identically but leaves the window alone. See
    /// `docs/design/policies/busy-indicator.md`.
    pub(crate) fn write_input(&mut self, bytes: Vec<u8>) {
        self.last_input_at = std::time::Instant::now();
        self.enqueue_to_pty(bytes);
    }

    /// Reply to a terminal query (DSR / DA / cursor position report). Runs on the
    /// reader worker during ingest, so it writes back through the same input
    /// channel.
    ///
    /// Deliberately does **not** touch `last_input_at`: this write originates
    /// from the terminal, not from the user, so counting it as input would let a
    /// TUI that polls (e.g. `ESC[6n` faster than `INPUT_ECHO_WINDOW`) hold the
    /// echo-suppression window open forever and stay `busy == false` while it is
    /// plainly producing output.
    pub(crate) fn send_terminal_response(&mut self, response: &str) {
        self.enqueue_to_pty(response.as_bytes().to_vec());
    }

    /// Hand bytes to the PTY writer (or the detached input sink) without
    /// classifying their origin. With neither wired, the bytes are dropped.
    fn enqueue_to_pty(&mut self, bytes: Vec<u8>) {
        if let Some(sink) = self.sink.as_ref() {
            if let Err(e) = sink.send(bytes) {
                tracing::warn!("terminal input channel closed during input: {e}");
            } else {
                self.enqueued_count += 1;
            }
        } else {
            tracing::trace!("terminal input dropped (no sink): {} bytes", bytes.len());
        }
    }

    pub(crate) fn apply_or_stage_change(&mut self, change: Change) {
        // VTE 명령이 현재 커서 위치를 읽으므로 변경을 즉시 적용한다.
        // 렌더러는 상태 락을 얻은 뒤 처리된 화면을 읽는다.
        self.apply_change(change);
    }

    pub(crate) fn apply_change(&mut self, change: Change) {
        self.mirror_pen(&change);

        // Text is the one Change termwiz resolves against the whole grid instead
        // of the DECSTBM region, and it can scroll internally (auto-wrap past the
        // bottom row) without emitting a ScrollRegionUp. That path confines the
        // move to the region and captures the evictions itself. It runs on the
        // alternate screen too — the region contract is the same there; only the
        // capture is skipped, inside that path, so alt-screen output never lands
        // in the primary screen's history.
        if let Change::Text(text) = change {
            self.apply_text_honoring_scroll_region(text);
            return;
        }

        if self.use_alternate {
            self.surface_mut().add_change(change);
            return;
        }

        self.capture_before_scroll(&change);
        self.surface_mut().add_change(change);
    }

    /// Keep `current_pen` in sync with the pen mutations termwiz's `Surface`
    /// performs internally (it offers no pen accessor). Mirrors exactly the
    /// `Surface::apply_change` cases that touch the pen: full attribute replace,
    /// single-attribute change, and the clear ops (which termwiz resets to
    /// default with the clear color). Read back by `map_sgr` to apply SGRs that
    /// lack an `AttributeChange` variant (Overline/UnderlineColor/VerticalAlign).
    fn mirror_pen(&mut self, change: &Change) {
        match change {
            Change::AllAttributes(attr) => self.current_pen = attr.clone(),
            Change::Attribute(attr_change) => self.current_pen.apply_change(attr_change),
            Change::ClearScreen(color)
            | Change::ClearToEndOfLine(color)
            | Change::ClearToEndOfScreen(color) => {
                self.current_pen = CellAttributes::default().set_background(*color).clone();
            }
            _ => {}
        }
    }

    /// Swap the active/inactive pen mirrors when crossing the primary↔alternate
    /// screen boundary. termwiz holds a separate pen per surface but exposes no
    /// accessor, so `current_pen` tracks the active surface and `saved_pen` holds
    /// the other; swapping on each transition keeps the mirror aligned with the
    /// surface that subsequent changes land on. Call exactly on a real transition
    /// of `use_alternate`. See `modes.rs` (alt-screen modes 1049 / 47 / 1047).
    pub(crate) fn swap_pen_for_surface_switch(&mut self) {
        std::mem::swap(&mut self.current_pen, &mut self.saved_pen);
    }
}

impl Terminal {
    /// Feed raw bytes through the shared ingest path. Useful for testing without
    /// a real PTY, and used by the debug `feed_bytes` IPC handler.
    pub fn process_bytes(&mut self, data: &[u8]) {
        if self.lock_state().ingest(data) {
            self.dirty.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    /// Set the input channel, typically to forward a detached mirror's input to attach.
    /// This replaces the current sender even if the terminal owns a PTY.
    pub fn set_input_sink(&mut self, sink: mpsc::Sender<Vec<u8>>) {
        let mut state = self.lock_state();
        state.sink = Some(OutputSink::external(sink).bind(state.connection.clone()));
    }

    /// Replace a detached remote connection without changing its VT content identity.
    /// Prior protocol/clipboard responses keep the old resource generation and are rejected.
    pub fn replace_external_connection(&mut self, sink: mpsc::Sender<Vec<u8>>) {
        let mut state = self.lock_state();
        state.connection.revoke();
        state.connection = crate::binding::ConnectionLease::new();
        state.sink = Some(OutputSink::external(sink).bind(state.connection.clone()));
        state.enqueued_count = 0;
        state.events.clear();
    }

    /// A nonblocking external endpoint. Remote applies its own bounded admission synchronously;
    /// Terminal owns neither a second unbounded byte queue nor a forwarding worker.
    pub fn bind_external_input(&mut self, send: crate::ExternalInput, reconnect: bool) {
        let mut state = self.lock_state();
        if reconnect {
            state.connection.revoke();
            state.connection = crate::binding::ConnectionLease::new();
            state.enqueued_count = 0;
            state.events.clear();
        }
        state.sink = Some(OutputSink::callback(send).bind(state.connection.clone()));
    }

    /// Plumb the host's resolved theme palette so OSC 10/11/12/4 color *queries*
    /// are answered with the colors the renderer actually draws. The host calls
    /// this on terminal creation and whenever the theme changes.
    pub fn set_color_palette(&mut self, palette: crate::color::ColorPalette) {
        self.lock_state().color_palette = Some(palette);
    }

    /// Apply a delayed host protocol response only to the requesting connection.
    /// Unlike user input, a response must not change echo/busy input timestamps.
    pub fn send_response_for(
        &mut self,
        generation: crate::ResourceGeneration,
        bytes: &[u8],
    ) -> bool {
        let mut state = self.lock_state();
        if state.connection.generation() != generation || !state.connection.is_active() {
            return false;
        }
        state.enqueue_to_pty(bytes.to_vec());
        true
    }

    /// Queue input against this exact physical connection, reporting admission failure.
    pub fn try_send_key_with_ack(
        &mut self,
        text: &str,
    ) -> Result<WriteAck, mpsc::SendError<Vec<u8>>> {
        let mut state = self.lock_state();
        let bytes = text.as_bytes().to_vec();
        let sink = state
            .sink
            .as_ref()
            .ok_or_else(|| mpsc::SendError(bytes.clone()))?;
        sink.send(bytes)?;
        let progress = sink.progress().clone();
        state.enqueued_count += 1;
        state.last_input_at = std::time::Instant::now();
        Ok(WriteAck {
            progress,
            target: state.enqueued_count,
        })
    }

    /// Send keyboard input to PTY (non-blocking, queued to writer thread).
    pub fn send_key(&mut self, text: &str) {
        self.lock_state().write_input(text.as_bytes().to_vec());
    }

    /// send_key처럼 큐에 넣고, 다른 스레드에서 PTY flush 완료를 기다릴 WriteAck도 반환한다.
    pub fn send_key_with_ack(&mut self, text: &str) -> WriteAck {
        let mut state = self.lock_state();
        state.write_input(text.as_bytes().to_vec());
        WriteAck {
            // Without a sink no writer reports completion; a fresh counter only meets target 0.
            progress: state
                .sink
                .as_ref()
                .map_or_else(sink::new_write_progress, |s| s.progress().clone()),
            target: state.enqueued_count,
        }
    }

    /// Send raw bytes to PTY (non-blocking, queued to writer thread).
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        self.lock_state().write_input(bytes.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Condvar, Mutex};

    use super::*;

    /// Condvar 재획득에서 발생한 poison도 보고하는지 독립 플래그로 확인한다.
    #[test]
    fn recover_wait_reports_the_poison_it_recovers() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let m = Arc::new(Mutex::new(0u64));
        let cv = Condvar::new();

        let poisoner = Arc::clone(&m);
        std::thread::spawn(move || {
            let _g = poisoner.lock().expect("아직 성한 락");
            panic!("락을 쥔 채 죽는다");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");

        // 이미 poison 된 락을 복구해 들어가 재획득에서 `Err` 를 받는다 —
        // 대기 중 poison 을 만났을 때와 같은 모양이다.
        let guard = m.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = cv.wait_timeout(guard, Duration::from_millis(10));
        assert!(
            outcome.is_err(),
            "재획득이 poison 을 만나야 이 축이 성립한다"
        );

        let flag = AtomicBool::new(false);
        let guard = recover_wait(outcome, "테스트 카운터", &flag);
        assert_eq!(*guard, 0, "복구한 guard 로 카운터를 다시 본다");
        assert!(
            flag.load(Ordering::Relaxed),
            "복구했으면 한 번은 보고해야 한다 — 조용한 복구는 조용한 유실과 구분되지 않는다"
        );
    }

    /// 대기 중 다른 스레드가 poison을 만드는 실행을 검사한다. 보고 플래그는 별도 시험에서 확인한다.
    #[test]
    fn a_poison_can_arrive_while_the_waiter_is_parked() {
        let progress: WriteProgress = Arc::new((Mutex::new(0), Condvar::new()));

        let for_waiter = Arc::clone(&progress);
        let waiter = std::thread::spawn(move || {
            let ack = WriteAck {
                progress: for_waiter,
                target: 1,
            };
            // 카운터를 올리지 않으므로 반환값은 시간 초과다.
            ack.wait(Duration::from_millis(600))
        });

        // 대기자가 Condvar 대기에서 락을 놓고 잠들 시간을 준다.
        std::thread::sleep(Duration::from_millis(100));

        let poisoner = Arc::clone(&progress);
        std::thread::spawn(move || {
            let _guard = poisoner.0.lock().expect("아직 성한 락");
            panic!("대기자가 자는 동안 락을 쥔 채 죽는다");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");
        assert!(progress.0.lock().is_err(), "poison 이 실제로 걸려야 한다");

        let timed_out = !waiter.join().expect("대기 스레드는 패닉하지 않는다");
        assert!(timed_out, "카운터를 안 올렸으므로 타임아웃이 정상이다");
    }

    /// poison 뒤 카운터를 올리지 않은 깨어남은 완료가 아니고, 대기 제한까지 계속 기다린다.
    /// std `wait_timeout_while`은 poison 재획득에서 깨어난 이유만 돌려줘, 알림 한 번으로 완료가 됐다.
    #[test]
    fn a_wakeup_after_poison_without_progress_is_not_completion() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let progress: WriteProgress = Arc::new((Mutex::new(0), Condvar::new()));
        let poisoner = Arc::clone(&progress);
        std::thread::spawn(move || {
            let _guard = poisoner.0.lock().expect("아직 성한 락");
            panic!("락을 쥔 채 죽는다");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");

        // 카운터는 그대로 두고 대기자를 계속 깨운다. 대기 중 깨어남이 몇 번이든 결과는 같아야 한다.
        let stop = Arc::new(AtomicBool::new(false));
        let notifier = {
            let progress = Arc::clone(&progress);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    progress.1.notify_all();
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        };

        let timeout = Duration::from_millis(200);
        let started = Instant::now();
        let reached = WriteAck {
            progress: Arc::clone(&progress),
            target: 1,
        }
        .wait(timeout);
        let waited = started.elapsed();
        stop.store(true, Ordering::Relaxed);
        notifier.join().expect("알림 스레드는 패닉하지 않는다");

        assert!(!reached, "카운터가 안 올랐으면 완료가 아니다");
        assert!(
            waited >= timeout,
            "깨어나도 대기 제한까지 기다린다: {waited:?} < {timeout:?}"
        );
    }

    /// poison 뒤에도 writer가 완료 횟수를 올리고 WriteAck가 그 완료를 확인하는지 검사한다.
    #[test]
    fn a_poisoned_write_counter_still_acks_completed_writes() {
        let progress: WriteProgress = Arc::new((Mutex::new(0), Condvar::new()));

        let poisoner = Arc::clone(&progress);
        std::thread::spawn(move || {
            let _guard = poisoner.0.lock().expect("아직 성한 락");
            panic!("이 스레드가 락을 쥔 채 죽는다");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");
        assert!(progress.0.lock().is_err(), "poison 이 실제로 걸려야 한다");

        // writer 스레드는 poison 뒤에도 flush 카운터를 올린다.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let for_writer = Arc::clone(&progress);
        let writer = std::thread::spawn(move || {
            crate::pty::run_writer_loop(Box::new(std::io::sink()), rx, for_writer, None);
        });
        tx.send(b"hello".to_vec())
            .expect("writer 스레드가 살아 있어야 한다");

        // writer의 완료를 기다린다. 실패만으로 러너 지연과 복구 오류를 구분할 수는 없다.
        let ack = WriteAck {
            progress: Arc::clone(&progress),
            target: 1,
        };
        assert!(
            ack.wait(Duration::from_secs(5)),
            "poison 뒤에도 flush 완료가 확인돼야 한다"
        );

        drop(tx);
        writer
            .join()
            .expect("writer 스레드가 패닉 없이 끝나야 한다");

        assert!(
            crate::sink::WRITE_PROGRESS_POISON_REPORTED.load(std::sync::atomic::Ordering::Relaxed),
            "복구했으면 한 번은 보고해야 한다 — 조용한 복구는 조용한 유실과 구분되지 않는다"
        );
    }
}
