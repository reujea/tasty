#[cfg(feature = "gui")]
pub(crate) mod capture_image;
#[cfg(feature = "gui")]
pub(crate) mod context;
#[cfg(feature = "gui")]
pub(crate) mod copy;
pub(crate) mod error;
#[cfg(feature = "gui")]
pub(crate) mod files;
#[cfg(feature = "gui")]
pub(crate) mod terminal;

#[cfg(feature = "gui")]
pub(crate) use context::ClipboardContext;
