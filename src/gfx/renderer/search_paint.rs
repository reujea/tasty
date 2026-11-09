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
    /// 반전 화면이면 없어 선택 위 일반 매치도 셀 글자를 유지한다.
    pub inactive_ink: Option<GpuRgba>,
    /// 선택 위 현재 매치의 바탕. 커서 칸은 `active_bg`를 쓴다.
    pub active_selected_bg: GpuRgba,
    /// 선택 위 매치의 글자. 현재 매치 안 커서 칸도 이 글자를 쓴다.
    pub selected_fg: GpuRgba,
}

impl<'a> SearchHighlights<'a> {
    /// 테마에서 매치 칠을 고른다. 현재 매치 글자만 포커스에 따라 다르다.
    pub fn for_theme(
        theme: &tasty_type_appearance::theme::Theme,
        focused: bool,
        epoch: tasty_terminal::ContentEpoch,
        matches: &'a [tasty_terminal::search::SearchMatch],
        active_index: usize,
    ) -> Self {
        Self {
            epoch,
            matches,
            active_index,
            inactive_bg: theme.search_match_bg.to_gpu_rgba(),
            active_bg: theme.search_match_active_bg.to_gpu_rgba(),
            active_fg: if focused {
                theme.search_match_active_fg.to_gpu_rgba()
            } else {
                theme.search_match_active_unfocused_fg().to_gpu_rgba()
            },
            inactive_ink: Some(theme.text_primary().to_gpu_rgba()),
            active_selected_bg: theme.search_match_active_selected_bg().to_gpu_rgba(),
            selected_fg: theme.search_match_selected_fg().to_gpu_rgba(),
        }
    }
}

/// 칸이 속한 매치.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MatchKind {
    Ordinary,
    Active,
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

    pub(super) fn kind_at(&self, col: usize, row: usize) -> Option<MatchKind> {
        let i = self
            .matches
            .iter()
            .position(|m| m.row == row && col >= m.col_start && col < m.col_end)?;
        Some(if i == self.active_index {
            MatchKind::Active
        } else {
            MatchKind::Ordinary
        })
    }

    /// 매치 칸의 바탕에 강조색을 합성하고 글자를 정한다. `cursor_cell`은 커서가 글자를 정한 칸이다.
    /// 현재 매치는 모든 글자를 덮고, 일반 매치는 기본 글자 셀만 바꾼다.
    /// 선택 위 글자와 현재 매치 안 커서 칸의 글자는 어두운 값 하나다. 선택 위 현재 매치 바탕은 커서 칸이 아닐 때만 따로 있다.
    pub(super) fn paint(
        &self,
        kind: MatchKind,
        attrs: &termwiz::cell::CellAttributes,
        cursor_cell: bool,
        selected: bool,
        bg: &mut GpuRgba,
        fg: &mut GpuRgba,
    ) {
        match kind {
            MatchKind::Active => {
                let fill = if selected && !cursor_cell {
                    self.active_selected_bg
                } else {
                    self.active_bg
                };
                *bg = composite_over(fill, *bg);
                *fg = if selected || cursor_cell {
                    self.selected_fg
                } else {
                    self.active_fg
                };
            }
            MatchKind::Ordinary => {
                *bg = composite_over(self.inactive_bg, *bg);
                if let Some(ink) = self.inactive_ink
                    && !cursor_cell
                    && reads_default_ink(attrs)
                {
                    *fg = if selected { self.selected_fg } else { ink };
                }
            }
        }
    }
}
