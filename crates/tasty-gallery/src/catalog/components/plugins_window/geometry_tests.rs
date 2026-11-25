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
