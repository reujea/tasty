//! native WebView 배치와 입력 영역. native 창은 마우스를 직접 받으므로 분할선 드래그와 창 가장자리
//! 리사이즈 입력을 가리지 않게 해야 한다. Linux는 WebView를 surface에 꽉 채우고, host 판정이
//! 분할선·리사이즈로 보는 픽셀만 input shape으로 뺀다. macOS는 같은 픽셀을 `hitTest:`에서 뺀다.
//! Windows는 아직 입력 영역을 빼지 못한다. Linux 밖에서는 pane 외곽 변에 여백도 둔다.

use crate::model::{PhysicalPx, PhysicalRect};

/// native WebView가 leaf 안에서 비워 두는 변별 여백(물리 px). 왼쪽·오른쪽·아래 순서다.
/// Linux는 입력 영역을 input shape으로 빼므로 여백이 없다.
#[cfg(target_os = "linux")]
pub fn webview_edge_inset(
    _leaf: PhysicalRect,
    _content: PhysicalRect,
    _scale_factor: f32,
) -> [PhysicalPx; 3] {
    [PhysicalPx::default(); 3]
}

/// native WebView가 leaf 안에서 비워 두는 변별 여백(물리 px). 왼쪽·오른쪽·아래 순서다.
/// pane 콘텐츠 영역 외곽에 닿는 변에는 분할선 입력 영역만큼 비워 둔다. 내부 leaf 사이에는
/// divider gap만 있고, 위쪽은 탭 바와 닿아 여백이 없다. 입력 영역을 빼는 방법이 없는 OS의 동작이다.
#[cfg(not(target_os = "linux"))]
pub fn webview_edge_inset(
    leaf: PhysicalRect,
    content: PhysicalRect,
    scale_factor: f32,
) -> [PhysicalPx; 3] {
    let inset = PhysicalPx(crate::state::mouse::divider_hit_threshold_physical(
        scale_factor,
    ));
    let on_edge = |a: PhysicalPx, b: PhysicalPx| (a - b).abs() < PhysicalPx(0.5);
    let pick = |touches: bool| {
        if touches {
            inset
        } else {
            PhysicalPx::default()
        }
    };
    [
        pick(on_edge(leaf.x, content.x)),
        pick(on_edge(leaf.x + leaf.width, content.x + content.width)),
        pick(on_edge(leaf.y + leaf.height, content.y + content.height)),
    ]
}

/// 정수 픽셀 `c` 가 `low < c < high` 인 구간 `[시작, 끝)`. 커서와 X input shape는 정수 픽셀 단위다.
fn open_pixels(low: f32, high: f32) -> (f32, f32) {
    (low.floor() + 1.0, high.ceil())
}

/// 창 좌표의 host 입력 영역(물리 px, 정수 픽셀 경계). host 판정과 같은 픽셀만 담는다.
/// - 분할선: `find_divider_at` 은 분할선 위치(틈의 앞쪽 변) `d` 에서 `|p - d| < reach` 인
///   열린 구간을 분할선으로 보고, 분할 사각형 안(`y <= p < y + 높이`)에서만 판정한다.
/// - 창 가장자리: `resize_direction_at` 은 `x <= band`, `x >= 너비 - band`(위·아래도 같다)를
///   리사이즈로 본다. 그래서 왼쪽·위 밴드는 `band + 1` 픽셀, 오른쪽·아래 밴드는 `band` 픽셀이다.
///
/// `resize_band` 가 0이면(최대화·전체화면·OS 장식) 창 가장자리 밴드를 넣지 않는다.
pub fn host_input_zones(
    dividers: &[PhysicalRect],
    window_width: PhysicalPx,
    window_height: PhysicalPx,
    resize_band: PhysicalPx,
    scale_factor: f32,
) -> Vec<PhysicalRect> {
    let reach = crate::state::mouse::divider_hit_threshold_physical(scale_factor);
    let span = |(start, end): (f32, f32)| (PhysicalPx(start), PhysicalPx((end - start).max(0.0)));
    let mut zones: Vec<PhysicalRect> = dividers
        .iter()
        .map(|d| {
            if d.width <= d.height {
                let (x, width) = span(open_pixels(d.x.value() - reach, d.x.value() + reach));
                let (y, height) = span((d.y.value().ceil(), (d.y + d.height).value().ceil()));
                PhysicalRect {
                    x,
                    y,
                    width,
                    height,
                }
            } else {
                let (y, height) = span(open_pixels(d.y.value() - reach, d.y.value() + reach));
                let (x, width) = span((d.x.value().ceil(), (d.x + d.width).value().ceil()));
                PhysicalRect {
                    x,
                    y,
                    width,
                    height,
                }
            }
        })
        .collect();
    if resize_band > PhysicalPx::default() {
        let band = resize_band.value();
        let (w, h) = (window_width.value(), window_height.value());
        let near = band.floor() + 1.0;
        let (right_x, right_w) = span(((w - band).ceil(), w));
        let (bottom_y, bottom_h) = span(((h - band).ceil(), h));
        let origin = PhysicalPx::default();
        zones.extend([
            PhysicalRect {
                x: origin,
                y: origin,
                width: PhysicalPx(near),
                height: window_height,
            },
            PhysicalRect {
                x: right_x,
                y: origin,
                width: right_w,
                height: window_height,
            },
            PhysicalRect {
                x: origin,
                y: origin,
                width: window_width,
                height: PhysicalPx(near),
            },
            PhysicalRect {
                x: origin,
                y: bottom_y,
                width: window_width,
                height: bottom_h,
            },
        ]);
    }
    zones
}

/// `webview` 와 겹치는 host 입력 영역을 WebView 창 기준 좌표로 돌려준다. 겹치지 않으면 뺀다.
pub fn webview_input_holes(webview: PhysicalRect, zones: &[PhysicalRect]) -> Vec<PhysicalRect> {
    zones
        .iter()
        .filter_map(|z| {
            let x0 = z.x.max(webview.x);
            let y0 = z.y.max(webview.y);
            let x1 = (z.x + z.width).min(webview.x + webview.width);
            let y1 = (z.y + z.height).min(webview.y + webview.height);
            (x1 > x0 && y1 > y0).then(|| PhysicalRect {
                x: x0 - webview.x,
                y: y0 - webview.y,
                width: x1 - x0,
                height: y1 - y0,
            })
        })
        .collect()
}

/// 안내 카드와 겹치는 부분을 WebView 창 기준 좌표로 돌려준다. native 창은 이 부분을 그리지도,
/// 포인터를 받지도 않아 뒤의 egui 안내가 보인다. 카드 경계는 바깥쪽 정수 픽셀로 넓혀 카드 가장자리가
/// 페이지에 덮이지 않게 한다. `webview` 는 `set_bounds` 와 같이 반올림한 창 사각형이다.
pub fn notice_clip_holes(webview: PhysicalRect, cards: &[PhysicalRect]) -> Vec<PhysicalRect> {
    let outward: Vec<PhysicalRect> = cards
        .iter()
        .map(|c| {
            let x0 = c.x.value().floor();
            let y0 = c.y.value().floor();
            PhysicalRect {
                x: PhysicalPx(x0),
                y: PhysicalPx(y0),
                width: PhysicalPx((c.x + c.width).value().ceil() - x0),
                height: PhysicalPx((c.y + c.height).value().ceil() - y0),
            }
        })
        .collect();
    webview_input_holes(webview, &outward)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(rects: &[PhysicalRect]) -> Vec<[f32; 4]> {
        rects
            .iter()
            .map(|r| [r.x.value(), r.y.value(), r.width.value(), r.height.value()])
            .collect()
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> PhysicalRect {
        PhysicalRect {
            x: PhysicalPx(x),
            y: PhysicalPx(y),
            width: PhysicalPx(width),
            height: PhysicalPx(height),
        }
    }

    /// 단일 pane: 분할선이 없고 창 오른쪽·아래 가장자리 밴드만 WebView와 겹친다.
    #[test]
    fn a_single_pane_webview_only_yields_the_window_resize_band() {
        let zones = host_input_zones(
            &[],
            PhysicalPx(1280.0),
            PhysicalPx(720.0),
            PhysicalPx(8.0),
            1.0,
        );
        let holes = webview_input_holes(rect(180.0, 61.0, 1100.0, 634.0), &zones);
        assert_eq!(values(&holes), values(&[rect(1092.0, 0.0, 8.0, 634.0)]));
    }

    /// 세로 분할: 분할선 hit 띠가 오른쪽 pane 의 왼쪽 끝을 덮고, 창 밴드는 오른쪽에만 닿는다.
    #[test]
    fn a_split_webview_yields_the_divider_band_and_the_window_edge() {
        let divider = rect(729.0, 30.0, 1.0, 665.0);
        let zones = host_input_zones(
            &[divider],
            PhysicalPx(1280.0),
            PhysicalPx(720.0),
            PhysicalPx(8.0),
            1.0,
        );
        let holes = webview_input_holes(rect(730.0, 61.0, 550.0, 634.0), &zones);
        // hit-test 는 |x - 729| < 4 라 띠는 726..=732 이고 WebView 안쪽은 730..=732 이다.
        assert_eq!(
            values(&holes),
            values(&[rect(0.0, 0.0, 3.0, 634.0), rect(542.0, 0.0, 8.0, 634.0)])
        );
    }

    /// 분할선 앞쪽(왼쪽) WebView: 띠는 host 가 분할선으로 보는 726..=728 만 뺀다. 725 는
    /// |725 - 729| = 4 라 분할선이 아니므로 페이지가 받아야 한다.
    #[test]
    fn a_webview_before_the_divider_yields_only_the_pixels_the_host_hits() {
        let divider = rect(729.0, 30.0, 1.0, 665.0);
        let zones = host_input_zones(
            &[divider],
            PhysicalPx(1280.0),
            PhysicalPx(720.0),
            PhysicalPx(8.0),
            1.0,
        );
        let holes = webview_input_holes(rect(180.0, 61.0, 549.0, 634.0), &zones);
        assert_eq!(values(&holes), values(&[rect(546.0, 0.0, 3.0, 634.0)]));
    }

    /// 창 왼쪽·위 밴드는 `x <= 8` 이라 9 픽셀, 오른쪽·아래 밴드는 `x >= 너비 - 8` 이라 8 픽셀이다.
    /// 사이드바를 숨기면 WebView 가 창 왼쪽 끝에 닿는다.
    #[test]
    fn the_window_bands_match_the_resize_hit_test() {
        let zones = host_input_zones(
            &[],
            PhysicalPx(1280.0),
            PhysicalPx(720.0),
            PhysicalPx(8.0),
            1.0,
        );
        let holes = webview_input_holes(rect(0.0, 61.0, 1280.0, 634.0), &zones);
        assert_eq!(
            values(&holes),
            values(&[rect(0.0, 0.0, 9.0, 634.0), rect(1272.0, 0.0, 8.0, 634.0)])
        );
        assert_eq!(
            values(&zones[2..]),
            values(&[rect(0.0, 0.0, 1280.0, 9.0), rect(0.0, 712.0, 1280.0, 8.0)])
        );
    }

    /// 배율 2: 분할선 띠는 논리 4px = 물리 8px 기준 열린 구간, 밴드는 물리 8px 그대로다.
    #[test]
    fn the_divider_band_scales_but_the_resize_band_stays_physical() {
        let divider = rect(1460.0, 60.0, 2.0, 1330.0);
        let zones = host_input_zones(
            &[divider],
            PhysicalPx(2560.0),
            PhysicalPx(1440.0),
            PhysicalPx(8.0),
            2.0,
        );
        // |x - 1460| < 8 → 1453..=1467
        assert_eq!(
            values(&zones[..1]),
            values(&[rect(1453.0, 60.0, 15.0, 1330.0)])
        );
        assert_eq!(
            values(&zones[2..3]),
            values(&[rect(2552.0, 0.0, 8.0, 1440.0)])
        );
    }

    /// 최대화·전체화면처럼 밴드가 0이면 창 가장자리는 WebView 입력에서 빼지 않는다.
    #[test]
    fn no_resize_band_leaves_the_window_edge_to_the_webview() {
        let zones = host_input_zones(
            &[],
            PhysicalPx(1280.0),
            PhysicalPx(720.0),
            PhysicalPx(0.0),
            1.0,
        );
        assert!(webview_input_holes(rect(180.0, 61.0, 1100.0, 634.0), &zones).is_empty());
    }

    /// 반 픽셀 분할선(배율 2 에서 논리 크기가 .5 로 끝나는 leaf): |y - 730.5| < 8 → 723..=738.
    #[test]
    fn a_fractional_divider_yields_whole_pixels() {
        let divider = rect(360.0, 730.5, 1240.0, 2.0);
        let zones = host_input_zones(
            &[divider],
            PhysicalPx(1600.0),
            PhysicalPx(1000.0),
            PhysicalPx(0.0),
            2.0,
        );
        assert_eq!(values(&zones), values(&[rect(360.0, 723.0, 1240.0, 16.0)]));
    }

    /// 띠는 host 판정(`find_divider_at`)이 분할선으로 보는 픽셀과 정확히 같다. 터미널/가운데/터미널
    /// 세로 3단(가로 분할선 둘)과 세로 분할을 배율 1·2 에서 픽셀마다 대조한다.
    #[test]
    fn the_bands_hold_exactly_the_pixels_the_divider_hit_test_hits() {
        use crate::model::{Pane, PaneNode, SplitDirection};
        let leaf = |id| {
            Box::new(PaneNode::Leaf(Pane::new_with_terminal_marker(
                id,
                id * 10,
                id * 100,
            )))
        };
        let stacked = PaneNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: leaf(1),
            second: Box::new(PaneNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: leaf(2),
                second: leaf(3),
            }),
        };
        let side_by_side = PaneNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.37,
            first: leaf(4),
            second: leaf(5),
        };
        for scale in [1.0_f32, 2.0] {
            let rect = rect(180.0 * scale, 30.0 * scale, 550.0 * scale, 437.0 * scale);
            let threshold = crate::state::mouse::divider_hit_threshold_physical(scale);
            for tree in [&stacked, &side_by_side] {
                let zones = host_input_zones(
                    &tree.collect_dividers(rect, scale),
                    PhysicalPx(2000.0),
                    PhysicalPx(2000.0),
                    PhysicalPx::default(),
                    scale,
                );
                let inside = |px: f32, py: f32| {
                    zones.iter().any(|z| {
                        px >= z.x.value()
                            && px < (z.x + z.width).value()
                            && py >= z.y.value()
                            && py < (z.y + z.height).value()
                    })
                };
                let (cx, cy) = (
                    (rect.x + rect.width * 0.5).value(),
                    (rect.y + rect.height * 0.5).value(),
                );
                for p in 0..1200 {
                    let p = p as f32;
                    for (px, py) in [(cx, p), (p, cy)] {
                        assert_eq!(
                            inside(px, py),
                            tree.find_divider_at(px, py, rect, threshold, scale)
                                .is_some(),
                            "배율 {scale} 픽셀 ({px}, {py})"
                        );
                    }
                }
            }
        }
    }

    /// 가로 분할선은 위아래로 띠를 만든다.
    #[test]
    fn a_horizontal_divider_yields_a_band_across() {
        let divider = rect(180.0, 400.0, 1100.0, 1.0);
        let zones = host_input_zones(
            &[divider],
            PhysicalPx(1280.0),
            PhysicalPx(720.0),
            PhysicalPx(0.0),
            1.0,
        );
        let holes = webview_input_holes(rect(180.0, 401.0, 1100.0, 294.0), &zones);
        assert_eq!(values(&holes), values(&[rect(0.0, 0.0, 1100.0, 3.0)]));
    }

    /// 좁은 오른쪽 pane 전체가 안내 아래에 있어도 bounds는 그대로 두고 겹친 부분만 뺀다.
    #[test]
    fn a_pane_entirely_under_the_notice_is_clipped_where_the_card_covers_it() {
        let card = rect(600.0, 40.0, 360.0, 200.0);
        let holes = notice_clip_holes(rect(700.0, 0.0, 268.0, 400.0), &[card]);
        assert_eq!(values(&holes), vec![[0.0, 40.0, 260.0, 200.0]]);
    }

    /// 카드 경계가 소수 픽셀이면 바깥쪽으로 넓힌다. 겹치지 않는 카드와 카드 사이 간격은 빼지 않는다.
    #[test]
    fn card_edges_round_outward_and_gaps_between_cards_stay_visible() {
        let cards = [
            rect(600.4, 40.6, 359.2, 99.8),
            rect(600.4, 152.6, 359.2, 80.0),
            rect(0.0, 0.0, 10.0, 10.0),
        ];
        let holes = notice_clip_holes(rect(500.0, 30.0, 400.0, 300.0), &cards);
        assert_eq!(
            values(&holes),
            vec![[100.0, 10.0, 300.0, 101.0], [100.0, 122.0, 300.0, 81.0]]
        );
    }

    /// 안내가 없으면 구멍이 없어 원래 영역으로 돌아간다.
    #[test]
    fn no_cards_means_no_clip() {
        assert!(notice_clip_holes(rect(0.0, 0.0, 800.0, 600.0), &[]).is_empty());
    }
}
