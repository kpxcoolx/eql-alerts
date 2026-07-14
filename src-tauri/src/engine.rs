use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerLibrary {
    pub groups: Vec<TriggerGroup>,
}

impl Default for TriggerLibrary {
    fn default() -> Self {
        Self { groups: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerGroup {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
}

impl Default for TriggerGroup {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "New group".to_string(),
            enabled: true,
            triggers: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Trigger {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Plain substring or regex (matched against action text after timestamp).
    pub search: String,
    pub use_regex: bool,
    pub display_text: Option<String>,
    pub timer_seconds: Option<u64>,
    pub timer_name: Option<String>,
    /// Matching any of these strings clears timers with the same timer_name.
    pub early_end: Vec<String>,
    /// Optional relative path or absolute path to a .wav / browser-playable file.
    pub sound: Option<String>,
    /// Spoken aloud via system TTS (from GINA TextToVoice).
    pub speak: Option<String>,
    /// When true (default), fire TTS. Turn off to use chime/sound instead.
    pub tts_enabled: bool,
    pub comments: Option<String>,
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "New trigger".to_string(),
            enabled: true,
            search: String::new(),
            use_regex: false,
            display_text: None,
            timer_seconds: None,
            timer_name: None,
            early_end: vec![],
            sound: None,
            speak: None,
            tts_enabled: true,
            comments: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiredAlert {
    pub id: String,
    pub trigger_id: String,
    pub trigger_name: String,
    pub kind: String,
    pub text: String,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTimer {
    pub id: String,
    pub trigger_id: String,
    pub name: String,
    pub started_ms: u64,
    pub ends_ms: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    pub character: Option<String>,
    pub log_path: Option<String>,
    pub monitoring: bool,
    pub recent_alerts: Vec<FiredAlert>,
    pub timers: Vec<ActiveTimer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchAction {
    pub alert: Option<FiredAlert>,
    pub sound: Option<String>,
    pub speak: Option<String>,
    pub started_timer: Option<ActiveTimer>,
    pub cleared_timer_ids: Vec<String>,
}

struct CompiledTrigger {
    group_id: String,
    trigger: Trigger,
    group_enabled: bool,
    regex: Option<Regex>,
    early_regexes: Vec<Regex>,
}

pub struct TriggerEngine {
    library: TriggerLibrary,
    compiled: Vec<CompiledTrigger>,
    character: Option<String>,
    log_path: Option<String>,
    monitoring: bool,
    recent_alerts: Vec<FiredAlert>,
    timers: Vec<ActiveTimer>,
    next_alert_id: u64,
    /// Suppress duplicate alert spam for the same trigger (melee range spam, etc.).
    last_fire_ms: HashMap<String, u64>,
}

/// Ignore a second fire of the same trigger within this window.
const ALERT_DEBOUNCE_MS: u64 = 2_000;

impl TriggerEngine {
    pub fn new(library: TriggerLibrary) -> Self {
        let mut engine = Self {
            library: TriggerLibrary { groups: vec![] },
            compiled: vec![],
            character: None,
            log_path: None,
            monitoring: false,
            recent_alerts: vec![],
            timers: vec![],
            next_alert_id: 1,
            last_fire_ms: HashMap::new(),
        };
        engine.set_library(library);
        engine
    }

    pub fn library(&self) -> &TriggerLibrary {
        &self.library
    }

    pub fn set_library(&mut self, library: TriggerLibrary) {
        self.library = library;
        self.recompile();
    }

    /// Flip group enable flags without recompiling regexes (fast path for class chips).
    pub fn set_groups_enabled(&mut self, ids: &[String], enabled: bool) {
        use std::collections::HashSet;
        let id_set: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
        for group in &mut self.library.groups {
            if id_set.contains(group.id.as_str()) {
                group.enabled = enabled;
            }
        }
        for compiled in &mut self.compiled {
            if id_set.contains(compiled.group_id.as_str()) {
                compiled.group_enabled = enabled;
            }
        }
    }

    pub fn set_character(&mut self, name: Option<String>) {
        self.character = name;
    }

    pub fn set_log_path(&mut self, path: Option<String>) {
        self.log_path = path;
    }

    pub fn set_monitoring(&mut self, monitoring: bool) {
        self.monitoring = monitoring;
    }

    pub fn clear_timers(&mut self) {
        self.timers.clear();
    }

    pub fn clear_timer(&mut self, timer_id: &str) -> bool {
        let before = self.timers.len();
        self.timers.retain(|t| t.id != timer_id);
        self.timers.len() != before
    }

    pub fn clear_alerts(&mut self) {
        self.recent_alerts.clear();
    }

    pub fn prune_expired_timers(&mut self) {
        let now = now_ms();
        self.timers.retain(|t| t.ends_ms > now);
    }

    pub fn snapshot(&self) -> EngineState {
        EngineState {
            character: self.character.clone(),
            log_path: self.log_path.clone(),
            monitoring: self.monitoring,
            recent_alerts: self.recent_alerts.clone(),
            timers: self.timers.clone(),
        }
    }

    fn recompile(&mut self) {
        let mut compiled = Vec::new();
        for group in &self.library.groups {
            for trigger in &group.triggers {
                // Patterns with {C} bind the character name at match time.
                let regex = if trigger.use_regex
                    && !trigger.search.is_empty()
                    && !trigger.search.contains("{C}")
                {
                    Regex::new(&trigger.search).ok()
                } else {
                    None
                };
                let mut early_regexes = Vec::new();
                for early in &trigger.early_end {
                    if trigger.use_regex && !early.contains("{C}") {
                        if let Ok(re) = Regex::new(early) {
                            early_regexes.push(re);
                        }
                    }
                }
                compiled.push(CompiledTrigger {
                    group_id: group.id.clone(),
                    trigger: trigger.clone(),
                    group_enabled: group.enabled,
                    regex,
                    early_regexes,
                });
            }
        }
        self.compiled = compiled;
    }

    /// Process one log action line (already stripped of timestamp).
    pub fn process_action(&mut self, action: &str) -> Vec<MatchAction> {
        self.prune_expired_timers();
        let mut actions = Vec::new();
        let now = now_ms();
        let character = self.character.clone().unwrap_or_default();

        // EQL logs real ability remaining CD when you press while on cooldown:
        // "You can use the ability Lay on Hands again in 15 minute(s) 0 seconds."
        if let Some(synced) = self.sync_ability_cooldown_from_log(action, now) {
            actions.push(synced);
        }

        // Early-end pass: clear timers when end text matches.
        let mut cleared = Vec::new();
        for compiled in &self.compiled {
            if !compiled.group_enabled || !compiled.trigger.enabled {
                continue;
            }
            if compiled.trigger.early_end.is_empty() {
                continue;
            }
            let matched = if compiled.trigger.use_regex {
                compiled
                    .early_regexes
                    .iter()
                    .any(|re| re.is_match(action))
            } else {
                compiled
                    .trigger
                    .early_end
                    .iter()
                    .any(|s| !s.is_empty() && action.contains(s.as_str()))
            };
            if !matched {
                continue;
            }
            let timer_name = compiled
                .trigger
                .timer_name
                .clone()
                .unwrap_or_else(|| compiled.trigger.name.clone());
            for timer in &self.timers {
                if timer.name == timer_name {
                    cleared.push(timer.id.clone());
                }
            }
        }
        if !cleared.is_empty() {
            self.timers.retain(|t| !cleared.contains(&t.id));
            actions.push(MatchAction {
                alert: None,
                sound: None,
                speak: None,
                started_timer: None,
                cleared_timer_ids: cleared,
            });
        }

        for compiled in &self.compiled {
            if !compiled.group_enabled || !compiled.trigger.enabled {
                continue;
            }
            if compiled.trigger.search.is_empty() {
                continue;
            }

            let caps_owned: Option<Vec<String>> = if compiled.trigger.use_regex {
                let bound = bind_character_token(&compiled.trigger.search, &character, true);
                let re_owned;
                let re = if compiled.trigger.search.contains("{C}") {
                    re_owned = Regex::new(&bound).ok();
                    re_owned.as_ref()
                } else {
                    compiled.regex.as_ref()
                };
                match re {
                    Some(re) => re.captures(action).map(|c| {
                        c.iter()
                            .skip(1)
                            .map(|m| m.map(|x| x.as_str().to_string()).unwrap_or_default())
                            .collect()
                    }),
                    None => None,
                }
            } else {
                let bound = bind_character_token(&compiled.trigger.search, &character, false);
                if action.contains(&bound) {
                    Some(Vec::new())
                } else {
                    None
                }
            };

            if caps_owned.is_none() {
                continue;
            }

            // Debounce noisy combat spam (range / LOS while autoattacking).
            let debounce_ms = if compiled.trigger.id.contains("out-of-range")
                || compiled.trigger.id.contains("los")
            {
                5_000
            } else {
                ALERT_DEBOUNCE_MS
            };
            if let Some(prev) = self.last_fire_ms.get(&compiled.trigger.id) {
                if now.saturating_sub(*prev) < debounce_ms {
                    continue;
                }
            }
            self.last_fire_ms
                .insert(compiled.trigger.id.clone(), now);

            let display_template = compiled
                .trigger
                .display_text
                .as_deref()
                .unwrap_or(&compiled.trigger.name);
            let display = expand_tokens(
                display_template,
                &character,
                action,
                caps_owned.as_deref(),
            );

            // TTS by default. Speak text falls back to toast/name when empty.
            let speak = if compiled.trigger.tts_enabled {
                let template = compiled
                    .trigger
                    .speak
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(display_template);
                Some(expand_tokens(
                    template,
                    &character,
                    action,
                    caps_owned.as_deref(),
                ))
            } else {
                None
            };

            // Chime only when TTS is off (sound mode).
            let sound = if !compiled.trigger.tts_enabled {
                let s = compiled
                    .trigger
                    .sound
                    .as_deref()
                    .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
                    .unwrap_or("ping");
                Some(s.to_string())
            } else {
                None
            };

            let mut started_timer = None;
            if let Some(secs) = compiled.trigger.timer_seconds {
                if secs > 0 {
                    let timer_name = compiled
                        .trigger
                        .timer_name
                        .clone()
                        .unwrap_or_else(|| compiled.trigger.name.clone());
                    // Restart same-named timers (GINA-ish default).
                    self.timers.retain(|t| t.name != timer_name);
                    let timer = ActiveTimer {
                        id: format!("t{}", self.next_alert_id),
                        trigger_id: compiled.trigger.id.clone(),
                        name: timer_name,
                        started_ms: now,
                        ends_ms: now + secs * 1000,
                        duration_secs: secs,
                    };
                    self.next_alert_id += 1;
                    started_timer = Some(timer.clone());
                    self.timers.push(timer);
                }
            }

            let alert = FiredAlert {
                id: format!("a{}", self.next_alert_id),
                trigger_id: compiled.trigger.id.clone(),
                trigger_name: compiled.trigger.name.clone(),
                kind: if started_timer.is_some() {
                    "timer".to_string()
                } else {
                    "text".to_string()
                },
                text: display,
                at_ms: now,
            };
            self.next_alert_id += 1;
            self.recent_alerts.insert(0, alert.clone());
            if self.recent_alerts.len() > 40 {
                self.recent_alerts.truncate(40);
            }

            actions.push(MatchAction {
                alert: Some(alert),
                sound,
                speak,
                started_timer,
                cleared_timer_ids: vec![],
            });
        }

        actions
    }

    /// Keep overlay timers aligned with the game's stated ability remaining time.
    fn sync_ability_cooldown_from_log(&mut self, action: &str, now: u64) -> Option<MatchAction> {
        let re = ability_cooldown_re();
        let caps = re.captures(action)?;
        let ability = caps.get(1)?.as_str().trim();
        if ability.is_empty() {
            return None;
        }
        let mins: u64 = caps.get(2)?.as_str().parse().ok()?;
        let secs: u64 = caps.get(3)?.as_str().parse().ok()?;
        let remaining = mins.saturating_mul(60).saturating_add(secs);
        if remaining == 0 {
            return None;
        }

        let timer_name = ability.to_string();
        let legacy_names = legacy_cooldown_names(ability);

        // Avoid spam updates when mash-firing the same remaining second.
        if let Some(existing) = self.timers.iter().find(|t| {
            t.name == timer_name || legacy_names.iter().any(|n| t.name == *n)
        }) {
            let left = (existing.ends_ms.saturating_sub(now) + 999) / 1000;
            if left == remaining {
                return None;
            }
        }

        let duration_secs = self
            .timers
            .iter()
            .find(|t| t.name == timer_name || legacy_names.iter().any(|n| t.name == *n))
            .map(|t| t.duration_secs.max(remaining))
            .unwrap_or(remaining);

        self.timers
            .retain(|t| t.name != timer_name && legacy_names.iter().all(|n| t.name != *n));

        let timer = ActiveTimer {
            id: format!("t{}", self.next_alert_id),
            trigger_id: "eql-ability-cooldown".into(),
            name: timer_name,
            started_ms: now,
            ends_ms: now + remaining * 1000,
            duration_secs,
        };
        self.next_alert_id += 1;
        self.timers.push(timer.clone());

        Some(MatchAction {
            alert: None,
            sound: None,
            speak: None,
            started_timer: Some(timer),
            cleared_timer_ids: vec![],
        })
    }
}

fn ability_cooldown_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^You can use the ability (.+) again in (\d+) minute\(s\) (\d+) seconds?\.$",
        )
        .expect("ability cooldown regex")
    })
}

fn legacy_cooldown_names(ability: &str) -> Vec<&'static str> {
    if ability == "Lay on Hands" {
        vec!["Lay Hands Cooldown", "Lay Hands"]
    } else {
        vec![]
    }
}

fn bind_character_token(pattern: &str, character: &str, for_regex: bool) -> String {
    if !pattern.contains("{C}") {
        return pattern.to_string();
    }
    if for_regex {
        pattern.replace("{C}", &regex::escape(character))
    } else {
        pattern.replace("{C}", character)
    }
}

fn expand_tokens(
    template: &str,
    character: &str,
    action: &str,
    captures: Option<&[String]>,
) -> String {
    let mut out = template
        .replace("{C}", character)
        .replace("{S}", action)
        .replace("{L}", action);

    if let Some(caps) = captures {
        // Prefer the first numeric capture for {N} (GINA digit token).
        let mut number = None;
        for cap in caps {
            if !cap.is_empty() && cap.chars().all(|c| c.is_ascii_digit()) {
                number = Some(cap.as_str());
                break;
            }
        }
        if let Some(n) = number {
            out = out.replace("{N}", n);
        } else if let Some(first) = caps.first() {
            out = out.replace("{N}", first);
        }

        for (i, cap) in caps.iter().enumerate() {
            let idx = i + 1;
            out = out.replace(&format!("${{{idx}}}"), cap);
            out = out.replace(&format!("${idx}"), cap);
            out = out.replace(&format!("{{{idx}}}"), cap);
        }
    }

    out
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_search_fires_text() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "zoning".into(),
                    name: "Zoning".into(),
                    enabled: true,
                    search: "LOADING, PLEASE WAIT...".into(),
                    use_regex: false,
                    display_text: Some("Zoning…".into()),
                    timer_seconds: None,
                    timer_name: None,
                    early_end: vec![],
                    sound: None,
                    speak: None,
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        let actions = engine.process_action("LOADING, PLEASE WAIT...");
        assert!(actions.iter().any(|a| {
            a.alert
                .as_ref()
                .map(|al| al.text.contains("Zoning"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn early_end_clears_timer() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "fear".into(),
                    name: "Fear".into(),
                    enabled: true,
                    search: "begins to cast".into(),
                    use_regex: false,
                    display_text: Some("FEAR".into()),
                    timer_seconds: Some(30),
                    timer_name: Some("Fear".into()),
                    early_end: vec!["fear has worn off".into()],
                    sound: None,
                    speak: None,
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        engine.process_action("A mob begins to cast");
        assert_eq!(engine.snapshot().timers.len(), 1);
        engine.process_action("Your fear has worn off");
        assert!(engine.snapshot().timers.is_empty());
    }

    #[test]
    fn speak_expands_numeric_capture() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "kick".into(),
                    name: "Kick".into(),
                    enabled: true,
                    search: r"You kick .+? for (\d+) points of damage\.".into(),
                    use_regex: true,
                    display_text: Some("Kick".into()),
                    timer_seconds: None,
                    timer_name: None,
                    early_end: vec![],
                    sound: None,
                    speak: Some("{N}".into()),
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        let actions = engine.process_action("You kick a gnoll for 42 points of damage.");
        assert_eq!(actions[0].speak.as_deref(), Some("42"));
    }

    #[test]
    fn character_bound_pet_death() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "pet".into(),
                    name: "Pet".into(),
                    enabled: true,
                    search: r"^{C}`s .+ has been slain by".into(),
                    use_regex: true,
                    display_text: Some("PET DIED".into()),
                    timer_seconds: None,
                    timer_name: None,
                    early_end: vec![],
                    sound: None,
                    speak: Some("Pet died".into()),
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        engine.set_character(Some("Kenkyo".into()));
        let hit = engine.process_action("Kenkyo`s warder has been slain by a wan ghoul knight!");
        assert_eq!(hit[0].speak.as_deref(), Some("Pet died"));
        let miss = engine.process_action("Fright pet has been slain by Labn!");
        assert!(miss.is_empty());
    }

    #[test]
    fn stun_does_not_match_cant_cast_while_stunned() {
        let mut lib = crate::starter::starter_pack();
        for g in &mut lib.groups {
            g.enabled = g.id == "eql-essentials-danger";
            for t in &mut g.triggers {
                t.enabled = t.id == "eql-essentials-stunned";
            }
        }
        let mut engine = TriggerEngine::new(lib);
        let false_pos = engine.process_action("You can't cast spells while stunned!");
        assert!(
            !false_pos.iter().any(|a| a.alert.is_some()),
            "substring stun must not match cast-blocked line"
        );
        let hit = engine.process_action("You are stunned!");
        assert!(hit.iter().any(|a| {
            a.speak.as_deref() == Some("Stunned")
                || a.alert
                    .as_ref()
                    .map(|x| x.text.contains("STUNNED"))
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn debounce_suppresses_rapid_repeats() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "range".into(),
                    name: "Range".into(),
                    enabled: true,
                    search: "too far away".into(),
                    use_regex: false,
                    display_text: Some("Out of range".into()),
                    timer_seconds: None,
                    timer_name: None,
                    early_end: vec![],
                    sound: None,
                    speak: Some("Out of range".into()),
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        let first = engine.process_action("Your target is too far away, get closer!");
        let second = engine.process_action("Your target is too far away, get closer!");
        assert_eq!(first.iter().filter(|a| a.alert.is_some()).count(), 1);
        assert_eq!(second.iter().filter(|a| a.alert.is_some()).count(), 0);
    }

    #[test]
    fn syncs_ability_cooldown_from_eql_log() {
        let mut engine = TriggerEngine::new(TriggerLibrary { groups: vec![] });
        let actions = engine.process_action(
            "You can use the ability Lay on Hands again in 15 minute(s) 0 seconds.",
        );
        assert_eq!(actions.len(), 1);
        let timer = actions[0].started_timer.as_ref().unwrap();
        assert_eq!(timer.name, "Lay on Hands");
        assert_eq!(timer.duration_secs, 900);
        assert!(actions[0].alert.is_none());
        assert!(actions[0].speak.is_none());

        // Same remaining second → no re-emit noise.
        let again = engine.process_action(
            "You can use the ability Lay on Hands again in 15 minute(s) 0 seconds.",
        );
        assert!(again.is_empty());

        // Tick down → update timer quietly.
        let updated = engine.process_action(
            "You can use the ability Lay on Hands again in 14 minute(s) 59 seconds.",
        );
        assert_eq!(updated.len(), 1);
        assert_eq!(
            updated[0].started_timer.as_ref().unwrap().duration_secs,
            900
        );
        let left = (updated[0].started_timer.as_ref().unwrap().ends_ms
            .saturating_sub(now_ms())
            + 999)
            / 1000;
        assert_eq!(left, 14 * 60 + 59);
    }
}
