#[cfg(any(feature = "gui", test))]
use crate::core::CoreState;

use super::RequestContext;
#[cfg(any(feature = "gui", test))]
use crate::runtime::engine_read::EngineRead;

/// 닫기 요청 출처. 복원 사본 저장, surface.closed의 reason, 계측 구분값을 정한다.
#[cfg(any(feature = "gui", debug_assertions))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCloseOrigin {
    /// 사용자 단축키·메뉴 또는 사용자 입력을 재현하는 debug IPC 경로.
    #[cfg(any(feature = "gui", debug_assertions))]
    User,
}

#[cfg(any(feature = "gui", debug_assertions))]
impl WorkspaceCloseOrigin {
    /// User가 없는 빌드도 처리하도록 cfg가 붙은 match 분기를 사용한다.
    #[cfg(feature = "gui")]
    fn is_user(self) -> bool {
        match self {
            #[cfg(any(feature = "gui", debug_assertions))]
            Self::User => true,
        }
    }
}

impl RequestContext {
    /// 워크스페이스가 없으면 기본 항목을 생성하고 true를 반환한다. 실패하면 false다.
    /// 원격 연결 해제 등으로 빈 상태가 됐을 때 사용자 창을 유지하기 위한 처리이며
    /// 별도의 host event는 만들지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) fn recreate_workspace_if_empty(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        context: &str,
    ) -> bool {
        if !engine.workspaces().is_empty() {
            return false;
        }
        self.dispatch_intent(
            crate::app::command::DomainIntent::CreateWorkspace {
                cwd: None,
                kind: "terminal".into(),
                surface_params: serde_json::json!({}),
                name: None,
                subtitle: None,
                description: None,
                category: None,
            }
            .from_system(),
        );
        tracing::debug!(context, "queued default workspace for an empty engine");
        true
    }

    /// 0-based 인덱스로 전환한다. 사용자 입력과 debug IPC에서만 호출한다.
    #[cfg(any(feature = "gui", debug_assertions, test))]
    pub fn switch_workspace(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        index: usize,
    ) {
        if index < engine.workspaces().len() {
            #[cfg(feature = "gui")]
            self.attach_pending.deselect();
            self.set_active_workspace_index(engine, index);
            let cat = engine
                .workspace_at(index)
                .expect("workspace index is valid")
                .category;
            self.category_last_active.insert(
                cat,
                engine
                    .workspace_at(index)
                    .expect("workspace index is valid")
                    .id,
            );
        }
    }

    /// 사용자가 고른 surface 가 보이도록 workspace·pane·탭·surface 선택을 차례로 옮긴다.
    /// 사용자 조작(DAG 상세의 세션 열기)에서만 호출한다. surface 가 없으면 아무것도 바꾸지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) fn reveal_surface(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
    ) -> bool {
        let Some((index, pane_id)) = engine.find_workspace_index_for_surface(surface_id) else {
            return false;
        };
        let Some(workspace) = engine.workspace_at(index) else {
            return false;
        };
        let Some(pane) = workspace.pane_layout().find_pane(pane_id) else {
            return false;
        };
        let Some(tab) = pane.tabs.iter().find(|t| t.contains_surface(surface_id)) else {
            return false;
        };
        self.switch_workspace(engine, index);
        self.navigation.select_pane(workspace, pane_id);
        self.navigation.select_tab(pane, tab.id);
        self.navigation.select_surface(tab, surface_id);
        true
    }

    /// 섹션 인덱스(0=normal)로 카테고리를 전환한다. 사용자 키 입력 경로다.
    /// 접힌 카테고리는 펼쳐 저장하고, 마지막으로 본 워크스페이스를 선택한다.
    /// 기록된 대상이 없거나 다른 카테고리로 이동했다면 첫 항목을 선택한다.
    #[cfg(any(feature = "gui", test))]
    pub fn switch_to_category(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        section_idx: usize,
    ) {
        let Some(cat) = engine.categories().get(section_idx).map(|c| c.id) else {
            return;
        };
        #[cfg(feature = "gui")]
        let collapsed = engine
            .categories()
            .get(section_idx)
            .is_some_and(|c| self.navigation.collapsed_categories.contains(&c.id));
        #[cfg(feature = "gui")]
        if collapsed {
            self.dispatch_intent(
                crate::intent::Intent::Ui(crate::intent::UiIntent::SetCategoryCollapsed {
                    id: cat,
                    collapsed: false,
                })
                .from_user_shortcut("switch_to_category"),
            );
        }
        let target = self
            .category_last_active
            .get(&cat)
            .copied()
            .and_then(|ws_id| {
                engine
                    .workspaces()
                    .into_iter()
                    .position(|w| w.id == ws_id && w.category == cat)
            })
            .or_else(|| {
                engine
                    .workspaces_in_category(cat)
                    .first()
                    .map(|(gi, _)| *gi)
            });
        if let Some(global) = target {
            self.switch_workspace(engine, global);
        }
    }

    /// 활성 카테고리의 local_idx를 전역 인덱스로 바꿔 전환한다.
    /// 카테고리 기능이 꺼졌거나 해당 위치가 없으면 처리하지 않는다.
    #[cfg(feature = "gui")]
    pub fn switch_workspace_in_active_category(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        local_idx: usize,
    ) {
        if self.active_workspace_index(engine) >= engine.workspaces().len() {
            return;
        }
        let cat = engine
            .workspace_at(self.active_workspace_index(engine))
            .expect("workspace index is valid")
            .category;
        let global = engine
            .workspaces_in_category(cat)
            .get(local_idx)
            .map(|(gi, _)| *gi);
        if let Some(global) = global {
            self.switch_workspace(engine, global);
        }
    }

    /// 활성 카테고리의 다음 워크스페이스로 이동한다. 사용자 키 입력 경로다.
    /// 마지막 항목에서는 workspace_switch_crosses_category에 따라 같은 카테고리의
    /// 처음으로 돌아가거나 다음 카테고리로 넘어간다.
    #[cfg(any(feature = "gui", test))]
    pub fn next_workspace_in_active_category(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        if let Some(target) = self.relative_workspace_in_active_category(engine, 1) {
            self.switch_workspace(engine, target);
        }
    }

    /// 이전 항목으로 이동한다. 경계 처리는 next_workspace_in_active_category와 반대다.
    #[cfg(any(feature = "gui", test))]
    pub fn prev_workspace_in_active_category(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        if let Some(target) = self.relative_workspace_in_active_category(engine, -1) {
            self.switch_workspace(engine, target);
        }
    }

    /// 활성 카테고리에서 delta(±1)만큼 이동할 대상의 전역 인덱스를 구한다.
    /// 설정에 따라 카테고리 경계를 넘고, 넘을 수 없으면 같은 카테고리에서 순환한다.
    #[cfg(any(feature = "gui", test))]
    fn relative_workspace_in_active_category(
        &self,
        engine: &EngineRead<'_>,
        delta: isize,
    ) -> Option<usize> {
        if self.active_workspace_index(engine) >= engine.workspaces().len() {
            return None;
        }
        let cat = engine
            .workspace_at(self.active_workspace_index(engine))
            .expect("workspace index is valid")
            .category;
        let locals = engine.workspaces_in_category(cat);
        let len = locals.len();
        let pos = locals
            .iter()
            .position(|(gi, _)| *gi == self.active_workspace_index(engine))?;

        if engine.settings.general.workspace_switch_crosses_category {
            let raw = pos as isize + delta;
            if raw < 0 || raw >= len as isize {
                if let Some(target) = self.relative_category_boundary_workspace(engine, delta) {
                    return Some(target);
                }
            } else {
                return Some(locals[raw as usize].0);
            }
        }

        if len <= 1 {
            return None;
        }
        let new_pos = (pos as isize + delta).rem_euclid(len as isize) as usize;
        Some(locals[new_pos].0)
    }

    /// 인접 카테고리의 첫 항목(delta > 0) 또는 마지막 항목을 선택한다.
    /// switch_to_category와 달리 마지막 조회 기록을 사용하지 않는다.
    #[cfg(any(feature = "gui", test))]
    fn relative_category_boundary_workspace(
        &self,
        engine: &CoreState,
        delta: isize,
    ) -> Option<usize> {
        let section_idx = self.relative_category_section(engine, delta)?;
        let target_cat = engine.categories().get(section_idx)?.id;
        let locals = engine.workspaces_in_category(target_cat);
        if delta > 0 {
            locals.first().map(|(gi, _)| *gi)
        } else {
            locals.last().map(|(gi, _)| *gi)
        }
    }

    /// 다음 카테고리로 순환하며, 해당 카테고리에서 마지막으로 본 항목을 선택한다.
    /// 카테고리가 하나뿐이면 처리하지 않는다. 사용자 키 입력 경로다.
    #[cfg(any(feature = "gui", test))]
    pub fn next_category(&mut self, engine: &crate::runtime::engine_read::EngineRead<'_>) {
        if let Some(section_idx) = self.relative_category_section(engine, 1) {
            self.switch_to_category(engine, section_idx);
        }
    }

    #[cfg(any(feature = "gui", test))]
    pub fn prev_category(&mut self, engine: &crate::runtime::engine_read::EngineRead<'_>) {
        if let Some(section_idx) = self.relative_category_section(engine, -1) {
            self.switch_to_category(engine, section_idx);
        }
    }

    /// delta(±1) 방향으로 순환할 카테고리의 섹션 인덱스를 구한다.
    #[cfg(any(feature = "gui", test))]
    fn relative_category_section(&self, engine: &CoreState, delta: isize) -> Option<usize> {
        if self.active_workspace_index(engine) >= engine.workspaces().len() {
            return None;
        }
        let cat = engine
            .workspace_at(self.active_workspace_index(engine))
            .expect("workspace index is valid")
            .category;
        let categories = engine.categories();
        let len = categories.len();
        if len <= 1 {
            return None;
        }
        let pos = categories.iter().position(|c| c.id == cat)?;
        let new_pos = (pos as isize + delta).rem_euclid(len as isize) as usize;
        Some(new_pos)
    }

    /// 순서를 바꾸고 활성 대상을 유지한다. 범위 밖이거나 같은 위치면 false다.
    #[cfg(feature = "gui")]
    pub fn move_workspace(&mut self, engine: &EngineRead<'_>, from: usize, to: usize) -> bool {
        let len = engine.workspaces().len();
        if from == to || from >= len || to >= len {
            return false;
        }
        let Some(workspace_id) = engine.workspace_at(from).map(|workspace| workspace.id) else {
            return false;
        };
        self.dispatch_intent(
            crate::app::command::DomainIntent::MoveWorkspace {
                workspace_id,
                to_index: to,
            }
            .from_user_context_menu(),
        );
        true
    }

    #[cfg(feature = "gui")]
    pub fn close_active_workspace(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) -> bool {
        self.close_workspace_at(
            engine,
            self.active_workspace_index(engine),
            WorkspaceCloseOrigin::User,
        )
    }

    /// 지정 워크스페이스를 닫고 관련 상태를 정리한다.
    /// origin은 복원 사본·surface.closed reason·계측 구분을 정한다.
    /// 제거 후 활성 인덱스를 보정하며 workspace.closed는 after_workspace_removed에서 보낸다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub fn close_workspace_at(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        ws_idx: usize,
        origin: WorkspaceCloseOrigin,
    ) -> bool {
        // gui 가 꺼지면 사용자 조작 경로가 없어 agent 출처로만 보낸다.
        #[cfg(not(feature = "gui"))]
        let _ = origin;
        let Some(workspace) = engine.workspace_at(ws_idx) else {
            return false;
        };
        let workspace_id = workspace.id;
        // 사용자가 저장된 원격 행을 닫으면 이 Tasty에서만 닫는다. 원격 닫기는 행 메뉴의 확인을 거친다.
        #[cfg(feature = "gui")]
        if origin.is_user() && workspace.is_saved_remote_row() {
            self.dispatch_intent(
                crate::intent::Intent::SavedRow {
                    workspace_id,
                    action: crate::intent::SavedRowAction::CloseLocally,
                }
                .from_user_context_menu(),
            );
            return true;
        }
        let targets = workspace.all_surface_ids();
        if self.refuse_if_hard_occupied(engine, targets) {
            return false;
        }
        let intent = crate::app::command::DomainIntent::CloseWorkspace { workspace_id };
        #[cfg(feature = "gui")]
        if origin.is_user() {
            self.dispatch_intent(intent.from_user_context_menu());
            return true;
        }
        self.dispatch_intent(intent.from_agent_ipc());
        true
    }
}
