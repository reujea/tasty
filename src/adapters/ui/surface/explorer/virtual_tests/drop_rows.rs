//! 본문 놓기 틴트 아래의 `..` 행이 놓기 칸의 글자를 읽는지 본다. 두 값이 갈리는 Latte로 잰다.

use std::collections::HashSet;
use std::path::PathBuf;

use egui::RawInput;
use tasty_model::{ExplorerPanel, ExplorerViewMode};
use tasty_type_appearance::theme::Theme;

fn latte() -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors(colors, is_light.unwrap_or(true))
}

/// 한 프레임을 그리고 `..` 글자의 색을 돌려준다.
fn parent_row_ink(mode: ExplorerViewMode, tinted: bool) -> egui::Color32 {
    crate::i18n::init("en");
    let theme = latte();
    let root = PathBuf::from("/tasty-test-no-such-dir");
    let panel = ExplorerPanel::new(1, root.clone());
    let mut view = super::super::ExplorerView::new();
    view.sync(&panel, None);
    view.bed_focused = false;
    if tinted {
        view.ops.drag.tint_body_for_test();
    }
    let font = crate::settings::EffectiveFont {
        font_family: String::new(),
        font_size: 13.0,
        custom_font_path: String::new(),
        line_height: 1.0,
        font_scale_mode: String::new(),
    };
    let ctx = egui::Context::default();
    let cut = HashSet::new();
    let mut action = None;
    let out = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| match mode {
            ExplorerViewMode::Grid => {
                super::super::grid_view(ui, &theme, &mut view, &font, &cut, &root, &mut action);
            }
            _ => super::super::list_view(ui, &theme, &mut view, &cut, &root, &mut action),
        });
    });
    out.shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text() == ".." => Some(text.fallback_color),
            _ => None,
        })
        .expect("the parent row is drawn")
}

#[test]
fn the_parent_row_reads_the_drop_cell_under_a_body_tint() {
    let raised = latte().text_secondary_raised().to_egui();
    for mode in [ExplorerViewMode::List, ExplorerViewMode::Grid] {
        assert_ne!(parent_row_ink(mode, false), raised, "{mode:?} at rest");
        assert_eq!(
            parent_row_ink(mode, true),
            raised,
            "{mode:?} under the tint"
        );
    }
}
