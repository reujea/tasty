//! 팝업 밖 누름과 덮는 팝업의 포인터 판정.
//!
//! 포인터 대상은 위에서부터 찾는다. 덮는 팝업(`covers_scope`)을 만나면 그 배치 범위 아래로 내려가지 않는다.
//! 팝업 상자 밖이면 그 팝업 자신(scrim)이 대상이다. 바깥 누름 알림은 누른 좌표가 팝업의 배치 범위 안이고
//! 덮인 노드가 아닌 팝업에만 간다.

use crate::adapters::ui::LayoutContext;

use super::occlusion::Occluder;
use super::route::RoutedPress;
use super::{PopupCloseReason, PopupInstanceId, PopupManager, PopupScope, PopupState};

/// 덮는 사각형 위 egui 입력을 받는 빈 레이어를 그린다. 그 아래 위젯(사이드바·탭 바·탐색기·상태바·배너)은
/// 누름·hover·휠을 받지 않는다. 호출자는 이 레이어를 팝업 레이어 바로 아래에 둔다. egui는 직전 프레임의 레이어로
/// 판정하므로 열린 다음 프레임부터 막는다.
///
/// hover만 감지한다. egui의 hit test는 포인터 둘레를 다 덮는 위젯이 있는 레이어 아래를 보지 않으므로 그것으로
/// 막힌다. 끌기를 감지하면 팝업 상자 중 콘텐츠 Area 밖(제목줄·테두리)의 누름을 이 레이어가 가져가
/// `is_using_pointer`가 참이 되고 팝업 이동·크기 조절이 시작되지 않는다.
pub(crate) fn block_pointer(ctx: &egui::Context, id: egui::Id, rect: egui::Rect) -> egui::LayerId {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .movable(false)
        .interactable(true)
        .sense(egui::Sense::hover())
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
        self.covers().then(|| {
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

    /// 포인터를 받는 상자와 그 자식 드롭다운의 사각형과 순번. 그린 뒤 위치로 모은다. 접힌 버튼 프레임은 배치
    /// 범위로 자른 부분, 드롭다운은 부모 범위 밖으로 나온 부분까지다. 그리지 않는 접힌 자식은 없다.
    pub(super) fn pointer_boxes(
        &self,
        ctx: &egui::Context,
        open_indices: &[usize],
        draw_ctx: Option<&LayoutContext>,
        screen_rect: egui::Rect,
    ) -> Vec<Occluder> {
        let mut boxes = Vec::new();
        for &i in open_indices {
            let p = &self.popups[i];
            let Some(mut rect) = p.box_rect() else {
                continue;
            };
            if p.collapsed() {
                let clip = Self::scope_rect(&p.placement_scope(), draw_ctx).unwrap_or(screen_rect);
                rect = rect.intersect(clip);
            }
            if rect.is_positive() {
                boxes.push(Occluder {
                    rect,
                    z_seq: p.z_seq,
                });
            }
            boxes.extend(
                super::child_overlay_rects(ctx, p.instance)
                    .into_iter()
                    .map(|rect| Occluder {
                        rect,
                        z_seq: p.z_seq,
                    }),
            );
        }
        boxes
    }

    /// 어느 팝업 상자에도 들지 않은 누름을 [`outside_press`]대로 나눈다. 닫을 인스턴스는 `closed`에 넣고,
    /// 포커스를 받을 덮는 팝업이 있으면 돌려준다.
    /// 가림은 host 상자·드롭다운·덮는 범위와 `others`(plugin 상자·덮는 범위)를 합친 z 순서로 본다.
    pub(super) fn route_outside_press(
        &mut self,
        ctx: &egui::Context,
        press: &RoutedPress,
        open_indices: &[usize],
        draw_ctx: Option<&LayoutContext>,
        others: &[Occluder],
        closed: &mut Vec<(PopupInstanceId, PopupCloseReason)>,
    ) -> Option<PopupInstanceId> {
        let screen_rect = ctx.screen_rect();
        let parents: Vec<PopupInstanceId> = self
            .popups
            .iter()
            .filter(|p| p.open)
            .filter_map(|p| p.parent)
            .collect();
        let mut occluders = self.cover_occluders(open_indices, draw_ctx, screen_rect);
        occluders.extend(self.pointer_boxes(ctx, open_indices, draw_ctx, screen_rect));
        occluders.extend_from_slice(others);
        let mut focus_to: Option<(PopupInstanceId, u64)> = None;
        for &i in open_indices {
            let popup = &mut self.popups[i];
            let placement = popup.placement_scope();
            let target = OutsideTarget {
                placement_scope: &placement,
                logical_scope: &popup.scope,
                z_seq: popup.z_seq,
                box_rect: popup.box_rect(),
                cover_rect: popup.cover_rect(draw_ctx, screen_rect),
                closes_on_outside_click: popup.close_on_outside_click,
                has_open_child: parents.contains(&popup.instance),
            };
            match outside_press(&target, press, &occluders) {
                OutsidePress::None => {}
                OutsidePress::Close => {
                    closed.push((popup.instance, PopupCloseReason::OutsideClick));
                }
                OutsidePress::Focus => {
                    if focus_to.is_none_or(|(_, z)| popup.z_seq > z) {
                        focus_to = Some((popup.instance, popup.z_seq));
                    }
                }
                // 포커스 위치는 누름의 포인터 대상이 정한다(`PopupManager::focus_on_press`). 이 팝업 밖이고 덮는
                // 범위도 아니면 대상이 이 팝업이 아니므로 이미 포커스를 잃었다.
                OutsidePress::Unfocus => {}
            }
        }
        focus_to.map(|(iid, _)| iid)
    }
}

/// 상자 밖 누름이 팝업 하나에 하는 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutsidePress {
    /// 아무 일도 없다(상자 안, 배치 범위 밖, 위 팝업에 덮임, 다른 범위에서 들어오는 첫 클릭).
    None,
    /// 바깥 클릭 선언대로 닫는다.
    Close,
    /// 덮는 범위 안 누름이라 팝업이 포커스를 받는다.
    Focus,
    /// 덮지 않는 팝업의 범위 안 누름이라 포커스를 푼다.
    Unfocus,
}

/// [`outside_press`]가 보는 팝업 하나. host·plugin 인스턴스가 같은 값으로 채운다.
pub(crate) struct OutsideTarget<'a> {
    pub(crate) placement_scope: &'a PopupScope,
    pub(crate) logical_scope: &'a PopupScope,
    pub(crate) z_seq: u64,
    /// 그린 상자(접혔으면 버튼 프레임). 그리지 않는 접힌 자식은 `None`이다.
    pub(crate) box_rect: Option<egui::Rect>,
    /// 덮는 팝업이면 덮는 사각형.
    pub(crate) cover_rect: Option<egui::Rect>,
    pub(crate) closes_on_outside_click: bool,
    /// 열린 자식이 있으면 바깥 클릭으로 닫지 않는다. 자식만 자기 규칙대로 닫힌다.
    pub(crate) has_open_child: bool,
}

/// 누름 하나를 팝업 하나의 범위 규칙으로 나눈다. host·plugin이 같이 쓴다.
///
/// - 상자 안 누름과 배치 범위 밖 누름은 그 팝업에 아무 일도 하지 않는다.
/// - 위의 팝업(`occluders` 중 z가 더 큰 것)이 덮은 자리의 누름은 알림이 없다.
/// - 누르기 직전 포커스가 논리 범위 밖이었으면 들어오는 첫 클릭이라 닫거나 포커스를 풀지 않는다.
///   덮는 팝업이면 그 범위 안 누름이 팝업 것이라 포커스를 준다.
/// - 그 밖에는 바깥 클릭 선언대로 닫는다. 닫지 않는 덮는 팝업은 포커스를 받고, 덮지 않는 팝업은 포커스를 푼다.
pub(crate) fn outside_press(
    target: &OutsideTarget<'_>,
    press: &RoutedPress,
    occluders: &[Occluder],
) -> OutsidePress {
    if target.box_rect.is_some_and(|r| r.contains(press.pos))
        || !press.path.contains(target.placement_scope)
        || occluders
            .iter()
            .any(|o| o.z_seq > target.z_seq && o.rect.contains(press.pos))
    {
        return OutsidePress::None;
    }
    let first_click = !press.focus_before.contains(target.logical_scope);
    if !first_click && target.closes_on_outside_click && !target.has_open_child {
        OutsidePress::Close
    } else if target.cover_rect.is_some_and(|r| r.contains(press.pos)) {
        OutsidePress::Focus
    } else if !first_click {
        OutsidePress::Unfocus
    } else {
        OutsidePress::None
    }
}

#[cfg(test)]
#[path = "press_tests.rs"]
mod tests;
