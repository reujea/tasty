//! Raised fields keep a visible box, including an invalid edge under focus.
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Ground, Input};

#[test]
fn raised_input_paints_fill_edge_and_focus_ring_in_both_themes() {
    let mocha = tasty_themes::mocha_fallback();
    let (partial, light) = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT)
        .unwrap()
        .to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    let latte = Theme::with_colors(colors, light.unwrap());
    for theme in [mocha, latte] {
        for (invalid, focused) in [(false, false), (false, true), (true, false), (true, true)] {
            for hover in [false, true] {
                let ctx = egui::Context::default();
                let mut bounds = egui::Rect::NOTHING;
                let mut output = None;
                for _ in 0..3 {
                    let raw = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400.0, 200.0),
                        )),
                        events: if hover && bounds.is_positive() {
                            vec![egui::Event::PointerMoved(bounds.center())]
                        } else {
                            vec![egui::Event::PointerGone]
                        },
                        ..Default::default()
                    };
                    output = Some(ctx.run(raw, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            bounds = Input::new()
                                .ground(Ground::Raised)
                                .invalid(invalid)
                                .focus_look(focused)
                                .show(ui, &theme, &mut String::from("query"))
                                .rect;
                        });
                    }));
                }
                let output = output.unwrap();
                let rects: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r) => Some(r),
                        _ => None,
                    })
                    .collect();
                assert!(
                    rects
                        .iter()
                        .any(|r| r.rect == bounds && r.fill == theme.input_raised_bg().to_egui())
                );
                let edge = if invalid {
                    theme.input_border_invalid()
                } else if focused {
                    theme.input_border_focus()
                } else {
                    theme.input_raised_border()
                }
                .to_egui();
                assert!(
                    rects.iter().any(|r| r.rect == bounds
                        && r.stroke.color == edge
                        && r.stroke.width == theme.border_width.value()),
                    "invalid={invalid}, focused={focused}, hover={hover}"
                );
                // 링은 테두리 바로 바깥에 붙는다: 같은 사각형에 Outside 획이다.
                let ring = rects.iter().find(|r| {
                    r.rect == bounds
                        && r.stroke.width > 0.0
                        && r.stroke_kind == egui::StrokeKind::Outside
                });
                assert_eq!(ring.is_some(), focused);
                if let Some(ring) = ring {
                    assert_eq!(ring.stroke.color, edge);
                    assert_eq!(ring.stroke.width, theme.border_width.value());
                }
            }
        }
    }
}
