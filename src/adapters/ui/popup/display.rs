//! 좁은 범위의 팝업 표시 상태: 범위에 들어가면 그대로(Fits), 아니면 "팝업 보기" 버튼만 담은 그 팝업의
//! 프레임(Collapsed), 버튼을 누르면 창 위(Expanded).
//!
//! 펼친 팝업은 배치 범위 Window를 덮는 노드다(`placement_scope`). 덮음 선언과 무관하게 창을 덮으므로
//! 포인터 차단·scrim·바깥 누름 판정이 Window 범위로 나온다. 논리 범위(`scope`)는 바뀌지 않는다.
//! 접힘 판정은 매 프레임 한다. 펼친 팝업은 판정하지 않고 접기 버튼이나 Esc로 다시 접힐 때까지 남는다.

use tasty_ui_widgets::{CollapsedPlacement, collapsed_frame_sizes, fits_bounds, place_collapsed};

use super::{PopupFit, PopupInstanceId, PopupManager, PopupScope, PopupState};
use crate::adapters::ui::LayoutContext;

/// 팝업 인스턴스의 표시 상태.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PopupDisplay {
    /// 범위 안에 그대로 그린다.
    Fits,
    /// 보기 버튼 프레임만 그린다. 프레임이 없으면 접힌 부모의 버튼 하나가 묶음을 대표해 그리지 않는다.
    Collapsed(Option<CollapsedPlacement>),
    /// 창 위에 그린다. `restore`는 펼치기 전 위치로, 다시 접을 때 되돌린다.
    Expanded { restore: egui::Pos2 },
}

/// 접힌 팝업 한 장을 그리는 조건.
pub(crate) struct CollapsedDraw {
    /// 이 팝업이 칠하는 scrim 사각형. 접혀도 덮음 선언대로 칠한다.
    pub(crate) scrim: Option<egui::Rect>,
    /// 범위를 덮는 팝업이면 modal 그림자.
    pub(crate) on_scrim: bool,
    /// 키 주인이면 버튼 링·포커스 가장자리를 켜고 Enter·Space를 받는다.
    pub(crate) focused: bool,
}

/// 접힌 팝업의 WebView 오버레이 대상. 펼친 상자 대신 버튼 프레임을 페이지 위에 합성한다.
pub(crate) fn collapsed_overlay_target(
    kind: crate::state::webview_overlay::TargetKind,
    layer: egui::LayerId,
    frame: egui::Rect,
    clip: egui::Rect,
    look: &CollapsedDraw,
    ppp: f32,
) -> crate::state::webview_overlay::OverlayTarget {
    let th = crate::theme::theme();
    let shadow = if look.on_scrim {
        th.shadow_modal()
    } else {
        th.shadow_popover()
    };
    crate::state::webview_overlay::OverlayTarget {
        kind,
        layer,
        rect: frame,
        opaque_rect: crate::state::webview_overlay::outlined_box(frame, th.border_width, clip, ppp),
        paint_rect: (frame + shadow.to_egui().margin()).intersect(clip),
        scrim: look.scrim,
    }
}

/// 접힌 팝업의 프레임과 보기 버튼을 그 팝업 레이어에 그린다. 버튼을 누르거나 키 주인일 때 Enter·Space를
/// 누르면 참이다(펼치기). `clip`은 논리 범위 사각형이다.
pub(crate) fn draw_collapsed(
    ctx: &egui::Context,
    th: &tasty_type_appearance::theme::Theme,
    layer_id: egui::LayerId,
    frame: CollapsedPlacement,
    clip: egui::Rect,
    look: CollapsedDraw,
) -> bool {
    if let Some(scrim) = look.scrim {
        ctx.layer_painter(layer_id)
            .with_clip_rect(clip)
            .rect_filled(scrim, 0.0, th.scrim().to_egui());
    }
    let keyed = look.focused
        && ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let clicked = egui::Area::new(layer_id.id)
        .order(egui::Order::Foreground)
        .fixed_pos(frame.rect.min)
        .movable(false)
        .interactable(true)
        .constrain(false)
        .show(ctx, |ui| {
            tasty_ui_widgets::paint_collapsed_popup(
                ui,
                th,
                frame,
                clip,
                tasty_ui_widgets::CollapsedLook {
                    label: crate::i18n::t("popup.collapsed_button.label"),
                    tooltip: crate::i18n::t("popup.collapsed_button.tooltip"),
                    on_scrim: look.on_scrim,
                    ring: look.focused,
                    edge: look.focused,
                },
            )
            .clicked()
        })
        .inner;
    clicked || keyed
}

/// 라벨 버튼 프레임과 아이콘 버튼 프레임의 바깥 크기. 접힘 배치(`place_collapsed`)가 쓴다.
pub(crate) fn collapsed_frames(ctx: &egui::Context) -> (egui::Vec2, egui::Vec2) {
    let th = crate::theme::theme();
    let measure = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("popup.collapsed_measure"),
        egui::UiBuilder::new().max_rect(ctx.screen_rect()),
    );
    collapsed_frame_sizes(
        &measure,
        &th,
        crate::i18n::t("popup.collapsed_button.label"),
    )
}

/// 제목줄 오른쪽 끝 버튼 자리(닫기 자리).
pub(crate) fn title_end_btn_rect(title: egui::Rect) -> egui::Rect {
    let th = crate::theme::theme();
    let size = th.popup_title_btn_size().value();
    let edge_pad = th.popup_title_edge_inset().value();
    let center = egui::pos2(title.max.x - size * 0.5 - edge_pad, title.center().y);
    egui::Rect::from_center_size(center, egui::vec2(size, size))
}

/// 자기 제목줄이 없는 팝업이 펼쳐 받는 host 제목줄. 상자 위쪽 제목줄 높이의 띠다.
pub(crate) fn host_bar_rect(box_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        box_rect.min,
        egui::vec2(box_rect.width(), super::title_bar_height().value()),
    )
}

/// host 제목줄 그리기 조건.
pub(crate) struct HostBar {
    /// 팝업 상자.
    pub(crate) box_rect: egui::Rect,
    pub(crate) radius: f32,
    /// 제목줄 아래 선 색(셸 묶음의 값).
    pub(crate) title_edge: egui::Color32,
    pub(crate) hovered: bool,
    pub(crate) pressed: bool,
    pub(crate) tip_id: egui::Id,
}

/// 펼친 headless 팝업의 host 제목줄과 접기 버튼을 그린다. 제목 문구는 없고 버튼은 닫기 자리에 있다.
pub(crate) fn paint_host_bar(
    ctx: &egui::Context,
    painter: &egui::Painter,
    th: &tasty_type_appearance::theme::Theme,
    layer_id: egui::LayerId,
    bar: HostBar,
) {
    let title = host_bar_rect(bar.box_rect);
    let cr = bar.radius as u8;
    painter.rect_filled(
        title,
        egui::CornerRadius {
            nw: cr,
            ne: cr,
            sw: 0,
            se: 0,
        },
        th.bg_sidebar(),
    );
    painter.line_segment(
        [title.left_bottom(), title.right_bottom()],
        egui::Stroke::new(th.border_width.value(), bar.title_edge),
    );
    let btn = title_end_btn_rect(title);
    super::draw::title_icon_button(
        ctx,
        painter,
        th,
        btn,
        crate::adapters::ui::icons::SHRINK,
        bar.hovered,
        bar.pressed,
    );
    if bar.hovered {
        egui::show_tooltip_at(ctx, layer_id, bar.tip_id, btn.left_bottom(), |ui| {
            ui.label(crate::i18n::t("popup.collapse_button.tooltip"))
        });
    }
}

impl PopupState {
    /// 덮는 노드인지. 덮음 선언이거나 창 위로 펼친 팝업이다(펼침은 배치 범위 Window를 덮는다).
    pub(crate) fn covers(&self) -> bool {
        self.look.covers_scope || self.expanded()
    }

    /// 창 위로 펼쳤는지.
    pub(crate) fn expanded(&self) -> bool {
        matches!(self.display, PopupDisplay::Expanded { .. })
    }

    /// 접혀 있는지(버튼만 그리거나 부모 버튼이 대표한다).
    pub(crate) fn collapsed(&self) -> bool {
        matches!(self.display, PopupDisplay::Collapsed(_))
    }

    /// 접힌 팝업의 버튼 프레임.
    pub(crate) fn collapsed_frame(&self) -> Option<CollapsedPlacement> {
        match self.display {
            PopupDisplay::Collapsed(frame) => frame,
            _ => None,
        }
    }

    /// 보이는 상자. 접힌 팝업은 버튼 프레임이고, 부모 버튼이 대표하면 없다.
    pub(crate) fn box_rect(&self) -> Option<egui::Rect> {
        match self.display {
            PopupDisplay::Collapsed(frame) => frame.map(|f| f.rect),
            _ => Some(self.popup_rect()),
        }
    }

    /// 제목줄이 있는지. 자기 제목줄이 없는(headless) 팝업도 펼치면 접기 버튼을 담은 host 제목줄을 받는다.
    pub(crate) fn has_title_bar(&self) -> bool {
        !self.headless || self.expanded()
    }

    /// 필요 크기에 더해 그리는 크롬. headless 팝업을 펼치면 host 제목줄만큼 커진다.
    pub(super) fn chrome_extra(&self) -> egui::Vec2 {
        if self.headless && self.expanded() {
            egui::vec2(0.0, super::title_bar_height().value())
        } else {
            egui::Vec2::ZERO
        }
    }

    /// 펼친 팝업 제목줄의 접기 버튼. 버튼 묶음 맨 앞(전체화면·닫기 왼쪽)이고, 다른 버튼이 없으면 닫기 자리다.
    pub(crate) fn collapse_btn_rect(&self) -> Option<egui::Rect> {
        if !self.expanded() {
            return None;
        }
        let close = self.close_btn_rect();
        if self.headless {
            return Some(close);
        }
        let left = self.fullscreen_btn_rect().unwrap_or(close);
        Some(left.translate(egui::vec2(-(left.width() + super::title_btn_gap()), 0.0)))
    }
}

impl PopupManager {
    /// 열린 인스턴스의 표시 상태를 정한다. 자식은 부모가 펼쳐졌거나 접혔으면 같은 상태이고, 부모가
    /// 들어가면 자기 크기로 판정한다. `open_indices`는 그리는 순서다(부모가 자식보다 먼저 열렸다).
    pub(super) fn judge_display(
        &mut self,
        ctx: &egui::Context,
        open_indices: &[usize],
        draw_ctx: Option<&LayoutContext>,
    ) {
        let inset_cap = crate::theme::theme().spacing_sm.value();
        let (label_frame, icon_frame) = collapsed_frames(ctx);
        let mut order: Vec<usize> = open_indices.to_vec();
        order.sort_by_key(|&i| self.popups[i].instance);
        for i in order {
            let parent = self.popups[i]
                .parent
                .and_then(|p| self.instance(p))
                .filter(|p| p.open)
                .map(|p| p.display);
            let popup = &mut self.popups[i];
            popup.display = match (parent, popup.display) {
                (Some(PopupDisplay::Expanded { .. }), PopupDisplay::Expanded { restore }) => {
                    PopupDisplay::Expanded { restore }
                }
                (Some(PopupDisplay::Expanded { .. }), _) => {
                    popup.request_center = true;
                    PopupDisplay::Expanded { restore: popup.pos }
                }
                (Some(PopupDisplay::Collapsed(_)), _) => PopupDisplay::Collapsed(None),
                (_, PopupDisplay::Expanded { restore }) => PopupDisplay::Expanded { restore },
                _ => Self::fit_display(popup, draw_ctx, inset_cap, label_frame, icon_frame),
            };
        }
    }

    /// 범위에 들어가는지로 Fits·Collapsed를 정한다. 줄이는 팝업과 창 범위 팝업은 접지 않는다.
    fn fit_display(
        popup: &PopupState,
        draw_ctx: Option<&LayoutContext>,
        inset_cap: f32,
        label_frame: egui::Vec2,
        icon_frame: egui::Vec2,
    ) -> PopupDisplay {
        if popup.arrange.fit == PopupFit::Shrink || popup.scope == PopupScope::Window {
            return PopupDisplay::Fits;
        }
        let Some(scope) = Self::scope_rect(&popup.scope, draw_ctx) else {
            return PopupDisplay::Fits;
        };
        let inset = if popup.arrange.scope_inset {
            tasty_ui_widgets::scope_inset(scope, inset_cap)
        } else {
            0.0
        };
        let bounds = scope.shrink(inset);
        if fits_bounds(popup.needed_size, bounds) {
            return PopupDisplay::Fits;
        }
        PopupDisplay::Collapsed(Some(place_collapsed(
            bounds,
            popup.needed_size,
            label_frame,
            icon_frame,
        )))
    }

    /// 접힌 인스턴스를 창 위로 펼친다. 포커스를 주고 맨 앞으로 올리며 창 가운데에 둔다.
    pub(super) fn expand(&mut self, iid: PopupInstanceId) {
        let Some(popup) = self.instance_mut(iid).filter(|p| p.collapsed()) else {
            return;
        };
        popup.display = PopupDisplay::Expanded { restore: popup.pos };
        popup.request_center = true;
        self.give_focus(iid, None);
        self.bring_to_front(iid);
    }

    /// 펼친 인스턴스를 다시 접는다. 부모와 함께 펼쳐졌으면 묶음의 뿌리를 접는다. 닫지 않으며 포커스와
    /// 상태를 그대로 둔다. 다음 프레임 판정이 범위에 들어가는지로 Fits·Collapsed를 정한다.
    pub(super) fn refold(&mut self, iid: PopupInstanceId) -> bool {
        let mut root = iid;
        while let Some(parent) = self
            .instance(root)
            .and_then(|p| p.parent)
            .filter(|&p| self.instance(p).is_some_and(|p| p.open && p.expanded()))
        {
            root = parent;
        }
        if !self.instance(root).is_some_and(PopupState::expanded) {
            return false;
        }
        // 뿌리와 그 아래 펼친 자식을 모두 접는다. 자식은 다음 판정에서 부모 상태를 다시 따른다.
        let in_bundle: Vec<bool> = self
            .popups
            .iter()
            .map(|p| {
                let mut at = Some(p.instance);
                while let Some(cur) = at {
                    if cur == root {
                        return true;
                    }
                    at = self.instance(cur).and_then(|q| q.parent);
                }
                false
            })
            .collect();
        for (popup, inside) in self.popups.iter_mut().zip(in_bundle) {
            if !inside || !popup.expanded() {
                continue;
            }
            if let PopupDisplay::Expanded { restore } = popup.display {
                popup.pos = restore;
            }
            popup.display = PopupDisplay::Fits;
            popup.request_center = false;
        }
        true
    }

    /// Esc: 키 주인(맨 위 포커스 인스턴스)이 펼쳐져 있으면 다시 접고 참이다. 닫기 사건은 없다.
    pub fn refold_focused(&mut self) -> bool {
        let Some((_, iid, _)) = self.focused_dismissal_target() else {
            return false;
        };
        self.refold(iid)
    }
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
