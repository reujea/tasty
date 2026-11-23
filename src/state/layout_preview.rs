//! Disposable View ratios. The committed tree and all resource owners remain borrowed.
use crate::core::CoreState;
use crate::model::{BinaryTree, DividerInfo, PhysicalRect, SplitDirection};
use std::sync::Weak;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutTarget {
    Workspace(u32),
    Tab(u32),
}

#[derive(Debug, Clone)]
pub(crate) struct DividerCommit {
    pub sequence: u64,
    pub target: LayoutTarget,
    pub path: Vec<bool>,
    pub leaves: Vec<u32>,
    pub revision: u64,
    pub ratio: f32,
}

#[derive(Clone)]
struct Preview {
    commit: DividerCommit,
    workspace: u32,
    mirror: Option<Weak<()>>,
    released: bool,
}

#[derive(Default)]
pub(crate) struct LayoutPreviews {
    next_sequence: u64,
    entries: Vec<Preview>,
}

impl Preview {
    fn valid(&self, core: &CoreState) -> bool {
        let target_exists = match self.commit.target {
            LayoutTarget::Workspace(id) => core.has_workspace(id),
            LayoutTarget::Tab(id) => core.find_pane_for_tab(id).is_some(),
        };
        if !target_exists {
            return false;
        }
        if let Some(token) = &self.mirror {
            core.matches_mirror_projection(self.workspace, token)
        } else {
            core.committed_structure_revision() == Some(self.commit.revision)
                && core.has_workspace(self.workspace)
        }
    }
}

impl LayoutPreviews {
    pub fn retain(&mut self, core: &CoreState) {
        self.entries.retain(|entry| entry.valid(core));
    }

    pub fn find_divider<T: BinaryTree<Id = u32>>(
        &self,
        core: &CoreState,
        target: LayoutTarget,
        tree: &T,
        position: (f32, f32),
        rect: PhysicalRect,
        scale: f32,
    ) -> Option<DividerInfo> {
        let (x, y) = position;
        let threshold = super::mouse::divider_hit_threshold_physical(scale);
        if !self.has(core, target) {
            return tree.find_divider_at(x, y, rect, threshold, scale);
        }
        self.geometry(core, target, tree, rect, scale)
            .dividers
            .into_iter()
            .find_map(|(_, info, ratio)| {
                let divider = divider_rect(info, ratio, T::border_width(scale));
                let hits = match info.direction {
                    SplitDirection::Vertical => {
                        y >= info.split_rect.y.value()
                            && y < (info.split_rect.y + info.split_rect.height).value()
                            && (x - divider.x.value()).abs() < threshold
                    }
                    SplitDirection::Horizontal => {
                        x >= info.split_rect.x.value()
                            && x < (info.split_rect.x + info.split_rect.width).value()
                            && (y - divider.y.value()).abs() < threshold
                    }
                };
                hits.then_some(info)
            })
    }

    pub fn begin<T: BinaryTree<Id = u32>>(
        &mut self,
        core: &CoreState,
        target: LayoutTarget,
        tree: &T,
        info: DividerInfo,
        rect: PhysicalRect,
        scale: f32,
    ) -> Option<u64> {
        self.retain(core);
        let geometry = self.geometry(core, target, tree, rect, scale);
        let (path, _, ratio) = geometry.dividers.into_iter().find(|(_, divider, _)| {
            divider.direction == info.direction && divider.split_rect.approx_eq(&info.split_rect)
        })?;
        let mut node = tree;
        for side in &path {
            let (_, _, first, second) = node.split_parts()?;
            node = if *side { second } else { first };
        }
        let workspace = match target {
            LayoutTarget::Workspace(id) => id,
            LayoutTarget::Tab(id) => {
                core.find_workspace_index_for_pane(core.find_pane_for_tab(id)?)
                    .and_then(|index| core.workspace_at(index))?
                    .id
            }
        };
        let mirror = core.mirror_projection_token(workspace);
        let revision = match &mirror {
            Some(_) => 0,
            None => core.committed_structure_revision()?,
        };
        self.next_sequence = self.next_sequence.checked_add(1)?;
        let sequence = self.next_sequence;
        self.entries.retain(|entry| {
            entry.commit.target != target || entry.commit.path != path || entry.released
        });
        self.entries.push(Preview {
            workspace,
            mirror,
            released: false,
            commit: DividerCommit {
                sequence,
                target,
                path,
                leaves: node.all_ids(),
                revision,
                ratio,
            },
        });
        Some(sequence)
    }

    pub fn update(&mut self, core: &CoreState, sequence: u64, ratio: f32) -> bool {
        self.retain(core);
        let Some(preview) = self
            .entries
            .iter_mut()
            .find(|entry| entry.commit.sequence == sequence)
        else {
            return false;
        };
        preview.commit.ratio = ratio.clamp(
            crate::model::DIVIDER_RATIO_MIN,
            crate::model::DIVIDER_RATIO_MAX,
        );
        true
    }

    /// Mirror ratios remain volatile until delta/reconnect. Local ratios await their commit reply.
    pub fn finish(&mut self, core: &CoreState, sequence: u64) -> Option<DividerCommit> {
        self.retain(core);
        let preview = self
            .entries
            .iter_mut()
            .find(|entry| entry.commit.sequence == sequence)?;
        preview.released = true;
        let commit = preview.commit.clone();
        if preview.mirror.is_some() {
            self.entries.retain(|entry| {
                entry.commit.sequence == sequence
                    || entry.commit.target != commit.target
                    || entry.commit.path != commit.path
            });
            None
        } else {
            Some(commit)
        }
    }

    pub fn cancel(&mut self, sequence: u64) {
        self.entries
            .retain(|entry| entry.commit.sequence != sequence);
    }

    pub fn has(&self, core: &CoreState, target: LayoutTarget) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.commit.target == target && entry.valid(core))
    }

    pub fn geometry<T: BinaryTree<Id = u32>>(
        &self,
        core: &CoreState,
        target: LayoutTarget,
        tree: &T,
        rect: PhysicalRect,
        scale: f32,
    ) -> tasty_model::TreeGeometry<u32> {
        tree.preview_geometry(rect, scale, &|path, ratio| {
            self.entries
                .iter()
                .rev()
                .find(|entry| {
                    entry.commit.target == target && entry.commit.path == path && entry.valid(core)
                })
                .map_or(ratio, |entry| entry.commit.ratio)
        })
    }
}

pub(crate) fn divider_rect(
    info: DividerInfo,
    ratio: f32,
    gap: crate::model::PhysicalPx,
) -> PhysicalRect {
    let (first, _) = info.split_rect.split_with_gap(info.direction, ratio, gap);
    match info.direction {
        SplitDirection::Vertical => PhysicalRect {
            x: first.x + first.width,
            y: info.split_rect.y,
            width: gap,
            height: info.split_rect.height,
        },
        SplitDirection::Horizontal => PhysicalRect {
            x: info.split_rect.x,
            y: first.y + first.height,
            width: info.split_rect.width,
            height: gap,
        },
    }
}

pub(crate) fn surface_regions<'a>(
    previews: Option<&LayoutPreviews>,
    core: &CoreState,
    tab: &'a crate::model::Tab,
    rect: PhysicalRect,
    scale: f32,
) -> Vec<crate::model::SurfaceRegion<'a>> {
    let target = LayoutTarget::Tab(tab.id);
    if let Some(previews) = previews.filter(|previews| previews.has(core, target)) {
        previews
            .geometry(core, target, tab.layout(), rect, scale)
            .leaves
            .into_iter()
            .filter_map(|(id, rect)| {
                tab.layout()
                    .find_surface(id)
                    .map(|surface| crate::model::SurfaceRegion { id, rect, surface })
            })
            .collect()
    } else {
        tab.surface_regions(rect)
    }
}

impl super::MainViewState {
    pub(crate) fn pane_rects(
        &self,
        core: &CoreState,
        workspace: &crate::model::Workspace,
        rect: PhysicalRect,
        scale: f32,
    ) -> Vec<(u32, PhysicalRect)> {
        self.attach_pending
            .pane_geometry(
                core,
                workspace.id,
                self.committed_pane_rects(core, workspace, rect, scale),
                scale,
            )
            .0
    }

    pub(crate) fn committed_pane_rects(
        &self,
        core: &CoreState,
        workspace: &crate::model::Workspace,
        rect: PhysicalRect,
        scale: f32,
    ) -> Vec<(u32, PhysicalRect)> {
        let target = LayoutTarget::Workspace(workspace.id);
        if self.layout_previews.has(core, target) {
            self.layout_previews
                .geometry(core, target, workspace.pane_layout(), rect, scale)
                .leaves
        } else {
            workspace.pane_layout().compute_rects(rect, scale)
        }
    }

    /// 선택된 attach 임시 탭이 이 탭의 pane을 차지해 이 탭이 그려지지 않는가.
    /// 그리는 영역(`tab_surface_regions`)과 가시성 판정(`draws_surface`)이 같은 조건을 쓴다.
    pub(crate) fn attach_tab_preview_hides(
        &self,
        core: &CoreState,
        tab: &crate::model::Tab,
    ) -> bool {
        self.attach_pending
            .selected(core, &self.navigation)
            .is_some_and(|entry| {
                entry.visible
                    && matches!(entry.creation, Some(super::attach_pending::Creation::Tab))
                    && core.find_pane_for_tab(tab.id) == Some(entry.pane)
            })
    }

    pub(crate) fn tab_surface_regions<'a>(
        &self,
        core: &CoreState,
        tab: &'a crate::model::Tab,
        rect: PhysicalRect,
        scale: f32,
    ) -> Vec<crate::model::SurfaceRegion<'a>> {
        if self.attach_tab_preview_hides(core, tab) {
            return Vec::new();
        }
        let mut regions = surface_regions(Some(&self.layout_previews), core, tab, rect, scale);
        let (geometry, _) = self.attach_pending.surface_geometry(
            core,
            tab.id,
            regions
                .iter()
                .map(|region| (region.id, region.rect))
                .collect(),
            scale,
        );
        for region in &mut regions {
            if let Some((_, rect)) = geometry.iter().find(|(id, _)| *id == region.id) {
                region.rect = *rect;
            }
        }
        regions
    }

    pub(crate) fn pane_dividers(
        &self,
        core: &CoreState,
        workspace: &crate::model::Workspace,
        rect: PhysicalRect,
        scale: f32,
    ) -> Vec<PhysicalRect> {
        let target = LayoutTarget::Workspace(workspace.id);
        if self.layout_previews.has(core, target) {
            self.layout_previews
                .geometry(core, target, workspace.pane_layout(), rect, scale)
                .dividers
                .into_iter()
                .map(|(_, info, ratio)| {
                    divider_rect(info, ratio, crate::model::PaneNode::border_width(scale))
                })
                .collect()
        } else {
            workspace.pane_layout().collect_dividers(rect, scale)
        }
    }

    pub(crate) fn surface_dividers(
        &self,
        core: &CoreState,
        tab: &crate::model::Tab,
        rect: PhysicalRect,
        scale: f32,
    ) -> Vec<PhysicalRect> {
        let target = LayoutTarget::Tab(tab.id);
        if self.layout_previews.has(core, target) {
            self.layout_previews
                .geometry(core, target, tab.layout(), rect, scale)
                .dividers
                .into_iter()
                .map(|(_, info, ratio)| {
                    divider_rect(
                        info,
                        ratio,
                        crate::model::SurfaceLayout::border_width(scale),
                    )
                })
                .collect()
        } else {
            tab.layout().collect_dividers(rect, scale)
        }
    }
}

mod explorer_floor;

#[cfg(test)]
mod tests;
