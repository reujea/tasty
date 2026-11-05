//! 목록과 상세 내용으로 나뉜 일반적인 레이아웃 예제.
//! 실제 Plugins 창은 components/plugins_window.rs에서 별도 치수와 필터 배치로 보여준다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// (name, selected)
const ITEMS: &[(&str, bool)] = &[
    ("ripgrep", false),
    ("agent-runner", true),
    ("git-lens", false),
    ("port-scanner", false),
];
/// detail Switch 행: (label, on)
const SWITCHES: &[(&str, bool)] = &[("Auto-start", true), ("Notifications", false)];
const PERMISSIONS: &[&str] = &["fs:read", "net", "clipboard"];

fn tag(p: &egui::Painter, theme: &Theme, pos: egui::Pos2, text: &str) -> f32 {
    let font = egui::FontId::proportional(theme.font_size_micro.value());
    let tw = p
        .layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
        .size()
        .x;
    let pad = theme.spacing_xs.value();
    let h = theme.spacing_lg.value();
    let r = egui::Rect::from_min_size(pos, egui::vec2(tw + pad * 2.0, h));
    p.rect_filled(
        r,
        theme.corner_radius_sm.value(),
        egui::Color32::from(theme.surface_raised()),
    );
    p.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        text,
        font,
        egui::Color32::from(theme.text_secondary_raised()),
    );
    r.width()
}

fn toggle(p: &egui::Painter, theme: &Theme, center: egui::Pos2, on: bool) {
    let w = theme.spacing_xl.value(); // 24
    let h = theme.spacing_md.value(); // 12
    let track = egui::Rect::from_center_size(center, egui::vec2(w, h));
    p.rect_filled(
        track,
        h * 0.5,
        egui::Color32::from(if on {
            theme.accent_primary()
        } else {
            theme.surface_raised()
        }),
    );
    let knob_r = h * 0.5 - theme.border_width.value();
    let kx = if on {
        track.max.x - knob_r - theme.border_width.value()
    } else {
        track.min.x + knob_r + theme.border_width.value()
    };
    p.circle_filled(
        egui::pos2(kx, center.y),
        knob_r,
        egui::Color32::from(theme.text_on_accent()),
    );
}

fn layout(ui: &mut egui::Ui, theme: &Theme) {
    let w = ui.available_width().min(theme.measure_xl.value());
    let h = theme.spacing_xl.value() * 9.0; // 216
    let list_w = theme.field_width_lg.value(); // 200
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_panel()),
    );
    let list = egui::Rect::from_min_size(rect.min, egui::vec2(list_w, h));
    p.rect_filled(list, 0.0, egui::Color32::from(theme.bg_sidebar()));
    let pad = theme.spacing_sm.value();
    let filter = egui::Rect::from_min_size(
        egui::pos2(list.min.x + pad, list.min.y + pad),
        egui::vec2(list_w - pad * 2.0, theme.item_height_interactive.value()),
    );
    p.rect_stroke(
        filter,
        theme.corner_radius_sm.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_default()),
        ),
        egui::StrokeKind::Inside,
    );
    p.text(
        egui::pos2(filter.min.x + theme.spacing_sm.value(), filter.center().y),
        egui::Align2::LEFT_CENTER,
        "Filter…",
        egui::FontId::proportional(theme.font_size_body.value()),
        egui::Color32::from(theme.text_placeholder()),
    );
    let mut y = filter.max.y + theme.spacing_sm.value();
    let row_h = theme.item_height_interactive.value();
    let dot_r = theme.status_dot_size.value() * 0.5;
    for (name, selected) in ITEMS {
        let row = egui::Rect::from_min_size(
            egui::pos2(list.min.x + theme.spacing_xs.value(), y),
            egui::vec2(list_w - theme.spacing_xs.value() * 2.0, row_h),
        );
        if *selected {
            p.rect_filled(
                row,
                theme.corner_radius_sm.value(),
                egui::Color32::from(theme.surface_active()),
            );
        }
        let dc = egui::pos2(row.min.x + theme.spacing_sm.value() + dot_r, row.center().y);
        p.circle_filled(
            dc,
            dot_r,
            egui::Color32::from(if *selected {
                theme.accent_agent_on_active()
            } else {
                theme.accent_agent()
            }),
        );
        p.text(
            egui::pos2(dc.x + dot_r + theme.spacing_sm.value(), row.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(theme.font_size_body.value()),
            egui::Color32::from(if *selected {
                theme.text_on_active()
            } else {
                theme.text_secondary()
            }),
        );
        y += row_h + theme.spacing_xs.value();
    }
    let dpad = theme.spacing_lg.value();
    let dx = list.max.x + dpad;
    let mut dy = rect.min.y + dpad;
    p.text(
        egui::pos2(dx, dy),
        egui::Align2::LEFT_TOP,
        "agent-runner",
        egui::FontId::proportional(theme.font_size_term_lg.value()),
        egui::Color32::from(theme.text_primary()),
    );
    let title_w = p
        .layout_no_wrap(
            "agent-runner".into(),
            egui::FontId::proportional(theme.font_size_term_lg.value()),
            egui::Color32::WHITE,
        )
        .size()
        .x;
    tag(
        &p,
        theme,
        egui::pos2(dx + title_w + theme.spacing_sm.value(), dy),
        "agent",
    );
    dy += theme.font_size_term_lg.value() + theme.spacing_lg.value();

    let label_w = theme.field_width_color.value() + theme.spacing_lg.value(); // 126 ≈ 130
    for (label, on) in SWITCHES {
        p.text(
            egui::pos2(dx, dy + theme.spacing_sm.value()),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(theme.font_size_body.value()),
            egui::Color32::from(theme.text_secondary()),
        );
        toggle(
            &p,
            theme,
            egui::pos2(
                dx + label_w + theme.spacing_md.value(),
                dy + theme.spacing_sm.value() + theme.font_size_body.value() * 0.5,
            ),
            *on,
        );
        dy += theme.item_height_interactive.value();
    }

    dy += theme.spacing_sm.value();
    p.text(
        egui::pos2(dx, dy),
        egui::Align2::LEFT_TOP,
        "PERMISSIONS",
        egui::FontId::proportional(theme.font_size_micro.value()),
        egui::Color32::from(theme.text_muted()),
    );
    dy += theme.spacing_lg.value();
    let mut tx = dx;
    for perm in PERMISSIONS {
        let tw = tag(&p, theme, egui::pos2(tx, dy), perm);
        tx += tw + theme.spacing_sm.value();
    }
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        layout(ui, theme);
    });

    spec::meta(
        ui,
        theme,
        &[
            ("list", "~200 fixed · bg-sidebar"),
            ("list item", "agent dot + name"),
            ("selected", "surface-active"),
            ("detail", "flex · spacing-lg padding"),
            ("detail head", "font-size-term-lg title + Tag"),
            ("switch row", "field-width-color + spacing-lg label; Switch"),
        ],
        &[
            TokenChip::new("bg-sidebar", "list fill", theme.bg_sidebar().into()),
            TokenChip::new("bg-panel", "detail fill", theme.bg_panel().into()),
            TokenChip::new(
                "surface-active",
                "selected item",
                theme.surface_active().into(),
            ),
            TokenChip::without_color("space-lg", "detail pad"),
            TokenChip::new("accent-agent", "agent dot", theme.accent_agent().into()),
        ],
    );

    spec::note(
        ui,
        theme,
        "한 단계 깊이 — 좌측에서 항목을 고르면 우측이 그 detail 로 채워진다. \
         리스트 폭은 고정, detail 이 남는 공간을 차지한다.",
    );
}
