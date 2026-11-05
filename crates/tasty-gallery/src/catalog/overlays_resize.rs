//! Overlays › Move & resize의 크기 조절 Spec: 8방향 손잡이 지도와 커서, 경계·겹침 규칙.
//! 시안 `overlays-windows.jsx`의 `ResizeMap`·`CursorMatrix`·`ClampDemo`·`ZOrderDemo`를 옮긴다.
//! 예제 상자와 미니 팝업의 크기는 시안의 전시용 치수다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{ControlSize, IconButton, IconButtonVariant};

use crate::catalog::icons;
use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage, wrap_item};

/// 손잡이 지도 상자와 그 안의 띠·모서리 크기(시안 `band = 12, corner = 16`).
const MAP_W: LogicalPx = LogicalPx(280.0);
const MAP_H: LogicalPx = LogicalPx(180.0);
const MAP_BAND: LogicalPx = LogicalPx(12.0);
const MAP_CORNER: LogicalPx = LogicalPx(16.0);
/// 손잡이 이름(N·NW…)과 scope rect 표시의 글자 크기. 시안 리터럴 9 이다.
const DEMO_LABEL_SIZE: LogicalPx = LogicalPx(9.0);
/// 손잡이 hover 칠의 accent-info 비율(시안 `color-mix 14%`).
const ZONE_HOVER_MIX: f32 = 0.14;

/// 커서 표의 폭, 행 안쪽 여백(5 · 8), 행 사이 간격 1, 글리프 칸 26, 칸 사이 10.
const MATRIX_W: LogicalPx = LogicalPx(320.0);
const MATRIX_ROW_PAD_Y: LogicalPx = LogicalPx(5.0);
const MATRIX_ROW_GAP: LogicalPx = LogicalPx(1.0);
const MATRIX_GLYPH_COL: LogicalPx = LogicalPx(26.0);
const MATRIX_COL_GAP: LogicalPx = LogicalPx(10.0);

/// scope 경계 예제와 겹침 예제의 상자.
const CLAMP_W: LogicalPx = LogicalPx(300.0);
const CLAMP_H: LogicalPx = LogicalPx(200.0);
const ZORDER_W: LogicalPx = LogicalPx(320.0);
const ZORDER_H: LogicalPx = LogicalPx(200.0);
/// 예제 안 미니 팝업의 폭과 본문 높이: 경계 예제, 겹침 예제의 뒤·앞 팝업.
const CLAMP_POPUP: (LogicalPx, LogicalPx) = (LogicalPx(188.0), LogicalPx(70.0));
const BEHIND_POPUP: (LogicalPx, LogicalPx) = (LogicalPx(190.0), LogicalPx(66.0));
const FRONT_POPUP: (LogicalPx, LogicalPx) = (LogicalPx(200.0), LogicalPx(70.0));
/// 미니 팝업 본문 안쪽 여백 10 과 줄 높이 배율 1.7.
const POPUP_BODY_PAD: LogicalPx = LogicalPx(10.0);
const POPUP_LINE_HEIGHT_FACTOR: f32 = 1.7;
/// 겹침 예제에서 뒤 팝업의 불투명도(시안 0.55).
const BEHIND_OPACITY: f32 = 0.55;
/// 예제 사이 간격(시안 stage gap 28)과 캡션 아래 간격 6.
const DEMO_GAP: LogicalPx = LogicalPx(28.0);
const CAPTION_GAP: LogicalPx = LogicalPx(6.0);

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

/// 시안 `FauxPopup`: 제목 줄(bg-app, 가운데 제목, 오른쪽 닫기) + mono 본문. `rect` 를 채운다.
/// `grab` 이면 닫기 버튼을 뺀 제목 줄에서 grab 커서를 보인다.
fn faux_popup(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    title: &str,
    lines: &[(&str, bool)],
    grab: bool,
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let bw = theme.border_width.value();
    let radius = theme.corner_radius.value();
    let p = child.painter().clone();
    p.add(theme.shadow_modal().to_egui().as_shape(rect, radius));
    p.rect(
        rect,
        radius,
        ec(theme.surface_raised()),
        egui::Stroke::new(bw, ec(theme.border_strong())),
        egui::StrokeKind::Inside,
    );
    let bar = egui::Rect::from_min_size(
        rect.min + egui::vec2(bw, bw),
        egui::vec2(rect.width() - bw * 2.0, theme.titlebar_height.value()),
    );
    p.rect_filled(bar, 0.0, ec(theme.bg_app()));
    p.hline(
        bar.x_range(),
        bar.bottom(),
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );
    p.text(
        bar.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(theme.font_size_term_sm.value()),
        ec(theme.titlebar_fg()),
    );
    let mut right = child.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(egui::vec2(theme.spacing_xs.value(), 0.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    let close = IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(&mut right, theme, &|ui, r, c| {
            icons::CLOSE.image(r.height(), c).paint_at(ui, r)
        });
    if grab {
        let zone = egui::Rect::from_min_max(bar.min, egui::pos2(close.rect.left(), bar.max.y));
        if child
            .interact(zone, child.id().with("grab"), egui::Sense::hover())
            .hovered()
        {
            child.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    let size = theme.font_size_caption.value();
    let mut y = bar.bottom() + POPUP_BODY_PAD.value();
    for (line, disabled) in lines {
        let fg = if *disabled {
            ec(theme.text_disabled())
        } else {
            ec(theme.text_muted_raised())
        };
        // 시안 본문처럼 팝업 폭 안에서 줄바꿈한다.
        let wrap = rect.width() - POPUP_BODY_PAD.value() * 2.0;
        let galley = p.layout((*line).to_owned(), egui::FontId::monospace(size), fg, wrap);
        let rows = galley.rows.len().max(1) as f32;
        p.galley(
            egui::pos2(rect.left() + POPUP_BODY_PAD.value(), y),
            galley,
            fg,
        );
        y += size * POPUP_LINE_HEIGHT_FACTOR * rows;
    }
}

/// 손잡이 한 칸. hover 하면 accent-info 를 엷게 칠하고 그 방향 커서를 보인다.
fn zone(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, cursor: egui::CursorIcon, label: &str) {
    let hovered = ui
        .interact(rect, ui.id().with(("zone", label)), egui::Sense::hover())
        .hovered();
    let p = ui.painter();
    if hovered {
        ui.ctx().set_cursor_icon(cursor);
        p.rect_filled(
            rect,
            0.0,
            ec(theme.accent_info()).gamma_multiply(ZONE_HOVER_MIX),
        );
    }
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(DEMO_LABEL_SIZE.value()),
        ec(theme.accent_info()),
    );
}

/// 시안 `ResizeMap`: 테두리 안쪽 띠의 8칸. 모서리가 가장자리보다 우선한다.
fn resize_map(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(MAP_W.value(), MAP_H.value()),
        egui::Sense::hover(),
    );
    faux_popup(
        ui,
        theme,
        rect,
        "remote_tool — resizable",
        &[
            ("min 420×320 · drag any edge or corner", false),
            ("no visual grip — cursor only", true),
        ],
        false,
    );
    let band = MAP_BAND.value();
    let corner = MAP_CORNER.value();
    let r = rect;
    use egui::CursorIcon as C;
    let edges = [
        (
            egui::Rect::from_min_max(
                egui::pos2(r.left() + corner, r.top()),
                egui::pos2(r.right() - corner, r.top() + band),
            ),
            C::ResizeVertical,
            "N",
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(r.left() + corner, r.bottom() - band),
                egui::pos2(r.right() - corner, r.bottom()),
            ),
            C::ResizeVertical,
            "S",
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(r.left(), r.top() + corner),
                egui::pos2(r.left() + band, r.bottom() - corner),
            ),
            C::ResizeHorizontal,
            "W",
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(r.right() - band, r.top() + corner),
                egui::pos2(r.right(), r.bottom() - corner),
            ),
            C::ResizeHorizontal,
            "E",
        ),
    ];
    let c = egui::vec2(corner, corner);
    let corners = [
        (
            egui::Rect::from_min_size(r.left_top(), c),
            C::ResizeNwSe,
            "NW",
        ),
        (
            egui::Rect::from_min_size(egui::pos2(r.right() - corner, r.top()), c),
            C::ResizeNeSw,
            "NE",
        ),
        (
            egui::Rect::from_min_size(egui::pos2(r.left(), r.bottom() - corner), c),
            C::ResizeNeSw,
            "SW",
        ),
        (
            egui::Rect::from_min_size(r.right_bottom() - c, c),
            C::ResizeNwSe,
            "SE",
        ),
    ];
    for (z, cursor, label) in edges.into_iter().chain(corners) {
        zone(ui, theme, z, cursor, label);
    }
}

/// 커서 표의 글리프. 갤러리 글꼴에 없는 대각 양방향 화살표(⤡ ⤢)와 ✕ 는 직접 그린다.
#[derive(Clone, Copy)]
enum Glyph {
    Text(&'static str),
    /// 대각 양방향 화살표. `rising` 이면 왼쪽 아래 → 오른쪽 위(⤢), 아니면 ⤡.
    Diagonal {
        rising: bool,
    },
    Close,
}

fn paint_glyph(ui: &egui::Ui, theme: &Theme, cell: egui::Rect, glyph: Glyph) {
    let fg = ec(theme.text_secondary());
    let size = theme.font_size_body.value();
    let p = ui.painter();
    match glyph {
        Glyph::Text(s) => {
            p.text(
                cell.center(),
                egui::Align2::CENTER_CENTER,
                s,
                egui::FontId::proportional(size),
                fg,
            );
        }
        Glyph::Diagonal { rising } => {
            let h = size * 0.35;
            let d = if rising {
                egui::vec2(h, -h)
            } else {
                egui::vec2(h, h)
            };
            let stroke = egui::Stroke::new(theme.border_width.value(), fg);
            p.arrow(cell.center(), d, stroke);
            p.arrow(cell.center(), -d, stroke);
        }
        Glyph::Close => {
            let r = egui::Rect::from_center_size(cell.center(), egui::Vec2::splat(size));
            icons::CLOSE.image(size, fg).paint_at(ui, r);
        }
    }
}

/// 시안 `CursorMatrix`: 위치별 커서. 행에 포인터를 올리면 그 커서가 보인다.
fn cursor_matrix(ui: &mut egui::Ui, theme: &Theme) {
    use egui::CursorIcon as C;
    let rows: [(&str, &str, Glyph, C); 8] = [
        (
            "Content (inside, not a handle)",
            "default",
            Glyph::Text("\u{2196}"),
            C::Default,
        ),
        (
            "Drag handle — hover",
            "grab",
            Glyph::Text("\u{270b}"),
            C::Grab,
        ),
        ("Dragging", "grabbing", Glyph::Text("\u{270a}"), C::Grabbing),
        (
            "Corner NW / SE",
            "nwse-resize",
            Glyph::Diagonal { rising: false },
            C::ResizeNwSe,
        ),
        (
            "Corner NE / SW",
            "nesw-resize",
            Glyph::Diagonal { rising: true },
            C::ResizeNeSw,
        ),
        (
            "Edge E / W",
            "ew-resize",
            Glyph::Text("\u{2194}"),
            C::ResizeHorizontal,
        ),
        (
            "Edge N / S",
            "ns-resize",
            Glyph::Text("\u{2195}"),
            C::ResizeVertical,
        ),
        (
            "Close button — hover",
            "pointer",
            Glyph::Close,
            C::PointingHand,
        ),
    ];
    egui::Frame::new()
        .fill(ec(theme.bg_panel()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
        .show(ui, |ui| {
            let w =
                MATRIX_W.value() - (theme.spacing_sm.value() + theme.border_width.value()) * 2.0;
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = MATRIX_ROW_GAP.value();
            for (area, name, glyph, cursor) in rows {
                let row = egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(
                        theme.spacing_sm.value() as i8,
                        MATRIX_ROW_PAD_Y.value() as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(w - theme.spacing_sm.value() * 2.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = MATRIX_COL_GAP.value();
                            let (g, _) = ui.allocate_exact_size(
                                egui::vec2(MATRIX_GLYPH_COL.value(), theme.font_size_body.value()),
                                egui::Sense::hover(),
                            );
                            paint_glyph(ui, theme, g, glyph);
                            ui.label(
                                egui::RichText::new(area)
                                    .size(theme.font_size_term_sm.value())
                                    .color(ec(theme.text_secondary())),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(name)
                                            .monospace()
                                            .size(theme.font_size_micro.value())
                                            .color(ec(theme.text_muted())),
                                    );
                                },
                            );
                        });
                    });
                if ui
                    .interact(row.response.rect, ui.id().with(name), egui::Sense::hover())
                    .hovered()
                {
                    ui.ctx().set_cursor_icon(cursor);
                }
            }
        });
}

/// 캡션 한 줄과 그 아래 예제 상자.
fn captioned(ui: &mut egui::Ui, theme: &Theme, caption: &str, add: impl FnOnce(&mut egui::Ui)) {
    wrap_item(ui, |ui| {
        ui.spacing_mut().item_spacing.y = CAPTION_GAP.value();
        ui.label(
            egui::RichText::new(caption)
                .size(theme.font_size_caption.value())
                .color(ec(theme.text_muted())),
        );
        add(ui);
    });
}

/// 미니 팝업 바깥 높이: 제목 줄 + 본문 높이 + 테두리.
fn popup_size(theme: &Theme, (w, body_h): (LogicalPx, LogicalPx)) -> egui::Vec2 {
    egui::vec2(
        w.value(),
        theme.titlebar_height.value() + body_h.value() + theme.border_width.value() * 2.0,
    )
}

/// 시안 `ClampDemo`: scope 경계(점선) 오른쪽 아래 구석에 멈춘 팝업.
fn clamp_demo(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(CLAMP_W.value(), CLAMP_H.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, theme.corner_radius.value(), ec(theme.bg_app()));
    let stroke = egui::Stroke::new(theme.border_width.value(), ec(theme.border_strong()));
    let dash = theme.spacing_xs.value();
    let r = rect.shrink(theme.border_width.value() * 0.5);
    for pts in [
        [r.left_top(), r.right_top()],
        [r.right_top(), r.right_bottom()],
        [r.right_bottom(), r.left_bottom()],
        [r.left_bottom(), r.left_top()],
    ] {
        p.extend(egui::Shape::dashed_line(&pts, stroke, dash, dash));
    }
    p.text(
        rect.left_top() + egui::vec2(POPUP_BODY_PAD.value() * 0.6, theme.spacing_xs.value()),
        egui::Align2::LEFT_TOP,
        "scope rect",
        egui::FontId::monospace(DEMO_LABEL_SIZE.value()),
        ec(theme.text_disabled()),
    );
    let size = popup_size(theme, CLAMP_POPUP);
    let popup = egui::Rect::from_min_size(rect.right_bottom() - size, size);
    let mut clipped = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    clipped.set_clip_rect(rect.intersect(ui.clip_rect()));
    faux_popup(
        &mut clipped,
        theme,
        popup,
        "popup",
        &[("clamped — no part leaves the scope", false)],
        true,
    );
}

/// 시안 `ZOrderDemo`: 겹친 두 팝업. 앞 팝업만 손잡이와 커서가 반응한다.
fn zorder_demo(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ZORDER_W.value(), ZORDER_H.value()),
        egui::Sense::hover(),
    );
    let back = egui::Rect::from_min_size(rect.min, popup_size(theme, BEHIND_POPUP));
    let mut back_ui = ui.new_child(egui::UiBuilder::new().max_rect(back));
    back_ui.set_opacity(BEHIND_OPACITY);
    faux_popup(
        &mut back_ui,
        theme,
        back,
        "behind — inert",
        &[("handles don't respond", true)],
        false,
    );
    let size = popup_size(theme, FRONT_POPUP);
    let front = egui::Rect::from_min_size(rect.right_bottom() - size, size);
    faux_popup(
        ui,
        theme,
        front,
        "front — active",
        &[("top popup owns the cursor; click brings to front", false)],
        true,
    );
}

/// Overlays › Move & resize — 8방향 크기 조절 손잡이 지도와 커서.
pub fn draw_resize_map(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(DEMO_GAP.value(), DEMO_GAP.value());
        wrap_item(ui, |ui| resize_map(ui, theme));
        wrap_item(ui, |ui| cursor_matrix(ui, theme));
    });
    meta(
        ui,
        theme,
        &[
            ("zones", "N · S · E · W + NW · NE · SW · SE"),
            ("band", "inside the border (token-width)"),
            ("corner", "wins over edge at the crossing"),
            ("edge drag", "that edge moves, opposite fixed"),
            ("grip", "none — cursor only"),
            ("persist", "user size sticks until the popup closes"),
        ],
        &[
            TokenChip::new(
                "accent-info",
                "zone labels (demo only)",
                ec(theme.accent_info()),
            ),
            TokenChip::new("border-strong", "popup edge", ec(theme.border_strong())),
        ],
    );
    note(
        ui,
        theme,
        "Once you resize, the size sticks (the sizer won't overwrite it); closing the popup resets it to its default size on next open.",
    );
}

/// Overlays › Move & resize — 조용한 한계와 맨 앞 팝업 우선.
pub fn draw_constraints(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(DEMO_GAP.value(), DEMO_GAP.value());
        captioned(ui, theme, "scope clamp — stops at the boundary", |ui| {
            clamp_demo(ui, theme)
        });
        captioned(
            ui,
            theme,
            "z-order — only the front popup responds",
            |ui| zorder_demo(ui, theme),
        );
    });
    meta(
        ui,
        theme,
        &[
            ("min size", "min_size, else default_size"),
            ("max", "scope rect is the cap (no max_size)"),
            ("clamp", "no part leaves the scope"),
            ("reposition", "shrinking scope pulls it back in"),
            ("limit signal", "none — quiet stop (no warning color)"),
            ("z-order", "top-most only · press = bring to front"),
        ],
        &[
            TokenChip::new(
                "border-strong",
                "scope rect (demo)",
                ec(theme.border_strong()),
            ),
            TokenChip::without_color("shadow-modal", "popup lift"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't flash a warning color (peach/red) when a popup hits min size or the scope edge — the limit is communicated by simply not moving. Reserve danger tones for actual errors.",
    );
}
