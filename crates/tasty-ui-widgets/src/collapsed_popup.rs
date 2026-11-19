//! 범위에 들어가지 않는 팝업의 접힌 형태("팝업 보기" 버튼만 담은 그 팝업의 프레임)와, 키 포커스를 가진
//! 팝업 셸 위쪽의 포커스 가장자리. 본체와 갤러리가 같은 배치·그리기를 쓴다.
//!
//! 정본은 tasty-page 갤러리 Overlays › Dialogs의 collapsed popup Spec과 popup focus Spec이다.

use tasty_type_appearance::theme::Theme;

use crate::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

/// 범위 안에서 팝업이 보이는 형태.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollapsedForm {
    /// 필요 크기가 경계에 들어가 팝업을 그대로 그린다.
    Expanded,
    /// 라벨 버튼 프레임.
    Label,
    /// 아이콘 버튼 프레임. 축마다 들어가지 않으면 경계 시작에 붙고 범위 사각형에서 잘린다.
    Icon,
}

/// 범위 안 배치 결과.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollapsedPlacement {
    pub form: CollapsedForm,
    pub rect: egui::Rect,
}

/// 판정 경계의 안쪽 여백: `max(0, min(cap, 폭/2, 높이/2))`, 네 변 같다.
pub fn scope_inset(scope: egui::Rect, cap: f32) -> f32 {
    cap.min(scope.width() / 2.0)
        .min(scope.height() / 2.0)
        .max(0.0)
}

/// 필요 크기가 `bounds`에 두 축 모두 들어가는지. 한 축만 모자라도 접는다.
pub fn fits_bounds(needed: egui::Vec2, bounds: egui::Rect) -> bool {
    needed.x <= bounds.width() && needed.y <= bounds.height()
}

/// 팝업의 표시 형태와 사각형을 정한다. 앵커는 `bounds` 가운데다.
/// - 필요 크기가 두 축 모두 `bounds`에 들어가면 펼친다. 줄이는 구간은 없다.
/// - 아니면 라벨 프레임이 두 축 모두 들어갈 때 라벨, 아니면 아이콘 프레임.
/// - 아이콘 프레임이 들어가지 않는 축은 `bounds` 시작에 붙인다. 끝은 호출자가 범위로 자른다.
pub fn place_collapsed(
    bounds: egui::Rect,
    needed: egui::Vec2,
    label_frame: egui::Vec2,
    icon_frame: egui::Vec2,
) -> CollapsedPlacement {
    let centered = |size: egui::Vec2| egui::Rect::from_center_size(bounds.center(), size);
    if fits_bounds(needed, bounds) {
        return CollapsedPlacement {
            form: CollapsedForm::Expanded,
            rect: centered(needed),
        };
    }
    if fits_bounds(label_frame, bounds) {
        return CollapsedPlacement {
            form: CollapsedForm::Label,
            rect: centered(label_frame),
        };
    }
    let axis = |start: f32, len: f32, center: f32, size: f32| {
        if size <= len {
            center - size / 2.0
        } else {
            start
        }
    };
    let min = egui::pos2(
        axis(
            bounds.left(),
            bounds.width(),
            bounds.center().x,
            icon_frame.x,
        ),
        axis(
            bounds.top(),
            bounds.height(),
            bounds.center().y,
            icon_frame.y,
        ),
    );
    CollapsedPlacement {
        form: CollapsedForm::Icon,
        rect: egui::Rect::from_min_size(min, icon_frame),
    }
}

/// 버튼 둘레의 프레임 두께(테두리 + content margin) 한 변.
fn frame_pad(theme: &Theme) -> f32 {
    theme.border_width.value() + theme.popup_content_margin().value()
}

type FitIcon = dyn Fn(&mut egui::Ui, egui::Rect, egui::Color32);

/// 정본은 `fit` 글리프를 14(icon-size-sm)로 그린다. 위젯이 준 칸 가운데에 그 크기로 둔다.
fn fit_icon(theme: &Theme) -> impl Fn(&mut egui::Ui, egui::Rect, egui::Color32) + use<> {
    let side = theme.icon_glyph_size_sm.value();
    move |ui, rect, c| {
        let glyph = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(side));
        tasty_icons::FIT.image(side, c).paint_at(ui, glyph);
    }
}

fn label_button<'a>(label: &'a str, theme: &Theme, icon: &'a FitIcon) -> Button<'a> {
    Button::new(label)
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Md)
        .leading_icon(icon)
        .leading_icon_size(theme.icon_glyph_size_sm)
}

/// 라벨 버튼 프레임과 아이콘 버튼 프레임의 바깥 크기.
pub fn collapsed_frame_sizes(
    ui: &egui::Ui,
    theme: &Theme,
    label: &str,
) -> (egui::Vec2, egui::Vec2) {
    let icon = fit_icon(theme);
    let pad = frame_pad(theme) * 2.0;
    let height = ControlSize::Md.height(theme);
    let label_w = label_button(label, theme, &icon)
        .desired_width(ui, theme)
        .value();
    (
        egui::vec2(label_w + pad, height + pad),
        egui::Vec2::splat(height + pad),
    )
}

/// 접힌 팝업 한 장을 그리는 조건.
#[derive(Clone, Copy, Debug)]
pub struct CollapsedLook<'a> {
    /// 버튼 라벨(아이콘 프레임에서는 접근 이름).
    pub label: &'a str,
    /// 버튼 툴팁.
    pub tooltip: &'a str,
    /// scrim 위면 modal 그림자, 아니면 popover 그림자.
    pub on_scrim: bool,
    /// 버튼 키 포커스 링. 사용자가 열어 키 포커스가 있을 때.
    pub ring: bool,
    /// 프레임 위쪽 포커스 가장자리. 팝업이 키 포커스를 가질 때.
    pub edge: bool,
}

/// 접힌 팝업 자체 프레임과 버튼을 그리고 버튼 응답을 돌려준다. `clip`은 범위 사각형이다.
pub fn paint_collapsed_popup(
    ui: &mut egui::Ui,
    theme: &Theme,
    placed: CollapsedPlacement,
    clip: egui::Rect,
    look: CollapsedLook<'_>,
) -> egui::Response {
    let painter = ui.painter_at(clip);
    let radius = theme.corner_radius.value();
    let shadow = if look.on_scrim {
        theme.shadow_modal()
    } else {
        theme.shadow_popover()
    };
    painter.add(shadow.to_egui().as_shape(placed.rect, radius));
    painter.rect(
        placed.rect,
        radius,
        theme.bg_panel().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
        egui::StrokeKind::Inside,
    );
    if look.edge {
        paint_popup_focus_edge(&painter, theme, placed.rect);
    }

    let inner = placed.rect.shrink(frame_pad(theme));
    let icon = fit_icon(theme);
    let resp = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.set_clip_rect(clip.intersect(ui.clip_rect()));
            match placed.form {
                CollapsedForm::Label => label_button(look.label, theme, &icon).show(ui, theme),
                _ => IconButton::new()
                    .variant(IconButtonVariant::Solid)
                    .size(ControlSize::Md)
                    .show(ui, theme, &icon),
            }
        })
        .inner;
    if look.ring {
        // `:focus-visible` 짝: 버튼 바깥 border_width 만큼 띄운 focus_ring_width 링.
        let offset = theme.border_width.value();
        painter.rect_stroke(
            resp.rect.expand(offset),
            theme.corner_radius.value() + offset,
            egui::Stroke::new(theme.focus_ring_width.value(), theme.border_focus()),
            egui::StrokeKind::Outside,
        );
    }
    resp.on_hover_text(look.tooltip)
}

/// 키 포커스를 가진 팝업 프레임 위쪽의 2px 막대. 1px 테두리 안쪽에 전체 폭으로 그리고 모서리 반경으로
/// 자른다. 콘텐츠 여백 위에 그리므로 자리를 따로 두지 않는다.
pub fn paint_popup_focus_edge(painter: &egui::Painter, theme: &Theme, frame: egui::Rect) {
    let border = theme.border_width.value();
    let inner = frame.shrink(border);
    if !inner.is_positive() {
        return;
    }
    let height = theme.popup_focus_edge_width().value().min(inner.height());
    let bar = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), height));
    let r = (theme.corner_radius.value() - border).max(0.0).round() as u8;
    painter.rect_filled(
        bar,
        egui::CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        theme.popup_focus_edge_focused().to_egui(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> (egui::Vec2, egui::Vec2) {
        // 라벨 프레임 en 134 × 38, 아이콘 프레임 38 × 38(정본 실측값).
        (egui::vec2(134.0, 38.0), egui::vec2(38.0, 38.0))
    }

    fn surface(w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(w, h))
    }

    fn bounds(scope: egui::Rect) -> egui::Rect {
        scope.shrink(scope_inset(scope, 8.0))
    }

    /// Given 필요 크기가 inset 뺀 bounds 에 두 축 모두 들어갈 때, Then 펼친다.
    #[test]
    fn a_popup_that_fits_the_bounds_stays_expanded() {
        let (label, icon) = frames();
        let placed = place_collapsed(
            bounds(surface(300.0, 200.0)),
            egui::vec2(284.0, 184.0),
            label,
            icon,
        );
        assert_eq!(placed.form, CollapsedForm::Expanded);
        assert_eq!(placed.rect.size(), egui::vec2(284.0, 184.0));
    }

    /// Given 높이만 모자랄 때, Then 접힌다(어느 한 축이면 충분하다).
    #[test]
    fn height_alone_is_enough_to_collapse() {
        let (label, icon) = frames();
        let placed = place_collapsed(
            bounds(surface(400.0, 100.0)),
            egui::vec2(200.0, 90.0),
            label,
            icon,
        );
        assert_eq!(placed.form, CollapsedForm::Label);
    }

    /// Given 라벨 프레임이 bounds 폭보다 넓을 때, Then 아이콘 프레임으로 바뀌고 가운데에 둔다.
    #[test]
    fn a_label_frame_wider_than_the_bounds_becomes_the_icon_frame() {
        let (label, icon) = frames();
        let scope = surface(64.0, 200.0);
        let placed = place_collapsed(bounds(scope), egui::vec2(300.0, 100.0), label, icon);
        assert_eq!(placed.form, CollapsedForm::Icon);
        assert_eq!(placed.rect.center(), scope.center());
    }

    /// Given 아이콘 프레임도 폭이 들어가지 않을 때, Then 그 축은 bounds 시작에 붙고 세로는 가운데다.
    #[test]
    fn an_icon_frame_too_wide_sits_on_the_bounds_start_on_that_axis() {
        let (label, icon) = frames();
        let scope = surface(24.0, 200.0);
        let placed = place_collapsed(bounds(scope), egui::vec2(300.0, 100.0), label, icon);
        assert_eq!(placed.form, CollapsedForm::Icon);
        assert_eq!(placed.rect.left(), scope.left() + 8.0);
        assert_eq!(placed.rect.center().y, scope.center().y);
    }

    /// Given 작은 surface, Then inset 은 폭·높이의 절반까지 줄어든다.
    #[test]
    fn the_inset_shrinks_to_half_a_small_surface() {
        assert_eq!(scope_inset(surface(10.0, 200.0), 8.0), 5.0);
        assert_eq!(scope_inset(surface(200.0, 6.0), 8.0), 3.0);
        assert_eq!(scope_inset(surface(200.0, 200.0), 8.0), 8.0);
    }
}
