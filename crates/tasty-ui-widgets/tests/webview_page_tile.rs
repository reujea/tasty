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

const SHORT_TITLE: &str = "GTK failed to initialize, so this view and the tray icon are off.";
const LONG_TITLE: &str =
    "GDK_BACKEND in Tasty's launch environment blocks X11, so this view and the tray icon are off.";
const LONG_SUB: &str = "Remove GDK_BACKEND from Tasty's launch environment, then restart Tasty.";

fn notice(
    title: &'static str,
    sub: Option<&'static str>,
    at_end: bool,
) -> WebviewPageState<'static> {
    WebviewPageState::Notice { title, sub, at_end }
}

/// Given 들어가는 안내, When 그린다, Then 블록이 세로 가운데에 오고 스크롤이 없다.
#[test]
fn a_notice_that_fits_sits_in_the_middle() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(20.0, 20.0), vec2(400.0, 300.0));
    let out = render(
        &theme,
        region,
        notice(SHORT_TITLE, Some(LONG_SUB), false),
        None,
    );
    assert!(!out.overflows);
    let glyph = out.glyph.expect("info glyph");
    let sub = out.sub.expect("fix line");
    assert_eq!(glyph.height(), theme.icon_glyph_size_md.value());
    assert!((glyph.center().x - region.center().x).abs() <= 0.5);
    let above = glyph.top() - region.top();
    let below = region.bottom() - sub.bottom();
    assert!((above - below).abs() <= 1.0, "above {above}, below {below}");
}

/// Given 영역보다 긴 안내, When 그린다, Then 위 여백에서 시작하고 스크롤이 생기며 해결책 줄은 잘리지 않는다.
#[test]
fn a_notice_taller_than_the_rect_starts_at_the_top_and_scrolls() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(160.0, 150.0));
    let out = render(
        &theme,
        region,
        notice(LONG_TITLE, Some(LONG_SUB), false),
        None,
    );
    assert!(out.overflows);
    let glyph = out.glyph.expect("info glyph");
    assert_eq!(glyph.top(), region.top() + theme.spacing_md.value());
    let sub = out.sub.expect("fix line");
    let caption_line = (theme.font_size_caption * theme.line_height_ui).value();
    assert!(
        sub.height() > caption_line,
        "the fix wraps over more than one line"
    );
    assert!(
        sub.bottom() > region.bottom(),
        "the fix continues below the rect"
    );
}

/// Given 넘치는 안내, When 그린다, Then 스크롤 영역이 영역 가장자리까지 차고 여백은 그 안쪽에 있다.
#[test]
fn the_notice_scrolls_edge_to_edge_with_the_padding_inside() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(10.0, 10.0), vec2(160.0, 150.0));
    let out = render(
        &theme,
        region,
        notice(LONG_TITLE, Some(LONG_SUB), false),
        None,
    );
    assert!(out.overflows);
    let scroll = out.scroll.expect("scroll area");
    for (got, want) in [
        (scroll.left(), region.left()),
        (scroll.right(), region.right()),
        (scroll.top(), region.top()),
        (scroll.bottom(), region.bottom()),
    ] {
        assert!((got - want).abs() <= 0.5, "{scroll:?} vs {region:?}");
    }
    let title = out.title.expect("title");
    let pad = theme.spacing_md.value();
    assert!(
        (title.left() - (region.left() + pad)).abs() <= 0.5,
        "{title:?}"
    );
    assert!(
        (title.right() - (region.right() - pad)).abs() <= 0.5,
        "{title:?}"
    );
}

/// Given 넘치는 안내와 끝 프레임 표시, When 그린다, Then 해결책 줄의 끝이 영역 안에 보인다.
#[test]
fn the_end_frame_shows_the_last_fix_line() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(160.0, 150.0));
    let out = render(
        &theme,
        region,
        notice(LONG_TITLE, Some(LONG_SUB), true),
        None,
    );
    assert!(out.overflows);
    let sub = out.sub.expect("fix line");
    let area_bottom = region.bottom() - theme.spacing_md.value();
    assert!(sub.top() >= region.top(), "{sub:?}");
    assert!((sub.bottom() - area_bottom).abs() <= 0.5, "{sub:?}");
}

/// Given 해결책이 없는 안내, When 그린다, Then 해결책 자리는 caption 한 줄 높이로 남는다.
#[test]
fn a_notice_without_a_fix_keeps_one_empty_caption_line() {
    let theme = tasty_themes::mocha_fallback();
    let region = Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(220.0, 150.0));
    let out = render(&theme, region, notice(SHORT_TITLE, None, false), None);
    let sub = out.sub.expect("reserved fix line");
    let caption_line = (theme.font_size_caption * theme.line_height_ui).value();
    assert!((sub.height() - caption_line).abs() <= 0.01);
    assert!(!out.overflows);
}

/// Given 안내 kind 네 개, Then 해결책 키는 제목 키에 `_sub`를 붙인 이름이고 GTK 초기화 실패만 해결책이 없다.
#[test]
fn every_notice_fix_key_follows_its_title_key() {
    use tasty_ui_widgets::DisplayNotice;
    for kind in DisplayNotice::ALL {
        match kind.sub_key() {
            Some(sub) => assert_eq!(sub, format!("{}_sub", kind.title_key())),
            None => assert_eq!(kind, DisplayNotice::GtkInit),
        }
    }
    let kinds: std::collections::BTreeSet<_> =
        DisplayNotice::ALL.iter().map(|k| k.design_kind()).collect();
    assert_eq!(kinds.len(), DisplayNotice::ALL.len());
}
