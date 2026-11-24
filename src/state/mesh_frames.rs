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
/// leaf로 간다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MeshPresses(Vec<(winit::event::MouseButton, MeshPress)>);

impl MeshPresses {
    /// `side`에서 누른 채인 leaf. 여럿이면 먼저 누른 것이다. 포인터 이동·휠이 이 leaf로 간다.
    pub(crate) fn held(&self, side: MeshSide) -> Option<MeshPress> {
        self.0.iter().map(|(_, p)| *p).find(|p| p.side == side)
    }

    fn of(&self, button: winit::event::MouseButton) -> Option<MeshPress> {
        self.0.iter().find(|(b, _)| *b == button).map(|(_, p)| *p)
    }

    fn forget(&mut self, button: winit::event::MouseButton) {
        self.0.retain(|(b, _)| *b != button);
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
        if hit.is_some() || recorded.is_some_and(|p| p.side == side) {
            presses.forget(button);
        }
        if let Some(r) = hit {
            presses.0.push((button, MeshPress { id: r.id, side }));
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
        assert_eq!(
            presses.held(MeshSide::Local),
            Some(pressed(1, MeshSide::Local))
        );
        assert_eq!(
            pointer_target(
                &regions,
                presses.held(MeshSide::Local),
                MeshSide::Local,
                5.0,
                9.0
            )
            .map(|r| r.id),
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
            assert_eq!(presses.held(side), Some(pressed(1, side)), "{side:?} moves");
            let left_up = button_target(&regions, &mut presses, side, Left, (5.0, 80.0), false);
            assert_eq!(left_up.map(|r| r.id), Some(1), "{side:?} left release");
            let right_up = button_target(&regions, &mut presses, side, Right, (5.0, 20.0), false);
            assert_eq!(right_up.map(|r| r.id), Some(3), "{side:?} right release");
            assert_eq!(presses, MeshPresses::default(), "{side:?}");
        }
    }

    /// 지난 누름이 남아 있어도 새 누름은 포인터 아래로 정한다. 빗나간 누름은 그 버튼의 지난 기억을 지운다.
    #[test]
    fn a_new_press_replaces_a_stale_one() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(3, 70.0, MeshSide::Local),
        ];
        let mut presses = MeshPresses(vec![(Left, pressed(1, MeshSide::Local))]);
        let down = button_target(
            &regions,
            &mut presses,
            MeshSide::Local,
            Left,
            (5.0, 80.0),
            true,
        );
        assert_eq!(down.map(|r| r.id), Some(3));
        assert_eq!(
            presses,
            MeshPresses(vec![(Left, pressed(3, MeshSide::Local))])
        );
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
