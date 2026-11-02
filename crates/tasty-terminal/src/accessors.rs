//! Surface / cols / rows / process info / mark accessors.
//!
//! Grid/VTE 상태를 읽는 접근자는 `impl TerminalState` (락 안에서 동작), child/PTY
//! 를 만지는 접근자는 `impl Terminal` (핸들). 핸들 쪽 메서드는 필요 시 짧게 락을
//! 잡아 상태 필드를 읽는다 (docs/surfaces/terminal/index.md#vte-에뮬레이션).

use std::sync::atomic::Ordering;

use termwiz::surface::Surface;

#[cfg(windows)]
use crate::CURSOR_OUTPUT_SUPPRESS_WINDOW;
use crate::{
    BUSY_LATCH_NONE, BUSY_OUTPUT_WINDOW, INPUT_ECHO_WINDOW, Terminal, TerminalEvent, TerminalState,
    foreground_process,
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

    /// is_busy와 같은 판정에 호출자가 조회한 foreground를 사용한다.
    /// shell_pid는 이 터미널의 자식 PID여야 하며 foreground는 그 PID의 조회 결과여야 한다.
    /// 셸 복귀나 출력 중단은 busy를 해제하고, 입력 에코는 새 busy 진입만 막는다.
    /// 이미 busy인 프로세스는 입력만으로 idle로 바뀌지 않는다.
    pub fn busy_with_foreground(
        &mut self,
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
        // 렌더 스레드를 기다리게 하지 않도록 락이 사용 중이면 busy로 추정한다.
        // 직접 관측한 결과가 아니므로 기존 latch는 바꾸지 않는다.
        let st = match self.state.try_lock() {
            Ok(st) => st,
            Err(std::sync::TryLockError::WouldBlock) => return true,
            Err(std::sync::TryLockError::Poisoned(p)) => tasty_utils::poison::recover_poisoned(
                p,
                crate::STATE_WHAT,
                &crate::STATE_POISON_REPORTED,
            ),
        };
        if st.last_activity_at.elapsed() >= BUSY_OUTPUT_WINDOW {
            self.clear_busy_latch();
            return false;
        }
        let latch = u64::from(info.pid);
        if self.busy_latch.load(Ordering::Relaxed) != latch {
            // Echo must come *after* the input it echoes: output that predates the
            // last keystroke is never suppressed.
            let is_echo = st.last_input_at <= st.last_activity_at
                && st.last_activity_at <= st.last_input_at + INPUT_ECHO_WINDOW;
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

    /// 마지막으로 처리한 비어 있지 않은 출력 시각. 락이 사용 중이면 최근 활동으로
    /// 간주해 현재 시각을 반환한다. IdleTimeout 계산에 사용한다.
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

    /// 처음으로 출력을 처리한 시각. 출력이 없었거나 락이 사용 중이면 None을 반환하므로
    /// 호출자는 다음 확인 때 다시 묻는다. `OutputAppended` 게이트와 관계없이 기록된다.
    pub fn first_output_at(&self) -> Option<std::time::Instant> {
        match self.state.try_lock() {
            Ok(st) => st.first_output_at,
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(p)) => {
                tasty_utils::poison::recover_poisoned(
                    p,
                    crate::STATE_WHAT,
                    &crate::STATE_POISON_REPORTED,
                )
                .first_output_at
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
                // 락 경합 중에는 커서의 중간 위치를 그리지 않는다.
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

    /// CWD reported by OSC 7. OS fallback is resolved by the separate Pty owner.
    pub fn cached_cwd(&self) -> Option<std::path::PathBuf> {
        self.lock_state().cached_cwd.clone()
    }

    /// Set the cached CWD. Used by the OS-level CWD polling mechanism.
    pub fn set_cached_cwd(&mut self, cwd: std::path::PathBuf) {
        self.lock_state().set_cached_cwd(cwd);
    }

    /// Last OSC 0/2 window title. The host uses the focused surface's title as its tab name.
    pub fn current_title(&self) -> Option<String> {
        self.lock_state().current_title.clone()
    }

    /// Take all accumulated events, leaving the internal buffer empty.
    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.lock_state().take_events()
    }

    /// Take buffered events without waiting for the state lock. On contention, return None
    /// and leave them for a later poll. Parsing wakes the loop after each ingest.
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
