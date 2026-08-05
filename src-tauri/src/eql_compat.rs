//! EverQuest Legends differences vs classic GINA packs.

use crate::engine::TriggerLibrary;
use serde::Deserialize;

const PERMANENT_BUFFS_JSON: &str =
    include_str!("../../samples/eql_permanent_buffs.json");

#[derive(Deserialize)]
struct PermanentBuffsFile {
    spells: Vec<String>,
}

fn permanent_names() -> Vec<String> {
    let file: PermanentBuffsFile =
        serde_json::from_str(PERMANENT_BUFFS_JSON).unwrap_or(PermanentBuffsFile {
            spells: vec![],
        });
    file.spells
        .into_iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn trigger_base_name(name: &str) -> String {
    name.split('(').next().unwrap_or(name).trim().to_ascii_lowercase()
}

pub fn is_permanent_buff_name(name: &str) -> bool {
    let base = trigger_base_name(name);
    permanent_names().iter().any(|p| p == &base)
}

/// Clear classic countdown timers for buffs that are permanent on Legends,
/// and silence cast alerts (no toast / TTS) — they never expire.
pub fn strip_permanent_buff_timers(library: &mut TriggerLibrary) -> usize {
    let permanent = permanent_names();
    if permanent.is_empty() {
        return 0;
    }

    let mut changed = 0usize;
    let note = "EQL: permanent buff — classic timer removed; alerts off";

    for group in &mut library.groups {
        for trigger in &mut group.triggers {
            let base = trigger_base_name(&trigger.name);
            if !permanent.iter().any(|p| p == &base) {
                continue;
            }
            let before = serde_json::to_string(trigger).unwrap_or_default();
            trigger.timer_seconds = None;
            trigger.timer_name = None;
            trigger.display_text = None;
            trigger.speak = None;
            trigger.sound = None;
            trigger.tts_enabled = false;
            trigger.enabled = false;
            match &mut trigger.comments {
                Some(existing) if existing.contains("permanent buff") => {
                    if !existing.contains("alerts off") {
                        *existing = note.to_string();
                    }
                }
                Some(existing) => {
                    if existing.trim().is_empty() {
                        *existing = note.to_string();
                    } else {
                        existing.push('\n');
                        existing.push_str(note);
                    }
                }
                None => trigger.comments = Some(note.to_string()),
            }
            let after = serde_json::to_string(trigger).unwrap_or_default();
            if before != after {
                changed += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Trigger, TriggerGroup};

    #[test]
    fn strips_yaulp_keeps_yaulp_iv() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "Paladin".into(),
                enabled: true,
                triggers: vec![
                    Trigger {
                        id: "1".into(),
                        name: "Yaulp".into(),
                        enabled: true,
                        search: "a".into(),
                        use_regex: false,
                        display_text: Some("Yaulp".into()),
                        timer_seconds: Some(24),
                        timer_name: Some("Yaulp".into()),
                        early_end: vec![],
                        sound: None,
                        speak: Some("Yaulp".into()),
                        tts_enabled: true,
                        comments: None,
                    },
                    Trigger {
                        id: "2".into(),
                        name: "Yaulp IV".into(),
                        enabled: true,
                        search: "b".into(),
                        use_regex: false,
                        display_text: Some("Yaulp IV".into()),
                        timer_seconds: Some(24),
                        timer_name: Some("Yaulp IV".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                ],
            }],
        };
        assert_eq!(strip_permanent_buff_timers(&mut lib), 1);
        let yaulp = &lib.groups[0].triggers[0];
        assert!(yaulp.timer_seconds.is_none());
        assert!(!yaulp.enabled);
        assert!(!yaulp.tts_enabled);
        assert!(yaulp.speak.is_none());
        assert_eq!(lib.groups[0].triggers[1].timer_seconds, Some(24));
        assert!(lib.groups[0].triggers[1].enabled);
    }
}
