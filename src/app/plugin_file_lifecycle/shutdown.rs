//! The file drain deadline also bounds already-admitted multi-stage plugin work.
use super::{App, Failure, Pipeline};
impl Failure {
    pub(super) fn shutdown_unknown() -> Self {
        Self {
            message: "plugin file lifecycle outcome unknown at shutdown deadline".into(),
            saved: None,
            applied: false,
            details: serde_json::json!({"outcome":"unknown"}),
        }
    }
    pub(crate) fn after_file_receipt(mut self) -> Self {
        self.saved = Some(true);
        self.details["files_committed"] = serde_json::json!(true);
        self.details["partial"] = serde_json::json!(true);
        self
    }
}
impl App {
    pub(crate) fn plugin_file_poll_deadline(&self) -> Option<std::time::Instant> {
        self.plugin_files
            .active
            .as_ref()
            .and_then(|pending| pending.poll_deadline())
    }
    /// Receipt/cache, owned continuations, then original deferred replies, including final polls.
    /// The order holds within one call. A receipt that finishes after its poll waits for the next
    /// call, so continuations must not rely on another consumer's receipt being collected.
    pub(crate) fn collect_plugin_file_shutdown(&mut self) {
        self.services.poll_file_writer();
        #[cfg(feature = "gui")]
        self.poll_child_registry_effects();
        self.finish_preset_files();
        self.finish_plugin_files();
        #[cfg(feature = "gui")]
        self.finish_remote_files();
        #[cfg(feature = "gui")]
        self.finish_settings_exports();
        #[cfg(feature = "gui")]
        self.settle_settings_saves();
        #[cfg(not(feature = "gui"))]
        crate::intent::headless::settle_settings_saves(&mut self.services);
        self.services
            .poll_memory_replies(self.plugin_manager.as_mut());
    }
    /// Accepted worker/retirement outcomes remain unresolved; queued commands never ran.
    pub(crate) fn expire_plugin_file_pipelines(&mut self) {
        if self.plugin_files.expired {
            return;
        }
        self.plugin_files.expired = true;
        #[cfg(feature = "gui")]
        {
            self.plugin_files.retry_shortcuts = false;
        }
        if let Some(mut active) = self.plugin_files.active.take() {
            if let Some(finish) = active.ready(self) {
                finish(self);
            } else {
                self.plugin_files.unresolved += 1;
                active.expire()(self);
            }
        }
        self.plugin_files.current = None;
        let rejected = self.plugin_files.queue.len();
        while let Some(queued) = self.plugin_files.queue.pop_front() {
            self.plugin_files.current = Some(Pipeline {
                base: queued.held.bytes(),
                held: queued.held,
            });
            (queued.start)(self);
            self.plugin_files.current = None;
        }
        #[cfg(feature = "gui")]
        let dirty = self.plugin_files.failed_shortcuts.len();
        #[cfg(not(feature = "gui"))]
        let dirty = 0usize;
        tracing::warn!(target:"tasty::shutdown",unresolved=self.plugin_files.unresolved,rejected_queued=rejected,unsaved_shortcuts=dirty,
            "plugin_file_pipeline_shutdown_deadline");
        self.services
            .poll_memory_replies(self.plugin_manager.as_mut());
    }
}
