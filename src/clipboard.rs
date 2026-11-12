#[cfg(feature = "gui")]
pub(crate) mod capture_image;
#[cfg(feature = "gui")]
pub(crate) mod context;
pub(crate) mod error;
#[cfg(feature = "gui")]
pub(crate) mod files;
#[cfg(feature = "gui")]
pub(crate) mod terminal;

#[cfg(feature = "gui")]
pub(crate) use context::ClipboardContext;
