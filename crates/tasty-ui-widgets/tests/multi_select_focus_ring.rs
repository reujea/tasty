//! MultiSelect 트리거는 열렸거나 키보드 포커스가 있으면 포커스 테두리와 그 바로 바깥에 붙인 ring을 그린다
//! (정본 MultiSelect.jsx `:focus-visible`·open). 클릭으로 연 바로 그 패스에도 모양이 맞고, 바깥 클릭으로 닫은
//! 패스에는 ring이 남지 않는다.

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{MultiSelectLabels, multi_select};

const OPTIONS: &[&str] = &["Waiting", "Ready"];
const WIDTH: f32 = 180.0;
const LABELS: MultiSelectLabels<'static> = MultiSelectLabels {
    none: "No status",
    some: "{} selected",
    some_one: "{} selected",
    all: "All statuses",
};

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

fn click(p: Pos2) -> Vec<Event> {
    let press = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    };
    vec![Event::PointerMoved(p), press(true), press(false)]
}

/// 한 패스를 그리고 트리거 사각형과, 그 사각형에 붙인 ring(border_width Outside 획)의 색을 돌려준다.
fn frame(ctx: &egui::Context, th: &Theme, events: Vec<Event>) -> (Rect, Vec<egui::Color32>) {
    let mut selected = [false, false];
    let mut trigger = Rect::NOTHING;
    let out = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            focused: true,
            events,
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| {
                trigger = Rect::from_min_size(
                    ui.cursor().min,
                    vec2(WIDTH, th.multiselect_height().value()),
                );
                multi_select(
                    ui,
                    th,
                    "ring",
                    &mut selected,
                    OPTIONS,
                    None,
                    &LABELS,
                    None,
                    WIDTH,
                    true,
                );
            });
        },
    );
    let rings = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.rect.min == trigger.min
                    && r.stroke_kind == egui::StrokeKind::Outside
                    && r.stroke.width == th.border_width.value() =>
            {
                Some(r.stroke.color)
            }
            _ => None,
        })
        .collect();
    (trigger, rings)
}

#[test]
fn keyboard_focus_on_the_closed_trigger_draws_the_flush_ring() {
    let th = theme();
    let ctx = egui::Context::default();
    let (_, rest) = frame(&ctx, &th, vec![]);
    assert!(rest.is_empty(), "no ring at rest");
    let tab = Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    frame(&ctx, &th, vec![tab]);
    let (_, focused) = frame(&ctx, &th, vec![]);
    assert_eq!(focused, [th.select_border_focus().to_egui()]);
}

#[test]
fn the_click_that_opens_the_menu_draws_the_ring_in_the_same_pass() {
    let th = theme();
    let ctx = egui::Context::default();
    let (trigger, _) = frame(&ctx, &th, vec![]);
    let (_, opened) = frame(&ctx, &th, click(trigger.center()));
    assert_eq!(opened, [th.select_border_focus().to_egui()]);
}

#[test]
fn the_outside_click_that_closes_the_menu_leaves_no_ring() {
    let th = theme();
    let ctx = egui::Context::default();
    let (trigger, _) = frame(&ctx, &th, vec![]);
    frame(&ctx, &th, click(trigger.center()));
    frame(&ctx, &th, vec![]);
    let (_, closed) = frame(&ctx, &th, click(pos2(390.0, 390.0)));
    assert!(closed.is_empty(), "{closed:?}");
}
