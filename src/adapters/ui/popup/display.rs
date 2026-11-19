//! 좁은 범위의 팝업 표시 상태: 범위에 들어가면 그대로(Fits), 아니면 "팝업 보기" 버튼만 담은 그 팝업의
//! 프레임(Collapsed), 버튼을 누르면 창 위(Expanded).
//!
//! 펼친 팝업은 배치 범위 Window를 덮는 노드다(`placement_scope`). 덮음 선언과 무관하게 창을 덮으므로
//! 포인터 차단·scrim·바깥 누름 판정이 Window 범위로 나온다. 논리 범위(`scope`)는 바뀌지 않는다.
//! 접힘 판정은 매 프레임 한다. 펼친 팝업은 판정하지 않고 접기 버튼이나 Esc로 다시 접힐 때까지 남는다.

use tasty_ui_widgets::{
    CollapsedPlacement, collapsed_frame_sizes, fits_bounds, place_collapsed_at,
};

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

/// 접을지 판정하는 입력. host·plugin 인스턴스가 같은 값으로 부르며 인스턴스 종류를 모른다.
pub(crate) struct FitInput {
    /// 배치 범위의 사각형. 창 범위이거나 사각형을 모르면 `None`이고 접지 않는다.
    pub(crate) scope_rect: Option<egui::Rect>,
    pub(crate) fit: PopupFit,
    /// 판정 경계에서 안쪽 여백을 빼는지.
    pub(crate) scope_inset: bool,
    /// 팝업의 필요 크기.
    pub(crate) needed: egui::Vec2,
    /// 접힌 프레임의 앵커(plugin Cursor 앵커는 포인터). `None`이면 판정 경계 가운데다.
    pub(crate) anchor: Option<egui::Pos2>,
}

/// 범위에 들어가지 않으면 범위 안 보기 버튼 프레임, 들어가면 `None`(그대로 그린다). 줄이는 팝업은 접지 않는다.
/// `inset_cap`은 안쪽 여백 상한(spacing_sm), `frames`는 라벨·아이콘 버튼 프레임 크기다.
pub(crate) fn collapse_placement(
    input: FitInput,
    inset_cap: f32,
    (label_frame, icon_frame): (egui::Vec2, egui::Vec2),
) -> Option<CollapsedPlacement> {
    if input.fit == PopupFit::Shrink {
        return None;
    }
    let scope = input.scope_rect?;
    let inset = if input.scope_inset {
        tasty_ui_widgets::scope_inset(scope, inset_cap)
    } else {
        0.0
    };
    let bounds = scope.shrink(inset);
    (!fits_bounds(input.needed, bounds))
        .then(|| place_collapsed_at(bounds, input.needed, label_frame, icon_frame, input.anchor))
}

/// 접힌 팝업 한 장을 그리는 조건.
pub(crate) struct CollapsedDraw {
    /// 이 팝업이 칠하는 scrim 사각형. 접혀도 덮음 선언대로 칠한다.
    pub(crate) scrim: Option<egui::Rect>,
    /// 범위를 덮는 팝업이면 modal 그림자.
    pub(crate) on_scrim: bool,
    /// 포커스 표시. 버튼 링·포커스 가장자리를 켠다. 키는 받지 않는다.
    pub(crate) focused: bool,
    /// 이번 프레임 키 주인이면 Enter·Space로 펼친다.
    pub(crate) takes_keys: bool,
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
    let keyed = look.takes_keys
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
        })
        .inner;
    // egui 는 키 포커스가 남은 버튼의 Enter·Space 도 클릭으로 돌려준다. 이번 프레임 키 주인이 아니면 포인터
    // 클릭만 받는다.
    let clicked = if look.takes_keys {
        clicked.clicked()
    } else {
        clicked.clicked_by(egui::PointerButton::Primary)
    };
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
    /// 접기 버튼의 키 포커스·툴팁 식별자.
    pub(crate) id: egui::Id,
    /// 이 팝업이 이번 프레임 키 주인이다.
    pub(crate) takes_keys: bool,
}

/// 펼친 headless 팝업의 host 제목줄과 접기 버튼을 그린다. 제목 문구는 없고 버튼은 닫기 자리에 있다.
/// 접기 버튼이 키로 눌렸으면 참이다.
pub(crate) fn paint_host_bar(
    ctx: &egui::Context,
    painter: &egui::Painter,
    th: &tasty_type_appearance::theme::Theme,
    layer_id: egui::LayerId,
    bar: HostBar,
) -> bool {
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
    collapse_button(
        ctx,
        painter,
        th,
        layer_id,
        CollapseSlot {
            id: bar.id,
            rect: btn,
            hovered: bar.hovered,
            takes_keys: bar.takes_keys,
        },
    )
}

/// 접기 버튼 하나의 자리와 상태.
pub(crate) struct CollapseSlot {
    /// 키 포커스·툴팁 식별자.
    pub(crate) id: egui::Id,
    pub(crate) rect: egui::Rect,
    pub(crate) hovered: bool,
    /// 이 팝업이 이번 프레임 키 주인이다. 아니면 키 포커스가 남아도 Enter·Space를 읽지 않는다.
    pub(crate) takes_keys: bool,
}

/// 펼친 팝업의 접기 버튼(host·plugin 공용 위젯). 키 포커스에서 Enter·Space를 누르면 참이다. 포인터 누름은
/// 호출자의 누름 경로가 판정한다.
pub(crate) fn collapse_button(
    ctx: &egui::Context,
    painter: &egui::Painter,
    th: &tasty_type_appearance::theme::Theme,
    layer_id: egui::LayerId,
    slot: CollapseSlot,
) -> bool {
    let CollapseSlot {
        id,
        rect,
        hovered,
        takes_keys,
    } = slot;
    let out = tasty_ui_widgets::popup_collapse_button(
        ctx,
        painter,
        th,
        tasty_ui_widgets::CollapseButton {
            id,
            layer_id,
            rect,
            label: crate::i18n::t("popup.collapse_button.label"),
            tooltip: crate::i18n::t("popup.collapse_button.tooltip"),
            hovered,
            pressed: hovered && ctx.input(|i| i.pointer.primary_down()),
            takes_keys,
        },
    );
    if let Some(err) = out.glyph_error {
        tracing::warn!("popup collapse button glyph failed to load: {err}");
    }
    out.activated
}

/// plugin 팝업의 펼침과 그 팝업이 연 host 팝업(파일 피커)이 따르는 표시 상태. 펼침은 사용자가 보기 버튼으로
/// 펼친 plugin 팝업이고, 다시 접기는 진입점(Esc·접기 버튼 포인터·Enter·Space, host·plugin 어느 쪽)과 무관하게
/// `PopupManager::refold`·`refold_plugin`이 여기서 함께 지운다. 표시 상태는 plugin 렌더가 프레임마다 적고 host
/// 판정은 직전 plugin 렌더의 값을 읽는다.
#[derive(Debug, Default)]
pub(crate) struct PluginParents {
    displays: std::collections::HashMap<u64, PopupDisplay>,
    expanded: std::collections::HashSet<u64>,
}

impl PopupManager {
    /// 이번 plugin 렌더가 배치한 plugin 팝업의 표시 상태. 접힘은 버튼 자리 없이 적는다(자식은 그리지 않는다).
    pub(crate) fn set_plugin_displays(
        &mut self,
        displays: impl Iterator<Item = (u64, PopupDisplay)>,
    ) {
        self.plugin_parents.displays = displays.collect();
    }

    /// 창 위로 펼친 plugin 팝업.
    pub(crate) fn plugin_expanded(&self) -> &std::collections::HashSet<u64> {
        &self.plugin_parents.expanded
    }

    /// plugin 팝업을 창 위로 펼친다(보기 버튼).
    pub(crate) fn expand_plugin(&mut self, plugin_popup: u64) {
        self.plugin_parents.expanded.insert(plugin_popup);
    }

    /// 닫힌 plugin 팝업의 펼침을 지운다. 다시 열면 범위 판정부터 한다.
    pub(crate) fn retain_plugin_expanded(&mut self, live: impl Fn(u64) -> bool) {
        self.plugin_parents.expanded.retain(|&id| live(id));
    }

    /// plugin 팝업을 다시 접는다(접기 버튼·Esc). 그 팝업이 연 펼친 host 팝업도 함께 접는다.
    pub(crate) fn refold_plugin(&mut self, plugin_parent: u64) {
        self.plugin_parents.expanded.remove(&plugin_parent);
        if let Some(d) = self.plugin_parents.displays.get_mut(&plugin_parent) {
            *d = PopupDisplay::Fits;
        }
        let children: Vec<PopupInstanceId> = self
            .popups
            .iter()
            .filter(|p| p.open && p.plugin_parent == Some(plugin_parent) && p.expanded())
            .map(|p| p.instance)
            .collect();
        for iid in children {
            self.refold(iid);
        }
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

    /// 펼친 팝업이 창에 맞춘 프레임보다 큰 만큼. 내용은 이만큼 더 크게 배치하고 프레임에서 잘린다.
    pub(crate) fn expanded_overflow(&self) -> egui::Vec2 {
        if !self.expanded() {
            return egui::Vec2::ZERO;
        }
        (self.needed_size + self.chrome_extra() - self.size).max(egui::Vec2::ZERO)
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
        let frames = collapsed_frames(ctx);
        let mut order: Vec<usize> = open_indices.to_vec();
        order.sort_by_key(|&i| self.popups[i].instance);
        for i in order {
            let parent = match (self.popups[i].parent, self.popups[i].plugin_parent) {
                (Some(p), _) => self.instance(p).filter(|p| p.open).map(|p| p.display),
                (None, Some(pid)) => self.plugin_parents.displays.get(&pid).copied(),
                (None, None) => None,
            };
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
                _ => Self::fit_display(popup, draw_ctx, inset_cap, frames),
            };
        }
        self.hand_hidden_focus_to_button(open_indices);
    }

    /// 부모 버튼이 대표하는(그리지 않는) 자식이 포커스를 가졌으면 그 포커스를 보이는 보기 버튼(묶음의 접힌
    /// 조상)으로 옮긴다. 보이지 않는 자식이 키 주인으로 남아 버튼의 Enter·Space·포커스 링을 가로채지 않는다.
    fn hand_hidden_focus_to_button(&mut self, open_indices: &[usize]) {
        for &i in open_indices {
            let popup = &self.popups[i];
            if !popup.focused || popup.display != PopupDisplay::Collapsed(None) {
                continue;
            }
            let mut at = popup.parent;
            let mut button = None;
            while let Some(cur) = at.and_then(|p| self.instance(p)).filter(|p| p.open) {
                if cur.collapsed_frame().is_some() {
                    button = Some(cur.instance);
                    break;
                }
                at = cur.parent;
            }
            let Some(button) = button else {
                continue;
            };
            self.popups[i].focused = false;
            if let Some(b) = self.instance_mut(button) {
                b.focused = true;
                b.passive = false;
            }
        }
    }

    /// 범위에 들어가는지로 Fits·Collapsed를 정한다([`collapse_placement`]). 창 범위 팝업은 접지 않는다.
    fn fit_display(
        popup: &PopupState,
        draw_ctx: Option<&LayoutContext>,
        inset_cap: f32,
        frames: (egui::Vec2, egui::Vec2),
    ) -> PopupDisplay {
        let input = FitInput {
            scope_rect: (popup.scope != PopupScope::Window)
                .then(|| Self::scope_rect(&popup.scope, draw_ctx))
                .flatten(),
            fit: popup.arrange.fit,
            scope_inset: popup.arrange.scope_inset,
            needed: popup.needed_size,
            anchor: None,
        };
        collapse_placement(input, inset_cap, frames).map_or(PopupDisplay::Fits, |frame| {
            PopupDisplay::Collapsed(Some(frame))
        })
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
    /// 상태를 그대로 둔다. 다음 프레임 판정이 범위에 들어가는지로 Fits·Collapsed를 정하고, 묶음이 접히면
    /// 그리지 않는 자식의 포커스를 보이는 보기 버튼으로 옮긴다.
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
        // 펼친 plugin 부모가 연 묶음이면 그 부모도 여기서 접는다. 같은 프레임의 plugin 렌더와 다음 host 판정이
        // 다시 펼치지 않는다.
        if let Some(pid) = self.instance(root).and_then(|p| p.plugin_parent)
            && self.plugin_parents.expanded.remove(&pid)
            && let Some(d) = self.plugin_parents.displays.get_mut(&pid)
        {
            *d = PopupDisplay::Fits;
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
