//! 기존 layout 슬롯 JSON을 새 구조 journal로 가져온다.
//!
//! 슬롯은 엔진 하나의 자료이므로 그 엔진의 구조 stream([`journal::engine_stream`])에 가져온다.
//! 슬롯 하나를 이벤트 batch 하나로 확정한다. 새 ID는 journal의 ID 예약에서 받으므로 여러 슬롯을
//! 가져와도 엔진 사이에서 겹치지 않는다. surface ID는 standalone PTY 범위 아래에서만 받는다. 슬롯 안의 위치
//! (workspace 순서, 깊이 우선 leaf pane 순서, tab 순서, 깊이 우선 surface 순서)와 새 ID의 대응을
//! 결과와 명령 기록에 함께 남긴다. 같은 슬롯을 같은 내용으로 다시 가져오면 저장된 대응을 돌려주고
//! 새로 쓰지 않는다. 선택·접힘 같은 화면 상태는 이벤트로 만들지 않고 결과로만 돌려준다.
//!
//! workspace 부제·설명과 tab의 사용자 지정 이름은 전용 이벤트로 기록한다. 옛 형식의 attach 매핑은 읽고 버린다.
//! terminal의 cwd·복원 명령·scrollback과 plugin surface 자료는 이벤트가 아니라 surface 저장 자료
//! ([`surface_data::SurfaceData`]) payload로 저장하고 자료 참조로 가리킨다. 제품 worker가 선택된
//! 슬롯에 구조 stream이 아직 없을 때 호출하며, 이미 활성화한 stream에는 파일을 다시 가져오지 않는다.

pub(crate) mod journal_import;
pub(crate) mod surface_data;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use tasty_core::{
    CodecError, DataRef, DomainEvent, IdKind, JournalModel, Placement, Ratio, SplitSpec,
    SurfaceSpec,
};
use tasty_event_store::{
    BatchId, CommandKey, CommandLookup, CommandStatus, CommitOutcome, CommitRequest, EventStore,
    ExpectedRevision, NewCommand, NewEvent, PayloadRef, StoreError, StreamAppend, StreamId,
    WriterEpoch,
};

use surface_data::SurfaceData;

use super::schema::{
    SavedCategory, SavedLayout, SavedPaneNode, SavedSurface, SavedSurfaceLayout, SavedWorkspace,
};
use super::{LayoutSlotId, SlotLoad, classify_slot_json};
use crate::model::{
    NORMAL_CATEGORY_ID, PaneId, SurfaceId, TabId, WorkspaceCategoryId, WorkspaceId,
};
use crate::runtime::journal::{self, JournalError};
use crate::runtime::terminal_store::PTY_ID_BASE;

/// 명령 기록의 호출자 범위. 슬롯 번호가 재시도 키가 된다.
pub(crate) const IMPORT_SCOPE: &str = "layout-import";

/// terminal surface의 kind.
pub(crate) const TERMINAL_KIND: &str = "terminal";

/// 저장된 scrollback 바이트를 읽는다. 파일 위치 규칙은 scrollback 저장소가 정한다.
pub(crate) trait ScrollbackSource {
    /// 없으면 `Ok(None)`이다.
    fn read_bytes(&self, persist_id: &str) -> std::io::Result<Option<Vec<u8>>>;
}

#[derive(Debug)]
pub(crate) enum ImportError {
    /// 슬롯 JSON을 해석하지 못했다.
    Unparsable,
    /// 이 빌드보다 새 슬롯 형식이다.
    UnsupportedVersion,
    /// 같은 슬롯을 다른 내용으로 이미 가져왔다.
    AlreadyImportedDifferently(LayoutSlotId),
    /// 예약한 ID가 runtime ID 폭(u32)을 넘었다.
    IdOverflow(u64),
    Store(StoreError),
    Codec(CodecError),
    Journal(JournalError),
    /// 슬롯에서 센 수보다 ID가 모자랐다. 계산 결함이다.
    IdPoolExhausted(IdKind),
    /// 명령 기록의 대응 자료를 쓰거나 읽지 못했다.
    Mapping(serde_json::Error),
    /// surface 저장 자료를 만들지 못했다.
    SurfaceData(serde_json::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unparsable => f.write_str("layout slot is not valid layout JSON"),
            Self::UnsupportedVersion => {
                f.write_str("layout slot version is newer than this build supports")
            }
            Self::AlreadyImportedDifferently(slot) => {
                write!(
                    f,
                    "layout slot {slot} was already imported with other content"
                )
            }
            Self::IdOverflow(id) => write!(f, "reserved id {id} does not fit a runtime id"),
            Self::Store(error) => write!(f, "{error}"),
            Self::Codec(error) => write!(f, "{error}"),
            Self::Journal(error) => write!(f, "{error}"),
            Self::IdPoolExhausted(kind) => {
                write!(f, "reserved {} ids ran out while importing", kind.label())
            }
            Self::Mapping(error) => write!(f, "import mapping: {error}"),
            Self::SurfaceData(error) => write!(f, "surface data: {error}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<StoreError> for ImportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<CodecError> for ImportError {
    fn from(error: CodecError) -> Self {
        Self::Codec(error)
    }
}

impl From<JournalError> for ImportError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}

/// 슬롯 안의 위치와 새 journal ID의 대응. 목록 순서가 슬롯의 위치다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImportMapping {
    pub(crate) slot: LayoutSlotId,
    /// (슬롯의 카테고리 ID, 새 카테고리 ID). normal(0)은 그대로다.
    pub(crate) categories: Vec<(WorkspaceCategoryId, WorkspaceCategoryId)>,
    pub(crate) workspaces: Vec<ImportedWorkspace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImportedWorkspace {
    pub(crate) id: WorkspaceId,
    /// 왼쪽부터 깊이 우선으로 센 leaf pane 순서.
    pub(crate) panes: Vec<ImportedPane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImportedPane {
    pub(crate) id: PaneId,
    pub(crate) tabs: Vec<ImportedTab>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImportedTab {
    pub(crate) id: TabId,
    /// 왼쪽부터 깊이 우선으로 센 leaf surface 순서.
    pub(crate) surfaces: Vec<SurfaceId>,
}

impl ImportMapping {
    pub(crate) fn category(&self, saved: WorkspaceCategoryId) -> Option<WorkspaceCategoryId> {
        self.categories
            .iter()
            .find(|(s, _)| *s == saved)
            .map(|(_, new)| *new)
    }

    #[cfg(test)]
    pub(crate) fn saved_category(&self, new: WorkspaceCategoryId) -> Option<WorkspaceCategoryId> {
        self.categories
            .iter()
            .find(|(_, n)| *n == new)
            .map(|(saved, _)| *saved)
    }

    #[cfg(test)]
    pub(crate) fn workspace_index(&self, id: WorkspaceId) -> Option<usize> {
        self.workspaces.iter().position(|w| w.id == id)
    }

    /// (workspace 순서, leaf pane 순서).
    #[cfg(test)]
    pub(crate) fn pane_index(&self, id: PaneId) -> Option<(usize, usize)> {
        self.positions()
            .find(|p| p.pane == id)
            .map(|p| (p.workspace_index, p.pane_index))
    }

    /// (workspace 순서, leaf pane 순서, tab 순서).
    #[cfg(test)]
    pub(crate) fn tab_index(&self, id: TabId) -> Option<(usize, usize, usize)> {
        self.positions()
            .find(|p| p.tab == id)
            .map(|p| (p.workspace_index, p.pane_index, p.tab_index))
    }

    /// (workspace 순서, leaf pane 순서, tab 순서, leaf surface 순서).
    #[cfg(test)]
    pub(crate) fn surface_index(&self, id: SurfaceId) -> Option<(usize, usize, usize, usize)> {
        self.positions().find(|p| p.surface == id).map(|p| {
            (
                p.workspace_index,
                p.pane_index,
                p.tab_index,
                p.surface_index,
            )
        })
    }

    #[cfg(test)]
    fn positions(&self) -> impl Iterator<Item = Position> + '_ {
        self.workspaces.iter().enumerate().flat_map(|(wi, w)| {
            w.panes.iter().enumerate().flat_map(move |(pi, p)| {
                p.tabs.iter().enumerate().flat_map(move |(ti, t)| {
                    t.surfaces.iter().enumerate().map(move |(si, s)| Position {
                        workspace_index: wi,
                        pane_index: pi,
                        tab_index: ti,
                        surface_index: si,
                        pane: p.id,
                        tab: t.id,
                        surface: *s,
                    })
                })
            })
        })
    }
}

#[cfg(test)]
struct Position {
    workspace_index: usize,
    pane_index: usize,
    tab_index: usize,
    surface_index: usize,
    pane: PaneId,
    tab: TabId,
    surface: SurfaceId,
}

/// 이벤트로 만들지 않은 화면 상태. 범위를 벗어난 선택 위치는 비워 둔다.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct ImportedView {
    pub(crate) active_workspace: Option<WorkspaceId>,
    pub(crate) focused_panes: BTreeMap<WorkspaceId, PaneId>,
    pub(crate) active_tabs: BTreeMap<PaneId, TabId>,
    #[serde(default)]
    pub(crate) selected_surfaces: BTreeMap<TabId, SurfaceId>,
    pub(crate) collapsed_categories: Vec<WorkspaceCategoryId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportOutcome {
    pub(crate) mapping: ImportMapping,
    pub(crate) view: ImportedView,
    /// 이번 호출이 확정한 batch. 이미 가져온 슬롯이면 `None`이다.
    pub(crate) batch: Option<BatchId>,
    /// 참조했지만 파일이 없어 참조만 기록한 scrollback.
    pub(crate) missing_scrollback: Vec<String>,
    /// 목록에 없는 카테고리를 가리켜 normal로 옮긴 workspace 순서.
    pub(crate) moved_to_normal: Vec<usize>,
}

/// 슬롯 JSON 하나를 그 엔진의 구조 stream에 가져온다. 이벤트·명령 기록은 한 transaction으로 확정한다.
/// ID 예약과 scrollback payload는 그보다 먼저 각자 확정하며, 확정이 실패하면 빈 구간과
/// 참조되지 않은 payload가 남는다.
pub(crate) fn import_slot(
    store: &mut EventStore,
    epoch: WriterEpoch,
    slot: LayoutSlotId,
    slot_json: &str,
    scrollback: &dyn ScrollbackSource,
) -> Result<ImportOutcome, ImportError> {
    let layout = match classify_slot_json(slot_json) {
        SlotLoad::Loaded(layout) => layout,
        SlotLoad::Unreadable => return Err(ImportError::UnsupportedVersion),
        SlotLoad::Unparsable | SlotLoad::Absent => return Err(ImportError::Unparsable),
    };
    let key = CommandKey {
        caller_scope: IMPORT_SCOPE.to_owned(),
        idempotency_key: format!("slot-{slot}"),
    };
    let digest = slot_json.as_bytes();
    match store.lookup_command(&key, digest)? {
        CommandLookup::Hit(record) => {
            let mapping: ImportMapping =
                serde_json::from_slice(&record.resolved).map_err(ImportError::Mapping)?;
            let view = view_of(&layout, &mapping);
            return Ok(ImportOutcome {
                mapping,
                view,
                batch: None,
                missing_scrollback: Vec::new(),
                moved_to_normal: Vec::new(),
            });
        }
        CommandLookup::DigestMismatch(_) => {
            return Err(ImportError::AlreadyImportedDifferently(slot));
        }
        CommandLookup::Miss => {}
    }

    let stream = journal::engine_stream(slot);
    let model = journal::load(store, &stream)?;
    let categories = categories_of(&layout);
    let ids = IdPools::reserve(store, epoch, &layout, &categories)?;
    let mut plan = Plan::new(ids, &model);
    plan.categories(&categories, &model)?;
    let mut sink = Sink {
        store,
        epoch,
        scrollback,
        holder: format!("admission/{}/legacy-slot-{slot}", epoch.0),
    };
    for (index, workspace) in layout.workspaces.iter().enumerate() {
        plan.workspace(&mut sink, index, workspace)?;
    }
    let mapping = ImportMapping {
        slot,
        categories: plan.category_map.clone(),
        workspaces: plan.workspaces.clone(),
    };
    let view = view_of(&layout, &mapping);
    let command_id = format!("import-slot-{slot}-{}", epoch.0);
    let request = commit_request(
        epoch,
        &command_id,
        key,
        digest,
        &mapping,
        &view,
        &plan,
        store,
        stream,
    )?;
    let batch = match store.commit(&request)? {
        CommitOutcome::Committed { batch } => batch.map(|cut| cut.batch_id),
        // 조회와 확정 사이에 같은 키가 기록됐다. 저장된 대응을 쓴다.
        CommitOutcome::Duplicate(record) => {
            let mapping: ImportMapping =
                serde_json::from_slice(&record.resolved).map_err(ImportError::Mapping)?;
            let view = view_of(&layout, &mapping);
            return Ok(ImportOutcome {
                mapping,
                view,
                batch: None,
                missing_scrollback: Vec::new(),
                moved_to_normal: Vec::new(),
            });
        }
    };
    let view = view_of(&layout, &mapping);
    if let Err(error) =
        store.release_payload_holder(epoch, &format!("admission/{}/legacy-slot-{slot}", epoch.0))
    {
        tracing::warn!(%error, "legacy import committed; admission pin cleanup deferred");
    }
    Ok(ImportOutcome {
        mapping,
        view,
        batch,
        missing_scrollback: plan.missing_scrollback,
        moved_to_normal: plan.moved_to_normal,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the commit request needs every value that the import plan already holds separately"
)]
fn commit_request(
    epoch: WriterEpoch,
    command_id: &str,
    key: CommandKey,
    digest: &[u8],
    mapping: &ImportMapping,
    view: &ImportedView,
    plan: &Plan,
    store: &EventStore,
    stream: StreamId,
) -> Result<CommitRequest, ImportError> {
    let head = store.stream_revision(&stream)?;
    let mut events = Vec::with_capacity(plan.events.len());
    for (index, (event, pins)) in plan.events.iter().enumerate() {
        events.push(NewEvent {
            event_id: format!("{command_id}/{index}"),
            payload: journal::to_payload(event)?,
            recorded_at_ms: 0,
            causation_id: None,
            actor: "layout-import".to_owned(),
            origin: format!("layout-slot-{}", mapping.slot),
            payload_refs: pins.clone(),
        });
    }
    let mut request = CommitRequest::new(epoch);
    request.command = Some(NewCommand {
        command_id: command_id.to_owned(),
        key: Some(key),
        request_digest: digest.to_vec(),
        resolved: serde_json::to_vec(mapping).map_err(ImportError::Mapping)?,
        status: CommandStatus::Completed,
        response: Some(serde_json::to_vec(view).map_err(ImportError::Mapping)?),
    });
    if !events.is_empty() {
        request.appends.push(StreamAppend {
            stream_id: stream,
            expected: head.map_or(ExpectedRevision::NoStream, ExpectedRevision::Exact),
            events,
        });
    }
    Ok(request)
}

/// 카테고리 목록이 없던 슬롯은 복원과 같이 normal 하나로 본다.
fn categories_of(layout: &SavedLayout) -> Vec<SavedCategory> {
    if layout.categories.is_empty() {
        return vec![SavedCategory {
            id: NORMAL_CATEGORY_ID,
            name: tasty_model::NORMAL_CATEGORY_NAME.to_owned(),
            collapsed: false,
        }];
    }
    layout
        .categories
        .iter()
        .map(|c| SavedCategory {
            id: c.id,
            name: c.name.clone(),
            collapsed: c.collapsed,
        })
        .collect()
}

/// 슬롯의 선택 위치를 새 ID로 옮긴다.
fn view_of(layout: &SavedLayout, mapping: &ImportMapping) -> ImportedView {
    let mut view = ImportedView {
        active_workspace: mapping
            .workspaces
            .get(
                layout
                    .active_workspace
                    .min(mapping.workspaces.len().saturating_sub(1)),
            )
            .map(|w| w.id),
        ..ImportedView::default()
    };
    for (saved, imported) in layout.workspaces.iter().zip(&mapping.workspaces) {
        if let Some(pane) = imported.panes.get(saved.focused_pane_index) {
            view.focused_panes.insert(imported.id, pane.id);
        }
        let mut saved_panes = Vec::new();
        collect_panes(&saved.pane_layout, &mut saved_panes);
        for (saved_pane, pane) in saved_panes.iter().zip(&imported.panes) {
            if let Some(tab) = pane
                .tabs
                .get(saved_pane.active_tab.min(pane.tabs.len().saturating_sub(1)))
            {
                view.active_tabs.insert(pane.id, tab.id);
            }
        }
    }
    for category in categories_of(layout) {
        if category.collapsed
            && let Some(id) = mapping.category(category.id)
        {
            view.collapsed_categories.push(id);
        }
    }
    view
}

fn collect_panes<'a>(node: &'a SavedPaneNode, out: &mut Vec<&'a super::schema::SavedPane>) {
    match node {
        SavedPaneNode::Leaf(pane) => out.push(pane),
        SavedPaneNode::Split { first, second, .. } => {
            collect_panes(first, out);
            collect_panes(second, out);
        }
    }
}

fn count_surfaces(node: &SavedSurfaceLayout) -> u64 {
    match node {
        SavedSurfaceLayout::Leaf(_) => 1,
        SavedSurfaceLayout::Split { first, second, .. } => {
            count_surfaces(first) + count_surfaces(second)
        }
    }
}

/// kind별 예약 상한. surface는 standalone PTY ID와 겹치지 않게 그 기준값 아래에서만 받는다.
fn max_id(kind: IdKind) -> u64 {
    match kind {
        IdKind::Surface => u64::from(PTY_ID_BASE - 1),
        _ => u64::from(u32::MAX),
    }
}

/// 가져올 객체 수만큼 미리 예약한 ID.
struct IdPools {
    pools: BTreeMap<IdKind, std::vec::IntoIter<u32>>,
}

impl IdPools {
    fn reserve(
        store: &mut EventStore,
        epoch: WriterEpoch,
        layout: &SavedLayout,
        categories: &[SavedCategory],
    ) -> Result<Self, ImportError> {
        let mut counts = BTreeMap::from([
            (
                IdKind::Category,
                categories
                    .iter()
                    .filter(|c| c.id != NORMAL_CATEGORY_ID)
                    .count() as u64,
            ),
            (IdKind::Workspace, layout.workspaces.len() as u64),
            (IdKind::Pane, 0),
            (IdKind::Tab, 0),
            (IdKind::Surface, 0),
        ]);
        for workspace in &layout.workspaces {
            let mut panes = Vec::new();
            collect_panes(&workspace.pane_layout, &mut panes);
            *counts.entry(IdKind::Pane).or_default() += panes.len() as u64;
            for pane in panes {
                *counts.entry(IdKind::Tab).or_default() += pane.tabs.len() as u64;
                for tab in &pane.tabs {
                    *counts.entry(IdKind::Surface).or_default() += count_surfaces(&tab.surface);
                }
            }
        }
        let mut pools = BTreeMap::new();
        for (kind, count) in counts {
            let mut ids = Vec::new();
            if count > 0 {
                let range = store.reserve_ids(epoch, kind.label(), count, max_id(kind))?;
                for id in range.ids() {
                    ids.push(u32::try_from(id).map_err(|_| ImportError::IdOverflow(id))?);
                }
            }
            pools.insert(kind, ids.into_iter());
        }
        Ok(Self { pools })
    }

    /// 예약 수는 슬롯에서 센 수와 같다. 모자라면 계산 결함이므로 오류로 멈춘다.
    fn take(&mut self, kind: IdKind) -> Result<u32, ImportError> {
        self.pools
            .get_mut(&kind)
            .and_then(Iterator::next)
            .ok_or(ImportError::IdPoolExhausted(kind))
    }

    fn take_n(&mut self, kind: IdKind, n: usize) -> Result<Vec<u32>, ImportError> {
        (0..n).map(|_| self.take(kind)).collect()
    }
}

/// payload 저장에 쓰는 journal과 scrollback 공급자.
struct Sink<'a> {
    store: &'a mut EventStore,
    epoch: WriterEpoch,
    scrollback: &'a dyn ScrollbackSource,
    holder: String,
}

/// 만들 이벤트와 대응을 모은다. 이벤트마다 같은 transaction에서 pin할 payload를 함께 둔다.
struct Plan {
    ids: IdPools,
    events: Vec<(DomainEvent, Vec<PayloadRef>)>,
    category_map: Vec<(WorkspaceCategoryId, WorkspaceCategoryId)>,
    workspaces: Vec<ImportedWorkspace>,
    workspace_base: usize,
    missing_scrollback: Vec<String>,
    moved_to_normal: Vec<usize>,
}

impl Plan {
    fn new(ids: IdPools, model: &JournalModel) -> Self {
        Self {
            ids,
            events: Vec::new(),
            category_map: Vec::new(),
            workspaces: Vec::new(),
            workspace_base: model.workspace_order.len(),
            missing_scrollback: Vec::new(),
            moved_to_normal: Vec::new(),
        }
    }

    fn push(&mut self, event: DomainEvent) {
        self.events.push((event, Vec::new()));
    }

    /// normal은 journal에 없을 때만 맨 앞에 만든다. 나머지는 새 ID로 뒤에 붙인다.
    fn categories(
        &mut self,
        categories: &[SavedCategory],
        model: &JournalModel,
    ) -> Result<(), ImportError> {
        let mut next_index = model.category_order.len();
        if !model.categories.contains_key(&NORMAL_CATEGORY_ID) {
            let name = categories
                .iter()
                .find(|c| c.id == NORMAL_CATEGORY_ID)
                .map_or(tasty_model::NORMAL_CATEGORY_NAME, |c| c.name.as_str());
            self.push(DomainEvent::CategoryCreated {
                id: NORMAL_CATEGORY_ID,
                name: name.to_owned(),
                index: 0,
            });
            next_index += 1;
        }
        self.category_map
            .push((NORMAL_CATEGORY_ID, NORMAL_CATEGORY_ID));
        for category in categories.iter().filter(|c| c.id != NORMAL_CATEGORY_ID) {
            let id = self.ids.take(IdKind::Category)?;
            self.push(DomainEvent::CategoryCreated {
                id,
                name: category.name.clone(),
                index: next_index,
            });
            next_index += 1;
            self.category_map.push((category.id, id));
        }
        Ok(())
    }

    fn workspace(
        &mut self,
        sink: &mut Sink<'_>,
        index: usize,
        saved: &SavedWorkspace,
    ) -> Result<(), ImportError> {
        let id = self.ids.take(IdKind::Workspace)?;
        let category = self.category_for(index, saved.category);
        let mut saved_panes = Vec::new();
        collect_panes(&saved.pane_layout, &mut saved_panes);
        let pane_ids = self.ids.take_n(IdKind::Pane, saved_panes.len())?;
        self.push(DomainEvent::WorkspaceCreated {
            id,
            name: saved.name.clone(),
            category,
            index: self.workspace_base + index,
            pane: pane_ids[0],
        });
        self.pane_splits(&saved.pane_layout, &pane_ids);
        self.workspace_details(id, saved);
        let mut panes = Vec::with_capacity(pane_ids.len());
        for (saved_pane, pane) in saved_panes.iter().zip(pane_ids) {
            let mut tabs = Vec::with_capacity(saved_pane.tabs.len());
            for (tab_index, saved_tab) in saved_pane.tabs.iter().enumerate() {
                let tab = ImportedTab {
                    id: self.ids.take(IdKind::Tab)?,
                    surfaces: Vec::new(),
                };
                let tab = self.tab(sink, pane, tab, tab_index, saved_tab)?;
                if let Some(explicit) = &saved_tab.explicit_name {
                    self.push(DomainEvent::TabExplicitNameSet {
                        id: tab.id,
                        name: Some(explicit.clone()),
                    });
                }
                tabs.push(tab);
            }
            panes.push(ImportedPane { id: pane, tabs });
        }
        self.workspaces.push(ImportedWorkspace { id, panes });
        Ok(())
    }

    fn category_for(&mut self, index: usize, saved: WorkspaceCategoryId) -> WorkspaceCategoryId {
        if let Some((_, new)) = self.category_map.iter().find(|(s, _)| *s == saved) {
            return *new;
        }
        tracing::warn!(
            "layout import: workspace {index} refers to unknown category {saved}; using normal"
        );
        self.moved_to_normal.push(index);
        NORMAL_CATEGORY_ID
    }

    /// 분할마다 그 자리의 첫 leaf를 둘로 나눈 뒤 양쪽을 채운다. `ids`는 깊이 우선 leaf 순서다.
    fn pane_splits(&mut self, node: &SavedPaneNode, ids: &[PaneId]) {
        if let SavedPaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } = node
        {
            let left = pane_leaves(first);
            self.push(DomainEvent::PaneSplit {
                target: ids[0],
                pane: ids[left],
                split: split((*direction).into(), *ratio),
            });
            self.pane_splits(first, &ids[..left]);
            self.pane_splits(second, &ids[left..]);
        }
    }

    fn tab(
        &mut self,
        sink: &mut Sink<'_>,
        pane: PaneId,
        mut tab: ImportedTab,
        index: usize,
        saved: &super::schema::SavedTab,
    ) -> Result<ImportedTab, ImportError> {
        let mut leaves = Vec::new();
        collect_surfaces(&saved.surface, &mut leaves);
        tab.surfaces = self.ids.take_n(IdKind::Surface, leaves.len())?;
        let (spec, pins) = self.surface_spec(sink, tab.surfaces[0], leaves[0])?;
        self.events.push((
            DomainEvent::TabCreated {
                id: tab.id,
                pane,
                index,
                name: saved.name.clone(),
                surface: spec,
            },
            pins,
        ));
        self.surface_splits(sink, &saved.surface, &tab.surfaces)?;
        Ok(tab)
    }

    /// pane 분할과 같은 순서로 surface 분할을 만든다. 새 surface의 자료 참조를 함께 pin한다.
    fn surface_splits(
        &mut self,
        sink: &mut Sink<'_>,
        node: &SavedSurfaceLayout,
        ids: &[SurfaceId],
    ) -> Result<(), ImportError> {
        if let SavedSurfaceLayout::Split {
            direction,
            ratio,
            first,
            second,
        } = node
        {
            let left = surface_leaves(first);
            let leaf = first_surface(second);
            let (spec, pins) = self.surface_spec(sink, ids[left], leaf)?;
            self.events.push((
                DomainEvent::SurfaceSplit {
                    target: ids[0],
                    surface: spec,
                    split: split((*direction).into(), *ratio),
                },
                pins,
            ));
            self.surface_splits(sink, first, &ids[..left])?;
            self.surface_splits(sink, second, &ids[left..])?;
        }
        Ok(())
    }

    /// surface 저장 자료를 payload로 저장하고 자료 참조로 둔다. 저장할 값이 없으면 참조도 없다.
    fn surface_spec(
        &mut self,
        sink: &mut Sink<'_>,
        id: SurfaceId,
        saved: &SavedSurface,
    ) -> Result<(SurfaceSpec, Vec<PayloadRef>), ImportError> {
        let (kind, data) = match saved {
            SavedSurface::Terminal {
                cwd,
                restore_command,
                scrollback_ref,
            } => {
                let scrollback = match scrollback_ref {
                    Some(persist_id) => self.scrollback_bytes(sink, persist_id),
                    None => None,
                };
                (
                    TERMINAL_KIND.to_owned(),
                    SurfaceData::Terminal {
                        cwd: cwd.clone(),
                        restore_command: restore_command.clone(),
                        scrollback_ref: scrollback_ref.clone(),
                        scrollback,
                    },
                )
            }
            SavedSurface::Generic { kind, data } => {
                let data = SurfaceData::Generic {
                    data: data.clone(),
                    cwd: None,
                };
                (kind.clone(), data)
            }
        };
        let payload = if data.is_empty() {
            None
        } else {
            let bytes = data.encode().map_err(ImportError::SurfaceData)?;
            Some(
                sink.store
                    .put_payload_pinned(sink.epoch, &bytes, &sink.holder)?,
            )
        };
        Ok((
            SurfaceSpec {
                id,
                kind,
                data: payload.map(|p| DataRef(p.0)),
            },
            payload.into_iter().collect(),
        ))
    }

    /// 파일이 없거나 읽지 못하면 참조만 남기고 내용 없이 계속한다.
    fn scrollback_bytes(&mut self, sink: &mut Sink<'_>, persist_id: &str) -> Option<Vec<u8>> {
        match sink.scrollback.read_bytes(persist_id) {
            Ok(Some(bytes)) => Some(bytes),
            Ok(None) => {
                tracing::warn!(
                    "layout import: scrollback {persist_id} is missing; recording the reference only"
                );
                self.missing_scrollback.push(persist_id.to_owned());
                None
            }
            Err(error) => {
                tracing::warn!(
                    "layout import: reading scrollback {persist_id} failed: {error}; recording the reference only"
                );
                self.missing_scrollback.push(persist_id.to_owned());
                None
            }
        }
    }

    /// 빈 부제·설명은 기본값이라 기록하지 않는다.
    fn workspace_details(&mut self, id: WorkspaceId, saved: &SavedWorkspace) {
        if !saved.subtitle.is_empty() || !saved.description.is_empty() {
            self.push(DomainEvent::WorkspaceDetailsSet {
                id,
                subtitle: saved.subtitle.clone(),
                description: saved.description.clone(),
            });
        }
    }
}

fn pane_leaves(node: &SavedPaneNode) -> usize {
    match node {
        SavedPaneNode::Leaf(_) => 1,
        SavedPaneNode::Split { first, second, .. } => pane_leaves(first) + pane_leaves(second),
    }
}

fn surface_leaves(node: &SavedSurfaceLayout) -> usize {
    match node {
        SavedSurfaceLayout::Leaf(_) => 1,
        SavedSurfaceLayout::Split { first, second, .. } => {
            surface_leaves(first) + surface_leaves(second)
        }
    }
}

fn first_surface(node: &SavedSurfaceLayout) -> &SavedSurface {
    match node {
        SavedSurfaceLayout::Leaf(surface) => surface,
        SavedSurfaceLayout::Split { first, .. } => first_surface(first),
    }
}

fn collect_surfaces<'a>(node: &'a SavedSurfaceLayout, out: &mut Vec<&'a SavedSurface>) {
    match node {
        SavedSurfaceLayout::Leaf(surface) => out.push(surface),
        SavedSurfaceLayout::Split { first, second, .. } => {
            collect_surfaces(first, out);
            collect_surfaces(second, out);
        }
    }
}

fn split(direction: tasty_model::SplitDirection, ratio: f32) -> SplitSpec {
    SplitSpec {
        direction,
        ratio: Ratio::from_f32(ratio),
        placement: Placement::After,
    }
}
