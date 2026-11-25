//! 플러그인 창 표본의 헤더·탭·목록 치수를 그린 도형에서 잰다. 값은 본체 `src/view/plugins/` 와 같은 토큰이다.

use super::{Tab, installed, window};
use crate::catalog::components::painted::{Painted, paint};
use tasty_type_appearance::theme::Theme;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// Installed 창 하나를 그리고, 창 바탕(bg_panel 채움) 사각형과 함께 돌려준다.
fn installed_window(th: &Theme) -> (Painted, egui::Rect) {
    let painted = paint(|ui| {
        window(
            ui,
            th,
            Tab::Installed {
                detail: installed::Detail::Selected(0),
            },
        );
    });
    let frame = painted.rect_filled_with(th.bg_panel().to_egui());
    (painted, frame)
}

/// 헤더 글리프는 창 왼쪽에서 `plugins_header_pad_left` + 보더, 닫기 칸은 창 오른쪽에서
/// `plugins_header_pad_right` + 보더 안쪽에 선다(정본 padding 은 1px 보더 안쪽 값이다).
#[test]
fn the_header_pads_read_the_plugins_header_pad_tokens() {
    let th = theme();
    let (painted, frame) = installed_window(&th);
    let header = egui::Rect::from_min_size(
        frame.min,
        egui::vec2(frame.width(), th.plugins_header_height().value()),
    );
    let glyphs: Vec<egui::Rect> = painted
        .glyphs
        .iter()
        .map(|(r, _)| *r)
        .filter(|r| header.contains_rect(*r))
        .collect();
    let first = glyphs
        .iter()
        .copied()
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("헤더 글리프가 없다");
    let last = glyphs
        .iter()
        .copied()
        .max_by(|a, b| a.right().total_cmp(&b.right()))
        .expect("헤더 글리프가 없다");
    let left = (th.plugins_header_pad_left() + th.border_width).value();
    let right = (th.plugins_header_pad_right() + th.border_width).value();
    assert!(
        (first.left() - frame.left() - left).abs() < 0.5,
        "plug 글리프 {first:?} 가 창 {frame:?} 왼쪽에서 {left} 가 아니다"
    );
    let close_right = last.center().x + th.item_height_interactive.value() * 0.5;
    assert!(
        (frame.right() - close_right - right).abs() < 0.5,
        "닫기 칸 오른쪽 {close_right} 가 창 {frame:?} 오른쪽에서 {right} 가 아니다"
    );
}

/// 헤더 밴드 높이는 `plugins_header_height` 다. 밴드 아래 separator 선이 창 위에서 그만큼 아래에 있다.
#[test]
fn the_header_band_is_plugins_header_height_tall() {
    let th = theme();
    let (painted, frame) = installed_window(&th);
    let sep = th.separator.to_egui_premultiplied();
    let line = painted
        .shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == sep && (points[0].y - points[1].y).abs() < 0.01 =>
            {
                Some(points[0].y)
            }
            _ => None,
        })
        .min_by(f32::total_cmp)
        .expect("헤더 아래 separator 선이 없다");
    // 선은 밴드 아래 변에서 반 픽셀 위에 긋는다(header 의 `rect.bottom() - 0.5`).
    let bottom = line + 0.5;
    let want = th.plugins_header_height().value();
    assert!(
        (bottom - frame.top() - want).abs() < 0.5,
        "헤더 아래 {bottom} 가 창 위 {} 에서 plugins_header_height {want} 가 아니다",
        frame.top()
    );
}

/// 선택된 세그먼트 탭(surface-active 채움)의 높이는 `plugins_tab_height` 이고 헤더 밴드 가운데에 있다.
#[test]
fn the_segment_tab_is_plugins_tab_height_tall() {
    let th = theme();
    let (painted, frame) = installed_window(&th);
    let header = egui::Rect::from_min_size(
        frame.min,
        egui::vec2(frame.width(), th.plugins_header_height().value()),
    );
    let active = th.surface_active().to_egui();
    let tab = painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == active && header.contains_rect(r.rect) => {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("헤더 안 선택 세그먼트가 없다");
    let want = th.plugins_tab_height().value();
    assert!(
        (tab.height() - want).abs() < 0.5 && (tab.center().y - header.center().y).abs() < 0.5,
        "선택 세그먼트 {tab:?} 가 헤더 {header:?} 가운데의 높이 {want} 가 아니다"
    );
}
