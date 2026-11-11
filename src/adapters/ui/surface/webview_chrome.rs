//! native WebView 아래의 배경·상태 안내. 페이지 자체는 OS WebView가 그린다.
//! Loading·Failed는 탐색 상태를 표시하고 나머지는 URL 유무로 빈 타일 또는 빈 화면 안내를 고른다.
//! 메뉴·팝업·토스트 때문에 불러온 페이지가 숨겨지면 이 빈 타일이 보인다.
//! 여기서는 문구·URL 줄·바탕만 정하고 타일은 공용 위젯이 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::WebviewPageState;

use crate::state::surface_focus::FocusMark;
use crate::theme;
use crate::webview::NavState;

/// URL 줄에 보일 문자열을 고른다. `label`이 있으면 그것을, 없으면 탐색 가능한 URL만 보인다.
/// raw HTML을 url로 보내는 surface(markdown 등)는 원문 대신 label을 보이거나 줄을 비운다.
pub fn chrome_caption<'a>(url: Option<&'a str>, label: Option<&'a str>) -> Option<&'a str> {
    label.or_else(|| url.filter(|u| crate::webview::is_navigable_url(u)))
}

/// tile이 페이지를 대신하는 동안 칠할 바탕과 글리프를 kind로 고른다.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WebviewTile {
    /// 페이지가 호스트 바탕을 따르는 kind. 페이지가 떠도 튀지 않게 tile도 같은 값을 칠한다.
    Bed { focused: bool },
    /// 페이지가 자기 바탕을 칠하는 kind. 띠로 분류하지만 아직 그리지 않아 포커스 표시가 없고 tile은 바뀌지 않는다.
    Plain,
}

impl WebviewTile {
    pub(crate) fn of(mark: FocusMark, bed_focused: bool) -> Self {
        match mark {
            FocusMark::Bed => Self::Bed {
                focused: bed_focused,
            },
            FocusMark::Edge => Self::Plain,
        }
    }

    fn bg(self, th: &Theme) -> egui::Color32 {
        match self {
            Self::Bed { focused } => tasty_ui_widgets::surface_bed(th, focused),
            Self::Plain => th.bg_panel().to_egui(),
        }
    }

    /// no page 글리프. `text-disabled`는 Latte unfocused 바탕 위에서 3에 못 미친다.
    fn no_page_glyph(self, th: &Theme) -> egui::Color32 {
        match self {
            Self::Bed { .. } => th.glyph_dim().to_egui(),
            Self::Plain => th.text_disabled().to_egui(),
        }
    }
}

/// webview-kind surface 의 host chrome 을 패널에 그린다. `nav` 가 Loading/Failed 면
/// 해당 상태 chrome, 그 외(Idle/Done)는 `url` 유무로 boundary(Some)/placeholder(None).
/// boundary는 배경·테두리만 있는 빈 타일이다. 페이지가 잠시 가려진 동안에만 보이므로
/// 글리프·라벨·URL을 그리지 않는다. Failed의 URL 줄은 [`chrome_caption`]이 고른 문자열을 보인다.
/// 배치와 그리기는 갤러리와 같은 [`tasty_ui_widgets::webview_page_tile`]이 맡는다.
/// `empty_hint`는 빈 안내 아래에 보일 caption이다(html surface의 주소창 안내).
pub fn draw_webview_chrome(
    ui: &mut egui::Ui,
    tile: WebviewTile,
    content: egui::Rect,
    url: Option<&str>,
    page_label: Option<&str>,
    nav: NavState,
    empty_hint: Option<&str>,
) {
    let th = theme::theme();
    let notice = crate::boot::display_backend::current_notice();
    let state = page_state(tile, &th, url, page_label, nav, notice);
    // 안내 아래에는 주소창 안내를 두지 않는다. 페이지를 열 수 없는 상태라 안내가 맞지 않는다.
    let empty_hint = empty_hint.filter(|_| notice.is_none());
    tasty_ui_widgets::webview_page_tile(ui, &th, tile.bg(&th), content, state, empty_hint);
}

/// 타일 상태를 고른다. 표시 경로 때문에 이 프로세스가 WebView를 만들 수 없으면(`notice`) 탐색 상태와
/// 관계없이 그 안내다. 그때는 어느 surface에도 WebView가 생기지 않는다.
fn page_state<'a>(
    tile: WebviewTile,
    th: &Theme,
    url: Option<&'a str>,
    page_label: Option<&'a str>,
    nav: NavState,
    notice: Option<tasty_ui_widgets::DisplayNotice>,
) -> WebviewPageState<'a> {
    if let Some(kind) = notice {
        return WebviewPageState::Notice {
            title: crate::i18n::t(kind.title_key()),
            sub: kind.sub_key().map(crate::i18n::t),
            at_end: false,
        };
    }
    match nav {
        NavState::Failed => WebviewPageState::Failed {
            title: crate::i18n::t("webview.error"),
            caption: chrome_caption(url, page_label),
        },
        NavState::Loading => WebviewPageState::Loading {
            title: crate::i18n::t("webview.loading"),
        },
        // URL이 있으면 boundary: 배경·테두리만 남긴다.
        NavState::Idle | NavState::Done if url.is_some() => WebviewPageState::Boundary,
        NavState::Idle | NavState::Done => WebviewPageState::Empty {
            title: crate::i18n::t("webview.no_page"),
            glyph: tile.no_page_glyph(th),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{WebviewTile, chrome_caption};
    use crate::state::surface_focus::FocusMark;
    use tasty_type_appearance::theme::Theme;

    fn latte() -> Theme {
        let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        Theme::with_colors(colors, is_light.unwrap_or(true))
    }

    #[test]
    fn a_bed_kind_tile_takes_the_bed_and_the_others_stay_plain() {
        for th in [tasty_themes::mocha_fallback(), latte()] {
            let on = WebviewTile::of(FocusMark::Bed, true);
            let off = WebviewTile::of(FocusMark::Bed, false);
            assert_eq!(on.bg(&th), th.surface_bed_focused().to_egui());
            assert_eq!(off.bg(&th), th.surface_bed_unfocused().to_egui());
            assert_eq!(on.no_page_glyph(&th), th.glyph_dim().to_egui());
            for focused in [true, false] {
                let plain = WebviewTile::of(FocusMark::Edge, focused);
                assert_eq!(plain.bg(&th), th.bg_panel().to_egui());
                assert_eq!(plain.no_page_glyph(&th), th.text_disabled().to_egui());
            }
        }
    }

    /// Given 표시 경로 안내, When 어느 탐색 상태든, Then 그 kind의 제목·해결책이다. gtk-init은 해결책이 없다.
    #[test]
    fn a_display_notice_replaces_every_navigation_state() {
        use super::{NavState, WebviewPageState, page_state};
        use tasty_ui_widgets::DisplayNotice;
        let th = tasty_themes::mocha_fallback();
        let tile = WebviewTile::of(FocusMark::Edge, true);
        for kind in DisplayNotice::ALL {
            for nav in [
                NavState::Idle,
                NavState::Loading,
                NavState::Done,
                NavState::Failed,
            ] {
                for url in [None, Some("file:///tmp/a.html")] {
                    let state = page_state(tile, &th, url, None, nav, Some(kind));
                    let WebviewPageState::Notice { title, sub, at_end } = state else {
                        panic!("{kind:?} {nav:?}: {state:?}");
                    };
                    assert_eq!(title, crate::i18n::t(kind.title_key()));
                    assert_eq!(sub, kind.sub_key().map(crate::i18n::t));
                    assert!(!at_end);
                }
            }
        }
        let gtk = page_state(
            tile,
            &th,
            None,
            None,
            NavState::Idle,
            Some(DisplayNotice::GtkInit),
        );
        assert!(matches!(gtk, WebviewPageState::Notice { sub: None, .. }));
    }

    /// Given 안내 없음, Then 지금과 같은 상태다(빈 타일·로딩·실패·빈 안내).
    #[test]
    fn without_a_notice_the_navigation_state_decides() {
        use super::{NavState, WebviewPageState, page_state};
        let th = tasty_themes::mocha_fallback();
        let tile = WebviewTile::of(FocusMark::Edge, true);
        let url = Some("https://tasty.dev");
        assert!(matches!(
            page_state(tile, &th, url, None, NavState::Idle, None),
            WebviewPageState::Boundary
        ));
        assert!(matches!(
            page_state(tile, &th, url, None, NavState::Loading, None),
            WebviewPageState::Loading { .. }
        ));
        assert!(matches!(
            page_state(tile, &th, url, None, NavState::Failed, None),
            WebviewPageState::Failed { .. }
        ));
        assert!(matches!(
            page_state(tile, &th, None, None, NavState::Done, None),
            WebviewPageState::Empty { .. }
        ));
    }

    #[test]
    fn a_label_wins_over_the_url() {
        assert_eq!(
            chrome_caption(Some("<html>…</html>"), Some("/docs/readme.md")),
            Some("/docs/readme.md")
        );
        assert_eq!(
            chrome_caption(Some("https://tasty.dev"), Some("Tasty")),
            Some("Tasty")
        );
    }

    #[test]
    fn a_navigable_url_shows_without_a_label() {
        for url in [
            "https://tasty.dev",
            "http://localhost:8080",
            "file:///tmp/a.html",
        ] {
            assert_eq!(chrome_caption(Some(url), None), Some(url));
        }
    }

    #[test]
    fn raw_html_without_a_label_shows_no_caption() {
        assert_eq!(
            chrome_caption(Some("<!doctype html><html></html>"), None),
            None
        );
        assert_eq!(chrome_caption(None, None), None);
    }
}
