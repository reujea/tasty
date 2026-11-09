//! 찾기 매치가 칸에 칠하는 바탕과 글자.

use tasty_type_appearance::color::GpuRgba;

use super::overlay::composite_over;
use crate::cell_palette::reads_default_ink;

/// Search match highlights to pass into the renderer.
#[derive(Clone, Copy)]
pub struct SearchHighlights<'a> {
    pub epoch: tasty_terminal::ContentEpoch,
    pub matches: &'a [tasty_terminal::search::SearchMatch],
    pub active_index: usize,
    pub inactive_bg: GpuRgba,
    pub active_bg: GpuRgba,
    /// 현재 매치 안 글리프 색. 셀 자신의 글자색(SGR)을 그 칸에서만 덮어쓴다.
    pub active_fg: GpuRgba,
    /// 일반 매치 안 기본 글자 셀의 글자색. ANSI 색 셀과 현재 매치에는 쓰지 않는다.
    pub inactive_ink: Option<GpuRgba>,
}

impl SearchHighlights<'_> {
    /// DECSCNM 반전 화면에서는 기본 글자가 바탕 색이고 매치 바탕이 글자 색 위에 깔린다.
    /// 여기에 매치 잉크를 칠하면 글자가 바탕에 묻히므로 셀 글자를 그대로 둔다.
    pub(super) fn on_screen(mut self, reversed: bool) -> Self {
        if reversed {
            self.inactive_ink = None;
        }
        self
    }

    /// 칸이 매치 안이면 강조색을 배경에 합성한다. 현재 매치는 글자색도 바꾸고, 일반 매치는
    /// 기본 글자 셀만 바꾼다. `ink_replaced`는 커서가 이미 글자색을 정한 칸이다.
    pub(super) fn paint(
        &self,
        col: usize,
        row: usize,
        attrs: &termwiz::cell::CellAttributes,
        ink_replaced: bool,
        bg: &mut GpuRgba,
        fg: &mut GpuRgba,
    ) {
        let Some(i) = self
            .matches
            .iter()
            .position(|m| m.row == row && col >= m.col_start && col < m.col_end)
        else {
            return;
        };
        if i == self.active_index {
            *bg = composite_over(self.active_bg, *bg);
            *fg = self.active_fg;
            return;
        }
        *bg = composite_over(self.inactive_bg, *bg);
        if let Some(ink) = self.inactive_ink
            && !ink_replaced
            && reads_default_ink(attrs)
        {
            *fg = ink;
        }
    }
}
