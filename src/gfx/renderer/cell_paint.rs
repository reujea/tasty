//! 한 칸의 바탕과 글자를 정한다. 현재 화면과 scrollback 경로가 같은 층을 같은 순서로 칠한다.

use tasty_selection::{NormalizedSelection, SelectionPoint};
use tasty_terminal_link::LinkHighlight;
use tasty_type_appearance::color::{GpuRgb, GpuRgba};
use termwiz::cell::CellAttributes;

use super::SearchHighlights;
use super::overlay::composite_over;
use crate::cell_palette::{compute_cell_colors, reads_default_ink};

/// 셀 위에 겹치는 상태. 칸마다 같은 값을 쓰므로 한 viewport에서 한 번 만든다.
#[derive(Clone, Copy)]
pub(super) struct CellOverlays<'a> {
    /// 범위, 바탕, 기본 글자 셀의 글자색.
    pub selection: Option<&'a (NormalizedSelection, GpuRgba, GpuRgba)>,
    /// 화면 반전이면 없다. 기본 글자가 바탕 색이라 그대로 둔다.
    pub selection_ink: Option<GpuRgba>,
    pub vi_cursor: Option<&'a (SelectionPoint, GpuRgba, GpuRgba)>,
    pub link: Option<&'a LinkHighlight>,
    pub search: Option<&'a SearchHighlights<'a>>,
}

/// 셀 색 위에 커서, 선택, vi 커서, 링크, 매치 순서로 칠한 (바탕, 글자)를 돌려준다.
// 이유: 칸 좌표, 셀 속성, 기본 색, 팔레트, 커서를 칸마다 따로 받는다. 묶으면 호출마다 구조체를 새로 만든다.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_cell(
    col: usize,
    absolute_row: usize,
    attrs: &CellAttributes,
    default_bg: GpuRgba,
    default_fg: GpuRgba,
    ansi: &[GpuRgb; 16],
    block_cursor: bool,
    overlays: &CellOverlays<'_>,
) -> (GpuRgba, GpuRgba) {
    let (mut bg, mut fg) = compute_cell_colors(attrs, default_bg, default_fg, ansi);
    let mut ink_replaced = false;
    if block_cursor {
        std::mem::swap(&mut bg, &mut fg);
        ink_replaced = true;
    }
    if let Some((sel, sel_bg, _)) = overlays.selection
        && tasty_selection::is_selected(col, absolute_row, sel)
    {
        bg = composite_over(*sel_bg, bg);
        if let Some(ink) = overlays.selection_ink
            && !ink_replaced
            && reads_default_ink(attrs)
        {
            fg = ink;
        }
    }
    if let Some((pt, cursor_bg, cursor_fg)) = overlays.vi_cursor
        && pt.col == col
        && pt.absolute_row == absolute_row
    {
        bg = composite_over(*cursor_bg, bg);
        fg = *cursor_fg;
        ink_replaced = true;
    }
    if let Some(link) = overlays.link
        && link.covers(col, absolute_row)
    {
        bg = composite_over(link.bg, bg);
        fg = link.fg;
        ink_replaced = true;
    }
    if let Some(search) = overlays.search {
        search.paint(col, absolute_row, attrs, ink_replaced, &mut bg, &mut fg);
    }
    (bg, fg)
}

/// viewport 하나에 적용할 화면 값. 화면 반전과 설정을 함께 본다.
pub(super) struct ScreenPaint<'a> {
    pub search: Option<SearchHighlights<'a>>,
    pub selection_ink: Option<GpuRgba>,
    pub default_bg: GpuRgba,
    pub default_fg: GpuRgba,
}

/// 설정이 켜져 있을 때만 DECSCNM으로 기본 색을 맞바꾼다. 꺼져 있어도 터미널 모드 자체는 유지해
/// 조회 결과는 바꾸지 않는다. 지난 content epoch의 매치는 칠하지 않는다.
pub(super) fn screen_paint<'a>(
    view: &tasty_terminal::TerminalReadView<'_>,
    reverse_screen_enabled: bool,
    search: Option<&SearchHighlights<'a>>,
    selection_ink: Option<GpuRgba>,
    default_bg: GpuRgba,
    default_fg: GpuRgba,
) -> ScreenPaint<'a> {
    let epoch = view.cut().epoch;
    let reversed = reverse_screen_enabled && view.screen_reverse();
    let search = search
        .filter(|s| s.epoch == epoch)
        .map(|s| s.on_screen(reversed));
    let (default_bg, default_fg) = if reversed {
        (default_fg, default_bg)
    } else {
        (default_bg, default_fg)
    };
    ScreenPaint {
        search,
        selection_ink: selection_ink.filter(|_| !reversed),
        default_bg,
        default_fg,
    }
}
