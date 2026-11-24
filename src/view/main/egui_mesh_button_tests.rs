//! 창 버튼 사건의 mesh 판정과 전달. 창(`MainView::forward_mesh_button`)과 같은 [`route_mesh_button`]을 창의
//! 호출 순서대로 불러, 로컬·attach 입력 큐에 들어간 사건을 잰다. 창 호출부의 순서와 모양은
//! `source_guards::input_destination_call_sites`가 고정한다.

use std::collections::HashMap;

use tasty_plugin_protocol::{ModifiersWire, PointerButtonWire, RawInputEventWire};
use winit::event::MouseButton::{self, Left, Right};

use super::{MeshButtonWindow, MeshForwardState, MeshPass, route_mesh_button};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::state::mesh_frames::{MeshFrameRegion, MeshPresses, MeshSide};
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

    /// `side` leaf `id`의 입력 큐에 든 버튼 사건 (버튼, 누름, 논리 y).
    pub(super) fn buttons(&self, side: MeshSide, id: u32) -> Vec<(PointerButtonWire, bool, f32)> {
        let events: &[RawInputEventWire] = match side {
            MeshSide::Local => self.local.get(&id).map_or(&[], |s| &s.events),
            MeshSide::Attach => self.attach.get(&id).map_or(&[], |s| s.events()),
        };
        events
            .iter()
            .filter_map(|e| match e {
                RawInputEventWire::PointerButton {
                    button, pressed, y, ..
                } => Some((*button, *pressed, *y)),
                _ => None,
            })
            .collect()
    }
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
                w.buttons(side, 1),
                vec![
                    (PointerButtonWire::Secondary, true, 10.0),
                    (PointerButtonWire::Secondary, false, 70.0)
                ],
                "{side:?} dragging={dragging} above={above}"
            );
            assert!(w.buttons(MeshSide::Local, 3).is_empty());
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
        w.buttons(MeshSide::Attach, 3),
        vec![(PointerButtonWire::Primary, true, 10.0)]
    );
    assert!(w.buttons(MeshSide::Local, 1).is_empty());
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
    assert!(w.buttons(MeshSide::Attach, 3).is_empty());
    for y in [80.0, 20.0] {
        assert_eq!(
            motion_target(&regions, &w.presses, MeshSide::Local, 5.0, y).map(|r| r.id),
            Some(1),
            "moves and wheels stay with A at y={y}"
        );
        assert!(motion_target(&regions, &w.presses, MeshSide::Attach, 5.0, y).is_none());
    }
}

/// 누른 뒤 포인터가 창 밖으로 나가 좌표가 비고 뗌이 오면, 기록을 지우고 뗌을 그 leaf가 마지막으로 받은 포인터
/// 위치로 그 leaf에 보낸다. 누른 뒤 받은 이동이 있으면 그 위치, 없으면 누른 위치다. 로컬과 attach가 같다.
#[test]
fn a_release_without_a_cursor_reaches_the_pressed_leaf_at_its_last_point() {
    use crate::state::mesh_frames::MeshPress;
    for side in [MeshSide::Local, MeshSide::Attach] {
        for moved in [None, Some((5.0, 40.0))] {
            let regions = [region(1, 10.0, side)];
            let mut w = Window::default();
            assert!(w.button(&regions, Some((5.0, 20.0)), (Right, true), false));
            if let Some(at) = moved {
                w.presses.saw(MeshPress { id: 1, side }, at);
            }
            assert!(
                w.button(&regions, None, (Right, false), false),
                "{side:?} the release is taken"
            );
            let y = moved.map_or(20.0, |(_, y)| y) - 10.0;
            assert_eq!(
                w.buttons(side, 1),
                vec![
                    (PointerButtonWire::Secondary, true, 10.0),
                    (PointerButtonWire::Secondary, false, y)
                ],
                "{side:?} moved={moved:?}"
            );
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
        assert!(w.buttons(side, 3).is_empty(), "{side:?} B gets no release");
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
