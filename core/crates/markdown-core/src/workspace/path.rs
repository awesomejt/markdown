//! Workspace-relative display paths and the shared entry filter.
use std::borrow::Borrow;

/// Largest accepted [`WorkspacePath`] (UTF-8 bytes).
pub const MAX_PATH_BYTES: usize = 4096;
/// Largest accepted number of path components.
pub const MAX_PATH_DEPTH: usize = 256;
/// Markdown file extensions, compared ASCII case-insensitively
/// (markdown-viewer `fsroot.ClassifyName`).
pub const MARKDOWN_EXTENSIONS: [&str; 3] = ["md", "markdown", "mdx"];

/// Why a display path was rejected. None carries the path itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    Empty,
    TooLong,
    TooDeep,
    /// Starts with `/`.
    Absolute,
    /// `//` or a trailing `/`.
    EmptyComponent,
    /// A `.` or `..` component.
    DotComponent,
    /// A control character or a bidirectional embedding, override or isolate.
    Control,
}
impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::TooDeep => "too_deep",
            Self::Absolute => "absolute",
            Self::EmptyComponent => "empty_component",
            Self::DotComponent => "dot_component",
            Self::Control => "control",
        })
    }
}
impl std::error::Error for PathError {}

/// A validated, `/`-separated path relative to the workspace root. It is a
/// display and matching label only: it grants no access, and the core never
/// resolves it. Adapters keep the real file capability behind the entry ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspacePath(String);
impl WorkspacePath {
    /// Validates without normalizing: no Unicode normalization, case change
    /// or separator conversion. Backslashes and colons are ordinary characters.
    pub fn new(path: impl Into<String>) -> Result<Self, PathError> {
        let path = path.into();
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        if path.len() > MAX_PATH_BYTES {
            return Err(PathError::TooLong);
        }
        if path.starts_with('/') {
            return Err(PathError::Absolute);
        }
        if path.chars().any(is_rejected_char) {
            return Err(PathError::Control);
        }
        let mut depth = 0;
        for component in path.split('/') {
            depth += 1;
            if depth > MAX_PATH_DEPTH {
                return Err(PathError::TooDeep);
            }
            match component {
                "" => return Err(PathError::EmptyComponent),
                "." | ".." => return Err(PathError::DotComponent),
                _ => {}
            }
        }
        Ok(Self(path))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// The final component.
    pub fn name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
    /// Byte offset of [`name`](Self::name) within the path.
    pub fn name_start(&self) -> usize {
        self.0.rfind('/').map_or(0, |i| i + 1)
    }
    /// True when any component starts with `.` (FR-1.4 dot-file rule).
    pub fn is_hidden(&self) -> bool {
        self.0.split('/').any(|c| c.starts_with('.'))
    }
    pub fn kind(&self) -> EntryKind {
        classify_name(self.name())
    }
    /// True when the path contains an invisible character: a format character
    /// (Unicode General_Category=Cf, the table shared with front matter) or a
    /// blank default-ignorable one such as a Hangul filler or U+2800 (the
    /// table shared with v2 URI targets). ZWNJ/ZWJ, which emoji and Indic names
    /// need, and the display modifiers (variation selectors, Mongolian FVS,
    /// CGJ) are exempt. Such a name can look identical to another; the UI
    /// should mark it rather than hide it.
    pub fn has_invisible(&self) -> bool {
        use crate::metadata::{display_modifier, invisible_character, unsafe_character};
        self.0.chars().any(|c| {
            !matches!(c, '\u{200C}' | '\u{200D}')
                && (unsafe_character(c) || (invisible_character(c) && !display_modifier(c)))
        })
    }
}
impl Borrow<str> for WorkspacePath {
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for WorkspacePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// C0/C1 controls, DEL, and every bidi formatting character (marks,
/// embeddings, overrides and isolates) that can make a displayed name read
/// differently from its characters.
fn is_rejected_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

/// Coarse content class of a file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    Markdown,
    Other,
}

/// Classify a file name by its extension. A leading dot does not start an
/// extension, so `.md` is [`EntryKind::Other`].
pub fn classify_name(name: &str) -> EntryKind {
    match name.rfind('.') {
        Some(dot) if dot > 0 => {
            let ext = &name[dot + 1..];
            if MARKDOWN_EXTENSIONS
                .iter()
                .any(|m| m.eq_ignore_ascii_case(ext))
            {
                EntryKind::Markdown
            } else {
                EntryKind::Other
            }
        }
        _ => EntryKind::Other,
    }
}

/// One adapter-supplied file. `id` is opaque to the core: the adapter maps it
/// back to its own capability (bookmark, handle, URI) to open the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceEntry {
    pub id: u64,
    pub path: WorkspacePath,
}

/// Which entries a search considers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryFilter {
    /// Hidden by the dot-file rule and `include_hidden` is false.
    Hidden,
    /// Not a Markdown name and the search is Markdown-only.
    NotMarkdown,
}
pub(crate) fn filter(
    path: &WorkspacePath,
    include_hidden: bool,
    markdown_only: bool,
) -> Option<EntryFilter> {
    if !include_hidden && path.is_hidden() {
        Some(EntryFilter::Hidden)
    } else if markdown_only && path.kind() != EntryKind::Markdown {
        Some(EntryFilter::NotMarkdown)
    } else {
        None
    }
}
