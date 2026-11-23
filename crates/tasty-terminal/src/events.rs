use std::sync::Arc;

/// Callback to wake the event loop when PTY data arrives.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Events emitted by the terminal during processing.
pub struct TerminalEvent {
    /// The surface ID that generated this event (0 if not yet assigned).
    pub surface_id: u32,
    pub generation: crate::ResourceGeneration,
    pub kind: TerminalEventKind,
    /// 이 사건을 만든 바이트를 같은 parser lock 안에서 살아 있는 attach 스트림이 하나라도 받았다.
    /// 참이면 원격 attach client가 같은 바이트를 받았으므로 그 사용자 쪽 효과는 client가 처리한다.
    pub attach_streamed: bool,
}

/// Types of events a terminal can emit.
pub enum TerminalEventKind {
    /// A notification from OSC 9 / OSC 99 / OSC 777.
    Notification { title: String, body: String },
    /// Bell character received.
    BellRing,
    /// Window title changed via OSC 0 / OSC 2.
    TitleChanged(String),
    /// Current working directory changed via OSC 7.
    CwdChanged(String),
    /// The child process has exited.
    ProcessExited,
    /// Terminal requested clipboard set via OSC 52.
    ClipboardSet(String),
    /// Terminal requested a clipboard read via OSC 52 (`OSC 52 ; c ; ? ST`).
    /// The terminal crate cannot read the system clipboard or know the user's
    /// privacy setting, so it only signals that a query arrived; the host gates
    /// it on `allow_clipboard_read` (default off) and, if allowed, replies with
    /// the base64-encoded clipboard contents.
    ClipboardQuery,
    /// OSC 133 prompt / command boundary marker.
    /// `phase` is one of `'A'` (prompt start), `'B'` (prompt end / cmd start point),
    /// `'C'` (output start), `'D'` (command finished). `payload` is the raw
    /// rest of the sequence (excluding terminator) — e.g. `B;cmd=ls`, `D;0`.
    PromptBoundary { phase: char, payload: String },
    /// Printable text emitted by the terminal. The host accumulates these
    /// chunks into lines (splitting on `\n`) and feeds them to the output
    /// observer router. Control sequences are stripped — only `Print` /
    /// `PrintString` actions and `LineFeed` (as `"\n"`) reach this event.
    OutputAppended { text: String },
}

/// Mouse tracking modes (DECSET 1000/1002/1003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseTrackingMode {
    None,
    Click,      // 1000
    CellMotion, // 1002
    AllMotion,  // 1003
}

/// Cursor shape set via DECSCUSR (`CSI Ps SP q`). Variants mirror the xterm
/// parameter mapping (and termwiz `CursorStyle`): `Default` restores the
/// terminal default (typically a steady block); the others select block /
/// underline / bar with an explicit blink flag. The renderer reads this via the
/// `cursor_shape()` accessor; storing it does not itself drive a redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Default, // 0
    BlinkingBlock,     // 1
    SteadyBlock,       // 2
    BlinkingUnderline, // 3
    SteadyUnderline,   // 4
    BlinkingBar,       // 5
    SteadyBar,         // 6
}
