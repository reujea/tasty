//! webview 페이지 타일이 Loading에서 Spinner와 넘긴 문구를 가운데에 그리고,
//! 낮은 영역에서는 위쪽 최소 여백(`spacing_xl`)을 지키며, 빈 안내 caption 이 있으면 상태와 관계없이
//! 그 줄 몫을 넣어 글리프를 같은 자리에 두고, boundary는 안내를 그리지 않는지 검사한다.

use egui::{Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{WebviewPageState, WebviewPageTileOutput, webview_page_tile};

/// 두 프레임을 구동하고 마지막 프레임의 출력을 돌려준다. 첫 프레임은 폰트 준비 전이다.
fn render(
    theme: &Theme,
    region: Rect,
    state: WebviewPageState<'static>,
    hint: Option<&'static str>,
) -> WebviewPageTileOutput {
    let ctx = egui::Context::default();
    let mut out = None;
    for _ in 0..2 {
        let _out = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(region));
                    out = Some(webview_page_tile(
                        &mut child,
                        theme,
                        theme.bg_panel().to_egui(),
                        region,
                        state,
                        hint,
                    ));
                });
            },
        );
    }
    out.expect("tile was drawn")
}

#[test]
fn loading_draws_the_spinner_and_title_in_the_middle() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(40.0, 30.0), vec2(400.0, 300.0));
    let out = render(
        &theme,
        region,
        WebviewPageState::Loading { title: "Loading" },
        None,
    );
    let glyph = out.glyph.expect("spinner");
    let title = out.title.expect("title");
    let spinner = theme.spinner_size.value();
    assert_eq!(glyph.width(), spinner);
    assert_eq!(glyph.height(), spinner);
    assert!((glyph.center().x - region.center().x).abs() <= 0.5);
    assert!((title.center().x - region.center().x).abs() <= 0.5);
    let block = theme.icon_glyph_size_md.value()
        + theme.spacing_sm.value()
        + theme.font_size_body.value() * 2.0;
    let top_pad = (region.height() - block) / 2.0;
    assert_eq!(glyph.top(), region.top() + top_pad);
    assert!(title.top() >= glyph.bottom() + theme.spacing_sm.value());
}

#[test]
fn a_low_region_keeps_the_minimum_top_space() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(0.0, 10.0), vec2(300.0, 40.0));
    let out = render(
        &theme,
        region,
        WebviewPageState::Loading { title: "Loading" },
        None,
    );
    let glyph = out.glyph.expect("spinner");
    assert_eq!(glyph.top(), region.top() + theme.spacing_xl.value());
}

#[test]
fn a_boundary_draws_no_glyph_or_title() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0));
    let out = render(&theme, region, WebviewPageState::Boundary, None);
    assert!(out.glyph.is_none());
    assert!(out.title.is_none());
}

#[test]
fn an_empty_hint_moves_every_state_to_the_same_glyph_top() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
    let hint = Some("Type a path or URL");
    let loading = render(
        &theme,
        region,
        WebviewPageState::Loading { title: "Loading" },
        hint,
    );
    let empty = render(
        &theme,
        region,
        WebviewPageState::Empty {
            title: "No page loaded",
            glyph: theme.text_disabled().to_egui(),
        },
        hint,
    );
    let bare = render(
        &theme,
        region,
        WebviewPageState::Loading { title: "Loading" },
        None,
    );
    let top = |o: WebviewPageTileOutput| o.glyph.expect("glyph").top();
    assert_eq!(top(loading), top(empty));
    let hint_h = theme.spacing_sm.value() + theme.font_size_caption.value();
    assert_eq!(top(bare) - top(loading), hint_h / 2.0);
}
