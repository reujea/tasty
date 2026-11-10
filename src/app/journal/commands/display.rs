//! Volatile mirror annotations follow the durable result, but never enter the local domain tree.
use super::*;
use std::sync::Weak;

pub(super) struct DisplayContinuation {
    pub engine: EngineId,
    pub mirrors: Vec<(u32, Weak<()>)>,
    pub action: DisplayAction,
}

pub(super) enum DisplayAction {
    Workspace {
        id: u32,
        user_direct: bool,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
        category: Option<u32>,
    },
    Order(Vec<u32>),
    TabName {
        tab_id: u32,
        name: Option<String>,
        notify: Option<bool>,
    },
}

impl DisplayContinuation {
    pub fn apply(
        self,
        sessions: &mut [&mut EngineSession],
    ) -> Option<(EngineId, Option<super::notification::Notification>)> {
        let session = sessions
            .iter_mut()
            .find(|session| session.id == self.engine)?;
        let core = &mut session.core_state;
        // Delta and reconnect both replace this token even when all numeric IDs survive.
        if !self
            .mirrors
            .iter()
            .all(|(id, token)| core.matches_mirror_projection(*id, token))
        {
            return None;
        }
        let mut host_event = None;
        match self.action {
            DisplayAction::Workspace {
                id,
                user_direct,
                name,
                subtitle,
                description,
                category,
            } => {
                let category = category.filter(|id| core.category_index(*id).is_some());
                if !core.has_workspace(id) {
                    return None;
                }
                if name.is_some() || subtitle.is_some() || description.is_some() {
                    host_event = Some(super::notification::Notification::Ready(
                        crate::core::host_event::PendingHostEvent::WorkspaceRenamed {
                            workspace_id: id,
                            name: name.clone(),
                            subtitle: subtitle.clone(),
                            description: description.clone(),
                            user_direct,
                        },
                    ));
                }
                // Local metadata already came from the committed projection. Only the remote
                // display annotation has a volatile write at this boundary.
                if let Some(workspace) = core.mirror_workspace_mut(id) {
                    if let Some(name) = name {
                        workspace.name = name;
                    }
                    if let Some(subtitle) = subtitle {
                        workspace.subtitle = subtitle;
                    }
                    if let Some(description) = description {
                        workspace.description = description;
                    }
                    if let Some(category) = category {
                        workspace.category = category;
                    }
                }
            }
            DisplayAction::TabName {
                tab_id,
                name,
                notify,
            } => {
                let pane_id = core.find_pane_for_tab(tab_id)?;
                let workspace = core
                    .find_workspace_index_for_pane(pane_id)
                    .and_then(|index| core.workspace_at(index))
                    .map(|workspace| (workspace.id, workspace.mirror))?;
                if workspace.1 {
                    let tab = core
                        .mirror_workspace_mut(workspace.0)?
                        .pane_layout_mut()
                        .find_pane_mut(pane_id)?
                        .tabs
                        .iter_mut()
                        .find(|tab| tab.id == tab_id)?;
                    tab.explicit_name = name;
                }
                if let Some(user_direct) = notify {
                    host_event = Some(super::notification::Notification::TabName {
                        tab_id,
                        user_direct,
                    });
                }
            }
            DisplayAction::Order(order) => {
                if !core.apply_workspace_display_order(order) {
                    return None;
                }
            }
        }
        Some((self.engine, host_event))
    }

    pub fn weight(&self) -> usize {
        self.mirrors.len() * std::mem::size_of::<(u32, Weak<()>)>()
            + match &self.action {
                DisplayAction::TabName { name, .. } => name.as_ref().map_or(0, String::len),
                DisplayAction::Order(order) => order.len() * std::mem::size_of::<u32>(),
                DisplayAction::Workspace {
                    name,
                    subtitle,
                    description,
                    ..
                } => {
                    name.as_ref().map_or(0, String::len)
                        + subtitle.as_ref().map_or(0, String::len)
                        + description.as_ref().map_or(0, String::len)
                }
            }
    }
}
