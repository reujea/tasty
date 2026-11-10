//! 시험 공용 도우미와 모든 이벤트 종류를 거치는 시나리오.

use tasty_model::SplitDirection;

use crate::{
    DataRef, DomainBatch, DomainEvent, MetadataTarget, Placement, Ratio, RecordedEvent, SplitSpec,
    SurfaceSpec,
};

pub fn split(direction: SplitDirection, ratio: f32, placement: Placement) -> SplitSpec {
    SplitSpec {
        direction,
        ratio: Ratio::from_f32(ratio),
        placement,
    }
}

pub fn surface(id: u32, kind: &str) -> SurfaceSpec {
    SurfaceSpec {
        id,
        kind: kind.to_owned(),
        data: None,
    }
}

/// revision이 `after` 다음부터 이어지는 도메인 batch.
pub fn batch(batch_id: u64, after: u64, events: &[DomainEvent]) -> DomainBatch {
    DomainBatch {
        batch_id,
        events: events
            .iter()
            .enumerate()
            .map(|(i, event)| RecordedEvent {
                revision: after + i as u64 + 1,
                event: event.clone(),
            })
            .collect(),
    }
}

/// 시나리오를 batch 목록으로 만든다. batch 번호는 1부터다.
pub fn scenario_batches() -> Vec<DomainBatch> {
    let mut after = 0;
    scenario()
        .iter()
        .enumerate()
        .map(|(i, events)| {
            let b = batch(i as u64 + 1, after, events);
            after += events.len() as u64;
            b
        })
        .collect()
}

/// 모든 이벤트 종류를 한 번 이상 거치는 batch 목록.
pub fn scenario() -> Vec<Vec<DomainEvent>> {
    let ws = 1;
    let ws2 = 2;
    vec![
        vec![
            DomainEvent::CategoryCreated {
                id: 0,
                name: "normal".to_owned(),
                index: 0,
            },
            DomainEvent::CategoryCreated {
                id: 1,
                name: "work".to_owned(),
                index: 1,
            },
            DomainEvent::WorkspaceCreated {
                id: ws,
                name: "main".to_owned(),
                category: 0,
                index: 0,
                pane: 1,
            },
            DomainEvent::TabCreated {
                id: 1,
                pane: 1,
                index: 0,
                name: "shell".to_owned(),
                surface: surface(1, "terminal"),
            },
        ],
        vec![
            DomainEvent::PaneSplit {
                target: 1,
                pane: 2,
                split: split(SplitDirection::Vertical, 0.3, Placement::After),
            },
            DomainEvent::TabCreated {
                id: 2,
                pane: 2,
                index: 0,
                name: "notes".to_owned(),
                surface: SurfaceSpec {
                    id: 2,
                    kind: "markdown".to_owned(),
                    data: Some(DataRef(1)),
                },
            },
            DomainEvent::SurfaceSplit {
                target: 1,
                surface: surface(3, "terminal"),
                split: split(SplitDirection::Horizontal, 0.1 + 0.2, Placement::Before),
            },
        ],
        vec![
            DomainEvent::WorkspaceCreated {
                id: ws2,
                name: "side".to_owned(),
                category: 1,
                index: 1,
                pane: 3,
            },
            DomainEvent::TabCreated {
                id: 3,
                pane: 3,
                index: 0,
                name: "logs".to_owned(),
                surface: surface(4, "terminal"),
            },
            DomainEvent::MetadataSet {
                target: MetadataTarget::Surface(1),
                key: "role".to_owned(),
                value: "orchestrator".to_owned(),
            },
            DomainEvent::MetadataSet {
                target: MetadataTarget::Workspace(ws),
                key: "project".to_owned(),
                value: "tasty".to_owned(),
            },
        ],
        vec![
            DomainEvent::CategoryRenamed {
                id: 1,
                name: "jobs".to_owned(),
            },
            DomainEvent::WorkspaceRenamed {
                id: ws2,
                name: "logs".to_owned(),
            },
            DomainEvent::TabRenamed {
                id: 1,
                name: "build".to_owned(),
            },
            DomainEvent::WorkspaceMoved {
                id: ws2,
                category: 0,
                index: 0,
            },
            DomainEvent::CategoryMoved { id: 1, index: 0 },
            DomainEvent::WorkspaceDetailsSet {
                id: ws,
                subtitle: "sub".to_owned(),
                description: "desc".to_owned(),
            },
            DomainEvent::WorkspaceAttachMappingSet {
                id: ws,
                mapping: serde_json::json!({"target": {"Profile": {"name": "box"}}, "remote_workspace": 4}),
            },
            DomainEvent::RemoteRowSet {
                id: 900,
                row: crate::RemoteRow {
                    name: "box".to_owned(),
                    subtitle: String::new(),
                    description: String::new(),
                    category: 0,
                    index: 1,
                    binding: tasty_model::RemoteBinding {
                        profile: "box".to_owned(),
                        remote_workspace: 4,
                    },
                },
            },
            DomainEvent::TabExplicitNameSet {
                id: 1,
                name: Some("Build!".to_owned()),
            },
        ],
        vec![
            DomainEvent::SurfaceMoved {
                id: 3,
                target: 2,
                split: split(SplitDirection::Vertical, 0.5, Placement::After),
            },
            DomainEvent::TabMoved {
                id: 3,
                pane: 1,
                index: 1,
            },
            DomainEvent::PaneMoved {
                id: 2,
                target: 3,
                split: split(SplitDirection::Horizontal, 0.75, Placement::Before),
            },
            DomainEvent::MetadataRemoved {
                target: MetadataTarget::Workspace(ws),
                key: "project".to_owned(),
            },
            DomainEvent::WorkspaceAttachMappingSet {
                id: ws,
                mapping: serde_json::Value::Null,
            },
            DomainEvent::RemoteRowRemoved { id: 900 },
        ],
        vec![
            DomainEvent::SurfaceClosed { id: 3 },
            DomainEvent::TabClosed { id: 3 },
            DomainEvent::PaneClosed { id: 3 },
            DomainEvent::CategoryClosed { id: 1 },
        ],
        vec![DomainEvent::WorkspaceClosed { id: ws2 }],
    ]
}
