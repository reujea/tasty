//! 원격 창 탭 줄과 segmented 칸의 키보드 포커스 링(정본 vendor/README.md "States via overlays", window-tab-focus.jsx,
//! remote-tab-next-to-selected-focus). Tab 키로 포커스를 옮긴 칸에 focus-ring-width border-focus 링이 하나 생긴다.
//! segmented 칸은 focus-ring-offset 바깥, 원격 탭은 설정 L1 탭과 같은 탭 안쪽 링이다. 고정 표본은 지정한 칸에만
//! 링을 그린다.

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

/// 한 패스를 그리고 `kind` 링 획(focus_ring_width) 목록을 (사각형, 반경, 색)으로 돌려준다.
fn pass_kind(
    ctx: &egui::Context,
    th: &Theme,
    events: Vec<Event>,
    kind: egui::StrokeKind,
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
                if r.stroke.width == th.focus_ring_width.value() && r.stroke_kind == kind =>
            {
                Some((r.rect, r.corner_radius.nw as f32, r.stroke.color))
            }
            _ => None,
        })
        .collect()
}

/// 바깥 링(segmented 칸).
fn pass(
    ctx: &egui::Context,
    th: &Theme,
    events: Vec<Event>,
    draw: &mut dyn FnMut(&mut egui::Ui),
) -> Vec<(Rect, f32, egui::Color32)> {
    pass_kind(ctx, th, events, egui::StrokeKind::Outside, draw)
}

/// 탭 안쪽 링(원격 탭 줄).
fn pass_inside(
    ctx: &egui::Context,
    th: &Theme,
    events: Vec<Event>,
    draw: &mut dyn FnMut(&mut egui::Ui),
) -> Vec<(Rect, f32, egui::Color32)> {
    pass_kind(ctx, th, events, egui::StrokeKind::Inside, draw)
}

/// 탭 줄 라벨.
const LABELS: [&str; 3] = ["Remote profiles", "Attach", "Passkeys"];

/// 탭 0이 열린 탭 줄을 그린다.
fn strip(ui: &mut egui::Ui, th: &Theme, focus_look: Option<usize>) {
    strip_open(ui, th, 0, focus_look);
}

/// `active` 탭이 열린 탭 줄을 그린다.
fn strip_open(ui: &mut egui::Ui, th: &Theme, active: usize, focus_look: Option<usize>) {
    let data = TabStripData {
        labels: &LABELS,
        active,
        x_range: ui.max_rect().x_range(),
    };
    match focus_look {
        Some(_) => draw_tab_strip_with_focus_look(ui, th, &data, focus_look),
        None => draw_tab_strip(ui, th, &data),
    };
}

/// 원격 탭 링의 안쪽 거리: 밑줄 2 + focus-ring-offset 1.
fn inset(th: &Theme) -> f32 {
    th.tab_indicator_width().value() + th.focus_ring_offset.value()
}

#[test]
fn tab_moves_the_inside_ring_along_the_remote_tab_strip() {
    let th = theme();
    let ctx = egui::Context::default();
    let inset = inset(&th);
    assert!(pass_inside(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None)).is_empty());
    pass_inside(&ctx, &th, vec![tab()], &mut |ui| strip(ui, &th, None));
    let first = pass_inside(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None));
    assert_eq!(first.len(), 1, "{first:?}");
    let (r0, radius, color) = first[0];
    assert_eq!(radius, 0.0, "square tab, square ring");
    assert_eq!(color, th.border_focus().to_egui());
    assert_eq!(r0.height() + inset * 2.0, 36.0, "3 in from each tab edge");
    pass_inside(&ctx, &th, vec![tab()], &mut |ui| strip(ui, &th, None));
    let second = pass_inside(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None));
    assert_eq!(second.len(), 1);
    // 두 번째 탭은 첫 탭 오른쪽에 탭 간격 2를 두고 놓인다. 링은 각 탭 안쪽이라 서로 탭 간격 + 안쪽 거리 둘만큼 떨어진다.
    assert_eq!(second[0].0.left() - r0.right(), 2.0 + inset * 2.0);
    assert_eq!(
        pass(&ctx, &th, vec![], &mut |ui| strip(ui, &th, None)),
        vec![],
        "no outside ring"
    );
}

/// `active` 탭이 열리고 `focus` 탭을 포커스 모양으로 고정한 탭 줄을 한 패스 그린다. (안쪽 링 사각형들, 탭 사각형들)을
/// 돌려준다. 탭 사각형은 링이 아니라 탭 줄이 egui에 등록한 각 탭의 응답 영역에서 읽는다.
fn strip_rects(th: &Theme, active: usize, focus: usize) -> (Vec<Rect>, Vec<Rect>) {
    let ctx = egui::Context::default();
    let mut ids = Vec::new();
    let rings = pass_inside(&ctx, th, vec![], &mut |ui| {
        ids = (0..LABELS.len())
            .map(|i| ui.id().with((i, "rt_tab")))
            .collect();
        strip_open(ui, th, active, Some(focus));
    });
    let tabs = ids
        .iter()
        .map(|id| {
            ctx.read_response(*id)
                .expect("each tab registers its rect")
                .rect
        })
        .collect();
    (rings.into_iter().map(|(r, _, _)| r).collect(), tabs)
}

/// 모든 탭의 링은 자기 탭 안쪽이다. 링 바깥 변은 탭 네 변에서 3(밑줄 2 + focus-ring-offset 1), 링 안쪽 변은
/// 정확히 5(+ 링 폭 2) 들어오고 이웃 탭 사각형과 겹치지 않는다.
#[test]
fn every_remote_tab_ring_sits_five_inside_its_own_tab() {
    let th = theme();
    let inset = inset(&th);
    let ring_w = th.focus_ring_width.value();
    for focus in 0..LABELS.len() {
        let (rings, tabs) = strip_rects(&th, 0, focus);
        assert_eq!(rings.len(), 1, "tab {focus}: {rings:?}");
        let ring = rings[0];
        let tab = tabs[focus];
        assert_eq!(ring, tab.shrink(inset), "outer edge, tab {focus}");
        assert_eq!(
            ring.shrink(ring_w),
            tab.shrink(5.0),
            "inner edge 5 in, tab {focus}"
        );
        for (j, other) in tabs.iter().enumerate() {
            if j != focus {
                assert!(!ring.intersects(*other), "ring {focus} touches tab {j}");
            }
        }
    }
}

/// 같은 탭을 선택 상태와 비선택 상태로 각각 그려도 링 사각형은 같다(정본: 선택이 옮겨도 링 위치가 같다).
#[test]
fn the_remote_tab_ring_keeps_its_place_when_the_selection_moves() {
    let th = theme();
    for focus in 0..LABELS.len() {
        let other = (focus + 1) % LABELS.len();
        let (selected, tabs_a) = strip_rects(&th, focus, focus);
        let (unselected, tabs_b) = strip_rects(&th, other, focus);
        assert_eq!(tabs_a, tabs_b, "the tabs stay put");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected, unselected, "tab {focus} selected vs not");
    }
}

#[test]
fn the_remote_tab_ring_is_painted_after_the_underlines() {
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
    pass_inside(&ctx, &th, vec![tab()], &mut |ui| strip(ui, &th, Some(2)));
    let rings = pass_inside(&ctx, &th, vec![], &mut |ui| strip(ui, &th, Some(2)));
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
