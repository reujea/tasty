//! Convert a preset into fixed values. This code never creates a kind, PTY, or live tree.
use crate::intent::ClonedPreset;
use crate::runtime::engine_access::EngineRef;
use crate::runtime::journal_product::{PreparationInput, ShellRecipe};
use std::collections::BTreeMap;
use tasty_core::{
    AssemblyDestination, ClosedSnapshot, CreationAssembly, EntityId, IdKind, Pane, Ratio,
    SplitTree, Surface, Tab, Workspace,
};
use tasty_presets::{PresetPane, PresetPaneNode, PresetSplitDirection, PresetSurfaceLayout};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct AssemblyDraft {
    pub snapshot: ClosedSnapshot,
    pub destination: AssemblyDestination,
    pub inputs: BTreeMap<u32, PreparationInput>,
    pub omit_failed: bool,
}
impl AssemblyDraft {
    pub fn validate(&self) -> Result<(), String> {
        CreationAssembly {
            snapshot: self.snapshot.clone(),
            destination: self.destination.clone(),
            inputs: Default::default(),
            undo: None,
            omit_failed: self.omit_failed,
        }
        .validate_graph()
        .map_err(|error| error.to_string())
    }
}
struct Builder<'a, 'engine> {
    engine: &'a EngineRef<'engine>,
    snapshot: ClosedSnapshot,
    inputs: BTreeMap<u32, PreparationInput>,
    next: u32,
}
impl Builder<'_, '_> {
    fn label(&mut self) -> Result<u32, String> {
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or("preset graph labels exhausted")?;
        Ok(id)
    }
    fn layout(&mut self, tab: u32, layout: &PresetSurfaceLayout) -> Result<SplitTree<u32>, String> {
        Ok(match layout {
            PresetSurfaceLayout::Split {
                direction,
                ratio,
                first,
                second,
            } => SplitTree::Split {
                direction: direction_of(*direction),
                ratio: preset_ratio(*ratio)?,
                first: Box::new(self.layout(tab, first)?),
                second: Box::new(self.layout(tab, second)?),
            },
            PresetSurfaceLayout::Leaf { surface } => {
                let id = self.label()?;
                let mut params = surface.params.clone();
                let definition = self.engine.runtime.surface_registry.get_live(&surface.kind);
                let mut cwd = surface.cwd.as_ref().map(std::path::PathBuf::from);
                if let Some(definition) = definition.as_ref() {
                    definition.normalize_param_aliases(&mut params);
                    self.engine
                        .apply_kind_default_params(definition, &mut params, None);
                    if cwd.is_none() {
                        cwd = crate::runtime::surface_registry::PresetFieldSpec::derive_cwd(
                            &definition.preset_fields,
                            &params,
                        );
                    }
                }
                let shell = if surface.kind == "terminal" {
                    let current = crate::core::state::ShellConfig::from_settings(
                        &self.engine.runtime.settings,
                    );
                    let mut initial = String::new();
                    if let Some(directory) = &surface.cwd {
                        initial.push_str(&format!("cd {}\r", shell_escape(directory)));
                    }
                    if let Some(command) = surface
                        .startup_command
                        .as_deref()
                        .map(str::trim)
                        .filter(|command| !command.is_empty())
                    {
                        initial.push_str(command);
                        initial.push('\r');
                    }
                    cwd = None;
                    Some(ShellRecipe {
                        executable: current.shell,
                        arguments: current.args,
                        environment: current.envs,
                        cols: self.engine.runtime.default_cols,
                        rows: self.engine.runtime.default_rows,
                        scrollback_lines: self.engine.runtime.settings.general.scrollback_lines,
                        disk_scrollback: self
                            .engine
                            .runtime
                            .settings
                            .performance
                            .scrollback_disk_swap,
                        disk_scrollback_lines: self
                            .engine
                            .runtime
                            .settings
                            .performance
                            .scrollback_disk_cap(),
                        startup_command: initial,
                        restore_command: None,
                    })
                } else {
                    None
                };
                self.inputs.insert(
                    id,
                    PreparationInput {
                        adopt: None,
                        child: None,
                        kind: surface.kind.clone(),
                        cwd,
                        params,
                        shell,
                        restore: None,
                        discard_capture: false,
                    },
                );
                self.snapshot.surfaces.insert(
                    id,
                    Surface {
                        tab,
                        kind: surface.kind.clone(),
                        data: None,
                        creation_seed: None,
                        metadata: Default::default(),
                        activation: None,
                        content_generation: 0,
                        snapshot_schema: 0,
                    },
                );
                SplitTree::Leaf(id)
            }
        })
    }
    fn tab(&mut self, pane: u32, preset: &tasty_presets::PresetTab) -> Result<u32, String> {
        let id = self.label()?;
        let layout = self.layout(id, &preset.layout)?;
        let first = first_surface(&preset.layout);
        let name = preset.explicit_name.clone().unwrap_or_else(|| {
            crate::runtime::surface_registry::default_tab_name_for_kind(
                &first.kind,
                first.cwd.as_deref().map(std::path::Path::new),
                &first.params,
                self.engine
                    .runtime
                    .surface_registry
                    .get_live(&first.kind)
                    .as_deref(),
            )
        });
        if let Some(surface) = layout.leaves().first() {
            self.snapshot
                .presentation
                .selected_surfaces
                .insert(id, *surface);
        }
        self.snapshot.tabs.insert(
            id,
            Tab {
                pane,
                name,
                explicit_name: preset.explicit_name.clone(),
                layout,
            },
        );
        Ok(id)
    }
    fn pane(&mut self, workspace: u32, preset: &PresetPane) -> Result<u32, String> {
        if preset.tabs.is_empty() {
            return Err("preset pane has no tabs".into());
        }
        let id = self.label()?;
        let tabs = preset
            .tabs
            .iter()
            .map(|tab| self.tab(id, tab))
            .collect::<Result<Vec<_>, _>>()?;
        let selected = tabs[preset.active_tab.min(tabs.len() - 1)];
        self.snapshot
            .presentation
            .selected_tabs
            .insert(id, selected);
        self.snapshot.panes.insert(id, Pane { workspace, tabs });
        Ok(id)
    }
    fn panes(&mut self, workspace: u32, node: &PresetPaneNode) -> Result<SplitTree<u32>, String> {
        Ok(match node {
            PresetPaneNode::Leaf { pane } => SplitTree::Leaf(self.pane(workspace, pane)?),
            PresetPaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => SplitTree::Split {
                direction: direction_of(*direction),
                ratio: preset_ratio(*ratio)?,
                first: Box::new(self.panes(workspace, first)?),
                second: Box::new(self.panes(workspace, second)?),
            },
        })
    }
}
pub(crate) fn draft(
    engine: &EngineRef<'_>,
    preset: &ClonedPreset,
    target_pane: Option<u32>,
    category: Option<u32>,
) -> Result<AssemblyDraft, String> {
    let mut build = Builder {
        engine,
        next: 1,
        inputs: Default::default(),
        snapshot: ClosedSnapshot {
            version: 1,
            root: EntityId {
                kind: IdKind::Workspace,
                id: 0,
            },
            origin_workspace: None,
            workspaces: Default::default(),
            panes: Default::default(),
            tabs: Default::default(),
            surfaces: Default::default(),
            tab_name: None,
            pane_position: None,
            presentation: Default::default(),
        },
    };
    let destination = match preset {
        ClonedPreset::Workspace(preset) => {
            let id = build.label()?;
            let layout = build.panes(id, &preset.layout)?;
            if let Some(pane) = layout.leaves().first() {
                build.snapshot.presentation.focused_panes.insert(id, *pane);
            }
            build.snapshot.workspaces.insert(
                id,
                Workspace {
                    name: if preset.name.is_empty() {
                        format!("Workspace {}", engine.workspaces().len() + 1)
                    } else {
                        preset.name.clone()
                    },
                    category: category
                        .filter(|category| {
                            engine
                                .categories()
                                .iter()
                                .any(|value| value.id == *category)
                        })
                        .unwrap_or(0),
                    subtitle: preset.subtitle.clone(),
                    description: preset.description.clone(),
                    metadata: Default::default(),
                    layout,
                },
            );
            build.snapshot.root = EntityId {
                kind: IdKind::Workspace,
                id,
            };
            AssemblyDestination::Workspace
        }
        ClonedPreset::Tab(preset) => {
            let pane = target_pane
                .and_then(|id| engine.find_pane_by_id(id))
                .ok_or("preset target pane is missing")?;
            let id = build.tab(pane.id, &preset.tab)?;
            build.snapshot.root = EntityId {
                kind: IdKind::Tab,
                id,
            };
            AssemblyDestination::Tab {
                pane: pane.id,
                index: pane.tabs.len(),
            }
        }
        ClonedPreset::Pane(preset) => {
            let target = target_pane.ok_or("preset target pane is missing")?;
            let workspace = engine
                .find_workspace_index_for_pane(target)
                .and_then(|index| engine.workspace_at(index))
                .ok_or("preset target workspace is missing")?;
            let id = build.pane(workspace.id, &preset.pane)?;
            build.snapshot.root = EntityId {
                kind: IdKind::Pane,
                id,
            };
            AssemblyDestination::Pane {
                target,
                split: tasty_core::SplitSpec {
                    direction: tasty_model::SplitDirection::Vertical,
                    ratio: Ratio::from_f32(0.5),
                    placement: tasty_core::Placement::After,
                },
            }
        }
    };
    let draft = AssemblyDraft {
        snapshot: build.snapshot,
        destination,
        inputs: build.inputs,
        omit_failed: false,
    };
    draft.validate()?;
    Ok(draft)
}
fn first_surface(layout: &PresetSurfaceLayout) -> &tasty_presets::PresetSurface {
    match layout {
        PresetSurfaceLayout::Leaf { surface } => surface,
        PresetSurfaceLayout::Split { first, .. } => first_surface(first),
    }
}
fn direction_of(direction: PresetSplitDirection) -> tasty_model::SplitDirection {
    match direction {
        PresetSplitDirection::Horizontal => tasty_model::SplitDirection::Horizontal,
        PresetSplitDirection::Vertical => tasty_model::SplitDirection::Vertical,
    }
}
fn preset_ratio(ratio: f32) -> Result<Ratio, String> {
    if !ratio.is_finite() {
        return Err("preset ratio is not finite".into());
    }
    Ok(Ratio::from_f32(ratio.clamp(0.05, 0.95)))
}
fn shell_escape(value: &str) -> String {
    if value.contains([' ', '\'', '"']) {
        format!("'{}'", value.replace('\'', "'\\''"))
    } else {
        value.to_owned()
    }
}
