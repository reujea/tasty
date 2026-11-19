//! 칸 서명 비교. 렌더러와 같은 칠 함수로 실제 터미널 격자를 훑으며 GPU 장치 없이 돈다.

use super::CursorPaint;
use super::cell_paint::CellOverlays;
use super::change_track::*;
use tasty_terminal::Terminal;
use tasty_type_appearance::color::{GpuRgb, GpuRgba};

fn rgba(v: f32) -> GpuRgba {
    GpuRgba::dangerously_force_from_array([v, v, v, 1.0])
}

fn ansi() -> [GpuRgb; 16] {
    [GpuRgb::dangerously_force_from_array([0.2, 0.3, 0.4]); 16]
}

fn no_overlays<'a>() -> CellOverlays<'a> {
    CellOverlays {
        selection: None,
        selection_ink: None,
        vi_cursor: None,
        link: None,
        search: None,
        preedit: None,
    }
}

type Cursor = Option<(usize, usize, bool, CursorPaint)>;

/// 렌더러의 현재 화면 경로처럼 surface 하나를 서명한다.
fn sign_surface(tracker: &mut ChangeTracker, terminal: &Terminal, id: u32, cursor: Cursor) {
    let ansi = ansi();
    terminal.with_content(|view| {
        let (cols, rows) = view.dimensions();
        tracker.begin_surface(id, view.cut().revision, cols, rows);
        let input = SignInput {
            default_bg: rgba(0.05),
            default_fg: rgba(0.8),
            ansi: &ansi,
            cursor,
            overlays: no_overlays(),
            cols,
            rows,
        };
        sign_live_screen(tracker, &input, view.surface(), view.screen_start());
        sign_overlays(tracker, &input, None, view.viewport().top_row);
        tracker.end_surface(&[], &[], (1, 1));
    });
}

fn frame(tracker: &mut ChangeTracker, terminal: &Terminal, cursor: Cursor) -> FrameChanges {
    tracker.begin_frame(true);
    sign_surface(tracker, terminal, 1, cursor);
    tracker.finish_frame().expect("tracking is on")
}

fn terminal_with(bytes: &str) -> Terminal {
    let mut terminal = Terminal::new_detached(10, 3);
    terminal.feed_bytes(bytes.as_bytes());
    terminal
}

/// 처음 그린 surface는 모든 칸과 surface 하나를 바뀐 것으로 센다.
#[test]
fn first_frame_counts_every_cell() {
    let terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    let changes = frame(&mut tracker, &terminal, None);
    assert_eq!(changes.surfaces, 1);
    assert_eq!(changes.cells, 30);
}

/// 같은 격자를 다시 그리면 바뀐 칸도 바뀐 surface도 없다.
#[test]
fn same_grid_twice_changes_nothing() {
    let terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    let changes = frame(&mut tracker, &terminal, None);
    assert_eq!(changes, FrameChanges::default());
}

/// 한 글자를 바꾸면 그 칸 하나만 센다. 출력이 있었으므로 surface도 하나 센다.
#[test]
fn one_changed_character_is_one_cell() {
    let mut terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    terminal.feed_bytes(b"\x1b[1;2HX");
    let changes = frame(&mut tracker, &terminal, None);
    assert_eq!(changes.cells, 1);
    assert_eq!(changes.surfaces, 1);
}

/// 같은 글자라도 색이 바뀌면 바뀐 칸이다.
#[test]
fn attribute_change_is_a_cell_change() {
    let mut terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    terminal.feed_bytes(b"\x1b[1;1H\x1b[31ma");
    assert_eq!(frame(&mut tracker, &terminal, None).cells, 1);
}

/// 넓은 글자는 두 칸을 덮으므로 바뀌면 두 칸을 센다.
#[test]
fn wide_character_change_counts_both_columns() {
    let mut terminal = terminal_with("가");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    terminal.feed_bytes("\x1b[1;1H나".as_bytes());
    assert_eq!(frame(&mut tracker, &terminal, None).cells, 2);
}

/// 블록 커서가 옮겨 가면 떠난 칸과 들어간 칸을 센다. 내용 revision은 그대로다.
#[test]
fn block_cursor_move_counts_two_cells() {
    let terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(
        &mut tracker,
        &terminal,
        Some((1, 0, false, CursorPaint::Block)),
    );
    let changes = frame(
        &mut tracker,
        &terminal,
        Some((2, 0, false, CursorPaint::Block)),
    );
    assert_eq!(changes.cells, 2);
    assert_eq!(changes.surfaces, 0);
}

/// 빈 칸 위의 블록 커서도 칸 하나로 센다.
#[test]
fn block_cursor_on_trailing_cell_counts() {
    let terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    let changes = frame(
        &mut tracker,
        &terminal,
        Some((5, 0, false, CursorPaint::Block)),
    );
    assert_eq!(changes.cells, 1);
}

/// 넓은 글자 위 밑줄 커서는 두 칸을 덮는다.
#[test]
fn underline_cursor_on_wide_character_spans_two_cells() {
    let terminal = terminal_with("가b");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    let changes = frame(
        &mut tracker,
        &terminal,
        Some((0, 0, true, CursorPaint::Underline)),
    );
    assert_eq!(changes.cells, 2);
}

/// 격자 크기가 바뀌면 모든 칸을 바뀐 것으로 센다.
#[test]
fn resized_grid_counts_every_cell() {
    let mut terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    assert!(terminal.resize(12, 3));
    let changes = frame(&mut tracker, &terminal, None);
    assert_eq!(changes.cells, 36);
}

/// 이번 프레임에 그리지 않은 surface는 다음 비교에서 처음 그린 것으로 센다.
#[test]
fn surface_skipped_for_a_frame_counts_as_new() {
    let terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    tracker.begin_frame(true);
    assert_eq!(tracker.finish_frame(), Some(FrameChanges::default()));
    assert_eq!(frame(&mut tracker, &terminal, None).surfaces, 1);
}

/// 계측이 꺼지면 칸 서명을 받지 않고, 켜져 있을 때 보관한 직전 프레임 사본도 버린다.
#[test]
fn disabled_frame_keeps_no_copies() {
    let terminal = terminal_with("abc");
    let mut tracker = ChangeTracker::default();
    frame(&mut tracker, &terminal, None);
    assert!(tracker.retained_surfaces() > 0);

    tracker.begin_frame(false);
    assert_eq!(tracker.retained_surfaces(), 0);
    tracker.begin_surface(1, 0, 10, 3);
    assert!(!tracker.recording());
    tracker.note(0..1, 0, 7);
    tracker.end_surface(&[1, 2], &[3], (1, 1));
    assert_eq!(tracker.finish_frame(), None);
    assert_eq!(tracker.retained_surfaces(), 0);
}

/// 인스턴스 바이트는 같은 순번끼리 비교하고 길이 차이를 더한다.
#[test]
fn instance_bytes_compare_by_position() {
    let mut tracker = ChangeTracker::default();
    tracker.begin_frame(true);
    tracker.begin_surface(1, 0, 1, 1);
    tracker.end_surface(&[1, 1, 2, 2], &[9, 9], (2, 2));
    tracker.finish_frame();

    tracker.begin_frame(true);
    tracker.begin_surface(1, 0, 1, 1);
    tracker.end_surface(&[1, 1, 3, 3, 4, 4], &[9, 9], (2, 2));
    let changes = tracker.finish_frame().expect("on");
    // 둘째 bg 인스턴스가 다르고(2바이트) 셋째가 새로 생겼다(2바이트).
    assert_eq!(changes.instance_bytes, 4);
}
