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
