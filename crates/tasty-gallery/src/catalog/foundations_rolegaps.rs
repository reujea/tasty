//! Foundations › Role gaps의 색 결정 Spec: 역할이 없던 색 일곱 자리(C1–C7)와 C1 정정.
//! 시안 `foundations.jsx`의 같은 Spec을 옮긴다. 표 열 폭·예제 폭은 시안의 전시용 치수다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage};

/// C1–C7 표의 열 폭: id · 이전 역할 · 스와치 · 현재 역할 · 판정. 위치 설명 열은 남는 폭을 쓴다.
const ROLE_ID_COL: LogicalPx = LogicalPx(26.0);
const ROLE_WAS_COL: LogicalPx = LogicalPx(190.0);
const ROLE_SWATCH: LogicalPx = LogicalPx(20.0);
const ROLE_NOW_COL: LogicalPx = LogicalPx(210.0);
const ROLE_VERDICT_COL: LogicalPx = LogicalPx(200.0);
/// 위치 설명 열이 이보다 좁아지지 않게 한다.
const ROLE_PLACE_MIN: LogicalPx = LogicalPx(120.0);
/// 시안 행 안쪽 여백 `8px 10px`의 가로값과 행 안 간격 12.
const ROLE_ROW_PAD_X: LogicalPx = LogicalPx(10.0);

/// C1 정정 예제: 미니 팝업 폭, 머리 높이, 안쪽 여백.
const MINI_POPUP_W: LogicalPx = LogicalPx(216.0);
const MINI_POPUP_HEAD_H: LogicalPx = LogicalPx(28.0);
const MINI_PAD_X: LogicalPx = LogicalPx(10.0);
/// 테마 카드 안쪽 여백 14와 예제 사이 간격 14, 캡션 아래 간격 5.
const THEME_CARD_PAD: LogicalPx = LogicalPx(14.0);
const CAPTION_GAP: LogicalPx = LogicalPx(5.0);
/// appearance 카드·pane divider 상자의 폭, 타일 높이, divider 상자 높이.
const SIDE_BOX_W: LogicalPx = LogicalPx(150.0);
const APPEARANCE_TILE_H: LogicalPx = LogicalPx(64.0);
const DIVIDER_BOX_H: LogicalPx = LogicalPx(104.0);

/// (id, 위치, 이전 역할, 현재 역할, 판정)
const ROLE_ROWS: [(&str, &str, &str, &str, &str); 7] = [
    (
        "C1",
        "pane divider · GPU-inactive surface border · popup frame",
        "surface-active (a fill)",
        "border-frame",
        "new role · popup frame MOVES",
    ),
    (
        "C2",
        "sidebar dim chevron · dim icons",
        "text-placeholder (unentered text)",
        "glyph-dim",
        "new role · same pixel",
    ),
    (
        "C3",
        "disabled text in ports / convert / remote",
        "text-placeholder",
        "text-disabled",
        "MOVES — disabled is its own ink",
    ),
    (
        "C4",
        "tab-strip scroll arrow, disabled",
        "border-strong (a border)",
        "text-disabled",
        "MOVES — same rule as C3",
    ),
    (
        "C5",
        "status-bar theme indicator",
        "accent-warning / accent-agent",
        "statusbar-theme-glyph",
        "MOVES — a glyph, no colour role",
    ),
    (
        "C6",
        "StatusDot idle",
        "text-muted",
        "status-dot-idle",
        "MOVES — the canonical idle tone",
    ),
    (
        "C7",
        "Plugins window header glyph",
        "accent-attention (a state)",
        "accent-decorative",
        "new role · same pixel",
    ),
];

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

fn role_color(theme: &Theme, id: &str) -> egui::Color32 {
    match id {
        "C1" => ec(theme.border_frame()),
        "C2" => ec(theme.glyph_dim()),
        "C3" | "C4" => ec(theme.text_disabled()),
        "C5" => ec(theme.statusbar_theme_glyph()),
        "C6" => ec(theme.status_dot_idle()),
        _ => ec(theme.accent_decorative()),
    }
}

fn text(ui: &mut egui::Ui, s: &str, size: LogicalPx, color: egui::Color32, mono: bool) {
    let mut rt = egui::RichText::new(s).size(size.value()).color(color);
    if mono {
        rt = rt.monospace();
    }
    ui.label(rt);
}

/// 고정 폭 칸 하나. 칸 안의 글자는 줄바꿈하지 않고 자른다.
fn cell(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) {
    let line_h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(w, line_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(w);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            add(ui);
        },
    );
}

/// Foundations › Role gaps — 자기 역할이 없던 색 일곱 자리.
pub fn draw_role_colors(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            let gap = theme.spacing_md.value();
            let fixed = ROLE_ID_COL.value()
                + ROLE_WAS_COL.value()
                + ROLE_SWATCH.value()
                + ROLE_NOW_COL.value()
                + ROLE_VERDICT_COL.value()
                + gap * 5.0
                + ROLE_ROW_PAD_X.value() * 2.0
                + theme.border_width.value() * 2.0;
            let place_w = (ui.available_width() - fixed).max(ROLE_PLACE_MIN.value());
            for (id, place, was, now, verdict) in ROLE_ROWS {
                egui::Frame::new()
                    .fill(ec(theme.bg_panel()))
                    .stroke(egui::Stroke::new(
                        theme.border_width.value(),
                        ec(theme.border_default()),
                    ))
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(egui::Margin::symmetric(
                        ROLE_ROW_PAD_X.value() as i8,
                        theme.spacing_sm.value() as i8,
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            let muted = ec(theme.text_muted());
                            cell(ui, ROLE_ID_COL.value(), |ui| {
                                text(ui, id, theme.font_size_caption, muted, true)
                            });
                            cell(ui, place_w, |ui| {
                                text(
                                    ui,
                                    place,
                                    theme.font_size_term_sm,
                                    ec(theme.text_secondary()),
                                    false,
                                )
                            });
                            cell(ui, ROLE_WAS_COL.value(), |ui| {
                                text(ui, was, theme.font_size_caption, muted, true)
                            });
                            let (sw, _) = ui.allocate_exact_size(
                                egui::Vec2::splat(ROLE_SWATCH.value()),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect(
                                sw,
                                theme.corner_radius_sm.value(),
                                role_color(theme, id),
                                egui::Stroke::new(
                                    theme.border_width.value(),
                                    ec(theme.border_default()),
                                ),
                                egui::StrokeKind::Inside,
                            );
                            cell(ui, ROLE_NOW_COL.value(), |ui| {
                                text(
                                    ui,
                                    now,
                                    theme.font_size_caption,
                                    ec(theme.text_primary()),
                                    true,
                                )
                            });
                            let verdict_fg = if verdict.contains("MOVES") {
                                ec(theme.accent_warning())
                            } else {
                                muted
                            };
                            cell(ui, ROLE_VERDICT_COL.value(), |ui| {
                                text(ui, verdict, theme.font_size_caption, verdict_fg, false)
                            });
                        });
                    });
            }
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "new semantic roles",
                "border-frame · glyph-dim · accent-decorative",
            ),
            ("new component role", "statusbar-theme-glyph (→ glyph-dim)"),
            ("moves (pixel changes)", "C3 · C4 · C5 · C6"),
            (
                "Latte",
                "all four new roles track the ramp — no per-theme remap",
            ),
            (
                "contrast",
                "C3 / C4 are disabled ink — exempt from 4.5:1; C2 is chrome, not text",
            ),
        ],
        &[
            TokenChip::new(
                "border-frame",
                "divider / frame edge",
                ec(theme.border_frame()),
            ),
            TokenChip::new("glyph-dim", "dim chrome glyphs", ec(theme.glyph_dim())),
            TokenChip::new(
                "accent-decorative",
                "header ornament",
                ec(theme.accent_decorative()),
            ),
            TokenChip::new(
                "text-disabled",
                "every disabled label / glyph",
                ec(theme.text_disabled()),
            ),
        ],
    );
    note(
        ui,
        theme,
        "The three value-preserving holdouts elsewhere (tab hover fill, tab separator, toast border) are confirmed as intended visual changes — move them to canonical; they were never design questions.",
    );
}

/// 캡션 한 줄과 그 아래 예제 하나.
fn captioned(
    ui: &mut egui::Ui,
    theme: &Theme,
    caption: &str,
    caption_fg: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = CAPTION_GAP.value();
        text(ui, caption, theme.font_size_caption, caption_fg, false);
        add(ui);
    });
}

/// 시안의 미니 팝업: 머리(bg-sidebar) · 항목 · 안쪽 divider(border-strong) · 섹션 라벨.
fn mini_popup(ui: &mut egui::Ui, theme: &Theme, edge: egui::Color32) {
    let bw = theme.border_width.value();
    egui::Frame::new()
        .fill(ec(theme.bg_panel()))
        .stroke(egui::Stroke::new(bw, edge))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            let w = MINI_POPUP_W.value() - bw * 2.0;
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(
                egui::vec2(w, MINI_POPUP_HEAD_H.value()),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(head, 0.0, ec(theme.bg_sidebar()));
            p.hline(
                head.x_range(),
                head.bottom() - bw * 0.5,
                egui::Stroke::new(bw, edge),
            );
            p.text(
                egui::pos2(head.left() + MINI_PAD_X.value(), head.center().y),
                egui::Align2::LEFT_CENTER,
                "Listening ports",
                egui::FontId::proportional(theme.font_size_term_sm.value()),
                ec(theme.text_secondary()),
            );
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: MINI_PAD_X.value() as i8,
                    right: MINI_PAD_X.value() as i8,
                    top: MINI_PAD_X.value() as i8,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    text(
                        ui,
                        crate::i18n::t("port_scanner.filter_show_all_system"),
                        theme.font_size_term_sm,
                        ec(theme.text_primary()),
                        false,
                    );
                });
            ui.add_space(MINI_PAD_X.value());
            let (line, _) = ui.allocate_exact_size(egui::vec2(w, bw), egui::Sense::hover());
            ui.painter()
                .rect_filled(line, 0.0, ec(theme.border_strong()));
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: MINI_PAD_X.value() as i8,
                    right: MINI_PAD_X.value() as i8,
                    top: theme.spacing_sm.value() as i8,
                    bottom: theme.spacing_md.value() as i8,
                })
                .show(ui, |ui| {
                    text(
                        ui,
                        "Favorites",
                        theme.font_size_caption,
                        ec(theme.text_muted()),
                        false,
                    );
                });
        });
}

/// 테두리 상자 하나(폭 고정). appearance 카드·pane divider 예제의 바깥 상자.
fn side_box(
    ui: &mut egui::Ui,
    theme: &Theme,
    h: f32,
    paint: impl FnOnce(&egui::Painter, egui::Rect),
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SIDE_BOX_W.value(), h), egui::Sense::hover());
    let p = ui.painter();
    p.rect(
        rect,
        theme.corner_radius.value(),
        ec(theme.bg_panel()),
        egui::Stroke::new(theme.border_width.value(), ec(theme.border_default())),
        egui::StrokeKind::Inside,
    );
    paint(p, rect.shrink(theme.border_width.value()));
}

fn theme_card(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    egui::Frame::new()
        .fill(ec(theme.bg_app()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(THEME_CARD_PAD.value() as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            text(
                ui,
                label,
                theme.font_size_caption,
                ec(theme.text_muted_raised()),
                false,
            );
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = THEME_CARD_PAD.value();
                captioned(
                    ui,
                    theme,
                    "settled — border-frame",
                    ec(theme.accent_success()),
                    |ui| mini_popup(ui, theme, ec(theme.border_frame())),
                );
                captioned(
                    ui,
                    theme,
                    "rejected — border-strong",
                    ec(theme.text_muted_raised()),
                    |ui| mini_popup(ui, theme, ec(theme.border_strong())),
                );
                let pad = MINI_PAD_X.value();
                captioned(
                    ui,
                    theme,
                    "appearance cards — inactive → border-strong",
                    ec(theme.text_muted_raised()),
                    |ui| {
                        side_box(
                            ui,
                            theme,
                            APPEARANCE_TILE_H.value() + pad * 2.0,
                            |p, inner| {
                                let inner = inner.shrink(pad - theme.border_width.value());
                                let gap = theme.spacing_sm.value();
                                let tile_w = (inner.width() - gap) / 2.0;
                                let left = egui::Rect::from_min_size(
                                    inner.min,
                                    egui::vec2(tile_w, inner.height()),
                                );
                                let right = left.translate(egui::vec2(tile_w + gap, 0.0));
                                let r = theme.corner_radius.value();
                                p.rect(
                                    left,
                                    r,
                                    ec(theme.surface_raised()),
                                    egui::Stroke::new(
                                        theme.border_width.value() * 2.0,
                                        ec(theme.accent_primary()),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                p.rect(
                                    right,
                                    r,
                                    ec(theme.surface_raised()),
                                    egui::Stroke::new(
                                        theme.border_width.value(),
                                        ec(theme.border_strong()),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                            },
                        );
                    },
                );
                captioned(
                    ui,
                    theme,
                    "pane divider — unchanged",
                    ec(theme.text_muted_raised()),
                    |ui| {
                        side_box(ui, theme, DIVIDER_BOX_H.value(), |p, inner| {
                            p.rect_filled(inner, 0.0, ec(theme.bg_app()));
                            let x = inner.center().x;
                            p.vline(
                                x,
                                inner.y_range(),
                                egui::Stroke::new(
                                    theme.border_width.value(),
                                    ec(theme.border_frame()),
                                ),
                            );
                        });
                    },
                );
            });
        });
}

/// Foundations › Role gaps — C1 정정: 팝업 테두리는 한 단계 움직이고, 그게 맞다.
pub fn draw_c1_correction(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    stage(ui, theme, StageVariant::Solo, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        theme_card(ui, &mocha, "Mocha");
        theme_card(ui, &latte, "Latte");
    });
    meta(
        ui,
        theme,
        &[
            ("popup frame", "border-frame"),
            (
                "popup titlebar underline",
                "the same — it is part of the frame",
            ),
            (
                "popup inner dividers",
                "unchanged — border-strong, one step below",
            ),
            ("pane divider", "border-frame — pixel unchanged"),
            (
                "GPU-inactive surface border",
                "border-frame — pixel unchanged",
            ),
            ("Mocha frame vs panel", "1.80:1 → 2.46:1"),
            ("Latte frame vs panel", "1.60:1 → 1.91:1"),
            (
                "4.5:1",
                "not applicable — a 1px structural edge is not text; the move raises separation in both themes",
            ),
            (
                "transcription fix",
                "C1 reads \"new role; the popup frame moves one step up\", not \"pixel-unchanged\"",
            ),
            (
                "fifth call site",
                "Settings › Appearance inactive card border is NOT a frame — it is a line inside a panel → border-strong (neutral-500 → 400, pixel changes). The frame list stays at four.",
            ),
        ],
        &[
            TokenChip::new(
                "border-frame",
                "popup frame · titlebar underline · pane divider · GPU-inactive surface",
                ec(theme.border_frame()),
            ),
            TokenChip::new(
                "border-strong",
                "dividers INSIDE a surface",
                ec(theme.border_strong()),
            ),
        ],
    );
    dont(
        ui,
        theme,
        "Don't mint a popup-only border role to put the old value back. Three places, one meaning — the edge that bounds a frame — and a fourth role would only record that one of them used to be wrong.",
    );
    note(
        ui,
        theme,
        "Frame vs partition, stated once. border-frame bounds a surface against what is behind it: popup frame · popup titlebar underline · pane divider · GPU-inactive surface border. Anything drawn inside a panel that separates siblings — dividers, card outlines, section rules — is a partition and reads border-strong (or border-default when quieter). The appearance-tab card border arrived on border_frame() only by a value-preserving port from surface_active(); it moves down one step so the outer-edge > inner-line order holds inside Settings too. The active card keeps its accent edge.",
    );
}
