//! 창 버튼 사건의 mesh 판정과 전달. 창(`MainView::forward_mesh_button`)과 같은 [`route_mesh_button`]을 창의
//! 호출 순서대로 불러, 로컬·attach 입력 큐에 들어간 사건을 잰다. 창 호출부의 순서와 모양은
//! `source_guards::input_destination_call_sites`가 고정한다.

use std::collections::HashMap;

use tasty_plugin_protocol::{ModifiersWire, PointerButtonWire, RawInputEventWire};
use winit::event::MouseButton::{self, Left, Right};

use super::{
    MeshButtonWindow, MeshForwardState, MeshPass, local_point, pointer_moved_event,
    route_mesh_button,
};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::state::mesh_frames::{MeshFrameRegion, MeshPress, MeshPresses, MeshSide};
use crate::view::main::attach_mesh_input::AttachMeshForwardState;

/// `id` leaf. 세로 `y`부터 높이 50, 너비 100(물리 px).
pub(super) fn region(id: u32, y: f32, side: MeshSide) -> MeshFrameRegion {
    MeshFrameRegion {
        id,
        rect: PhysicalRect {
            x: PhysicalPx(0.0),
            y: PhysicalPx(y),
            width: PhysicalPx(100.0),
            height: PhysicalPx(50.0),
        },
        side,
    }
}

/// 창이 mesh 전달에 쓰는 상태.
#[derive(Default)]
pub(super) struct Window {
    pub(super) presses: MeshPresses,
    pub(super) local: HashMap<u32, MeshForwardState>,
    pub(super) attach: HashMap<u32, AttachMeshForwardState>,
    pub(super) left_mouse_down: bool,
    pub(super) dragging_divider: bool,
}

impl Window {
    /// [`route_mesh_button`] 한 번.
    pub(super) fn pass(
        &mut self,
        regions: &[MeshFrameRegion],
        cursor: Option<(f32, f32)>,
        pass: MeshPass,
        (button, pressed): (MouseButton, bool),
    ) -> bool {
        let w = MeshButtonWindow {
            regions,
            presses: &mut self.presses,
            local: &mut self.local,
            attach: &mut self.attach,
            left_mouse_down: &mut self.left_mouse_down,
            cursor,
            dragging_divider: self.dragging_divider,
            ppp: 1.0,
            modifiers: ModifiersWire::default(),
        };
        route_mesh_button(w, pass, button, pressed)
    }

    /// 창의 포인터 이동 전달 한 번(`r` leaf로, 물리 `at`). 창처럼 [`pointer_moved_event`]로 사건을 만든다.
    pub(super) fn moved(&mut self, r: MeshFrameRegion, at: (f32, f32)) {
        let press = MeshPress {
            id: r.id,
            side: r.side,
        };
        let event = pointer_moved_event(&mut self.presses, press, local_point(r.rect, at, 1.0));
        match r.side {
            MeshSide::Local => self.local.entry(r.id).or_default().events.push(event),
            MeshSide::Attach => self.attach.entry(r.id).or_default().push_event(event),
        }
    }

    /// 창의 버튼 처리 순서: 기록된 뗌 단계가 먼저다. 그 단계가 가져가지 않았고 위 층(`above`)이 가져가지
    /// 않았으면 아래 단계로 간다.
    pub(super) fn button(
        &mut self,
        regions: &[MeshFrameRegion],
        cursor: Option<(f32, f32)>,
        event: (MouseButton, bool),
        above: bool,
    ) -> bool {
        self.pass(regions, cursor, MeshPass::Recorded, event)
            || (!above && self.pass(regions, cursor, MeshPass::Below, event))
    }

    /// `side` leaf `id`의 입력 큐 그대로.
    pub(super) fn queue(&self, side: MeshSide, id: u32) -> Vec<RawInputEventWire> {
        match side {
            MeshSide::Local => self.local.get(&id).map_or(Vec::new(), |s| s.events.clone()),
            MeshSide::Attach => self
                .attach
                .get(&id)
                .map_or(Vec::new(), |s| s.events().to_vec()),
        }
    }

    /// `side` leaf `id`의 입력 큐에 든 사건 순서. 버튼·이동·떠남 밖의 사건도 [`Seen::Other`]로 남긴다.
    pub(super) fn events(&self, side: MeshSide, id: u32) -> Vec<Seen> {
        let events: &[RawInputEventWire] = match side {
            MeshSide::Local => self.local.get(&id).map_or(&[], |s| &s.events),
            MeshSide::Attach => self.attach.get(&id).map_or(&[], |s| s.events()),
        };
        events
            .iter()
            .map(|e| match e {
                RawInputEventWire::PointerButton {
                    button,
                    pressed,
                    x,
                    y,
                    ..
                } => Seen::Button(*button, *pressed, *x, *y),
                RawInputEventWire::PointerMoved { x, y } => Seen::Moved(*x, *y),
                RawInputEventWire::PointerGone => Seen::Gone,
                _ => Seen::Other,
            })
            .collect()
    }
}

/// 입력 큐에 든 사건 하나. 버튼은 (버튼, 누름, 논리 x, 논리 y), 이동은 논리 (x, y)다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Seen {
    Button(PointerButtonWire, bool, f32, f32),
    Moved(f32, f32),
    Gone,
    Other,
}

/// mesh leaf를 오른쪽으로 누른 뒤 분할선 드래그가 시작되고, 위 층이 사건을 가져가는 자리에서 오른쪽을 떼도 뗌은
/// 누른 leaf의 입력 큐에 들어가고 기록이 지워진다. 포인터 아래 다른 leaf는 받지 않는다. 로컬과 attach가 같다.
#[test]
fn a_recorded_release_lands_in_the_pressed_leafs_queue() {
    for side in [MeshSide::Local, MeshSide::Attach] {
        for (dragging, above) in [(true, false), (false, true), (true, true)] {
            let regions = [region(1, 10.0, side), region(3, 70.0, MeshSide::Local)];
            let mut w = Window::default();
            assert!(w.button(&regions, Some((5.0, 20.0)), (Right, true), false));
            w.dragging_divider = dragging;
            assert!(
                w.button(&regions, Some((5.0, 80.0)), (Right, false), above),
                "{side:?} dragging={dragging} above={above}"
            );
            assert_eq!(
                w.events(side, 1),
                vec![
                    Seen::Button(PointerButtonWire::Secondary, true, 5.0, 10.0),
                    Seen::Button(PointerButtonWire::Secondary, false, 5.0, 70.0)
                ],
                "{side:?} dragging={dragging} above={above}"
            );
            assert!(w.events(MeshSide::Local, 3).is_empty());
            assert_eq!(w.presses, MeshPresses::default(), "record cleared");
        }
    }
}

/// 기록 없는 사건은 아래 단계에서 포인터 아래 leaf로 간다. 분할선 드래그 중이면 로컬 leaf는 받지 않고 attach는 받는다.
#[test]
fn unrecorded_events_go_below_and_skip_the_local_mesh_during_a_drag() {
    let regions = [
        region(1, 10.0, MeshSide::Local),
        region(3, 70.0, MeshSide::Attach),
    ];
    let mut w = Window {
        dragging_divider: true,
        ..Window::default()
    };
    assert!(!w.button(&regions, Some((5.0, 20.0)), (Left, false), false));
    assert!(w.button(&regions, Some((5.0, 80.0)), (Left, true), false));
    assert_eq!(
        w.events(MeshSide::Attach, 3),
        vec![Seen::Button(PointerButtonWire::Primary, true, 5.0, 10.0)]
    );
    assert!(w.events(MeshSide::Local, 1).is_empty());
}

/// 누름 기록에 넣는 버튼은 wire로 보낼 수 있는 버튼과 같다.
#[test]
fn the_recorded_buttons_are_the_wire_buttons() {
    for b in [
        Left,
        Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
        MouseButton::Other(7),
    ] {
        assert_eq!(
            crate::state::mesh_frames::wire_button(b),
            super::map_button(b).is_some(),
            "{b:?}"
        );
    }
}

/// 로컬 A를 왼쪽으로 누른 채 attach B 위에서 wire로 보낼 수 없는 버튼을 누르면, 그 누름은 기록하지 않으므로 이동·휠은
/// 그대로 A로 간다. B의 입력 큐에도 아무것도 들어가지 않는다.
#[test]
fn an_extra_button_does_not_move_the_held_target() {
    use crate::state::mesh_frames::motion_target;
    let regions = [
        region(1, 10.0, MeshSide::Local),
        region(3, 70.0, MeshSide::Attach),
    ];
    let mut w = Window::default();
    assert!(w.button(&regions, Some((5.0, 20.0)), (Left, true), false));
    w.button(
        &regions,
        Some((5.0, 80.0)),
        (MouseButton::Back, true),
        false,
    );
    assert!(w.events(MeshSide::Attach, 3).is_empty());
    for y in [80.0, 20.0] {
        assert_eq!(
            motion_target(&regions, &w.presses, MeshSide::Local, 5.0, y).map(|r| r.id),
            Some(1),
            "moves and wheels stay with A at y={y}"
        );
        assert!(motion_target(&regions, &w.presses, MeshSide::Attach, 5.0, y).is_none());
    }
}

/// 누른 뒤 포인터가 창 밖으로 나가 좌표가 비고 뗌이 오면, 기록을 지우고 뗌을 그 leaf에 마지막으로 보낸 논리
/// 좌표로 보낸다. 누른 뒤 창의 이동 전달로 보낸 이동이 있으면 그 좌표, 없으면 누른 좌표다. 창 밖 뗌의 좌표로 SDK가
/// 포인터를 안에 두지 않도록 뗌 바로 뒤에 `PointerGone`이 다시 간다. 로컬과 attach가 같다.
#[test]
fn a_release_without_a_cursor_reaches_the_pressed_leaf_at_its_last_point() {
    for side in [MeshSide::Local, MeshSide::Attach] {
        for moved in [None, Some((5.0, 40.0))] {
            let regions = [region(1, 10.0, side)];
            let mut w = Window::default();
            assert!(w.button(&regions, Some((5.0, 20.0)), (Right, true), false));
            if let Some(at) = moved {
                w.moved(regions[0], at);
            }
            assert!(
                w.button(&regions, None, (Right, false), false),
                "{side:?} the release is taken"
            );
            let y = moved.map_or(20.0, |(_, y)| y) - 10.0;
            let mut expected = vec![Seen::Button(PointerButtonWire::Secondary, true, 5.0, 10.0)];
            expected.extend(moved.map(|(x, y)| Seen::Moved(x, y - 10.0)));
            expected.extend([
                Seen::Button(PointerButtonWire::Secondary, false, 5.0, y),
                Seen::Gone,
            ]);
            assert_eq!(w.events(side, 1), expected, "{side:?} moved={moved:?}");
            assert_eq!(w.presses, MeshPresses::default(), "{side:?} record cleared");
        }
    }
}

/// A를 누른 뒤 A가 배치에서 빠지고(탭 전환 등) 포인터 아래 같은 쪽 mesh B에서 떼면, 뗌은 기록을 지우며 소비되고 B는
/// 짝 없는 뗌을 받지 않는다. 로컬과 attach가 같다.
#[test]
fn a_release_of_a_vanished_leaf_is_consumed() {
    for side in [MeshSide::Local, MeshSide::Attach] {
        let mut w = Window::default();
        let before = [region(1, 10.0, side), region(3, 70.0, side)];
        assert!(w.button(&before, Some((5.0, 20.0)), (Left, true), false));
        w.left_mouse_down = true;
        let after = [region(3, 70.0, side)];
        assert!(
            w.button(&after, Some((5.0, 80.0)), (Left, false), false),
            "{side:?} the release is taken"
        );
        assert!(w.events(side, 3).is_empty(), "{side:?} B gets no release");
        assert_eq!(w.presses, MeshPresses::default(), "{side:?} record cleared");
        assert!(
            !w.left_mouse_down,
            "{side:?} a mesh left release ends the window press"
        );
    }
}

/// mesh가 가져간 뗌은 왼쪽 버튼일 때만 창의 왼쪽 누름 상태를 끈다. 오른쪽·가운데 뗌 뒤에는 왼쪽 누름(선택 드래그
/// 등)이 그대로다. 기록된 뗌 단계와 아래 단계가 같다.
#[test]
fn only_a_left_mesh_release_ends_the_window_left_press() {
    let regions = [region(1, 10.0, MeshSide::Local)];
    for other in [Right, MouseButton::Middle] {
        for recorded in [true, false] {
            let mut w = Window {
                left_mouse_down: true,
                ..Window::default()
            };
            if recorded {
                assert!(w.button(&regions, Some((5.0, 20.0)), (other, true), false));
            }
            assert!(w.button(&regions, Some((5.0, 20.0)), (other, false), false));
            assert!(
                w.left_mouse_down,
                "{other:?} release (recorded={recorded}) keeps the left press"
            );
        }
    }
    let mut w = Window {
        left_mouse_down: true,
        ..Window::default()
    };
    assert!(w.button(&regions, Some((5.0, 20.0)), (Left, false), false));
    assert!(!w.left_mouse_down, "a left release ends it");
}

/// 창 밖 뗌의 좌표는 그 leaf에 좌표를 실어 마지막으로 보낸 사건(이동·누름·뗌)의 좌표다. 로컬 A를 왼쪽·가운데로
/// 누르고 attach B를 오른쪽으로 누른 뒤 이동하면 이동은 B로 간다. 창 안에서 왼쪽을 떼고 창 밖에서 가운데를 떼면
/// A의 가운데 뗌은 앞선 왼쪽 뗌 좌표(x·y 둘 다)로 가고, 그 뒤 A에 `PointerGone`이 간다.
#[test]
fn a_release_outside_the_window_uses_the_last_point_sent_to_its_leaf() {
    let a = region(1, 10.0, MeshSide::Local);
    let b = region(3, 70.0, MeshSide::Attach);
    let regions = [a, b];
    let mut w = Window::default();
    assert!(w.button(&regions, Some((5.0, 20.0)), (Left, true), false));
    assert!(w.button(
        &regions,
        Some((5.0, 25.0)),
        (MouseButton::Middle, true),
        false
    ));
    assert!(w.button(&regions, Some((5.0, 80.0)), (Right, true), false));
    w.moved(b, (5.0, 90.0));
    assert!(w.button(&regions, Some((7.0, 35.0)), (Left, false), false));
    assert!(w.button(&regions, None, (MouseButton::Middle, false), false));
    assert_eq!(
        w.events(MeshSide::Local, 1),
        vec![
            Seen::Button(PointerButtonWire::Primary, true, 5.0, 10.0),
            Seen::Button(PointerButtonWire::Middle, true, 5.0, 15.0),
            Seen::Button(PointerButtonWire::Primary, false, 7.0, 25.0),
            Seen::Button(PointerButtonWire::Middle, false, 7.0, 25.0),
            Seen::Gone,
        ]
    );
    assert_eq!(
        w.events(MeshSide::Attach, 3),
        vec![
            Seen::Button(PointerButtonWire::Secondary, true, 5.0, 10.0),
            Seen::Moved(5.0, 20.0),
        ]
    );
}

/// leaf 하나(100x50 논리 pt)에 0..=100 정수 Slider 하나를 그리는 egui 패스 한 번. SDK(`tasty-plugin-sdk`
/// egui_surface/input.rs)처럼 wire 사건을 egui 사건으로 1:1로 옮긴다. Slider의 사각형을 돌려준다.
fn slider_pass(ctx: &egui::Context, events: &[RawInputEventWire], value: &mut i32) -> egui::Rect {
    let events = events
        .iter()
        .map(|e| match *e {
            RawInputEventWire::PointerMoved { x, y } => egui::Event::PointerMoved(egui::pos2(x, y)),
            RawInputEventWire::PointerButton {
                x,
                y,
                button,
                pressed,
                ..
            } => egui::Event::PointerButton {
                pos: egui::pos2(x, y),
                button: match button {
                    PointerButtonWire::Primary => egui::PointerButton::Primary,
                    PointerButtonWire::Secondary => egui::PointerButton::Secondary,
                    PointerButtonWire::Middle => egui::PointerButton::Middle,
                },
                pressed,
                modifiers: egui::Modifiers::default(),
            },
            RawInputEventWire::PointerGone => egui::Event::PointerGone,
            ref other => panic!("이 시험이 옮기지 않는 사건: {other:?}"),
        })
        .collect();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(100.0, 50.0),
        )),
        events,
        ..Default::default()
    };
    let mut rect = egui::Rect::NOTHING;
    // 이 시험은 Slider 값과 사각형만 본다. 패스 출력(도형·텍스처)은 쓰지 않는다.
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().slider_width = 60.0;
            rect = ui.add(egui::Slider::new(value, 0..=100)).rect;
        });
    });
    rect
}

/// 창 밖 뗌은 같은 패스의 입력 결과를 바꾸지 않는다. leaf 큐를 실제 egui 패스로 돌려, 0..=100 Slider를 눌러
/// 65에 둔 뒤 CursorLeft의 PointerGone을 받고 창 밖에서 떼면 값이 65 그대로인지 잰다. 뗌 묶음에 합성 이동을
/// 넣으면 그 패스의 끌기 좌표가 바뀌어 값이 튄다. 로컬과 attach가 같다.
#[test]
fn a_release_outside_the_window_keeps_the_dragged_value() {
    // 누르면 65가 되는 leaf 논리 x(빈 문맥마다 한 번 배치하고 눌러 본다).
    let press_at = |x: f32, y: f32| {
        let ctx = egui::Context::default();
        let mut v = 0;
        slider_pass(&ctx, &[], &mut v);
        let button = |pressed| RawInputEventWire::PointerButton {
            x,
            y,
            button: PointerButtonWire::Primary,
            pressed,
            modifiers: ModifiersWire::default(),
        };
        slider_pass(&ctx, &[button(true)], &mut v);
        v
    };
    let rect = slider_pass(&egui::Context::default(), &[], &mut 0);
    let y = rect.center().y;
    let x = (0..400)
        .map(|i| rect.left() + rect.width() * i as f32 / 400.0)
        .find(|x| press_at(*x, y) == 65)
        .expect("전제: 65가 되는 x가 있다");
    for side in [MeshSide::Local, MeshSide::Attach] {
        let regions = [region(1, 10.0, side)];
        let ctx = egui::Context::default();
        let mut value = 0;
        slider_pass(&ctx, &[], &mut value);
        let mut w = Window::default();
        assert!(w.button(&regions, Some((x, y + 10.0)), (Left, true), false));
        let pressed = w.queue(side, 1);
        slider_pass(&ctx, &pressed, &mut value);
        assert_eq!(value, 65, "{side:?} 전제: 누른 값");
        slider_pass(&ctx, &[RawInputEventWire::PointerGone], &mut value);
        assert_eq!(value, 65, "{side:?} CursorLeft 뒤");
        assert!(w.button(&regions, None, (Left, false), false));
        let released = w.queue(side, 1).split_off(pressed.len());
        slider_pass(&ctx, &released, &mut value);
        assert_eq!(value, 65, "{side:?} 창 밖 뗌 뒤: {released:?}");
        slider_pass(&ctx, &[], &mut value);
        assert_eq!(value, 65, "{side:?} 그 뒤 패스");
    }
}
