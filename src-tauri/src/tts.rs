//! Alert chimes + spoken callouts.
//!
//! Chimes (Ping, etc.) play via `eql-speak` (AVFoundation).
//! Voice/TTS WAV play through the WebView (`tts-play` event → HTMLAudioElement)
//! because AVFoundation/cpal often report success but stay silent on surround
//! devices (SteelSeries Arena 6ch), while WebKit audio is audible.

use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::io::Write;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use rodio::{Decoder, OutputStream, Sink};
use tauri::{AppHandle, Emitter};

static SPEECH_LOCK: Mutex<()> = Mutex::new(());
static LAST_SPEECH_PID: Mutex<Option<u32>> = Mutex::new(None);
static AUDIO_STOP: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);
/// Bumped by stop_speech so in-flight Kokoro work is dropped before play.
static SPEECH_GEN: AtomicU64 = AtomicU64::new(0);
/// Preferred output device name (empty / None = system default).
static PREFERRED_OUTPUT: Mutex<Option<String>> = Mutex::new(None);
static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);
/// Reuse Kokoro AIFF files for repeated callouts (cuts toast→voice lag).
static SPEAK_FILE_CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

fn speak_cache() -> &'static Mutex<HashMap<String, PathBuf>> {
    SPEAK_FILE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn phrase_cache_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let dir = PathBuf::from(home)
            .join("Library/Caches/com.eqlegends.alerts/tts-phrases");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    let dir = std::env::temp_dir().join("eql-alerts-tts").join("phrase-cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn set_app_handle(app: AppHandle) {
    if let Ok(mut slot) = APP_HANDLE.lock() {
        *slot = Some(app);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioOutputDevice {
    pub name: String,
    pub channels: u16,
    pub is_default: bool,
}

/// Set the preferred audio output device by name. Pass `None` or empty for system default.
pub fn set_preferred_output_device(name: Option<&str>) {
    let cleaned = name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Ok(mut slot) = PREFERRED_OUTPUT.lock() {
        *slot = cleaned;
    }
}

pub fn preferred_output_device() -> Option<String> {
    PREFERRED_OUTPUT
        .lock()
        .ok()
        .and_then(|g| g.clone())
}

pub fn list_output_devices() -> Vec<AudioOutputDevice> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut out = Vec::new();
    let Ok(devices) = host.output_devices() else {
        return out;
    };
    for device in devices {
        let Ok(name) = device.name() else {
            continue;
        };
        let channels = device
            .default_output_config()
            .map(|c| c.channels())
            .unwrap_or(2);
        let is_default = name == default_name;
        out.push(AudioOutputDevice {
            name,
            channels,
            is_default,
        });
    }
    out.sort_by(|a, b| match (b.is_default, a.is_default) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
    });
    out
}

fn resolve_output_device() -> Result<cpal::Device, String> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let preferred = preferred_output_device();
    if let Some(want) = preferred {
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if device.name().ok().as_deref() == Some(want.as_str()) {
                    return Ok(device);
                }
            }
        }
        tts_log(&format!(
            "preferred device {want:?} not found — falling back to system default"
        ));
    }
    host.default_output_device()
        .ok_or_else(|| "no default audio output device".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceGender {
    Female,
    Male,
}

impl VoiceGender {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "male" | "m" | "man" => Self::Male,
            _ => Self::Female,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertSoundInfo {
    pub id: String,
    pub label: String,
}

fn tts_log(msg: &str) {
    eprintln!("eql-alerts TTS: {msg}");
    let path = PathBuf::from("/tmp/eql-alerts-tts.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{msg}");
    }
}

pub fn alert_sounds() -> Vec<AlertSoundInfo> {
    let mut out = vec![AlertSoundInfo {
        id: "none".into(),
        label: "None".into(),
    }];
    for (id, label) in [
        ("glass", "Glass"),
        ("ping", "Ping"),
        ("sosumi", "Sosumi"),
        ("submarine", "Submarine"),
        ("hero", "Hero"),
        ("funk", "Funk"),
        ("purr", "Purr"),
        ("tink", "Tink"),
        ("bottle", "Bottle"),
        ("blow", "Blow"),
        ("pop", "Pop"),
        ("morse", "Morse"),
        ("frog", "Frog"),
        ("basso", "Basso"),
    ] {
        if resolve_sound_path(id).is_some() {
            out.push(AlertSoundInfo {
                id: id.into(),
                label: label.into(),
            });
        }
    }
    out
}

pub fn resolve_sound_path(sound: &str) -> Option<PathBuf> {
    let key = sound.trim();
    if key.is_empty() || key.eq_ignore_ascii_case("none") {
        return None;
    }

    let path = Path::new(key);
    if path.is_absolute() && path.exists() {
        return Some(path.to_path_buf());
    }

    let preset = key.to_ascii_lowercase();

    #[cfg(target_os = "macos")]
    {
        let file = match preset.as_str() {
            "glass" => "Glass.aiff",
            "ping" => "Ping.aiff",
            "sosumi" => "Sosumi.aiff",
            "submarine" => "Submarine.aiff",
            "hero" => "Hero.aiff",
            "funk" => "Funk.aiff",
            "purr" => "Purr.aiff",
            "tink" => "Tink.aiff",
            "bottle" => "Bottle.aiff",
            "blow" => "Blow.aiff",
            "pop" => "Pop.aiff",
            "morse" => "Morse.aiff",
            "frog" => "Frog.aiff",
            "basso" => "Basso.aiff",
            _ => return None,
        };
        let full = PathBuf::from("/System/Library/Sounds").join(file);
        if full.exists() {
            return Some(full);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let file = match preset.as_str() {
            "glass" | "ping" | "tink" | "pop" => "Windows Notify System Generic.wav",
            "sosumi" | "hero" | "funk" => "Windows Notify Email.wav",
            "submarine" | "blow" | "basso" => "Windows Notify.wav",
            "purr" | "bottle" | "frog" | "morse" => "Windows Proximity Notification.wav",
            _ => return None,
        };
        let full = PathBuf::from(r"C:\Windows\Media").join(file);
        if full.exists() {
            return Some(full);
        }
    }

    None
}

pub fn play_sound(sound: &str) -> Result<(), String> {
    let Some(path) = resolve_sound_path(sound) else {
        return Ok(());
    };
    // Chimes stay at full scale; voice volume is separate.
    // Do not track chime PIDs as "speech" — stop_speech must not kill them
    // mid-alert or confuse overlapping callouts.
    play_file_tracked(&path, false, 1.0, false)
}

fn clamp_volume(v: f64) -> f64 {
    if !v.is_finite() {
        return 1.0;
    }
    v.clamp(0.0, 1.0)
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
}

fn is_aiff(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("aiff") || e.eq_ignore_ascii_case("aif"))
        .unwrap_or(false)
}

fn output_channel_count() -> u16 {
    use cpal::traits::DeviceTrait;
    let Ok(device) = resolve_output_device() else {
        return 2;
    };
    device
        .default_output_config()
        .map(|c| c.channels())
        .unwrap_or(2)
}

/// Rewrite a mono/stereo WAV so surround devices get audible front L/R.
/// SteelSeries Arena (and similar) only expose 6ch @ 48kHz to cpal — feeding
/// stereo into that stream plays as silence.
fn wav_for_output_device(src: &Path) -> Result<(PathBuf, bool), String> {
    use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

    let out_ch = output_channel_count();
    if out_ch <= 2 {
        return Ok((src.to_path_buf(), false));
    }

    let mut reader = WavReader::open(src).map_err(|e| format!("wav read: {e}"))?;
    let in_spec = reader.spec();
    let in_ch = in_spec.channels as usize;
    if in_ch == 0 {
        return Err("wav has 0 channels".into());
    }

    let samples_i16: Vec<i16> = match in_spec.sample_format {
        SampleFormat::Int => reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("wav samples: {e}"))?,
        SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("wav samples: {e}"))?,
    };

    let frames: Vec<(i16, i16)> = samples_i16
        .chunks(in_ch)
        .map(|c| {
            let l = c[0];
            let r = if c.len() > 1 { c[1] } else { l };
            (l, r)
        })
        .collect();
    if frames.is_empty() {
        return Err("wav is empty".into());
    }

    let out_rate = 48_000u32;
    let ratio = out_rate as f64 / f64::from(in_spec.sample_rate.max(1));
    let out_frames = ((frames.len() as f64) * ratio).round().max(1.0) as usize;

    let out_path = std::env::temp_dir().join(format!("eql-play-{}.wav", uuid::Uuid::new_v4()));
    let out_spec = WavSpec {
        channels: out_ch,
        sample_rate: out_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(&out_path, out_spec).map_err(|e| format!("wav write: {e}"))?;

    for i in 0..out_frames {
        let src_i = ((i as f64) / ratio)
            .floor()
            .clamp(0.0, (frames.len() - 1) as f64) as usize;
        let (l, r) = frames[src_i];
        writer.write_sample(l).map_err(|e| e.to_string())?;
        writer.write_sample(r).map_err(|e| e.to_string())?;
        for _ in 2..out_ch {
            writer.write_sample(0i16).map_err(|e| e.to_string())?;
        }
    }
    writer
        .finalize()
        .map_err(|e| format!("wav finalize: {e}"))?;
    tts_log(&format!(
        "upmixed {} → {}ch {}Hz {}",
        src.display(),
        out_ch,
        out_rate,
        out_path.display()
    ));
    Ok((out_path, true))
}

fn open_output_stream() -> Result<(OutputStream, rodio::OutputStreamHandle), String> {
    use cpal::traits::DeviceTrait;

    let device = resolve_output_device()?;
    let name = device.name().unwrap_or_else(|_| "unknown".into());

    // Prefer stereo when the device allows it.
    let mut preferred: Option<cpal::SupportedStreamConfig> = None;
    if let Ok(configs) = device.supported_output_configs() {
        for range in configs {
            if range.channels() == 2 {
                preferred = Some(range.with_max_sample_rate());
                break;
            }
        }
    }

    if let Some(config) = preferred {
        tts_log(&format!(
            "audio device={name} stereo {}Hz {}ch",
            config.sample_rate().0,
            config.channels()
        ));
        return OutputStream::try_from_device_config(&device, config)
            .map_err(|e| format!("stereo stream ({name}): {e}"));
    }

    let ch = device
        .default_output_config()
        .map(|c| c.channels())
        .unwrap_or(0);
    tts_log(&format!("audio device={name} (default config {ch}ch)"));
    OutputStream::try_from_device(&device).map_err(|e| format!("stream ({name}): {e}"))
}

fn play_file_rodio(path: &Path, wait: bool, volume: f64) -> Result<(), String> {
    let volume = clamp_volume(volume) as f32;
    let (play_path, is_temp) = wav_for_output_device(path)?;

    // Signal any previous in-process clip to stop.
    if let Ok(mut slot) = AUDIO_STOP.lock() {
        if let Some(tx) = slot.take() {
            let _ = tx.send(());
        }
    }

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    if let Ok(mut slot) = AUDIO_STOP.lock() {
        *slot = Some(stop_tx);
    }

    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();

    thread::spawn(move || {
        let result = (|| {
            let (_stream, handle) = open_output_stream()?;
            let sink = Sink::try_new(&handle).map_err(|e| format!("audio sink: {e}"))?;
            let file =
                File::open(&play_path).map_err(|e| format!("open {}: {e}", play_path.display()))?;
            let source =
                Decoder::new(BufReader::new(file)).map_err(|e| format!("decode wav: {e}"))?;
            sink.set_volume(volume);
            sink.append(source);
            tts_log(&format!(
                "rodio play {} vol={volume:.2} wait={wait}",
                play_path.display()
            ));

            loop {
                if sink.empty() {
                    break;
                }
                if stop_rx.try_recv().is_ok() {
                    sink.stop();
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        })();
        if is_temp {
            let _ = std::fs::remove_file(&play_path);
        }
        let _ = done_tx.send(result);
    });

    if wait {
        return done_rx
            .recv_timeout(Duration::from_secs(45))
            .map_err(|_| "audio playback timed out".to_string())?;
    }
    Ok(())
}

fn play_file_helper(path: &Path, wait: bool, volume: f64, track_pid: bool) -> Result<(), String> {
    let volume = clamp_volume(volume);
    #[cfg(target_os = "macos")]
    {
        if let Some(bin) = eql_speak_bin() {
            let vol = format!("{volume:.3}");
            let path_str = path.to_string_lossy().into_owned();
            tts_log(&format!(
                "eql-speak play {} vol={volume:.2} wait={wait} track={track_pid}",
                path.display()
            ));
            if wait {
                return run_helper_wait(&bin, &["--volume", &vol, &path_str], track_pid);
            }
            let mut child = Command::new(&bin)
                .args(["--volume", &vol])
                .arg(&path_str)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("eql-speak play: {e}"))?;
            let pid = child.id();
            if track_pid {
                remember_pid(pid);
            }
            // Reap in the background so fire-and-forget playback does not leak zombies
            // or hold SPEECH_LOCK for the whole clip (that delayed interrupt callouts).
            thread::spawn(move || {
                let _ = child.wait();
                if let Ok(mut guard) = LAST_SPEECH_PID.lock() {
                    if *guard == Some(pid) {
                        *guard = None;
                    }
                }
            });
            return Ok(());
        }
        return Err("eql-speak missing".into());
    }

    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy().replace('\'', "''");
        let mut child = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("$p = New-Object Media.SoundPlayer '{path_str}'; $p.PlaySync();"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("sound player: {e}"))?;
        let _ = (volume, track_pid);
        if wait {
            let _ = child.wait();
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (path, wait, volume, track_pid);
        Err("Alert sounds are only wired for macOS and Windows".into())
    }
}

fn wav_duration_ms(path: &Path) -> u64 {
    let Ok(reader) = hound::WavReader::open(path) else {
        return 1_500;
    };
    let spec = reader.spec();
    let frames = reader.duration() as u64;
    let rate = u64::from(spec.sample_rate.max(1));
    ((frames * 1000) / rate).saturating_add(120).max(200)
}

fn play_file_webview(path: &Path, wait: bool, volume: f64) -> Result<(), String> {
    let volume = clamp_volume(volume);
    let bytes = std::fs::read(path).map_err(|e| format!("read wav: {e}"))?;
    let wav_base64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    let app = APP_HANDLE
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .ok_or_else(|| "app handle not ready for webview TTS".to_string())?;

    tts_log(&format!(
        "webview tts {} bytes={} vol={volume:.2}",
        path.display(),
        bytes.len()
    ));
    app.emit(
        "tts-play",
        serde_json::json!({
            "wav_base64": wav_base64,
            "volume": volume,
        }),
    )
    .map_err(|e| format!("emit tts-play: {e}"))?;

    if wait {
        thread::sleep(Duration::from_millis(wav_duration_ms(path)));
    }
    Ok(())
}

#[allow(dead_code)]
fn play_file(path: &Path, wait: bool, volume: f64) -> Result<(), String> {
    play_file_tracked(path, wait, volume, true)
}

fn play_file_tracked(path: &Path, wait: bool, volume: f64, track_pid: bool) -> Result<(), String> {
    // AIFF (Kokoro-on-Mac + system chimes) → eql-speak — proven audible with Ping.
    if is_aiff(path) {
        return play_file_helper(path, wait, volume, track_pid);
    }

    // WAV → WebView HTMLAudio; fallback to helpers.
    if is_wav(path) {
        match play_file_webview(path, wait, volume) {
            Ok(()) => return Ok(()),
            Err(err) => tts_log(&format!("webview tts failed ({err}), trying helper")),
        }
        #[cfg(target_os = "macos")]
        {
            match play_file_helper(path, wait, volume, track_pid) {
                Ok(()) => return Ok(()),
                Err(err) => tts_log(&format!("eql-speak failed ({err}), trying rodio")),
            }
            return play_file_rodio(path, wait, volume);
        }
        #[cfg(not(target_os = "macos"))]
        {
            match play_file_rodio(path, wait, volume) {
                Ok(()) => return Ok(()),
                Err(err) => tts_log(&format!("rodio failed ({err}), trying helper")),
            }
            return play_file_helper(path, wait, volume, track_pid);
        }
    }

    play_file_helper(path, wait, volume, track_pid)
}

pub fn stop_speech() {
    SPEECH_GEN.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut slot) = AUDIO_STOP.lock() {
        if let Some(tx) = slot.take() {
            let _ = tx.send(());
        }
    }
    if let Ok(mut guard) = LAST_SPEECH_PID.lock() {
        if let Some(pid) = guard.take() {
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

pub fn speak_now(text: &str, voice_id: &str, volume: f64) -> Result<(), String> {
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return Ok(());
    }
    // Soft stop: do not kill -9 mid-buffer. Overlapping short callouts is
    // better than silence from cutting the clip before the device plays.
    let _guard = SPEECH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    speak_blocking(cleaned, voice_id, volume)
}

pub fn speak(text: &str, voice_id: &str, volume: f64) -> Result<(), String> {
    let cleaned = text.trim().to_string();
    if cleaned.is_empty() {
        return Ok(());
    }
    let voice_id = voice_id.to_string();
    thread::spawn(move || {
        let _guard = SPEECH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(err) = speak_blocking(&cleaned, &voice_id, volume) {
            tts_log(&format!("background speak failed: {err}"));
        }
    });
    Ok(())
}

fn voice_gender_from_id(voice_id: &str) -> VoiceGender {
    let id = voice_id.trim().to_ascii_lowercase();
    if id.starts_with("am_") || id.starts_with("bm_") || id.contains("male") {
        VoiceGender::Male
    } else {
        VoiceGender::Female
    }
}

fn eql_speak_bin() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join("eql-speak")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("eql-speak"));
            candidates.push(parent.join("binaries").join("eql-speak"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

fn run_helper_wait(bin: &Path, args: &[&str], track_pid: bool) -> Result<(), String> {
    let child = Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn eql-speak: {e}"))?;
    if track_pid {
        remember_pid(child.id());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("eql-speak wait: {e}"))?;
    if track_pid {
        clear_pid();
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !err.is_empty() {
        tts_log(&format!("helper stderr: {err}"));
    }
    if !output.status.success() {
        return Err(format!("eql-speak exit {}: {err}", output.status));
    }
    Ok(())
}

fn copy_callout_to_cache(src: &Path) -> Result<PathBuf, String> {
    let cache = std::env::temp_dir().join("eql-alerts-tts");
    std::fs::create_dir_all(&cache).map_err(|e| format!("cache dir: {e}"))?;
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav");
    let dest = cache.join(format!("callout-{}.{}", uuid::Uuid::new_v4(), ext));
    std::fs::copy(src, &dest).map_err(|e| format!("cache copy: {e}"))?;
    Ok(dest)
}

fn speak_blocking(text: &str, voice_id: &str, volume: f64) -> Result<(), String> {
    let volume = clamp_volume(volume);
    let voice_id = if voice_id.trim().is_empty() {
        "bf_isabella"
    } else {
        voice_id.trim()
    };
    let gender = voice_gender_from_id(voice_id);
    let ticket = SPEECH_GEN.load(Ordering::SeqCst);
    tts_log(&format!(
        "speak_blocking start voice={voice_id} gender={} vol={volume:.2} text={text:?}",
        gender.as_str()
    ));

    let cache_key = format!("{voice_id}\0{text}");
    if let Ok(guard) = speak_cache().lock() {
        if let Some(cached) = guard.get(&cache_key) {
            if cached.exists() {
                if SPEECH_GEN.load(Ordering::SeqCst) != ticket {
                    tts_log("speak cancelled before cache play");
                    return Ok(());
                }
                tts_log(&format!("speak cache hit {}", cached.display()));
                return play_file_tracked(cached, false, volume, true);
            }
        }
    }

    // Kokoro neural TTS only — selected voice always drives the callout.
    match crate::kokoro::synthesize_to_wav(text, voice_id, 1.15) {
        Ok(wav) => {
            if SPEECH_GEN.load(Ordering::SeqCst) != ticket {
                tts_log("speak cancelled after kokoro synth");
                let _ = std::fs::remove_file(&wav);
                return Ok(());
            }
            tts_log(&format!("kokoro file {}", wav.display()));
            let play_path = match copy_callout_to_cache(&wav) {
                Ok(p) => p,
                Err(err) => {
                    tts_log(&format!("cache copy failed ({err}), using temp file"));
                    wav.clone()
                }
            };
            // Durable phrase cache survives temp dir / app restarts.
            let durable = phrase_cache_dir();
            let safe: String = format!("{:x}", {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut h = DefaultHasher::new();
                cache_key.hash(&mut h);
                h.finish()
            });
            let ext = play_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("aiff");
            let durable_path = durable.join(format!("{safe}.{ext}"));
            if std::fs::copy(&play_path, &durable_path).is_ok() {
                if let Ok(mut guard) = speak_cache().lock() {
                    guard.insert(cache_key, durable_path.clone());
                }
            }
            if SPEECH_GEN.load(Ordering::SeqCst) != ticket {
                tts_log("speak cancelled before kokoro play");
                if play_path != wav {
                    let _ = std::fs::remove_file(&play_path);
                }
                let _ = std::fs::remove_file(&wav);
                return Ok(());
            }
            let play = play_file_tracked(&play_path, false, volume, true);
            if play_path != wav {
                let _ = std::fs::remove_file(&wav);
            }
            if play_path != durable_path {
                let _ = std::fs::remove_file(&play_path);
            }
            return play;
        }
        Err(err) => {
            tts_log(&format!("kokoro failed: {err}"));
            Err(format!("Kokoro TTS failed: {err}"))
        }
    }
}

/// Pre-synth essentials so the first fight hit is Kokoro cache-hot.
pub fn warm_essential_callouts(voice_id: &str) {
    let voice_id = if voice_id.trim().is_empty() {
        "bf_isabella".to_string()
    } else {
        voice_id.trim().to_string()
    };
    thread::spawn(move || {
        let phrases = [
            "Interrupted",
            "Stunned",
            "Out of mana",
            "Fizzle",
            "Pet died",
            "Low health",
            "You died",
            "Enraged",
            "Did not take hold",
        ];
        for phrase in phrases {
            let cache_key = format!("{voice_id}\0{phrase}");
            if let Ok(guard) = speak_cache().lock() {
                if guard.get(&cache_key).is_some_and(|p| p.exists()) {
                    continue;
                }
            }
            match crate::kokoro::synthesize_to_wav(phrase, &voice_id, 1.15) {
                Ok(wav) => {
                    let durable = phrase_cache_dir();
                    let safe: String = format!("{:x}", {
                        use std::collections::hash_map::DefaultHasher;
                        use std::hash::{Hash, Hasher};
                        let mut h = DefaultHasher::new();
                        cache_key.hash(&mut h);
                        h.finish()
                    });
                    let ext = wav
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("aiff");
                    let durable_path = durable.join(format!("{safe}.{ext}"));
                    if std::fs::copy(&wav, &durable_path).is_ok() {
                        if let Ok(mut guard) = speak_cache().lock() {
                            guard.insert(cache_key, durable_path);
                        }
                        tts_log(&format!("warmed TTS cache for {phrase:?}"));
                    }
                    let _ = std::fs::remove_file(&wav);
                }
                Err(err) => tts_log(&format!("warm {phrase:?} failed: {err}")),
            }
        }
    });
}

fn remember_pid(pid: u32) {
    if let Ok(mut guard) = LAST_SPEECH_PID.lock() {
        *guard = Some(pid);
    }
}

fn clear_pid() {
    if let Ok(mut guard) = LAST_SPEECH_PID.lock() {
        *guard = None;
    }
}

pub fn play_alert(
    sound: Option<&str>,
    speak_text: Option<&str>,
    voice_id: &str,
    volume: f64,
) -> Result<(), String> {
    let sound = sound.map(|s| s.to_string());
    let speak_text = speak_text.map(|s| s.to_string());
    let voice_id = voice_id.to_string();
    let volume = clamp_volume(volume);

    thread::spawn(move || {
        let has_speak = speak_text
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let has_sound = sound
            .as_ref()
            .map(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
            .unwrap_or(false);

        // Chime immediately with the toast — Kokoro synth can lag ~0.5–2s.
        if has_sound {
            if let Some(ref s) = sound {
                let _ = play_sound(s);
            }
        }

        if has_speak {
            let text = speak_text.as_ref().unwrap().trim().to_string();
            tts_log(&format!("play_alert speak: {text:?}"));
            // If another callout is playing, cut it and speak this one (don't queue
            // behind a spam backlog of "Out of range").
            stop_speech();
            let _guard = SPEECH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(err) = speak_blocking(&text, &voice_id, volume) {
                tts_log(&format!("play_alert speak failed: {err}"));
            }
        }
    });
    Ok(())
}

pub fn start_debug_server() {
    thread::spawn(|| {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let Ok(listener) = TcpListener::bind("127.0.0.1:17422") else {
            tts_log("debug server bind failed on 127.0.0.1:17422");
            return;
        };
        tts_log("debug server listening on http://127.0.0.1:17422/speak?text=Out%20of%20mana");
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let line = req.lines().next().unwrap_or("");
            let mut text = "Out of mana".to_string();
            let mut voice = "bf_isabella".to_string();
            if let Some(path) = line.split_whitespace().nth(1) {
                if let Some(q) = path.split('?').nth(1) {
                    for part in q.split('&') {
                        let mut kv = part.splitn(2, '=');
                        let k = kv.next().unwrap_or("");
                        let v = kv.next().unwrap_or("");
                        let decoded = urlencoding_decode(v);
                        if k == "text" && !decoded.is_empty() {
                            text = decoded;
                        } else if k == "voice" && !decoded.is_empty() {
                            voice = decoded;
                        } else if k == "gender" {
                            if decoded.eq_ignore_ascii_case("male") {
                                voice = "am_michael".into();
                            } else {
                                voice = "bf_isabella".into();
                            }
                        }
                    }
                }
            }

            let result = speak_now(&text, &voice, 1.0);
            let body = match result {
                Ok(()) => format!("OK spoke: {text}\n"),
                Err(e) => format!("ERR {e}\n"),
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
}

fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
