//! Linux-local file capabilities and newest-request-only background loading.
mod access;
mod loader;
pub mod resources;

pub use access::{DocumentCapability, DocumentError};
pub use loader::{DocumentLoader, LoadEvent};
pub use resources::{DocumentResources, ImageBudget, ImageFormatInspector, ResourceError};
