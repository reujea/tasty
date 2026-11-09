//! Install an authorized candidate before publishing its final structural result.

use super::*;
use crate::runtime::pending_submit::PendingSubmit;
use tasty_terminal::{PtyPhase, PtyRetirement};

pub(crate) struct Installation {
    pub(crate) lease: EffectLease,
    pub(super) surface_id: u32,
    pub(super) previous_resource: Option<ResourceGeneration>,
    pub(super) connection: Option<(Terminal, Pty)>,
    pub(super) publication: Option<PublicationAction>,
    pub(super) unpublished_remote: Option<crate::plugin_bridge::host_cmd::SurfaceBinding>,
    pub(super) registration: Option<KindRegistration>,
    pub(super) scrollback_persist_id: Option<String>,
    pub(super) metadata: Vec<(String, String)>,
    pub(super) adoption: Option<crate::runtime::journal_product::AdoptRecipe>,
    pub(super) child: Option<crate::runtime::journal_product::ChildRecipe>,
    pub(super) one_shot_input: Option<String>,
    pub(super) retirement: Option<PtyRetirement>,
    pub(super) remote_retirements: Vec<crate::plugin_bridge::host_cmd::RemoteRetirementReceipt>,
    /// Receipts of the new candidate withdrawn after a partially applied installation.
    pub(super) candidate_retirement: Option<PtyRetirement>,
    pub(super) installed_child: Option<crate::core::child_parent::ParentKey>,
    /// The candidate's physical generation once it replaced the live owner.
    pub(super) installed_candidate: Option<ResourceGeneration>,
    /// The standalone metadata the adopted candidate carried before it replaced the live owner.
    pub(super) adopted_standalone: Option<tasty_terminal::StandalonePty>,
}

/// `applied` is false only when no old owner was retired and no new owner was registered or
/// installed. Such a failure leaves the published target intact and is a known rejection.
#[derive(Debug)]
pub(crate) struct InstallFailure {
    pub(crate) error: anyhow::Error,
    pub(crate) applied: bool,
}

/// Test-only failure injection at a fixed installation step of the production path.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InstallFault {
    /// After validation, before any owner changes.
    BeforeChange,
    /// After the candidate replaced the live owner.
    AfterSwap,
}

#[cfg(test)]
thread_local! {
    pub(crate) static INSTALL_FAULT: std::cell::Cell<Option<InstallFault>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn inject(point: InstallFault) -> anyhow::Result<()> {
    if INSTALL_FAULT.with(|fault| fault.get()) == Some(point) {
        INSTALL_FAULT.with(|fault| fault.set(None));
        anyhow::bail!("injected installation failure {point:?}");
    }
    Ok(())
}

pub(crate) struct RetiringKind {
    surface_id: u32,
    mesh: Option<crate::plugin_bridge::host_cmd::MeshBinding>,
    remote: Option<crate::plugin_bridge::host_cmd::SurfaceBinding>,
}

impl RetiringKind {
    pub(crate) fn capture(surface: &dyn crate::model::Surface) -> Self {
        Self {
            surface_id: surface.surface_id().expect("retiring leaf has an ID"),
            mesh: surface
                .as_any()
                .downcast_ref::<crate::runtime::egui_mesh_surface::EguiMeshSurface>()
                .map(|mesh| mesh.retirement_binding.clone()),
            remote: surface
                .as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                .map(|remote| remote.handles().binding()),
        }
    }
}

pub(crate) struct Installed {
    pub(crate) lease: EffectLease,
    #[cfg(feature = "gui")]
    pub(crate) previous_resource: Option<ResourceGeneration>,
    pub(crate) surface_id: u32,
    retirement: Option<PtyRetirement>,
    remote_retirements: Vec<crate::plugin_bridge::host_cmd::RemoteRetirementReceipt>,
    installed_generation: Option<ResourceGeneration>,
    /// The one-shot body was written; its CR follows on the same physical owner.
    input: Option<PendingSubmit>,
    child_receipt: Option<crate::runtime::child_terminal::Ticket>,
    pending_body: Option<(String, bool)>,
}

impl Installed {
    #[cfg(test)]
    pub(crate) fn with_test_retirement(
        lease: EffectLease,
        receipt: crate::plugin_bridge::host_cmd::RemoteRetirementReceipt,
    ) -> Self {
        Self {
            lease,
            #[cfg(feature = "gui")]
            previous_resource: None,
            surface_id: 1,
            retirement: None,
            remote_retirements: vec![receipt],
            installed_generation: None,
            input: None,
            child_receipt: None,
            pending_body: None,
        }
    }

    pub(crate) fn reconciliation_evidence(
        &self,
        engine: &crate::runtime::engine_access::EngineRef<'_>,
        incarnation: u64,
    ) -> Option<Vec<u8>> {
        if !matches!(self.cleanup_complete(), Ok(true))
            || engine.runtime.terminals.generation(self.surface_id) != self.installed_generation
        {
            return None;
        }
        serde_json::to_vec(&serde_json::json!({"version":1,"source":"owned-preparation-receipts","runtime_epoch":self.lease.runtime_epoch,"engine_incarnation":incarnation,"lease":self.lease,"installation_complete":true,"cleanup_complete":true,"surface":self.surface_id,"physical_generation":self.installed_generation.map(|generation|generation.value()),"remote_destroy_acknowledged":self.remote_retirements.len()})).ok()
    }
    pub(crate) fn waiting_input(&self) -> bool {
        self.input.is_some() || self.pending_body.is_some()
    }
    #[cfg(all(test, unix))]
    pub(crate) fn holds_body(&self) -> bool {
        self.pending_body.is_some()
    }
    /// Sends the CR once it is due. A target whose physical owner changed or that became hard
    /// occupied never receives it; the failure drops the input so no later poll retries it.
    pub(crate) fn poll_input(&mut self, engine: &mut EngineMut<'_>) -> anyhow::Result<()> {
        if let Some(receipt) = &self.child_receipt {
            match receipt.observation() {
                None => return Ok(()),
                Some(Err(error)) => {
                    self.pending_body = None;
                    anyhow::bail!(error);
                }
                Some(Ok(_)) => {}
            }
        }
        if let Some((body, submit)) = self.pending_body.take() {
            let generation = engine
                .runtime
                .terminals
                .generation(self.surface_id)
                .filter(|generation| Some(*generation) == self.installed_generation)
                .ok_or_else(|| {
                    anyhow::anyhow!("child input owner changed before relationship receipt")
                })?;
            if engine.live.occupancy.is_hard_occupied(self.surface_id) {
                anyhow::bail!(
                    "child input target became hard occupied before relationship receipt"
                );
            }
            let ack = engine
                .runtime
                .terminals
                .get_mut(self.surface_id)
                .ok_or_else(|| anyhow::anyhow!("child input owner disappeared"))?
                .try_send_key_with_ack(&body)
                .map_err(|_| anyhow::anyhow!("child body queue closed"))?;
            self.input = submit.then(|| PendingSubmit::new(self.surface_id, generation, ack));
        }
        let Some(input) = self.input.as_mut() else {
            return Ok(());
        };
        if !engine
            .runtime
            .terminals
            .matches_generation(input.surface(), input.generation())
            || engine.live.occupancy.is_hard_occupied(input.surface())
        {
            self.input = None;
            anyhow::bail!("child input target changed after body enqueue; late submit discarded");
        }
        if !input.submit_due(std::time::Instant::now()) {
            return Ok(());
        }
        let surface = input.surface();
        self.input = None;
        engine
            .runtime
            .terminals
            .get_mut(surface)
            .ok_or_else(|| anyhow::anyhow!("child input owner disappeared"))?
            .try_send_key_with_ack("\r")
            .map_err(|_| anyhow::anyhow!("child submit queue closed"))?;
        Ok(())
    }
    /// When the pending CR should be checked next.
    pub(crate) fn input_deadline(&self) -> Option<std::time::Instant> {
        self.input.as_ref().map(PendingSubmit::next)
    }

    /// Signalling/removing an old owner is not evidence that the OS child was reaped.
    pub(crate) fn cleanup_complete(&self) -> anyhow::Result<bool> {
        if self.waiting_input() {
            return Ok(false);
        }
        self.release_complete()
    }

    /// Disposal can finish without claiming that an interrupted one-shot input was delivered.
    pub(crate) fn release_complete(&self) -> anyhow::Result<bool> {
        if let Some(receipt) = &self.child_receipt {
            match receipt.observation() {
                None => return Ok(false),
                Some(Err(error)) => anyhow::bail!(error),
                Some(Ok(_)) => {}
            }
        }
        for receipt in &self.remote_retirements {
            match receipt.observation() {
                Some(Ok(())) => {}
                Some(Err(reason)) => anyhow::bail!(reason),
                None => return Ok(false),
            }
        }
        match self
            .retirement
            .as_ref()
            .map(|receipt| receipt.observation().phase)
        {
            None | Some(PtyPhase::Reaped) => Ok(true),
            Some(PtyPhase::WaitFailed) => {
                anyhow::bail!("old PTY owner could not confirm child reap")
            }
            Some(_) => Ok(false),
        }
    }
}

impl Installation {
    pub(crate) fn surface_id(&self) -> u32 {
        self.surface_id
    }

    /// The caller keeps the entire affected batch hidden until the completion commit is applied.
    /// A failure after external registration is never answered by a legacy fallback.
    pub(crate) fn install(
        &mut self,
        engine: &mut EngineMut<'_>,
        plugins: Option<&mut crate::plugin::PluginManager>,
        retired_kind: Option<RetiringKind>,
    ) -> Result<Installed, InstallFailure> {
        let mut applied = false;
        self.install_steps(engine, plugins, retired_kind, &mut applied)
            .map_err(|error| InstallFailure { error, applied })
    }

    fn install_steps(
        &mut self,
        engine: &mut EngineMut<'_>,
        mut plugins: Option<&mut crate::plugin::PluginManager>,
        retired_kind: Option<RetiringKind>,
        applied: &mut bool,
    ) -> anyhow::Result<Installed> {
        self.validate_installation_owner(engine)?;
        #[cfg(test)]
        inject(InstallFault::BeforeChange)?;
        if let Some(old) = retired_kind {
            if old.surface_id != self.surface_id {
                anyhow::bail!("retired kind belongs to another surface");
            }
            if let Some(binding) = old.remote {
                let manager = plugins
                    .as_deref_mut()
                    .ok_or_else(|| anyhow::anyhow!("remote retirement has no plugin host"))?;
                self.remote_retirements.push(
                    manager
                        .enqueue_observed_remote_retirement(self.surface_id, binding)
                        .map_err(anyhow::Error::msg)?,
                );
                *applied = true;
            } else if let Some(binding) = old.mesh {
                let manager =
                    plugins.ok_or_else(|| anyhow::anyhow!("mesh retirement has no plugin host"))?;
                self.remote_retirements.push(
                    manager
                        .enqueue_observed_mesh_retirement(self.surface_id, &binding)
                        .map_err(anyhow::Error::msg)?,
                );
                *applied = true;
            }
        }
        // Destroy of the old registration precedes enqueueing Created/Restored for the new one.
        if let Some(publication) = self.publication.take() {
            *applied = true;
            publication()?;
        }
        *applied = true;
        let previous = match self.connection.take() {
            Some((mut terminal, mut pty)) => {
                self.installed_candidate = Some(pty.generation());
                if let Some(adoption) = &self.adoption {
                    if pty.generation().value() != adoption.resource_generation {
                        anyhow::bail!("standalone installation has another physical owner");
                    }
                    self.adopted_standalone = pty.adopt();
                    terminal.rewire_waker(engine.make_waker(self.surface_id));
                    if let Some(factory) = &engine.runtime.waker_factory {
                        factory.forget_surface(adoption.pty_id);
                    }
                }
                engine
                    .runtime
                    .terminals
                    .replace(self.surface_id, terminal, Some(pty))
            }
            None => engine.runtime.terminals.remove(self.surface_id),
        };
        // Own the old receipt before any later metadata/input step can fail.
        self.retirement = previous.and_then(|(terminal, pty)| {
            drop(terminal);
            pty.map(Pty::retire)
        });
        #[cfg(test)]
        inject(InstallFault::AfterSwap)?;
        if let Some(persist_id) = self.scrollback_persist_id.take() {
            engine
                .runtime
                .terminals
                .set_scrollback_persist_id(self.surface_id, persist_id);
        }
        self.install_metadata(engine);
        let submit = self.child.as_ref().is_some_and(|child| !child.replacing);
        let mut child_receipt = None;
        if let Some(child) = self.child.take() {
            if child.replacing {
                engine
                    .runtime
                    .child_terminals
                    .update_child(&child.parent, child.index, |entry| {
                        entry.cwd = child.cwd;
                        entry.role = child.role;
                        entry.nickname = child.nickname;
                    });
                engine
                    .runtime
                    .child_terminals
                    .set_idle(self.surface_id, false);
            } else {
                let label = child.nickname.clone().or_else(|| child.role.clone());
                engine
                    .occupy_soft(self.surface_id, child.parent.clone(), label)
                    .map_err(|error| {
                        anyhow::anyhow!("child occupancy install failed: {error:?}")
                    })?;
                self.installed_child = Some(child.parent.clone());
                engine.runtime.child_terminals.register_child(
                    child.parent,
                    crate::runtime::child_terminal::ChildEntry {
                        child_surface_id: self.surface_id,
                        index: child.index,
                        cwd: child.cwd,
                        role: child.role,
                        nickname: child.nickname,
                    },
                );
            }
            child_receipt = Some(
                engine
                    .runtime
                    .child_terminals
                    .submit()
                    .map_err(anyhow::Error::msg)?,
            );
        }
        let pending_body = self.one_shot_input.take().map(|body| (body, submit));
        // A candidate may have become quiet or exited before its earlier wake was handled.
        // Processing the installed owner preserves its queued OSC/output events for the ordinary drain.
        engine.runtime.terminals.process_surface(self.surface_id);
        Ok(Installed {
            lease: self.lease.clone(),
            #[cfg(feature = "gui")]
            previous_resource: self.previous_resource,
            surface_id: self.surface_id,
            installed_generation: engine.runtime.terminals.generation(self.surface_id),
            retirement: self.retirement.take(),
            remote_retirements: std::mem::take(&mut self.remote_retirements),
            input: None,
            child_receipt,
            pending_body,
        })
    }
}

impl Installation {
    /// Withdraw the unpublished new owner after a partially applied installation, so no live
    /// resource remains that the committed structure does not describe. Returns whether the old
    /// published owner was already retired; that target is then unavailable, not usable.
    pub(crate) fn withdraw_unpublished(
        &mut self,
        engine: &mut EngineMut<'_>,
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> bool {
        let old_retired = self.retirement.is_some() || !self.remote_retirements.is_empty();
        self.withdraw_child(engine);
        self.withdraw_candidate_terminal(engine);
        self.withdraw_remote_registration(plugins);
        old_retired
    }

    fn withdraw_child(&mut self, engine: &mut EngineMut<'_>) {
        let Some(parent) = self.installed_child.take() else {
            return;
        };
        engine
            .runtime
            .child_terminals
            .unregister_child_by_surface(self.surface_id);
        if let Err(error) = engine.release_soft_occupancy(self.surface_id, parent) {
            tracing::warn!(
                surface = self.surface_id,
                "withdrawn child kept no soft occupancy: {error:?}"
            );
        }
    }

    /// An adopted standalone pair returns to its original registration; one that cannot is
    /// retained in this owner. The withdrawal never kills it.
    fn withdraw_candidate_terminal(&mut self, engine: &mut EngineMut<'_>) {
        let installed_new = self.installed_candidate.is_some_and(|generation| {
            engine
                .runtime
                .terminals
                .matches_generation(self.surface_id, generation)
        });
        if self.adoption.is_some() {
            self.return_adopted_standalone(engine, installed_new);
            return;
        }
        let pair = if installed_new {
            engine
                .runtime
                .terminals
                .remove(self.surface_id)
                .and_then(|(terminal, pty)| pty.map(|pty| (terminal, pty)))
        } else {
            self.connection.take()
        };
        let Some((terminal, pty)) = pair else {
            return;
        };
        drop(terminal);
        self.candidate_retirement = Some(pty.retire());
    }

    /// The live connection is taken without revoking it, so the returned owner keeps its output.
    fn return_adopted_standalone(&mut self, engine: &mut EngineMut<'_>, installed_new: bool) {
        let Some(adoption) = &self.adoption else {
            return;
        };
        let pty_id = adoption.pty_id;
        let (mut terminal, mut pty) = if installed_new {
            let Some((terminal, pty, persist_id)) = engine
                .runtime
                .terminals
                .take_adopted(self.surface_id, adoption.resource_generation)
            else {
                return;
            };
            self.scrollback_persist_id = self.scrollback_persist_id.take().or(persist_id);
            (terminal, pty)
        } else {
            let Some(pair) = self.connection.take() else {
                return;
            };
            pair
        };
        if let Some(metadata) = self.adopted_standalone.take() {
            pty.set_standalone(metadata);
        }
        terminal.rewire_waker(engine.make_waker(pty_id));
        if let Err((terminal, pty, persist_id)) = engine
            .runtime
            .terminals
            .restore_standalone_adoption(pty_id, terminal, pty, self.scrollback_persist_id.take())
        {
            tracing::warn!(
                surface = self.surface_id,
                pty = pty_id,
                "adopted standalone PTY could not return to its registration and stays retained"
            );
            self.connection = Some((terminal, pty));
            self.scrollback_persist_id = persist_id;
        }
    }

    /// An adopted standalone process that could not return. No receipt can reconcile it.
    pub(crate) fn retains_adopted_standalone(&self) -> bool {
        self.adoption.is_some() && self.connection.is_some()
    }

    fn withdraw_remote_registration(&mut self, plugins: Option<&mut crate::plugin::PluginManager>) {
        if self.publication.take().is_some() {
            // Consuming the still-private closure proves Created/Restored was never enqueued.
            self.unpublished_remote = None;
            return;
        }
        let Some(binding) = self.unpublished_remote.take() else {
            return;
        };
        match plugins.map(|manager| {
            manager.enqueue_observed_remote_retirement(self.surface_id, binding.clone())
        }) {
            Some(Ok(receipt)) => self.remote_retirements.push(receipt),
            Some(Err(error)) => {
                tracing::warn!(
                    surface = self.surface_id,
                    "new plugin owner retained: {error}"
                );
                self.unpublished_remote = Some(binding);
            }
            None => self.unpublished_remote = Some(binding),
        }
    }

    /// Every withdrawn or retired owner has its exact completion receipt.
    pub(crate) fn release_complete(&self) -> anyhow::Result<bool> {
        if self.connection.is_some() || self.unpublished_remote.is_some() {
            return Ok(false);
        }
        for receipt in &self.remote_retirements {
            match receipt.observation() {
                Some(Ok(())) => {}
                Some(Err(reason)) => anyhow::bail!(reason),
                None => return Ok(false),
            }
        }
        for receipt in [&self.retirement, &self.candidate_retirement]
            .into_iter()
            .flatten()
        {
            match receipt.observation().phase {
                PtyPhase::Reaped => {}
                PtyPhase::WaitFailed => anyhow::bail!("withdrawn PTY owner could not confirm reap"),
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    pub(crate) fn retire_for_release(
        mut self,
        release: &mut crate::runtime::resource_retirement::EngineRelease,
    ) {
        if let Some(adoption) = &self.adoption
            && let Some((terminal, pty)) = self.connection.take()
        {
            release.retain_unreturned_standalone(adoption.pty_id, terminal, pty);
        }
        if let Some((terminal, pty)) = self.connection.take() {
            drop(terminal);
            release.retain_pty(pty.retire());
        }
        if let Some(receipt) = self.candidate_retirement.take() {
            release.retain_pty(receipt);
        }
        if self.publication.take().is_some()
            && let Some(binding) = self.unpublished_remote.take()
        {
            let (receipt, completion) =
                crate::plugin_bridge::host_cmd::RemoteRetirementReceipt::pending(
                    self.surface_id,
                    binding,
                );
            completion.finish(Ok(()));
            release.retain_remote(receipt);
        }
        if let Some(receipt) = self.retirement.take() {
            release.retain_pty(receipt);
        }
        for receipt in self.remote_retirements {
            release.retain_remote(receipt);
        }
        // A consumed publication or one-shot input is never reconstructed or resent here.
    }
}

impl Installed {
    pub(crate) fn retire_for_release(
        mut self,
        release: &mut crate::runtime::resource_retirement::EngineRelease,
    ) {
        if let Some(receipt) = self.retirement.take() {
            release.retain_pty(receipt);
        }
        for receipt in self.remote_retirements {
            release.retain_remote(receipt);
        }
        // The input receipt remains an unknown delivery outcome; disposing its exact PTY is a
        // separate process-owner result and never causes this input to be sent again.
    }
}

impl Installation {
    fn validate_installation_owner(&self, engine: &EngineMut<'_>) -> anyhow::Result<()> {
        if let Some(registration) = &self.registration {
            registration.validate(&engine.runtime.surface_registry)?;
        }
        if engine.runtime.terminals.generation(self.surface_id) != self.previous_resource {
            anyhow::bail!("prepared installation would replace a different physical owner");
        }
        if let Some(child) = &self.child {
            let existing = engine
                .runtime
                .child_terminals
                .find_child(&child.parent, child.index);
            if if child.replacing {
                existing.is_none_or(|entry| entry.child_surface_id != self.surface_id)
            } else {
                existing.is_some()
                    || engine
                        .live
                        .occupancy
                        .occupancy_of(self.surface_id)
                        .is_some()
            } {
                anyhow::bail!("reserved child relation or occupancy was replaced");
            }
        }
        Ok(())
    }
    /// metadata를 결과를 기다리지 않는 독립 쓰기 묶음 하나로 넣는다. writer 순서를 따르므로 뒤에 오는
    /// capture 읽기와 은퇴 정리는 이 쓰기의 반영을 본다. 실패는 메인 루프가 거둘 때 로그로 남긴다.
    fn install_metadata(&self, engine: &EngineMut<'_>) {
        if self.metadata.is_empty() {
            return;
        }
        let ops = self
            .metadata
            .iter()
            .map(|(key, value)| tasty_memory::WriteOp::Put {
                owner: tasty_memory::HOST_OWNER.to_string(),
                scope: tasty_memory::Scope::Surface(self.surface_id),
                key: key.clone(),
                value: tasty_memory::MemoryValue::Text(value.clone()),
                opts: tasty_memory::PutOpts::default(),
            })
            .collect();
        engine.runtime.memory_access.submit_detached(
            tasty_memory::writer::MemoryRequest::Batch {
                mode: tasty_memory::BatchMode::Independent,
                ops,
            },
            "surface metadata installation",
            None,
        );
    }
}
