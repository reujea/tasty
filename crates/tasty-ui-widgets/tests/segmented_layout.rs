//! segmented는 정본 배치(tasty-page overlay-frames.jsx `seg`)대로 칸마다 따로 그린 알약을 `segmented-gap` 간격으로 놓는다.
//! 선택 칸은 accent-primary 채움과 같은 색 테두리, 나머지는 surface-raised 채움과 border-default 테두리다.

use egui::{Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::segmented;

/// `zoom` 배율의 테마로 세 칸 묶음을 한 패스 그리고 알약 상자를 왼쪽부터 돌려준다.
fn pills_at(zoom: f32) -> (Theme, Vec<egui::epaint::RectShape>) {
    let th = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
    let ctx = egui::Context::default();
    let out = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| {
                segmented(ui, &th, &["SSH profile", "Direct (inline)", "Other"], 1);
            });
        },
    );
    let pills: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.stroke.width == th.border_width.value() => Some(r.clone()),
            _ => None,
        })
        .collect();
    (th, pills)
}

#[test]
fn each_segment_is_its_own_pill_segmented_gap_apart() {
    let (th, pills) = pills_at(1.0);
    assert_eq!(pills.len(), 3, "{pills:?}");
    let h = th.item_height_interactive.value();
    for p in &pills {
        assert_eq!(p.rect.height(), h);
        assert_eq!(p.corner_radius.nw as f32, th.corner_radius.value());
        assert_eq!(p.stroke_kind, egui::StrokeKind::Inside);
    }
    for w in pills.windows(2) {
        assert_eq!(
            w[1].rect.left() - w[0].rect.right(),
            th.segmented_gap().value()
        );
    }
    let accent = th.accent_primary().to_egui();
    assert_eq!((pills[1].fill, pills[1].stroke.color), (accent, accent));
    for i in [0, 2] {
        assert_eq!(pills[i].fill, th.surface_raised().to_egui());
        assert_eq!(pills[i].stroke.color, th.border_default().to_egui());
    }
}

/// 알약 사이 간격은 정본 `segmented-gap` 을 읽어 알약 높이·padding처럼 UI 배율을 따른다(접근자가 배율 곱을
/// 반올림한다. egui가 상자 좌표를 1/16 단위로 맞추므로 그 반 단위까지 허용한다).
#[test]
fn the_pill_gap_is_segmented_gap_at_each_zoom() {
    for zoom in [0.85, 1.0, 1.2] {
        let (th, pills) = pills_at(zoom);
        assert_eq!(pills.len(), 3, "{pills:?}");
        assert_eq!(th.item_height_interactive.value(), pills[0].rect.height());
        for w in pills.windows(2) {
            let gap = w[1].rect.left() - w[0].rect.right();
            assert!(
                (gap - th.segmented_gap().value()).abs() <= 1.0 / 32.0,
                "배율 {zoom}: 알약 사이 {gap} 가 segmented_gap {} 가 아니다",
                th.segmented_gap().value()
            );
        }
    }
}
