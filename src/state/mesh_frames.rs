//! 활성 탭에서 plugin mesh 프레임을 그리는 surface와 그 프레임 영역. 합성·컨텍스트 크기·포인터 대상·입력
//! 좌표가 모두 이 목록을 읽어 같은 영역을 쓴다(`docs/dev-guide/egui-mesh-channel.md`).
//! 띠 kind의 프레임은 leaf 위쪽 포커스 띠 행 아래에서 시작한다. 띠 행은 plugin 입력 영역이 아니다.

use crate::model::PhysicalRect;
use crate::runtime::engine_read::EngineRead;

use super::MainViewState;

/// 프레임을 그리는 쪽. 로컬 plugin이 그리는 surface와 attach로 받은 원격 surface다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MeshSide {
    Local,
    Attach,
}

/// mesh 프레임 하나의 surface와 물리 영역. 띠 kind면 띠 행을 뺀 영역이다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeshFrameRegion {
    pub(crate) id: u32,
    pub(crate) rect: PhysicalRect,
    pub(crate) side: MeshSide,
}

/// leaf에서 mesh 프레임이 차지하는 영역. 띠를 예약하는 leaf면 띠 행 아래, 아니면 leaf 전체다.
pub(crate) fn frame_rect(
    theme: &tasty_type_appearance::theme::Theme,
    leaf: PhysicalRect,
    reserves_edge: bool,
    scale_factor: f32,
) -> PhysicalRect {
    if reserves_edge {
        super::surface_focus::below_edge(theme, leaf, scale_factor)
    } else {
        leaf
    }
}

/// 버튼을 누른 mesh leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MeshPress {
    pub(crate) id: u32,
    pub(crate) side: MeshSide,
}

/// 버튼마다 누른 mesh leaf. 한 버튼을 누른 채 다른 버튼을 다른 leaf에서 눌러도 각 뗌은 그 버튼을 누른
/// leaf로 간다. 누른 leaf마다 마지막으로 받은 포인터 위치(물리 px)도 둔다. 포인터가 창 밖으로 나가 좌표가 없을
/// 때 온 뗌을 그 위치로 보낸다.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MeshPresses {
    held: Vec<(winit::event::MouseButton, MeshPress)>,
    seen: Vec<(MeshPress, (f32, f32))>,
}

impl MeshPresses {
    /// 마지막으로 누른 채인 leaf(로컬·attach 가리지 않는다). 누른 동안의 포인터 이동·휠이 이 leaf로 간다.
    pub(crate) fn last(&self) -> Option<MeshPress> {
        self.held.last().map(|(_, p)| *p)
    }

    /// `button`을 누른 leaf.
    pub(crate) fn of(&self, button: winit::event::MouseButton) -> Option<MeshPress> {
        self.held
            .iter()
            .find(|(b, _)| *b == button)
            .map(|(_, p)| *p)
    }

    /// `button`을 `press` leaf의 `at`에서 눌렀다.
    fn record(&mut self, button: winit::event::MouseButton, press: MeshPress, at: (f32, f32)) {
        self.held.push((button, press));
        self.saw(press, at);
    }

    /// 누른 채인 `press` leaf가 `at`의 포인터를 받았다. 누른 leaf가 아니면 아무것도 하지 않는다.
    pub(crate) fn saw(&mut self, press: MeshPress, at: (f32, f32)) {
        if !self.held.iter().any(|(_, p)| *p == press) {
            return;
        }
        self.seen.retain(|(p, _)| *p != press);
        self.seen.push((press, at));
    }

    /// `press` leaf가 마지막으로 받은 포인터 위치.
    fn seen_at(&self, press: MeshPress) -> Option<(f32, f32)> {
        self.seen
            .iter()
            .find(|(p, _)| *p == press)
            .map(|(_, at)| *at)
    }

    fn forget(&mut self, button: winit::event::MouseButton) {
        self.held.retain(|(b, _)| *b != button);
        let held = &self.held;
        self.seen.retain(|(p, _)| held.iter().any(|(_, h)| h == p));
    }
}

/// 포인터 사건의 `side` 대상. 그 쪽에서 버튼을 누른 채면 누른 leaf다(포인터가 띠 행이나 leaf 밖이어도).
/// 아니면 포인터 아래 영역이다.
pub(crate) fn pointer_target(
    regions: &[MeshFrameRegion],
    press: Option<MeshPress>,
    side: MeshSide,
    x: f32,
    y: f32,
) -> Option<MeshFrameRegion> {
    use crate::model::PhysicalPx;
    match press.filter(|p| p.side == side) {
        Some(p) => regions
            .iter()
            .find(|r| r.id == p.id && r.side == side)
            .copied(),
        None => regions
            .iter()
            .find(|r| r.side == side && r.rect.contains(PhysicalPx(x), PhysicalPx(y)))
            .copied(),
    }
}

/// plugin wire로 보낼 수 있는 버튼(왼쪽·오른쪽·가운데). `view::main::egui_mesh::map_button`이 옮기는 버튼과 같다.
/// 이 밖의 버튼은 누름 기록에 넣지 않는다.
pub(crate) fn wire_button(button: winit::event::MouseButton) -> bool {
    use winit::event::MouseButton::{Left, Middle, Right};
    matches!(button, Left | Right | Middle)
}

/// 포인터 이동·휠의 `side` 대상. 버튼을 누른 채면 마지막으로 누른 leaf가 받고, 그 leaf가 다른 쪽이면 이쪽은
/// 받지 않는다(로컬 A를 누른 채 attach B를 누르면 이동·휠은 B로 간다). 누른 버튼이 없으면 포인터 아래 영역이다.
/// 로컬과 attach가 같은 판정을 쓴다.
pub(crate) fn motion_target(
    regions: &[MeshFrameRegion],
    presses: &MeshPresses,
    side: MeshSide,
    x: f32,
    y: f32,
) -> Option<MeshFrameRegion> {
    match presses.last() {
        Some(p) if p.side != side => None,
        last => pointer_target(regions, last, side, x, y),
    }
}

/// `button` 사건의 `side` 대상. 누름은 포인터 아래 leaf를 그 버튼으로 기억하고, 뗌은 그 버튼을 누른 leaf로
/// 보낸 뒤 잊는다. 다른 쪽이 누른 버튼의 뗌은 이쪽이 받지 않는다. 이쪽에서 빗나간 누름은 그 버튼의 이쪽
/// 지난 기억을 지운다. 로컬과 attach가 같은 판정을 쓴다.
pub(crate) fn button_target(
    regions: &[MeshFrameRegion],
    presses: &mut MeshPresses,
    side: MeshSide,
    button: winit::event::MouseButton,
    (x, y): (f32, f32),
    pressed: bool,
) -> Option<MeshFrameRegion> {
    let recorded = presses.of(button);
    if pressed {
        let hit = pointer_target(regions, None, side, x, y);
        if !wire_button(button) {
            // plugin에 보낼 수 없는 버튼이라 그 leaf가 누름을 받지 않는다. 기록하면 이동·휠 대상만 바뀐다.
            return hit;
        }
        if hit.is_some() || recorded.is_some_and(|p| p.side == side) {
            presses.forget(button);
        }
        if let Some(r) = hit {
            presses.record(button, MeshPress { id: r.id, side }, (x, y));
        }
        return hit;
    }
    match recorded {
        Some(p) if p.side == side => {
            presses.forget(button);
            pointer_target(regions, Some(p), side, x, y)
        }
        Some(_) => None,
        None => pointer_target(regions, None, side, x, y),
    }
}

/// 창의 버튼 처리에서 mesh 판정을 부르는 단계.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MeshButtonStage {
    /// 다른 분기(메뉴 닫기·분할선·위 층)보다 먼저. mesh에서 누른 버튼의 뗌만 본다.
    Recorded,
    /// 위 층·분할선 판정 뒤의 나머지 사건. 분할선 드래그 중이면 로컬 mesh는 건너뛴다.
    Below { dragging_divider: bool },
}

/// 창 버튼 사건 하나의 mesh 판정 결과.
#[derive(Clone, Copy, Debug)]
pub(crate) enum MeshButtonRoute {
    /// 이 leaf에 이 위치(물리 px)로 보낸다.
    Send(MeshFrameRegion, (f32, f32)),
    /// mesh가 가져가되 보내지 않는다(기록만 지운다).
    Consume,
    /// mesh 사건이 아니다. 창의 다른 분기로 간다.
    Pass,
}

/// 창 버튼 사건의 mesh 대상(로컬·attach 공통). `at`은 창 안 포인터이고 창 밖이면 `None`이다.
///
/// `Recorded` 단계는 기록된 버튼의 뗌을 기록한 leaf로 보내고 기록을 지운다. 이 단계가 다른 분기보다 앞이라
/// mesh에서 누른 버튼의 뗌은 분할선 드래그 중이든 위 층이 사건을 가져가든 사라지지 않는다. 좌표가 없으면 그
/// leaf가 마지막으로 받은 포인터 위치로 보내고, 그 위치도 없으면 기록만 지운다(`Consume`). 그 leaf가 배치에서
/// 사라졌어도 기록을 지우고 뗌을 가져간다(`Consume`). 포인터 아래 다른 mesh가 짝 없는 뗌을 받지 않는다.
/// `Below` 단계는 좌표가 있을 때 로컬, 그다음 attach 순서로 [`button_target`]을 쓴다.
pub(crate) fn mesh_button_target(
    regions: &[MeshFrameRegion],
    presses: &mut MeshPresses,
    stage: MeshButtonStage,
    button: winit::event::MouseButton,
    at: Option<(f32, f32)>,
    pressed: bool,
) -> MeshButtonRoute {
    match stage {
        MeshButtonStage::Recorded => {
            let Some(p) = presses.of(button).filter(|_| !pressed) else {
                return MeshButtonRoute::Pass;
            };
            let at = at.or_else(|| presses.seen_at(p));
            presses.forget(button);
            let Some(region) = regions.iter().find(|r| r.id == p.id && r.side == p.side) else {
                // 누른 leaf가 배치에서 빠졌다. 아래 단계가 기록 없는 사건으로 다시 찾으면 포인터 아래 다른 mesh에
                // 짝 없는 뗌이 간다.
                return MeshButtonRoute::Consume;
            };
            at.map_or(MeshButtonRoute::Consume, |at| {
                MeshButtonRoute::Send(*region, at)
            })
        }
        MeshButtonStage::Below { dragging_divider } => {
            let Some((x, y)) = at else {
                return MeshButtonRoute::Pass;
            };
            let local = (!dragging_divider)
                .then(|| button_target(regions, presses, MeshSide::Local, button, (x, y), pressed))
                .flatten();
            local
                .or_else(|| {
                    button_target(regions, presses, MeshSide::Attach, button, (x, y), pressed)
                })
                .map_or(MeshButtonRoute::Pass, |r| MeshButtonRoute::Send(r, (x, y)))
        }
    }
}

impl MainViewState {
    /// 활성 workspace의 그리는 탭에서 mesh 프레임 영역을 모은다.
    pub(crate) fn mesh_frame_regions(
        &self,
        engine: &EngineRead<'_>,
        terminal_rect: PhysicalRect,
        scale_factor: f32,
    ) -> Vec<MeshFrameRegion> {
        let theme = crate::theme::theme();
        let mut out = Vec::new();
        for (_pane_id, _pane_rect, regions) in
            self.surface_regions(engine, terminal_rect, scale_factor)
        {
            for r in regions {
                let Some(surface) = engine.find_surface_by_id(r.id) else {
                    continue;
                };
                let side = if surface.mesh().is_some() {
                    MeshSide::Local
                } else if surface.attach_mesh().is_some() {
                    MeshSide::Attach
                } else {
                    continue;
                };
                let reserves = super::surface_focus::reserves_edge(engine, r.id);
                let rect = frame_rect(&theme, r.rect, reserves, scale_factor);
                out.push(MeshFrameRegion {
                    id: r.id,
                    rect,
                    side,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PhysicalPx;

    fn region(id: u32, y: f32, side: MeshSide) -> MeshFrameRegion {
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

    /// 띠 leaf는 띠 행 아래, 띠가 없는 leaf는 leaf 전체다.
    #[test]
    fn the_frame_starts_below_the_edge_only_for_an_edge_leaf() {
        let theme = tasty_themes::mocha_fallback();
        let leaf = region(1, 100.75, MeshSide::Local).rect;
        let edge = frame_rect(&theme, leaf, true, 1.25);
        assert_eq!(edge.y.value(), 103.0);
        assert_eq!(edge.y.value() + edge.height.value(), 150.75);
        let bed = frame_rect(&theme, leaf, false, 1.25);
        assert_eq!((bed.y, bed.height), (leaf.y, leaf.height));
    }

    use winit::event::MouseButton::{Left, Right};

    fn pressed(id: u32, side: MeshSide) -> MeshPress {
        MeshPress { id, side }
    }

    /// 본문에서 누르고 같은 leaf의 띠 행(영역 밖 위쪽)에서 떼면 뗌도 그 leaf로 간다. 기억은 뗌 뒤 지운다.
    #[test]
    fn a_release_over_the_edge_row_reaches_the_pressed_mesh() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(2, 70.0, MeshSide::Attach),
        ];
        let mut presses = MeshPresses::default();
        let down = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 20.0),
            true,
        );
        assert_eq!(down.map(|r| r.id), Some(1));
        assert_eq!(presses.last(), Some(pressed(1, MeshSide::Local)));
        assert_eq!(
            motion_target(&regions, &presses, MeshSide::Local, 5.0, 9.0).map(|r| r.id),
            Some(1),
            "a move over the edge row stays with the pressed mesh"
        );
        let up = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 9.0),
            false,
        );
        assert_eq!(up.map(|r| r.id), Some(1));
        assert_eq!(presses, MeshPresses::default());
        let later = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 9.0),
            false,
        );
        assert!(
            later.is_none(),
            "without a press the edge row is not the mesh"
        );
    }

    /// 원격 mesh를 누른 뗌은 로컬 쪽이 받지 않고 원격 쪽이 받는다. 같은 경계를 attach에도 둔다.
    #[test]
    fn a_release_goes_to_the_side_that_was_pressed() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(2, 70.0, MeshSide::Attach),
        ];
        let mut presses = MeshPresses::default();
        let local_down = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 80.0),
            true,
        );
        assert!(local_down.is_none());
        let down = button_target(
            &regions,
            &mut presses,
            MeshSide::Attach,
            Left,
            (5.0, 80.0),
            true,
        );
        assert_eq!(down.map(|r| r.id), Some(2));
        let local_up = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 20.0),
            false,
        );
        assert!(local_up.is_none());
        let up = button_target(
            &regions,
            &mut presses,
            MeshSide::Attach,
            Left,
            (5.0, 20.0),
            false,
        );
        assert_eq!(up.map(|r| r.id), Some(2));
        assert_eq!(presses, MeshPresses::default());
    }

    /// 한 쪽에서 A를 왼쪽으로 누른 채 B를 오른쪽으로 누르고 B 위에서 왼쪽을 떼면 왼쪽 뗌은 A로, 오른쪽
    /// 뗌은 B로 간다. 로컬과 attach가 같다.
    #[test]
    fn each_release_goes_to_the_leaf_its_button_pressed() {
        for side in [MeshSide::Local, MeshSide::Attach] {
            let regions = [region(1, 10.0, side), region(3, 70.0, side)];
            let mut presses = MeshPresses::default();
            let a = button_target(&regions, &mut presses, side, Left, (5.0, 20.0), true);
            assert_eq!(a.map(|r| r.id), Some(1), "{side:?}");
            let b = button_target(&regions, &mut presses, side, Right, (5.0, 80.0), true);
            assert_eq!(b.map(|r| r.id), Some(3), "{side:?}");
            assert_eq!(presses.last(), Some(pressed(3, side)), "{side:?} moves");
            let left_up = button_target(&regions, &mut presses, side, Left, (5.0, 80.0), false);
            assert_eq!(left_up.map(|r| r.id), Some(1), "{side:?} left release");
            let right_up = button_target(&regions, &mut presses, side, Right, (5.0, 20.0), false);
            assert_eq!(right_up.map(|r| r.id), Some(3), "{side:?} right release");
            assert_eq!(presses, MeshPresses::default(), "{side:?}");
        }
    }

    /// 로컬 A를 왼쪽으로 누른 채 attach B를 오른쪽으로 누르면, 누른 동안의 포인터 이동·휠은 마지막으로 누른 B가
    /// 받는다. 포인터가 A 위여도 로컬 조회가 먼저 가져가지 않는다. B를 떼면 다시 A가 받는다. 반대 순서도 같다.
    #[test]
    fn moves_and_wheels_go_to_the_last_pressed_leaf() {
        for (first, second) in [
            (MeshSide::Local, MeshSide::Attach),
            (MeshSide::Attach, MeshSide::Local),
        ] {
            let regions = [region(1, 10.0, first), region(3, 70.0, second)];
            let mut presses = MeshPresses::default();
            let a = button_target(&regions, &mut presses, first, Left, (5.0, 20.0), true);
            assert_eq!(a.map(|r| r.id), Some(1));
            let b = button_target(&regions, &mut presses, second, Right, (5.0, 80.0), true);
            assert_eq!(b.map(|r| r.id), Some(3));
            for (x, y) in [(5.0, 80.0), (5.0, 20.0), (5.0, 300.0)] {
                assert!(
                    motion_target(&regions, &presses, first, x, y).is_none(),
                    "{first:?} lookup at {y} must not take the move"
                );
                assert_eq!(
                    motion_target(&regions, &presses, second, x, y).map(|r| r.id),
                    Some(3),
                    "{second:?} B takes the move at {y}"
                );
            }
            let b_up = button_target(&regions, &mut presses, second, Right, (5.0, 20.0), false);
            assert_eq!(b_up.map(|r| r.id), Some(3));
            assert_eq!(
                motion_target(&regions, &presses, first, 5.0, 80.0).map(|r| r.id),
                Some(1),
                "A is pressed again the last one held"
            );
            assert!(motion_target(&regions, &presses, second, 5.0, 80.0).is_none());
        }
    }

    /// 창의 버튼 처리 한 번: 맨 앞 `Recorded` 단계가 사건을 가져가지 않았고 위 층이 가져가지 않았으면(`above`)
    /// `Below` 단계로 간다. 분할선 누름은 mesh 판정 없이 드래그를 시작한다(`dragging`).
    fn window_button(
        regions: &[MeshFrameRegion],
        presses: &mut MeshPresses,
        button: winit::event::MouseButton,
        at: (f32, f32),
        (pressed, dragging, above): (bool, bool, bool),
    ) -> Option<MeshFrameRegion> {
        let recorded = mesh_button_target(
            regions,
            presses,
            MeshButtonStage::Recorded,
            button,
            Some(at),
            pressed,
        );
        match recorded {
            MeshButtonRoute::Send(r, _) => return Some(r),
            MeshButtonRoute::Consume => return None,
            MeshButtonRoute::Pass if above => return None,
            MeshButtonRoute::Pass => {}
        }
        let stage = MeshButtonStage::Below {
            dragging_divider: dragging,
        };
        match mesh_button_target(regions, presses, stage, button, Some(at), pressed) {
            MeshButtonRoute::Send(r, _) => Some(r),
            _ => None,
        }
    }

    /// mesh leaf를 오른쪽으로 누른 뒤 왼쪽으로 분할선 드래그를 시작하고 오른쪽을 떼면, 뗌은 분할선 드래그 중에도
    /// 누른 leaf로 가고 기록이 지워진다. 위 층(팝업)이 뗌을 가져가는 경우도 같다. 로컬과 attach가 같다.
    #[test]
    fn a_recorded_release_reaches_its_leaf_whichever_branch_takes_it() {
        for side in [MeshSide::Local, MeshSide::Attach] {
            for (dragging, above) in [(true, false), (false, true), (true, true)] {
                let regions = [region(1, 10.0, side), region(3, 70.0, MeshSide::Local)];
                let mut presses = MeshPresses::default();
                let flags = (true, false, false);
                let down = window_button(&regions, &mut presses, Right, (5.0, 20.0), flags);
                assert_eq!(down.map(|r| r.id), Some(1), "{side:?}");
                let flags = (false, dragging, above);
                let up = window_button(&regions, &mut presses, Right, (5.0, 200.0), flags);
                assert_eq!(
                    up.map(|r| (r.id, r.side)),
                    Some((1, side)),
                    "{side:?} dragging={dragging} above={above}"
                );
                assert_eq!(presses, MeshPresses::default(), "{side:?} record cleared");
            }
        }
    }

    /// 분할선 드래그 중의 새 누름과 기록 없는 뗌은 로컬 mesh로 가지 않는다. attach는 받는다.
    #[test]
    fn a_divider_drag_keeps_unrecorded_events_from_the_local_mesh() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(3, 70.0, MeshSide::Attach),
        ];
        let mut presses = MeshPresses::default();
        let drag = (true, true, false);
        let local = window_button(&regions, &mut presses, Right, (5.0, 20.0), drag);
        assert!(local.is_none());
        let remote = window_button(&regions, &mut presses, Right, (5.0, 80.0), drag);
        assert_eq!(remote.map(|r| r.id), Some(3));
        let release = (false, true, false);
        let stray = window_button(&regions, &mut presses, Left, (5.0, 20.0), release);
        assert!(stray.is_none(), "an unrecorded release during a drag");
    }

    /// 지난 누름이 남아 있어도 새 누름은 포인터 아래로 정한다. 빗나간 누름은 그 버튼의 지난 기억을 지운다.
    #[test]
    fn a_new_press_replaces_a_stale_one() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(3, 70.0, MeshSide::Local),
        ];
        let mut presses = MeshPresses::default();
        presses.record(Left, pressed(1, MeshSide::Local), (5.0, 20.0));
        let down = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 80.0),
            true,
        );
        assert_eq!(down.map(|r| r.id), Some(3));
        assert_eq!(presses.held, vec![(Left, pressed(3, MeshSide::Local))]);
        let miss = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 200.0),
            true,
        );
        assert!(miss.is_none());
        assert_eq!(presses, MeshPresses::default());
    }
}
