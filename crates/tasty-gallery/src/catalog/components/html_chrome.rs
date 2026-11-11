//! 네이티브 WebView 영역의 경계와 로딩·오류 안내 예제.
//! 갤러리는 웹 페이지를 띄우지 않고 주변 UI만 Theme 값으로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{HtmlAddressKind, Spinner, SurfaceFocusEdge};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::{Section, Spec};

/// chrome 타일 폭(시안 `HtmlTile` width 220).
const TILE_W: LogicalPx = LogicalPx(220.0);
/// chrome 타일 높이.
const TILE_H: LogicalPx = LogicalPx(150.0);
/// 시안 `html-focus-edge` 주소 띠 표본의 크기(`size-360`, 160).
const BAND_W: LogicalPx = LogicalPx(360.0);
const BAND_H: LogicalPx = LogicalPx(160.0);

/// 시안 `HtmlTile`의 네 상태.
#[derive(Clone, Copy)]
pub(crate) enum TileState {
    Boundary,
    Placeholder,
    Loading,
    Error,
}

pub(crate) const TILE_STATES: [(TileState, &str); 4] = [
    (TileState::Boundary, "boundary"),
    (TileState::Placeholder, "placeholder"),
    (TileState::Loading, "loading"),
    (TileState::Error, "error"),
];

/// 상태 타일 하나. `bed`는 타일 바탕, `idle_glyph`는 페이지 없음 글리프 색이다.
pub(crate) fn state_tile(
    ui: &mut egui::Ui,
    theme: &Theme,
    state: TileState,
    bed: egui::Color32,
    idle_glyph: tasty_type_appearance::color::HexColor,
) {
    tile(ui, theme, bed, |ui| match state {
        // 메뉴·팝업이 불러온 페이지를 잠시 숨기는 동안 보이는 자리다. 글리프·라벨·URL 없이 타일만 그린다.
        TileState::Boundary => {}
        TileState::Placeholder => {
            glyph(
                ui,
                icons::HTML,
                theme.icon_glyph_size_md.value(),
                idle_glyph,
            );
            gap(ui, theme);
            label(ui, theme, "No page loaded", theme.text_muted());
        }
        TileState::Loading => {
            Spinner::new()
                .size(theme.spinner_size.value())
                .show(ui, theme);
            gap(ui, theme);
            label(ui, theme, "Loading…", theme.text_muted());
        }
        TileState::Error => {
            glyph(
                ui,
                icons::ALERT_CIRCLE,
                theme.icon_glyph_size_md.value(),
                theme.accent_danger_raised(),
            );
            gap(ui, theme);
            label(ui, theme, "Failed to load", theme.accent_danger_raised());
            url_line(ui, theme, "https://offline.example");
        }
    });
}

pub fn section() -> Section {
    crate::catalog::section(
        "html-chrome",
        "HTML (webview) chrome",
        vec![
            Spec {
                id: "html-chrome",
                title: "HTML (webview) chrome",
                when: Some(
                    "Native overlay · thin chrome · boundary / placeholder / loading / error",
                ),
                draw,
            },
            super::html_display_backend::spec(),
            Spec {
                id: "html-focus-edge",
                title: "HTML — focus edge above the band",
                when: Some("edge → address band → banners → page · nothing else follows focus"),
                draw: draw_focus_edge,
            },
        ],
    )
}

/// 시안 `SurfaceLeaf`: 띠 행과 그 아래 내용. 띠는 두 상태에서 같은 행을 차지한다.
fn leaf(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    focused: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.vertical(|ui| {
        // 띠 행과 내용 사이에만 간격을 두지 않는다. 내용의 간격은 원래 값을 쓴다.
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        let (row, _) = ui.allocate_exact_size(
            egui::vec2(width, theme.surface_focus_edge_width().value()),
            egui::Sense::hover(),
        );
        ui.spacing_mut().item_spacing.y = spacing;
        SurfaceFocusEdge {
            focused,
            ring: None,
            corner_radius: LogicalPx(0.0),
        }
        .paint(ui.painter(), theme, row);
        body(ui);
    });
}

fn draw_focus_edge(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (state, name) in TILE_STATES {
            for (focused, word) in [(true, "focused"), (false, "unfocused")] {
                spec::cluster(ui, theme, &format!("{name}, {word}"), |ui| {
                    leaf(ui, theme, TILE_W.value(), focused, |ui| {
                        state_tile(
                            ui,
                            theme,
                            state,
                            theme.bg_panel().to_egui(),
                            theme.text_disabled(),
                        );
                    });
                });
            }
        }
        spec::cluster(ui, theme, "band, field editing, focused", |ui| {
            leaf(ui, theme, BAND_W.value(), true, |ui| {
                super::html_address::band_surface(
                    ui,
                    theme,
                    "html-edge-editing",
                    BAND_W,
                    BAND_H,
                    HtmlAddressKind::Local,
                    "~/work/tasty/target/rep",
                    true,
                );
            });
        });
        spec::cluster(ui, theme, "band, unfocused", |ui| {
            leaf(ui, theme, BAND_W.value(), false, |ui| {
                super::html_address::band_surface(
                    ui,
                    theme,
                    "html-edge-idle",
                    BAND_W,
                    BAND_H,
                    HtmlAddressKind::Local,
                    "~/work/tasty/target/report.html",
                    false,
                );
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("focus", "surface focus edge only"),
            (
                "band",
                "unchanged by focus, and the address band is not a focus mark",
            ),
            ("banners", "unchanged by focus"),
            (
                "chrome tiles",
                "the 4 states unchanged by focus, below the edge",
            ),
            ("page", "author CSS, never touched"),
        ],
        &[
            TokenChip::new(
                "surface-focus-edge-focused",
                "focused",
                theme.surface_focus_edge_focused().to_egui(),
            ),
            TokenChip::new(
                "surface-focus-edge-unfocused",
                "unfocused",
                theme.surface_focus_edge_unfocused().to_egui(),
            ),
            TokenChip::new(
                "input-border-focus",
                "field ring",
                theme.input_border_focus().to_egui(),
            ),
        ],
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (state, name) in [
            (TileState::Boundary, "boundary — hidden page"),
            (TileState::Placeholder, "placeholder — no URL"),
            (TileState::Loading, "loading"),
            (TileState::Error, "error — load failed"),
        ] {
            spec::cluster(ui, theme, name, |ui| {
                state_tile(
                    ui,
                    theme,
                    state,
                    theme.bg_panel().to_egui(),
                    theme.text_disabled(),
                );
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("kind", "rendering = webview · OS overlay"),
            ("content", "native WebView — token-irrelevant"),
            ("chrome", "tile boundary + state placeholder"),
            (
                "boundary",
                "plain tile only — while an egui overlay hides a loaded page; no glyph, label or URL",
            ),
            ("placeholder", "globe (disabled) + 'No page loaded'"),
            ("loading", "Spinner (ui-widgets)"),
            ("error", "alertCircle · accent-danger-raised"),
            ("frame", "bg-panel · 1px border-default"),
        ],
        &[
            TokenChip::new("bg-panel", "tile", theme.bg_panel().to_egui()),
            TokenChip::new("border-default", "tile", theme.border_default().to_egui()),
            TokenChip::new(
                "text-muted",
                "placeholder / loading label",
                theme.text_muted().to_egui(),
            ),
            TokenChip::new(
                "text-disabled",
                "placeholder glyph",
                theme.text_disabled().to_egui(),
            ),
            TokenChip::new(
                "spinner-indicator",
                "loading",
                theme.spinner_indicator().to_egui(),
            ),
            TokenChip::new(
                "accent-danger-raised",
                "error",
                theme.accent_danger_raised().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "An HTML surface mounts a native OS WebView overlay, so the page pixels are not \
         tasty's to theme — only the chrome is. This specimen is deliberately thin: the \
         tile boundary where the overlay attaches, plus the placeholder / loading / error \
         states the host paints before or instead of a live page. The boundary tile is \
         plain: it shows only while a menu, a popup or an overlapping toast card hides a \
         loaded page, so it carries no glyph, label or URL.",
    );
}

/// 고정 W×H 테두리 타일, 콘텐츠를 상단에서 가운데 정렬로 쌓는다. 내용과 무관하게 크기가 같다.
/// 좌우에는 시안 `HtmlTile`의 내용 padding 16(space-lg)을 두며 바깥 폭은 그대로다.
fn tile(ui: &mut egui::Ui, theme: &Theme, bed: egui::Color32, add: impl FnOnce(&mut egui::Ui)) {
    let pad_x = theme.spacing_lg.value();
    egui::Frame::new()
        .fill(bed)
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(pad_x as i8, 0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(TILE_W.value() - pad_x * 2.0, TILE_H.value()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    // 시안 타일은 내용과 무관하게 고정 크기다. 내용이 없는 boundary 도 같은 크기로 그린다.
                    ui.set_min_size(ui.available_size());
                    ui.add_space(theme.spacing_xl.value() * 2.0);
                    add(ui);
                },
            );
        });
}

fn glyph(ui: &mut egui::Ui, g: icons::MockGlyph, size: f32, color: impl Into<egui::Color32>) {
    ui.add(g.image(size, color.into()));
}

fn label(ui: &mut egui::Ui, theme: &Theme, text: &str, color: impl Into<egui::Color32>) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_body.value())
            .color(color.into()),
    );
}

/// URL 줄 — 시안 `HtmlTile`의 sub 줄(mono caption 11 · text-muted · 한 줄 말줄임).
fn url_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(egui::FontId::monospace(theme.font_size_caption.value()))
                .color(theme.text_muted().to_egui()),
        )
        .truncate(),
    );
}

fn gap(ui: &mut egui::Ui, theme: &Theme) {
    ui.add_space(theme.spacing_sm.value());
}
