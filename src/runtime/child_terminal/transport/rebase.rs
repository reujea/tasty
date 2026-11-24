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
