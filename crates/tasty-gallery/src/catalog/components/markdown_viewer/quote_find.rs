//! 인용문과 찾기 적중의 글자. 시안 `MdQuoteFind`를 옮긴다.
//! 적중은 불투명이라 어느 바탕에 놓여도 같은 글자 쌍이 통과한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::surface_bed;

use super::document::{quote_block, table_fills};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 시안 `maxWidth: 480`. `box-sizing: border-box`라 안쪽 여백을 포함한다.
const SOLO_W: LogicalPx = LogicalPx(480.0);
/// 두 상태를 나란히 놓는 칸의 폭. 안쪽 여백을 포함한다.
const VIEW_W: LogicalPx = LogicalPx(444.0);
/// 시안 `lineHeight: 1.6`.
const LEADING_RATIO: f32 = 1.6;

#[derive(Clone, Copy)]
enum Piece<'a> {
    Text(&'a str),
    Hit(&'a str),
    Current(&'a str),
}

/// 카탈로그 항목. 동결 명부의 `catalog.rs` 에는 이 호출 한 줄만 둔다.
pub fn spec() -> crate::catalog::Spec {
    crate::catalog::Spec {
        id: "markdown-quote-and-find-ink",
        title: "Markdown — quote and find ink on both beds",
        when: Some("a quote takes no opacity · a find hit is opaque · focus changes neither"),
        draw,
    }
}

fn hit_colors(theme: &Theme, current: bool) -> (egui::Color32, egui::Color32) {
    if current {
        (
            theme.md_find_current_bg().to_egui(),
            theme.md_find_current_fg().to_egui(),
        )
    } else {
        (
            theme.md_find_hit_bg().to_egui(),
            theme.md_find_hit_fg().to_egui(),
        )
    }
}

/// 글자와 적중이 섞인 한 줄. 적중의 글자는 아래 글자색을 물려받지 않는다.
fn line(
    ui: &mut egui::Ui,
    theme: &Theme,
    font: &egui::FontId,
    ink: egui::Color32,
    pieces: &[Piece],
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for piece in pieces {
            let (text, fill) = match piece {
                Piece::Text(t) => (*t, None),
                Piece::Hit(t) => (*t, Some(hit_colors(theme, false))),
                Piece::Current(t) => (*t, Some(hit_colors(theme, true))),
            };
            match fill {
                None => {
                    ui.label(egui::RichText::new(text).font(font.clone()).color(ink));
                }
                Some((bg, fg)) => {
                    egui::Frame::new()
                        .fill(bg)
                        .corner_radius(theme.corner_radius_sm.value())
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(text).font(font.clone()).color(fg));
                        });
                }
            }
        }
    });
}

/// 표 칸의 글자나 적중을 왼쪽 가운데 기준으로 칠한다. `ink`는 적중이 아닌 글자의 색이다.
fn cell(p: &egui::Painter, theme: &Theme, at: egui::Pos2, piece: Piece, ink: egui::Color32) {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let (text, fill, fg) = match piece {
        Piece::Text(t) => (t, None, ink),
        Piece::Hit(t) => {
            let (bg, fg) = hit_colors(theme, false);
            (t, Some(bg), fg)
        }
        Piece::Current(t) => {
            let (bg, fg) = hit_colors(theme, true);
            (t, Some(bg), fg)
        }
    };
    let galley = p.layout_no_wrap(text.to_owned(), font, fg);
    let rect = egui::Rect::from_min_size(
        egui::pos2(at.x, at.y - galley.size().y / 2.0),
        galley.size(),
    );
    if let Some(bg) = fill {
        p.rect_filled(rect, theme.corner_radius_sm.value(), bg);
    }
    p.galley(rect.min, galley, fg);
}

/// 시안 `MdTable`: 머리글과 row, zebra 두 줄. 적중은 머리글에도 놓인다.
fn find_table(ui: &mut egui::Ui, theme: &Theme, focused: bool) {
    let (row_bg, zebra_bg) = table_fills(theme, focused);
    let pad_x = theme.md_table_cell_padding_x().value();
    let pad_y = theme.md_table_cell_padding_y().value();
    let row_h = theme.font_size_body.value() * LEADING_RATIO + pad_y * 2.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h * 3.0),
        egui::Sense::hover(),
    );
    let p = ui.painter();
    let r = theme.corner_radius.value();
    let radius = r.round().clamp(0.0, 255.0) as u8;
    let row_rect = |i: f32| {
        egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, row_h * i),
            egui::vec2(rect.width(), row_h),
        )
    };
    p.rect_filled(
        row_rect(0.0),
        egui::CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        },
        theme.md_table_header_bg().to_egui(),
    );
    p.rect_filled(row_rect(1.0), 0.0, row_bg);
    p.rect_filled(
        row_rect(2.0),
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        },
        zebra_bg,
    );
    let border = egui::Stroke::new(
        theme.border_width.value(),
        theme.md_table_border().to_egui(),
    );
    for i in [1.0, 2.0] {
        p.hline(rect.x_range(), rect.top() + row_h * i, border);
    }
    // 선 끝이 둥근 외곽선 밖으로 나오지 않게 안쪽까지만 긋는다.
    p.vline(
        rect.center().x,
        rect.top()..=rect.bottom() - theme.border_width.value(),
        border,
    );
    p.rect_stroke(rect, r, border, egui::StrokeKind::Inside);
    let rows = [
        [Piece::Hit("Theme"), Piece::Text("Bed")],
        [Piece::Hit("theme"), Piece::Text("row")],
        [Piece::Current("theme"), Piece::Text("zebra")],
    ];
    for (i, [left, right]) in rows.into_iter().enumerate() {
        let y = row_rect(i as f32).center().y;
        let ink = if i == 0 {
            theme.md_table_header_fg().to_egui()
        } else {
            theme.md_table_cell_fg().to_egui()
        };
        cell(p, theme, egui::pos2(rect.left() + pad_x, y), left, ink);
        cell(p, theme, egui::pos2(rect.center().x + pad_x, y), right, ink);
    }
}

/// 시안 `MdQuoteFind` 한 상태.
fn quote_find(ui: &mut egui::Ui, theme: &Theme, focused: bool, width: LogicalPx) {
    let body = egui::FontId::proportional(theme.font_size_body.value());
    let mono = egui::FontId::monospace(theme.font_size_caption.value());
    let quote_ink = theme.md_quote_fg().to_egui();
    let bar_w = theme.md_quote_bar_width().value();
    let bar_gap = theme.spacing_md.value();
    egui::Frame::new()
        .fill(surface_bed(theme, focused))
        .inner_margin(theme.spacing_md.value())
        .show(ui, |ui| {
            // 상위 묶음은 가로 배치다. 문서는 세로 배치에서 쌓인다.
            ui.vertical(|ui| {
                ui.set_width(width.value() - theme.spacing_md.value() * 2.0);
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                line(
                    ui,
                    theme,
                    &body,
                    theme.md_doc_fg().to_egui(),
                    &[
                        Piece::Text("Body text with a "),
                        Piece::Hit("theme"),
                        Piece::Text(" hit and the "),
                        Piece::Current("theme"),
                        Piece::Text(" current hit."),
                    ],
                );
                quote_block(ui, theme, bar_w, bar_gap, |ui| {
                    line(
                        ui,
                        theme,
                        &body,
                        quote_ink,
                        &[
                            Piece::Text("A quote with a "),
                            Piece::Hit("theme"),
                            Piece::Text(" hit."),
                        ],
                    );
                    ui.add_space(theme.spacing_xs.value());
                    quote_block(ui, theme, bar_w, bar_gap, |ui| {
                        line(
                            ui,
                            theme,
                            &body,
                            quote_ink,
                            &[Piece::Text("A nested quote keeps the same ink.")],
                        );
                    });
                });
                find_table(ui, theme, focused);
                egui::Frame::new()
                    .fill(theme.md_code_bg().to_egui())
                    .stroke(egui::Stroke::new(
                        theme.border_width.value(),
                        theme.md_code_border().to_egui_premultiplied(),
                    ))
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(theme.spacing_sm.value())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        line(
                            ui,
                            theme,
                            &mono,
                            theme.text_secondary_raised().to_egui(),
                            &[
                                Piece::Text("set "),
                                Piece::Hit("theme"),
                                Piece::Text(" = "),
                                Piece::Current("theme"),
                            ],
                        );
                    });
            });
        });
}

fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        quote_find(ui, theme, true, SOLO_W);
    });
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (focused, label) in [
            (true, "Quote and find, focused"),
            (false, "Quote and find, unfocused"),
        ] {
            spec::cluster(ui, theme, label, |ui| {
                quote_find(ui, theme, focused, VIEW_W);
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "quote",
                "Mocha 9.43 focused, 7.37 unfocused · Latte 6.25, 4.73",
            ),
            ("find hit", "14.76 · 5.03 on any ground"),
            ("find current", "8.91 · 4.91 on any ground"),
            ("focus change", "neither hit fill changes"),
            ("motion", "0ms, one frame"),
        ],
        &[
            TokenChip::new("md-quote-fg", "quote text", theme.md_quote_fg().to_egui()),
            TokenChip::new("md-quote-bar", "quote bar", theme.md_quote_bar().to_egui()),
            TokenChip::new(
                "md-find-hit-bg",
                "find hit fill",
                theme.md_find_hit_bg().to_egui(),
            ),
            TokenChip::new(
                "md-find-hit-fg",
                "find hit text",
                theme.md_find_hit_fg().to_egui(),
            ),
            TokenChip::new(
                "md-find-current-bg",
                "current hit fill",
                theme.md_find_current_bg().to_egui(),
            ),
            TokenChip::new(
                "md-find-current-fg",
                "current hit text",
                theme.md_find_current_fg().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Implementing side: a hit's ink replaces the ink under it, including a link or a highlight hue, so inherit nothing. A hit keeps the tiny radius. Hits and the current hit do not change on a focus change, so they need no pushed value.",
    );
    spec::dont(
        ui,
        theme,
        "Don't put opacity on a quote. It lowers the contrast on one bed more than the other, and it stacks in a nested quote: Latte unfocused was 3.90, nested 3.30.",
    );
}
