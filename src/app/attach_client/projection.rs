//! Build mirror descriptor trees and reserve IDs without borrowing an engine or resource owner.

#[cfg(test)]
mod tests;

use crate::model::{Pane, PaneNode, SplitDirection, SurfaceLayout, Tab, Workspace};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tasty_remote::client_session::MirrorStructureIds;

pub(super) type MirrorMarkdownLeaves = HashMap<u32, String>;

fn stable_mirror_id(
    ids: &mut HashMap<u32, u32>,
    remote: u32,
    next: impl FnOnce() -> anyhow::Result<u32>,
) -> anyhow::Result<u32> {
    if remote != 0
        && let Some(id) = ids.get(&remote)
    {
        return Ok(*id);
    }
    let id = next()?;
    if remote != 0 {
        ids.insert(remote, id);
    }
    Ok(id)
}

pub(super) fn mirror_id_needs(
    tree: &Value,
    surfaces: usize,
    new_workspace: bool,
) -> anyhow::Result<Vec<(tasty_core::IdKind, u32)>> {
    use tasty_core::IdKind;
    fn count(value: &Value) -> usize {
        match value {
            Value::Array(values) => values
                .iter()
                .fold(1usize, |sum, value| sum.saturating_add(count(value))),
            Value::Object(values) => values
                .values()
                .fold(1usize, |sum, value| sum.saturating_add(count(value))),
            _ => 1,
        }
    }
    // Upper bound includes malformed-tree fallback leaves and both legacy/recursive wire shapes.
    let nodes = count(tree)
        .checked_mul(2)
        .and_then(|nodes| nodes.checked_add(8))
        .ok_or_else(|| anyhow::anyhow!("mirror structure size overflow"))?;
    let needs = vec![
        (IdKind::Workspace, u32::from(new_workspace)),
        (IdKind::Pane, u32::try_from(nodes)?),
        (IdKind::Tab, u32::try_from(nodes)?),
        (
            IdKind::Surface,
            u32::try_from(
                nodes
                    .checked_add(surfaces)
                    .ok_or_else(|| anyhow::anyhow!("mirror surface count overflow"))?,
            )?,
        ),
    ];
    Ok(needs)
}
pub(super) fn lease_mirror_ids(
    ids: &crate::runtime::id_reservations::IdReservations,
    wake: &tasty_terminal::Waker,
    tree: &Value,
    surfaces: usize,
    new_workspace: bool,
) -> anyhow::Result<crate::runtime::id_reservations::ReservedIds> {
    let result = ids.lease(&mirror_id_needs(tree, surfaces, new_workspace)?);
    wake();
    result.map_err(anyhow::Error::new)
}

#[allow(clippy::too_many_arguments)] // reason: fixed mirror parser inputs
fn build_pane_from_json(
    structure_ids: &mut MirrorStructureIds,
    navigation: &mut crate::state::navigation::NavigationState,
    p: &Value,
    ids: &crate::runtime::id_reservations::ReservedIds,
    map: &HashMap<u32, u32>,
    term: &HashSet<u32>,
    mesh: &HashMap<u32, MirrorMeshInfo>,
    explorer: &HashMap<u32, std::path::PathBuf>,
    markdown: &mut MirrorMarkdownLeaves,
) -> anyhow::Result<Pane> {
    let mut tabs = Vec::new();
    let mut active = 0;
    for (index, value) in p
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let layout = match build_layout(
            navigation,
            value.get("layout").unwrap_or(&Value::Null),
            &MirrorLayoutSources {
                ids,
                map,
                term,
                mesh,
                explorer,
            },
            markdown,
        )? {
            Some(layout) => layout,
            None => SurfaceLayout::Leaf(crate::model::SurfaceDescriptor::new(
                ids.next_surface()?,
                "empty",
            )),
        };
        let remote = value.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
        let tab_id = stable_mirror_id(&mut structure_ids.remote_tabs, remote, || ids.next_tab())?;
        let focus = value
            .get("focused_surface")
            .and_then(Value::as_u64)
            .and_then(|id| map.get(&(id as u32)))
            .copied()
            .or_else(|| layout.first_surface_id())
            .unwrap_or(0);
        let tab = Tab {
            id: tab_id,
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(crate::i18n::t("attach.tab_title_fallback"))
                .into(),
            explicit_name: None,
            layout_opt: Some(layout),
        };
        if value
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            active = index;
        }
        navigation.initialize_surface(&tab, focus);
        tabs.push(tab);
    }
    if tabs.is_empty() {
        tabs.push(Tab {
            id: ids.next_tab()?,
            name: crate::i18n::t("attach.tab_title_fallback").into(),
            explicit_name: None,
            layout_opt: Some(SurfaceLayout::Leaf(crate::model::SurfaceDescriptor::new(
                ids.next_surface()?,
                "empty",
            ))),
        });
    }
    let remote = p.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let id = stable_mirror_id(&mut structure_ids.panes, remote, || ids.next_pane())?;
    let pane = Pane { id, tabs };
    navigation.initialize_tab(&pane, active);
    Ok(pane)
}

#[allow(clippy::too_many_arguments)] // reason: fixed recursive mirror parser inputs
fn build_pane_node(
    structure_ids: &mut MirrorStructureIds,
    navigation: &mut crate::state::navigation::NavigationState,
    node: &Value,
    ids: &crate::runtime::id_reservations::ReservedIds,
    map: &HashMap<u32, u32>,
    term: &HashSet<u32>,
    mesh: &HashMap<u32, MirrorMeshInfo>,
    explorer: &HashMap<u32, std::path::PathBuf>,
    markdown: &mut MirrorMarkdownLeaves,
    pane_id_map: &mut HashMap<u32, u32>,
) -> anyhow::Result<Option<PaneNode>> {
    Ok(match node.get("type").and_then(Value::as_str) {
        Some("Leaf") => {
            let Some(remote) = node.get("id").and_then(Value::as_u64) else {
                return Ok(None);
            };
            let pane = build_pane_from_json(
                structure_ids,
                navigation,
                node,
                ids,
                map,
                term,
                mesh,
                explorer,
                markdown,
            )?;
            pane_id_map.insert(remote as u32, pane.id);
            Some(PaneNode::Leaf(pane))
        }
        Some("Split") => {
            let (Some(first), Some(second)) = (node.get("first"), node.get("second")) else {
                return Ok(None);
            };
            let Some(first) = build_pane_node(
                structure_ids,
                navigation,
                first,
                ids,
                map,
                term,
                mesh,
                explorer,
                markdown,
                pane_id_map,
            )?
            else {
                return Ok(None);
            };
            let Some(second) = build_pane_node(
                structure_ids,
                navigation,
                second,
                ids,
                map,
                term,
                mesh,
                explorer,
                markdown,
                pane_id_map,
            )?
            else {
                return Ok(None);
            };
            Some(PaneNode::Split {
                direction: wire_direction(node),
                ratio: node.get("ratio").and_then(Value::as_f64).unwrap_or(0.5) as f32,
                first: Box::new(first),
                second: Box::new(second),
            })
        }
        _ => None,
    })
}
fn wire_direction(node: &Value) -> SplitDirection {
    if node.get("direction").and_then(Value::as_str) == Some("vertical") {
        SplitDirection::Vertical
    } else {
        SplitDirection::Horizontal
    }
}

#[allow(clippy::too_many_arguments)] // reason: mirror projection and reserved identity inputs
pub(super) fn build_mirror_workspace(
    structure_ids: &mut MirrorStructureIds,
    navigation: &mut crate::state::navigation::NavigationState,
    ws_id: u32,
    name: &str,
    tree: &Value,
    ids: &crate::runtime::id_reservations::ReservedIds,
    map: &HashMap<u32, u32>,
    term: &HashSet<u32>,
    mesh: &HashMap<u32, MirrorMeshInfo>,
    explorer: &HashMap<u32, std::path::PathBuf>,
    markdown: &mut MirrorMarkdownLeaves,
) -> anyhow::Result<Workspace> {
    let focused = tree
        .get("focused_pane")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let mut pane_map = HashMap::new();
    let mut layout = if let Some(node) = tree.get("pane_layout") {
        build_pane_node(
            structure_ids,
            navigation,
            node,
            ids,
            map,
            term,
            mesh,
            explorer,
            markdown,
            &mut pane_map,
        )?
    } else {
        None
    };
    if layout.is_none() {
        for value in tree
            .get("panes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let pane = build_pane_from_json(
                structure_ids,
                navigation,
                value,
                ids,
                map,
                term,
                mesh,
                explorer,
                markdown,
            )?;
            pane_map.insert(
                value.get("id").and_then(Value::as_u64).unwrap_or(0) as u32,
                pane.id,
            );
            layout = Some(match layout {
                None => PaneNode::Leaf(pane),
                Some(first) => PaneNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(PaneNode::Leaf(pane)),
                },
            });
        }
    }
    let layout = match layout {
        Some(layout) => layout,
        None => PaneNode::Leaf(Pane::new_with_surface(
            ids.next_pane()?,
            ids.next_tab()?,
            crate::i18n::t("attach.tab_title_fallback").into(),
            crate::model::SurfaceDescriptor::new(ids.next_surface()?, "empty"),
        )),
    };
    let focused = pane_map
        .get(&focused)
        .copied()
        .or_else(|| layout.first_pane().map(|pane| pane.id))
        .unwrap_or(0);
    let mut workspace = Workspace::from_restored(ws_id, name.into(), String::new(), layout);
    workspace.mirror = true;
    navigation.initialize_pane(&workspace, focused);
    structure_ids.retain_workspace(&workspace);
    Ok(workspace)
}

struct MirrorLayoutSources<'a> {
    ids: &'a crate::runtime::id_reservations::ReservedIds,
    map: &'a HashMap<u32, u32>,
    term: &'a HashSet<u32>,
    mesh: &'a HashMap<u32, MirrorMeshInfo>,
    explorer: &'a HashMap<u32, std::path::PathBuf>,
}
fn build_layout(
    navigation: &mut crate::state::navigation::NavigationState,
    node: &Value,
    sources: &MirrorLayoutSources<'_>,
    markdown: &mut MirrorMarkdownLeaves,
) -> anyhow::Result<Option<SurfaceLayout>> {
    Ok(match node.get("type").and_then(Value::as_str) {
        Some("Leaf") => {
            let Some(remote) = node.get("id").and_then(Value::as_u64) else {
                return Ok(None);
            };
            let local = match sources.map.get(&(remote as u32)) {
                Some(local) => *local,
                None => sources.ids.next_surface()?,
            };
            let kind = if sources.term.contains(&local) {
                "terminal"
            } else if let Some(info) = sources.mesh.get(&local) {
                info.kind.as_str()
            } else if sources.explorer.contains_key(&local) {
                "explorer"
            } else {
                markdown.get(&local).map_or("empty", String::as_str)
            };
            Some(SurfaceLayout::Leaf(crate::model::SurfaceDescriptor::new(
                local, kind,
            )))
        }
        Some("Split") => {
            let (Some(first), Some(second)) = (node.get("first"), node.get("second")) else {
                return Ok(None);
            };
            let Some(first) = build_layout(navigation, first, sources, markdown)? else {
                return Ok(None);
            };
            let Some(second) = build_layout(navigation, second, sources, markdown)? else {
                return Ok(None);
            };
            let node_id = crate::model::SplitNodeId::allocate();
            navigation.split_hints.insert(
                node_id,
                node.get("focus_second")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            Some(SurfaceLayout::Split {
                direction: wire_direction(node),
                ratio: node.get("ratio").and_then(Value::as_f64).unwrap_or(0.5) as f32,
                first: Box::new(first),
                second: Box::new(second),
                node_id,
            })
        }
        _ => None,
    })
}

/// mesh leaf를 AttachMeshSurface로 만들 때 필요한 표시 정보.
#[derive(Debug, Clone)]
pub(super) struct MirrorMeshInfo {
    pub(super) kind: String,
    pub(super) plugin_id: String,
    pub(super) display_name: String,
    /// 서버가 실은 `bed_focus`. 싣지 않는 옛 서버면 거짓(띠)이다.
    pub(super) bed_focus: bool,
}
