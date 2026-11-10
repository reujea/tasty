//! codec: 모든 이벤트의 왕복, 모르는 tag·version 거절, 분할 비율 비트 보존, snapshot 왕복.

use std::collections::BTreeSet;

use super::common::{scenario, scenario_batches};
use crate::{
    CodecError, DomainEvent, EVENT_SCHEMA_VERSION, EncodedEvent, MODEL_VERSION, Ratio, StreamBatch,
    StructureModels, decode_event, decode_snapshot, encode_event, encode_snapshot, evolve_streams,
};

fn decode(payload: &EncodedEvent) -> Result<DomainEvent, CodecError> {
    decode_event(&payload.type_tag, payload.schema_version, &payload.bytes)
}

#[test]
fn every_event_round_trips_and_the_tag_list_is_complete() {
    let events: Vec<DomainEvent> = scenario()
        .into_iter()
        .flatten()
        .chain(super::lifecycle::examples())
        .chain([
            DomainEvent::SurfaceSeedImported {
                id: 10,
                input: crate::DataRef(110),
            },
            DomainEvent::OperationResourcePrepared {
                id: crate::OperationId("prepared".into()),
                data: Some(crate::DataRef(120)),
                deferred: true,
            },
            DomainEvent::OperationRecoveryObserved {
                id: crate::OperationId("unknown".into()),
                evidence: crate::DataRef(121),
            },
            DomainEvent::StructureReplaced {
                replacement: crate::Replacement {
                    source: crate::EntityId {
                        kind: crate::IdKind::Surface,
                        id: 10,
                    },
                    target: crate::EntityId {
                        kind: crate::IdKind::Surface,
                        id: 11,
                    },
                },
                removed: vec![],
            },
            DomainEvent::UndoRecordAdded {
                record: crate::UndoRecord {
                    id: crate::OperationId("closed".into()),
                    target: crate::CloseTarget::Surface(10),
                    capture: crate::UndoCapture {
                        snapshot: crate::DataRef(122),
                        retained: vec![crate::DataRef(123)],
                    },
                },
            },
            DomainEvent::UndoRecordConsumed {
                id: crate::OperationId("closed".into()),
            },
            DomainEvent::UndoRecordEvicted {
                id: crate::OperationId("expired".into()),
            },
        ])
        .collect();
    let mut seen = BTreeSet::new();
    for event in &events {
        let payload = encode_event(event).expect("encode");
        assert_eq!(payload.type_tag, event.type_tag());
        assert_eq!(payload.schema_version, EVENT_SCHEMA_VERSION);
        assert_eq!(&decode(&payload).expect("decode"), event);
        seen.insert(event.type_tag());
    }
    let known: BTreeSet<&str> = DomainEvent::TAGS.iter().copied().collect();
    assert_eq!(known.len(), DomainEvent::TAGS.len(), "duplicate tag");
    assert_eq!(seen, known, "scenario must cover every known tag");
}

#[test]
fn unknown_tag_is_rejected() {
    let payload = EncodedEvent {
        type_tag: "window.created".to_owned(),
        schema_version: EVENT_SCHEMA_VERSION,
        bytes: b"{}".to_vec(),
    };
    assert!(matches!(
        decode(&payload),
        Err(CodecError::UnknownTag(tag)) if tag == "window.created"
    ));
}

#[test]
fn unsupported_schema_version_is_rejected() {
    let event = DomainEvent::TabClosed { id: 1 };
    let mut payload = encode_event(&event).expect("encode");
    payload.schema_version = EVENT_SCHEMA_VERSION + 1;
    assert!(matches!(
        decode(&payload),
        Err(CodecError::UnsupportedVersion { version, .. }) if version == EVENT_SCHEMA_VERSION + 1
    ));
}

#[test]
fn body_that_disagrees_with_the_tag_is_rejected() {
    let event = DomainEvent::TabClosed { id: 1 };
    let mut payload = encode_event(&event).expect("encode");
    payload.type_tag = "pane.closed".to_owned();
    assert!(matches!(
        decode(&payload),
        Err(CodecError::TagMismatch {
            body: "tab.closed",
            ..
        })
    ));
    payload.bytes = b"not json".to_vec();
    assert!(matches!(decode(&payload), Err(CodecError::Body { .. })));
}

#[test]
fn split_ratio_keeps_its_bits() {
    let values = [
        0.1f32 + 0.2,
        f32::MIN_POSITIVE / 3.0,
        f32::from_bits(0x7fc0_1234),
        -0.0,
        1.0 / 3.0,
    ];
    for value in values {
        let event = DomainEvent::PaneSplit {
            target: 1,
            pane: 2,
            split: super::common::split(
                tasty_model::SplitDirection::Vertical,
                value,
                crate::Placement::After,
            ),
        };
        let decoded = decode(&encode_event(&event).expect("encode")).expect("decode");
        let DomainEvent::PaneSplit { split, .. } = decoded else {
            panic!("wrong variant");
        };
        assert_eq!(split.ratio.to_f32().to_bits(), value.to_bits());
        assert_eq!(split.ratio, Ratio::from_f32(value));
    }
}

#[test]
fn snapshot_round_trips_at_every_batch() {
    let mut models = StructureModels::default();
    for batch in scenario_batches() {
        let batch = StreamBatch {
            batch_id: batch.batch_id,
            streams: [("structure:slot-1".to_owned(), batch.events)].into(),
        };
        evolve_streams(&mut models, &batch).expect("evolve");
        let bytes = encode_snapshot(&models).expect("encode");
        let decoded = decode_snapshot(MODEL_VERSION, &bytes).expect("decode");
        assert_eq!(decoded, models);
        assert_eq!(encode_snapshot(&decoded).expect("re-encode"), bytes);
    }
}

#[test]
fn snapshot_with_another_model_version_is_rejected() {
    let bytes = encode_snapshot(&StructureModels::default()).expect("encode");
    assert!(matches!(
        decode_snapshot(MODEL_VERSION + 1, &bytes),
        Err(CodecError::UnsupportedModelVersion(v)) if v == MODEL_VERSION + 1
    ));
    assert!(matches!(
        decode_snapshot(MODEL_VERSION, b"[]"),
        Err(CodecError::Snapshot(_))
    ));
}

#[test]
fn an_old_attach_mapping_record_still_decodes_and_is_dropped() {
    // 매핑을 없애기 전 journal이 쓴 그대로의 본문이다.
    let old = br#"{"type":"workspace.attach_mapping_set","id":3,"mapping":{"target":{"Inline":{"host":"me@box"}},"remote_workspace":7}}"#;
    let event: DomainEvent = serde_json::from_slice(old).unwrap();
    assert!(matches!(
        event,
        DomainEvent::WorkspaceAttachMappingSet { id: 3, .. }
    ));
    let cleared = br#"{"type":"workspace.attach_mapping_set","id":3,"mapping":null}"#;
    assert!(serde_json::from_slice::<DomainEvent>(cleared).is_ok());
}
