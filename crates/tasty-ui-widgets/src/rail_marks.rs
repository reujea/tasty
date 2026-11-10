//! 접힌 레일의 표지. 카테고리 경계의 선, 카테고리를 끈 레일의 맨 위 구분선,
//! mirror 워크스페이스 아바타의 오른쪽 아래 칩을 그린다. 본체 레일과 갤러리 예제가 같은 함수를 쓴다.
//! 선은 상자 테두리가 아니라 아바타 글자와 같은 무게의 표지라서 1px 보더 규칙의 예외로 2px다.

use tasty_type_appearance::theme::Theme;

/// 카테고리 선의 길이(`rail-category-line-length`).
pub fn rail_category_line_length(theme: &Theme) -> f32 {
    theme.rail_category_line_length().value()
}

/// `center`에 카테고리 선을 그린다. 모든 카테고리가 같은 색을 쓰고,
/// `lit`(hover·pressed)일 때만 밝은 색으로 바꾼다. 끝은 둥글다.
pub fn paint_rail_category_line(
    painter: &egui::Painter,
    theme: &Theme,
    center: egui::Pos2,
    lit: bool,
) {
    let line_h = theme.rail_category_line_width().value();
    let rect =
        egui::Rect::from_center_size(center, egui::vec2(rail_category_line_length(theme), line_h));
    let color: egui::Color32 = if lit {
        theme.rail_category_line_hover().into()
    } else {
        theme.rail_category_line().into()
    };
    painter.rect_filled(rect, line_h * 0.5, color);
}

/// 카테고리를 끈 레일의 맨 위 구분선 크기. 길이는 `rail-flat-divider-length`, 굵기는 카테고리 선과 같다.
pub fn rail_flat_divider_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.rail_flat_divider_length().value(),
        theme.rail_category_line_width().value(),
    )
}

/// `rect`에 맨 위 구분선을 그린다. 카테고리 선의 idle과 같은 색·굵기다.
pub fn paint_rail_flat_divider(painter: &egui::Painter, theme: &Theme, rect: egui::Rect) {
    let line_h = theme.rail_category_line_width().value();
    painter.rect_filled(rect, line_h * 0.5, theme.rail_category_line());
}

/// 접힌 레일 아바타의 오른쪽 아래 모서리에 mirror 칩을 그린다.
/// 배경색 halo로 다른 알림·점유 표시와 구분하고, 12 원 안쪽의 1px 테두리가
/// 아바타의 idle·active·hover 바탕 어디서나 칩을 드러낸다.
/// 원의 지름은 `workspace-mirror-chip-size` 다.
pub fn paint_workspace_mirror_chip(ui: &egui::Ui, theme: &Theme, avatar: egui::Rect) {
    paint_mirror_chip(ui, theme, avatar, true);
}

/// 연결되지 않은 저장 원격 행의 mirror 칩. 테두리는 border-control, 글리프는 glyph-dim 이다.
pub fn paint_workspace_mirror_chip_offline(ui: &egui::Ui, theme: &Theme, avatar: egui::Rect) {
    paint_mirror_chip(ui, theme, avatar, false);
}

fn paint_mirror_chip(ui: &egui::Ui, theme: &Theme, avatar: egui::Rect, live: bool) {
    let halo_r = theme.spacing_sm.value();
    let chip_r = theme.workspace_mirror_chip_size().value() * 0.5;
    let border = theme.border_width.value();
    let glyph = theme.spacing_sm.value();
    let inset = theme.spacing_xs.value();
    let center = egui::pos2(avatar.max.x - inset, avatar.max.y - inset);
    let painter = ui.painter();
    painter.circle_filled(center, halo_r, theme.bg_sidebar());
    painter.circle_filled(center, chip_r, theme.workspace_mirror_chip_bg());
    painter.circle_stroke(
        center,
        chip_r - border * 0.5,
        egui::Stroke::new(
            border,
            if live {
                theme.workspace_mirror_chip_border()
            } else {
                theme.border_control()
            },
        ),
    );
    let glyph_rect = egui::Rect::from_center_size(center, egui::vec2(glyph, glyph));
    let fg = if live {
        theme.workspace_mirror_fg()
    } else {
        theme.glyph_dim()
    };
    tasty_icons::REMOTE
        .image(glyph, fg.into())
        .paint_at(ui, glyph_rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: impl FnOnce(&egui::Ui, &Theme)) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        let theme = tasty_themes::mocha_fallback();
        let mut f = Some(f);
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(f) = f.take() {
                    f(ui, &theme);
                }
            });
        });
        out.shapes
    }

    /// 패널 바탕을 뺀, 그려진 사각형들.
    fn rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::RectShape> {
        shapes
            .iter()
            .skip(1)
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn category_line_is_24_by_2_and_lights_up() {
        let theme = tasty_themes::mocha_fallback();
        let c = egui::pos2(100.0, 100.0);
        for (lit, color) in [
            (false, egui::Color32::from(theme.rail_category_line())),
            (true, egui::Color32::from(theme.rail_category_line_hover())),
        ] {
            let shapes = run(|ui, t| paint_rail_category_line(ui.painter(), t, c, lit));
            let r = rects(&shapes);
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].rect.size(), egui::vec2(24.0, 2.0));
            assert_eq!(r[0].rect.center(), c);
            assert_eq!(r[0].fill, color);
        }
    }

    #[test]
    fn flat_divider_is_28_by_2_in_the_idle_line_colour() {
        let theme = tasty_themes::mocha_fallback();
        assert_eq!(rail_flat_divider_size(&theme), egui::vec2(28.0, 2.0));
        let rect =
            egui::Rect::from_min_size(egui::pos2(10.0, 10.0), rail_flat_divider_size(&theme));
        let shapes = run(|ui, t| paint_rail_flat_divider(ui.painter(), t, rect));
        let r = rects(&shapes);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].rect, rect);
        assert_eq!(r[0].fill, egui::Color32::from(theme.rail_category_line()));
    }

    #[test]
    fn mirror_chip_sits_bottom_right_with_a_one_px_edge() {
        let theme = tasty_themes::mocha_fallback();
        let avatar = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(32.0, 28.0));
        let shapes = run(|ui, t| paint_workspace_mirror_chip(ui, t, avatar));
        let circles: Vec<_> = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Circle(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(circles.len(), 3);
        for c in &circles {
            assert_eq!(c.center, egui::pos2(28.0, 24.0));
        }
        assert_eq!(circles[0].radius, 8.0);
        assert_eq!(circles[1].radius, 6.0);
        assert_eq!(circles[2].radius, 5.5);
        assert_eq!(circles[2].stroke.width, 1.0);
        assert_eq!(
            circles[2].stroke.color,
            egui::Color32::from(theme.workspace_mirror_chip_border())
        );
    }
}
