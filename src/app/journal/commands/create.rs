//! Creation inputs are fixed only after the original key misses. Factories run after effect commit.
use super::*;
use crate::runtime::journal_product::{EngineBinding, PreparationInput, ShellRecipe};
use tasty_core::{CreationDestination, CreationPlan, IdKind, SurfaceSpec};

pub(super) struct Request {
    pub engine: EngineId,
    pub binding: EngineBinding,
    pub input: Option<PreparationInput>,
    pub plan: CreationPlan,
    pub renamed_name: Option<String>,
    pub renamed_subtitle: Option<String>,
    pub renamed_description: Option<String>,
    pub shape: Shape,
    pub activate: bool,
    pub one_shot_input: Option<String>,
    pub recent: Option<(String, String)>,
}

pub(super) enum Shape {
    Wake,
    Image { path: String },
    ChildRespawn { index: u32 },
    Child { index: u32, workspace: u32 },
    Adopt,
    Workspace,
    Tab,
    Pane,
    Surface,
    Convert,
}

impl Request {
    pub fn resolve(
        request: &JsonRpcRequest,
        session: &EngineSession,
        cwd: Option<std::path::PathBuf>,
    ) -> Result<Self, JsonRpcResponse> {
        let core = &session.core_state;
        let params = &request.params;
        let id = serde_json::Value::Null;
        let bad = |message| JsonRpcResponse::invalid_params(id.clone(), message);
        let internal = |message: String| JsonRpcResponse::internal_error(id.clone(), message);
        let kind = params
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("terminal");
        if let Some(definition) = session.runtime.surface_registry.get_live(kind)
            && let Some(missing) = definition.first_missing_required_param(params)
        {
            return Err(bad(format!(
                "Missing '{missing}' parameter for {kind} type"
            )));
        }
        let category = crate::ipc::handler::workspace::resolve_category_param(core, params)
            .map_err(&bad)?
            .unwrap_or(0);
        if kind == "empty" {
            return Err(internal(
                "Cannot create workspace with empty surface kind".into(),
            ));
        }
        let mut request = Self::base(session, kind, params, cwd)?;
        if let CreationDestination::Workspace {
            category: current, ..
        } = &mut request.plan.destination
        {
            *current = category;
        }
        Ok(request)
    }

    pub(super) fn base(
        session: &EngineSession,
        kind: &str,
        params: &serde_json::Value,
        cwd: Option<std::path::PathBuf>,
    ) -> Result<Self, JsonRpcResponse> {
        let core = &session.core_state;
        let internal =
            |message: String| JsonRpcResponse::internal_error(serde_json::Value::Null, message);
        let category = 0;
        let shell = crate::core::state::ShellConfig::from_settings(&session.runtime.settings);
        let display_index = core.workspaces().len();
        let tab_name = if kind == "terminal" {
            "Shell".into()
        } else {
            crate::runtime::surface_registry::default_tab_name_for_kind(
                kind,
                cwd.as_deref(),
                params,
                session.runtime.surface_registry.get_live(kind).as_deref(),
            )
        };
        Ok(Self {
            engine: session.id,
            binding: session
                .journal_binding
                .clone()
                .ok_or_else(|| internal("engine has no journal binding".into()))?,
            input: Some(PreparationInput {
                adopt: None,
                child: None,
                kind: kind.into(),
                cwd,
                params: params.clone(),
                restore: None,
                discard_capture: false,
                shell: (kind == "terminal").then_some(ShellRecipe {
                    executable: shell.shell,
                    arguments: shell.args,
                    environment: shell.envs,
                    cols: session.runtime.default_cols,
                    rows: session.runtime.default_rows,
                    scrollback_lines: session.runtime.settings.general.scrollback_lines,
                    disk_scrollback: session.runtime.settings.performance.scrollback_disk_swap,
                    disk_scrollback_lines: session
                        .runtime
                        .settings
                        .performance
                        .scrollback_disk_cap(),
                    startup_command: session.runtime.settings.general.startup_command.clone(),
                    restore_command: None,
                }),
            }),
            plan: CreationPlan {
                destination: CreationDestination::Workspace {
                    workspace: 0,
                    pane: 0,
                    tab: 0,
                    name: params
                        .get("name")
                        .and_then(|v| v.as_str())
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Workspace {}", display_index + 1)),
                    category,
                    subtitle: params
                        .get("subtitle")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    description: params
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                },
                surface: SurfaceSpec {
                    id: 0,
                    kind: kind.into(),
                    data: None,
                },
                tab_name,
                explicit_name: None,
            },
            renamed_name: params
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(str::to_owned),
            renamed_subtitle: params
                .get("subtitle")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            shape: Shape::Workspace,
            one_shot_input: None,
            recent: None,
            activate: false,
            renamed_description: params
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
        })
    }

    pub fn from_spec(
        spec: super::create_spec::Spec,
        session: &EngineSession,
    ) -> Result<Self, JsonRpcResponse> {
        use super::create_spec::Destination as D;
        let mut result = Self::base(session, &spec.kind, &spec.params, spec.cwd)?;
        let core = &session.core_state;
        let bad = |reason: String| JsonRpcResponse::invalid_params(serde_json::Value::Null, reason);
        result.plan.destination = match spec.destination {
            D::Adopt { pane, pty } => {
                let target = core
                    .find_pane_by_id(pane)
                    .ok_or_else(|| bad(format!("pane {pane} not found")))?;
                if core
                    .find_workspace_index_for_pane(pane)
                    .and_then(|index| core.workspace_at(index))
                    .is_some_and(|workspace| workspace.mirror)
                {
                    return Err(bad("cannot adopt a local PTY into a remote mirror".into()));
                }
                let owner = session
                    .runtime
                    .terminals
                    .standalone(pty)
                    .ok_or_else(|| bad(format!("headless pty {pty} not found")))?;
                if owner.state().exit().is_some() {
                    return Err(bad(format!("headless pty {pty} already exited")));
                }
                let input = result
                    .input
                    .as_mut()
                    .ok_or_else(|| bad("adoption input missing".into()))?;
                input.shell = None;
                input.adopt = Some(crate::runtime::journal_product::AdoptRecipe {
                    pty_id: pty,
                    resource_generation: owner.generation().value(),
                    runtime_epoch: result.binding.runtime_epoch,
                });
                result.shape = Shape::Adopt;
                result.activate = false;
                CreationDestination::Tab {
                    pane,
                    tab: 0,
                    index: target.tabs.len(),
                }
            }
            D::Workspace {
                name,
                subtitle,
                description,
                category,
            } => {
                result.activate = true;
                if spec.kind == "empty" {
                    return Err(bad("Cannot create workspace with empty surface kind".into()));
                }

                let CreationDestination::Workspace {
                    name: auto,
                    category: cat,
                    subtitle: sub,
                    description: desc,
                    ..
                } = &mut result.plan.destination
                else {
                    unreachable!("workspace draft")
                };
                if let Some(name) = name.clone() {
                    *auto = name;
                }
                if let Some(category) =
                    category.filter(|category| core.category_index(*category).is_some())
                {
                    *cat = category;
                }
                if let Some(subtitle) = subtitle.clone() {
                    *sub = subtitle;
                }
                if let Some(description) = description.clone() {
                    *desc = description;
                }
                result.renamed_name = name;
                result.renamed_subtitle = subtitle;
                result.renamed_description = description;
                return Ok(result);
            }
            D::Tab {
                pane,
                name,
                activate,
            } => {
                let target = core
                    .find_pane_by_id(pane)
                    .ok_or_else(|| bad(format!("Pane {pane} not found")))?;
                result.shape = Shape::Tab;
                result.activate = activate;
                result.plan.explicit_name = name.clone();
                if let Some(name) = name {
                    result.plan.tab_name = name;
                }
                CreationDestination::Tab {
                    pane,
                    tab: 0,
                    index: target.tabs.len(),
                }
            }
            D::Pane { target, direction } => {
                if core.find_pane_by_id(target).is_none() {
                    return Err(bad(format!("Pane {target} not found")));
                }
                result.shape = Shape::Pane;
                result.activate = true;
                CreationDestination::Pane {
                    target,
                    pane: 0,
                    tab: 0,
                    split: tasty_core::SplitSpec {
                        direction,
                        ratio: tasty_core::Ratio::from_f32(0.5),
                        placement: tasty_core::Placement::After,
                    },
                }
            }
            D::Surface { target, direction } => {
                if core.find_surface_by_id(target).is_none() {
                    return Err(bad(format!("Surface {target} not found")));
                }
                result.shape = Shape::Surface;
                result.activate = true;
                CreationDestination::Split {
                    target,
                    split: tasty_core::SplitSpec {
                        direction,
                        ratio: tasty_core::Ratio::from_f32(0.5),
                        placement: tasty_core::Placement::After,
                    },
                }
            }
            D::Convert { surface, respawn } => {
                if respawn
                    && let Some(shell) =
                        result.input.as_mut().and_then(|input| input.shell.as_mut())
                {
                    shell.startup_command.clear();
                    shell.restore_command = None;
                }
                if core.find_surface_by_id(surface).is_none() {
                    return Err(bad(format!("Surface {surface} not found")));
                }
                result.shape = Shape::Convert;
                result.plan.surface.id = surface;
                let tab = core.find_tab_for_surface(surface).and_then(|tab| {
                    core.find_pane_for_tab(tab)
                        .and_then(|pane| core.find_pane_by_id(pane))
                        .and_then(|pane| pane.tabs.iter().find(|candidate| candidate.id == tab))
                });
                let explicit_name = if respawn {
                    None
                } else {
                    tab.filter(|tab| tab.all_surface_ids().len() == 1).map(|_| {
                        if spec.kind == "terminal" {
                            None
                        } else {
                            Some(result.plan.tab_name.clone())
                        }
                    })
                };
                // Zero is an admission marker. Only the worker resolves it from canonical state,
                // before decide and before persisting the immutable operation plan.
                CreationDestination::Convert {
                    surface,
                    previous_activation: Some(0),
                    explicit_name,
                }
            }
        };
        Ok(result)
    }

    pub fn reservation(&mut self) -> Result<Work, String> {
        let mut kinds = match self.plan.destination {
            CreationDestination::Assembly { .. } => {
                return Err("assembly reservations belong to the coordinator".into());
            }
            CreationDestination::Workspace { .. } => {
                vec![(IdKind::Workspace, 1), (IdKind::Pane, 1), (IdKind::Tab, 1)]
            }
            CreationDestination::Tab { .. } => vec![(IdKind::Tab, 1)],
            CreationDestination::Pane { .. } => vec![(IdKind::Pane, 1), (IdKind::Tab, 1)],
            CreationDestination::Split { .. } => Vec::new(),
            CreationDestination::Convert { .. } | CreationDestination::Restore { .. } => {
                return Ok(Work::PutPreparation(
                    self.input.take().ok_or("creation input missing")?,
                ));
            }
        };
        kinds.push((IdKind::Surface, 1));
        Ok(Work::Reserve(kinds))
    }

    pub fn reserved(&mut self, ranges: &[tasty_event_store::IdRange]) -> Result<Work, String> {
        let id = |kind: IdKind| {
            ranges
                .iter()
                .find(|range| range.kind == kind.label() && range.end == range.start + 1)
                .ok_or_else(|| format!("missing reserved {} identity", kind.label()))
                .and_then(|range| u32::try_from(range.start).map_err(|error| error.to_string()))
        };
        match &mut self.plan.destination {
            CreationDestination::Workspace {
                workspace,
                pane,
                tab,
                ..
            } => {
                *workspace = id(IdKind::Workspace)?;
                *pane = id(IdKind::Pane)?;
                *tab = id(IdKind::Tab)?;
            }
            CreationDestination::Tab { tab, .. } => *tab = id(IdKind::Tab)?,
            CreationDestination::Pane { pane, tab, .. } => {
                *pane = id(IdKind::Pane)?;
                *tab = id(IdKind::Tab)?;
            }
            _ => {}
        }
        if !matches!(
            self.plan.destination,
            CreationDestination::Convert { .. } | CreationDestination::Restore { .. }
        ) {
            self.plan.surface.id = id(IdKind::Surface)?;
        }
        Ok(Work::PutPreparation(
            self.input
                .take()
                .ok_or("creation input already submitted")?,
        ))
    }

    pub fn stored(&self, input: tasty_core::DataRef) -> Work {
        let response = match &self.plan.destination {
            CreationDestination::Restore { .. } if matches!(self.shape, Shape::Wake) => {
                ResponsePlan::Fixed(JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"woke":true,"surface_id":self.plan.surface.id,"pty_ready":true}),
                ))
            }
            CreationDestination::Convert { .. } if matches!(self.shape, Shape::Image { .. }) => {
                let Shape::Image { path } = &self.shape else {
                    unreachable!("image response")
                };
                ResponsePlan::Fixed(JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"ok":true,"surface_id":self.plan.surface.id,"path":path}),
                ))
            }
            CreationDestination::Convert { .. }
                if matches!(self.shape, Shape::ChildRespawn { .. }) =>
            {
                let Shape::ChildRespawn { index } = self.shape else {
                    unreachable!("child respawn response")
                };
                ResponsePlan::Fixed(JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"child_surface_id":self.plan.surface.id,"child_index":index}),
                ))
            }
            CreationDestination::Tab { pane, .. } if matches!(self.shape, Shape::Child { .. }) => {
                let Shape::Child { index, workspace } = self.shape else {
                    unreachable!("child response")
                };
                ResponsePlan::Fixed(JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"child_surface_id":self.plan.surface.id,"child_index":index,"pane_id":pane,"workspace_id":workspace}),
                ))
            }
            CreationDestination::Tab { pane, tab, .. } if matches!(self.shape, Shape::Adopt) => {
                ResponsePlan::Fixed(JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"pane_id":pane,"tab_id":tab,"surface_id":self.plan.surface.id}),
                ))
            }
            CreationDestination::Workspace { workspace, .. } => ResponsePlan::WorkspaceCreated {
                stream: self.binding.stream.clone(),
                id: *workspace,
                surface_id: self.plan.surface.id,
            },
            CreationDestination::Tab { pane, tab, .. } => ResponsePlan::TabCreated {
                stream: self.binding.stream.clone(),
                pane: *pane,
                tab: *tab,
                surface: self.plan.surface.id,
                activate: self.activate,
            },
            CreationDestination::Pane { pane, .. } => {
                ResponsePlan::Fixed(JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"new_pane_id":pane,"new_surface_id":self.plan.surface.id}),
                ))
            }
            CreationDestination::Split { .. } => ResponsePlan::Fixed(JsonRpcResponse::success(
                serde_json::Value::Null,
                serde_json::json!({"new_surface_id":self.plan.surface.id}),
            )),
            _ => ResponsePlan::Fixed(JsonRpcResponse::success(
                serde_json::Value::Null,
                serde_json::json!({"ok":true,"surface_id":self.plan.surface.id}),
            )),
        };
        Work::Resolve {
            changes: vec![StreamCommand {
                stream: self.binding.stream.clone(),
                command: tasty_core::StructuralCommand::PrepareCreation {
                    operation: tasty_core::OperationId(String::new()),
                    command_id: String::new(),
                    input,
                    plan: self.plan.clone(),
                },
            }],
            response: Some(response),
        }
    }

    pub fn weight(&self) -> usize {
        serde_json::to_vec(&(
            &self.input,
            &self.plan,
            &self.binding.stream,
            &self.renamed_name,
            &self.renamed_subtitle,
            &self.renamed_description,
            if let Shape::Image { path } = &self.shape {
                Some(path)
            } else {
                None
            },
            &self.recent,
        ))
        .expect("creation input serializes")
        .len()
        .saturating_add(self.one_shot_input.as_ref().map_or(0, String::len))
    }
}

impl JournalApplication {
    pub(crate) fn resolve_public_creation(&mut self, ticket: u64, session: &mut EngineSession) {
        if !self.bind_command_engine(ticket, session.id) {
            return;
        }
        let Some(pending) = self.commands.pending.get_mut(&ticket) else {
            return;
        };
        if !matches!(
            pending.close_cause,
            super::close::Cause::RemoteHolder { .. }
        ) && let Some(response) = crate::ipc::handler::hard_occupied_structural_guard(
            &session.as_ref(),
            0,
            &pending.request.method,
            &pending.request.params,
            &serde_json::Value::Null,
        ) {
            self.reject_resolved_request(ticket, response);
            return;
        }
        let view = self
            .completion_views
            .get(&session.id)
            .cloned()
            .unwrap_or_default();
        let resolved = if pending.request.method == "terminal.spawn" {
            let caller_plugin = match &pending.reply {
                super::Reply::Plugin { plugin_id, .. } => Some(plugin_id.clone()),
                _ => None,
            };
            match super::child::resolve_spawn(
                &pending.request,
                session,
                &view,
                &mut pending.child_spawn,
                caller_plugin.as_deref(),
            ) {
                Ok(Some(request)) => Ok(request),
                Ok(None) => {
                    pending.needs_resolution = true;
                    self.refresh_command_weight(ticket);
                    return;
                }
                Err(response) => Err(response),
            }
        } else if pending.request.method == "terminal.respawn" {
            super::child::resolve_respawn(&pending.request, session)
        } else {
            super::create_spec::Spec::from_public(&pending.request, &session.as_ref(), &view)
                .and_then(|spec| Request::from_spec(spec, session))
                .and_then(|request| {
                    hold_explorer_floor(
                        request,
                        session,
                        self.split_geometries
                            .get(&session.id)
                            .map_or(&[], Vec::as_slice),
                    )
                })
        };
        match resolved {
            Ok(mut resource) => {
                if pending.request.method == "image.open" {
                    resource.shape = Shape::Image {
                        path: pending.request.params["path"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    };
                }
                resource.activate = false;
                let work = match resource.reservation() {
                    Ok(work) => work,
                    Err(error) => {
                        self.reject_resolved_request(
                            ticket,
                            JsonRpcResponse::internal_error(serde_json::Value::Null, error),
                        );
                        return;
                    }
                };
                let pending = self
                    .commands
                    .pending
                    .get_mut(&ticket)
                    .expect("creation admission remains owned");
                pending.request.params = serde_json::Value::Null;
                pending.resource = Some(resource);
                pending.queued = Some(work);
                self.refresh_command_weight(ticket);
                (self.wake)();
            }
            Err(response) => self.reject_resolved_request(ticket, response),
        }
    }
    pub(crate) fn resolve_workspace_creation(
        &mut self,
        ticket: u64,
        session: &EngineSession,
        cwd: Option<std::path::PathBuf>,
    ) {
        if !self.bind_command_engine(ticket, session.id) {
            return;
        }
        let has_creation = self.has_creation(session.id);
        let Some(pending) = self.commands.pending.get_mut(&ticket) else {
            return;
        };
        if has_creation {
            pending.needs_resolution = true;
            return;
        }
        // 새 workspace는 언제나 이 인스턴스에 있다.
        super::create_spec::resolve_relative_file(&mut pending.request.params, true);
        match Request::resolve(&pending.request, session, cwd) {
            Ok(request) => {
                pending.resource = Some(request);
                pending.request.params = serde_json::Value::Null;
                pending.queued = Some(Work::Reserve(vec![
                    (IdKind::Workspace, 1),
                    (IdKind::Pane, 1),
                    (IdKind::Tab, 1),
                    (IdKind::Surface, 1),
                ]));
                self.refresh_command_weight(ticket);
                (self.wake)();
            }
            Err(response) => self.reject_resolved_request(ticket, response),
        }
    }

    pub(crate) fn resolve_fixed_creation(&mut self, ticket: u64, session: &EngineSession) {
        if !self.bind_command_engine(ticket, session.id) {
            return;
        }
        let has_creation = self.has_creation(session.id);
        let views = self
            .split_geometries
            .get(&session.id)
            .cloned()
            .unwrap_or_default();
        let Some(pending) = self.commands.pending.get_mut(&ticket) else {
            return;
        };
        if has_creation {
            pending.needs_resolution = true;
            return;
        }
        let result =
            serde_json::from_value::<super::create_spec::Spec>(pending.request.params.clone())
                .map_err(|error| {
                    JsonRpcResponse::invalid_params(serde_json::Value::Null, error.to_string())
                })
                .and_then(|spec| Request::from_spec(spec, session))
                .and_then(|request| hold_explorer_floor(request, session, &views));
        match result {
            Ok(mut resource) => {
                if session
                    .runtime
                    .surface_registry
                    .get_live(&resource.plan.surface.kind)
                    .is_some_and(|kind| kind.records_recent)
                {
                    resource.recent = resource
                        .input
                        .as_ref()
                        .and_then(|input| input.params.get("file"))
                        .and_then(|file| file.as_str())
                        .filter(|file| !file.is_empty())
                        .map(|file| (resource.plan.surface.kind.clone(), file.to_owned()));
                }
                match resource.reservation() {
                    Ok(work) => pending.queued = Some(work),
                    Err(error) => {
                        self.reject_resolved_request(
                            ticket,
                            JsonRpcResponse::internal_error(serde_json::Value::Null, error),
                        );
                        return;
                    }
                }
                pending.resource = Some(resource);
                pending.request.params = serde_json::Value::Null;
                self.refresh_command_weight(ticket);
                (self.wake)();
            }
            Err(error) => self.reject_resolved_request(ticket, error),
        }
    }

    /// A command whose read is still with the worker is skipped. Every caller runs on a worker
    /// reply. Apart from an admission waiting for a credit, the worker handles and answers
    /// requests in submission order, and neither a read nor the completions that call this wait
    /// for a credit, so that read already observes the change. If it still returns InProgress, a
    /// later completion queues the next read.
    pub(in crate::app::journal) fn refresh_in_progress_commands(&mut self) {
        for pending in self.commands.pending.values_mut() {
            if pending.queued.is_none()
                && !pending.in_flight
                && let Some(command) = &pending.waiting_command
            {
                pending.queued = Some(Work::ReadCommand(command.clone()));
            }
        }
    }
}

#[cfg(feature = "gui")]
#[derive(Clone)]
pub(crate) enum TutorialCreated {
    Tab { pane: u32, tab: u32 },
    Pane { target: u32, pane: u32 },
    Surface { surface: u32 },
}
#[cfg(feature = "gui")]
impl TutorialCreated {
    #[cfg(feature = "gui")]
    pub fn observe(&self, state: &mut crate::state::MainViewState, core: &crate::core::CoreState) {
        match *self {
            Self::Tab { pane, tab } => state.observe_tutorial_tab_created(core, pane, tab),
            Self::Pane { target, pane } => {
                if let Some(workspace) = core
                    .find_workspace_index_for_pane(pane)
                    .and_then(|index| core.workspace_at(index))
                {
                    state.observe_tutorial_pane_split(workspace.id, target, pane);
                }
            }
            Self::Surface { surface } => {
                if let Some((index, pane)) = core.find_workspace_index_for_surface(surface) {
                    state.observe_tutorial_surface_split(core, index, pane, surface);
                }
            }
        }
    }
}

pub(super) struct Completed {
    #[cfg(feature = "gui")]
    pub tutorial: Option<TutorialCreated>,
    pub engine: EngineId,
    pub surface: u32,
    pub destination: CreationDestination,
    pub activate: bool,
    pub name: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    recent: Option<(String, String)>,
}
impl Completed {
    pub fn from_request(request: &Request) -> Self {
        Self {
            #[cfg(feature = "gui")]
            tutorial: match (&request.shape, &request.plan.destination) {
                (Shape::Tab, CreationDestination::Tab { pane, tab, .. }) => {
                    Some(TutorialCreated::Tab {
                        pane: *pane,
                        tab: *tab,
                    })
                }
                (Shape::Pane, CreationDestination::Pane { target, pane, .. }) => {
                    Some(TutorialCreated::Pane {
                        target: *target,
                        pane: *pane,
                    })
                }
                (Shape::Surface, CreationDestination::Split { .. }) => {
                    Some(TutorialCreated::Surface {
                        surface: request.plan.surface.id,
                    })
                }
                _ => None,
            },
            recent: request.recent.clone(),
            engine: request.engine,
            surface: request.plan.surface.id,
            destination: request.plan.destination.clone(),
            activate: request.activate,
            name: request.renamed_name.clone(),
            subtitle: request.renamed_subtitle.clone(),
            description: request.renamed_description.clone(),
        }
    }
    pub fn weight(&self) -> usize {
        serde_json::to_vec(&self.destination).map_or(0, |bytes| bytes.len())
            + self
                .recent
                .as_ref()
                .map_or(0, |(kind, file)| kind.len() + file.len())
            + self.name.as_ref().map_or(0, String::len)
            + self.subtitle.as_ref().map_or(0, String::len)
            + self.description.as_ref().map_or(0, String::len)
    }
    pub fn notify(
        self,
        sessions: &mut [&mut EngineSession],
        queue: &mut Vec<(EngineId, notification::Notification)>,
    ) {
        use crate::core::host_event::PendingHostEvent as E;
        // This service shares the existing home cache with View reads. Selection or View
        // retirement cannot erase a successfully opened file; failed/Stored paths never call us.
        if let Some((kind, file)) = self.recent {
            crate::recent_files::RecentFiles::load().add(&kind, file);
        }
        let Some(session) = sessions.iter().find(|session| session.id == self.engine) else {
            return;
        };
        let engine = session.as_ref();
        let Some((index, pane_id)) = engine.find_workspace_index_for_surface(self.surface) else {
            return;
        };
        let Some(workspace) = engine.workspace_at(index) else {
            return;
        };
        let Some(tab_id) = engine.find_tab_for_surface(self.surface) else {
            return;
        };
        let Some(surface) = engine.find_surface_by_id(self.surface) else {
            return;
        };
        let mut push = |event| queue.push((self.engine, notification::Notification::Ready(event)));
        if matches!(self.destination, CreationDestination::Workspace { .. }) {
            push(E::WorkspaceCreated {
                workspace_id: workspace.id,
                name: workspace.name.clone(),
            });
            if self.name.is_some() || self.subtitle.is_some() || self.description.is_some() {
                push(E::WorkspaceRenamed {
                    workspace_id: workspace.id,
                    name: self.name,
                    subtitle: self.subtitle,
                    description: self.description,
                    user_direct: false,
                });
            }
        }
        if matches!(
            self.destination,
            CreationDestination::Tab { .. } | CreationDestination::Pane { .. }
        ) {
            push(E::TabCreated {
                tab_id,
                pane_id,
                workspace_id: workspace.id,
                kind: surface.kind().into(),
            });
        }
        if let CreationDestination::Pane { target, split, .. } = &self.destination {
            push(E::PaneSplit {
                original_pane: *target,
                new_pane: pane_id,
                direction: split.direction,
            });
            push(E::PaneCreated {
                pane_id,
                workspace_id: workspace.id,
            });
        }
        if !matches!(
            self.destination,
            CreationDestination::Restore { .. } | CreationDestination::Convert { .. }
        ) {
            push(E::SurfaceCreated {
                surface_id: self.surface,
                kind: surface.kind(),
                tab_id,
                pane_id,
                workspace_id: workspace.id,
                created_by_plugin: None,
            });
        }
    }
}

/// split 이면 탐색기 칸 하한에 맞게 비율을 고치거나 거절한다([`super::split_floor`]).
fn hold_explorer_floor(
    mut request: Request,
    session: &EngineSession,
    views: &[super::split_floor::ShownWorkspace],
) -> Result<Request, JsonRpcResponse> {
    super::split_floor::hold(
        &mut request.plan.destination,
        &request.plan.surface.kind,
        &session.core_state,
        views,
    )?;
    Ok(request)
}
