//! Components의 결정 기록 Spec: 키캡 하나, 글리프 크기, 점 가족.
//! 시안 `components.jsx`의 같은 Spec을 옮긴다. 폐기된 값은 비교용으로 그대로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{KbdKey, Spinner, kbd, kbd_parts_width};

use crate::catalog::icons;
use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage};
use crate::catalog::toast_card::{self, ToastKind};

/// 키캡 비교 패널의 폭.
const KEYCAP_PANEL_W: LogicalPx = LogicalPx(380.0);
/// 팔레트가 따로 그리던 키캡의 값. 폐기된 값이며 비교 행에서만 쓴다.
const DROPPED_CAP_SIZE: LogicalPx = LogicalPx(18.0);
const DROPPED_CAP_PAD_X: LogicalPx = LogicalPx(5.0);
const DROPPED_CAP_GAP: LogicalPx = LogicalPx(4.0);

/// 글리프 예제 칸의 높이. 28px 글리프까지 같은 줄에 세운다.
const GLYPH_SLOT_H: LogicalPx = LogicalPx(34.0);
/// clipboard 이미지 글리프. 이미 있던 콘텐츠 글리프 예외 크기다.
const CLIPBOARD_GLYPH: LogicalPx = LogicalPx(28.0);

/// 점 예제 칸: 24px 크롬 줄 안의 40px 칸.
const DOT_CELL_W: LogicalPx = LogicalPx(40.0);
/// 활성 탭 표시. 다른 역할이라 점 가족 밖의 4px이다.
const TAB_MARKER: LogicalPx = LogicalPx(4.0);
/// 배율 비교 막대의 폭.
const DOT_BAR_W: LogicalPx = LogicalPx(150.0);
/// 토스트 간격 비교: 현재 제품 값 6과 정한 값 8.
const TOAST_GAP_BEFORE: LogicalPx = LogicalPx(6.0);

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

fn caption(ui: &mut egui::Ui, theme: &Theme, s: &str, color: egui::Color32) {
    ui.label(
        egui::RichText::new(s)
            .size(theme.font_size_caption.value())
            .color(color),
    );
}

fn mono_micro(ui: &mut egui::Ui, theme: &Theme, s: &str, color: egui::Color32) {
    ui.label(
        egui::RichText::new(s)
            .monospace()
            .size(theme.font_size_micro.value())
            .color(color),
    );
}

/// 폐기된 팔레트 키캡 한 벌의 폭. 그리기와 같은 식으로 계산한다.
fn dropped_caps_width(ui: &egui::Ui, theme: &Theme, keys: &str) -> f32 {
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let caps: Vec<f32> = keys
        .split('+')
        .map(|key| {
            let w = ui.fonts(|f| {
                f.layout_no_wrap(key.to_string(), font.clone(), ec(theme.kbd_fg()))
                    .size()
                    .x
            });
            (w + DROPPED_CAP_PAD_X.value() * 2.0).max(DROPPED_CAP_SIZE.value())
        })
        .collect();
    caps.iter().sum::<f32>() + DROPPED_CAP_GAP.value() * caps.len().saturating_sub(1) as f32
}

/// 폐기된 팔레트 키캡 한 벌. Kbd와 같은 색 토큰을 쓰고 치수만 예전 값이다.
fn dropped_caps(ui: &mut egui::Ui, theme: &Theme, keys: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = DROPPED_CAP_GAP.value();
        for key in keys.split('+') {
            let font = egui::FontId::monospace(theme.font_size_caption.value());
            let galley = ui
                .painter()
                .layout_no_wrap(key.to_string(), font, ec(theme.kbd_fg()));
            let w =
                (galley.size().x + DROPPED_CAP_PAD_X.value() * 2.0).max(DROPPED_CAP_SIZE.value());
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(w, DROPPED_CAP_SIZE.value()),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            let r = theme.corner_radius_sm.value();
            let depth = theme.kbd_shadow_depth().value();
            p.rect_filled(
                rect.translate(egui::vec2(0.0, depth)),
                r,
                ec(theme.kbd_border()),
            );
            p.rect_filled(rect, r, ec(theme.kbd_bg()));
            p.rect_stroke(
                rect,
                r,
                egui::Stroke::new(theme.border_width.value(), ec(theme.kbd_border())),
                egui::StrokeKind::Inside,
            );
            p.galley(
                rect.center() - galley.size() * 0.5,
                galley,
                ec(theme.kbd_fg()),
            );
        }
    });
}

/// 팔레트 행 두 개를 담은 패널. `canonical`이면 공용 Kbd를 쓴다.
fn keycap_panel(ui: &mut egui::Ui, theme: &Theme, label: &str, canonical: bool) {
    ui.vertical(|ui| {
        ui.set_width(KEYCAP_PANEL_W.value());
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        let color = if canonical {
            theme.accent_success()
        } else {
            theme.text_muted_raised()
        };
        caption(ui, theme, label, ec(color));
        egui::Frame::new()
            .fill(ec(theme.surface_raised()))
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                ec(theme.border_strong()),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (cmd, keys) in [
                    ("Split pane right", "Ctrl+Shift+D"),
                    ("Open file…", "Ctrl+P"),
                ] {
                    let (row, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), theme.item_height_interactive.value()),
                        egui::Sense::hover(),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(row.shrink2(egui::vec2(theme.spacing_sm.value(), 0.0)))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    child.label(
                        egui::RichText::new(cmd)
                            .size(theme.font_size_body.value())
                            .color(ec(theme.text_secondary_raised())),
                    );
                    // 오른쪽 정렬 레이아웃은 키 순서까지 뒤집으므로 폭을 먼저 재고 왼쪽부터 그린다.
                    let keys_w = if canonical {
                        let parts: Vec<KbdKey<'_>> = keys.split('+').map(KbdKey::Text).collect();
                        kbd_parts_width(child.ctx(), theme, &parts).value()
                    } else {
                        dropped_caps_width(&child, theme, keys)
                    };
                    child.add_space((child.available_width() - keys_w).max(0.0));
                    if canonical {
                        kbd(&mut child, theme, keys, tasty_ui_widgets::Ground::Panel);
                    } else {
                        dropped_caps(&mut child, theme, keys);
                    }
                }
            });
    });
}

/// Components › Kbd — 팔레트 전용 키캡을 없애고 Kbd 하나로 맞춘다.
pub fn draw_one_keycap(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        keycap_panel(ui, theme, "settled — Kbd (16 / 4 / 4 / 10)", true);
        keycap_panel(
            ui,
            theme,
            "dropped — palette-only cap (18 / 5 / 4 / 11)",
            false,
        );
    });
    meta(
        ui,
        theme,
        &[
            ("min side", "16 — kbd-size (was 18)"),
            ("padding-x", "4 — kbd-padding-x (was 5)"),
            ("gap", "3 — kbd-gap (was 4)"),
            ("font", "10 — kbd-font-size (was 11)"),
            ("bottom edge", "unchanged — kbd-shadow-depth"),
            ("row", "28px palette row — unchanged, 6px air per side"),
            ("new tokens", "none"),
        ],
        &[
            TokenChip::without_color("kbd-size", "cap min side"),
            TokenChip::without_color("kbd-padding-x", "cap side padding"),
            TokenChip::without_color("kbd-gap", "between caps"),
            TokenChip::without_color("kbd-font-size", "cap label"),
        ],
    );
    note(
        ui,
        theme,
        "The switch-number overlay keycap already reads the Kbd tokens, so the palette was the last divergent cap. One keycap in the system now.",
    );
}

/// Components › Badge · Tag · Kbd — 글자 크기에 있던 글리프 값을 아이콘 크기로 옮긴다.
pub fn draw_glyph_sizes(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        let ink = ec(theme.text_secondary());
        let slots: [(&str, &str, LogicalPx); 3] = [
            ("toggle check", "icon-size-xs", theme.icon_glyph_size_xs),
            ("spinner", "icon-size-md", theme.icon_glyph_size_md),
            ("clipboard glyph", "28px", CLIPBOARD_GLYPH),
        ];
        for (i, (label, size_name, size)) in slots.into_iter().enumerate() {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                let (slot, _) = ui.allocate_exact_size(
                    egui::vec2(CLIPBOARD_GLYPH.value(), GLYPH_SLOT_H.value()),
                    egui::Sense::hover(),
                );
                let rect = egui::Rect::from_center_size(
                    slot.center(),
                    egui::vec2(size.value(), size.value()),
                );
                match i {
                    0 => icons::CHECK.image(size.value(), ink).paint_at(ui, rect),
                    1 => {
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        Spinner::new().size(size.value()).show(&mut child, theme);
                    }
                    _ => icons::CLIPBOARD.image(size.value(), ink).paint_at(ui, rect),
                }
                mono_micro(
                    ui,
                    theme,
                    &format!("{label} · {size_name}"),
                    ec(theme.text_muted()),
                );
            });
        }
    });
    meta(
        ui,
        theme,
        &[
            ("toggle check", "12 → icon-size-xs"),
            ("spinner default", "16 → icon-size-md"),
            (
                "switch-overlay digit",
                "16 — stays on kbd-size (a keycap, not an icon)",
            ),
            (
                "clipboard glyph",
                "30 → 28 (the existing content-glyph exception)",
            ),
            (
                "central glyph 22 / empty glyph 26",
                "unchanged named exceptions",
            ),
            ("new font tokens", "none"),
        ],
        &[
            TokenChip::without_color("icon-size-xs", "12 — inline glyphs"),
            TokenChip::without_color("icon-size-md", "16 — toolbar / spinner"),
            TokenChip::without_color("spinner-size", "→ icon-size-md"),
        ],
    );
    note(
        ui,
        theme,
        "Font sizes and glyph sizes are separate families. A 16px glyph next to 13px text is not a type-scale violation; it is an icon at its own size.",
    );
}

/// 24px 크롬 칸 안의 점. `ring`이면 attached ring을 offset 뒤에 그린다.
fn paint_dot(
    p: &egui::Painter,
    theme: &Theme,
    center: egui::Pos2,
    size: f32,
    ring: Option<(f32, f32)>,
) {
    let r = size * 0.5;
    if let Some((offset, width)) = ring {
        p.circle_stroke(
            center,
            r + offset + width * 0.5,
            egui::Stroke::new(width, ec(theme.status_dot_attached_ring())),
        );
    }
    p.circle_filled(center, r, ec(theme.status_dot_success()));
}

/// Components › StatusDot — 점은 일반 8과 밀집 크롬 6 두 크기, 그리고 attached ring.
pub fn draw_dot_family(ui: &mut egui::Ui, theme: &Theme) {
    let chrome_h = theme.item_height_tab.value();
    let ring = (
        theme.status_dot_attached_ring_offset().value(),
        theme.status_dot_attached_ring_width().value(),
    );
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
            let cells: [(&str, LogicalPx, bool); 4] = [
                ("generic · 8", theme.status_dot_size, false),
                ("dense chrome · 6", theme.status_dot_size_compact(), false),
                (
                    "attached · 6 + ring 2/2",
                    theme.status_dot_size_compact(),
                    true,
                ),
                ("tab marker · 4 (other role)", TAB_MARKER, false),
            ];
            for (label, size, with_ring) in cells {
                let font = egui::FontId::monospace(theme.font_size_micro.value());
                let label_w = ui
                    .painter()
                    .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER)
                    .rect
                    .width();
                let w = label_w.max(DOT_CELL_W.value());
                // vertical_centered는 남은 폭 전체를 차지하므로 열 폭을 먼저 정한다.
                ui.allocate_ui_with_layout(
                    egui::vec2(w, 0.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.set_width(w);
                        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                        let (cell, _) = ui.allocate_exact_size(
                            egui::vec2(DOT_CELL_W.value(), chrome_h),
                            egui::Sense::hover(),
                        );
                        let p = ui.painter();
                        p.rect_filled(cell, theme.corner_radius_sm.value(), ec(theme.bg_sidebar()));
                        paint_dot(
                            p,
                            theme,
                            cell.center(),
                            size.value(),
                            with_ring.then_some(ring),
                        );
                        mono_micro(ui, theme, label, ec(theme.text_muted()));
                    },
                );
            }
        });
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            for (label, zoom) in [("ui_scale 0.85", 0.85_f32), ("1", 1.0), ("1.2", 1.2)] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                    let compact = theme.status_dot_size_compact().value();
                    let bbox = (compact + (ring.0 + ring.1) * 2.0) * zoom;
                    mono_micro(
                        ui,
                        theme,
                        &format!("{label} — bbox {bbox:.1} in a 24 bar"),
                        ec(theme.text_muted()),
                    );
                    let (bar, _) = ui.allocate_exact_size(
                        egui::vec2(DOT_BAR_W.value(), chrome_h),
                        egui::Sense::hover(),
                    );
                    let p = ui.painter();
                    p.rect_filled(bar, theme.corner_radius_sm.value(), ec(theme.bg_sidebar()));
                    let size = compact * zoom;
                    let outer = (ring.0 + ring.1) * zoom;
                    let center = egui::pos2(
                        bar.left() + theme.spacing_sm.value() + size * 0.5 + outer,
                        bar.center().y,
                    );
                    paint_dot(p, theme, center, size, Some((ring.0 * zoom, ring.1 * zoom)));
                    p.text(
                        egui::pos2(
                            center.x + size * 0.5 + outer + theme.spacing_sm.value(),
                            bar.center().y,
                        ),
                        egui::Align2::LEFT_CENTER,
                        "attached",
                        egui::FontId::proportional(theme.font_size_caption.value()),
                        ec(theme.text_muted()),
                    );
                });
            }
        });
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            for (label, gap, settled) in [
                ("toast gap 6 — the product today", TOAST_GAP_BEFORE, false),
                ("toast gap 8 — settled", theme.toast_gap(), true),
            ] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                    let color = if settled {
                        theme.accent_success()
                    } else {
                        theme.text_muted()
                    };
                    mono_micro(ui, theme, label, ec(color));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = gap.value();
                        toast_card::draw_single_card(
                            ui,
                            theme,
                            ToastKind::Success,
                            "Keybindings exported.",
                            &[],
                            1.0,
                        );
                        toast_card::draw_single_card(
                            ui,
                            theme,
                            ToastKind::Info,
                            "Two notices while importing the bundle — open Import / Export to read them.",
                            &[],
                            1.0,
                        );
                    });
                });
            }
        });
    });
    meta(
        ui,
        theme,
        &[
            ("generic dot", "8 — status-dot-size (unchanged)"),
            ("dense chrome", "6 — status-dot-size-compact (new)"),
            ("tab busy dot", "6 — keeps its value, now its own role"),
            ("status bar dot", "7 → 6 (visible change)"),
            ("sidebar rail dot", "6 — unchanged, now tokenised"),
            ("active tab marker", "4 — different role, untouched"),
            (
                "attached ring",
                "width 2 / offset 2 (1.5 → 2, visible change)",
            ),
            (
                "attached bbox",
                "compact 6 → 14 · generic 8 → 16 (workspace row, the only ring in product)",
            ),
            ("offset measured", "dot outer edge → ring inner edge"),
            ("toast gap", "6 → 8 (visible change, 4px grid)"),
            ("badge / tag dots", "unchanged at 8"),
        ],
        &[
            TokenChip::without_color("status-dot-size-compact", "tab · status bar · rail"),
            TokenChip::without_color("tab-dot-size", "→ compact"),
            TokenChip::without_color("statusbar-dot-size", "→ compact"),
            TokenChip::without_color("status-dot-attached-ring-width", "2"),
            TokenChip::without_color("status-dot-attached-ring-offset", "2"),
            TokenChip::without_color("toast-gap", "8, unchanged token"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't push the whole family to one number. A dot in a 24px strip and a dot on a 36px list row are the same meaning at two densities; collapsing them either crowds the strip or shrinks every badge.",
    );
    note(
        ui,
        theme,
        "Unchanged by this decision: every badge, tag and status consumer of the generic 8; the 4px active marker; the dot colours and their meanings (idle / busy / attached / needs-input / completion).",
    );
}
