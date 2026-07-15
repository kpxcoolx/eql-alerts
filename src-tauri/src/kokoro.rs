//! Kokoro neural TTS (macOS + Windows) via local helper daemon.

use serde::Deserialize;
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

static DAEMON: Mutex<Option<Child>> = Mutex::new(None);

const KOKORO_PORT: u16 = 17423;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KokoroVoice {
    pub id: String,
    pub label: String,
    pub gender: String,
    pub locale: String,
}

fn helper_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tts-kokoro")
}

fn python_bin(root: &Path) -> Option<PathBuf> {
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

fn models_ready(root: &Path) -> bool {
    root.join("models/kokoro-v1.0.onnx").exists()
        && root.join("models/voices-v1.0.bin").exists()
        && root.join("speak.py").exists()
}

pub fn is_available() -> bool {
    let root = helper_root();
    python_bin(&root).is_some() && models_ready(&root)
}

fn http_get(path: &str, timeout_ms: u64) -> Result<String, String> {
    let addr = format!("127.0.0.1:{KOKORO_PORT}");
    let url = format!("http://{addr}{path}");
    // Tiny blocking HTTP via curl — portable on Mac/Windows when curl is present.
    let output = Command::new("curl")
        .args([
            "-sS",
            "--fail",
            "--max-time",
            &format!("{}", (timeout_ms / 1000).max(1)),
            &url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kokoro http failed: {err}"));
    }
    let body = String::from_utf8_lossy(&output.stdout).into_owned();
    if body.trim().is_empty() {
        return Err("kokoro http empty reply".into());
    }
    Ok(body)
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

    if !is_available() {
        return Err(
            "Kokoro TTS not set up. Run scripts/setup_kokoro.sh (Mac) or scripts/setup_kokoro.bat (Windows)."
                .into(),
        );
    }
    let root = helper_root();
    let py = python_bin(&root).ok_or("Kokoro python missing")?;
    let script = root.join("speak.py");

    let mut guard = DAEMON.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(child) = guard.as_mut() {
        if child.try_wait().ok().flatten().is_none() && daemon_healthy() {
            return Ok(());
        }
    }

    let child = Command::new(&py)
        .arg(&script)
        .args(["serve", "--host", "127.0.0.1", "--port", &KOKORO_PORT.to_string()])
        .current_dir(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
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

pub fn list_voices() -> Result<Vec<KokoroVoice>, String> {
    ensure_daemon()?;
    let body = http_get("/voices", 30_000)?;
    serde_json::from_str(&body).map_err(|e| format!("parse voices: {e}"))
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
