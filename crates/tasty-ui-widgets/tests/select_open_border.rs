//! 목록이 열린 Select 트리거는 MultiSelect 처럼 select-border-focus 테두리를 쓰고, 시안 CSS의
//! focus box-shadow처럼 바깥에 같은 색 ring을 그린다. 닫힌 트리거는 select-border이고 ring이 없다.
//! 실제 egui 프레임에서 트리거를 클릭해 연다.

// 이유: 테스트에서는 사용하지 않는 반환값을 버리는 것을 허용한다.
#![allow(clippy::let_underscore_must_use)]
use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::select;

/// CentralPanel 안쪽 여백만큼 들어간 트리거 위치. 클릭 좌표가 트리거 안에 들어가도록 넉넉한 안쪽 점을 쓴다.
const INSIDE: Pos2 = pos2(60.0, 20.0);
const WIDTH: f32 = 160.0;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 마지막 프레임의 트리거 상자 테두리 색과, 트리거를 테두리 굵기만큼 넓힌 ring의 색들.
struct Trigger {
    border: egui::Color32,
    rings: Vec<egui::Color32>,
}

/// 프레임마다 입력 이벤트를 넣고 마지막 프레임에서 트리거 상자와 ring을 읽는다.
/// CentralPanel 에 둔다(고정 Area 는 클릭을 위젯까지 넘기지 않았다).
fn trigger(theme: &Theme, frames: &[Vec<Event>]) -> Trigger {
    let ctx = egui::Context::default();
    let mut out = None;
    let mut origin = Pos2::ZERO;
    for events in frames {
        out = Some(ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0))),
                focused: true,
                events: events.clone(),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    origin = ui.cursor().min;
                    let mut sel = 0;
                    select(ui, theme, "t", &mut sel, &["Ctrl", "Alt"], WIDTH, true);
                });
            },
        ));
    }
    let out = out.expect("한 프레임 이상 돌렸다");
    let trigger = Rect::from_min_size(origin, vec2(WIDTH, theme.select_height().value()));
    let strokes: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.rect == trigger => Some(r.stroke.color),
            _ => None,
        })
        .collect();
    assert_eq!(
        strokes.len(),
        1,
        "트리거 상자를 하나만 찾아야 한다: {strokes:?}"
    );
    let ring = trigger.expand(theme.border_width.value());
    let rings = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.rect == ring => Some(r.stroke.color),
            _ => None,
        })
        .collect();
    Trigger {
        border: strokes[0],
        rings,
    }
}

fn click_frames(at: Pos2) -> Vec<Vec<Event>> {
    let press = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    vec![
        vec![],
        vec![Event::PointerMoved(at), press(true), press(false)],
        // 클릭으로 열린 상태를 다음 프레임의 트리거가 읽는다. 포인터는 트리거 밖으로 뺀다.
        vec![Event::PointerMoved(pos2(500.0, 380.0))],
    ]
}

#[test]
fn an_open_select_paints_the_focus_border() {
    let th = theme();
    assert_ne!(
        th.select_border_focus(),
        th.select_border(),
        "두 색이 같으면 이 테스트는 아무것도 가르지 못한다"
    );
    let open = trigger(&th, &click_frames(INSIDE));
    assert_eq!(
        open.border,
        th.select_border_focus().to_egui(),
        "목록이 열린 트리거가 select-border-focus 가 아니다"
    );
    assert_eq!(
        open.rings,
        [th.select_border_focus().to_egui()],
        "목록이 열린 트리거 바깥에 focus ring 하나"
    );
}

#[test]
fn a_closed_select_paints_the_resting_border() {
    let th = theme();
    let closed = trigger(
        &th,
        &[vec![], vec![Event::PointerMoved(pos2(500.0, 380.0))]],
    );
    assert!(closed.rings.is_empty(), "닫힌 트리거에는 ring이 없다");
    assert_eq!(
        closed.border,
        th.select_border().to_egui(),
        "닫힌 트리거가 select-border 가 아니다"
    );
}

/// 갤러리 고정 트리거는 상태마다 본체 Select와 같은 테두리·ring을 그리고, disabled면 모양을 무시한다.
#[test]
fn a_pinned_trigger_paints_the_same_borders_as_the_live_select() {
    use tasty_type_geometry::length::LogicalPx;
    use tasty_ui_widgets::{SelectLook, select_pinned};
    let th = theme();
    let pinned = |look, enabled| {
        let ctx = egui::Context::default();
        let mut rect = Rect::NOTHING;
        let out = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0))),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    rect = select_pinned(
                        ui,
                        &th,
                        tasty_ui_widgets::Ground::Panel,
                        Some("Tasty rc"),
                        "Select a profile…",
                        LogicalPx(WIDTH),
                        look,
                        enabled,
                    );
                });
            },
        );
        let ring = rect.expand(th.border_width.value());
        let mut border = Vec::new();
        let mut rings = Vec::new();
        for s in &out.shapes {
            if let egui::Shape::Rect(r) = &s.shape {
                if r.rect == rect {
                    border.push(r.stroke.color);
                } else if r.rect == ring {
                    rings.push(r.stroke.color);
                }
            }
        }
        (border, rings)
    };
    let focus = th.select_border_focus().to_egui();
    assert_eq!(
        pinned(SelectLook::Normal, true),
        (vec![th.select_border().to_egui()], vec![])
    );
    assert_eq!(
        pinned(SelectLook::Hover, true),
        (vec![th.border_control().to_egui()], vec![])
    );
    assert_eq!(pinned(SelectLook::Open, true), (vec![focus], vec![focus]));
    assert_eq!(
        pinned(SelectLook::Open, false),
        (vec![th.state_disabled_border().to_egui()], vec![]),
        "disabled는 상태 모양 없이 disabled 상자다"
    );
}
