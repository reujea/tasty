#[cfg(feature = "gui")]
use crate::core::CoreState;
#[cfg(feature = "gui")]
use crate::model::{PaneId, PhysicalPx, PhysicalRect, SurfaceRegion};
#[cfg(feature = "gui")]
use crate::runtime::engine_read::EngineRead;

use super::RequestContext;

impl RequestContext {
    /// Compute all surface regions for the active workspace.
    /// Returns: for each pane, the pane rect and all surface regions within it.
    #[cfg(feature = "gui")]
    pub fn surface_regions<'a>(
        &self,
        engine: &'a CoreState,
        terminal_rect: PhysicalRect,
        scale_factor: f32,
    ) -> Vec<(PaneId, PhysicalRect, Vec<SurfaceRegion<'a>>)> {
        let ws = self.active_workspace(engine);
        let pane_rects = self.pane_rects(engine, ws, terminal_rect, scale_factor);

        let mut result = Vec::new();
        for (pane_id, pane_rect) in pane_rects {
            if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                let content_rect = self.tab_content_rect(pane_rect);
                let regions = match self.drawn_tab(pane) {
                    Some(tab) => self.tab_surface_regions(engine, tab, content_rect, scale_factor),
                    None => Vec::new(),
                };
                result.push((pane_id, pane_rect, regions));
            }
        }
        result
    }

    /// 그리는 터미널 중 마지막 레이아웃 스윕이 그리는 것으로 처리하지 않은 것이 있는가. 그런 터미널은
    /// 옛 grid로 그려지므로 다음 redraw가 다시 스윕해야 한다. attach draft가 자르는 surface는 스윕이
    /// 늘 건너뛰므로 세지 않는다(세면 redraw가 반복된다).
    #[cfg(feature = "gui")]
    pub(crate) fn draws_unswept_terminal(
        &self,
        engine: &EngineRead<'_>,
        regions: &[(PaneId, PhysicalRect, Vec<SurfaceRegion<'_>>)],
    ) -> bool {
        regions.iter().flat_map(|(_, _, r)| r).any(|region| {
            !self.swept_drawn.contains(&region.id)
                && engine.terminals.contains(region.id)
                && !self.attach_pending.clips_surface(engine, region.id)
        })
    }

    /// pane 사각형에서 탭 바를 뺀 탭 콘텐츠 영역. 활성 탭의 surface 들은 이 안에 배치된다.
    #[cfg(feature = "gui")]
    pub(crate) fn tab_content_rect(&self, pane_rect: PhysicalRect) -> PhysicalRect {
        let tab_bar_h = self.tab_bar_height;
        PhysicalRect {
            x: pane_rect.x,
            y: pane_rect.y + tab_bar_h,
            width: pane_rect.width,
            height: (pane_rect.height - tab_bar_h).max(PhysicalPx(1.0)),
        }
    }

    /// 보이지 않는 탭도 포함해 모든 egui-mesh surface의 ID와 소유 플러그인을 반환한다.
    /// 텍스처·전송 상태는 가시성이 아니라 surface 수명에 맞춰 유지해야 한다.
    #[cfg(feature = "gui")]
    pub fn egui_mesh_surfaces_existing(&self, engine: &EngineRead<'_>) -> Vec<(u32, String)> {
        let mut out: Vec<(u32, String)> = Vec::new();
        for ws in &engine.workspaces() {
            for pane_id in ws.pane_layout().all_pane_ids() {
                let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
                    continue;
                };
                for tab in &pane.tabs {
                    let Some(layout) = tab.layout_if_initialized() else {
                        continue;
                    };
                    for sid in layout.all_surface_ids() {
                        if let Some(s) = engine.find_surface_by_id(sid)
                            && let Some(ms) = s.mesh()
                        {
                            out.push((sid, ms.plugin_id.clone()));
                        }
                    }
                }
            }
        }
        out
    }

    /// 모든 탭에 있는 attach mesh mirror의 로컬 ID. 로컬 플러그인 프로세스는 조회하지 않는다.
    #[cfg(feature = "gui")]
    pub fn attach_mesh_surfaces_existing(&self, engine: &EngineRead<'_>) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for ws in &engine.workspaces() {
            for pane_id in ws.pane_layout().all_pane_ids() {
                let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
                    continue;
                };
                for tab in &pane.tabs {
                    let Some(layout) = tab.layout_if_initialized() else {
                        continue;
                    };
                    for sid in layout.all_surface_ids() {
                        if let Some(s) = engine.find_surface_by_id(sid)
                            && s.attach_mesh().is_some()
                        {
                            out.push(sid);
                        }
                    }
                }
            }
        }
        out
    }

    /// Get the actual content rect for the focused surface (accounting for tab bar).
    /// Returns None if no surface is focused.
    #[cfg(feature = "gui")]
    pub fn focused_surface_rect(
        &self,
        engine: &CoreState,
        terminal_rect: PhysicalRect,
        scale_factor: f32,
    ) -> Option<PhysicalRect> {
        let surface_id = self.focused_surface_id(engine)?;
        for (_pane_id, _pane_rect, regions) in
            &self.surface_regions(engine, terminal_rect, scale_factor)
        {
            for r in regions {
                if r.id == surface_id {
                    return Some(r.rect);
                }
            }
        }
        None
    }

    /// Get the physical pixel rect of a specific terminal cell within a surface.
    #[allow(clippy::too_many_arguments)] // reason: 셀 위치 계산에 surface·행·열과 화면 좌표 정보가 함께 필요하다
    #[cfg(feature = "gui")]
    pub fn surface_cell_rect(
        &self,
        engine: &CoreState,
        terminal_rect: PhysicalRect,
        surface_id: u32,
        col: usize,
        row: usize,
        cell_w: f32,
        cell_h: f32,
        scale_factor: f32,
    ) -> Option<PhysicalRect> {
        for (_pane_id, _pane_rect, regions) in
            &self.surface_regions(engine, terminal_rect, scale_factor)
        {
            for r in regions {
                if r.id == surface_id {
                    return Some(PhysicalRect {
                        x: r.rect.x + PhysicalPx(col as f32 * cell_w),
                        y: r.rect.y + PhysicalPx(row as f32 * cell_h),
                        width: PhysicalPx(cell_w.max(1.0)),
                        height: PhysicalPx(cell_h.max(1.0)),
                    });
                }
            }
        }
        None
    }

    /// Get the rect of a specific surface by id.
    #[cfg(feature = "gui")]
    pub fn surface_rect_by_id(
        &self,
        engine: &CoreState,
        surface_id: u32,
        terminal_rect: PhysicalRect,
        scale_factor: f32,
    ) -> Option<PhysicalRect> {
        for (_pane_id, _pane_rect, regions) in
            &self.surface_regions(engine, terminal_rect, scale_factor)
        {
            for r in regions {
                if r.id == surface_id {
                    return Some(r.rect);
                }
            }
        }
        None
    }

    /// Find the surface ID at the given physical pixel position.
    #[cfg(feature = "gui")]
    pub fn surface_id_at_position(
        &self,
        engine: &CoreState,
        x: f32,
        y: f32,
        terminal_rect: PhysicalRect,
        scale_factor: f32,
    ) -> Option<u32> {
        for (_pane_id, _pane_rect, regions) in
            &self.surface_regions(engine, terminal_rect, scale_factor)
        {
            for r in regions {
                if r.rect.contains(PhysicalPx(x), PhysicalPx(y)) {
                    return Some(r.id);
                }
            }
        }
        None
    }

    /// View geometry로 고정한 크기를 AppServices::resize_terminals에 넘긴다. 점유·mirror 및 grid/tap/OS 순서는 실행 경계가 유지한다.
    /// PTY resize는 미뤄지므로 호출자가 resize 이벤트 처리 후 flush_all_pty_resizes를 호출해야 한다.
    #[cfg(feature = "gui")]
    pub fn resize_all(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        terminal_rect: PhysicalRect,
        cell_width: f32,
        cell_height: f32,
        scale_factor: f32,
    ) {
        let mut targets = Vec::new();
        let mut swept_drawn = std::collections::HashSet::new();
        // 그리는 집합(`surface_regions`)과 같은 workspace·탭 선택을 쓴다. 그리지 않는 target은
        // 목표만 기록되고 보이게 된 첫 스윕에 적용된다(docs/design/policies/render-on-draw.md).
        let active_workspace =
            (!engine.workspaces().is_empty()).then(|| self.active_workspace(engine).id);
        for workspace in engine.workspaces().iter() {
            let workspace_drawn = self.window_drawn && Some(workspace.id) == active_workspace;
            for (pane_id, pane_rect) in
                self.pane_rects(engine, workspace, terminal_rect, scale_factor)
            {
                let Some(pane) = workspace.pane_layout().find_pane(pane_id) else {
                    continue;
                };
                let content = PhysicalRect {
                    x: pane_rect.x,
                    y: pane_rect.y + self.tab_bar_height,
                    width: pane_rect.width,
                    height: (pane_rect.height - self.tab_bar_height).max(PhysicalPx(1.0)),
                };
                let drawn_tab = self.drawn_tab(pane).map(|tab| tab.id);
                for tab in &pane.tabs {
                    if tab.layout_if_initialized().is_none() {
                        continue;
                    }
                    let drawn = workspace_drawn && drawn_tab == Some(tab.id);
                    for region in self.tab_surface_regions(engine, tab, content, scale_factor) {
                        // A draft clips the old grid. Only the committed Delta may resize it.
                        if self.attach_pending.clips_surface(engine, region.id) {
                            continue;
                        }
                        let cols = ((region.rect.width.value() / cell_width.max(1.0)).floor()
                            as usize)
                            .max(1);
                        let rows = ((region.rect.height.value() / cell_height.max(1.0)).floor()
                            as usize)
                            .max(1);
                        if drawn {
                            swept_drawn.insert(region.id);
                        }
                        if let Some(binding) =
                            crate::runtime::surface_binding::SurfaceBinding::capture(
                                engine, region.id,
                            )
                        {
                            targets.push((
                                binding,
                                crate::app::frame_effect::LayoutTarget {
                                    surface_id: region.id,
                                    cols,
                                    rows,
                                    drawn,
                                },
                            ));
                        }
                    }
                }
            }
        }
        self.swept_drawn = swept_drawn;
        self.dispatch_intent(
            crate::intent::Intent::Frame(
                crate::app::frame_effect::GeometryEffect::Resize { targets }.into(),
            )
            .from_user_shortcut("layout-resize"),
        );
    }
}
