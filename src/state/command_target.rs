//! 명령을 입력한 시점의 대상. 입력 경계(키·팔레트 Enter·WebView 키)에서 만들고, 실행기는 이 값의 id 로
//! 대상을 찾는다. 실행기는 navigation 의 포커스를 다시 읽지 않으며, 대상이 사라졌으면 실행하지 않는다.

use super::RequestContext;
use super::attach_pending::deferred::CommandSurfaceTarget;
use super::attach_pending::{Creation, PendingId};
use crate::core::CoreState;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CommandTarget {
    /// 대상 workspace id. workspace 가 하나도 없으면 없다. 초안이면 초안의 workspace 다.
    pub workspace: Option<u32>,
    /// 대상 pane id. 초안이면 초안이 기댄(anchor) 기존 pane 이다. 새 pane 을 만드는 초안이어도 이 값은 기존
    /// pane 이고, 만들 pane 은 `draft_creation` 으로 구분한다.
    pub pane: Option<u32>,
    /// 대상 pane 에서 선택된 탭 id. 초안이 선택돼 있으면 초안 뒤의 기존 탭이다. 탭 자체를 대상으로 하는
    /// 명령은 `draft` 를 먼저 본다.
    pub tab: Option<u32>,
    /// 대상 surface. 생성 대기 초안이면 그 초안이다.
    pub surface: Option<CommandSurfaceTarget>,
    /// 입력 때 선택돼 있던 생성 대기 초안.
    pub draft: Option<PendingId>,
    /// 초안이 만들 것. `Creation::Pane` 이면 `pane` 옆에 새 pane 을 만든다.
    pub draft_creation: Option<Creation>,
    /// 초안이 대상일 때 초안 뒤 탭에서 입력 때 골라져 있던 surface. surface 이동의 기준이다.
    pub draft_base_surface: Option<u32>,
}

impl CommandTarget {
    /// 대상 surface id. 초안이면 아직 id 가 없어 없다.
    pub(crate) fn surface_id(&self) -> Option<u32> {
        match self.surface {
            Some(CommandSurfaceTarget::Surface(sid)) => Some(sid),
            _ => None,
        }
    }

    pub(crate) fn workspace_index(&self, engine: &CoreState) -> Option<usize> {
        let id = self.workspace?;
        engine.workspaces().into_iter().position(|ws| ws.id == id)
    }

    pub(crate) fn find_workspace<'a>(
        &self,
        engine: &'a CoreState,
    ) -> Option<&'a crate::model::Workspace> {
        engine.workspace_at(self.workspace_index(engine)?)
    }

    /// 대상 workspace 안의 대상 pane.
    pub(crate) fn find_pane<'a>(&self, engine: &'a CoreState) -> Option<&'a crate::model::Pane> {
        self.find_workspace(engine)?
            .pane_layout()
            .find_pane(self.pane?)
    }

    /// 대상 pane 안의 대상 탭과 그 위치.
    pub(crate) fn find_tab<'a>(
        &self,
        engine: &'a CoreState,
    ) -> Option<(&'a crate::model::Pane, usize, &'a crate::model::Tab)> {
        let pane = self.find_pane(engine)?;
        let id = self.tab?;
        let (index, tab) = pane.tabs.iter().enumerate().find(|(_, t)| t.id == id)?;
        Some((pane, index, tab))
    }
}

impl RequestContext {
    /// 지금 포커스로 만든 명령 대상. 키 입력과 팔레트 Enter 가 입력 시점에 부른다.
    pub(crate) fn command_target(&self, engine: &CoreState) -> CommandTarget {
        if engine.workspaces().is_empty() {
            return CommandTarget::default();
        }
        // 초안은 다른 pane 의 초안 탭을 눌러 고를 수 있다. 그때 모델 포커스 pane 은 초안 자리와 다르므로
        // 상위 대상은 초안 자신에서 만든다.
        if let Some(entry) = self.attach_pending.selected(engine, &self.navigation) {
            let base = engine
                .find_pane_by_id(entry.pane)
                .and_then(|pane| pane.tabs.iter().find(|tab| tab.id == entry.tab))
                .and_then(|tab| self.navigation.surface_id(tab));
            return CommandTarget {
                workspace: Some(entry.workspace),
                pane: Some(entry.pane),
                tab: Some(entry.tab),
                surface: self.command_surface_target(engine),
                draft: Some(entry.id),
                draft_creation: entry.creation,
                draft_base_surface: base,
            };
        }
        let workspace = self.active_workspace(engine).id;
        let pane = self.focused_pane(engine);
        CommandTarget {
            workspace: Some(workspace),
            pane: pane.map(|p| p.id),
            tab: pane
                .and_then(|p| p.tabs.get(self.navigation.tab_index(p)))
                .map(|t| t.id),
            surface: self.command_surface_target(engine),
            draft: None,
            draft_creation: None,
            draft_base_surface: None,
        }
    }

    /// `sid` 를 담은 workspace·pane·탭으로 만든 명령 대상. native WebView 가 받은 키처럼 모델 포커스와
    /// 다른 surface 가 입력을 받았을 때 쓴다. surface 가 없으면 없다.
    pub(crate) fn command_target_for_surface(
        &self,
        engine: &CoreState,
        sid: u32,
    ) -> Option<CommandTarget> {
        let (index, pane) = engine.find_workspace_index_for_surface(sid)?;
        Some(CommandTarget {
            workspace: Some(engine.workspace_at(index)?.id),
            pane: Some(pane),
            tab: Some(engine.find_tab_for_surface(sid)?),
            surface: Some(CommandSurfaceTarget::Surface(sid)),
            draft: None,
            draft_creation: None,
            draft_base_surface: None,
        })
    }
}
