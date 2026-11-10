//! 저널 전용 구조 모델. 이벤트를 적용한 결과이며 GUI·PTY·선택 상태를 담지 않는다.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tasty_model::{
    PaneId, RemoteBinding, SplitDirection, SurfaceId, TabId, WorkspaceCategoryId, WorkspaceId,
};

use crate::ids::{BatchId, Revision};

/// 분할 비율. f32의 비트를 그대로 보관해 인코딩 왕복에서 값이 바뀌지 않게 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ratio(u32);

impl Ratio {
    pub fn from_f32(value: f32) -> Self {
        Self(value.to_bits())
    }

    pub fn to_f32(self) -> f32 {
        f32::from_bits(self.0)
    }

    pub fn bits(self) -> u32 {
        self.0
    }
}

/// `tasty_model::SplitDirection`의 직렬화 형식. 모델 타입에 serde를 더하지 않고 여기서 정한다.
#[derive(Serialize, Deserialize)]
#[serde(remote = "SplitDirection")]
pub(crate) enum SplitDirectionDef {
    Horizontal,
    Vertical,
}

/// 새 노드를 기존 노드의 어느 쪽에 둘지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Placement {
    Before,
    After,
}

/// 분할 트리. pane 배치와 tab 안의 surface 배치가 같은 형태를 쓴다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SplitTree<Id> {
    Leaf(Id),
    Split {
        #[serde(with = "SplitDirectionDef")]
        direction: SplitDirection,
        ratio: Ratio,
        first: Box<SplitTree<Id>>,
        second: Box<SplitTree<Id>>,
    },
}

impl<Id: Copy + PartialEq> SplitTree<Id> {
    /// 왼쪽부터 깊이 우선 순서의 leaf.
    pub fn leaves(&self) -> Vec<Id> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<Id>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }

    /// `target` leaf를 `target`과 `new` 두 leaf의 분할로 바꾼다. 없으면 false.
    pub(crate) fn split_leaf(
        &mut self,
        target: Id,
        new: Id,
        direction: SplitDirection,
        ratio: Ratio,
        placement: Placement,
    ) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                let (first, second) = match placement {
                    Placement::Before => (new, target),
                    Placement::After => (target, new),
                };
                *self = Self::Split {
                    direction,
                    ratio,
                    first: Box::new(Self::Leaf(first)),
                    second: Box::new(Self::Leaf(second)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split_leaf(target, new, direction, ratio, placement)
                    || second.split_leaf(target, new, direction, ratio, placement)
            }
        }
    }

    /// `target` leaf를 빼고 부모 분할을 형제로 접는다. 마지막 leaf는 뺄 수 없다.
    pub(crate) fn remove_leaf(&mut self, target: Id) -> RemoveLeaf {
        match self {
            Self::Leaf(id) if *id == target => RemoveLeaf::LastLeaf,
            Self::Leaf(_) => RemoveLeaf::NotFound,
            Self::Split { first, second, .. } => {
                let keep = if matches!(**first, Self::Leaf(id) if id == target) {
                    Some(second.as_ref().clone())
                } else if matches!(**second, Self::Leaf(id) if id == target) {
                    Some(first.as_ref().clone())
                } else {
                    None
                };
                if let Some(keep) = keep {
                    *self = keep;
                    return RemoveLeaf::Removed;
                }
                match first.remove_leaf(target) {
                    RemoveLeaf::NotFound => second.remove_leaf(target),
                    other => other,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoveLeaf {
    Removed,
    NotFound,
    LastLeaf,
}

/// surface kind 고유 저장 자료의 참조. 내용은 journal의 불변 payload에 있고 해석은 kind 소유자가 한다.
/// terminal의 cwd·복원 명령·scrollback처럼 관측해 저장하는 값은 이벤트가 아니라 이 자료에 담는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DataRef(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub category: WorkspaceCategoryId,
    pub layout: SplitTree<PaneId>,
    /// 사용자·에이전트가 쓰는 부제. 없으면 빈 문자열이다.
    pub subtitle: String,
    pub description: String,
    /// 사용자 정의 키만 담는다. 위의 typed 속성을 예약 키로 넣지 않는다.
    pub metadata: BTreeMap<String, String>,
}

/// 사용자가 GUI로 붙인 원격 워크스페이스의 사이드바 행. 내용(탭·pane·surface)은 저장하지 않는다.
/// id는 그 행의 로컬 workspace id다. 로컬 workspace 순서와 따로 보관하며 `index`는 로컬과 원격을
/// 합친 사이드바 표시 순서에서의 자리다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRow {
    pub name: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub description: String,
    pub category: WorkspaceCategoryId,
    pub index: usize,
    pub binding: RemoteBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    pub workspace: WorkspaceId,
    pub tabs: Vec<TabId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub pane: PaneId,
    pub name: String,
    /// 사용자가 지정한 이름. 빈 문자열도 지정한 값이며 `None`과 다르다.
    pub explicit_name: Option<String>,
    pub layout: SplitTree<SurfaceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub tab: TabId,
    /// terminal·markdown·plugin kind 등. 해석은 kind 소유자가 한다.
    pub kind: String,
    pub data: Option<DataRef>,
    /// Kind launch seed remains pinned until an independent capture can restore the surface.
    #[serde(default)]
    pub creation_seed: Option<DataRef>,
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub activation: Option<crate::Activation>,
    /// Capture request order within an activation, distinct from resource identity.
    #[serde(default)]
    pub content_generation: u64,
    #[serde(default)]
    pub snapshot_schema: u32,
}

/// 모델에 마지막으로 적용한 journal 위치.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Applied {
    /// 마지막으로 적용한 batch. 구조 이벤트가 없는 batch도 포함한다.
    pub batch: Option<BatchId>,
    /// 구조 stream의 마지막 적용 revision.
    pub revision: Option<Revision>,
}

/// 엔진 하나의 구조 stream을 적용한 결과. 순서는 `*_order`와 각 부모의 목록이 정한다.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JournalModel {
    pub applied: Applied,
    #[serde(default)]
    pub engine_incarnation: u64,
    #[serde(default)]
    pub engine_retired: bool,
    /// Survives operation compaction and failed or cancelled preparation.
    #[serde(default)]
    pub activation_high_water: BTreeMap<SurfaceId, u64>,
    pub categories: BTreeMap<WorkspaceCategoryId, Category>,
    pub category_order: Vec<WorkspaceCategoryId>,
    pub workspaces: BTreeMap<WorkspaceId, Workspace>,
    pub workspace_order: Vec<WorkspaceId>,
    /// 저장된 원격 워크스페이스 행. 구조 비교(digest)에 들어가지 않는다.
    #[serde(default)]
    pub remote_rows: BTreeMap<WorkspaceId, RemoteRow>,
    pub panes: BTreeMap<PaneId, Pane>,
    pub tabs: BTreeMap<TabId, Tab>,
    pub surfaces: BTreeMap<SurfaceId, Surface>,
    #[serde(default)]
    pub operations: BTreeMap<crate::OperationId, crate::Operation>,
    #[serde(default)]
    pub undo_records: Vec<crate::UndoRecord>,
}

impl JournalModel {
    /// Snapshot pins include pending preparation and reconciliation evidence, not only visible surfaces.
    pub fn data_refs(&self) -> impl Iterator<Item = DataRef> + '_ {
        self.surfaces
            .values()
            .flat_map(|surface| surface.data.into_iter().chain(surface.creation_seed))
            // Terminal known results retain command identity/history, but do not keep a second
            // live undo/capture pin after UndoRecordConsumed/Evicted. Event retention pins remain
            // independent until log compaction is allowed to remove that history.
            .chain(
                self.operations
                    .values()
                    .filter(|operation| {
                        operation.outcome.is_none()
                            || matches!(
                                operation.outcome,
                                Some(crate::OperationOutcome::Uncertain { .. })
                            )
                            || operation.creation.as_ref().is_some_and(|plan| {
                                match &plan.destination {
                                    crate::CreationDestination::Assembly { operation: group } => {
                                        self.operations.get(group).is_some_and(|coordinator| {
                                            coordinator.outcome.is_none()
                                                || matches!(
                                                    coordinator.outcome,
                                                    Some(crate::OperationOutcome::Uncertain { .. })
                                                )
                                        })
                                    }
                                    _ => false,
                                }
                            })
                    })
                    .flat_map(|operation| {
                        std::iter::once(operation.input)
                            .chain(operation.assembly.iter().flat_map(|plan| plan.data_refs()))
                            .chain(operation.reconciliation_evidence)
                            .chain(operation.prepared_data)
                            .chain(
                                operation
                                    .creation
                                    .as_ref()
                                    .and_then(|plan| plan.surface.data),
                            )
                            .chain(
                                operation
                                    .retirement
                                    .as_ref()
                                    .and_then(|plan| plan.undo.as_ref())
                                    .into_iter()
                                    .flat_map(|capture| capture.data_refs()),
                            )
                    }),
            )
            .chain(
                self.undo_records
                    .iter()
                    .flat_map(|record| record.capture.data_refs()),
            )
    }
}
