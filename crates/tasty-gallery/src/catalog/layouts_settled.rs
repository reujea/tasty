//! Layouts의 결정 기록 Spec: 섞인 분할과 attention 척도.
//! 시안 `layouts.jsx`의 같은 Spec을 옮긴다.

use tasty_design_tokens::generated::semantic::{
    ATTENTION_RANK_COMPLETION, ATTENTION_RANK_NEEDS_INPUT,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::STRUCT_GAP_1;
use tasty_ui_widgets::{TagVariant, tag_mono};

use tasty_ui_widgets::surface_bed;

use crate::catalog::components::sidebar::paint_ws_count_badge_at;
use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};

/// 섞인 분할 무대의 높이와 폭.
const SPLIT_STAGE_H: LogicalPx = LogicalPx(240.0);
const SPLIT_STAGE_W: LogicalPx = LogicalPx(640.0);
/// 문서 표면의 주소 표시줄 높이.
const ADDRESS_BAR_H: LogicalPx = LogicalPx(36.0);

/// 척도 표의 열 폭: rank · kind · meaning · role · fill.
const SCALE_COLS: [LogicalPx; 5] = [
    LogicalPx(44.0),
    LogicalPx(120.0),
    LogicalPx(220.0),
    LogicalPx(150.0),
    LogicalPx(60.0),
];
/// fill 열의 색 견본 한 변.
const SCALE_SWATCH: LogicalPx = LogicalPx(18.0);
/// 예약된 kind 행의 흐림 정도.
const RESERVED_OPACITY: f32 = 0.55;
/// 척도 표의 행 높이.
const SCALE_ROW_H: LogicalPx = LogicalPx(32.0);
/// 예약된 Error·Approval rank. 시안에만 있는 값이며 토큰이 아직 없다.
const RANK_ERROR_RESERVED: u16 = 40;
const RANK_APPROVAL_RESERVED: u16 = 20;

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

/// 표면 머리줄: 상태 점, 이름, 오른쪽 surface id 태그.
fn pane_header(ui: &mut egui::Ui, theme: &Theme, dot: egui::Color32, name: &str, id: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let size = theme.status_dot_size.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), size * 0.5, dot);
        ui.label(
            egui::RichText::new(name)
                .size(theme.font_size_caption.value())
                .color(ec(theme.text_secondary())),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            tag_mono(ui, theme, id, TagVariant::Default, false);
        });
    });
}

fn pane_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    bg: egui::Color32,
    body: impl FnOnce(&mut egui::Ui),
) {
    let p = ui.painter();
    p.rect_filled(rect, theme.corner_radius.value(), bg);
    p.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), ec(theme.border_default())),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    // 스크롤 영역의 clip과 겹치는 부분만 남겨 갤러리 머리줄 위에 그리지 않는다.
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    body(&mut child);
}

/// 시안 `fakePane`. 터미널 표면 하나: 머리줄, 프롬프트, 포커스면 커서.
pub(crate) fn terminal_pane(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    label: &str,
    focused: bool,
    agent: bool,
) {
    let term = theme.surface("terminal");
    let bg = if focused {
        term.focused_bg
    } else {
        term.unfocused_bg
    };
    let dot = if agent {
        theme.accent_agent()
    } else if focused {
        theme.accent_success()
    } else {
        theme.text_muted()
    };
    let id = if focused { "s_01HX" } else { "s_02KQ" };
    pane_frame(ui, theme, rect, ec(bg), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::Frame::new()
            .fill(ec(theme.bg_panel()))
            .inner_margin(egui::Margin::symmetric(
                theme.spacing_md.value() as i8,
                theme.spacing_sm.value() as i8,
            ))
            .show(ui, |ui| {
                pane_header(ui, theme, ec(dot), label, id);
            });
        egui::Frame::new()
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                let font = egui::FontId::monospace(theme.font_size_term_sm.value());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    ui.label(
                        egui::RichText::new("~/tasty")
                            .font(font.clone())
                            .color(ec(theme.accent_success_raised())),
                    );
                    ui.label(
                        egui::RichText::new("main")
                            .font(font.clone())
                            .color(ec(theme.accent_primary())),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("❯")
                            .font(font.clone())
                            .color(ec(if agent {
                                theme.accent_agent_raised()
                            } else {
                                theme.accent_agent()
                            })),
                    );
                    if focused {
                        let (cur, _) = ui.allocate_exact_size(
                            egui::vec2(theme.status_dot_size.value(), theme.spacing_lg.value()),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(cur, 0.0, ec(term.focused_fg));
                    }
                });
            });
    });
}

/// 시안 `fakeDocPane`. Markdown 표면 하나: 포커스 바탕, 36px 주소 표시줄, 본문.
fn doc_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, focused: bool) {
    pane_frame(ui, theme, rect, surface_bed(theme, focused), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let (bar, _) = ui.allocate_exact_size(
            egui::vec2(rect.width(), ADDRESS_BAR_H.value()),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(bar, 0.0, ec(theme.bg_sidebar()));
        ui.painter().hline(
            bar.x_range(),
            bar.bottom(),
            egui::Stroke::new(theme.border_width.value(), ec(theme.border_default())),
        );
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(egui::vec2(theme.spacing_sm.value(), 0.0)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        tag_mono(&mut row, theme, "s_05MD", TagVariant::Default, false);
        let (field, _) = row.allocate_exact_size(
            egui::vec2(row.available_width(), theme.item_height_interactive.value()),
            egui::Sense::hover(),
        );
        let p = row.painter();
        p.rect_filled(field, theme.corner_radius.value(), ec(theme.input_bg()));
        p.rect_stroke(
            field,
            theme.corner_radius.value(),
            egui::Stroke::new(theme.border_width.value(), ec(theme.input_border())),
            egui::StrokeKind::Inside,
        );
        p.with_clip_rect(field.intersect(row.clip_rect())).text(
            egui::pos2(field.left() + theme.spacing_sm.value(), field.center().y),
            egui::Align2::LEFT_CENTER,
            "~/tasty/notes/split-layout.md",
            egui::FontId::monospace(theme.font_size_caption.value()),
            ec(theme.text_secondary()),
        );
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(
                theme.spacing_lg.value() as i8,
                theme.spacing_md.value() as i8,
            ))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                ui.label(
                    egui::RichText::new("Split layout")
                        .monospace()
                        .size(theme.font_size_prose_h1.value())
                        .color(ec(theme.text_primary())),
                );
                ui.label(
                    egui::RichText::new(
                        "A pane group holds surfaces of different kinds. Only one of them has focus.",
                    )
                    .size(theme.font_size_term_sm.value())
                    .color(ec(theme.text_secondary())),
                );
            });
    });
}

/// 무대 한 칸: 왼쪽 터미널과 오른쪽 Markdown. `doc_focused`가 어느 쪽에 포커스가 있는지 정한다.
fn split_row(ui: &mut egui::Ui, theme: &Theme, doc_focused: bool) {
    let (area, _) = ui.allocate_exact_size(
        egui::vec2(SPLIT_STAGE_W.value(), SPLIT_STAGE_H.value()),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(area, 0.0, ec(theme.bg_panel()));
    let inner = area.shrink(theme.spacing_md.value());
    let gap = theme.spacing_sm.value();
    let half = (inner.width() - gap) * 0.5;
    let left = egui::Rect::from_min_size(inner.min, egui::vec2(half, inner.height()));
    let right = egui::Rect::from_min_size(
        egui::pos2(left.right() + gap, inner.top()),
        egui::vec2(half, inner.height()),
    );
    ui.push_id(doc_focused, |ui| {
        terminal_pane(ui, theme, left, "you · zsh", !doc_focused, false);
        doc_pane(ui, theme, right, doc_focused);
    });
}

/// Layouts › Dividers & surfaces — 한 PaneGroup 안의 터미널과 Markdown.
pub fn draw_mixed_split(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        split_row(ui, theme, false);
    });
    stage(ui, theme, StageVariant::Tight, |ui| {
        cluster(ui, theme, "Markdown focused", |ui| {
            split_row(ui, theme, true)
        });
    });
    meta(
        ui,
        theme,
        &[
            ("group", "one tab, surfaces of different kinds"),
            ("focus", "the bed of each surface"),
            ("told apart by", "address band and prose, not by color"),
            ("markdown chrome", "36px address bar, kept in split"),
            ("prose measure", "max 620px"),
            ("status bar", "focused surface id"),
        ],
        &[
            TokenChip::new(
                "surface-bed-focused",
                "focused",
                ec(theme.surface_bed_focused()),
            ),
            TokenChip::new(
                "surface-bed-unfocused",
                "unfocused",
                ec(theme.surface_bed_unfocused()),
            ),
            TokenChip::without_color("separator", "surface border"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't give markdown a bed of its own to set it apart from a terminal. Two beds per state would make focus harder to read, and the address band already says which kind it is.",
    );
}

struct Kind {
    rank: f32,
    name: &'static str,
    what: &'static str,
    role: &'static str,
    fill: Option<(egui::Color32, egui::Color32)>,
}

fn scale_cell(ui: &mut egui::Ui, w: LogicalPx, body: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(w.value(), ui.available_height()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(w.value());
            body(ui);
        },
    );
}

/// Layouts › Occupancy — attention kind의 rank·색·빌린 역할을 한 표로 보인다.
pub fn draw_attention_scale(ui: &mut egui::Ui, theme: &Theme) {
    let kinds = [
        Kind {
            rank: f32::from(RANK_ERROR_RESERVED),
            name: "Error",
            what: "failed and unrecovered",
            role: "accent-danger",
            fill: None,
        },
        Kind {
            rank: ATTENTION_RANK_NEEDS_INPUT,
            name: "NeedsInput",
            what: "blocked — waiting on the human",
            role: "accent-warning",
            fill: Some((
                ec(theme.attention_needs_input()),
                ec(theme.badge_warning_bg()),
            )),
        },
        Kind {
            rank: f32::from(RANK_APPROVAL_RESERVED),
            name: "Approval",
            what: "an agent asks permission",
            role: "accent-agent",
            fill: None,
        },
        Kind {
            rank: ATTENTION_RANK_COMPLETION,
            name: "Completion",
            what: "task finished — clears on focus",
            role: "accent-primary",
            fill: Some((
                ec(theme.attention_completion()),
                ec(theme.badge_primary_bg()),
            )),
        },
    ];
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(ec(theme.bg_panel()))
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.spacing_md.value(), STRUCT_GAP_1.value());
                ui.horizontal(|ui| {
                    ui.add_space(theme.spacing_sm.value());
                    for (head, w) in ["RANK", "KIND", "MEANING", "ROLE BORROWED", "FILL"]
                        .into_iter()
                        .zip(SCALE_COLS)
                    {
                        scale_cell(ui, w, |ui| {
                            ui.label(
                                egui::RichText::new(head)
                                    .monospace()
                                    .size(theme.font_size_micro.value())
                                    .color(ec(theme.text_muted_raised())),
                            );
                        });
                    }
                });
                for k in &kinds {
                    let row_w = SCALE_COLS.iter().map(|w| w.value()).sum::<f32>()
                        + theme.spacing_md.value() * (SCALE_COLS.len() - 1) as f32
                        + theme.spacing_sm.value() * 2.0;
                    let (row, _) = ui.allocate_exact_size(
                        egui::vec2(row_w, SCALE_ROW_H.value()),
                        egui::Sense::hover(),
                    );
                    let r = theme.corner_radius_sm.value();
                    if k.fill.is_some() {
                        ui.painter().rect_filled(row, r, ec(theme.surface_raised()));
                    } else {
                        ui.painter().rect_stroke(
                            row,
                            r,
                            egui::Stroke::new(
                                theme.border_width.value(),
                                ec(theme.border_default()),
                            ),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(row.shrink2(egui::vec2(theme.spacing_sm.value(), 0.0)))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    if k.fill.is_none() {
                        child.multiply_opacity(RESERVED_OPACITY);
                    }
                    child.spacing_mut().item_spacing.x = theme.spacing_md.value();
                    scale_cell(&mut child, SCALE_COLS[0], |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}", k.rank))
                                .monospace()
                                .size(theme.font_size_term_sm.value())
                                .color(ec(theme.text_muted_raised())),
                        );
                    });
                    scale_cell(&mut child, SCALE_COLS[1], |ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                        ui.label(
                            egui::RichText::new(k.name)
                                .size(theme.font_size_body.value())
                                .color(ec(theme.text_primary())),
                        );
                        if k.fill.is_none() {
                            ui.label(
                                egui::RichText::new("· reserved")
                                    .size(theme.font_size_micro.value())
                                    .color(ec(theme.text_muted_raised())),
                            );
                        }
                    });
                    scale_cell(&mut child, SCALE_COLS[2], |ui| {
                        ui.label(
                            egui::RichText::new(k.what)
                                .size(theme.font_size_term_sm.value())
                                .color(ec(theme.text_secondary_raised())),
                        );
                    });
                    scale_cell(&mut child, SCALE_COLS[3], |ui| {
                        ui.label(
                            egui::RichText::new(k.role)
                                .monospace()
                                .size(theme.font_size_caption.value())
                                .color(ec(theme.text_secondary_raised())),
                        );
                    });
                    scale_cell(&mut child, SCALE_COLS[4], |ui| {
                        let sw = SCALE_SWATCH.value();
                        let (cell, _) = ui.allocate_exact_size(
                            egui::vec2(SCALE_COLS[4].value(), sw),
                            egui::Sense::hover(),
                        );
                        let swatch = egui::Rect::from_min_size(
                            egui::pos2(cell.left(), cell.center().y - sw * 0.5),
                            egui::vec2(sw, sw),
                        );
                        let p = ui.painter();
                        match k.fill {
                            Some((fill, badge_fill)) => {
                                p.rect_filled(swatch, r, fill);
                                let badge_row = egui::Rect::from_min_max(
                                    egui::pos2(swatch.right(), cell.top()),
                                    egui::pos2(cell.right(), cell.bottom()),
                                );
                                paint_ws_count_badge_at(p, theme, badge_row, 0.0, "3", badge_fill);
                            }
                            None => {
                                p.rect_stroke(
                                    swatch,
                                    r,
                                    egui::Stroke::new(
                                        theme.border_width.value(),
                                        ec(theme.border_strong()),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                    });
                }
            });
    });
    meta(
        ui,
        theme,
        &[
            ("kinds (live)", "needs-input · completion"),
            ("reserved", "error (rank 40) · approval (rank 20)"),
            (
                "rank spacing",
                "10 — a future kind slots between without renumbering",
            ),
            (
                "collision rule",
                "highest rank takes the single visual slot",
            ),
            (
                "contrast",
                "fill × on-accent fg — both live kinds clear 4.5:1",
            ),
        ],
        &[
            TokenChip::new(
                "attention-needs-input",
                "blocked, waiting on the human (→ yellow)",
                ec(theme.attention_needs_input()),
            ),
            TokenChip::new(
                "attention-completion",
                "task finished, FYI (→ blue)",
                ec(theme.attention_completion()),
            ),
            TokenChip::new(
                "attention-needs-input-fg",
                "glyph atop the yellow fill",
                ec(theme.attention_needs_input_fg()),
            ),
            TokenChip::new(
                "attention-completion-fg",
                "glyph atop the blue fill",
                ec(theme.attention_completion_fg()),
            ),
            TokenChip::without_color("attention-rank-needs-input", "30"),
            TokenChip::without_color("attention-rank-completion", "10"),
        ],
    );
    note(
        ui,
        theme,
        "NeedsInput outranks everything because it is the only kind that is a request — work has stopped until the human answers. Completion is an FYI and clears on focus. No attention kind mints a new hue; a kind that can't honestly borrow an existing accent role is a sign the role list is missing something.",
    );
}
