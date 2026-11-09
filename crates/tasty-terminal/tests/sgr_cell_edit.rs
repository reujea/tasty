//! DCH, ICH and insert mode move cells with their own attributes; new cells follow the erase rule.
//! Rule: docs/surfaces/terminal/index.md#스크롤-영역과-소거.

use tasty_terminal::{CellInfo, Terminal};

fn cell(t: &Terminal, row: usize, col: usize) -> CellInfo {
    t.cell_info(row, col).expect("cell")
}

#[test]
fn dch_keeps_the_attributes_of_shifted_cells() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"\x1b[4ma\x1b[0mb\x1b[1G\x1b[P");
    let b = cell(&t, 0, 0);
    assert_eq!(b.text, "b");
    assert_eq!(b.underline_style, "none");

    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"x\x1b[7;41ma\x1b[0mb\x1b[1G\x1b[P\x1b[0m");
    let a = cell(&t, 0, 0);
    assert_eq!(a.text, "a");
    assert!(a.inverse);
    assert_eq!(a.bg, "palette:1");
    assert!(!cell(&t, 0, 1).inverse);
}

#[test]
fn dch_under_an_underline_pen_leaves_the_tail_erased() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"ab\x1b[1G\x1b[4;41m\x1b[P");
    assert_eq!(cell(&t, 0, 0).text, "b");
    assert_eq!(cell(&t, 0, 0).underline_style, "none");
    assert_eq!(cell(&t, 0, 0).bg, "default");
    for col in 1..10 {
        assert_eq!(cell(&t, 0, col).underline_style, "none", "col {col}");
    }
    // Only the vacated column takes the current background; shifted blanks keep theirs.
    assert_eq!(cell(&t, 0, 8).bg, "default");
    assert_eq!(cell(&t, 0, 9).bg, "palette:1");
    // The pen stays for the next character.
    t.feed_bytes(b"Z");
    assert_eq!(cell(&t, 0, 0).underline_style, "single");
}

#[test]
fn ich_keeps_the_attributes_of_shifted_cells() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"\x1b[4mab\x1b[0m\x1b[1G\x1b[@");
    assert_eq!(cell(&t, 0, 0).underline_style, "none");
    assert_eq!(cell(&t, 0, 1).text, "a");
    assert_eq!(cell(&t, 0, 1).underline_style, "single");
    assert_eq!(cell(&t, 0, 2).text, "b");
    assert_eq!(cell(&t, 0, 2).underline_style, "single");
    assert_eq!(cell(&t, 0, 3).underline_style, "none");
}

#[test]
fn ich_blank_columns_follow_the_erase_rule() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"ab\x1b[1G\x1b[4;41m\x1b[2@");
    for col in 0..2 {
        assert_eq!(cell(&t, 0, col).underline_style, "none");
        assert_eq!(cell(&t, 0, col).bg, "palette:1");
    }
    assert_eq!(cell(&t, 0, 2).text, "a");
    assert_eq!(cell(&t, 0, 2).bg, "default");
}

#[test]
fn insert_mode_keeps_shifted_cells_and_prints_with_the_pen() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes(b"\x1b[7mab\x1b[0m\x1b[1G\x1b[4h\x1b[4mZ\x1b[0m");
    assert_eq!(cell(&t, 0, 0).text, "Z");
    assert_eq!(cell(&t, 0, 0).underline_style, "single");
    assert!(!cell(&t, 0, 0).inverse);
    for (col, text) in [(1, "a"), (2, "b")] {
        assert_eq!(cell(&t, 0, col).text, text);
        assert!(cell(&t, 0, col).inverse);
        assert_eq!(cell(&t, 0, col).underline_style, "none");
    }
    assert_eq!(cell(&t, 0, 3).underline_style, "none");
}

/// Wide cells move as one and keep their attributes; one that no longer fits falls off.
#[test]
fn wide_cells_keep_attributes_through_dch_and_ich() {
    let mut t = Terminal::new_detached(6, 3);
    t.feed_bytes("x\x1b[4m가\x1b[0my\x1b[1G\x1b[P".as_bytes());
    assert_eq!(cell(&t, 0, 0).text, "가");
    assert_eq!(cell(&t, 0, 0).width, 2);
    assert_eq!(cell(&t, 0, 0).underline_style, "single");
    assert_eq!(cell(&t, 0, 2).text, "y");
    assert_eq!(cell(&t, 0, 2).underline_style, "none");

    let mut t = Terminal::new_detached(6, 3);
    t.feed_bytes("ab\x1b[4m가\x1b[0m\x1b[1G\x1b[3@".as_bytes());
    assert_eq!(cell(&t, 0, 3).text, "a");
    assert_eq!(cell(&t, 0, 5).text, " ", "the wide cell no longer fits");
    assert_eq!(cell(&t, 0, 5).underline_style, "none");

    let mut t = Terminal::new_detached(6, 3);
    t.feed_bytes("ab\x1b[4m가\x1b[0m\x1b[1G\x1b[2@".as_bytes());
    assert_eq!(cell(&t, 0, 4).text, "가", "the wide cell fits at the edge");
    assert_eq!(cell(&t, 0, 4).underline_style, "single");

    let mut t = Terminal::new_detached(6, 3);
    t.feed_bytes("\x1b[4m가\x1b[0my\x1b[1G\x1b[P".as_bytes());
    assert_eq!(
        cell(&t, 0, 0).text,
        "y",
        "DCH of one column removes the whole wide cell"
    );
    assert_eq!(cell(&t, 0, 0).underline_style, "none");
}

/// Starting on the second column of a wide cell, that column reads as a default blank. Without it
/// every cell to its right lands one column off.
#[test]
fn ich_from_the_second_column_of_a_wide_cell_keeps_later_columns() {
    let mut t = Terminal::new_detached(10, 3);
    t.feed_bytes("가\x1b[4mx\x1b[0my\x1b[2G\x1b[@".as_bytes());
    assert_eq!(cell(&t, 0, 3).text, "x");
    assert_eq!(cell(&t, 0, 3).underline_style, "single");
    assert_eq!(cell(&t, 0, 4).text, "y");
    assert_eq!(cell(&t, 0, 4).underline_style, "none");
}
