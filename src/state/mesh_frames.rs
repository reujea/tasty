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

/// 버튼 사건의 `side` 대상. 누름은 포인터 아래 leaf를 기억하고, 뗌은 기억한 leaf로 보낸 뒤 잊는다.
/// 다른 쪽이 누른 뗌은 이쪽이 받지 않는다. 이쪽에서 빗나간 누름은 이쪽의 지난 기억을 지운다.
pub(crate) fn button_target(
    regions: &[MeshFrameRegion],
    press: &mut Option<MeshPress>,
    side: MeshSide,
    (x, y): (f32, f32),
    pressed: bool,
) -> Option<MeshFrameRegion> {
    let mine = press.filter(|p| p.side == side);
    if pressed {
        let hit = pointer_target(regions, None, side, x, y);
        match hit {
            Some(r) => *press = Some(MeshPress { id: r.id, side }),
            None if mine.is_some() => *press = None,
            None => {}
        }
        return hit;
    }
    match (*press, mine) {
        (_, Some(p)) => {
            *press = None;
            pointer_target(regions, Some(p), side, x, y)
        }
        (Some(_), None) => None,
        (None, None) => pointer_target(regions, None, side, x, y),
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

    /// 본문에서 누르고 같은 leaf의 띠 행(영역 밖 위쪽)에서 떼면 뗌도 그 leaf로 간다. 기억은 뗌 뒤 지운다.
    #[test]
    fn a_release_over_the_edge_row_reaches_the_pressed_mesh() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(2, 70.0, MeshSide::Attach),
        ];
        let mut press = None;
        let down = button_target(&regions, &mut press, MeshSide::Local, (5.0, 20.0), true);
        assert_eq!(down.map(|r| r.id), Some(1));
        assert_eq!(
            press,
            Some(MeshPress {
                id: 1,
                side: MeshSide::Local
            })
        );
        assert_eq!(
            pointer_target(&regions, press, MeshSide::Local, 5.0, 9.0).map(|r| r.id),
            Some(1),
            "a move over the edge row stays with the pressed mesh"
        );
        let up = button_target(&regions, &mut press, MeshSide::Local, (5.0, 9.0), false);
        assert_eq!(up.map(|r| r.id), Some(1));
        assert_eq!(press, None);
        let later = button_target(&regions, &mut press, MeshSide::Local, (5.0, 9.0), false);
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
        let mut press = None;
        assert!(button_target(&regions, &mut press, MeshSide::Local, (5.0, 80.0), true).is_none());
        let down = button_target(&regions, &mut press, MeshSide::Attach, (5.0, 80.0), true);
        assert_eq!(down.map(|r| r.id), Some(2));
        assert!(button_target(&regions, &mut press, MeshSide::Local, (5.0, 20.0), false).is_none());
        let up = button_target(&regions, &mut press, MeshSide::Attach, (5.0, 20.0), false);
        assert_eq!(up.map(|r| r.id), Some(2));
        assert_eq!(press, None);
    }

    /// 지난 누름이 남아 있어도 새 누름은 포인터 아래로 정한다. 빗나간 누름은 지난 기억을 지운다.
    #[test]
    fn a_new_press_replaces_a_stale_one() {
        let regions = [
            region(1, 10.0, MeshSide::Local),
            region(3, 70.0, MeshSide::Local),
        ];
        let mut press = Some(MeshPress {
            id: 1,
            side: MeshSide::Local,
        });
        let down = button_target(&regions, &mut press, MeshSide::Local, (5.0, 80.0), true);
        assert_eq!(down.map(|r| r.id), Some(3));
        assert_eq!(
            press,
            Some(MeshPress {
                id: 3,
                side: MeshSide::Local
            })
        );
        assert!(button_target(&regions, &mut press, MeshSide::Local, (5.0, 200.0), true).is_none());
        assert_eq!(press, None);
    }
}
