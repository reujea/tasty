//! Admission-ordered plugin continuations; the common file worker owns every write.
#[cfg(any(feature = "gui", test))]
pub(crate) mod post_commit;

mod retained;
mod shutdown;

use super::{App, file_writer::command};
use retained::{Budget, Held};
use std::rc::Rc;
#[cfg(feature = "gui")]
type ShortcutSlot = Rc<std::cell::RefCell<super::plugin_glue::shortcut::Draft>>;
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_PENDING: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct Failure {
    pub(crate) message: String,
    pub(crate) saved: Option<bool>,
    pub(crate) applied: bool,
    pub(crate) details: serde_json::Value,
}
impl Failure {
    pub(crate) fn response_data(&self) -> serde_json::Value {
        let mut data = if self.details.is_object() {
            self.details.clone()
        } else {
            serde_json::json!({})
        };
        if self.outcome_unknown() {
            data["outcome"] = serde_json::json!("unknown");
        }
        data["saved"] = serde_json::json!(self.saved);
        data["applied"] = serde_json::json!(self.applied);
        data
    }
    pub(crate) fn outcome_unknown(&self) -> bool {
        self.saved.is_none() || self.details["outcome"].as_str() == Some("unknown")
    }
    pub(crate) fn before(error: impl std::fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            saved: Some(false),
            applied: false,
            details: serde_json::Value::Null,
        }
    }
}
pub(crate) fn classify_result<T, E>(result: &Result<T, E>) -> command::CommandDisposition {
    command::CommandDisposition {
        outcome: if result.is_ok() {
            command::CommandOutcome::KnownSuccess
        } else {
            command::CommandOutcome::KnownFailure
        },
        partial: false,
    }
}
#[cfg(any(feature = "gui", test))]
pub(crate) fn classify_failure<T>(result: &Result<T, Failure>) -> command::CommandDisposition {
    match result {
        Ok(_) => command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        Err(error) => command::CommandDisposition {
            outcome: if error.outcome_unknown() {
                command::CommandOutcome::Unknown
            } else {
                command::CommandOutcome::KnownFailure
            },
            partial: error.applied
                || error.details["rollback_failed"].as_bool().unwrap_or(false)
                || error.details["cleanup_failed"].as_bool().unwrap_or(false),
        },
    }
}
#[derive(Clone, Default)]
pub(crate) struct Reply(Arc<Mutex<Option<Result<Value, Failure>>>>);
impl Reply {
    pub(crate) fn poll(&self) -> Option<Result<Value, Failure>> {
        self.0.lock().unwrap().take()
    }
    pub(crate) fn finish(&self, result: Result<Value, Failure>) {
        *self.0.lock().unwrap() = Some(result);
    }
}
type Start = Box<dyn FnOnce(&mut App)>;
struct Queued {
    start: Start,
    held: Held,
}
struct Pipeline {
    held: Held,
    base: usize,
}
type Finish = Box<dyn FnOnce(&mut App)>;
trait Pending {
    fn ready(&mut self, app: &mut App) -> Option<Finish>;
    fn expire(self: Box<Self>) -> Finish;
    fn poll_deadline(&self) -> Option<std::time::Instant> {
        None
    }
}
struct Accepted<T, F> {
    receipt: command::Ticket<T>,
    finish: Option<F>,
    weigh: Arc<dyn Fn(&T) -> usize + Send + Sync>,
    context_bytes: usize,
}
impl<T: Send + 'static, F: FnOnce(&mut App, Result<T, Failure>) + 'static> Pending
    for Accepted<T, F>
{
    fn expire(mut self: Box<Self>) -> Finish {
        let finish = self.finish.take().expect("file continuation consumed once");
        Box::new(move |app| finish(app, Err(Failure::shutdown_unknown())))
    }
    fn ready(&mut self, app: &mut App) -> Option<Finish> {
        let output = self.receipt.poll()?.map_err(|error| Failure {
            message: error.message,
            saved: if error.unknown { None } else { Some(false) },
            applied: false,
            details: serde_json::Value::Null,
        });
        let output_bytes = match &output {
            Ok(value) => (self.weigh)(value),
            Err(error) => error
                .message
                .len()
                .saturating_add(command::value_bytes(&error.details)),
        };
        app.plugin_files
            .retain_result(self.context_bytes.saturating_add(output_bytes));
        let finish = self.finish.take().expect("file continuation consumed once");
        Some(Box::new(move |app| finish(app, output)))
    }
}
struct Retired {
    id: String,
    finish: Option<RetirementFinish>,
}
type RetirementFinish = Box<dyn FnOnce(&mut App, Result<(), Failure>)>;
impl Pending for Retired {
    fn poll_deadline(&self) -> Option<std::time::Instant> {
        Some(std::time::Instant::now() + tasty_host_plugin::process::CHILD_EXIT_POLL_INTERVAL)
    }
    fn expire(mut self: Box<Self>) -> Finish {
        let finish = self
            .finish
            .take()
            .expect("retirement continuation consumed once");
        let mut error = Failure::shutdown_unknown();
        error.details["retirement_outcome"] = serde_json::json!("unknown");
        Box::new(move |app| finish(app, Err(error)))
    }
    fn ready(&mut self, app: &mut App) -> Option<Finish> {
        if app
            .plugin_manager
            .as_mut()
            .is_some_and(|mgr| !mgr.poll_file_retirement(&self.id))
        {
            return None;
        }
        let finish = self.finish.take()?;
        Some(Box::new(move |app| finish(app, Ok(()))))
    }
}
#[derive(Default)]
pub(crate) struct PluginFiles {
    queue: VecDeque<Queued>,
    budget: Rc<Budget>,
    current: Option<Pipeline>,
    active: Option<Box<dyn Pending>>,
    closed: bool,
    expired: bool,
    unresolved: usize,
    #[cfg(feature = "gui")]
    pub(crate) shortcuts:
        Option<std::rc::Rc<std::cell::RefCell<super::plugin_glue::shortcut::Draft>>>,
    #[cfg(feature = "gui")]
    pub(crate) failed_shortcuts: super::plugin_glue::shortcut::Draft,
    #[cfg(feature = "gui")]
    shortcut_retained: Option<Held>,
    #[cfg(feature = "gui")]
    failed_retained: Option<Held>,
    #[cfg(feature = "gui")]
    retry_shortcuts: bool,
    #[cfg(feature = "gui")]
    unsaved_reported: bool,
}
impl PluginFiles {
    pub(crate) fn enqueue_sized<F: FnOnce(&mut App) + 'static>(
        &mut self,
        bytes: usize,
        start: F,
    ) -> Result<Held, Failure> {
        let held = self
            .budget
            .reserve(bytes.saturating_add(std::mem::size_of::<F>()))?;
        self.enqueue_reserved(held.clone(), start, false)?;
        Ok(held)
    }
    fn enqueue_reserved<F: FnOnce(&mut App) + 'static>(
        &mut self,
        held: Held,
        start: F,
        retry: bool,
    ) -> Result<(), Failure> {
        if (self.closed && !retry) || self.pending() >= MAX_PENDING {
            return Err(Failure::before(
                "plugin file lifecycle admission is closed or full",
            ));
        }
        #[cfg(feature = "gui")]
        self.clear_shortcut_slot();
        self.queue.push_back(Queued {
            held,
            start: Box::new(start),
        });
        Ok(())
    }
    fn begin_work(&mut self, bytes: usize, extra: usize) -> Result<usize, Failure> {
        let base = self.current.as_ref().map_or(0, |current| current.base);
        let context = base.saturating_add(extra);
        let input = context.saturating_add(bytes);
        match &self.current {
            Some(current) => current.held.resize(input)?,
            None => {
                self.current = Some(Pipeline {
                    base: 0,
                    held: self.budget.reserve(input)?,
                })
            }
        }
        Ok(context)
    }
    fn retain_result(&mut self, bytes: usize) {
        self.current
            .as_ref()
            .expect("accepted continuation owns reservation")
            .held
            .result(bytes);
    }
    #[cfg(feature = "gui")]
    pub(crate) fn clear_shortcut_slot(&mut self) {
        self.shortcuts = None;
        self.shortcut_retained = None;
    }
    #[cfg(feature = "gui")]
    pub(crate) fn reserve_shortcuts(
        &mut self,
        draft: super::plugin_glue::shortcut::Draft,
    ) -> Result<(ShortcutSlot, Held), Failure> {
        let mut combined = self.failed_shortcuts.clone();
        combined.extend(draft);
        let bytes = super::plugin_glue::shortcut::draft_bytes(&combined);
        let held = match &self.failed_retained {
            Some(held) => {
                held.resize(bytes)?;
                held.clone()
            }
            None => self.budget.reserve(bytes)?,
        };
        self.failed_shortcuts.clear();
        self.failed_retained = None;
        Ok((Rc::new(std::cell::RefCell::new(combined)), held))
    }
    #[cfg(feature = "gui")]
    pub(crate) fn enqueue_shortcuts<F: FnOnce(&mut App) + 'static>(
        &mut self,
        held: Held,
        start: F,
        retry: bool,
    ) -> Result<(), Failure> {
        held.resize(held.bytes().saturating_add(std::mem::size_of::<F>()))?;
        self.enqueue_reserved(held, start, retry)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn mark_shortcut_slot(
        &mut self,
        pending: Rc<std::cell::RefCell<super::plugin_glue::shortcut::Draft>>,
        held: Held,
    ) {
        self.shortcuts = Some(pending);
        self.shortcut_retained = Some(held);
    }
    #[cfg(feature = "gui")]
    pub(crate) fn keep_unqueued_shortcuts(
        &mut self,
        draft: super::plugin_glue::shortcut::Draft,
        held: Held,
    ) {
        self.failed_shortcuts = draft;
        self.failed_retained = Some(held);
    }
    #[cfg(feature = "gui")]
    pub(crate) fn coalesce_shortcuts(
        &mut self,
        draft: super::plugin_glue::shortcut::Draft,
    ) -> Result<(), super::plugin_glue::shortcut::Draft> {
        match &self.shortcuts {
            Some(pending) => {
                let mut merged = pending.borrow().clone();
                merged.extend(draft.clone());
                let Some(held) = &self.shortcut_retained else {
                    return Err(draft);
                };
                let overhead = held
                    .bytes()
                    .saturating_sub(super::plugin_glue::shortcut::draft_bytes(&pending.borrow()));
                if held
                    .resize(
                        overhead.saturating_add(super::plugin_glue::shortcut::draft_bytes(&merged)),
                    )
                    .is_err()
                {
                    return Err(draft);
                }
                *pending.borrow_mut() = merged;
                Ok(())
            }
            None => Err(draft),
        }
    }
    #[cfg(feature = "gui")]
    pub(crate) fn settle_shortcuts(
        &mut self,
        draft: super::plugin_glue::shortcut::Draft,
        saved: bool,
    ) {
        if saved {
            for (key, value) in &draft {
                if self.failed_shortcuts.get(key) == Some(value) {
                    self.failed_shortcuts.remove(key);
                }
            }
        } else {
            self.failed_shortcuts.extend(draft);
        }
        if self.failed_shortcuts.is_empty() {
            self.failed_retained = None;
        } else {
            let bytes = super::plugin_glue::shortcut::draft_bytes(&self.failed_shortcuts);
            match &self.failed_retained {
                Some(held) => held.result(bytes),
                None => self.failed_retained = Some(self.budget.retain_result(bytes)),
            }
        }
    }
    pub(crate) fn stopping(&self) -> bool {
        self.closed
    }
    pub(crate) fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.active.is_some() || self.current.is_some())
    }
}
impl App {
    /// May submit the next stage of an already accepted operation. Drain these before closing files.
    pub(crate) fn finish_plugin_files(&mut self) {
        if let Some(mut active) = self.plugin_files.active.take() {
            match active.ready(self) {
                Some(finish) => {
                    finish(self);
                    if self.plugin_files.active.is_none() {
                        self.plugin_files.current = None;
                    }
                }
                None => self.plugin_files.active = Some(active),
            }
        }
        #[cfg(feature = "gui")]
        if self.plugin_files.closed && self.plugin_files.pending() == 0 {
            if self.plugin_files.retry_shortcuts {
                self.plugin_files.retry_shortcuts = false;
                let dirty = self.plugin_files.failed_shortcuts.clone();
                self.queue_plugin_shortcut_draft(dirty, true);
            } else if !self.plugin_files.failed_shortcuts.is_empty()
                && !self.plugin_files.unsaved_reported
            {
                self.plugin_files.unsaved_reported = true;
                tracing::warn!(
                    edits = self.plugin_files.failed_shortcuts.len(),
                    "plugin shortcuts remain unsaved after shutdown retry"
                );
            }
        }
        if self.plugin_files.active.is_none() {
            if let Some(queued) = self.plugin_files.queue.pop_front() {
                self.plugin_files.current = Some(Pipeline {
                    base: queued.held.bytes(),
                    held: queued.held,
                });
                (queued.start)(self);
                if self.plugin_files.active.is_none() {
                    self.plugin_files.current = None;
                }
            }
            if self.plugin_files.pending() != 0 {
                (self.services.reply_wake())();
            }
        }
    }
    pub(crate) fn stop_plugin_file_producers(&mut self) {
        #[cfg(feature = "gui")]
        if !self.plugin_files.closed {
            self.plugin_files.retry_shortcuts = true;
        }
        self.plugin_files.closed = true;
    }
    pub(crate) fn plugin_files_pending(&self) -> usize {
        self.plugin_files
            .pending()
            .saturating_add(self.plugin_files.unresolved)
    }

    pub(crate) fn after_plugin_retired(
        &mut self,
        id: String,
        finish: impl FnOnce(&mut App, Result<(), Failure>) + 'static,
    ) {
        if self.plugin_files.expired {
            let retired = self
                .plugin_manager
                .as_mut()
                .is_none_or(|manager| manager.poll_file_retirement(&id));
            if retired {
                finish(self, Ok(()));
            } else {
                self.plugin_files.unresolved += 1;
                let mut error = Failure::shutdown_unknown();
                error.details["retirement_outcome"] = serde_json::json!("unknown");
                finish(self, Err(error));
            }
            return;
        }
        let retained = self
            .plugin_files
            .current
            .as_ref()
            .expect("retirement belongs to an admitted pipeline")
            .held
            .bytes();
        self.plugin_files.retain_result(
            retained
                .saturating_add(id.len())
                .saturating_add(std::mem::size_of_val(&finish)),
        );
        self.plugin_files.active = Some(Box::new(Retired {
            id,
            finish: Some(Box::new(finish)),
        }));
    }

    pub(crate) fn run_plugin_file<T: Send + 'static>(
        &mut self,
        bytes: (usize, usize),
        work: impl FnOnce() -> T + Send + 'static,
        weigh: impl Fn(&T) -> usize + Send + Sync + 'static,
        classify: impl FnOnce(&T) -> command::CommandDisposition + Send + 'static,
        finish: impl FnOnce(&mut App, Result<T, Failure>) + 'static,
    ) {
        if self.plugin_files.expired {
            finish(
                self,
                Err(Failure::before(
                    "plugin file operation not started before shutdown deadline",
                )),
            );
            return;
        }
        let (bytes, extra_retained_bytes) = bytes;
        let context_bytes = match self.plugin_files.begin_work(
            bytes,
            extra_retained_bytes.saturating_add(std::mem::size_of_val(&finish)),
        ) {
            Ok(context) => context,
            Err(error) => {
                finish(self, Err(error));
                if self.plugin_files.active.is_none() {
                    self.plugin_files.current = None;
                }
                return;
            }
        };
        let weigh: Arc<dyn Fn(&T) -> usize + Send + Sync> = Arc::new(weigh);
        let work_weigh = weigh.clone();
        let queue = self.services.file_command_queue();
        let submitted = if let Some(queue) = &queue {
            command::submit_classified(
                Some(queue),
                bytes,
                work,
                move |value| work_weigh(value),
                classify,
            )
            .map_err(|error| Failure {
                message: error.message,
                saved: if error.unknown { None } else { Some(false) },
                applied: false,
                details: serde_json::Value::Null,
            })
        } else {
            Err(Failure::before(
                "file writer unavailable; plugin operation not accepted",
            ))
        };
        match submitted {
            Ok(receipt) => {
                self.plugin_files.active = Some(Box::new(Accepted {
                    receipt,
                    finish: Some(finish),
                    weigh,
                    context_bytes,
                }))
            }
            Err(error) => {
                finish(self, Err(error));
                if self.plugin_files.active.is_none() {
                    self.plugin_files.current = None;
                }
            }
        }
    }
}

#[cfg(all(test, not(feature = "gui")))]
mod tests;

#[cfg(all(test, feature = "gui"))]
mod shortcut_tests;

#[cfg(test)]
mod classification_tests;

#[cfg(all(test, target_os = "linux"))]
pub(crate) mod shutdown_tests;

#[cfg(all(test, feature = "gui", target_os = "linux"))]
pub(crate) mod native_tests;
