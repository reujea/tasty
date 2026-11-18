//! 실제 detached Terminal에 바이트를 넣어 VTE 입력·편집·응답을 검사한다.
//! 실제 PTY나 셸 없이 제품의 ingest와 핸들러를 사용한다.
use std::sync::mpsc::{self, Receiver};

use tasty_terminal::{Terminal, TerminalEvent, TerminalEventKind};

/// Thin harness over the production detached terminal. Captures query responses
/// (DSR / DA / cursor position report) via the input sink.
struct Term {
    term: Terminal,
    input_rx: Receiver<Vec<u8>>,
}

impl Term {
    fn new(cols: usize, rows: usize) -> Self {
        let mut term = Terminal::new_detached(cols, rows);
        let (tx, input_rx) = mpsc::channel();
        term.set_input_sink(tx);
        Self { term, input_rx }
    }

    /// Feed raw bytes through the real ingest path.
    fn feed(&mut self, data: &[u8]) {
        self.term.feed_bytes(data);
    }

    /// Feed a string.
    fn feed_str(&mut self, s: &str) {
        self.term.feed_bytes(s.as_bytes());
    }

    /// Text content of a row (0-indexed), trailing spaces trimmed.
    fn row(&self, row: usize) -> String {
        self.term.screen_row(row, true)
    }

    /// All bytes the terminal wrote back to its input sink (query responses).
    fn sent_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        while let Ok(chunk) = self.input_rx.try_recv() {
            out.extend_from_slice(&chunk);
        }
        out
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.term.take_events()
    }

    fn current_title(&self) -> Option<String> {
        self.term.current_title()
    }

    fn is_alternate(&self) -> bool {
        self.term.is_alternate_screen()
    }

    fn bracketed_paste(&self) -> bool {
        self.term.bracketed_paste()
    }

    fn application_cursor_keys(&self) -> bool {
        self.term.application_cursor_keys()
    }

    fn synchronized_output(&self) -> bool {
        self.term.synchronized_output()
    }

    fn synchronized_output_block(&self) -> Option<u64> {
        self.term.synchronized_output_block()
    }
}

#[test]
fn type_hello() {
    let mut t = Term::new(80, 24);
    t.feed_str("hello");
    assert_eq!(t.row(0), "hello");
}

#[test]
fn type_multiple_words() {
    let mut t = Term::new(80, 24);
    t.feed_str("hello world");
    assert_eq!(t.row(0), "hello world");
}

#[test]
fn type_with_newline() {
    let mut t = Term::new(80, 24);
    t.feed_str("line1\r\nline2");
    assert_eq!(t.row(0), "line1");
    assert_eq!(t.row(1), "line2");
}

#[test]
fn carriage_return_overwrites() {
    let mut t = Term::new(80, 24);
    t.feed_str("abcdef\rXY");
    assert_eq!(t.row(0), "XYcdef");
}

#[test]
fn backspace_moves_cursor_left() {
    let mut t = Term::new(80, 24);
    // Shell typically sends: "abc" then BS+space+BS to erase 'c'
    t.feed_str("abc\x08 \x08");
    // BS + 공백 + BS로 마지막 글자를 지우고 커서를 그 칸에 둔다.
    assert_eq!(t.row(0), "ab");
}

#[test]
fn backspace_at_start_of_line_stays() {
    let mut t = Term::new(80, 24);
    t.feed_str("\x08\x08hello");
    // Backspace at position 0 should do nothing
    assert_eq!(t.row(0), "hello");
}

#[test]
fn backspace_shell_erase_pattern() {
    let mut t = Term::new(80, 24);
    t.feed_str("helo\x08 \x08lo");
    assert_eq!(t.row(0), "hello");
}

#[test]
fn multiple_backspace_erase() {
    let mut t = Term::new(80, 24);
    t.feed_str("abcde");
    t.feed_str("\x08 \x08\x08 \x08\x08 \x08");
    assert_eq!(t.row(0), "ab");
}

#[test]
fn cursor_move_right() {
    let mut t = Term::new(80, 24);
    t.feed_str("ab");
    t.feed(b"\x1b[2C"); // move right 2
    t.feed_str("X");
    assert_eq!(t.row(0), "ab  X");
}

#[test]
fn cursor_move_left() {
    let mut t = Term::new(80, 24);
    t.feed_str("abcde");
    t.feed(b"\x1b[3D"); // move left 3
    t.feed_str("X");
    assert_eq!(t.row(0), "abXde");
}

#[test]
fn cursor_absolute_position() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[3;5H"); // row 3, col 5 (1-based)
    t.feed_str("X");
    assert_eq!(t.row(2), "    X"); // row 2 (0-based), col 4 (0-based)
}

#[test]
fn cursor_column_absolute() {
    let mut t = Term::new(80, 24);
    t.feed_str("0123456789");
    t.feed(b"\x1b[6G"); // column 6 (1-based) = index 5
    t.feed_str("X");
    assert_eq!(t.row(0), "01234X6789");
}

#[test]
fn erase_to_end_of_line() {
    let mut t = Term::new(80, 24);
    t.feed_str("hello world");
    t.feed(b"\x1b[6G"); // move to column 6
    t.feed(b"\x1b[K"); // erase to end of line
    assert_eq!(t.row(0), "hello");
}

#[test]
fn erase_entire_display() {
    let mut t = Term::new(80, 24);
    t.feed_str("line1\r\nline2\r\nline3");
    t.feed(b"\x1b[2J"); // erase display
    assert_eq!(t.row(0), "");
    assert_eq!(t.row(1), "");
    assert_eq!(t.row(2), "");
}

#[test]
fn erase_to_end_of_display() {
    let mut t = Term::new(80, 24);
    t.feed_str("line1\r\nline2\r\nline3");
    t.feed(b"\x1b[2;1H"); // go to row 2, col 1
    t.feed(b"\x1b[J"); // erase to end of display
    assert_eq!(t.row(0), "line1");
    assert_eq!(t.row(1), "");
    assert_eq!(t.row(2), "");
}

#[test]
fn overwrite_line() {
    let mut t = Term::new(80, 24);
    t.feed_str("old text\rnew");
    assert_eq!(t.row(0), "new text");
}

#[test]
fn overwrite_with_erase() {
    let mut t = Term::new(80, 24);
    t.feed_str("old text\r\x1b[Knew text");
    assert_eq!(t.row(0), "new text");
}

#[test]
fn sgr_colored_text() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[31mred\x1b[0m normal");
    assert_eq!(t.row(0), "red normal");
}

#[test]
fn sgr_bold_text() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[1mbold\x1b[0m");
    assert_eq!(t.row(0), "bold");
}

#[test]
fn alternate_screen_switch() {
    let mut t = Term::new(80, 24);
    t.feed_str("main screen");
    assert_eq!(t.row(0), "main screen");

    // Enter alternate screen
    t.feed(b"\x1b[?1049h");
    assert!(t.is_alternate());
    assert_eq!(t.row(0), ""); // alternate is empty

    t.feed_str("alt screen");
    assert_eq!(t.row(0), "alt screen");

    // Leave alternate screen
    t.feed(b"\x1b[?1049l");
    assert!(!t.is_alternate());
    assert_eq!(t.row(0), "main screen"); // original content restored
}

#[test]
fn bracketed_paste_mode() {
    let mut t = Term::new(80, 24);
    assert!(!t.bracketed_paste());

    t.feed(b"\x1b[?2004h"); // enable
    assert!(t.bracketed_paste());

    t.feed(b"\x1b[?2004l"); // disable
    assert!(!t.bracketed_paste());
}

#[test]
fn application_cursor_keys_mode() {
    let mut t = Term::new(80, 24);
    assert!(!t.application_cursor_keys());

    t.feed(b"\x1b[?1h"); // enable DECCKM
    assert!(t.application_cursor_keys());

    t.feed(b"\x1b[?1l"); // disable
    assert!(!t.application_cursor_keys());
}

#[test]
fn synchronized_output_applies_immediately() {
    let mut t = Term::new(80, 24);

    // Changes are applied immediately even during sync output,
    // so cursor_position() stays current for VTE operations that
    // depend on it (EraseLine, DeleteLine, etc.).
    t.feed(b"\x1b[?2026habc\rXY");
    assert_eq!(t.row(0), "XYc");
    assert!(t.synchronized_output());

    t.feed(b"\x1b[?2026l");
    assert_eq!(t.row(0), "XYc");
    assert!(!t.synchronized_output());
}

#[test]
fn synchronized_output_block_changes_when_one_batch_ends_and_begins_a_block() {
    let mut t = Term::new(80, 24);
    assert_eq!(t.synchronized_output_block(), None);

    t.feed(b"\x1b[?2026ha");
    let first = t.synchronized_output_block();
    assert!(first.is_some());
    // Setting the mode again inside a block does not start a new block.
    t.feed(b"\x1b[?2026hb");
    assert_eq!(t.synchronized_output_block(), first);

    // One batch ends the block and begins the next: the mode reads on before and after.
    t.feed(b"\x1b[?2026l\x1b[?2026hc");
    let second = t.synchronized_output_block();
    assert!(second.is_some());
    assert_ne!(second, first);

    t.feed(b"\x1b[?2026l");
    assert_eq!(t.synchronized_output_block(), None);
}

#[test]
fn cursor_position_report_responds_with_one_based_coordinates() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[3;5H");
    t.feed(b"\x1b[6n");
    assert_eq!(String::from_utf8_lossy(&t.sent_bytes()), "\x1b[3;5R");
}

#[test]
fn status_report_returns_terminal_ok() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[5n");
    assert_eq!(String::from_utf8_lossy(&t.sent_bytes()), "\x1b[0n");
}

#[test]
fn osc_set_window_title_emits_event() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b]2;hi\x07"); // OSC 2 — set window title
    let events = t.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.kind, TerminalEventKind::TitleChanged(s) if s == "hi")),
        "expected TitleChanged(\"hi\") from production OSC handler, got none"
    );
}

/// 헬퍼: feed 후 TitleChanged 이벤트 존재 여부.
fn has_title_changed(events: &[TerminalEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(&e.kind, TerminalEventKind::TitleChanged(_)))
}

// ConPTY 가 spawn 시 세팅하는 셸 exe 경로 제목(혼합 슬래시)은 소스에서 무시 —
// current_title 세팅도, TitleChanged 발화도 없어야 한다 (직접 투영 경로 차단).
#[test]
fn osc_title_shell_exe_path_is_ignored() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b]0;C:\\Program Files/Git/bin/bash.exe\x07");
    assert_eq!(t.current_title(), None);
    assert!(
        !has_title_changed(&t.take_events()),
        "shell exe path title must not emit TitleChanged"
    );
}

// Unix 형태 셸 절대경로도 동일하게 무시된다 (경로 형태 + known shell basename).
#[test]
fn osc_title_unix_shell_path_is_ignored() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b]0;/usr/bin/bash\x07");
    assert_eq!(t.current_title(), None);
    assert!(!has_title_changed(&t.take_events()));
}

// 경로 형태가 아닌 일반 제목은 정상 세팅 + 이벤트 발화 (회귀 방지).
#[test]
fn osc_title_non_path_title_still_sets() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b]0;my project\x07");
    assert_eq!(t.current_title(), Some("my project".to_string()));
    assert!(has_title_changed(&t.take_events()));
}

// bare "bash" (구분자 없음) 는 필터하지 않는다 — 오탐 방지.
#[test]
fn osc_title_bare_shell_name_still_sets() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b]0;bash\x07");
    assert_eq!(t.current_title(), Some("bash".to_string()));
    assert!(has_title_changed(&t.take_events()));
}

// basename 이 셸이 아닌 경로형 제목 (예: "user@host: ~/dev") 은 통과한다.
#[test]
fn osc_title_pathlike_non_shell_basename_still_sets() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b]0;user@host: ~/dev\x07");
    assert_eq!(t.current_title(), Some("user@host: ~/dev".to_string()));
    assert!(has_title_changed(&t.take_events()));
}

#[test]
fn full_reset() {
    let mut t = Term::new(80, 24);
    t.feed_str("some text");
    t.feed(b"\x1b[?1h"); // enable DECCKM
    t.feed(b"\x1b[?2004h"); // enable bracketed paste
    t.feed(b"\x1bc"); // RIS (full reset)

    assert!(!t.application_cursor_keys());
    assert!(!t.bracketed_paste());
    assert_eq!(t.row(0), ""); // screen cleared
}

#[test]
fn full_reset_ends_synchronized_output() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[?2026h");
    let before = t.synchronized_output_block();
    t.feed(b"\x1bc"); // RIS (full reset)
    assert!(!t.synchronized_output());
    assert_eq!(t.synchronized_output_block(), None);

    // A block entered after the reset reads as a new block.
    t.feed(b"\x1b[?2026h");
    assert!(t.synchronized_output_block().is_some());
    assert_ne!(t.synchronized_output_block(), before);
}

#[test]
fn line_wrapping() {
    let mut t = Term::new(10, 24);
    t.feed_str("0123456789wrap");
    assert_eq!(t.row(0), "0123456789");
    assert_eq!(t.row(1), "wrap");
}

#[test]
fn tab_character() {
    let mut t = Term::new(80, 24);
    t.feed_str("a\tb");
    let row = t.row(0);
    assert!(row.starts_with("a"));
    assert!(row.contains("b"));
}

#[test]
fn empty_input() {
    let mut t = Term::new(80, 24);
    t.feed(b"");
    assert_eq!(t.row(0), "");
}

#[test]
fn only_newlines() {
    let mut t = Term::new(80, 24);
    t.feed_str("\r\n\r\n\r\n");
    assert_eq!(t.row(0), "");
    assert_eq!(t.row(1), "");
    assert_eq!(t.row(2), "");
}

#[test]
fn cursor_up_from_top() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[10A"); // move up 10 from row 0 — should clamp
    t.feed_str("X");
    assert_eq!(t.row(0), "X"); // still on row 0
}

#[test]
fn unicode_text() {
    let mut t = Term::new(80, 24);
    t.feed_str("한글 테스트");
    let row = t.row(0);
    assert!(row.contains("한글"));
    assert!(row.contains("테스트"));
}

#[test]
fn mixed_ascii_and_escape() {
    let mut t = Term::new(80, 24);
    t.feed(b"\x1b[32m$ \x1b[0mhello");
    assert_eq!(t.row(0), "$ hello");
}

#[test]
fn decstr_resets_sgr_to_default() {
    let mut t = Term::new(80, 24);
    t.feed_str("\x1b[1m"); // bold on
    t.feed_str("\x1b[!p"); // DECSTR
    t.feed_str("X");
    let info = t.term.cell_info(0, 0).unwrap();
    assert!(!info.bold, "SGR should be reset after DECSTR");
}

#[test]
fn decstr_resets_cursor_visibility_and_app_keys() {
    let mut t = Term::new(80, 24);
    t.feed_str("\x1b[?1h"); // DECCKM: application cursor keys on
    t.feed_str("\x1b[?25l"); // DECTCEM: hide cursor
    assert!(t.application_cursor_keys());
    assert!(!t.term.cursor_visible());
    t.feed_str("\x1b[!p"); // DECSTR
    assert!(
        !t.application_cursor_keys(),
        "application cursor keys reset"
    );
    assert!(t.term.cursor_visible(), "cursor visibility restored");
}

#[test]
fn decstr_preserves_screen_content() {
    let mut t = Term::new(80, 24);
    t.feed_str("keep me");
    t.feed_str("\x1b[!p"); // DECSTR must NOT clear the screen
    assert_eq!(t.row(0), "keep me");
}

#[test]
fn decstr_clears_saved_cursor() {
    let mut t = Term::new(80, 24);
    t.feed_str("\x1b[3;6H"); // move to row 3, col 6 (1-indexed)
    t.feed_str("\x1b7"); // DECSC: save cursor
    t.feed_str("\x1b[!p"); // DECSTR clears the saved cursor
    t.feed_str("\x1b[H"); // home → (0, 0)
    t.feed_str("Z"); // writes at (0, 0), cursor → (1, 0)
    t.feed_str("\x1b8"); // DECRC: no-op since saved cursor cleared
    t.feed_str("Q"); // writes at (1, 0)
    assert_eq!(t.row(0), "ZQ");
}

#[test]
fn decstr_resets_insert_mode() {
    let mut t = Term::new(80, 24);
    t.feed_str("ABCDE");
    t.feed_str("\r"); // cursor → col 0
    t.feed_str("\x1b[4h"); // IRM: insert mode on
    t.feed_str("\x1b[!p"); // DECSTR resets insert mode
    t.feed_str("XY"); // replace mode → overwrites
    assert_eq!(t.row(0), "XYCDE");
}

#[test]
fn decstr_resets_scroll_region() {
    let mut t = Term::new(80, 3);
    t.feed_str("\x1b[2;3r"); // DECSTBM: scroll region rows 2-3
    t.feed_str("\x1b[!p"); // DECSTR resets region to full screen
    t.feed_str("\x1b[H"); // cursor home
    t.feed_str("L0\r\nL1\r\nL2\r\n"); // bottom newline scrolls full screen
    // Full-screen scroll: L0 evicted, rows shift up.
    assert_eq!(t.row(0), "L1");
    assert_eq!(t.row(1), "L2");
}
