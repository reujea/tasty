//! Boot-only recovery before the first View/IPC is published. Never replay a failed restore.
use super::*;

impl JournalApplication {
    /// Stop the original worker and detach exact resource owners before clearing the projection.
    /// Durable effects remain unfinished in the original journal; disposal is not their success.
    /// Pending PTY/plugin receipts remain owned and are polled independently of the new session.
    pub(crate) fn poll_boot_recovery(
        &mut self,
        session: &mut EngineSession,
        mut plugins: Option<&mut crate::plugin::PluginManager>,
        failure: &str,
    ) -> anyhow::Result<()> {
        self.halted.get_or_insert_with(|| failure.to_owned());
        self.worker.abort_without_checkpoint();
        self.release_halted_resources(session, plugins.as_deref_mut());
        let complete = crate::runtime::resource_retirement::poll_engine_release(session, plugins);
        // Ownership has left every runtime map. Preserve incomplete receipts separately so
        // a failed plugin reply does not prevent the empty terminal session from opening.
        let has_unreturned = session
            .engine_release
            .as_ref()
            .is_some_and(|release| release.has_unreturned_standalones());
        let retained = if complete && !has_unreturned {
            None
        } else {
            session.engine_release.take()
        };
        let bound = session.journal_binding.is_some();
        if let Err(error) = self.replace_with_ephemeral(session.id, failure, bound) {
            session.engine_release = retained;
            return Err(error);
        }
        self.boot_release = retained;
        session.journal_binding = None;
        session.engine_release = None;
        session.core_state = crate::core::CoreState::new_base();
        session.remote.row_tokens.clear();
        session.persistence =
            crate::runtime::engine_session::EnginePersistence::new(session.persistence.slot);
        session.runtime.ids = Default::default();
        session.runtime.pending_submits.clear();
        session.runtime.pending_scrollback_inject.clear();
        session.runtime.pending_host_events.clear();
        session.runtime.pending_lifecycle_events.clear();
        let slot = session
            .persistence
            .slot
            .ok_or_else(|| anyhow::anyhow!("GUI engine has no slot"))?;
        // No legacy import and no second activation of stored restore commands in this run.
        self.begin_engine(
            session,
            EngineSelection::Slot {
                slot,
                resume: false,
            },
        )
        .map_err(anyhow::Error::msg)?;
        Ok(())
    }
}

impl JournalApplication {
    pub(super) fn poll_boot_release(&mut self, plugins: Option<&mut crate::plugin::PluginManager>) {
        if let Some(release) = &mut self.boot_release
            && release.poll_detached(plugins)
            && !release.has_unreturned_standalones()
        {
            self.boot_release = None;
        }
    }
}
