//! Public response values are frozen in the same commit as their structural result.
use serde::{Deserialize, Serialize};
use tasty_core::{Rejection, StructureModels};
use tasty_ipc::protocol::JsonRpcResponse;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CompletionView {
    pub mirror_count: usize,
    pub focused_panes: std::collections::BTreeMap<u32, u32>,
    pub selected_tabs: std::collections::BTreeMap<u32, u32>,
    pub selected_surfaces: std::collections::BTreeMap<u32, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum ResponsePlan {
    Moved {
        success: JsonRpcResponse,
        not_moved: JsonRpcResponse,
    },
    AssemblyRestored {
        stream: String,
        root: tasty_core::EntityId,
        surfaces: Vec<u32>,
        presentation: tasty_core::UndoPresentation,
    },
    Fixed(JsonRpcResponse),
    Closed {
        success: JsonRpcResponse,
        not_closed: JsonRpcResponse,
    },
    WorkspaceCreated {
        stream: String,
        id: u32,
        surface_id: u32,
    },
    TabCreated {
        stream: String,
        pane: u32,
        tab: u32,
        surface: u32,
        activate: bool,
    },
    Multiple(Vec<ResponsePlan>),
    CategoryCreated {
        stream: String,
        id: u32,
    },
    WorkspaceUpdated {
        stream: String,
        id: u32,
        display_index: usize,
    },
}

impl ResponsePlan {
    pub(super) fn render(
        &self,
        after: &StructureModels,
        view: &CompletionView,
    ) -> Result<Vec<u8>, Rejection> {
        let response = match self {
            Self::AssemblyRestored {
                stream,
                root,
                surfaces,
                presentation,
            } => {
                let model = after
                    .streams
                    .get(stream)
                    .ok_or_else(|| Rejection("restored engine missing".into()))?;
                let mut presentation = presentation.clone();
                presentation.focused_panes.retain(|workspace, pane| {
                    model
                        .panes
                        .get(pane)
                        .is_some_and(|pane| pane.workspace == *workspace)
                });
                presentation
                    .selected_tabs
                    .retain(|pane, tab| model.tabs.get(tab).is_some_and(|tab| tab.pane == *pane));
                presentation.selected_surfaces.retain(|tab, surface| {
                    model
                        .surfaces
                        .get(surface)
                        .is_some_and(|surface| surface.tab == *tab)
                });
                let pane = match root.kind {
                    tasty_core::IdKind::Workspace => presentation
                        .focused_panes
                        .get(&root.id)
                        .copied()
                        .or_else(|| {
                            model
                                .workspaces
                                .get(&root.id)
                                .and_then(|workspace| workspace.layout.leaves().first().copied())
                        }),
                    tasty_core::IdKind::Pane => Some(root.id),
                    _ => model.tabs.get(&root.id).map(|tab| tab.pane),
                };
                let tab = pane.and_then(|pane| {
                    presentation.selected_tabs.get(&pane).copied().or_else(|| {
                        model
                            .panes
                            .get(&pane)
                            .and_then(|pane| pane.tabs.first().copied())
                    })
                });
                let surface = tab
                    .and_then(|tab| {
                        presentation
                            .selected_surfaces
                            .get(&tab)
                            .copied()
                            .or_else(|| {
                                model
                                    .tabs
                                    .get(&tab)
                                    .and_then(|tab| tab.layout.leaves().first().copied())
                            })
                    })
                    .filter(|surface| surfaces.contains(surface))
                    .or_else(|| {
                        surfaces
                            .iter()
                            .find(|surface| model.surfaces.contains_key(surface))
                            .copied()
                    });
                JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"restored":surface.is_some(),"kind":root.kind.label(),"restored_surface_id":surface,"presentation":presentation}),
                )
            }
            Self::Moved { success, .. } => success.clone(),
            Self::Fixed(response) => response.clone(),
            Self::Closed { success, .. } => success.clone(),
            Self::Multiple(plans) => {
                let results = plans
                    .iter()
                    .map(|plan| {
                        plan.render(after, view).and_then(|bytes| {
                            serde_json::from_slice::<JsonRpcResponse>(&bytes)
                                .map_err(|error| Rejection(error.to_string()))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(error) = results.iter().find(|response| response.error.is_some()) {
                    error.clone()
                } else {
                    JsonRpcResponse::success(
                        serde_json::Value::Null,
                        serde_json::Value::Array(
                            results
                                .into_iter()
                                .map(|response| response.result.unwrap_or_default())
                                .collect(),
                        ),
                    )
                }
            }
            Self::WorkspaceCreated {
                stream,
                id,
                surface_id,
            } => {
                let workspace = after
                    .streams
                    .get(stream)
                    .and_then(|model| model.workspaces.get(id))
                    .ok_or_else(|| Rejection("created workspace response target missing".into()))?;
                let display_index = after.streams[stream]
                    .workspace_order
                    .iter()
                    .position(|workspace| workspace == id)
                    .ok_or_else(|| Rejection("created workspace order missing".into()))?
                    .checked_add(view.mirror_count)
                    .ok_or_else(|| Rejection("workspace display index overflow".into()))?;
                JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({
                        "id": id, "name":workspace.name, "subtitle":workspace.subtitle,
                        "description":workspace.description,"index":display_index,"surface_id":surface_id,
                        "category":workspace.category,
                    }),
                )
            }
            Self::TabCreated {
                stream,
                pane,
                tab,
                surface,
                activate,
            } => {
                let model = after
                    .streams
                    .get(stream)
                    .ok_or_else(|| Rejection("created tab stream missing".into()))?;
                let pane_model = model
                    .panes
                    .get(pane)
                    .ok_or_else(|| Rejection("created tab pane missing".into()))?;
                let selected = if *activate {
                    Some(*tab)
                } else {
                    view.selected_tabs.get(pane).copied()
                };
                let active_tab = selected
                    .and_then(|selected| pane_model.tabs.iter().position(|id| *id == selected))
                    .unwrap_or(0);
                JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"pane_id":pane,"surface_id":surface,"tab_count":pane_model.tabs.len(),"active_tab":active_tab}),
                )
            }
            Self::WorkspaceUpdated {
                stream,
                id,
                display_index,
            } => {
                let workspace = after
                    .streams
                    .get(stream)
                    .and_then(|model| model.workspaces.get(id))
                    .ok_or_else(|| Rejection("updated workspace response target missing".into()))?;
                JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({
                        "id": id, "name": workspace.name, "subtitle": workspace.subtitle,
                        "description": workspace.description, "index": display_index,
                        "category": workspace.category,
                    }),
                )
            }
            Self::CategoryCreated { stream, id } => {
                let category = after
                    .streams
                    .get(stream)
                    .and_then(|model| model.categories.get(id))
                    .ok_or_else(|| Rejection("created category response target missing".into()))?;
                JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::json!({"id":id,"name":category.name}),
                )
            }
        };
        serde_json::to_vec(&response).map_err(|error| Rejection(error.to_string()))
    }

    pub(super) fn rejected(error: &Rejection) -> Vec<u8> {
        serde_json::to_vec(&JsonRpcResponse::invalid_params(
            serde_json::Value::Null,
            error.to_string(),
        ))
        .expect("error response serializes")
    }
}

/// The immutable input/template is distinct from both operation progress and the final wire body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RecordedResolution {
    pub version: u32,
    pub changes: Vec<super::StreamCommand>,
    pub response: Option<ResponsePlan>,
    #[serde(default)]
    pub completion_view: Option<CompletionView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResponseProgress {
    pub version: u32,
    pub results: Vec<tasty_core::StructuralResult>,
    /// Frozen at each operation's completion, never recomputed from a later live tree.
    pub replies: Vec<Option<JsonRpcResponse>>,
}

#[derive(Debug, Clone)]
pub(crate) struct OriginalResults {
    pub response: Option<ResponsePlan>,
    pub progress: ResponseProgress,
}

impl ResponseProgress {
    pub(crate) fn new(results: Vec<tasty_core::StructuralResult>) -> Self {
        Self {
            version: 1,
            replies: vec![None; results.len()],
            results,
        }
    }

    pub(crate) fn freeze(
        &mut self,
        plan: &ResponsePlan,
        model: &StructureModels,
        view: &CompletionView,
    ) -> Result<Option<Vec<u8>>, String> {
        use tasty_core::StructuralResult as R;
        let render = |plan: &ResponsePlan, result: Option<&R>| -> Result<JsonRpcResponse, String> {
            if let Some(R::Failed { reason }) = result {
                return Ok(JsonRpcResponse::internal_error(
                    serde_json::Value::Null,
                    reason,
                ));
            }
            if let (ResponsePlan::Moved { success, not_moved }, Some(R::Moved { moved })) =
                (plan, result)
            {
                return Ok(if *moved {
                    success.clone()
                } else {
                    not_moved.clone()
                });
            }
            if let (
                ResponsePlan::Closed {
                    success,
                    not_closed,
                },
                Some(R::Closed { closed }),
            ) = (plan, result)
            {
                return Ok(if *closed {
                    success.clone()
                } else {
                    not_closed.clone()
                });
            }
            let bytes = plan
                .render(model, view)
                .map_err(|error| error.to_string())?;
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())
        };
        if let ResponsePlan::Multiple(plans) = plan {
            if plans.len() != self.results.len() || self.replies.len() != self.results.len() {
                return Err(
                    "public result template differs from the original command arity".into(),
                );
            }
            for ((result, reply), plan) in self.results.iter().zip(&mut self.replies).zip(plans) {
                if reply.is_none() && !matches!(result, R::Pending { .. }) {
                    *reply = Some(render(plan, Some(result))?);
                }
            }
            if self.replies.iter().any(Option::is_none) {
                return Ok(None);
            }
            let replies: Vec<_> = self.replies.iter().flatten().collect();
            let response = if let Some(error) = replies.iter().find(|reply| reply.error.is_some()) {
                (*error).clone()
            } else {
                JsonRpcResponse::success(
                    serde_json::Value::Null,
                    serde_json::Value::Array(
                        replies
                            .into_iter()
                            .map(|reply| reply.result.clone().unwrap_or_default())
                            .collect(),
                    ),
                )
            };
            return serde_json::to_vec(&response)
                .map(Some)
                .map_err(|error| error.to_string());
        }
        if self
            .results
            .iter()
            .any(|result| matches!(result, R::Pending { .. }))
        {
            return Ok(None);
        }
        let failed = self
            .results
            .iter()
            .find(|result| matches!(result, R::Failed { .. }));
        serde_json::to_vec(&render(plan, failed.or_else(|| self.results.first()))?)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

impl OriginalResults {
    pub(crate) fn from_record(record: &tasty_event_store::CommandRecord) -> Result<Self, String> {
        let response = match serde_json::from_slice::<RecordedResolution>(&record.resolved) {
            Ok(resolution) if resolution.version == 1 => resolution.response,
            Ok(_) => return Err("unsupported command resolution version".into()),
            Err(_) => {
                // Older internal commands stored only fixed changes and a Vec of private results.
                serde_json::from_slice::<Vec<super::StreamCommand>>(&record.resolved)
                    .map_err(|error| error.to_string())?;
                None
            }
        };
        let bytes = record
            .response
            .as_deref()
            .ok_or("original operation progress missing")?;
        let progress = if response.is_some() {
            let progress: ResponseProgress =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            if progress.version != 1 {
                return Err("unsupported operation response progress version".into());
            }
            progress
        } else {
            ResponseProgress::new(serde_json::from_slice(bytes).map_err(|error| error.to_string())?)
        };
        Ok(Self { response, progress })
    }
}
