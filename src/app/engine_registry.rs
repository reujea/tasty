//! App이 소유한 모든 engine과 창·parked·임시 관계의 유일한 원본.
//!
//! engine은 [`EngineSession`] 하나로 `sessions`에 머물고, 창을 열거나 닫아도 컬렉션을 옮기지 않는다.
//! 창·parked·임시·retiring은 관계 상태다. retiring owner도 확정 완료까지 같은 표에 남는다.
//! MainView는 engine을 소유하거나 id를 들지 않고 자기 창 ID로 이 registry를 조회한다.
//! 규칙은 [App·Engine·View 상태 소유권](../../docs/dev-guide/app-state-ownership.md)를 따른다.

use std::collections::HashMap;

use winit::window::WindowId;

use crate::runtime::engine_access::{EngineMut, EngineRef};
use crate::runtime::engine_session::{EngineId, EngineSession};
use crate::state::MainViewState;

/// 창이 없어진 engine을 다시 보여 줄 때 쓸 View 복원 자료. engine 원본은 registry에 남는다.
pub(crate) struct ParkedView {
    pub(crate) engine: EngineId,
    pub(crate) state: MainViewState,
}

/// [`EngineRegistry::split_by_id`]의 결과. id별 engine, 창 관계, parked 목록, 임시 engine id 순이다.
pub(crate) type SplitById<'a> = (
    HashMap<EngineId, EngineMut<'a>>,
    &'a HashMap<WindowId, EngineId>,
    &'a mut Vec<ParkedView>,
    Option<EngineId>,
);

pub(crate) enum RetiringView {
    Discard,
    Releasing,
    Preserve {
        navigation: Box<crate::state::navigation::NavigationState>,
        checkpoint: Option<u64>,
    },
}

#[derive(Default)]
pub(crate) struct EngineRegistry {
    sessions: HashMap<EngineId, EngineSession>,
    by_window: HashMap<WindowId, EngineId>,
    /// 보관 순서다. 새 창은 가장 먼저 보관한 항목을 받는다.
    parked: Vec<ParkedView>,
    /// 창에 배정되기 전의 engine.
    pending: Option<EngineId>,
    retiring: HashMap<EngineId, RetiringView>,
    /// 원 standalone 등록으로 돌아가지 못한 채 engine이 해제된 채택 프로세스. 종료하지 않고 App이 끝날 때까지 보관한다.
    unreturned: Vec<crate::runtime::resource_retirement::UnreturnedStandalone>,
}

impl EngineRegistry {
    /// Presentation receives a read capability without borrowing the session mutably.
    pub(crate) fn read(&self, id: EngineId) -> Option<crate::runtime::engine_read::EngineRead<'_>> {
        self.sessions.get(&id).map(EngineSession::read)
    }
    pub(crate) fn window_read(
        &self,
        window: WindowId,
    ) -> Option<crate::runtime::engine_read::EngineRead<'_>> {
        self.read(*self.by_window.get(&window)?)
    }

    /// 새 engine을 임시 관계로 넣는다. 이미 임시 engine이 있으면 넣지 않고 되돌려준다.
    pub(crate) fn insert_pending(
        &mut self,
        session: EngineSession,
    ) -> Result<EngineId, EngineSession> {
        if self.pending.is_some() {
            return Err(session);
        }
        let id = session.id;
        self.sessions.insert(id, session);
        self.pending = Some(id);
        Ok(id)
    }

    /// Failed OS/View assembly releases its pending relation while retaining the execution owner.
    pub(crate) fn begin_retiring_pending(&mut self, id: EngineId) -> bool {
        if self.pending != Some(id) {
            return false;
        }
        self.pending = None;
        self.retiring.insert(id, RetiringView::Discard);
        true
    }

    pub(crate) fn pending_id(&self) -> Option<EngineId> {
        self.pending
    }

    pub(crate) fn pending(&self) -> Option<EngineRef<'_>> {
        self.pending
            .and_then(|id| self.sessions.get(&id))
            .map(|s| s.as_ref())
    }

    pub(crate) fn pending_mut(&mut self) -> Option<EngineMut<'_>> {
        let id = self.pending?;
        self.sessions.get_mut(&id).map(|s| s.borrow_mut())
    }

    /// 임시 engine을 창에 연결한다. 임시 관계에서 빠진다.
    pub(crate) fn attach_window(&mut self, wid: WindowId, id: EngineId) {
        if self.pending == Some(id) {
            self.pending = None;
        }
        self.parked.retain(|p| p.engine != id);
        self.by_window.insert(wid, id);
    }

    /// 창 관계만 끊고 engine은 그대로 둔다. View 복원 자료를 parked 뒤에 붙인다.
    pub(crate) fn park(&mut self, wid: WindowId, mut state: MainViewState) -> Option<EngineId> {
        let id = self.by_window.remove(&wid)?;
        end_view_bound_runs(&mut state);
        self.parked.push(ParkedView { engine: id, state });
        Some(id)
    }

    /// 가장 먼저 보관한 parked engine을 임시 관계로 옮기고 View 복원 자료를 돌려준다.
    /// 이미 임시 engine이 있으면 옮기지 않는다.
    pub(crate) fn unpark_first(&mut self) -> Option<(EngineId, MainViewState)> {
        if self.pending.is_some() || self.parked.is_empty() {
            return None;
        }
        let ParkedView { engine, state } = self.parked.remove(0);
        self.pending = Some(engine);
        Some((engine, state))
    }

    /// Keep the sole resource owner while its slot retirement is awaiting durable publication.
    pub(crate) fn begin_retiring_window(&mut self, wid: WindowId) -> Option<EngineId> {
        let id = self.by_window.remove(&wid)?;
        self.retiring.insert(id, RetiringView::Discard);
        Some(id)
    }

    pub(crate) fn mark_retiring_release(&mut self, id: EngineId) {
        if let Some(relation) = self.retiring.get_mut(&id) {
            *relation = RetiringView::Releasing;
        }
    }
    pub(crate) fn releasing_ids(&self) -> Vec<EngineId> {
        self.retiring
            .iter()
            .filter_map(|(id, relation)| matches!(relation, RetiringView::Releasing).then_some(*id))
            .collect()
    }
    pub(crate) fn has_releasing(&self) -> bool {
        self.retiring
            .values()
            .any(|relation| matches!(relation, RetiringView::Releasing))
    }

    /// The released session is dropped by the caller, so unreturned processes move here first.
    pub(crate) fn finish_retiring(&mut self, id: EngineId) -> Option<EngineSession> {
        let mut session = self
            .retiring
            .remove(&id)
            .and_then(|_| self.sessions.remove(&id))?;
        if let Some(release) = session.engine_release.as_mut() {
            self.unreturned.retain_mut(|kept| !kept.has_exited());
            self.unreturned
                .extend(release.take_unreturned_standalones());
        }
        Some(session)
    }
    #[cfg(all(test, unix))]
    pub(crate) fn unreturned_process_ids(&self) -> Vec<Option<u32>> {
        self.unreturned
            .iter()
            .map(crate::runtime::resource_retirement::UnreturnedStandalone::process_id)
            .collect()
    }

    pub(crate) fn preserve_closed_view(
        &mut self,
        wid: WindowId,
        navigation: crate::state::navigation::NavigationState,
    ) -> Option<EngineId> {
        let id = self.by_window.remove(&wid)?;
        self.retiring.insert(
            id,
            RetiringView::Preserve {
                navigation: Box::new(navigation),
                checkpoint: None,
            },
        );
        Some(id)
    }
    pub(crate) fn preserved_closes(
        &self,
    ) -> Vec<(
        EngineId,
        crate::state::navigation::NavigationState,
        Option<u64>,
    )> {
        self.retiring
            .iter()
            .filter_map(|(id, retiring)| match retiring {
                RetiringView::Preserve {
                    navigation,
                    checkpoint,
                } => Some((*id, navigation.as_ref().clone(), *checkpoint)),
                _ => None,
            })
            .collect()
    }
    pub(crate) fn mark_closed_view_checkpoint(&mut self, id: EngineId, sequence: u64) {
        if let Some(RetiringView::Preserve { checkpoint, .. }) = self.retiring.get_mut(&id) {
            *checkpoint = Some(sequence);
        }
    }

    pub(crate) fn retiring_slots(&self) -> impl Iterator<Item = u32> + '_ {
        self.retiring
            .keys()
            .filter_map(|id| self.sessions.get(id)?.persistence.slot)
    }

    pub(crate) fn of_window(&self, wid: WindowId) -> Option<EngineId> {
        self.by_window.get(&wid).copied()
    }

    /// engine이 연결된 창. parked·임시 engine이면 None이다.
    pub(crate) fn window_of(&self, id: EngineId) -> Option<WindowId> {
        self.by_window
            .iter()
            .find(|(_, e)| **e == id)
            .map(|(wid, _)| *wid)
    }

    pub(crate) fn get(&self, id: EngineId) -> Option<EngineRef<'_>> {
        self.sessions.get(&id).map(|s| s.as_ref())
    }

    pub(crate) fn get_mut(&mut self, id: EngineId) -> Option<EngineMut<'_>> {
        self.sessions.get_mut(&id).map(|s| s.borrow_mut())
    }

    pub(crate) fn all_sessions(&self) -> impl Iterator<Item = &EngineSession> {
        self.sessions.values()
    }

    /// Publication borrows every live, parked and opening owner as one application batch.
    pub(crate) fn all_sessions_mut(&mut self) -> impl Iterator<Item = &mut EngineSession> {
        self.sessions.values_mut()
    }

    pub(crate) fn journal_binding(
        &self,
        id: EngineId,
    ) -> Option<&crate::runtime::journal_product::EngineBinding> {
        self.sessions.get(&id)?.journal_binding.as_ref()
    }

    pub(crate) fn session_mut(&mut self, id: EngineId) -> Option<&mut EngineSession> {
        self.sessions.get_mut(&id)
    }

    pub(crate) fn window_engine(&self, wid: WindowId) -> Option<EngineRef<'_>> {
        self.of_window(wid).and_then(|id| self.get(id))
    }

    pub(crate) fn window_engine_mut(&mut self, wid: WindowId) -> Option<EngineMut<'_>> {
        self.of_window(wid).and_then(|id| self.get_mut(id))
    }

    pub(crate) fn parked_count(&self) -> usize {
        self.parked.len()
    }

    /// parked 항목의 id·View 복원 자료·engine. 보관 순서다.
    pub(crate) fn parked_sessions(
        &self,
    ) -> impl Iterator<Item = (EngineId, &MainViewState, EngineRef<'_>)> {
        self.parked.iter().filter_map(|p| {
            self.sessions
                .get(&p.engine)
                .map(|s| (p.engine, &p.state, s.as_ref()))
        })
    }

    /// parked 항목을 가변으로 순회한다. 한 engine은 한 항목에만 있으므로 각 참조를 한 번만 넘긴다.
    pub(crate) fn parked_sessions_mut(
        &mut self,
    ) -> impl Iterator<Item = (EngineId, &mut MainViewState, EngineMut<'_>)> {
        let Self {
            sessions, parked, ..
        } = self;
        let mut by_id: HashMap<EngineId, &mut EngineSession> =
            sessions.values_mut().map(|s| (s.id, s)).collect();
        parked.iter_mut().filter_map(move |p| {
            by_id
                .remove(&p.engine)
                .map(|s| (p.engine, &mut p.state, s.borrow_mut()))
        })
    }

    /// id로 parked 항목을 찾는다.
    pub(crate) fn parked_session_mut(
        &mut self,
        id: EngineId,
    ) -> Option<(&mut MainViewState, EngineMut<'_>)> {
        let Self {
            sessions, parked, ..
        } = self;
        let p = parked.iter_mut().find(|p| p.engine == id)?;
        let s = sessions.get_mut(&id)?;
        Some((&mut p.state, s.borrow_mut()))
    }

    /// 창 engine과 그 창 ID. `order`는 창 순회 순서다.
    pub(crate) fn windows_in<'a>(
        &'a self,
        order: impl Iterator<Item = WindowId> + 'a,
    ) -> impl Iterator<Item = (WindowId, EngineRef<'a>)> + 'a {
        order.filter_map(move |wid| self.window_engine(wid).map(|e| (wid, e)))
    }

    /// 가변 참조를 id별로 한 번씩 꺼낼 수 있는 표. 창 루프가 창 순서대로 꺼낸다.
    pub(crate) fn split_by_id(&mut self) -> SplitById<'_> {
        let Self {
            sessions,
            by_window,
            parked,
            pending,
            retiring: _,
            unreturned: _,
        } = self;
        let by_id = sessions
            .values_mut()
            .map(|s| (s.id, s.borrow_mut()))
            .collect();
        (by_id, by_window, parked, *pending)
    }

    /// 창·parked·임시 관계에 있는 id 목록.
    #[cfg(test)]
    pub(crate) fn relation_ids(&self) -> RelationIds {
        RelationIds {
            windows: self.by_window.values().copied().collect(),
            parked: self.parked.iter().map(|p| p.engine).collect(),
            pending: self.pending,
        }
    }

    /// 시험용. 창을 거쳐 parked로 옮기는 실제 전이를 그대로 밟는다.
    #[cfg(test)]
    pub(crate) fn park_for_test(
        &mut self,
        state: MainViewState,
        session: EngineSession,
    ) -> EngineId {
        let wid = WindowId::from(u64::MAX - self.sessions.len() as u64);
        let id = self
            .insert_pending(session)
            .unwrap_or_else(|_| panic!("시험 중 임시 engine이 남아 있다"));
        self.attach_window(wid, id);
        self.park(wid, state);
        id
    }

    #[cfg(test)]
    pub(crate) fn session_ids(&self) -> impl Iterator<Item = EngineId> + '_ {
        self.sessions.keys().copied()
    }
}

/// 관계별 engine id. 한 id는 셋 중 정확히 한 곳에 나온다.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct RelationIds {
    pub(crate) windows: Vec<EngineId>,
    pub(crate) parked: Vec<EngineId>,
    pub(crate) pending: Option<EngineId>,
}

/// parked 자료가 남아 있어도 그것을 보여 주던 View는 사라졌으므로, 그 View의 선택을 기다리던
/// 요청은 지금 끝낸다. 복원한 창의 picker는 그대로 쓸 수 있고 그 선택은 결과를 남기지 않는다.
fn end_view_bound_runs(state: &mut MainViewState) {
    if let Some(guard) = state
        .dialogs
        .file_handler_picker
        .as_mut()
        .and_then(|picker| picker.run.take())
    {
        crate::request_runs::finish(
            guard.take(),
            crate::request_runs::RunState::Cancelled,
            PICKER_VIEW_GONE,
            serde_json::Value::Null,
        );
    }
}

const PICKER_VIEW_GONE: &str = "the window showing the file handler picker was closed";

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn test_engine(
        slot: Option<u32>,
    ) -> (MainViewState, crate::runtime::engine_session::EngineSession) {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        engine.persistence.slot = slot;
        (state, engine_session)
    }

    fn wid(n: u64) -> WindowId {
        WindowId::from(n)
    }

    /// 한 id는 창·parked·임시 중 정확히 한 곳에 있고, 관계의 합집합이 sessions 키와 같다.
    fn assert_relations_partition_sessions(reg: &EngineRegistry) {
        let r = reg.relation_ids();
        let mut seen = HashSet::new();
        for id in r
            .windows
            .iter()
            .chain(r.parked.iter())
            .chain(r.pending.iter())
        {
            assert!(seen.insert(*id), "{id:?}가 두 관계에 동시에 있다: {r:?}");
        }
        let keys: HashSet<EngineId> = reg.session_ids().collect();
        assert_eq!(seen, keys, "관계의 합집합과 sessions 키가 다르다: {r:?}");
    }

    fn slots(reg: &EngineRegistry) -> Vec<u32> {
        let mut v: Vec<u32> = reg
            .session_ids()
            .filter_map(|id| reg.get(id).and_then(|e| e.persistence.slot))
            .collect();
        v.sort_unstable();
        v
    }

    /// 창 하나를 여는 경로와 같다. parked가 있으면 그것을, 없으면 새 engine을 임시로 둔 뒤 창에 붙인다.
    fn open_window(reg: &mut EngineRegistry, w: WindowId, fresh: EngineSession) -> EngineId {
        let id = match reg.unpark_first() {
            Some((id, _)) => id,
            None => reg
                .insert_pending(fresh)
                .unwrap_or_else(|_| panic!("임시 engine이 남아 있다")),
        };
        reg.attach_window(w, id);
        id
    }

    #[test]
    fn parking_a_window_ends_the_run_its_open_picker_was_holding() {
        let mut reg = EngineRegistry::default();
        let (mut state, engine) = test_engine(Some(1));
        open_window(&mut reg, wid(1), engine);
        let run =
            crate::request_runs::begin("file_handler.dispatch", None, serde_json::Value::Null);
        state.dialogs.file_handler_picker = Some(crate::state::FileHandlerPickerData {
            origin_surface_id: None,
            dispatch_origin: crate::file::dispatch::FileDispatchOrigin::Agent,
            target: crate::file::format::FileTarget::new("/tmp/park-picker.txt").into(),
            target_display: "/tmp/park-picker.txt".to_string(),
            detector: None,
            candidates: vec![],
            candidates_are_fallback: false,
            recent: vec![],
            default_handler: None,
            selected: None,
            result: None,
            ignore_size_limit: false,
            run: Some(crate::request_runs::RunGuard::new(run, "picker dropped")),
            remote_host: None,
            opened_in: None,
            row_link: None,
        });
        assert!(reg.park(wid(1), state).is_some());
        let ended = crate::request_runs::lookup(run, None).unwrap();
        assert_eq!(ended["state"], "cancelled");
        assert_eq!(ended["result"], PICKER_VIEW_GONE);
        // 복원한 창에서 picker를 계속 쓸 수 있다.
        let (_, restored) = reg.unpark_first().unwrap();
        let picker = restored.dialogs.file_handler_picker.as_ref().unwrap();
        assert!(picker.run.is_none());
    }

    #[test]
    fn transitions_keep_every_engine_in_exactly_one_relation() {
        let mut reg = EngineRegistry::default();
        assert_relations_partition_sessions(&reg);

        let a = open_window(&mut reg, wid(1), test_engine(Some(1)).1);
        let b = open_window(&mut reg, wid(2), test_engine(Some(2)).1);
        assert_relations_partition_sessions(&reg);
        assert_eq!(slots(&reg), [1, 2]);

        // 최소화(macOS)는 모든 창을 park한다. engine은 registry에 남아 슬롯을 유지한다.
        assert_eq!(reg.park(wid(1), test_engine(None).0), Some(a));
        assert_eq!(reg.park(wid(2), test_engine(None).0), Some(b));
        assert_relations_partition_sessions(&reg);
        assert_eq!(reg.parked_count(), 2);
        assert_eq!(slots(&reg), [1, 2], "park는 슬롯 점유를 바꾸지 않는다");

        // 복원은 가장 먼저 보관한 engine을 받는다.
        assert_eq!(open_window(&mut reg, wid(3), test_engine(Some(9)).1), a);
        assert_relations_partition_sessions(&reg);
        assert_eq!(slots(&reg), [1, 2], "unpark는 새 engine을 만들지 않는다");

        // 임시 engine이 남아 있으면 unpark하지 않는다.
        let pending = reg
            .insert_pending(test_engine(Some(5)).1)
            .unwrap_or_else(|_| panic!("임시 engine 없음"));
        assert!(reg.unpark_first().is_none());
        assert!(reg.insert_pending(test_engine(None).1).is_err());
        assert_relations_partition_sessions(&reg);
        reg.attach_window(wid(4), pending);

        // Retirement first removes the View relation; the process owner remains until release.
        assert_eq!(reg.begin_retiring_window(wid(3)), Some(a));
        assert!(reg.session_mut(a).is_some());
        reg.mark_retiring_release(a);
        let retired = reg.finish_retiring(a).expect("released owner");
        assert_eq!(retired.id, a);
        assert!(reg.begin_retiring_window(wid(3)).is_none());
        assert_relations_partition_sessions(&reg);
        assert_eq!(slots(&reg), [2, 5]);
        assert_eq!(reg.window_of(pending), Some(wid(4)));
        assert_eq!(reg.window_of(b), None, "parked engine은 창이 없다");
    }

    /// 종료 저장은 창 engine과 parked engine을 한 번씩 본다. 두 집합은 겹치지 않고 임시 engine은 없다.
    #[test]
    fn quit_flush_targets_are_windows_and_parked_without_duplicates() {
        let mut reg = EngineRegistry::default();
        let a = open_window(&mut reg, wid(1), test_engine(Some(1)).1);
        let b = open_window(&mut reg, wid(2), test_engine(Some(2)).1);
        let c = open_window(&mut reg, wid(3), test_engine(Some(3)).1);
        reg.park(wid(2), test_engine(None).0);
        let r = reg.relation_ids();
        let targets: Vec<EngineId> = r.windows.iter().chain(r.parked.iter()).copied().collect();
        let unique: HashSet<EngineId> = targets.iter().copied().collect();
        assert_eq!(targets.len(), unique.len(), "같은 engine을 두 번 저장한다");
        assert_eq!(unique, HashSet::from([a, b, c]));
        assert!(r.pending.is_none());
    }
}
