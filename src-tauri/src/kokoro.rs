//! Kokoro neural TTS (macOS + Windows) via local helper daemon.

use serde::Deserialize;
use serde::Serialize;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Hide console window when spawning helper processes on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static DAEMON: Mutex<Option<Child>> = Mutex::new(None);
/// Serialize first-run zip extract so UI/warm threads don't race.
static EXTRACT_LOCK: Mutex<()> = Mutex::new(());

const KOKORO_PORT: u16 = 17423;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KokoroVoice {
    pub id: String,
    pub label: String,
    pub gender: String,
    pub locale: String,
}

fn models_ready(root: &Path) -> bool {
    root.join("models/kokoro-v1.0.onnx").exists()
        && root.join("models/voices-v1.0.bin").exists()
        && root.join("speak.py").exists()
}

fn python_bin(root: &Path) -> Option<PathBuf> {
    // Packaged portable Python (python_relpath.txt written by pack_kokoro_windows.ps1).
    let marker = root.join("python_relpath.txt");
    if let Ok(rel) = std::fs::read_to_string(&marker) {
        let p = root.join(rel.trim());
        if p.exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let p = root.join(".venv/Scripts/python.exe");
        if p.exists() {
            return Some(p);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let p = root.join(".venv/bin/python");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn root_ready(root: &Path) -> bool {
    if python_bin(root).is_none() || !models_ready(root) {
        return false;
    }
    // Packaged Windows zip writes python_relpath.txt + .deps-ok after a real pip install.
    // Reject older broken extracts that only had bare CPython + models.
    if root.join("python_relpath.txt").exists() && !root.join(".deps-ok").exists() {
        return false;
    }
    true
}

/// Writable install location for the packaged Kokoro zip (Windows NSIS/Mac).
fn extracted_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(base)
                .join("com.eqlegends.alerts")
                .join("tts-kokoro");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support/com.eqlegends.alerts/tts-kokoro");
        }
    }
    std::env::temp_dir().join("eql-alerts-tts-kokoro")
}

fn pack_zip_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tts-kokoro.zip"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("resources/tts-kokoro.zip"));
            out.push(dir.join("tts-kokoro.zip"));
            // macOS .app: Contents/MacOS → Contents/Resources
            out.push(dir.join("../Resources/resources/tts-kokoro.zip"));
            out.push(dir.join("../Resources/tts-kokoro.zip"));
        }
    }
    out
}

fn find_pack_zip() -> Option<PathBuf> {
    pack_zip_candidates().into_iter().find(|p| p.exists())
}

fn find_ready_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    // Dev / already-unpacked bundle dirs (not the versioned LocalAppData extract).
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tts-kokoro"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tts-kokoro"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("resources/tts-kokoro"));
            candidates.push(dir.join("tts-kokoro"));
            candidates.push(dir.join("../Resources/resources/tts-kokoro"));
            candidates.push(dir.join("../Resources/tts-kokoro"));
        }
    }
    candidates.into_iter().find(|p| root_ready(p))
}

fn extract_current(root: &Path) -> bool {
    match std::fs::read_to_string(root.join(".pack-version")) {
        Ok(v) => v.trim() == env!("CARGO_PKG_VERSION") && root_ready(root),
        Err(_) => false,
    }
}

fn extract_pack_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let zip_bytes = std::fs::metadata(zip_path).map(|m| m.len()).unwrap_or(0);
    crate::app_log::write(&format!(
        "Kokoro: extracting {} ({} MB) → {}",
        zip_path.display(),
        zip_bytes / (1024 * 1024),
        dest.display()
    ));

    // Replace any previous extract so upgrades get a fresh Python/models tree.
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| format!("clear kokoro extract: {e}"))?;
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("create kokoro extract: {e}"))?;

    let file = std::fs::File::open(zip_path).map_err(|e| format!("open kokoro zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("kokoro zip: {e}"))?;
    let entry_count = archive.len();
    for i in 0..entry_count {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("kokoro zip entry: {e}"))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("kokoro zip mkdir: {e}"))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("kokoro zip mkdir: {e}"))?;
        }
        let mut out =
            std::fs::File::create(&out_path).map_err(|e| format!("kokoro zip write: {e}"))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("kokoro zip copy: {e}"))?;
    }

    std::fs::write(dest.join(".pack-version"), env!("CARGO_PKG_VERSION"))
        .map_err(|e| format!("kokoro pack version: {e}"))?;

    if !root_ready(dest) {
        let err = format!(
            "Kokoro pack extracted but incomplete at {}",
            dest.display()
        );
        crate::app_log::write(&format!("Kokoro: {err}"));
        return Err(err);
    }
    crate::app_log::write(&format!(
        "Kokoro: extract done ({entry_count} entries) at {}",
        dest.display()
    ));
    Ok(())
}

/// Resolve a ready Kokoro tree, extracting the bundled zip on first run / upgrade.
fn helper_root() -> Result<PathBuf, String> {
    let extracted = extracted_root();
    if extract_current(&extracted) {
        return Ok(extracted);
    }
    // Local checkout ready without extract — no lock needed.
    if let Some(ready) = find_ready_root() {
        return Ok(ready);
    }

    let _guard = EXTRACT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Another thread may have finished extracting while we waited.
    if extract_current(&extracted) {
        return Ok(extracted);
    }
    // Packaged installer: (re)extract zip into LocalAppData so Python is writable & portable.
    if let Some(zip) = find_pack_zip() {
        extract_pack_zip(&zip, &extracted)?;
        return Ok(extracted);
    }
    let tried = pack_zip_candidates()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    crate::app_log::write(&format!("Kokoro: no pack zip found. Tried: {tried}"));
    Err(
        "Kokoro TTS not set up. Reinstall the app, or run scripts/setup_kokoro.sh (Mac) / scripts/setup_kokoro.bat (Windows)."
            .into(),
    )
}

pub fn is_available() -> bool {
    if extract_current(&extracted_root()) {
        return true;
    }
    if find_pack_zip().is_some() {
        return true;
    }
    find_ready_root().is_some()
}

/// True when the local daemon is already answering (does not extract or spawn).
pub fn daemon_running() -> bool {
    daemon_healthy()
}

/// Fetch voice catalog only if the daemon is already up — never blocks on extract/startup.
pub fn list_voices_if_running() -> Result<Vec<KokoroVoice>, String> {
    if !daemon_healthy() {
        return Err("daemon not running".into());
    }
    let body = http_get("/voices", 2_000)?;
    serde_json::from_str(&body).map_err(|e| format!("parse voices: {e}"))
}

fn http_get(path: &str, timeout_ms: u64) -> Result<String, String> {
    // Plain TCP HTTP — avoid `curl` on Windows, which flashes a console per request
    // (ensure_daemon polls health in a tight loop at startup).
    let addr: SocketAddr = format!("127.0.0.1:{KOKORO_PORT}")
        .parse()
        .map_err(|e| format!("kokoro addr: {e}"))?;
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("kokoro connect: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("kokoro read timeout: {e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("kokoro write timeout: {e}"))?;

    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{KOKORO_PORT}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("kokoro write: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("kokoro read: {e}"))?;
    let raw = String::from_utf8_lossy(&buf);
    let Some((header, body)) = raw.split_once("\r\n\r\n") else {
        return Err("kokoro http bad reply".into());
    };
    let status_ok = header.starts_with("HTTP/1.1 200") || header.starts_with("HTTP/1.0 200");
    if !status_ok {
        let status_line = header.lines().next().unwrap_or("?");
        return Err(format!("kokoro http failed: {status_line}"));
    }
    let body = body.trim();
    if body.is_empty() {
        return Err("kokoro http empty reply".into());
    }
    Ok(body.to_owned())
}

fn port_open() -> bool {
    let Ok(addr) = format!("127.0.0.1:{KOKORO_PORT}").parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(80)).is_ok()
}

fn daemon_healthy() -> bool {
    match http_get("/voices", 2_000) {
        Ok(body) => {
            let t = body.trim();
            t.starts_with('[') || t.contains("\"id\"")
        }
        Err(_) => false,
    }
}

fn stop_tracked_daemon() {
    let mut guard = DAEMON.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn kill_port_listener() {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let output = Command::new("lsof")
            .args([
                &format!("-tiTCP:{KOKORO_PORT}"),
                "-sTCP:LISTEN",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        if let Ok(output) = output {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid in pids.split_whitespace() {
                let _ = Command::new("kill").args(["-9", pid]).status();
            }
        }
    }
}

pub fn ensure_daemon() -> Result<(), String> {
    if daemon_healthy() {
        return Ok(());
    }

    crate::app_log::write("Kokoro: ensure_daemon — daemon not healthy, starting…");

    // Port open but unhealthy (stale process from another checkout) — replace it.
    stop_tracked_daemon();
    if port_open() {
        crate::app_log::write("Kokoro: killing stale listener on port");
        kill_port_listener();
        thread::sleep(Duration::from_millis(150));
    }

    let root = match helper_root() {
        Ok(r) => r,
        Err(err) => {
            crate::app_log::write(&format!("Kokoro: helper_root failed: {err}"));
            return Err(err);
        }
    };
    let py = match python_bin(&root) {
        Some(p) => p,
        None => {
            let err = "Kokoro python missing".to_string();
            crate::app_log::write(&format!("Kokoro: {err} (root={})", root.display()));
            return Err(err);
        }
    };
    let script = root.join("speak.py");
    crate::app_log::write(&format!(
        "Kokoro: spawning {} {} (cwd={})",
        py.display(),
        script.display(),
        root.display()
    ));

    let mut guard = DAEMON.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(child) = guard.as_mut() {
        if child.try_wait().ok().flatten().is_none() && daemon_healthy() {
            return Ok(());
        }
    }

    let stderr_path = root.join("daemon.stderr.log");
    let stderr_file = std::fs::File::create(&stderr_path).map_err(|e| {
        let msg = format!("create kokoro stderr log: {e}");
        crate::app_log::write(&format!("Kokoro: {msg}"));
        msg
    })?;

    let mut cmd = Command::new(&py);
    cmd.arg(&script)
        .args(["serve", "--host", "127.0.0.1", "--port", &KOKORO_PORT.to_string()])
        .current_dir(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let child = cmd.spawn().map_err(|e| {
        let msg = format!("start kokoro daemon: {e}");
        crate::app_log::write(&format!("Kokoro: {msg}"));
        msg
    })?;
    *guard = Some(child);
    drop(guard);

    // Model loads before the socket opens — allow a few minutes on slow VMs.
    // Never call daemon_healthy() while the port is closed (2s connect timeout each try).
    for _ in 0..1800 {
        if let Ok(mut g) = DAEMON.lock() {
            if let Some(child) = g.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let tail = read_log_tail(&stderr_path, 1200);
                    let err = format!("Kokoro daemon exited ({status}): {tail}");
                    crate::app_log::write(&format!("Kokoro: {err}"));
                    return Err(err);
                }
            }
        }
        if port_open() && daemon_healthy() {
            crate::app_log::write("Kokoro: daemon healthy");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let tail = read_log_tail(&stderr_path, 1200);
    let err = format!("Kokoro daemon failed to start (timeout): {tail}");
    crate::app_log::write(&format!("Kokoro: {err}"));
    Err(err)
}

fn read_log_tail(path: &Path, max_chars: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "(no stderr)".into();
    }
    let collapsed = trimmed.replace('\r', "");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let start = collapsed
        .char_indices()
        .rev()
        .nth(max_chars - 1)
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("…{}", &collapsed[start..])
}

/// Start daemon if needed, then return the live voice catalog.
pub fn list_voices() -> Result<Vec<KokoroVoice>, String> {
    ensure_daemon()?;
    list_voices_if_running()
}

#[derive(Debug, Deserialize)]
struct SpeakResponse {
    #[allow(dead_code)]
    ok: Option<bool>,
    path: Option<String>,
    error: Option<String>,
}

pub fn synthesize_to_wav(text: &str, voice: &str, speed: f64) -> Result<PathBuf, String> {
    ensure_daemon()?;
    let text_q = urlencoding_encode(text);
    let voice_q = urlencoding_encode(voice);
    let path = format!(
        "/speak?text={text_q}&voice={voice_q}&speed={speed:.3}"
    );
    let body = http_get(&path, 45_000)?;
    let parsed: SpeakResponse =
        serde_json::from_str(&body).map_err(|e| format!("parse speak: {e} / {body}"))?;
    if let Some(err) = parsed.error {
        return Err(err);
    }
    let Some(p) = parsed.path else {
        return Err(format!("kokoro speak missing path: {body}"));
    };
    let pb = PathBuf::from(p);
    if !pb.exists() {
        return Err(format!("kokoro wav missing: {}", pb.display()));
    }
    Ok(pb)
}

fn urlencoding_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Built-in catalog when the daemon is not running yet (UI still works).
pub fn fallback_voice_catalog() -> Vec<KokoroVoice> {
    [
        ("af_heart", "Heart", "female", "en-US"),
        ("af_bella", "Bella", "female", "en-US"),
        ("af_nicole", "Nicole", "female", "en-US"),
        ("af_sarah", "Sarah", "female", "en-US"),
        ("af_kore", "Kore", "female", "en-US"),
        ("af_aoede", "Aoede", "female", "en-US"),
        ("af_nova", "Nova", "female", "en-US"),
        ("bf_emma", "Emma (UK)", "female", "en-GB"),
        ("bf_isabella", "Isabella (UK)", "female", "en-GB"),
        ("am_michael", "Michael", "male", "en-US"),
        ("am_fenrir", "Fenrir", "male", "en-US"),
        ("am_puck", "Puck", "male", "en-US"),
        ("am_echo", "Echo", "male", "en-US"),
        ("am_liam", "Liam", "male", "en-US"),
        ("bm_george", "George (UK)", "male", "en-GB"),
        ("bm_fable", "Fable (UK)", "male", "en-GB"),
    ]
    .into_iter()
    .map(|(id, label, gender, locale)| KokoroVoice {
        id: id.into(),
        label: label.into(),
        gender: gender.into(),
        locale: locale.into(),
    })
    .collect()
}
