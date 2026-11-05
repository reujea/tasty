//! 마크다운 플러그인의 HTML 화면을 egui로 근사한 예제. 실제 WebView는 실행하지 않는다.
//! 제목 크기·표·구문 강조·알림은 Theme 값을 사용하되 브라우저와의 픽셀 일치는 보장하지 않는다.
//! 주소창과 목차는 정적으로 그리며 이미지는 파일을 읽지 않고 대체 영역을 표시한다.

mod callout_kinds;
mod css_path;
mod document;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, Spinner};

pub use callout_kinds::spec as callout_kinds_spec;
pub use css_path::{content_colour_spec, heading_hierarchy_spec};
use document::document;

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 문서 카드 폭(전시 박스).
const DOC_W: LogicalPx = LogicalPx(560.0);
/// 상태 타일 치수.
const TILE_W: LogicalPx = LogicalPx(200.0);
const TILE_H: LogicalPx = LogicalPx(132.0);

/// 주소 띠 견본 폭.
const ADDR_BAR_W: LogicalPx = LogicalPx(360.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        spec::cluster(
            ui,
            theme,
            "address bar — HTML chrome (in-document)",
            |ui| {
                address_bar(ui, theme);
            },
        );
        spec::cluster(
            ui,
            theme,
            "table of contents — collapsible, in-document",
            |ui| {
                toc_chrome(ui, theme);
            },
        );
    });

    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        ui.set_max_width(DOC_W.value());
        document(ui, theme);
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "load failed", |ui| {
            tile(ui, theme, |ui| {
                ui.label(rich(
                    theme,
                    "Failed to load",
                    theme.font_size_max.value(),
                    theme.accent_danger().to_egui(),
                ));
                ui.label(
                    egui::RichText::new("notes.md: No such file")
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted_raised().to_egui()),
                );
            });
        });
        spec::cluster(ui, theme, "empty file", |ui| {
            tile(ui, theme, |ui| {
                ui.label(rich(
                    theme,
                    "This file is empty",
                    theme.font_size_body.value(),
                    theme.text_muted_raised().to_egui(),
                ));
            });
        });
        // 대용량 확인을 취소한 뒤의 대기 상태. 버튼은 파일을 바로 읽지 않고 확인을 다시 띄운다.
        spec::cluster(ui, theme, "over 1 MiB — deferred", |ui| {
            tile(ui, theme, |ui| {
                ui.label(rich(
                    theme,
                    "This file is larger than 1\u{a0}MiB and has not been loaded",
                    theme.font_size_body.value(),
                    theme.text_muted_raised().to_egui(),
                ));
                ui.add_space(theme.spacing_sm.value());
                Button::new("Open file…")
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
            });
        });
        spec::cluster(ui, theme, "loading", |ui| {
            tile(ui, theme, |ui| {
                Spinner::new()
                    .size(theme.spinner_size.value())
                    .show(ui, theme);
                ui.add_space(theme.spacing_sm.value());
                ui.label(rich(
                    theme,
                    "Loading…",
                    theme.font_size_body.value(),
                    theme.text_muted_raised().to_egui(),
                ));
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "addr bar",
                // 높이는 렌더러 CSS의 --md-addr-bar-h를 기준으로 관리한다.
                "--md-addr-bar-h sticky top · bg-sidebar · in-document HTML chrome",
            ),
            (
                "addr field",
                "in-document <input>+<button> · nav-fragment scheme",
            ),
            (
                "body",
                "13 · text-secondary · CSS line-height (full control)",
            ),
            ("h1", "prose-h1 — the only cap-exempt content size"),
            (
                "headings",
                "h1 20 → h6 13, 5-step CSS interpolation (20 · 18.6 · 17.2 · 15.8 · 14.4 · 13)",
            ),
            ("heading style", "600 · text-primary at every level"),
            ("leading", "CSS line-height 1.6 · block spacing CSS"),
            ("small", "11 · muted"),
            (
                "heading id",
                "GitHub-compatible auto slug — no explicit {#id} syntax",
            ),
            (
                "toc",
                "collapsible <nav> · surface-raised · indent = space-sm × level",
            ),
            ("code", "mono · surface-raised · language class preserved"),
            (
                "syntax highlighting",
                "client-side highlight.js (offline vendor) · hljs-* token colors from Theme hues",
            ),
            (
                "hljs roles",
                "keyword mauve · string green · title/function blue · number peach · type yellow · comment text-muted italic · tag teal · variable lavender · built_in red",
            ),
            (
                "diff",
                "deletion / addition bg = accent-danger / accent-success at tint-fill-alpha",
            ),
            ("link", "accent-primary · nav-fragment intercepted"),
            ("table", "real <table> — header band + zebra + padding"),
            ("states", "failed=accent-danger · empty=muted"),
            (
                "deferred",
                "line muted · Button secondary sm “Open file…” below at space-sm · raises the 1 MiB confirm again",
            ),
            (
                "callouts",
                "blockquote + collapsible details — md-quote-bar-width bar · type fill at tint-fill-alpha · 16px icon · radius · marker → icon → label 4 each (md-callout-icon-gap) · chevron marker md-callout-marker-size, right closed / down open",
            ),
        ],
        &[
            TokenChip::new("bg-panel", "surface", theme.bg_panel().to_egui()),
            TokenChip::new(
                "md-doc-bg",
                "document bed (crust)",
                theme.surface("markdown").focused_bg.to_egui(),
            ),
            TokenChip::new(
                "text-primary",
                "headings · strong",
                theme.text_primary().to_egui(),
            ),
            TokenChip::new(
                "text-secondary",
                "body",
                theme.text_secondary_raised().to_egui(),
            ),
            TokenChip::new("text-muted", "caption", theme.text_muted_raised().to_egui()),
            TokenChip::new(
                "accent-primary",
                "link · note alert",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "code bg",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new("separator", "hr", theme.separator.to_egui_premultiplied()),
            TokenChip::new(
                "border-strong",
                "blockquote · table grid",
                theme.border_strong().to_egui(),
            ),
            TokenChip::new(
                "md-table-border",
                "table grid (surface1)",
                theme.md_table_border().to_egui(),
            ),
            TokenChip::new(
                "md-table-header-bg",
                "table header",
                theme.md_table_header_bg().to_egui(),
            ),
            TokenChip::without_color("font-size-prose-h1", "h1 (cap-exempt)"),
            TokenChip::new(
                "md-table-row-bg-zebra",
                "table even row",
                theme.md_table_row_bg_zebra().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "load failed · caution alert",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new(
                "accent-success",
                "tip alert",
                theme.accent_success().to_egui(),
            ),
            TokenChip::new(
                "accent-warning",
                "warning alert",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "spinner-indicator",
                "loading",
                theme.spinner_indicator().to_egui(),
            ),
            TokenChip::without_color("shadow-modal", "popup lift"),
            TokenChip::new(
                "accent-agent",
                "important alert",
                theme.accent_agent().to_egui(),
            ),
            TokenChip::new("mauve", "hljs-keyword", theme.mauve.to_egui()),
            TokenChip::new("blue", "hljs-title/function", theme.blue.to_egui()),
            TokenChip::new("green", "hljs-string", theme.green.to_egui()),
            TokenChip::new("red", "hljs-built_in", theme.red.to_egui()),
            TokenChip::new("peach", "hljs-number", theme.peach.to_egui()),
            TokenChip::new("yellow", "hljs-type", theme.yellow.to_egui()),
            TokenChip::new("teal", "hljs-tag", theme.teal.to_egui()),
            TokenChip::new("lavender", "hljs-variable", theme.lavender.to_egui()),
            TokenChip::without_color("tint-fill-alpha", "diff bg · callout fill"),
        ],
    );

    spec::note(
        ui,
        theme,
        "The plugin renders sanitized HTML in a native WebView and applies Theme values through CSS. It interpolates heading sizes between prose-h1 and body, generates heading IDs and a collapsible table of contents, and uses a bundled highlight.js for fenced code. GFM alerts use localized labels and distinct colors. This gallery reproduces those elements with egui and fixed example data; it does not run the browser, highlighting script or navigation.",
    );
}

/// 상태 타일 — 고정 W×H 테두리 박스, 콘텐츠 세로 가운데.
fn tile(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(TILE_W.value(), TILE_H.value()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space(theme.spacing_xl.value());
                    add(ui);
                },
            );
        });
}

/// 문서 안 HTML 주소 띠(로컬 idle). 상태별 견본은 `md_address` 의 Spec 에 있다.
fn address_bar(ui: &mut egui::Ui, theme: &Theme) {
    super::md_address::bar(
        ui,
        theme,
        ADDR_BAR_W.value(),
        "en",
        &super::md_address::Bar {
            remote: false,
            state: super::md_address::BarState::Idle,
            path: "/docs/readme.md",
            no_go: false,
        },
    );
}

/// 문서 목차를 항상 펼친 상태로 그린다. 클릭 이동·접기 동작은 구현하지 않는다.
fn toc_chrome(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_md.value() as i8,
            theme.spacing_sm.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width((DOC_W - theme.spacing_lg.scaled(2.0)).value());
            // 감싸는 cluster 의 가로 줄바꿈을 이어받지 않도록 목차 행을 세로로 쌓는다.
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(rich(
                        theme,
                        "\u{25be}",
                        theme.font_size_body.value(),
                        theme.text_primary().to_egui(),
                    ));
                    ui.add_space(theme.spacing_xs.value());
                    ui.label(
                        rich(
                            theme,
                            "Table of contents",
                            theme.font_size_body.value(),
                            theme.text_primary().to_egui(),
                        )
                        .strong(),
                    );
                });
                ui.add_space(theme.spacing_xs.value());
                for (level, label) in [
                    (1u8, "Markdown surface"),
                    (2, "Headings & emphasis"),
                    (3, "Lists"),
                    (3, "Code block"),
                    (3, "Image"),
                    (3, "Table"),
                    (3, "Blockquote"),
                    (3, "Callouts"),
                    (4, "Subsection (h4)"),
                ] {
                    toc_row(ui, theme, level, label);
                }
            });
        });
}

/// One TOC entry — indent grows by `--md-space-sm` per level below h1 (`theme_css`'s
/// `.tasty-toc-l<N>` ladder), link-colored label (an in-document `<a href="#slug">`).
fn toc_row(ui: &mut egui::Ui, theme: &Theme, level: u8, label: &str) {
    ui.horizontal(|ui| {
        ui.add_space(level.saturating_sub(1) as f32 * theme.spacing_sm.value());
        ui.label(rich(
            theme,
            label,
            theme.font_size_body.value(),
            theme.accent_primary().to_egui(),
        ));
    });
}

/// 지정 size/color 라벨 텍스트.
fn rich(_theme: &Theme, text: &str, size: f32, color: egui::Color32) -> egui::RichText {
    egui::RichText::new(text).size(size).color(color)
}
