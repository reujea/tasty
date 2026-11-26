//! Components › One keycap 표본을 그린 도형에서 잰다. 정본은 정착안 팔레트 패널 하나만 그린다.

use super::draw_one_keycap;
use crate::catalog::components::painted::{Painted, paint};
use tasty_type_appearance::theme::Theme;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 팔레트 패널(surface-raised 채움, border-strong 외곽선) 사각형들.
fn panels(th: &Theme, painted: &Painted) -> Vec<egui::Rect> {
    let fill = th.surface_raised().to_egui();
    let edge = th.border_strong().to_egui();
    painted
        .shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == fill && r.stroke.color == edge => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// 무대(bg-panel 채움, border-default 외곽선) 사각형.
fn stage_rect(th: &Theme, painted: &Painted) -> egui::Rect {
    let fill = th.bg_panel().to_egui();
    let edge = th.border_default().to_egui();
    painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == fill && r.stroke.color == edge => Some(r.rect),
            _ => None,
        })
        .expect("무대가 없다")
}

/// 넓은 열에서는 패널 하나가 `palette_width` 폭이고, 키캡은 두 행의 공용 Kbd 다섯 개뿐이다.
#[test]
fn one_panel_is_palette_width_wide_in_a_wide_column() {
    let th = theme();
    let painted = paint(|ui| draw_one_keycap(ui, &th));
    let found = panels(&th, &painted);
    assert_eq!(found.len(), 1, "팔레트 패널이 하나가 아니다: {found:?}");
    let width = found[0].width();
    let want = th.palette_width().value();
    assert!(
        (width - want).abs() < 0.5,
        "패널 폭 {width} 가 palette_width {want} 가 아니다"
    );
    // kbd-bg 는 패널 채움(surface-raised)과 같은 색이라 패널 안쪽의 더 작은 사각형만 키캡으로 센다.
    let panel = found[0];
    let cap_fill = th.kbd_bg().to_egui();
    let caps: Vec<egui::Rect> = painted
        .shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r)
                if r.fill == cap_fill && r.rect != panel && panel.contains_rect(r.rect) =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        caps.len(),
        5,
        "Ctrl·Shift·D·Ctrl·P 키캡 다섯 개가 아니다: {caps:?}"
    );
    for cap in &caps {
        assert!(
            (cap.height() - th.kbd_size().value()).abs() < 0.5,
            "키캡 {cap:?} 높이가 kbd_size 가 아니다"
        );
    }
}

/// `palette_width` 보다 좁은 열에서는 패널이 무대 안쪽 폭을 채운다.
#[test]
fn the_panel_fills_a_column_narrower_than_palette_width() {
    let th = theme();
    let column = 316.0;
    let painted = paint(|ui| {
        ui.allocate_ui(egui::vec2(column, 2000.0), |ui| {
            ui.set_max_width(column);
            draw_one_keycap(ui, &th);
        });
    });
    let stage = stage_rect(&th, &painted);
    let inner = stage.width() - 2.0 * (th.spacing_xl + th.border_width).value();
    let found = panels(&th, &painted);
    assert_eq!(found.len(), 1, "팔레트 패널이 하나가 아니다: {found:?}");
    let width = found[0].width();
    assert!(
        width < th.palette_width().value(),
        "좁은 열에서 패널 폭 {width} 가 palette_width 보다 작지 않다"
    );
    assert!(
        (width - inner).abs() < 0.5,
        "패널 폭 {width} 가 무대 {stage:?} 안쪽 폭 {inner} 가 아니다"
    );
}

/// Meta 의 gap 행은 kbd-gap 값(배율 1에서 4)을 적는다.
#[test]
fn the_meta_gap_row_reads_the_kbd_gap_value() {
    let th = theme();
    let painted = paint(|ui| draw_one_keycap(ui, &th));
    let row = painted.text("· kbd-gap");
    let want = format!("{}px · kbd-gap", th.kbd_gap().value());
    assert_eq!(row.text, want, "Meta gap 행이 kbd-gap 값이 아니다");
    assert_eq!(want, "4px · kbd-gap");
}
