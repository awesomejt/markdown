//! Adapter-owned workspace directory enumeration and virtualized tree state (LNX-002B).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use markdown_linux_core::classify_workspace_path;

use super::access::{FolderCapability, FolderError};

pub const MAX_WORKSPACE_ENTRIES: usize = 100_000;
const MAX_WORKSPACE_DEPTH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceEntry {
    pub id: u64,
    pub rel_path: String,
    pub name: String,
    pub is_dir: bool,
    pub is_markdown: bool,
    pub is_hidden: bool,
    pub has_invisible: bool,
    pub depth: u32,
    pub parent_rel: Option<String>,
    pub(crate) full_path: PathBuf,
}

impl WorkspaceEntry {
    pub fn full_path(&self) -> &Path {
        &self.full_path
    }
}

#[derive(Clone, Debug)]
pub struct WorkspaceTree {
    pub root_name: String,
    pub entries: Vec<WorkspaceEntry>,
    pub truncated: bool,
    pub skipped_invalid: usize,
    /// Map from parent `rel_path` (empty string for root) to sorted child indices in `entries`.
    children: HashMap<String, Vec<usize>>,
    by_rel_path: HashMap<String, usize>,
}

impl WorkspaceTree {
    pub fn enumerate(folder: &FolderCapability) -> Result<Self, FolderError> {
        folder.validate()?;
        let root_path = std::fs::canonicalize(folder.path()).map_err(FolderError::io)?;
        let root_name = folder.display_name();

        let mut entries = Vec::new();
        let mut skipped_invalid = 0usize;
        let mut truncated = false;
        let mut next_id = 1u64;

        // Iterative DFS/BFS stack: (dir_full_path, rel_prefix, depth)
        let mut stack: Vec<(PathBuf, String, u32)> = vec![(root_path, String::new(), 0)];

        while let Some((dir_path, rel_prefix, depth)) = stack.pop() {
            if depth as usize >= MAX_WORKSPACE_DEPTH {
                continue;
            }
            let read_dir = match std::fs::read_dir(&dir_path) {
                Ok(rd) => rd,
                Err(_) => continue,
            };

            let mut subdirs = Vec::new();
            for item in read_dir {
                if entries.len() >= MAX_WORKSPACE_ENTRIES {
                    truncated = true;
                    break;
                }
                let Ok(entry) = item else {
                    continue;
                };
                let Ok(ft) = entry.file_type() else {
                    continue;
                };
                // Never follow directory symlinks to prevent cycles or root escapes.
                if ft.is_symlink() && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir()) {
                    skipped_invalid += 1;
                    continue;
                }
                let is_dir = ft.is_dir();
                if !is_dir && !ft.is_file() && !ft.is_symlink() {
                    continue;
                }

                let os_name = entry.file_name();
                let Some(name_str) = os_name.to_str() else {
                    skipped_invalid += 1;
                    continue;
                };

                let rel_path = if rel_prefix.is_empty() {
                    name_str.to_string()
                } else {
                    format!("{rel_prefix}/{name_str}")
                };

                let Ok(Ok(info)) = classify_workspace_path(rel_path.as_bytes()) else {
                    skipped_invalid += 1;
                    continue;
                };

                let display_name = rel_path
                    .get(info.name_start as usize..)
                    .unwrap_or(name_str)
                    .to_string();

                let full_path = entry.path();
                let parent_rel = if rel_prefix.is_empty() {
                    None
                } else {
                    Some(rel_prefix.clone())
                };

                let id = next_id;
                next_id += 1;

                entries.push(WorkspaceEntry {
                    id,
                    rel_path: rel_path.clone(),
                    name: display_name,
                    is_dir,
                    is_markdown: !is_dir && info.is_markdown,
                    is_hidden: info.is_hidden,
                    has_invisible: info.has_invisible,
                    depth,
                    parent_rel,
                    full_path: full_path.clone(),
                });

                if is_dir {
                    subdirs.push((full_path, rel_path, depth + 1));
                }
            }

            if truncated {
                break;
            }
            for sub in subdirs {
                stack.push(sub);
            }
        }

        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_rel_path: HashMap<String, usize> = HashMap::with_capacity(entries.len());
        for (idx, entry) in entries.iter().enumerate() {
            by_rel_path.insert(entry.rel_path.clone(), idx);
            let parent_key = entry.parent_rel.as_deref().unwrap_or("").to_string();
            children.entry(parent_key).or_default().push(idx);
        }

        for list in children.values_mut() {
            list.sort_by(|&a, &b| {
                let ea = &entries[a];
                let eb = &entries[b];
                eb.is_dir
                    .cmp(&ea.is_dir)
                    .then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
                    .then_with(|| ea.rel_path.cmp(&eb.rel_path))
            });
        }

        Ok(Self {
            root_name,
            entries,
            truncated,
            skipped_invalid,
            children,
            by_rel_path,
        })
    }

    pub fn get(&self, index: usize) -> Option<&WorkspaceEntry> {
        self.entries.get(index)
    }

    pub fn index_of(&self, rel_path: &str) -> Option<usize> {
        self.by_rel_path.get(rel_path).copied()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleRow {
    pub entry_index: usize,
    pub expanded: bool,
}

#[derive(Clone, Debug)]
pub struct WorkspaceBrowserModel {
    pub tree: WorkspaceTree,
    expanded: HashSet<String>,
    pub show_hidden: bool,
    pub markdown_only: bool,
    visible: Vec<VisibleRow>,
}

impl WorkspaceBrowserModel {
    pub fn new(tree: WorkspaceTree) -> Self {
        let mut model = Self {
            tree,
            expanded: HashSet::new(),
            show_hidden: false,
            markdown_only: true,
            visible: Vec::new(),
        };
        model.rebuild();
        model
    }

    pub fn visible_rows(&self) -> &[VisibleRow] {
        &self.visible
    }

    pub fn visible_entry(&self, row_index: usize) -> Option<(&WorkspaceEntry, bool)> {
        let row = self.visible.get(row_index)?;
        let entry = self.tree.get(row.entry_index)?;
        Some((entry, row.expanded))
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        if self.show_hidden != show {
            self.show_hidden = show;
            self.rebuild();
        }
    }

    pub fn set_markdown_only(&mut self, md_only: bool) {
        if self.markdown_only != md_only {
            self.markdown_only = md_only;
            self.rebuild();
        }
    }

    pub fn toggle_expand(&mut self, row_index: usize) -> bool {
        let Some(row) = self.visible.get(row_index) else {
            return false;
        };
        let Some(entry) = self.tree.get(row.entry_index) else {
            return false;
        };
        if !entry.is_dir {
            return false;
        }
        let key = entry.rel_path.clone();
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        self.rebuild();
        true
    }

    /// Expand directory at `row_index` if collapsed. Returns `true` if state changed.
    pub fn expand_row(&mut self, row_index: usize) -> bool {
        let Some(row) = self.visible.get(row_index) else {
            return false;
        };
        let Some(entry) = self.tree.get(row.entry_index) else {
            return false;
        };
        if !entry.is_dir || row.expanded {
            return false;
        }
        self.expanded.insert(entry.rel_path.clone());
        self.rebuild();
        true
    }

    /// Collapse directory at `row_index` if expanded, otherwise return its parent's visible row index.
    pub fn collapse_or_parent(&mut self, row_index: usize) -> Option<usize> {
        let row = self.visible.get(row_index)?;
        let entry = self.tree.get(row.entry_index)?;
        if entry.is_dir && row.expanded {
            self.expanded.remove(&entry.rel_path);
            self.rebuild();
            return Some(row_index);
        }
        let parent_rel = entry.parent_rel.as_deref()?;
        self.visible.iter().position(|r| {
            self.tree.get(r.entry_index).map(|e| e.rel_path.as_str()) == Some(parent_rel)
        })
    }

    pub fn expand_all_ancestors_of(&mut self, rel_path: &str) {
        let mut cur = rel_path;
        while let Some((parent, _)) = cur.rsplit_once('/') {
            self.expanded.insert(parent.to_string());
            cur = parent;
        }
        self.rebuild();
    }

    pub fn row_index_for_rel_path(&self, rel_path: &str) -> Option<usize> {
        self.visible.iter().position(|r| {
            self.tree
                .get(r.entry_index)
                .is_some_and(|e| e.rel_path == rel_path)
        })
    }

    fn rebuild(&mut self) {
        self.visible.clear();
        self.append_children("");
    }

    fn append_children(&mut self, parent_key: &str) {
        let Some(child_indices) = self.tree.children.get(parent_key).cloned() else {
            return;
        };
        for idx in child_indices {
            let entry = &self.tree.entries[idx];
            if !self.show_hidden && entry.is_hidden {
                continue;
            }
            if !entry.is_dir && self.markdown_only && !entry.is_markdown {
                continue;
            }
            let is_expanded = entry.is_dir && self.expanded.contains(&entry.rel_path);
            self.visible.push(VisibleRow {
                entry_index: idx,
                expanded: is_expanded,
            });
            if is_expanded {
                let rel = entry.rel_path.clone();
                self.append_children(&rel);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "markdown-linux-ws-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn enumerates_sorts_filters_and_flags_invisible() {
        let dir = temp_dir("basic");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("README.md"), b"# Root").unwrap();
        std::fs::write(dir.join("notes.txt"), b"plain").unwrap();
        std::fs::write(dir.join(".hidden.md"), b"# Hidden").unwrap();
        std::fs::write(dir.join(".git/HEAD.md"), b"# Git").unwrap();
        std::fs::write(dir.join("sub/Guide.MDX"), b"# Guide").unwrap();
        std::fs::write(dir.join(format!("sub/{}blank.md", '\u{2800}')), b"# Blank").unwrap();
        // Invalid UTF-8 entry must be skipped without failing enumeration.
        let bad_name = std::ffi::OsStr::from_bytes(b"bad\xff.md");
        let _ = std::fs::write(dir.join(bad_name), b"# Bad");

        let cap = FolderCapability::from_local_folder(dir.clone()).unwrap();
        let tree = WorkspaceTree::enumerate(&cap).unwrap();
        assert_eq!(tree.skipped_invalid, 1);
        assert!(!tree.truncated);

        let mut model = WorkspaceBrowserModel::new(tree);
        // Default: hidden excluded, markdown_only=true -> "sub" dir and "README.md"
        assert_eq!(model.visible_rows().len(), 2);
        assert_eq!(model.visible_entry(0).unwrap().0.name, "sub");
        assert!(model.visible_entry(0).unwrap().0.is_dir);
        assert_eq!(model.visible_entry(1).unwrap().0.name, "README.md");

        // Expand "sub"
        assert!(model.expand_row(0));
        assert_eq!(model.visible_rows().len(), 4);
        assert_eq!(model.visible_entry(1).unwrap().0.name, "Guide.MDX");
        assert!(model.visible_entry(2).unwrap().0.has_invisible);

        // Collapse via child -> jumps to parent "sub" (index 0)
        assert_eq!(model.collapse_or_parent(1), Some(0));
        // Collapse "sub" itself -> stays at 0 and collapses
        assert_eq!(model.collapse_or_parent(0), Some(0));
        assert_eq!(model.visible_rows().len(), 2);

        // Toggle all files + hidden
        model.set_markdown_only(false);
        assert_eq!(model.visible_rows().len(), 3); // sub, notes.txt, README.md
        model.set_show_hidden(true);
        assert_eq!(model.visible_rows().len(), 5); // .git, sub, .hidden.md, notes.txt, README.md

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_symlinked_directory_loops() {
        let dir = temp_dir("symlink");
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("a/doc.md"), b"# Doc").unwrap();
        symlink(&dir, dir.join("a/loop")).unwrap();

        let cap = FolderCapability::from_local_folder(dir.clone()).unwrap();
        let tree = WorkspaceTree::enumerate(&cap).unwrap();
        assert!(tree.entries.iter().any(|e| e.rel_path == "a/doc.md"));
        assert!(!tree.entries.iter().any(|e| e.rel_path == "a/loop"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_folder_enumeration_and_navigation() {
        let dir = temp_dir("large");
        for i in 0..2500 {
            let name = format!("doc-{i:04}.md");
            std::fs::write(dir.join(name), b"# Doc").unwrap();
        }
        let cap = FolderCapability::from_local_folder(dir.clone()).unwrap();
        let tree = WorkspaceTree::enumerate(&cap).unwrap();
        assert_eq!(tree.entries.len(), 2500);
        let model = WorkspaceBrowserModel::new(tree);
        assert_eq!(model.visible_rows().len(), 2500);
        assert_eq!(model.visible_entry(0).unwrap().0.name, "doc-0000.md");
        assert_eq!(model.visible_entry(2499).unwrap().0.name, "doc-2499.md");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
