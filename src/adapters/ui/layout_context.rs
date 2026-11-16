/// 팝업·토스트의 표시 범위와 화면 안 배치를 계산할 pane·surface·workspace 영역.
pub struct LayoutContext {
    pub active_workspace: usize,
    /// 팝업 범위는 순서가 바뀌어도 대상을 따라가도록 ID로 비교한다.
    pub active_workspace_id: u32,
    /// (pane_id, rect) for all visible panes.
    pub pane_rects: Vec<(u32, egui::Rect)>,
    /// (surface_id, rect) for all visible surfaces.
    pub surface_rects: Vec<(u32, egui::Rect)>,
    /// (pane_id, active_tab_index) for each pane.
    pub active_tabs: Vec<(u32, usize)>,
    /// (pane_id, active_tab_id) for each pane.
    pub active_tab_ids: Vec<(u32, u32)>,
    /// 활성 workspace 의 콘텐츠 영역(탭 바와 pane). 사이드바·상태바는 들어가지 않는다.
    pub workspace_rect: Option<egui::Rect>,
    /// 보이는 surface 마다 소속 (surface_id, tab_id, pane_id). 이벤트의 범위 경로를 계산할 때 쓴다.
    pub surface_owners: Vec<SurfaceOwner>,
}

/// surface 하나가 속한 tab 과 pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceOwner {
    pub surface: u32,
    pub tab: u32,
    pub pane: u32,
}

impl LayoutContext {
    /// A window without workspaces or panes, for a window that draws only `Window` scope toasts
    /// (the quit dialog and the boot screens).
    pub(crate) fn window_only() -> Self {
        Self {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: Vec::new(),
            surface_rects: Vec::new(),
            active_tabs: Vec::new(),
            active_tab_ids: Vec::new(),
            workspace_rect: None,
            surface_owners: Vec::new(),
        }
    }
}

/// 현재 상태와 레이아웃으로 팝업·토스트·배너가 공유할 영역 정보를 만든다.
pub(crate) fn build_layout_context(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pane_rects: &[(u32, crate::model::PhysicalRect)],
    terminal_rect: crate::model::PhysicalRect,
    scale_factor: f32,
) -> LayoutContext {
    let active_workspace = state.active_workspace_index(engine);

    let pane_rects_logical: Vec<(u32, egui::Rect)> = pane_rects
        .iter()
        .map(|(id, r)| (*id, crate::adapters::ui::to_egui_rect(*r, scale_factor)))
        .collect();

    let mut active_tabs = Vec::new();
    let mut active_tab_ids = Vec::new();
    let ws = state.active_workspace(engine);
    for &pid in &ws.pane_layout().all_pane_ids() {
        if let Some(pane) = ws.pane_layout().find_pane(pid) {
            active_tabs.push((pid, state.navigation.tab_index(pane)));
            if let Some(tab) = pane.tabs.get(state.navigation.tab_index(pane)) {
                active_tab_ids.push((pid, tab.id));
            }
        }
    }

    let mut surface_rects = Vec::new();
    let mut surface_owners = Vec::new();
    for (pane_id, _pane_rect, regions) in state.surface_regions(engine, terminal_rect, scale_factor)
    {
        let tab = active_tab_ids
            .iter()
            .find(|(pid, _)| *pid == pane_id)
            .map(|(_, tid)| *tid);
        for r in regions {
            surface_rects.push((
                r.id,
                crate::adapters::ui::to_egui_rect(r.rect, scale_factor),
            ));
            if let Some(tab) = tab {
                surface_owners.push(SurfaceOwner {
                    surface: r.id,
                    tab,
                    pane: pane_id,
                });
            }
        }
    }

    LayoutContext {
        active_workspace,
        active_workspace_id: ws.id,
        pane_rects: pane_rects_logical,
        surface_rects,
        active_tabs,
        active_tab_ids,
        workspace_rect: Some(crate::adapters::ui::to_egui_rect(
            terminal_rect,
            scale_factor,
        )),
        surface_owners,
    }
}
