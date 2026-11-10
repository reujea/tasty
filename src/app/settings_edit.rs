//! Settings editors return values. App applies registry/file effects after the modal is removed.
use crate::file::{
    format::{DetectorDecl, DetectorId},
    handler::{HandlerId, UserHandlerUpsertDecl},
};
#[derive(Clone, Debug)]
pub(crate) enum RegistryEdit {
    Extension {
        extension: String,
        order: Vec<DetectorId>,
    },
    DetectorEnabled(DetectorId, bool),
    RemoveDetector(DetectorId),
    AddDetector(DetectorDecl),
    HandlerEnabled(HandlerId, bool),
    RemoveHandler(HandlerId),
    AddHandler(UserHandlerUpsertDecl),
    HookEnabled(crate::hook_handler::HookHandlerId, bool),
    RemoveHook(crate::hook_handler::HookHandlerId),
    UpsertHook(crate::hook_handler::registry::UserHookHandlerUpsertDecl),
}
#[derive(Default)]
pub(crate) struct SettingsEdits {
    pub(crate) registry: Vec<RegistryEdit>,
    pub(crate) clear_scrollback: bool,
}

pub(crate) struct SettingsEditOwner {
    pub(crate) format: std::sync::Arc<crate::file::format::FileFormatRegistry>,
    pub(crate) handler: std::sync::Arc<crate::file::handler::FileHandlerRegistry>,
}
impl super::App {
    pub(crate) fn apply_settings_edits(&mut self, owner: SettingsEditOwner, edits: SettingsEdits) {
        if edits.clear_scrollback {
            crate::scrollback_store::clear_all();
        }
        let SettingsEditOwner { format, handler } = owner;
        let hook = crate::hook_handler::global();
        let mut files_changed = false;
        let mut hooks_changed = false;
        for edit in edits.registry {
            let (files, hooks) = apply_registry_edit(&format, &handler, hook, edit);
            files_changed |= files;
            hooks_changed |= hooks;
        }
        if files_changed {
            if let Some(rules) = &self.services.registries.file_rules {
                rules.submit(&format, &handler);
            } else {
                super::file_consumers::report(
                    "file-handlers.toml",
                    vec!["file path is unavailable".into()],
                );
            }
        }
        if hooks_changed
            && let Err(error) = hook.submit_file(self.services.file_snapshot_sink(), true)
        {
            super::file_consumers::report("hook-handlers.toml", vec![error]);
        }
    }
}

pub(crate) fn apply_registry_edit(
    format: &crate::file::format::FileFormatRegistry,
    handler: &crate::file::handler::FileHandlerRegistry,
    hook: &crate::hook_handler::registry::HookHandlerRegistry,
    edit: RegistryEdit,
) -> (bool, bool) {
    match edit {
        RegistryEdit::Extension { extension, order } => {
            if order.is_empty() {
                format.clear_user_extension_priority(&extension);
            } else {
                format.set_user_extension_priority(&extension, order);
            }
            (true, false)
        }
        RegistryEdit::DetectorEnabled(id, enabled) => {
            format.set_user_detector_disabled(&id, !enabled);
            (true, false)
        }
        RegistryEdit::RemoveDetector(id) => {
            format.remove_user_detector(&id);
            (true, false)
        }
        RegistryEdit::AddDetector(value) => {
            if let Err(error) = format.upsert_user_detector(value) {
                report_registry_edit_failure(error, "settings detector edit failed");
            }
            (true, false)
        }
        RegistryEdit::HandlerEnabled(id, enabled) => {
            handler.set_user_handler_disabled(&id, !enabled);
            (true, false)
        }
        RegistryEdit::RemoveHandler(id) => {
            handler.remove_user_handler(&id);
            (true, false)
        }
        RegistryEdit::AddHandler(value) => {
            if let Err(error) = handler.upsert_user_handler(value) {
                report_registry_edit_failure(error, "settings handler edit failed");
            }
            (true, false)
        }
        RegistryEdit::HookEnabled(id, enabled) => {
            hook.set_user_handler_disabled(&id, !enabled);
            (false, true)
        }
        RegistryEdit::RemoveHook(id) => {
            hook.remove_user_handler(&id);
            (false, true)
        }
        RegistryEdit::UpsertHook(value) => {
            if let Err(error) = hook.upsert_user_handler(value) {
                report_registry_edit_failure(error, "settings hook edit failed");
            }
            (false, true)
        }
    }
}

fn report_registry_edit_failure(error: impl std::fmt::Display, message: &str) {
    tracing::warn!(%error, "{message}");
}
