//! App integration for remote mirrors; each child imports its actual collaborators.
mod agent_children;
mod agent_origin;
pub(crate) mod attempts;
mod bulk;
mod connection;
mod dispatch;
mod documents;
mod forward;
mod git_query;
mod home_probe;
mod html;
pub(crate) mod into_gui;
mod location;
mod navigation;
pub(crate) mod op_runs;
mod output;
pub(crate) mod pending;
mod projection;
mod resize_sync;
mod resources;
pub(crate) mod round_intake;
mod survivors;
mod terminal_read;

mod file_download;
pub(crate) mod saved_row;
mod user_waits;
mod wire;
pub(crate) mod workspace_close;

#[cfg(test)]
pub(crate) mod tests;

use git_query::deliver_git_query_result;

pub(crate) use bulk::{BULK_REJECT_PREFIX, BulkEndpoint, upload_file_over_bulk};
pub(super) use connection::find_parked_with_workspace;
pub(crate) use forward::RemoteTarget;
