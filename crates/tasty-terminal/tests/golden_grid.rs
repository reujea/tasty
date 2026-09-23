//! Golden cell-grid snapshot tests for the terminal render-critical path.
//!
//! These pin the *deterministic text representation* of the cell grid produced
//! by the production VTE ingest path (`Terminal::new_detached` → `feed_bytes`,
//! the same handlers a real PTY drives). The input sequences mirror the
//! high-level commands of `tasty-tui-simulator` (see
//! `crates/tasty-tui-simulator/src/lib.rs` — e.g. `cursor`, `print`, `bold`,
//! `scroll-region`), but are written as raw escapes so the test needs no GUI
//! surface and runs headless inside `cargo test --workspace`. This test runs
//! automatically in the headless combination only (`check-headless` runs the whole
//! suite); the default-combination job is `--lib --bins` and never sees integration
//! targets (see `docs/dev-guide/ci-gates.md`).
//!
//! Scope is deliberately narrow (see `docs/dev-guide/e2e-tests.md#tui-테스트-가이드--시뮬레이터--셀-검증--골든-스냅샷`):
//!   - COVERED: cursor positioning/layout, line wrapping, scroll-region scroll,
//!     erase-display, and per-cell SGR attributes (bold/italic/underline/
//!     inverse/strikethrough + palette fg/bg).
//!   - NOT COVERED here: GPU pixel rendering (environment-dependent → unfit for
//!     golden), and chrome/widget layout (lives in `src/view`, needs the GUI
//!     harness). Those remain on manual visual verification
//!     (`docs/ai-verification/screenshot-methods.md#시각-판정-체크리스트`).
//!
//! Why text (not pixels): the grid's deterministic text form is stable across
//! OS/GPU, so a logic regression (e.g. SGR bold no longer applied) flips a
//! golden line and fails the test, while pixel goldens would be flaky.

use tasty_terminal::Terminal;

/// Render the visible grid as deterministic text: one line per row, trailing
/// spaces trimmed, blank rows preserved as empty lines (so the full row count
/// is pinned, not just the populated prefix).
fn grid_text(term: &Terminal) -> String {
    let (_, rows) = term.dimensions();
    (0..rows)
        .map(|r| term.screen_row(r, true))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compact per-cell attribute dump for every populated cell, sorted by
/// `(row, col)`. One line per cell: `"{row},{col} {glyph} {flags}"`.
///
/// `flags` is a comma-joined, fixed-order list of the non-default attributes
/// (so the output is deterministic). A populated cell with no attributes shows
/// `-`. Cells that are an empty default space are skipped entirely.
fn styled_cells(term: &Terminal) -> String {
    let (_, rows) = term.dimensions();
    let mut lines = Vec::new();
    for r in 0..rows {
        for (c, info) in term.row_cells(r) {
            let mut flags = Vec::new();
            if info.intensity == "bold" {
                flags.push("bold".to_string());
            }
            if info.intensity == "half" {
                flags.push("faint".to_string());
            }
            if info.italic {
                flags.push("italic".to_string());
            }
            if info.underline {
                flags.push(format!("underline:{}", info.underline_style));
            }
            if info.inverse {
                flags.push("inverse".to_string());
            }
            if info.strikethrough {
                flags.push("strike".to_string());
            }
            if info.fg != "default" {
                flags.push(format!("fg={}", info.fg));
            }
            if info.bg != "default" {
                flags.push(format!("bg={}", info.bg));
            }

            // Skip blank, fully-default filler cells.
            if info.text == " " && flags.is_empty() {
                continue;
            }

            let flag_str = if flags.is_empty() {
                "-".to_string()
            } else {
                flags.join(",")
            };
            lines.push(format!("{r},{c} {} {flag_str}", info.text));
        }
    }
    lines.join("\n")
}

// ============================================================
// Critical path 1 — cursor absolute positioning + layout
// (simulator: `cursor <r> <c>` / `print`)
// ============================================================

#[test]
fn golden_cursor_positioning_layout() {
    let mut t = Terminal::new_detached(12, 4);
    t.feed_bytes(b"\x1b[2J\x1b[H"); // clear + home
    t.feed_bytes(b"\x1b[1;1HABC"); // row 0, col 0
    t.feed_bytes(b"\x1b[2;4HDE"); // row 1, col 3
    t.feed_bytes(b"\x1b[4;1HZ"); // row 3, col 0

    let expected = "\
ABC
   DE

Z";
    assert_eq!(grid_text(&t), expected);
}

// ============================================================
// Critical path 2 — line wrapping (autowrap onto next row)
// ============================================================

#[test]
fn golden_line_wrapping() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"0123456789abcd");

    let expected = "\
0123456789
abcd
";
    assert_eq!(grid_text(&t), expected);
}

// ============================================================
// Critical path 3 — scroll region (DECSTBM) scroll-up
// (simulator: `scroll-region <top> <bottom>`)
// ============================================================

#[test]
fn golden_scroll_region_scroll_up() {
    let mut t = Terminal::new_detached(8, 5);
    t.feed_bytes(b"\x1b[2J\x1b[H");
    t.feed_bytes(b"r0\r\nr1\r\nr2\r\nr3\r\nr4"); // rows 0..4

    // Limit scrolling to rows 3..5 (1-based) = rows 2..4 (0-based).
    t.feed_bytes(b"\x1b[3;5r");
    // Park on the bottom margin and emit LF → region scrolls up by one.
    t.feed_bytes(b"\x1b[5;1H\n");

    // Rows 0,1 untouched; old r2 evicted; r3→row2, r4→row3, row4 blank.
    let expected = "\
r0
r1
r3
r4
";
    assert_eq!(grid_text(&t), expected);
}

// ============================================================
// Critical path 4 — erase display below cursor (CSI J)
// ============================================================

#[test]
fn golden_erase_display_below() {
    let mut t = Terminal::new_detached(8, 4);
    t.feed_bytes(b"\x1b[2J\x1b[H");
    t.feed_bytes(b"aaa\r\nbbb\r\nccc\r\nddd");
    t.feed_bytes(b"\x1b[2;1H"); // row 1, col 0
    t.feed_bytes(b"\x1b[J"); // erase from cursor to end of display

    let expected = "\
aaa


";
    assert_eq!(grid_text(&t), expected);
}

// ============================================================
// Critical path 5 — per-cell SGR attributes
// (simulator: `bold`/`italic`/`underline`/`inverse`/`strikethrough`/`fg`)
//
// This is the canonical regression guard: breaking SGR application (e.g.
// dropping bold) flips the matching golden line and fails the test.
// ============================================================

#[test]
fn golden_sgr_attributes() {
    let mut t = Terminal::new_detached(20, 1);
    t.feed_bytes(b"\x1b[2J\x1b[H");
    t.feed_bytes(b"\x1b[1mB"); // bold
    t.feed_bytes(b"\x1b[22m\x1b[3mI"); // bold off, italic
    t.feed_bytes(b"\x1b[23m\x1b[4mU"); // italic off, underline
    t.feed_bytes(b"\x1b[24m\x1b[7mR"); // underline off, inverse
    t.feed_bytes(b"\x1b[27m\x1b[9mS"); // inverse off, strikethrough
    t.feed_bytes(b"\x1b[0m\x1b[31mr"); // reset, red (palette 1) fg
    t.feed_bytes(b"\x1b[0mx"); // reset, plain

    let expected = "\
0,0 B bold
0,1 I italic
0,2 U underline:single
0,3 R inverse
0,4 S strike
0,5 r fg=palette:1
0,6 x -";
    assert_eq!(styled_cells(&t), expected);
}
