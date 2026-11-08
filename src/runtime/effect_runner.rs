//! Executes an already claimed materialization. Prepared objects remain private until publication.

mod installation;
mod spawner;

use crate::model::TerminalSurface;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::journal_product::{ClaimedPreparation, EffectLease};
use crate::runtime::surface_registry::PublicationAction;
pub(crate) struct PreparedLeaf {
    pub(crate) logical_kind: String,
    pub(crate) surface: Box<dyn crate::model::Surface>,
}
use tasty_terminal::{Pty, ResourceGeneration, Terminal};

#[cfg(test)]
pub(crate) use installation::{INSTALL_FAULT, InstallFault};
pub(crate) use installation::{Installation, Installed, RetiringKind};
pub(crate) use spawner::{PendingSpawn, SpawnOutcome, TerminalJob, TerminalSpawner};
#[cfg(test)]
pub(crate) use spawner::{QUEUE_CAPACITY as SPAWN_QUEUE_CAPACITY, test_job as spawn_test_job};

/// An application binding permits execution for this selected engine in this process only.
#[derive(Clone, Debug)]
pub(crate) struct ExecutionBinding {
    pub(crate) stream: String,
    pub(crate) runtime_epoch: u64,
    pub(crate) engine_incarnation: u64,
}

pub(crate) struct PreparedMaterialization {
    pub(crate) lease: EffectLease,
    pub(crate) leaf: PreparedLeaf,
    pub(crate) connection: Option<(Terminal, Pty)>,
    pub(crate) publication: Option<PublicationAction>,
    previous_resource: Option<ResourceGeneration>,
    registration: Option<KindRegistration>,
    installed: bool,
    deferred: bool,
    adoption: Option<crate::runtime::journal_product::AdoptRecipe>,
    scrollback_persist_id: Option<String>,
    metadata: Vec<(String, String)>,
    child: Option<crate::runtime::journal_product::ChildRecipe>,
    one_shot_input: Option<String>,
    /// Convert and Restore replace a surface that already exists in the structure.
    replaces_structure: bool,
}

#[derive(Clone)]
pub(super) struct KindRegistration {
    kind: String,
    definition: std::sync::Weak<crate::runtime::surface_registry::SurfaceKindDef>,
}

impl KindRegistration {
    fn validate(
        &self,
        registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
    ) -> anyhow::Result<()> {
        if let Some(plugin_id) = registry.withdrawn_by(&self.kind) {
            return Err(crate::runtime::surface_registry::SurfaceKindWithdrawn {
                kind: self.kind.clone(),
                plugin_id,
            }
            .into());
        }
        let current = registry
            .get_live(&self.kind)
            .ok_or_else(|| anyhow::anyhow!("prepared kind is no longer registered"))?;
        if !self.definition.ptr_eq(&std::sync::Arc::downgrade(&current)) {
            anyhow::bail!("prepared kind registration was replaced before installation");
        }
        Ok(())
    }
}

pub(crate) struct PreparationFailure {
    pub(crate) reason: String,
    pub(crate) uncertain: bool,
}
impl std::fmt::Display for PreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

/// A claimed preparation either finished on the App owner or waits for its terminal child.
pub(crate) enum Begun {
    Ready(Box<PreparedMaterialization>),
    Spawning(Box<SpawnRequest>),
}

/// The private candidate whose terminal child the spawner starts. The App completes it.
pub(crate) struct SpawnRequest {
    pub(crate) job: TerminalJob,
    partial: Box<PreparedMaterialization>,
}

pub(crate) fn prepare(
    engine: &mut EngineMut<'_>,
    binding: &ExecutionBinding,
    claimed: ClaimedPreparation,
) -> Result<Begun, PreparationFailure> {
    let mut execution_started = false;
    prepare_inner(engine, binding, claimed, &mut execution_started).map_err(|error| {
        PreparationFailure {
            reason: error.to_string(),
            uncertain: execution_started,
        }
    })
}

/// Runs a whole preparation, waiting on the spawner, for tests that inspect the private candidate.
#[cfg(test)]
pub(crate) fn prepare_and_wait(
    engine: &mut EngineMut<'_>,
    binding: &ExecutionBinding,
    claimed: ClaimedPreparation,
    spawner: &TerminalSpawner,
) -> Result<PreparedMaterialization, String> {
    let request = match prepare(engine, binding, claimed).map_err(|error| error.reason)? {
        Begun::Ready(prepared) => return Ok(*prepared),
        Begun::Spawning(request) => request,
    };
    let (job, candidate) = (*request).split();
    let pending = spawner.submit(job).map_err(|(_, reason)| reason)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let spawned = loop {
        if let Some(outcome) = pending.try_take() {
            break outcome.map_err(|failure| failure.reason)?;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "terminal spawn stalled"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    match candidate.complete(engine, spawned) {
        SpawnCompletion::Ready(prepared) => Ok(*prepared),
        SpawnCompletion::Stale { reason, .. } => Err(reason),
    }
}

/// How a spawned candidate meets the engine it was prepared for.
pub(crate) enum SpawnCompletion {
    Ready(Box<PreparedMaterialization>),
    /// The target changed while the child started; the child is retired, nothing was installed.
    Stale {
        lease: EffectLease,
        reason: String,
        retirement: tasty_terminal::PtyRetirement,
    },
}

impl SpawnRequest {
    pub(crate) fn split(self) -> (TerminalJob, PendingCandidate) {
        (self.job, PendingCandidate(self.partial))
    }
}

/// The candidate whose child is still starting. It owns no external resource yet.
pub(crate) struct PendingCandidate(Box<PreparedMaterialization>);

impl PendingCandidate {
    pub(crate) fn lease(&self) -> &EffectLease {
        &self.0.lease
    }
    /// Compares the target with the claim-time observation before the child joins the candidate.
    pub(crate) fn complete(
        self,
        engine: &EngineMut<'_>,
        spawned: spawner::SpawnedTerminal,
    ) -> SpawnCompletion {
        let mut partial = self.0;
        let surface_id = partial
            .surface_id()
            .expect("terminal candidate has a fixed ID");
        let current = engine.runtime.terminals.generation(surface_id);
        let stale = if current != partial.previous_resource {
            Some("the target terminal changed while its replacement was starting")
        } else if partial.previous_resource.is_none()
            && !partial.replaces_structure
            && engine.find_surface_by_id(surface_id).is_some()
        {
            Some("reserved surface ID gained a live owner while its terminal was starting")
        } else {
            None
        };
        if let Some(reason) = stale {
            drop(spawned.terminal);
            return SpawnCompletion::Stale {
                lease: partial.lease,
                reason: reason.to_owned(),
                retirement: spawned.pty.retire(),
            };
        }
        partial.connection = Some((spawned.terminal, spawned.pty));
        partial.scrollback_persist_id = spawned.scrollback_persist_id;
        SpawnCompletion::Ready(partial)
    }
}

fn prepare_inner(
    engine: &mut EngineMut<'_>,
    binding: &ExecutionBinding,
    claimed: ClaimedPreparation,
    execution_started: &mut bool,
) -> anyhow::Result<Begun> {
    if binding.stream != claimed.lease.stream
        || binding.runtime_epoch != claimed.lease.runtime_epoch
        || binding.engine_incarnation != claimed.engine_incarnation
    {
        anyhow::bail!("materialization claim belongs to a different engine binding");
    }
    let surface_id = claimed.plan.surface.id;
    let previous_resource = engine.runtime.terminals.generation(surface_id);
    let replaces_structure = matches!(
        claimed.plan.destination,
        tasty_core::CreationDestination::Convert { .. }
            | tasty_core::CreationDestination::Restore { .. }
    );
    if !replaces_structure
        && (previous_resource.is_some() || engine.find_surface_by_id(surface_id).is_some())
    {
        anyhow::bail!("reserved surface ID already has a live owner");
    }

    // Decode and validate opaque data before acquiring an OS child. A malformed capture must not
    // drop a newly spawned Pty while reporting a known pre-execution failure.
    let mut decoded_capture = claimed
        .capture
        .as_deref()
        .map(crate::core::layout_persistence::import::surface_data::SurfaceData::decode)
        .transpose()?;
    if decoded_capture.as_ref().is_some_and(|capture| {
        matches!(
            capture,
            crate::core::layout_persistence::import::surface_data::SurfaceData::Terminal { .. }
        ) != (claimed.input.kind == "terminal")
    }) {
        anyhow::bail!("activation capture belongs to another kind");
    }
    // 내용 복원을 끈 뒤에는 아직 남아 있는 이전 payload(지연 탭·undo 기록 등)의 scrollback도
    // 터미널에 넣지 않는다. payload 자체는 그 holder가 계속 소유한다.
    if !engine.runtime.settings.general.restore_surface_content
        && let Some(crate::core::layout_persistence::import::surface_data::SurfaceData::Terminal {
            scrollback,
            ..
        }) = decoded_capture.as_mut()
    {
        *scrollback = None;
    }
    let mut scrollback_persist_id = None;
    let metadata = if matches!(
        claimed.plan.destination,
        tasty_core::CreationDestination::Pane { .. }
            | tasty_core::CreationDestination::Split { .. }
    ) {
        claimed
            .input
            .params
            .get("meta")
            .and_then(|value| value.as_object())
            .into_iter()
            .flatten()
            .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
            .collect()
    } else {
        Vec::new()
    };
    let input = claimed.input;
    let adoption = input.adopt.clone();
    let child = input.child.clone();
    if child
        .as_ref()
        .is_some_and(|child| child.runtime_epoch != binding.runtime_epoch)
    {
        anyhow::bail!(
            "child creation belongs to an earlier runtime; one-shot input cannot be replayed"
        );
    }
    let deferred = matches!(
        claimed.plan.destination,
        tasty_core::CreationDestination::Assembly { .. }
    ) && input.kind != "terminal"
        && engine
            .runtime
            .surface_registry
            .get_live(&input.kind)
            .is_none();
    let mut spawn_job = None;
    let (surface, connection, publication, registration) = if let Some(adoption) = &adoption {
        if input.kind != "terminal" || adoption.runtime_epoch != binding.runtime_epoch {
            anyhow::bail!("standalone transfer belongs to another runtime or kind");
        }
        let (terminal, pty, persist_id) = engine
            .runtime
            .terminals
            .take_standalone_for_adoption(adoption.pty_id, adoption.resource_generation)
            .ok_or_else(|| anyhow::anyhow!("standalone owner exited or changed before transfer"))?;
        scrollback_persist_id = persist_id;
        (
            Box::new(TerminalSurface { id: surface_id }) as Box<dyn crate::model::Surface>,
            Some((terminal, pty)),
            None,
            None,
        )
    } else if input.kind == "terminal" {
        let shell = input
            .shell
            .ok_or_else(|| anyhow::anyhow!("terminal preparation has no shell recipe"))?;
        let job = TerminalJob {
            lease: claimed.lease.clone(),
            surface_id,
            cols: shell.cols,
            rows: shell.rows,
            shell: (!shell.executable.is_empty()).then_some(shell.executable),
            arguments: shell.arguments,
            environment: shell.environment,
            working_dir: input.cwd.clone(),
            initial_input: shell
                .restore_command
                .as_ref()
                .map(|command| format!("{command}\r")),
            waker: engine.make_waker(surface_id),
            palette: crate::runtime::terminal_store::current_terminal_palette(),
            scrollback_lines: shell.scrollback_lines,
            disk_scrollback: shell.disk_scrollback,
            disk_scrollback_lines: shell.disk_scrollback_lines,
            capture: decoded_capture,
            startup_command: shell.startup_command,
        };
        spawn_job = Some(job);
        (
            Box::new(TerminalSurface { id: surface_id }) as Box<dyn crate::model::Surface>,
            None,
            None,
            None,
        )
    } else if deferred {
        (
            Box::new(crate::runtime::surface_restorer::JournalPlaceholder {
                attempts: 0,
                failure: None,
                recovery_blocked: false,
                id: surface_id,
                kind: input.kind.clone(),
                data: claimed.plan.surface.data,
                creation_seed: None,
                activation: Some(tasty_core::Activation {
                    generation: claimed.lease.resource_generation,
                    phase: tasty_core::ActivationPhase::Deferred,
                }),
            }) as Box<dyn crate::model::Surface>,
            None,
            None,
            None,
        )
    } else {
        if let Some(plugin_id) = engine.runtime.surface_registry.withdrawn_by(&input.kind) {
            return Err(crate::runtime::surface_registry::SurfaceKindWithdrawn {
                kind: input.kind,
                plugin_id,
            }
            .into());
        }
        let definition = engine
            .runtime
            .surface_registry
            .get_live(&input.kind)
            .ok_or_else(|| anyhow::anyhow!("surface kind {} is not registered", input.kind))?;
        let (restore, cwd) = match decoded_capture {
            Some(crate::core::layout_persistence::import::surface_data::SurfaceData::Generic {
                data,
                cwd,
            }) => (Some(data), cwd.map(std::path::PathBuf::from)),
            Some(_) => unreachable!("capture kind was validated before execution"),
            None => (input.restore, input.cwd),
        };
        *execution_started = true;
        let prepared = match restore {
            Some(data) => (definition.restore)(surface_id, cwd.as_deref(), &data)?,
            None => (definition.create)(surface_id, cwd.as_deref(), &input.params)?,
        };
        let registration = KindRegistration {
            kind: input.kind.clone(),
            definition: std::sync::Arc::downgrade(&definition),
        };
        (
            prepared.surface,
            None,
            prepared.publication,
            Some(registration),
        )
    };
    let prepared = PreparedMaterialization {
        lease: claimed.lease,
        leaf: PreparedLeaf {
            logical_kind: input.kind,
            surface,
        },
        connection,
        publication,
        previous_resource,
        registration,
        installed: false,
        deferred,
        adoption,
        scrollback_persist_id,
        metadata,
        child,
        one_shot_input: None,
        replaces_structure,
    };
    Ok(match spawn_job {
        Some(job) => Begun::Spawning(Box::new(SpawnRequest {
            job,
            partial: Box::new(prepared),
        })),
        None => Begun::Ready(Box::new(prepared)),
    })
}

impl PreparedMaterialization {
    /// Installation consumes external handles while the engine keeps the unpublished kind leaf.
    pub(crate) fn set_one_shot_input(&mut self, input: Option<String>) {
        self.one_shot_input = input;
    }
    pub(crate) fn is_deferred(&self) -> bool {
        self.deferred
    }
    pub(crate) fn surface_id(&self) -> Option<u32> {
        self.leaf.surface.surface_id()
    }

    pub(crate) fn begin_installation(
        &mut self,
        registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
    ) -> anyhow::Result<Installation> {
        if self.installed {
            anyhow::bail!("prepared materialization was already installed");
        }
        if let Some(registration) = &self.registration {
            registration.validate(registry)?;
        }
        if let Some(adoption) = &self.adoption {
            let (_, pty) = self
                .connection
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("standalone transfer has no physical owner"))?;
            if pty.generation().value() != adoption.resource_generation
                || pty.state().standalone().is_none()
            {
                anyhow::bail!("standalone transfer binding changed before installation");
            }
            // check_alive records reap without consuming the once-only exit notification.
            // Rejection keeps this exact pair available for rollback to its standalone key.
            if !pty.check_alive()
                || pty.state().observation().phase == tasty_terminal::PtyPhase::WaitFailed
            {
                anyhow::bail!(
                    "standalone owner exited or could not be observed before installation"
                );
            }
        }
        if self.child.as_ref().is_some_and(|child| child.has_command)
            && self.one_shot_input.is_none()
        {
            anyhow::bail!("child command continuation is unavailable; input cannot be replayed");
        }
        self.installed = true;
        Ok(Installation {
            lease: self.lease.clone(),
            surface_id: self
                .leaf
                .surface
                .surface_id()
                .expect("prepared surface has a fixed ID"),
            previous_resource: self.previous_resource,
            connection: self.connection.take(),
            unpublished_remote: self
                .leaf
                .surface
                .as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                .map(|remote| remote.handles().binding()),
            publication: self.publication.take(),
            registration: self.registration.take(),
            scrollback_persist_id: self.scrollback_persist_id.take(),
            metadata: std::mem::take(&mut self.metadata),
            adoption: self.adoption.clone(),
            child: self.child.clone(),
            one_shot_input: self.one_shot_input.take(),
            retirement: None,
            remote_retirements: Vec::new(),
            candidate_retirement: None,
            installed_child: None,
            installed_candidate: None,
            adopted_standalone: None,
        })
    }

    /// Return the handles of an installation that failed before changing any owner, so the
    /// ordinary discard path retires the candidate or restores a standalone transfer.
    pub(crate) fn revert_installation(&mut self, installation: Installation) {
        debug_assert!(installation.retirement.is_none());
        debug_assert!(installation.remote_retirements.is_empty());
        self.connection = installation.connection;
        self.publication = installation.publication;
        self.registration = installation.registration;
        self.scrollback_persist_id = installation.scrollback_persist_id;
        self.metadata = installation.metadata;
        self.one_shot_input = installation.one_shot_input;
        self.installed = false;
    }

    pub(crate) fn discard(
        mut self,
        engine: &mut EngineMut<'_>,
    ) -> Result<Option<tasty_terminal::PtyRetirement>, (Self, String)> {
        let Some((terminal, pty)) = self.connection.take() else {
            return Ok(None);
        };
        if let Some(adoption) = &self.adoption {
            if let Err(pair) = engine.runtime.terminals.restore_standalone_adoption(
                adoption.pty_id,
                terminal,
                pty,
                self.scrollback_persist_id.take(),
            ) {
                self.connection = Some((pair.0, pair.1));
                self.scrollback_persist_id = pair.2;
                return Err((
                    self,
                    "original standalone binding cannot be restored; reconciliation required"
                        .into(),
                ));
            }
            return Ok(None);
        }
        drop(terminal);
        Ok(Some(pty.retire()))
    }

    /// Publication needs the installed owner only. Retiring the replaced owner is a separate
    /// completion that the creation keeps observing after the structure is published.
    pub(crate) fn into_leaf(self, installed: &Installed) -> anyhow::Result<PreparedLeaf> {
        // A pending child CR does not hold publication back; it follows on the published owner.
        if self.lease != installed.lease
            || !self.installed
            || self.connection.is_some()
            || self.publication.is_some()
        {
            anyhow::bail!("uninstalled materialization cannot be published");
        }
        Ok(self.leaf)
    }
}

impl PreparedMaterialization {
    /// A standalone transfer keeps its process: a release never retires a user's adopted PTY,
    /// whether its return failed or a halt found it still private. The rest of the private
    /// candidate is released as usual.
    pub(crate) fn release_keeping_standalone(
        mut self,
        release: &mut crate::runtime::resource_retirement::EngineRelease,
    ) {
        if let (Some(adoption), Some((terminal, pty))) = (&self.adoption, self.connection.take()) {
            release.retain_unreturned_standalone(adoption.pty_id, terminal, pty);
        }
        self.retire_for_release(release);
    }

    pub(crate) fn retire_for_release(
        mut self,
        release: &mut crate::runtime::resource_retirement::EngineRelease,
    ) {
        if let Some((terminal, pty)) = self.connection.take() {
            drop(terminal);
            release.retain_pty(pty.retire());
        }
        if self.publication.take().is_some()
            && let Some(remote) =
                self.leaf
                    .surface
                    .as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
        {
            // Consuming the still-private closure proves Created/Restored was never enqueued.
            let (receipt, completion) =
                crate::plugin_bridge::host_cmd::RemoteRetirementReceipt::pending(
                    remote.id,
                    remote.handles().binding(),
                );
            completion.finish(Ok(()));
            release.retain_remote(receipt);
        }
        release.retain_surface(self.leaf.surface);
    }
}

fn restore_terminal_capture(
    terminal: &mut Terminal,
    surface_id: u32,
    decoded_capture: Option<crate::core::layout_persistence::import::surface_data::SurfaceData>,
    scrollback_persist_id: &mut Option<String>,
) -> anyhow::Result<()> {
    if let Some(capture) = decoded_capture {
        match capture {
            crate::core::layout_persistence::import::surface_data::SurfaceData::Terminal {
                scrollback,
                scrollback_ref,
                ..
            } => {
                *scrollback_persist_id = scrollback_ref;
                if let Some(blob) = scrollback {
                    if let Some(lines) = tasty_terminal::disk_scrollback::deserialize_lines(&blob) {
                        if !lines.is_empty() {
                            terminal.inject_scrollback(lines);
                            let prefill = terminal.rows() / 2;
                            terminal.prefill_visible_from_scrollback(prefill);
                        }
                    } else {
                        tracing::warn!(
                            surface_id,
                            "stored scrollback could not be decoded; keeping its immutable capture"
                        );
                    }
                }
            }
            _ => anyhow::bail!("terminal activation capture belongs to another kind"),
        }
    }
    Ok(())
}
