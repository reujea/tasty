//! 포트·원격 표본에서 이 갤러리가 본체와 같은 역할 토큰으로 옮긴 치수를 그린 도형에서 잰다.

use super::painted::{Painted, paint};
use tasty_type_appearance::theme::Theme;

/// 공용 Spinner 가 트랙 원에 곱하는 저대비 alpha(tasty-ui-widgets spinner.rs 의 TRACK_ALPHA, 정본
/// `.tasty-spinner__track { opacity: 0.22 }`). 그린 도형 가운데 트랙 원을 고르는 데만 쓴다.
const SPINNER_TRACK_ALPHA: f32 = 0.22;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 창 프레임(bg_panel 채움 + border_strong 테두리) 사각형. 정본처럼 테두리를 포함한 바깥 크기다.
fn window_frame(th: &Theme, painted: &Painted) -> egui::Rect {
    let fill = th.bg_panel().to_egui();
    let edge = th.border_strong().to_egui();
    painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == fill && r.stroke.color == edge => Some(r.rect),
            _ => None,
        })
        .expect("창 프레임이 없다")
}

fn assert_inset(what: &str, left: f32, inner_left: f32, pad: f32) {
    let inset = left - inner_left;
    assert!(
        (inset - pad).abs() < 0.5,
        "{what}: 창 안쪽 왼쪽에서 {inset} 떨어져 있다. ports_pad_x {pad} 가 아니다"
    );
}

/// 포트 표본의 헤더·필터·푸터 줄은 본체처럼 창 안쪽 왼쪽에서 `ports_pad_x` 만큼 들어가 시작한다.
#[test]
fn the_ports_sample_rows_start_ports_pad_x_inside_the_window() {
    let th = theme();
    let painted = paint(|ui| super::port_scanner::draw(ui, &th));
    let frame = window_frame(&th, &painted);
    let inner_left = frame.left() + th.border_width.value();
    let pad = th.ports_pad_x().value();

    // 헤더: 첫 칸은 PORT 글리프다. 헤더 줄 높이 안의 가장 왼쪽 글리프로 잰다.
    let header_band = egui::Rect::from_min_size(
        frame.min,
        egui::vec2(frame.width(), th.item_height_interactive.value() * 2.0),
    );
    let glyph = painted
        .glyphs
        .iter()
        .map(|(r, _)| *r)
        .filter(|r| header_band.contains_rect(*r))
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("헤더 글리프가 없다");
    assert_inset("헤더 PORT 글리프", glyph.left(), inner_left, pad);

    // 필터 줄: 첫 칸은 accent 로 채운 glyph 크기 정사각 체크 상자다(헤더의 accent 태그와 가른다).
    let accent = th.accent_primary().to_egui();
    let side = th.icon_glyph_size_md.value();
    let check = painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r)
                if r.fill == accent
                    && frame.contains_rect(r.rect)
                    && (r.rect.width() - side).abs() < 0.5
                    && (r.rect.height() - side).abs() < 0.5 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("필터 줄 체크 상자가 없다");
    assert_inset("필터 체크 상자", check.left(), inner_left, pad);

    // 푸터: 첫 칸은 행 수 문구다.
    let footer = painted.text("5 of 5 ports");
    assert_inset("푸터 행 수", footer.rect.left(), inner_left, pad);
}

/// 원격 표본의 탐지 중 Spinner 는 본체처럼 `remote_detect_spinner_size` 크기다. Spinner 는 viewBox 24 에
/// 반지름 10 인 트랙 원을 그리므로 트랙 원 반지름의 24/10 배가 Spinner 한 변이다.
#[test]
fn the_remote_sample_detect_spinner_is_remote_detect_spinner_size() {
    let th = theme();
    let painted = paint(|ui| super::remote::draw(ui, &th));
    let track = th
        .spinner_indicator()
        .to_egui()
        .gamma_multiply(SPINNER_TRACK_ALPHA);
    let radius = painted
        .shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Circle(c) if c.stroke.color == track => Some(c.radius),
            _ => None,
        })
        .expect("탐지 중 Spinner 트랙이 없다");
    let size = radius * 24.0 / 10.0;
    let want = th.remote_detect_spinner_size().value();
    assert!(
        (size - want).abs() < 0.25,
        "Spinner 한 변 {size} 가 remote_detect_spinner_size {want} 가 아니다"
    );
}

/// 포트 표본의 헤더 줄은 본체처럼 위아래로 `spacing_md` 를 둔다. 헤더 줄에서 가장 높은 필터 입력 칸이 창 안쪽
/// 위와 헤더 아래 구분선에서 각각 `spacing_md` 떨어져 있다.
#[test]
fn the_ports_sample_header_pads_its_row_by_space_md() {
    let th = theme();
    let painted = paint(|ui| super::port_scanner::draw(ui, &th));
    let frame = window_frame(&th, &painted);
    let inner_top = frame.top() + th.border_width.value();
    let label = painted
        .text(crate::i18n::t("port_scanner.search_placeholder"))
        .rect;
    let field = painted
        .shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r)
                if r.brush.is_none()
                    && r.rect.contains_rect(label)
                    && r.rect.height() < frame.height() / 2.0 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("헤더 필터 입력 칸이 없다");
    let pad = th.spacing_md.value();
    let above = field.top() - inner_top;
    assert!(
        (above - pad).abs() < 0.5,
        "헤더 필터 칸 {field:?} 이 창 안쪽 위에서 {above} 떨어져 있다. spacing_md {pad} 가 아니다"
    );
    let separator = painted
        .shapes
        .iter()
        .filter_map(|s| match s {
            // 선은 굵기 가운데 y 에 그린다. 선의 위 변까지 잰다.
            egui::Shape::LineSegment { points, stroke }
                if (points[0].y - points[1].y).abs() < 0.01 && points[0].y >= field.bottom() =>
            {
                Some(points[0].y - stroke.width / 2.0)
            }
            egui::Shape::Rect(r)
                if r.rect.height() <= th.border_width.value()
                    && r.rect.width() > frame.width() / 2.0
                    && r.rect.top() >= field.bottom() =>
            {
                Some(r.rect.top())
            }
            _ => None,
        })
        .min_by(f32::total_cmp)
        .expect("헤더 아래 구분선이 없다");
    let below = separator - field.bottom();
    assert!(
        (below - pad).abs() < 0.5,
        "헤더 필터 칸 {field:?} 아래와 구분선 {separator} 사이 {below} 가 spacing_md {pad} 가 아니다"
    );
}
