//! Local intent versions are bookkeeping, never persisted payload or history snapshots.
use super::*;
#[derive(Clone, Default, serde::Serialize)]
pub(super) struct StateIntents {
    idle: Vec<(u32, (u64, bool))>,
    needs: Vec<(u32, (u64, bool))>,
}
impl StateIntents {
    pub(super) fn between(before: &RegistryData, after: &RegistryData) -> Self {
        Self {
            idle: after
                .idle_intents
                .iter()
                .filter(|(id, value)| before.idle_intents.get(id) != Some(value))
                .map(|(id, value)| (*id, *value))
                .collect(),
            needs: after
                .needs_input_intents
                .iter()
                .filter(|(id, value)| before.needs_input_intents.get(id) != Some(value))
                .map(|(id, value)| (*id, *value))
                .collect(),
        }
    }
    pub(super) fn apply(&self, value: &mut RegistryData) {
        for (child, intent) in &self.idle {
            value.idle_intents.insert(*child, *intent);
        }
        for (child, intent) in &self.needs {
            value.needs_input_intents.insert(*child, *intent);
        }
    }
}
pub(super) fn complete(
    client: &mut Client,
    intent: Intent,
    outcome: Result<persistence::Outcome, String>,
) -> ResultValue {
    let unsent = Delta::between(&client.accepted, &client.visible);
    let unsent_states = StateIntents::between(&client.accepted, &client.visible);
    let old_visible = client.visible.clone();
    let succeeded = outcome.is_ok();
    let result = outcome.map(|outcome| {
        let mut committed = outcome.registry;
        committed.baseline = None;
        committed.idle_intents = client.confirmed.idle_intents.clone();
        committed.needs_input_intents = client.confirmed.needs_input_intents.clone();
        intent.states.apply(&mut committed);
        retain_present_states(&mut committed);
        client.confirmed = committed;
        outcome.index
    });
    let mut accepted = client.confirmed.clone();
    for later in &client.intents {
        later.delta.apply(&mut accepted);
        later.states.apply(&mut accepted);
        project(&mut accepted, &later.operation);
    }
    retain_present_states(&mut accepted);
    accepted.owned.extend(old_visible.owned.iter().copied());
    accepted.slot = old_visible.slot;
    accepted.intent_serial = old_visible.intent_serial;
    client.accepted = accepted.clone();
    if succeeded {
        client.visible = accepted;
        unsent.apply(&mut client.visible);
        unsent_states.apply(&mut client.visible);
        retain_present_states(&mut client.visible);
    }
    client.visible.owned.extend(old_visible.owned);
    client.visible.slot = old_visible.slot;
    client.visible.intent_serial = old_visible.intent_serial;
    result
}

/// 실패한 nickname 대입을 조회 값에서 거둔다. 쓰기 실패 뒤에도 visible은 보존되므로(위 `complete`), 두면
/// 오류로 답한 값이 조회에 남고 다음 저장의 필드 차이에 섞여 파일에 쓰인다. 같은 자식에 대한 뒤 대입이
/// 대기 중이거나 조회 값이 그 사이 다른 값으로 바뀌었으면 그 값을 덮지 않는다. 되돌릴 값은 확정 값에
/// 뒤 명령을 얹은 `accepted`의 값이다.
pub(super) fn restore_failed_nickname(client: &mut Client, operation: &Operation) {
    let Operation::SetNickname {
        parent,
        index,
        surface,
        nickname,
    } = operation
    else {
        return;
    };
    let later = client.intents.iter().any(|later| {
        matches!(&later.operation, Operation::SetNickname { surface: next, .. } if next == surface)
    });
    if later {
        return;
    }
    let nickname_in = |value: &RegistryData| {
        value
            .list_children(parent)
            .iter()
            .find(|c| c.index == *index && c.child_surface_id == *surface)
            .map(|c| c.nickname.clone())
    };
    if nickname_in(&client.visible).as_ref() != Some(nickname) {
        return;
    }
    if let Some(restored) = nickname_in(&client.accepted) {
        client
            .visible
            .set_child_nickname(parent, *index, *surface, restored);
    }
}

fn retain_present_states(value: &mut RegistryData) {
    // Disk and local deletion deltas remove fields, not just their values. Retire only
    // those fields' bookkeeping after replaying this layer's explicit assignments.
    value
        .idle_intents
        .retain(|child, _| value.idle.contains_key(child));
    value
        .needs_input_intents
        .retain(|child, _| value.needs_input.contains_key(child));
}

pub(super) fn project(value: &mut RegistryData, operation: &Operation) {
    let cleanup = matches!(operation, Operation::Forget(_));
    #[cfg(feature = "gui")]
    let cleanup = cleanup
        || matches!(
            operation,
            Operation::ReleaseOwned(_) | Operation::RetirePending { .. }
        );
    if cleanup && let Err(error) = value.apply_operation(operation.clone()) {
        tracing::warn!(%error,"child cleanup projection failed");
    }
}
