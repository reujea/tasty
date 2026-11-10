//! Attach waits for selected terminal activations while retaining its original holder/registration.
use crate::app::journal::{ActivationOutcome, ActivationReceipt, JournalApplication};
use crate::runtime::engine_access::EngineMut;
use crate::runtime::engine_session::{EngineId, EngineSession};
use tasty_ipc::stream_hub::StreamHub;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Surface(u32),
    Workspace(u32),
}
struct Leaf {
    id: u32,
    activation: Option<u64>,
    receipt: Option<ActivationReceipt>,
    physical: Option<tasty_terminal::ResourceGeneration>,
}
pub(crate) struct PendingAttach {
    cancelled: bool,
    engine: EngineId,
    target: Target,
    client: u32,
    binding: std::sync::Weak<()>,
    grant: u64,
    leaves: Vec<Leaf>,
    members: Vec<u32>,
}
impl PendingAttach {
    pub(crate) fn client(&self) -> u32 {
        self.client
    }
    pub(crate) fn cancel(&mut self) {
        self.cancelled = true;
    }
    fn weight(&self) -> usize {
        self.members.capacity() * std::mem::size_of::<u32>()
            + self.leaves.capacity() * std::mem::size_of::<Leaf>()
            + std::mem::size_of::<Self>()
    }
    fn lease_is_current(&self, engine: &EngineMut<'_>) -> bool {
        match self.target {
            Target::Surface(id) => engine.live.occupancy.occupancy_of(id).is_some_and(|lock| {
                lock.holder == crate::core::attach::Holder::StreamClient(self.client)
                    && lock.granted_seq == self.grant
            }),
            Target::Workspace(id) => {
                engine
                    .live
                    .occupancy
                    .workspaces_snapshot()
                    .into_iter()
                    .any(|(workspace, lock)| {
                        workspace == id
                            && lock.holder == self.client
                            && lock.granted_seq == self.grant
                    })
            }
        }
    }
    fn release(&self, engine: &mut EngineMut<'_>) {
        if !self.lease_is_current(engine) {
            return;
        }
        match self.target {
            Target::Surface(id) => {
                if let Err(error) = engine.live.occupancy.release(id, self.client) {
                    tracing::warn!(
                        ?error,
                        surface = id,
                        "current attach activation release failed"
                    );
                }
            }
            Target::Workspace(id) => {
                engine.live.occupancy.force_detach_workspace(id);
            }
        }
    }
}

/// mirror를 다시 mirror하는 체인은 지원하지 않는다. 접수 단계에서 거절해 점유를 잡지 않는다.
pub(crate) const MIRROR_TARGET_REASON: &str = "mirror_workspace";

fn targets_mirror(engine: &EngineMut<'_>, target: Target) -> bool {
    match target {
        Target::Surface(surface) => engine.is_mirror_surface(surface),
        Target::Workspace(workspace) => engine
            .find_workspace_index_for_id(workspace)
            .and_then(|index| engine.workspace_at(index))
            .is_some_and(|workspace| workspace.mirror),
    }
}

pub(crate) fn begin(
    pending: &mut Vec<PendingAttach>,
    journal: &JournalApplication,
    id: EngineId,
    engine: &mut EngineMut<'_>,
    target: Target,
    client: u32,
    hub: &StreamHub,
) {
    let Some(binding) = hub.client_binding(client) else {
        return;
    };
    if pending.iter().any(|old| {
        !old.cancelled
            && old.client == client
            && old.binding.ptr_eq(&binding)
            && old.target == target
    }) {
        return;
    }
    let mut old = std::mem::take(pending);
    for mut item in old.drain(..) {
        if item.client == client {
            if item.engine == id {
                item.release(engine);
            } else {
                item.cancelled = true;
                pending.push(item);
            }
        } else {
            pending.push(item);
        }
    }
    journal.wake_application();
    if pending.len() >= 64 {
        crate::remote::server::reject_attach(hub, client, "busy", None);
        return;
    }
    if targets_mirror(engine, target) {
        crate::remote::server::reject_attach(hub, client, MIRROR_TARGET_REASON, None);
        return;
    }
    let (members, terminals) = match target {
        Target::Surface(surface) => {
            let terminal = engine.runtime.terminals.contains(surface)
                || engine
                    .runtime
                    .surfaces
                    .get(&surface)
                    .and_then(|value| {
                        value
                            .as_any()
                            .downcast_ref::<crate::runtime::surface_restorer::JournalPlaceholder>()
                    })
                    .is_some_and(|value| value.kind == "terminal");
            if !terminal {
                engine.attach_surface_for_stream(surface, client, hub);
                return;
            }
            (vec![surface], vec![surface])
        }
        Target::Workspace(workspace) => {
            let Some(index) = engine.find_workspace_index_for_id(workspace) else {
                crate::remote::server::reject_attach(hub, client, "workspace_not_found", None);
                return;
            };
            (
                engine
                    .workspace_at(index)
                    .expect("found workspace")
                    .all_surface_ids(),
                engine.classify_attach_surfaces(workspace).terminals,
            )
        }
    };
    let weight = members
        .len()
        .saturating_mul(std::mem::size_of::<u32>())
        .saturating_add(terminals.len().saturating_mul(std::mem::size_of::<Leaf>()));
    if pending
        .iter()
        .map(PendingAttach::weight)
        .sum::<usize>()
        .saturating_add(weight)
        > tasty_ipc::admission::QUEUED_BYTES_LIMIT
    {
        crate::remote::server::reject_attach(hub, client, "busy", None);
        return;
    }
    let lock = match target {
        Target::Surface(surface) => engine.live.occupancy.acquire(surface, client),
        Target::Workspace(workspace) => engine
            .live
            .occupancy
            .acquire_workspace(workspace, &terminals, &members, client),
    };
    let grant = match lock {
        Ok(lock) => lock.granted_seq,
        Err(crate::core::attach::AttachError::AlreadyAttached { holder }) => {
            crate::remote::server::reject_attach(hub, client, "already_attached", Some(holder));
            return;
        }
        Err(_) => {
            crate::remote::server::reject_attach(hub, client, "lock_error", None);
            return;
        }
    };
    engine
        .live
        .occupancy
        .set_attachment_ready(client, grant, false);
    let leaves = terminals
        .into_iter()
        .map(|surface| Leaf {
            id: surface,
            activation: engine
                .core
                .find_surface_by_id(surface)
                .and_then(|surface| surface.activation_generation),
            receipt: None,
            physical: engine.runtime.terminals.generation(surface),
        })
        .collect();
    pending.push(PendingAttach {
        cancelled: false,
        engine: id,
        target,
        client,
        binding,
        grant,
        leaves,
        members,
    });
    journal.wake_application();
}

// Poll the original activation receipt without marking the workspace ready prematurely.
fn poll_leaf_activation(
    leaf: &mut Leaf,
    journal: &mut JournalApplication,
    session: &mut EngineSession,
) -> Result<(), &'static str> {
    let current = session
        .core_state
        .find_surface_by_id(leaf.id)
        .and_then(|surface| surface.activation_generation);
    if let Some(generation) = leaf.physical {
        if current != leaf.activation
            || !session
                .runtime
                .terminals
                .matches_generation(leaf.id, generation)
        {
            return Err("target_changed");
        }
        return Ok(());
    }
    if let Some(receipt) = &leaf.receipt {
        match receipt.get() {
            Some(ActivationOutcome::Ready {
                activation,
                physical: Some(generation),
            }) if current == *activation
                && session
                    .runtime
                    .terminals
                    .matches_generation(leaf.id, *generation) =>
            {
                leaf.activation = *activation;
                leaf.physical = Some(*generation);
            }
            Some(_) => return Err("spawn_failed"),
            None => {}
        }
    } else if current != leaf.activation {
        return Err("target_changed");
    } else {
        match journal.request_restore_receipt(session, leaf.id, leaf.activation) {
            Ok(receipt) => leaf.receipt = receipt,
            Err(error) => {
                tracing::warn!("attach activation failed: {error}");
                return Err("spawn_failed");
            }
        }
    }
    Ok(())
}

pub(crate) fn poll(
    pending: &mut Vec<PendingAttach>,
    journal: &mut JournalApplication,
    sessions: &mut [&mut EngineSession],
    hub: &StreamHub,
) {
    let mut owned = std::mem::take(pending);
    for mut request in owned.drain(..) {
        let Some(session) = sessions
            .iter_mut()
            .find(|session| session.id == request.engine)
        else {
            if !request.cancelled && hub.matches_client_binding(request.client, &request.binding) {
                crate::remote::server::reject_attach(hub, request.client, "not_found", None);
            }
            continue;
        };
        if request.cancelled || journal.is_halted() {
            request.release(&mut session.borrow_mut());
            continue;
        }
        if !hub.matches_client_binding(request.client, &request.binding)
            || !request.lease_is_current(&session.borrow_mut())
        {
            request.release(&mut session.borrow_mut());
            continue;
        }
        let mut failure = request
            .leaves
            .iter_mut()
            .find_map(|leaf| poll_leaf_activation(leaf, journal, session).err());
        if let Target::Workspace(workspace) = request.target {
            let current = session
                .core_state
                .find_workspace_index_for_id(workspace)
                .and_then(|index| session.core_state.workspace_at(index))
                .map(|workspace| workspace.all_surface_ids());
            if current.as_ref().is_none_or(|current| {
                current.len() != request.members.len()
                    || current.iter().any(|id| !request.members.contains(id))
            }) {
                failure = Some("target_changed");
            }
        }
        if let Some(reason) = failure {
            request.release(&mut session.borrow_mut());
            crate::remote::server::reject_attach(hub, request.client, reason, None);
            continue;
        }
        if request.leaves.iter().any(|leaf| leaf.physical.is_none()) {
            pending.push(request);
            continue;
        }
        session
            .live
            .occupancy
            .set_attachment_ready(request.client, request.grant, true);
        match request.target {
            Target::Surface(surface) => {
                session
                    .borrow_mut()
                    .attach_surface_for_stream(surface, request.client, hub)
            }
            Target::Workspace(workspace) => {
                if !session
                    .borrow_mut()
                    .attach_workspace_for_stream(workspace, request.client, hub)
                {
                    request.release(&mut session.borrow_mut());
                    crate::remote::server::reject_attach(
                        hub,
                        request.client,
                        "publish_failed",
                        None,
                    );
                    continue;
                }
                // Initial descriptor/taps subsume activation deltas held behind readiness.
                session
                    .remote
                    .pending_workspace_taps
                    .retain(|_, entry| entry.0 != workspace);
                session.remote.clear_structure_changed(workspace);
            }
        }
    }
}

pub(crate) fn cancel_engine(
    pending: &mut Vec<PendingAttach>,
    id: EngineId,
    engine: &mut EngineMut<'_>,
    hub: &StreamHub,
) {
    let mut owned = std::mem::take(pending);
    for request in owned.drain(..) {
        if request.engine != id {
            pending.push(request);
            continue;
        }
        request.release(engine);
        if !request.cancelled && hub.matches_client_binding(request.client, &request.binding) {
            crate::remote::server::reject_attach(hub, request.client, "engine_closed", None);
        }
    }
}

#[cfg(test)]
mod order_tests;
