//! One process-local untrusted manager/profile; only the transient native probe is per socket.
use super::super::{
    http_context::{
        linux::{NativeProbeEvidence, NativeProbeStore, NativeTrustTarget},
        preparation::Preparation,
    },
    http_endpoint::listener::trust::{
        TrustObservation,
        pool::{ProbeId, ProbePool},
    },
};
use super::*;
use std::time::Duration;
type Evidence = Result<NativeProbeEvidence, super::super::http_context::linux::ContextError>;
struct Active {
    id: ProbeId,
    target: NativeTrustTarget,
    session: Arc<HttpSession>,
    result: Rc<RefCell<Option<Evidence>>>,
    settled: bool,
}
#[derive(Default)]
struct State {
    preparation: Option<Preparation>,
    preparing_source: Option<std::sync::Weak<HttpSession>>,
    pool: Option<ProbePool<Rc<NativeProbeStore>>>,
    active: Option<Active>,
    timer: Option<gtk::glib::SourceId>,
}
thread_local! {static PROCESS: Rc<RefCell<State>> = Rc::default();}
pub(super) fn poll(
    session: Arc<HttpSession>,
    view: &PreparedView,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    PROCESS.with(|shared| {
        advance(&mut shared.borrow_mut());
        if !session.is_current() {
            return Ok(());
        }
        let mut state = shared.borrow_mut();
        if state.pool.is_none() {
            if state.preparation.is_some()
                && state
                    .preparing_source
                    .as_ref()
                    .and_then(|owner| owner.upgrade())
                    .is_none_or(|owner| !owner.is_current())
            {
                state.preparation.take();
                state.preparing_source.take();
            }
            if state.preparation.is_none() {
                state.preparation = Some(session.prepare_probe()?);
                state.preparing_source = Some(Arc::downgrade(&session));
            }
            if let Some(result) = state
                .preparation
                .as_ref()
                .and_then(|preparation| preparation.poll())
            {
                state.preparation = None;
                state.preparing_source = None;
                let profile = result.map_err(|error| error.to_string())?;
                let store = NativeProbeStore::new(profile)
                    .map_err(|error| format!("native TLS probe profile: {error:?}"))?;
                state.pool = Some(ProbePool::new(store));
            }
        }
        if state.active.is_none() {
            let target = view.trust_target();
            let fence = target.clone();
            let source = session.clone();
            if let Some(pool) = &mut state.pool {
                let id = session
                    .with_native_trust(|inbox| {
                        pool.poll(inbox, move || fence.current() && source.is_current())
                    })
                    .flatten();
                if let Some(id) = id {
                    let request = pool.request(id).ok_or("native TLS request retired")?;
                    let metadata = session.source.metadata_id();
                    let identity = request.identity().ok_or("native TLS identity retired")?;
                    if (
                        identity.epoch,
                        identity.surface,
                        identity.binding,
                        identity.document,
                        identity.source_generation,
                    ) != (
                        metadata.epoch,
                        metadata.surface_id,
                        metadata.binding,
                        metadata.document,
                        session.source.source_generation(),
                    ) {
                        pool.reject(id);
                        return Err("native TLS target identity mismatch".into());
                    }
                    let proxy = request
                        .probe_proxy_uri()
                        .ok_or("private TLS probe retired")?;
                    let host = request.host();
                    let host = if host.contains(':') {
                        format!("[{host}]")
                    } else {
                        host.into()
                    };
                    let uri = format!("https://{host}:{}/", request.port());
                    let result: Rc<RefCell<Option<Evidence>>> = Rc::default();
                    let receipt = result.clone();
                    let signal = wake.clone();
                    let current = session.clone();
                    let started = pool.resource().start(
                        proxy,
                        uri,
                        Arc::new(move || current.is_current()),
                        Box::new(move |value| {
                            *receipt.borrow_mut() = Some(value);
                            signal();
                        }),
                    );
                    state.active = Some(Active {
                        id,
                        target,
                        session: session.clone(),
                        result,
                        settled: false,
                    });
                    if let Err(error) = started {
                        state
                            .active
                            .as_ref()
                            .expect("active probe")
                            .result
                            .replace(Some(Err(error)));
                    }
                }
            }
        }
        drop(state);
        arm(shared, wake);
        Ok(())
    })
}
/// private probe나 probe profile 준비가 진행 중이면 true다. 프로세스 공용 상태라 창을 가리지 않는다.
pub(super) fn in_progress() -> bool {
    PROCESS.with(|shared| {
        shared
            .try_borrow()
            .is_ok_and(|state| state.active.is_some() || state.preparation.is_some())
    })
}
fn advance(state: &mut State) {
    let Some(active) = &mut state.active else {
        return;
    };
    let Some(pool) = &mut state.pool else {
        return;
    };
    if !active.settled {
        if !active.target.current() || !active.session.is_current() {
            pool.retire();
            pool.resource().cancel();
            active.settled = true;
        } else {
            let result = active.result.borrow_mut().take();
            if let Some(result) = result {
                let accepted = (|| {
                    let observed =
                        result.map_err(|error| format!("native TLS probe: {error:?}"))?;
                    let request = pool
                        .request(active.id)
                        .ok_or("native TLS receipt retired")?;
                    let expected = (request.host().to_owned(), request.port(), request.leaf());
                    // The private probe has already destroyed its view. Only this actual current
                    // native target can install the exact endpoint pin after UNKNOWN_CA evidence.
                    let evidence = active
                        .target
                        .accept(
                            &observed.uri,
                            &observed.certificate,
                            observed.flags,
                            &expected.0,
                            expected.1,
                            expected.2,
                        )
                        .map_err(|error| format!("native TLS evidence: {error:?}"))?;
                    if expected != (evidence.host.clone(), evidence.port, evidence.leaf) {
                        return Err("native TLS authority mismatch".to_owned());
                    }
                    if !pool.ack(
                        active.id,
                        TrustObservation {
                            host: evidence.host,
                            port: evidence.port,
                            leaf: evidence.leaf,
                            unknown_ca_only: true,
                        },
                    ) {
                        return Err("native TLS acknowledgment refused".into());
                    }
                    tracing::debug!(
                        "native TLS private UNKNOWN_CA proof accepted for current target"
                    );
                    Ok(())
                })();
                if let Err(error) = accepted {
                    tracing::warn!(%error,"native TLS barrier refused");
                    pool.reject(active.id);
                    active.session.fail();
                }
                active.settled = true;
            }
        }
    }
    if active.settled && !pool.resource().busy() && pool.complete(active.id) {
        state.active = None;
    }
}
fn arm(shared: &Rc<RefCell<State>>, wake: Arc<dyn Fn() + Send + Sync>) {
    let mut state = shared.borrow_mut();
    if state.active.is_none() || state.timer.is_some() {
        return;
    }
    let shared = shared.clone();
    state.timer = Some(gtk::glib::timeout_add_local_once(
        Duration::from_millis(20),
        move || {
            shared.borrow_mut().timer.take();
            advance(&mut shared.borrow_mut());
            wake();
            arm(&shared, wake);
        },
    ));
}
