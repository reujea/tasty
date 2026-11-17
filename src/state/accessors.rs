//! 활성 워크스페이스·pane·surface 접근. 워크스페이스가 없을 수 있는 호출자는 Option 또는 빈 목록 검사를 사용한다.

#[cfg(feature = "gui")]
use crate::runtime::engine_read::EngineRead;
#[cfg(feature = "gui")]
use tasty_terminal::Terminal;

use super::RequestContext;
use crate::core::CoreState;

impl RequestContext {
    pub(crate) fn active_workspace_index(&self, engine: &CoreState) -> usize {
        self.navigation.workspace_index(&engine.workspaces())
    }

    #[cfg(any(feature = "gui", debug_assertions, test))]
    pub(crate) fn set_active_workspace_index(&mut self, engine: &CoreState, index: usize) {
        if let Some(ws) = engine.workspace_at(index) {
            self.navigation
                .select_workspace(&engine.workspaces(), ws.id);
        }
    }

    #[cfg(feature = "gui")]
    pub(crate) fn select_pane(&mut self, engine: &CoreState, pane_id: u32) {
        if let Some(ws) = engine
            .workspaces()
            .into_iter()
            .find(|ws| ws.pane_layout().find_pane(pane_id).is_some())
        {
            self.navigation.select_pane(ws, pane_id);
        }
    }

    /// Retire presentation entries with their structure. Unlike the old fields
    /// on domain objects, these maps do not disappear when the objects drop.
    pub(crate) fn reconcile_presentation(&mut self, engine: &CoreState) {
        self.navigation.reconcile(&engine.workspaces());
        #[cfg(feature = "gui")]
        self.layout_previews.retain(engine);
        #[cfg(feature = "gui")]
        self.attach_pending.retain(engine);
        #[cfg(feature = "gui")]
        self.terminal_views.retain(engine);
        #[cfg(feature = "gui")]
        {
            // Reconcile against current structure, not delayed close notifications. In particular,
            // a successor under the same ID still has a descriptor and keeps its View caches.
            let removed: std::collections::HashSet<_> = self
                .explorer_views
                .cached_surfaces()
                .chain(self.dag_graph_views.cached_surfaces())
                .chain(self.shell_integration_hint_shown.iter().copied())
                .filter(|id| !engine.has_surface(*id))
                .collect();
            for surface in removed {
                self.release_surface_views(surface);
            }
        }
        self.navigation
            .collapsed_categories
            .retain(|id| engine.categories().iter().any(|c| c.id == *id));
        #[cfg(any(feature = "gui", debug_assertions, test))]
        self.category_last_active.retain(|category, workspace| {
            engine
                .workspaces()
                .into_iter()
                .any(|ws| ws.id == *workspace)
                && engine.categories().iter().any(|c| c.id == *category)
        });
        #[cfg(feature = "gui")]
        self.tab_bar_scroll
            .retain(|id, _| engine.find_pane_by_id(*id).is_some());
    }

    /// Invariant: caller must ensure `engine.workspaces()` is non-empty.
    /// Parked states (after the last window closes) can have zero workspaces —
    /// such callers must use `engine.workspaces().is_empty()` checks instead.
    pub fn active_workspace<'a>(&self, engine: &'a CoreState) -> &'a crate::model::Workspace {
        debug_assert!(
            !engine.workspaces().is_empty(),
            "active_workspace called with empty workspaces"
        );
        let idx = self
            .active_workspace_index(engine)
            .min(engine.workspaces().len().saturating_sub(1));
        engine.workspace_at(idx).expect("workspace index is valid")
    }

    /// Get the focused pane in the active workspace, or the first pane as fallback.
    /// Returns `None` if no workspaces exist (parked state after last-window close).
    pub fn focused_pane<'a>(&self, engine: &'a CoreState) -> Option<&'a crate::model::Pane> {
        if engine.workspaces().is_empty() {
            return None;
        }
        let ws = self.active_workspace(engine);
        let layout = ws.pane_layout();
        layout
            .find_pane(self.navigation.pane_id(ws)?)
            .or_else(|| layout.first_pane())
    }

    pub fn focused_surface_id(&self, engine: &CoreState) -> Option<u32> {
        #[cfg(feature = "gui")]
        if self
            .attach_pending
            .selected(engine, &self.navigation)
            .is_some()
        {
            return None;
        }
        let pane = self.focused_pane(engine)?;
        let tab = pane.tabs.get(self.navigation.tab_index(pane))?;
        self.navigation.surface_id(tab)
    }

    /// 명령을 입력한 지금의 대상. 생성 대기 초안이 선택돼 있으면 그 초안이다.
    /// 키 입력 라우팅은 계속 `focused_surface_id`를 보며 초안에는 입력하지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) fn command_surface_target(
        &self,
        engine: &CoreState,
    ) -> Option<super::attach_pending::deferred::CommandSurfaceTarget> {
        use super::attach_pending::deferred::CommandSurfaceTarget;
        if let Some(entry) = self.attach_pending.selected(engine, &self.navigation) {
            return (entry.creation.is_some() && entry.visible)
                .then_some(CommandSurfaceTarget::Pending(entry.id));
        }
        self.focused_surface_id(engine)
            .map(CommandSurfaceTarget::Surface)
    }

    #[cfg(feature = "gui")]
    pub fn focused_terminal<'a>(&self, engine: &EngineRead<'a>) -> Option<&'a Terminal> {
        let id = self.focused_surface_id(engine)?;
        engine.terminals.get(id)
    }

    pub fn focused_pane_id(&self, engine: &CoreState) -> crate::model::PaneId {
        self.navigation
            .pane_id(self.active_workspace(engine))
            .unwrap_or(0)
    }

    #[cfg(feature = "gui")]
    pub(crate) fn switch_overlay(
        &self,
    ) -> Option<crate::adapters::ui::switch_overlay::SwitchOverlayState> {
        self.switch_overlay
    }

    /// 수식키에 맞는 숫자 전환 안내를 갱신하고 변경 여부를 반환한다.
    /// ctrl/shift/alt는 플랫폼별 정규화를 마친 값이어야 한다.
    #[cfg(feature = "gui")]
    pub(crate) fn update_switch_overlay(
        &mut self,
        engine: &EngineRead<'_>,
        kb: &crate::settings::KeybindingSettings,
        ctrl: bool,
        shift: bool,
        alt: bool,
        option: bool,
    ) -> bool {
        use crate::adapters::ui::switch_overlay::{
            SwitchOverlayState, SwitchTarget, switch_target_for,
        };
        let next = switch_target_for(kb, ctrl, shift, alt, option)
            .filter(|t| {
                *t != SwitchTarget::Category || engine.settings.general.workspace_categories_enabled
            })
            .map(|target| {
                let pane_id = match target {
                    SwitchTarget::Tab if !engine.workspaces().is_empty() => {
                        Some(self.focused_pane_id(engine))
                    }
                    _ => None,
                };
                SwitchOverlayState { target, pane_id }
            });
        let changed = next != self.switch_overlay;
        self.switch_overlay = next;
        changed
    }

    /// 숫자 전환 안내를 지우고 변경 여부를 반환한다.
    #[cfg(feature = "gui")]
    pub(crate) fn clear_switch_overlay(&mut self) -> bool {
        let changed = self.switch_overlay.is_some();
        self.switch_overlay = None;
        changed
    }
}
