use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundLog {
    pub path: String,
    pub character: Option<String>,
    pub modified_secs: u64,
    pub size_bytes: u64,
    pub source: String,
}

pub fn candidate_log_dirs() -> Vec<(PathBuf, &'static str)> {
    let mut dirs = Vec::new();

    if let Ok(public) = std::env::var("PUBLIC") {
        dirs.push((
            PathBuf::from(public)
                .join("Daybreak Game Company")
                .join("Installed Games")
                .join("EverQuest Legends")
                .join("Logs"),
            "windows",
        ));
    }
    dirs.push((
        PathBuf::from(
            r"C:\Users\Public\Daybreak Game Company\Installed Games\EverQuest Legends\Logs",
        ),
        "windows",
    ));

    for logs in osxeql_log_dirs() {
        dirs.push((logs, "osxeql"));
    }

    for volume_logs in parallels_log_dirs() {
        dirs.push((volume_logs, "parallels"));
    }

    if let Ok(cwd) = std::env::current_dir() {
        dirs.push((cwd.join("samples"), "sample"));
        if let Some(parent) = cwd.parent() {
            dirs.push((parent.join("samples"), "sample"));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push((parent.join("Logs"), "local"));
            dirs.push((parent.join("samples"), "sample"));
        }
    }

    dirs
}

/// Native Mac Wine install from [osxEQL](https://github.com/kpxcoolx/osxEQL).
/// Game lives under the Wine prefix in Application Support (not under /Volumes).
pub fn osxeql_log_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(home) = std::env::var("HOME") else {
        return dirs;
    };
    let home = PathBuf::from(home);
    let relative = PathBuf::from("drive_c/users/Public/Daybreak Game Company")
        .join("Installed Games")
        .join("EverQuest Legends")
        .join("Logs");

    // Active prefix used by current osxEQL builds.
    dirs.push(
        home.join("Library/Application Support/osxEQL/prefix")
            .join(&relative),
    );
    // Legacy extracted-from-CrossOver prefix (older installs).
    dirs.push(
        home.join("Library/Application Support/osxEQL/prefix-cx")
            .join(&relative),
    );

    dirs
}

pub fn parallels_log_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let known = PathBuf::from(
        "/Volumes/[C] Windows 11.hidden/Users/Public/Daybreak Game Company/Installed Games/EverQuest Legends/Logs",
    );
    dirs.push(known);

    let Ok(volumes) = fs::read_dir("/Volumes") else {
        return dirs;
    };

    for entry in volumes.flatten() {
        let volume = entry.path();
        let logs = volume
            .join("Users/Public/Daybreak Game Company")
            .join("Installed Games")
            .join("EverQuest Legends")
            .join("Logs");
        if !dirs.iter().any(|d| d == &logs) {
            dirs.push(logs);
        }
    }

    dirs
}

pub fn find_eq_logs() -> Vec<FoundLog> {
    let mut found = Vec::new();

    for (dir, source) in candidate_log_dirs() {
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_eq_log_file(&path) {
                continue;
            }
            if let Some(mut item) = describe_log(&path) {
                item.source = source.to_string();
                found.push(item);
            }
        }
    }

    found.sort_by(|a, b| {
        let source_rank = |s: &str| match s {
            "osxeql" | "parallels" | "windows" => 0,
            "local" => 1,
            _ => 2,
        };
        source_rank(&a.source)
            .cmp(&source_rank(&b.source))
            .then(b.modified_secs.cmp(&a.modified_secs))
            .then(b.size_bytes.cmp(&a.size_bytes))
    });
    found.dedup_by(|a, b| a.path == b.path);
    found
}

pub fn best_log() -> Option<FoundLog> {
    find_eq_logs().into_iter().next()
}

fn is_eq_log_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.starts_with("eqlog_") && lower.ends_with(".txt")
}

fn describe_log(path: &Path) -> Option<FoundLog> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path_str = path.to_string_lossy().to_string();
    Some(FoundLog {
        character: character_from_path(&path_str),
        path: path_str,
        modified_secs,
        size_bytes: meta.len(),
        source: "unknown".to_string(),
    })
}

pub fn character_from_path(path: &str) -> Option<String> {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.strip_suffix(".txt").unwrap_or(file);
    let rest = stem.strip_prefix("eqlog_")?;
    let mut parts = rest.splitn(2, '_');
    let name = parts.next()?.to_string();
    let _server = parts.next()?;
    if name.is_empty() {
        return None;
    }
    Some(name)
}

/// Strip `[timestamp]` and return the action text (what triggers match against).
pub fn split_log_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if !line.starts_with('[') {
        return None;
    }
    let close = line.find(']')?;
    let timestamp = line[1..close].trim().to_string();
    let action = line[close + 1..].trim().to_string();
    if action.is_empty() {
        return None;
    }
    Some((timestamp, action))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osxeql_dirs_point_at_wine_prefix_logs() {
        std::env::set_var("HOME", "/Users/test");
        let dirs = osxeql_log_dirs();
        assert_eq!(dirs.len(), 2);
        assert!(dirs[0].ends_with(
            "Library/Application Support/osxEQL/prefix/drive_c/users/Public/Daybreak Game Company/Installed Games/EverQuest Legends/Logs"
        ));
        assert!(dirs[1].ends_with(
            "Library/Application Support/osxEQL/prefix-cx/drive_c/users/Public/Daybreak Game Company/Installed Games/EverQuest Legends/Logs"
        ));
    }
}
