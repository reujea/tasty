//! 키보드 포커스 링. 디자인 정본(vendor/README.md "States via overlays")의 두 모양을 한 곳에서 그린다.
//!
//! - 필드 링: 필드 테두리 바로 바깥에 틈 없이 붙인 border_width 링. Input·CodeArea·Select·MultiSelect가 쓴다.
//! - 컨트롤 링: 컨트롤 상자에서 focus_ring_offset 만큼 띄운 focus_ring_width 링. 버튼·체크박스·창 안 탭이 쓰고,
//!   Switch는 트랙에서 두 배만큼 띄운다. 설정 L1 탭만 탭 안쪽에 그린다.
//!
//! 링 조건은 위젯의 `has_focus()`다. 이 크레이트의 버튼·토글·탭은 포인터 클릭으로 egui 포커스를 요청하지 않으므로
//! 그 포커스는 Tab 이동이나 앱의 `request_focus`에서만 생긴다(정본 `:focus-visible`과 같은 범위).

use tasty_type_appearance::theme::Theme;

use crate::control::Ground;

/// 컨트롤 링 색. 링 아래 바탕으로 정한다(정본 RULES focus-ring 표): active 바탕은 accent-primary-raised,
/// 나머지(panel·raised)는 border-focus.
pub fn control_ring_color(theme: &Theme, ground: Ground) -> egui::Color32 {
    match ground {
        Ground::Active => theme.accent_primary_raised().to_egui(),
        Ground::Panel | Ground::Raised => theme.border_focus().to_egui(),
    }
}

/// 컨트롤 링을 상자에서 띄우는 거리(focus-ring-offset).
pub fn control_ring_gap(theme: &Theme) -> f32 {
    theme.focus_ring_offset.value()
}

/// Switch 링을 트랙에서 띄우는 거리. 정본 Switch.jsx `outline-offset: 2px`(토큰 없음)를 focus-ring-offset의 두 배로
/// 적는다. 둘 다 UI 배율을 받지 않는 hairline이다.
pub fn switch_ring_gap(theme: &Theme) -> f32 {
    theme.focus_ring_offset.value() * 2.0
}

/// `rect`에서 `gap`만큼 띄운 focus_ring_width 링. 모서리는 상자 반경에 `gap`을 더해 상자 곡선을 따른다.
pub fn paint_control_ring(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    radius: f32,
    gap: f32,
    color: egui::Color32,
) {
    painter.rect_stroke(
        rect.expand(gap),
        radius + gap,
        egui::Stroke::new(theme.focus_ring_width.value(), color),
        egui::StrokeKind::Outside,
    );
}

/// `rect`를 네 변 모두 `inset`만큼 줄인 안쪽에 그리는 focus_ring_width 링(설정 L1 탭).
pub fn paint_inset_ring(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    inset: f32,
    color: egui::Color32,
) {
    painter.rect_stroke(
        rect.shrink(inset),
        0.0,
        egui::Stroke::new(theme.focus_ring_width.value(), color),
        egui::StrokeKind::Inside,
    );
}

/// 필드 테두리 바로 바깥에 붙인 border_width 링. `rect`는 필드 바깥 사각형(테두리를 Inside로 그린 상자)이다.
/// Outside 획은 사각형과 반경을 함께 넓혀 테두리와 링 사이에 틈이 없다.
pub fn paint_field_ring(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    radius: f32,
    color: egui::Color32,
) {
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), color),
        egui::StrokeKind::Outside,
    );
}

/// 고정 포커스 표본 문맥을 기록하는 egui 임시 데이터 키. 값은 중첩 깊이다.
fn pinned_focus_id() -> egui::Id {
    egui::Id::new("tasty_ui_widgets::focus_ring::pinned_focus")
}

/// `content` 안의 컨트롤(Button·IconButton·Checkbox·Switch)을 키보드 포커스를 가진 모양으로 그린다. egui 포커스는 한
/// 위젯만 가지므로 여러 포커스 표본을 한 화면에 두는 갤러리용이다. 비활성 컨트롤에는 링을 그리지 않는다.
pub fn pinned_focus<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let id = pinned_focus_id();
    let prev = ctx.data(|d| d.get_temp::<u32>(id)).unwrap_or(0);
    ctx.data_mut(|d| d.insert_temp(id, prev + 1));
    let out = content(ui);
    ctx.data_mut(|d| d.insert_temp(id, prev));
    out
}

/// 컨트롤이 포커스 링을 그릴지. 활성 컨트롤이 실제 egui 포커스를 가졌거나 [`pinned_focus`] 안이다.
pub(crate) fn shows_focus(ui: &egui::Ui, resp: &egui::Response, enabled: bool) -> bool {
    enabled
        && (resp.has_focus()
            || ui
                .ctx()
                .data(|d| d.get_temp::<u32>(pinned_focus_id()))
                .unwrap_or(0)
                > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_ground_takes_the_raised_accent_and_the_rest_border_focus() {
        let theme = tasty_themes::mocha_fallback();
        assert_eq!(
            control_ring_color(&theme, Ground::Active),
            theme.accent_primary_raised().to_egui()
        );
        for g in [Ground::Panel, Ground::Raised] {
            assert_eq!(
                control_ring_color(&theme, g),
                theme.border_focus().to_egui()
            );
        }
    }
}
