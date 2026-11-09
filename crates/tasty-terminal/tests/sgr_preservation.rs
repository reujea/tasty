//! SGR attributes keep their values through what copies cells: the mirror snapshot, the pen after
//! it and the blank-row checks of capture and shrink.

use tasty_terminal::{CellInfo, Terminal};

const COMBINED: &[u8] = b"\x1b[1;3;4:3;58;2;10;20;30;7;5;8;9;53;38;5;200;48;2;1;2;3m";

/// The fields a mirror must reproduce; `vertical_align` stays out of the snapshot.
fn attrs(info: &CellInfo) -> [String; 12] {
    [
        info.fg.clone(),
        info.bg.clone(),
        info.intensity.to_string(),
        info.italic.to_string(),
        info.underline_style.to_string(),
        info.underline_color.clone(),
        info.blink.to_string(),
        info.inverse.to_string(),
        info.invisible.to_string(),
        info.strikethrough.to_string(),
        info.overline.to_string(),
        info.width.to_string(),
    ]
}

fn cell(t: &Terminal, row: usize, col: usize) -> CellInfo {
    t.cell_info(row, col).expect("cell")
}

#[test]
fn mirror_snapshot_keeps_underline_style_and_color() {
    let mut server = Terminal::new_detached(80, 24);
    server.feed_bytes(b"\x1b[4:3;58;5;9mX");
    let mut mirror = Terminal::new_detached(80, 24);
    mirror.feed_bytes(&server.snapshot_as_vt());
    let info = mirror.cell_info(0, 0).unwrap();
    assert_eq!(info.underline_style, "curly");
    assert_eq!(info.underline_color, "palette:9");
}

#[test]
fn mirror_first_screen_matches_the_source_for_each_attribute() {
    let inputs: &[&[u8]] = &[
        b"\x1b[1m",
        b"\x1b[2m",
        b"\x1b[3m",
        b"\x1b[4m",
        b"\x1b[21m",
        b"\x1b[4:3m",
        b"\x1b[4:4m",
        b"\x1b[4:5m",
        b"\x1b[58;5;9m",
        b"\x1b[58;2;10;20;30m",
        b"\x1b[58:2::10:20:30m",
        b"\x1b[5m",
        b"\x1b[6m",
        b"\x1b[7m",
        b"\x1b[8m",
        b"\x1b[9m",
        b"\x1b[53m",
        b"\x1b[4:4;58:5:200;53m",
        COMBINED,
    ];
    for input in inputs {
        let mut server = Terminal::new_detached(20, 3);
        server.feed_bytes(input);
        server.feed_bytes(b"X\x1b[0mY");
        let mut mirror = Terminal::new_detached(20, 3);
        mirror.feed_bytes(&server.snapshot_as_vt());
        for col in 0..2 {
            assert_eq!(
                attrs(&cell(&mirror, 0, col)),
                attrs(&cell(&server, 0, col)),
                "input {:?} col {col}",
                String::from_utf8_lossy(input)
            );
        }
    }
}

/// The colon form keeps a parser that ignores SGR 58 from reading its arguments as blink or strike.
#[test]
fn snapshot_writes_the_underline_color_in_colon_form() {
    let mut server = Terminal::new_detached(20, 3);
    server.feed_bytes(b"\x1b[4:3;58;5;9mX\x1b[0m\x1b[58;2;10;20;30mY");
    let bytes = String::from_utf8(server.snapshot_as_vt()).unwrap();
    assert!(bytes.contains("4:3;58:5:9"), "{bytes:?}");
    assert!(bytes.contains("58:2::10:20:30"), "{bytes:?}");
}

#[test]
fn output_after_the_first_screen_keeps_the_pen() {
    let mut server = Terminal::new_detached(20, 3);
    server.feed_bytes(b"A");
    server.feed_bytes(COMBINED);
    let mut mirror = Terminal::new_detached(20, 3);
    mirror.feed_bytes(&server.snapshot_as_vt());
    server.feed_bytes(b"X");
    mirror.feed_bytes(b"X");
    assert_eq!(cell(&server, 0, 1).underline_style, "curly");
    assert_eq!(attrs(&cell(&mirror, 0, 1)), attrs(&cell(&server, 0, 1)));
    // The pen is not painted onto the screen cells that were written before it.
    assert_eq!(attrs(&cell(&mirror, 0, 0)), attrs(&cell(&server, 0, 0)));
}

#[test]
fn attach_subscription_snapshot_restores_the_pen() {
    let mut server = Terminal::new_detached(20, 3);
    server.feed_bytes(b"\x1b[4m");
    let subscription = server.snapshot_and_stream();
    let mut mirror = Terminal::new_detached(20, 3);
    mirror.feed_bytes(&subscription.snapshot);
    server.feed_bytes(b"X");
    mirror.feed_bytes(b"X");
    assert_eq!(cell(&server, 0, 0).underline_style, "single");
    assert_eq!(cell(&mirror, 0, 0).underline_style, "single");
}

/// The readonly copy feeds a new snapshot into the same terminal on every change, so the pen
/// restored by the last one must not reach the next screen.
#[test]
fn repeated_snapshot_into_one_mirror_stays_identical() {
    let mut server = Terminal::new_detached(10, 3);
    server.feed_bytes(b"plain\x1b[41;4m");
    let mut mirror = Terminal::new_detached(10, 3);
    mirror.feed_bytes(&server.snapshot_as_vt());
    mirror.feed_bytes(&server.snapshot_as_vt());
    for col in 0..5 {
        assert_eq!(attrs(&cell(&mirror, 0, col)), attrs(&cell(&server, 0, col)));
    }
    // Past the text no cell may carry the restored pen's background.
    for row in 0..3 {
        for col in 5..10 {
            let bg = mirror.cell_info(row, col).map(|c| c.bg);
            assert!(
                bg.is_none() || bg.as_deref() == Some("default"),
                "row {row} col {col} bg {bg:?}"
            );
        }
    }
    server.feed_bytes(b"X");
    mirror.feed_bytes(b"X");
    assert_eq!(attrs(&cell(&mirror, 0, 5)), attrs(&cell(&server, 0, 5)));
}

/// A snapshot covers only the source's rows and columns. In a larger receiver, such as the outer
/// terminal of a raw bridge that resyncs after a loss, the rest stays as the erase left it, so the
/// erase must not use the pen the previous snapshot restored.
#[test]
fn repeated_snapshot_into_a_larger_receiver_does_not_paint_outside_the_source() {
    let mut server = Terminal::new_detached(10, 3);
    server.feed_bytes(b"abc\x1b[41;4m");
    let mut receiver = Terminal::new_detached(20, 5);
    for round in 0..2 {
        receiver.feed_bytes(&server.snapshot_as_vt());
        for (row, col) in [(0, 15), (4, 0)] {
            let bg = receiver.cell_info(row, col).map(|c| c.bg);
            assert!(
                bg.is_none() || bg.as_deref() == Some("default"),
                "round {round} row {row} col {col} bg {bg:?}"
            );
        }
    }
    // The pen still reaches what follows.
    receiver.feed_bytes(b"X");
    assert_eq!(cell(&receiver, 0, 3).bg, "palette:1");
}

#[test]
fn decorated_blank_last_row_is_kept_by_the_capture() {
    for sgr in ["4", "9", "53", "7", "4:3"] {
        let mut t = Terminal::new_detached(10, 4);
        t.feed_bytes(format!("x\r\n\x1b[{sgr}m   \x1b[0m").as_bytes());
        assert_eq!(t.screen_snapshot_lines().len(), 2, "SGR {sgr}");
    }
    let mut plain = Terminal::new_detached(10, 4);
    plain.feed_bytes(b"x\r\n   ");
    assert_eq!(plain.screen_snapshot_lines().len(), 1);
    for undecorated in ["1", "3", "41"] {
        let mut t = Terminal::new_detached(10, 4);
        t.feed_bytes(format!("x\r\n\x1b[{undecorated}m   \x1b[0m").as_bytes());
        assert_eq!(t.screen_snapshot_lines().len(), 1, "SGR {undecorated}");
    }
}

#[test]
fn decorated_blank_row_is_kept_when_rows_shrink() {
    let build = |row: &str| {
        let mut t = Terminal::new_detached(10, 4);
        t.set_scrollback_limit(100);
        t.feed_bytes(format!("x\r\ny\r\n{row}\x1b[1;2H").as_bytes());
        t.resize(10, 2);
        t
    };
    let blank = build("   ");
    assert_eq!(blank.scrollback_len(), 0, "plain blank rows are dropped");
    assert_eq!(blank.screen_row(0, true), "x");

    let decorated = build("\x1b[4m   \x1b[0m");
    assert_eq!(
        decorated.scrollback_len(),
        1,
        "the row stays, so the top scrolls off"
    );
    assert_eq!(decorated.screen_row(0, true), "y");
}

#[test]
fn record_of_a_decorated_blank_row_round_trips_through_the_blob() {
    use tasty_terminal::disk_scrollback::{deserialize_lines, serialize_lines};
    let mut t = Terminal::new_detached(10, 4);
    t.feed_bytes(b"x\r\n\x1b[4:3;58;5;9m   \x1b[0m");
    let lines = t.screen_snapshot_lines();
    let out = deserialize_lines(&serialize_lines(&lines)).unwrap();
    assert_eq!(out.len(), 2);
    let cells = out[1].to_cells();
    assert_eq!(cells[0].1.underline(), termwiz::cell::Underline::Curly);
}
