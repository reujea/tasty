//! Human-facing request kinds and their admission times, never transport identifiers.
use crate::ipc::stream::StructuralOp;
use std::time::Instant;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WaitKind {
    NewTab,
    SplitSurface,
    SplitPane,
    CloseSurface,
    CloseTab,
    ClosePane,
    CloseWorkspace,
    MoveTab,
    MoveSurface,
    Restore,
    Convert,
    Resize,
    Input,
    Folder,
    Markdown,
    Git,
    /// 원격 프로그램의 OSC 52 쓰기를 받는 중이거나 client 기록을 기다린다. 지연 없이 보인다.
    Copy,
}
impl WaitKind {
    pub fn from_op(op: &StructuralOp) -> Self {
        match op {
            StructuralOp::NewTab { .. } => Self::NewTab,
            StructuralOp::SplitSurface { .. } => Self::SplitSurface,
            StructuralOp::SplitPane { .. } => Self::SplitPane,
            StructuralOp::CloseSurface { .. } => Self::CloseSurface,
            StructuralOp::CloseTab { .. } => Self::CloseTab,
            StructuralOp::ClosePane { .. } => Self::ClosePane,
            StructuralOp::CloseWorkspace { .. } => Self::CloseWorkspace,
            StructuralOp::MoveTab { .. } => Self::MoveTab,
            StructuralOp::MoveSurface { .. } => Self::MoveSurface,
            StructuralOp::RestoreClosedItem { .. } => Self::Restore,
            StructuralOp::ConvertSurface { .. } => Self::Convert,
        }
    }
    pub fn label_key(self) -> &'static str {
        match self {
            Self::NewTab => "attach.pending.new_tab",
            Self::SplitSurface => "attach.pending.split_surface",
            Self::SplitPane => "attach.pending.split_pane",
            Self::CloseSurface => "attach.pending.close_surface",
            Self::CloseTab => "attach.pending.close_tab",
            Self::ClosePane => "attach.pending.close_pane",
            Self::CloseWorkspace => "attach.pending.close_workspace",
            Self::MoveTab => "attach.pending.move_tab",
            Self::MoveSurface => "attach.pending.move_surface",
            Self::Restore => "attach.pending.restore",
            Self::Convert => "attach.pending.convert",
            Self::Resize => "attach.pending.resize",
            Self::Input => "attach.pending.input",
            Self::Folder => "attach.pending.folder",
            Self::Markdown => "attach.pending.markdown",
            Self::Git => "attach.pending.git",
            Self::Copy => "attach.pending.copy",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Wait {
    pub kind: WaitKind,
    pub admitted: Instant,
}
impl Wait {
    pub fn new(kind: WaitKind, admitted: Instant) -> Self {
        Self { kind, admitted }
    }
}
/// Each kind gets its own oldest admission, independent of the chip's aggregate time.
pub(crate) fn by_kind(waits: &[Wait]) -> std::collections::BTreeMap<WaitKind, Instant> {
    let mut result = std::collections::BTreeMap::new();
    for wait in waits {
        result
            .entry(wait.kind)
            .and_modify(|old: &mut Instant| *old = (*old).min(wait.admitted))
            .or_insert(wait.admitted);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kinds_keep_their_own_oldest_time_without_identifiers() {
        let start = Instant::now();
        let later = start + std::time::Duration::from_secs(8);
        let rows = by_kind(&[
            Wait::new(WaitKind::Input, later),
            Wait::new(WaitKind::NewTab, start),
            Wait::new(WaitKind::NewTab, later),
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[&WaitKind::Input], later);
        assert_eq!(rows[&WaitKind::NewTab], start);
    }
}
