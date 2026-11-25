//! 원격 창 탭 줄과 segmented 칸의 키보드 포커스 링(정본 vendor/README.md "States via overlays", window-tab-focus.jsx).
//! Tab 키로 포커스를 옮긴 칸에 focus-ring-offset 띄운 focus-ring-width border-focus 링이 하나 생긴다. 고정 표본은
//! 지정한 칸에만 링을 그린다.

use egui::{Event, Modifiers, Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    TabStripData, draw_tab_strip, draw_tab_strip_with_focus_look, segment_row, segmented,
};

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

fn tab() -> Event {
    Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

/// 한 패스를 그리고 링 획(focus_ring_width Outside) 목록을 (사각형, 반경, 색)으로 돌려준다.
fn pass(
    ctx: &egui::Context,
    th: &Theme,
    events: Vec<Event>,
    draw: &mut dyn FnMut(&mut egui::Ui),
) -> Vec<(Rect, f32, egui::Color32)> {
    let out = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 200.0))),
            focused: true,
            events,
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| draw(ui));
        },
    );
    out.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.stroke.width == th.focus_ring_width.value()
                    && r.stroke_kind == egui::StrokeKind::Outside =>
            {
                Some((r.rect, r.corner_radius.nw as f32, r.stroke.color))
            }
            _ => None,
        })
        .collect()
}

/// 탭 줄을 그린다. 탭 사각형은 바 위쪽에서 시작하는 높이 36 칸이라 링으로 거꾸로 확인한다.
fn strip(ui: &mut egui::Ui, th: &Theme, focus_look: Option<usize>) {
    let data = TabStripData {
        labels: &["Remote profiles", "Attach", "Passkeys"],
        active: 0,
        x_range: ui.max_rect().x_range(),
    };
    match focus_look {
        Some(_) => draw_tab_strip_with_focus_look(ui, th, &data, focus_look),
        None => draw_tab_strip(ui, th, &data),
    };
}

#[test]
fn tab_moves_the_ring_along_the_remote_tab_strip() {
    let th = theme();
    let ctx = egui::Context::default();
    let gap = th.focus_ring_offset.value();
    assert!(pass(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None)).is_empty());
    pass(&ctx, &th, vec![tab()], &mut |ui| strip(ui, &th, None));
    let first = pass(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None));
    assert_eq!(first.len(), 1, "{first:?}");
    let (r0, radius, color) = first[0];
    assert_eq!(radius, gap, "square tab corner grows by the gap");
    assert_eq!(color, th.border_focus().to_egui());
    assert_eq!(r0.height() - gap * 2.0, 36.0);
    pass(&ctx, &th, vec![tab()], &mut |ui| strip(ui, &th, None));
    let second = pass(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None));
    assert_eq!(second.len(), 1);
    // 두 번째 탭은 첫 탭 오른쪽에 탭 간격 2를 두고 놓인다. 링끼리는 그 간격에서 링 두께만큼 겹친다.
    assert_eq!(second[0].0.left() - r0.right(), 2.0 - gap * 2.0);
}

#[test]
fn the_ring_on_a_remote_tab_sits_over_its_neighbours() {
    let th = theme();
    let ctx = egui::Context::default();
    let out = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 200.0))),
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| strip(ui, &th, Some(1)));
        },
    );
    let last = out.shapes.iter().rposition(|s| {
        matches!(&s.shape, egui::Shape::Rect(r) if r.stroke.width == th.focus_ring_width.value())
    });
    let underline = out.shapes.iter().rposition(|s| {
        matches!(&s.shape, egui::Shape::LineSegment { stroke, .. } if stroke.width == th.tab_indicator_width().value())
    });
    assert!(underline.is_some(), "the active underline is drawn");
    assert!(last > underline, "ring painted after the active underline");
}

#[test]
fn a_pinned_tab_strip_rings_only_the_named_tab() {
    let th = theme();
    let ctx = egui::Context::default();
    pass(&ctx, &th, vec![tab()], &mut |ui| strip(ui, &th, Some(2)));
    let rings = pass(&ctx, &th, vec![], &mut |ui| strip(ui, &th, Some(2)));
    assert_eq!(rings.len(), 1, "real focus on tab 0 draws no second ring");
    assert!(rings[0].0.left() > 200.0, "{rings:?}");
}

#[test]
fn tab_rings_the_focused_segment_one_offset_off_its_pill() {
    let th = theme();
    let ctx = egui::Context::default();
    let mut draw = |ui: &mut egui::Ui| {
        segmented(ui, &th, &["SSH profile", "Direct (inline)"], 0);
    };
    assert!(pass(&ctx, &th, vec![], &mut draw).is_empty());
    pass(&ctx, &th, vec![tab()], &mut draw);
    pass(&ctx, &th, vec![tab()], &mut draw);
    let rings = pass(&ctx, &th, vec![], &mut draw);
    let gap = th.focus_ring_offset.value();
    assert_eq!(rings.len(), 1, "{rings:?}");
    assert_eq!(rings[0].1, th.corner_radius.value() + gap);
    assert_eq!(rings[0].2, th.border_focus().to_egui());
    assert_eq!(
        rings[0].0.height(),
        th.item_height_interactive.value() + gap * 2.0
    );
}

#[test]
fn a_pinned_segment_row_rings_only_the_named_segment() {
    let th = theme();
    let ctx = egui::Context::default();
    let rings = pass(&ctx, &th, vec![], &mut |ui| {
        segment_row(
            ui,
            &th,
            &["SSH profile", "Direct (inline)"],
            0,
            egui::Sense::hover(),
            Some(1),
        );
    });
    assert_eq!(rings.len(), 1);
    assert!(rings[0].0.left() > 60.0, "{rings:?}");
}
