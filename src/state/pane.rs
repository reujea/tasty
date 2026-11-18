#[cfg(feature = "gui")]
use crate::core::CoreState;
#[cfg(any(feature = "gui", test))]
use crate::runtime::engine_read::EngineRead;

use super::RequestContext;

impl RequestContext {
    /// 닫을 대상 중 hard 점유된 surface가 있으면 true를 반환해 요청을 거절한다.
    /// 종료된 PTY의 사후 정리에는 적용하지 않는다. 그 경로까지 막으면 surface가 남는다.
    /// 로컬 사용자는 점유 해제 버튼으로 먼저 연결을 끊을 수 있다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn refuse_if_hard_occupied(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        targets: impl IntoIterator<Item = u32>,
    ) -> bool {
        let Some(_occupied) = targets
            .into_iter()
            .find(|sid| engine.live.occupancy.is_hard_occupied(*sid))
        else {
            return false;
        };
        #[cfg(feature = "gui")]
        self.toasts.push(
            crate::i18n::t("attach.toast.close_blocked_hard_occupied"),
            crate::model::toast_kind::ToastKind::Warning,
            crate::model::toast_kind::ToastScope::Window,
        );
        true
    }

    /// mirror의 구조 변경을 원격 큐에 넣고 true를 반환해 로컬 실행을 멈춘다.
    /// op를 만들 수 없으면 전달하지 않고 차단 토스트를 표시한다.
    /// mirror 워크스페이스 자체를 닫는 동작은 이 함수를 거치지 않는다.
    ///
    /// GUI 사용자 요청으로 표시하며, 닫기 요청은 변경 전 트리에서 구한 포커스 후보를 받는다.
    /// 새 탭·분할·이동처럼 닫기가 아닌 요청에는 빈 후보 목록을 넘긴다.
    #[cfg(feature = "gui")]
    pub(crate) fn forward_mirror_structural(
        &mut self,
        engine: &EngineRead<'_>,
        op: Option<crate::ipc::stream::StructuralOp>,
        close_focus_candidates: Vec<u32>,
    ) -> bool {
        let mirror = self.active_workspace(engine).mirror;
        self.forward_mirror_structural_in(mirror, op, close_focus_candidates)
    }

    /// `forward_mirror_structural` 과 같되, 활성 workspace 가 아니라 명령 대상 workspace 의 mirror 여부로 판정한다.
    #[cfg(feature = "gui")]
    pub(crate) fn forward_mirror_structural_in(
        &mut self,
        mirror: bool,
        op: Option<crate::ipc::stream::StructuralOp>,
        close_focus_candidates: Vec<u32>,
    ) -> bool {
        if !mirror {
            return false;
        }
        match op {
            Some(op) => {
                self.dispatch_intent(
                    crate::intent::Intent::ForwardMirror {
                        op,
                        close_focus_candidates,
                    }
                    .from_user_menu("mirror.structural"),
                );
            }
            None => {
                #[cfg(feature = "gui")]
                self.toasts.push(
                    crate::i18n::t("attach.toast.mirror_structural_blocked"),
                    crate::model::toast_kind::ToastKind::Warning,
                    crate::model::toast_kind::ToastScope::Window,
                );
            }
        }
        true
    }

    /// 닫을 탭의 다음 탭, 마지막이면 이전 탭을 우선하는 로컬 포커스 후보다.
    /// 나머지 탭도 순서대로 포함하며 하나뿐인 탭에는 후보가 없다.
    #[cfg(feature = "gui")]
    pub(crate) fn pane_sibling_tab_focus_candidates(
        &self,
        pane: &crate::model::Pane,
        closing_tab_index: usize,
    ) -> Vec<u32> {
        let n = pane.tabs.len();
        if n <= 1 {
            return Vec::new();
        }
        let primary = if closing_tab_index + 1 < n {
            closing_tab_index + 1
        } else {
            closing_tab_index.wrapping_sub(1)
        };
        let mut out = Vec::new();
        if let Some(sid) = pane
            .tabs
            .get(primary)
            .and_then(|t| self.navigation.surface_id(t))
        {
            out.push(sid);
        }
        for (idx, tab) in pane.tabs.iter().enumerate() {
            if idx == closing_tab_index || idx == primary {
                continue;
            }
            if let Some(sid) = self.navigation.surface_id(tab) {
                out.push(sid);
            }
        }
        out
    }

    /// 같은 분할 탭의 다른 surface를 순서대로 후보에 넣는다.
    /// 단일 surface 탭이면 pane_sibling_tab_focus_candidates에 위임한다.
    #[cfg(feature = "gui")]
    fn surface_close_focus_candidates(
        &self,
        pane: &crate::model::Pane,
        tab_index: usize,
        surface_id: u32,
    ) -> Vec<u32> {
        let Some(tab) = pane.tabs.get(tab_index) else {
            return Vec::new();
        };
        if tab.is_split() {
            tab.layout_opt
                .as_ref()
                .map(|l| l.all_surface_ids())
                .unwrap_or_default()
                .into_iter()
                .filter(|&sid| sid != surface_id)
                .collect()
        } else {
            self.pane_sibling_tab_focus_candidates(pane, tab_index)
        }
    }

    /// 포커스된 pane 닫기를 처리한다. mirror 요청을 전달한 경우에도 true다.
    #[cfg(feature = "gui")]
    pub fn close_active_pane(&mut self, engine: &EngineRead<'_>) -> bool {
        if self
            .attach_pending
            .selected(engine, &self.navigation)
            .is_some()
        {
            return false;
        }
        let target = self.command_target(engine);
        self.close_pane_in(engine, &target)
    }

    /// 명령 대상의 pane 을 닫는다. mirror 요청을 전달한 경우에도 true다. 대상 workspace 가 없으면 false 다.
    #[cfg(feature = "gui")]
    pub(crate) fn close_pane_in(
        &mut self,
        engine: &EngineRead<'_>,
        target: &super::command_target::CommandTarget,
    ) -> bool {
        let Some(ws) = target.find_workspace(engine) else {
            return false;
        };
        let Some(target_id) = target.pane else {
            return false;
        };
        let mirror_op =
            target
                .surface_id()
                .map(|sid| crate::ipc::stream::StructuralOp::ClosePane {
                    anchor_surface_id: sid,
                });
        // pane 닫기는 같은 pane 안의 인접 포커스 후보를 사용하지 않는다.
        if self.forward_mirror_structural_in(ws.mirror, mirror_op, Vec::new()) {
            return true;
        }
        let in_pane: Vec<u32> = ws
            .pane_layout()
            .find_pane(target_id)
            .map(|pane| {
                pane.tabs
                    .iter()
                    .flat_map(|tab| tab.all_surface_ids())
                    .collect()
            })
            .unwrap_or_default();
        if self.refuse_if_hard_occupied(engine, in_pane) {
            return false;
        }

        self.dispatch_intent(
            crate::app::command::DomainIntent::ClosePane { pane_id: target_id }
                .from_user_context_menu(),
        );
        true
    }

    /// 포커스된 surface를 닫고 필요하면 빈 탭·pane·워크스페이스도 정리한다.
    #[cfg(feature = "gui")]
    pub fn close_active_surface(&mut self, engine: &EngineRead<'_>) -> bool {
        let target = self.command_target(engine);
        self.close_surface_in(engine, &target)
    }

    /// 명령 대상의 surface 를 닫는다. 대상이 초안이라 surface id 가 없으면 mirror 에서는 차단을 알리고
    /// true, 아니면 false 다.
    #[cfg(feature = "gui")]
    pub(crate) fn close_surface_in(
        &mut self,
        engine: &EngineRead<'_>,
        target: &super::command_target::CommandTarget,
    ) -> bool {
        let Some(ws) = target.find_workspace(engine) else {
            return false;
        };
        let surface = target.surface_id();
        let mirror_op =
            surface.map(|sid| crate::ipc::stream::StructuralOp::CloseSurface { surface_id: sid });
        let candidates = match (surface, target.find_tab(engine)) {
            (Some(sid), Some((pane, index, _))) => {
                self.surface_close_focus_candidates(pane, index, sid)
            }
            _ => Vec::new(),
        };
        if self.forward_mirror_structural_in(ws.mirror, mirror_op, candidates) {
            return true;
        }
        if self.refuse_if_hard_occupied(engine, surface) {
            return false;
        }
        let Some(surface_id) = surface else {
            return false;
        };
        self.close_surface_by_id(engine, surface_id, true)
    }

    /// ID로 surface를 닫으며 복원 사본을 저장한다.
    /// 사본이 필요 없는 경로는 close_surface_by_id_no_snapshot을 사용한다.
    #[cfg(any(feature = "gui", test))]
    pub fn close_surface_by_id(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        is_user_close: bool,
    ) -> bool {
        self.queue_surface_close(engine, surface_id, true, is_user_close)
    }

    /// 복원 사본 없이 닫는다. 워크스페이스가 모두 사라지면 다음 화면 처리에 필요한 기본 항목을 만든다.
    #[cfg(test)]
    pub fn close_surface_by_id_no_snapshot(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        is_user_close: bool,
    ) -> bool {
        self.queue_surface_close(engine, surface_id, false, is_user_close)
    }

    /// Queue fixed IDs and an explicit user snapshot request; execution and repair are App-owned.
    #[cfg(any(feature = "gui", test))]
    fn queue_surface_close(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        capture: bool,
        is_user: bool,
    ) -> bool {
        // gui 가 꺼지면 사용자 조작 경로가 없어 agent 출처로만 보낸다.
        #[cfg(not(feature = "gui"))]
        let _ = is_user;
        if !engine.core.has_surface(surface_id) {
            return false;
        }
        let intent = crate::app::command::DomainIntent::CloseSurface {
            surface_id,
            presentation: capture.then(|| {
                Box::new(crate::model::StructurePresentationSnapshot::capture(
                    engine.workspaces(),
                    engine.categories(),
                    &self.navigation,
                ))
            }),
        };
        #[cfg(feature = "gui")]
        if is_user {
            self.dispatch_intent(intent.from_user_context_menu());
            return true;
        }
        self.dispatch_intent(intent.from_agent_ipc());
        true
    }
}

#[cfg(feature = "gui")]
impl RequestContext {
    pub(crate) fn observe_tutorial_surface_split(
        &mut self,
        engine: &CoreState,
        workspace_index: usize,
        pane_id: u32,
        new_surface_id: u32,
    ) {
        if let Some(ws) = engine.workspace_at(workspace_index)
            && let Some(pane) = ws.pane_layout().find_pane(pane_id)
            && let Some(tab) = pane
                .tabs
                .iter()
                .find(|tab| tab.contains_surface(new_surface_id))
        {
            self.tutorial
                .observe(crate::adapters::ui::tutorial::PracticeEvent::SplitSurface {
                    workspace: ws.id,
                    pane: pane_id,
                    tab: tab.id,
                });
        }
    }

    pub(crate) fn observe_tutorial_pane_split(
        &mut self,
        workspace: u32,
        original: u32,
        new_pane: u32,
    ) {
        self.tutorial
            .observe(crate::adapters::ui::tutorial::PracticeEvent::SplitPane {
                workspace,
                original,
                new_pane,
            });
    }
}

#[cfg(test)]
mod close_origin_tests;
