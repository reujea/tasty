//! Register actual App documents with the connection table only after the browser's first Begin.
use super::*;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::{app::http_session::HttpSession, runtime::engine_access::EngineMut};
use std::sync::Arc;
use tasty_remote::client_session::AttachClientSession;

pub(in crate::app::attach_client) fn source_changed(
    sess: &AttachClientSession,
    engine: &mut EngineMut<'_>,
    remote: u32,
) {
    let Some(surface) = sess
        .state
        .remote_to_local
        .get(&remote)
        .and_then(|local| engine.runtime.surfaces.get_mut(local))
        .and_then(|surface| surface.as_any_mut().downcast_mut::<RemoteSurface>())
    else {
        return;
    };
    let Some(session) = surface.http_session.take() else {
        return;
    };
    // A source notification is not a native Reload continuation. Retire its receipt
    // before fetching new metadata, and let the old native allocation be destroyed.
    session.close();
    session.cancel_retained();
    if let Some(mut mirror) = surface.lock_html_mirror() {
        mirror.user_reload();
    }
    surface.with_html_script(|state| {
        *state = tasty_model::html_script::HtmlScriptState::new(state.sandbox());
        state.on_host_load_requested();
        state.on_load_started();
    });
}
pub(super) fn synchronize(sess: &mut AttachClientSession, engine: &mut EngineMut<'_>) -> bool {
    let settings_source =
        crate::runtime::surface_registry::webview_kind::webview_settings_source("html");
    let settings = crate::webview::HtmlWebViewSettings::resolve(
        settings_source.as_ref(),
        &engine.runtime.settings,
    );
    let available = engine.runtime.http_viewer_available
        && sess.state.resync_pending.is_none()
        && !sess
            .transport
            .disconnected
            .load(std::sync::atomic::Ordering::Acquire)
        && sess.transport.frame_tx.epoch().is_active()
        && sess.transport.html.server_http;
    let projection = engine
        .read()
        .mirror_projection_token(sess.state.local_workspace);
    let mut sessions = Vec::new();
    for (&remote, &local) in &sess.state.remote_to_local {
        let Some(surface) = engine
            .runtime
            .surfaces
            .get_mut(&local)
            .and_then(|s| s.as_any_mut().downcast_mut::<RemoteSurface>())
        else {
            continue;
        };
        if let Some(session) = refresh(
            surface,
            sess,
            available,
            settings.allow_remote_content,
            engine.runtime.waker.clone(),
            projection.clone().unwrap_or_default(),
        ) {
            sessions.push((remote, session));
        }
    }
    drive_sessions(sess, engine, sessions, settings.allow_remote_content)
}
fn drive_sessions(
    sess: &mut AttachClientSession,
    engine: &mut EngineMut<'_>,
    sessions: Vec<(u32, Arc<HttpSession>)>,
    allow_remote_content: bool,
) -> bool {
    let mut changed = false;
    for (remote, session) in sessions {
        if session.failed() {
            continue;
        }
        if let Err(error) = session.poll(engine.runtime.waker.clone(), allow_remote_content) {
            tracing::warn!(%error,"remote HTTP App preparation failed");
            session.fail();
            changed = true;
            continue;
        }
        let id = session.source.metadata_id();
        changed |= session.drive(|| {
            sess.transport
                .html
                .entry_id(remote, id.binding, id.document)
        });
        if let Some(document) = session.bridge.document()
            && let Err(error) = sess.transport.html.http_documents.insert(document)
        {
            tracing::warn!(?error, "remote HTTP document registration refused");
            session.close();
            changed = true;
        }
    }
    changed
}

fn retire_unavailable(surface: &mut RemoteSurface) {
    let connected = surface
        .html_mirror
        .as_ref()
        .is_some_and(|mirror| lock_mirror(mirror).connected);
    if connected
        && surface
            .http_session
            .as_ref()
            .is_some_and(|session| session.retaining())
    {
        return;
    }
    if let Some(session) = &surface.http_session {
        session.close();
        if !connected && session.entry_body().is_some() {
            return;
        }
    }
    surface.http_session.take();
}
/// Whether the surface's HTTP session gives way to a new one for the current source.
///
/// A failed session stays as a stable failure while its source is unchanged. Dropping its view
/// revokes the native owner, and treating that as a reason to replace it reopened the same
/// failing document in a loop. A user Reload, a new source or a reconnect retire the source and
/// start a new session through the other paths.
fn replaces_session(failed: bool, native_owner_current: bool, source_changed: bool) -> bool {
    source_changed || (!failed && !native_owner_current)
}
fn refresh(
    surface: &mut RemoteSurface,
    sess: &AttachClientSession,
    available: bool,
    allow_remote_content: bool,
    wake: Arc<dyn Fn() + Send + Sync>,
    projection: std::sync::Weak<()>,
) -> Option<Arc<HttpSession>> {
    let source = surface
        .html_mirror
        .as_ref()
        .and_then(|mirror| lock_mirror(mirror).http_source());
    let available = available
        && source.as_ref().is_some_and(|source| {
            source.is_current()
                && (!source.entry_url().starts_with("https:") || sess.transport.html.server_https)
        });
    if !available {
        retire_unavailable(surface);
        return None;
    }
    let mut predecessor = None;
    if surface.http_session.as_ref().is_some_and(|session| {
        replaces_session(
            session.failed(),
            session.native_owner_current(),
            source.as_ref().is_none_or(|source| {
                source.metadata_id() != session.source.metadata_id()
                    || source.source_generation() != session.source.source_generation()
                    || source.entry_url() != session.source.entry_url()
            }),
        )
    }) && let Some(session) = surface.http_session.take()
    {
        predecessor = match session.take_successor() {
            Ok(receipt) => receipt,
            Err(error) => {
                tracing::warn!(%error,"native HTTP successor refused");
                return None;
            }
        };
        session.close();
    }
    if surface.http_session.is_none() {
        let source = source?;
        let authority = reload_authority(
            surface,
            &source,
            sess.transport.frame_tx.epoch(),
            projection,
        );
        match HttpSession::new(
            source,
            sess.transport.frame_tx.clone(),
            allow_remote_content,
            sess.transport.html.server_https,
            surface.http_native_store.clone(),
            wake,
            crate::app::http_session::ReloadSetup {
                authority,
                negotiated: sess.transport.html.server_http_reload,
                predecessor,
            },
        ) {
            Ok(session) => surface.http_session = Some(session),
            Err(error) => {
                tracing::warn!(%error,"remote HTTP App session refused");
                return None;
            }
        }
    }
    // The failed view is gone before its state is mirrored, so the kept failure reports itself.
    if surface
        .http_session
        .as_ref()
        .is_some_and(|session| session.failed())
        && surface.nav_state() != crate::model::NavState::Failed
    {
        surface.set_nav_state(crate::model::NavState::Failed);
    }
    surface.http_session.clone()
}

pub(in crate::app::attach_client) fn receive(
    sess: &mut AttachClientSession,
    engine: &mut EngineMut<'_>,
    reply: tasty_ipc::html_http::HttpReply,
) {
    let remote = reply.id().surface_id;
    let observation = crate::app::http_session::Observation::capture(&reply);
    if !sess.transport.html.http_documents.receive(reply) {
        return;
    }
    if let Some(observation) = observation
        && let Some(local) = sess.state.remote_to_local.get(&remote)
        && let Some(surface) = engine.runtime.surfaces.get(local).and_then(|s| {
            s.as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
        })
        && let Some(session) = &surface.http_session
    {
        session.observe(observation);
    }
}

fn reload_authority(
    surface: &RemoteSurface,
    source: &tasty_remote::html_mirror::HttpSource,
    epoch: tasty_remote::connection::ConnectionEpoch,
    projection: std::sync::Weak<()>,
) -> Arc<dyn Fn() -> bool + Send + Sync> {
    let mirror = surface.html_mirror.as_ref().map(Arc::downgrade);
    let id = source.metadata_id();
    let generation = source.source_generation();
    let address = source.address().to_owned();
    // reason: poisoned mirror authority is unprovable; refuse its receipt without recovery.
    Arc::new(move || {
        if projection.strong_count() == 0 || !epoch.is_active() {
            return false;
        }
        mirror.as_ref().and_then(|owner| owner.upgrade()).is_some_and(|owner| {
            owner.lock().ok().is_some_and(|mirror| {
                mirror.connected && mirror.connection.same(&epoch)
                    && mirror.binding == id.binding && mirror.remote_surface == id.surface_id
                    && mirror.metadata.as_ref().is_some_and(|metadata| {
                        metadata.source_generation == generation
                            && matches!(&metadata.source, tasty_ipc::html_transfer::HtmlSource::Http { url } if url == &address)
                    })
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::replaces_session;

    #[test]
    fn a_failed_session_stays_while_its_source_is_unchanged() {
        assert!(!replaces_session(true, false, false));
        assert!(!replaces_session(true, true, false));
    }

    #[test]
    fn a_changed_source_replaces_any_session() {
        assert!(replaces_session(true, true, true));
        assert!(replaces_session(false, true, true));
    }

    #[test]
    fn a_live_session_without_its_native_owner_is_replaced() {
        assert!(replaces_session(false, false, false));
        assert!(!replaces_session(false, true, false));
    }
}
