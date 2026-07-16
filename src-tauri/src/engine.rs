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
    /// Captures from the log line that started this timer (GINA `${1}` early-end).
    #[serde(default)]
    pub captures: Vec<String>,
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
    group_name: String,
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
    /// Recent "You begin casting …" names (normalized), for self-only land timers.
    pending_casts: Vec<(String, u64)>,
}

/// Ignore a second fire of the same trigger within this window.
const ALERT_DEBOUNCE_MS: u64 = 2_000;
/// How long a "You begin casting" counts as yours for shared land-emote timers.
const PENDING_CAST_MS: u64 = 8_000;

/// Search already attributes the event to you (cast, your hit, character token).
pub fn is_self_attributed_search(search: &str) -> bool {
    let s = search.trim_start_matches('^');
    s.starts_with("You ")
        || s.starts_with("Your ")
        || s.starts_with("(You ")
        || search.contains("{C}")
        || search.contains("from your ")
}

/// Spell name used in combat logs: strip clicky notes and slash alternates.
pub fn spell_basename(name: &str) -> Option<String> {
    let mut base = name.split(" (").next().unwrap_or(name).trim();
    if let Some((before, _)) = base.split_once('/') {
        base = before.trim();
    }
    if base.is_empty() || base.contains('[') || base.contains('|') {
        return None;
    }
    Some(base.to_string())
}

/// Optional EQL upgrade rank after a spell name (`Plague IV`, `Odium II`).
pub const SPELL_RANK_SUFFIX: &str = r"(?: [IVX]+)?";

/// Your land-hit line for a DoT/spell, matching any upgrade rank.
pub fn you_hit_by_spell_pattern(spell: &str) -> String {
    let escaped = regex::escape(spell);
    format!(
        r"^You hit ([\w -'`]+) for [\d,]+ points of \w+ damage by {escaped}{SPELL_RANK_SUFFIX}\.$"
    )
}

fn normalize_spell_name(name: &str) -> String {
    let mut s = name.trim().to_ascii_lowercase();
    let romans = [
        " xviii", " xvii", " xvi", " xv", " xiv", " xiii", " xii", " xi", " x", " ix",
        " viii", " vii", " vi", " v", " iv", " iii", " ii", " i",
    ];
    for roman in romans {
        if let Some(stripped) = s.strip_suffix(roman) {
            s = stripped.to_string();
            break;
        }
    }
    s
}

/// Cast names that must have been started recently for shared land-emote triggers.
fn required_recent_casts(compiled: &CompiledTrigger) -> Option<Vec<String>> {
    match compiled.trigger.name.as_str() {
        "Slowed" => Some(
            [
                "drowsy",
                "walking sleep",
                "tagar's insects",
                "togor's insects",
                "turgur's insects",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        ),
        "Maloed" => Some(
            ["malo", "malosini", "malise", "malaisement", "malosi"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        _ => {
            let group = compiled.group_name.to_ascii_lowercase();
            if !group.contains("crowd control") {
                return None;
            }
            spell_basename(&compiled.trigger.name).map(|name| vec![name])
        }
    }
}

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
            pending_casts: Vec::new(),
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

    /// Synthetic fire for overlay preview from the trigger editor.
    /// Skips match/debounce/enabled checks. Uses `sample_action` for `{S}` / captures when set.
    pub fn test_fire(&mut self, trigger: &Trigger, sample_action: Option<&str>) -> MatchAction {
        self.prune_expired_timers();
        let now = now_ms();
        let character = self
            .character
            .clone()
            .unwrap_or_else(|| "Character".to_string());
        let action = sample_action.unwrap_or("").trim();

        let caps_owned: Option<Vec<String>> = if action.is_empty() {
            None
        } else if trigger.use_regex && !trigger.search.is_empty() {
            let bound = bind_character_token(&trigger.search, &character, true);
            match Regex::new(&bound) {
                Ok(re) => re.captures(action).map(|c| {
                    c.iter()
                        .skip(1)
                        .map(|m| m.map(|x| x.as_str().to_string()).unwrap_or_default())
                        .collect()
                }),
                Err(_) => None,
            }
        } else {
            Some(Vec::new())
        };

        let display_template = trigger
            .display_text
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&trigger.name);
        let display = expand_tokens(
            display_template,
            &character,
            action,
            caps_owned.as_deref(),
        );

        let speak = if trigger.tts_enabled {
            let template = trigger
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

        // Sound mode only: play chime when set. Explicit "none"/empty = visual only.
        let sound = if !trigger.tts_enabled {
            trigger
                .sound
                .as_deref()
                .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
                .map(|s| s.to_string())
        } else {
            None
        };

        let mut started_timer = None;
        if let Some(secs) = trigger.timer_seconds {
            if secs > 0 {
                let name_template = trigger
                    .timer_name
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| trigger.name.clone());
                let timer_name =
                    expand_tokens(&name_template, &character, action, caps_owned.as_deref());
                self.timers.retain(|t| t.name != timer_name);
                let timer = ActiveTimer {
                    id: format!("t{}", self.next_alert_id),
                    trigger_id: trigger.id.clone(),
                    name: timer_name,
                    started_ms: now,
                    ends_ms: now + secs * 1000,
                    duration_secs: secs,
                    captures: caps_owned.clone().unwrap_or_default(),
                };
                self.next_alert_id += 1;
                started_timer = Some(timer.clone());
                self.timers.push(timer);
            }
        }

        let speak_useful = speak
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let has_explicit_display = trigger
            .display_text
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let show_toast = if started_timer.is_some() {
            has_explicit_display
        } else {
            !display.trim().is_empty()
        };
        let sound_useful = sound
            .as_ref()
            .map(|s| !s.eq_ignore_ascii_case("none"))
            .unwrap_or(false);

        let alert = if show_toast {
            let alert = FiredAlert {
                id: format!("a{}", self.next_alert_id),
                trigger_id: trigger.id.clone(),
                trigger_name: trigger.name.clone(),
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
            Some(alert)
        } else {
            None
        };

        MatchAction {
            alert,
            sound: if sound_useful { sound } else { None },
            speak: if speak_useful { speak } else { None },
            started_timer,
            cleared_timer_ids: vec![],
        }
    }

    /// Drop finished timers. Returns true if any were removed.
    pub fn prune_expired_timers(&mut self) -> bool {
        let now = now_ms();
        let before = self.timers.len();
        self.timers.retain(|t| t.ends_ms > now);
        self.timers.len() != before
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
                    // Skip `${1}` patterns — those substitute timer-start captures at match time.
                    if trigger.use_regex
                        && !early.contains("{C}")
                        && !has_unexpanded_capture(early)
                    {
                        if let Ok(re) = Regex::new(early) {
                            early_regexes.push(re);
                        }
                    }
                }
                compiled.push(CompiledTrigger {
                    group_id: group.id.clone(),
                    group_name: group.name.clone(),
                    trigger: trigger.clone(),
                    group_enabled: group.enabled,
                    regex,
                    early_regexes,
                });
            }
        }
        self.compiled = compiled;
    }

    fn note_begin_casting(&mut self, action: &str, now: u64) {
        let Some(rest) = action.strip_prefix("You begin casting ") else {
            return;
        };
        let Some(name) = rest.strip_suffix('.') else {
            return;
        };
        self.pending_casts
            .retain(|(_, t)| now.saturating_sub(*t) < PENDING_CAST_MS);
        self.pending_casts
            .push((normalize_spell_name(name), now));
    }

    fn has_recent_cast(&self, spell: &str, now: u64) -> bool {
        let want = normalize_spell_name(spell);
        self.pending_casts.iter().any(|(name, t)| {
            if now.saturating_sub(*t) >= PENDING_CAST_MS {
                return false;
            }
            name == &want
                || (want.starts_with("mesmerize")
                    && (name == "dazzle" || name.starts_with("mesmerize")))
                || (name.starts_with("mesmerize") && want.starts_with("mesmerize"))
        })
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

        self.note_begin_casting(action, now);

        // Early-end pass: clear timers when end text matches.
        let mut cleared = Vec::new();
        for compiled in &self.compiled {
            if !compiled.group_enabled || !compiled.trigger.enabled {
                continue;
            }
            if compiled.trigger.early_end.is_empty() {
                continue;
            }
            let timer_name = compiled
                .trigger
                .timer_name
                .clone()
                .unwrap_or_else(|| compiled.trigger.name.clone());

            // Plain early-ends (worn-off, etc.): no GINA capture tokens.
            let matched_plain = if compiled.trigger.use_regex {
                compiled.early_regexes.iter().any(|re| re.is_match(action))
            } else {
                compiled
                    .trigger
                    .early_end
                    .iter()
                    .filter(|s| !s.is_empty() && !has_unexpanded_capture(s))
                    .any(|s| action.contains(s.as_str()))
            };
            if matched_plain {
                // Don't expand early-end regex captures into timer names — alternation
                // groups like (f|fs) would corrupt "Mesmerize - ${1}". Worn-off lines
                // also omit the mob; clear by soonest matching prefix instead.
                for id in timer_ids_to_clear(&self.timers, &timer_name, None) {
                    if !cleared.contains(&id) {
                        cleared.push(id);
                    }
                }
            }

            // GINA `${1}` early-ends (e.g. slain): substitute captures from timer start.
            for early in &compiled.trigger.early_end {
                if early.is_empty() || !has_unexpanded_capture(early) {
                    continue;
                }
                for timer in &self.timers {
                    if timer.trigger_id != compiled.trigger.id {
                        continue;
                    }
                    let caps = early_end_captures(timer, &timer_name);
                    if caps.is_empty() {
                        continue;
                    }
                    if !early_end_pattern_matches(
                        early,
                        compiled.trigger.use_regex,
                        action,
                        &character,
                        &caps,
                    ) {
                        continue;
                    }
                    if !cleared.contains(&timer.id) {
                        cleared.push(timer.id.clone());
                    }
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

            // Shared land emotes (mez, slow yawns, malo) — only fire if you cast.
            if !is_self_attributed_search(&compiled.trigger.search) {
                if let Some(spells) = required_recent_casts(compiled) {
                    let mine = spells
                        .iter()
                        .any(|spell| self.has_recent_cast(spell, now));
                    if !mine {
                        continue;
                    }
                }
            }

            // Debounce noisy combat spam (range / LOS while autoattacking).
            // Timer triggers still run so multi-mob mez clocks start; toast/TTS
            // for the same timer name is debounced after the name is known.
            let is_timer = compiled.trigger.timer_seconds.unwrap_or(0) > 0;
            if !is_timer {
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
            }

            let has_explicit_display = compiled
                .trigger
                .display_text
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let display_template = compiled
                .trigger
                .display_text
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&compiled.trigger.name);
            let display = expand_tokens(
                display_template,
                &character,
                action,
                caps_owned.as_deref(),
            );

            // TTS by default. Speak text falls back to toast/name when empty.
            let mut speak = if compiled.trigger.tts_enabled {
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

            // Chime only when TTS is off (sound mode). Explicit "none" = visual only.
            let mut sound = if !compiled.trigger.tts_enabled {
                compiled
                    .trigger
                    .sound
                    .as_deref()
                    .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
                    .map(|s| s.to_string())
            } else {
                None
            };

            let mut started_timer = None;
            let mut suppress_notice = false;
            if let Some(secs) = compiled.trigger.timer_seconds {
                if secs > 0 {
                    let mut secs = secs;
                    let mut name_template = compiled
                        .trigger
                        .timer_name
                        .clone()
                        .unwrap_or_else(|| compiled.trigger.name.clone());

                    // Dazzle shares "has been mesmerized" with Mesmerize on EQL.
                    let dazzle_pending = self.has_recent_cast("dazzle", now);
                    let is_mesmerize_land = name_template.starts_with("Mesmerize");
                    if dazzle_pending && is_mesmerize_land {
                        secs = 96;
                        name_template = "Dazzle - ${1}".to_string();
                        self.pending_casts.retain(|(n, _)| n != "dazzle");
                    }

                    let timer_name = expand_tokens(
                        &name_template,
                        &character,
                        action,
                        caps_owned.as_deref(),
                    );

                    // EQL often logs miss+hit for one kick. Refresh the clock but
                    // don't toast/TTS again for the same cooldown name.
                    let notice_key =
                        format!("timer:{}:{}", compiled.trigger.id, timer_name);
                    if let Some(prev) = self.last_fire_ms.get(&notice_key) {
                        if now.saturating_sub(*prev) < ALERT_DEBOUNCE_MS {
                            suppress_notice = true;
                        }
                    }
                    self.last_fire_ms.insert(notice_key, now);

                    // Restart same-named timers (GINA-ish default).
                    self.timers.retain(|t| t.name != timer_name);
                    let timer = ActiveTimer {
                        id: format!("t{}", self.next_alert_id),
                        trigger_id: compiled.trigger.id.clone(),
                        name: timer_name,
                        started_ms: now,
                        ends_ms: now + secs * 1000,
                        duration_secs: secs,
                        captures: caps_owned.clone().unwrap_or_default(),
                    };
                    self.next_alert_id += 1;
                    started_timer = Some(timer.clone());
                    self.timers.push(timer);
                }
            }

            if suppress_notice {
                speak = None;
                sound = None;
            }

            let speak_useful = speak
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            // Timer bars are enough for cooldown clocks; only toast when the
            // trigger set an explicit display line (e.g. YOU DIED).
            let show_toast = if started_timer.is_some() {
                has_explicit_display && !suppress_notice
            } else {
                !display.trim().is_empty() && !suppress_notice
            };
            let sound_useful = sound
                .as_ref()
                .map(|s| !s.eq_ignore_ascii_case("none"))
                .unwrap_or(false);
            // Silent match (e.g. Dazzle cast track / early-end-only helper).
            if started_timer.is_none() && !speak_useful && !show_toast && !sound_useful {
                continue;
            }

            let alert = if show_toast {
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
                Some(alert)
            } else {
                None
            };

            actions.push(MatchAction {
                alert,
                sound: if sound_useful { sound } else { None },
                speak: if speak_useful { speak } else { None },
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

        let timer_name = ability.to_string();
        let legacy_names = legacy_cooldown_names(ability);

        // Ready (0 remaining): drop any stuck overlay timer for this ability.
        if remaining == 0 {
            let cleared: Vec<String> = self
                .timers
                .iter()
                .filter(|t| {
                    t.name == timer_name || legacy_names.iter().any(|n| t.name == *n)
                })
                .map(|t| t.id.clone())
                .collect();
            if cleared.is_empty() {
                return None;
            }
            self.timers.retain(|t| !cleared.contains(&t.id));
            return Some(MatchAction {
                alert: None,
                sound: None,
                speak: None,
                started_timer: None,
                cleared_timer_ids: cleared,
            });
        }

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
            captures: vec![],
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

/// Expand capture tokens into a regex pattern, escaping substituted values.
fn expand_tokens_for_regex(
    template: &str,
    character: &str,
    action: &str,
    captures: &[String],
) -> String {
    let escaped: Vec<String> = captures.iter().map(|c| regex::escape(c)).collect();
    expand_tokens(template, character, action, Some(&escaped))
}

fn early_end_captures(timer: &ActiveTimer, template: &str) -> Vec<String> {
    if !timer.captures.is_empty() {
        return timer.captures.clone();
    }
    timer_captures_from_name(template, &timer.name).unwrap_or_default()
}

fn early_end_pattern_matches(
    pattern: &str,
    use_regex: bool,
    action: &str,
    character: &str,
    captures: &[String],
) -> bool {
    if use_regex {
        let expanded = expand_tokens_for_regex(pattern, character, action, captures);
        if has_unexpanded_capture(&expanded) {
            return false;
        }
        return Regex::new(&expanded)
            .ok()
            .map(|re| re.is_match(action))
            .unwrap_or(false);
    }
    let expanded = expand_tokens(pattern, character, action, Some(captures));
    !expanded.is_empty() && action.contains(&expanded)
}

/// Infer `${1}` from timer display names like "Immo - a goblin".
fn timer_captures_from_name(template: &str, name: &str) -> Option<Vec<String>> {
    let prefix = timer_clear_prefix(template);
    if prefix.is_empty() {
        return None;
    }
    let sep = format!("{prefix} - ");
    let rest = name.strip_prefix(&sep)?;
    if rest.is_empty() {
        return None;
    }
    Some(vec![rest.to_string()])
}

fn has_unexpanded_capture(name: &str) -> bool {
    if name.contains("${") {
        return true;
    }
    // GINA-style {1} left unexpanded.
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = name[i + 1..].find('}') {
                let inner = &name[i + 1..i + 1 + end];
                if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                    return true;
                }
                i += 1 + end + 1;
                continue;
            }
        }
        i += 1;
    }
    false
}

/// Prefix used when early-end text has no mob capture (e.g. worn-off).
fn timer_clear_prefix(template: &str) -> String {
    let mut end = template.len();
    if let Some(pos) = template.find("${") {
        end = end.min(pos);
    }
    if let Some(pos) = template.find("$1") {
        end = end.min(pos);
    }
    if let Some(pos) = template.find("{1}") {
        end = end.min(pos);
    }
    template[..end]
        .trim_end_matches([' ', '-', ':'])
        .trim()
        .to_string()
}

fn timer_matches_prefix(name: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    if name == prefix {
        return true;
    }
    name.starts_with(&format!("{prefix} - "))
}

/// Exact name when captures expand the template; otherwise clear the soonest
/// timer matching the template prefix (mez worn-off has no mob name).
fn timer_ids_to_clear(
    timers: &[ActiveTimer],
    template: &str,
    captures: Option<&[String]>,
) -> Vec<String> {
    let expanded = expand_tokens(template, "", "", captures);
    if has_unexpanded_capture(&expanded) {
        let prefix = timer_clear_prefix(template);
        let soonest = timers
            .iter()
            .filter(|t| timer_matches_prefix(&t.name, &prefix))
            .min_by_key(|t| t.ends_ms);
        return soonest.map(|t| vec![t.id.clone()]).unwrap_or_default();
    }
    timers
        .iter()
        .filter(|t| t.name == expanded)
        .map(|t| t.id.clone())
        .collect()
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
    fn test_fire_shows_toast_and_timer() {
        let mut engine = TriggerEngine::new(TriggerLibrary::default());
        engine.set_character(Some("Francis".into()));
        let trigger = Trigger {
            id: "mez".into(),
            name: "Mesmerize".into(),
            enabled: false,
            search: r"^(.+) has been mesmerized\.$".into(),
            use_regex: true,
            display_text: Some("Mezzed ${1}".into()),
            timer_seconds: Some(24),
            timer_name: Some("Mesmerize - ${1}".into()),
            early_end: vec![],
            sound: None,
            speak: Some("Mesmerize on ${1}".into()),
            tts_enabled: true,
            comments: None,
        };
        let action = engine.test_fire(
            &trigger,
            Some("a froglok tuk rider has been mesmerized."),
        );
        let alert = action.alert.expect("alert");
        assert_eq!(alert.text, "Mezzed a froglok tuk rider");
        assert_eq!(
            action.speak.as_deref(),
            Some("Mesmerize on a froglok tuk rider")
        );
        let timer = action.started_timer.expect("timer");
        assert_eq!(timer.name, "Mesmerize - a froglok tuk rider");
        assert_eq!(timer.duration_secs, 24);
        assert_eq!(engine.snapshot().recent_alerts.len(), 1);
        assert_eq!(engine.snapshot().timers.len(), 1);
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
    fn timer_name_expands_capture() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "mez".into(),
                    name: "Mesmerize".into(),
                    enabled: true,
                    search: r"^([\w -'`]+) has been mesmerized\.$".into(),
                    use_regex: true,
                    display_text: Some("".into()),
                    timer_seconds: Some(24),
                    timer_name: Some("Mesmerize - ${1}".into()),
                    early_end: vec![r"^Your Mesmerize spell has worn off\.$".into()],
                    sound: Some("none".into()),
                    speak: None,
                    tts_enabled: false,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        engine.process_action("a goblin has been mesmerized.");
        let timers = engine.snapshot().timers;
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].name, "Mesmerize - a goblin");
        assert_eq!(timers[0].duration_secs, 24);
    }

    #[test]
    fn mez_worn_off_clears_soonest_mob_timer() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "mez".into(),
                    name: "Mesmerize".into(),
                    enabled: true,
                    search: r"^([\w -'`]+) has been mesmerized\.$".into(),
                    use_regex: true,
                    display_text: Some("".into()),
                    timer_seconds: Some(24),
                    timer_name: Some("Mesmerize - ${1}".into()),
                    early_end: vec![r"^Your Mesmerize spell has worn off\.$".into()],
                    sound: Some("none".into()),
                    speak: None,
                    tts_enabled: false,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        engine.process_action("a goblin has been mesmerized.");
        engine.process_action("a beetle has been mesmerized.");
        assert_eq!(engine.snapshot().timers.len(), 2);
        // Goblin's timer started first → soonest to expire.
        engine.process_action("Your Mesmerize spell has worn off.");
        let left = engine.snapshot().timers;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "Mesmerize - a beetle");
    }

    #[test]
    fn slain_early_end_clears_matching_mob_timer() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "immo".into(),
                    name: "Immolate".into(),
                    enabled: true,
                    search: r"^([\w -'`]+) is immolated by flame\.$".into(),
                    use_regex: true,
                    display_text: Some("".into()),
                    timer_seconds: Some(60),
                    timer_name: Some("Immo - ${1}".into()),
                    early_end: vec![
                        r"^(You have slain ${1}|${1} has been slain by (?:[^!]+))\!$".into(),
                    ],
                    sound: Some("none".into()),
                    speak: None,
                    tts_enabled: false,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        engine.process_action("a goblin is immolated by flame.");
        engine.process_action("a beetle is immolated by flame.");
        assert_eq!(engine.snapshot().timers.len(), 2);

        engine.process_action("a goblin has been slain by Labn!");
        let left = engine.snapshot().timers;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "Immo - a beetle");

        engine.process_action("You have slain a beetle!");
        assert!(engine.snapshot().timers.is_empty());
    }

    #[test]
    fn dazzle_cast_overrides_mesmerize_land_duration() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "Classes / Enchanter / Crowd Control".into(),
                enabled: true,
                triggers: vec![
                    Trigger {
                        id: "mez".into(),
                        name: "Mesmerize".into(),
                        enabled: true,
                        search: r"^([\w -'`]+) has been mesmerized\.$".into(),
                        use_regex: true,
                        display_text: Some("".into()),
                        timer_seconds: Some(24),
                        timer_name: Some("Mesmerize - ${1}".into()),
                        early_end: vec![r"^Your Mesmerize spell has worn off\.$".into()],
                        sound: Some("none".into()),
                        speak: None,
                        tts_enabled: false,
                        comments: None,
                    },
                    Trigger {
                        id: "dazzle".into(),
                        name: "Dazzle".into(),
                        enabled: true,
                        search: r"^You begin casting Dazzle\.$".into(),
                        use_regex: true,
                        display_text: Some("".into()),
                        timer_seconds: None,
                        timer_name: Some("Dazzle - ${1}".into()),
                        early_end: vec![r"^Your Dazzle spell has worn off\.$".into()],
                        sound: Some("none".into()),
                        speak: None,
                        tts_enabled: false,
                        comments: None,
                    },
                ],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        engine.process_action("You begin casting Dazzle.");
        engine.process_action("a goblin has been mesmerized.");
        let timers = engine.snapshot().timers;
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].name, "Dazzle - a goblin");
        assert_eq!(timers[0].duration_secs, 96);

        engine.process_action("Your Dazzle spell has worn off.");
        assert!(engine.snapshot().timers.is_empty());
    }

    #[test]
    fn slowed_ignores_other_players_drowsy_yawns() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "warn".into(),
                name: "Classes / Shaman / Warnings".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "eql-shm-slow-landed".into(),
                    name: "Slowed".into(),
                    enabled: true,
                    search: r"^([\w -'`]+)(?: yawns|'s motions slow as a plague of insects chews at their skin)\.$"
                        .into(),
                    use_regex: true,
                    display_text: Some("${1} Slowed".into()),
                    timer_seconds: None,
                    timer_name: None,
                    early_end: vec![],
                    sound: None,
                    speak: Some("${1} Slowed".into()),
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);

        // Party Drowsy — shared yawns line must not alert.
        assert!(engine
            .process_action("a shin ghoul knight yawns.")
            .is_empty());

        engine.process_action("You begin casting Togor's Insects IV.");
        let yours = engine.process_action("a vampire bat yawns.");
        assert_eq!(yours.len(), 1);
        assert_eq!(yours[0].alert.as_ref().map(|a| a.text.as_str()), Some("a vampire bat Slowed"));
    }

    #[test]
    fn crowd_control_ignores_other_players_mez() {
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "cc".into(),
                name: "Classes / Enchanter / Crowd Control".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "mez".into(),
                    name: "Mesmerize/Mesmerization".into(),
                    enabled: true,
                    search: r"^([\w -'`]+) has been mesmerized\.$".into(),
                    use_regex: true,
                    display_text: None,
                    timer_seconds: Some(24),
                    timer_name: Some("Mesmerize - ${1}".into()),
                    early_end: vec![],
                    sound: Some("none".into()),
                    speak: None,
                    tts_enabled: false,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);

        assert!(engine
            .process_action("a shin ghoul knight has been mesmerized.")
            .is_empty());
        assert!(engine.snapshot().timers.is_empty());

        engine.process_action("You begin casting Mesmerize IV.");
        let yours = engine.process_action("a shin ghoul knight has been mesmerized.");
        assert_eq!(yours.len(), 1);
        assert_eq!(
            yours[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Mesmerize - a shin ghoul knight")
        );
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

    #[test]
    fn clears_ability_timer_when_cooldown_reports_ready() {
        let mut engine = TriggerEngine::new(TriggerLibrary { groups: vec![] });
        engine.process_action(
            "You can use the ability Mend again in 1 minute(s) 8 seconds.",
        );
        assert_eq!(engine.snapshot().timers.len(), 1);
        assert_eq!(engine.snapshot().timers[0].name, "Mend");

        let cleared = engine.process_action(
            "You can use the ability Mend again in 0 minute(s) 0 seconds.",
        );
        assert_eq!(cleared.len(), 1);
        assert!(cleared[0].started_timer.is_none());
        assert_eq!(cleared[0].cleared_timer_ids.len(), 1);
        assert!(engine.snapshot().timers.is_empty());
    }

    fn flying_kick_trigger() -> Trigger {
        Trigger {
            id: "24fc739023d9-2".into(),
            name: "Flying Kick Cooldown".into(),
            enabled: true,
            search: "(You kick .+? for \\d+ poin(t|ts) of damage.|You try to kick .+?, but (miss|(.+? (ripostes|dodges|parries)|.+?'s magical skin absorbs the blow))!)".into(),
            use_regex: true,
            display_text: None,
            timer_seconds: Some(4),
            timer_name: Some("Flying Kick!".into()),
            early_end: vec![],
            sound: None,
            speak: Some("Flying Kick Cooldown".into()),
            tts_enabled: true,
            comments: None,
        }
    }

    #[test]
    fn kick_miss_then_hit_speaks_once_and_no_toast() {
        // EQL logs miss + hit for one Flying Kick; only one callout / timer.
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "monk".into(),
                name: "Monk".into(),
                enabled: true,
                triggers: vec![flying_kick_trigger()],
            }],
        };
        let mut engine = TriggerEngine::new(lib);

        let miss = engine
            .process_action("You try to kick a vis ghoul knight, but miss!");
        assert_eq!(miss.len(), 1);
        assert!(miss[0].alert.is_none(), "cooldown clocks use the timer bar");
        assert_eq!(miss[0].speak.as_deref(), Some("Flying Kick Cooldown"));
        assert_eq!(
            miss[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Flying Kick!")
        );

        let hit = engine.process_action(
            "You kick a vis ghoul knight for 161 points of damage. (Critical)",
        );
        assert_eq!(hit.len(), 1);
        assert!(hit[0].alert.is_none());
        assert!(hit[0].speak.is_none(), "second line must not TTS again");
        assert_eq!(
            hit[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Flying Kick!")
        );
        assert_eq!(engine.snapshot().timers.len(), 1);
        assert!(engine.snapshot().recent_alerts.is_empty());
    }

    #[test]
    fn different_timer_names_still_announce() {
        // Multi-mob mez: each target is a different timer name → each speaks.
        let lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "g".into(),
                name: "G".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "mez".into(),
                    name: "Mesmerize".into(),
                    enabled: true,
                    search: r"^(.+) has been mesmerized\.$".into(),
                    use_regex: true,
                    display_text: Some("Mezzed ${1}".into()),
                    timer_seconds: Some(24),
                    timer_name: Some("Mesmerize - ${1}".into()),
                    early_end: vec![],
                    sound: None,
                    speak: Some("Mesmerize on ${1}".into()),
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        let mut engine = TriggerEngine::new(lib);
        let a = engine.process_action("a beetle has been mesmerized.");
        let b = engine.process_action("a rat has been mesmerized.");
        assert_eq!(a[0].speak.as_deref(), Some("Mesmerize on a beetle"));
        assert_eq!(b[0].speak.as_deref(), Some("Mesmerize on a rat"));
        assert_eq!(a[0].alert.as_ref().map(|x| x.text.as_str()), Some("Mezzed a beetle"));
        assert_eq!(b[0].alert.as_ref().map(|x| x.text.as_str()), Some("Mezzed a rat"));
        assert_eq!(engine.snapshot().timers.len(), 2);
    }
}
