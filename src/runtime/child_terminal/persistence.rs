//! Owned registry commands execute one complete read/merge/replace transaction.
pub(super) mod delta;

use super::{ParentKey, RegistryData};
use delta::Delta;
use std::path::{Path, PathBuf};
use tasty_utils::file_replace::{self, Fingerprint};

#[derive(Clone, serde::Serialize)]
pub(super) enum Operation {
    Merge,
    Reserve(ParentKey),
    Adopt(ParentKey, super::ChildEntry),
    Forget(u32),
    /// 필드 차이(delta)가 비어도 값을 대입한다. 같은 값을 다시 정한 요청이 앞선 저장의 실패에 묻히지 않고,
    /// 그 번호가 이미 다른 자식에게 넘어갔으면 저장하지 않고 실패한다.
    SetNickname {
        parent: ParentKey,
        index: u32,
        surface: u32,
        nickname: Option<String>,
    },
    #[cfg(feature = "gui")]
    ReleaseOwned(std::collections::HashSet<u32>),
    #[cfg(feature = "gui")]
    RetirePending {
        parent: Option<u32>,
        child: u32,
    },
    #[cfg(feature = "gui")]
    Prune(std::collections::HashSet<u32>),
    #[cfg(feature = "gui")]
    Clear,
}

pub(super) struct Command {
    path: PathBuf,
    owned: std::collections::HashSet<u32>,
    slot: Option<u32>,
    load_error: Option<String>,
    operation: Operation,
    delta: Delta,
}

pub(super) struct Outcome {
    pub(super) registry: RegistryData,
    pub(super) index: Option<u32>,
}

impl Command {
    pub(super) fn new(path: PathBuf, mut registry: RegistryData, operation: Operation) -> Self {
        let present = registry.surfaces();
        registry
            .slot_of
            .retain(|surface, _| present.contains(surface));
        if let Some(slot) = registry.slot {
            for surface in present.intersection(&registry.owned) {
                registry.slot_of.insert(*surface, slot);
            }
        }
        let empty = RegistryData::default();
        let before = registry.baseline.as_deref().unwrap_or(&empty);
        let delta = Delta::between(before, &registry);
        Self {
            path,
            owned: registry.owned,
            slot: registry.slot,
            load_error: registry.load_error,
            operation,
            delta,
        }
    }

    pub(super) fn execute(self) -> Result<Outcome, String> {
        self.execute_inner()
            .map_err(|error| format!("child registry: {error:#}"))
    }

    fn execute_inner(self) -> anyhow::Result<Outcome> {
        if let Some(error) = &self.load_error {
            anyhow::bail!("original load failed: {error}");
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let target = file_replace::resolve_link(&self.path)?;
        let _participant = file_replace::lock(&self.path)?;
        if target != file_replace::resolve_link(&self.path)? {
            anyhow::bail!("registry target changed while acquiring participant lock");
        }
        let (mut disk, observed) = read_observed(&self.path)?;
        #[cfg(feature = "gui")]
        let retired_bounds = retirement_bounds(&self.operation, &disk);
        self.delta.apply(&mut disk);
        #[cfg(feature = "gui")]
        let protected_counters: Vec<_> = retired_bounds
            .into_iter()
            .filter_map(|(parent, bound)| {
                let next = *disk.next_index.get(&parent)?;
                bound
                    .is_none_or(|bound| next > bound)
                    .then_some((parent, next))
            })
            .collect();
        disk.path = Some(self.path.clone());
        disk.owned = self.owned;
        disk.slot = self.slot;
        let index = disk
            .apply_operation(self.operation)
            .map_err(anyhow::Error::msg)?;
        #[cfg(feature = "gui")]
        for (parent, next) in protected_counters {
            let current = disk.next_index.entry(parent).or_default();
            *current = (*current).max(next);
        }
        // Atomic adopt creates identities after the input delta is built. Tag those new owned
        // identities in this same transaction, including a counter reserved before registration.
        let present = disk.surfaces();
        disk.slot_of.retain(|surface, _| present.contains(surface));
        if let Some(slot) = disk.slot {
            for surface in present.intersection(&disk.owned) {
                disk.slot_of.insert(*surface, slot);
            }
        }
        let bytes = serde_json::to_vec_pretty(&disk)?;
        if target != file_replace::resolve_link(&self.path)?
            || observed != Fingerprint::read(&self.path)?
        {
            anyhow::bail!("registry changed outside the participant lock; not replaced");
        }
        file_replace::replace(&self.path, &bytes)?;
        disk.checkpoint();
        Ok(Outcome {
            registry: disk,
            index,
        })
    }
}

#[cfg(feature = "gui")]
fn retirement_bounds(
    operation: &Operation,
    disk: &RegistryData,
) -> std::collections::HashMap<u32, Option<u32>> {
    let gone = match operation {
        Operation::ReleaseOwned(gone) => gone.clone(),
        Operation::RetirePending { parent, child } => {
            let mut gone = std::collections::HashSet::new();
            gone.insert(*child);
            gone.extend(parent);
            gone
        }
        _ => return Default::default(),
    };
    gone.iter()
        .map(|parent| {
            let bound = disk
                .children
                .get(parent)
                .into_iter()
                .flatten()
                .filter(|child| gone.contains(&child.child_surface_id))
                .map(|child| child.index.saturating_add(1))
                .max();
            (*parent, bound)
        })
        .collect()
}

pub(super) fn read(path: &Path) -> Result<RegistryData, String> {
    read_observed(path)
        .map(|(registry, _)| registry)
        .map_err(|error| format!("{error:#}"))
}

fn read_observed(path: &Path) -> anyhow::Result<(RegistryData, Fingerprint)> {
    match std::fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            anyhow::bail!("registry target is not a regular file")
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((RegistryData::default(), Fingerprint::Missing));
        }
        Err(error) => anyhow::bail!("registry metadata failed ({:?})", error.kind()),
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((RegistryData::default(), Fingerprint::Missing));
        }
        Err(error) => anyhow::bail!("registry read failed ({:?})", error.kind()),
    };
    // Never include JSON excerpts or registry payloads in diagnostics.
    let registry =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("registry parse failed"))?;
    Ok((registry, Fingerprint::of_bytes(&bytes)))
}

#[cfg(test)]
mod tests;
