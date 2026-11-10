//! 저장된 원격 행을 연결하지 못한 이유를 알리는 사이드바 행 표지와 접힌 레일 칩.
//! 툴팁 문자열은 호출자가 주입한다.

use tasty_type_appearance::theme::Theme;

use crate::tooltip::{Tooltip, tooltip_hover_delay_elapsed};

/// 워크스페이스 행 끝 칸의 거절 표지 한 변 길이.
pub fn attach_refusal_mark_size(theme: &Theme) -> f32 {
    theme.icon_glyph_size_sm.value()
}

/// 워크스페이스 행 끝 칸에 경고 글리프를 놓고 hover 때 `tooltip`(대상과 이유)을 보인다.
/// 클릭은 받지 않는다. 행이 활성이든 아니든 같은 모양이다.
pub fn attach_refusal_mark(ui: &mut egui::Ui, theme: &Theme, tooltip: &str) -> egui::Response {
    let size = attach_refusal_mark_size(theme);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    tasty_icons::ALERT_TRIANGLE
        .image(size, theme.attach_refusal_glyph().to_egui())
        .paint_at(ui, rect);
    if tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered()) {
        Tooltip::new(tooltip)
            .id_source(resp.id)
            .show(ui, theme, rect);
    }
    resp
}

/// 접힌 레일 아바타의 왼쪽 위 모서리에 경고 칩을 그리고 칩 rect를 돌려준다.
/// 오른쪽 위는 알림 점, 오른쪽 아래는 mirror 칩, 왼쪽 아래는 move 칩이 쓴다. 네 표시는 함께 나올 수 있다.
/// 칩은 아바타 밖으로 border 폭만큼 나온다. `bed`는 칩 바탕색으로, 레일 배경과 같게 전달한다.
pub fn paint_attach_refusal_chip(
    ui: &egui::Ui,
    theme: &Theme,
    avatar: egui::Rect,
    bed: egui::Color32,
) -> egui::Rect {
    let chip = theme.attach_refusal_chip_size().value();
    let outset = theme.border_width.value();
    let rect = egui::Rect::from_min_size(
        egui::pos2(avatar.min.x - outset, avatar.min.y - outset),
        egui::vec2(chip, chip),
    );
    ui.painter().circle_filled(rect.center(), chip * 0.5, bed);
    let glyph = theme.attach_refusal_chip_glyph_size().value();
    let glyph_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    tasty_icons::ALERT_TRIANGLE
        .image(glyph, theme.attach_refusal_glyph().to_egui())
        .paint_at(ui, glyph_rect);
    rect
}

/// 레일 아바타 hover 때 행 표지와 같은 `tooltip`을 아바타 옆에 보인다. 칩은 따로 클릭을 받지 않는다.
/// `second_line`은 아바타가 로딩 중일 때 그 문구를 muted 둘째 줄로 더한다.
pub fn attach_refusal_avatar_tooltip(
    ui: &egui::Ui,
    theme: &Theme,
    avatar: &egui::Response,
    tooltip: &str,
    second_line: Option<&str>,
) {
    if tooltip_hover_delay_elapsed(ui.ctx(), theme, avatar.id, avatar.hovered()) {
        let mut bubble = Tooltip::new(tooltip).id_source(avatar.id);
        if let Some(line) = second_line {
            bubble = bubble.second_line(line);
        }
        bubble.show(ui, theme, avatar.rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rail_chip_takes_the_top_left_corner_outside_the_avatar() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let ctx = egui::Context::default();
        let side = theme.sidebar_collapsed_workspace_height.value();
        let avatar = egui::Rect::from_min_size(egui::pos2(40.0, 30.0), egui::Vec2::splat(side));
        let mut chip = egui::Rect::NOTHING;
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                chip = paint_attach_refusal_chip(ui, &theme, avatar, theme.bg_sidebar().into());
            });
        });
        let size = theme.attach_refusal_chip_size().value();
        let outset = theme.border_width.value();
        assert_eq!(chip.size(), egui::vec2(size, size));
        assert_eq!(chip.min, avatar.min - egui::vec2(outset, outset));
        // 바탕 원은 칩 가운데에 칩 지름으로 그려진다. 글리프는 SVG 로더가 없는 시험에서 그려지지 않는다.
        let bed = out.shapes.iter().any(|c| match &c.shape {
            egui::Shape::Circle(circle) => {
                circle.center == chip.center() && (circle.radius - size * 0.5).abs() < 0.01
            }
            _ => false,
        });
        assert!(bed, "chip bed circle at {chip:?}");
    }
}
