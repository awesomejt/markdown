//! Persistent window state across application relaunches (LNX-001H).
//!
//! Stores document file paths, reading positions as base64 core anchors,
//! text scaling, and outline visibility in `$XDG_CONFIG_HOME/markdown/restoration.json`
//! (or `MARKDOWN_RESTORATION_PATH`).

use std::path::PathBuf;
use std::sync::Mutex;

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(BASE64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.is_empty() {
        return Some(Vec::new());
    }
    let mut decode_table = [0xFFu8; 256];
    for (i, &c) in BASE64_CHARS.iter().enumerate() {
        decode_table[c as usize] = i as u8;
    }

    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;

    for &b in bytes {
        if b == b'=' {
            break;
        }
        if b.is_ascii_whitespace() {
            continue;
        }
        let val = decode_table[b as usize];
        if val == 0xFF {
            return None;
        }
        buffer = (buffer << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedPosition {
    pub top: Vec<u8>,
    pub selection_start: Vec<u8>,
    pub selection_end: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowRestorationState {
    pub file_path: String,
    pub top_anchor_base64: String,
    pub selection_start_anchor_base64: Option<String>,
    pub selection_end_anchor_base64: Option<String>,
    pub text_scale: f64,
    pub outline_shown: bool,
}

impl WindowRestorationState {
    pub fn to_saved_position(&self) -> Option<SavedPosition> {
        let top = base64_decode(&self.top_anchor_base64)?;
        if top.is_empty() {
            return None;
        }
        let start = match &self.selection_start_anchor_base64 {
            Some(s) => base64_decode(s).unwrap_or_else(|| top.clone()),
            None => top.clone(),
        };
        let end = self
            .selection_end_anchor_base64
            .as_deref()
            .and_then(base64_decode);
        Some(SavedPosition {
            top,
            selection_start: start,
            selection_end: end,
        })
    }

    pub fn from_saved_position(
        file_path: String,
        pos: Option<&SavedPosition>,
        text_scale: f64,
        outline_shown: bool,
    ) -> Self {
        Self {
            file_path,
            top_anchor_base64: pos.map(|p| base64_encode(&p.top)).unwrap_or_default(),
            selection_start_anchor_base64: pos.map(|p| base64_encode(&p.selection_start)),
            selection_end_anchor_base64: pos
                .and_then(|p| p.selection_end.as_deref().map(base64_encode)),
            text_scale,
            outline_shown,
        }
    }
}

static IN_MEMORY: Mutex<Option<Vec<WindowRestorationState>>> = Mutex::new(None);

pub fn default_store_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MARKDOWN_RESTORATION_PATH") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("markdown").join("restoration.json"))
}

pub fn is_memory_mode() -> bool {
    std::env::var_os("MARKDOWN_RESTORATION_IN_MEMORY").is_some()
}

pub fn save(states: &[WindowRestorationState]) {
    if is_memory_mode() {
        let mut mem = IN_MEMORY.lock().unwrap();
        *mem = Some(states.to_vec());
        return;
    }
    let Some(path) = default_store_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = format_json(states);
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn load() -> Vec<WindowRestorationState> {
    if is_memory_mode() {
        let mem = IN_MEMORY.lock().unwrap();
        return mem.as_ref().cloned().unwrap_or_default();
    }
    let Some(path) = default_store_path() else {
        return Vec::new();
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_json(&data)
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

fn format_json(states: &[WindowRestorationState]) -> String {
    let mut out = String::from("{\n  \"version\": 1,\n  \"windows\": [\n");
    for (i, w) in states.iter().enumerate() {
        let comma = if i + 1 < states.len() { "," } else { "" };
        let sel_start = match &w.selection_start_anchor_base64 {
            Some(s) => format!("\"{}\"", escape_json(s)),
            None => "null".to_string(),
        };
        let sel_end = match &w.selection_end_anchor_base64 {
            Some(s) => format!("\"{}\"", escape_json(s)),
            None => "null".to_string(),
        };
        out.push_str(&format!(
            "    {{\n      \"file_path\": \"{}\",\n      \"top_anchor_base64\": \"{}\",\n      \"selection_start_anchor_base64\": {},\n      \"selection_end_anchor_base64\": {},\n      \"text_scale\": {},\n      \"outline_shown\": {}\n    }}{}\n",
            escape_json(&w.file_path),
            escape_json(&w.top_anchor_base64),
            sel_start,
            sel_end,
            w.text_scale,
            w.outline_shown,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    out
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
            c => out.push(c),
        }
    }
    out
}

fn parse_json(json: &str) -> Vec<WindowRestorationState> {
    let mut states = Vec::new();
    let Some(windows_idx) = json.find("\"windows\"") else {
        return states;
    };
    let rest = &json[windows_idx..];
    let Some(arr_start) = rest.find('[') else {
        return states;
    };
    let Some(arr_end) = rest.rfind(']') else {
        return states;
    };
    let arr_str = &rest[arr_start + 1..arr_end];

    for block in arr_str.split('{') {
        let Some(block_end) = block.find('}') else {
            continue;
        };
        let item_str = &block[..block_end];
        let file_path = extract_string(item_str, "file_path").unwrap_or_default();
        if file_path.is_empty() {
            continue;
        }
        let top_anchor_base64 = extract_string(item_str, "top_anchor_base64").unwrap_or_default();
        let selection_start_anchor_base64 =
            extract_string(item_str, "selection_start_anchor_base64");
        let selection_end_anchor_base64 = extract_string(item_str, "selection_end_anchor_base64");
        let text_scale = extract_number(item_str, "text_scale").unwrap_or(1.0);
        let outline_shown = extract_bool(item_str, "outline_shown").unwrap_or(true);

        states.push(WindowRestorationState {
            file_path,
            top_anchor_base64,
            selection_start_anchor_base64,
            selection_end_anchor_base64,
            text_scale,
            outline_shown,
        });
    }

    states
}

fn extract_string(block: &str, key: &str) -> Option<String> {
    let key_pat = format!("\"{key}\"");
    let key_pos = block.find(&key_pat)?;
    let after_key = &block[key_pos + key_pat.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    if after_colon.starts_with("null") {
        return None;
    }
    if !after_colon.starts_with('"') {
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
                    other => s.push(other),
                }
            }
        } else {
            s.push(c);
        }
    }
    None
}

fn extract_number(block: &str, key: &str) -> Option<f64> {
    let key_pat = format!("\"{key}\"");
    let key_pos = block.find(&key_pat)?;
    let after_key = &block[key_pos + key_pat.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();
    let num_str: String = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    num_str.parse().ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        let cases: &[&[u8]] = &[
            b"",
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            b"\x00\x01\x02\x03\xFF\xFE\xFD",
        ];
        for &data in cases {
            let enc = base64_encode(data);
            let dec = base64_decode(&enc).unwrap();
            assert_eq!(dec, data);
        }
    }

    #[test]
    fn restoration_json_roundtrip() {
        let states = vec![
            WindowRestorationState {
                file_path: "/tmp/test.md".to_string(),
                top_anchor_base64: base64_encode(b"top-anchor"),
                selection_start_anchor_base64: Some(base64_encode(b"start-anchor")),
                selection_end_anchor_base64: Some(base64_encode(b"end-anchor")),
                text_scale: 1.15,
                outline_shown: true,
            },
            WindowRestorationState {
                file_path: "/tmp/other.md".to_string(),
                top_anchor_base64: base64_encode(b"top2"),
                selection_start_anchor_base64: None,
                selection_end_anchor_base64: None,
                text_scale: 0.85,
                outline_shown: false,
            },
        ];
        let json = format_json(&states);
        let parsed = parse_json(&json);
        assert_eq!(states.len(), parsed.len());
        assert_eq!(states[0], parsed[0]);
        assert_eq!(states[1], parsed[1]);
    }
}
