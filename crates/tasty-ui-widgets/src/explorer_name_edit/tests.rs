use super::*;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 편집 줄을 두 프레임 그려 (줄, 입력 칸) rect 를 돌려준다.
fn rects(layout: ExplorerNameLayout) -> (egui::Rect, egui::Rect) {
    rects_in(layout, ExplorerNameOptions::default())
}

fn rects_in(
    layout: ExplorerNameLayout,
    options: ExplorerNameOptions<'_>,
) -> (egui::Rect, egui::Rect) {
    let theme = theme();
    let ctx = egui::Context::default();
    let mut edit = ExplorerNameEdit::new("New folder".into(), 0..10);
    let mut out = None;
    for _ in 0..2 {
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    // 넓은 입력이 왼쪽으로 나올 자리를 두고 그린다.
                    let at = ui.max_rect().min + egui::Vec2::X * 120.0;
                    let ui = &mut ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_size(at, egui::vec2(400.0, 300.0))),
                    );
                    let (row, field, _) = explorer_name_row_in(
                        ui,
                        &theme,
                        layout,
                        options,
                        tasty_icons::FOLDER,
                        &mut edit,
                        false,
                    );
                    out = Some((row, field));
                });
        }));
    }
    out.expect("drawn")
}

#[test]
fn a_detail_editor_is_one_table_row_and_stays_in_the_name_column() {
    let theme = theme();
    let (row, field) = rects(ExplorerNameLayout::Detail {
        inset: 10.0,
        name_width: 200.0,
    });
    assert_eq!(row.height(), theme.table_cell_height().value());
    assert!(field.right() <= row.left() + 200.0, "{field:?}");
}

#[test]
fn a_list_editor_grows_the_row_to_the_table_row_height() {
    let theme = theme();
    let (row, field) = rects(ExplorerNameLayout::List);
    assert!(theme.table_cell_height().value() > theme.tree_row_height().value());
    assert_eq!(row.height(), theme.table_cell_height().value());
    // 글리프 가운데가 아래 tree_row 아이콘 가운데와 같다. 입력 칸은 글리프 오른쪽 간격 뒤에서 시작한다.
    let glyph_right = crate::tree_row_icon_center(&theme) + theme.icon_glyph_size_md.value() * 0.5;
    assert_eq!(
        field.left() - row.left(),
        glyph_right + theme.spacing_sm.value()
    );
}

/// 목록 편집 줄을 배율 `zoom` 으로 두 프레임 그려(글리프 텍스처가 자리 잡게) (줄, 그린 글리프 사각형) 을 돌려준다.
fn list_glyph_at(zoom: f32) -> (egui::Rect, egui::Rect) {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 칠해지지 않는다.
    egui_extras::install_image_loaders(&ctx);
    let mut edit = ExplorerNameEdit::new("New folder".into(), 0..10);
    let mut row = None;
    let mut glyphs = Vec::new();
    for _ in 0..2 {
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let (r, _, _) = explorer_name_row_in(
                        ui,
                        &theme,
                        ExplorerNameLayout::List,
                        ExplorerNameOptions::default(),
                        tasty_icons::FOLDER,
                        &mut edit,
                        false,
                    );
                    row = Some(r);
                });
        });
        glyphs = out
            .shapes
            .into_iter()
            .filter_map(|c| match c.shape {
                egui::Shape::Rect(r) if r.brush.is_some() => Some(r.rect),
                _ => None,
            })
            .collect();
    }
    let row = row.expect("drawn");
    assert_eq!(glyphs.len(), 1, "줄 안 글리프가 하나가 아니다: {glyphs:?}");
    (row, glyphs[0])
}

/// 목록 편집 줄이 그린 글리프의 가운데는 트리 행 아이콘 가운데(`tree_row_icon_center`)다. 배율 1에서 29.
#[test]
fn a_list_editor_glyph_centres_on_the_tree_row_icon() {
    for zoom in [0.85, 1.0, 1.2] {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
        let (row, glyph) = list_glyph_at(zoom);
        let center = glyph.center().x - row.left();
        let want = crate::tree_row_icon_center(&theme);
        assert!(
            (center - want).abs() < 0.5,
            "배율 {zoom}: 그린 글리프 가운데 {center} 가 tree_row_icon_center {want} 가 아니다"
        );
        if zoom == 1.0 {
            assert!(
                (center - 29.0).abs() < 0.5,
                "배율 1: 글리프 가운데 {center} 가 29 가 아니다"
            );
        }
    }
}

#[test]
fn a_field_with_the_in_folder_caption_stops_at_the_create_field_width() {
    let theme = theme();
    let options = ExplorerNameOptions {
        caption: Some("in mockup-exports"),
        ..Default::default()
    };
    let max = theme.explorer_create_field_max_width().value();
    for layout in [
        ExplorerNameLayout::List,
        ExplorerNameLayout::Detail {
            inset: 10.0,
            name_width: 400.0,
        },
    ] {
        let (_, field) = rects_in(layout, options);
        assert_eq!(field.width(), max, "{layout:?}");
        let (_, plain) = rects(layout);
        assert!(plain.width() > max, "{layout:?}: no caption, no cap");
    }
}

#[test]
fn a_grid_editor_centres_a_field_wider_than_the_cell() {
    let theme = theme();
    let cell = egui::vec2(80.0, 100.0);
    let (rect, field) = rects(ExplorerNameLayout::Grid { cell, slot: 16.0 });
    assert_eq!(rect.size(), cell);
    assert_eq!(field.width(), theme.field_width_md.value());
    assert!((field.center().x - rect.center().x).abs() < 0.5);
}

#[test]
fn the_first_frame_takes_the_initial_selection() {
    let theme = theme();
    let ctx = egui::Context::default();
    let mut edit = ExplorerNameEdit::new("untitled.txt".into(), 0..8);
    let mut field_id = None;
    for _ in 0..3 {
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                explorer_name_row(
                    ui,
                    &theme,
                    ExplorerNameLayout::List,
                    tasty_icons::FILE,
                    &mut edit,
                    false,
                );
                field_id = ctx.memory(|m| m.focused());
            });
        }));
    }
    assert!(edit.initial_selection.is_none());
    let id = field_id.expect("the field has focus");
    let state = egui::TextEdit::load_state(&ctx, id).expect("state");
    let range = state.cursor.char_range().expect("range");
    assert_eq!(
        (
            range.primary.index.min(range.secondary.index),
            range.primary.index.max(range.secondary.index)
        ),
        (0, 8)
    );
}

#[test]
fn a_grid_field_at_the_list_edge_moves_inside_the_visible_area() {
    let r = |a: f32, b: f32| egui::Rangef::new(a, b);
    assert_eq!(inside_shift(r(10.0, 50.0), r(0.0, 100.0)), 0.0);
    assert_eq!(inside_shift(r(-20.0, 140.0), r(0.0, 300.0)), 20.0);
    assert_eq!(inside_shift(r(200.0, 360.0), r(0.0, 300.0)), -60.0);
    // 영역이 입력보다 좁으면 왼쪽 끝을 맞춘다.
    assert_eq!(inside_shift(r(-20.0, 140.0), r(0.0, 100.0)), 20.0);
    assert_eq!(inside_shift(r(30.0, 190.0), r(0.0, 100.0)), -30.0);
}

#[test]
fn a_waiting_field_is_read_only_with_a_spinner_after_it_and_sends_no_events() {
    let theme = theme();
    let (row, idle) = rects(ExplorerNameLayout::List);
    let options = ExplorerNameOptions {
        pending: Some("Waiting for build-eu"),
        ..Default::default()
    };
    let (_, field) = rects_in(ExplorerNameLayout::List, options);
    // 칸 뒤에 간격과 Spinner 자리를 남긴다.
    let slot = theme.spacing_sm.value() + theme.icon_glyph_size_sm.value();
    assert_eq!(field.left(), idle.left());
    assert!(
        (idle.right() - field.right() - slot).abs() < 0.01,
        "{idle:?} {field:?}"
    );
    assert!(field.right() + slot <= row.right());

    // 글자를 바꾸지 않고, Enter·Esc 도 이벤트로 내지 않는다.
    let ctx = egui::Context::default();
    let mut edit = ExplorerNameEdit::new("New folder".into(), 0..10);
    let mut events = Vec::new();
    let mut fills = Vec::new();
    for key in [
        None,
        Some(egui::Key::A),
        Some(egui::Key::Enter),
        Some(egui::Key::Escape),
    ] {
        let mut input = egui::RawInput::default();
        if let Some(key) = key {
            input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
            if key == egui::Key::A {
                input.events.push(egui::Event::Text("a".into()));
            }
        }
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (_, _, event) = explorer_name_row_in(
                    ui,
                    &theme,
                    ExplorerNameLayout::List,
                    options,
                    tasty_icons::FOLDER,
                    &mut edit,
                    true,
                );
                events.push(event);
            });
        });
        fills.extend(out.shapes.iter().filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) => Some(r.fill),
            _ => None,
        }));
    }
    assert_eq!(edit.buf, "New folder");
    assert!(
        events.iter().all(|e| *e == ExplorerNameEvent::None),
        "{events:?}"
    );
    assert!(fills.contains(&theme.input_readonly_bg().to_egui()));
}
