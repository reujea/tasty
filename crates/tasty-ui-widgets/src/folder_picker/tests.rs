use super::*;

#[test]
fn translated_footer_keeps_buttons_and_path_tail_inside_the_minimum_width() {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let path = format!("/home/{}/selected", "long-parent-".repeat(20));
    for (label, confirm, cancel, hint) in [
        (
            "Folder",
            "Select folder",
            "Cancel",
            "Double-click {name} to open it.",
        ),
        (
            "폴더",
            "폴더 선택",
            "취소",
            "{name} 폴더를 열려면 더블클릭하세요.",
        ),
        (
            "フォルダー",
            "フォルダーを選択",
            "キャンセル",
            "{name} を開くにはダブルクリックします。",
        ),
    ] {
        for width in [LogicalPx(288.0), LogicalPx(448.0), LogicalPx(608.0)] {
            let ctx = egui::Context::default();
            let footer = FolderPickerFooter {
                label,
                path: &path,
                hint,
                selected: Some("selected"),
                confirm,
                cancel,
                ready: true,
            };
            let mut bounds = Rect::NOTHING;
            let mut shapes = Vec::new();
            for _ in 0..2 {
                let out = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width.value(), 360.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ctx, |ui| {
                                let height = footer.height(ui, &theme, width);
                                bounds = Rect::from_min_size(
                                    ui.next_widget_position(),
                                    egui::vec2(width.value(), height.value()),
                                );
                                footer.show(ui, &theme, &|_, _, _| {});
                            });
                    },
                );
                shapes = out.shapes;
            }
            for word in [confirm, cancel] {
                let text = shapes
                    .iter()
                    .filter_map(|s| {
                        if let egui::Shape::Text(t) = &s.shape {
                            Some((t, s.clip_rect))
                        } else {
                            None
                        }
                    })
                    .find(|(t, _)| t.galley.text() == word)
                    .expect("button text");
                let rect = Rect::from_min_size(text.0.pos, text.0.galley.size());
                assert!(
                    bounds.expand(1.0).contains_rect(rect),
                    "{word}: {rect:?} outside {bounds:?}"
                );
                assert!(text.1.contains_rect(rect));
            }
            assert!(shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().starts_with('…') && t.galley.text().ends_with("selected"))));
        }
    }
}
#[test]
fn unread_folder_omits_hint_and_uses_muted_ink_without_opacity() {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let ctx = egui::Context::default();
    let footer = FolderPickerFooter {
        label: "Folder",
        path: "/chosen",
        hint: "MUST-NOT-BE-DRAWN",
        selected: None,
        confirm: "Select folder",
        cancel: "Cancel",
        ready: false,
    };
    let out = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            footer.show(ui, &theme, &|_, _, _| {});
        });
    });
    let texts: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.shape {
                Some(t)
            } else {
                None
            }
        })
        .collect();
    assert!(!texts.iter().any(|t| t.galley.text() == footer.hint));
    assert!(
        texts
            .iter()
            .any(|t| t.galley.text() == "/chosen"
                && t.fallback_color == theme.text_muted().to_egui())
    );
}
