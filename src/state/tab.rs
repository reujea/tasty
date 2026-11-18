#[cfg(feature = "gui")]
use crate::runtime::engine_read::EngineRead;
#[cfg(feature = "gui")]
use serde_json::Value;

#[cfg(any(feature = "gui", debug_assertions))]
use tasty_model::TabSwitch;

use super::RequestContext;
#[cfg(any(feature = "gui", debug_assertions, test))]
use crate::core::CoreState;

impl RequestContext {
    /// Resolve the menu owner once; App admits and executes the explicit creation command.
    /// cwd comes from the owner surface, not the focused one, so the result does not depend on focus.
    #[cfg(feature = "gui")]
    pub fn add_kind_tab_by_owner(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        owner_surface_id: u32,
        kind: &str,
        params: &Value,
    ) -> anyhow::Result<()> {
        let (_, pane_id) = engine
            .find_workspace_index_for_surface(owner_surface_id)
            .ok_or_else(|| anyhow::anyhow!("owner surface {owner_surface_id} not found"))?;
        let cwd = self.resolve_inherit_cwd_from_surface(engine, owner_surface_id);
        self.dispatch_intent(
            crate::app::command::DomainIntent::CreateTab {
                pane_id,
                cwd,
                cwd_source: Some(owner_surface_id),
                kind: kind.into(),
                name: None,
                surface_params: params.clone(),
                activate: true,
            }
            .from_user_context_menu(),
        );
        Ok(())
    }

    #[cfg(feature = "gui")]
    pub fn set_explorer_cwd(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        sid: u32,
        folder: std::path::PathBuf,
    ) {
        let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        else {
            return;
        };
        self.dispatch_intent(
            crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::ExplorerCwd {
                target,
                folder,
            })
            .from_user_context_menu(),
        );
        #[cfg(feature = "gui")]
        if let Some(view) = self.explorer_views.get_mut(sid) {
            view.request_reload();
        }
    }

    #[cfg(feature = "gui")]
    pub fn next_tab_in_pane(&mut self, engine: &CoreState) {
        let target = self.command_target(engine);
        self.step_visible_tab_in(engine, &target, true);
    }

    #[cfg(feature = "gui")]
    pub fn prev_tab_in_pane(&mut self, engine: &CoreState) {
        let target = self.command_target(engine);
        self.step_visible_tab_in(engine, &target, false);
    }

    /// 명령 대상 pane 에서 대상 탭(입력 때 초안이 선택돼 있었으면 그 초안)의 다음·이전 탭으로 바꾼다.
    #[cfg(feature = "gui")]
    pub(crate) fn step_visible_tab_in(
        &mut self,
        engine: &CoreState,
        target: &super::command_target::CommandTarget,
        forward: bool,
    ) {
        let Some(pane) = target.find_pane(engine) else {
            return;
        };
        let drafts = self.visible_draft_tabs(engine, pane.id);
        let real = pane.tabs.len();
        let count = real + drafts.len();
        if count == 0 {
            return;
        }
        // 입력 때 초안이 대상이었으면 그 초안 자리에서만 움직인다. 초안이 사라졌으면 뒤의 기존 탭으로 바꿔
        // 읽지 않는다.
        let selected = match target.draft {
            Some(draft) => {
                let Some(index) = drafts.iter().position(|id| *id == draft) else {
                    tracing::info!("tab step ignored: the target draft {} is gone", draft.0);
                    return;
                };
                real + index
            }
            None => {
                let Some((_, index, _)) = target.find_tab(engine) else {
                    tracing::info!("tab step ignored: the target tab is gone");
                    return;
                };
                index
            }
        };
        let next = if forward {
            (selected + 1) % count
        } else {
            (selected + count - 1) % count
        };
        self.goto_tab_in(engine, Some(pane.id), next);
    }

    #[cfg(feature = "gui")]
    fn visible_draft_tabs(
        &self,
        engine: &CoreState,
        pane: u32,
    ) -> Vec<super::attach_pending::PendingId> {
        self.attach_pending
            .visible(engine, self.active_workspace(engine).id)
            .filter(|entry| {
                entry.pane == pane
                    && matches!(entry.creation, Some(super::attach_pending::Creation::Tab))
            })
            .map(|entry| entry.id)
            .collect()
    }

    /// 포커스된 pane에서 0-based 인덱스로 탭을 바꾼다. pane 부재와 범위 오류를 구분한다. 사용자 입력은 명령
    /// 대상의 pane 으로 `goto_tab_in` 을 부르고, 이 함수는 debug IPC 와 시험만 쓴다.
    #[cfg(any(all(test, feature = "gui"), debug_assertions))]
    pub fn goto_tab_in_pane(&mut self, engine: &CoreState, index: usize) -> TabSwitch {
        let pane = self.focused_pane(engine).map(|p| p.id);
        self.goto_tab_in(engine, pane, index)
    }

    /// `pane` 에서 0-based 인덱스로 탭을 바꾼다. 실제 탭 뒤의 번호는 그 pane 의 새 탭 초안이다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn goto_tab_in(
        &mut self,
        engine: &CoreState,
        pane: Option<u32>,
        index: usize,
    ) -> TabSwitch {
        let pane = pane.and_then(|id| engine.find_pane_by_id(id));
        #[cfg(feature = "gui")]
        if let Some(pane) = pane {
            let drafts = self.visible_draft_tabs(engine, pane.id);
            if index >= pane.tabs.len() {
                if let Some(id) = drafts.get(index - pane.tabs.len()) {
                    let already = self
                        .attach_pending
                        .selected(engine, &self.navigation)
                        .is_some_and(|entry| entry.id == *id);
                    self.attach_pending.select(*id, &self.navigation);
                    return if already {
                        TabSwitch::AlreadyActive
                    } else {
                        TabSwitch::Switched
                    };
                }
                return TabSwitch::OutOfRange {
                    tabs: pane.tabs.len() + drafts.len(),
                };
            }
        }
        #[cfg(feature = "gui")]
        self.attach_pending.deselect();
        #[cfg(feature = "gui")]
        let before = self.tutorial_tab_snapshot(engine);
        let result = if let Some(pane) = pane {
            self.navigation.goto_tab(pane, index)
        } else {
            TabSwitch::NoPane
        };
        #[cfg(feature = "gui")]
        self.observe_tutorial_tab_switch(engine, before);
        result
    }

    /// 지정한 pane의 탭을 닫는다. 포커스와 무관하게 사본 저장·정리·dirty 갱신을 수행한다.
    #[cfg(feature = "gui")]
    pub fn close_tab(&mut self, engine: &EngineRead<'_>, pane_id: u32, tab_index: usize) -> bool {
        let mirror_op = self
            .active_workspace(engine)
            .pane_layout()
            .find_pane(pane_id)
            .and_then(|p| p.tabs.get(tab_index))
            .and_then(|t| self.navigation.surface_id(t))
            .map(|sid| crate::ipc::stream::StructuralOp::CloseTab {
                anchor_surface_id: sid,
            });
        let candidates = self
            .active_workspace(engine)
            .pane_layout()
            .find_pane(pane_id)
            .map(|pane| self.pane_sibling_tab_focus_candidates(pane, tab_index))
            .unwrap_or_default();
        if self.forward_mirror_structural(engine, mirror_op, candidates) {
            return true;
        }
        let in_tab: Vec<u32> = {
            let mut t = Vec::new();
            if let Some(pane) = self
                .active_workspace(engine)
                .pane_layout()
                .find_pane(pane_id)
                && let Some(tab) = pane.tabs.get(tab_index)
            {
                t.extend(tab.all_surface_ids());
            }
            t
        };
        if self.refuse_if_hard_occupied(engine, in_tab) {
            return false;
        }
        let Some(tab_id) = self
            .active_workspace(engine)
            .pane_layout()
            .find_pane(pane_id)
            .and_then(|pane| pane.tabs.get(tab_index))
            .map(|tab| tab.id)
        else {
            return false;
        };
        self.close_tab_through_core(engine, tab_id)
    }

    /// AppServices 탭 닫기로 트리를 바꾸고 복원 기록을 남긴 뒤 창 쪽 정리와 알림을 이어서 한다.
    #[cfg(feature = "gui")]
    fn close_tab_through_core(&mut self, engine: &EngineRead<'_>, tab_id: u32) -> bool {
        if engine.find_pane_for_tab(tab_id).is_none() {
            return false;
        }
        self.dispatch_intent(
            crate::app::command::DomainIntent::CloseTab { tab_id }.from_user_context_menu(),
        );
        true
    }

    /// 명령 대상의 탭을 닫는다. mirror 요청을 전달한 경우에도 true다. 대상 탭이 없으면 false 다.
    #[cfg(feature = "gui")]
    pub(crate) fn close_tab_in(
        &mut self,
        engine: &EngineRead<'_>,
        target: &super::command_target::CommandTarget,
    ) -> bool {
        let (Some(ws), Some((pane, index, tab))) =
            (target.find_workspace(engine), target.find_tab(engine))
        else {
            return false;
        };
        let mirror_op =
            self.navigation
                .surface_id(tab)
                .map(|sid| crate::ipc::stream::StructuralOp::CloseTab {
                    anchor_surface_id: sid,
                });
        let candidates = self.pane_sibling_tab_focus_candidates(pane, index);
        if self.forward_mirror_structural_in(ws.mirror, mirror_op, candidates) {
            return true;
        }
        if self.refuse_if_hard_occupied(engine, tab.all_surface_ids()) {
            return false;
        }
        self.close_tab_through_core(engine, tab.id)
    }

    /// 명령 대상 탭의 `rename_tab` 대상과 지금 보이는 이름. 입력 때 초안이 선택돼 있었으면 대상 탭은 초안 뒤의
    /// 기존 탭이므로 대상이 없다.
    #[cfg(feature = "gui")]
    pub(crate) fn tab_rename_in(
        &self,
        engine: &EngineRead<'_>,
        target: &super::command_target::CommandTarget,
    ) -> Option<(super::RenameTarget, String)> {
        if let Some(draft) = target.draft {
            tracing::info!(
                "rename_tab ignored: pending attach draft {} is selected",
                draft.0
            );
            return None;
        }
        let (_, _, tab) = target.find_tab(engine)?;
        let current_name = engine.tab_display_name(tab, self.navigation.surface_id(tab));
        Some((
            super::RenameTarget::TabName { tab_id: tab.id },
            current_name,
        ))
    }
}

#[cfg(feature = "gui")]
impl RequestContext {
    pub(crate) fn observe_tutorial_tab_created(&mut self, engine: &CoreState, pane: u32, tab: u32) {
        if self.tutorial.active.is_none() {
            return;
        }
        if let Some(ws) = engine
            .workspaces()
            .into_iter()
            .find(|w| w.pane_layout().find_pane(pane).is_some())
        {
            self.tutorial
                .observe(crate::adapters::ui::tutorial::PracticeEvent::NewTab {
                    workspace: ws.id,
                    pane,
                    tab,
                });
        }
    }
    pub(crate) fn tutorial_tab_snapshot(&self, engine: &CoreState) -> Option<(u32, u32, u32)> {
        let ws = engine.workspace_at(self.active_workspace_index(engine))?;
        let pane = ws
            .pane_layout()
            .find_pane(self.navigation.pane_id(ws).unwrap_or(0))?;
        Some((
            ws.id,
            pane.id,
            pane.tabs.get(self.navigation.tab_index(pane))?.id,
        ))
    }
    pub(crate) fn observe_tutorial_tab_switch(
        &mut self,
        engine: &CoreState,
        before: Option<(u32, u32, u32)>,
    ) {
        if let (Some((workspace, pane, from)), Some((ws, p, to))) =
            (before, self.tutorial_tab_snapshot(engine))
            && workspace == ws
            && pane == p
        {
            self.tutorial
                .observe(crate::adapters::ui::tutorial::PracticeEvent::SwitchTab {
                    workspace,
                    pane,
                    from,
                    to,
                });
        }
    }
}
