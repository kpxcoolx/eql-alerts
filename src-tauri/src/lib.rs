mod app_log;
mod engine;
mod eql_compat;
mod gina_import;
mod kokoro;
mod log_find;
mod log_tail;
mod settings;
mod starter;
mod tts;

pub use tts::VoiceGender;

pub fn tts_speak_now(text: &str, gender: VoiceGender) -> Result<(), String> {
    let voice = match gender {
        VoiceGender::Male => "am_michael",
        VoiceGender::Female => "bf_isabella",
    };
    tts::speak_now(text, voice, 1.0)
}

pub fn tts_voice_female() -> VoiceGender {
    VoiceGender::Female
}

pub fn tts_voice_male() -> VoiceGender {
    VoiceGender::Male
}

use engine::{EngineState, Trigger, TriggerEngine, TriggerLibrary};
use gina_import::{import_gina_package, merge_libraries};
use log_find::{best_log, character_from_path, find_eq_logs, split_log_line, FoundLog};
use log_tail::TailHandle;
use parking_lot::Mutex;
use settings::{
    config_dir, load_settings, remember_log_path, remember_window, sanitize_main_geometry,
    save_settings, AppSettings, WindowGeometry,
};

fn active_voice_id(settings: &AppSettings) -> String {
    if !settings.voice_id.trim().is_empty() {
        return settings.voice_id.trim().to_string();
    }
    // Legacy settings.json without voice_id.
    if settings.voice_gender.eq_ignore_ascii_case("male") {
        if settings.voice_male.trim().is_empty() {
            "am_michael".into()
        } else {
            settings.voice_male.clone()
        }
    } else if settings.voice_female.trim().is_empty() {
        "bf_isabella".into()
    } else {
        settings.voice_female.clone()
    }
}

fn sync_voice_slots(settings: &mut AppSettings) {
    let id = active_voice_id(settings);
    settings.voice_id = id.clone();
    let male = id.starts_with("am_") || id.starts_with("bm_");
    if male {
        settings.voice_gender = "male".into();
        settings.voice_male = id;
    } else {
        settings.voice_gender = "female".into();
        settings.voice_female = id;
    }
}

fn apply_audio_output(settings: &AppSettings) {
    tts::set_preferred_output_device(Some(settings.audio_output_device.as_str()));
}
use eql_compat::strip_permanent_buff_timers;
use starter::{
    ensure_essentials, apply_gameplay_essentials_defaults, demote_optional_essentials,
    ensure_default_tts, ensure_eql_ability_timers, ensure_eql_disease_dot_timers,
    ensure_eql_mez_timers, ensure_shaman_warnings, is_placeholder_library, starter_pack,
    starter_stats,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

struct AppState {
    engine: Mutex<TriggerEngine>,
    tail: Mutex<Option<TailHandle>>,
    overlay_open: Mutex<bool>,
    overlay_click_through: Mutex<bool>,
}

#[derive(Clone, serde::Serialize)]
struct OverlayStatus {
    open: bool,
    click_through: bool,
    x: Option<f64>,
    y: Option<f64>,
}

fn triggers_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("triggers.json"))
}

fn load_library(app: &AppHandle) -> TriggerLibrary {
    let Ok(path) = triggers_path(app) else {
        return starter_pack();
    };

    if !path.exists() {
        let pack = starter_pack();
        let _ = save_library(app, &pack);
        mark_eql_compat_done(app);
        return pack;
    }

    let Ok(text) = fs::read_to_string(&path) else {
        return starter_pack();
    };
    let mut parsed: TriggerLibrary = serde_json::from_str(&text).unwrap_or_default();

    // Upgrade early builds that only had the tiny "General" demo set.
    if is_placeholder_library(&parsed) {
        let pack = starter_pack();
        let _ = save_library(app, &pack);
        mark_eql_compat_done(app);
        return pack;
    }

    // Existing installs: strip classic timers for Legends-permanent buffs once.
    let mut settings = load_settings(app);
    let mut dirty = false;
    if !settings.eql_compat_permanent_v1 {
        if strip_permanent_buff_timers(&mut parsed) > 0 {
            dirty = true;
        }
        settings.eql_compat_permanent_v1 = true;
    }
    if !settings.eql_tts_v1 {
        if ensure_default_tts(&mut parsed) > 0 {
            dirty = true;
        }
        settings.eql_tts_v1 = true;
    }
    // Fill in any missing essentials triggers; keep existing user edits.
    if ensure_essentials(&mut parsed) > 0 {
        dirty = true;
    }
    if ensure_default_tts(&mut parsed) > 0 {
        dirty = true;
    }
    if ensure_eql_ability_timers(&mut parsed) > 0 {
        dirty = true;
    }
    if ensure_shaman_warnings(&mut parsed) > 0 {
        dirty = true;
    }
    if ensure_eql_mez_timers(&mut parsed) > 0 {
        dirty = true;
    }
    if ensure_eql_disease_dot_timers(&mut parsed) > 0 {
        dirty = true;
    }
    // One-shot: Combat/Danger/Fades/Social were force-armed — demote to opt-in.
    if !settings.eql_essentials_opt_in_v1 {
        if demote_optional_essentials(&mut parsed) > 0 {
            dirty = true;
        }
        settings.eql_essentials_opt_in_v1 = true;
    }
    // One-shot: arm gameplay essentials; leave Social + spammy triggers opt-in.
    if !settings.eql_essentials_gameplay_v2 {
        if apply_gameplay_essentials_defaults(&mut parsed) > 0 {
            dirty = true;
        }
        settings.eql_essentials_gameplay_v2 = true;
    }
    if dirty {
        let _ = save_library(app, &parsed);
    }
    let _ = save_settings(app, &settings);

    parsed
}

fn mark_eql_compat_done(app: &AppHandle) {
    let mut settings = load_settings(app);
    settings.eql_compat_permanent_v1 = true;
    settings.eql_tts_v1 = true;
    settings.eql_essentials_opt_in_v1 = true;
    settings.eql_essentials_gameplay_v2 = true;
    let _ = save_settings(app, &settings);
}

fn save_library(app: &AppHandle, library: &TriggerLibrary) -> Result<(), String> {
    let path = triggers_path(app)?;
    // Compact JSON — much faster than pretty for ~800 triggers.
    let text = serde_json::to_string(library).map_err(|e| format!("serialize triggers: {e}"))?;
    fs::write(path, text).map_err(|e| format!("write triggers: {e}"))
}

fn save_library_async(app: &AppHandle, library: &TriggerLibrary) {
    let Ok(path) = triggers_path(app) else {
        return;
    };
    let Ok(text) = serde_json::to_string(library) else {
        return;
    };
    std::thread::spawn(move || {
        let _ = fs::write(path, text);
    });
}

#[derive(Clone, serde::Serialize)]
struct StarterInstallResult {
    library: TriggerLibrary,
    groups: usize,
    triggers: usize,
}

fn emit_state(state: &Arc<AppState>, app: &AppHandle) -> EngineState {
    let snapshot = {
        let mut engine = state.engine.lock();
        engine.prune_expired_timers();
        engine.snapshot()
    };
    let _ = app.emit("alerts-update", &snapshot);
    snapshot
}

fn overlay_position(app: &AppHandle) -> (Option<f64>, Option<f64>) {
    let Some(win) = app.get_webview_window("overlay") else {
        return (None, None);
    };
    match win.outer_position() {
        Ok(pos) => {
            let scale = win.scale_factor().unwrap_or(1.0);
            (Some(pos.x as f64 / scale), Some(pos.y as f64 / scale))
        }
        Err(_) => (None, None),
    }
}

fn status(app: &AppHandle, state: &AppState) -> OverlayStatus {
    let (x, y) = overlay_position(app);
    OverlayStatus {
        open: *state.overlay_open.lock(),
        click_through: *state.overlay_click_through.lock(),
        x,
        y,
    }
}

fn emit_overlay_status(app: &AppHandle, state: &AppState) {
    let _ = app.emit("overlay-status", status(app, state));
}

fn apply_click_through(app: &AppHandle, state: &AppState, enabled: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("overlay") {
        win.set_ignore_cursor_events(enabled)
            .map_err(|e| format!("click-through: {e}"))?;
    }
    *state.overlay_click_through.lock() = enabled;
    emit_overlay_status(app, state);
    Ok(())
}

fn stop_tail(state: &AppState) {
    if let Some(handle) = state.tail.lock().take() {
        handle.stop();
    }
}

fn start_monitoring_inner(
    path: String,
    from_start: bool,
    state: &Arc<AppState>,
    app: &AppHandle,
) -> Result<EngineState, String> {
    stop_tail(state);

    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!("Log file not found: {path}"));
    }

    let character = character_from_path(&path);
    {
        let mut engine = state.engine.lock();
        engine.set_log_path(Some(path.clone()));
        engine.set_character(character);
        engine.set_monitoring(true);
    }
    let _ = remember_log_path(app, &path);

    let app_handle = app.clone();
    let state_clone = Arc::clone(state);
    let handle = log_tail::start_tailing(path_buf, from_start, move |line| {
        let Some((_ts, action)) = split_log_line(&line) else {
            return;
        };
        let actions = state_clone.engine.lock().process_action(&action);
        if actions.is_empty() {
            return;
        }
        for action in &actions {
            let settings = load_settings(&app_handle);
            apply_audio_output(&settings);
            let voice_id = active_voice_id(&settings);
            let volume = settings.voice_volume;
            // Engine already chose speak (TTS on) vs sound (TTS off).
            // Do not auto-inject a chime on top of voice — that masked callouts.
            let sound = action.sound.clone();
            let _ = tts::play_alert(sound.as_deref(), action.speak.as_deref(), &voice_id, volume);
        }
        let _ = emit_state(&state_clone, &app_handle);
    })?;

    *state.tail.lock() = Some(handle);
    Ok(emit_state(state, app))
}

fn create_overlay_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(());
    }

    let settings = load_settings(app);
    let geo = settings.overlay_window.unwrap_or(WindowGeometry {
        x: 40.0,
        y: 80.0,
        width: 360.0,
        height: 420.0,
    });

    let url = WebviewUrl::App("overlay.html".into());
    WebviewWindowBuilder::new(app, "overlay", url)
        .title("EQL Alerts Overlay")
        .always_on_top(true)
        .decorations(false)
        .transparent(true)
        .resizable(true)
        .skip_taskbar(true)
        .visible(false)
        .inner_size(geo.width, geo.height)
        .position(geo.x, geo.y)
        .build()
        .map_err(|e| format!("overlay window: {e}"))?;

    Ok(())
}

fn persist_window_geometry(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    let Ok(pos) = win.outer_position() else {
        return;
    };
    let Ok(size) = win.outer_size() else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0);
    let geometry = WindowGeometry {
        x: pos.x as f64 / scale,
        y: pos.y as f64 / scale,
        width: size.width as f64 / scale,
        height: size.height as f64 / scale,
    };
    let _ = remember_window(app, label, geometry);
}

fn shutdown_app(app: &AppHandle) {
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        stop_tail(state.inner());
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
    app.exit(0);
}

#[tauri::command]
fn host_os() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
fn get_settings(app: AppHandle) -> AppSettings {
    let mut settings = load_settings(&app);
    settings.voice_id = active_voice_id(&settings);
    sync_voice_slots(&mut settings);
    apply_audio_output(&settings);
    settings
}

#[tauri::command]
fn save_app_settings(settings: AppSettings, app: AppHandle) -> Result<AppSettings, String> {
    let mut merged = load_settings(&app);
    let voice_id = if settings.voice_id.trim().is_empty() {
        active_voice_id(&settings)
    } else {
        settings.voice_id.trim().to_string()
    };
    merged.last_log_path = settings.last_log_path;
    merged.auto_monitor_on_start = settings.auto_monitor_on_start;
    merged.quick_start_dismissed = settings.quick_start_dismissed;
    merged.voice_id = voice_id;
    merged.voice_gender = settings.voice_gender;
    merged.voice_female = if settings.voice_female.trim().is_empty() {
        "bf_isabella".into()
    } else {
        settings.voice_female
    };
    merged.voice_male = if settings.voice_male.trim().is_empty() {
        "am_michael".into()
    } else {
        settings.voice_male
    };
    sync_voice_slots(&mut merged);
    merged.voice_volume = settings.voice_volume.clamp(0.0, 1.0);
    merged.audio_output_device = settings.audio_output_device.trim().to_string();
    merged.default_alert_sound = settings.default_alert_sound;
    if settings.main_window.is_some() {
        merged.main_window = settings.main_window;
    }
    if settings.overlay_window.is_some() {
        merged.overlay_window = settings.overlay_window;
    }
    save_settings(&app, &merged)?;
    apply_audio_output(&merged);
    Ok(merged)
}

#[tauri::command]
fn save_window_geometry(
    label: String,
    geometry: WindowGeometry,
    app: AppHandle,
) -> Result<AppSettings, String> {
    remember_window(&app, &label, geometry)
}

#[tauri::command]
fn find_logs() -> Vec<FoundLog> {
    find_eq_logs()
}

#[tauri::command]
fn auto_detect_log() -> Result<FoundLog, String> {
    best_log().ok_or_else(|| {
        if cfg!(target_os = "macos") {
            "No eqlog_*.txt found. On Mac with Parallels, keep the Windows VM running so C: is mounted under /Volumes, or choose a log manually.".to_string()
        } else {
            "No eqlog_*.txt found. Check EverQuest Legends\\Logs, or choose a log manually.".to_string()
        }
    })
}

#[tauri::command]
fn get_engine_state(state: State<'_, Arc<AppState>>) -> EngineState {
    let mut engine = state.engine.lock();
    engine.prune_expired_timers();
    engine.snapshot()
}

#[tauri::command]
fn get_triggers(state: State<'_, Arc<AppState>>) -> TriggerLibrary {
    state.engine.lock().library().clone()
}

#[tauri::command]
fn set_groups_enabled(
    ids: Vec<String>,
    enabled: bool,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    let mut engine = state.engine.lock();
    engine.set_groups_enabled(&ids, enabled);
    save_library_async(&app, engine.library());
    Ok(())
}

#[tauri::command]
fn save_triggers(
    library: TriggerLibrary,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<TriggerLibrary, String> {
    save_library(&app, &library)?;
    state.engine.lock().set_library(library.clone());
    let _ = emit_state(&state, &app);
    Ok(library)
}

#[derive(Clone, serde::Serialize)]
struct ImportResult {
    library: TriggerLibrary,
    groups: usize,
    triggers: usize,
}

#[tauri::command]
fn import_triggers_path(
    path: String,
    merge: bool,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<ImportResult, String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!("File not found: {path}"));
    }

    let ext = path_buf
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let imported = if ext == "json" {
        let text = fs::read_to_string(&path_buf).map_err(|e| format!("read json: {e}"))?;
        serde_json::from_str::<TriggerLibrary>(&text)
            .map_err(|e| format!("parse triggers json: {e}"))?
    } else if ext == "gtp" || ext == "xml" {
        import_gina_package(&path_buf)?
    } else {
        return Err("Unsupported file. Use a GINA .gtp / ShareData.xml or our .json pack.".into());
    };

    let groups = imported.groups.len();
    let triggers = imported.groups.iter().map(|g| g.triggers.len()).sum();

    let library = if merge {
        let current = state.engine.lock().library().clone();
        merge_libraries(&current, imported)
    } else {
        imported
    };

    save_library(&app, &library)?;
    state.engine.lock().set_library(library.clone());
    let _ = emit_state(&state, &app);
    Ok(ImportResult {
        library,
        groups,
        triggers,
    })
}

#[tauri::command]
fn install_starter_pack(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<StarterInstallResult, String> {
    let library = starter_pack();
    let (groups, triggers) = starter_stats(&library);
    save_library(&app, &library)?;
    state.engine.lock().set_library(library.clone());
    let _ = emit_state(&state, &app);
    Ok(StarterInstallResult {
        library,
        groups,
        triggers,
    })
}

#[tauri::command]
fn clear_timers(state: State<'_, Arc<AppState>>, app: AppHandle) -> EngineState {
    state.engine.lock().clear_timers();
    emit_state(&state, &app)
}

#[tauri::command]
fn clear_timer(timer_id: String, state: State<'_, Arc<AppState>>, app: AppHandle) -> EngineState {
    state.engine.lock().clear_timer(&timer_id);
    emit_state(&state, &app)
}

#[tauri::command]
fn clear_alerts(state: State<'_, Arc<AppState>>, app: AppHandle) -> EngineState {
    state.engine.lock().clear_alerts();
    emit_state(&state, &app)
}

#[tauri::command]
fn test_trigger(
    trigger: Trigger,
    sample_action: Option<String>,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<EngineState, String> {
    let sample = sample_action
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let action = {
        let mut engine = state.engine.lock();
        engine.test_fire(&trigger, sample)
    };

    let settings = load_settings(&app);
    apply_audio_output(&settings);
    let voice_id = active_voice_id(&settings);
    let volume = settings.voice_volume;
    let _ = tts::play_alert(action.sound.as_deref(), action.speak.as_deref(), &voice_id, volume);

    Ok(emit_state(&state, &app))
}

#[tauri::command]
fn speak_text(text: String, app: AppHandle) -> Result<(), String> {
    let settings = load_settings(&app);
    apply_audio_output(&settings);
    let voice = active_voice_id(&settings);
    tts::speak(&text, &voice, settings.voice_volume)
}

#[tauri::command]
fn list_alert_sounds() -> Vec<tts::AlertSoundInfo> {
    tts::alert_sounds()
}

#[tauri::command]
fn play_alert_sound(sound: String) -> Result<(), String> {
    tts::play_sound(&sound)
}

#[tauri::command]
fn test_speech(
    text: Option<String>,
    sound: Option<String>,
    app: AppHandle,
) -> Result<String, String> {
    let _ = sound; // chime is Preview; Test is voice-only so speech isn't masked
    let settings = load_settings(&app);
    apply_audio_output(&settings);
    let voice = active_voice_id(&settings);
    let line = text.unwrap_or_default().trim().to_string();
    let line = if line.is_empty() {
        "Alert test".to_string()
    } else {
        line
    };

    app_log::write(&format!(
        "test_speech: {line:?} voice={voice} vol={:.2}",
        settings.voice_volume
    ));
    match tts::speak_now(&line, &voice, settings.voice_volume) {
        Ok(()) => Ok(format!("Spoke with {voice}: “{line}”")),
        Err(err) => {
            app_log::write(&format!("test_speech FAILED: {err}"));
            Err(err)
        }
    }
}

#[tauri::command]
fn list_kokoro_voices() -> Vec<kokoro::KokoroVoice> {
    // Never ensure_daemon/extract here — that freezes the Windows UI for minutes on first run.
    match kokoro::list_voices_if_running() {
        Ok(v) if !v.is_empty() => v,
        _ => kokoro::fallback_voice_catalog(),
    }
}

#[derive(serde::Serialize)]
struct VoicePreview {
    message: String,
    /// Optional WAV for WebView playback (empty when played natively as AIFF).
    #[serde(default)]
    wav_base64: String,
}

#[tauri::command]
fn list_audio_output_devices() -> Vec<tts::AudioOutputDevice> {
    tts::list_output_devices()
}

#[tauri::command]
fn preview_kokoro_voice(
    voice_id: String,
    text: Option<String>,
    volume: Option<f64>,
    app: AppHandle,
) -> Result<VoicePreview, String> {
    let settings = load_settings(&app);
    apply_audio_output(&settings);
    let voice = if voice_id.trim().is_empty() {
        active_voice_id(&settings)
    } else {
        voice_id.trim().to_string()
    };
    let line = text.unwrap_or_default().trim().to_string();
    let line = if line.is_empty() {
        "Out of mana".to_string()
    } else {
        line
    };

    let vol = volume.unwrap_or(settings.voice_volume).clamp(0.0, 1.0);
    tts::speak_now(&line, &voice, vol)?;
    Ok(VoicePreview {
        message: format!("Preview {voice}: “{line}”"),
        wav_base64: String::new(),
    })
}

#[tauri::command]
fn kokoro_status() -> serde_json::Value {
    // Probe only — never extract or spawn from the UI invoke path.
    serde_json::json!({
        "installed": kokoro::is_available(),
        "daemon": kokoro::daemon_running(),
        "log_path": app_log::log_path().display().to_string(),
    })
}

#[tauri::command]
fn get_app_log_path() -> String {
    app_log::log_path().display().to_string()
}

#[tauri::command]
fn get_overlay_status(state: State<'_, Arc<AppState>>, app: AppHandle) -> OverlayStatus {
    status(&app, &state)
}

#[tauri::command]
fn start_monitoring(
    path: String,
    from_start: bool,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<EngineState, String> {
    start_monitoring_inner(path, from_start, &state, &app)
}

#[tauri::command]
fn stop_monitoring(state: State<'_, Arc<AppState>>, app: AppHandle) -> EngineState {
    stop_tail(&state);
    {
        let mut engine = state.engine.lock();
        engine.set_monitoring(false);
    }
    emit_state(&state, &app)
}

#[tauri::command]
fn open_overlay(state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<OverlayStatus, String> {
    create_overlay_window(&app)?;
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.show();
        let _ = win.set_always_on_top(true);
        let _ = win.set_ignore_cursor_events(false);
    }
    *state.overlay_open.lock() = true;
    *state.overlay_click_through.lock() = false;
    emit_overlay_status(&app, &state);
    Ok(status(&app, &state))
}

#[tauri::command]
fn close_overlay(state: State<'_, Arc<AppState>>, app: AppHandle) -> OverlayStatus {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
    *state.overlay_open.lock() = false;
    *state.overlay_click_through.lock() = false;
    emit_overlay_status(&app, &state);
    status(&app, &state)
}

#[tauri::command]
fn set_overlay_click_through(
    enabled: bool,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<OverlayStatus, String> {
    apply_click_through(&app, &state, enabled)?;
    Ok(status(&app, &state))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app_log::write(&format!(
        "startup v{} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    ));
    app_log::write(&format!("log file: {}", app_log::log_path().display()));

    let state = Arc::new(AppState {
        engine: Mutex::new(TriggerEngine::new(TriggerLibrary::default())),
        tail: Mutex::new(None),
        overlay_open: Mutex::new(false),
        overlay_click_through: Mutex::new(false),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state.clone())
        .on_window_event(|window, event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                let label = window.label().to_string();
                if label == "main" || label == "overlay" {
                    persist_window_geometry(window.app_handle(), &label);
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "overlay" {
                    // Keep the overlay process alive — hide instead of destroy.
                    api.prevent_close();
                    let _ = window.hide();
                    if let Some(app_state) = window.app_handle().try_state::<Arc<AppState>>() {
                        *app_state.overlay_open.lock() = false;
                        *app_state.overlay_click_through.lock() = false;
                        let _ = window.set_ignore_cursor_events(false);
                        emit_overlay_status(window.app_handle(), app_state.inner());
                    }
                    return;
                }
                if window.label() != "main" {
                    return;
                }
                api.prevent_close();
                shutdown_app(window.app_handle());
            }
            WindowEvent::Destroyed => {
                if window.label() == "main" {
                    shutdown_app(window.app_handle());
                    return;
                }
                if window.label() == "overlay" {
                    if let Some(app_state) = window.app_handle().try_state::<Arc<AppState>>() {
                        *app_state.overlay_open.lock() = false;
                        *app_state.overlay_click_through.lock() = false;
                        emit_overlay_status(window.app_handle(), app_state.inner());
                    }
                }
            }
            _ => {}
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let Some(app_state) = app.try_state::<Arc<AppState>>() else {
                        return;
                    };
                    match shortcut.key {
                        tauri_plugin_global_shortcut::Code::KeyU => {
                            let _ = apply_click_through(app, app_state.inner(), false);
                        }
                        tauri_plugin_global_shortcut::Code::KeyL => {
                            let _ = apply_click_through(app, app_state.inner(), true);
                        }
                        _ => {}
                    }
                })
                .build(),
        )
        .setup(|app| {
            use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

            let handle = app.handle().clone();
            let library = load_library(&handle);
            if let Some(app_state) = app.try_state::<Arc<AppState>>() {
                app_state.engine.lock().set_library(library);
            }

            let settings = load_settings(&handle);
            apply_audio_output(&settings);
            tts::set_app_handle(handle.clone());

            if let Some(geo) = &settings.main_window {
                if let Some(win) = app.get_webview_window("main") {
                    let geo = sanitize_main_geometry(geo);
                    let _ = win.set_size(LogicalSize::new(geo.width, geo.height));
                    let _ = win.set_position(LogicalPosition::new(geo.x, geo.y));
                }
            }

            #[cfg(target_os = "macos")]
            let mods = Modifiers::SUPER | Modifiers::SHIFT;
            #[cfg(not(target_os = "macos"))]
            let mods = Modifiers::CONTROL | Modifiers::SHIFT;

            app.global_shortcut()
                .register(Shortcut::new(Some(mods), Code::KeyU))
                .map_err(|e| e.to_string())?;
            app.global_shortcut()
                .register(Shortcut::new(Some(mods), Code::KeyL))
                .map_err(|e| e.to_string())?;

            if let Err(err) = create_overlay_window(app.handle()) {
                app_log::write(&format!("overlay pre-create failed: {err}"));
            }

            if settings.auto_monitor_on_start {
                let path = settings
                    .last_log_path
                    .clone()
                    .or_else(|| best_log().map(|l| l.path));
                if let Some(path) = path {
                    if let Some(app_state) = app.try_state::<Arc<AppState>>() {
                        let _ = start_monitoring_inner(path, false, app_state.inner(), &handle);
                    }
                }
            }

            // Local TTS probe (dev only): curl 'http://127.0.0.1:17422/speak?text=Out%20of%20mana'
            #[cfg(debug_assertions)]
            tts::start_debug_server();

            // Warm Kokoro neural TTS in the background (Mac + Windows), then
            // pre-cache common combat callouts so interrupt/stun aren't cold.
            let warm_voice = active_voice_id(&load_settings(app.handle()));
            std::thread::spawn(move || {
                app_log::write("Kokoro: background warm starting");
                if let Err(err) = kokoro::ensure_daemon() {
                    app_log::write(&format!("Kokoro: background warm failed: {err}"));
                } else {
                    app_log::write("Kokoro: background warm ok");
                }
                tts::warm_essential_callouts(&warm_voice);
            });

            // Prune finished timers on a schedule. Without this, overlay stays at
            // 0s until the next log line (or clear) triggers emit_state.
            if let Some(app_state) = app.try_state::<Arc<AppState>>() {
                let tick_state = Arc::clone(app_state.inner());
                let tick_app = handle.clone();
                thread::spawn(move || loop {
                    thread::sleep(Duration::from_millis(250));
                    let removed = tick_state.engine.lock().prune_expired_timers();
                    if removed {
                        let _ = emit_state(&tick_state, &tick_app);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host_os,
            get_settings,
            save_app_settings,
            save_window_geometry,
            find_logs,
            auto_detect_log,
            get_engine_state,
            get_triggers,
            save_triggers,
            set_groups_enabled,
            import_triggers_path,
            install_starter_pack,
            clear_timers,
            clear_timer,
            clear_alerts,
            test_trigger,
            speak_text,
            test_speech,
            list_alert_sounds,
            play_alert_sound,
            list_kokoro_voices,
            list_audio_output_devices,
            preview_kokoro_voice,
            kokoro_status,
            get_app_log_path,
            get_overlay_status,
            start_monitoring,
            stop_monitoring,
            open_overlay,
            close_overlay,
            set_overlay_click_through,
        ])
        .run(tauri::generate_context!())
        .expect("error while running EQL Alerts");
}
