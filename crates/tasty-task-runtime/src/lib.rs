// reason: Test fixtures intentionally discard setup/cleanup results; production builds retain the must-use lint.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]
//! Task execution and completion ownership. TaskStore remains the sole durable task source.
//! Stopping a runner or dropping an engine scope neither cancels tasks nor kills running children.
pub mod agent_task;
pub mod agent_turns;
pub mod completion;
pub mod event_feed;
pub mod graph_view;
pub mod hook_wait;
#[cfg(all(test, unix))]
mod process_gone_tests_support;
mod report;
pub(crate) mod runner_host;
pub(crate) mod runner_thread;
mod service;
mod store_call;
pub mod task;
pub mod task_output_ref;
mod task_tx;
pub mod task_waker;
#[cfg(test)]
mod terminal_cleanup_tests;
mod terminal_recovery;
#[cfg(test)]
mod terminal_recovery_tests;
pub use runner_thread::{RunnerRegistry, RunnerStatus, RunnerStopObservation, RunnerStopReceipt};
pub use service::{TaskAwaiter, TaskScope, TaskService};
pub use store_call::FromBlocked;
pub use task_tx::{TaskEffects, TaskTx};
pub use terminal_recovery::TerminalRecovery;
