//! 표시 경로 때문에 WebView를 만들 수 없을 때 페이지 영역에 보이는 안내(시안 `html-display-backend-notice`).
//! 타일은 본체와 같은 [`tasty_ui_widgets::webview_page_tile`]의 `Notice`이고 kind와 문구 키는
//! [`DisplayNotice`]가 정한다. 문구는 본체와 같은 `lang/{code}.toml`에서 읽는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    DisplayNotice, HtmlAddressKind, WebviewPageState, WebviewPageTileOutput, webview_page_tile,
};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};
use crate::i18n::t_lang;

/// 시안 `HtmlTile`의 표본 크기(220 × 150)와 좁은 폭(160).
pub(crate) const TILE_W: LogicalPx = LogicalPx(220.0);
const NARROW_W: LogicalPx = LogicalPx(160.0);
const TILE_H: LogicalPx = LogicalPx(150.0);
/// 시안 `HtmlAddressSurfaceG` 표본 크기(`size-460`, 240).
const BAND_W: LogicalPx = LogicalPx(460.0);
const BAND_H: LogicalPx = LogicalPx(240.0);

const LANGS: [&str; 4] = ["en", "ko", "ja", "zh-Hans"];

/// 시안 `DB_CAUSES`의 순서와 설명.
const CAUSES: [(DisplayNotice, &str); 4] = [
    (DisplayNotice::NativeWayland, "cause 1 · no XWayland"),
    (
        DisplayNotice::NativeWaylandXFailed,
        "cause 1b · X server in DISPLAY unreachable",
    ),
    (
        DisplayNotice::GdkBackend,
        "cause 2 · GDK_BACKEND without x11",
    ),
    (DisplayNotice::GtkInit, "GTK init failed · X11 window"),
];

/// 시안 `DB_OVERFLOW`: 블록이 영역보다 커서 해결책 줄이 스크롤로만 보이는 표본(kind, 언어, 폭).
const OVERFLOW: [(DisplayNotice, &str, LogicalPx); 6] = [
    (DisplayNotice::GdkBackend, "en", TILE_W),
    (DisplayNotice::GdkBackend, "ja", TILE_W),
    (DisplayNotice::NativeWaylandXFailed, "ja", TILE_W),
    (DisplayNotice::NativeWayland, "ja", NARROW_W),
    (DisplayNotice::NativeWaylandXFailed, "ja", NARROW_W),
    (DisplayNotice::GdkBackend, "ja", NARROW_W),
];

/// `lang`의 안내 상태. 해결책 키가 없는 kind는 해결책 줄을 비운다.
pub(crate) fn notice_state(
    kind: DisplayNotice,
    lang: &str,
    at_end: bool,
) -> WebviewPageState<'static> {
    WebviewPageState::Notice {
        title: t_lang(lang, kind.title_key()),
        sub: kind.sub_key().map(|key| t_lang(lang, key)),
        at_end,
    }
}

/// 고정 크기 타일 하나. `bed`는 타일 바탕이다.
pub(crate) fn notice_tile(
    ui: &mut egui::Ui,
    theme: &Theme,
    bed: egui::Color32,
    width: LogicalPx,
    state: WebviewPageState<'_>,
) -> WebviewPageTileOutput {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), TILE_H.value()),
        egui::Sense::hover(),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    webview_page_tile(&mut child, theme, bed, rect, state, None)
}

pub fn spec() -> crate::catalog::Spec {
    crate::catalog::spec(
        "html-display-backend-notice",
        "HTML and Markdown — display backend notice",
        Some(
            "native-wayland · native-wayland-x · gdk-backend · gtk-init · cause and fix, wrapped, never ellipsized · scrolls when taller than the rect",
        ),
        draw,
    )
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let latte = with_zoom(crate::host_shell::latte_theme());
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for (kind, cap) in CAUSES {
            cluster(ui, theme, cap, |ui| {
                ui.push_id(("db", kind.design_kind()), |ui| {
                    notice_tile(
                        ui,
                        theme,
                        theme.bg_panel().to_egui(),
                        TILE_W,
                        notice_state(kind, "en", false),
                    );
                });
            });
        }
        cluster(ui, theme, "html surface with its band · cause 1", |ui| {
            super::html_address::band_page(
                ui,
                theme,
                "db-band-local",
                BAND_W,
                BAND_H,
                HtmlAddressKind::Local,
                "~/work/tasty/target/report.html",
                DisplayNotice::NativeWayland.design_kind(),
            );
        });
        cluster(
            ui,
            theme,
            "html mirror, remote web address · cause 2",
            |ui| {
                super::html_address::band_page(
                    ui,
                    theme,
                    "db-band-mirror",
                    BAND_W,
                    BAND_H,
                    HtmlAddressKind::Mirror,
                    "build-eu ↦ https://intranet.example/status",
                    DisplayNotice::GdkBackend.design_kind(),
                );
            },
        );
        for (kind, cap) in CAUSES {
            cluster(ui, &latte, &format!("Latte · {cap}"), |ui| {
                ui.push_id(("db-latte", kind.design_kind()), |ui| {
                    notice_tile(
                        ui,
                        &latte,
                        latte.bg_panel().to_egui(),
                        TILE_W,
                        notice_state(kind, "en", false),
                    );
                });
            });
        }
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for (kind, _) in CAUSES {
            for lang in LANGS {
                let name = format!("html-notice-{}-{lang} · 220 × 150", kind.design_kind());
                cluster(ui, theme, &name, |ui| {
                    ui.push_id(("db-lang", kind.design_kind(), lang), |ui| {
                        notice_tile(
                            ui,
                            theme,
                            theme.bg_panel().to_egui(),
                            TILE_W,
                            notice_state(kind, lang, false),
                        );
                    });
                });
            }
        }
        for (kind, _) in CAUSES {
            let name = format!("html-notice-{}-narrow · ja · 160 × 150", kind.design_kind());
            cluster(ui, theme, &name, |ui| {
                ui.push_id(("db-narrow", kind.design_kind()), |ui| {
                    notice_tile(
                        ui,
                        theme,
                        theme.bg_panel().to_egui(),
                        NARROW_W,
                        notice_state(kind, "ja", false),
                    );
                });
            });
        }
        for (kind, lang, width) in OVERFLOW {
            let name = format!(
                "html-notice-{}-{lang}-{}-scrolled-end",
                kind.design_kind(),
                width.value()
            );
            cluster(ui, theme, &name, |ui| {
                ui.push_id(
                    ("db-end", kind.design_kind(), lang, width.value() as u32),
                    |ui| {
                        notice_tile(
                            ui,
                            theme,
                            theme.bg_panel().to_egui(),
                            width,
                            notice_state(kind, lang, true),
                        );
                    },
                );
            });
        }
    });
    meta(
        ui,
        theme,
        &[
            (
                "kinds",
                "native-wayland (1) · native-wayland-x (1b) · gdk-backend (2) · gtk-init (GTK init failed, X11 window) — one at a time",
            ),
            (
                "where",
                "the page rect of html, markdown and html mirror surfaces; the band stays above it",
            ),
            (
                "when",
                "the webview could not be created; the notice stays while the surface is visible, until Tasty restarts",
            ),
            ("glyph", "info · glyph-dim"),
            ("title", "cause · body · text-muted · wraps"),
            (
                "sub",
                "fix (gtk-init: none, the line stays reserved and empty) · caption · text-muted · wraps · never ellipsis",
            ),
            (
                "block",
                "centred while it fits · otherwise starts at the top padding and the rect scrolls vertically — never clipped",
            ),
            ("button", "none, and no link, for every kind"),
            (
                "copy keys",
                "webview.native_wayland(_sub) · webview.native_wayland_x_failed(_sub) · webview.gdk_backend(_sub) · webview.gtk_init_failed (no _sub)",
            ),
        ],
        &[
            TokenChip::new("glyph-dim", "info glyph", theme.glyph_dim().to_egui()),
            TokenChip::new(
                "text-muted",
                "title · fix line",
                theme.text_muted().to_egui(),
            ),
            TokenChip::without_color("font-size-caption", "fix line"),
            TokenChip::without_color("space-md", "padding"),
            TokenChip::without_color("space-sm", "glyph · title · fix gaps"),
        ],
    );
    note(
        ui,
        theme,
        "Every cause is drawn in all four languages at the gallery tile size and the Japanese text at 160 wide. \
         Where the block is taller than the rect the tile scrolls, so the fix is reached by scrolling, never cut; \
         those tiles also appear scrolled to the end. gtk-init has no fix line and fits at both sizes, so it has \
         no scrolled frame. The copy is the app's, from the same lang files.",
    );
    dont(
        ui,
        theme,
        "Don't put the fix in a tooltip or behind a link, add a button such as one that opens the user guide, \
         or ellipsize the fix. The notice must name the cause and the fix where it is read.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given 네 언어 카탈로그, Then 안내 kind의 제목·해결책 키가 모두 있고 GTK 초기화 실패의 해결책 키는 없다.
    #[test]
    fn every_notice_key_is_in_every_catalog() {
        for lang in LANGS {
            let table = crate::i18n::catalog_value(lang);
            let has = |key: &str| {
                let (section, name) = key.split_once('.').expect("dotted key");
                table
                    .get(section)
                    .and_then(|s| s.get(name))
                    .is_some_and(|v| v.as_str().is_some_and(|v| !v.is_empty()))
            };
            for kind in DisplayNotice::ALL {
                assert!(has(kind.title_key()), "{lang}: {}", kind.title_key());
                if let Some(sub) = kind.sub_key() {
                    assert!(has(sub), "{lang}: {sub}");
                }
            }
            assert!(!has("webview.gtk_init_failed_sub"), "{lang}");
        }
    }
}
