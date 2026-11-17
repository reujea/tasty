use super::App;
use super::plugin_file_lifecycle::{Failure, Reply};
use crate::app::command::CoreEvent;
use serde_json::{Value, json};
use tasty_host_plugin::manager::file_config::ConfigChange;

impl App {
    /// Preparation happens in admission order, after the preceding receipt updated the cache.
    pub(crate) fn enqueue_plugin_config(
        &mut self,
        change: ConfigChange,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) -> Result<(), Failure> {
        self.plugin_files
            .enqueue_sized(change_bytes(&change), move |app| {
                app.start_plugin_config(change, finish);
            })?;
        (self.services.reply_wake())();
        Ok(())
    }

    pub(crate) fn start_plugin_config(
        &mut self,
        change: ConfigChange,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) {
        self.start_plugin_config_retained(change, 0, finish);
    }
    pub(crate) fn start_plugin_config_retained(
        &mut self,
        change: ConfigChange,
        context_bytes: usize,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) {
        let app = self;
        let request = app
            .plugin_manager
            .as_ref()
            .ok_or_else(|| Failure::before("plugin manager not initialized"))
            .and_then(|mgr| mgr.prepare_config_change(&change).map_err(Failure::before));
        match request {
            Err(error) => finish(app, Err(error)),
            Ok(request) => app.run_plugin_file(
                (
                    request.retained_bytes(),
                    context_bytes.saturating_add(change_bytes(&change)),
                ),
                move || request.execute().map_err(|e| e.to_string()),
                |result| match result {
                    Ok(config) => super::file_writer::command::value_bytes(config),
                    Err(e) => e.len(),
                },
                crate::app::plugin_file_lifecycle::classify_result,
                move |app, result| match result.and_then(|result| result.map_err(Failure::before)) {
                    Err(error) => finish(app, Err(error)),
                    Ok(config) => {
                        if let ConfigChange::Enabled { id, enabled: true } = &change
                            && !app.plugin_files.stopping()
                        {
                            app.after_plugin_retired(id.clone(), move |app, retired| {
                                let result = retired
                                    .map_err(Failure::after_file_receipt)
                                    .and_then(|()| app.apply_plugin_config(config, &change));
                                finish(app, result);
                            });
                            return;
                        }
                        let result = app.apply_plugin_config(config, &change);
                        finish(app, result);
                    }
                },
            ),
        }
    }

    pub(crate) fn reply_plugin_config(
        &mut self,
        id: Value,
        change: ConfigChange,
    ) -> tasty_ipc::protocol::JsonRpcResponse {
        let verb = match &change {
            ConfigChange::Enabled { enabled: true, .. } => Some("enable"),
            ConfigChange::Enabled { enabled: false, .. } => Some("disable"),
            _ => None,
        };
        let reply = Reply::default();
        let done = reply.clone();
        if let Err(error) = self.enqueue_plugin_config(change, move |_, result| {
            done.finish(result.map_err(|mut error| {
                if let Some(verb) = verb {
                    error.message = format!("{verb} failed: {}", error.message);
                }
                error
            }));
        }) {
            let mut error = error;
            if let Some(verb) = verb {
                error.message = format!("{verb} failed: {}", error.message);
            }
            reply.finish(Err(error));
        }
        self.plugin_file_reply(id, reply)
    }

    pub(crate) fn plugin_file_reply(
        &self,
        id: Value,
        reply: Reply,
    ) -> tasty_ipc::protocol::JsonRpcResponse {
        self.services.reply_with_file(
            id.clone(),
            move || reply.poll(),
            move |_, result| match result {
                Ok(body) => tasty_ipc::protocol::JsonRpcResponse::success(id, body),
                Err(error) => {
                    let data = error.response_data();
                    tasty_ipc::protocol::JsonRpcResponse::error_with_data(
                        id,
                        -32000,
                        error.message,
                        data,
                    )
                }
            },
        )
    }

    fn apply_plugin_config(
        &mut self,
        config: tasty_host_plugin::PluginsConfig,
        change: &ConfigChange,
    ) -> Result<Value, Failure> {
        if self.plugin_files.stopping()
            && matches!(change, ConfigChange::Enabled { enabled: true, .. })
        {
            if let Some(mgr) = self.plugin_manager.as_mut() {
                mgr.config = config;
            }
            return Err(Failure {
                message: "plugin enable saved for next start; host is shutting down".into(),
                saved: Some(true),
                applied: false,
                details: Value::Null,
            });
        }
        let mgr = self.plugin_manager.as_mut().ok_or_else(|| Failure {
            message: "plugin manager disappeared after file commit".into(),
            saved: Some(true),
            applied: false,
            details: Value::Null,
        })?;
        let was_running = match change {
            ConfigChange::Enabled { id, .. } => mgr.is_running(id),
            _ => false,
        };
        mgr.apply_config_change(config, change)
            .map_err(|e| Failure {
                message: e.to_string(),
                saved: Some(true),
                applied: false,
                details: Value::Null,
            })?;
        let (body, events) = config_result(mgr, change, was_running);
        if let ConfigChange::Enabled { id, enabled: false } = change {
            self.services
                .registries
                .surface_registry
                .withdraw_plugin(id);
        }
        #[cfg(feature = "gui")]
        self.cascade_plugin_events(events);
        #[cfg(not(feature = "gui"))]
        crate::app::plugin_events::cascade_toggle_events_headless(
            self.plugin_manager.as_mut().expect("manager checked"),
            &self.services.registries.plugin_hook_events,
            events,
        );
        Ok(body)
    }
}

fn config_result(
    mgr: &crate::plugin::PluginManager,
    change: &ConfigChange,
    was_running: bool,
) -> (Value, Vec<CoreEvent>) {
    match change {
        ConfigChange::Enabled { id, enabled } => {
            let mut body = if *enabled {
                json!({"enabled":id})
            } else {
                json!({"disabled":id})
            };
            if *enabled {
                body["start"] = crate::ipc::handler::plugin::start_json(mgr.start_status(id));
            }
            let mut events = vec![CoreEvent::PluginEnableToggled {
                plugin_id: id.clone(),
                enabled: *enabled,
            }];
            if !enabled && was_running {
                events.push(CoreEvent::PluginUnloaded {
                    plugin_id: id.clone(),
                    reason: tasty_plugin_protocol::events::LifecycleReason::User,
                });
            }
            (body, events)
        }
        ConfigChange::Shortcuts(_) => (json!({}), vec![]),
    }
}

fn change_bytes(change: &ConfigChange) -> usize {
    match change {
        ConfigChange::Enabled { id, .. } => id.len(),
        ConfigChange::Shortcuts(values) => super::file_writer::command::value_bytes(values),
    }
}
