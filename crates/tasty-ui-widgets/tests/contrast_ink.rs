//! Verify specified color contrast and the actual trusted body paint without changing its tint.
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{PluginTrustKind, plugin_trust_box};

fn themes() -> [Theme; 2] {
    let (partial, light) = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT)
        .unwrap()
        .to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    [
        tasty_themes::mocha_fallback(),
        Theme::with_colors(colors, light.unwrap()),
    ]
}

#[allow(clippy::disallowed_methods)] // reason: 대비 시험에서 Theme 채널을 합성한 계산값을 조립하며 제품 색상을 정의하지 않는다.
fn composite(src: egui::Color32, dst: egui::Color32) -> egui::Color32 {
    let channel = |s: u8, d: u8| {
        (f32::from(s) + f32::from(d) * (1.0 - f32::from(src.a()) / 255.0)).round() as u8
    };
    egui::Color32::from_rgb(
        channel(src.r(), dst.r()),
        channel(src.g(), dst.g()),
        channel(src.b(), dst.b()),
    )
}

fn contrast(a: egui::Color32, b: egui::Color32) -> f64 {
    let luminance = |c: egui::Color32| {
        let linear = |v: u8| {
            let s = f64::from(v) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn warning_and_trusted_inks_clear_their_actual_composited_grounds() {
    for th in themes() {
        let base = th.bg_panel().to_egui();
        let hover = composite(
            th.notification_item_bg_hover().to_egui_premultiplied(),
            base,
        );
        assert_eq!(
            th.notification_item_warning_glyph(),
            th.accent_warning_glyph()
        );
        for bg in [base, hover] {
            assert!(contrast(th.notification_item_warning_glyph().to_egui(), bg) >= 3.0);
        }
        assert_eq!(th.plugin_trust_body_trusted_fg(), th.accent_success_ink());
        assert_eq!(th.plugin_trust_glyph_success(), th.accent_success_ink());
        for ground in [th.bg_sidebar(), th.bg_panel()] {
            let bg = composite(
                th.accent_success()
                    .to_egui()
                    .gamma_multiply(th.tint_fill_alpha()),
                ground.to_egui(),
            );
            assert!(contrast(th.plugin_trust_body_trusted_fg().to_egui(), bg) >= 4.5);
            assert!(contrast(th.plugin_trust_glyph_success().to_egui(), bg) >= 3.0);
        }
        assert_eq!(th.notification_item_error_glyph(), th.accent_danger());
        if !th.is_light {
            assert_eq!(th.accent_success_ink(), th.accent_success());
            assert_eq!(th.notification_item_warning_glyph(), th.accent_warning());
        }
    }
}

#[test]
fn trusted_body_paints_the_ink_role_while_fill_and_border_keep_the_original_tone() {
    for th in themes() {
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_width(th.measure_xl.value());
                plugin_trust_box(
                    ui,
                    &th,
                    PluginTrustKind::Trusted,
                    "",
                    &["trust sentinel"],
                    |_| {},
                );
            });
        });
        let mut body = false;
        let mut fill = false;
        let mut stroke = false;
        let mut shapes: Vec<_> = out.shapes.into_iter().map(|s| s.shape).collect();
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(v) => shapes.extend(v),
                egui::Shape::Text(t) if t.galley.text() == "trust sentinel" => {
                    body = true;
                    assert_eq!(
                        t.galley.job.sections[0].format.color,
                        th.plugin_trust_body_trusted_fg().to_egui()
                    );
                }
                egui::Shape::Rect(r) => {
                    fill |= r.fill
                        == th
                            .accent_success()
                            .to_egui()
                            .gamma_multiply(th.tint_fill_alpha());
                    stroke |= r.stroke.color
                        == th
                            .accent_success()
                            .to_egui()
                            .gamma_multiply(th.tint_border_alpha());
                }
                _ => {}
            }
        }
        assert!(
            body && fill && stroke,
            "actual body, tint and border must all be painted"
        );
    }
}

#[test]
fn hovered_row_label_reads_the_raised_secondary_on_every_parent() {
    use tasty_ui_widgets::{RowParent, RowState};
    for th in themes() {
        for parent in [RowParent::Mantle, RowParent::Base] {
            assert_eq!(
                RowState::Hover.label(&th, parent, th.tree_row_fg()),
                th.text_secondary_raised()
            );
        }
        assert_eq!(th.icon_button_fg_hover(), th.text_secondary());
        assert_eq!(th.button_ghost_fg_hover(), th.text_secondary_raised());
    }
}
