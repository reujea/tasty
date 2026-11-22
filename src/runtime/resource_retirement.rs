//! Sole ownership of resources detached by a committed tombstone, before cleanup is claimed.
use crate::runtime::engine_session::EngineSession;
use crate::runtime::journal_product::{ClaimedRetirement, EffectLease};
use tasty_core::{Operation, RetirementPlan};
use tasty_terminal::{Pty, PtyPhase, PtyRetirement, Terminal};
struct RemovedOwner {
    surface: Box<dyn crate::model::Surface>,
    pair: Option<(Terminal, Option<Pty>)>,
    scrollback: Option<String>,
}
pub(crate) struct ResourceRetirement {
    plan: RetirementPlan,
    engine_incarnation: u64,
    runtime_epoch: u64,
    owners: Vec<RemovedOwner>,
    receipts: Vec<PtyRetirement>,
    remote_receipts: Vec<crate::plugin_bridge::host_cmd::RemoteRetirementReceipt>,
    started: Option<std::time::Instant>,
    lease: Option<EffectLease>,
    start_complete: bool,
    failure: Option<String>,
    metadata_complete: bool,
    /// 넣었지만 결과를 아직 거두지 않은 scope 정리.
    metadata_purge: Option<tasty_memory::writer::MemoryTicket>,
}
impl ResourceRetirement {
    /// Called only while publishing an already committed close, before the batch ACK.
    pub(crate) fn capture(
        session: &mut EngineSession,
        operation: &Operation,
    ) -> Result<Self, String> {
        let plan = operation
            .retirement
            .clone()
            .ok_or("retirement plan missing")?;
        let binding = session
            .journal_binding
            .as_ref()
            .ok_or("retirement engine is unbound")?;
        if binding.incarnation != operation.engine_incarnation {
            return Err("retirement belongs to another engine incarnation".into());
        }
        let runtime_epoch = binding.runtime_epoch;
        for target in &plan.surfaces {
            let descriptor = session
                .core_state
                .find_surface_by_id(target.id)
                .ok_or("retirement descriptor missing")?;
            if descriptor.kind != target.kind
                || descriptor.activation_generation != target.activation_generation
            {
                return Err("retirement descriptor differs from the committed old owner".into());
            }
            if !session.runtime.surfaces.contains_key(&target.id) {
                return Err("retirement runtime owner missing".into());
            }
        }
        let owners = plan
            .surfaces
            .iter()
            .map(|target| {
                let scrollback = session
                    .runtime
                    .terminals
                    .scrollback_persist_id(target.id)
                    .map(str::to_owned);
                RemovedOwner {
                    surface: session
                        .runtime
                        .surfaces
                        .remove(&target.id)
                        .expect("preflight retained the owner"),
                    pair: session.runtime.terminals.remove(target.id),
                    scrollback,
                }
            })
            .collect();
        Ok(Self {
            plan,
            engine_incarnation: operation.engine_incarnation,
            runtime_epoch,
            owners,
            receipts: Vec::new(),
            remote_receipts: Vec::new(),
            started: None,
            lease: None,
            start_complete: false,
            failure: None,
            metadata_complete: false,
            metadata_purge: None,
        })
    }
    pub(crate) fn start(
        &mut self,
        claim: ClaimedRetirement,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        if self.started.is_some()
            || claim.plan != self.plan
            || claim.engine_incarnation != self.engine_incarnation
            || claim.lease.runtime_epoch != self.runtime_epoch
        {
            return Err("cleanup claim does not own the detached resources".into());
        }
        self.lease = Some(claim.lease);
        self.started = Some(std::time::Instant::now());
        let result = self.start_owned(engine, plugins);
        self.start_complete = result.is_ok();
        self.failure = result.as_ref().err().cloned();
        result
    }
    fn start_owned(
        &mut self,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
        mut plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        if self.owners.iter().any(|owner| {
            owner
                .surface
                .as_any()
                .is::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                || owner
                    .surface
                    .as_any()
                    .is::<crate::runtime::egui_mesh_surface::EguiMeshSurface>()
        }) && plugins.is_none()
        {
            return Err("plugin host is absent for committed retirement".into());
        }
        // The original boxes and Pty handles are private to this operation. No ID lookup can
        // retarget destruction to a replacement installed later under the same logical identity.
        for owner in &self.owners {
            if let Some(remote) = owner
                .surface
                .as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>(
            ) && !self
                .remote_receipts
                .iter()
                .any(|receipt| receipt.matches(remote.id, &remote.handles().binding()))
            {
                self.remote_receipts.push(
                    plugins
                        .as_deref_mut()
                        .ok_or("plugin host disappeared")?
                        .enqueue_observed_remote_retirement(
                            remote.id,
                            remote.handles().binding(),
                        )?,
                );
            }
        }
        for owner in &self.owners {
            if let Some(mesh) = owner
                .surface
                .as_any()
                .downcast_ref::<crate::runtime::egui_mesh_surface::EguiMeshSurface>(
            ) && !self
                .remote_receipts
                .iter()
                .any(|receipt| receipt.matches(mesh.id, &mesh.retirement_binding.binding()))
            {
                self.remote_receipts.push(
                    plugins
                        .as_deref_mut()
                        .ok_or("plugin host disappeared")?
                        .enqueue_observed_mesh_retirement(mesh.id, &mesh.retirement_binding)?,
                );
            }
        }
        for mut owner in self.owners.drain(..) {
            let id = owner
                .surface
                .surface_id()
                .ok_or("retiring kind has no identity")?;
            if let Some((terminal, pty)) = owner.pair.take() {
                drop(terminal);
                if let Some(pty) = pty {
                    self.receipts.push(pty.retire());
                }
            }
            if let Some(reference) = owner.scrollback {
                crate::scrollback_store::delete(&reference);
            }
            engine.cleanup_surface_observations(id);
            drop(owner.surface);
        }
        Ok(())
    }
    pub(crate) fn retry_owned(
        &mut self,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) {
        if self.started.is_none() || self.start_complete {
            return;
        }
        // Only an unsent bound retirement is retried. Already enqueued exact bindings retain their
        // original ACK receipts, including a failed/unknown response; no callback is replayed.
        let result = self.start_owned(engine, plugins);
        self.start_complete = result.is_ok();
        self.failure = result.err();
    }
    pub(crate) fn outcome(&self) -> Option<tasty_core::OperationOutcome> {
        use tasty_core::OperationOutcome;
        let started = self.started?;
        if let Some(reason) = &self.failure {
            return Some(OperationOutcome::Uncertain {
                reason: reason.clone(),
            });
        }
        if !self.start_complete {
            return None;
        }
        if self
            .receipts
            .iter()
            .any(|receipt| receipt.observation().phase == PtyPhase::WaitFailed)
        {
            return Some(OperationOutcome::Uncertain {
                reason: "closed PTY owner has no confirmed reap".into(),
            });
        }
        if let Some(reason) = self
            .remote_receipts
            .iter()
            .find_map(|receipt| receipt.observation().and_then(Result::err))
        {
            return Some(OperationOutcome::Uncertain { reason });
        }
        if self
            .receipts
            .iter()
            .all(|receipt| receipt.observation().phase == PtyPhase::Reaped)
            && self
                .remote_receipts
                .iter()
                .all(|receipt| matches!(receipt.observation(), Some(Ok(()))))
        {
            return Some(OperationOutcome::Succeeded);
        }
        if started.elapsed() >= std::time::Duration::from_secs(5) {
            return Some(OperationOutcome::Uncertain {
                reason: "resource retirement acknowledgement deadline elapsed".into(),
            });
        }
        None
    }
    /// 은퇴한 surface·workspace scope를 지운다. 정리는 writer 요청 하나로 넣고 다음 호출에서 결과를
    /// 거둔다. 아직 결과가 없으면 `Ok(false)`다. 대상 ID가 다시 살아 있으면 넣지 않는다. 같은 ID의
    /// 새 소유자가 쓰는 metadata는 이 요청 뒤에 접수되므로 정리가 지우지 않는다.
    pub(crate) fn finish_metadata(
        &mut self,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    ) -> Result<bool, String> {
        if self.metadata_complete {
            return Ok(true);
        }
        if let Some(ticket) = self.metadata_purge.as_mut() {
            let Some(completion) = ticket.try_take() else {
                return Ok(false);
            };
            self.metadata_purge = None;
            return match tasty_memory::writer::settle(completion) {
                Ok(_) => {
                    self.metadata_complete = true;
                    Ok(true)
                }
                Err(failure) => Err(format!("retired scope purge failed: {failure:?}")),
            };
        }
        if !matches!(
            self.outcome(),
            Some(tasty_core::OperationOutcome::Succeeded)
        ) {
            return Err("cleanup receipt is not complete".into());
        }
        let mut scopes = Vec::new();
        for target in &self.plan.removed {
            let scope = match target.kind {
                tasty_core::IdKind::Surface => {
                    if engine.core.find_surface_by_id(target.id).is_some() {
                        return Err(
                            "retired surface identity is live again; refusing metadata cleanup"
                                .into(),
                        );
                    }
                    tasty_memory::Scope::Surface(target.id)
                }
                tasty_core::IdKind::Workspace => {
                    if engine.core.has_workspace(target.id) {
                        return Err(
                            "retired workspace identity is live again; refusing metadata cleanup"
                                .into(),
                        );
                    }
                    tasty_memory::Scope::Workspace(target.id)
                }
                _ => continue,
            };
            scopes.push(scope);
        }
        let request = tasty_memory::writer::MemoryRequest::unit(0, move |memory| {
            for scope in &scopes {
                memory.purge_scope(scope)?;
            }
            Ok(())
        });
        let mut ticket = engine
            .runtime
            .memory_access
            .submit(request)
            .map_err(|error| format!("retired scope purge was not accepted: {error}"))?;
        match ticket.try_take() {
            Some(completion) => match tasty_memory::writer::settle(completion) {
                Ok(_) => {
                    self.metadata_complete = true;
                    Ok(true)
                }
                Err(failure) => Err(format!("retired scope purge failed: {failure:?}")),
            },
            None => {
                self.metadata_purge = Some(ticket);
                Ok(false)
            }
        }
    }
    /// Notification delivery follows durable completion. It performs no destruction and is not
    /// replayed from historical operations during recovery.
    pub(crate) fn notify_completed(
        &self,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    ) {
        use crate::core::host_event::{PendingHostEvent, PendingSurfaceClosed};
        for surface in &self.plan.surfaces {
            engine
                .runtime
                .pending_lifecycle_events
                .push(PendingSurfaceClosed {
                    surface_id: surface.id,
                    kind: Some(surface.kind.clone()),
                    is_user_close: self.plan.is_user_close,
                });
        }
        for (tab_id, pane_id) in &self.plan.tab_parents {
            engine.enqueue_host_event(PendingHostEvent::TabClosed {
                tab_id: *tab_id,
                pane_id: *pane_id,
            });
        }
        for entity in &self.plan.removed {
            match entity.kind {
                tasty_core::IdKind::Pane => {
                    engine.enqueue_host_event(PendingHostEvent::PaneClosed { pane_id: entity.id })
                }
                tasty_core::IdKind::Workspace => {
                    engine.enqueue_host_event(PendingHostEvent::WorkspaceClosed {
                        workspace_id: entity.id,
                    })
                }
                _ => {}
            }
        }
    }
    pub(crate) fn reconciliation_evidence(&self) -> Option<Vec<u8>> {
        if !self.start_complete
            || self.failure.is_some()
            || !self.owners.is_empty()
            || !self.metadata_complete
            || !self
                .receipts
                .iter()
                .all(|receipt| receipt.observation().phase == PtyPhase::Reaped)
            || !self
                .remote_receipts
                .iter()
                .all(|receipt| matches!(receipt.observation(), Some(Ok(()))))
        {
            return None;
        }
        let receipts:Vec<_>=self.receipts.iter().map(|receipt| {let observation=receipt.observation();serde_json::json!({"generation":receipt.generation().value(),"phase":"reaped","code":observation.exit.map(|exit|exit.code)})}).collect();
        serde_json::to_vec(&serde_json::json!({"version":1,"source":"owned-retirement-receipts","runtime_epoch":self.runtime_epoch,"engine_incarnation":self.engine_incarnation,"metadata_complete":true,"lease":self.lease,"targets":self.plan.surfaces,"receipts":receipts,"remote_destroy_acknowledged":self.remote_receipts.len()})).ok()
    }
    pub(crate) fn lease(&self) -> Option<&EffectLease> {
        self.lease.as_ref()
    }
    /// 정리 수신이 모두 끝나 metadata 정리만 남은 은퇴. 시험에서 정리 단계만 따로 본다.
    #[cfg(test)]
    pub(crate) fn awaiting_metadata_for_test(
        removed: Vec<tasty_core::EntityId>,
        lease: EffectLease,
    ) -> Self {
        Self {
            plan: RetirementPlan {
                replacement: None,
                target: tasty_core::CloseTarget::Surface(0),
                removed,
                surfaces: Vec::new(),
                tab_parents: Vec::new(),
                is_user_close: false,
                remote_user_close: false,
                undo: None,
            },
            engine_incarnation: 0,
            runtime_epoch: lease.runtime_epoch,
            owners: Vec::new(),
            receipts: Vec::new(),
            remote_receipts: Vec::new(),
            started: Some(std::time::Instant::now()),
            lease: Some(lease),
            start_complete: true,
            failure: None,
            metadata_complete: false,
            metadata_purge: None,
        }
    }
}

/// Process-owner disposal after the View/slot continuation has drained. It never changes structure,
/// purges stored metadata, cancels TaskScope work, or turns a timeout into a successful receipt.
#[derive(Default)]
pub(crate) struct EngineRelease {
    started: bool,
    owners: Vec<Box<dyn crate::model::Surface>>,
    ptys: Vec<PtyRetirement>,
    remote: Vec<crate::plugin_bridge::host_cmd::RemoteRetirementReceipt>,
    /// Terminal children still starting for a creation this release took over.
    spawns: Vec<crate::runtime::effect_runner::PendingSpawn>,
    unreturned: Vec<UnreturnedStandalone>,
    warned: bool,
}

/// An adopted standalone process outside its original registration: its return failed, or a halt
/// found the transfer still private. No release retires it, and it does not hold the release
/// open; the App keeps it until the process ends.
pub(crate) struct UnreturnedStandalone {
    // The Terminal drains the PTY output, so it stays with its process.
    _terminal: tasty_terminal::Terminal,
    pty: tasty_terminal::Pty,
}
impl UnreturnedStandalone {
    /// Nothing else observes a kept process, so this reaps an exited child and settles its
    /// reader; a child that is never waited for stays a zombie and its reader keeps waking.
    pub(crate) fn has_exited(&mut self) -> bool {
        self.pty.observe_exit();
        self.pty.state().exit().is_some()
    }
    #[cfg(all(test, unix, feature = "gui"))]
    pub(crate) fn process_id(&self) -> Option<u32> {
        self.pty.process_id()
    }
}

impl EngineRelease {
    #[cfg(feature = "gui")]
    pub(crate) fn has_unreturned_standalones(&self) -> bool {
        !self.unreturned.is_empty()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn needs_detached_poll(&self) -> bool {
        !self.spawns.is_empty()
            || !self.owners.is_empty()
            || self.ptys.iter().any(|receipt| {
                !matches!(
                    receipt.observation().phase,
                    PtyPhase::Reaped | PtyPhase::WaitFailed
                )
            })
            || self
                .remote
                .iter()
                .any(|receipt| receipt.observation().is_none())
    }
    pub(crate) fn retain_unreturned_standalone(
        &mut self,
        pty_id: u32,
        terminal: tasty_terminal::Terminal,
        pty: tasty_terminal::Pty,
    ) {
        tracing::warn!(
            pty = pty_id,
            "adopted standalone PTY is outside its registration; it is kept alive outside the engine"
        );
        self.unreturned.retain_mut(|kept| !kept.has_exited());
        self.unreturned.push(UnreturnedStandalone {
            _terminal: terminal,
            pty,
        });
    }
    /// Moves the unreturned processes out before the released session is dropped.
    #[cfg(feature = "gui")]
    pub(crate) fn take_unreturned_standalones(&mut self) -> Vec<UnreturnedStandalone> {
        std::mem::take(&mut self.unreturned)
    }
    pub(crate) fn retain_spawn(&mut self, pending: crate::runtime::effect_runner::PendingSpawn) {
        self.spawns.push(pending);
    }
    /// A finished spawn becomes an ordinary retired PTY; a failed one left its owner to the
    /// spawner. Returns whether any spawn is still running.
    fn collect_spawns(&mut self) -> bool {
        let mut ptys = Vec::new();
        self.spawns.retain(|pending| match pending.try_take() {
            None => true,
            Some(Ok(spawned)) => {
                drop(spawned.terminal);
                ptys.push(spawned.pty.retire());
                false
            }
            Some(Err(reason)) => {
                tracing::warn!(
                    operation = %pending.lease().operation.0,
                    "released terminal preparation ended without a candidate: {reason}"
                );
                false
            }
        });
        self.ptys.extend(ptys);
        !self.spawns.is_empty()
    }
    #[cfg(all(test, unix))]
    pub(crate) fn unreturned_pty_generations(&self) -> Vec<tasty_terminal::ResourceGeneration> {
        self.unreturned
            .iter()
            .map(|kept| kept.pty.generation())
            .collect()
    }
    #[cfg(all(test, unix))]
    pub(crate) fn retained_pty_receipts(
        &self,
    ) -> Vec<(
        tasty_terminal::ResourceGeneration,
        tasty_terminal::PtyObservation,
    )> {
        self.ptys
            .iter()
            .map(|receipt| (receipt.generation(), receipt.observation()))
            .collect()
    }
    #[cfg(all(test, unix))]
    pub(crate) fn retained_pty_generations(&self) -> Vec<tasty_terminal::ResourceGeneration> {
        self.ptys.iter().map(PtyRetirement::generation).collect()
    }

    pub(crate) fn retain_installation(
        &mut self,
        installed: crate::runtime::effect_runner::Installed,
    ) {
        installed.retire_for_release(self);
    }
    pub(crate) fn retain_pty(&mut self, receipt: PtyRetirement) {
        self.ptys.push(receipt);
    }
    pub(crate) fn retain_remote(
        &mut self,
        receipt: crate::plugin_bridge::host_cmd::RemoteRetirementReceipt,
    ) {
        self.remote.push(receipt);
    }
    pub(crate) fn poll(
        &mut self,
        session: &mut EngineSession,
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> bool {
        if !self.begin_release(session) {
            return false;
        }
        self.poll_detached(plugins)
    }

    /// Once all runtime maps have transferred to this container, receipt polling cannot touch
    /// a successor session. Failed/missing receipts retain their exact owners indefinitely.
    pub(crate) fn poll_detached(
        &mut self,
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> bool {
        if !self.started {
            return false;
        }
        let spawning = self.collect_spawns();
        self.retire_surface_owners(plugins);
        let failed = self
            .ptys
            .iter()
            .any(|receipt| receipt.observation().phase == PtyPhase::WaitFailed)
            || self
                .remote
                .iter()
                .any(|receipt| matches!(receipt.observation(), Some(Err(_))));
        if failed && !self.warned {
            tracing::warn!(
                "engine release lacks an exact resource completion receipt; owner retained"
            );
            self.warned = true;
        }
        !spawning
            && self.owners.is_empty()
            && self
                .ptys
                .iter()
                .all(|receipt| receipt.observation().phase == PtyPhase::Reaped)
            && self
                .remote
                .iter()
                .all(|receipt| matches!(receipt.observation(), Some(Ok(()))))
    }
    fn begin_release(&mut self, session: &mut EngineSession) -> bool {
        if !self.started {
            if !session.pending_materializations.is_empty()
                || !session.pending_resource_retirements.is_empty()
            {
                return false;
            }
            // TaskScope Drop remains non-cancelling. Hooks/observers retain their existing owner
            // lifetime until this exact terminal retirement finishes; no runner stop is synthesized.
            self.owners
                .extend(session.runtime.surfaces.drain().map(|(_, surface)| surface));
            let ids: Vec<_> = session.runtime.terminals.iter().map(|(id, _)| id).collect();
            for id in ids {
                session.observer_router.drop_surface(id);
                session.hooks.forget_surface(id);
                if let Some(factory) = session.runtime.waker_factory.as_ref() {
                    factory.forget_surface(id);
                }
                if let Some((terminal, pty)) = session.runtime.terminals.remove(id) {
                    drop(terminal);
                    if let Some(pty) = pty {
                        self.ptys.push(pty.retire());
                    }
                }
            }
            self.started = true;
        }
        true
    }

    fn retire_surface_owners(&mut self, mut plugins: Option<&mut crate::plugin::PluginManager>) {
        let mut retained = Vec::new();
        for surface in self.owners.drain(..) {
            if let Some(remote) = surface
                .as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>(
            ) {
                if self
                    .remote
                    .iter()
                    .any(|receipt| receipt.matches(remote.id, &remote.handles().binding()))
                {
                    drop(surface);
                    continue;
                }
                if !retain_remote_receipt(
                    &mut self.remote,
                    &mut self.warned,
                    plugins.as_deref_mut(),
                    remote,
                ) {
                    retained.push(surface);
                    continue;
                }
            }
            if let Some(mesh) = surface
                .as_any()
                .downcast_ref::<crate::runtime::egui_mesh_surface::EguiMeshSurface>()
                && !self
                    .remote
                    .iter()
                    .any(|receipt| receipt.matches(mesh.id, &mesh.retirement_binding.binding()))
            {
                let result = plugins
                    .as_deref_mut()
                    .ok_or_else(|| "mesh plugin host unavailable".to_owned())
                    .and_then(|plugins| {
                        plugins.enqueue_observed_mesh_retirement(mesh.id, &mesh.retirement_binding)
                    });
                match result {
                    Ok(receipt) => self.remote.push(receipt),
                    Err(_) => {
                        retained.push(surface);
                        continue;
                    }
                }
            }
            drop(surface);
        }
        self.owners = retained;
    }
}

/// Take/restore avoids lending the session and one of its resource fields mutably at once.
pub(crate) fn poll_engine_release(
    session: &mut EngineSession,
    plugins: Option<&mut crate::plugin::PluginManager>,
) -> bool {
    let mut release = session.engine_release.take().unwrap_or_default();
    let complete = release.poll(session, plugins);
    session.engine_release = Some(release);
    complete
}

impl ResourceRetirement {
    pub(crate) fn retain_for_release(mut self, release: &mut EngineRelease) {
        release.ptys.append(&mut self.receipts);
        release.remote.append(&mut self.remote_receipts);
        for mut owner in self.owners.drain(..) {
            if let Some((terminal, pty)) = owner.pair.take() {
                drop(terminal);
                if let Some(pty) = pty {
                    release.ptys.push(pty.retire());
                }
            }
            release.owners.push(owner.surface);
        }
        // Halt disposal cannot publish the old command outcome or repeat lifecycle notifications.
    }
}
impl EngineRelease {
    pub(crate) fn retain_surface(&mut self, surface: Box<dyn crate::model::Surface>) {
        self.owners.push(surface);
    }
}

fn retain_remote_receipt(
    receipts: &mut Vec<crate::plugin_bridge::host_cmd::RemoteRetirementReceipt>,
    warned: &mut bool,
    plugins: Option<&mut crate::plugin::PluginManager>,
    remote: &crate::plugin_bridge::remote_surface::RemoteSurface,
) -> bool {
    let result = plugins
        .ok_or_else(|| "plugin host unavailable during engine release".to_owned())
        .and_then(|plugins| {
            plugins.enqueue_observed_remote_retirement(remote.id, remote.handles().binding())
        });
    match result {
        Ok(receipt) => receipts.push(receipt),
        Err(reason) => {
            if !*warned {
                tracing::warn!(%reason,"engine retains an unconfirmed remote owner");
                *warned = true;
            }
            return false;
        }
    }
    true
}
