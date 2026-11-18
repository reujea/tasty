//! Common per-View values; OS/GPU objects remain in ViewBase.
use crate::view::repaint::RepaintGate;
use winit::keyboard::ModifiersState;
pub(crate) struct ViewState {
    /// Unique for this View incarnation, including replacement under the same OS window ID.
    identity: std::sync::Arc<()>,
    pub(crate) dirty: bool,
    pub(crate) repaint: RepaintGate,
    pub(crate) sync_hold: crate::view::sync_hold::SyncHold,
    pub(crate) focused: bool,
    pub(crate) modifiers: ModifiersState,
    pub(crate) close_requested: bool,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            identity: std::sync::Arc::new(()),
            dirty: true,
            repaint: RepaintGate::new(),
            sync_hold: Default::default(),
            focused: true,
            modifiers: ModifiersState::empty(),
            close_requested: false,
        }
    }
}
impl ViewState {
    pub(crate) fn identity(&self) -> std::sync::Weak<()> {
        std::sync::Arc::downgrade(&self.identity)
    }
    pub(crate) fn matches_identity(&self, identity: &std::sync::Weak<()>) -> bool {
        self.identity().ptr_eq(identity)
    }
}
