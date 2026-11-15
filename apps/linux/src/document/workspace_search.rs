//! Bounded, cancellable background workspace search over CORE-005B/B2/C primitives (LNX-002C).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use markdown_linux_core::{
    ContentAdmitOutcome, ContentFileOutcome, ContentHit, ContentProgress, ContentSearchOptions,
    CoreError, NameRank, NameSearchOptions, WorkspaceNameEntry, admit_workspace_content,
    ffi::MD_CONTENT_DEFAULT_MAX_FILE_BYTES, find_workspace_names, search_workspace_content,
};

use super::access::FolderCapability;
use super::workspace::{WorkspaceEntry, WorkspaceTree};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSearchRequest {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub include_hidden: bool,
    pub all_files_for_names: bool,
    pub name_options: NameSearchOptions,
    pub content_options: ContentSearchOptions,
}

impl WorkspaceSearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            case_sensitive: false,
            whole_word: false,
            include_hidden: false,
            all_files_for_names: false,
            name_options: NameSearchOptions::default(),
            content_options: ContentSearchOptions::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceNameMatch {
    pub id: u64,
    pub rel_path: String,
    pub name: String,
    pub full_path: PathBuf,
    pub has_invisible: bool,
    pub rank: NameRank,
    pub match_start: u32,
    pub match_end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceContentFileMatch {
    pub id: u64,
    pub rel_path: String,
    pub full_path: PathBuf,
    pub has_invisible: bool,
    pub hits: Vec<ContentHit>,
    pub truncated: bool,
    pub source_length: u64,
    pub source_sha256: [u8; 64],
}

#[derive(Clone, Debug)]
pub enum WorkspaceSearchUpdate {
    Names {
        search_id: u64,
        hits: Vec<WorkspaceNameMatch>,
        truncated: bool,
        entries_limited: bool,
        total_hits: u64,
    },
    ContentBatch {
        search_id: u64,
        files: Vec<WorkspaceContentFileMatch>,
        progress: ContentProgress,
    },
    Finished {
        search_id: u64,
        progress: ContentProgress,
        cancelled: bool,
        error: Option<String>,
    },
}

#[derive(Clone)]
pub struct WorkspaceSearchHandle {
    pub search_id: u64,
    cancel_flag: Arc<AtomicBool>,
}

impl WorkspaceSearchHandle {
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }
}

pub fn start_workspace_search(
    folder: FolderCapability,
    tree: Arc<WorkspaceTree>,
    request: WorkspaceSearchRequest,
    search_id: u64,
    sender: mpsc::Sender<WorkspaceSearchUpdate>,
) -> WorkspaceSearchHandle {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_flag);

    let worker_sender = sender.clone();
    let spawn_res = std::thread::Builder::new()
        .name("workspace-search".into())
        .spawn(move || {
            run_workspace_search(
                folder,
                &tree,
                &request,
                search_id,
                &worker_cancel,
                &worker_sender,
            );
        });

    if let Err(err) = spawn_res {
        let _ = sender.send(WorkspaceSearchUpdate::Finished {
            search_id,
            progress: ContentProgress::new(),
            cancelled: false,
            error: Some(format!("Failed to start search worker: {err}")),
        });
    }

    WorkspaceSearchHandle {
        search_id,
        cancel_flag,
    }
}

pub fn run_workspace_search(
    folder: FolderCapability,
    tree: &WorkspaceTree,
    request: &WorkspaceSearchRequest,
    search_id: u64,
    cancel_flag: &AtomicBool,
    sender: &mpsc::Sender<WorkspaceSearchUpdate>,
) {
    let mut progress = ContentProgress::new();
    if request.query.trim().is_empty() {
        let _ = sender.send(WorkspaceSearchUpdate::Finished {
            search_id,
            progress,
            cancelled: false,
            error: None,
        });
        return;
    }

    if cancel_flag.load(Ordering::Relaxed) {
        let _ = sender.send(WorkspaceSearchUpdate::Finished {
            search_id,
            progress,
            cancelled: true,
            error: None,
        });
        return;
    }

    let mut file_entries: Vec<&WorkspaceEntry> =
        tree.entries.iter().filter(|e| !e.is_dir).collect();
    file_entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let name_inputs: Vec<WorkspaceNameEntry<'_>> = file_entries
        .iter()
        .map(|e| WorkspaceNameEntry {
            id: e.id,
            rel_path: &e.rel_path,
        })
        .collect();

    let mut name_opts = request.name_options;
    name_opts.case_sensitive = request.case_sensitive;
    name_opts.include_hidden = request.include_hidden;
    name_opts.all_files = request.all_files_for_names;

    match find_workspace_names(&name_inputs, &request.query, &name_opts) {
        Ok(res) => {
            let hits = res
                .hits
                .into_iter()
                .filter_map(|h| {
                    let entry = file_entries.get(h.entry_index as usize)?;
                    Some(WorkspaceNameMatch {
                        id: entry.id,
                        rel_path: entry.rel_path.clone(),
                        name: entry.name.clone(),
                        full_path: entry.full_path().to_path_buf(),
                        has_invisible: entry.has_invisible,
                        rank: h.rank,
                        match_start: h.match_start,
                        match_end: h.match_end,
                    })
                })
                .collect();
            let _ = sender.send(WorkspaceSearchUpdate::Names {
                search_id,
                hits,
                truncated: res.truncated,
                entries_limited: res.entries_limited,
                total_hits: res.total_hits,
            });
        }
        Err(CoreError::QueryTooLong) => {
            let _ = sender.send(WorkspaceSearchUpdate::Finished {
                search_id,
                progress,
                cancelled: false,
                error: Some("Search query is too long".to_string()),
            });
            return;
        }
        Err(err) => {
            let _ = sender.send(WorkspaceSearchUpdate::Finished {
                search_id,
                progress,
                cancelled: false,
                error: Some(format!("Workspace name search failed: {err}")),
            });
            return;
        }
    }

    if cancel_flag.load(Ordering::Relaxed) {
        let _ = sender.send(WorkspaceSearchUpdate::Finished {
            search_id,
            progress,
            cancelled: true,
            error: None,
        });
        return;
    }

    if let Err(err) = folder.validate() {
        let _ = sender.send(WorkspaceSearchUpdate::Finished {
            search_id,
            progress,
            cancelled: false,
            error: Some(err.to_string()),
        });
        return;
    }
    let Ok(canonical_root) = std::fs::canonicalize(folder.path()) else {
        let _ = sender.send(WorkspaceSearchUpdate::Finished {
            search_id,
            progress,
            cancelled: false,
            error: Some("Workspace folder is no longer available".to_string()),
        });
        return;
    };

    let mut content_opts = request.content_options;
    content_opts.case_sensitive = request.case_sensitive;
    content_opts.whole_word = request.whole_word;
    content_opts.include_hidden = request.include_hidden;
    let max_file_bytes = if content_opts.max_file_bytes == 0 {
        MD_CONTENT_DEFAULT_MAX_FILE_BYTES as usize
    } else {
        content_opts.max_file_bytes as usize
    };

    let mut buf = Vec::new();
    let mut batch = Vec::new();

    for entry in &file_entries {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = sender.send(WorkspaceSearchUpdate::Finished {
                search_id,
                progress,
                cancelled: true,
                error: None,
            });
            return;
        }

        let Some(file_len) = stat_contained_file(entry.full_path(), &canonical_root) else {
            continue;
        };

        match admit_workspace_content(
            &request.query,
            &content_opts,
            &entry.rel_path,
            file_len,
            &mut progress,
        ) {
            Ok(ContentAdmitOutcome::Read) => {}
            Ok(ContentAdmitOutcome::Skipped(_)) => continue,
            Ok(ContentAdmitOutcome::Stopped(_)) => break,
            Err(CoreError::QueryTooLong) => {
                let _ = sender.send(WorkspaceSearchUpdate::Finished {
                    search_id,
                    progress,
                    cancelled: false,
                    error: Some("Search query is too long".to_string()),
                });
                return;
            }
            Err(_) => continue,
        }

        if !read_contained_file(entry.full_path(), &canonical_root, max_file_bytes, &mut buf) {
            continue;
        }

        match search_workspace_content(
            &request.query,
            &content_opts,
            &entry.rel_path,
            &buf,
            &mut progress,
        ) {
            Ok(ContentFileOutcome::Searched {
                hits,
                truncated,
                source_length,
                source_sha256,
            }) => {
                if !hits.is_empty() {
                    batch.push(WorkspaceContentFileMatch {
                        id: entry.id,
                        rel_path: entry.rel_path.clone(),
                        full_path: entry.full_path().to_path_buf(),
                        has_invisible: entry.has_invisible,
                        hits,
                        truncated,
                        source_length,
                        source_sha256,
                    });
                    if batch.len() >= 8 {
                        let _ = sender.send(WorkspaceSearchUpdate::ContentBatch {
                            search_id,
                            files: std::mem::take(&mut batch),
                            progress,
                        });
                    }
                }
            }
            Ok(ContentFileOutcome::Skipped(_)) => continue,
            Ok(ContentFileOutcome::Stopped(_)) => break,
            Err(_) => continue,
        }
    }

    if !batch.is_empty() {
        let _ = sender.send(WorkspaceSearchUpdate::ContentBatch {
            search_id,
            files: batch,
            progress,
        });
    }

    let _ = sender.send(WorkspaceSearchUpdate::Finished {
        search_id,
        progress,
        cancelled: cancel_flag.load(Ordering::Relaxed),
        error: None,
    });
}

fn stat_contained_file(path: &Path, canonical_root: &Path) -> Option<u64> {
    let canonical = std::fs::canonicalize(path).ok()?;
    if !canonical.starts_with(canonical_root) {
        return None;
    }
    let meta = std::fs::metadata(&canonical).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some(meta.len())
}

fn read_contained_file(
    path: &Path,
    canonical_root: &Path,
    max_file_bytes: usize,
    buf: &mut Vec<u8>,
) -> bool {
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return false;
    };
    if !canonical.starts_with(canonical_root) {
        return false;
    }
    let Ok(file) = File::open(&canonical) else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    buf.clear();
    let limit = max_file_bytes.saturating_add(1) as u64;
    file.take(limit).read_to_end(buf).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "markdown_linux_ws_search_{label}_{}_{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn workspace_search_finds_names_and_content_with_bounds_and_cancel() {
        let dir = make_temp_dir("basic");
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        fs::write(
            dir.join("docs/alpha.md"),
            "# Alpha\n\nNeedle one here.\n\nNeedle two here.\n",
        )
        .unwrap();
        fs::write(dir.join("needle-file.md"), "# Beta\n\nNo match in body.\n").unwrap();
        fs::write(
            dir.join(".hidden/secret.md"),
            "# Secret\n\nNeedle hidden.\n",
        )
        .unwrap();
        fs::write(dir.join("notes.txt"), "Needle in plain text\n").unwrap();

        let cap = FolderCapability::from_local_folder(dir.clone()).unwrap();
        let tree = Arc::new(WorkspaceTree::enumerate(&cap).unwrap());

        let (tx, rx) = mpsc::channel();
        let mut req = WorkspaceSearchRequest::new("needle");
        req.content_options.max_total_matches = 2;
        let cancel = AtomicBool::new(false);
        run_workspace_search(cap.clone(), &tree, &req, 1, &cancel, &tx);

        let updates: Vec<_> = rx.try_iter().collect();
        let names = updates.iter().find_map(|u| match u {
            WorkspaceSearchUpdate::Names { hits, .. } => Some(hits.clone()),
            _ => None,
        });
        let names = names.expect("expected name hits");
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].rel_path, "needle-file.md");

        let content_files: Vec<_> = updates
            .iter()
            .filter_map(|u| match u {
                WorkspaceSearchUpdate::ContentBatch { files, .. } => Some(files.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(content_files.len(), 1);
        assert_eq!(content_files[0].rel_path, "docs/alpha.md");
        assert_eq!(content_files[0].hits.len(), 2);

        let finished = updates.iter().find_map(|u| match u {
            WorkspaceSearchUpdate::Finished {
                progress,
                cancelled,
                error,
                ..
            } => Some((*progress, *cancelled, error.clone())),
            _ => None,
        });
        let (progress, cancelled, error) = finished.expect("expected finished");
        assert!(!cancelled);
        assert!(error.is_none());
        assert_eq!(progress.matches(), 2);
        assert_eq!(progress.skipped_hidden(), 1);

        // Pre-cancelled search returns cancelled=true with no content batches
        let (tx2, rx2) = mpsc::channel();
        let cancel2 = AtomicBool::new(true);
        run_workspace_search(cap, &tree, &req, 2, &cancel2, &tx2);
        let updates2: Vec<_> = rx2.try_iter().collect();
        assert!(matches!(
            updates2.last(),
            Some(WorkspaceSearchUpdate::Finished {
                cancelled: true,
                ..
            })
        ));

        let _ = fs::remove_dir_all(&dir);
    }
}
