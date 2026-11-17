//! App-owned navigation proofs bind native WebView activity to the original View and document source.
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::engine_access::EngineRef;
use crate::runtime::surface_binding::SurfaceBinding;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// Identity only: no mutable resource or ScriptGate is exposed back to the View.
pub(crate) struct NativeWebviewBinding {
    target: SurfaceBinding,
    surface: u32,
    source: Weak<Mutex<Option<String>>>,
    mirror: Option<Weak<Mutex<tasty_remote::html_mirror::HtmlMirror>>>,
    document: Option<tasty_remote::html_body::NativeViewBinding>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    http: Option<Weak<crate::app::http_session::HttpSession>>,
}
impl NativeWebviewBinding {
    pub(crate) fn capture(engine: &EngineRef<'_>, surface: u32) -> Option<Self> {
        let remote = engine
            .find_surface_by_id(surface)?
            .as_any()
            .downcast_ref::<RemoteSurface>()?;
        Some(Self {
            target: SurfaceBinding::capture(&engine.read(), surface)?,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            http: remote.http_session.as_ref().map(Arc::downgrade),
            surface,
            source: Arc::downgrade(&remote.webview_url),
            mirror: remote.html_mirror.as_ref().map(Arc::downgrade),
            document: remote
                .lock_html_mirror()
                .and_then(|m| m.body_store().native_binding()),
        })
    }
    pub(crate) fn current(&self, engine: &crate::runtime::engine_access::EngineRef<'_>) -> bool {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if let Some(owner) = &self.http {
            let Some(session) = owner.upgrade() else {
                return false;
            };
            if !session.is_current() {
                let frozen = engine
                    .runtime
                    .surfaces
                    .get(&self.surface)
                    .and_then(|surface| surface.as_any().downcast_ref::<RemoteSurface>())
                    .is_some_and(|remote| {
                        remote
                            .http_session
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, &session))
                            && (session.failed()
                                || (session.entry_body().is_some()
                                    && remote
                                        .lock_html_mirror()
                                        .is_some_and(|mirror| !mirror.connected)))
                    });
                if !frozen {
                    return false;
                }
            }
        }
        self.current_surface(engine)
    }
    pub(crate) fn current_surface(
        &self,
        engine: &crate::runtime::engine_access::EngineRef<'_>,
    ) -> bool {
        self.target.current(engine)
            && engine
                .runtime
                .surfaces
                .get(&self.surface)
                .and_then(|surface| surface.as_any().downcast_ref::<RemoteSurface>())
                .is_some_and(|remote| {
                    Arc::downgrade(&remote.webview_url).ptr_eq(&self.source)
                        && self.document
                            == remote
                                .lock_html_mirror()
                                .and_then(|m| m.body_store().native_binding())
                        && match (&self.mirror, &remote.html_mirror) {
                            (None, None) => true,
                            (Some(old), Some(current)) => old.ptr_eq(&Arc::downgrade(current)),
                            _ => false,
                        }
                })
    }
}

struct ViewProofs {
    view: Weak<()>,
    records: crate::plugin_bridge::user_navigation::UserNavigations,
    sources: HashMap<u32, Weak<Mutex<Option<String>>>>,
}
#[derive(Default)]
pub(crate) struct NavigationProofs {
    views: Mutex<Vec<ViewProofs>>,
}
static PROOFS_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
impl NavigationProofs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ViewProofs>> {
        crate::poison::recover_mutex(
            self.views.lock(),
            "native navigation proofs",
            &PROOFS_POISONED,
        )
    }
    pub(crate) fn record(
        &self,
        view: &Weak<()>,
        surface: u32,
        owner: Option<&crate::plugin_bridge::user_navigation::NavigationOwner>,
        source: Option<&Arc<Mutex<Option<String>>>>,
        navigation: &crate::webview::PendingNavigation,
    ) {
        let mut views = self.lock();
        views.retain(|entry| entry.view.strong_count() != 0);
        if view.strong_count() == 0 {
            return;
        }
        let index = views
            .iter()
            .position(|entry| entry.view.ptr_eq(view))
            .unwrap_or_else(|| {
                views.push(ViewProofs {
                    view: view.clone(),
                    records: HashMap::new(),
                    sources: HashMap::new(),
                });
                views.len() - 1
            });
        let entry = &mut views[index];
        crate::plugin_bridge::user_navigation::record(
            &mut entry.records,
            surface,
            owner,
            navigation,
        );
        if entry.records.contains_key(&surface)
            && let Some(source) = source
        {
            entry.sources.insert(surface, Arc::downgrade(source));
        } else {
            entry.sources.remove(&surface);
        }
    }
    pub(crate) fn settle_frame(&self, view: &Weak<()>, live: &[u32], takeovers: &[(u32, bool)]) {
        let mut views = self.lock();
        views.retain(|entry| entry.view.strong_count() != 0);
        let Some(entry) = views.iter_mut().find(|entry| entry.view.ptr_eq(view)) else {
            return;
        };
        entry.records.retain(|surface, _| live.contains(surface));
        for &(surface, takeover) in takeovers {
            crate::plugin_bridge::user_navigation::settle_frame(
                &mut entry.records,
                surface,
                takeover,
            );
        }
        entry
            .sources
            .retain(|surface, _| entry.records.contains_key(surface));
    }
    pub(crate) fn invalidate(&self, view: &Weak<()>, surface: u32) {
        let mut views = self.lock();
        if let Some(entry) = views.iter_mut().find(|entry| entry.view.ptr_eq(view)) {
            entry.records.remove(&surface);
            entry.sources.remove(&surface);
        }
    }
    pub(crate) fn take(
        &self,
        view: &Weak<()>,
        engine: &EngineRef<'_>,
        plugin: &str,
        surface: u32,
        url: &str,
    ) -> bool {
        let source = engine
            .find_surface_by_id(surface)
            .and_then(|surface| surface.as_any().downcast_ref::<RemoteSurface>())
            .map(|remote| &remote.webview_url);
        self.take_current(view, source, plugin, surface, url)
    }
    fn take_current(
        &self,
        view: &Weak<()>,
        current: Option<&Arc<Mutex<Option<String>>>>,
        plugin: &str,
        surface: u32,
        url: &str,
    ) -> bool {
        let mut views = self.lock();
        views.retain(|entry| entry.view.strong_count() != 0);
        let Some(entry) = views.iter_mut().find(|entry| entry.view.ptr_eq(view)) else {
            return false;
        };
        if !entry.sources.get(&surface).is_some_and(|source| {
            current.is_some_and(|current| source.ptr_eq(&Arc::downgrade(current)))
        }) {
            entry.records.remove(&surface);
            entry.sources.remove(&surface);
            return false;
        }
        let taken =
            crate::plugin_bridge::user_navigation::take(&mut entry.records, plugin, surface, url);
        if taken {
            entry.sources.remove(&surface);
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_claims_keep_the_original_view_plugin_url_and_one_shot_scope() {
        let proofs = NavigationProofs::default();
        let view = Arc::new(());
        let other_view = Arc::new(());
        let source = Arc::new(Mutex::new(Some("file:///document.html".into())));
        let owner = crate::plugin_bridge::user_navigation::NavigationOwner {
            plugin_id: "p".into(),
            wrote_page: true,
        };
        let nav = crate::webview::PendingNavigation {
            url: "file:///link.html".into(),
            user_gesture: true,
        };
        proofs.record(&Arc::downgrade(&view), 1, Some(&owner), Some(&source), &nav);
        assert!(!proofs.take_current(
            &Arc::downgrade(&other_view),
            Some(&source),
            "p",
            1,
            &nav.url
        ));
        assert!(!proofs.take_current(&Arc::downgrade(&view), Some(&source), "other", 1, &nav.url));
        assert!(proofs.take_current(&Arc::downgrade(&view), Some(&source), "p", 1, &nav.url));
        assert!(!proofs.take_current(&Arc::downgrade(&view), Some(&source), "p", 1, &nav.url));
    }

    #[test]
    fn replaced_resources_and_owner_takeover_invalidate_navigation_proof() {
        let proofs = NavigationProofs::default();
        let view = Arc::new(());
        let identity = Arc::downgrade(&view);
        let owner = crate::plugin_bridge::user_navigation::NavigationOwner {
            plugin_id: "p".into(),
            wrote_page: true,
        };
        let nav = crate::webview::PendingNavigation {
            url: "file:///link.html".into(),
            user_gesture: true,
        };
        let source = Arc::new(Mutex::new(None));
        proofs.record(&identity, 1, Some(&owner), Some(&source), &nav);
        let retired_source = source;
        let source = Arc::new(Mutex::new(None));
        assert!(!proofs.take_current(&identity, Some(&source), "p", 1, &nav.url));
        drop(retired_source);
        proofs.record(&identity, 1, Some(&owner), Some(&source), &nav);
        proofs.settle_frame(&identity, &[1], &[(1, true)]);
        assert!(!proofs.take_current(&identity, Some(&source), "p", 1, &nav.url));
        proofs.record(&identity, 1, Some(&owner), Some(&source), &nav);
        drop(view);
        assert!(!proofs.take_current(&identity, Some(&source), "p", 1, &nav.url));
    }
    #[test]
    fn native_entry_binding_retires_when_only_the_mirror_body_owner_changes() {
        let (_state, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
        let (sender, _frames) = tasty_remote::connection::channel();
        let mirror = || {
            Arc::new(Mutex::new(tasty_remote::html_mirror::HtmlMirror::new(
                7,
                9,
                sender.epoch(),
            )))
        };
        let mut remote = RemoteSurface::new(sid, "html", "com.tasty.html".into(), "html".into())
            .with_capabilities(
                crate::runtime::surface_registry::KindCapabilities::undeclared(
                    "html",
                    "com.tasty.html",
                ),
            );
        remote.html_mirror = Some(mirror());
        engine.runtime.surfaces.insert(sid, Box::new(remote));
        let binding = NativeWebviewBinding::capture(&engine.as_ref(), sid).unwrap();
        assert!(binding.current(&engine.as_ref()));
        let remote = engine
            .runtime
            .surfaces
            .get_mut(&sid)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<RemoteSurface>()
            .unwrap();
        let previous = remote.html_mirror.replace(mirror()).unwrap();
        assert!(!binding.current(&engine.as_ref()));
        assert!(Arc::strong_count(&previous) > 0);
        let replacement = NativeWebviewBinding::capture(&engine.as_ref(), sid).unwrap();
        assert!(replacement.current(&engine.as_ref()));
    }
}
