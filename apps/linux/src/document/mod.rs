//! Linux-local file capabilities and newest-request-only background loading.
mod access;
mod loader;
pub mod resources;
pub mod watcher;
pub mod workspace;
pub mod workspace_search;

pub use access::{
    DocumentCapability, DocumentError, FolderCapability, FolderError, revision_token_status,
    sanitize_basename,
};
pub use loader::{DocumentLoader, LoadEvent};
pub use resources::{DocumentResources, ImageBudget, ImageFormatInspector, ResourceError};
pub use watcher::{DocumentWatcher, WorkspaceWatcher};
pub use workspace::{
    MAX_WORKSPACE_ENTRIES, VisibleRow, WorkspaceBrowserModel, WorkspaceEntry, WorkspaceTree,
};
pub use workspace_search::{
    WorkspaceContentFileMatch, WorkspaceNameMatch, WorkspaceSearchHandle, WorkspaceSearchRequest,
    WorkspaceSearchUpdate, start_workspace_search,
};
