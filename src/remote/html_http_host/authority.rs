//! Capture the exact surface's ready workspace grant, registration and authoritative source.
use super::*;
use crate::{plugin_bridge::remote_surface::RemoteSurface, remote::transfer_spool::TransferOwner};
use tasty_ipc::html_transfer::HtmlSource;

pub(super) struct Authority {
    pub(super) binding: Weak<()>,
    owner: TransferOwner,
    pub(super) publication: Arc<dyn Fn() -> bool + Send + Sync>,
    secure: bool,
    workspace: u32,
    grant: u64,
    source: Weak<Mutex<tasty_remote::html_source::HtmlSourceState>>,
    pub(super) generation: u64,
    entry: TransferId,
    url: String,
    entry_url: String,
    allow_remote_content: bool,
}
fn grant(engine: &EngineRef<'_>, client: u32, surface: u32) -> Option<(u32, u64)> {
    let workspace = engine.live.occupancy.workspace_of_surface(surface)?;
    engine
        .live
        .occupancy
        .workspaces_snapshot()
        .into_iter()
        .find_map(|(id, grant)| {
            (id == workspace && grant.holder == client && grant.ready)
                .then_some((id, grant.granted_seq))
        })
}
fn surface<'a>(engine: &'a EngineRef<'_>, id: u32) -> Option<&'a RemoteSurface> {
    engine
        .find_surface_by_id(id)?
        .as_any()
        .downcast_ref::<RemoteSurface>()
        .filter(|s| s.capabilities.remote_http_serve)
}
impl Authority {
    pub(super) fn capture(
        engine: &EngineRef<'_>,
        hub: &StreamHub,
        client: u32,
        request: &HttpRequest,
    ) -> core::Result<Self> {
        let HttpRequest::Begin {
            id,
            source_generation,
            logical_url,
            allow_remote_content,
            ..
        } = request
        else {
            return Err(core::Error::Stale);
        };
        if !hub.html_http_enabled(client, false) {
            return Err(core::Error::Stale);
        }
        let (workspace, grant) = grant(engine, client, id.surface_id).ok_or(core::Error::Stale)?;
        let surface = surface(engine, id.surface_id).ok_or(core::Error::Stale)?;
        let (HtmlSource::Http { url }, generation) = surface.html_source_info() else {
            return Err(core::Error::UnsupportedScheme);
        };
        if *source_generation != generation {
            return Err(core::Error::Stale);
        }
        let mut expected = reqwest::Url::parse(&url).map_err(|_| core::Error::Stale)?;
        let mut supplied = reqwest::Url::parse(logical_url).map_err(|_| core::Error::Stale)?;
        if !hub.html_http_enabled(client, supplied.scheme() == "https") {
            return Err(core::Error::UnsupportedScheme);
        }
        expected.set_fragment(None);
        supplied.set_fragment(None);
        if expected != supplied {
            return Err(core::Error::Stale);
        }
        let (grant_life, member_life) = engine
            .live
            .occupancy
            .workspace_publication(id.surface_id, client, grant)
            .ok_or(core::Error::Stale)?;
        let source_life = Arc::downgrade(&surface.html_source);
        let source_url = url.clone();
        let poison_reported = AtomicBool::new(false);
        let publication = Arc::new(move || {
            if grant_life.upgrade().is_none() || member_life.upgrade().is_none() {
                return false;
            }
            let Some(source) = source_life.upgrade() else {
                return false;
            };
            match source.lock() {
                Ok(state) => {
                    state.generation == generation
                        && matches!(&state.source, Some(HtmlSource::Http { url }) if url == &source_url)
                }
                Err(_) => {
                    if !poison_reported.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        tracing::error!("HTTP source poisoned; queued publication refused");
                    }
                    false
                }
            }
        });
        Ok(Self {
            publication,
            binding: hub.client_binding(client).ok_or(core::Error::Stale)?,
            owner: TransferOwner::capture(engine, hub, client, false).ok_or(core::Error::Stale)?,
            secure: expected.scheme() == "https",
            workspace,
            grant,
            source: Arc::downgrade(&surface.html_source),
            generation,
            entry: *id,
            url,
            entry_url: logical_url.clone(),
            allow_remote_content: *allow_remote_content,
        })
    }
    pub(super) fn successor(
        &self,
        engine: &EngineRef<'_>,
        hub: &StreamHub,
        client: u32,
        request: &HttpRequest,
    ) -> core::Result<Self> {
        let HttpRequest::Begin {
            id,
            logical_url,
            allow_remote_content,
            ..
        } = request
        else {
            return Err(core::Error::Stale);
        };
        let old = self.entry;
        if !self.current(engine, hub, client)
            || (id.epoch, id.surface_id, id.binding) != (old.epoch, old.surface_id, old.binding)
            || id.document <= old.document
            || *allow_remote_content != self.allow_remote_content
        {
            return Err(core::Error::Stale);
        }
        // Re-capture the real original source/grant; the host has separately proven this URL
        // against the predecessor's completed request before entering this narrow path.
        let mut original = request.clone();
        if let HttpRequest::Begin { logical_url, .. } = &mut original {
            *logical_url = self.url.clone();
        }
        let mut captured = Self::capture(engine, hub, client, &original)?;
        captured.entry_url = logical_url.clone();
        captured.secure = reqwest::Url::parse(logical_url)
            .map_err(|_| core::Error::Stale)?
            .scheme()
            == "https";
        Ok(captured)
    }
    pub(super) fn current(&self, engine: &EngineRef<'_>, hub: &StreamHub, client: u32) -> bool {
        if !(self.publication)()
            || !hub.html_http_enabled(client, self.secure)
            || !self.owner.current(engine, hub, client)
            || !hub.matches_client_binding(client, &self.binding)
            || grant(engine, client, self.entry.surface_id) != Some((self.workspace, self.grant))
        {
            return false;
        }
        let Some(surface) = surface(engine, self.entry.surface_id) else {
            return false;
        };
        if !self.source.ptr_eq(&Arc::downgrade(&surface.html_source)) {
            return false;
        }
        let Some(source) = self.source.upgrade() else {
            return false;
        };
        match source.lock() {
            Ok(state) => {
                state.generation == self.generation
                    && matches!(&state.source, Some(HtmlSource::Http { url }) if url == &self.url)
            }
            Err(_) => {
                tracing::error!("HTTP source poisoned; refusing publication");
                false
            }
        }
    }
    pub(super) fn permits(
        &self,
        request: &HttpRequest,
        hub: &StreamHub,
        client: u32,
    ) -> core::Result<()> {
        let HttpRequest::Begin {
            source_generation,
            allow_remote_content,
            upload,
            logical_url,
            ..
        } = request
        else {
            return Err(core::Error::Stale);
        };
        if *source_generation != self.generation {
            return Err(core::Error::Stale);
        }
        let url = reqwest::Url::parse(logical_url).map_err(|_| core::Error::Invalid("HTTP URL"))?;
        if !hub.html_http_enabled(client, url.scheme() == "https") {
            return Err(core::Error::UnsupportedScheme);
        }

        if *allow_remote_content != self.allow_remote_content {
            return Err(core::Error::Invalid("document origin policy changed"));
        }
        if upload.checked_len().map_err(core::Error::Invalid)? > setup::limits().upload_bytes {
            return Err(core::Error::Quota("upload bytes"));
        }
        Ok(())
    }
    pub(super) fn bridge_capture(&self) -> bridge::Capture {
        bridge::Capture {
            entry: self.entry,
            source_generation: self.generation,
            entry_url: self.entry_url.clone(),
            allow_remote_content: self.allow_remote_content,
        }
    }
}
