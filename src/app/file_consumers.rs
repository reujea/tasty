//! Shared file consumers use the existing file writer without depending on settings revisions.
#[cfg(all(test, feature = "gui"))]
mod wake_tests;

use std::sync::Arc;

use tasty_utils::batch_writer::{Handle, RequestReceipt};

use super::file_writer::{self, FileExecutor, FileRequest};
use crate::core::file_snapshot::{PendingSave, Snapshot, Submit};
use crate::runtime::registries::RuntimeRegistries;

struct Pending(RequestReceipt<FileExecutor>);
impl PendingSave for Pending {
    fn poll(&self) -> Option<Result<(), String>> {
        self.0
            .try_take()
            .map(|outcome| file_writer::saved_of(&outcome))
    }
}

pub(crate) fn sink(queue: Handle<FileExecutor>) -> Submit {
    Arc::new(move |snapshot: Snapshot| {
        queue
            .submit(FileRequest::Snapshot(snapshot))
            .map(|receipt| Box::new(Pending(receipt)) as Box<dyn PendingSave>)
            .map_err(|e| e.to_string())
    })
}

pub(crate) fn attach(registries: &RuntimeRegistries, queue: Handle<FileExecutor>) {
    registries.child_terminals.attach(queue.clone());
    let sink = sink(queue);
    if let Some(rules) = &registries.file_rules {
        rules.install_sink(sink.clone());
    }
    #[cfg(feature = "gui")]
    {
        registries.explorer_favorites.install_sink(sink.clone());
        registries.port_favorites.install_sink(sink.clone());
        registries.remote_force_detached.install_sink(sink);
    }
}

pub(crate) fn poll(registries: &RuntimeRegistries) {
    registries.child_terminals.poll();
    report(
        "hook-handlers.toml",
        crate::hook_handler::global().poll_file(),
    );
    if let Some(rules) = &registries.file_rules {
        report("file-handlers.toml", rules.poll());
    }
    #[cfg(feature = "gui")]
    {
        report(
            "explorer-favorites.toml",
            registries.explorer_favorites.poll(),
        );
        report("port-favorites.toml", registries.port_favorites.poll());
        report(
            "remote-force-detached.toml",
            registries.remote_force_detached.poll(),
        );
    }
}

pub(crate) fn report(file: &str, errors: Vec<String>) {
    for error in errors {
        tracing::warn!(file, %error, "file snapshot was not saved");
        crate::system_notice::post(crate::system_notice::SystemNotice::warning(
            if matches!(file, "file-handlers.toml" | "hook-handlers.toml") {
                "settings-file-writer"
            } else {
                "file-writer"
            },
            crate::i18n::t("storage_notice.file_save_title").into(),
            crate::i18n::t_fmt2("storage_notice.file_save_body", file, &error),
        ));
    }
}

pub(crate) fn before_shutdown(registries: &RuntimeRegistries) {
    poll(registries);
    crate::hook_handler::global().before_file_shutdown();
    if let Some(rules) = &registries.file_rules {
        rules.before_shutdown();
    }
    #[cfg(feature = "gui")]
    {
        registries.explorer_favorites.retry_latest_before_shutdown();
        registries.port_favorites.retry_latest_before_shutdown();
        registries
            .remote_force_detached
            .retry_latest_before_shutdown();
    }
}

pub(crate) fn after_shutdown(registries: &RuntimeRegistries) {
    poll(registries);
    registries.child_terminals.finish_shutdown();
    report_shutdown(
        "hook-handlers.toml",
        crate::hook_handler::global().finish_file_shutdown(),
    );
    report_shutdown(
        "file-handlers.toml",
        registries
            .file_rules
            .as_ref()
            .and_then(|rules| rules.finish_shutdown()),
    );
    #[cfg(feature = "gui")]
    {
        report_shutdown(
            "explorer-favorites.toml",
            registries.explorer_favorites.finish_shutdown(),
        );
        report_shutdown(
            "port-favorites.toml",
            registries.port_favorites.finish_shutdown(),
        );
        report_shutdown(
            "remote-force-detached.toml",
            registries.remote_force_detached.finish_shutdown(),
        );
    }
}
fn report_shutdown(file: &str, unsaved: Option<(u64, u64)>) {
    if let Some((visible, committed)) = unsaved {
        tracing::warn!(file, visible, committed, "file remains unsaved at shutdown");
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    pub(crate) fn poll_child_registry_effects(&mut self) {
        for session in self.engines.all_sessions_mut() {
            session.borrow_mut().poll_child_registry_effects();
        }
    }
}
