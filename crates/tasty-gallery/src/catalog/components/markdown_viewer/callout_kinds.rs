//! 본체 마크다운 렌더러가 그리는 콜아웃 다섯 종(아이콘·라벨·채움). 문서 카드의 시안 Callouts
//! 표본도 같은 모양이라 `callout_box`를 함께 쓴다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

use super::{DOC_W, rich};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 콜아웃 한 종의 아이콘·강조색·제목색·라벨·본문.
type CalloutKind = (
    icons::MockGlyph,
    fn(&Theme) -> HexColor,
    fn(&Theme) -> HexColor,
    &'static str,
    &'static str,
);

const KINDS: [CalloutKind; 5] = [
    (
        icons::ALERT_CIRCLE,
        Theme::accent_primary,
        Theme::accent_primary_raised,
        "Note",
        "Highlights information users should take into account, even when skimming.",
    ),
    (
        icons::STAR_FILL,
        Theme::accent_success,
        Theme::accent_success_raised,
        "Tip",
        "Optional information to help a user be more successful.",
    ),
    (
        icons::BELL,
        Theme::accent_agent,
        Theme::accent_agent_raised,
        "Important",
        "Crucial information necessary for users to succeed.",
    ),
    (
        icons::ALERT_TRIANGLE,
        Theme::accent_warning,
        Theme::accent_warning_raised,
        "Warning",
        "Critical content demanding immediate user attention due to possible risks.",
    ),
    (
        icons::CLOSE,
        Theme::accent_danger,
        Theme::accent_danger_raised,
        "Caution",
        "Negative potential consequences of an action.",
    ),
];

/// 카탈로그 항목. 동결 명부의 `catalog.rs` 에는 이 호출 한 줄만 둔다.
pub fn spec() -> crate::catalog::Spec {
    crate::catalog::Spec {
        id: "markdown-callout-kinds",
        title: "Markdown callout kinds",
        when: Some("Five GFM / Obsidian kinds as the app renders them — icon · label · type fill"),
        draw,
    }
}

fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.set_max_width(DOC_W.value());
        egui::Frame::new()
            .fill(theme.surface("markdown").focused_bg.to_egui())
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                theme.border_default().to_egui(),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::symmetric(
                theme.spacing_lg.value() as i8,
                theme.spacing_md.value() as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(DOC_W.value());
                let bed = theme.surface("markdown").focused_bg.to_egui();
                for (icon, accent, raised, label, body) in KINDS {
                    callout_box(
                        ui,
                        theme,
                        &Callout {
                            icon,
                            accent: accent(theme),
                            raised: raised(theme),
                            label,
                            body,
                            folded: false,
                            bed,
                        },
                    );
                    ui.add_space(theme.spacing_xs.value());
                }
            });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "kinds",
                "note · tip · important · warning · caution (+ Obsidian aliases)",
            ),
            (
                "box",
                "type colour at tint-fill-alpha over the bed · md-quote-bar-width left bar · radius · 16px icon + label, accent-<hue>-raised · body text-primary",
            ),
            (
                "source",
                "the renderer look — the design Callouts specimen draws the same box",
            ),
        ],
        &[
            TokenChip::new("accent-primary", "note", theme.accent_primary().to_egui()),
            TokenChip::new("accent-success", "tip", theme.accent_success().to_egui()),
            TokenChip::new("accent-agent", "important", theme.accent_agent().to_egui()),
            TokenChip::new(
                "accent-warning",
                "warning",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new("accent-danger", "caution", theme.accent_danger().to_egui()),
            TokenChip::new(
                "accent-<hue>-raised",
                "title and icon",
                theme.accent_primary_raised().to_egui(),
            ),
            TokenChip::new("text-primary", "body", theme.text_primary().to_egui()),
            TokenChip::without_color("md-quote-bar-width", "left bar"),
            TokenChip::without_color("tint-fill-alpha", "type fill"),
        ],
    );
}

/// 콜아웃 하나의 내용. `bed`는 강조색을 섞을 문서 바탕이다.
pub(super) struct Callout<'a> {
    pub icon: icons::MockGlyph,
    pub accent: HexColor,
    pub raised: HexColor,
    pub label: &'a str,
    pub body: &'a str,
    pub folded: bool,
    pub bed: egui::Color32,
}

/// 시안 Callouts의 콜아웃 하나: 강조색 `tint-fill-alpha` 채움 + 왼쪽 md-quote-bar-width
/// 막대 + radius. 제목 줄은 (접힘이면 chevron) + 16px 종류 아이콘 + 굵은 라벨을 space-xs
/// 간격으로 놓고 `accent-<hue>-raised`로 칠한다. 본문은 두 형태 모두 `text-primary`이고
/// 접힌 형태만 caption 크기다.
pub(super) fn callout_box(ui: &mut egui::Ui, theme: &Theme, c: &Callout) {
    // WebView 는 반투명 강조색을 문서 바탕 위에 sRGB 값으로 섞는다. 같은 sRGB 로 미리 섞은
    // 불투명 색을 칠해 그 결과와 맞춘다.
    let color = c.accent;
    let title = c.raised.to_egui();
    let fill = c
        .bed
        .lerp_to_gamma(color.to_egui(), theme.tint_fill_alpha());
    let resp = egui::Frame::new()
        .fill(fill)
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_md.value() as i8,
            theme.spacing_sm.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.md_callout_icon_gap().value();
                let glyph = |ui: &mut egui::Ui, g: icons::MockGlyph, sz: f32| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
                    g.image(sz, title).paint_at(ui, rect);
                };
                if c.folded {
                    glyph(
                        ui,
                        icons::CHEVRON_RIGHT,
                        theme.md_callout_marker_size().value(),
                    );
                }
                glyph(ui, c.icon, theme.icon_glyph_size_md.value());
                ui.label(rich(theme, c.label, theme.font_size_body.value(), title).strong());
            });
            let size = if c.folded {
                theme.font_size_caption.value()
            } else {
                theme.font_size_body.value()
            };
            ui.label(rich(theme, c.body, size, theme.text_primary().to_egui()));
        });
    let r = resp.response.rect;
    let bar = egui::Rect::from_min_size(
        r.min,
        egui::vec2(theme.md_quote_bar_width().value(), r.height()),
    );
    ui.painter().rect_filled(bar, 0.0, color.to_egui());
}
