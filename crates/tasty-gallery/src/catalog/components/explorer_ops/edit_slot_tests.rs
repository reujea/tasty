//! Detail 편집 줄이 본체처럼 표가 비워 둔 행의 Name 칸에 맞춰 서는지 그린 도형으로 본다.
//! 본체는 그 칸의 시작에 글리프를 두고 칸 끝에서 `spacing_sm` 앞에서 입력 칸을 끝낸다
//! (`explorer/create.rs` 의 `Slot::Detail`, `explorer_name_row_in`).

use super::*;
use crate::catalog::components::painted::{Painted, paint};

/// `framed` 안 `width` 폭 상세 표의 Name 칸 가로 범위를 표 왼쪽 기준으로 잰다.
fn name_cell_offset(theme: &Theme, width: f32) -> egui::Rangef {
    let cell = std::cell::Cell::new(None);
    paint(|ui| {
        ui.allocate_ui(egui::vec2(width, 400.0), |ui| {
            framed(ui, theme, |ui| {
                ui.set_width(width);
                let left = ui.max_rect().left();
                detail_columns_table(ui, theme, &[()], "probe", |ui, _, _, col| {
                    if col == 0 {
                        let x = ui.max_rect().x_range();
                        cell.set(Some(egui::Rangef::new(x.min - left, x.max - left)));
                    }
                });
            });
        });
    });
    cell.get().expect("the Name cell was laid out")
}

/// 편집 줄(surface-active, 모서리 없음, 행 높이)과 그 안 입력 칸 테두리 사각형.
fn edit_rows(painted: &Painted, theme: &Theme) -> Vec<(egui::Rect, egui::Rect)> {
    let row_h = theme.table_cell_height().value();
    let rows: Vec<egui::Rect> = painted
        .shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r)
                if r.brush.is_none()
                    && r.fill == theme.surface_active().to_egui()
                    && r.corner_radius == egui::CornerRadius::ZERO
                    && (r.rect.height() - row_h).abs() < 0.5 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect();
    rows.into_iter()
        .map(|row| {
            let field = painted
                .shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Rect(r)
                        if r.stroke.width > 0.0
                            && row.contains_rect(r.rect)
                            && r.rect.left() > row.left() =>
                    {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .min_by(|a, b| a.area().total_cmp(&b.area()))
                .unwrap_or_else(|| panic!("no input edge in the edit row {row:?}"));
            (row, field)
        })
        .collect()
}

fn assert_on_name_cell(theme: &Theme, cell: egui::Rangef, row: egui::Rect, field: egui::Rect) {
    let glyph_end = cell.min + theme.icon_glyph_size_md.value() + theme.spacing_sm.value();
    assert!(
        (field.left() - row.left() - glyph_end).abs() < 0.5,
        "the field starts after the glyph at the Name cell start: field {field:?}, row {row:?}, cell {cell:?}"
    );
    assert!(
        (field.right() - row.left() - (cell.max - theme.spacing_sm.value())).abs() < 0.5,
        "the field ends spacing_sm before the Name cell end: field {field:?}, row {row:?}, cell {cell:?}"
    );
}

#[test]
fn the_new_folder_field_ends_spacing_sm_before_the_name_cell_end() {
    let theme = tasty_themes::mocha_fallback();
    let cell = name_cell_offset(&theme, CELL_W.value());
    let painted = paint(|ui| draw_name_input(ui, &theme));
    let rows = edit_rows(&painted, &theme);
    assert_eq!(rows.len(), 1, "one Detail edit row: {rows:?}");
    let (row, field) = rows[0];
    assert_on_name_cell(&theme, cell, row, field);
    // 줄 바탕은 표의 행 띠(헤더 띠와 같은 폭)를 넘지 않는다. 표는 Name 칸 하한으로 칸보다 넓게 넘친다.
    let header = painted.rect_filled_with(theme.table_header_bg().to_egui());
    assert_eq!(
        row.x_range(),
        header.x_range(),
        "the edit row spans the table band"
    );
}

#[test]
fn every_name_error_field_ends_spacing_sm_before_the_name_cell_end() {
    let theme = tasty_themes::mocha_fallback();
    let cell = name_cell_offset(&theme, CELL_W.value());
    let painted = paint(|ui| draw_name_errors(ui, &theme));
    let rows = edit_rows(&painted, &tasty_themes::mocha_fallback());
    // 네 오류 행을 Mocha·Latte 로 그린다. Mocha 줄만 surface-active 가 이 테마 값이다.
    assert_eq!(rows.len(), 4, "four Mocha error rows: {rows:?}");
    for (row, field) in rows {
        assert_on_name_cell(&theme, cell, row, field);
    }
}

#[test]
fn the_create_in_folder_field_starts_after_the_indent_at_the_name_cell_start() {
    let theme = tasty_themes::mocha_fallback();
    let cell = name_cell_offset(&theme, hidden_create::CREATE_W.value());
    let painted = paint(|ui| hidden_create::draw_create_in_folder(ui, &theme));
    let rows = edit_rows(&painted, &theme);
    assert_eq!(rows.len(), 1, "one Mocha create-in-folder row: {rows:?}");
    let (row, field) = rows[0];
    let start = cell.min
        + theme.explorer_create_indent().value()
        + theme.icon_glyph_size_md.value()
        + theme.spacing_sm.value();
    assert!(
        (field.left() - row.left() - start).abs() < 0.5,
        "field {field:?}, row {row:?}, cell {cell:?}"
    );
    assert!(
        field.right() - row.left() <= cell.max - theme.spacing_sm.value() + 0.5,
        "the field stays inside the Name cell: field {field:?}, row {row:?}, cell {cell:?}"
    );
}

/// 포커스를 가진 첫 행 편집 칸의 링이 헤더 아래선 위까지 다 보인다. 본체는 편집 줄을 표 뒤 surface ui
/// 의 자식에 그려 clip 이 surface 라 링 위 1px 가 헤더 위에 남는다. 링 도형과 그 clip, 헤더 채움 순서를 본다.
#[test]
fn a_focused_first_row_field_shows_its_whole_ring_over_the_header() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);
    let mut edit = ExplorerNameEdit::new(t("explorer.new.folder_default").into(), 0..3);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            ..Default::default()
        };
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui(egui::vec2(CELL_W.value(), 400.0), |ui| {
                    framed(ui, &theme, |ui| {
                        ui.set_width(CELL_W.value());
                        detail_with_editor(ui, &theme, &mut edit, None);
                    });
                });
            });
        });
        shapes = out.shapes;
    }
    let bw = theme.border_width.value();
    let focus = theme.input_border_focus().to_egui();
    let ring_at = shapes
        .iter()
        .position(|c| {
            matches!(&c.shape, egui::Shape::Rect(r)
                if r.stroke.color == focus && r.stroke_kind == egui::StrokeKind::Outside)
        })
        .expect("the focused field painted its ring");
    let egui::Shape::Rect(ring) = &shapes[ring_at].shape else {
        unreachable!()
    };
    let clip = shapes[ring_at].clip_rect;
    let outer = ring.rect.expand(bw);
    let ring_top = outer.top();
    assert!(
        clip.contains_rect(outer),
        "the ring's clip {clip:?} keeps all four outer lines of {outer:?}"
    );
    let header_at = shapes
        .iter()
        .position(|c| {
            matches!(&c.shape, egui::Shape::Rect(r)
                if r.fill == theme.table_header_bg().to_egui()
                    && r.rect.y_range().contains(ring_top))
        })
        .expect("the header band lies under the ring's top line");
    assert!(
        header_at < ring_at,
        "the ring ({ring_at}) is painted after the header band ({header_at})"
    );
}
