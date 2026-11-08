pub(crate) mod import;
pub(crate) mod legacy_export;

use tasty_core::DataRef;
#[cfg(test)]
use tasty_core::canonical::{CanonData, ResolveData, fnv_hex};
use tasty_event_store::{EventStore, PayloadRef};
/// 저장소의 payload 바이트를 그대로 해시한다.
#[cfg(test)]
pub(crate) struct StoreBytes<'a>(pub(crate) &'a EventStore);

#[cfg(test)]
impl ResolveData for StoreBytes<'_> {
    fn resolve(&self, data: DataRef) -> CanonData {
        match self.0.read_payload(PayloadRef(data.0)) {
            Ok(bytes) => CanonData::Bytes {
                len: bytes.len(),
                hash: fnv_hex(&bytes),
            },
            Err(error) => CanonData::Unreadable(error.to_string()),
        }
    }
}

/// A frozen immutable source. No runtime instance or activation callback crosses the worker queue.
#[derive(Debug, Clone)]
pub(crate) struct SavedSurfaceSource {
    pub(crate) kind: String,
    pub(crate) data: Option<DataRef>,
    pub(crate) creation_seed: Option<DataRef>,
}

pub(crate) fn preset_surface(
    store: &EventStore,
    source: &SavedSurfaceSource,
    remaining: &mut usize,
) -> Result<tasty_presets::PresetSurface, String> {
    use crate::core::layout_persistence::import::surface_data::SurfaceData;
    let mut surface = tasty_presets::PresetSurface {
        id: None,
        kind: source.kind.clone(),
        cwd: None,
        startup_command: None,
        params: serde_json::json!({}),
    };
    if let Some(reference) = source.data {
        let bytes = store
            .read_payload_bounded(PayloadRef(reference.0), *remaining)
            .map_err(|error| error.to_string())?;
        *remaining = remaining.saturating_sub(bytes.len());
        match SurfaceData::decode(&bytes).map_err(|error| error.to_string())? {
            SurfaceData::Terminal { cwd, .. } if source.kind == "terminal" => surface.cwd = cwd,
            SurfaceData::Generic { data, .. } if source.kind != "terminal" => surface.params = data,
            _ => return Err("preset snapshot kind differs from the frozen source".into()),
        }
    } else if let Some(reference) = source.creation_seed {
        let bytes = store
            .read_payload_bounded(PayloadRef(reference.0), *remaining)
            .map_err(|error| error.to_string())?;
        *remaining = remaining.saturating_sub(bytes.len());
        let seed: crate::runtime::journal_product::PreparationInput =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if seed.kind != source.kind {
            return Err("preset creation seed belongs to another kind".into());
        }
        surface.cwd = seed.cwd.map(|cwd| cwd.to_string_lossy().into_owned());
        if source.kind != "terminal" {
            surface.params = seed.restore.unwrap_or(seed.params);
        }
        // A terminal preset never replays the old shell's launch arguments, stdin or restore command.
    }
    Ok(surface)
}

/// In-process readers are registered before the App can acknowledge another publication. The
/// storage worker holds this lock while transferring references to durable snapshot pins and GC.
#[derive(Debug, Default)]
pub(crate) struct PayloadReaders {
    state: std::sync::Mutex<(u64, std::collections::BTreeMap<u64, Vec<DataRef>>)>,
}
#[derive(Debug, Clone)]
pub(crate) struct PayloadReadLease {
    // The last clone releases the reader ticket through ReadLease::drop.
    _owner: std::sync::Arc<ReadLease>,
}
#[derive(Debug)]
struct ReadLease {
    owner: std::sync::Arc<PayloadReaders>,
    ticket: u64,
}
impl Drop for ReadLease {
    fn drop(&mut self) {
        match self.owner.state.lock() {
            Ok(mut state) => {
                state.1.remove(&self.ticket);
            }
            Err(error) => tracing::error!("payload reader release failed; pins retained: {error}"),
        }
    }
}
impl PayloadReaders {
    pub(crate) fn lease(
        self: &std::sync::Arc<Self>,
        references: Vec<DataRef>,
    ) -> Result<PayloadReadLease, String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        state.0 = state
            .0
            .checked_add(1)
            .ok_or("payload reader tickets exhausted")?;
        let ticket = state.0;
        state.1.insert(ticket, references);
        Ok(PayloadReadLease {
            _owner: std::sync::Arc::new(ReadLease {
                owner: self.clone(),
                ticket,
            }),
        })
    }
    pub(crate) fn with_refs<T>(
        &self,
        read: impl FnOnce(Vec<PayloadRef>) -> Result<T, String>,
    ) -> Result<T, String> {
        let state = self.state.lock().map_err(|error| error.to_string())?;
        let references = state
            .1
            .values()
            .flatten()
            .map(|reference| PayloadRef(reference.0))
            .collect();
        read(references)
    }
}
