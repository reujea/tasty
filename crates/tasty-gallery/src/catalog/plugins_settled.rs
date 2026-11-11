//! Markdown 플러그인의 결정 기록 Spec: 포커스를 따르는 문서 바탕, 주소 표시줄 상태와 대용량 확인.
//! 시안 `plugins.jsx`의 같은 Spec을 옮긴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::surface_bed;

use crate::catalog::components::markdown_viewer::document;
use crate::catalog::components::{html_chrome, html_display_backend, md_address, md_large_file};
use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};

/// 시안 바탕 견본의 치수: 폭 200, 안쪽 여백 12, 표와 코드 사이 8, 표 줄 12, 코드 칸 16, 라벨 간격 6.
const BED_W: LogicalPx = LogicalPx(200.0);
const BED_PAD: LogicalPx = LogicalPx(12.0);
const BED_INNER_GAP: LogicalPx = LogicalPx(8.0);
const BED_STRIPE_H: LogicalPx = LogicalPx(12.0);
const BED_CODE_H: LogicalPx = LogicalPx(16.0);
const BED_LABEL_GAP: LogicalPx = LogicalPx(6.0);
/// 시안 `.fixed-view` 표·코드 칸의 전시 폭. 시안은 칸 폭을 그리드가 정한다.
const VIEW_W: LogicalPx = LogicalPx(360.0);
/// 주소 표시줄 예제와 타일의 폭, 타일 높이.
const ADDRESS_W: LogicalPx = LogicalPx(440.0);
const TILE_H: LogicalPx = LogicalPx(232.0);
/// 타일 본문은 확인 팝업 뒤에서 흐리게 보인다.
const TILE_BODY_OPACITY: f32 = 0.35;

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

/// 전후 비교 견본 한 장. 표 row·zebra와 코드 칸을 바탕 위에 얹는다.
fn bed_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    bed: egui::Color32,
    row: egui::Color32,
    zebra: egui::Color32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = BED_LABEL_GAP.value();
        let stripe = BED_STRIPE_H.value();
        let code_h = BED_CODE_H.value();
        let pad = BED_PAD.value();
        let gap = BED_INNER_GAP.value();
        let edge = theme.border_width.value();
        let table_h = stripe * 4.0 + edge * 2.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(BED_W.value(), pad * 2.0 + table_h + gap + code_h),
            egui::Sense::hover(),
        );
        let p = ui.painter();
        let r = theme.corner_radius.value();
        p.rect_filled(rect, r, bed);
        p.rect_stroke(
            rect,
            r,
            egui::Stroke::new(edge, ec(theme.border_default())),
            egui::StrokeKind::Inside,
        );
        let inner = rect.shrink(pad);
        let table = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), table_h));
        let body = table.shrink(edge);
        for (i, fill) in [ec(theme.md_table_header_bg()), row, zebra, row]
            .into_iter()
            .enumerate()
        {
            let top = body.top() + stripe * i as f32;
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(body.left(), top),
                    egui::vec2(body.width(), stripe),
                ),
                0.0,
                fill,
            );
        }
        p.rect_stroke(
            table,
            r,
            egui::Stroke::new(edge, ec(theme.md_table_border())),
            egui::StrokeKind::Inside,
        );
        let code = egui::Rect::from_min_size(
            egui::pos2(inner.left(), table.bottom() + gap),
            egui::vec2(inner.width(), code_h),
        );
        p.rect_filled(code, r, ec(theme.md_code_bg()));
        p.rect_stroke(
            code,
            r,
            egui::Stroke::new(edge, theme.md_code_border().to_egui_premultiplied()),
            egui::StrokeKind::Inside,
        );
        ui.label(
            egui::RichText::new(label)
                .size(theme.font_size_caption.value())
                .color(ec(theme.text_muted())),
        );
    });
}

/// 표와 코드 칸 한 장. 바탕, row, zebra만 포커스를 따른다.
fn table_and_code(ui: &mut egui::Ui, theme: &Theme, focused: bool) {
    egui::Frame::new()
        .fill(surface_bed(theme, focused))
        .inner_margin(theme.spacing_md.value())
        .show(ui, |ui| {
            // 상위 묶음은 가로 배치다. 표는 세로 배치에서 폭을 받는다.
            ui.vertical(|ui| {
                ui.set_width(VIEW_W.value());
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                document::table(
                    ui,
                    theme,
                    focused,
                    &[
                        ["surface", "viewer", "12"],
                        ["popup", "overlay", "8"],
                        ["banner", "notice", "3"],
                    ],
                );
                egui::Frame::new()
                    .fill(ec(theme.md_code_bg()))
                    .stroke(egui::Stroke::new(
                        theme.border_width.value(),
                        theme.md_code_border().to_egui_premultiplied(),
                    ))
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(theme.spacing_sm.value())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new("fn main() {}")
                                .font(egui::FontId::monospace(theme.font_size_caption.value()))
                                .color(ec(theme.text_secondary_raised())),
                        );
                    });
            });
        });
}

/// 카탈로그 항목. 동결 명부의 `catalog.rs` 에는 이 호출 한 줄만 둔다.
pub fn doc_background_spec() -> crate::catalog::Spec {
    crate::catalog::spec(
        "markdown-doc-bg",
        "Markdown — document background follows focus",
        Some("bed, table row and zebra swap · one frame, no reload"),
        draw_doc_background,
    )
}

/// Plugins › Markdown — 문서 바탕은 터미널과 같은 바탕 쌍을 따른다. 표 row·zebra도 함께 바뀐다.
pub fn draw_doc_background(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        bed_card(
            ui,
            theme,
            "Focused",
            surface_bed(theme, true),
            ec(theme.md_table_row_bg()),
            ec(theme.md_table_row_bg_zebra()),
        );
        bed_card(
            ui,
            theme,
            "Unfocused",
            surface_bed(theme, false),
            ec(theme.md_table_row_bg_unfocused()),
            ec(theme.md_table_row_bg_zebra_unfocused()),
        );
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for (focused, word) in [(true, "focused"), (false, "unfocused")] {
            cluster(ui, theme, &format!("Table and code, {word}"), |ui| {
                ui.push_id(("table", word), |ui| table_and_code(ui, theme, focused));
            });
        }
        for (state, name) in html_chrome::TILE_STATES {
            for (focused, word) in [(true, "focused"), (false, "unfocused")] {
                cluster(ui, theme, &format!("Chrome tile, {name}, {word}"), |ui| {
                    html_chrome::state_tile(
                        ui,
                        theme,
                        state,
                        surface_bed(theme, focused),
                        theme.glyph_dim(),
                    );
                });
            }
        }
        for kind in tasty_ui_widgets::DisplayNotice::ALL {
            for (focused, word) in [(true, "focused"), (false, "unfocused")] {
                let name = format!("Chrome tile, {}, {word}", kind.design_kind());
                cluster(ui, theme, &name, |ui| {
                    ui.push_id(("md-notice", kind.design_kind(), word), |ui| {
                        html_display_backend::notice_tile(
                            ui,
                            theme,
                            surface_bed(theme, focused),
                            html_display_backend::TILE_W,
                            html_display_backend::notice_state(kind, "en", false),
                        );
                    });
                });
            }
        }
    });
    meta(
        ui,
        theme,
        &[
            ("focused", "Mocha #000000, Latte #ffffff"),
            ("unfocused", "Mocha base #1e1e2e, Latte crust #dce0e8"),
            ("same as the terminal", "yes, one pair of beds"),
            ("motion", "0ms, one frame"),
            (
                "stays",
                "body ink, links, code fill, table header and grid, address band",
            ),
            ("unknown focus", "unfocused values"),
            ("no page glyph", "glyph-dim on these beds"),
        ],
        &[
            TokenChip::new(
                "surface-bed-focused",
                "--md-bg, focused",
                ec(theme.surface_bed_focused()),
            ),
            TokenChip::new(
                "surface-bed-unfocused",
                "--md-bg, unfocused",
                ec(theme.surface_bed_unfocused()),
            ),
            TokenChip::new(
                "md-table-row-bg",
                "row, focused",
                ec(theme.md_table_row_bg()),
            ),
            TokenChip::new(
                "md-table-row-bg-unfocused",
                "row, unfocused",
                ec(theme.md_table_row_bg_unfocused()),
            ),
            TokenChip::new(
                "md-table-row-bg-zebra",
                "zebra, focused",
                ec(theme.md_table_row_bg_zebra()),
            ),
            TokenChip::new(
                "md-table-row-bg-zebra-unfocused",
                "zebra, unfocused",
                ec(theme.md_table_row_bg_zebra_unfocused()),
            ),
            TokenChip::new(
                "md-code-bg",
                "code fill → --md-code-bg",
                ec(theme.md_code_bg()),
            ),
            TokenChip::new(
                "md-table-border",
                "grid → --md-border",
                ec(theme.md_table_border()),
            ),
        ],
    );
    note(
        ui,
        theme,
        "Three values change: bed, table row, zebra. A base row would melt into the unfocused Mocha bed, which is base, so the row becomes mantle and the zebra becomes base. Body ink stays md-doc-fg in both states. How long prose reads on pure black is unmeasured, so check it in the app. A chrome tile stands in for the document on the same bed, so nothing jumps when the document appears.",
    );
    dont(
        ui,
        theme,
        "Don't reload the document or rewrite its HTML to change the bed. Scroll, selection and find state must survive a focus change.",
    );
}

/// 로컬 주소 띠 하나. 띠 모양은 `md_address` 의 본 견본과 같다.
fn address_bar(ui: &mut egui::Ui, theme: &Theme, path: &str, editing: bool) {
    let state = if editing {
        md_address::BarState::Editing
    } else {
        md_address::BarState::Idle
    };
    md_address::bar(
        ui,
        theme,
        ADDRESS_W.value(),
        "en",
        &md_address::Bar {
            remote: false,
            state,
            path,
            no_go: false,
        },
    );
}

/// Plugins › Markdown — 주소 표시줄의 두 상태와 타일에 묶인 대용량 확인.
pub fn draw_address_states(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        cluster(ui, theme, "display (idle)", |ui| {
            ui.push_id("idle", |ui| {
                address_bar(ui, theme, "~/work/tasty/README.md", false)
            });
        });
        cluster(ui, theme, "editing (focus)", |ui| {
            ui.push_id("editing", |ui| {
                address_bar(ui, theme, "~/work/tasty/docs/design/systems/theme.md", true)
            });
        });
        cluster(
            ui,
            theme,
            "over 1 MiB — confirm scoped to the tile",
            |ui| {
                let (tile, _) = ui.allocate_exact_size(
                    egui::vec2(ADDRESS_W.value(), TILE_H.value()),
                    egui::Sense::hover(),
                );
                let p = ui.painter();
                p.rect_filled(tile, theme.corner_radius.value(), ec(theme.bg_panel()));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(tile)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(tile.intersect(ui.clip_rect()));
                child.spacing_mut().item_spacing.y = 0.0;
                child.push_id("tile", |ui| {
                    address_bar(ui, theme, "~/work/tasty/docs/CHANGELOG-full.md", false)
                });
                let bar_bottom = child.min_rect().bottom();
                child.scope(|ui| {
                    ui.multiply_opacity(TILE_BODY_OPACITY);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(
                            theme.spacing_lg.value() as i8,
                            theme.spacing_md.value() as i8,
                        ))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Changelog")
                                    .size(theme.font_size_prose_h1.value())
                                    .color(ec(theme.text_primary())),
                            );
                            ui.label(
                                egui::RichText::new("A very long history…")
                                    .size(theme.font_size_body.value())
                                    .color(ec(theme.text_secondary())),
                            );
                        });
                });
                // 막은 범위는 주소 표시줄 아래 타일뿐이다. 창 전체를 덮지 않는다.
                let scope = egui::Rect::from_min_max(egui::pos2(tile.left(), bar_bottom), tile.max);
                ui.painter()
                    .with_clip_rect(scope.intersect(ui.clip_rect()))
                    .rect_filled(scope, 0.0, ec(theme.scrim()));
                // 시안 scrim 은 `alignItems: center; justifyContent: center` 다. 카드 크기를 먼저
                // 재고 막은 범위의 정중앙에 놓는다.
                let inner = scope.shrink(theme.spacing_md.value());
                let mut sizing = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("large-file-sizing")
                        .max_rect(inner)
                        .layout(egui::Layout::top_down(egui::Align::Min))
                        .sizing_pass()
                        .invisible(),
                );
                md_large_file::popup_card(&mut sizing, theme);
                let card = egui::Rect::from_center_size(inner.center(), sizing.min_rect().size());
                let mut popup = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("large-file-popup")
                        .max_rect(card)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                popup.set_clip_rect(scope.intersect(ui.clip_rect()));
                md_large_file::popup_card(&mut popup, theme);
            },
        );
    });
    meta(
        ui,
        theme,
        &[
            ("field", "mono path · click to edit"),
            ("idle", "secondary text, input-border"),
            ("editing", "focus ring + caret, input-border-focus"),
            ("Go", "IconButton · ↵ also confirms"),
            ("popup scope", "surface (clamped to tile)"),
            ("popup", "360px · Open / Cancel"),
        ],
        &[
            TokenChip::new("input-bg", "field fill", ec(theme.input_bg())),
            TokenChip::new(
                "input-border-focus",
                "edit border",
                ec(theme.input_border_focus()),
            ),
            TokenChip::new("border-focus", "focus ring", ec(theme.border_focus())),
            TokenChip::new("accent-warning", "size chip", ec(theme.accent_warning())),
            TokenChip::without_color("shadow-modal", "popup lift"),
            TokenChip::new(
                "accent-primary",
                "Open (primary)",
                ec(theme.accent_primary()),
            ),
        ],
    );
    note(
        ui,
        theme,
        "The address bar is the shared editable path field with a file leading icon — same edit / navigate / revert contract as the Explorer toolbar. The large-file confirm is surface-scoped, so it dims only this tile; Cancel opens nothing.",
    );
}
