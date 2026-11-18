use crate::core::CoreState;
use crate::model::PhysicalPx;

use super::RequestContext;

impl RequestContext {
    pub fn move_pane_focus_forward(&mut self, engine: &CoreState) {
        let target = self.command_target(engine);
        self.move_pane_focus_in(engine, &target, true);
    }

    pub fn move_pane_focus_backward(&mut self, engine: &CoreState) {
        let target = self.command_target(engine);
        self.move_pane_focus_in(engine, &target, false);
    }

    /// 명령 대상 workspace 에서 대상 pane 의 다음·이전 pane 으로 포커스를 옮긴다.
    pub(crate) fn move_pane_focus_in(
        &mut self,
        engine: &CoreState,
        target: &super::command_target::CommandTarget,
        forward: bool,
    ) {
        self.attach_pending.deselect();
        let (Some(ws), Some(id)) = (target.find_workspace(engine), target.pane) else {
            return;
        };
        let layout = ws.pane_layout();
        let next = if forward {
            layout.next_pane_id(id)
        } else {
            layout.prev_pane_id(id)
        };
        self.navigation.select_pane(ws, next);
    }

    pub fn move_surface_focus_forward(&mut self, engine: &CoreState) {
        let target = self.command_target(engine);
        self.move_surface_focus_in(engine, &target, false);
    }

    pub fn move_surface_focus_backward(&mut self, engine: &CoreState) {
        let target = self.command_target(engine);
        self.move_surface_focus_in(engine, &target, true);
    }

    /// 명령 대상 탭 안에서 대상 surface 의 다음·이전 surface 를 고른다. 입력 때 초안이 선택돼 있었으면 그 탭에서
    /// 입력 때 골라져 있던 surface 를 기준으로 하고, 그 사이 초안이 사라졌으면 하지 않는다.
    pub(crate) fn move_surface_focus_in(
        &mut self,
        engine: &CoreState,
        target: &super::command_target::CommandTarget,
        backward: bool,
    ) {
        #[cfg(feature = "gui")]
        if let Some(draft) = target.draft
            && !self.attach_pending.draft_live(engine, draft)
        {
            tracing::info!(
                "surface focus step ignored: the target draft {} is gone",
                draft.0
            );
            return;
        }
        self.attach_pending.deselect();
        let Some((_, _, tab)) = target.find_tab(engine) else {
            return;
        };
        let ids = tab.all_surface_ids();
        if ids.is_empty() {
            return;
        }
        let current = match target.draft {
            Some(_) => target.draft_base_surface,
            None => target
                .surface_id()
                .or_else(|| self.navigation.surface_id(tab)),
        };
        let position = ids.iter().position(|id| Some(*id) == current).unwrap_or(0);
        let step = if backward { ids.len() - 1 } else { 1 };
        self.navigation
            .select_surface(tab, ids[(position + step) % ids.len()]);
    }

    pub fn focus_pane_at_position(
        &mut self,
        engine: &CoreState,
        x: f32,
        y: f32,
        terminal_rect: crate::model::PhysicalRect,
        scale_factor: f32,
    ) -> bool {
        let Some(pane_id) = self.pane_at_position(engine, x, y, terminal_rect, scale_factor) else {
            return false;
        };
        let ws = self.active_workspace(engine);
        self.navigation.select_pane(ws, pane_id)
    }

    /// 활성 workspace 에서 좌표 아래 pane(탭 바 포함). 포커스는 바꾸지 않는다.
    pub fn pane_at_position(
        &self,
        engine: &CoreState,
        x: f32,
        y: f32,
        terminal_rect: crate::model::PhysicalRect,
        scale_factor: f32,
    ) -> Option<u32> {
        let ws = self.active_workspace(engine);
        #[cfg(feature = "gui")]
        let pane_rects = self.pane_rects(engine, ws, terminal_rect, scale_factor);
        #[cfg(not(feature = "gui"))]
        let pane_rects = ws.pane_layout().compute_rects(terminal_rect, scale_factor);
        pane_rects
            .into_iter()
            .find(|(_, rect)| rect.contains(PhysicalPx(x), PhysicalPx(y)))
            .map(|(pane_id, _)| pane_id)
    }

    pub fn focus_surface_at_position(
        &mut self,
        engine: &CoreState,
        x: f32,
        y: f32,
        terminal_rect: crate::model::PhysicalRect,
        scale_factor: f32,
    ) -> bool {
        let Some(id) = self.surface_at_position(engine, x, y, terminal_rect, scale_factor) else {
            return false;
        };
        self.attach_pending.deselect();
        let ws = self.active_workspace(engine);
        let Some(pane_id) = self.navigation.pane_id(ws) else {
            return false;
        };
        let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
            return false;
        };
        let Some(tab) = pane.tabs.get(self.navigation.tab_index(pane)) else {
            return false;
        };
        self.navigation.select_surface(tab, id)
    }

    /// 포커스된 pane 의 보이는 탭에서 좌표 아래 surface. 포커스는 바꾸지 않는다.
    pub fn surface_at_position(
        &self,
        engine: &CoreState,
        x: f32,
        y: f32,
        terminal_rect: crate::model::PhysicalRect,
        scale_factor: f32,
    ) -> Option<u32> {
        let ws = self.active_workspace(engine);
        let pane_id = self.navigation.pane_id(ws)?;
        #[cfg(feature = "gui")]
        let pane_rects = self.pane_rects(engine, ws, terminal_rect, scale_factor);
        #[cfg(not(feature = "gui"))]
        let pane_rects = ws.pane_layout().compute_rects(terminal_rect, scale_factor);
        let (_, rect) = pane_rects.into_iter().find(|(id, _)| *id == pane_id)?;
        let content = crate::model::PhysicalRect {
            x: rect.x,
            y: rect.y + self.tab_bar_height,
            width: rect.width,
            height: (rect.height - self.tab_bar_height).max(PhysicalPx(1.0)),
        };
        let pane = ws.pane_layout().find_pane(pane_id)?;
        let tab = pane.tabs.get(self.navigation.tab_index(pane))?;
        #[cfg(feature = "gui")]
        let surface = self
            .tab_surface_regions(engine, tab, content, scale_factor)
            .into_iter()
            .find(|region| region.rect.contains(PhysicalPx(x), PhysicalPx(y)))
            .map(|region| region.id);
        #[cfg(not(feature = "gui"))]
        let surface = tab.layout().find_surface_at(x, y, content);
        surface
    }

    /// Native input may focus a surface only in a currently displayed tab.
    pub fn focus_surface_by_id(&mut self, engine: &CoreState, surface_id: u32) -> bool {
        self.attach_pending.deselect();
        let ws = self.active_workspace(engine);
        for pane_id in ws.pane_layout().all_pane_ids() {
            let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
                continue;
            };
            let Some(tab) = pane.tabs.get(self.navigation.tab_index(pane)) else {
                continue;
            };
            if tab.contains_surface(surface_id) {
                let pane_changed = self.navigation.select_pane(ws, pane_id);
                let surface_changed = self.navigation.select_surface(tab, surface_id);
                return pane_changed || surface_changed;
            }
        }
        false
    }
}
