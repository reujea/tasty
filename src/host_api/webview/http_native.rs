//! GTK-only publication receipt. Profile filesystem work is already finished on its worker.
mod reload;
mod trust;
use super::http_context::linux::{ContextConfig, DataStore, PreparedView};
use crate::app::http_session::HttpSession;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(crate) struct PendingNative {
    session: Arc<HttpSession>,
    authority_url: String,
    store: Option<DataStore>,
    pin_pending: Option<
        std::sync::mpsc::Receiver<
            Result<
                super::http_endpoint::listener::preparation::ReadyAuthority,
                super::http_endpoint::listener::EndpointError,
            >,
        >,
    >,
    pin: Option<super::http_endpoint::listener::preparation::ReadyAuthority>,
    result: Rc<RefCell<Option<Result<PreparedView, super::http_context::linux::ContextError>>>>,
}
pub(crate) struct NativeDocument {
    native_owner: Arc<()>,
    session: Rc<RefCell<Arc<HttpSession>>>,
    pub view: PreparedView,
    store: DataStore,
    pin: RefCell<Option<super::http_endpoint::listener::preparation::ReadyAuthority>>,
    replacement: RefCell<Option<reload::Replacement>>,
}
impl PendingNative {
    pub(crate) fn new(session: Arc<HttpSession>) -> Self {
        Self {
            authority_url: session.source.entry_url().into(),
            session,
            store: None,
            pin_pending: None,
            pin: None,
            result: Rc::default(),
        }
    }
    pub(crate) fn current(&self) -> bool {
        self.session.is_current()
    }
    pub(crate) fn poll(
        &mut self,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Option<NativeDocument>, String> {
        let result = self.poll_inner(wake);
        if result.is_err() {
            self.session.fail();
        }
        result
    }
    fn poll_inner(
        &mut self,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Option<NativeDocument>, String> {
        if !self.current() {
            return Err("native HTTP preparation retired".into());
        }
        if self.store.is_none() {
            if !self.poll_pin()? {
                return Ok(None);
            }
            let Some(profile) = self.session.profile() else {
                return Ok(None);
            };
            let profile = profile.map_err(|error| error.to_string())?;
            tasty_platform::gtk_init::ensure()?;
            let store =
                DataStore::new(profile).map_err(|error| format!("native HTTP store: {error:?}"))?;
            let config = self.context_config()?;
            let result = self.result.clone();
            store.prepare(
                config,
                Box::new(move |value| {
                    *result.borrow_mut() = Some(value);
                    wake();
                }),
            );
            self.store = Some(store);
        }
        let Some(result) = self.result.borrow_mut().take() else {
            return Ok(None);
        };
        let view =
            result.map_err(|error| format!("native HTTP selected-clear barrier: {error:?}"))?;
        view.use_endpoint_trust()
            .map_err(|error| format!("native endpoint TLS owner: {error:?}"))?;
        self.session.native_store.publish();
        let native_owner = Arc::new(());
        self.session.bind_native_owner(&native_owner)?;
        Ok(Some(NativeDocument {
            native_owner,
            session: Rc::new(RefCell::new(self.session.clone())),
            view,
            store: self.store.take().ok_or("native HTTP store retired")?,
            pin: RefCell::new(self.pin.take()),
            replacement: RefCell::new(None),
        }))
    }
    fn poll_pin(&mut self) -> Result<bool, String> {
        let Some(endpoint) = self.session.endpoint() else {
            return Ok(false);
        };
        if self.authority_url.starts_with("https:") && self.pin.is_none() {
            if self.pin_pending.is_none() {
                self.pin_pending = Some(
                    self.session
                        .prepare_authority(&self.authority_url)
                        .map_err(|error| format!("initial HTTPS authority: {error:?}"))?,
                );
            }
            match self
                .pin_pending
                .as_ref()
                .expect("pending authority")
                .try_recv()
            {
                Ok(result) => {
                    let ready = result.map_err(|error| format!("initial HTTPS pin: {error:?}"))?;
                    if !ready.is_current()
                        || endpoint.expected_leaf(ready.host(), ready.port()) != Some(ready.leaf())
                    {
                        return Err("initial HTTPS pin retired".into());
                    }
                    self.pin = Some(ready);
                    self.pin_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(false),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("initial HTTPS pin worker stopped".into());
                }
            }
        }
        Ok(true)
    }
    fn context_config(&self) -> Result<ContextConfig, String> {
        let endpoint = self
            .session
            .endpoint()
            .ok_or("native HTTP endpoint not ready")?;
        let session = self.session.clone();
        let weak = Arc::downgrade(&endpoint);
        let config = ContextConfig {
            proxy_uri: endpoint.proxy_uri(),
            current: Arc::new(move || session.is_current()),
            expected_leaf: Arc::new(move |host, port| {
                weak.upgrade()
                    .and_then(|endpoint| endpoint.expected_leaf(host, port))
            }),
        };
        Ok(config)
    }
}
impl NativeDocument {
    pub(crate) fn poll_trust(&self, wake: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
        trust::poll(self.session(), &self.view, wake)
    }
}
impl Drop for NativeDocument {
    fn drop(&mut self) {
        // Revoke endpoint/body callbacks before destroying the native view. The session's runtime
        // retains endpoint teardown on its worker, without joining on GTK.
        let session = self.session.borrow();
        session.close();
        session.cancel_retained();
        self.view.close();
    }
}
