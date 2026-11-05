//! 마크다운 본문이 WebView와 CSS로 그려질 때의 색 대응표와 제목 단계. 본체
//! `tasty-plugin-markdown` 렌더러의 `theme_css`·`hljs_css`·`heading_sizes_px`와 같은 값을 쓴다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

use super::rich;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 대응 한 줄: 왼쪽 이름 · 토큰 이름 · 그 색을 읽는 함수.
type MapEntry = (&'static str, &'static str, fn(&Theme) -> HexColor);

/// CSS 변수 → Tasty 토큰. 렌더러 `theme_css`의 색 변수 중 시안 Meta가 다루는 것.
const CSS_VARS: &[MapEntry] = &[
    ("--md-fg", "text-secondary", Theme::text_secondary),
    ("--md-strong", "text-primary", Theme::text_primary),
    ("--md-link", "accent-primary", Theme::accent_primary),
    ("--md-code-bg", "surface-raised", Theme::surface_raised),
    ("--md-quote-bar", "border-strong", Theme::border_strong),
];

/// highlight.js 역할 → 팔레트 색. 렌더러 `hljs_css`와 같은 순서다.
const HLJS_ROLES: &[MapEntry] = &[
    ("keyword", "mauve", |t| t.mauve),
    ("string", "green", |t| t.green),
    ("title / function", "blue", |t| t.blue),
    ("number", "peach", |t| t.peach),
    ("type", "yellow", |t| t.yellow),
    ("comment (italic)", "text-muted", Theme::text_muted),
    ("tag", "teal", |t| t.teal),
    ("variable", "lavender", |t| t.lavender),
    ("built_in", "red", |t| t.red),
];

/// 카탈로그 항목: 본문 색 대응.
pub fn content_colour_spec() -> crate::catalog::Spec {
    crate::catalog::Spec {
        id: "markdown-content-colour",
        title: "Markdown — content colour (WebView + CSS)",
        when: Some("theme_css() writes tokens into --md-* · hljs_css() maps highlight roles"),
        draw: draw_content_colour,
    }
}

/// 카탈로그 항목: 제목 단계.
pub fn heading_hierarchy_spec() -> crate::catalog::Spec {
    crate::catalog::Spec {
        id: "markdown-heading-hierarchy",
        title: "Markdown — heading hierarchy (CSS-interpolated)",
        when: Some("heading_sizes_px() — five steps from prose-h1 20 down to body 13"),
        draw: draw_heading_hierarchy,
    }
}

fn draw_content_colour(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // cluster 는 내용을 가로로 흘리므로 대응 행은 세로 묶음에 넣는다.
        spec::cluster(ui, theme, "CSS variable → Tasty token", |ui| {
            ui.vertical(|ui| {
                for (var, tok, color) in CSS_VARS {
                    map_row(ui, theme, var, tok, color(theme));
                }
                // separator 는 premultiplied 로 저장되어 접근자가 다르다.
                let rule = theme.separator.unpremultiplied();
                map_row(ui, theme, "--md-code-border · --md-rule", "separator", rule);
            });
        });
        spec::cluster(ui, theme, "highlight.js role → palette", |ui| {
            ui.vertical(|ui| {
                for (role, tok, color) in HLJS_ROLES {
                    map_row(ui, theme, role, tok, color(theme));
                }
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("path", "WebView · theme_css() → CSS variables"),
            ("--md-fg", "text-secondary"),
            ("--md-strong", "text-primary"),
            ("--md-link", "accent-primary"),
            ("--md-code-bg / -border", "surface-raised / separator"),
            ("--md-quote-bar · --md-rule", "border-strong · separator"),
            (
                "hljs",
                "keyword mauve · string green · title/function blue · number peach · type yellow · comment text-muted italic · tag teal · variable lavender · built_in red",
            ),
            (
                "diff",
                "deletion / addition bg = accent-danger / accent-success at tint-fill-alpha",
            ),
            (
                "callouts",
                "type colour bar (md-quote-bar-width) + type colour fill at tint-fill-alpha + radius + 16px type icon before the label · note accent-primary · tip accent-success · important accent-agent · warning accent-warning · caution accent-danger",
            ),
        ],
        &[
            TokenChip::new(
                "text-secondary",
                "body",
                theme.text_secondary_raised().to_egui(),
            ),
            TokenChip::new(
                "text-primary",
                "headings / strong",
                theme.text_primary().to_egui(),
            ),
            TokenChip::new("accent-primary", "links", theme.accent_primary().to_egui()),
            TokenChip::new(
                "surface-raised",
                "code fills",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "border-strong",
                "blockquote bar",
                theme.border_strong().to_egui(),
            ),
            TokenChip::new(
                "separator",
                "code-block border",
                theme.separator.to_egui_premultiplied(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The stylesheet reads only these variables, so the page follows Mocha and Latte one to one. Both themes follow automatically because the palette roles re-point per theme. The retired egui_commonmark and syntect path no longer applies.",
    );
}

fn draw_heading_hierarchy(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for (i, size) in heading_sizes(theme).into_iter().enumerate() {
            ladder_row(ui, theme, &format!("h{}", i + 1), |ui| {
                ui.label(
                    egui::RichText::new(format!("The quick brown fox — {size:.1}"))
                        .size(size)
                        .strong()
                        .color(theme.text_primary().to_egui()),
                );
            });
        }
        ladder_row(ui, theme, "p", |ui| {
            ui.label(rich(
                theme,
                "Body — 13, CSS line-height 1.6, secondary.",
                theme.font_size_body.value(),
                theme.text_secondary().to_egui(),
            ));
        });
        ladder_row(ui, theme, "small", |ui| {
            ui.label(rich(
                theme,
                "Caption — 11, muted.",
                theme.font_size_caption.value(),
                theme.text_muted().to_egui(),
            ));
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "ladder",
                "CSS — 5-step interpolation 20 → 13 (--md-h1 … --md-h6)",
            ),
            ("h1", "prose-h1 (20) · cap-exempt"),
            ("h6", "body 13"),
            ("leading", "CSS line-height 1.6"),
            ("block spacing", "CSS"),
            ("small", "11 · muted"),
        ],
        &[
            TokenChip::without_color("font-size-prose-h1", "Heading anchor (cap-exempt)"),
            TokenChip::new(
                "text-primary",
                "headings / strong",
                theme.text_primary().to_egui(),
            ),
            TokenChip::new("text-secondary", "body", theme.text_secondary().to_egui()),
            TokenChip::new("text-muted", "de-emphasis", theme.text_muted().to_egui()),
        ],
    );
}

/// 렌더러 `heading_sizes_px`와 같은 계산: h1 = prose-h1, h6 = body, 그 사이는 같은 간격이다.
pub(super) fn heading_sizes(theme: &Theme) -> [f32; 6] {
    let h1 = theme.font_size_prose_h1.value();
    let body = theme.font_size_body.value();
    let step = (h1 - body) / 5.0;
    std::array::from_fn(|i| if i == 5 { body } else { h1 - step * i as f32 })
}

/// 왼쪽 mono 태그 + 견본 한 줄.
fn ladder_row(ui: &mut egui::Ui, theme: &Theme, tag: &str, draw: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(
            egui::vec2(theme.spacing_xl.value(), theme.font_size_body.value()),
            egui::Sense::hover(),
        );
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            tag,
            egui::FontId::monospace(theme.font_size_micro.value()),
            theme.text_muted().to_egui(),
        );
        ui.add_space(theme.spacing_md.value());
        draw(ui);
    });
}

/// 대응 한 줄: 왼쪽 이름(mono) · 색 견본 · 토큰 이름.
fn map_row(ui: &mut egui::Ui, theme: &Theme, name: &str, tok: &str, color: HexColor) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(name)
                .monospace()
                .size(theme.font_size_caption.value())
                .color(theme.text_secondary().to_egui()),
        );
        let side = theme.spacing_md.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        ui.painter().rect(
            r,
            theme.corner_radius_sm.value(),
            color.to_egui(),
            egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
        ui.label(
            egui::RichText::new(tok)
                .monospace()
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    });
}
