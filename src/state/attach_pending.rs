//! View-only attach admission and layout drafts. No domain IDs or resources are allocated.
use super::navigation::NavigationState;
use crate::core::CoreState;
use crate::ipc::stream::{SplitAxis, StructuralOp};
use crate::model::{BinaryTree, PhysicalRect, SplitDirection};
use std::sync::{Arc, Weak};
use tasty_remote::{connection::ConnectionEpoch, pending_request::PendingRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PendingId(pub u64);
mod creation_match;
pub(crate) mod deferred;
pub(crate) mod waits;
type ProjectedRects = (Vec<(u32, PhysicalRect)>, Vec<(PendingId, PhysicalRect)>);
#[derive(Clone, Copy, Debug)]
pub(crate) enum Creation {
    Tab,
    Surface(SplitDirection),
    Pane(SplitDirection),
}
pub(crate) struct Entry {
    pub id: PendingId,
    pub workspace: u32,
    pub pane: u32,
    pub tab: u32,
    pub anchor: u32,
    pub kind: String,
    pub creation: Option<Creation>,
    pub receipt: Arc<PendingRequest>,
    pub epoch: ConnectionEpoch,
    pub view: Weak<()>,
    selection: Weak<()>,
    pub visible: bool,
    baseline: Option<creation_match::CreationBaseline>,
    pub wait_kind: waits::WaitKind,
}
impl Entry {
    fn live(&self, core: &CoreState) -> bool {
        !self.receipt.finished()
            && self.epoch.is_active()
            && if self.creation.is_some() {
                core.has_surface(self.anchor)
            } else {
                core.has_workspace(self.workspace)
            }
    }
    fn drop_reason(&self) -> &'static str {
        if self.receipt.finished() {
            "the request ended without a created surface"
        } else if !self.epoch.is_active() {
            "the connection epoch ended"
        } else {
            "the anchor surface is gone"
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryConsumer {
    Git(u64),
    Markdown(u32),
}
pub(crate) struct Query {
    pub consumer: QueryConsumer,
    pub request: u64,
    pub workspace: u32,
    pub admitted: std::time::Instant,
    pub epoch: ConnectionEpoch,
    pub target: crate::runtime::surface_binding::SurfaceBinding,
}
#[derive(Default)]
pub(crate) struct AttachPending {
    next: u64,
    pub selection_serial: u64,
    pub entries: Vec<Entry>,
    pub queries: Vec<Query>,
    selected: Option<(PendingId, Weak<()>)>,
    pub deferred: deferred::DeferredCommands,
}
impl AttachPending {
    pub fn begin(
        &mut self,
        core: &CoreState,
        navigation: &NavigationState,
        op: &StructuralOp,
        epoch: ConnectionEpoch,
        view: Weak<()>,
        admitted: std::time::Instant,
    ) -> Option<Arc<PendingRequest>> {
        let anchor = op.anchor_surface_id();
        let (index, pane) = core.find_workspace_index_for_surface(anchor)?;
        let workspace = core.workspace_at(index)?;
        let pane_model = workspace.pane_layout().find_pane(pane)?;
        // The wire representative is not necessarily the user's selected tab.
        let tab = match op {
            StructuralOp::SplitSurface { .. } => core.find_tab_for_surface(anchor)?,
            _ => pane_model.tabs.get(navigation.tab_index(pane_model))?.id,
        };
        let axis = |axis: &SplitAxis| match axis {
            SplitAxis::Horizontal => SplitDirection::Horizontal,
            SplitAxis::Vertical => SplitDirection::Vertical,
        };
        let (creation, kind) = match op {
            StructuralOp::NewTab { surface_kind, .. } => {
                (Some(Creation::Tab), surface_kind.clone())
            }
            StructuralOp::SplitSurface {
                direction,
                surface_kind,
                ..
            } => (
                Some(Creation::Surface(axis(direction))),
                surface_kind.clone(),
            ),
            StructuralOp::SplitPane {
                direction,
                surface_kind,
                ..
            } => (Some(Creation::Pane(axis(direction))), surface_kind.clone()),
            _ => (None, String::new()),
        };
        self.next = self.next.checked_add(1)?;
        let id = PendingId(self.next);
        let receipt = Arc::new(PendingRequest::new(admitted));
        if creation.is_some() {
            self.selection_serial = self.selection_serial.wrapping_add(1);
            self.selected = Some((id, navigation.generation()));
        }
        self.entries.push(Entry {
            id,
            workspace: workspace.id,
            pane,
            tab,
            anchor,
            kind,
            creation,
            receipt: receipt.clone(),
            epoch,
            view,
            selection: navigation.generation(),
            visible: true,
            baseline: creation.map(|_| creation_match::CreationBaseline::capture(workspace)),
            wait_kind: waits::WaitKind::from_op(op),
        });
        Some(receipt)
    }
    pub fn selected(&self, core: &CoreState, navigation: &NavigationState) -> Option<&Entry> {
        let (id, generation) = self.selected.as_ref()?;
        if !navigation.matches_generation(generation) {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| entry.id == *id && entry.live(core) && entry.view.strong_count() > 0)
    }
    pub fn select(&mut self, id: PendingId, navigation: &NavigationState) {
        self.selection_serial = self.selection_serial.wrapping_add(1);
        self.selected = Some((id, navigation.generation()));
    }
    pub fn deselect(&mut self) {
        if self.selected.take().is_some() {
            self.selection_serial = self.selection_serial.wrapping_add(1);
        }
    }
    pub fn retain(&mut self, core: &CoreState) {
        let deferred = &mut self.deferred;
        self.entries.retain(|entry| {
            let live = entry.live(core);
            if !live {
                deferred.dropped(entry.id, entry.drop_reason());
            }
            live
        });
    }
    pub fn permits_legacy_focus(
        &self,
        epoch: &ConnectionEpoch,
        op: u64,
        navigation: &NavigationState,
        view: Option<&Weak<()>>,
    ) -> bool {
        self.entries
            .iter()
            .find(|entry| entry.epoch.same(epoch) && entry.receipt.op() == Some(op))
            .is_none_or(|entry| {
                navigation.matches_generation(&entry.selection)
                    && view.is_some_and(|view| view.ptr_eq(&entry.view))
            })
    }
    pub fn query_result(&mut self, epoch: &ConnectionEpoch, request: u64, git: bool) {
        self.queries.retain(|query| {
            !(query.epoch.same(epoch)
                && query.request == request
                && matches!(query.consumer, QueryConsumer::Git(_)) == git)
        });
    }
    pub fn result(&self, epoch: &ConnectionEpoch, op: u64, created: Option<u32>, ok: bool) {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.epoch.same(epoch) && entry.receipt.op() == Some(op))
        {
            if ok {
                entry.receipt.result(created);
            } else {
                entry.receipt.finish();
            }
        }
    }
    /// A correlated creation may already be present in an earlier merged snapshot.
    pub fn applied_before_result(
        &mut self,
        epoch: &ConnectionEpoch,
        op: u64,
        navigation: &NavigationState,
        view: Option<&Weak<()>>,
        mapping: &std::collections::HashMap<u32, u32>,
    ) -> Option<u32> {
        let entry = self.entries.iter().find(|entry| {
            entry.epoch.same(epoch)
                && entry.receipt.op() == Some(op)
                && entry.creation.is_some()
                && entry.receipt.has_result()
        })?;
        let local = *mapping.get(&entry.receipt.created()?)?;
        self.deferred.created(entry.id, local);
        let selected = self.selected.as_ref().is_some_and(|(id, generation)| {
            *id == entry.id && navigation.matches_generation(generation)
        });
        entry.receipt.finish();
        (selected && view.is_some_and(|view| view.ptr_eq(&entry.view))).then_some(local)
    }

    /// Called only after the snapshot and its ID reservation have actually applied.
    /// Unknown merged results collapse drafts without guessing which resource to focus.
    pub fn applied(
        &mut self,
        core: &CoreState,
        navigation: &NavigationState,
        epoch: &ConnectionEpoch,
        view: Option<&Weak<()>>,
        mapping: &std::collections::HashMap<u32, u32>,
        new_remote: &[u32],
    ) -> Option<u32> {
        let selected = self
            .selected
            .as_ref()
            .filter(|(_, generation)| navigation.matches_generation(generation))
            .map(|(id, _)| *id);
        let evidence = creation_match::plans(&self.entries, core, epoch, mapping, new_remote);
        let mut focus = None;
        for (entry, evidence) in self.entries.iter_mut().zip(evidence) {
            if !entry.epoch.same(epoch)
                || entry.receipt.finished()
                || !core.has_workspace(entry.workspace)
            {
                continue;
            }
            if entry.receipt.has_result() {
                let local = match entry.receipt.created() {
                    Some(id) => mapping.get(&id).copied(),
                    None => evidence.target,
                };
                if let (Some(_), Some(local)) = (entry.creation, local) {
                    self.deferred.created(entry.id, local);
                }
                if entry.creation.is_none() || local.is_some() {
                    if selected == Some(entry.id)
                        && view.is_some_and(|view| view.ptr_eq(&entry.view))
                    {
                        focus = local;
                    }
                    entry.receipt.finish();
                }
            }
            if entry.creation.is_some() && evidence.overlap && entry.visible {
                entry.visible = false;
                // The draft no longer names one surface, so commands bound to it cannot run.
                self.deferred
                    .dropped(entry.id, "the merged result does not name one surface");
            }
        }
        if self
            .entries
            .iter()
            .any(|entry| selected == Some(entry.id) && !entry.visible)
        {
            // A folded draft has no input target. Keep the existing real selection usable.
            self.deselect();
        }
        focus
    }
    pub fn visible<'a>(
        &'a self,
        core: &'a CoreState,
        workspace: u32,
    ) -> impl Iterator<Item = &'a Entry> {
        self.entries.iter().filter(move |entry| {
            entry.workspace == workspace
                && entry.visible
                && entry.live(core)
                && entry.view.strong_count() > 0
        })
    }
    pub fn clips_surface(&self, core: &CoreState, surface: u32) -> bool {
        self.entries
            .iter()
            .filter(|entry| entry.visible && entry.live(core))
            .any(|entry| match entry.creation {
                Some(Creation::Surface(_)) => entry.anchor == surface,
                Some(Creation::Pane(_)) => core
                    .find_workspace_index_for_surface(surface)
                    .is_some_and(|(_, pane)| pane == entry.pane),
                _ => false,
            })
    }
    pub fn pane_geometry(
        &self,
        core: &CoreState,
        workspace: u32,
        mut real: Vec<(u32, PhysicalRect)>,
        scale: f32,
    ) -> ProjectedRects {
        let mut pending = Vec::new();
        for entry in self.visible(core, workspace) {
            if let Some(Creation::Pane(direction)) = entry.creation
                && let Some((_, rect)) = real.iter_mut().find(|(id, _)| *id == entry.pane)
            {
                let (old, new) = rect.split_with_gap(
                    direction,
                    0.5,
                    crate::model::PaneNode::border_width(scale),
                );
                *rect = old;
                pending.push((entry.id, new));
            }
        }
        (real, pending)
    }
    pub fn surface_geometry(
        &self,
        core: &CoreState,
        tab: u32,
        mut real: Vec<(u32, PhysicalRect)>,
        scale: f32,
    ) -> ProjectedRects {
        let mut pending = Vec::new();
        for entry in self.entries.iter().filter(|entry| {
            entry.tab == tab && entry.visible && entry.live(core) && entry.view.strong_count() > 0
        }) {
            if let Some(Creation::Surface(direction)) = entry.creation
                && let Some((_, rect)) = real.iter_mut().find(|(id, _)| *id == entry.anchor)
            {
                let (old, new) = rect.split_with_gap(
                    direction,
                    0.5,
                    crate::model::SurfaceLayout::border_width(scale),
                );
                *rect = old;
                pending.push((entry.id, new));
            }
        }
        (real, pending)
    }
}

impl super::MainViewState {
    pub(crate) fn pending_regions(
        &self,
        core: &CoreState,
        rect: PhysicalRect,
        scale: f32,
    ) -> Vec<(PendingId, PhysicalRect, bool)> {
        let ws = self.active_workspace(core);
        let (panes, pending_panes) = self.attach_pending.pane_geometry(
            core,
            ws.id,
            self.committed_pane_rects(core, ws, rect, scale),
            scale,
        );
        let content = |rect: PhysicalRect| PhysicalRect {
            y: rect.y + self.tab_bar_height,
            height: (rect.height - self.tab_bar_height).max(crate::model::PhysicalPx(0.0)),
            ..rect
        };
        let mut result: Vec<_> = pending_panes
            .into_iter()
            .map(|(id, rect)| (id, rect, true))
            .collect();
        for (pane, rect) in panes {
            if let Some(entry) = self.attach_pending.selected(core, &self.navigation)
                && entry.visible
                && entry.pane == pane
                && matches!(entry.creation, Some(Creation::Tab))
            {
                result.push((entry.id, content(rect), false));
                continue;
            }
            let Some(pane) = ws.pane_layout().find_pane(pane) else {
                continue;
            };
            let Some(tab) = pane.tabs.get(self.navigation.tab_index(pane)) else {
                continue;
            };
            let regions = super::layout_preview::surface_regions(
                Some(&self.layout_previews),
                core,
                tab,
                content(rect),
                scale,
            );
            let (_, pending) = self.attach_pending.surface_geometry(
                core,
                tab.id,
                regions
                    .iter()
                    .map(|region| (region.id, region.rect))
                    .collect(),
                scale,
            );
            result.extend(pending.into_iter().map(|(id, rect)| (id, rect, false)));
        }
        result
    }
}

#[cfg(test)]
mod draft_action_tests;
#[cfg(test)]
mod tests;
