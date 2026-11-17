use super::{document, intent::Intent};
use crate::registry_state::PluginsConfig;
use std::{collections::BTreeSet, io, path::PathBuf};

/// An immutable field-level update; trust and lifecycle barriers must not be coalesced with it.
#[derive(Clone)]
pub struct ConfigUpdate {
    path: PathBuf,
    before: toml::Table,
    after: toml::Table,
    intents: Vec<Intent>,
}
impl ConfigUpdate {
    pub fn between(before: &PluginsConfig, after: &PluginsConfig) -> io::Result<Self> {
        let path = before
            .origin
            .as_ref()
            .map(|o| o.path.clone())
            .or_else(PluginsConfig::path)
            .ok_or_else(|| io::Error::other("plugins.toml path unavailable"))?;
        if let Some(origin) = &before.origin {
            origin.baseline()?;
        }
        Ok(Self {
            path,
            before: document::encode(before)?,
            after: document::encode(after)?,
            intents: Vec::new(),
        })
    }
    /// Explicit commands apply to the latest locked document even when the cached value agrees.
    pub fn with_enabled_intent(mut self, id: &str, enabled: bool) -> Self {
        self.intents.push(Intent::Flag {
            section: "disabled",
            id: id.into(),
            present: !enabled,
        });
        self
    }
    pub fn with_builtin_removed_intent(mut self, id: &str, present: bool) -> Self {
        self.intents.push(Intent::Flag {
            section: "removed_builtins",
            id: id.into(),
            present,
        });
        self
    }
    pub fn with_restore_builtins_intent(mut self, ids: &[String], all: bool) -> Self {
        if all {
            self.intents.push(Intent::RestoreAll);
        } else {
            for id in ids {
                self = self.with_builtin_removed_intent(id, false);
            }
        }
        self
    }
    /// Combine adjacent, not-yet-executed snapshots of the same edit sequence.
    /// Ordered lifecycle/trust commands are barriers and must not use this operation.
    pub fn coalesce(self, newer: Self) -> io::Result<Self> {
        if !self.intents.is_empty()
            || !newer.intents.is_empty()
            || self.path != newer.path
            || self.after != newer.before
        {
            return Err(io::Error::other(
                "plugin snapshots do not share an edit sequence",
            ));
        }
        Ok(Self {
            path: self.path,
            before: self.before,
            after: newer.after,
            intents: Vec::new(),
        })
    }
    /// Bytes retained by the immutable request, for shared queue admission.
    pub fn retained_bytes(&self) -> usize {
        self.path
            .as_os_str()
            .len()
            .saturating_add(self.before.to_string().len())
            .saturating_add(self.after.to_string().len())
            .saturating_add(
                self.intents
                    .iter()
                    .map(Intent::retained_bytes)
                    .sum::<usize>(),
            )
    }
    pub fn execute(self) -> io::Result<PluginsConfig> {
        self.execute_with_change().map(|(config, _)| config)
    }
    /// Whether this operation changed the latest locked document, rather than the caller cache.
    pub fn execute_with_change(self) -> io::Result<(PluginsConfig, bool)> {
        let (mut committed, changed): (PluginsConfig, bool) =
            document::mutate(&self.path, |table| {
                let original = table.clone();
                let _: PluginsConfig = document::decode(table)?;
                for section in ["disabled", "removed_builtins"] {
                    merge_flags(table, &self.before, &self.after, section)?;
                }
                merge_shortcuts(table, &self.before, &self.after)?;
                for intent in &self.intents {
                    intent.apply(table)?;
                }
                Ok((document::decode(table)?, *table != original))
            })?;
        committed.origin = Some(document::Origin::new(
            self.path,
            Ok(document::encode(&committed)?),
        ));
        committed.replace_shortcut_overrides(committed.keybindings.clone());
        Ok((committed, changed))
    }
}
fn ids(table: &toml::Table, section: &str) -> BTreeSet<String> {
    document::at(table, &[section, "ids"])
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn merge_flags(
    table: &mut toml::Table,
    before: &toml::Table,
    after: &toml::Table,
    section: &str,
) -> io::Result<()> {
    let a = ids(before, section);
    let b = ids(after, section);
    if a == b {
        return Ok(());
    }
    let mut current = ids(table, section);
    for id in a.symmetric_difference(&b) {
        if b.contains(id) {
            current.insert(id.clone());
        } else {
            current.remove(id);
        }
    }
    let old = document::at(table, &[section, "ids"]).cloned();
    let new = toml::Value::Array(current.into_iter().map(toml::Value::String).collect());
    document::patch(table, &[section, "ids"], old.as_ref(), Some(&new))
}
fn keys<'a>(a: &'a toml::Table, b: &'a toml::Table, section: &str) -> BTreeSet<&'a String> {
    [a, b]
        .into_iter()
        .filter_map(|t| t.get(section).and_then(toml::Value::as_table))
        .flat_map(|t| t.keys())
        .collect()
}
fn merge_shortcuts(
    table: &mut toml::Table,
    before: &toml::Table,
    after: &toml::Table,
) -> io::Result<()> {
    for id in keys(before, after, "keybindings") {
        let empty = toml::Table::new();
        let a = document::at(before, &["keybindings", id])
            .and_then(toml::Value::as_table)
            .unwrap_or(&empty);
        let b = document::at(after, &["keybindings", id])
            .and_then(toml::Value::as_table)
            .unwrap_or(&empty);
        for command in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
            document::patch(
                table,
                &["keybindings", id, command],
                a.get(command),
                b.get(command),
            )?;
        }
    }
    Ok(())
}
pub(crate) fn save(config: &PluginsConfig) -> io::Result<()> {
    let before = match &config.origin {
        Some(origin) => document::decode(&origin.baseline()?)?,
        None => PluginsConfig::default(),
    };
    let mut before: PluginsConfig = before;
    before.origin = config.origin.clone();
    ConfigUpdate::between(&before, config)?.execute()?;
    if let Some(origin) = &config.origin {
        origin.advance(document::encode(config)?)?;
    }
    Ok(())
}
