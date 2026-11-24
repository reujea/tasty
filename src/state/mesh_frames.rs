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
                let rect = if super::surface_focus::reserves_edge(engine, r.id) {
                    super::surface_focus::below_edge(&theme, r.rect, scale_factor)
                } else {
                    r.rect
                };
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
