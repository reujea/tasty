//! 저장된 원격 행을 가진 engine을 찾는다.
//! 세션 연결·재연결 자체는 `connection`에 있다.

use crate::app::App;
use crate::runtime::engine_session::EngineId;

impl App {
    /// 이 workspace를 저장된 원격 행으로 가진 engine.
    pub(crate) fn saved_remote_row_engine(&self, workspace: u32) -> Option<EngineId> {
        self.engines.all_sessions().find_map(|session| {
            let core = &session.core_state;
            core.find_workspace_index_for_id(workspace)
                .and_then(|index| core.workspace_at(index))
                .is_some_and(|ws| ws.is_saved_remote_row())
                .then_some(session.id)
        })
    }
}
