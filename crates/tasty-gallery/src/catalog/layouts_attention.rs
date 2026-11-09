//! Layouts › Attention kinds. 시안 `layouts.jsx`의 Workspace row · Collapsed rail ·
//! Tab title & surface border 무대를 옮긴다. 배지 묶음, 레일 점, 탭 제목 색, 테두리 순위는
//! 본체와 같은 `tasty_ui_widgets` 함수로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Attention, RailDot, attention_edge_stroke, occupancy_edge_shows, occupancy_edge_stroke,
    paint_rail_dot, running_count_column, surface_edge_attention, tab_title_color,
    workspace_attention_badges,
};

use crate::catalog::icons::{MockGlyph, TERMINAL};
use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage};
use crate::catalog::{Section, Spec, layouts_settled};

/// 시안 `AttentionRows` 무대 값: 경우 설명 열 폭, 행 폭(펼친 사이드바).
const ROW_CAPTION_W: LogicalPx = LogicalPx(190.0);
const ROW_W: LogicalPx = LogicalPx(212.0);
/// 시안 `AttentionRows` 경우 사이 간격(gap 6). 무대 배치 값이라 역할 토큰이 없다.
const ROW_CASE_GAP: LogicalPx = LogicalPx(6.0);
/// 시안 `AttentionRail` 경우 설명 칸 최소 높이(minHeight 24). 두 줄 설명이 아바타 줄을 밀지 않게 한다.
const RAIL_CAPTION_MIN_H: LogicalPx = LogicalPx(24.0);
/// 테두리 무대의 surface 높이(시안 무대 200 − 위아래 여백).
const LADDER_PANE_H: LogicalPx = LogicalPx(176.0);

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

pub fn section() -> Section {
    Section {
        id: "attention",
        title: "Attention kinds",
        specs: vec![
            Spec {
                id: "attention-scale",
                title: "The scale — kind → color → rank",
                when: Some("needs-input 30 · completion 10 · error 40 and approval 20 reserved"),
                draw: layouts_settled::draw_attention_scale,
            },
            Spec {
                id: "attention-rows",
                title: "Workspace row — one or two count badges",
                when: Some(
                    "trailing slot is kind-independent · needs-input leads when both · 99+ on both",
                ),
                draw: draw_rows,
            },
            Spec {
                id: "attention-rail",
                title: "Collapsed rail — one dot, highest rank wins",
                when: Some("needs-input › completion › busy · no count on the rail"),
                draw: draw_rail,
            },
            Spec {
                id: "attention-ladder",
                title: "Tab title & surface border — the priority ladder",
                when: Some(
                    "title: needs-input → completion → active → rest · border: needs-input → occupancy → completion",
                ),
                draw: draw_ladder,
            },
        ],
    }
}

/// 경우 하나: 설명, 이름, 실행 중 여부, NeedsInput 수, Completion 수.
type RowCase = (&'static str, &'static str, bool, usize, usize);

const ROW_CASES: &[RowCase] = &[
    ("Completion only", "docs-site", true, 0, 3),
    ("NeedsInput only — same slot", "data-etl", true, 1, 0),
    ("Both — needs-input leads", "tasty-core", false, 2, 5),
    ("Overflow — 99+ on both", "monorepo", true, 120, 140),
    ("Quiet", "scratch", false, 0, 0),
];

fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .monospace()
            .size(theme.font_size_micro.value())
            .color(ec(theme.text_muted())),
    );
}

/// 본체 `draw_workspace_card`와 같은 여백·점 슬롯 구조의 행. 끝의 배지 묶음은 본체와 같은
/// 공용 함수가 그린다.
fn ws_row(ui: &mut egui::Ui, theme: &Theme, name: &str, busy: bool, ni: usize, done: usize) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: theme.workspace_row_padding_x().value() as i8,
            right: theme.spacing_sm.value() as i8,
            top: theme.spacing_xs.value() as i8,
            bottom: theme.spacing_xs.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_width(
                ROW_W.value() - theme.workspace_row_padding_x().value() - theme.spacing_sm.value(),
            );
            ui.horizontal(|ui| {
                let running = usize::from(busy);
                ui.spacing_mut().item_spacing.x = theme.workspace_running_count_gap().value();
                let row_h = ui.fonts(|f| {
                    f.row_height(&egui::FontId::proportional(theme.font_size_body.value()))
                });
                let (dot_rect, _) = ui.allocate_exact_size(
                    egui::vec2(theme.workspace_dot_slot().value(), row_h),
                    egui::Sense::hover(),
                );
                let dot = if busy {
                    theme.accent_success()
                } else {
                    theme.status_dot_idle()
                };
                ui.painter().circle_filled(
                    dot_rect.center(),
                    theme.badge_dot_size().value() * 0.5,
                    ec(dot),
                );
                ui.spacing_mut().item_spacing.x = theme.workspace_dot_gap().value();
                running_count_column(ui, theme, running, row_h, theme.text_muted());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    workspace_attention_badges(ui, theme, ni, done);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(name)
                                    .size(theme.font_size_body.value())
                                    .color(ec(theme.text_secondary())),
                            )
                            .truncate(),
                        );
                    });
                });
            });
        });
}

fn draw_rows(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(ec(theme.bg_sidebar()))
            .inner_margin(theme.spacing_md.value() as i8)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = ROW_CASE_GAP.value();
                for (cap, name, busy, ni, done) in ROW_CASES {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                        ui.allocate_ui_with_layout(
                            egui::vec2(ROW_CAPTION_W.value(), 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_width(ROW_CAPTION_W.value());
                                caption(ui, theme, cap);
                            },
                        );
                        ws_row(ui, theme, name, *busy, *ni, *done);
                    });
                }
            });
    });
    meta(
        ui,
        theme,
        &[
            ("slot", "row trailing edge — kind-independent"),
            ("order (both)", "needs-input → completion"),
            ("gap", "badge-group-gap"),
            ("overflow", "99+ — both kinds"),
            ("height", "16px badge, micro 10px numerals"),
        ],
        &[
            TokenChip::new(
                "badge-warning-bg",
                "NeedsInput badge fill",
                ec(theme.badge_warning_bg()),
            ),
            TokenChip::new(
                "badge-warning-fg",
                "NeedsInput numeral",
                ec(theme.badge_warning_fg()),
            ),
            TokenChip::new(
                "badge-primary-bg",
                "Completion badge fill",
                ec(theme.badge_primary_bg()),
            ),
            TokenChip::new(
                "badge-primary-fg",
                "Completion numeral",
                ec(theme.badge_primary_fg()),
            ),
            TokenChip::without_color("badge-group-gap", "4px between the two"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't reserve an empty slot for the missing kind. A fixed two-slot row makes every quiet workspace look like it has holes in it; the group is inline and collapses to the count that exists.",
    );
}

/// 경우 하나: 설명, 머리글자, NeedsInput, Completion, 실행 중, 결과.
type RailCase = (&'static str, &'static str, bool, bool, bool, &'static str);

const RAIL_CASES: &[RailCase] = &[
    ("busy only", "B", false, false, true, "running — green"),
    ("completion", "C", false, true, false, "finished — blue"),
    ("needs-input", "N", true, false, false, "blocked — yellow"),
    ("completion + busy", "C", false, true, true, "blue wins"),
    (
        "needs-input + completion",
        "N",
        true,
        true,
        false,
        "yellow wins",
    ),
    ("all three", "A", true, true, true, "yellow wins"),
];

/// 본체 `draw_collapsed_avatar`와 같은 크기의 아바타와 머리글자. 점은 공용 함수가 그린다.
fn rail_avatar(ui: &mut egui::Ui, theme: &Theme, letter: &str, dot: Option<RailDot>) {
    let size = egui::vec2(
        theme.sidebar_collapsed_slot_width.value(),
        theme.sidebar_collapsed_workspace_height.value(),
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::monospace(theme.font_size_body.value()),
        ec(theme.text_muted()),
    );
    if let Some(dot) = dot {
        paint_rail_dot(ui.painter(), theme, rect, dot);
    }
}

fn draw_rail(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(ec(theme.bg_sidebar()))
            .inner_margin(egui::Margin::symmetric(
                theme.spacing_md.value() as i8,
                theme.spacing_md.value() as i8,
            ))
            .show(ui, |ui| {
                let col_w = (ui.available_width() / RAIL_CASES.len() as f32).floor();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, (cap, letter, ni, done, busy, res)) in RAIL_CASES.iter().enumerate() {
                        let resp = ui.allocate_ui_with_layout(
                            egui::vec2(col_w, 0.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.set_width(col_w);
                                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                                ui.allocate_ui_with_layout(
                                    egui::vec2(col_w, RAIL_CAPTION_MIN_H.value()),
                                    egui::Layout::top_down(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_height(RAIL_CAPTION_MIN_H.value());
                                        caption(ui, theme, cap);
                                    },
                                );
                                rail_avatar(ui, theme, letter, RailDot::resolve(*ni, *done, *busy));
                                ui.label(
                                    egui::RichText::new(*res)
                                        .size(theme.font_size_micro.value())
                                        .color(ec(theme.text_secondary())),
                                );
                            },
                        );
                        if i + 1 < RAIL_CASES.len() {
                            let r = resp.response.rect;
                            ui.painter().vline(
                                r.max.x,
                                r.y_range(),
                                egui::Stroke::new(
                                    theme.border_width.value(),
                                    theme.separator.to_egui_premultiplied(),
                                ),
                            );
                        }
                    }
                });
            });
    });
    meta(
        ui,
        theme,
        &[
            ("slot", "1 dot, top-right of the avatar"),
            ("size", "6 · status-dot-size-compact"),
            (
                "ring",
                "1.5px status-dot-ring (→ bg-sidebar) on every dot, busy included — keeps the dot legible over any avatar",
            ),
            ("order", "needs-input › completion › busy"),
            ("count", "not shown — expand the sidebar"),
        ],
        &[
            TokenChip::new(
                "status-dot-needs-input",
                "rail dot — NeedsInput",
                ec(theme.status_dot_needs_input()),
            ),
            TokenChip::new(
                "status-dot-completion",
                "rail dot — Completion",
                ec(theme.status_dot_completion()),
            ),
            TokenChip::new(
                "status-dot-success",
                "rail dot — busy/running",
                ec(theme.status_dot_success()),
            ),
            TokenChip::without_color("status-dot-size-compact", "6px"),
            TokenChip::new(
                "status-dot-ring",
                "cut-out ring (→ bg-sidebar)",
                ec(theme.status_dot_ring()),
            ),
            TokenChip::without_color("status-dot-ring-width", "1.5px — OFF-SCALE hairline"),
        ],
    );
    note(
        ui,
        theme,
        "Attention beats activity. Busy-green says “something is happening”, which is the normal state of this product and the least actionable thing on the rail; both attention kinds are events that want a human. The previous code already preferred highlight over busy — this makes it a rule.",
    );
}

/// 탭 하나: 제목, attention, 활성 여부, 표지.
type TabCase = (&'static str, Option<Attention>, bool, &'static str);

const TAB_CASES: &[TabCase] = &[
    ("build.log", Some(Attention::NeedsInput), false, "blocked"),
    ("deploy.sh", Some(Attention::Completion), false, "done"),
    ("zsh", None, true, "active"),
    ("notes.md", None, false, "rest"),
];

fn paint_glyph(
    ui: &mut egui::Ui,
    glyph: MockGlyph,
    rect: egui::Rect,
    size: f32,
    color: egui::Color32,
) {
    glyph.image(size, color).paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(size, size)),
    );
}

fn tab_strip(ui: &mut egui::Ui, theme: &Theme) {
    let bar_h = theme.tab_bar_height.value();
    let tab_w = theme.tab_width.value();
    let w = tab_w * TAB_CASES.len() as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, bar_h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, ec(theme.bg_sidebar()));
    let pad = theme.spacing_sm.value();
    let icon = theme.icon_glyph_size_sm.value();
    for (i, (name, kind, active, mark)) in TAB_CASES.iter().enumerate() {
        let tab = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + tab_w * i as f32, rect.min.y),
            egui::vec2(tab_w, bar_h),
        );
        p.rect_filled(
            tab,
            0.0,
            ec(if *active {
                theme.tab_bg_active()
            } else {
                theme.tab_bg()
            }),
        );
        if *active {
            let bar = egui::Rect::from_min_size(
                tab.min,
                egui::vec2(tab_w, theme.tab_indicator_width().value()),
            );
            p.rect_filled(bar, 0.0, ec(theme.tab_indicator()));
        }
        p.vline(
            tab.max.x,
            tab.y_range(),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.tab_separator().to_egui_premultiplied(),
            ),
        );
        let fg = ec(tab_title_color(theme, *kind, *active));
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(tab.min.x + pad, tab.min.y),
            egui::vec2(icon, bar_h),
        );
        paint_glyph(ui, TERMINAL, icon_rect, icon, fg);
        p.text(
            egui::pos2(icon_rect.max.x + theme.spacing_xs.value(), tab.center().y),
            egui::Align2::LEFT_CENTER,
            *name,
            egui::FontId::proportional(theme.tab_bar_label_font_size.value()),
            fg,
        );
        p.text(
            egui::pos2(tab.max.x - pad, tab.center().y),
            egui::Align2::RIGHT_CENTER,
            *mark,
            egui::FontId::monospace(theme.font_size_micro.value()),
            ec(theme.text_muted()),
        );
    }
    p.hline(
        rect.x_range(),
        rect.max.y - theme.border_width.value() * 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// 테두리 경우: 라벨, 설명, 마지막 줄, attention, 점유 여부.
type PaneCase = (
    &'static str,
    &'static str,
    &'static str,
    Option<Attention>,
    bool,
);

const PANE_CASES: &[PaneCase] = &[
    (
        "needs input",
        "blocked — rank 30",
        "Overwrite build/? [y/N]",
        Some(Attention::NeedsInput),
        false,
    ),
    (
        "occupied · soft",
        "held — below needs-input",
        "running tests…",
        None,
        true,
    ),
    (
        "completed",
        "rank 10 — clears on focus",
        "build passed ✓",
        Some(Attention::Completion),
        false,
    ),
];

/// 본체와 같은 순서로 테두리를 모은다: attention 선, 그 위에 점유선. 두 판정 함수가
/// 한 자리에 선 하나만 남긴다.
fn pane_strokes(theme: &Theme, kind: Option<Attention>, occupied: bool) -> Vec<egui::Stroke> {
    let mut strokes = Vec::new();
    if let Some(k) = surface_edge_attention(kind, occupied) {
        strokes.push(attention_edge_stroke(theme, k));
    }
    if occupied && occupancy_edge_shows(kind) {
        strokes.push(occupancy_edge_stroke(theme, false));
    }
    strokes
}

fn ladder_pane(ui: &mut egui::Ui, theme: &Theme, w: f32, case: &PaneCase) {
    let (label, sub, line, kind, occupied) = *case;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w, LADDER_PANE_H.value()), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let term = theme.surface("terminal");
    let strokes = pane_strokes(theme, kind, occupied);
    // 머리줄 라벨은 맨 위에 보이는 선의 색을 따른다.
    let stroke = strokes.last().copied().unwrap_or(egui::Stroke::NONE);
    let edge_w = strokes.iter().map(|s| s.width).fold(0.0, f32::max);
    let radius = theme.corner_radius.value();
    p.rect_filled(rect, radius, ec(term.focused_bg));
    let pad = theme.spacing_sm.value();
    let head_h = theme.font_size_caption.value() + pad * 2.0;
    let head = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), head_h)).shrink(edge_w);
    p.rect_filled(head, 0.0, ec(theme.bg_panel()));
    p.hline(
        head.x_range(),
        head.max.y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let label_galley = p.layout_no_wrap(
        label.to_string(),
        egui::FontId::monospace(theme.font_size_caption.value()),
        stroke.color,
    );
    let lx = head.min.x + pad;
    let label_w = label_galley.size().x;
    p.galley(
        egui::pos2(lx, head.center().y - label_galley.size().y * 0.5),
        label_galley,
        stroke.color,
    );
    p.text(
        egui::pos2(lx + label_w + theme.spacing_xs.value(), head.center().y),
        egui::Align2::LEFT_CENTER,
        format!("· {sub}"),
        egui::FontId::proportional(theme.font_size_micro.value()),
        ec(theme.text_muted()),
    );
    let mono = egui::FontId::monospace(theme.font_size_term_sm.value());
    let y = head.max.y + theme.spacing_md.value();
    p.text(
        egui::pos2(rect.min.x + pad, y),
        egui::Align2::LEFT_TOP,
        "~/tasty main",
        mono.clone(),
        ec(theme.accent_success()),
    );
    p.text(
        egui::pos2(
            rect.min.x + pad,
            y + theme.font_size_term_sm.value() + theme.spacing_xs.value(),
        ),
        egui::Align2::LEFT_TOP,
        format!("❯ {line}"),
        mono,
        ec(term.focused_fg),
    );
    for s in strokes {
        p.rect_stroke(rect, radius, s, egui::StrokeKind::Inside);
    }
}

fn draw_ladder(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| tab_strip(ui, theme));
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(ec(theme.bg_panel()))
            .inner_margin(theme.spacing_md.value() as i8)
            .show(ui, |ui| {
                let gap = theme.spacing_sm.value();
                let n = PANE_CASES.len() as f32;
                let w = ((ui.available_width() - gap * (n - 1.0)) / n).floor();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for case in PANE_CASES {
                        ladder_pane(ui, theme, w, case);
                    }
                });
            });
    });
    meta(
        ui,
        theme,
        &[
            ("tab — needs-input", "tab-fg-needs-input"),
            ("tab — completion", "tab-fg-completion"),
            ("tab — active", "tab-fg-active"),
            ("tab — rest", "tab-fg"),
            ("border — needs-input", "2px, inside"),
            ("border priority", "needs-input > occupancy > completion"),
        ],
        &[
            TokenChip::new(
                "tab-fg-needs-input",
                "blocked tab title",
                ec(theme.tab_fg_needs_input()),
            ),
            TokenChip::new(
                "tab-fg-completion",
                "finished tab title",
                ec(theme.tab_fg_completion()),
            ),
            TokenChip::new(
                "surface-highlight-input-border",
                "blocked surface edge",
                ec(theme.surface_highlight_input_border()),
            ),
            TokenChip::without_color("surface-highlight-input-width", "2px — matches completion"),
        ],
    );
    note(
        ui,
        theme,
        "Why NeedsInput outranks occupancy. Occupancy reads as “held, working, as expected” — which is exactly the state a blocked prompt would hide behind. A session that has stopped to ask you something must not look like a session that is busy. Completion stays below occupancy, as before. Attention clears only when the surface actually receives focus (focused pane, current workspace, window focused). The visible focused tab therefore never shows a tint; an active tab in an unfocused pane or another workspace can hold attention and keeps its tint by the ladder above (needs-input / completion over active).",
    );
    dont(
        ui,
        theme,
        "Don't stack the two edges (a 1px occupancy line inside a 2px NeedsInput line). One channel, one color — stacked edges read as a rendering bug at these widths.",
    );
}
