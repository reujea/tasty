//! Build the existing layout wire format from a fixed journal model and a separate View checkpoint.
//! This is a value export: the caller publishes its files only after the entire result is ready.
use super::SavedSurfaceSource;
use crate::core::layout_persistence::import::{ImportedView, surface_data::SurfaceData};
use crate::core::layout_persistence::schema::{
    SavedCategory, SavedLayout, SavedPane, SavedPaneNode, SavedSurface, SavedSurfaceLayout,
    SavedTab, SavedWorkspace,
};
use std::collections::BTreeMap;
use tasty_core::{JournalModel, SplitTree};
use tasty_event_store::{EventStore, PayloadRef};

pub(crate) struct LegacyExport {
    pub(crate) layout: SavedLayout,
    /// Fresh immutable legacy IDs. No runtime persist_id is overwritten by export preparation.
    #[expect(
        dead_code,
        reason = "Internal content export returns immutable legacy blobs; journal import copies DataRefs instead"
    )]
    pub(crate) scrollback: BTreeMap<String, Vec<u8>>,
}
pub(crate) fn capture(
    store: &EventStore,
    model: &JournalModel,
    view: &ImportedView,
    include_content: bool,
) -> Result<LegacyExport, String> {
    if model.engine_retired {
        return Err("cannot export a retired engine".into());
    }
    let mut exporter = Exporter {
        store,
        model,
        view,
        include_content,
        scrollback: BTreeMap::new(),
    };
    let workspaces = model
        .workspace_order
        .iter()
        .map(|id| {
            let workspace = model
                .workspaces
                .get(id)
                .ok_or("export workspace order is inconsistent")?;
            Ok(SavedWorkspace {
                name: workspace.name.clone(),
                subtitle: workspace.subtitle.clone(),
                description: workspace.description.clone(),
                pane_layout: exporter.panes(&workspace.layout)?,
                focused_pane_index: workspace
                    .layout
                    .leaves()
                    .iter()
                    .position(|pane| view.focused_panes.get(id) == Some(pane))
                    .unwrap_or(0),
                category: workspace.category,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let active_workspace = model
        .workspace_order
        .iter()
        .position(|id| Some(*id) == view.active_workspace)
        .unwrap_or(0)
        .min(workspaces.len().saturating_sub(1));
    let categories = model
        .category_order
        .iter()
        .map(|id| {
            let category = model
                .categories
                .get(id)
                .ok_or("export category order is inconsistent")?;
            Ok(SavedCategory {
                id: *id,
                name: category.name.clone(),
                collapsed: view.collapsed_categories.contains(id),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(LegacyExport {
        layout: SavedLayout {
            version: crate::core::layout_persistence::LAYOUT_VERSION,
            workspaces,
            active_workspace,
            categories,
        },
        scrollback: exporter.scrollback,
    })
}
struct Exporter<'a> {
    store: &'a EventStore,
    model: &'a JournalModel,
    view: &'a ImportedView,
    include_content: bool,
    scrollback: BTreeMap<String, Vec<u8>>,
}
impl Exporter<'_> {
    fn panes(&mut self, tree: &SplitTree<u32>) -> Result<SavedPaneNode, String> {
        Ok(match tree {
            SplitTree::Leaf(id) => {
                let pane = self.model.panes.get(id).ok_or("export pane missing")?;
                let active_tab = pane
                    .tabs
                    .iter()
                    .position(|tab| self.view.active_tabs.get(id) == Some(tab))
                    .unwrap_or(0);
                let tabs = pane
                    .tabs
                    .iter()
                    .map(|id| {
                        let tab = self.model.tabs.get(id).ok_or("export tab missing")?;
                        Ok(SavedTab {
                            name: tab.name.clone(),
                            explicit_name: tab.explicit_name.clone(),
                            surface: self.surfaces(&tab.layout)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                SavedPaneNode::Leaf(SavedPane { tabs, active_tab })
            }
            SplitTree::Split {
                direction,
                ratio,
                first,
                second,
            } => SavedPaneNode::Split {
                direction: (*direction).into(),
                ratio: ratio.to_f32(),
                first: Box::new(self.panes(first)?),
                second: Box::new(self.panes(second)?),
            },
        })
    }
    fn surfaces(&mut self, tree: &SplitTree<u32>) -> Result<SavedSurfaceLayout, String> {
        Ok(match tree {
            SplitTree::Leaf(id) => {
                let surface = self
                    .model
                    .surfaces
                    .get(id)
                    .ok_or("export surface missing")?;
                SavedSurfaceLayout::Leaf(self.surface(&SavedSurfaceSource {
                    kind: surface.kind.clone(),
                    data: surface.data,
                    creation_seed: surface.creation_seed,
                })?)
            }
            SplitTree::Split {
                direction,
                ratio,
                first,
                second,
            } => SavedSurfaceLayout::Split {
                direction: (*direction).into(),
                ratio: ratio.to_f32(),
                first: Box::new(self.surfaces(first)?),
                second: Box::new(self.surfaces(second)?),
            },
        })
    }
    fn surface(&mut self, source: &SavedSurfaceSource) -> Result<SavedSurface, String> {
        if let Some(reference) = source.data {
            let bytes = self
                .store
                .read_payload(PayloadRef(reference.0))
                .map_err(|error| error.to_string())?;
            return match SurfaceData::decode(&bytes).map_err(|error| error.to_string())? {
                SurfaceData::Generic { data, .. } if source.kind != "terminal" => {
                    Ok(SavedSurface::Generic {
                        kind: source.kind.clone(),
                        data,
                    })
                }
                SurfaceData::Terminal {
                    cwd,
                    restore_command,
                    scrollback_ref,
                    scrollback,
                } if source.kind == "terminal" => {
                    let scrollback_ref = if !self.include_content {
                        None
                    } else if let Some(bytes) = scrollback {
                        let id = crate::scrollback_store::new_persist_id();
                        self.scrollback.insert(id.clone(), bytes);
                        Some(id)
                    } else {
                        scrollback_ref
                    };
                    Ok(SavedSurface::Terminal {
                        cwd,
                        restore_command,
                        scrollback_ref,
                    })
                }
                _ => Err("export capture kind differs from the committed surface".into()),
            };
        }
        let seed = source
            .creation_seed
            .map(|reference| {
                let bytes = self
                    .store
                    .read_payload(PayloadRef(reference.0))
                    .map_err(|error| error.to_string())?;
                serde_json::from_slice::<crate::runtime::journal_product::PreparationInput>(&bytes)
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        if seed.as_ref().is_some_and(|seed| seed.kind != source.kind) {
            return Err("export creation seed belongs to another kind".into());
        }
        if source.kind == "terminal" {
            Ok(SavedSurface::Terminal {
                cwd: seed
                    .as_ref()
                    .and_then(|seed| seed.cwd.as_ref())
                    .map(|cwd| cwd.to_string_lossy().into_owned()),
                restore_command: seed
                    .and_then(|seed| seed.shell)
                    .and_then(|shell| shell.restore_command),
                scrollback_ref: None,
            })
        } else {
            Ok(SavedSurface::Generic {
                kind: source.kind.clone(),
                data: seed
                    .map(|seed| seed.restore.unwrap_or(seed.params))
                    .unwrap_or_else(|| serde_json::json!({})),
            })
        }
    }
}
