// 이유: 테스트의 반환값 무시는 허용하되 제품 코드의 반환값 무시는 계속 검사한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]
mod accessors;
mod attach_stream;
mod binding;
mod color;
mod events;
mod handle;
mod io;
mod modes;
mod mouse_report;
mod output_buffer;
mod pty;
mod resize;
mod screen;
mod scrollback;
mod sink;
mod snapshot;
mod spawn_error;
mod viewport;
mod vte_handler;

pub mod cwd;
pub mod disk_scrollback;
pub mod foreground_process;
pub mod search;
pub mod waker_factory;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak, mpsc};

use termwiz::cell::CellAttributes;
use termwiz::escape::csi::CSI;
use termwiz::escape::parser::Parser;
use termwiz::escape::{Action, ControlCode, Esc, EscCode};
use termwiz::surface::Surface;

pub use attach_stream::{
    ATTACH_STREAM_MAX_BYTES, ATTACH_STREAM_MAX_EVENTS, AttachEvent, AttachEventReceiver,
    AttachStreamSubscription,
};
pub use binding::ResourceGeneration;
pub use color::{ColorPalette, TerminalRgb};
pub use events::*;
pub use io::WriteAck;
pub use mouse_report::encode_mouse_report;
pub use output_buffer::{
    OUTPUT_RETENTION_MAX_BYTES, OutputCursor, OutputRead, OutputReadError, OutputReadRequest,
};
pub use pty::{
    Pty, PtyExit, PtyObservation, PtyPhase, PtyRetirement, PtyState, StandalonePty, pty_drop_totals,
};
pub use scrollback::ScrollbackLine;
pub use spawn_error::{SpawnError, SpawnStage};
pub use viewport::{ContentCut, ContentEpoch, TerminalViewport, ViewportInfo};

/// Configuration for creating a new Terminal.
pub struct TerminalConfig<'a> {
    pub cols: usize,
    pub rows: usize,
    pub shell: Option<&'a str>,
    pub args: &'a [&'a str],
    pub surface_id: u32,
    pub working_dir: Option<&'a std::path::Path>,
    /// 자식 생성 뒤 writer 스레드보다 먼저 PTY에 직접 쓰는 초기 입력.
    /// 줄바꿈 등 제출 문자도 호출자가 포함한다. 셸 초기화가 입력 큐를 비우면
    /// 유실될 수 있으므로 자식이 이 입력을 읽는다는 보장은 없다.
    pub initial_input: Option<&'a str>,
    /// 자식 셸에 추가할 환경변수. ZDOTDIR 등 셸 통합 설정도 포함한다.
    pub extra_env: &'a [(&'a str, &'a str)],
}

/// Information about a single cell for debug inspection.
#[derive(Debug, Clone)]
pub struct CellInfo {
    pub text: String,
    pub fg: String,
    pub bg: String,
    /// Legacy bool kept for backward compat. True iff `intensity == "bold"`.
    pub bold: bool,
    pub italic: bool,
    /// Legacy bool kept for backward compat. True iff `underline_style != "none"`.
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
    pub width: usize,
    /// "normal" | "bold" | "half" (faint/dim, SGR 2)
    pub intensity: &'static str,
    /// "none" | "single" | "double" | "curly" | "dotted" | "dashed"
    pub underline_style: &'static str,
    /// "default" | "palette:N" | "#rrggbb"
    pub underline_color: String,
    /// "none" | "slow" | "rapid"
    pub blink: &'static str,
    pub invisible: bool,
    pub overline: bool,
    /// "baseline" | "super" | "sub"
    pub vertical_align: &'static str,
}

/// A server-side subscriber to a terminal's raw PTY output.
struct OutputTap {
    tx: mpsc::SyncSender<Vec<u8>>,
    /// Consecutive Full sends. Remove the subscriber when this reaches the limit.
    lag: u32,
}

/// Bounded capacity for each output tap channel.
const OUTPUT_TAP_CAP: usize = 1024;
/// Consecutive `Full` sends after which a slow tap is unsubscribed.
const OUTPUT_TAP_LAG_LIMIT: u32 = 64;
/// Resize channel capacity. Full drops that update but retains the subscriber;
/// it does not replace an older queued update with the newest one.
const RESIZE_TAP_CAP: usize = 8;

/// 파서와 메인 스레드가 공유하는 VTE 상태. 파서는 raw 청크마다 락을 얻고 해제한다.
/// 렌더·IPC·리사이즈도 같은 락을 사용하므로 경합할 수 있다.
pub(crate) struct TerminalState {
    connection: binding::ConnectionLease,
    content_revision: u64,
    alternate_epoch: ContentEpoch,
    /// Primary screen buffer.
    pub(crate) primary_surface: Surface,
    /// Alternate screen buffer (lazily created on DECSET 1049/47).
    pub(crate) alternate_surface: Option<Surface>,
    /// Whether the alternate screen is active.
    pub(crate) use_alternate: bool,
    parser: Parser,
    /// Current connection for input/response bytes: the local Pty writer, or the
    /// attach stream of a detached mirror (`set_input_sink`). `None` until wired.
    /// Lives in the shared state because VTE responses (DSR/DA/OSC queries) are
    /// emitted during ingest.
    sink: Option<sink::OutputSink>,
    /// sink에 넣은 횟수. WriteAck가 기다릴 순번을 정한다.
    enqueued_count: u64,
    /// Server-side raw output subscribers. Each tap receives the exact raw PTY
    /// chunks (in apply order) so a remote mirror can replay them. Empty on a
    /// detached terminal and in the common no-subscriber case (zero overhead).
    output_taps: Vec<OutputTap>,
    attach_streams: Vec<attach_stream::AttachStreamTap>,
    /// Server-side resize subscribers. Each tap receives `(cols, rows)` whenever
    /// the grid actually changes so an attached client can keep its mirror grid
    /// in lockstep with the authoritative remote size. Empty in the common
    /// no-subscriber case (zero overhead).
    resize_taps: Vec<mpsc::SyncSender<(usize, usize)>>,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    /// Saved cursor position for ESC 7 / ESC 8
    pub(crate) saved_cursor: Option<(usize, usize)>,
    /// Saved cursor position specifically for alternate screen enter/exit.
    pub(crate) alt_saved_cursor: Option<(usize, usize)>,
    /// Events accumulated during ingest(), consumed via take_events().
    pub(crate) events: Vec<TerminalEvent>,
    /// Raw PTY output buffer behind the read-mark, positional output read and
    /// scan-mark IPC reads; it does not interpret the output itself.
    output: output_buffer::OutputBuffer,
    /// DECCKM: application cursor keys mode.
    pub(crate) application_cursor_keys: bool,
    /// DECTCEM: cursor visibility.
    pub(crate) cursor_visible: bool,
    /// DECSCUSR: cursor shape (block/underline/bar + blink). The renderer reads
    /// this via `cursor_shape()`; storing it does not itself drive a redraw.
    pub(crate) cursor_shape: CursorShape,
    /// Bracketed paste mode (mode 2004).
    pub(crate) bracketed_paste: bool,
    /// 마우스 트래킹 — 1000/1002/1003 **각각의** on/off 비트. 실효 레벨은
    /// `mouse_tracking()` 이 계산해 돌려준다(켜진 것 중 가장 넓은 것).
    pub(crate) mouse_tracking: modes::MouseTrackingRegisters,
    /// 트래킹 `None → ON` 엣지에서 무장되는 "첫 마우스 캡처 안내 toast" 플래그. 호스트가
    /// `take_mouse_capture_hint()` 로 1회 소비(읽고 disarm)한다. 좌·우 클릭 중 먼저 발생한
    /// 캡처 상호작용이 소비해 세션당 1회만 안내된다 (docs/surfaces/terminal/index.md#마우스-입력).
    pub(crate) mouse_capture_hint_armed: bool,
    /// SGR mouse encoding (mode 1006).
    pub(crate) sgr_mouse: bool,
    /// Focus event tracking (mode 1004).
    pub(crate) focus_tracking: bool,
    /// IRM: insert/replace mode (standard mode 4). When true, printed glyphs
    /// shift existing cells right instead of overwriting them.
    pub(crate) insert_mode: bool,
    /// Scroll region top/bottom (1-based inclusive, None = full screen).
    pub(crate) scroll_region: Option<(usize, usize)>,
    /// Whether synchronized output mode (DECSET 2026) is active.
    /// Note: changes are always applied immediately regardless of this flag.
    /// See apply_or_stage_change() for rationale.
    pub(crate) synchronized_output: bool,
    /// Scrollback buffer (memory + optional disk).
    scrollback: scrollback::Scrollback,
    /// CWD cached from OSC 7 (CurrentWorkingDirectory) sequences emitted by the shell.
    /// Used by get_cwd() to avoid spawning external processes.
    pub(crate) cached_cwd: Option<std::path::PathBuf>,
    /// Saved right-side cells for each line, preserved when cols shrink.
    saved_line_tails: Vec<Vec<(String, CellAttributes)>>,
    /// Number of scrollback lines pushed by `handle_rows_shrink` awaiting a
    /// symmetric `handle_rows_grow` to restore them.
    restorable_scrollback_count: usize,
    /// Timestamp of the most recent non-empty PTY output processed.
    last_output_at: std::time::Instant,
    /// 처음으로 비어 있지 않은 출력을 처리한 시각. 출력 이벤트 게이트와 관계없이 기록한다.
    first_output_at: Option<std::time::Instant>,
    /// busy 판정이 쓰는 마지막 활동 출력 시각. 제목만 바꾸는 action 뿐인 출력은 갱신하지 않는다.
    last_activity_at: std::time::Instant,
    /// Timestamp of the most recent user input sent to the PTY.
    last_input_at: std::time::Instant,
    /// Timestamp of the most recent output action that repositioned the cursor
    /// or edited the screen.
    last_screen_control_at: Option<std::time::Instant>,
    /// Whether `OutputAppended` events are pushed during ingest.
    emit_output_events: bool,
    /// Mirror of the active Surface's current pen (text attributes). termwiz's
    /// `Surface` mutates its pen internally but exposes no accessor, so we track
    /// it here to support SGRs that have no `AttributeChange` variant
    /// (Overline/UnderlineColor/VerticalAlign): those are applied by cloning the
    /// pen, setting the one field, and emitting `Change::AllAttributes`. Kept in
    /// sync by `mirror_pen()` on every change. See `vte_handler/control.rs`.
    current_pen: CellAttributes,
    /// Pen mirror of the *inactive* surface while the active one is tracked by
    /// `current_pen`. termwiz keeps an independent pen per surface, so on every
    /// primary↔alternate transition we swap this with `current_pen` to keep the
    /// mirror aligned with the surface that actually receives changes — without
    /// this, an SGR applied on the alt screen would clone the primary's stale pen
    /// (e.g. leftover bold) into `cell_info`. See `swap_pen_for_surface_switch`.
    saved_pen: CellAttributes,
    /// Resolved theme palette plumbed in by the host so OSC 10/11/12/4 color
    /// *queries* report the colors the renderer actually draws. `None` until the
    /// host sets it (a query before plumbing is left unanswered). Refreshed on
    /// terminal creation and on every theme change. See `vte_handler/osc.rs`.
    pub(crate) color_palette: Option<crate::color::ColorPalette>,
    /// Last character printed to the grid, used by REP (CSI b) to repeat it.
    /// `None` until the first print; reset by RIS (full reset).
    pub(crate) last_print: Option<String>,
    /// Horizontal tab stops, indexed by column (`true` = stop at that column).
    /// Initialised to every 8th column; mutated by HTS/TBC, rebuilt to the
    /// default on resize and RIS. HT/CHT/CBT navigate between stops.
    pub(crate) tab_stops: Vec<bool>,
    /// DECSCNM (DEC private mode 5): reverse screen. When set, the renderer
    /// swaps the default foreground/background so the whole screen is inverted.
    /// Reset by RIS. Read by the host via `screen_reverse()`.
    pub(crate) reverse_screen: bool,
    /// DECOM (DEC private mode 6): origin mode. When set, absolute cursor
    /// positioning (CUP/VPA/HVP) is relative to the scroll-region top and the
    /// cursor is confined to the region. Reset by RIS and DECSTR.
    pub(crate) origin_mode: bool,
    /// Current window title (last value emitted via OSC 0/2). Tracked so the
    /// XTWINOPS title stack (CSI 22/23 t) can save and restore it.
    pub(crate) current_title: Option<String>,
    /// Saved window titles for the XTWINOPS title stack (push/pop).
    pub(crate) title_stack: Vec<Option<String>>,
    /// G0 designated as the DEC special line-drawing charset (`ESC ( 0`).
    pub(crate) charset_g0_line_drawing: bool,
    /// G1 designated as the DEC special line-drawing charset (`ESC ) 0`).
    pub(crate) charset_g1_line_drawing: bool,
    /// Whether G1 is currently invoked into GL (SO/`ESC N` selects G1, SI
    /// selects G0). When the active set is line-drawing, printed ASCII in
    /// `0x60..=0x7e` is mapped to box-drawing glyphs.
    pub(crate) charset_active_g1: bool,
}

/// VT content owner. OS child/master and raw workers belong to a separate [`Pty`].
/// The shared state reaches its current input/response sink only through a channel.
/// Grid/mode/scrollback accessors lock the same state the reader ingests into;
/// the GUI input thread does not parse VT.
pub struct Terminal {
    /// Shared VTE state. The Pty reader worker locks this per raw chunk to ingest;
    /// the main thread locks it for render/IPC/resize/event-drain.
    state: Arc<Mutex<TerminalState>>,
    /// Set by the reader worker whenever it ingests a chunk; `process()` swaps it
    /// to false and reports whether anything changed since the last poll.
    dirty: Arc<AtomicBool>,
    /// Last known grid dimensions `(cols, rows)`, mirrored on the handle so
    /// `cols()`/`rows()` and the no-op `resize()` fast path avoid locking the
    /// shared state. The per-frame `resize_all` sweep would otherwise lock every
    /// terminal (including busy background ones) on each redraw (docs/surfaces/terminal/index.md#vte-에뮬레이션).
    cached_dims: (usize, usize),
    /// Handle-side mirror of `TerminalState::emit_output_events`, so the host's
    /// per-wake `set_output_events_enabled` (called on every targeted poll) is a
    /// lock-free no-op when the gate is unchanged — otherwise it would wait on a
    /// busy background terminal's parser lock every wake, re-serializing the input
    /// thread against parsing (docs/surfaces/terminal/index.md#vte-에뮬레이션).
    cached_emit_events: bool,
    /// The reader worker's wake callback, held behind a mutex so it can be
    /// re-targeted after construction. A headless PTY's Terminal is created with
    /// a waker targeting its pty id; when it is promoted to a real Surface
    /// (`pty.attach_surface`, docs/features/headless-pty/index.md#내부-동작-headless-valid) its
    /// store key changes to the new surface_id, so the waker must be
    /// [rewired](Terminal::rewire_waker) to that id — otherwise targeted PTY polling
    /// would keep draining the stale key and the promoted terminal would appear
    /// frozen. A no-op callback for a detached mirror.
    waker: Arc<Mutex<Waker>>,
    /// Foreground PID that the last *observed* busy decision was made for, or
    /// [`BUSY_LATCH_NONE`]. It is the "was busy a moment ago" state of docs/design/policies/busy-indicator.md#판정--해제-두-조건--진입-조건-하나:
    /// while it names the current foreground, user input can no longer push the
    /// terminal back to idle — only the shell regaining the foreground or the
    /// output going quiet can. Keying it by PID means a newly started program
    /// never inherits the previous program's busy. Atomic so the shell-foreground
    /// release path can clear it without taking the parser lock.
    busy_latch: AtomicU64,
}

/// Receiving side of the Pty reader worker: ingests raw chunks into the shared
/// state and wakes the host. Holds only a weak state reference so dropping the
/// terminal stops further ingest.
struct TerminalIngest {
    generation: ResourceGeneration,
    state: Weak<Mutex<TerminalState>>,
    dirty: Arc<AtomicBool>,
    waker: Arc<Mutex<Waker>>,
}

impl TerminalIngest {
    fn wake_now(&self) {
        // Clone the current callback out from under a brief lock and invoke it
        // after releasing, so `rewire_waker` can re-target it without racing.
        let w = tasty_utils::poison::recover_mutex(
            self.waker.lock(),
            WAKER_WHAT,
            &WAKER_POISON_REPORTED,
        )
        .clone();
        w();
    }
}

impl pty::PtyOutput for TerminalIngest {
    fn on_bytes(&self, data: &[u8]) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        let mut state =
            tasty_utils::poison::recover_mutex(state.lock(), STATE_WHAT, &STATE_POISON_REPORTED);
        if state.connection.generation() != self.generation || !state.connection.is_active() {
            return false;
        }
        state.ingest(data);
        drop(state);
        self.dirty.store(true, Ordering::Release);
        self.wake_now();
        true
    }

    fn on_eof(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    fn wake(&self) {
        self.wake_now();
    }

    fn is_attached(&self) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        let state =
            tasty_utils::poison::recover_mutex(state.lock(), STATE_WHAT, &STATE_POISON_REPORTED);
        state.connection.is_active() && state.connection.generation() == self.generation
    }
}

/// [`Terminal::busy_latch`] value meaning "not latched".
pub(crate) const BUSY_LATCH_NONE: u64 = u64::MAX;

/// How long after the last PTY output a terminal still counts as busy.
pub(crate) const BUSY_OUTPUT_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

/// Maximum delay between user input and its PTY echo (echo is not "busy").
pub(crate) const INPUT_ECHO_WINDOW: std::time::Duration = std::time::Duration::from_millis(200);

/// Short renderer-side grace period that hides the focused cursor during
/// program-driven output bursts. This prevents redraw-heavy CLIs from exposing
/// intermediate cursor hops while keeping user input echo visible.
#[cfg(windows)]
pub(crate) const CURSOR_OUTPUT_SUPPRESS_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(120);

/// Default horizontal tab stops: a stop at column 0 and every 8th column.
pub(crate) fn default_tab_stops(cols: usize) -> Vec<bool> {
    (0..cols).map(|c| c % 8 == 0).collect()
}

/// 공유 상태 락의 poison 복구를 프로세스에서 처음 한 번 보고한다.
pub(crate) const STATE_WHAT: &str = "the terminal state";
pub(crate) static STATE_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 렌더 콜백 락의 poison 복구를 처음 한 번 보고한다.
pub(crate) const WAKER_WHAT: &str = "the terminal render waker";
pub(crate) static WAKER_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

impl TerminalState {
    /// Build the PTY-independent VTE state for a fresh terminal.
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            connection: binding::ConnectionLease::new(),
            content_revision: 0,
            primary_surface: Surface::new(cols, rows),
            alternate_surface: None,
            use_alternate: false,
            alternate_epoch: ContentEpoch::fresh(),
            parser: Parser::new(),
            sink: None,
            enqueued_count: 0,
            output_taps: Vec::new(),
            attach_streams: Vec::new(),
            resize_taps: Vec::new(),
            cols,
            rows,
            saved_cursor: None,
            alt_saved_cursor: None,
            events: Vec::new(),
            output: output_buffer::OutputBuffer::new(),
            application_cursor_keys: false,
            cursor_visible: true,
            cursor_shape: CursorShape::default(),
            bracketed_paste: false,
            mouse_tracking: modes::MouseTrackingRegisters::default(),
            mouse_capture_hint_armed: false,
            sgr_mouse: false,
            focus_tracking: false,
            insert_mode: false,
            scroll_region: None,
            synchronized_output: false,
            scrollback: scrollback::Scrollback::new(),
            cached_cwd: None,
            saved_line_tails: Vec::new(),
            restorable_scrollback_count: 0,
            last_output_at: std::time::Instant::now(),
            first_output_at: None,
            last_activity_at: std::time::Instant::now(),
            // Start in the past so the first PTY output is never mistaken for echo.
            last_input_at: std::time::Instant::now() - INPUT_ECHO_WINDOW,
            last_screen_control_at: None,
            emit_output_events: false,
            current_pen: CellAttributes::default(),
            saved_pen: CellAttributes::default(),
            color_palette: None,
            last_print: None,
            tab_stops: default_tab_stops(cols),
            reverse_screen: false,
            origin_mode: false,
            current_title: None,
            title_stack: Vec::new(),
            charset_g0_line_drawing: false,
            charset_g1_line_drawing: false,
            charset_active_g1: false,
        }
    }

    /// Parse a chunk of raw VT bytes and apply it to the surface. Shared by the
    /// Pty reader worker (via [`TerminalIngest`]), [`Terminal::feed_bytes`] (mirror), and
    /// `process_bytes` (test injection) so all three take an identical path.
    /// Returns true if the surface changed.
    pub(crate) fn ingest(&mut self, data: &[u8]) -> bool {
        if !data.is_empty() {
            self.content_revision = self.content_revision.wrapping_add(1);
        }
        if data.is_empty() {
            return false;
        }
        self.output.append(data);
        self.last_output_at = std::time::Instant::now();
        self.first_output_at.get_or_insert(self.last_output_at);
        self.fan_out_to_taps(data);

        let mut changed = false;
        let actions = self.parser.parse_as_vec(data);
        for action in actions {
            // Mode 분기의 continue 보다 앞에서 판정해야 동기화 출력 같은 Mode action 도 활동이 된다.
            if !is_busy_neutral_action(&action) {
                self.last_activity_at = std::time::Instant::now();
            }
            if is_screen_repaint_action(&action) {
                self.last_screen_control_at = Some(std::time::Instant::now());
            }
            // Intercept Mode actions (DECSET/DECRST) -- they affect Terminal
            // state rather than Surface content.
            if let Action::CSI(CSI::Mode(ref mode)) = action {
                self.handle_mode(mode);
                changed = true;
                continue;
            }
            let changes = self.action_to_changes(action);
            if !changes.is_empty() {
                for change in changes {
                    self.apply_or_stage_change(change);
                }
                changed = true;
            }
        }

        if changed {
            self.flush_surface_change_logs();
        }
        self.fan_out_attach_output(data);
        changed
    }

    /// termwiz의 변경 로그를 비운다. Tasty는 diff가 아니라 grid를 읽으므로
    /// 이 로그를 소비하지 않으며, 남겨 두면 출력마다 메모리가 누적된다.
    fn flush_surface_change_logs(&mut self) {
        let seq = self.primary_surface.current_seqno();
        self.primary_surface.flush_changes_older_than(seq);
        if let Some(alt) = &mut self.alternate_surface {
            let seq = alt.current_seqno();
            alt.flush_changes_older_than(seq);
        }
    }

    /// Register a server-side subscriber to this terminal's raw PTY output.
    pub(crate) fn add_output_tap(&mut self) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(OUTPUT_TAP_CAP);
        self.output_taps.push(OutputTap { tx, lag: 0 });
        rx
    }

    /// Serialize the screen and register output and resize taps under one lock, so
    /// no reader worker ingest can land between the snapshot and the taps.
    pub(crate) fn snapshot_and_tap(&mut self) -> AttachSubscription {
        AttachSubscription {
            snapshot: self.snapshot_as_vt(),
            output: self.add_output_tap(),
            resize: self.add_resize_tap(),
        }
    }

    /// Registered output taps, including disconnected ones until the next ingest removes them.
    /// Public to support downstream tests, where dependency cfg(test) items are unavailable.
    pub(crate) fn output_tap_count(&self) -> usize {
        self.output_taps.len()
    }

    /// Register a server-side subscriber to this terminal's grid resizes. The
    /// receiver yields `(cols, rows)` on every actual dimension change.
    pub(crate) fn add_resize_tap(&mut self) -> mpsc::Receiver<(usize, usize)> {
        let (tx, rx) = mpsc::sync_channel::<(usize, usize)>(RESIZE_TAP_CAP);
        self.resize_taps.push(tx);
        rx
    }

    /// Send a resize update. Full channels lose this update; disconnected subscribers are removed.
    fn fan_out_resize(&mut self, cols: usize, rows: usize) {
        if self.resize_taps.is_empty() {
            return;
        }
        self.resize_taps.retain(|tx| {
            !matches!(
                tx.try_send((cols, rows)),
                Err(mpsc::TrySendError::Disconnected(_))
            )
        });
    }

    /// Fan a raw chunk out to all output subscribers without blocking the pump.
    fn fan_out_to_taps(&mut self, data: &[u8]) {
        if self.output_taps.is_empty() {
            return;
        }
        self.output_taps
            .retain_mut(|tap| match tap.tx.try_send(data.to_vec()) {
                Ok(()) => {
                    tap.lag = 0;
                    true
                }
                Err(mpsc::TrySendError::Full(_)) => {
                    tap.lag += 1;
                    tap.lag < OUTPUT_TAP_LAG_LIMIT
                }
                Err(mpsc::TrySendError::Disconnected(_)) => false,
            });
    }
}

/// busy 판정의 활동으로 세지 않는 action. 터미널 제목만 바꾸는 action 과, 어떤 문자열을 ESC \ 로 끝내든
/// 따로 나오는 문자열 종결자(ST)다. 입력을 기다리며 제목만 깜빡이는 프로그램이 busy 로 남지 않게 한다
/// (docs/design/policies/busy-indicator.md). ST 는 혼자서는 화면을 바꾸지 않고, 다른 문자열 시퀀스를
/// 끝낼 때는 그 시퀀스의 action 이 따로 활동으로 세어진다. termwiz 가 action 으로 내지 않는 APC·PM·SOS 는
/// ST 만 남아 활동이 아니다.
fn is_busy_neutral_action(action: &Action) -> bool {
    use termwiz::escape::OperatingSystemCommand as Osc;
    use termwiz::escape::csi::Window;
    match action {
        Action::OperatingSystemCommand(osc) => matches!(
            **osc,
            Osc::SetIconNameAndWindowTitle(_)
                | Osc::SetWindowTitle(_)
                | Osc::SetWindowTitleSun(_)
                | Osc::SetIconName(_)
                | Osc::SetIconNameSun(_)
        ),
        Action::Esc(Esc::Code(EscCode::StringTerminator)) => true,
        Action::CSI(CSI::Window(window)) => matches!(
            **window,
            Window::PushIconAndWindowTitle
                | Window::PushIconTitle
                | Window::PushWindowTitle
                | Window::PopIconAndWindowTitle
                | Window::PopIconTitle
                | Window::PopWindowTitle
        ),
        _ => false,
    }
}

fn is_screen_repaint_action(action: &Action) -> bool {
    matches!(
        action,
        Action::CSI(CSI::Cursor(_) | CSI::Edit(_))
            | Action::Control(ControlCode::CarriageReturn)
            | Action::Esc(Esc::Code(
                EscCode::DecSaveCursorPosition | EscCode::DecRestoreCursorPosition
            ))
    )
}

/// Spawn both independent owners and connect their byte endpoints before the reader starts.
/// The caller must retain both values; dropping Pty terminates its own child.
/// A failure reports whether a child existed; see [`SpawnStage`].
pub fn spawn_terminal(
    config: TerminalConfig<'_>,
    waker: Waker,
) -> Result<(Terminal, Pty), SpawnError> {
    let (mut pty, reader, sink) = Pty::spawn(&config)?;
    let mut terminal = Terminal::new_detached(config.cols, config.rows);
    terminal.waker = Arc::new(Mutex::new(waker));
    {
        let mut state = terminal.lock_state();
        state.connection = pty.connection();
        state.sink = Some(sink);
    }
    pty.start_reader(
        reader,
        TerminalIngest {
            generation: pty.generation(),
            state: Arc::downgrade(&terminal.state),
            dirty: Arc::clone(&terminal.dirty),
            waker: Arc::clone(&terminal.waker),
        },
    )
    .map_err(SpawnError::after_child)?;
    Ok((terminal, pty))
}

impl Terminal {
    /// The connection identity is independent of content reset/alternate-screen epochs.
    pub fn resource_generation(&self) -> ResourceGeneration {
        self.lock_state().connection.generation()
    }

    /// Retire this binding before replacing a store entry. Old readers and sinks cannot
    /// deliver through it even if an observer still temporarily retains old content.
    pub fn disconnect(&mut self) {
        let mut state = self.lock_state();
        state.connection.revoke();
        state.sink = None;
        state.output_taps.clear();
        state.resize_taps.clear();
        state.events.clear();
    }

    /// Queue a resource observation only for the connection which produced it.
    pub fn record_exit(&mut self, generation: ResourceGeneration) -> bool {
        let mut state = self.lock_state();
        if state.connection.generation() != generation || !state.connection.is_active() {
            return false;
        }
        state.events.push(TerminalEvent {
            surface_id: 0,
            generation,
            kind: TerminalEventKind::ProcessExited,
        });
        true
    }

    /// Re-target the reader worker's wake callback. Used when a headless PTY's
    /// Terminal is re-keyed to a new `surface_id` during promotion to a real
    /// Surface (`pty.attach_surface`, docs/features/headless-pty/index.md#내부-동작-headless-valid):
    /// the host installs a waker for
    /// the new id so targeted PTY polling drains the terminal at its new store
    /// key. Detached mirrors have no reader worker, so this is inert for them.
    pub fn rewire_waker(&mut self, waker: Waker) {
        *tasty_utils::poison::recover_mutex(
            self.waker.lock(),
            WAKER_WHAT,
            &WAKER_POISON_REPORTED,
        ) = Arc::clone(&waker);
        waker();
    }

    /// Create a detached mirror terminal with no PTY, child, or threads. Its grid
    /// is reconstructed purely from bytes pushed via [`Terminal::feed_bytes`].
    pub fn new_detached(cols: usize, rows: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(TerminalState::new(cols, rows))),
            dirty: Arc::new(AtomicBool::new(false)),
            cached_dims: (cols, rows),
            cached_emit_events: false,
            // No reader worker — a no-op waker keeps the field total.
            waker: Arc::new(Mutex::new(Arc::new(|| {}))),
            busy_latch: AtomicU64::new(BUSY_LATCH_NONE),
        }
    }

    /// Lock the shared VTE state. Recovers from poisoning (a panicked ingest must
    /// not wedge the whole terminal).
    pub(crate) fn lock_state(&self) -> MutexGuard<'_, TerminalState> {
        tasty_utils::poison::recover_mutex(self.state.lock(), STATE_WHAT, &STATE_POISON_REPORTED)
    }

    /// Run a closure with shared read access to the active surface. The state lock
    /// is held for the closure's duration — keep it short on the render path.
    pub fn with_surface<R>(&self, f: impl FnOnce(&Surface) -> R) -> R {
        let st = self.lock_state();
        f(st.surface())
    }

    /// Report whether the reader ingested content since the last poll. The host
    /// collection separately flushes and observes the paired Pty before draining events.
    pub fn process(&mut self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// Feed externally supplied raw VT bytes into the parser, updating the surface
    /// as if they had arrived from a PTY. Returns true if the surface changed.
    /// Mirror ingestion path: a detached terminal has the caller supply bytes.
    pub fn feed_bytes(&mut self, data: &[u8]) -> bool {
        let changed = self.lock_state().ingest(data);
        if changed {
            self.dirty.store(true, Ordering::Release);
        }
        changed
    }

    /// Register a server-side subscriber to this terminal's raw PTY output.
    pub fn add_output_tap(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.lock_state().add_output_tap()
    }

    /// Take an attach mirror's initial screen and subscribe to later output and
    /// resizes atomically. The Pty reader worker ingests on its own thread, so
    /// separate [`Terminal::snapshot_as_vt`] and tap calls can drop output that
    /// arrives between them.
    pub fn snapshot_and_tap(&mut self) -> AttachSubscription {
        self.lock_state().snapshot_and_tap()
    }

    /// Currently registered output tap count. See
    /// [`TerminalState::output_tap_count`] for why this isn't `#[cfg(test)]`-gated.
    pub fn output_tap_count(&self) -> usize {
        self.lock_state().output_tap_count()
    }

    /// Register a server-side subscriber to this terminal's grid resizes. Yields
    /// `(cols, rows)` on each actual change — used to keep a remote mirror grid
    /// in lockstep with this authoritative terminal.
    pub fn add_resize_tap(&mut self) -> mpsc::Receiver<(usize, usize)> {
        self.lock_state().add_resize_tap()
    }
}

/// An attach mirror's starting point: output ingested before the subscription is
/// in `snapshot`, everything after arrives through `output`.
pub struct AttachSubscription {
    pub snapshot: Vec<u8>,
    pub output: mpsc::Receiver<Vec<u8>>,
    pub resize: mpsc::Receiver<(usize, usize)>,
}

/// Read-only render view over a terminal's shared [`TerminalState`], exposing
/// exactly what the GPU renderer needs while the state lock is held (see
/// [`Terminal::with_view`]).
pub struct TerminalReadView<'a> {
    state: &'a TerminalState,
    viewport: ViewportInfo,
}

impl TerminalReadView<'_> {
    /// Active surface (primary or alternate).
    pub fn surface(&self) -> &Surface {
        self.state.surface()
    }

    /// Active surface dimensions `(cols, rows)`.
    pub fn dimensions(&self) -> (usize, usize) {
        self.state.surface().dimensions()
    }

    /// Whether the cursor is visible (DECTCEM).
    pub fn cursor_visible(&self) -> bool {
        self.state.cursor_visible()
    }

    /// Current cursor shape (DECSCUSR). Defaults to [`CursorShape::Default`].
    pub fn cursor_shape(&self) -> CursorShape {
        self.state.cursor_shape()
    }

    /// Whether reverse-screen mode (DECSCNM) is active. The renderer swaps the
    /// default foreground/background when this is set.
    pub fn screen_reverse(&self) -> bool {
        self.state.screen_reverse()
    }

    /// Current scrollback scroll offset (0 = live bottom).
    pub fn scroll_offset(&self) -> usize {
        self.viewport.scroll_offset()
    }

    /// Retained primary history count, including during alternate display.
    /// Display history length is ContentCut::history_len (zero in alternate).
    pub fn scrollback_len(&self) -> usize {
        self.state.scrollback_len()
    }

    /// Read absolute history rows `range` as [`Self::scrollback_line`] would, one entry per
    /// row. Disk rows in the range share one file open instead of one open per row.
    pub fn scrollback_lines(
        &self,
        range: std::ops::Range<usize>,
    ) -> Vec<Option<std::borrow::Cow<'_, ScrollbackLine>>> {
        let mut out: Vec<_> = range.clone().map(|_| None).collect();
        if self.is_alternate_screen() {
            return out;
        }
        let first = self.viewport.cut.first_row;
        let relative = range.start.saturating_sub(first)..range.end.saturating_sub(first);
        self.state
            .scrollback
            .for_each_line(relative, |index, line| {
                out[index + first - range.start] = Some(line);
            });
        out
    }

    /// Visit absolute history rows `range` oldest first without collecting them. Disk rows
    /// in the range share one file open. Rows that cannot be read are skipped, as
    /// [`Self::scrollback_lines`] leaves them `None`.
    pub fn for_each_scrollback_line(
        &self,
        range: std::ops::Range<usize>,
        mut visit: impl FnMut(usize, &ScrollbackLine),
    ) {
        if self.is_alternate_screen() {
            return;
        }
        let first = self.viewport.cut.first_row;
        let relative = range.start.saturating_sub(first)..range.end.saturating_sub(first);
        self.state
            .scrollback
            .for_each_line(relative, |index, line| visit(index + first, &line));
    }

    /// Read one absolute history row. Memory rows borrow; disk rows load only
    /// the requested line, under the same content cut as the viewport.
    pub fn scrollback_line(&self, index: usize) -> Option<std::borrow::Cow<'_, ScrollbackLine>> {
        if self.is_alternate_screen() {
            return None;
        }
        let relative = index.checked_sub(self.viewport.cut.first_row)?;
        if let Some(line) = self.state.scrollback_line(relative) {
            Some(std::borrow::Cow::Borrowed(line))
        } else {
            self.state
                .scrollback_line_full(relative)
                .map(std::borrow::Cow::Owned)
        }
    }
}

#[cfg(test)]
mod tests;

pub use sink::ExternalInput;
