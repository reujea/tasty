//! Receipt ownership outlives callers; no registry guard enters the filesystem.
mod rebase;
#[cfg(test)]
mod tests;
use super::{
    ChildTerminalRegistry, RegistryData,
    persistence::{self, Operation, delta::Delta},
};
use crate::app::file_writer::{
    FileExecutor,
    command::{self, CommandDisposition, CommandOutcome},
};
use rebase::StateIntents;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};
use tasty_utils::batch_writer::{Handle, Limits};

type ResultValue = Result<Option<u32>, String>;
#[derive(Clone)]
pub(crate) struct Ticket {
    hub: Arc<Hub>,
    result: Arc<Mutex<Option<ResultValue>>>,
    application: Option<Arc<Mutex<Application>>>,
}
pub(super) enum Application {
    Pending,
    Applied(Result<(), String>),
    Retired,
}
impl Ticket {
    pub(super) fn with_application(&self) -> (Self, Arc<Mutex<Application>>) {
        let application = Arc::new(Mutex::new(Application::Pending));
        let mut ticket = self.clone();
        ticket.application = Some(application.clone());
        (ticket, application)
    }
    pub(crate) fn observation(&self) -> Option<ResultValue> {
        self.hub.poll();
        let result = self.result.lock().unwrap().clone()?;
        match &self.application {
            None => Some(result),
            Some(application) => match &*application.lock().unwrap() {
                Application::Pending => None,
                Application::Applied(Ok(())) => Some(result),
                Application::Applied(Err(error)) => Some(Err(error.clone())),
                Application::Retired => Some(match result {
                    Err(error) => Err(error),
                    Ok(_) => {
                        Err("child relationship saved; engine retired before live effect".into())
                    }
                }),
            },
        }
    }

    pub(crate) fn poll(&self) -> Option<ResultValue> {
        let result = self.observation()?;
        self.result.lock().unwrap().take().map(|_| result)
    }
}

pub(super) struct Client {
    pub(super) visible: RegistryData,
    confirmed: RegistryData,
    accepted: RegistryData,
    intents: VecDeque<Intent>,
}
#[derive(Clone)]
struct Intent {
    delta: Delta,
    states: StateIntents,
    operation: Operation,
    bytes: usize,
}
struct Pending {
    client: Arc<Mutex<Client>>,
    receipt: command::Ticket<Result<persistence::Outcome, String>>,
    result: Arc<Mutex<Option<ResultValue>>>,
}
struct Cleanup {
    #[cfg(feature = "gui")]
    parent: Option<u32>,
    #[cfg(feature = "gui")]
    retire: bool,
    client: Arc<Mutex<Client>>,
    child: u32,
    needed: bool,
    bytes: usize,
}
#[cfg(feature = "gui")]
struct Retirement {
    client: Arc<Mutex<Client>>,
    bytes: usize,
}
struct State {
    queue: Option<Handle<FileExecutor>>,
    initial: Option<RegistryData>,
    loading: Option<command::Ticket<RegistryData>>,
    clients: Vec<Weak<Mutex<Client>>>,
    pending: VecDeque<Pending>,
    bytes: usize,
    cleanups: VecDeque<Cleanup>,
    #[cfg(feature = "gui")]
    retirements: VecDeque<Retirement>,
    known_failures: u64,
    unknown: u64,
    shutdown_reported: bool,
}
pub(crate) struct Hub {
    path: Option<PathBuf>,
    limits: Limits,
    state: Mutex<State>,
    #[cfg(test)]
    pub(super) fixture_writer: Mutex<Option<crate::app::file_writer::FileWriterParts>>,
    #[cfg(test)]
    panic_next: std::sync::atomic::AtomicBool,
}
impl Hub {
    pub(crate) fn new(path: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            path,
            limits: Limits::default(),
            #[cfg(test)]
            fixture_writer: Mutex::new(None),
            #[cfg(test)]
            panic_next: std::sync::atomic::AtomicBool::new(false),
            state: Mutex::new(State {
                queue: None,
                initial: None,
                loading: None,
                clients: Vec::new(),
                pending: VecDeque::new(),
                bytes: 0,
                cleanups: VecDeque::new(),
                #[cfg(feature = "gui")]
                retirements: VecDeque::new(),
                known_failures: 0,
                unknown: 0,
                shutdown_reported: false,
            }),
        })
    }
    #[cfg(all(test, unix, feature = "gui"))]
    pub(super) fn test_initial(&self, data: RegistryData) {
        self.state.lock().unwrap().initial = Some(data);
    }
    pub(crate) fn attach(&self, queue: Handle<FileExecutor>) {
        self.state.lock().unwrap().queue = Some(queue);
        self.prepare(false);
    }
    /// Submit startup work; waiting, when required, belongs to the existing boot worker.
    pub(crate) fn prepare(&self, _clear: bool) {
        let mut state = self.state.lock().unwrap();
        let Some(queue) = state.queue.as_ref() else {
            state.initial = Some(RegistryData {
                path: self.path.clone(),
                load_error: Some("child registry file writer unavailable".into()),
                ..RegistryData::default()
            });
            return;
        };
        let path = self.path.clone();
        match command::submit(
            Some(queue),
            std::mem::size_of::<RegistryData>()
                .saturating_add(path.as_ref().map_or(0, |path| path.as_os_str().len())),
            move || {
                let mut data = path
                    .as_deref()
                    .map(super::data::read_file)
                    .unwrap_or_default();
                data.path = path;
                #[cfg(feature = "gui")]
                if _clear
                    && data.load_error.is_none()
                    && let Some(path) = data.path.clone()
                {
                    match persistence::Command::new(path, data.clone(), Operation::Clear).execute()
                    {
                        Ok(outcome) => data = outcome.registry,
                        Err(error) => data.load_error = Some(error),
                    }
                }
                data.baseline = None;
                data
            },
            registry_bytes,
        ) {
            Ok(ticket) => {
                state.initial = None;
                state.loading = Some(ticket);
            }
            Err(error) => {
                tracing::warn!(message = %error.message, "child registry initial read not admitted");
                state.initial = Some(RegistryData {
                    path: self.path.clone(),
                    load_error: Some(error.message),
                    ..RegistryData::default()
                });
            }
        }
    }
    /// Called only in the existing boot worker or before the headless event loop.
    pub(crate) fn wait_bootstrap(&self) {
        while !self.ready() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    pub(crate) fn ready(&self) -> bool {
        self.poll();
        let state = self.state.lock().unwrap();
        state.initial.is_some() || state.queue.is_none()
    }
    pub(super) fn client(self: &Arc<Self>) -> ChildTerminalRegistry {
        self.poll();
        let mut state = self.state.lock().unwrap();
        let value = state.initial.clone().unwrap_or_default();
        let client = Arc::new(Mutex::new(Client {
            visible: value.clone(),
            confirmed: value.clone(),
            accepted: value,
            intents: VecDeque::new(),
        }));
        state.clients.retain(|client| client.strong_count() > 0);
        state.clients.push(Arc::downgrade(&client));
        ChildTerminalRegistry {
            client,
            hub: self.clone(),
            effects: VecDeque::new(),
        }
    }
    pub(super) fn unregister(&self, owner: &Arc<Mutex<Client>>, child: u32) -> bool {
        self.poll();
        let mut state = self.state.lock().unwrap();
        let mut client = owner.lock().unwrap();
        let removed = client.visible.unregister_child_by_surface(child);
        let mut pending = false;
        for cleanup in &mut state.cleanups {
            if Arc::ptr_eq(&cleanup.client, owner) && cleanup.child == child {
                cleanup.needed = true;
                pending = true;
            }
        }
        removed || pending
    }
    #[cfg(feature = "gui")]
    pub(super) fn retire_owned(&self, owner: &Arc<Mutex<Client>>) {
        self.poll();
        let mut state = self.state.lock().unwrap();
        if state
            .retirements
            .iter()
            .any(|item| Arc::ptr_eq(&item.client, owner))
        {
            return;
        }
        for cleanup in &mut state.cleanups {
            if Arc::ptr_eq(&cleanup.client, owner) {
                cleanup.needed = true;
                cleanup.retire = true;
            }
        }
        let mut client = owner.lock().unwrap();
        let owned = client.visible.owned.clone();
        client.visible.drop_surfaces(&owned);
        let bytes = registry_bytes(&client.confirmed)
            .saturating_add(registry_bytes(&client.accepted))
            .saturating_add(registry_bytes(&client.visible))
            .saturating_add(std::mem::size_of::<Retirement>());
        drop(client);
        if let Err(error) = self.admit(&mut state, owner, Operation::ReleaseOwned(owned)) {
            tracing::warn!(%error,"child registry retirement awaits admission");
            if state
                .cleanups
                .iter()
                .any(|item| item.needed && Arc::ptr_eq(&item.client, owner))
            {
                return;
            }
            if state
                .pending
                .len()
                .saturating_add(state.cleanups.len())
                .saturating_add(state.retirements.len())
                >= self.limits.queue_items
                || bytes > self.limits.queue_bytes.saturating_sub(state.bytes)
            {
                state.known_failures = state.known_failures.saturating_add(1);
                tracing::warn!("child registry retirement exceeds retention budget; not accepted");
                return;
            }
            state.bytes = state.bytes.saturating_add(bytes);
            state.retirements.push_back(Retirement {
                client: owner.clone(),
                bytes,
            });
        }
    }
    pub(crate) fn finish_shutdown(&self) {
        self.poll();
        let mut state = self.state.lock().unwrap();
        if std::mem::replace(&mut state.shutdown_reported, true) {
            return;
        }
        let pending = state.pending.len();
        let cleanup = state
            .cleanups
            .iter()
            .filter(|cleanup| cleanup.needed)
            .count();
        #[cfg(feature = "gui")]
        let cleanup = cleanup.saturating_add(state.retirements.len());
        let dirty = state
            .clients
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|client| {
                let client = client.lock().unwrap();
                Delta::between(&client.confirmed, &client.visible).has_edits()
            })
            .count();
        let protected = state
            .initial
            .as_ref()
            .is_some_and(|value| value.load_error.is_some());
        let known_failures = state.known_failures;
        let unknown = state.unknown;
        if pending != 0
            || cleanup != 0
            || dirty != 0
            || protected
            || known_failures != 0
            || unknown != 0
        {
            tracing::warn!(
                pending,
                cleanup,
                dirty,
                protected,
                known_failures,
                unknown,
                "child registry final drain has incomplete or failed operations"
            );
        } else {
            tracing::info!(
                pending,
                cleanup,
                dirty,
                protected,
                known_failures,
                unknown,
                "child registry final drain observed"
            );
        }
    }
    pub(crate) fn admission_drained(&self) -> bool {
        self.poll();
        let state = self.state.lock().unwrap();
        let drained = !state.cleanups.iter().any(|cleanup| cleanup.needed);
        #[cfg(feature = "gui")]
        let drained = drained && state.retirements.is_empty();
        drained
    }
    pub(super) fn submit(
        self: &Arc<Self>,
        client: &Arc<Mutex<Client>>,
        operation: Operation,
    ) -> Result<Ticket, String> {
        self.poll();
        let mut state = self.state.lock().unwrap();
        let result = self.admit(&mut state, client, operation)?;
        Ok(Ticket {
            hub: self.clone(),
            result,
            application: None,
        })
    }
    fn admit(
        &self,
        state: &mut State,
        client: &Arc<Mutex<Client>>,
        operation: Operation,
    ) -> Result<Arc<Mutex<Option<ResultValue>>>, String> {
        if state.initial.is_none() {
            return Err("child registry initial read is pending".into());
        }
        let queue = state
            .queue
            .as_ref()
            .ok_or("child registry file writer unavailable")?;
        if let Operation::Adopt(_, child) = &operation
            && state.cleanups.iter().any(|cleanup| {
                Arc::ptr_eq(&cleanup.client, client) && cleanup.child == child.child_surface_id
            })
        {
            return Err("child relationship admission already pending for this surface".into());
        }
        let mut client_state = client.lock().unwrap();
        let mut desired = client_state.visible.clone();
        desired.baseline = Some(Arc::new(client_state.accepted.clone()));
        let delta = Delta::between(&client_state.accepted, &desired);
        let states = StateIntents::between(&client_state.accepted, &desired);
        let bytes = command::value_bytes(&delta)
            .saturating_mul(2)
            .saturating_add(command::value_bytes(&operation).saturating_mul(2))
            .saturating_add(command::value_bytes(&states))
            .saturating_add(registry_bytes(&desired))
            .saturating_add(std::mem::size_of::<Pending>());
        let cleanup = if let Operation::Adopt(_parent, child) = &operation {
            Some(Cleanup {
                #[cfg(feature = "gui")]
                parent: if let super::ParentKey::Surface(parent) = _parent {
                    Some(*parent)
                } else {
                    None
                },
                #[cfg(feature = "gui")]
                retire: false,
                client: client.clone(),
                child: child.child_surface_id,
                needed: false,
                bytes: std::mem::size_of::<Cleanup>().saturating_add(registry_bytes(&desired)),
            })
        } else {
            None
        };
        let retained = bytes.saturating_add(cleanup.as_ref().map_or(0, |cleanup| cleanup.bytes));
        #[cfg(feature = "gui")]
        let retirement_count = state.retirements.len();
        #[cfg(not(feature = "gui"))]
        let retirement_count = 0;
        if state
            .pending
            .len()
            .saturating_add(retirement_count)
            .saturating_add(state.cleanups.len())
            .saturating_add(1 + usize::from(cleanup.is_some()))
            > self.limits.queue_items
            || retained > self.limits.queue_bytes.saturating_sub(state.bytes)
        {
            return Err("child registry pending command budget exceeded".into());
        }
        let path = self.path.clone().ok_or("child registry path unavailable")?;
        let work = persistence::Command::new(path, desired.clone(), operation.clone());
        #[cfg(test)]
        let panic_next = self
            .panic_next
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        let receipt = command::submit_classified(
            Some(queue),
            retained,
            move || {
                #[cfg(test)]
                assert!(!panic_next, "owned child registry worker panic injection");
                work.execute().map(|mut outcome| {
                    outcome.registry.baseline = None;
                    outcome
                })
            },
            |result| match result {
                Ok(outcome) => registry_bytes(&outcome.registry),
                Err(error) => error.len(),
            },
            |result| CommandDisposition {
                outcome: if result.is_ok() {
                    CommandOutcome::KnownSuccess
                } else {
                    CommandOutcome::KnownFailure
                },
                partial: false,
            },
        )
        .map_err(|error| error.message)?;
        let result = Arc::new(Mutex::new(None));
        client_state.accepted = desired;
        client_state.accepted.baseline = None;
        rebase::project(&mut client_state.accepted, &operation);
        rebase::project(&mut client_state.visible, &operation);
        client_state.intents.push_back(Intent {
            delta,
            states,
            operation,
            bytes,
        });
        state.bytes += retained;
        if let Some(cleanup) = cleanup {
            state.cleanups.push_back(cleanup);
        }
        state.pending.push_back(Pending {
            client: client.clone(),
            receipt,
            result: result.clone(),
        });
        Ok(result)
    }
    pub(crate) fn poll(&self) {
        let mut state = self.state.lock().unwrap();
        self.collect_initial(&mut state);
        while let Some(pending) = state.pending.front() {
            let Some(result) = pending.receipt.poll() else {
                break;
            };
            let pending = state.pending.pop_front().unwrap();
            self.collect_result(&mut state, pending, result);
        }
        self.pump_cleanup(&mut state);
        #[cfg(feature = "gui")]
        self.pump_retirements(&mut state);
    }
    fn collect_initial(&self, state: &mut State) {
        if let Some(result) = state.loading.as_ref().and_then(command::Ticket::poll) {
            state.loading = None;
            let initial = match result {
                Ok(value) => value,
                Err(error) => RegistryData {
                    path: self.path.clone(),
                    load_error: Some(error.message),
                    ..RegistryData::default()
                },
            };
            for weak in &state.clients {
                if let Some(client) = weak.upgrade() {
                    let mut client = client.lock().unwrap();
                    let unsent = Delta::between(&client.accepted, &client.visible);
                    let unsent_states = StateIntents::between(&client.accepted, &client.visible);
                    let serial = client.visible.intent_serial;
                    let owned = client.visible.owned.clone();
                    let slot = client.visible.slot;
                    client.confirmed = initial.clone();
                    client.accepted = initial.clone();
                    client.visible = initial.clone();
                    client.visible.bind_slot(slot);
                    client.visible.owned.extend(owned);
                    client.visible.slot = slot;
                    unsent.apply(&mut client.visible);
                    unsent_states.apply(&mut client.visible);
                    client.visible.intent_serial = serial;
                }
            }
            state.initial = Some(initial);
        }
    }
    fn collect_result(
        &self,
        state: &mut State,
        pending: Pending,
        result: Result<Result<persistence::Outcome, String>, command::Failure>,
    ) {
        let mut client = pending.client.lock().unwrap();
        let intent = client.intents.pop_front().unwrap();
        state.bytes = state.bytes.saturating_sub(intent.bytes);
        let unknown = matches!(&result, Err(error) if error.unknown);
        let outcome = match result {
            Ok(result) => result,
            Err(error) => Err(if error.unknown {
                format!("child registry outcome unknown: {}", error.message)
            } else {
                error.message
            }),
        };
        if outcome.is_err() {
            if unknown {
                state.unknown = state.unknown.saturating_add(1);
            } else {
                state.known_failures = state.known_failures.saturating_add(1);
            }
        }
        if let Ok(outcome) = &outcome {
            let mut latest = outcome.registry.clone();
            latest.baseline = None;
            latest.owned.clear();
            latest.slot = None;
            latest.idle_intents.clear();
            latest.needs_input_intents.clear();
            state.initial = Some(latest);
        }
        if let Operation::Adopt(_, child) = &intent.operation
            && let Some(index) = state.cleanups.iter().position(|cleanup| {
                Arc::ptr_eq(&cleanup.client, &pending.client)
                    && cleanup.child == child.child_surface_id
                    && !cleanup.needed
            })
        {
            let cleanup = state.cleanups.remove(index).unwrap();
            state.bytes = state.bytes.saturating_sub(cleanup.bytes);
        }
        let result = rebase::complete(&mut client, intent, outcome);
        for cleanup in &state.cleanups {
            if cleanup.needed && Arc::ptr_eq(&cleanup.client, &pending.client) {
                client.visible.unregister_child_by_surface(cleanup.child);
            }
        }
        if let Err(error) = &result {
            tracing::warn!(%error, "child registry command failed; not replayed");
        }
        *pending.result.lock().unwrap() = Some(result);
    }
    #[cfg(feature = "gui")]
    fn pump_retirements(&self, state: &mut State) {
        let count = state.retirements.len();
        for _ in 0..count {
            let item = state.retirements.pop_front().unwrap();
            state.bytes = state.bytes.saturating_sub(item.bytes);
            let owned = item.client.lock().unwrap().visible.owned.clone();
            if self
                .admit(state, &item.client, Operation::ReleaseOwned(owned))
                .is_err()
            {
                state.bytes = state.bytes.saturating_add(item.bytes);
                state.retirements.push_back(item);
            }
        }
    }
    fn pump_cleanup(&self, state: &mut State) {
        // Admission failures retain a bounded close intent; executed/unknown commands never replay.
        let candidates = state.cleanups.len();
        for _ in 0..candidates {
            let cleanup = state.cleanups.pop_front().unwrap();
            if !cleanup.needed {
                state.cleanups.push_back(cleanup);
                continue;
            }
            state.bytes = state.bytes.saturating_sub(cleanup.bytes);
            #[cfg(feature = "gui")]
            let operation = if cleanup.retire {
                Operation::RetirePending {
                    parent: cleanup.parent,
                    child: cleanup.child,
                }
            } else {
                Operation::Forget(cleanup.child)
            };
            #[cfg(not(feature = "gui"))]
            let operation = Operation::Forget(cleanup.child);
            match self.admit(state, &cleanup.client, operation) {
                Ok(_) => {}
                Err(_) => {
                    state.bytes = state.bytes.saturating_add(cleanup.bytes);
                    state.cleanups.push_back(cleanup);
                }
            }
        }
    }
}

fn registry_bytes(value: &RegistryData) -> usize {
    command::value_bytes(value)
        .saturating_add(value.load_error.as_ref().map_or(0, String::len))
        .saturating_add(value.owned.len().saturating_mul(std::mem::size_of::<u32>()))
        .saturating_add(value.path.as_ref().map_or(0, |path| path.as_os_str().len()))
        .saturating_add(command::value_bytes(&value.idle_intents))
        .saturating_add(command::value_bytes(&value.needs_input_intents))
}
