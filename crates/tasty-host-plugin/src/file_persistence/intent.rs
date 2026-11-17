use super::document;
use std::{collections::BTreeSet, io};

#[derive(Clone)]
pub(super) enum Intent {
    Flag {
        section: &'static str,
        id: String,
        present: bool,
    },
    RestoreAll,
}
impl Intent {
    pub(super) fn retained_bytes(&self) -> usize {
        match self {
            Self::Flag { id, .. } => id.len(),
            Self::RestoreAll => 0,
        }
    }
    pub(super) fn apply(&self, table: &mut toml::Table) -> io::Result<()> {
        match self {
            Self::Flag {
                section,
                id,
                present,
            } => edit_set(table, &[section, "ids"], id, *present),
            Self::RestoreAll => replace_set(table, &["removed_builtins", "ids"], BTreeSet::new()),
        }
    }
}
fn edit_set(table: &mut toml::Table, path: &[&str], value: &str, present: bool) -> io::Result<()> {
    let mut values: BTreeSet<String> = document::at(table, path)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect();
    if present {
        values.insert(value.to_owned());
    } else {
        values.remove(value);
    }
    replace_set(table, path, values)
}
fn replace_set(table: &mut toml::Table, path: &[&str], values: BTreeSet<String>) -> io::Result<()> {
    let before = document::at(table, path).cloned();
    if before.is_none() && values.is_empty() {
        return Ok(());
    }
    let after = toml::Value::Array(values.into_iter().map(toml::Value::String).collect());
    document::patch(table, path, before.as_ref(), Some(&after))
}
