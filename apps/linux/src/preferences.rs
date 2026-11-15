//! Persistent preferences for theme, text scale, and reader/startup settings (LNX-001F, LNX-002D).

use std::path::PathBuf;
use std::sync::Mutex;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;
pub const SCALE_STEPS: [f64; 9] = [0.75, 0.85, 1.0, 1.15, 1.30, 1.50, 1.75, 2.0, 2.5];
pub const DEFAULT_SCALE_INDEX: usize = 2; // 1.0 (100%)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationStatus {
    Current,
    Migrated { from_version: u32 },
    FutureVersion { version: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemePreference::System => "system",
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ThemePreference::System => "System",
            ThemePreference::Light => "Light",
            ThemePreference::Dark => "Dark",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "light" => ThemePreference::Light,
            "dark" => ThemePreference::Dark,
            _ => ThemePreference::System,
        }
    }
}

impl std::str::FromStr for ThemePreference {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str(s))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub schema_version: u32,
    pub theme: ThemePreference,
    pub text_scale: f64,
    pub show_outline_by_default: bool,
    pub restore_session_on_launch: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemePreference::System,
            text_scale: SCALE_STEPS[DEFAULT_SCALE_INDEX],
            show_outline_by_default: true,
            restore_session_on_launch: true,
        }
    }
}

static IN_MEMORY: Mutex<Option<Preferences>> = Mutex::new(None);

fn config_path() -> Option<PathBuf> {
    if is_memory_mode() {
        return None;
    }
    if let Some(p) = std::env::var_os("MARKDOWN_PREFERENCES_PATH") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("markdown").join("preferences.json"))
}

pub fn is_memory_mode() -> bool {
    if std::env::var_os("MARKDOWN_PREFERENCES_IN_MEMORY").is_some() {
        return true;
    }
    if std::env::var_os("MARKDOWN_PREFERENCES_PATH").is_some() {
        return false;
    }
    std::env::var("GSETTINGS_BACKEND").as_deref() == Ok("memory")
}

pub fn load() -> Preferences {
    load_with_status().0
}

pub fn load_with_status() -> (Preferences, MigrationStatus) {
    if is_memory_mode() {
        let mem = IN_MEMORY.lock().unwrap();
        if let Some(prefs) = mem.as_ref() {
            return (prefs.clone(), MigrationStatus::Current);
        }
        return (Preferences::default(), MigrationStatus::Current);
    }

    let Some(path) = config_path() else {
        return (Preferences::default(), MigrationStatus::Current);
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return (Preferences::default(), MigrationStatus::Current);
    };

    let (prefs, status) = parse_preferences_with_status(&data);
    if matches!(status, MigrationStatus::Migrated { .. }) {
        save(&prefs);
    }
    (prefs, status)
}

pub fn save(prefs: &Preferences) {
    let mut normalized = prefs.clone();
    normalized.schema_version = CURRENT_SCHEMA_VERSION;
    normalized.text_scale = clamped_scale(normalized.text_scale);

    if is_memory_mode() {
        let mut mem = IN_MEMORY.lock().unwrap();
        *mem = Some(normalized);
        return;
    }

    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = format!(
        "{{\n  \"schema_version\": {},\n  \"theme\": \"{}\",\n  \"text_scale\": {:.2},\n  \"show_outline_by_default\": {},\n  \"restore_session_on_launch\": {}\n}}\n",
        CURRENT_SCHEMA_VERSION,
        normalized.theme.as_str(),
        normalized.text_scale,
        normalized.show_outline_by_default,
        normalized.restore_session_on_launch,
    );
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if std::fs::write(&tmp, payload).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn parse_preferences(json: &str) -> Preferences {
    parse_preferences_with_status(json).0
}

pub fn parse_preferences_with_status(json: &str) -> (Preferences, MigrationStatus) {
    let mut prefs = Preferences::default();
    let mut parsed_version: Option<u32> = None;
    let mut saw_legacy_field = false;

    for part in json.split([',', '{', '}', '\n']) {
        let part = part.trim();
        if let Some(idx) = part.find("\"schema_version\":") {
            let rest = &part[idx + "\"schema_version\":".len()..];
            let val = rest.trim().trim_matches(|c| c == '"' || c == ' ');
            if let Ok(ver) = val.parse::<u32>() {
                parsed_version = Some(ver);
            }
        } else if let Some(idx) = part.find("\"theme\":") {
            let rest = &part[idx + "\"theme\":".len()..];
            let val = rest.trim().trim_matches(|c| c == '"' || c == ' ');
            prefs.theme = ThemePreference::from_str(val);
            saw_legacy_field = true;
        } else if let Some(idx) = part.find("\"text_scale\":") {
            let rest = &part[idx + "\"text_scale\":".len()..];
            let val = rest.trim().trim_matches(|c| c == '"' || c == ' ');
            if let Ok(scale) = val.parse::<f64>() {
                prefs.text_scale = clamped_scale(scale);
            }
            saw_legacy_field = true;
        } else if let Some(idx) = part.find("\"show_outline_by_default\":") {
            let rest = &part[idx + "\"show_outline_by_default\":".len()..];
            let val = rest.trim().trim_matches(|c| c == '"' || c == ' ');
            if let Ok(b) = val.parse::<bool>() {
                prefs.show_outline_by_default = b;
            }
        } else if let Some(idx) = part.find("\"restore_session_on_launch\":") {
            let rest = &part[idx + "\"restore_session_on_launch\":".len()..];
            let val = rest.trim().trim_matches(|c| c == '"' || c == ' ');
            if let Ok(b) = val.parse::<bool>() {
                prefs.restore_session_on_launch = b;
            }
        }
    }

    let status = match parsed_version {
        Some(v) if v == CURRENT_SCHEMA_VERSION => MigrationStatus::Current,
        Some(v) if v < CURRENT_SCHEMA_VERSION => MigrationStatus::Migrated { from_version: v },
        Some(v) => MigrationStatus::FutureVersion { version: v },
        None if saw_legacy_field => MigrationStatus::Migrated { from_version: 1 },
        None => MigrationStatus::Current,
    };
    prefs.schema_version = CURRENT_SCHEMA_VERSION;
    (prefs, status)
}

pub fn clamped_scale(scale: f64) -> f64 {
    if scale.is_nan() || scale.is_infinite() {
        return SCALE_STEPS[DEFAULT_SCALE_INDEX];
    }
    if scale <= SCALE_STEPS[0] {
        return SCALE_STEPS[0];
    }
    if scale >= SCALE_STEPS[SCALE_STEPS.len() - 1] {
        return SCALE_STEPS[SCALE_STEPS.len() - 1];
    }
    let idx = closest_scale_index(scale);
    SCALE_STEPS[idx]
}

pub fn closest_scale_index(scale: f64) -> usize {
    if scale.is_nan() || scale.is_infinite() {
        return DEFAULT_SCALE_INDEX;
    }
    SCALE_STEPS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (scale - **a)
                .abs()
                .partial_cmp(&(scale - **b).abs())
                .unwrap()
        })
        .map(|(i, _)| i)
        .unwrap_or(DEFAULT_SCALE_INDEX)
}

pub fn larger_scale(current: f64) -> Option<f64> {
    let i = closest_scale_index(current);
    if i + 1 < SCALE_STEPS.len() {
        Some(SCALE_STEPS[i + 1])
    } else {
        None
    }
}

pub fn smaller_scale(current: f64) -> Option<f64> {
    let i = closest_scale_index(current);
    if i > 0 {
        Some(SCALE_STEPS[i - 1])
    } else {
        None
    }
}

pub fn next_scale(current: f64) -> f64 {
    larger_scale(current).unwrap_or(SCALE_STEPS[SCALE_STEPS.len() - 1])
}

pub fn prev_scale(current: f64) -> f64 {
    smaller_scale(current).unwrap_or(SCALE_STEPS[0])
}

pub fn reset_scale() -> f64 {
    SCALE_STEPS[DEFAULT_SCALE_INDEX]
}
