//! Engine-owned bounded HTML workers. Main loops deliver owned commands, never file I/O.
#[cfg(feature = "gui")]
mod control;
mod resources;
#[cfg(feature = "gui")]
use control::refuse_http;

use crate::{
    model::Surface,
    plugin_bridge::remote_surface::RemoteSurface,
    remote::{html_files::Entry, transfer_spool::TransferOwner},
    runtime::engine_access::{EngineMut, EngineRef},
};
use base64::Engine;
use std::{
    collections::HashMap,
    io::Read,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tasty_ipc::{
    html_transfer::{
        AUTO_BYTES, CHUNK_BYTES, HtmlMetadata, HtmlReply, HtmlRequest, HtmlSource, TransferId,
    },
    stream_hub::{PushResult, StreamHub},
};

// Capacity is held until actual join, including after an engine disappears during file I/O.
static WORKER_COUNT: AtomicUsize = AtomicUsize::new(0);
static RETIRED: Mutex<Vec<(JoinHandle<()>, WorkerPermit)>> = Mutex::new(Vec::new());
static SOURCE_POISONED: AtomicBool = AtomicBool::new(false);
static RETIRED_POISONED: AtomicBool = AtomicBool::new(false);
struct WorkerPermit;
impl WorkerPermit {
    fn acquire() -> Option<Self> {
        reap_retired();
        WORKER_COUNT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 8).then_some(count + 1)
            })
            .ok()
            .map(|_| Self)
    }
}
impl Drop for WorkerPermit {
    fn drop(&mut self) {
        WORKER_COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}
fn reap_retired() {
    let mut retired =
        crate::poison::recover_mutex(RETIRED.lock(), "retired HTML workers", &RETIRED_POISONED);
    let mut pending = Vec::new();
    for (thread, permit) in retired.drain(..) {
        if thread.is_finished() {
            if thread.join().is_err() {
                tracing::warn!("retired HTML worker panicked");
            }
            drop(permit);
        } else {
            pending.push((thread, permit));
        }
    }
    *retired = pending;
}

struct Worker {
    permit: WorkerPermit,
    owner: TransferOwner,
    source: Weak<Mutex<tasty_remote::html_source::HtmlSourceState>>,
    source_generation: u64,
    commands: SyncSender<HtmlRequest>,
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}
#[derive(Default)]
pub(crate) struct HtmlTransfers {
    workers: HashMap<(u32, TransferId), Worker>,
    roots: resources::Roots,
}
impl Drop for HtmlTransfers {
    fn drop(&mut self) {
        for (_, w) in self.workers.drain() {
            w.cancel.store(true, Ordering::Release);
            if w.thread.is_finished() {
                if w.thread.join().is_err() {
                    tracing::warn!("HTML worker panicked during cleanup");
                }
            } else {
                crate::poison::recover_mutex(
                    RETIRED.lock(),
                    "retired HTML workers",
                    &RETIRED_POISONED,
                )
                .push((w.thread, w.permit));
            }
        }
    }
}
impl HtmlTransfers {
    fn command(&mut self, key: (u32, TransferId), owner: &TransferOwner, msg: HtmlRequest) {
        if matches!(msg, HtmlRequest::HtmlCancel { .. }) {
            self.roots.cancel_entry(key.0, key.1);
        }
        let Some(worker) = self.workers.get(&key) else {
            return;
        };
        if !worker.owner.same(owner) || !source_current(&worker.source, worker.source_generation) {
            worker.cancel.store(true, Ordering::Release);
            return;
        }
        if matches!(msg, HtmlRequest::HtmlCancel { .. }) || worker.commands.try_send(msg).is_err() {
            worker.cancel.store(true, Ordering::Release);
        }
    }
    fn collect(&mut self, engine: &EngineRef<'_>, hub: &StreamHub) {
        reap_retired();
        self.roots.collect(engine, hub);
        for (key, w) in std::mem::take(&mut self.workers) {
            if !w.owner.current(engine, hub, key.0)
                || !source_current(&w.source, w.source_generation)
            {
                w.cancel.store(true, Ordering::Release);
            }
            if w.thread.is_finished() {
                if w.thread.join().is_err() {
                    tracing::warn!("HTML transfer worker panicked");
                }
            } else {
                self.workers.insert(key, w);
            }
        }
    }
}
impl EngineMut<'_> {
    pub(crate) fn handle_html_control(
        &mut self,
        hub: &StreamHub,
        client: u32,
        msg: tasty_ipc::stream_hub::HtmlControl,
    ) {
        match msg {
            tasty_ipc::stream_hub::HtmlControl::File(msg) => {
                self.handle_html_request(hub, client, msg)
            }
            tasty_ipc::stream_hub::HtmlControl::Http(msg) => {
                self.handle_http_request(hub, client, msg)
            }
        }
    }
    pub(crate) fn poll_html_transfers(&mut self) {
        let Some(hub) = self.remote.notifier() else {
            return;
        };
        let mut transfers = std::mem::take(&mut self.remote.html_transfers);
        transfers.collect(&self.as_ref(), &hub);
        self.remote.html_transfers = transfers;
        self.poll_http_transfers(&hub);
    }
    pub(crate) fn handle_html_request(&mut self, hub: &StreamHub, client: u32, msg: HtmlRequest) {
        if matches!(msg, HtmlRequest::HtmlCapabilities { .. }) {
            if let Some(workspace) = self.live.occupancy.workspace_held_by(client) {
                self.remote.mark_structure_changed(workspace);
            }
            return;
        }
        let Some(id) = msg.id() else {
            return;
        };
        if !hub.html_transfer_enabled(client) {
            return;
        }
        let Some(binding) = hub.client_binding(client) else {
            return;
        };
        let Some(owner) = TransferOwner::capture(&self.as_ref(), hub, client, false) else {
            send(
                hub,
                client,
                &binding,
                HtmlReply::HtmlFailure {
                    id,
                    reason: "client does not hold a workspace attach".into(),
                },
            );
            return;
        };
        self.poll_html_transfers();
        let key = (client, id);
        if !matches!(
            msg,
            HtmlRequest::HtmlEntryRequest { .. }
                | HtmlRequest::HtmlResourceRequest { .. }
                | HtmlRequest::HtmlFileEntryRequest { .. }
                | HtmlRequest::HtmlNavigateRequest { .. }
        ) {
            self.remote.html_transfers.command(key, &owner, msg);
            return;
        }
        if !hub.admit_html_request(client, &binding, id) {
            if let Some(worker) = self.remote.html_transfers.workers.get(&key) {
                worker.cancel.store(true, Ordering::Release);
            }
            send(
                hub,
                client,
                &binding,
                HtmlReply::HtmlFailure {
                    id,
                    reason:
                        "HTML request and transfer identities must increase within a connection"
                            .into(),
                },
            );
            return;
        }
        if self.remote.html_transfers.workers.contains_key(&key)
            || self.remote.html_transfers.workers.len() >= 8
        {
            send(
                hub,
                client,
                &binding,
                HtmlReply::HtmlFailure {
                    id,
                    reason: "HTML transfer capacity or duplicate identity".into(),
                },
            );
            return;
        }
        let Some(surface) = self
            .as_ref()
            .find_surface_by_id(id.surface_id)
            .and_then(|s| s.as_any().downcast_ref::<RemoteSurface>())
        else {
            send(
                hub,
                client,
                &binding,
                HtmlReply::HtmlFailure {
                    id,
                    reason: "unknown HTML surface".into(),
                },
            );
            return;
        };
        if !surface.capabilities.remote_http_serve {
            send(
                hub,
                client,
                &binding,
                HtmlReply::HtmlFailure {
                    id,
                    reason: "surface is not an HTML source".into(),
                },
            );
            return;
        }
        let (source, generation) = surface.html_source_info();
        let source_state = Arc::downgrade(&surface.html_source);
        let cwd = surface
            .source_cwd()
            .map(|p| p.to_string_lossy().into_owned());
        let Some(permit) = WorkerPermit::acquire() else {
            send(
                hub,
                client,
                &binding,
                HtmlReply::HtmlFailure {
                    id,
                    reason: "HTML worker capacity exhausted".into(),
                },
            );
            return;
        };
        let lease = match self.remote.html_transfers.roots.lease(
            client,
            id,
            &msg,
            owner.clone(),
            source_state.clone(),
            generation,
        ) {
            Ok(lease) => lease,
            Err(reason) => {
                send(hub, client, &binding, HtmlReply::HtmlFailure { id, reason });
                return;
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let context = Context {
            hub: hub.clone(),
            client,
            binding,
            source_state: source_state.clone(),
            generation,
            cancel: cancel.clone(),
            id,
            idle_timeout: Duration::from_secs(30),
            lease,
        };
        let entry_request = !matches!(msg, HtmlRequest::HtmlResourceRequest { .. });
        match std::thread::Builder::new()
            .name("html-transfer".into())
            .spawn(move || context.run(source, cwd, rx))
        {
            Ok(thread) => {
                self.remote.html_transfers.workers.insert(
                    key,
                    Worker {
                        permit,
                        owner,
                        source: source_state,
                        source_generation: generation,
                        commands: tx,
                        cancel,
                        thread,
                    },
                );
            }
            Err(error) => {
                tracing::warn!(%error,"HTML worker spawn failed");
                if entry_request {
                    self.remote.html_transfers.roots.cancel_entry(client, id);
                }
                if let Some(binding) = hub.client_binding(client) {
                    send(
                        hub,
                        client,
                        &binding,
                        HtmlReply::HtmlFailure {
                            id,
                            reason: "HTML worker unavailable".into(),
                        },
                    );
                }
            }
        }
    }
}
fn source_current(
    source: &Weak<Mutex<tasty_remote::html_source::HtmlSourceState>>,
    generation: u64,
) -> bool {
    source.upgrade().is_some_and(|source| match source.lock() {
        Ok(state) => state.generation == generation && state.source.is_some(),
        Err(error) => {
            if !SOURCE_POISONED.swap(true, Ordering::Relaxed) {
                tracing::error!(%error,"HTML source mutex poisoned; cancelling transfer");
            }
            false
        }
    })
}

fn send(hub: &StreamHub, client: u32, binding: &Weak<()>, reply: HtmlReply) -> bool {
    match reply.frame() {
        Ok(frame) => {
            if hub.push_bound(client, binding, frame) == PushResult::Sent {
                true
            } else {
                hub.unregister_bound(client, binding);
                false
            }
        }
        Err(error) => {
            tracing::warn!(%error,"HTML reply encoding failed");
            hub.unregister_bound(client, binding);
            false
        }
    }
}
#[cfg(feature = "gui")]
pub(crate) fn refuse(hub: &StreamHub, client: u32, msg: tasty_ipc::stream_hub::HtmlControl) {
    let msg = match msg {
        tasty_ipc::stream_hub::HtmlControl::File(msg) => msg,
        tasty_ipc::stream_hub::HtmlControl::Http(msg) => {
            refuse_http(hub, client, msg);
            return;
        }
    };
    if hub.html_transfer_enabled(client)
        && let Some(id) = msg.id()
        && let Some(binding) = hub.client_binding(client)
    {
        send(
            hub,
            client,
            &binding,
            HtmlReply::HtmlFailure {
                id,
                reason: "client does not hold a workspace attach".into(),
            },
        );
    }
}
struct Context {
    hub: StreamHub,
    client: u32,
    binding: Weak<()>,
    source_state: Weak<Mutex<tasty_remote::html_source::HtmlSourceState>>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    id: TransferId,
    idle_timeout: Duration,
    lease: resources::Lease,
}
impl Context {
    fn send(&self, reply: HtmlReply) -> bool {
        send(&self.hub, self.client, &self.binding, reply)
    }
    fn active(&self) -> bool {
        !self.cancel.load(Ordering::Acquire)
            && !self.lease.cancel.load(Ordering::Acquire)
            && self.hub.matches_client_binding(self.client, &self.binding)
            && self.hub.html_transfer_enabled(self.client)
            && source_current(&self.source_state, self.generation)
    }
    fn run(self, source: HtmlSource, cwd: Option<String>, rx: Receiver<HtmlRequest>) {
        let result = self.transfer(source, cwd, rx);
        if let Err(reason) = result {
            self.send(HtmlReply::HtmlFailure {
                id: self.id,
                reason,
            });
        }
    }
    fn transfer(
        &self,
        source: HtmlSource,
        cwd: Option<String>,
        rx: Receiver<HtmlRequest>,
    ) -> Result<(), String> {
        let HtmlSource::File { path, .. } = &source else {
            if self.active() {
                self.send(HtmlReply::HtmlMetadata {
                    id: self.id,
                    metadata: HtmlMetadata {
                        source: source.clone(),
                        source_generation: self.generation,
                        root: None,
                        cwd,
                        size: 0,
                        regular: false,
                        file: None,
                    },
                });
            }
            return Err(match source {
                HtmlSource::Http { .. } => "HTTP source requires remote proxy",
                _ => "HTML source is not a file",
            }
            .into());
        };
        let file_metadata = self.lease.file_metadata(path)?;
        let (entry, source) = self.lease.open(path, source.clone())?;
        let cwd = if self.lease.nested() {
            match &source {
                HtmlSource::File { path, .. } => std::path::Path::new(path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned()),
                _ => cwd,
            }
        } else {
            cwd
        };
        let mut file = &entry.file;
        let size = entry.file.metadata().map_err(|e| e.to_string())?.len();
        if !self.active() {
            self.send(HtmlReply::HtmlCancelled { id: self.id });
            return Ok(());
        }
        if !self.send(HtmlReply::HtmlMetadata {
            id: self.id,
            metadata: HtmlMetadata {
                source,
                source_generation: self.generation,
                root: Some(entry.root.to_string_lossy().into_owned()),
                cwd,
                size,
                regular: true,
                file: Some(file_metadata),
            },
        }) {
            return Ok(());
        }
        let mut offset = 0;
        let mut allowed = size <= AUTO_BYTES;
        let mut idle = Instant::now();
        loop {
            if !self.active() {
                self.send(HtmlReply::HtmlCancelled { id: self.id });
                return Ok(());
            }
            if idle.elapsed() > self.idle_timeout {
                return Err("HTML credit timeout".into());
            }
            let msg = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(m) => m,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Ok(()),
            };
            let HtmlRequest::HtmlRead {
                id,
                offset: next,
                allow,
            } = msg
            else {
                return Ok(());
            };
            if id != self.id || next != offset {
                return Err("HTML credit offset mismatch".into());
            }
            allowed |= allow;
            if !allowed {
                return Err("large HTML entry requires size allowance".into());
            }
            if !entry.unchanged() || !self.lease.unchanged() {
                return Err("HTML source changed; request fresh metadata".into());
            }
            let len = (size - offset).min(CHUNK_BYTES as u64) as usize;
            let mut bytes = vec![0; len];
            file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
            if !entry.unchanged() || !self.lease.unchanged() {
                return Err("HTML source changed while reading".into());
            }
            if !self.active() {
                self.send(HtmlReply::HtmlCancelled { id: self.id });
                return Ok(());
            }
            if len > 0
                && !self.send(HtmlReply::HtmlChunk {
                    id: self.id,
                    offset,
                    data_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                })
            {
                return Ok(());
            }
            offset += len as u64;
            idle = Instant::now();
            if offset == size {
                self.send(HtmlReply::HtmlComplete { id: self.id, size });
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests;
