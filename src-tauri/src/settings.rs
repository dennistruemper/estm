//! App preferences (`settings.json` next to the SQLite DB).

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// User-defined global picker shortcut (modifiers + one letter).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerHotkey {
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
    pub ctrl: bool,
    pub key: String,
}

impl Default for PickerHotkey {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                shift: true,
                alt: false,
                cmd: true,
                ctrl: false,
                key: "E".into(),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                shift: true,
                alt: false,
                cmd: false,
                ctrl: true,
                key: "E".into(),
            }
        }
    }
}

impl PickerHotkey {
    fn normalize_key(&self) -> Result<String, String> {
        let trimmed = self.key.trim();
        let ch = trimmed
            .chars()
            .next()
            .ok_or_else(|| "shortcut key must be a single letter".to_string())?;
        if !ch.is_ascii_alphabetic() {
            return Err("shortcut key must be A–Z".to_string());
        }
        if trimmed.len() > ch.len_utf8() {
            return Err("shortcut key must be a single letter".to_string());
        }
        Ok(ch.to_ascii_uppercase().to_string())
    }

    /// Build a string for `global-hotkey` / the Tauri plugin (`Control+Shift+KeyE`).
    pub fn to_shortcut_string(&self) -> Result<String, String> {
        if !self.shift && !self.alt && !self.cmd && !self.ctrl {
            return Err("enable at least one modifier (Shift, Alt, Cmd, or Ctrl)".to_string());
        }

        let key = self.normalize_key()?;
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Control");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.cmd {
            parts.push("Command");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(key.as_str());
        Ok(parts.join("+"))
    }

    /// Human-readable label for hints (platform-aware symbols on macOS).
    pub fn display_label(&self) -> String {
        let key = self
            .normalize_key()
            .unwrap_or_else(|_| self.key.to_ascii_uppercase());

        #[cfg(target_os = "macos")]
        {
            let mut s = String::new();
            if self.ctrl {
                s.push('⌃');
            }
            if self.alt {
                s.push('⌥');
            }
            if self.cmd {
                s.push('⌘');
            }
            if self.shift {
                s.push('⇧');
            }
            s.push_str(&key);
            s
        }

        #[cfg(not(target_os = "macos"))]
        {
            let mut parts = Vec::new();
            if self.ctrl {
                parts.push("Ctrl");
            }
            if self.alt {
                parts.push("Alt");
            }
            if self.cmd {
                parts.push("Win");
            }
            if self.shift {
                parts.push("Shift");
            }
            parts.push(key.as_str());
            parts.join("+")
        }
    }

    fn from_legacy_preset(id: &str) -> Self {
        match id {
            "hyper" => {
                #[cfg(target_os = "macos")]
                {
                    Self {
                        shift: false,
                        alt: true,
                        cmd: true,
                        ctrl: true,
                        key: "E".into(),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Self {
                        shift: true,
                        alt: true,
                        cmd: false,
                        ctrl: true,
                        key: "E".into(),
                    }
                }
            }
            _ => Self::default(),
        }
    }
}

/// Optional filter prefix + digit quick-pick (e.g. `.` then `3` for row 3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickPickPrefix {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_quick_pick_prefix_char")]
    pub prefix: String,
}

fn default_quick_pick_prefix_char() -> String {
    ".".into()
}

impl Default for QuickPickPrefix {
    fn default() -> Self {
        Self {
            enabled: false,
            prefix: default_quick_pick_prefix_char(),
        }
    }
}

impl QuickPickPrefix {
    pub fn normalize_prefix(raw: &str) -> Result<String, String> {
        let t = raw.trim();
        if t.is_empty() {
            return Err("prefix must not be empty".to_string());
        }
        if t.len() > 8 {
            return Err("prefix must be at most 8 characters".to_string());
        }
        if t.chars().any(|c| c.is_ascii_digit()) {
            return Err("prefix cannot contain digits 0–9".to_string());
        }
        Ok(t.to_string())
    }
}

/// How many clips to keep and optional age eviction (unpinned clips only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryLimits {
    #[serde(default = "default_max_entries")]
    pub max_entries: u32,
    #[serde(default = "default_use_max_age")]
    pub use_max_age: bool,
    #[serde(default = "default_max_age_days")]
    pub max_age_days: u32,
}

fn default_max_entries() -> u32 {
    500
}

fn default_use_max_age() -> bool {
    true
}

fn default_max_age_days() -> u32 {
    14
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_entries: default_max_entries(),
            use_max_age: default_use_max_age(),
            max_age_days: default_max_age_days(),
        }
    }
}

impl HistoryLimits {
    pub fn normalize(
        max_entries: u32,
        use_max_age: bool,
        max_age_days: u32,
    ) -> Result<Self, String> {
        if !(1..=50_000).contains(&max_entries) {
            return Err("max entries must be between 1 and 50,000".to_string());
        }
        if use_max_age && !(1..=3650).contains(&max_age_days) {
            return Err("max age must be between 1 and 3,650 days".to_string());
        }
        Ok(Self {
            max_entries,
            use_max_age,
            max_age_days,
        })
    }

    pub fn to_db_limits(&self) -> crate::db::HistoryLimits {
        let max_age_ms = if self.use_max_age {
            Some(self.max_age_days as i64 * 24 * 60 * 60 * 1000)
        } else {
            None
        };
        crate::db::HistoryLimits {
            max_rows: self.max_entries as i64,
            max_age_ms,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub picker_hotkey: PickerHotkey,
    #[serde(default)]
    pub quick_pick_prefix: QuickPickPrefix,
    #[serde(default)]
    pub history_limits: HistoryLimits,
}

pub fn settings_path() -> PathBuf {
    directories::ProjectDirs::from("fun", "estm", "ESTM")
        .expect("unable to resolve application storage directory")
        .data_dir()
        .join("settings.json")
}

fn migrate_picker_hotkey(value: &serde_json::Value) -> PickerHotkey {
    match value {
        serde_json::Value::String(preset) => PickerHotkey::from_legacy_preset(preset),
        serde_json::Value::Object(_) => serde_json::from_value(value.clone())
            .unwrap_or_default(),
        _ => PickerHotkey::default(),
    }
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return Settings::default();
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Settings::default();
    };

    let Some(obj) = value.as_object() else {
        return Settings::default();
    };

    let picker_hotkey = obj
        .get("pickerHotkey")
        .map(migrate_picker_hotkey)
        .unwrap_or_default();

    let mut quick_pick_prefix: QuickPickPrefix = obj
        .get("quickPickPrefix")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    if let Ok(p) = QuickPickPrefix::normalize_prefix(&quick_pick_prefix.prefix) {
        quick_pick_prefix.prefix = p;
    } else {
        quick_pick_prefix.prefix = default_quick_pick_prefix_char();
    }

    let mut history_limits: HistoryLimits = obj
        .get("historyLimits")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    if let Ok(h) = HistoryLimits::normalize(
        history_limits.max_entries,
        history_limits.use_max_age,
        history_limits.max_age_days,
    ) {
        history_limits = h;
    } else {
        history_limits = HistoryLimits::default();
    }

    Settings {
        picker_hotkey,
        quick_pick_prefix,
        history_limits,
    }
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Tracks the OS-global shortcut string currently registered (for unregister on change).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct PickerHotkeyState(pub Mutex<Option<String>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub picker_hotkey: PickerHotkey,
    pub active_label: String,
    pub quick_pick_prefix_enabled: bool,
    pub quick_pick_prefix: String,
    pub history_max_entries: u32,
    pub history_use_max_age: bool,
    pub history_max_age_days: u32,
}

pub fn snapshot() -> SettingsSnapshot {
    let cfg = load();
    SettingsSnapshot {
        active_label: cfg.picker_hotkey.display_label(),
        picker_hotkey: cfg.picker_hotkey,
        quick_pick_prefix_enabled: cfg.quick_pick_prefix.enabled,
        quick_pick_prefix: cfg.quick_pick_prefix.prefix,
        history_max_entries: cfg.history_limits.max_entries,
        history_use_max_age: cfg.history_limits.use_max_age,
        history_max_age_days: cfg.history_limits.max_age_days,
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn apply_picker_hotkey<R: Runtime>(
    app: &AppHandle<R>,
    hotkey: &PickerHotkey,
) -> Result<(), String> {
    let new = hotkey.to_shortcut_string()?;
    let gs = app.global_shortcut();

    let state = app.state::<PickerHotkeyState>();
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "hotkey state lock poisoned".to_string())?;

    if let Some(ref old) = *guard {
        let _ = gs.unregister(old.as_str());
    }

    gs.register(new.as_str())
        .map_err(|e| format!("register picker hotkey ({new}): {e}"))?;

    *guard = Some(new);
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn install_picker_hotkey_handler<R: Runtime>(
    app: &AppHandle<R>,
    toggle: impl Fn(&AppHandle<R>) + Send + Sync + 'static,
) -> Result<(), String> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new().with_handler(move |app_h, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle(app_h);
            }
        })
        .build(),
    )
    .map_err(|e| e.to_string())?;

    app.manage(PickerHotkeyState(Mutex::new(None)));

    let hotkey = load().picker_hotkey;
    apply_picker_hotkey(app, &hotkey)
}
