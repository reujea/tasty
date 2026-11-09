//! Engine-local child metadata views backed by ordered shared-writer commands.
mod data;
mod effects;
mod persistence;
#[cfg(test)]
mod test_support;
mod transport;

pub(crate) use data::ParentKey;
use data::RegistryData;
#[cfg(test)]
use data::read_file;
pub use data::{ChildEntry, ReconcileSummary, now_epoch_ms};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
pub(crate) use transport::{Hub, Ticket};

pub struct ChildTerminalRegistry {
    client: Arc<Mutex<transport::Client>>,
    hub: Arc<Hub>,
    effects: std::collections::VecDeque<effects::PendingEffect>,
}
impl Default for ChildTerminalRegistry {
    fn default() -> Self {
        Hub::new(None).client()
    }
}
impl ChildTerminalRegistry {
    #[cfg(all(test, unix, feature = "gui"))]
    pub fn load() -> Self {
        let data = RegistryData::load();
        let hub = Hub::new(data.path.clone());
        hub.test_initial(data);
        hub.client()
    }
    #[cfg(test)]
    pub fn next_index_for(&mut self, parent: u32) -> u32 {
        self.edit(|s| s.next_index_for(parent))
    }
    #[cfg(test)]
    pub(crate) fn reserve_index(&mut self, parent: &ParentKey) -> Result<u32, String> {
        test_support::wait(self.reserve(parent)?)?
            .ok_or_else(|| "child reservation missing index".into())
    }
    pub(crate) fn from_hub(hub: &Arc<Hub>) -> Self {
        hub.client()
    }
    fn read<T>(&self, f: impl FnOnce(&RegistryData) -> T) -> T {
        self.hub.poll();
        f(&self.client.lock().unwrap().visible)
    }
    fn edit<T>(&mut self, f: impl FnOnce(&mut RegistryData) -> T) -> T {
        self.hub.poll();
        f(&mut self.client.lock().unwrap().visible)
    }
    pub(crate) fn submit(&mut self) -> Result<Ticket, String> {
        self.hub.submit(&self.client, persistence::Operation::Merge)
    }
    pub fn save(&mut self) {
        if let Err(error) = self.submit() {
            tracing::warn!(%error, "child registry command not admitted");
        }
    }
    pub(crate) fn reserve(&mut self, parent: &ParentKey) -> Result<Ticket, String> {
        self.hub.submit(
            &self.client,
            persistence::Operation::Reserve(parent.clone()),
        )
    }
    pub(crate) fn adopt(&mut self, parent: ParentKey, child: ChildEntry) -> Result<Ticket, String> {
        self.hub
            .submit(&self.client, persistence::Operation::Adopt(parent, child))
    }
    pub(crate) fn bind_slot(&mut self, slot: Option<u32>) {
        self.edit(|s| s.bind_slot(slot));
    }
    #[cfg(feature = "gui")]
    pub(crate) fn forget_slots_except(&mut self, keep: &HashSet<u32>) {
        if let Err(error) = self
            .hub
            .submit(&self.client, persistence::Operation::Prune(keep.clone()))
        {
            tracing::warn!(%error, "child registry prune not admitted");
        }
    }
    #[cfg(feature = "gui")]
    pub(crate) fn release_owned(&mut self) {
        self.hub.retire_owned(&self.client);
    }
    pub fn register_child(&mut self, parent: impl Into<ParentKey>, child: ChildEntry) {
        self.edit(|s| s.register_child(parent, child));
    }
    pub fn list_children(&self, parent: &ParentKey) -> Vec<ChildEntry> {
        self.read(|s| s.list_children(parent).to_vec())
    }
    pub fn find_child(&self, parent: &ParentKey, index: u32) -> Option<ChildEntry> {
        self.read(|s| s.find_child(parent, index).cloned())
    }
    pub fn update_child<F: FnOnce(&mut ChildEntry)>(
        &mut self,
        parent: &ParentKey,
        index: u32,
        f: F,
    ) -> bool {
        self.edit(|s| s.update_child(parent, index, f))
    }
    pub fn parent_of_child(&self, child: u32) -> Option<ParentKey> {
        self.read(|s| s.parent_of_child(child))
    }
    pub fn remove_child(&mut self, parent: &ParentKey, index: u32) -> Option<ChildEntry> {
        self.edit(|s| s.remove_child(parent, index))
    }
    pub fn unregister_child_by_surface(&mut self, child: u32) -> bool {
        self.hub.unregister(&self.client, child)
    }
    pub fn set_idle(&mut self, child: u32, value: bool) {
        self.edit(|s| s.set_idle(child, value));
    }
    pub fn set_needs_input(&mut self, child: u32, value: bool) {
        self.edit(|s| s.set_needs_input(child, value));
    }
    pub(crate) fn clear_idle_for_new_prompt(&mut self, child: u32) -> bool {
        self.edit(|s| s.clear_idle_for_new_prompt(child))
    }
    pub fn last_state_report_at(&self, child: u32) -> Option<u64> {
        self.read(|s| s.last_state_report_at(child))
    }
    pub fn hook_silence(&self, child: u32, now: u64) -> Option<std::time::Duration> {
        self.read(|s| s.hook_silence(child, now))
    }
    pub fn state_of(&self, child: u32) -> &'static str {
        self.read(|s| s.state_of(child))
    }
    pub fn single_parent(&self, live: &HashSet<u32>) -> Option<u32> {
        self.read(|s| s.single_parent(live))
    }
    pub fn reconcile_with_live_surfaces(&mut self, live: &HashSet<u32>) -> ReconcileSummary {
        self.edit(|s| s.reconcile_with_live_surfaces(live))
    }
}
