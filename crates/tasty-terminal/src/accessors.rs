//! Surface / cols / rows / process info / mark accessors.
//!
//! Grid/VTE 상태를 읽는 접근자는 `impl TerminalState` (락 안에서 동작), child/PTY
//! 를 만지는 접근자는 `impl Terminal` (핸들). 핸들 쪽 메서드는 필요 시 짧게 락을
//! 잡아 상태 필드를 읽는다 (ADR-0002).

use std::sync::atomic::Ordering;

use termwiz::surface::Surface;

#[cfg(windows)]
use crate::CURSOR_OUTPUT_SUPPRESS_WINDOW;
use crate::{
    BUSY_LATCH_NONE, BUSY_OUTPUT_WINDOW, INPUT_ECHO_WINDOW, Terminal, TerminalEvent, TerminalState,
    cwd, foreground_process,
};

impl TerminalState {
    pub(crate) fn surface(&self) -> &Surface {
        if self.use_alternate {
            self.alternate_surface
                .as_ref()
                .unwrap_or(&self.primary_surface)
        } else {
            &self.primary_surface
        }
    }

    pub(crate) fn surface_mut(&mut self) -> &mut Surface {
        if self.use_alternate {
            self.alternate_surface
                .as_mut()
                .unwrap_or(&mut self.primary_surface)
        } else {
            &mut self.primary_surface
        }
    }

    /// Take all accumulated events, leaving the internal buffer empty.
    pub(crate) fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn set_output_events_enabled(&mut self, enabled: bool) {
        self.emit_output_events = enabled;
    }

    pub(crate) fn set_mark(&mut self) {
        self.output.set_mark();
    }

    pub(crate) fn renew_output_stream(&mut self) {
        self.output.renew_stream();
    }

    pub(crate) fn take_since_output_scan_mark(&mut self, strip_ansi: bool) -> String {
        self.output.take_since_scan_mark(strip_ansi)
    }

    pub(crate) fn read_since_mark(&self, strip_ansi: bool) -> String {
        self.output.read_since_mark(strip_ansi)
    }

    pub(crate) fn read_output(
        &self,
        req: &crate::OutputReadRequest,
    ) -> Result<crate::OutputRead, crate::OutputReadError> {
        self.output.read(req)
    }

    pub(crate) fn set_cached_cwd(&mut self, cwd: std::path::PathBuf) {
        self.cached_cwd = Some(cwd);
    }
}

impl Terminal {
    /// Grid columns. Served from the handle-side cache (lock-free) — kept in sync
    /// by `resize()`.
    pub fn cols(&self) -> usize {
        self.cached_dims.0
    }

    /// Grid rows. Served from the handle-side cache (lock-free).
    pub fn rows(&self) -> usize {
        self.cached_dims.1
    }

    /// Get the PID of the child process. `None` for a detached mirror (no child)
    /// or after [`take_child`](Self::take_child) hands the child off.
    pub fn process_id(&self) -> Option<u32> {
        self.pty.as_ref()?.child.as_ref()?.process_id()
    }

    /// Whether this terminal is a detached mirror (no PTY/child). Its grid is
    /// authoritative from the remote handshake/resize and must NOT be overwritten
    /// by the local layout resize sweep — the local resize path skips these.
    pub fn is_detached(&self) -> bool {
        self.pty.is_none()
    }

    /// Get the foreground process info (name, PID) for this terminal.
    pub fn foreground_process_info(&self) -> Option<foreground_process::ForegroundProcessInfo> {
        let shell_pid = self.process_id()?;
        foreground_process::get_foreground_process(shell_pid)
    }

    /// Whether the terminal is currently considered "active" — a non-shell
    /// foreground program is running AND the PTY produced output within the last
    /// `BUSY_OUTPUT_WINDOW`. Output within `INPUT_ECHO_WINDOW` after the last
    /// keystroke is treated as echo, but only blocks *entering* busy; a terminal
    /// that was already busy stays busy while the user types (ADR-0261).
    pub fn is_busy(&self) -> bool {
        let Some(shell_pid) = self.process_id() else {
            return false;
        };
        let info = foreground_process::get_foreground_process(shell_pid);
        self.busy_with_foreground(shell_pid, info.as_ref())
    }

    /// Same busy decision as [`is_busy`](Self::is_busy), but the (expensive on
    /// Windows) foreground lookup is supplied by the caller. The batch poll in
    /// `refresh_busy_surfaces` resolves every surface's foreground from a single
    /// system snapshot and then calls this per terminal, turning a per-surface
    /// snapshot into one snapshot per tick. `shell_pid` must be this terminal's
    /// own child PID and `foreground` the result of resolving it.
    ///
    /// The decision carries state (`busy_latch`), in the order ADR-0261 fixes:
    /// the shell holding the foreground and the output going quiet release first
    /// and clear the latch; input echo is consulted last and only when the latch
    /// does not name the current foreground. Asking twice in a row returns the
    /// same answer — the latch is only set when the answer is already `true`.
    pub fn busy_with_foreground(
        &self,
        shell_pid: u32,
        foreground: Option<&foreground_process::ForegroundProcessInfo>,
    ) -> bool {
        let Some(info) = foreground else {
            self.clear_busy_latch();
            return false;
        };
        if info.pid == shell_pid || foreground_process::is_known_shell_name(&info.name) {
            self.clear_busy_latch();
            return false;
        }
        // Non-blocking: `refresh_busy_surfaces` polls every terminal at 1Hz, and a
        // blocking lock here would wait on each busy parser thread mid-ingest,
        // spiking the input thread's tail latency (ADR-0002). A contended lock
        // means the parser is actively ingesting output → that is "busy". That
        // `true` is a guess, not an observation, so it neither sets nor clears
        // the latch (ADR-0261).
        let st = match self.state.try_lock() {
            Ok(st) => st,
            Err(std::sync::TryLockError::WouldBlock) => return true,
            Err(std::sync::TryLockError::Poisoned(p)) => tasty_utils::poison::recover_poisoned(
                p,
                crate::STATE_WHAT,
                &crate::STATE_POISON_REPORTED,
            ),
        };
        if st.last_output_at.elapsed() >= BUSY_OUTPUT_WINDOW {
            self.clear_busy_latch();
            return false;
        }
        let latch = u64::from(info.pid);
        if self.busy_latch.load(Ordering::Relaxed) != latch {
            // Echo must come *after* the input it echoes: output that predates the
            // last keystroke is never suppressed.
            let is_echo = st.last_input_at <= st.last_output_at
                && st.last_output_at <= st.last_input_at + INPUT_ECHO_WINDOW;
            if is_echo {
                self.clear_busy_latch();
                return false;
            }
        }
        self.busy_latch.store(latch, Ordering::Relaxed);
        true
    }

    fn clear_busy_latch(&self) {
        self.busy_latch.store(BUSY_LATCH_NONE, Ordering::Relaxed);
    }

    /// PTY 가 마지막으로 non-empty 출력을 낸 시각. `IdleTimeout` 훅의 idle
    /// 경과시간 계산에 쓰인다. 논블로킹(ADR-0002) — 락이 막혀 있으면 파서가
    /// 한창 ingest 중이라는 뜻이므로 "지금 막 활동 중"으로 간주해
    /// `Instant::now()` 를 반환한다(`busy_with_foreground` 의 WouldBlock=busy
    /// 처리와 동형).
    pub fn last_output_at(&self) -> std::time::Instant {
        match self.state.try_lock() {
            Ok(st) => st.last_output_at,
            Err(std::sync::TryLockError::WouldBlock) => std::time::Instant::now(),
            Err(std::sync::TryLockError::Poisoned(p)) => {
                tasty_utils::poison::recover_poisoned(
                    p,
                    crate::STATE_WHAT,
                    &crate::STATE_POISON_REPORTED,
                )
                .last_output_at
            }
        }
    }

    /// Whether the renderer should temporarily hide the focused text cursor
    /// while a program is repainting terminal output. This suppresses visible
    /// intermediate cursor hops from redraw-heavy TUIs/CLIs, but leaves plain
    /// user-input echo visible because printable output alone is not a
    /// screen-control action.
    pub fn should_suppress_cursor_during_output(&self) -> bool {
        #[cfg(not(windows))]
        {
            false
        }
        #[cfg(windows)]
        {
            match self.state.try_lock() {
                Ok(st) => st.should_suppress_cursor_during_output(),
                // Parser holds the lock while ingesting output, so this is exactly
                // the burst window where drawing an intermediate cursor is noisy.
                Err(std::sync::TryLockError::WouldBlock) => true,
                Err(std::sync::TryLockError::Poisoned(p)) => tasty_utils::poison::recover_poisoned(
                    p,
                    crate::STATE_WHAT,
                    &crate::STATE_POISON_REPORTED,
                )
                .should_suppress_cursor_during_output(),
            }
        }
    }

    /// Get the current working directory of the child process. Prefers the CWD
    /// cached from OSC 7; falls back to an OS-level query (not cached).
    pub fn get_cwd(&self) -> Option<std::path::PathBuf> {
        if let Some(cwd) = self.lock_state().cached_cwd.clone() {
            return Some(cwd);
        }
        if let Some(pid) = self.process_id() {
            return cwd::get_cwd_of_pid(pid);
        }
        None
    }

    /// Set the cached CWD. Used by the OS-level CWD polling mechanism.
    pub fn set_cached_cwd(&mut self, cwd: std::path::PathBuf) {
        self.lock_state().set_cached_cwd(cwd);
    }

    /// Current window title (last value emitted via OSC 0/2), if any. The host
    /// projects the focused surface's title onto its tab name — mirrors the
    /// `get_cwd` lock pattern (short lock to clone the field).
    pub fn current_title(&self) -> Option<String> {
        self.lock_state().current_title.clone()
    }

    /// Check if the child process is still running. A detached mirror has no
    /// child; reported as alive.
    pub fn is_alive(&mut self) -> bool {
        match self.pty.as_mut().and_then(|pty| pty.child.as_mut()) {
            Some(child) => child.try_wait().ok().flatten().is_none(),
            // 자식 없음: detached mirror 이거나 take_child 로 자식이 이관됨 — alive 로 본다.
            None => true,
        }
    }

    /// Check if the child process has exited. Returns false if exited. A detached
    /// mirror (no child) is always considered alive.
    pub fn check_process_alive(&mut self) -> bool {
        match self.pty.as_mut().and_then(|pty| pty.child.as_mut()) {
            Some(child) => !matches!(child.try_wait(), Ok(Some(_status))),
            // 자식 없음: detached mirror 이거나 take_child 로 이관됨 — alive 로 본다.
            None => true,
        }
    }

    /// Hand off ownership of the waitable child process so an external owner (the
    /// headless `pty_registry` exit-watcher, ADR-0050) can call `child.wait()` for a
    /// real exit code. After this the terminal's own exit-detection
    /// ([`check_process_alive`](Self::check_process_alive)) and Drop-time kill/reap
    /// no longer apply to that child — the new owner is responsible for kill/reap.
    ///
    /// Returns `None` if already taken or if this is a detached mirror with no PTY.
    /// Surface terminals never call this, so their child stays `Some` and their
    /// lifecycle (Drop-kill, zombie reaping) is unchanged.
    pub fn take_child(&mut self) -> Option<Box<dyn portable_pty::Child + Send + Sync>> {
        // The new owner reaps the exit; the parser thread's post-EOF wakes, which
        // exist to drive this handle's own exit check, would only spin.
        self.exit_settled.store(true, Ordering::Release);
        self.pty.as_mut().and_then(|pty| pty.child.take())
    }

    /// Take all accumulated events, leaving the internal buffer empty.
    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.lock_state().take_events()
    }

    /// Like [`take_events`](Self::take_events) but never blocks: if the parser
    /// thread currently holds the state lock (mid-chunk ingest), returns `None`
    /// and leaves the events buffered for a later poll. The host's per-wake event
    /// drain iterates *every* terminal, so a blocking take would re-serialize the
    /// input thread against all busy parser threads — defeating ADR-0002. Events
    /// are never lost: the parser wakes the loop again after each ingest.
    pub fn try_take_events(&mut self) -> Option<Vec<TerminalEvent>> {
        match self.state.try_lock() {
            Ok(mut st) => Some(st.take_events()),
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(p)) => Some(
                tasty_utils::poison::recover_poisoned(
                    p,
                    crate::STATE_WHAT,
                    &crate::STATE_POISON_REPORTED,
                )
                .take_events(),
            ),
        }
    }

    /// Enable/disable `OutputAppended` emission. Defaults to off. Lock-free no-op
    /// when the gate is unchanged (the common per-wake case).
    pub fn set_output_events_enabled(&mut self, enabled: bool) {
        if self.cached_emit_events == enabled {
            return;
        }
        self.cached_emit_events = enabled;
        self.lock_state().set_output_events_enabled(enabled);
    }

    /// Set a read mark at the current end of the output buffer.
    pub fn set_mark(&mut self) {
        self.lock_state().set_mark();
    }

    /// Return the output accumulated since the previous scan-cursor read and
    /// advance that cursor past it. Serves `surface.read_since_scan_mark`; see
    /// `OutputBuffer::take_since_scan_mark` for why the two steps are one call.
    pub fn take_since_output_scan_mark(&mut self, strip_ansi: bool) -> String {
        self.lock_state().take_since_output_scan_mark(strip_ansi)
    }

    /// Read output since the last mark. If no mark was set, reads from the beginning.
    pub fn read_since_mark(&self, strip_ansi: bool) -> String {
        self.lock_state().read_since_mark(strip_ansi)
    }

    /// Give this terminal's output stream a new token, so a position taken
    /// before this point reads as a different stream. A mirror calls this where
    /// the bytes it replays stop being continuous; see
    /// `OutputBuffer::renew_stream`.
    pub fn renew_output_stream(&mut self) {
        self.lock_state().renew_output_stream();
    }

    /// Answer a read from a position, saying where the retained window is, what
    /// fell out of it and where to continue. Serves the cursor form of
    /// `surface.read_since_mark`; the contract is on
    /// [`crate::OutputRead`].
    pub fn read_output(
        &self,
        req: &crate::OutputReadRequest,
    ) -> Result<crate::OutputRead, crate::OutputReadError> {
        self.lock_state().read_output(req)
    }
}

#[cfg(windows)]
impl TerminalState {
    fn should_suppress_cursor_during_output(&self) -> bool {
        self.last_screen_control_at
            .is_some_and(|at| at.elapsed() < CURSOR_OUTPUT_SUPPRESS_WINDOW)
    }
}
