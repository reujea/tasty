//! 엔진 구조 stream의 도메인 이벤트. 이미 결정된 사실만 담으며 적용 중에 새 ID·시각을 만들지 않는다.

use serde::{Deserialize, Serialize};

use tasty_model::{PaneId, SplitDirection, SurfaceId, TabId, WorkspaceCategoryId, WorkspaceId};

use crate::ids::{BatchId, Revision};
use crate::model::{DataRef, Placement, Ratio, SplitDirectionDef};
use crate::{Activation, Operation, OperationId, OperationOutcome};

/// metadata를 가진 대상.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetadataTarget {
    Workspace(WorkspaceId),
    Surface(SurfaceId),
}

/// 새 surface의 kind와 자료 참조.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceSpec {
    pub id: SurfaceId,
    pub kind: String,
    pub data: Option<DataRef>,
}

/// 분할로 만들 새 노드의 위치.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SplitSpec {
    #[serde(with = "SplitDirectionDef")]
    pub direction: SplitDirection,
    pub ratio: Ratio,
    pub placement: Placement,
}

/// 구조 이벤트. serde 태그는 codec의 type tag와 같다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DomainEvent {
    #[serde(rename = "structure.replaced")]
    StructureReplaced {
        replacement: crate::Replacement,
        removed: Vec<crate::EntityId>,
    },
    #[serde(rename = "undo.added")]
    UndoRecordAdded { record: crate::UndoRecord },
    #[serde(rename = "undo.consumed")]
    UndoRecordConsumed { id: OperationId },
    #[serde(rename = "undo.evicted")]
    UndoRecordEvicted { id: OperationId },
    #[serde(rename = "engine.incarnation_started")]
    EngineIncarnationStarted { previous: u64, current: u64 },
    #[serde(rename = "engine.retired")]
    EngineRetired { incarnation: u64 },
    #[serde(rename = "category.created")]
    CategoryCreated {
        id: WorkspaceCategoryId,
        name: String,
        index: usize,
    },
    #[serde(rename = "category.renamed")]
    CategoryRenamed {
        id: WorkspaceCategoryId,
        name: String,
    },
    #[serde(rename = "category.moved")]
    CategoryMoved {
        id: WorkspaceCategoryId,
        index: usize,
    },
    /// 소속 workspace가 없는 카테고리만 닫을 수 있다.
    #[serde(rename = "category.closed")]
    CategoryClosed { id: WorkspaceCategoryId },

    /// 첫 pane과 함께 만든다. tab은 뒤따르는 이벤트가 만든다.
    #[serde(rename = "workspace.created")]
    WorkspaceCreated {
        id: WorkspaceId,
        name: String,
        category: WorkspaceCategoryId,
        index: usize,
        pane: PaneId,
    },
    #[serde(rename = "workspace.renamed")]
    WorkspaceRenamed { id: WorkspaceId, name: String },
    /// 부제와 설명을 함께 정한다. 빈 문자열은 값이 없다는 뜻이다.
    #[serde(rename = "workspace.details_set")]
    WorkspaceDetailsSet {
        id: WorkspaceId,
        subtitle: String,
        description: String,
    },
    /// 없어진 workspace attach 매핑의 옛 기록. 옛 journal을 읽을 수 있게 남기며 값은 버린다.
    #[serde(rename = "workspace.attach_mapping_set")]
    WorkspaceAttachMappingSet {
        id: WorkspaceId,
        #[serde(default)]
        mapping: serde_json::Value,
    },
    /// 저장된 원격 워크스페이스 행을 만들거나 그 값 전체를 바꾼다.
    #[serde(rename = "remote_row.set")]
    RemoteRowSet {
        id: WorkspaceId,
        row: crate::model::RemoteRow,
    },
    /// 저장된 원격 워크스페이스 행을 지운다. 원격 workspace는 건드리지 않는다.
    #[serde(rename = "remote_row.removed")]
    RemoteRowRemoved { id: WorkspaceId },
    /// 순서와 카테고리 소속을 함께 바꾼다.
    #[serde(rename = "workspace.moved")]
    WorkspaceMoved {
        id: WorkspaceId,
        category: WorkspaceCategoryId,
        index: usize,
    },
    /// 소속 pane·tab·surface를 함께 닫는다.
    #[serde(rename = "workspace.closed")]
    WorkspaceClosed { id: WorkspaceId },

    #[serde(rename = "pane.split")]
    PaneSplit {
        target: PaneId,
        pane: PaneId,
        split: SplitSpec,
    },
    /// pane을 원래 위치에서 빼서 `target` 옆에 다시 붙인다. 다른 workspace로도 옮길 수 있다.
    #[serde(rename = "pane.moved")]
    PaneMoved {
        id: PaneId,
        target: PaneId,
        split: SplitSpec,
    },
    /// 소속 tab·surface를 함께 닫는다. workspace의 마지막 pane은 닫을 수 없다.
    #[serde(rename = "pane.closed")]
    PaneClosed { id: PaneId },

    #[serde(rename = "tab.created")]
    TabCreated {
        id: TabId,
        pane: PaneId,
        index: usize,
        name: String,
        surface: SurfaceSpec,
    },
    #[serde(rename = "tab.renamed")]
    TabRenamed { id: TabId, name: String },
    /// `None`이면 사용자 지정 이름을 지운다.
    #[serde(rename = "tab.explicit_name_set")]
    TabExplicitNameSet { id: TabId, name: Option<String> },
    #[serde(rename = "tab.moved")]
    TabMoved {
        id: TabId,
        pane: PaneId,
        index: usize,
    },
    #[serde(rename = "tab.closed")]
    TabClosed { id: TabId },

    #[serde(rename = "surface.split")]
    SurfaceSplit {
        target: SurfaceId,
        surface: SurfaceSpec,
        split: SplitSpec,
    },
    /// surface를 빼서 `target` 옆에 다시 붙인다. 다른 tab으로도 옮길 수 있다.
    #[serde(rename = "surface.moved")]
    SurfaceMoved {
        id: SurfaceId,
        target: SurfaceId,
        split: SplitSpec,
    },
    /// tab의 마지막 surface는 닫을 수 없다. 그 경우 tab을 닫는다.
    #[serde(rename = "surface.closed")]
    SurfaceClosed { id: SurfaceId },

    /// The preparation has succeeded; the same surface identity now has this kind and data.
    #[serde(rename = "surface.converted")]
    SurfaceConverted {
        id: SurfaceId,
        kind: String,
        data: Option<DataRef>,
    },
    #[serde(rename = "surface.creation_seeded")]
    SurfaceCreationSeeded {
        id: SurfaceId,
        activation_generation: u64,
        input: DataRef,
    },
    /// Import a creation recipe without copying the source runtime activation or effect.
    #[serde(rename = "surface.seed_imported")]
    SurfaceSeedImported { id: SurfaceId, input: DataRef },
    #[serde(rename = "surface.data_recorded")]
    SurfaceDataRecorded {
        id: SurfaceId,
        activation_generation: Option<u64>,
        content_generation: u64,
        snapshot_schema: u32,
        data: DataRef,
    },
    #[serde(rename = "surface.activation_changed")]
    SurfaceActivationChanged {
        id: SurfaceId,
        previous_generation: Option<u64>,
        activation: Activation,
    },
    /// A path within the committed layout, false for first and true for second.
    #[serde(rename = "pane.ratio_set")]
    PaneRatioSet {
        workspace: WorkspaceId,
        path: Vec<bool>,
        ratio: Ratio,
    },
    #[serde(rename = "surface.ratio_set")]
    SurfaceRatioSet {
        tab: TabId,
        path: Vec<bool>,
        ratio: Ratio,
    },

    #[serde(rename = "operation.prepared")]
    OperationPrepared { operation: Box<Operation> },
    #[serde(rename = "operation.resource_prepared")]
    OperationResourcePrepared {
        id: OperationId,
        data: Option<DataRef>,
        deferred: bool,
    },
    #[serde(rename = "operation.awaiting_cleanup")]
    OperationAwaitingCleanup {
        id: OperationId,
        outcome: OperationOutcome,
        cleanup: crate::CleanupPlan,
        prepared_data: Option<DataRef>,
        #[serde(default)]
        deferred: bool,
    },
    #[serde(rename = "operation.finished")]
    OperationFinished {
        id: OperationId,
        outcome: OperationOutcome,
    },
    #[serde(rename = "operation.recovery_observed")]
    OperationRecoveryObserved { id: OperationId, evidence: DataRef },
    #[serde(rename = "operation.reconciled")]
    OperationReconciled {
        id: OperationId,
        outcome: OperationOutcome,
        evidence: DataRef,
    },

    #[serde(rename = "metadata.set")]
    MetadataSet {
        target: MetadataTarget,
        key: String,
        value: String,
    },
    #[serde(rename = "metadata.removed")]
    MetadataRemoved { target: MetadataTarget, key: String },
}

impl DomainEvent {
    /// Immutable content referenced by this fact. The storage adapter pins these in its commit.
    pub fn data_refs(&self) -> Vec<DataRef> {
        match self {
            Self::UndoRecordAdded { record } => record.capture.data_refs().collect(),
            Self::TabCreated { surface, .. } | Self::SurfaceSplit { surface, .. } => {
                surface.data.into_iter().collect()
            }
            Self::SurfaceConverted { data, .. } => data.iter().copied().collect(),
            Self::SurfaceDataRecorded { data, .. } => vec![*data],
            Self::SurfaceCreationSeeded { input, .. } | Self::SurfaceSeedImported { input, .. } => {
                vec![*input]
            }
            Self::OperationPrepared { operation } => operation
                .assembly
                .iter()
                .flat_map(|plan| plan.data_refs())
                .chain(std::iter::once(operation.input))
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
                .collect(),
            Self::OperationResourcePrepared { data, .. } => data.iter().copied().collect(),
            Self::OperationAwaitingCleanup { prepared_data, .. } => {
                prepared_data.iter().copied().collect()
            }
            Self::OperationRecoveryObserved { evidence, .. }
            | Self::OperationReconciled { evidence, .. } => vec![*evidence],
            _ => Vec::new(),
        }
    }

    /// 이 빌드가 아는 모든 type tag. codec은 이 밖의 tag를 거절한다.
    pub const TAGS: &'static [&'static str] = &[
        "structure.replaced",
        "undo.added",
        "undo.consumed",
        "undo.evicted",
        "engine.incarnation_started",
        "engine.retired",
        "category.created",
        "category.renamed",
        "category.moved",
        "category.closed",
        "workspace.created",
        "workspace.renamed",
        "workspace.details_set",
        "workspace.attach_mapping_set",
        "remote_row.set",
        "remote_row.removed",
        "workspace.moved",
        "workspace.closed",
        "pane.split",
        "pane.moved",
        "pane.closed",
        "tab.created",
        "tab.renamed",
        "tab.explicit_name_set",
        "tab.moved",
        "tab.closed",
        "surface.split",
        "surface.moved",
        "surface.closed",
        "surface.converted",
        "surface.data_recorded",
        "surface.creation_seeded",
        "surface.seed_imported",
        "surface.activation_changed",
        "pane.ratio_set",
        "surface.ratio_set",
        "operation.prepared",
        "operation.resource_prepared",
        "operation.awaiting_cleanup",
        "operation.finished",
        "operation.recovery_observed",
        "operation.reconciled",
        "metadata.set",
        "metadata.removed",
    ];

    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::StructureReplaced { .. } => "structure.replaced",
            Self::UndoRecordAdded { .. } => "undo.added",
            Self::UndoRecordConsumed { .. } => "undo.consumed",
            Self::UndoRecordEvicted { .. } => "undo.evicted",
            Self::EngineIncarnationStarted { .. } => "engine.incarnation_started",
            Self::EngineRetired { .. } => "engine.retired",
            Self::CategoryCreated { .. } => "category.created",
            Self::CategoryRenamed { .. } => "category.renamed",
            Self::CategoryMoved { .. } => "category.moved",
            Self::CategoryClosed { .. } => "category.closed",
            Self::WorkspaceCreated { .. } => "workspace.created",
            Self::WorkspaceRenamed { .. } => "workspace.renamed",
            Self::WorkspaceDetailsSet { .. } => "workspace.details_set",
            Self::WorkspaceAttachMappingSet { .. } => "workspace.attach_mapping_set",
            Self::RemoteRowSet { .. } => "remote_row.set",
            Self::RemoteRowRemoved { .. } => "remote_row.removed",
            Self::WorkspaceMoved { .. } => "workspace.moved",
            Self::WorkspaceClosed { .. } => "workspace.closed",
            Self::PaneSplit { .. } => "pane.split",
            Self::PaneMoved { .. } => "pane.moved",
            Self::PaneClosed { .. } => "pane.closed",
            Self::TabCreated { .. } => "tab.created",
            Self::TabRenamed { .. } => "tab.renamed",
            Self::TabExplicitNameSet { .. } => "tab.explicit_name_set",
            Self::TabMoved { .. } => "tab.moved",
            Self::TabClosed { .. } => "tab.closed",
            Self::SurfaceSplit { .. } => "surface.split",
            Self::SurfaceMoved { .. } => "surface.moved",
            Self::SurfaceClosed { .. } => "surface.closed",
            Self::SurfaceConverted { .. } => "surface.converted",
            Self::SurfaceDataRecorded { .. } => "surface.data_recorded",
            Self::SurfaceCreationSeeded { .. } => "surface.creation_seeded",
            Self::SurfaceSeedImported { .. } => "surface.seed_imported",
            Self::SurfaceActivationChanged { .. } => "surface.activation_changed",
            Self::PaneRatioSet { .. } => "pane.ratio_set",
            Self::SurfaceRatioSet { .. } => "surface.ratio_set",
            Self::OperationPrepared { .. } => "operation.prepared",
            Self::OperationResourcePrepared { .. } => "operation.resource_prepared",
            Self::OperationAwaitingCleanup { .. } => "operation.awaiting_cleanup",
            Self::OperationFinished { .. } => "operation.finished",
            Self::OperationRecoveryObserved { .. } => "operation.recovery_observed",
            Self::OperationReconciled { .. } => "operation.reconciled",
            Self::MetadataSet { .. } => "metadata.set",
            Self::MetadataRemoved { .. } => "metadata.removed",
        }
    }
}

/// 엔진 구조 stream 하나에 확정된 이벤트와 그 stream의 revision.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedEvent {
    pub revision: Revision,
    pub event: DomainEvent,
}

/// 확정 batch에서 엔진 구조 stream 하나의 도메인 입력. 저장 batch에서 그 stream 이벤트만 해석해 순서대로 담는다.
/// 구조 이벤트가 없는 batch도 적용 위치를 옮기기 위해 빈 목록으로 전달한다.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainBatch {
    pub batch_id: BatchId,
    pub events: Vec<RecordedEvent>,
}
