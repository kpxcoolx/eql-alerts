//! Simple append-only log for troubleshooting (Windows + Mac).
//!
//! Windows: `%LOCALAPPDATA%\com.eqlegends.alerts\eql-alerts.log`
//! macOS:   `~/Library/Logs/com.eqlegends.alerts/eql-alerts.log`

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn log_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(base)
                .join("com.eqlegends.alerts")
                .join("eql-alerts.log");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Logs/com.eqlegends.alerts")
                .join("eql-alerts.log");
        }
    }
    std::env::temp_dir().join("eql-alerts.log")
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Append a line to the app log and echo to stderr (visible when launched from a console).
pub fn write(msg: &str) {
    let line = format!("{} {msg}", now_ms());
    eprintln!("eql-alerts: {line}");

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let _guard = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}