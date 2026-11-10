//! Remote occupancy, transport queues and mirror presentation resources.
pub(crate) mod state;

pub(crate) mod server;

pub(crate) mod agent_children;
pub(crate) mod terminal_read;

pub(crate) mod file_download;

pub(crate) mod structure_sync;

#[cfg(feature = "gui")]
pub(crate) mod readonly;

#[cfg(feature = "gui")]
pub(crate) mod mesh_frames;

pub(crate) mod mesh_mirror;

pub(crate) mod capture_upload;

pub(crate) mod bulk_transfer;

pub(crate) mod subscription;

pub(crate) mod transfer_spool;

pub(crate) mod html_files;
pub(crate) mod html_transfer;

mod html_descriptor;

pub(crate) mod html_http;
pub(crate) mod html_http_host;
pub(crate) mod html_http_server;

#[cfg(all(debug_assertions, feature = "gui", target_os = "linux"))]
pub(crate) mod http_native_fixture;
