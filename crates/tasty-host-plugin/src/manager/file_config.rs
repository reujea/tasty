//! Preparing a file request must not start, retire, or mutate the live manager.
use super::PluginManager;
use crate::{file_persistence::ConfigUpdate, registry_state::ShortcutOverride};

#[derive(Clone)]
pub enum ConfigChange {
    Enabled { id: String, enabled: bool },
    Shortcuts(Vec<(String, String, Option<ShortcutOverride>)>),
}

impl PluginManager {
    pub fn prepare_config_change(&self, change: &ConfigChange) -> anyhow::Result<ConfigUpdate> {
        let mut next = self.config.clone();
        match change {
            ConfigChange::Enabled { id, enabled } => self.edit_enabled(&mut next, id, *enabled)?,
            ConfigChange::Shortcuts(edits) => {
                for (id, command, value) in edits {
                    match value {
                        Some(value) => next.set_shortcut_override(id, command, value.clone()),
                        None => {
                            next.clear_shortcut_override(id, command);
                        }
                    }
                }
            }
        }
        let update = ConfigUpdate::between(&self.config, &next)?;
        Ok(match change {
            ConfigChange::Enabled { id, enabled } => update.with_enabled_intent(id, *enabled),
            ConfigChange::Shortcuts(_) => update,
        })
    }

    fn edit_enabled(
        &self,
        next: &mut crate::PluginsConfig,
        id: &str,
        enabled: bool,
    ) -> anyhow::Result<()> {
        if !self.packages.iter().any(|p| p.manifest.id == id) {
            if self.reserved.iter().any(|blocked| blocked.id == id) {
                anyhow::bail!(
                    "plugin '{id}' was not registered because its id uses the reserved com.tasty. prefix; remove its folder from Plugins > Attention"
                );
            }
            anyhow::bail!("plugin '{id}' not installed");
        }
        if enabled {
            next.enable(id);
        } else {
            next.disable(id);
        }
        Ok(())
    }

    /// The host invokes this only for a successful file receipt, before publishing its reply.
    pub fn apply_config_change(
        &mut self,
        config: crate::PluginsConfig,
        change: &ConfigChange,
    ) -> anyhow::Result<()> {
        self.config = config;
        match change {
            ConfigChange::Enabled { id, enabled: true } => self.enable_committed(id)?,
            ConfigChange::Enabled { id, enabled: false } => self.disable_committed(id)?,
            ConfigChange::Shortcuts(_) => {}
        }
        Ok(())
    }
}
