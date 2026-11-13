//! Persistent preferences for theme and text scale (LNX-001F).

use std::path::PathBuf;
use std::sync::Mutex;

pub const SCALE_STEPS: [f64; 9] = [0.75, 0.85, 1.0, 1.15, 1.30, 1.50, 1.75, 2.0, 2.5];
pub const DEFAULT_SCALE_INDEX: usize = 2; // 1.0 (100%)

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
    pub theme: ThemePreference,
    pub text_scale: f64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            text_scale: SCALE_STEPS[DEFAULT_SCALE_INDEX],
        }
    }
}

static IN_MEMORY: Mutex<Option<Preferences>> = Mutex::new(None);

fn config_path() -> Option<PathBuf> {
    if is_memory_mode() {
        return None;
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("markdown").join("preferences.json"))
}

pub fn is_memory_mode() -> bool {
    std::env::var_os("MARKDOWN_PREFERENCES_IN_MEMORY").is_some()
        || std::env::var("GSETTINGS_BACKEND").as_deref() == Ok("memory")
}

pub fn load() -> Preferences {
    if is_memory_mode() {
        let mem = IN_MEMORY.lock().unwrap();
        if let Some(prefs) = mem.as_ref() {
            return prefs.clone();
        }
        return Preferences::default();
    }

    let Some(path) = config_path() else {
        return Preferences::default();
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return Preferences::default();
    };

    parse_preferences(&data)
}

pub fn save(prefs: &Preferences) {
    if is_memory_mode() {
        let mut mem = IN_MEMORY.lock().unwrap();
        *mem = Some(prefs.clone());
        return;
    }

    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = format!(
        "{{\n  \"theme\": \"{}\",\n  \"text_scale\": {:.2}\n}}\n",
        prefs.theme.as_str(),
        prefs.text_scale
    );
    let _ = std::fs::write(&path, payload);
}

fn parse_preferences(json: &str) -> Preferences {
    let mut prefs = Preferences::default();
    for line in json.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("\"theme\":") {
            let val = rest
                .trim()
                .trim_matches(|c| c == '"' || c == ',' || c == ' ');
            prefs.theme = ThemePreference::from_str(val);
        } else if let Some(rest) = line.strip_prefix("\"text_scale\":") {
            let val = rest.trim().trim_matches(|c| c == ',' || c == ' ');
            if let Ok(scale) = val.parse::<f64>() {
                // Find closest allowed scale step
                if let Some(&closest) = SCALE_STEPS.iter().min_by(|a, b| {
                    (scale - **a)
                        .abs()
                        .partial_cmp(&(scale - **b).abs())
                        .unwrap()
                }) {
                    prefs.text_scale = closest;
                }
            }
        }
    }
    prefs
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
