//! 접힌 레일의 표지 예제. 카테고리 선과 mirror 칩을 Mocha·Latte 두 테마에서 상태별로 비교한다.
//! 치수와 토큰은 본체 접힌 레일의 카테고리 버튼·워크스페이스 아바타와 같다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    LoadingPlacement, RailDot, num_keycap, paint_attach_refusal_chip, paint_dashed_outline,
    paint_loading_overlay, paint_move_source_chip, paint_num_keycap, paint_rail_category_line,
    paint_rail_dot, paint_rail_flat_divider, paint_workspace_mirror_chip,
    paint_workspace_mirror_chip_offline, rail_flat_divider_size,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 카테고리 버튼 바탕 상태. 본체는 pressed를 hover보다 우선한다.
#[derive(Clone, Copy, PartialEq)]
enum Bed {
    Idle,
    Hover,
    Pressed,
}

/// 예제 아바타의 고정 상태.
#[derive(Clone, Copy)]
pub(crate) struct Avatar {
    pub(crate) letter: &'static str,
    pub(crate) active: bool,
    pub(crate) hover: bool,
    pub(crate) notif: bool,
    pub(crate) ring: bool,
    pub(crate) mirror: bool,
    /// mirror 칩을 연결되지 않은 저장 원격 행의 꼴(border-control 테두리, glyph-dim 글리프)로 그린다.
    pub(crate) offline: bool,
    /// 왼쪽 아래 move 칩.
    pub(crate) mv: bool,
    /// 왼쪽 위 attach 거절 칩.
    pub(crate) refused: bool,
    /// 이니셜 둘레의 attach 응답 대기 링.
    pub(crate) loading: bool,
    /// 전환 키캡 숫자. 이니셜 대신 그리고 모든 표지 위에 둔다.
    pub(crate) keycap: Option<&'static str>,
    /// 오버레이 rect(아바타 36)를 점선으로 보인다.
    pub(crate) boxes: bool,
}

/// 레일 한 줄에 놓이는 표지.
#[derive(Clone, Copy)]
enum RailItem {
    Line(Bed),
    Avatar(Avatar),
}

impl Avatar {
    /// 표시를 모두 끈 아바타. 상수 예제의 나머지 필드를 채운다.
    pub(crate) const NONE: Self = Self {
        letter: "",
        active: false,
        hover: false,
        notif: false,
        ring: false,
        mirror: false,
        offline: false,
        mv: false,
        refused: false,
        loading: false,
        keycap: None,
        boxes: false,
    };
}

/// 아바타 크기. 본체 `collapsed_ws_size`와 같다.
pub(crate) fn avatar_size(t: &Theme) -> egui::Vec2 {
    egui::vec2(
        t.sidebar_collapsed_slot_width.value(),
        t.sidebar_collapsed_workspace_height.value(),
    )
}

/// 카테고리 버튼 크기.
fn button_size(t: &Theme) -> egui::Vec2 {
    egui::vec2(t.sidebar_collapsed_slot_width.value(), t.spacing_lg.value())
}

fn paint_category_button(
    ui: &mut egui::Ui,
    t: &Theme,
    rect: egui::Rect,
    bed: Bed,
    cap: Option<&str>,
) {
    let radius = t.corner_radius_sm.value();
    match bed {
        Bed::Pressed => {
            ui.painter()
                .rect_filled(rect, radius, t.active_overlay.to_egui_premultiplied());
        }
        Bed::Hover => {
            ui.painter()
                .rect_filled(rect, radius, t.hover_overlay.to_egui_premultiplied());
        }
        Bed::Idle => {}
    }
    if let Some(digit) = cap {
        let size = t.switch_overlay_size().value();
        let cap_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(size, size));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cap_rect));
        num_keycap(&mut child, t, digit, false);
        return;
    }
    paint_rail_category_line(ui.painter(), t, rect.center(), bed != Bed::Idle);
}

/// 카테고리를 끈 레일의 맨 위 구분선.
fn paint_flat_divider(ui: &mut egui::Ui, t: &Theme, center: egui::Pos2) {
    let rect = egui::Rect::from_center_size(center, rail_flat_divider_size(t));
    paint_rail_flat_divider(ui.painter(), t, rect);
}

pub(crate) fn paint_avatar(ui: &mut egui::Ui, t: &Theme, rect: egui::Rect, a: Avatar) {
    let radius = t.corner_radius.value();
    if a.active {
        ui.painter()
            .rect_filled(rect, radius, t.overlay_active().to_egui_premultiplied());
    }
    if a.hover {
        ui.painter()
            .rect_filled(rect, radius, t.hover_overlay.to_egui_premultiplied());
    }
    if a.keycap.is_none() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            a.letter,
            egui::FontId::monospace(t.font_size_body.value()),
            egui::Color32::from(if a.active {
                t.accent_primary()
            } else {
                t.text_muted()
            }),
        );
    }
    // 순서: 이니셜 < 스피너 링 < 상태 점·attached ring·모서리 칩 < 전환 키캡.
    if a.loading {
        paint_loading_overlay(
            ui,
            t,
            rect,
            LoadingPlacement::Rail,
            egui::Color32::TRANSPARENT,
        );
        if a.boxes {
            paint_dashed_outline(
                ui.painter(),
                t,
                rect,
                0.0,
                egui::Color32::from(t.accent_move()),
            );
        }
    }
    if a.notif {
        paint_rail_dot(ui.painter(), t, rect, RailDot::Completion);
    }
    if a.ring {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                t.status_dot_attached_ring_width().value(),
                egui::Color32::from(t.border_attached()),
            ),
            egui::StrokeKind::Inside,
        );
    }
    let bed = egui::Color32::from(t.bg_sidebar());
    if a.mv {
        paint_move_source_chip(ui, t, rect, bed);
    }
    if a.refused {
        paint_attach_refusal_chip(ui, t, rect, bed);
    }
    if a.mirror {
        if a.offline {
            paint_workspace_mirror_chip_offline(ui, t, rect);
        } else {
            paint_workspace_mirror_chip(ui, t, rect);
        }
    }
    if let Some(digit) = a.keycap {
        paint_num_keycap(ui.painter(), t, rect.center(), digit, a.active, 1.0);
    }
}

const RAIL: &[RailItem] = &[
    RailItem::Line(Bed::Idle),
    RailItem::Avatar(Avatar {
        letter: "T",
        active: true,
        hover: false,
        notif: false,
        ring: false,
        mirror: false,
        ..Avatar::NONE
    }),
    RailItem::Avatar(Avatar {
        letter: "S",
        active: false,
        hover: false,
        notif: false,
        ring: false,
        mirror: false,
        ..Avatar::NONE
    }),
    RailItem::Line(Bed::Hover),
    RailItem::Avatar(Avatar {
        letter: "E",
        active: false,
        hover: false,
        notif: false,
        ring: false,
        mirror: true,
        ..Avatar::NONE
    }),
    RailItem::Avatar(Avatar {
        letter: "D",
        active: false,
        hover: false,
        notif: true,
        ring: false,
        mirror: false,
        ..Avatar::NONE
    }),
    RailItem::Line(Bed::Idle),
    RailItem::Line(Bed::Idle),
];

/// 카테고리 경계 셋과 아바타 넷을 쌓은 접힌 레일 조각.
fn mini_rail(ui: &mut egui::Ui, t: &Theme) {
    let gap = t.spacing_xs.value();
    let pad = t.spacing_sm.value();
    let w = t.sidebar_collapsed_slot_width.value() + t.spacing_lg.value() + t.spacing_xs.value();
    let h = RAIL
        .iter()
        .map(|item| match item {
            RailItem::Line(_) => button_size(t).y,
            RailItem::Avatar(_) => avatar_size(t).y,
        })
        .sum::<f32>()
        + gap * (RAIL.len() as f32 - 1.0)
        + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let bw = t.border_width.value();
    ui.painter().vline(
        rect.max.x - bw * 0.5,
        rect.y_range(),
        egui::Stroke::new(bw, t.separator.to_egui_premultiplied()),
    );
    let cx = rect.center().x;
    let mut y = rect.min.y + pad;
    for item in RAIL {
        match *item {
            RailItem::Line(bed) => {
                let s = button_size(t);
                let r = egui::Rect::from_min_size(egui::pos2(cx - s.x * 0.5, y), s);
                paint_category_button(ui, t, r, bed, None);
                y += s.y + gap;
            }
            RailItem::Avatar(a) => {
                let s = avatar_size(t);
                let r = egui::Rect::from_min_size(egui::pos2(cx - s.x * 0.5, y), s);
                paint_avatar(ui, t, r, a);
                y += s.y + gap;
            }
        }
    }
}

/// 표본 한 칸. 위에 표지, 아래에 상태 이름을 둔다.
fn cell(
    ui: &mut egui::Ui,
    t: &Theme,
    caption: &str,
    paint: impl FnOnce(&mut egui::Ui, egui::Rect),
) {
    let caption = egui::RichText::new(caption)
        .monospace()
        .size(t.font_size_micro.value())
        .color(t.text_muted().to_egui());
    // 칸은 최소 64이고, 상태 이름이 더 길면 이름 폭을 따른다.
    let caption_w = egui::WidgetText::from(caption.clone())
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        )
        .size()
        .x;
    let col_w = (t.spacing_xl.value() * 2.0 + t.spacing_lg.value()).max(caption_w);
    let mark_h = t.spacing_xl.value() + t.spacing_md.value();
    ui.allocate_ui_with_layout(
        egui::vec2(col_w, 0.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.y = t.spacing_sm.value();
            let (slot, _) = ui.allocate_exact_size(egui::vec2(col_w, mark_h), egui::Sense::hover());
            paint(ui, slot);
            ui.label(caption);
        },
    );
}

fn heading(ui: &mut egui::Ui, t: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(t.font_size_caption.value())
            .color(t.text_secondary().to_egui()),
    );
}

fn centered(slot: egui::Rect, size: egui::Vec2) -> egui::Rect {
    egui::Rect::from_center_size(slot.center(), size)
}

fn board(ui: &mut egui::Ui, t: &Theme, name: &str) {
    egui::Frame::new()
        .fill(t.bg_sidebar().to_egui())
        .stroke(egui::Stroke::new(
            t.border_width.value(),
            t.border_default().to_egui(),
        ))
        .corner_radius(t.corner_radius.value())
        .inner_margin(egui::Margin::same(t.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = t.spacing_lg.value();
                mini_rail(ui, t);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = t.spacing_lg.value();
                    heading(ui, t, &format!("{name} — category line"));
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = t.spacing_md.value();
                        let bs = button_size(t);
                        for (caption, bed) in [
                            ("idle", Bed::Idle),
                            ("hover", Bed::Hover),
                            ("pressed", Bed::Pressed),
                        ] {
                            cell(ui, t, caption, |ui, slot| {
                                paint_category_button(ui, t, centered(slot, bs), bed, None)
                            });
                        }
                        cell(ui, t, "modifier held", |ui, slot| {
                            paint_category_button(ui, t, centered(slot, bs), Bed::Idle, Some("2"))
                        });
                        cell(ui, t, "flat divider", |ui, slot| {
                            paint_flat_divider(ui, t, slot.center())
                        });
                    });
                    heading(ui, t, &format!("{name} — mirror chip"));
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = t.spacing_md.value();
                        let base = Avatar {
                            letter: "E",
                            mirror: true,
                            ..Avatar::NONE
                        };
                        for (caption, a) in [
                            ("idle", base),
                            (
                                "active",
                                Avatar {
                                    letter: "T",
                                    active: true,
                                    ..base
                                },
                            ),
                            (
                                "hover",
                                Avatar {
                                    hover: true,
                                    ..base
                                },
                            ),
                            (
                                "+ notif",
                                Avatar {
                                    notif: true,
                                    ..base
                                },
                            ),
                            ("+ attached ring", Avatar { ring: true, ..base }),
                        ] {
                            cell(ui, t, caption, |ui, slot| {
                                paint_avatar(ui, t, centered(slot, avatar_size(t)), a)
                            });
                        }
                    });
                });
            });
        });
}

/// 시안 "Rail marks — category line and mirror chip": 같은 보드를 Mocha·Latte로 그린다.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = tasty_themes::mocha_fallback();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                    board(ui, &mocha, "Mocha");
                    board(ui, &latte, "Latte");
                });
            });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "line",
                "24 × 2 · round ends · centred in the 32 × 16 button · bed radius-sm",
            ),
            ("line idle", "rail-category-line → text-muted"),
            (
                "line hover / pressed",
                "rail-category-line-hover → text-secondary · bed overlay-hover / overlay-active",
            ),
            ("active category", "same colour (no accent)"),
            ("modifier held", "switch keycap replaces the line"),
            (
                "flat divider",
                "28 × 2 · rail-category-line · above the first avatar",
            ),
            (
                "chip",
                "12 circle · 1px edge · 8 glyph · 2px bg-sidebar halo · bottom-right",
            ),
            (
                "chip edge",
                "workspace-mirror-chip-border → workspace-mirror-fg",
            ),
            (
                "avatar",
                "32 × 28 rail slot (the design board draws 36 × 36)",
            ),
        ],
        &[
            TokenChip::without_color("rail-category-line-length", "line 24"),
            TokenChip::without_color("rail-flat-divider-length", "flat top divider 28"),
            TokenChip::without_color("workspace-mirror-chip-size", "mirror chip 12"),
            TokenChip::new(
                "rail-category-line",
                "idle line · flat divider",
                theme.rail_category_line().into(),
            ),
            TokenChip::new(
                "rail-category-line-hover",
                "hover / pressed line",
                theme.rail_category_line_hover().into(),
            ),
            TokenChip::new(
                "workspace-mirror-chip-border",
                "chip edge",
                theme.workspace_mirror_chip_border().into(),
            ),
            TokenChip::new(
                "workspace-mirror-chip-bg",
                "chip bed",
                theme.workspace_mirror_chip_bg().into(),
            ),
        ],
    );
}
