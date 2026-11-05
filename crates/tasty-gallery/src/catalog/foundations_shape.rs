//! 모서리 반경·테두리·컨트롤 높이·동작 시간 토큰 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{BadgeVariant, badge};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, note, stage};

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let box_w = theme.field_width_xs.value();
    let box_h = theme.spacing_xl.value() * 2.0;

    stage(ui, theme, StageVariant::Wrap, |ui| {
        cluster(ui, theme, "radius 4", |ui| {
            radius_box(ui, theme, box_w, box_h, theme.corner_radius.value());
        });
        cluster(ui, theme, "radius-sm 2", |ui| {
            radius_box(ui, theme, box_w, box_h, theme.corner_radius_sm.value());
        });
        cluster(ui, theme, "radius-pill", |ui| {
            badge(ui, theme, "99+", BadgeVariant::Danger);
        });
        cluster(ui, theme, "fixed heights", |ui| {
            height_box(ui, theme, "tree 22", theme.item_height_tree.value());
            height_box(
                ui,
                theme,
                "control 28",
                theme.item_height_interactive.value(),
            );
            height_box(ui, theme, "tab 24", theme.item_height_tab.value());
        });
    });

    meta(
        ui,
        theme,
        &[
            ("radius", "4px (sm 2px); pill uses full radius"),
            ("border", "1px border-default"),
            ("UI motion", "90–120ms ease-ui"),
            ("terminal motion", "0ms — content never animates"),
        ],
        &[
            TokenChip::without_color("radius", "4px everywhere"),
            TokenChip::without_color("radius-sm", "2px inner"),
            TokenChip::without_color("motion-ui", "120ms feedback"),
            TokenChip::without_color("motion-term", "0ms — terminal"),
            TokenChip::without_color("focus-ring-width", "2px ring"),
        ],
    );
    note(
        ui,
        theme,
        "터미널 정체성 — 모서리는 거의 직각(4px), 콘텐츠는 즉시 그려진다(0ms). 장식적 곡률·트랜지션 없음.",
    );
}

fn radius_box(ui: &mut egui::Ui, theme: &Theme, w: f32, h: f32, radius: f32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, radius, ec(theme.surface_raised()));
    p.rect_stroke(
        r,
        radius,
        egui::Stroke::new(theme.border_width.value(), ec(theme.border_default())),
        egui::StrokeKind::Inside,
    );
}

fn height_box(ui: &mut egui::Ui, theme: &Theme, label: &str, h: f32) {
    egui::Frame::new()
        .fill(ec(theme.surface_raised()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin {
            left: theme.spacing_md.value() as i8,
            right: theme.spacing_md.value() as i8,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.set_height(h);
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(ec(theme.text_secondary_raised())),
                );
            });
        });
}
