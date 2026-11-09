//! `Terminal` 단위 테스트.

use super::*;
use crate::pty::ALIVE_CHECK_INTERVAL;
use std::sync::Arc;
use termwiz::escape::csi::CSI;
use termwiz::escape::parser::Parser;

fn noop_waker() -> Waker {
    Arc::new(|| {})
}

fn test_terminal(cols: usize, rows: usize) -> Terminal {
    Terminal::new_detached(cols, rows)
}

/// Windows는 Git Bash만 실행한다. 다른 OS는 기본 셸(None)에 맡긴다.
const TEST_SHELL: Option<&str> = if cfg!(windows) {
    Some("C:/Program Files/Git/bin/bash.exe")
} else {
    None
};

fn test_pty_terminal(cols: usize, rows: usize) -> (Terminal, Pty) {
    spawn_terminal(
        TerminalConfig {
            cols,
            rows,
            shell: TEST_SHELL,
            args: &[],
            surface_id: 0,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        noop_waker(),
    )
    .expect("terminal/PTY creation")
}

fn process_pair(terminal: &mut Terminal, pty: &mut Pty) -> bool {
    pty.force_flush_resize();
    if pty.observe_exit() {
        terminal.record_exit(pty.generation());
    }
    terminal.process()
}

#[test]
#[cfg(windows)]
fn cursor_suppression_hides_program_output_burst() {
    let mut terminal = Terminal::new_detached(80, 24);
    terminal.process_bytes(b"\x1b[2K\r");

    assert!(terminal.should_suppress_cursor_during_output());
}

#[test]
#[cfg(windows)]
fn cursor_suppression_ignores_recent_input_echo() {
    let mut terminal = Terminal::new_detached(80, 24);
    {
        let mut st = terminal.lock_state();
        st.last_input_at = std::time::Instant::now();
    }
    terminal.process_bytes(b"x");

    assert!(!terminal.should_suppress_cursor_during_output());
}

#[test]
#[cfg(windows)]
fn cursor_suppression_detects_repaint_immediately_after_input() {
    let mut terminal = Terminal::new_detached(80, 24);
    {
        let mut st = terminal.lock_state();
        st.last_input_at = std::time::Instant::now();
    }
    terminal.process_bytes(b"\x1b[3D\x1b[2K");

    assert!(terminal.should_suppress_cursor_during_output());
}

#[test]
#[cfg(windows)]
fn cursor_suppression_expires_after_output_quiets() {
    let mut terminal = Terminal::new_detached(80, 24);
    terminal.process_bytes(b"\r");
    {
        let mut st = terminal.lock_state();
        st.last_screen_control_at = Some(
            std::time::Instant::now()
                - CURSOR_OUTPUT_SUPPRESS_WINDOW
                - std::time::Duration::from_millis(1),
        );
    }

    assert!(!terminal.should_suppress_cursor_during_output());
}

#[test]
fn decset_application_cursor_keys() {
    let terminal = test_terminal(80, 24);
    assert!(!terminal.application_cursor_keys());

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?1h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.application_cursor_keys());

    let actions = parser.parse_as_vec(b"\x1b[?1l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(!terminal.application_cursor_keys());
}

#[test]
fn decset_cursor_visibility() {
    let terminal = test_terminal(80, 24);
    assert!(terminal.cursor_visible());

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?25l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(!terminal.cursor_visible());

    let actions = parser.parse_as_vec(b"\x1b[?25h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.cursor_visible());
}

#[test]
fn decset_bracketed_paste() {
    let terminal = test_terminal(80, 24);
    assert!(!terminal.bracketed_paste());

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?2004h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.bracketed_paste());

    let actions = parser.parse_as_vec(b"\x1b[?2004l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(!terminal.bracketed_paste());
}

#[test]
fn decset_mouse_tracking() {
    let terminal = test_terminal(80, 24);
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::None);

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?1000h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::Click);

    let actions = parser.parse_as_vec(b"\x1b[?1003h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::AllMotion);

    // 1003을 꺼도 1000이 남아 Click 모드를 유지한다.
    let actions = parser.parse_as_vec(b"\x1b[?1003l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::Click);

    let actions = parser.parse_as_vec(b"\x1b[?1000l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::None);
}

/// 한 마우스 모드를 꺼도 다른 모드가 유지되는지 확인한다.
#[test]
fn mouse_modes_are_independent_registers() {
    let mut terminal = test_terminal(80, 24);
    let mut parser = Parser::new();
    let mut apply = |terminal: &mut Terminal, bytes: &[u8]| {
        for action in parser.parse_as_vec(bytes) {
            if let Action::CSI(CSI::Mode(ref mode)) = action {
                terminal.lock_state().handle_mode(mode);
            }
        }
    };

    apply(&mut terminal, b"\x1b[?1003h");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::AllMotion);
    // 켠 적도 없는 1002 를 끄는 것이 1003 을 건드리면 안 된다.
    apply(&mut terminal, b"\x1b[?1002l");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::AllMotion);

    apply(&mut terminal, b"\x1b[?1003l");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::None);

    // 실효 레벨은 켜진 것 중 가장 넓은 것 — 끄는 순서와 무관하다.
    apply(&mut terminal, b"\x1b[?1000h\x1b[?1002h");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::CellMotion);
    apply(&mut terminal, b"\x1b[?1000l");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::CellMotion);
    apply(&mut terminal, b"\x1b[?1002l");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::None);
}

/// 다른 모드가 켜져 있으면 부분 해제 뒤에도 첫 캡처 안내를 유지한다.
#[test]
fn mouse_capture_hint_survives_partial_disable() {
    let mut terminal = test_terminal(80, 24);
    let mut parser = Parser::new();
    let mut apply = |terminal: &mut Terminal, bytes: &[u8]| {
        for action in parser.parse_as_vec(bytes) {
            if let Action::CSI(CSI::Mode(ref mode)) = action {
                terminal.lock_state().handle_mode(mode);
            }
        }
    };

    apply(&mut terminal, b"\x1b[?1003h\x1b[?1002l");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::AllMotion);
    assert!(terminal.take_mouse_capture_hint(), "무장이 유지돼야 한다");
}

#[test]
fn mouse_capture_hint_arms_on_none_to_on_edge() {
    let mut terminal = test_terminal(80, 24);
    let mut parser = Parser::new();
    let mut apply = |terminal: &mut Terminal, bytes: &[u8]| {
        for action in parser.parse_as_vec(bytes) {
            if let Action::CSI(CSI::Mode(ref mode)) = action {
                terminal.lock_state().handle_mode(mode);
            }
        }
    };

    // None → ON(1000h): 무장 → 첫 take 만 true, 소비 후 false.
    // (좌·우 클릭 중 먼저 호출한 쪽만 true 를 받는다 — 첫 상호작용 1회.)
    apply(&mut terminal, b"\x1b[?1000h");
    assert!(terminal.take_mouse_capture_hint());
    assert!(!terminal.take_mouse_capture_hint());

    // ON → ON(1000 켜진 채 1002h 전환): 재무장하지 않는다.
    apply(&mut terminal, b"\x1b[?1002h");
    assert!(!terminal.take_mouse_capture_hint());

    // OFF(→None) 후 다시 None → ON: 재무장. 1000/1002 가 **둘 다** 켜져 있으므로
    // 실효 레벨을 None 으로 내리려면 둘 다 꺼야 한다(독립 레지스터).
    apply(&mut terminal, b"\x1b[?1002l\x1b[?1000l");
    apply(&mut terminal, b"\x1b[?1000h");
    assert!(terminal.take_mouse_capture_hint());
}

#[test]
fn mouse_capture_hint_disarms_on_ris() {
    let mut terminal = test_terminal(80, 24);
    let mut parser = Parser::new();
    let mut apply = |terminal: &mut Terminal, bytes: &[u8]| {
        for action in parser.parse_as_vec(bytes) {
            if let Action::CSI(CSI::Mode(ref mode)) = action {
                terminal.lock_state().handle_mode(mode);
            }
        }
    };

    // 트래킹 ON 으로 무장한 뒤 RIS(ESC c): mouse_tracking 리셋 + 안내 disarm.
    apply(&mut terminal, b"\x1b[?1000h");
    terminal.process_bytes(b"\x1bc");
    assert_eq!(terminal.mouse_tracking(), MouseTrackingMode::None);
    assert!(!terminal.take_mouse_capture_hint());

    // 재진입 시 다시 무장된다.
    apply(&mut terminal, b"\x1b[?1000h");
    assert!(terminal.take_mouse_capture_hint());
}

#[test]
fn alternate_screen_switching() {
    let terminal = test_terminal(80, 24);
    assert!(!terminal.is_alternate_screen());

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?1049h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.is_alternate_screen());
    assert!(terminal.lock_state().alternate_surface.is_some());

    let actions = parser.parse_as_vec(b"\x1b[?1049l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(!terminal.is_alternate_screen());
}

#[test]
fn alternate_screen_mode_47() {
    let terminal = test_terminal(80, 24);

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?47h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.is_alternate_screen());

    let actions = parser.parse_as_vec(b"\x1b[?47l");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(!terminal.is_alternate_screen());
}

#[test]
fn alternate_screen_resize() {
    let mut terminal = test_terminal(80, 24);

    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?1049h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }

    terminal.resize(120, 40);
    assert_eq!(terminal.cols(), 120);
    assert_eq!(terminal.rows(), 40);
    let (cols, rows) = terminal.dimensions();
    assert_eq!(cols, 120);
    assert_eq!(rows, 40);
}

#[test]
fn arrow_key_sequences_normal_vs_application() {
    let terminal = test_terminal(80, 24);

    assert!(!terminal.application_cursor_keys());
    let mut parser = Parser::new();
    let actions = parser.parse_as_vec(b"\x1b[?1h");
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.application_cursor_keys());
}

#[test]
fn full_reset_clears_modes() {
    let terminal = test_terminal(80, 24);

    let mut parser = Parser::new();
    let data = b"\x1b[?1h\x1b[?25l\x1b[?2004h\x1b[?1049h";
    let actions = parser.parse_as_vec(data);
    for action in actions {
        if let Action::CSI(CSI::Mode(ref mode)) = action {
            terminal.lock_state().handle_mode(mode);
        }
    }
    assert!(terminal.application_cursor_keys());
    assert!(!terminal.cursor_visible());
    assert!(terminal.bracketed_paste());
    assert!(terminal.is_alternate_screen());

    let actions = parser.parse_as_vec(b"\x1bc");
    for action in actions {
        let _changes = terminal.lock_state().action_to_changes(action);
    }
    assert!(!terminal.application_cursor_keys());
    assert!(terminal.cursor_visible());
    assert!(!terminal.bracketed_paste());
    assert!(!terminal.is_alternate_screen());
}

fn first_scrollback_text(terminal: &Terminal, index: usize) -> String {
    terminal
        .lock_state()
        .scrollback_line(index)
        .map(|l| {
            l.cells()
                .map(|(s, _)| s)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .unwrap_or_default()
}

#[test]
fn scrollback_captured_on_lf_at_bottom_row() {
    let mut terminal = test_terminal(10, 4);
    terminal.process_bytes(b"row0\r\nrow1\r\nrow2\r\nrow3");
    assert_eq!(terminal.scrollback_len(), 0);
    terminal.process_bytes(b"\r\nrow4");
    assert_eq!(
        terminal.scrollback_len(),
        1,
        "newline at bottom row must push row0 to scrollback"
    );
    assert_eq!(first_scrollback_text(&terminal, 0), "row0");
}

#[test]
fn soft_wrapped_line_records_wrap_flag_in_scrollback() {
    // 25글자를 10열에 출력해 wrap된 행을 만든 뒤 스크롤백으로 밀어낸다.
    let mut terminal = test_terminal(10, 4);
    let payload: Vec<u8> = (b'a'..=b'y').collect(); // 25 chars
    terminal.process_bytes(&payload);
    // Force two extra LFs at the bottom to flush wrapped lines into scrollback.
    terminal.process_bytes(b"\r\nnext\r\nmore\r\nfinal");

    assert!(
        terminal.scrollback_len() >= 1,
        "expected wrapped lines to be pushed into scrollback"
    );
    // The first scrollback line is the head of the wrapped command, which
    // continues on the next line — it must be marked wrapped.
    assert_eq!(
        terminal.scrollback_line_wrapped(0),
        Some(true),
        "first scrollback line was a soft-wrap continuation point"
    );
}

#[test]
fn hard_newline_line_is_not_wrapped() {
    let mut terminal = test_terminal(10, 4);
    // "row0\nrow1\nrow2\nrow3\nrow4" — row0 ends with a real LF, no wrap.
    terminal.process_bytes(b"row0\r\nrow1\r\nrow2\r\nrow3\r\nrow4");
    assert!(terminal.scrollback_len() >= 1);
    assert_eq!(
        terminal.scrollback_line_wrapped(0),
        Some(false),
        "row0 ended in a hard newline, not a wrap"
    );
}

/// Join every scrollback line (oldest→newest) plus every visible row into a
/// single string, so a marker can be counted across the whole buffer.
fn all_buffer_text(terminal: &Terminal) -> String {
    let state = terminal.lock_state();
    let mut out = String::new();
    for i in 0..state.scrollback_len() {
        if let Some(cells) = state.scrollback_line_owned(i) {
            for (s, _) in &cells {
                out.push_str(s);
            }
            out.push('\n');
        }
    }
    for line in state.surface().screen_lines().iter() {
        for cell in line.visible_cells() {
            out.push_str(cell.str());
        }
        out.push('\n');
    }
    out
}

/// Auto-wrap으로 termwiz 내부에서 밀어낸 행도 스크롤백에 남아야 한다.
#[test]
fn wrap_induced_scroll_at_bottom_row_reaches_scrollback() {
    let mut terminal = test_terminal(10, 4);
    // 8 visual rows of 10 identical chars each (no LF): "0000000000".."7777777777".
    // The screen holds 4; the first 4 rows are scrolled off by wrap-scroll.
    let mut payload = Vec::new();
    for d in b'0'..=b'7' {
        payload.extend(std::iter::repeat_n(d, 10));
    }
    terminal.process_bytes(&payload);

    assert_eq!(
        terminal.scrollback_len(),
        4,
        "wrap-induced scroll at the bottom row must push the 4 evicted rows to scrollback"
    );
    assert_eq!(first_scrollback_text(&terminal, 0), "0000000000");
    assert_eq!(first_scrollback_text(&terminal, 3), "3333333333");
}

/// Auto-wrap만으로 스크롤할 때 각 표지가 화면과 스크롤백 전체에 정확히 한 번 남아야 한다.
#[test]
fn wrapping_lines_are_preserved_exactly_once() {
    let mut terminal = test_terminal(10, 4);
    // N visual rows, each exactly 10 chars beginning with a unique marker,
    // concatenated with NO newlines so wrap alone drives the scrolling.
    const N: usize = 8;
    let mut payload = String::new();
    for i in 0..N {
        payload.push_str(&format!("MK{i:02}aaaaaa")); // "MKnn" + 6 = 10 cols
    }
    terminal.process_bytes(payload.as_bytes());

    let buffer = all_buffer_text(&terminal);
    for i in 0..N {
        let marker = format!("MK{i:02}");
        let count = buffer.matches(&marker).count();
        assert_eq!(
            count, 1,
            "marker {marker} must appear exactly once (count={count}); \
             0 = lost on wrap-scroll, >1 = duplicated"
        );
    }
}

/// 화면을 제자리에서 다시 그려도 이미 스크롤된 표지가 복제되면 안 된다.
#[test]
fn repaint_after_wrap_scroll_does_not_duplicate_committed_lines() {
    let mut terminal = test_terminal(10, 4);
    // 6 wrap-driven rows: MK00..MK05. 4 fit; MK00/MK01 scroll into history.
    let mut payload = String::new();
    for i in 0..6 {
        payload.push_str(&format!("MK{i:02}aaaaaa"));
    }
    terminal.process_bytes(payload.as_bytes());

    // TUI frame repaint: rewrite each of the 4 visible rows in place via
    // absolute positioning + erase-line. No newline, so nothing scrolls.
    for row in 0..4 {
        terminal.process_bytes(format!("\x1b[{};1H\x1b[2K", row + 1).as_bytes());
        terminal.process_bytes(format!("RP{row:02}bbbbbb").as_bytes());
    }

    let buffer = all_buffer_text(&terminal);
    // Committed (scrolled-off) markers must survive exactly once.
    for i in 0..2 {
        let marker = format!("MK{i:02}");
        let count = buffer.matches(&marker).count();
        assert_eq!(
            count, 1,
            "committed marker {marker} must appear exactly once (count={count})"
        );
    }
}

fn join_line(line: &crate::ScrollbackLine) -> String {
    line.cells().map(|(s, _)| s).collect()
}

#[test]
fn screen_snapshot_captures_visible_content() {
    let mut terminal = test_terminal(10, 4);
    terminal.process_bytes(b"alpha\r\nbeta\r\ngamma");
    let snap = terminal.screen_snapshot_lines();
    // 화면에 3 줄 출력. 마지막 빈 줄들은 trim 되어 3 줄만 남는다.
    assert_eq!(snap.len(), 3);
    assert_eq!(join_line(&snap[0]).trim_end(), "alpha");
    assert_eq!(join_line(&snap[1]).trim_end(), "beta");
    assert_eq!(join_line(&snap[2]).trim_end(), "gamma");
}

#[test]
fn screen_snapshot_trims_trailing_blank_rows() {
    let mut terminal = test_terminal(10, 6);
    terminal.process_bytes(b"hello");
    let snap = terminal.screen_snapshot_lines();
    // 한 줄만 출력했으니 뒤따르는 5 줄 빈 row 는 모두 잘려야 한다.
    assert_eq!(snap.len(), 1);
    assert_eq!(join_line(&snap[0]).trim_end(), "hello");
}

#[test]
fn screen_snapshot_empty_terminal_returns_empty_vec() {
    let terminal = test_terminal(10, 4);
    assert!(terminal.screen_snapshot_lines().is_empty());
}

#[test]
fn prefill_visible_from_scrollback_draws_lines_and_parks_cursor() {
    use crate::ScrollbackLine;
    use termwiz::cell::CellAttributes;

    let mut terminal = test_terminal(20, 10);

    // Inject 15 historical lines.
    let mut injected = Vec::new();
    for i in 0..15 {
        let label = format!("OLD_{i:02}");
        let cells: Vec<(String, CellAttributes)> = label
            .chars()
            .map(|c| (c.to_string(), CellAttributes::default()))
            .collect();
        injected.push(ScrollbackLine::new(cells, false));
    }
    terminal.inject_scrollback(injected);

    // Prefill half the visible rows (5 of 10).
    let drawn = terminal.prefill_visible_from_scrollback(5);
    assert_eq!(drawn, 5);
    assert_eq!(terminal.scrollback_len(), 10); // 5 popped

    // Visible rows 0..4 should now hold the popped lines (oldest first).
    let lines = terminal.screen_lines();
    for (row, expected_idx) in (10..15).enumerate() {
        let text: String = lines[row]
            .visible_cells()
            .map(|c| c.str().to_string())
            .collect();
        let expected = format!("OLD_{expected_idx:02}");
        assert!(
            text.starts_with(&expected),
            "row {row} expected to start with {expected:?}, got {text:?}"
        );
    }

    // Cursor parked on row right after prefilled block.
    assert_eq!(terminal.cursor_position(), (0, 5));
}

#[test]
fn prefill_saturates_when_scrollback_is_shorter_than_requested() {
    use crate::ScrollbackLine;
    use termwiz::cell::CellAttributes;

    let mut terminal = test_terminal(20, 10);

    let mut injected = Vec::new();
    for i in 0..3 {
        let label = format!("OLD_{i}");
        let cells: Vec<(String, CellAttributes)> = label
            .chars()
            .map(|c| (c.to_string(), CellAttributes::default()))
            .collect();
        injected.push(ScrollbackLine::new(cells, false));
    }
    terminal.inject_scrollback(injected);

    // Request 5, but only 3 are available.
    let drawn = terminal.prefill_visible_from_scrollback(5);
    assert_eq!(drawn, 3);
    assert_eq!(terminal.scrollback_len(), 0);
    assert_eq!(terminal.cursor_position(), (0, 3));
}

/// Regression: when scrollback contains injected/historical lines (e.g. from
/// a previous session restore) and the cursor sits high with blank rows
/// below, repeated shrink→grow cycles must NOT keep pulling fresh lines
/// from scrollback into the visible area. Each cycle should be idempotent.
#[test]
fn resize_ping_pong_does_not_accumulate_visible_lines_when_cursor_is_high() {
    use crate::ScrollbackLine;
    use termwiz::cell::CellAttributes;

    let mut terminal = test_terminal(20, 10);

    // Inject 30 historical scrollback lines (simulates previous-session restore).
    let mut injected = Vec::new();
    for i in 0..30 {
        let label = format!("OLD_{i:02}");
        let cells: Vec<(String, CellAttributes)> = label
            .chars()
            .map(|c| (c.to_string(), CellAttributes::default()))
            .collect();
        injected.push(ScrollbackLine::new(cells, false));
    }
    terminal.inject_scrollback(injected);
    let initial_scrollback_len = terminal.scrollback_len();
    assert_eq!(initial_scrollback_len, 30);

    // Move cursor to (0,0) and write a short prompt — cursor stays high,
    // bottom rows remain blank.
    terminal.process_bytes(b"\x1b[H$ ");

    // 3 ping-pong cycles: grow → shrink → grow → shrink → grow → shrink.
    for _ in 0..3 {
        terminal.resize(20, 14); // grow by 4 rows
        terminal.resize(20, 10); // shrink back
    }

    // Scrollback should not have shrunk: handle_rows_grow should not have
    // popped lines that no corresponding shrink had pushed.
    assert_eq!(
        terminal.scrollback_len(),
        initial_scrollback_len,
        "scrollback was consumed by grow operations that had no matching shrink push"
    );

    // The visible screen should still show only the prompt at row 0 plus
    // blank rows below — no historical OLD_NN lines should have leaked
    // into the visible area.
    let lines = terminal.screen_lines();
    for (row, line) in lines.iter().enumerate() {
        let text: String = line.visible_cells().map(|c| c.str().to_string()).collect();
        assert!(
            !text.contains("OLD_"),
            "row {row} contains historical scrollback content: {text:?}"
        );
    }
}

/// A representative byte sequence exercising text, CR/LF, SGR color/intensity,
/// absolute cursor moves, EraseLine, a DECSET mode toggle, and a clear.
const MIRROR_SEQ: &[u8] = b"hello\r\n\x1b[31mred\x1b[0m world\r\n\x1b[2J\x1b[H\x1b[1mbold\x1b[0m\x1b[?1h\x1b[3;5Hxy\x1b[K";

fn assert_grid_eq(a: &Terminal, b: &Terminal, ctx: &str) {
    assert_eq!(
        a.screen_text(true),
        b.screen_text(true),
        "{ctx}: screen_text"
    );
    assert_eq!(a.cursor_position(), b.cursor_position(), "{ctx}: cursor");
    assert_eq!(
        a.application_cursor_keys(),
        b.application_cursor_keys(),
        "{ctx}: DECCKM"
    );
    assert_eq!(
        a.is_alternate_screen(),
        b.is_alternate_screen(),
        "{ctx}: alt-screen"
    );
    // Compare per-cell text + key attributes on the populated rows.
    for row in 0..4 {
        let ac = a.row_cells(row);
        let bc = b.row_cells(row);
        assert_eq!(ac.len(), bc.len(), "{ctx}: row {row} cell count");
        for ((aidx, ai), (bidx, bi)) in ac.iter().zip(bc.iter()) {
            assert_eq!(aidx, bidx, "{ctx}: row {row} cell index");
            assert_eq!(ai.text, bi.text, "{ctx}: row {row} text");
            assert_eq!(
                (ai.fg.as_str(), ai.intensity, ai.inverse),
                (bi.fg.as_str(), bi.intensity, bi.inverse),
                "{ctx}: row {row} attrs"
            );
        }
    }
}

#[test]
fn detached_feed_matches_pty_process_path() {
    let (mut real, _pty) = test_pty_terminal(40, 12); // PTY-backed; shell output unused
    let mut mirror = Terminal::new_detached(40, 12);
    real.process_bytes(MIRROR_SEQ); // shared ingest path (= process() parsing)
    mirror.feed_bytes(MIRROR_SEQ);
    assert_grid_eq(&real, &mirror, "single-shot");
}

#[test]
fn detached_feed_is_chunk_boundary_invariant() {
    // Feeding the same stream split at arbitrary byte boundaries (including
    // mid-escape) must reconstruct an identical grid — the parser carries
    // state across feed_bytes calls.
    let whole = {
        let mut t = Terminal::new_detached(40, 12);
        t.feed_bytes(MIRROR_SEQ);
        t
    };
    // Split points: mid-escape "\x1b[3" | ";5Hxy..." and a few others.
    for split in [1usize, 7, 18, 30, MIRROR_SEQ.len() - 3] {
        let mut t = Terminal::new_detached(40, 12);
        t.feed_bytes(&MIRROR_SEQ[..split]);
        t.feed_bytes(&MIRROR_SEQ[split..]);
        assert_grid_eq(&whole, &t, &format!("split@{split}"));
    }
}

#[test]
fn detached_alt_screen_parity() {
    let seq = b"main\x1b[?1049h\x1b[Halt-content\x1b[?1049lback";
    let (mut real, _pty) = test_pty_terminal(40, 12);
    let mut mirror = Terminal::new_detached(40, 12);
    real.process_bytes(b"main\x1b[?1049h\x1b[Halt-content");
    mirror.feed_bytes(b"main\x1b[?1049h\x1b[Halt-content");
    assert_grid_eq(&real, &mirror, "in-alt");
    real.process_bytes(b"\x1b[?1049lback");
    mirror.feed_bytes(b"\x1b[?1049lback");
    assert_grid_eq(&real, &mirror, "after-exit");
    let _ = seq;
}

#[test]
fn detached_terminal_has_no_pty_state() {
    let mut t = Terminal::new_detached(40, 12);
    // resize touches only the surface; no PTY notification is queued.
    t.resize(80, 24);
    assert_eq!(t.cols(), 80);
    assert_eq!(t.rows(), 24);
    let (cols, rows) = t.dimensions();
    assert_eq!((cols, rows), (80, 24));
    // process() on a detached terminal is a harmless no-op (no child exit event).
    assert!(!t.process());
    assert!(t.take_events().is_empty());
}

#[test]
fn feed_bytes_reports_change() {
    let mut t = Terminal::new_detached(40, 12);
    assert!(!t.feed_bytes(b""), "empty feed does not change the surface");
    assert!(t.feed_bytes(b"x"), "text feed changes the surface");
}

/// 상태 락을 얻지 못하면 이벤트를 버리지 않고 남긴 채 None을 반환해야 한다.
#[test]
fn try_take_events_skips_while_the_state_lock_is_held_but_take_events_waits() {
    let mut t = Terminal::new_detached(40, 12);
    t.feed_bytes(b"\x1b]133;D;0\x07");

    let held = std::sync::Arc::clone(&t.state);
    let guard = held.lock().expect("아직 성한 락");
    assert!(
        t.try_take_events().is_none(),
        "상태 락을 얻지 못하면 try_take_events는 기다리지 않고 None을 반환해야 한다"
    );
    drop(guard);

    assert!(
        !t.take_events().is_empty(),
        "락 경합 때 남겨 둔 이벤트를 다음 take에서 읽을 수 있어야 한다"
    );
}

#[test]
fn detached_input_forwards_to_sink() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(40, 12);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.send_bytes(b"abc");
    t.send_key("Z");
    assert_eq!(rx.recv().unwrap(), b"abc".to_vec());
    assert_eq!(rx.recv().unwrap(), b"Z".to_vec());
}

// OSC 52 read query (`OSC 52 ; c ; ? ST`) must surface a single ClipboardQuery
// event through the production ingest path — the host gates the actual reply.
#[test]
fn osc52_read_query_emits_clipboard_query_event() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]52;c;?\x07");
    let queries = t
        .take_events()
        .into_iter()
        .filter(|e| matches!(e.kind, TerminalEventKind::ClipboardQuery))
        .count();
    assert_eq!(queries, 1, "exactly one ClipboardQuery expected");
}

// Regression: OSC 52 write (set) still produces a ClipboardSet event, not a
// ClipboardQuery. `aGk=` is base64 for "hi".
#[test]
fn osc52_write_still_emits_clipboard_set() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]52;c;aGk=\x07");
    let events = t.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.kind, TerminalEventKind::ClipboardSet(s) if s == "hi")),
        "OSC 52 write should emit ClipboardSet(\"hi\")"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, TerminalEventKind::ClipboardQuery)),
        "a write must not emit a ClipboardQuery"
    );
}

// OSC 133 전용 파서 결과가 PromptBoundary 이벤트로 변환되는지 검사한다.
#[test]
fn osc133_a_phase_emits_prompt_boundary() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]133;A\x07");
    let events = t.take_events();
    assert!(
        events.iter().any(
            |e| matches!(&e.kind, TerminalEventKind::PromptBoundary { phase, .. } if *phase == 'A')
        ),
        "OSC 133;A should emit PromptBoundary{{phase: 'A', ..}}"
    );
}

#[test]
fn osc133_c_phase_emits_prompt_boundary() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]133;C\x07");
    let events = t.take_events();
    assert!(
        events.iter().any(
            |e| matches!(&e.kind, TerminalEventKind::PromptBoundary { phase, .. } if *phase == 'C')
        ),
        "OSC 133;C should emit PromptBoundary{{phase: 'C', ..}}"
    );
}

// OSC 133 D의 종료 코드는 전용 파서 결과에서 payload로 전달한다.
#[test]
fn osc133_d_phase_carries_exit_code_as_payload() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]133;D;0\x07");
    let events = t.take_events();
    assert!(
        events.iter().any(|e| matches!(
            &e.kind,
            TerminalEventKind::PromptBoundary { phase, payload } if *phase == 'D' && payload == "0"
        )),
        "OSC 133;D;0 should emit PromptBoundary{{phase: 'D', payload: \"0\"}}"
    );
}

#[test]
fn osc133_d_phase_nonzero_exit_code() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]133;D;127\x07");
    let events = t.take_events();
    assert!(
        events.iter().any(|e| matches!(
            &e.kind,
            TerminalEventKind::PromptBoundary { phase, payload } if *phase == 'D' && payload == "127"
        )),
        "OSC 133;D;127 should emit PromptBoundary{{phase: 'D', payload: \"127\"}}"
    );
}

// Bare B (no `cmd=` payload) is the common case real shell integration scripts
// send — termwiz parses this successfully into `FinalTermSemanticPrompt`
// too (unlike a `cmd=`-carrying B, which fails termwiz's strict single-token
// parse and falls back to `Unspecified`).
#[test]
fn osc133_bare_b_phase_emits_prompt_boundary() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]133;B\x07");
    let events = t.take_events();
    assert!(
        events.iter().any(
            |e| matches!(&e.kind, TerminalEventKind::PromptBoundary { phase, .. } if *phase == 'B')
        ),
        "bare OSC 133;B should emit PromptBoundary{{phase: 'B', ..}}"
    );
}

#[test]
fn da2_query_emits_secondary_attributes_response() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b[>c"); // DA2
    let resp = rx.try_recv().expect("no DA2 response");
    assert!(
        resp.starts_with(b"\x1b[>") && resp.ends_with(b"c"),
        "unexpected DA2 response: {resp:?}"
    );
}

#[test]
fn xtversion_query_emits_name_and_version_response() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b[>0q"); // XTVERSION
    let resp = rx.try_recv().expect("no XTVERSION response");
    assert!(
        resp.starts_with(b"\x1bP") && resp.ends_with(b"\x1b\\"),
        "XTVERSION must be a DCS string terminated by ST: {resp:?}"
    );
    assert!(
        resp.windows(5).any(|w| w == b"tasty"),
        "XTVERSION must contain the terminal name: {resp:?}"
    );
}

#[test]
fn xtgettcap_query_emits_unsupported_reply() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    // XtGetTcap for "Co" (436f). Currently unsupported → DCS 0 + r 436f ST.
    t.feed_bytes(b"\x1bP+q436f\x1b\\");
    let resp = rx.try_recv().expect("no XtGetTcap response");
    assert_eq!(
        resp, b"\x1bP0+r436f\x1b\\",
        "XtGetTcap must answer status 0 (currently unsupported): {resp:?}"
    );
}

#[test]
fn xtgettcap_multi_cap_query_emits_one_reply_each() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    // Two caps in one query: "Co" (436f) ; "TN" (544e).
    t.feed_bytes(b"\x1bP+q436f;544e\x1b\\");
    let first = rx.try_recv().expect("no reply for first cap");
    assert_eq!(
        first, b"\x1bP0+r436f\x1b\\",
        "unexpected first reply: {first:?}"
    );
    let second = rx.try_recv().expect("no reply for second cap");
    assert_eq!(
        second, b"\x1bP0+r544e\x1b\\",
        "unexpected second reply: {second:?}"
    );
}

#[test]
fn da1_query_still_emits_primary_attributes_response() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b[c"); // DA1 — must be unchanged
    assert_eq!(
        rx.try_recv().expect("no DA1 response"),
        b"\x1b[?1;2c".to_vec()
    );
}

#[test]
fn detached_input_without_sink_is_dropped() {
    // No sink wired: must not panic or hang, just drop.
    let mut t = Terminal::new_detached(40, 12);
    t.send_bytes(b"abc");
    t.send_key("Z");
}

// Detached 터미널로 실제 ingest 경로를 쓰되 비동기 셸 배너가 시험 입력에 섞이지 않게 한다.

#[test]
fn output_tap_receives_raw_bytes_and_replays_to_mirror() {
    let mut t = Terminal::new_detached(40, 12);
    let rx = t.add_output_tap();
    t.feed_bytes(b"\x1b[31mX");
    assert_eq!(rx.try_recv().unwrap(), b"\x1b[31mX".to_vec());

    // Replaying the tapped bytes into a mirror yields an identical grid.
    let mut mirror = Terminal::new_detached(40, 12);
    mirror.feed_bytes(b"\x1b[31mX");
    assert_grid_eq(&t, &mirror, "tap-replay");
}

#[test]
fn output_tap_is_non_destructive() {
    // The grid produced with a tap attached must match the grid without one.
    let mut tapped = Terminal::new_detached(40, 12);
    let _rx = tapped.add_output_tap();
    tapped.feed_bytes(MIRROR_SEQ);

    let mut untapped = Terminal::new_detached(40, 12);
    untapped.feed_bytes(MIRROR_SEQ);
    assert_grid_eq(&tapped, &untapped, "tap-nondestructive");
}

#[test]
fn output_tap_count_reflects_registrations() {
    let mut t = Terminal::new_detached(40, 12);
    assert_eq!(t.output_tap_count(), 0);
    let _rx1 = t.add_output_tap();
    assert_eq!(t.output_tap_count(), 1);
    let _rx2 = t.add_output_tap();
    assert_eq!(t.output_tap_count(), 2);
}

#[test]
fn output_tap_disconnected_is_pruned() {
    let mut t = Terminal::new_detached(40, 12);
    let rx = t.add_output_tap();
    drop(rx); // subscriber gone
    // Next ingest detects the disconnect, prunes the tap, and applies normally.
    t.feed_bytes(b"hello");
    assert!(t.screen_text(true).contains("hello"));
    // A fresh tap still works after pruning.
    let rx2 = t.add_output_tap();
    t.feed_bytes(b"!");
    assert_eq!(rx2.try_recv().unwrap(), b"!".to_vec());
}

#[test]
fn resize_tap_emits_new_dims_only_on_change() {
    let mut t = test_terminal(40, 12);
    let rx = t.add_resize_tap();
    // No-op resize (same dims) must NOT emit.
    t.resize(40, 12);
    assert!(rx.try_recv().is_err());
    // Real resize emits the authoritative new grid.
    t.resize(100, 30);
    assert_eq!(rx.try_recv().unwrap(), (100, 30));
    // A second change emits again.
    t.resize(80, 24);
    assert_eq!(rx.try_recv().unwrap(), (80, 24));
}

/// grid 변경과 tap 통지가 먼저이고, OS resize는 예약만 된 뒤 flush에서 적용된다.
#[test]
fn resize_taps_the_grid_before_the_os_resize_is_applied() {
    let (mut t, mut pty) = test_pty_terminal(40, 12);
    let rx = t.add_resize_tap();
    if t.resize(100, 30) {
        pty.schedule_resize(t.resource_generation(), 100, 30);
    }

    assert_eq!(
        rx.try_recv().unwrap(),
        (100, 30),
        "tap은 grid 변경 즉시 온다"
    );
    assert_eq!(t.dimensions(), (100, 30));
    assert!(pty.has_pending_resize(), "OS resize는 아직 예약 상태다");
    assert_eq!(
        pty.os_size(),
        Some((40, 12)),
        "tap 시점에 OS 크기는 그대로다"
    );

    pty.force_flush_resize();
    assert_eq!(pty.os_size(), Some((100, 30)), "flush 뒤 OS에 적용된다");
}

#[test]
fn resize_tap_disconnected_is_pruned() {
    let mut t = test_terminal(40, 12);
    let rx = t.add_resize_tap();
    drop(rx); // subscriber gone
    // Next resize detects the disconnect, prunes the tap, and resizes normally.
    t.resize(60, 18);
    assert_eq!((t.cols(), t.rows()), (60, 18));
}

#[test]
fn detached_mirror_is_detached_and_resizes_grid() {
    let (source, pty) = test_pty_terminal(40, 12);
    assert_eq!(source.resource_generation(), pty.generation());
    let mut mirror = Terminal::new_detached(157, 45);
    assert_ne!(mirror.resource_generation(), pty.generation());
    assert_eq!((mirror.cols(), mirror.rows()), (157, 45));
    mirror.resize(120, 40);
    assert_eq!((mirror.cols(), mirror.rows()), (120, 40));
    assert_eq!(
        pty.os_size(),
        Some((40, 12)),
        "remote content cannot resize a local resource"
    );
}

/// Snapshot-specific grid comparison. Unlike [`assert_grid_eq`], this does NOT
/// compare raw `visible_cells()` counts: termwiz back-fills a row to full width
/// when reached via absolute cursor addressing (CUP), while `EraseLine` shrinks
/// it — so the raw cell count depends on the write/erase *history*, not the final
/// visible content. The snapshot reproduces visible content, not history. We
/// therefore compare the rendered text, cursor, modes, and the attributes of each
/// non-blank cell (which is what the renderer actually draws).
fn assert_snapshot_eq(server: &Terminal, mirror: &Terminal, ctx: &str) {
    assert_eq!(
        server.screen_text(true),
        mirror.screen_text(true),
        "{ctx}: screen_text"
    );
    assert_eq!(
        server.cursor_position(),
        mirror.cursor_position(),
        "{ctx}: cursor"
    );
    assert_eq!(
        server.application_cursor_keys(),
        mirror.application_cursor_keys(),
        "{ctx}: DECCKM"
    );
    assert_eq!(
        server.is_alternate_screen(),
        mirror.is_alternate_screen(),
        "{ctx}: alt-screen"
    );
    // Per non-blank cell: text + key attributes must match at the same position.
    let nonblank = |t: &Terminal, row: usize| -> Vec<(usize, String, String, &'static str, bool)> {
        t.row_cells(row)
            .into_iter()
            .filter(|(_, ci)| ci.text != " " && !ci.text.is_empty())
            .map(|(idx, ci)| (idx, ci.text, ci.fg, ci.intensity, ci.inverse))
            .collect()
    };
    for row in 0..server.rows() {
        assert_eq!(
            nonblank(server, row),
            nonblank(mirror, row),
            "{ctx}: row {row} non-blank cells"
        );
    }
}

#[test]
fn snapshot_as_vt_reconstructs_grid_in_mirror() {
    // Build a populated server-side screen (text + SGR + cursor move + DECCKM).
    let mut server = test_terminal(40, 12);
    server.process_bytes(MIRROR_SEQ);

    // Serialize the current screen and feed it into a fresh mirror: the mirror's
    // visible content + cursor + modes must match the server's.
    let snapshot = server.snapshot_as_vt();
    let mut mirror = Terminal::new_detached(40, 12);
    mirror.feed_bytes(&snapshot);
    assert_snapshot_eq(&server, &mirror, "snapshot-replay");
}

#[test]
fn snapshot_as_vt_preserves_colors_and_intensity() {
    // 24-bit fg, palette bg, bold — all must round-trip through the snapshot.
    let mut server = test_terminal(20, 4);
    server.process_bytes(b"\x1b[38;2;10;200;30m\x1b[44m\x1b[1mAB\x1b[0mC");
    let snapshot = server.snapshot_as_vt();
    let mut mirror = Terminal::new_detached(20, 4);
    mirror.feed_bytes(&snapshot);

    let a = server.row_cells(0);
    let b = mirror.row_cells(0);
    assert_eq!(a.len(), b.len(), "cell count");
    for ((_, ai), (_, bi)) in a.iter().zip(b.iter()) {
        assert_eq!(
            (
                ai.text.as_str(),
                ai.fg.as_str(),
                ai.bg.as_str(),
                ai.intensity
            ),
            (
                bi.text.as_str(),
                bi.fg.as_str(),
                bi.bg.as_str(),
                bi.intensity
            ),
            "cell attrs round-trip"
        );
    }
}

#[test]
fn snapshot_as_vt_preserves_alt_screen() {
    let mut server = test_terminal(20, 4);
    server.process_bytes(b"primary\x1b[?1049h\x1b[Halt-only");
    assert!(server.is_alternate_screen());
    let snapshot = server.snapshot_as_vt();
    let mut mirror = Terminal::new_detached(20, 4);
    mirror.feed_bytes(&snapshot);
    assert!(mirror.is_alternate_screen(), "mirror enters alt-screen");
    assert_eq!(
        server.screen_text(true),
        mirror.screen_text(true),
        "alt content"
    );
}

#[test]
fn alive_check_throttled_within_window() {
    let (mut t, mut pty) = test_pty_terminal(80, 24);
    // In-the-past init: the first process() must check immediately.
    assert!(pty.state.last_alive_check.elapsed() >= ALIVE_CHECK_INTERVAL);

    // 첫 process 호출이 검사 시각을 갱신했는지 직접 비교한다.
    let before = pty.state.last_alive_check;
    process_pair(&mut t, &mut pty);
    let first = pty.state.last_alive_check;
    assert!(first > before, "first check stamped now");

    // Immediate second call falls inside the throttle window — no re-check.
    process_pair(&mut t, &mut pty);
    assert_eq!(
        first, pty.state.last_alive_check,
        "check skipped within window"
    );

    // After the window elapses the check runs (and re-stamps) again.
    std::thread::sleep(ALIVE_CHECK_INTERVAL + std::time::Duration::from_millis(100));
    process_pair(&mut t, &mut pty);
    assert!(
        pty.state.last_alive_check > first,
        "check re-ran after window"
    );
}

#[test]
fn process_exited_eventually_emitted() {
    // wake를 기다리되 ALIVE_CHECK_INTERVAL마다 process를 호출해 종료를 다시 확인한다.
    // 실패 시 호출 횟수·마지막 alive 판정·화면 꼬리를 함께 남긴다. waker는 블로킹하지 않는다.
    // 사용자 셸($SHELL)의 기동은 initial_input을 먹을 수 있다. 예를 들어 시작 파일이 없는 HOME의
    // zsh는 zsh-newuser-install이 첫 키("e")를 읽어 "xit"만 남는다. 그래서 Unix는 그런 대화형
    // 설정 단계가 없는 `sh`를 쓴다. Windows는 `sh`가 없어 기본 셸을 쓴다.
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let waker: Waker = Arc::new(move || {
        // 버퍼(1)가 이미 차 있으면 깨우기 신호가 대기 중이라는 뜻이라, 이번 send 실패는 무시해도
        // 대기 측을 깨우기에 충분하다. 콜백은 절대 블록되면 안 되므로 send(블로킹)가 아니라 try_send.
        let _ = tx.try_send(());
    });
    let (mut t, mut pty) = spawn_terminal(
        TerminalConfig {
            cols: 80,
            rows: 24,
            shell: if cfg!(unix) { Some("sh") } else { TEST_SHELL },
            args: &[],
            surface_id: 0,
            working_dir: None,
            initial_input: Some("exit\r"),
            extra_env: &[],
        },
        waker,
    )
    .expect("terminal creation");

    // 셸 종료를 기다릴 전체 제한. 종료 감지뿐 아니라 셸 기동·입력 전달 시간도 포함한다.
    const BUDGET: std::time::Duration = std::time::Duration::from_secs(30);
    let started = std::time::Instant::now();
    let deadline = started + BUDGET;
    let mut seen = false;
    let mut polls = 0usize;
    while std::time::Instant::now() < deadline {
        process_pair(&mut t, &mut pty);
        polls += 1;
        if t.lock_state()
            .events
            .iter()
            .any(|e| matches!(e.kind, TerminalEventKind::ProcessExited))
        {
            seen = true;
            break;
        }
        // wake가 없어도 alive-check 주기마다 확인하며 전체 남은 시간을 넘겨 기다리지 않는다.
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        // wake 로 깼는지 상한으로 깼는지 구분할 필요가 없다 — 루프 상단의 process()+이벤트 검사와
        // deadline 조건이 다음 반복에서 판정한다. 그래서 recv 결과는 무시한다.
        let _ = rx.recv_timeout(remaining.min(ALIVE_CHECK_INTERVAL));
    }
    if !seen {
        // 실패 시 실제 관측값을 남기되 이 값만으로 원인을 단정하지 않는다.
        let alive = pty.check_alive();
        let screen = t.lock_state().screen_text(false);
        let visible = screen.trim_end();
        let tail: String = visible
            .chars()
            .rev()
            .take(48)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        panic!(
            "ProcessExited가 제한 {BUDGET:?} 안에 나오지 않았다(경과 {elapsed:?}). \
             process() 호출 {polls}회, 종료 검사 간격 {ALIVE_CHECK_INTERVAL:?}, 마지막 alive={alive}. \
             화면 꼬리=\"{tail}\". 화면의 exit 에코만으로 셸이 입력을 처리했다고 판단할 수 없다.",
            elapsed = started.elapsed(),
            tail = tail.escape_default(),
        );
    }
}

/// 자식이 PTY를 닫고 1초 뒤 종료하게 해 EOF가 종료보다 먼저 오는 상황을 만든다.
/// 폴링 없이 wake만 받아도 ProcessExited가 나와야 한다.
/// macOS는 자식이 PTY fd를 모두 닫아도 master EOF가 자식 종료와 함께 와서 이 상황을 만들 수 없다.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn process_exit_after_pty_eof_is_seen_by_wakes_alone() {
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let waker: Waker = Arc::new(move || {
        // 버퍼(1)가 차 있으면 이미 깨우기 신호가 대기 중이라 이번 실패는 버려도 된다.
        let _ = tx.try_send(());
    });
    // 바깥 셸(`-li` 가 붙는다)은 job control 용 tty fd 를 따로 쥘 수 있어 곧장 비대화형
    // `sh` 로 exec 한다(pid 는 그대로라 우리 자식이다). 그 셸이 표준 fd 를 PTY 에서 떼면
    // master 가 EOF 를 보고, 자식은 1 초 뒤에야 끝난다.
    let (mut t, mut pty) = spawn_terminal(
        TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("sh"),
            args: &[
                "-c",
                "exec sh -c 'exec </dev/null >/dev/null 2>&1; sleep 1'",
            ],
            surface_id: 0,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        waker,
    )
    .expect("terminal creation");

    const BUDGET: std::time::Duration = std::time::Duration::from_secs(15);
    let deadline = std::time::Instant::now() + BUDGET;
    let mut wakes = 0usize;
    let mut eof_seen_alive = false;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if rx.recv_timeout(remaining).is_err() {
            panic!(
                "no wake within {BUDGET:?} after {wakes} wakes (EOF seen while alive: \
                 {eof_seen_alive}) — the exit behind an early PTY EOF was never driven"
            );
        }
        wakes += 1;
        process_pair(&mut t, &mut pty);
        if t.lock_state()
            .events
            .iter()
            .any(|e| matches!(e.kind, TerminalEventKind::ProcessExited))
        {
            break;
        }
        if pty
            .state()
            .reader_eof
            .load(std::sync::atomic::Ordering::Acquire)
        {
            eof_seen_alive = true;
        }
    }
    assert!(
        eof_seen_alive,
        "the child must outlive the PTY EOF, or this test does not open the window it is about"
    );
}

/// ProcessExited 뒤 반복 wake가 멈춰야 한다. EOF 또는 이미 진행 중인 wake 하나는
/// 종료 판정과 경합해 늦게 올 수 있으므로 허용한다.
#[cfg(unix)]
#[test]
fn eof_rewakes_stop_once_the_exit_is_settled() {
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let waker: Waker = Arc::new(move || {
        // 버퍼(1)가 차 있으면 이미 깨우기 신호가 대기 중이라 이번 실패는 버려도 된다.
        let _ = tx.try_send(());
    });
    let (mut t, mut pty) = spawn_terminal(
        TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("sh"),
            args: &["-c", "exit 0"],
            surface_id: 0,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        waker,
    )
    .expect("terminal creation");

    const BUDGET: std::time::Duration = std::time::Duration::from_secs(15);
    let deadline = std::time::Instant::now() + BUDGET;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        rx.recv_timeout(remaining)
            .unwrap_or_else(|_| panic!("no ProcessExited within {BUDGET:?}"));
        process_pair(&mut t, &mut pty);
        if t.lock_state()
            .events
            .iter()
            .any(|e| matches!(e.kind, TerminalEventKind::ProcessExited))
        {
            break;
        }
    }

    // 상한(500 ms)의 네 배 — 플래그가 안 서면 이 창에 wake 가 여러 번 온다.
    let window = ALIVE_CHECK_INTERVAL * 4;
    let until = std::time::Instant::now() + window;
    let mut late_wakes = 0usize;
    loop {
        let remaining = until.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || rx.recv_timeout(remaining).is_err() {
            break;
        }
        late_wakes += 1;
    }
    assert!(
        late_wakes <= 1,
        "{late_wakes} wakes in {window:?} after ProcessExited — the EOF re-wake did not stop \
         once the exit was settled"
    );
}

/// Concat of all OutputAppended texts — termwiz may deliver "hello" as one
/// PrintString or a chain of Print(c), so individual events are not stable.
fn appended_text(events: &[TerminalEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            TerminalEventKind::OutputAppended { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn no_output_appended_when_gate_off() {
    let mut t = Terminal::new_detached(80, 24);
    // The gate defaults to off — no setter call needed.
    t.feed_bytes(b"hello world");
    assert_eq!(appended_text(&t.take_events()), "");

    t.set_output_events_enabled(false);
    t.feed_bytes(b"more");
    assert_eq!(appended_text(&t.take_events()), "");
}

#[test]
fn output_appended_emitted_when_gate_on() {
    let mut t = Terminal::new_detached(80, 24);
    t.set_output_events_enabled(true);
    t.feed_bytes(b"hello");
    assert_eq!(appended_text(&t.take_events()), "hello");
}

#[test]
fn output_appended_stops_after_gate_turned_off() {
    let mut t = Terminal::new_detached(80, 24);
    t.set_output_events_enabled(true);
    t.feed_bytes(b"on");
    assert_eq!(appended_text(&t.take_events()), "on");

    t.set_output_events_enabled(false);
    t.feed_bytes(b"off");
    assert_eq!(appended_text(&t.take_events()), "");
}

#[test]
fn detached_terminal_never_emits_process_exited() {
    let mut t = Terminal::new_detached(40, 12);
    t.process();
    std::thread::sleep(ALIVE_CHECK_INTERVAL + std::time::Duration::from_millis(50));
    t.process();
    assert!(
        !t.lock_state()
            .events
            .iter()
            .any(|e| matches!(e.kind, TerminalEventKind::ProcessExited)),
        "detached mirror has no child to exit"
    );
}

// Overline·UnderlineColor·VerticalAlign은 mirrored pen을 통한 실제 ingest 경로로 확인한다.
// 단순 TestTerminal의 SGR 변환은 이 항목을 생략하므로 사용하지 않는다.

#[test]
fn sgr_overline_reflected_in_cell_info() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[53mX");
    let info = t.cell_info(0, 0).expect("cell 0,0");
    assert_eq!(info.text, "X");
    assert!(info.overline, "SGR 53 should set overline");

    // SGR 55 turns overline back off.
    t.feed_bytes(b"\x1b[55mY");
    let info = t.cell_info(0, 1).expect("cell 0,1");
    assert_eq!(info.text, "Y");
    assert!(!info.overline, "SGR 55 should clear overline");
}

#[test]
fn sgr_underline_color_reflected_in_cell_info() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[58;5;9mX");
    let info = t.cell_info(0, 0).expect("cell 0,0");
    assert_eq!(info.text, "X");
    assert_ne!(
        info.underline_color, "default",
        "SGR 58 should set a non-default underline color"
    );
    assert_eq!(info.underline_color, "palette:9");

    // SGR 59 resets the underline color to default.
    t.feed_bytes(b"\x1b[59mY");
    let info = t.cell_info(0, 1).expect("cell 0,1");
    assert_eq!(info.underline_color, "default", "SGR 59 should reset");
}

#[test]
fn sgr_vertical_align_reflected_in_cell_info() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[73mS"); // superscript
    let info = t.cell_info(0, 0).expect("cell 0,0");
    assert_eq!(info.vertical_align, "super");

    t.feed_bytes(b"\x1b[74mU"); // subscript
    let info = t.cell_info(0, 1).expect("cell 0,1");
    assert_eq!(info.vertical_align, "sub");

    t.feed_bytes(b"\x1b[75mB"); // back to baseline
    let info = t.cell_info(0, 2).expect("cell 0,2");
    assert_eq!(info.vertical_align, "baseline");
}

#[test]
fn sgr_overline_preserves_other_attributes() {
    // Bold set before overline must survive the AllAttributes round-trip.
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[1m\x1b[53mX");
    let info = t.cell_info(0, 0).expect("cell 0,0");
    assert!(info.bold, "bold (SGR 1) must be preserved");
    assert!(info.overline, "overline (SGR 53) must be applied");
}

#[test]
fn sgr_reset_clears_overline_and_align() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[53;73mX"); // overline + superscript
    let info = t.cell_info(0, 0).expect("cell 0,0");
    assert!(info.overline);
    assert_eq!(info.vertical_align, "super");

    t.feed_bytes(b"\x1b[0mY"); // full SGR reset
    let info = t.cell_info(0, 1).expect("cell 0,1");
    assert!(!info.overline, "SGR 0 should clear overline");
    assert_eq!(
        info.vertical_align, "baseline",
        "SGR 0 should reset vertical align"
    );
}

#[test]
fn sgr_overline_does_not_regress_basic_attributes() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[1;4;7mX");
    let info = t.cell_info(0, 0).expect("cell 0,0");
    assert!(info.bold);
    assert!(info.underline);
    assert!(info.inverse);
    assert!(!info.overline, "overline must stay false when unset");
}

#[test]
fn osc8_hyperlink_attaches_uri_to_cells_then_clears() {
    // OSC 8 opens a hyperlink, prints "LINK", then closes it. The four LINK
    // cells must carry the URI; cells printed after the close must not.
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]8;;https://example.com\x1b\\LINK\x1b]8;;\x1b\\");

    for col in 0..4 {
        let attrs = t.cell_attrs(0, col).expect("LINK cell");
        let link = attrs
            .hyperlink()
            .unwrap_or_else(|| panic!("cell {col} should carry a hyperlink"));
        assert_eq!(link.uri(), "https://example.com");
    }

    // After the OSC 8 close (empty URI), subsequent cells have no hyperlink.
    t.feed_bytes(b"PLAIN");
    let attrs = t.cell_attrs(0, 4).expect("PLAIN cell");
    assert!(
        attrs.hyperlink().is_none(),
        "hyperlink must not leak past the OSC 8 close"
    );
}

#[test]
fn decscusr_sets_cursor_shape() {
    // Default on a fresh terminal.
    let mut t = Terminal::new_detached(80, 24);
    assert_eq!(t.cursor_shape(), CursorShape::Default);

    // Each xterm parameter maps to its shape + blink flag.
    for (seq, expected) in [
        (&b"\x1b[1 q"[..], CursorShape::BlinkingBlock),
        (&b"\x1b[2 q"[..], CursorShape::SteadyBlock),
        (&b"\x1b[3 q"[..], CursorShape::BlinkingUnderline),
        (&b"\x1b[4 q"[..], CursorShape::SteadyUnderline),
        (&b"\x1b[5 q"[..], CursorShape::BlinkingBar),
        (&b"\x1b[6 q"[..], CursorShape::SteadyBar),
        (&b"\x1b[0 q"[..], CursorShape::Default),
    ] {
        t.feed_bytes(seq);
        assert_eq!(t.cursor_shape(), expected, "seq {seq:?}");
    }
}

#[test]
fn decscusr_reset_by_full_reset() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[5 q"); // blinking bar
    assert_eq!(t.cursor_shape(), CursorShape::BlinkingBar);
    t.feed_bytes(b"\x1bc"); // RIS / FullReset
    assert_eq!(t.cursor_shape(), CursorShape::Default);
}

#[test]
fn decscusr_survives_soft_reset() {
    // DECSTR (CSI ! p) intentionally does NOT reset the cursor shape (matches
    // xterm — only RIS restores the default). Regression guard.
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[5 q"); // blinking bar
    assert_eq!(t.cursor_shape(), CursorShape::BlinkingBar);
    t.feed_bytes(b"\x1b[!p"); // DECSTR soft reset
    assert_eq!(t.cursor_shape(), CursorShape::BlinkingBar);
}

/// Distinct per-channel test palette so each color number's response is
/// unambiguous. fg/bg/cursor and a couple of ANSI entries use values that
/// survive the 8→16-bit widening unchanged in the low byte.
fn test_palette() -> ColorPalette {
    let mut ansi = [TerminalRgb::new(0, 0, 0); 16];
    ansi[1] = TerminalRgb::new(0xff, 0x00, 0x00); // ANSI 1 (red)
    ansi[4] = TerminalRgb::new(0x12, 0x34, 0x56); // ANSI 4 (blue), arbitrary
    ColorPalette {
        foreground: TerminalRgb::new(0xab, 0xcd, 0xef),
        background: TerminalRgb::new(0x10, 0x20, 0x30),
        cursor: TerminalRgb::new(0x11, 0x22, 0x33),
        ansi,
    }
}

#[test]
fn osc11_bg_query_responds_with_palette_background() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.set_color_palette(test_palette());
    t.feed_bytes(b"\x1b]11;?\x1b\\"); // bg color query (ST terminated)
    let resp = rx.try_recv().expect("no OSC 11 response");
    // bg = 0x10/0x20/0x30 → widened by *0x101.
    assert_eq!(resp, b"\x1b]11;rgb:1010/2020/3030\x1b\\".to_vec());
}

#[test]
fn osc10_fg_query_responds_with_palette_foreground() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.set_color_palette(test_palette());
    t.feed_bytes(b"\x1b]10;?\x1b\\"); // fg color query
    let resp = rx.try_recv().expect("no OSC 10 response");
    assert_eq!(resp, b"\x1b]10;rgb:abab/cdcd/efef\x1b\\".to_vec());
}

#[test]
fn osc12_cursor_query_responds_with_palette_cursor() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.set_color_palette(test_palette());
    t.feed_bytes(b"\x1b]12;?\x1b\\"); // cursor color query
    let resp = rx.try_recv().expect("no OSC 12 response");
    assert_eq!(resp, b"\x1b]12;rgb:1111/2222/3333\x1b\\".to_vec());
}

#[test]
fn osc4_palette_query_responds_with_ansi_color() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.set_color_palette(test_palette());
    t.feed_bytes(b"\x1b]4;4;?\x1b\\"); // query ANSI index 4
    let resp = rx.try_recv().expect("no OSC 4 response");
    assert_eq!(resp, b"\x1b]4;4;rgb:1212/3434/5656\x1b\\".to_vec());
}

#[test]
fn osc10_multi_query_reflects_fg_bg_cursor_in_sequence() {
    // `OSC 10 ; ? ; ? ; ?` walks fg(10) → bg(11) → cursor(12); each `?` answers
    // the next color number.
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.set_color_palette(test_palette());
    t.feed_bytes(b"\x1b]10;?;?;?\x1b\\");
    assert_eq!(
        rx.try_recv().expect("no fg response"),
        b"\x1b]10;rgb:abab/cdcd/efef\x1b\\".to_vec()
    );
    assert_eq!(
        rx.try_recv().expect("no bg response"),
        b"\x1b]11;rgb:1010/2020/3030\x1b\\".to_vec()
    );
    assert_eq!(
        rx.try_recv().expect("no cursor response"),
        b"\x1b]12;rgb:1111/2222/3333\x1b\\".to_vec()
    );
}

#[test]
fn osc_color_query_without_palette_is_silent() {
    // No palette plumbed → no source → leave the query unanswered (don't guess).
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b]11;?\x1b\\");
    assert!(rx.try_recv().is_err(), "unset palette must not respond");
}

#[test]
fn osc_color_set_request_is_not_answered() {
    // A *set* (color spec, not `?`) must not emit any response — only queries do.
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.set_color_palette(test_palette());
    t.feed_bytes(b"\x1b]11;rgb:0000/0000/0000\x1b\\"); // set bg, no query
    assert!(rx.try_recv().is_err(), "set request must not respond");
}

// 화면 전환·리사이즈의 직접 add_change 경로도 활성 화면의 pen과 일치해야 한다.

#[test]
fn alt_screen_entry_does_not_leak_primary_pen_into_cell_info() {
    let mut t = Terminal::new_detached(80, 24);
    // Primary: set bold, write a cell.
    t.feed_bytes(b"\x1b[1mP");
    assert!(t.cell_info(0, 0).expect("primary cell").bold);

    // Enter alt screen (1049h), then apply overline only.
    t.feed_bytes(b"\x1b[?1049h");
    t.feed_bytes(b"\x1b[53mX");
    let a = t.cell_info(0, 0).expect("alt cell");
    assert_eq!(a.text, "X");
    assert!(a.overline, "alt cell should have overline");
    assert!(
        !a.bold,
        "stale primary bold must not leak onto the alt cell"
    );
}

#[test]
fn alt_screen_exit_restores_primary_pen() {
    let mut t = Terminal::new_detached(80, 24);
    // Primary bold, write P at 0,0 (cursor advances to col 1).
    t.feed_bytes(b"\x1b[1mP");
    // Round-trip through the alt screen with a different pen.
    t.feed_bytes(b"\x1b[?1049h");
    t.feed_bytes(b"\x1b[53mX"); // overline on alt, no bold
    t.feed_bytes(b"\x1b[?1049l"); // leave alt; cursor restored to (col 1, row 0)
    // Apply overline on primary; the restored pen still carries bold.
    t.feed_bytes(b"\x1b[53mQ");
    let q = t.cell_info(0, 1).expect("primary cell after exit");
    assert_eq!(q.text, "Q");
    assert!(
        q.bold,
        "primary pen (bold) must be restored after leaving alt"
    );
    assert!(q.overline, "overline applied on primary after exit");
    // Original primary cell intact.
    let p = t.cell_info(0, 0).expect("original primary cell");
    assert_eq!(p.text, "P");
    assert!(p.bold);
    assert!(!p.overline, "original cell never had overline");
}

#[test]
fn alt_screen_47_switch_does_not_leak_pen() {
    // Mode 47 switches without clearing; the pen must still track the surface.
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b[1mP"); // primary bold
    t.feed_bytes(b"\x1b[?47h"); // enter alt (no clear)
    t.feed_bytes(b"\x1b[53mX"); // overline only on alt
    let a = t.cell_info(0, 0).expect("alt cell");
    assert!(a.overline);
    assert!(!a.bold, "primary bold must not leak across the 47h switch");
}

#[test]
fn resize_restore_does_not_leave_stale_pen() {
    // Shrink then grow forces the grow/tail restore paths to emit
    // `AllAttributes` per restored cell directly on the surface. After that the
    // active pen must still be the logical pen (bold), not the plain attrs left
    // by the last restored cell.
    let mut t = Terminal::new_detached(80, 6);
    t.feed_bytes(b"L1\r\nL2\r\nL3\r\nL4\r\n");
    t.feed_bytes(b"\x1b[1m"); // logical pen = bold (no cell written yet)
    t.resize(80, 3); // shrink: pushes top lines to scrollback
    t.resize(80, 6); // grow: restores them via direct AllAttributes(plain)
    // Home + plain cell: should pick up the logical bold pen, not the
    // restoration artifact left on the surface by the restore loop.
    t.feed_bytes(b"\x1b[HZ");
    let z = t.cell_info(0, 0).expect("cell 0,0");
    assert_eq!(z.text, "Z");
    assert!(
        z.bold,
        "resize restore must not leave a stale (non-bold) pen"
    );
}

#[test]
fn decaln_fills_screen_with_e() {
    let mut t = Terminal::new_detached(8, 3);
    t.feed_bytes(b"\x1b#8");
    for row in 0..3 {
        assert_eq!(
            t.screen_row(row, true),
            "EEEEEEEE",
            "row {row} should be all E"
        );
    }
    // DECALN homes the cursor.
    assert_eq!(t.cursor_position(), (0, 0));
}

#[test]
fn nel_moves_to_next_line_col0() {
    let mut t = Terminal::new_detached(10, 4);
    t.feed_bytes(b"abc\x1bEx");
    // 'x' should land at column 0 of row 1, not after 'abc'.
    assert_eq!(t.screen_row(0, true), "abc");
    assert_eq!(t.screen_row(1, true), "x");
    assert_eq!(t.cursor_position(), (1, 1));
}

#[test]
fn nel_scrolls_at_bottom() {
    let mut t = Terminal::new_detached(10, 2);
    t.feed_bytes(b"r0\x1bEr1\x1bEr2");
    // After two NELs on a 2-row screen, the first row scrolled off.
    assert_eq!(t.screen_row(0, true), "r1");
    assert_eq!(t.screen_row(1, true), "r2");
}

#[test]
fn rep_repeats_last_character() {
    let mut t = Terminal::new_detached(10, 2);
    // 'a' then REP 4 → "aaaaa" (1 original + 4 repeats).
    t.feed_bytes(b"a\x1b[4b");
    assert_eq!(t.screen_row(0, true), "aaaaa");
    assert_eq!(t.cursor_position(), (5, 0));
}

#[test]
fn rep_default_count_is_one() {
    let mut t = Terminal::new_detached(10, 2);
    t.feed_bytes(b"X\x1b[b"); // no param → repeat once
    assert_eq!(t.screen_row(0, true), "XX");
}

#[test]
fn rep_without_prior_print_is_noop() {
    let mut t = Terminal::new_detached(10, 2);
    t.feed_bytes(b"\x1b[5b"); // nothing printed yet
    assert_eq!(t.screen_row(0, true), "");
}

#[test]
fn ht_advances_to_8col_tab_stop() {
    let mut t = Terminal::new_detached(40, 2);
    t.feed_bytes(b"\tX");
    // Default tab stop at column 8.
    assert_eq!(t.cursor_position().0, 9); // 8 (stop) + 1 (X)
    let row = t.screen_row(0, true);
    assert_eq!(row.chars().nth(8), Some('X'));
}

#[test]
fn ht_between_text_aligns_columns() {
    let mut t = Terminal::new_detached(40, 2);
    t.feed_bytes(b"A\tB");
    let row = t.screen_row(0, true);
    assert_eq!(row.chars().next(), Some('A'));
    assert_eq!(row.chars().nth(8), Some('B'));
}

#[test]
fn ht_clamps_at_right_margin() {
    let mut t = Terminal::new_detached(5, 2);
    // No tab stop beyond column 0 within 5 cols → clamps at last column (4).
    t.feed_bytes(b"\t");
    assert_eq!(t.cursor_position().0, 4);
}

#[test]
fn cbt_moves_back_a_tab_stop() {
    let mut t = Terminal::new_detached(40, 2);
    t.feed_bytes(b"\t\t"); // → column 16
    assert_eq!(t.cursor_position().0, 16);
    t.feed_bytes(b"\x1b[Z"); // CBT 1 → column 8
    assert_eq!(t.cursor_position().0, 8);
}

#[test]
fn cht_moves_forward_n_tab_stops() {
    let mut t = Terminal::new_detached(40, 2);
    t.feed_bytes(b"\x1b[2I"); // CHT 2 → column 16
    assert_eq!(t.cursor_position().0, 16);
}

#[test]
fn hts_sets_and_tbc_clears_custom_stop() {
    let mut t = Terminal::new_detached(40, 2);
    // Move to column 3, set a custom tab stop there.
    t.feed_bytes(b"\x1b[4G"); // CHA to column 4 (1-based) → col 3
    assert_eq!(t.cursor_position().0, 3);
    t.feed_bytes(b"\x1bH"); // HTS at col 3
    // From home, HT should now stop at the custom column 3.
    t.feed_bytes(b"\r\t");
    assert_eq!(t.cursor_position().0, 3);
    // Clear that stop (TBC 0) at col 3, then HT from home jumps to default 8.
    t.feed_bytes(b"\x1b[g"); // TBC 0 at current col (3)
    t.feed_bytes(b"\r\t");
    assert_eq!(t.cursor_position().0, 8);
}

#[test]
fn tbc_3_clears_all_stops() {
    let mut t = Terminal::new_detached(40, 2);
    t.feed_bytes(b"\x1b[3g"); // clear all stops
    t.feed_bytes(b"\t"); // no stops → clamp at right margin (39)
    assert_eq!(t.cursor_position().0, 39);
}

#[test]
fn decscnm_toggles_reverse_flag() {
    let mut t = Terminal::new_detached(10, 2);
    assert!(!t.screen_reverse());
    t.feed_bytes(b"\x1b[?5h"); // set DECSCNM
    assert!(t.screen_reverse());
    t.feed_bytes(b"\x1b[?5l"); // reset
    assert!(!t.screen_reverse());
}

#[test]
fn decscnm_reset_by_full_reset() {
    let mut t = Terminal::new_detached(10, 2);
    t.feed_bytes(b"\x1b[?5h");
    assert!(t.screen_reverse());
    t.feed_bytes(b"\x1bc"); // RIS
    assert!(!t.screen_reverse());
}

#[test]
fn decom_makes_cup_region_relative() {
    let mut t = Terminal::new_detached(20, 10);
    // Scroll region rows 3..=6 (1-based 4..7).
    t.feed_bytes(b"\x1b[4;7r");
    // Enable origin mode — cursor homes to region top (row 3).
    t.feed_bytes(b"\x1b[?6h");
    assert_eq!(t.cursor_position(), (0, 3));
    // CUP to line 1 (region-relative) → physical row 3.
    t.feed_bytes(b"\x1b[1;1HA");
    assert_eq!(t.screen_row(3, true), "A");
    // CUP to line 2 → physical row 4.
    t.feed_bytes(b"\x1b[2;1HB");
    assert_eq!(t.screen_row(4, true), "B");
}

#[test]
fn decom_clamps_to_region_bottom() {
    let mut t = Terminal::new_detached(20, 10);
    t.feed_bytes(b"\x1b[4;7r"); // region rows 3..=6
    t.feed_bytes(b"\x1b[?6h");
    // Line 99 (region-relative) clamps to region bottom (row 6).
    t.feed_bytes(b"\x1b[99;1HZ");
    assert_eq!(t.screen_row(6, true), "Z");
}

#[test]
fn decom_off_is_absolute() {
    let mut t = Terminal::new_detached(20, 10);
    t.feed_bytes(b"\x1b[4;7r");
    // Without origin mode, CUP line 1 is absolute row 0.
    t.feed_bytes(b"\x1b[1;1HA");
    assert_eq!(t.screen_row(0, true), "A");
}

#[test]
fn decom_reset_by_full_reset() {
    let mut t = Terminal::new_detached(20, 10);
    t.feed_bytes(b"\x1b[4;7r\x1b[?6h");
    t.feed_bytes(b"\x1bc"); // RIS
    t.feed_bytes(b"\x1b[1;1HA");
    assert_eq!(t.screen_row(0, true), "A");
}

#[test]
fn xtwinops_reports_text_area_cells() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b[18t"); // report text area size in cells
    let resp = rx.try_recv().expect("no XTWINOPS 18t response");
    assert_eq!(resp, b"\x1b[8;24;80t");
}

#[test]
fn xtwinops_reports_screen_cells() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(100, 30);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b[19t");
    let resp = rx.try_recv().expect("no XTWINOPS 19t response");
    assert_eq!(resp, b"\x1b[9;30;100t");
}

#[test]
fn xtwinops_pixel_reports_are_unanswered() {
    use std::sync::mpsc;
    let mut t = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    t.set_input_sink(tx);
    t.feed_bytes(b"\x1b[14t"); // text area pixels — intentionally no response
    t.feed_bytes(b"\x1b[16t"); // cell pixels — intentionally no response
    assert!(rx.try_recv().is_err(), "pixel reports must not respond");
}

#[test]
fn xtwinops_title_stack_push_pop() {
    let mut t = Terminal::new_detached(80, 24);
    t.feed_bytes(b"\x1b]2;first\x07"); // set title "first"
    t.take_events(); // drain
    t.feed_bytes(b"\x1b[22;0t"); // push "first"
    t.feed_bytes(b"\x1b]2;second\x07"); // set title "second"
    t.take_events();
    t.feed_bytes(b"\x1b[23;0t"); // pop → restore "first"
    let restored = t
        .take_events()
        .into_iter()
        .find_map(|e| match e.kind {
            TerminalEventKind::TitleChanged(s) => Some(s),
            _ => None,
        })
        .expect("no TitleChanged on pop");
    assert_eq!(restored, "first");
}

#[test]
fn dec_line_drawing_maps_box_chars() {
    let mut t = Terminal::new_detached(10, 2);
    // Designate G0 = line drawing, print "lqk" → "┌─┐".
    t.feed_bytes(b"\x1b(0lqk");
    assert_eq!(t.screen_row(0, true), "┌─┐");
    // Back to ASCII: "lqk" prints literally.
    t.feed_bytes(b"\x1b(B\r\nlqk");
    assert_eq!(t.screen_row(1, true), "lqk");
}

#[test]
fn dec_line_drawing_so_si_switches_g1() {
    let mut t = Terminal::new_detached(10, 2);
    // G1 = line drawing; G0 stays ASCII. SO invokes G1, SI back to G0.
    t.feed_bytes(b"\x1b)0"); // designate G1 line drawing
    t.feed_bytes(b"a\x0eq\x0fb"); // 'a' (G0/ascii), SO, 'q'→─, SI, 'b'
    assert_eq!(t.screen_row(0, true), "a─b");
}

#[test]
fn dec_line_drawing_reset_by_ris() {
    let mut t = Terminal::new_detached(10, 2);
    t.feed_bytes(b"\x1b(0");
    t.feed_bytes(b"\x1bc"); // RIS clears charset designation
    t.feed_bytes(b"q");
    assert_eq!(t.screen_row(0, true), "q");
}

#[test]
fn terminal_query_response_does_not_suppress_busy() {
    // 터미널이 커서 위치 질의에 응답한 직후에 나온 출력은 "사용자 입력 에코" 가 아니다.
    let mut terminal = Terminal::new_detached(80, 24);
    terminal.process_bytes(b"\x1b[6n"); // DSR → send_terminal_response 경로
    terminal.process_bytes(b"x"); // 응답 직후(=억제 창 안)의 프로그램 출력

    let st = terminal.lock_state();
    assert!(
        st.last_activity_at > st.last_input_at + INPUT_ECHO_WINDOW,
        "터미널 자체 응답이 입력 에코 억제 창을 갱신하면 안 된다",
    );
}

#[test]
fn user_input_still_suppresses_echo() {
    // 사용자 입력 직후의 출력은 에코 억제 대상이다.
    let mut terminal = Terminal::new_detached(80, 24);
    terminal.send_key("x");
    terminal.process_bytes(b"x");

    let st = terminal.lock_state();
    assert!(st.last_activity_at <= st.last_input_at + INPUT_ECHO_WINDOW);
}

#[test]
fn terminal_query_response_still_reaches_pty() {
    // 억제 창에서 분리해도 응답의 실제 write 경로(sink send + enqueued_count)는 그대로다.
    use std::sync::mpsc;
    let mut terminal = Terminal::new_detached(80, 24);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    terminal.set_input_sink(tx);
    terminal.process_bytes(b"\x1b[6n");

    assert_eq!(
        rx.try_recv().expect("no DSR cursor position response"),
        b"\x1b[1;1R"
    );
    assert_eq!(terminal.lock_state().enqueued_count, 1);
}

const FAKE_SHELL_PID: u32 = 1;

fn fake_fg(name: &str, pid: u32) -> foreground_process::ForegroundProcessInfo {
    foreground_process::ForegroundProcessInfo {
        name: name.to_string(),
        pid,
    }
}

/// 출력·입력 시각을 직접 설정한다. 판정 시각까지 실제 스케줄 지연이 생길 수는 있다.
/// 출력 시각은 busy 판정용 활동 시각과 `last_output_at` 을 함께 돌려 놓는다. 하나만 돌리면
/// 제목만 오는 출력이 `last_output_at` 을 갱신하는지 가리지 못한다.
fn set_times(t: &Terminal, output_ago: std::time::Duration, input_ago: std::time::Duration) {
    let now = std::time::Instant::now();
    let mut st = t.lock_state();
    let output_at = now.checked_sub(output_ago).expect("instant underflow");
    st.last_output_at = output_at;
    st.last_activity_at = output_at;
    st.last_input_at = now.checked_sub(input_ago).expect("instant underflow");
}

fn ms(n: u64) -> std::time::Duration {
    std::time::Duration::from_millis(n)
}

/// 출력이 입력 직후 에코 창 안에 있는 "타이핑 에코" 형태.
fn set_echo_after_input(t: &Terminal) {
    set_times(t, ms(200), ms(300));
}

/// 입력과 무관한 최근 프로그램 출력으로 busy 에 진입시킨다.
fn enter_busy(t: &mut Terminal, fg: &foreground_process::ForegroundProcessInfo) {
    set_times(t, ms(100), ms(1000));
    assert!(
        t.busy_with_foreground(FAKE_SHELL_PID, Some(fg)),
        "전제: 입력과 무관한 최근 출력은 busy 다",
    );
}

#[test]
fn busy_is_kept_while_typing_over_output() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("claude", 2);
    enter_busy(&mut t, &fg);
    set_echo_after_input(&t);
    assert!(
        t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)),
        "직전이 busy 면 입력 에코 구간이어도 busy 를 유지해야 한다",
    );
}

#[test]
fn typing_into_idle_program_does_not_enter_busy() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("vim", 2);
    set_echo_after_input(&t);
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
    // 억제된 판정은 래치를 만들지 않는다 — 다시 물어도 idle.
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn input_does_not_invalidate_earlier_output() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("cargo", 2);
    // 출력 500ms 전, 입력은 그 뒤인 100ms 전 — 출력이 입력보다 앞선다.
    set_times(&t, ms(500), ms(100));
    assert!(
        t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)),
        "입력 이전의 출력은 에코가 아니므로 억제 대상이 아니다",
    );
}

#[test]
fn busy_releases_when_output_goes_quiet() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("cargo", 2);
    enter_busy(&mut t, &fg);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
    // 만료가 래치를 지웠으므로 뒤이은 타이핑 에코는 진입이 막힌다.
    set_echo_after_input(&t);
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn output_expiry_wins_over_recent_input() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("claude", 2);
    enter_busy(&mut t, &fg);
    // 출력은 창 밖으로 멈췄고 키는 방금 쳤다.
    set_times(&t, BUSY_OUTPUT_WINDOW + ms(500), ms(0));
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
    set_echo_after_input(&t);
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn shell_foreground_releases_latched_busy() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("vim", 2);
    enter_busy(&mut t, &fg);
    set_echo_after_input(&t);
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fake_fg("bash", 3))));
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fake_fg("zsh", FAKE_SHELL_PID))));
    // 같은 프로그램이 다시 foreground 로 와도(예: 중단 후 fg) 래치는 이미 풀려 있다.
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn foreground_change_does_not_inherit_busy() {
    let mut t = Terminal::new_detached(80, 24);
    enter_busy(&mut t, &fake_fg("cargo", 2));
    set_echo_after_input(&t);
    assert!(
        !t.busy_with_foreground(FAKE_SHELL_PID, Some(&fake_fg("vim", 4))),
        "다른 프로그램은 이전 프로그램의 busy 를 물려받지 않는다",
    );
}

#[test]
fn contended_lock_guess_does_not_latch() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("claude", 2);
    set_echo_after_input(&t);
    {
        let state = Arc::clone(&t.state);
        let _held =
            tasty_utils::poison::recover_mutex(state.lock(), STATE_WHAT, &STATE_POISON_REPORTED);
        assert!(
            t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)),
            "락 경합은 그 tick 에 한해 busy 로 추정한다",
        );
    }
    assert!(
        !t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)),
        "추정 갈래의 true 가 유지 상태의 근거가 되면 안 된다",
    );
}

/// reader worker처럼 다른 스레드가 상태 잠금으로 ingest하는 동안 attach 구독을 건다.
/// 구독 시점 이전 출력은 snapshot에, 이후 출력은 tap에 정확히 한 번 들어가야 한다.
fn attach_subscription_accounts_for_every_byte(
    subscribe: impl Fn(&mut Terminal) -> (Vec<u8>, mpsc::Receiver<Vec<u8>>),
) -> usize {
    use std::sync::atomic::AtomicUsize;
    const CHUNK: &[u8] = b"xxxxxxxx";
    const CHUNKS: usize = 350; // 80x40 격자에 줄바꿈 없이 들어가고 tap 용량보다 작다.
    let mut mismatches = 0;
    for _ in 0..1000 {
        let mut t = Terminal::new_detached(80, 40);
        let state = Arc::clone(&t.state);
        let fed = Arc::new(AtomicUsize::new(0));
        let fed_in_worker = Arc::clone(&fed);
        let feeder = std::thread::spawn(move || {
            for _ in 0..CHUNKS {
                tasty_utils::poison::recover_mutex(
                    state.lock(),
                    STATE_WHAT,
                    &STATE_POISON_REPORTED,
                )
                .ingest(CHUNK);
                fed_in_worker.fetch_add(1, Ordering::Release);
            }
        });
        while fed.load(Ordering::Acquire) < CHUNKS / 4 {
            std::hint::spin_loop();
        }
        let (snapshot, tap) = subscribe(&mut t);
        feeder.join().expect("feeder");
        let in_snapshot = snapshot.iter().filter(|&&b| b == b'x').count();
        let tapped: usize = tap.try_iter().map(|c| c.len()).sum();
        if in_snapshot + tapped != CHUNKS * CHUNK.len() {
            mismatches += 1;
        }
    }
    mismatches
}

#[test]
fn snapshot_and_tap_accounts_for_output_ingested_concurrently() {
    let mismatches = attach_subscription_accounts_for_every_byte(|t| {
        let sub = t.snapshot_and_tap();
        (sub.snapshot, sub.output)
    });
    assert_eq!(
        mismatches, 0,
        "snapshot과 tap 사이에 들어온 출력이 빠지거나 겹쳤다"
    );
}

// 제목 변경(OSC 0/1/2, Sun 형식, XTWINOPS push/pop)만 있는 출력은 busy 판정의 활동이 아니다.
// 입력을 기다리며 제목만 깜빡이는 프로그램(Codex)이 일하는 것처럼 보이지 않게 한다.

#[test]
fn title_only_output_does_not_keep_busy() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("codex", 2);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    t.process_bytes(b"\x1b]0;[ ! ] Action Required | tasty\x07");
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
    t.process_bytes(b"\x1b]2;title\x07\x1b]1;icon\x07\x1b[22;0t\x1b[23;0t");
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn title_with_content_output_is_busy() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("codex", 2);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    t.process_bytes(b"\x1b]0;working\x07x");
    assert!(t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn split_title_osc_does_not_count_as_activity() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("codex", 2);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    t.process_bytes(b"\x1b]0;[ . ] Action Req");
    t.process_bytes(b"uired | tasty\x07");
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn every_title_form_is_excluded() {
    // OSC 0/1/2(BEL·ST 종결), Sun 형식(OSC l / L), push/pop 의 ;0 ;1 ;2
    let forms: [&[u8]; 10] = [
        b"\x1b]0;a\x07",
        b"\x1b]0;a\x1b\\",
        b"\x1b]1;a\x07",
        b"\x1b]2;a\x07",
        b"\x1b]la\x1b\\",
        b"\x1b]La\x1b\\",
        b"\x1b[22;0t",
        b"\x1b[22;1t",
        b"\x1b[22;2t",
        b"\x1b[23;0t\x1b[23;1t\x1b[23;2t",
    ];
    let fg = fake_fg("codex", 2);
    for f in forms {
        let mut t = Terminal::new_detached(80, 24);
        set_times(
            &t,
            BUSY_OUTPUT_WINDOW + ms(500),
            BUSY_OUTPUT_WINDOW + ms(1000),
        );
        t.process_bytes(f);
        assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)), "{f:?}");
    }
}

#[test]
fn title_with_non_title_control_is_busy() {
    // 제목과 동기화 출력·커서 이동·질의가 함께 오면 활동이다. Mode 분기의 continue 보다 앞에서
    // 판정해야 통과한다.
    let forms: [&[u8]; 3] = [
        b"\x1b[?2026h\x1b]0;a\x07\x1b[?2026l",
        b"\x1b]0;a\x07\x1b[5;1H",
        b"\x1b]0;a\x07\x1b[6n",
    ];
    let fg = fake_fg("codex", 2);
    for f in forms {
        let mut t = Terminal::new_detached(80, 24);
        set_times(
            &t,
            BUSY_OUTPUT_WINDOW + ms(500),
            BUSY_OUTPUT_WINDOW + ms(1000),
        );
        t.process_bytes(f);
        assert!(t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)), "{f:?}");
    }
}

#[test]
fn st_terminated_non_title_strings_and_other_escapes_alone_are_busy() {
    // ST 는 항상 중립이지만 그것이 끝내는 제목이 아닌 문자열은 따로 활동이 된다. ST 가 아닌 ESC 코드도
    // 활동이다. 모든 OSC 나 모든 ESC 를 중립으로 넓히면 실패한다.
    let forms: [&[u8]; 7] = [
        b"\x1b]8;;https://example.com\x1b\\",
        b"\x1b]52;c;aGk=\x1b\\",
        b"\x1b]133;A\x1b\\",
        b"\x1bP$qm\x1b\\",
        b"\x1b7",
        b"\x1bM",
        b"\x1b=",
    ];
    let fg = fake_fg("codex", 2);
    for f in forms {
        let mut t = Terminal::new_detached(80, 24);
        set_times(
            &t,
            BUSY_OUTPUT_WINDOW + ms(500),
            BUSY_OUTPUT_WINDOW + ms(1000),
        );
        t.process_bytes(f);
        assert!(t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)), "{f:?}");
    }
}

#[test]
fn split_title_first_piece_alone_is_not_activity() {
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("codex", 2);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    t.process_bytes(b"\x1b]0;[ . ] Action Req");
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn busy_expires_when_only_titles_follow() {
    // 이미 busy 인 프로그램이 제목만 계속 바꾸면 창이 지난 뒤 해제된다.
    let mut t = Terminal::new_detached(80, 24);
    let fg = fake_fg("codex", 2);
    enter_busy(&mut t, &fg);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    t.process_bytes(b"\x1b]0;[ ! ] Action Required\x07");
    assert!(!t.busy_with_foreground(FAKE_SHELL_PID, Some(&fg)));
}

#[test]
fn title_only_output_still_refreshes_last_output_at() {
    // set_times 가 last_output_at 도 과거로 돌려 놓아야 이 시험이 결함을 검출한다.
    let mut t = Terminal::new_detached(80, 24);
    set_times(
        &t,
        BUSY_OUTPUT_WINDOW + ms(500),
        BUSY_OUTPUT_WINDOW + ms(1000),
    );
    t.process_bytes(b"\x1b]0;title\x07");
    assert!(t.last_output_at().elapsed() < BUSY_OUTPUT_WINDOW);
}

/// 첫 출력 시각은 `OutputAppended` 게이트가 꺼져 있어도 기록되고 이후 출력으로 바뀌지 않는다.
#[test]
fn first_output_at_is_recorded_with_output_events_disabled() {
    let mut t = Terminal::new_detached(80, 24);
    t.set_output_events_enabled(false);
    assert_eq!(t.first_output_at(), None);
    t.feed_bytes(b"$ ");
    let first = t.first_output_at().expect("first output");
    t.feed_bytes(b"more");
    assert_eq!(t.first_output_at(), Some(first));
}
