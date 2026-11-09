//! Task DAG 빈 상태. 본체 DAG surface·DAG 목록 popup과 갤러리가 함께 호출한다.
//!
//! 디자인 `DagEmpty`: 글리프(glyph-dim) → space-sm → 제목(body, text-secondary) → space-sm →
//! 안내(caption, text-muted, measure-sm 폭에서 줄바꿈)를 영역 가운데에 쌓는다. 안내의 백틱 구간
//! (CLI 명령)은 code run으로 그린다. 안내는 줄마다 가운데에 맞춘다. 문구와 글리프는 호출부가 고른다.

use tasty_icons::Icon;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::ui_code::{
    UiCodeContainer, center_ui_copy_rows, paint_ui_copy, ui_copy_job, ui_copy_size,
};

/// 빈 상태 글리프 크기. 24px 아이콘 토큰이 아직 없어 시안 값을 그대로 둔다.
pub const DAG_EMPTY_ICON_SIZE: LogicalPx = LogicalPx(24.0);

/// 빈 상태 문구와 글리프.
pub struct DagEmptyView<'a> {
    pub icon: Icon,
    pub title: &'a str,
    pub hint: &'a str,
}

/// `rect` 가운데에 빈 상태를 그린다. 배경은 칠하지 않는다.
pub fn paint_dag_empty(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, view: &DagEmptyView<'_>) {
    let side = DAG_EMPTY_ICON_SIZE.value();
    let gap = theme.spacing_sm.value();
    let title_font = egui::FontId::proportional(theme.font_size_body.value());
    let title_h = ui.fonts(|f| f.row_height(&title_font));
    let measure = theme
        .measure_sm
        .value()
        .min(rect.width() - theme.spacing_xl.value() * 2.0)
        .max(theme.spacing_xl.value());
    let body = ui.fonts(|f| {
        f.layout_job(ui_copy_job(
            theme,
            view.hint,
            theme.font_size_caption,
            theme.text_muted().to_egui(),
            measure,
        ))
    });
    let body = std::sync::Arc::new(center_ui_copy_rows(theme, &body));
    let body_w = ui_copy_size(theme, &body).x;

    let total = side + gap + title_h + gap + body.size().y;
    let mut y = rect.center().y - total / 2.0;
    view.icon.image(side, theme.glyph_dim().to_egui()).paint_at(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.center().x - side / 2.0, y),
            egui::vec2(side, side),
        ),
    );
    y += side + gap;
    ui.painter().text(
        egui::pos2(rect.center().x, y),
        egui::Align2::CENTER_TOP,
        view.title,
        title_font,
        theme.text_secondary().to_egui(),
    );
    y += title_h + gap;
    paint_ui_copy(
        ui.painter(),
        theme,
        UiCodeContainer::Panel,
        egui::pos2(rect.center().x - body_w / 2.0, y),
        body,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph_tint(theme: &Theme) -> egui::Color32 {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let view = DagEmptyView {
            icon: tasty_icons::GIT_TREE,
            title: "t",
            hint: "h",
        };
        let mut tint = None;
        for _ in 0..3 {
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    paint_dag_empty(ui, theme, ui.max_rect(), &view);
                });
            });
            tint = out.shapes.iter().find_map(|clipped| match &clipped.shape {
                egui::Shape::Mesh(mesh) => mesh.vertices.first().map(|v| v.color),
                egui::Shape::Rect(rect) if rect.brush.is_some() => Some(rect.fill),
                _ => None,
            });
        }
        tint.expect("the glyph is painted")
    }

    /// 탭 surface의 두 바탕과 팝업이 같은 잉크다. 호출부는 값을 넘기지 않는다.
    #[test]
    fn the_empty_glyph_reads_glyph_dim_on_both_themes() {
        let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        let latte = Theme::with_colors(colors, is_light.unwrap_or(true));
        for theme in [tasty_themes::mocha_fallback(), latte] {
            assert_ne!(theme.glyph_dim(), theme.text_disabled());
            assert_eq!(glyph_tint(&theme), theme.glyph_dim().to_egui());
        }
    }
}
