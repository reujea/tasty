//! segmented는 정본 배치(tasty-page overlay-frames.jsx `seg`)대로 칸마다 따로 그린 알약을 6 간격으로 놓는다.
//! 선택 칸은 accent-primary 채움과 같은 색 테두리, 나머지는 surface-raised 채움과 border-default 테두리다.

use egui::{Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::segmented;

#[test]
fn each_segment_is_its_own_pill_six_apart() {
    let th = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
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
    assert_eq!(pills.len(), 3, "{pills:?}");
    let h = th.item_height_interactive.value();
    for p in &pills {
        assert_eq!(p.rect.height(), h);
        assert_eq!(p.corner_radius.nw as f32, th.corner_radius.value());
        assert_eq!(p.stroke_kind, egui::StrokeKind::Inside);
    }
    for w in pills.windows(2) {
        assert_eq!(w[1].rect.left() - w[0].rect.right(), 6.0);
    }
    let accent = th.accent_primary().to_egui();
    assert_eq!((pills[1].fill, pills[1].stroke.color), (accent, accent));
    for i in [0, 2] {
        assert_eq!(pills[i].fill, th.surface_raised().to_egui());
        assert_eq!(pills[i].stroke.color, th.border_default().to_egui());
    }
}
