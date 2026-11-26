//! 플러그인 창 표본의 헤더·탭·목록 치수를 그린 도형에서 잰다. 값은 본체 `src/view/plugins/` 와 같은 토큰이다.

use super::{ListWidths, Tab, installed, window, window_with};
use crate::catalog::components::painted::{Painted, paint};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 창 하나를 그리고, 창 바탕(bg_panel 채움) 사각형과 함께 돌려준다.
fn painted_window(th: &Theme, tab: Tab) -> (Painted, egui::Rect) {
    let painted = paint(|ui| window(ui, th, tab));
    let frame = painted.rect_filled_with(th.bg_panel().to_egui());
    (painted, frame)
}

fn installed_window(th: &Theme) -> (Painted, egui::Rect) {
    painted_window(
        th,
        Tab::Installed {
            detail: installed::Detail::Selected(0),
        },
    )
}

/// 헤더 안 선택 세그먼트(surface-active 채움) 사각형.
fn selected_segment(th: &Theme, painted: &Painted, frame: egui::Rect) -> egui::Rect {
    let header = egui::Rect::from_min_size(
        frame.min,
        egui::vec2(frame.width(), th.plugins_header_height().value()),
    );
    let active = th.surface_active().to_egui();
    painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == active && header.contains_rect(r.rect) => {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("헤더 안 선택 세그먼트가 없다")
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

/// Installed 와 Attention 탭 사이는 `plugins_tab_gap` 이다. 탭마다 선택해 그려 채움 사각형으로 잰다.
#[test]
fn neighbouring_segment_tabs_are_plugins_tab_gap_apart() {
    let th = theme();
    let (installed, frame_a) = installed_window(&th);
    let first = selected_segment(&th, &installed, frame_a);
    let (attention, frame_b) = painted_window(
        &th,
        Tab::Attention {
            empty: false,
            selected: 0,
        },
    );
    let second = selected_segment(&th, &attention, frame_b);
    let gap = (second.left() - frame_b.left()) - (first.right() - frame_a.left());
    let want = th.plugins_tab_gap().value();
    assert!(
        (gap - want).abs() < 0.5,
        "Installed {first:?} 와 Attention {second:?} 사이 {gap} 가 plugins_tab_gap {want} 가 아니다"
    );
}

/// 헤더 아래 목록에서 선택된 행(surface-active 채움) 사각형.
fn selected_row(th: &Theme, painted: &Painted, frame: egui::Rect) -> egui::Rect {
    let below = frame.top() + th.plugins_header_height().value();
    let active = th.surface_active().to_egui();
    painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == active && r.rect.top() >= below => Some(r.rect),
            _ => None,
        })
        .expect("목록 선택 행이 없다")
}

/// 목록 행 간격은 Installed·Attention 모두 행 높이 + `plugins_list_row_gap` 이다. 첫 행과 둘째 행을 선택해
/// 그린 채움의 위치 차로 잰다.
#[test]
fn list_rows_step_by_the_row_height_plus_plugins_list_row_gap() {
    let th = theme();
    let want = tasty_ui_widgets::tokens::PLUGIN_LIST_ROW_HEIGHT.value()
        + th.plugins_list_row_gap().value();
    let pairs = [
        (
            Tab::Installed {
                detail: installed::Detail::Selected(0),
            },
            Tab::Installed {
                detail: installed::Detail::Selected(1),
            },
        ),
        (
            Tab::Attention {
                empty: false,
                selected: 0,
            },
            Tab::Attention {
                empty: false,
                selected: 1,
            },
        ),
    ];
    for (a, b) in pairs {
        let (pa, fa) = painted_window(&th, a);
        let (pb, fb) = painted_window(&th, b);
        let step = (selected_row(&th, &pb, fb).top() - fb.top())
            - (selected_row(&th, &pa, fa).top() - fa.top());
        assert!(
            (step - want).abs() < 0.5,
            "목록 행 간격 {step} 가 행 높이 + plugins_list_row_gap = {want} 가 아니다"
        );
    }
}

/// 목록과 상세 사이 구분선(separator 세로선)의 x.
fn divider_x(th: &Theme, painted: &Painted, frame: egui::Rect) -> f32 {
    let ink = th.separator.to_egui_premultiplied();
    painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == ink
                    && points[0].x == points[1].x
                    && points[0].x > frame.left()
                    && points[0].x < frame.right() =>
            {
                Some(points[0].x)
            }
            _ => None,
        })
        .expect("목록과 상세 사이 구분선이 없다")
}

/// Attention 탭은 목록 열을 Attention 폭으로, Installed 탭은 Installed 폭으로 잡는다. 두 토큰이 지금 같은
/// 240 이라 일부러 다른 폭을 넣어 탭별 분기를 가른다.
#[test]
fn the_attention_list_takes_the_attention_width_and_installed_takes_the_list_width() {
    let th = theme();
    let widths = ListWidths {
        installed: LogicalPx(240.0),
        attention: LogicalPx(300.0),
    };
    let cases = [
        (
            Tab::Attention {
                empty: false,
                selected: 0,
            },
            widths.attention,
        ),
        (
            Tab::Attention {
                empty: true,
                selected: 0,
            },
            widths.attention,
        ),
        (
            Tab::Installed {
                detail: installed::Detail::Selected(0),
            },
            widths.installed,
        ),
    ];
    for (tab, want) in cases {
        let want = want.value();
        let painted = paint(|ui| window_with(ui, &th, tab, widths));
        let frame = painted.rect_filled_with(th.bg_panel().to_egui());
        let list = divider_x(&th, &painted, frame) - frame.left();
        assert!(
            (list - want).abs() < 0.5,
            "목록 열 폭 {list} 가 {want} 가 아니다"
        );
        let stage = frame.width() - th.measure_md.value();
        assert!(
            (stage - want).abs() < 0.5,
            "창 폭 {} 가 목록 폭 {want} + measure_md 가 아니다",
            frame.width()
        );
    }
}

/// 탭별 폭은 본체·정본과 같은 토큰을 읽는다. 두 접근자는 지금 같은 값을 내므로 값이 아니라 이름을 고정한다.
#[test]
fn the_list_widths_read_the_installed_and_attention_width_tokens() {
    let source = include_str!("../plugins_window.rs");
    for line in [
        "installed: theme.plugins_list_width(),",
        "attention: theme.plugins_attention_list_width(),",
    ] {
        assert!(source.contains(line), "ListWidths::of 에 `{line}` 가 없다");
    }
}

/// 헤더 구분선은 보더 폭 × `plugins_header_divider_height` 다. 지원 배율에서 옛 합(space-lg + space-xs)과
/// 값이 같아 그린 도형으로 가를 수 없으므로 토큰 이름도 고정한다.
#[test]
fn the_header_divider_is_plugins_header_divider_height_tall() {
    let th = theme();
    let (painted, frame) = installed_window(&th);
    let header = egui::Rect::from_min_size(
        frame.min,
        egui::vec2(frame.width(), th.plugins_header_height().value()),
    );
    let fill = th.separator.to_egui_premultiplied();
    let divider = painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r)
                if r.fill == fill
                    && header.contains_rect(r.rect)
                    && r.rect.width() == th.border_width.value() =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("헤더 구분선이 없다");
    assert_eq!(
        divider.height(),
        th.plugins_header_divider_height().value(),
        "헤더 구분선 {divider:?} 높이가 plugins_header_divider_height 가 아니다"
    );
    let source = include_str!("../plugins_window.rs");
    assert!(
        source.contains("let div_h = theme.plugins_header_divider_height().value();"),
        "헤더 구분선 높이가 plugins_header_divider_height 를 읽지 않는다"
    );
}

/// Add 탭 본문 글자는 본체처럼 창 왼쪽에서 정본 `space-xl` 만큼 들어가 시작한다.
#[test]
fn the_add_tab_body_starts_space_xl_inside_the_window() {
    let th = theme();
    for preview in [false, true] {
        let (painted, frame) = painted_window(&th, Tab::Add { preview });
        let band = frame.top() + th.plugins_header_height().value();
        let left = painted
            .texts
            .iter()
            .filter(|t| t.rect.top() >= band && !t.text.trim().is_empty())
            .map(|t| t.rect.left())
            .min_by(f32::total_cmp)
            .expect("Add 탭 본문 글자가 없다");
        let inset = left - frame.left();
        assert!(
            (inset - th.spacing_xl.value()).abs() < 1.0,
            "Add(preview {preview}): 본문 가장 왼쪽 글자가 창 왼쪽에서 {inset} 떨어져 있다. spacing_xl {} 가 아니다",
            th.spacing_xl.value()
        );
    }
}

/// 본문 열이 `measure_xl` 보다 좁은 창에서는 경로 줄이 창 오른쪽에서도 본체처럼 정본 `space-xl` 만큼 들어가 끝난다.
/// 경로 줄 맨 오른쪽 Verify 버튼 상자의 오른쪽 끝을 잰다.
#[test]
fn the_add_tab_body_ends_space_xl_inside_a_narrow_window() {
    let th = theme();
    let width = 400.0;
    assert!(
        width - 2.0 * th.spacing_xl.value() < th.measure_xl.value(),
        "본문 열이 measure_xl 에 닿지 않을 만큼 좁은 창이어야 한다"
    );
    for verified in [false, true] {
        let mut pane = egui::Rect::NOTHING;
        let painted = paint(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 480.0), egui::Sense::hover());
            pane = rect;
            super::add::form_pane(ui, &th, rect, verified);
        });
        let verify = painted
            .texts_with(crate::i18n::t_lang("en", "plugins.add_confirm_path"))
            .into_iter()
            .map(|t| t.rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .expect("Verify 글자가 없다");
        let button = painted
            .shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r)
                    if r.rect.contains_rect(verify) && r.rect.width() < width / 2.0 =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("Verify 버튼 상자가 없다");
        let inset = pane.right() - button.right();
        assert!(
            (inset - th.spacing_xl.value()).abs() < 0.5,
            "Add(verified {verified}): Verify 버튼 상자 오른쪽 끝이 창 오른쪽에서 {inset} 떨어져 있다. spacing_xl {} 이 아니다",
            th.spacing_xl.value()
        );
    }
}
