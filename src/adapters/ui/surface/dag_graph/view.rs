//! surface별 DAG 조회 상태와 공통 그래프 뷰 상태.
//! 보이는 surface만 다음 조회 시각을 내보내고 호스트가 나머지 타이머를 취소한다.
//! 이동·배율·선택·레이아웃 캐시는 공통 [`GraphView`]가 가진다.

use crate::runtime::engine_read::EngineRead;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tasty_dag_layout::{LayoutConfig, Orientation};
use tasty_model::{DagDirection, DagGraphSurface, SurfaceId};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::GraphView;

use super::super::graph_source::GraphSource;
use super::model::{DagData, DagListEntry, DagStatus, RunnerBadgeData, build_graph};

/// DAG 데이터 조회 주기.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// surface와 팝업이 공유하는 관찰 대상. 워크스페이스는 데이터 조회 시 별도로 받는다.
pub struct DagTarget<'a> {
    /// 보고 있는 DAG. 헤더 드롭다운 선택이 여기 반영된다.
    pub dag_id: &'a mut Option<String>,
    /// 레이어 진행 방향. 방향 토글이 여기 반영된다.
    pub direction: &'a mut DagDirection,
}

/// 이번 프레임에 어떤 DAG surface 가 무엇을 보고 있는지 — 렌더 루프 진입 **전에**
/// 수집해 폴링에 넘긴다(루프 안에서는 `engine` 을 재차입할 수 없다).
#[derive(Debug, Clone)]
pub struct DagPollRequest {
    pub surface_id: SurfaceId,
    /// 관찰 대상 workspace — 모델이 명시하지 않으면 소속 workspace.
    pub workspace_id: u32,
    pub dag_id: Option<String>,
}

impl DagPollRequest {
    /// 렌더 중인 surface 로부터. `containing_workspace` 는 이 surface 가 놓인
    /// workspace 다(활성 workspace 가 아니라 **소속** — 렌더 대상이 활성 workspace
    /// 뿐이라 값이 같을 뿐이다).
    pub fn from_surface(panel: &DagGraphSurface, containing_workspace: u32) -> Self {
        Self {
            surface_id: panel.id,
            workspace_id: panel.workspace_id.unwrap_or(containing_workspace),
            dag_id: panel.dag_id.clone(),
        }
    }
}

/// DAG 데이터 소스 — 조회 주기·취소·마지막 결과. 화면 상태는 모른다.
#[derive(Default)]
pub struct DagGraphSource {
    /// 마지막 폴링 결과. `None` 이면 아직 한 번도 읽지 않았다.
    pub data: Option<DagData>,
    /// 마지막 폴링 시각 — [`POLL_INTERVAL`] 게이트.
    last_poll: Option<Instant>,
    query: Option<crate::runtime::dag_query::DagQuery>,
    target: Option<(u32, Option<String>)>,
    surface_activation: Option<u64>,
    /// 폴링이 실패했을 때의 사유(스토어 에러). 화면 하단에 조용히 표시한다.
    pub error: Option<String>,
}

/// DAG 조회 대상. 워크스페이스와 (지정했다면) DAG id.
#[derive(Clone, Copy)]
pub struct DagSourceTarget<'a> {
    pub workspace_id: u32,
    pub dag_id: Option<&'a str>,
}

impl DagGraphSource {
    /// 이 프레임에 새로 읽어야 하는지.
    fn is_stale(&self, now: Instant) -> bool {
        self.last_poll
            .is_none_or(|t| now.duration_since(t) >= POLL_INTERVAL)
    }
}

impl GraphSource for DagGraphSource {
    type Target<'a> = DagSourceTarget<'a>;

    /// 다음 폴링 시각. 아직 한 번도 안 읽었으면 `None`(= 즉시 읽어야 함).
    /// 호스트가 이 값을 `Tick::DagGraph` 데드라인으로 쓴다.
    fn next_poll_at(&self) -> Option<Instant> {
        if self.query.as_ref().is_some_and(|query| query.is_pending()) {
            Some(Instant::now() + POLL_INTERVAL)
        } else {
            self.last_poll.map(|t| t + POLL_INTERVAL)
        }
    }

    /// surface와 팝업이 공유하는 주기별 데이터 조회.
    fn poll_if_stale(&mut self, engine: &EngineRead<'_>, target: DagSourceTarget<'_>) {
        let DagSourceTarget {
            workspace_id,
            dag_id,
        } = target;
        let now = Instant::now();
        let source = engine.dag_source();
        let target = (workspace_id, dag_id.map(str::to_owned));
        if self.target.as_ref() != Some(&target)
            || self
                .query
                .as_ref()
                .is_some_and(|query| !source.matches(query))
        {
            self.invalidate_poll();
            self.data = None;
            self.error = None;
            self.target = Some(target);
        }
        if let Some(result) = self.query.as_mut().and_then(|query| query.poll(source)) {
            self.last_poll = Some(now);
            match result.and_then(|snapshots| {
                snapshots
                    .into_iter()
                    .find(|snapshot| snapshot.id == workspace_id)
                    .ok_or_else(|| "DAG workspace missing from result".into())
            }) {
                Ok(snapshot) => {
                    self.error = None;
                    self.data = Some(present(snapshot, dag_id));
                }
                Err(error) => {
                    tracing::warn!(target: "tasty::dag", "dag poll failed: {error}");
                    if matches!(error, crate::runtime::dag_query::ReadError::Retired) {
                        self.data = None;
                    }
                    self.error = Some(error.to_string());
                }
            }
        }
        if self.query.as_ref().is_none_or(|query| !query.is_pending()) && self.is_stale(now) {
            self.query = Some(source.request(vec![workspace_id]));
            self.last_poll = Some(now);
        }
    }

    /// 보고 있던 DAG 가 바뀌었으니 다음 프레임에 곧바로 다시 읽는다.
    /// (popup 의 목록→디테일 진입처럼 500ms 를 기다릴 이유가 없는 전환용.)
    fn invalidate_poll(&mut self) {
        self.query = None;
        self.last_poll = None;
    }
}

/// DAG 그래프 하나 — 데이터 소스와 공통 그래프 뷰 상태의 쌍. surface와 팝업이 같이 쓴다.
#[derive(Default)]
pub struct DagGraphView {
    pub source: DagGraphSource,
    pub canvas: GraphView,
}

impl DagGraphView {
    /// surface 세대가 바뀌면(닫혔다 다시 열림·교체) 소스와 화면 상태를 함께 버린다.
    fn bind_surface_activation(&mut self, activation: Option<u64>) {
        if self.source.surface_activation != activation {
            *self = Self::default();
            self.source.surface_activation = activation;
        }
    }
}

/// surface id 로 keying 하는 뷰 스토어. `MainViewState` 가 보유한다.
#[derive(Default)]
pub struct DagGraphViewStore {
    views: HashMap<SurfaceId, DagGraphView>,
    /// 직전 [`Self::poll`] 이 받은 = **이번 프레임에 실제로 보인** surface 들.
    /// 폴링 타이머는 이 집합에만 걸린다(보이지 않으면 예약 없음).
    visible: Vec<SurfaceId>,
}

impl DagGraphViewStore {
    pub(crate) fn cached_surfaces(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.views.keys().copied()
    }

    pub fn get_or_init(&mut self, surface_id: SurfaceId) -> &mut DagGraphView {
        self.views.entry(surface_id).or_default()
    }

    /// 닫힌 surface의 상태와 조회 예약 대상을 함께 지운다.
    pub fn drop_view(&mut self, surface_id: SurfaceId) {
        self.views.remove(&surface_id);
        self.visible.retain(|s| *s != surface_id);
    }

    /// 보이는 뷰의 다음 조회 시각. 호스트는 여기에 없는 타이머를 취소한다.
    pub fn pending_poll_deadlines(&self, now: Instant) -> Vec<(SurfaceId, Instant)> {
        self.visible
            .iter()
            .filter_map(|sid| {
                let v = self.views.get(sid)?;
                Some((*sid, v.source.next_poll_at().unwrap_or(now)))
            })
            .collect()
    }

    /// 렌더링의 engine 대여가 시작되기 전에 보이는 DAG 데이터를 읽는다.
    pub fn poll(&mut self, engine: &EngineRead<'_>, requests: &[DagPollRequest]) {
        self.note_visible(requests);
        for req in requests {
            let view = self.views.entry(req.surface_id).or_default();
            view.bind_surface_activation(
                engine
                    .core
                    .find_surface_by_id(req.surface_id)
                    .and_then(|surface| surface.activation_generation),
            );
            view.source.poll_if_stale(
                engine,
                DagSourceTarget {
                    workspace_id: req.workspace_id,
                    dag_id: req.dag_id.as_deref(),
                },
            );
        }
    }

    /// 보이는 대상 목록을 교체한다. 모두 숨겨진 프레임도 빈 requests로 호출해야
    /// 이전 대상의 타이머가 다시 등록되지 않는다.
    fn note_visible(&mut self, requests: &[DagPollRequest]) {
        for (id, view) in &mut self.views {
            if !requests.iter().any(|request| request.surface_id == *id) {
                view.source.query = None;
            }
        }
        self.visible.clear();
        self.visible.extend(requests.iter().map(|r| r.surface_id));
    }
}

/// Build presentation from a completed App read; no storage access occurs here.
fn present(
    snapshot: crate::runtime::dag_query::WorkspaceSnapshot,
    dag_id: Option<&str>,
) -> DagData {
    use tasty_agent::TaskGraph;
    let crate::runtime::dag_query::WorkspaceSnapshot {
        id: workspace_id,
        tasks,
        dags: summaries,
        runner: (running, crashed),
    } = snapshot;

    let dags: Vec<DagListEntry> = summaries
        .iter()
        .map(|s| DagListEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            rollup: DagStatus::from_name(s.rollup_state),
            task_count: s.task_count,
        })
        .collect();

    let chosen = pick_target(&summaries, dag_id);
    let target_missing = dag_id.is_some() && chosen.is_none();

    let current = chosen.map(|summary| {
        let subset: Vec<tasty_agent::Task> = tasks
            .iter()
            .filter(|t| summary.task_ids.contains(&t.id))
            .cloned()
            .collect();
        let mut graph = build_graph(summary, &subset);
        // 순환 관계가 있어도 그래프는 그린다. 배너에 표시할 경로만 추가로 찾으며
        // 그룹 밖 의존성 오류는 순환 경고로 처리하지 않는다.
        graph.cycle = if summary.has_cycle {
            match TaskGraph::build(&subset).detect_cycles() {
                Err(tasty_agent::AgentError::DependencyCycle(msg)) => Some(msg),
                _ => None,
            }
        } else {
            None
        };
        graph
    });

    let runner = current
        .as_ref()
        .map(|g| RunnerBadgeData {
            running,
            crashed,
            ready: g
                .nodes
                .iter()
                .filter(|n| n.status == DagStatus::Ready)
                .count(),
            running_count: g
                .nodes
                .iter()
                .filter(|n| n.status == DagStatus::Running)
                .count(),
        })
        .unwrap_or(RunnerBadgeData {
            running,
            crashed,
            ..RunnerBadgeData::default()
        });

    DagData {
        workspace_id,
        dags,
        current,
        runner,
        target_missing,
    }
}

/// 지정한 DAG가 없으면 대체하지 않는다. 미지정이면 실행 중인 것, 그다음 최근 갱신한 것을 고른다.
fn pick_target<'a>(
    summaries: &'a [tasty_agent::DagSummary],
    dag_id: Option<&str>,
) -> Option<&'a tasty_agent::DagSummary> {
    if let Some(id) = dag_id {
        return summaries.iter().find(|s| s.id == id);
    }
    summaries
        .iter()
        .filter(|s| s.rollup_state == "running")
        .max_by_key(|s| s.updated_at)
        .or_else(|| summaries.iter().max_by_key(|s| s.updated_at))
}

/// 테마 토큰 → 레이아웃 설정. 이 crate 는 `Theme` 를 모르므로 여기서 주입한다.
pub fn layout_config(theme: &Theme, direction: DagDirection) -> LayoutConfig {
    LayoutConfig {
        orientation: match direction {
            DagDirection::LeftRight => Orientation::LeftRight,
            DagDirection::TopDown => Orientation::TopDown,
        },
        node_size: (theme.dag_node_width(), theme.dag_node_height()),
        layer_gap: theme.dag_layer_gap(),
        sibling_gap: theme.dag_sibling_gap(),
        ..LayoutConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{DagEdgeData, DagGraphData, DagNodeData, DagRelation};
    use super::*;

    fn target(workspace_id: u32, dag_id: Option<&str>) -> DagSourceTarget<'_> {
        DagSourceTarget {
            workspace_id,
            dag_id,
        }
    }

    fn req(surface_id: SurfaceId) -> DagPollRequest {
        DagPollRequest {
            surface_id,
            workspace_id: 1,
            dag_id: None,
        }
    }

    #[test]
    fn changing_dag_target_discards_the_old_read_and_same_target_failure_keeps_its_snapshot() {
        let (_, engine) = crate::state::tests::test_state();
        let workspace = engine.core_state.workspace_at(0).unwrap().id;
        let mut view = DagGraphSource::default();
        view.poll_if_stale(&engine.read(), target(workspace, Some("first")));
        let old = engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap();
        view.poll_if_stale(&engine.read(), target(workspace, Some("second")));
        assert!(!old.is_live());
        old.complete(Err("stale first result".into()));
        let current = engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap();
        current.complete(Ok(vec![crate::runtime::dag_query::WorkspaceSnapshot {
            id: workspace,
            tasks: Vec::new(),
            dags: Vec::new(),
            runner: (false, false),
        }]));
        view.poll_if_stale(&engine.read(), target(workspace, Some("second")));
        assert!(view.error.is_none());
        assert_eq!(view.data.as_ref().unwrap().workspace_id, workspace);
        view.invalidate_poll();
        view.poll_if_stale(&engine.read(), target(workspace, Some("second")));
        engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap()
            .complete(Err("current store error".into()));
        view.poll_if_stale(&engine.read(), target(workspace, Some("second")));
        assert_eq!(view.error.as_deref(), Some("current store error"));
        assert_eq!(view.data.as_ref().unwrap().workspace_id, workspace);
    }

    #[test]
    fn retired_workspace_and_replaced_activation_clear_display_and_cancel_pending_reads() {
        let (_, engine) = crate::state::tests::test_state();
        let workspace = engine.core_state.workspace_at(0).unwrap().id;
        let mut view = DagGraphView::default();
        view.bind_surface_activation(Some(1));
        view.source
            .poll_if_stale(&engine.read(), target(workspace, None));
        engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap()
            .complete(Ok(vec![crate::runtime::dag_query::WorkspaceSnapshot {
                id: workspace,
                tasks: Vec::new(),
                dags: Vec::new(),
                runner: (false, false),
            }]));
        view.source
            .poll_if_stale(&engine.read(), target(workspace, None));
        assert!(view.source.data.is_some());
        view.source.invalidate_poll();
        view.source
            .poll_if_stale(&engine.read(), target(workspace, None));
        engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap()
            .complete(Err(crate::runtime::dag_query::ReadError::Retired));
        view.source
            .poll_if_stale(&engine.read(), target(workspace, None));
        assert!(view.source.data.is_none());
        view.source.invalidate_poll();
        view.source
            .poll_if_stale(&engine.read(), target(workspace, None));
        let old = engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap();
        view.canvas.selected = Some("old-task".into());
        view.bind_surface_activation(Some(2));
        assert!(!old.is_live());
        assert!(view.canvas.selected.is_none());
        assert!(view.source.query.is_none());
    }

    #[test]
    fn dropping_a_view_removes_it_from_the_poll_deadlines() {
        let now = Instant::now();
        let mut store = DagGraphViewStore::default();
        store.get_or_init(7);
        store.note_visible(&[req(7)]);
        assert_eq!(store.pending_poll_deadlines(now).len(), 1);

        store.drop_view(7);
        assert!(
            store.pending_poll_deadlines(now).is_empty(),
            "닫힌 뷰가 폴링 예약을 남기면 누수가 된다"
        );
    }

    /// 닫히지 않고 배경으로 간 뷰는 상태를 남기되 조회 예약에서는 제외한다.
    #[test]
    fn a_frame_with_no_visible_dag_view_clears_the_poll_deadlines() {
        let now = Instant::now();
        let mut store = DagGraphViewStore::default();
        store.get_or_init(7);
        store.note_visible(&[req(7)]);
        assert_eq!(store.pending_poll_deadlines(now).len(), 1);

        store.note_visible(&[]);
        assert!(store.pending_poll_deadlines(now).is_empty());
        assert!(
            store.views.contains_key(&7),
            "배경으로 밀렸을 뿐 닫힌 게 아니다 — 뷰 상태는 보존한다"
        );
    }

    #[test]
    fn only_the_views_visible_this_frame_keep_their_deadlines() {
        let now = Instant::now();
        let mut store = DagGraphViewStore::default();
        store.get_or_init(7);
        store.get_or_init(9);
        store.note_visible(&[req(7), req(9)]);
        assert_eq!(store.pending_poll_deadlines(now).len(), 2);

        store.note_visible(&[req(9)]);
        let due = store.pending_poll_deadlines(now);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].0, 9);
    }

    #[test]
    fn poll_deadline_advances_after_a_read() {
        let now = Instant::now();
        let mut view = DagGraphSource::default();
        assert_eq!(view.next_poll_at(), None, "미폴링 = 즉시");
        view.last_poll = Some(now);
        assert_eq!(view.next_poll_at(), Some(now + POLL_INTERVAL));
    }

    #[test]
    fn explicit_target_is_never_silently_substituted() {
        let summaries = vec![
            summary("d:one", "running", 10),
            summary("d:two", "waiting", 20),
        ];
        assert_eq!(pick_target(&summaries, Some("d:two")).unwrap().id, "d:two");
        assert!(pick_target(&summaries, Some("d:gone")).is_none());
        assert_eq!(pick_target(&summaries, None).unwrap().id, "d:one");
        let idle = vec![
            summary("d:one", "succeeded", 10),
            summary("d:two", "waiting", 20),
        ];
        assert_eq!(pick_target(&idle, None).unwrap().id, "d:two");
        assert!(pick_target(&[], None).is_none());
    }

    /// 상태만 바뀌면 레이아웃 캐시 키는 유지한다.
    #[test]
    fn shape_key_ignores_task_state() {
        let lr = LayoutConfig::default();
        let key = |data: &DagData, cfg: &LayoutConfig| {
            tasty_ui_widgets::graph_shape_key(data.current.as_ref(), cfg)
        };
        let waiting = key(
            &graph_data(&[("a", DagStatus::Waiting), ("b", DagStatus::Waiting)]),
            &lr,
        );

        for state in [
            DagStatus::Ready,
            DagStatus::Running,
            DagStatus::Succeeded,
            DagStatus::Failed,
            DagStatus::Cancelled,
            DagStatus::Skipped,
            DagStatus::Unknown,
        ] {
            assert_eq!(
                key(&graph_data(&[("a", state), ("b", DagStatus::Running)]), &lr),
                waiting,
                "state {state:?} 가 레이아웃 캐시를 무효화했다"
            );
        }

        let three = graph_data(&[
            ("a", DagStatus::Waiting),
            ("b", DagStatus::Waiting),
            ("c", DagStatus::Waiting),
        ]);
        assert_ne!(key(&three, &lr), waiting);

        let two = graph_data(&[("a", DagStatus::Waiting), ("b", DagStatus::Waiting)]);
        assert_eq!(key(&two, &lr), waiting);
        let td = LayoutConfig {
            orientation: Orientation::TopDown,
            ..LayoutConfig::default()
        };
        assert_ne!(key(&two, &td), waiting);
        let wider = LayoutConfig {
            layer_gap: lr.layer_gap + tasty_type_geometry::length::LogicalPx(8.0),
            ..LayoutConfig::default()
        };
        assert_ne!(key(&two, &wider), waiting);
    }

    /// `(id, 상태)` 목록 → 앞 노드에서 뒤 노드로 이어지는 사슬 그래프.
    fn graph_data(nodes: &[(&str, DagStatus)]) -> DagData {
        let graph = DagGraphData {
            id: "d:test".to_string(),
            name: "test".to_string(),
            nodes: nodes
                .iter()
                .map(|(id, status)| DagNodeData {
                    id: (*id).to_string(),
                    name: (*id).to_string(),
                    status: *status,
                    command_kind: "run",
                    on_failure_kind: "abort",
                    command_text: String::new(),
                    started_at: None,
                    finished_at: None,
                    exit_code: None,
                    error_tail: None,
                    output_tail: None,
                    incoming: Vec::new(),
                    skip: None,
                    phase: None,
                    unknown_reason: None,
                })
                .collect(),
            edges: (1..nodes.len())
                .map(|i| DagEdgeData {
                    from: i - 1,
                    to: i,
                    relation: DagRelation::DependsOn,
                    selection: None,
                })
                .collect(),
            cycle: None,
            done: 0,
        };
        DagData {
            workspace_id: 1,
            dags: Vec::new(),
            current: Some(graph),
            runner: RunnerBadgeData::default(),
            target_missing: false,
        }
    }

    fn summary(id: &str, rollup: &'static str, updated_at: u64) -> tasty_agent::DagSummary {
        tasty_agent::DagSummary {
            id: id.to_string(),
            workspace_id: 1,
            name: id.to_string(),
            source: "explicit",
            task_count: 0,
            state_counts: Default::default(),
            rollup_state: rollup,
            created_at: 0,
            updated_at,
            root_task_ids: Vec::new(),
            has_cycle: false,
            task_ids: Vec::new(),
        }
    }
}
