//! Persistent capability grants and recent files/workspaces (LNX-002A).
//!
//! Stores recent files and workspace folders with grant status in
//! `$XDG_CONFIG_HOME/markdown/recents.json` (or `MARKDOWN_RECENTS_PATH`).

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::document::{DocumentCapability, DocumentError, FolderCapability, FolderError};

pub const MAX_RECENT_ITEMS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentKind {
    File,
    Workspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentStatus {
    Available,
    MissingOrMoved,
    PermissionDenied,
    AccessRevoked,
    VolumeUnavailable,
    InvalidTarget,
}

impl RecentStatus {
    pub fn message(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::MissingOrMoved => "Missing or moved",
            Self::PermissionDenied => "Permission denied",
            Self::AccessRevoked => "Access revoked",
            Self::VolumeUnavailable => "Storage unavailable",
            Self::InvalidTarget => "Invalid target",
        }
    }

    pub fn is_recoverable(self) -> bool {
        matches!(
            self,
            Self::MissingOrMoved
                | Self::PermissionDenied
                | Self::AccessRevoked
                | Self::VolumeUnavailable
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentEntry {
    pub kind: RecentKind,
    pub path: PathBuf,
    pub display_name: String,
    pub last_opened_epoch_secs: u64,
    pub grant_active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentsState {
    pub files: Vec<RecentEntry>,
    pub workspaces: Vec<RecentEntry>,
}

static IN_MEMORY: Mutex<Option<RecentsState>> = Mutex::new(None);

pub fn default_store_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MARKDOWN_RECENTS_PATH") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("markdown").join("recents.json"))
}

pub fn is_memory_mode() -> bool {
    std::env::var_os("MARKDOWN_RECENTS_IN_MEMORY").is_some()
}

pub fn load() -> RecentsState {
    if is_memory_mode() {
        let mem = IN_MEMORY.lock().unwrap();
        return mem.clone().unwrap_or_default();
    }
    let Some(path) = default_store_path() else {
        return RecentsState::default();
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return RecentsState::default();
    };
    parse_json(&data)
}

pub fn save(state: &RecentsState) {
    if is_memory_mode() {
        let mut mem = IN_MEMORY.lock().unwrap();
        *mem = Some(state.clone());
        return;
    }
    let Some(path) = default_store_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = format_json(state);
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn clear() {
    if is_memory_mode() {
        let mut mem = IN_MEMORY.lock().unwrap();
        *mem = None;
        return;
    }
    if let Some(path) = default_store_path() {
        let _ = std::fs::remove_file(path);
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn upsert_entry(list: &mut Vec<RecentEntry>, entry: RecentEntry) {
    list.retain(|e| e.path != entry.path);
    list.insert(0, entry);
    if list.len() > MAX_RECENT_ITEMS {
        list.truncate(MAX_RECENT_ITEMS);
    }
}

pub fn record_file(path: &Path, display_name: &str) {
    let mut state = load();
    upsert_entry(
        &mut state.files,
        RecentEntry {
            kind: RecentKind::File,
            path: path.to_path_buf(),
            display_name: display_name.to_string(),
            last_opened_epoch_secs: now_epoch_secs(),
            grant_active: true,
        },
    );
    save(&state);
}

pub fn record_workspace(path: &Path, display_name: &str) {
    let mut state = load();
    upsert_entry(
        &mut state.workspaces,
        RecentEntry {
            kind: RecentKind::Workspace,
            path: path.to_path_buf(),
            display_name: display_name.to_string(),
            last_opened_epoch_secs: now_epoch_secs(),
            grant_active: true,
        },
    );
    save(&state);
}

pub fn revoke_grant(kind: RecentKind, path: &Path) {
    let mut state = load();
    let list = match kind {
        RecentKind::File => &mut state.files,
        RecentKind::Workspace => &mut state.workspaces,
    };
    for entry in list.iter_mut() {
        if entry.path == path {
            entry.grant_active = false;
        }
    }
    save(&state);
}

pub fn relocate_entry(
    kind: RecentKind,
    old_path: &Path,
    new_path: &Path,
    new_display_name: &str,
) -> bool {
    let mut state = load();
    let list = match kind {
        RecentKind::File => &mut state.files,
        RecentKind::Workspace => &mut state.workspaces,
    };
    let pos = list.iter().position(|e| e.path == old_path);
    let Some(idx) = pos else {
        return false;
    };
    list.remove(idx);
    list.retain(|e| e.path != new_path);
    list.insert(
        0,
        RecentEntry {
            kind,
            path: new_path.to_path_buf(),
            display_name: new_display_name.to_string(),
            last_opened_epoch_secs: now_epoch_secs(),
            grant_active: true,
        },
    );
    if list.len() > MAX_RECENT_ITEMS {
        list.truncate(MAX_RECENT_ITEMS);
    }
    save(&state);
    true
}

pub fn remove_entry(kind: RecentKind, path: &Path) {
    let mut state = load();
    let list = match kind {
        RecentKind::File => &mut state.files,
        RecentKind::Workspace => &mut state.workspaces,
    };
    list.retain(|e| e.path != path);
    save(&state);
}

pub fn check_availability(entry: &RecentEntry) -> RecentStatus {
    if !entry.grant_active {
        return RecentStatus::AccessRevoked;
    }
    match entry.kind {
        RecentKind::File => {
            let cap = DocumentCapability::from_local_file(entry.path.clone());
            match crate::document::revision_token_status(&cap) {
                Ok(()) => RecentStatus::Available,
                Err(DocumentError::NotFound) => RecentStatus::MissingOrMoved,
                Err(DocumentError::PermissionDenied) => RecentStatus::PermissionDenied,
                Err(DocumentError::AccessRevoked) => RecentStatus::AccessRevoked,
                Err(DocumentError::VolumeUnavailable) => RecentStatus::VolumeUnavailable,
                Err(_) => RecentStatus::InvalidTarget,
            }
        }
        RecentKind::Workspace => {
            let cap = FolderCapability::from_persisted(entry.path.clone(), entry.grant_active);
            match cap.validate() {
                Ok(()) => RecentStatus::Available,
                Err(FolderError::NotFound) => RecentStatus::MissingOrMoved,
                Err(FolderError::PermissionDenied) => RecentStatus::PermissionDenied,
                Err(FolderError::AccessRevoked) => RecentStatus::AccessRevoked,
                Err(FolderError::VolumeUnavailable) => RecentStatus::VolumeUnavailable,
                Err(_) => RecentStatus::InvalidTarget,
            }
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let hi = (pair[0] as char).to_digit(16)? as u8;
        let lo = (pair[1] as char).to_digit(16)? as u8;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn encode_os_path(path: &Path) -> (String, Option<String>) {
    let os: &OsStr = path.as_os_str();
    match os.to_str() {
        Some(utf8) => (utf8.to_string(), None),
        None => (
            os.to_string_lossy().into_owned(),
            Some(hex_encode(os.as_bytes())),
        ),
    }
}

fn decode_os_path(path_str: &str, path_hex: Option<&str>) -> Option<PathBuf> {
    if let Some(hex) = path_hex
        && let Some(raw) = hex_decode(hex)
    {
        return Some(PathBuf::from(OsString::from_vec(raw)));
    }
    if path_str.is_empty() {
        None
    } else {
        Some(PathBuf::from(path_str))
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn format_entries(out: &mut String, entries: &[RecentEntry]) {
    for (i, e) in entries.iter().enumerate() {
        let comma = if i + 1 < entries.len() { "," } else { "" };
        let (path_str, path_hex) = encode_os_path(&e.path);
        let hex_field = match path_hex {
            Some(h) => format!("\"{}\"", escape_json(&h)),
            None => "null".to_string(),
        };
        out.push_str(&format!(
            "    {{\n      \"path\": \"{}\",\n      \"path_hex\": {},\n      \"display_name\": \"{}\",\n      \"last_opened_epoch_secs\": {},\n      \"grant_active\": {}\n    }}{}\n",
            escape_json(&path_str),
            hex_field,
            escape_json(&e.display_name),
            e.last_opened_epoch_secs,
            e.grant_active,
            comma
        ));
    }
}

fn format_json(state: &RecentsState) -> String {
    let mut out = String::from("{\n  \"version\": 1,\n  \"files\": [\n");
    format_entries(&mut out, &state.files);
    out.push_str("  ],\n  \"workspaces\": [\n");
    format_entries(&mut out, &state.workspaces);
    out.push_str("  ]\n}\n");
    out
}

fn extract_array_section<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let key_pat = format!("\"{key}\"");
    let key_pos = json.find(&key_pat)?;
    let rest = &json[key_pos + key_pat.len()..];
    let arr_start = rest.find('[')?;
    let mut depth = 0usize;
    let mut in_quotes = false;
    let mut escape = false;
    for (i, c) in rest[arr_start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' && in_quotes {
            escape = true;
            continue;
        }
        if c == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if !in_quotes {
            if c == '[' {
                depth += 1;
            } else if c == ']' {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[arr_start + 1..arr_start + i]);
                }
            }
        }
    }
    None
}

fn extract_json_objects(s: &str) -> Vec<&str> {
    let mut objects = Vec::new();
    let mut depth = 0;
    let mut obj_start = 0;
    let mut in_quotes = false;
    let mut escape = false;

    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' && in_quotes {
            escape = true;
            continue;
        }
        if c == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if !in_quotes {
            if c == '{' {
                if depth == 0 {
                    obj_start = i;
                }
                depth += 1;
            } else if c == '}' && depth > 0 {
                depth -= 1;
                if depth == 0 {
                    objects.push(&s[obj_start..=i]);
                }
            }
        }
    }
    objects
}

fn extract_string(block: &str, key: &str) -> Option<String> {
    let key_pat = format!("\"{key}\"");
    let key_pos = block.find(&key_pat)?;
    let after_key = &block[key_pos + key_pat.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    if after_colon.starts_with("null") || !after_colon.starts_with('"') {
        return None;
    }
    let rest = &after_colon[1..];
    let mut s = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            return Some(s);
        }
        if c == '\\' {
            if let Some(escaped) = chars.next() {
                match escaped {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Ok(code) = u32::from_str_radix(&hex, 16)
                            && let Some(ch) = char::from_u32(code)
                        {
                            s.push(ch);
                        }
                    }
                    other => s.push(other),
                }
            }
        } else {
            s.push(c);
        }
    }
    None
}

fn extract_u64(block: &str, key: &str) -> Option<u64> {
    let key_pat = format!("\"{key}\"");
    let key_pos = block.find(&key_pat)?;
    let after_key = &block[key_pos + key_pat.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    let num_end = after_colon
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_colon.len());
    after_colon[..num_end].parse::<u64>().ok()
}

fn extract_bool(block: &str, key: &str) -> Option<bool> {
    let key_pat = format!("\"{key}\"");
    let key_pos = block.find(&key_pat)?;
    let after_key = &block[key_pos + key_pat.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    if after_colon.starts_with("true") {
        Some(true)
    } else if after_colon.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_entries(arr_str: &str, kind: RecentKind) -> Vec<RecentEntry> {
    let mut entries = Vec::new();
    for item_str in extract_json_objects(arr_str) {
        let path_str = extract_string(item_str, "path").unwrap_or_default();
        let path_hex = extract_string(item_str, "path_hex");
        let Some(path) = decode_os_path(&path_str, path_hex.as_deref()) else {
            continue;
        };
        let display_name = extract_string(item_str, "display_name")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::document::sanitize_basename(&path));
        let last_opened_epoch_secs = extract_u64(item_str, "last_opened_epoch_secs").unwrap_or(0);
        let grant_active = extract_bool(item_str, "grant_active").unwrap_or(true);
        entries.push(RecentEntry {
            kind,
            path,
            display_name,
            last_opened_epoch_secs,
            grant_active,
        });
        if entries.len() >= MAX_RECENT_ITEMS {
            break;
        }
    }
    entries
}

fn parse_json(json: &str) -> RecentsState {
    let files = extract_array_section(json, "files")
        .map(|arr| parse_entries(arr, RecentKind::File))
        .unwrap_or_default();
    let workspaces = extract_array_section(json, "workspaces")
        .map(|arr| parse_entries(arr, RecentKind::Workspace))
        .unwrap_or_default();
    RecentsState { files, workspaces }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "markdown-recents-{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recents_persist_dedup_cap_and_non_utf8_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = temp_dir("persist");
        let store_path = dir.join("recents.json");
        unsafe {
            std::env::set_var("MARKDOWN_RECENTS_PATH", &store_path);
            std::env::remove_var("MARKDOWN_RECENTS_IN_MEMORY");
        }
        clear();

        let non_utf8 = dir.join(OsString::from_vec(b"notes-\xff.md".to_vec()));
        std::fs::write(&non_utf8, b"# Hello\n").unwrap();
        record_file(&non_utf8, "notes-\u{fffd}.md");

        for i in 0..12 {
            let p = dir.join(format!("doc-{i}.md"));
            record_file(&p, &format!("doc-{i}.md"));
        }
        // Re-record non_utf8 to move it back to the top.
        record_file(&non_utf8, "notes-\u{fffd}.md");

        let loaded = load();
        assert_eq!(loaded.files.len(), MAX_RECENT_ITEMS);
        assert_eq!(loaded.files[0].path, non_utf8);
        assert_eq!(
            check_availability(&loaded.files[0]),
            RecentStatus::Available
        );

        unsafe {
            std::env::remove_var("MARKDOWN_RECENTS_PATH");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn workspace_availability_revocation_and_relocation() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = temp_dir("recovery");
        let store_path = dir.join("recents.json");
        unsafe {
            std::env::set_var("MARKDOWN_RECENTS_PATH", &store_path);
            std::env::remove_var("MARKDOWN_RECENTS_IN_MEMORY");
        }
        clear();

        let ws1 = dir.join("ws-one");
        let ws2 = dir.join("ws-moved");
        std::fs::create_dir(&ws1).unwrap();
        record_workspace(&ws1, "ws-one");

        // 1. Available initially
        let state = load();
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(
            check_availability(&state.workspaces[0]),
            RecentStatus::Available
        );

        // 2. Permission denied
        std::fs::set_permissions(&ws1, std::fs::Permissions::from_mode(0o0)).unwrap();
        if std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout
            != b"0\n"
        {
            assert_eq!(
                check_availability(&load().workspaces[0]),
                RecentStatus::PermissionDenied
            );
        }
        std::fs::set_permissions(&ws1, std::fs::Permissions::from_mode(0o755)).unwrap();

        // 3. Revoked grant
        revoke_grant(RecentKind::Workspace, &ws1);
        assert_eq!(
            check_availability(&load().workspaces[0]),
            RecentStatus::AccessRevoked
        );

        // 4. Moved / missing folder
        std::fs::rename(&ws1, &ws2).unwrap();
        record_workspace(&ws1, "ws-one"); // re-enable grant_active while missing
        assert_eq!(
            check_availability(&load().workspaces[0]),
            RecentStatus::MissingOrMoved
        );

        // 5. Relocate to ws2
        assert!(relocate_entry(
            RecentKind::Workspace,
            &ws1,
            &ws2,
            "ws-moved"
        ));
        let state = load();
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].path, ws2);
        assert!(state.workspaces[0].grant_active);
        assert_eq!(
            check_availability(&state.workspaces[0]),
            RecentStatus::Available
        );

        unsafe {
            std::env::remove_var("MARKDOWN_RECENTS_PATH");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
