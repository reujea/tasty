//! 범위에 들어가지 않는 팝업의 접힌 형태("팝업 보기" 버튼만 담은 그 팝업의 프레임)와, 키 포커스를 가진
//! 팝업 셸 위쪽의 포커스 가장자리. 본체와 갤러리가 같은 배치·그리기를 쓴다.
//!
//! 정본은 tasty-page 갤러리 Overlays › Dialogs의 collapsed popup Spec과 popup focus Spec이다.

use tasty_type_appearance::theme::Theme;

use crate::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonState, IconButtonVariant,
    paint_icon_button_state, paint_popup_title_glyph,
};

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

/// 팝업의 표시 형태와 사각형을 정한다. 앵커는 `bounds` 가운데다([`place_collapsed_at`]).
pub fn place_collapsed(
    bounds: egui::Rect,
    needed: egui::Vec2,
    label_frame: egui::Vec2,
    icon_frame: egui::Vec2,
) -> CollapsedPlacement {
    place_collapsed_at(bounds, needed, label_frame, icon_frame, None)
}

/// 팝업의 표시 형태와 사각형을 정한다.
/// - 필요 크기가 두 축 모두 `bounds`에 들어가면 펼친다. 줄이는 구간은 없다.
/// - 아니면 라벨 프레임이 두 축 모두 들어갈 때 라벨, 아니면 아이콘 프레임.
/// - 프레임은 앵커(`None`이면 `bounds` 가운데)를 `bounds` 안으로 당긴 점을 가운데로 두고, 그 축에 들어가면
///   `bounds` 안으로 민다. 아이콘 프레임이 들어가지 않는 축은 `bounds` 시작에 붙인다. 끝은 호출자가 범위로
///   자른다.
pub fn place_collapsed_at(
    bounds: egui::Rect,
    needed: egui::Vec2,
    label_frame: egui::Vec2,
    icon_frame: egui::Vec2,
    anchor: Option<egui::Pos2>,
) -> CollapsedPlacement {
    let center = anchor.map_or(bounds.center(), |a| {
        egui::pos2(
            a.x.clamp(bounds.left(), bounds.right()),
            a.y.clamp(bounds.top(), bounds.bottom()),
        )
    });
    let axis = |start: f32, end: f32, center: f32, size: f32| {
        if size <= end - start {
            (center - size / 2.0).clamp(start, end - size)
        } else {
            start
        }
    };
    let place = |size: egui::Vec2| {
        egui::Rect::from_min_size(
            egui::pos2(
                axis(bounds.left(), bounds.right(), center.x, size.x),
                axis(bounds.top(), bounds.bottom(), center.y, size.y),
            ),
            size,
        )
    };
    let (form, size) = if fits_bounds(needed, bounds) {
        (CollapsedForm::Expanded, needed)
    } else if fits_bounds(label_frame, bounds) {
        (CollapsedForm::Label, label_frame)
    } else {
        (CollapsedForm::Icon, icon_frame)
    };
    CollapsedPlacement {
        form,
        rect: place(size),
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

/// 창 위로 펼친 팝업의 배치. 프레임은 창 안에 들어가게 줄여 제목줄과 접기·닫기 버튼이 늘 온전하고, 내용은 필요
/// 크기를 유지해 프레임 밖으로 넘친 오른쪽·아래만 잘린다(사용자 결정 2026-11-19, 정본 collapsed-popup.jsx).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExpandedPlacement {
    /// 그리는 프레임(창 안).
    pub frame: egui::Rect,
    /// 필요 크기가 프레임보다 큰 만큼. 내용은 이만큼 더 크게 배치하고 프레임에서 자른다.
    pub overflow: egui::Vec2,
}

/// 펼친 팝업을 `window` 안에 둔다. `pos`는 원하는 왼쪽 위(가운데 정렬 값 등)이고, 줄인 프레임이 창 안에
/// 들어가도록 옮긴다. 창보다 큰 축은 창 시작에 붙는다.
pub fn place_expanded(window: egui::Rect, pos: egui::Pos2, size: egui::Vec2) -> ExpandedPlacement {
    let frame_size = size.min(window.size()).max(egui::Vec2::ZERO);
    let min = egui::pos2(
        pos.x.clamp(
            window.min.x,
            (window.max.x - frame_size.x).max(window.min.x),
        ),
        pos.y.clamp(
            window.min.y,
            (window.max.y - frame_size.y).max(window.min.y),
        ),
    );
    ExpandedPlacement {
        frame: egui::Rect::from_min_size(min, frame_size),
        overflow: (size - frame_size).max(egui::Vec2::ZERO),
    }
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
    // 아이콘 프레임은 글자가 없어 라벨을 접근 이름으로 준다.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, look.label));
    if look.ring {
        paint_focus_ring(&painter, theme, resp.rect);
        // 키 포커스로 고른 버튼은 포인터가 없어도 툴팁을 보인다.
        resp.show_tooltip_text(look.tooltip);
        return resp;
    }
    resp.on_hover_text(look.tooltip)
}

/// `:focus-visible` 짝: 버튼 바깥 focus_ring_offset 만큼 띄운 focus_ring_width 컨트롤 링.
fn paint_focus_ring(painter: &egui::Painter, theme: &Theme, button: egui::Rect) {
    crate::focus_ring::paint_control_ring(
        painter,
        theme,
        button,
        theme.corner_radius.value(),
        crate::focus_ring::control_ring_gap(theme),
        theme.border_focus().to_egui(),
    );
}

/// 펼친 팝업 제목줄의 접기 버튼 한 개. 포인터 hover·누름 상태는 호출자가 정한다(팝업 누름 경로가 판정한다).
#[derive(Clone, Copy, Debug)]
pub struct CollapseButton<'a> {
    /// 키 포커스·툴팁 식별자. 팝업 인스턴스마다 다르다.
    pub id: egui::Id,
    /// 버튼을 그리는 팝업 레이어.
    pub layer_id: egui::LayerId,
    pub rect: egui::Rect,
    /// 접근 이름.
    pub label: &'a str,
    pub tooltip: &'a str,
    pub hovered: bool,
    pub pressed: bool,
}

/// [`popup_collapse_button`]의 결과.
#[derive(Debug, Default)]
pub struct CollapseButtonOutput {
    /// 버튼이 egui 키 포커스를 가졌다. 키는 읽지 않는다. 호출자가 그 셸의 키 통로로 판정한다.
    pub focused: bool,
    /// 글리프를 읽지 못했다. 호출자가 기록한다.
    pub glyph_error: Option<egui::load::LoadError>,
}

/// 접기 버튼을 IconButton sm 규칙으로 그리고 키 포커스(Tab)·접근 이름·툴팁을 붙인다. hover나 키 포커스면
/// 툴팁을 보인다. 포인터 누름은 판정하지 않는다(팝업 누름 경로가 판정한다).
pub fn popup_collapse_button(
    ctx: &egui::Context,
    painter: &egui::Painter,
    theme: &Theme,
    b: CollapseButton<'_>,
) -> CollapseButtonOutput {
    let tint = paint_icon_button_state(
        painter,
        theme,
        b.rect,
        IconButtonState::ghost(b.hovered, b.pressed),
    );
    let glyph = ControlSize::Sm.icon_glyph(theme);
    let glyph_rect = egui::Rect::from_center_size(b.rect.center(), egui::Vec2::splat(glyph));
    let glyph_error =
        paint_popup_title_glyph(ctx, painter, tasty_icons::SHRINK, glyph_rect, tint).err();
    let ui = egui::Ui::new(
        ctx.clone(),
        b.id.with("ui"),
        egui::UiBuilder::new().layer_id(b.layer_id).max_rect(b.rect),
    );
    let resp = ui.interact(b.rect, b.id, egui::Sense::focusable_noninteractive());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, b.label));
    let focused = resp.has_focus();
    if focused {
        paint_focus_ring(painter, theme, b.rect);
    }
    if b.hovered || focused {
        egui::show_tooltip_at(
            ctx,
            b.layer_id,
            b.id.with("tip"),
            b.rect.left_bottom(),
            |ui| ui.label(b.tooltip),
        );
    }
    CollapseButtonOutput {
        focused,
        glyph_error,
    }
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

    /// 창보다 큰 펼친 팝업은 프레임이 창 안에 들어가고, 넘친 만큼이 overflow 다. 작은 팝업은 그대로다.
    #[test]
    fn an_expanded_popup_larger_than_the_window_fits_its_frame_and_keeps_the_overflow() {
        let window = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let big = place_expanded(window, egui::pos2(-100.0, -50.0), egui::vec2(600.0, 500.0));
        assert_eq!(big.frame, window);
        assert_eq!(big.overflow, egui::vec2(200.0, 200.0));
        let small = place_expanded(window, egui::pos2(100.0, 50.0), egui::vec2(200.0, 100.0));
        assert_eq!(small.frame.min, egui::pos2(100.0, 50.0));
        assert_eq!(small.overflow, egui::Vec2::ZERO);
        let wide = place_expanded(window, egui::pos2(-100.0, 100.0), egui::vec2(600.0, 100.0));
        assert_eq!(
            wide.frame,
            egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(400.0, 100.0))
        );
    }

    /// T-5-4: 접기 버튼은 Tab 키 포커스를 받고 그 포커스를 알린다. 키는 읽지 않아 Enter 사건이 egui 입력에 남는다
    /// (호출자가 그 셸의 키 통로로 판정한다). 접근 이름은 라벨이다.
    #[test]
    fn the_fold_button_reports_focus_without_reading_keys() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let id = egui::Id::new("collapse");
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(24.0, 24.0));
        let run = |key: Option<egui::Key>, focus: bool| {
            let events = key
                .map(|key| {
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                })
                .unwrap_or_default();
            let mut focused = false;
            let mut left = false;
            let out = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    if focus {
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                    let painter = ctx.layer_painter(egui::LayerId::background());
                    focused = popup_collapse_button(
                        ctx,
                        &painter,
                        &theme,
                        CollapseButton {
                            id,
                            layer_id: egui::LayerId::background(),
                            rect,
                            label: "Collapse popup",
                            tooltip: "tip",
                            hovered: false,
                            pressed: false,
                        },
                    )
                    .focused;
                    left = ctx.input(|i| {
                        i.events
                            .iter()
                            .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
                    });
                },
            );
            (focused, left, out)
        };
        assert!(!run(Some(egui::Key::Enter), false).0, "포커스 없음");
        // 포커스를 얻는 프레임에 접근 이름이 실린 사건이 나간다.
        let (_, _, out) = run(None, true);
        assert!(
            out.platform_output.events.iter().any(|e| matches!(
                e,
                egui::output::OutputEvent::FocusGained(info)
                    if info.label.as_deref() == Some("Collapse popup")
            )),
            "접근 이름: {:?}",
            out.platform_output.events
        );
        for key in [egui::Key::Enter, egui::Key::Space] {
            let (focused, left, _) = run(Some(key), true);
            assert!(focused, "{key:?}: 포커스를 알린다");
            assert!(left, "{key:?}: 키를 읽지 않아 사건이 남는다");
        }
    }

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

    /// Given 포인터 앵커, Then 프레임은 포인터를 가운데로 두고 bounds 안으로 밀린다(plugin Cursor 앵커).
    #[test]
    fn a_pointer_anchor_centres_the_frame_on_the_pointer_inside_the_bounds() {
        let (label, icon) = frames();
        let scope = surface(400.0, 100.0);
        let b = bounds(scope);
        let needed = egui::vec2(500.0, 90.0);
        let at = |p: egui::Pos2| place_collapsed_at(b, needed, label, icon, Some(p));
        let inside = egui::pos2(b.left() + 200.0, b.center().y);
        assert_eq!(at(inside).form, CollapsedForm::Label);
        assert_eq!(at(inside).rect.center(), inside);
        // 오른쪽 끝을 넘은 포인터는 bounds 안으로 당긴다.
        let outside = egui::pos2(b.right() + 50.0, b.top() - 20.0);
        let rect = at(outside).rect;
        assert_eq!(rect.right(), b.right());
        assert_eq!(rect.top(), b.top());
        // 앵커가 없으면 가운데다.
        assert_eq!(
            place_collapsed_at(b, needed, label, icon, None),
            place_collapsed(b, needed, label, icon)
        );
    }

    /// Given 작은 surface, Then inset 은 폭·높이의 절반까지 줄어든다.
    #[test]
    fn the_inset_shrinks_to_half_a_small_surface() {
        assert_eq!(scope_inset(surface(10.0, 200.0), 8.0), 5.0);
        assert_eq!(scope_inset(surface(200.0, 6.0), 8.0), 3.0);
        assert_eq!(scope_inset(surface(200.0, 200.0), 8.0), 8.0);
    }
}
