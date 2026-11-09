//! 한 칸의 바탕과 글자를 정한다. 현재 화면과 scrollback 경로가 같은 층을 같은 순서로 칠한다.

use tasty_selection::{NormalizedSelection, SelectionPoint};
use tasty_terminal_link::LinkHighlight;
use tasty_type_appearance::color::{GpuRgb, GpuRgba};
use termwiz::cell::CellAttributes;

use super::overlay::composite_over;
use super::{CursorPaint, RenderPreedit, SearchHighlights};
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
    /// 조합 중인 글자. 덮는 칸은 칸의 선을 긋지 않는다.
    pub preedit: Option<&'a RenderPreedit>,
}

/// 한 칸의 칠. `underline`은 포인터 아래 링크의 밑줄 색이다.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct CellPaint {
    pub bg: GpuRgba,
    pub fg: GpuRgba,
    pub underline: Option<GpuRgba>,
}

/// 셀 색 위에 커서, 선택, vi 커서, 링크, 매치 순서로 칠한다.
/// 링크 칸은 어느 매치에서나 선택 칸으로 읽는다. 커서 칸은 링크 안에서도 커서 칸이고 밑줄은 그 글자색이다.
/// `cursor`는 이 칸에 걸린 텍스트 커서 모양이고, 블록만 바탕과 글자를 맞바꾼다.
/// 블록 커서 칸은 선택을 칠하지 않는다. 선택 위에서 맞바꾼 글자가 등급에 못 미치고 커서 위치가 안 보인다.
// 이유: 칸 좌표, 셀 속성, 기본 색, 팔레트, 커서를 칸마다 따로 받는다. 묶으면 호출마다 구조체를 새로 만든다.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_cell(
    col: usize,
    absolute_row: usize,
    attrs: &CellAttributes,
    default_bg: GpuRgba,
    default_fg: GpuRgba,
    ansi: &[GpuRgb; 16],
    cursor: Option<CursorPaint>,
    overlays: &CellOverlays<'_>,
) -> CellPaint {
    let block_cursor = cursor == Some(CursorPaint::Block);
    let (mut bg, mut fg) = compute_cell_colors(attrs, default_bg, default_fg, ansi);
    let match_kind = overlays.search.and_then(|s| s.kind_at(col, absolute_row));
    let mut cursor_cell = false;
    if block_cursor {
        std::mem::swap(&mut bg, &mut fg);
        cursor_cell = true;
    }
    let selection = overlays
        .selection
        .filter(|(sel, ..)| tasty_selection::is_selected(col, absolute_row, sel));
    if let Some((_, sel_bg, _)) = selection
        && !block_cursor
    {
        bg = composite_over(*sel_bg, bg);
        if let Some(ink) = overlays.selection_ink
            && !cursor_cell
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
        cursor_cell = true;
    }
    let link = overlays.link.filter(|l| l.covers(col, absolute_row));
    if let Some(link) = link
        && !cursor_cell
    {
        return CellPaint {
            bg: composite_over(link.bg, bg),
            fg: link.fg,
            underline: Some(link.underline),
        };
    }
    if let (Some(search), Some(kind)) = (overlays.search, match_kind) {
        let selected = selection.is_some();
        search.paint(kind, attrs, cursor_cell, selected, &mut bg, &mut fg);
    }
    CellPaint {
        bg,
        fg,
        underline: link.map(|_| fg),
    }
}

/// 이 칸에 걸린 텍스트 커서 모양. 넓은 글자의 커서는 오른쪽 칸까지 덮는다.
pub(super) fn cursor_paint_at(
    cursor: Option<(usize, usize, bool, CursorPaint)>,
    col: usize,
    row: usize,
) -> Option<CursorPaint> {
    cursor
        .filter(|&(cx, cy, wide, _)| row == cy && (col == cx || (wide && col == cx + 1)))
        .map(|(.., shape)| shape)
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
