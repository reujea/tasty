//! 워크스페이스 탭과 페인 탭을 두 줄로 구분한 정적 예제.
//! 배경과 활성 표시를 달리해 두 단계의 소속 관계를 보여준다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};

const WORKSPACES: &[(&str, bool)] = &[("main", true), ("review", false), ("agent", false)];
const PANE_TABS: &[(&str, bool)] = &[("README.md", false), ("build.rs", true), ("run.rs", false)];

fn window(ui: &mut egui::Ui, theme: &Theme) {
    let w = theme.measure_lg.value(); // 460
    let tier1_h = theme.spacing_xl.value() + theme.spacing_sm.value(); // 32
    let tier2_h = theme.item_height_tab.value(); // 24
    let content_h = theme.spacing_xl.value() * 4.0; // 96
    let total_h = tier1_h + tier2_h + content_h;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, total_h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_app()),
    );

    let pad = theme.spacing_md.value(); // 12 tab padding
    let dot_r = theme.status_dot_size.value() * 0.5;
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let tier1 = egui::Rect::from_min_size(rect.min, egui::vec2(w, tier1_h));
    let mut x = tier1.min.x;
    for (name, active) in WORKSPACES {
        let label_w = font_w(&p, name, &font) + dot_r * 2.0 + theme.spacing_xs.value();
        let tab_w = label_w + pad * 2.0;
        let tab = egui::Rect::from_min_size(egui::pos2(x, tier1.min.y), egui::vec2(tab_w, tier1_h));
        if *active {
            p.rect_filled(tab, 0.0, egui::Color32::from(theme.surface_active()));
        }
        let cy = tab.center().y;
        let dc = egui::pos2(tab.min.x + pad + dot_r, cy);
        p.circle_filled(
            dc,
            dot_r,
            egui::Color32::from(if *active {
                theme.accent_success()
            } else {
                theme.text_muted()
            }),
        );
        p.text(
            egui::pos2(dc.x + dot_r + theme.spacing_xs.value(), cy),
            egui::Align2::LEFT_CENTER,
            name,
            font.clone(),
            egui::Color32::from(if *active {
                theme.text_on_active()
            } else {
                theme.text_secondary()
            }),
        );
        x += tab_w;
    }
    let tier2 =
        egui::Rect::from_min_size(egui::pos2(rect.min.x, tier1.max.y), egui::vec2(w, tier2_h));
    p.rect_filled(tier2, 0.0, egui::Color32::from(theme.bg_sidebar()));
    let tab_w = theme.field_width_md.value() * 0.625; // 100
    let small = egui::FontId::proportional(theme.font_size_caption.value());
    let mut tx = tier2.min.x;
    for (i, (name, active)) in PANE_TABS.iter().enumerate() {
        let tab =
            egui::Rect::from_min_size(egui::pos2(tx, tier2.min.y), egui::vec2(tab_w, tier2_h));
        if *active {
            p.rect_filled(tab, 0.0, egui::Color32::from(theme.bg_panel()));
            let bar = egui::Rect::from_min_size(
                tab.min,
                egui::vec2(tab_w, theme.tab_indicator_width().value()),
            );
            p.rect_filled(bar, 0.0, egui::Color32::from(theme.accent_primary()));
        }
        if i > 0 {
            p.vline(
                tx,
                tier2.y_range(),
                egui::Stroke::new(
                    theme.border_width.value(),
                    theme.separator.to_egui_premultiplied(),
                ),
            );
        }
        p.text(
            tab.center(),
            egui::Align2::CENTER_CENTER,
            name,
            small.clone(),
            egui::Color32::from(if *active {
                theme.text_primary()
            } else {
                theme.text_muted()
            }),
        );
        tx += tab_w;
    }
    let content = egui::Rect::from_min_max(egui::pos2(rect.min.x, tier2.max.y), rect.max)
        .shrink(theme.spacing_sm.value());
    p.rect_filled(
        content,
        theme.corner_radius_sm.value(),
        egui::Color32::from(theme.surface("terminal").focused_bg),
    );
}

fn font_w(p: &egui::Painter, text: &str, font: &egui::FontId) -> f32 {
    p.layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
        .size()
        .x
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        window(ui, theme);
    });

    spec::meta(
        ui,
        theme,
        &[
            ("tier1", "32 · bg-app · workspace tabs"),
            ("tier2", "24 · bg-sidebar · pane tabs"),
            ("active tier1", "surface-active fill"),
            ("active tier2", "bg-panel + accent top bar"),
            ("content", "terminal focused background · spacing-sm margin"),
            ("depth", "2 tier max"),
        ],
        &[
            TokenChip::new("bg-app", "tier1 strip", theme.bg_app().into()),
            TokenChip::new("bg-sidebar", "tier2 strip", theme.bg_sidebar().into()),
            TokenChip::new(
                "surface-active",
                "active workspace",
                theme.surface_active().into(),
            ),
            TokenChip::new(
                "accent-primary",
                "active pane bar",
                theme.accent_primary().into(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "워크스페이스 탭과 그 안의 페인 탭을 배경과 활성 표시로 구분한다. 이 예제는 두 단계까지만 보여준다.",
    );
}
