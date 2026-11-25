use super::*;

fn frame(
    ctx: &egui::Context,
    th: &Theme,
    lang: &str,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    specimen_frame(ctx, th, Specimen::Narrow, lang, events)
}

fn specimen_frame(
    ctx: &egui::Context,
    th: &Theme,
    variant: Specimen,
    lang: &str,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 2000.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::Area::new(egui::Id::new("narrow"))
                .fixed_pos(egui::Pos2::ZERO)
                .fade_in(false)
                .show(ctx, |ui| {
                    ui.set_width(288.0);
                    ui.set_max_width(288.0);
                    page(ui, th, variant, lang);
                });
        },
    )
}

fn texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn narrow_locales_render_the_received_pending_fields_and_japanese_commits_ten() {
    let th = tasty_themes::mocha_fallback();
    for (lang, bounds) in [
        ("en", Some(("2", "10000", "2"))),
        ("ko", None),
        ("ja", Some(("10", "100000", "10"))),
    ] {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        for _ in 0..3 {
            frame(&ctx, &th, lang, Vec::new());
        }
        let output = frame(&ctx, &th, lang, Vec::new());
        let invalid: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.stroke.color == th.input_border_invalid().to_egui() =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        if let Some((min, max, value)) = bounds {
            let expected = tasty_i18n::fill_args(
                crate::i18n::t_lang(lang, "settings.anomaly.range"),
                &[min, max, value],
            );
            assert!(
                texts(&output).contains(&expected),
                "{lang}: actual emitted range is not the received one: {:?}",
                texts(&output)
            );
            assert!(!invalid.is_empty());
        } else {
            assert!(invalid.is_empty(), "received Korean narrow frame is normal");
        }
        if lang == "ja" {
            let point = invalid[0].center();
            let click = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                &ctx,
                &th,
                lang,
                vec![egui::Event::PointerMoved(point), click(true)],
            );
            frame(&ctx, &th, lang, vec![click(false)]);
            frame(
                &ctx,
                &th,
                lang,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let committed = frame(&ctx, &th, lang, Vec::new());
            assert!(
                texts(&committed).contains(&"10".into()),
                "Enter commits Japanese call-burst threshold to10"
            );
            assert!(!committed.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color==th.input_border_invalid().to_egui())));
        }
    }

    // 실제로 그린 범위 안내를 수령 문구와 대조해 formatter 전환에도 값과 문구가 유지되는지 확인한다.
    for (lang, expected) in [
        ("en", "Between 2 and 10000. Commits as 2."),
        ("ko", "2~10000 사이의 값입니다. 2 값으로 확정됩니다."),
        ("ja", "2〜10000 の範囲です。2 として確定します。"),
    ] {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        for _ in 0..3 {
            specimen_frame(&ctx, &th, Specimen::Invalid, lang, Vec::new());
        }
        let output = specimen_frame(&ctx, &th, Specimen::Invalid, lang, Vec::new());
        assert!(
            texts(&output).contains(&expected.into()),
            "{lang}: range copy changed"
        );
    }
}
