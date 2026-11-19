//! 팝업 밖 누름과 덮는 팝업의 포인터 판정.
//!
//! 포인터 대상은 위에서부터 찾는다. 덮는 팝업(`covers_scope`)을 만나면 그 배치 범위 아래로 내려가지 않는다.
//! 팝업 상자 밖이면 그 팝업 자신(scrim)이 대상이다. 바깥 누름 알림은 누른 좌표가 팝업의 배치 범위 안이고
//! 덮인 노드가 아닌 팝업에만 간다.

use crate::adapters::ui::LayoutContext;

use super::occlusion::Occluder;
use super::route::RoutedPress;
use super::{PopupCloseReason, PopupInstanceId, PopupManager, PopupState};

/// 덮는 사각형 위 egui 입력을 받는 빈 레이어를 그린다. 그 아래 위젯(사이드바·탭 바·탐색기·상태바·배너)은
/// 누름·hover·휠을 받지 않는다. 호출자는 이 레이어를 팝업 레이어 바로 아래에 둔다. egui는 직전 프레임의 레이어로
/// 판정하므로 열린 다음 프레임부터 막는다.
pub(crate) fn block_pointer(ctx: &egui::Context, id: egui::Id, rect: egui::Rect) -> egui::LayerId {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .movable(false)
        .interactable(true)
        .sense(egui::Sense::click_and_drag())
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
        });
    egui::LayerId::new(egui::Order::Foreground, id)
}

impl PopupState {
    /// 이 팝업이 덮는 사각형. 덮지 않는 팝업은 `None`이다. 배치 범위 사각형이 없으면(창 범위) 화면이다.
    pub(crate) fn cover_rect(
        &self,
        draw_ctx: Option<&LayoutContext>,
        screen_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        self.look.covers_scope.then(|| {
            PopupManager::scope_rect(&self.placement_scope(), draw_ctx).unwrap_or(screen_rect)
        })
    }
}

impl PopupManager {
    /// `pos`를 덮는 맨 위 팝업과 그 순번. `open_indices`는 그리는 순서(아래에서 위)다.
    pub(super) fn cover_at(
        &self,
        pos: egui::Pos2,
        open_indices: &[usize],
        draw_ctx: Option<&LayoutContext>,
        screen_rect: egui::Rect,
    ) -> Option<(PopupInstanceId, u64)> {
        open_indices.iter().rev().find_map(|&i| {
            let p = &self.popups[i];
            p.cover_rect(draw_ctx, screen_rect)
                .is_some_and(|r| r.contains(pos))
                .then_some((p.instance, p.z_seq))
        })
    }

    /// 보이는 덮는 팝업의 덮는 사각형과 순번. 포인터 대상 찾기가 직전 프레임 값으로 쓴다.
    pub(super) fn cover_occluders(
        &self,
        open_indices: &[usize],
        draw_ctx: Option<&LayoutContext>,
        screen_rect: egui::Rect,
    ) -> Vec<Occluder> {
        open_indices
            .iter()
            .filter_map(|&i| {
                let p = &self.popups[i];
                p.cover_rect(draw_ctx, screen_rect).map(|rect| Occluder {
                    rect,
                    z_seq: p.z_seq,
                })
            })
            .collect()
    }

    /// 어느 팝업 상자에도 들지 않은 누름을 범위 규칙대로 나눈다. 닫을 인스턴스는 `closed`에 넣고, 포커스를
    /// 받을 덮는 팝업이 있으면 돌려준다.
    ///
    /// - 배치 범위 밖 누름은 그 팝업에 아무 일도 하지 않는다.
    /// - 위의 팝업(host 덮음, plugin 상자·덮음)이 덮은 팝업은 알림을 받지 않는다.
    /// - 누르기 직전 포커스가 논리 범위 밖이었으면 들어오는 첫 클릭이라 닫거나 포커스를 풀지 않는다.
    ///   덮는 팝업이면 그 범위 안 누름이 팝업 것이라 포커스를 준다.
    /// - 그 밖에는 바깥 클릭 선언대로 닫는다. 닫지 않는 덮는 팝업은 포커스를 받고, 덮지 않는 팝업은
    ///   `sticky_focus`가 아니면 포커스를 푼다.
    pub(super) fn route_outside_press(
        &mut self,
        press: &RoutedPress,
        open_indices: &[usize],
        draw_ctx: Option<&LayoutContext>,
        screen_rect: egui::Rect,
        others: &[Occluder],
        closed: &mut Vec<(PopupInstanceId, PopupCloseReason)>,
    ) -> Option<PopupInstanceId> {
        // 열린 자식이 있는 부모는 바깥 클릭으로 닫지 않는다. 자식만 자기 규칙대로 닫힌다.
        let parents: Vec<PopupInstanceId> = self
            .popups
            .iter()
            .filter(|p| p.open)
            .filter_map(|p| p.parent)
            .collect();
        let covers = self.cover_occluders(open_indices, draw_ctx, screen_rect);
        let mut focus_to: Option<(PopupInstanceId, u64)> = None;
        for &i in open_indices {
            let popup = &mut self.popups[i];
            if !press.path.contains(&popup.placement_scope()) {
                continue;
            }
            let covered = covers
                .iter()
                .chain(others)
                .any(|o| o.z_seq > popup.z_seq && o.rect.contains(press.pos));
            if covered {
                continue;
            }
            let covering = popup.look.covers_scope;
            let first_click = !press.focus_before.contains(&popup.scope);
            if !first_click && popup.close_on_outside_click && !parents.contains(&popup.instance) {
                closed.push((popup.instance, PopupCloseReason::OutsideClick));
            } else if covering {
                if focus_to.is_none_or(|(_, z)| popup.z_seq > z) {
                    focus_to = Some((popup.instance, popup.z_seq));
                }
            } else if !first_click && !popup.sticky_focus {
                popup.focused = false;
            }
        }
        focus_to.map(|(iid, _)| iid)
    }
}

#[cfg(test)]
#[path = "press_tests.rs"]
mod tests;
