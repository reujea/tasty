//! 선택 행 띠가 표 영역의 좌우 끝을 넘어 보이지 않는지, 색을 지정하지 않은 라벨이 행에서는
//! table-row-fg, hover 행에서는 text-primary, 선택 행에서는 table-row-fg-selected 로 그려지는지 검사한다. 계약은 docs/architecture/ui-widgets-crate.md 를 따른다.

use std::cell::RefCell;

use egui::{Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Table, TableAlign, TableColumn, TableColumnWidth, in_selected_table_row, table_cell_ink,
};

const ROWS: &[&str] = &["alpha", "bravo", "charlie"];
const SELECTED: &str = "bravo";

fn raw() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0))),
        ..Default::default()
    }
}

fn columns() -> Vec<TableColumn<'static, ()>> {
    vec![
        TableColumn {
            title: "Name",
            width: TableColumnWidth::Exact(LogicalPx(120.0)),
            align: TableAlign::Left,
            sort_id: None,
        },
        TableColumn {
            title: "Kind",
            width: TableColumnWidth::Remainder {
                at_least: LogicalPx(80.0),
                clip: true,
            },
            align: TableAlign::Left,
            sort_id: None,
        },
    ]
}

/// 두 프레임을 그려 마지막 프레임의 도형, 표를 둔 영역, 선택 행 셀의 기본 글자색을 돌려준다.
fn render(theme: &Theme) -> (Vec<egui::epaint::ClippedShape>, Rect, Vec<egui::Color32>) {
    let ctx = egui::Context::default();
    tasty_egui_theme::apply_theme_to_egui(theme, &ctx);
    let area = RefCell::new(Rect::NOTHING);
    let selected_ink = RefCell::new(Vec::new());
    let mut shapes = Vec::new();
    for _ in 0..2 {
        selected_ink.borrow_mut().clear();
        let out = ctx.run(raw(), |c| {
            egui::CentralPanel::default().show(c, |ui| {
                *area.borrow_mut() = ui.available_rect_before_wrap();
                Table::new(columns()).selectable(true).show(
                    ui,
                    theme,
                    ROWS,
                    |row: &&str| *row == SELECTED,
                    |ui, _th, row: &&str, _col| {
                        if *row == SELECTED {
                            selected_ink.borrow_mut().push(ui.visuals().text_color());
                        }
                        ui.label(*row);
                    },
                );
            });
        });
        shapes = out.shapes;
    }
    (shapes, area.into_inner(), selected_ink.into_inner())
}

/// egui_extras 는 띠를 가로로 item_spacing.x 의 절반씩 넓힌다. 보이는 부분(띠 ∩ clip)은 표 영역 안이어야 한다.
#[test]
fn the_selected_band_stays_inside_the_table_area() {
    let theme = tasty_themes::mocha_fallback();
    let fill: egui::Color32 = theme.table_row_bg_selected().into();
    let (shapes, area, _) = render(&theme);
    let visible: Vec<Rect> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Rect(r) if r.fill == fill => Some(r.rect.intersect(c.clip_rect)),
            _ => None,
        })
        .collect();
    assert!(!visible.is_empty(), "no selected band was painted");
    let left = visible
        .iter()
        .map(|r| r.left())
        .fold(f32::INFINITY, f32::min);
    let right = visible
        .iter()
        .map(|r| r.right())
        .fold(f32::NEG_INFINITY, f32::max);
    assert_eq!(
        (left, right),
        (area.left(), area.right()),
        "visible band {visible:?} vs table area {area:?}"
    );
}

#[test]
fn an_uncoloured_label_in_the_selected_row_uses_the_selected_ink() {
    let theme = tasty_themes::mocha_fallback();
    let (_, _, ink) = render(&theme);
    let expected = theme.table_row_fg_selected().to_egui();
    assert!(
        !ink.is_empty() && ink.iter().all(|c| *c == expected),
        "selected row ink {ink:?}, expected table-row-fg-selected {expected:?}"
    );
}

/// 글자색을 명시하는 셀은 선택 행에서만 선택 행 글자색을 받고, 표 밖에는 그 표시가 남지 않는다.
#[test]
fn an_explicit_cell_ink_follows_only_the_selected_row() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = egui::Context::default();
    let rest = theme.text_muted();
    let seen = RefCell::new(Vec::new());
    let after = RefCell::new(None);
    let _out = ctx.run(raw(), |c| {
        egui::CentralPanel::default().show(c, |ui| {
            Table::new(columns()).selectable(true).show(
                ui,
                &theme,
                ROWS,
                |row: &&str| *row == SELECTED,
                |ui, th, row: &&str, _col| {
                    seen.borrow_mut().push((
                        *row,
                        in_selected_table_row(ui),
                        table_cell_ink(ui, th, rest),
                    ));
                },
            );
            *after.borrow_mut() = Some(in_selected_table_row(ui));
        });
    });
    let seen = seen.into_inner();
    assert!(!seen.is_empty());
    for (row, selected, ink) in seen {
        assert_eq!(selected, row == SELECTED, "row {row}");
        let expected = if row == SELECTED {
            theme.table_row_fg_selected().to_egui()
        } else {
            rest.to_egui()
        };
        assert_eq!(ink, expected, "row {row}");
    }
    assert_eq!(after.into_inner(), Some(false));
}

/// 포인터를 둔 채 여러 프레임을 그려, 마지막 프레임에서 행별 셀의 기본 글자색과 셀 영역을 돌려준다.
fn render_hovering(
    theme: &Theme,
    pointer: Option<Pos2>,
) -> std::collections::HashMap<&'static str, (egui::Color32, Rect)> {
    let ctx = egui::Context::default();
    tasty_egui_theme::apply_theme_to_egui(theme, &ctx);
    let cells = RefCell::new(std::collections::HashMap::new());
    for _ in 0..3 {
        cells.borrow_mut().clear();
        let mut input = raw();
        if let Some(p) = pointer {
            input.events.push(egui::Event::PointerMoved(p));
        }
        let _out = ctx.run(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                Table::new(columns()).selectable(true).show(
                    ui,
                    theme,
                    ROWS,
                    |row: &&str| *row == SELECTED,
                    |ui, _th, row: &&str, col| {
                        if col == 0 {
                            cells
                                .borrow_mut()
                                .insert(*row, (ui.visuals().text_color(), ui.max_rect()));
                        }
                        ui.label(*row);
                    },
                );
            });
        });
    }
    cells.into_inner()
}

/// 시안의 행 글자색은 table-row-fg 이고, hover 행에서 text-primary 가 된다.
#[test]
fn an_uncoloured_label_uses_table_row_fg_and_the_raised_band_when_hovered() {
    let theme = tasty_themes::mocha_fallback();
    let primary = theme.text_primary().to_egui();
    let row_fg = theme.table_row_fg().to_egui();
    assert_ne!(
        row_fg, primary,
        "the two inks must differ for the check to mean anything"
    );
    let idle = render_hovering(&theme, None);
    assert_eq!(idle["alpha"].0, row_fg, "rest row ink");
    let target = idle["alpha"].1.center();
    let hovered = render_hovering(&theme, Some(target));
    assert_eq!(
        hovered["alpha"].0,
        theme.text_secondary_raised().to_egui(),
        "hovered row ink"
    );
    assert_eq!(
        hovered["charlie"].0, row_fg,
        "a row the pointer is not on keeps the row ink"
    );
}
