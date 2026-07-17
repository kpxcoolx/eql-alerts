use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            x: 80.0,
            y: 60.0,
            width: 1360.0,
            height: 880.0,
        }
    }
}

/// Prefer a usable size when restoring a previously shrunk main window.
pub fn sanitize_main_geometry(geo: &WindowGeometry) -> WindowGeometry {
    let mut out = geo.clone();
    if out.width < 1200.0 {
        out.width = 1360.0;
    }
    if out.height < 780.0 {
        out.height = 880.0;
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub last_log_path: Option<String>,
    pub auto_monitor_on_start: bool,
    pub main_window: Option<WindowGeometry>,
    pub overlay_window: Option<WindowGeometry>,
    /// User dismissed or finished the first-run quick start.
    pub quick_start_dismissed: bool,
    /// One-shot: stripped classic timers for Legends-permanent buffs.
    pub eql_compat_permanent_v1: bool,
    /// One-shot: copied GINA TTS (`speak`) fields from the starter pack.
    pub eql_tts_v1: bool,
    /// One-shot: demote Combat/Danger/Fades/Social essentials to opt-in (were force-armed).
    pub eql_essentials_opt_in_v1: bool,
    /// One-shot: arm gameplay essentials (Core/Combat/Danger/Fades) and quiet spammy triggers.
    pub eql_essentials_gameplay_v2: bool,
    /// Active Kokoro voice id (e.g. bf_isabella, am_michael).
    pub voice_id: String,
    /// Legacy gender slot — kept for older settings.json files.
    pub voice_gender: String,
    /// Legacy female voice id.
    pub voice_female: String,
    /// Legacy male voice id.
    pub voice_male: String,
    /// Voice callout volume 0.0–1.0 (independent of system volume / chimes).
    pub voice_volume: f64,
    /// Master switch: when false, live alerts never speak (per-trigger TTS stays as-is).
    pub tts_enabled: bool,
    /// Preferred audio output device name (`""` = system default).
    pub audio_output_device: String,
    /// When a trigger has voice but no sound, also play this chime (`none` to skip).
    pub default_alert_sound: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_log_path: None,
            auto_monitor_on_start: true,
            main_window: None,
            overlay_window: None,
            quick_start_dismissed: false,
            eql_compat_permanent_v1: false,
            eql_tts_v1: false,
            eql_essentials_opt_in_v1: false,
            eql_essentials_gameplay_v2: false,
            voice_id: "bf_isabella".to_string(),
            voice_gender: "female".to_string(),
            voice_female: "bf_isabella".to_string(),
            voice_male: "am_michael".to_string(),
            voice_volume: 0.2,
            tts_enabled: true,
            audio_output_device: String::new(),
            default_alert_sound: "none".to_string(),
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("config dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

pub fn load_settings(app: &AppHandle) -> AppSettings {
    let Ok(path) = settings_path(app) else {
        return AppSettings::default();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return AppSettings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let text =
        serde_json::to_string_pretty(settings).map_err(|e| format!("serialize settings: {e}"))?;
    fs::write(path, text).map_err(|e| format!("write settings: {e}"))
}

pub fn remember_log_path(app: &AppHandle, path: &str) -> Result<(), String> {
    let mut settings = load_settings(app);
    settings.last_log_path = Some(path.to_string());
    save_settings(app, &settings)
}

pub fn remember_window(
    app: &AppHandle,
    label: &str,
    geometry: WindowGeometry,
) -> Result<AppSettings, String> {
    let mut settings = load_settings(app);
    match label {
        "main" => settings.main_window = Some(geometry),
        "overlay" => settings.overlay_window = Some(geometry),
        _ => return Err(format!("unknown window label: {label}")),
    }
    save_settings(app, &settings)?;
    Ok(settings)
}

pub fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("config dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    Ok(dir)
}
