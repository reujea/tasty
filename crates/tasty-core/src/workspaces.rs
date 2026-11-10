//! Composite reads borrow disjoint local and mirror projections. Neither tree is cloned.

use crate::CoreState;
use tasty_model::Workspace;

impl CoreState {
    /// Existing list indices refer to the combined display order. Journal input uses local only.
    pub fn workspaces(&self) -> WorkspaceRead<'_> {
        if self.mirror_workspaces.is_empty() {
            return WorkspaceRead::local(&self.local_workspaces);
        }
        WorkspaceRead {
            local: &self.local_workspaces,
            mirrors: &self.mirror_workspaces,
            order: &self.workspace_display_order,
        }
    }

    pub fn workspace_at(&self, index: usize) -> Option<&Workspace> {
        self.workspaces().get(index)
    }

    pub(crate) fn workspace_at_mut(&mut self, index: usize) -> Option<&mut Workspace> {
        let id = self.workspaces().get(index)?.id;
        self.local_workspaces
            .iter_mut()
            .chain(&mut self.mirror_workspaces)
            .find(|workspace| workspace.id == id)
    }

    pub(crate) fn workspaces_mut(&mut self) -> Vec<&mut Workspace> {
        let order = &self.workspace_display_order;
        let mut workspaces: Vec<_> = self
            .local_workspaces
            .iter_mut()
            .chain(&mut self.mirror_workspaces)
            .collect();
        workspaces.sort_by_key(|workspace| {
            order
                .iter()
                .position(|id| *id == workspace.id)
                .unwrap_or(usize::MAX)
        });
        workspaces
    }

    pub(crate) fn push_local_workspace(&mut self, workspace: Workspace) {
        self.insert_local_workspace(self.workspaces().len(), workspace);
    }

    pub fn push_mirror_workspace(&mut self, workspace: Workspace) {
        assert!(workspace.mirror, "remote projection requires a mirror");
        self.refresh_workspace_display_order();
        self.workspace_display_order.push(workspace.id);
        self.mirror_projection_tokens
            .insert(workspace.id, std::sync::Arc::new(()));
        self.mirror_workspaces.push(workspace);
    }

    /// 저장된 원격 행을 화면 순서의 `index`에 넣는다. 목록보다 큰 값은 끝으로 둔다.
    pub fn insert_mirror_workspace(&mut self, index: usize, workspace: Workspace) {
        assert!(workspace.mirror, "remote projection requires a mirror");
        self.refresh_workspace_display_order();
        let index = index.min(self.workspace_display_order.len());
        self.workspace_display_order.insert(index, workspace.id);
        self.mirror_projection_tokens
            .insert(workspace.id, std::sync::Arc::new(()));
        self.mirror_workspaces.push(workspace);
    }

    pub(crate) fn insert_local_workspace(&mut self, index: usize, workspace: Workspace) {
        assert!(!workspace.mirror, "local projection cannot own a mirror");
        self.refresh_workspace_display_order();
        let local_index = self.workspace_display_order[..index]
            .iter()
            .filter(|id| {
                self.local_workspaces
                    .iter()
                    .any(|workspace| workspace.id == **id)
            })
            .count();
        self.workspace_display_order.insert(index, workspace.id);
        self.local_workspaces.insert(local_index, workspace);
    }

    pub(crate) fn remove_workspace_at(&mut self, index: usize) -> Workspace {
        let id = self
            .workspace_at(index)
            .expect("workspace index is valid")
            .id;
        self.workspace_display_order
            .retain(|candidate| *candidate != id);
        if let Some(index) = self
            .local_workspaces
            .iter()
            .position(|workspace| workspace.id == id)
        {
            self.local_workspaces.remove(index)
        } else {
            let index = self
                .mirror_workspaces
                .iter()
                .position(|workspace| workspace.id == id)
                .expect("composite workspace exists");
            self.mirror_projection_tokens.remove(&id);
            self.mirror_workspaces.remove(index)
        }
    }

    pub fn replace_mirror_workspace(&mut self, workspace: Workspace) -> Result<(), Workspace> {
        assert!(workspace.mirror, "remote projection requires a mirror");
        if let Some(slot) = self
            .mirror_workspaces
            .iter_mut()
            .find(|candidate| candidate.id == workspace.id)
        {
            self.mirror_projection_tokens
                .insert(workspace.id, std::sync::Arc::new(()));
            let mut workspace = workspace;
            // 저장된 원격 행의 이름·설명·카테고리는 사용자가 정한 값이다. 원격 구조로 덮지 않는다.
            if slot.remote_row.is_some() {
                workspace.remote_row = slot.remote_row.take();
                workspace.name = std::mem::take(&mut slot.name);
                workspace.subtitle = std::mem::take(&mut slot.subtitle);
                workspace.description = std::mem::take(&mut slot.description);
                workspace.category = slot.category;
            }
            *slot = workspace;
            Ok(())
        } else {
            Err(workspace)
        }
    }

    pub fn mirror_projection_token(&self, id: u32) -> Option<std::sync::Weak<()>> {
        self.mirror_projection_tokens
            .get(&id)
            .map(std::sync::Arc::downgrade)
    }

    pub fn matches_mirror_projection(&self, id: u32, token: &std::sync::Weak<()>) -> bool {
        self.mirror_projection_token(id)
            .is_some_and(|current| current.ptr_eq(token))
    }

    /// A display continuation cannot reorder the committed local tree or discard new objects.
    pub fn apply_workspace_display_order(&mut self, order: Vec<u32>) -> bool {
        if order.len() != self.workspaces().len()
            || order
                .iter()
                .enumerate()
                .any(|(index, id)| order[..index].contains(id) || !self.has_workspace(*id))
            || !order
                .iter()
                .filter(|id| {
                    self.local_workspaces
                        .iter()
                        .any(|workspace| workspace.id == **id)
                })
                .copied()
                .eq(self.local_workspaces.iter().map(|workspace| workspace.id))
        {
            return false;
        }
        self.workspace_display_order = order;
        true
    }

    pub(crate) fn reorder_local_workspaces(&mut self, order: &[u32]) -> Result<(), String> {
        if order.len() != self.local_workspaces.len()
            || order.iter().enumerate().any(|(index, id)| {
                order[..index].contains(id)
                    || !self
                        .local_workspaces
                        .iter()
                        .any(|workspace| workspace.id == *id)
            })
        {
            return Err("committed workspace order differs from the live local projection".into());
        }
        self.local_workspaces
            .sort_by_key(|workspace| order.iter().position(|id| *id == workspace.id));
        self.reconcile_local_display_order();
        Ok(())
    }

    pub(crate) fn replace_local_workspaces(&mut self, workspaces: Vec<Workspace>) {
        assert!(workspaces.iter().all(|workspace| !workspace.mirror));
        self.local_workspaces = workspaces;
        self.reconcile_local_display_order();
    }

    fn reconcile_local_display_order(&mut self) {
        let mut locals = self.local_workspaces.iter().map(|workspace| workspace.id);
        let mut order: Vec<_> = self
            .workspace_display_order
            .iter()
            .filter_map(|id| {
                if self
                    .mirror_workspaces
                    .iter()
                    .any(|workspace| workspace.id == *id)
                {
                    Some(*id)
                } else {
                    locals.next()
                }
            })
            .collect();
        order.extend(locals);
        self.workspace_display_order = order;
        self.refresh_workspace_display_order();
    }

    fn refresh_workspace_display_order(&mut self) {
        self.workspace_display_order = self
            .workspaces()
            .into_iter()
            .map(|workspace| workspace.id)
            .collect();
    }
}

/// Borrowed composite sequence. Iteration does not allocate, sort, or clone workspace objects.
#[derive(Clone, Copy)]
pub struct WorkspaceRead<'a> {
    local: &'a [Workspace],
    mirrors: &'a [Workspace],
    order: &'a [u32],
}

impl<'a> WorkspaceRead<'a> {
    pub(crate) fn local(local: &'a [Workspace]) -> Self {
        Self {
            local,
            mirrors: &[],
            order: &[],
        }
    }
    pub fn len(self) -> usize {
        self.local.len() + self.mirrors.len()
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn get(self, index: usize) -> Option<&'a Workspace> {
        self.iter().nth(index)
    }
    pub fn iter(self) -> WorkspaceIter<'a> {
        WorkspaceIter {
            read: self,
            ordered: 0,
            fallback: 0,
            yielded: 0,
        }
    }
}

pub struct WorkspaceIter<'a> {
    read: WorkspaceRead<'a>,
    ordered: usize,
    fallback: usize,
    yielded: usize,
}

impl<'a> Iterator for WorkspaceIter<'a> {
    type Item = &'a Workspace;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(id) = self.read.order.get(self.ordered) {
            self.ordered += 1;
            if let Some(workspace) = self
                .read
                .local
                .iter()
                .chain(self.read.mirrors)
                .find(|workspace| workspace.id == *id)
            {
                self.yielded += 1;
                return Some(workspace);
            }
        }
        while self.fallback < self.read.len() {
            let workspace = if self.fallback < self.read.local.len() {
                &self.read.local[self.fallback]
            } else {
                &self.read.mirrors[self.fallback - self.read.local.len()]
            };
            self.fallback += 1;
            if !self.read.order.contains(&workspace.id) {
                self.yielded += 1;
                return Some(workspace);
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.read.len() - self.yielded;
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for WorkspaceIter<'_> {}

impl<'a> IntoIterator for WorkspaceRead<'a> {
    type Item = &'a Workspace;
    type IntoIter = WorkspaceIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a> IntoIterator for &WorkspaceRead<'a> {
    type Item = &'a Workspace;
    type IntoIter = WorkspaceIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
