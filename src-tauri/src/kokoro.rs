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
    python_bin(root).is_some() && models_ready(root)
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
    // Replace any previous extract so upgrades get a fresh Python/models tree.
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| format!("clear kokoro extract: {e}"))?;
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("create kokoro extract: {e}"))?;

    let file = std::fs::File::open(zip_path).map_err(|e| format!("open kokoro zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("kokoro zip: {e}"))?;
    for i in 0..archive.len() {
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
        return Err(format!(
            "Kokoro pack extracted but incomplete at {}",
            dest.display()
        ));
    }
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

    // Port open but unhealthy (stale process from another checkout) — replace it.
    stop_tracked_daemon();
    if port_open() {
        kill_port_listener();
        thread::sleep(Duration::from_millis(150));
    }

    let root = helper_root()?;
    let py = python_bin(&root).ok_or("Kokoro python missing")?;
    let script = root.join("speak.py");

    let mut guard = DAEMON.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(child) = guard.as_mut() {
        if child.try_wait().ok().flatten().is_none() && daemon_healthy() {
            return Ok(());
        }
    }

    let mut cmd = Command::new(&py);
    cmd.arg(&script)
        .args(["serve", "--host", "127.0.0.1", "--port", &KOKORO_PORT.to_string()])
        .current_dir(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("start kokoro daemon: {e}"))?;
    *guard = Some(child);
    drop(guard);

    for _ in 0..100 {
        if daemon_healthy() {
            return Ok(());
        }
        if port_open() {
            // Daemon accepted TCP but /voices not ready yet.
            let _ = http_get("/voices", 5_000);
            if daemon_healthy() {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("Kokoro daemon failed to start (timeout)".into())
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
