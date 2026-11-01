//! Linux-local file capabilities and newest-request-only background loading.
mod access;
mod loader;
pub use access::{DocumentCapability, DocumentError};
pub use loader::{DocumentLoader, LoadEvent};
