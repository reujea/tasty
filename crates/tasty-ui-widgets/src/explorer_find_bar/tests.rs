use super::*;

#[test]
fn a_match_ignores_case_and_returns_byte_bounds() {
    assert_eq!(match_range("Report.pdf", "re"), Some(0..2));
    assert_eq!(match_range("mockup-exports", "RE"), None);
    assert_eq!(match_range("mockup-exports", "po"), Some(9..11));
    assert_eq!(match_range("한글report", "rep"), Some(6..9));
    assert_eq!(match_range("notes.md", ""), None);
    assert_eq!(match_range("a", "abc"), None);
}

#[test]
fn the_bar_is_the_search_bar_height() {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let ctx = egui::Context::default();
    let mut query = String::from("re");
    let mut deep = false;
    let mut height = 0.0;
    drop(ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let top = ui.cursor().top();
                explorer_find_bar(
                    ui,
                    &theme,
                    ExplorerFindBar {
                        query: &mut query,
                        subfolders: Some(&mut deep),
                        status: ExplorerFindStatus::Text("2 of 5"),
                        labels: ExplorerFindLabels {
                            placeholder: "Filter this folder",
                            subfolders: "Subfolders",
                            close: "Close",
                        },
                        focus: false,
                    },
                );
                height = ui.cursor().top() - top;
            });
    }));
    assert_eq!(height, theme.explorer_search_bar_height().value());
}

#[test]
fn a_tree_row_paints_the_matching_part_of_its_label() {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let match_fg = theme.accent_primary_raised().to_egui();
    let ctx = egui::Context::default();
    let painted = |query: &'static str| {
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                crate::tree_row_matching(
                    ui,
                    &theme,
                    0,
                    false,
                    false,
                    None,
                    "report.pdf",
                    query,
                    None,
                    false,
                );
            });
        });
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(text) if text.galley.text() == "report.pdf" => Some(
                    text.galley
                        .job
                        .sections
                        .iter()
                        .map(|s| (s.byte_range.clone(), s.format.color == match_fg))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .next()
            .expect("label painted")
    };
    assert_eq!(painted("PO"), [(0..2, false), (2..4, true), (4..10, false)]);
    assert_eq!(painted(""), [(0..10, false)]);
}

#[test]
fn nfd_display_maps_raw_highlights_without_splitting_utf8_or_composed_letters() {
    let theme = tasty_themes::mocha_fallback();
    let ink = theme.text_primary().to_egui();
    let marked = theme.accent_primary_raised().to_egui();
    for (raw, query, display, highlighted) in [
        ("cafe\u{301}.txt", "e", "café.txt", "é"),
        ("cafe\u{301}.txt", "\u{301}", "café.txt", "é"),
        ("cafe\u{301}.txt", ".txt", "café.txt", ".txt"),
        (
            "\u{1100}\u{1161}\u{1102}\u{1161}.txt",
            "\u{1100}",
            "가나.txt",
            "가",
        ),
        (
            "\u{1100}\u{1161}\u{1102}\u{1161}.txt",
            "\u{1161}\u{1102}",
            "가나.txt",
            "가나",
        ),
        (
            "\u{30ab}\u{3099}.txt",
            "\u{3099}",
            "\u{30ac}.txt",
            "\u{30ac}",
        ),
        ("a\u{315}\u{300}x", "\u{315}", "à\u{315}x", "à\u{315}"),
        (
            "e\u{301}\u{1f469}\u{200d}\u{1f4bb}.txt",
            "\u{1f4bb}",
            "é\u{1f469}\u{200d}\u{1f4bb}.txt",
            "\u{1f4bb}",
        ),
        ("q\u{301}.txt", "\u{301}", "q\u{301}.txt", "\u{301}"),
        ("한글é.txt", "글", "한글é.txt", "글"),
    ] {
        let job = explorer_match_job(
            &theme,
            raw,
            query,
            egui::FontId::monospace(theme.font_size_caption.value()),
            ink,
            false,
        );
        assert_eq!(job.text, display, "{raw:?}");
        let mut selected = String::new();
        for section in &job.sections {
            let span = section.byte_range.clone();
            assert!(job.text.is_char_boundary(span.start) && job.text.is_char_boundary(span.end));
            if section.format.color == marked {
                selected.push_str(&job.text[span]);
            }
        }
        assert_eq!(selected, highlighted, "{raw:?} / {query:?}");
    }
}

#[test]
fn composing_display_does_not_normalize_name_matching() {
    let theme = tasty_themes::mocha_fallback();
    let raw = "cafe\u{301}.rs";
    assert_eq!(match_range(raw, "café"), None);
    let job = explorer_match_job(
        &theme,
        raw,
        "café",
        egui::FontId::monospace(theme.font_size_caption.value()),
        theme.text_primary().to_egui(),
        false,
    );
    assert_eq!(job.text, "café.rs");
    assert!(
        job.sections
            .iter()
            .all(|s| s.format.color != theme.accent_primary_raised().to_egui())
    );
    assert_eq!(raw.as_bytes(), b"cafe\xcc\x81.rs");
}

#[test]
fn a_selected_row_underlines_the_match_run_and_a_resting_row_does_not() {
    let theme = tasty_themes::mocha_fallback();
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let ink = theme.text_on_active().to_egui();
    let underlines = |selected: bool| {
        explorer_match_job(&theme, "report.pdf", "po", font.clone(), ink, selected)
            .sections
            .iter()
            .map(|s| s.format.underline)
            .collect::<Vec<_>>()
    };
    let run = egui::Stroke::new(
        theme.autocomplete_match_underline_width().value(),
        theme.explorer_match_fg_active().to_egui(),
    );
    assert_eq!(
        underlines(true),
        [egui::Stroke::NONE, run, egui::Stroke::NONE]
    );
    assert_eq!(underlines(false), [egui::Stroke::NONE; 3]);
}

/// 서식 필드만 보면 렌더러가 무시해도 통과하므로 글자 mesh에 선이 들어갔는지 본다.
#[test]
fn the_match_underline_reaches_the_text_mesh() {
    let theme = tasty_themes::mocha_fallback();
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let ink = theme.text_on_active().to_egui();
    let ctx = egui::Context::default();
    drop(ctx.run(egui::RawInput::default(), |_| {}));
    let vertices = |selected: bool| {
        let job = explorer_match_job(&theme, "report.pdf", "po", font.clone(), ink, selected);
        ctx.fonts(|f| f.layout_job(job)).rows[0]
            .visuals
            .mesh
            .vertices
            .len()
    };
    assert!(vertices(true) > vertices(false));
}
