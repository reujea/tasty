//! 목록 항목의 글리프와 이름 색. 세 보기(Grid·List·Detail)가 같은 규칙을 쓴다.
//! 숨김 항목은 이름과 글리프를 explorer-hidden-fg 로 그린다(불투명도로 흐리게 하지 않는다).
//! 선택한 항목은 surface-active 위이므로 숨김 여부와 관계없이 이름과 글리프가 선택 색이다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{RowParent, RowState};

use super::view::hidden::is_hidden;
use super::view::{DirEntryInfo, ExplorerView};
use crate::adapters::ui::icons::{self, Icon};

fn hidden_fg(theme: &Theme) -> egui::Color32 {
    theme.explorer_hidden_fg().to_egui()
}

/// 엔트리의 아이콘 + glyph 색 (design GridCell/DetailRow/ExpListMini):
/// 폴더/파일 = text-muted, 이미지 파일 = IMAGE 아이콘 + accent-info, 숨김 항목 = explorer-hidden-fg.
/// 대상이 없는 링크는 숨김 여부와 관계없이 링크 경고 글리프와 색을 그대로 쓴다.
/// 선택한 항목의 모양 글리프는 행 잉크(text-on-active)이고, 색이 뜻을 담는 이미지와 깨진 링크는 on-active 띠를 쓴다.
pub(super) fn entry_icon(theme: &Theme, e: &DirEntryInfo, selected: bool) -> (Icon, egui::Color32) {
    if let Some((icon, color)) = super::link::broken_icon(theme, e) {
        let color = if selected {
            theme.accent_warning_on_active().to_egui()
        } else {
            color
        };
        return (icon, color);
    }
    if selected {
        return if e.is_dir {
            (icons::FOLDER, theme.text_on_active().to_egui())
        } else if super::is_image_ext(&e.ext) {
            (icons::IMAGE, theme.accent_info_on_active().to_egui())
        } else {
            (icons::FILE, theme.text_on_active().to_egui())
        };
    }
    let (icon, color) = if e.is_dir {
        (icons::FOLDER, theme.text_muted().to_egui())
    } else if super::is_image_ext(&e.ext) {
        (icons::IMAGE, theme.accent_info_raised().to_egui())
    } else {
        (icons::FILE, theme.text_muted().to_egui())
    };
    (
        icon,
        if is_hidden(e) {
            hidden_fg(theme)
        } else {
            color
        },
    )
}

/// 행 상태와 놓인 바탕에서 고르는 글자. 세 보기가 같은 규칙을 쓴다.
#[derive(Clone, Copy)]
pub(super) struct RowInk {
    state: RowState,
    parent: RowParent,
    cut: bool,
}

impl RowInk {
    pub(super) fn of(state: RowState, parent: RowParent, cut: bool) -> Self {
        Self { state, parent, cut }
    }

    fn resting(view: &ExplorerView, e: &DirEntryInfo, state: RowState, cut: bool) -> Self {
        let state = state.under_drop(view.ops.drag.drops_on(&e.path));
        Self::of(state, RowParent::bed(view.bed_focused), cut)
    }

    /// 목록 행이 쉴 때의 글자. 선택·호버 잉크는 행 위젯이 고른다.
    pub(super) fn at_rest(view: &ExplorerView, e: &DirEntryInfo, cut: bool) -> Self {
        Self::resting(view, e, RowState::Rest, cut)
    }

    /// 자세히 보기의 칸이 지금 놓인 행의 글자.
    pub(super) fn in_table(
        ui: &egui::Ui,
        view: &ExplorerView,
        e: &DirEntryInfo,
        cut: bool,
        selected: bool,
    ) -> Self {
        let hovered = tasty_ui_widgets::in_hovered_table_row(ui);
        Self::resting(view, e, RowState::of(true, selected, hovered, false), cut)
    }

    /// 이름 색. 쉬는 행은 잘라내기 대기면 cut-pending-fg, 숨김 항목이면 explorer-hidden-fg, 아니면 보기마다
    /// 정한 `rest`다. 그 밖의 상태는 행 잉크가 앞선다. 호버 칸의 대비는 쉴 때 색으로는 지켜지지 않는다.
    pub(super) fn name(self, theme: &Theme, e: &DirEntryInfo, rest: HexColor) -> egui::Color32 {
        let rest = if self.cut {
            theme.cut_pending_fg()
        } else if is_hidden(e) {
            theme.explorer_hidden_fg()
        } else {
            rest
        };
        self.state.label(theme, self.parent, rest).to_egui()
    }

    /// 크기·날짜·종류처럼 이름보다 약한 글자.
    pub(super) fn meta(self, theme: &Theme) -> egui::Color32 {
        self.state.meta(theme, self.parent).to_egui()
    }
}
