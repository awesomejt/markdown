//! Safe wrapper for workspace path classification via the C ABI (CORE-005B2/CORE-005D).

use crate::ffi;
use crate::runtime::{CoreError, checked};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspacePathInfo {
    pub is_markdown: bool,
    pub is_hidden: bool,
    pub has_invisible: bool,
    pub name_start: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePathError {
    Empty,
    TooLong,
    TooDeep,
    Absolute,
    EmptyComponent,
    DotComponent,
    Control,
    InvalidUtf8,
}

/// Classify a relative `/`-separated display path without opening or resolving files.
pub fn classify_workspace_path(
    path: &[u8],
) -> Result<Result<WorkspacePathInfo, WorkspacePathError>, CoreError> {
    let mut info = ffi::md_workspace_path_info {
        struct_size: ffi::size_of::<ffi::md_workspace_path_info>(),
        ..Default::default()
    };
    let ptr = if path.is_empty() {
        std::ptr::null()
    } else {
        path.as_ptr()
    };
    // SAFETY: `ptr` is either null with length 0 or points to `path.len()` readable bytes,
    // and `info` is an exclusively borrowed `md_workspace_path_info` with its exact struct_size.
    checked(unsafe { ffi::md_workspace_classify_path(ptr, path.len(), &mut info) })?;
    let res = match info.error {
        ffi::MD_PATH_VALID => Ok(WorkspacePathInfo {
            is_markdown: (info.flags & ffi::MD_PATH_MARKDOWN) != 0,
            is_hidden: (info.flags & ffi::MD_PATH_HIDDEN) != 0,
            has_invisible: (info.flags & ffi::MD_PATH_INVISIBLE) != 0,
            name_start: info.name_start,
        }),
        ffi::MD_PATH_EMPTY => Err(WorkspacePathError::Empty),
        ffi::MD_PATH_TOO_LONG => Err(WorkspacePathError::TooLong),
        ffi::MD_PATH_TOO_DEEP => Err(WorkspacePathError::TooDeep),
        ffi::MD_PATH_ABSOLUTE => Err(WorkspacePathError::Absolute),
        ffi::MD_PATH_EMPTY_COMPONENT => Err(WorkspacePathError::EmptyComponent),
        ffi::MD_PATH_DOT_COMPONENT => Err(WorkspacePathError::DotComponent),
        ffi::MD_PATH_CONTROL => Err(WorkspacePathError::Control),
        ffi::MD_PATH_INVALID_UTF8 => Err(WorkspacePathError::InvalidUtf8),
        _ => return Err(CoreError::IncompatibleAbi),
    };
    Ok(res)
}
