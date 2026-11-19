//! 선언이 정하는 배치·포커스 규칙: 포커스를 받지 않는 팝업, 그립으로만 바꾸는 크기, 안내 열,
//! 위에 붙은 팝업이 차지한 띠.

use super::{
    PopupAnchor, PopupDecl, PopupInstanceId, PopupKeyboard, PopupManager, PopupResizeHandle,
    PopupState, ResizeEdges,
};
use tasty_type_geometry::length::{LogicalPx, PhysicalPx};

/// 인스턴스가 선언에서 받아 두는 배치·포커스 값.
#[derive(Debug, Clone, Copy)]
pub(super) struct Arrange {
    /// 거짓이면 열거나 눌러도 키보드 포커스를 받지 않고 맨 앞으로 올라오지 않으며 다른 팝업 아래에 그린다.
    pub(super) takes_focus: bool,
    pub(super) resize_handle: PopupResizeHandle,
    pub(super) anchor: PopupAnchor,
}

impl Arrange {
    /// 선언 없이 만든 팝업의 값.
    pub(super) const PLAIN: Self = Self {
        takes_focus: true,
        resize_handle: PopupResizeHandle::Edges,
        anchor: PopupAnchor::ScopeCenter,
    };

    pub(super) fn of(decl: &PopupDecl) -> Self {
        Self {
            takes_focus: decl.keyboard != PopupKeyboard::None,
            resize_handle: decl.resize_handle,
            anchor: decl.anchor,
        }
    }
}

/// 안내 열의 자리. 위쪽 경계와 가로 범위는 `rect`의 왼쪽·위·폭이다. 호출자가 프레임마다 정한다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoticeColumn {
    pub rect: egui::Rect,
    /// 안내 사이 간격.
    pub gap: LogicalPx,
}

impl PopupState {
    /// 이 위치에서 크기 조절을 시작하면 움직일 변. 그립만 쓰는 팝업은 오른쪽 아래 그립 안에서만 잡힌다.
    pub(super) fn resize_edges_at(&self, pos: egui::Pos2, band: LogicalPx) -> Option<ResizeEdges> {
        let rect = self.popup_rect();
        match self.arrange.resize_handle {
            PopupResizeHandle::Edges => super::draw::resize_edges_at(rect, pos, band.value()),
            PopupResizeHandle::Grip { size } => {
                let grip =
                    egui::Rect::from_min_max(rect.max - egui::Vec2::splat(size.value()), rect.max);
                grip.contains(pos).then_some(ResizeEdges {
                    left: false,
                    right: true,
                    top: false,
                    bottom: true,
                })
            }
        }
    }
}

impl PopupManager {
    /// 안내 열의 자리를 정한다. `None`이면 열에 놓을 팝업을 지금 위치에 둔다.
    pub fn set_notice_column(&mut self, column: Option<NoticeColumn>) {
        self.notice_column = column;
    }

    /// 열에 놓는 보이는 인스턴스를 연 순서로 위에서부터 쌓고, 차지한 사각형의 합을 돌려준다.
    /// 높이는 인스턴스의 지금 크기(직전 프레임에 잰 값)다.
    pub(super) fn place_column(&mut self, open_indices: &[usize]) -> Option<egui::Rect> {
        let column = self.notice_column?;
        let mut stacked: Vec<usize> = open_indices
            .iter()
            .copied()
            .filter(|&i| self.popups[i].arrange.anchor == PopupAnchor::Column)
            .collect();
        stacked.sort_by_key(|&i| self.popups[i].instance);
        let mut top = column.rect.top();
        let mut occupied: Option<egui::Rect> = None;
        for i in stacked {
            let popup = &mut self.popups[i];
            popup.pos = egui::pos2(column.rect.left(), top);
            popup.size.x = column.rect.width();
            popup.request_center = false;
            popup.request_top = false;
            let rect = popup.popup_rect();
            occupied = Some(occupied.map_or(rect, |r| r.union(rect)));
            top = rect.bottom() + column.gap.value();
        }
        occupied
    }

    /// 범위 위쪽에 붙은 보이는 팝업이 차지한 사각형. 다른 배치(안내 열)가 이 띠를 피한다.
    pub fn reserved_top_bands(&self) -> Vec<egui::Rect> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.arrange.anchor == PopupAnchor::ScopeTop)
            .map(|p| p.popup_rect())
            .collect()
    }

    /// 포커스를 받지 않는 보이는 팝업이 이 논리 위치를 덮는지. 직전 프레임에 그린 자리로 판정한다.
    /// 그런 팝업 위의 누름은 팝업이 받으므로 아래 pane을 활성화하거나 선택을 시작하지 않는다.
    pub fn quiet_popup_covers(&self, pos: egui::Pos2) -> bool {
        self.popups.iter().any(|p| {
            p.open && p.scope_visible && !p.arrange.takes_focus && p.popup_rect().contains(pos)
        })
    }

    /// 포커스를 받지 않는 보이는 팝업의 사각형(logical px). native WebView는 숨김·bounds·키보드 포커스를
    /// 그대로 두고 이 부분만 그림과 포인터에서 빼서 팝업이 보이게 한다.
    pub fn quiet_popup_rects(&self) -> Vec<egui::Rect> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && !p.arrange.takes_focus)
            .map(|p| p.popup_rect())
            .collect()
    }

    /// [`Self::quiet_popup_covers`]를 winit 포인터의 물리 좌표로 묻는다. egui가 아직 그 포인터로 프레임을
    /// 돌리지 않은 첫 클릭에도 맞도록 egui의 hover 판정이 아니라 사각형으로 가린다.
    pub fn quiet_popup_covers_physical(
        &self,
        x: PhysicalPx,
        y: PhysicalPx,
        pixels_per_point: f32,
    ) -> bool {
        self.quiet_popup_covers(egui::pos2(
            x.to_logical(pixels_per_point).value(),
            y.to_logical(pixels_per_point).value(),
        ))
    }

    /// 포커스를 받지 않는 인스턴스인지. 없는 인스턴스는 거짓이다.
    pub(super) fn never_focused(&self, iid: PopupInstanceId) -> bool {
        self.instance(iid).is_some_and(|p| !p.arrange.takes_focus)
    }
}
