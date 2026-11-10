//! Explicit resume keeps identity; a fresh start advances incarnation without erasing old duties.
use super::*;

pub(super) fn decide(
    model: &JournalModel,
    command: &StructuralCommand,
) -> Result<StructuralDecision, Rejection> {
    if let StructuralCommand::RetireEngine {
        expected_incarnation,
    } = command
    {
        if model.engine_incarnation != *expected_incarnation || *expected_incarnation == 0 {
            return Err(Rejection(
                "cannot retire a different engine incarnation".into(),
            ));
        }
        return Ok(StructuralDecision {
            events: if model.engine_retired {
                Vec::new()
            } else {
                vec![DomainEvent::EngineRetired {
                    incarnation: *expected_incarnation,
                }]
            },
            effects: Vec::new(),
            completed_command: None,
            result: StructuralResult::Updated,
        });
    }
    let StructuralCommand::OpenEngine {
        expected_incarnation,
        reset_structure,
        normal_category_name,
    } = command
    else {
        unreachable!("bootstrap command dispatch")
    };
    if model.engine_incarnation != *expected_incarnation {
        return Err(Rejection("engine binding changed before bootstrap".into()));
    }
    let reset_structure = *reset_structure || model.engine_retired;
    let mut events = Vec::new();
    let incarnation = if reset_structure || model.engine_incarnation == 0 {
        let current = model
            .engine_incarnation
            .checked_add(1)
            .ok_or_else(|| Rejection("engine incarnation exhausted".into()))?;
        events.push(DomainEvent::EngineIncarnationStarted {
            previous: model.engine_incarnation,
            current,
        });
        current
    } else {
        model.engine_incarnation
    };
    if reset_structure {
        events.extend(
            model
                .undo_records
                .iter()
                .map(|record| DomainEvent::UndoRecordEvicted {
                    id: record.id.clone(),
                }),
        );
        // 저장된 원격 행도 이 incarnation 의 구조다. 슬롯을 새로 시작하면 함께 지운다.
        events.extend(
            model
                .remote_rows
                .keys()
                .map(|id| DomainEvent::RemoteRowRemoved { id: *id }),
        );
        events.extend(
            model
                .workspace_order
                .iter()
                .map(|id| DomainEvent::WorkspaceClosed { id: *id }),
        );
        events.extend(
            model
                .category_order
                .iter()
                .filter(|id| **id != 0)
                .map(|id| DomainEvent::CategoryClosed { id: *id }),
        );
    }
    if !model.categories.contains_key(&0) {
        events.push(DomainEvent::CategoryCreated {
            id: 0,
            name: normal_category_name.clone(),
            index: 0,
        });
    }
    Ok(StructuralDecision {
        events,
        effects: Vec::new(),
        completed_command: None,
        result: StructuralResult::EngineOpened { incarnation },
    })
}
