//! 상단 탭·왼쪽 섹션 목록·오른쪽 콘텐츠로 나뉜 일반적인 레이아웃 예제.
//! 실제 Settings 창은 components/settings.rs에서 별도 치수로 보여준다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};

const L1_TABS: &[(&str, bool)] = &[
    ("General", false),
    ("Terminal", false),
    ("Appearance", true),
    ("Plugins", false),
];
const L2_SECTIONS: &[(&str, bool)] = &[
    ("Theme", true),
    ("Font", false),
    ("Colors", false),
    ("Cursor", false),
];

fn layout(ui: &mut egui::Ui, theme: &Theme) {
    let w = ui.available_width().min(theme.measure_xl.value());
    let l1_h = theme.item_height_interactive.value() + theme.spacing_md.value(); // 40
    let body_h = theme.spacing_xl.value() * 8.0; // 192
    let l2_w = theme.field_width_md.value() + theme.spacing_sm.value(); // 168
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, l1_h + body_h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_panel()),
    );
    let l1 = egui::Rect::from_min_size(rect.min, egui::vec2(w, l1_h));
    p.rect_filled(l1, 0.0, egui::Color32::from(theme.bg_sidebar()));
    let pad = theme.spacing_md.value(); // 12
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let mut x = l1.min.x + pad;
    for (label, active) in L1_TABS {
        let tw = p
            .layout_no_wrap((*label).into(), font.clone(), egui::Color32::WHITE)
            .size()
            .x;
        p.text(
            egui::pos2(x, l1.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font.clone(),
            egui::Color32::from(if *active {
                theme.text_primary()
            } else {
                theme.text_secondary()
            }),
        );
        if *active {
            let underline = egui::Rect::from_min_size(
                egui::pos2(x, l1.max.y - theme.tab_indicator_width().value()),
                egui::vec2(tw, theme.tab_indicator_width().value()),
            );
            p.rect_filled(underline, 0.0, egui::Color32::from(theme.accent_primary()));
        }
        x += tw + pad * 2.0;
    }
    let l2 = egui::Rect::from_min_size(egui::pos2(rect.min.x, l1.max.y), egui::vec2(l2_w, body_h));
    p.rect_filled(l2, 0.0, egui::Color32::from(theme.bg_sidebar()));
    let spad = theme.spacing_sm.value();
    let filter = egui::Rect::from_min_size(
        egui::pos2(l2.min.x + spad, l2.min.y + spad),
        egui::vec2(l2_w - spad * 2.0, theme.item_height_interactive.value()),
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
        font.clone(),
        egui::Color32::from(theme.text_placeholder()),
    );
    let mut y = filter.max.y + spad;
    let row_h = theme.item_height_interactive.value();
    for (name, selected) in L2_SECTIONS {
        let row = egui::Rect::from_min_size(
            egui::pos2(l2.min.x + theme.spacing_xs.value(), y),
            egui::vec2(l2_w - theme.spacing_xs.value() * 2.0, row_h),
        );
        if *selected {
            p.rect_filled(
                row,
                theme.corner_radius_sm.value(),
                egui::Color32::from(theme.surface_active()),
            );
        }
        p.text(
            egui::pos2(row.min.x + theme.spacing_sm.value(), row.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            font.clone(),
            egui::Color32::from(if *selected {
                theme.text_on_active()
            } else {
                theme.text_secondary()
            }),
        );
        y += row_h + theme.spacing_xs.value();
    }
    let dpad = theme.spacing_lg.value();
    let cx0 = l2.max.x + dpad;
    let mut cy = l1.max.y + dpad;
    p.text(
        egui::pos2(cx0, cy),
        egui::Align2::LEFT_TOP,
        "THEME PRESETS",
        egui::FontId::proportional(theme.font_size_micro.value()),
        egui::Color32::from(theme.text_muted()),
    );
    cy += theme.spacing_lg.value();

    let presets = [
        theme.accent_primary(),
        theme.accent_success(),
        theme.accent_warning(),
        theme.accent_danger(),
        theme.accent_info(),
        theme.accent_agent(),
    ];
    let cell = theme.spacing_xl.value() * 2.0; // 48
    let gap = theme.spacing_sm.value();
    let cols = 3;
    for (i, color) in presets.iter().enumerate() {
        let col = (i % cols) as f32;
        let rowi = (i / cols) as f32;
        let c = egui::Rect::from_min_size(
            egui::pos2(cx0 + col * (cell + gap), cy + rowi * (cell + gap)),
            egui::vec2(cell, cell),
        );
        p.rect_filled(c, theme.corner_radius.value(), egui::Color32::from(*color));
        if i == 0 {
            p.rect_stroke(
                c,
                theme.corner_radius.value(),
                egui::Stroke::new(
                    theme.focus_ring_width.value(),
                    egui::Color32::from(theme.text_primary()),
                ),
                egui::StrokeKind::Outside,
            );
        }
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
            ("L1 bar", "40 · bg-sidebar"),
            ("L1 active", "2px accent underline"),
            ("L2 list", "168 · filter + sections"),
            ("L2 selected", "surface-active"),
            ("content", "flex · spacing-lg padding"),
            ("rule", "L1 fixed · L2 grows"),
        ],
        &[
            TokenChip::new("bg-sidebar", "L1/L2 fill", theme.bg_sidebar().into()),
            TokenChip::new("bg-panel", "content fill", theme.bg_panel().into()),
            TokenChip::new(
                "surface-active",
                "selected section",
                theme.surface_active().into(),
            ),
            TokenChip::new(
                "accent-primary",
                "L1 underline",
                theme.accent_primary().into(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "두 단계 깊이 — 상단 L1 탭으로 큰 영역을, 좌측 L2 리스트로 그 안의 섹션을 \
         고른다. L1 은 underline, L2 는 surface-active 로 활성 표시 방식을 구분.",
    );
}
