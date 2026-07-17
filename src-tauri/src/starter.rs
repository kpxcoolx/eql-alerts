//! Built-in starter trigger library for EverQuest Legends.

use crate::engine::{Trigger, TriggerGroup, TriggerLibrary};
use crate::eql_compat::strip_permanent_buff_timers;

const STARTER_JSON: &str = include_str!("../../samples/eql_starter.triggers.json");

/// Class packs + current classic EQL Raids (groups start disabled).
pub fn starter_pack() -> TriggerLibrary {
    let mut pack: TriggerLibrary =
        serde_json::from_str(STARTER_JSON).unwrap_or_else(|_| TriggerLibrary { groups: vec![] });

    // Classic GINA timers for Legends-permanent buffs (Yaulp, Divine Might, …).
    strip_permanent_buff_timers(&mut pack);

    ensure_essentials(&mut pack);
    let _ = ensure_shaman_warnings(&mut pack);
    let _ = ensure_shaman_dots(&mut pack);
    let _ = ensure_shaman_buffs(&mut pack);
    let _ = ensure_eql_mez_timers(&mut pack);
    let _ = ensure_divine_invulnerability(&mut pack);
    let _ = ensure_eql_disease_dot_timers(&mut pack);
    let _ = ensure_default_tts(&mut pack);
    pack
}

fn t(
    id: &str,
    name: &str,
    search: &str,
    display: Option<&str>,
    speak: Option<&str>,
    sound: Option<&str>,
    comments: Option<&str>,
) -> Trigger {
    Trigger {
        id: id.to_string(),
        name: name.to_string(),
        enabled: true,
        search: search.to_string(),
        use_regex: false,
        display_text: display.map(|s| s.to_string()),
        timer_seconds: None,
        timer_name: None,
        early_end: vec![],
        sound: sound.map(|s| s.to_string()),
        speak: speak.map(|s| s.to_string()),
        tts_enabled: true,
        comments: comments.map(|s| s.to_string()),
    }
}

fn t_regex(
    id: &str,
    name: &str,
    search: &str,
    display: Option<&str>,
    speak: Option<&str>,
    sound: Option<&str>,
    comments: Option<&str>,
) -> Trigger {
    let mut trigger = t(id, name, search, display, speak, sound, comments);
    trigger.use_regex = true;
    trigger
}

fn t_timer(
    id: &str,
    name: &str,
    search: &str,
    display: Option<&str>,
    speak: Option<&str>,
    sound: Option<&str>,
    timer_seconds: u64,
    timer_name: &str,
    early_end: Vec<&str>,
    comments: Option<&str>,
) -> Trigger {
    let mut trigger = t(id, name, search, display, speak, sound, comments);
    trigger.timer_seconds = Some(timer_seconds);
    trigger.timer_name = Some(timer_name.to_string());
    trigger.early_end = early_end.into_iter().map(|s| s.to_string()).collect();
    trigger
}

fn t_timer_regex(
    id: &str,
    name: &str,
    search: &str,
    display: Option<&str>,
    speak: Option<&str>,
    sound: Option<&str>,
    timer_seconds: u64,
    timer_name: &str,
    early_end: Vec<&str>,
    comments: Option<&str>,
) -> Trigger {
    let mut trigger = t_timer(
        id,
        name,
        search,
        display,
        speak,
        sound,
        timer_seconds,
        timer_name,
        early_end,
        comments,
    );
    trigger.use_regex = true;
    trigger
}

/** Opt-in-within-category: group may be armed, but this trigger stays off until toggled. */
fn off(mut trigger: Trigger) -> Trigger {
    trigger.enabled = false;
    if let Some(note) = &mut trigger.comments {
        if !note.contains("opt-in") {
            note.push_str(" · opt-in (off by default)");
        }
    } else {
        trigger.comments = Some("opt-in (off by default)".to_string());
    }
    trigger
}

/// Built-in always-on categories under `EQL Essentials / …`.
fn essentials_groups() -> Vec<TriggerGroup> {
    vec![
        TriggerGroup {
            id: "eql-essentials-core".to_string(),
            name: "EQL Essentials / Core".to_string(),
            enabled: true,
            triggers: vec![
                t(
                    "eql-essentials-zoning",
                    "Zoning",
                    "LOADING, PLEASE WAIT...",
                    Some("Zoning…"),
                    None,
                    None,
                    Some("Core — always useful while learning"),
                ),
                t_timer(
                    "eql-essentials-slain",
                    "You have been slain",
                    "You have been slain by",
                    Some("YOU DIED"),
                    Some("You died"),
                    Some("sosumi"),
                    6,
                    "Death",
                    vec![],
                    None,
                ),
                t_regex(
                    "eql-essentials-fizzle",
                    "Spell fizzle",
                    r"^Your(?: .+)? spell fizzles!$",
                    Some("Fizzle"),
                    Some("Fizzle"),
                    Some("tink"),
                    Some("Only your fizzles — classic Your spell fizzles! and EQL Your <Spell> spell fizzles!"),
                ),
                t_regex(
                    "eql-essentials-oom",
                    "Insufficient Mana",
                    r"^Insufficient Mana to cast this spell!$",
                    Some("Out of mana!"),
                    Some("Out of mana"),
                    Some("ping"),
                    Some("Anchored full-line match — casters / hybrids"),
                ),
                t_timer(
                    "eql-essentials-low-health",
                    "Low health",
                    "You are bleeding to death!",
                    Some("LOW HEALTH"),
                    Some("Low health"),
                    Some("submarine"),
                    5,
                    "Low Health",
                    vec![],
                    Some("Classic EQ low-HP warning"),
                ),
                t_timer(
                    "eql-essentials-low-pet-health",
                    "Low pet health",
                    "I have 50 percent of my hit points left",
                    Some("PET LOW HEALTH"),
                    Some("Pet low health"),
                    Some("submarine"),
                    5,
                    "Low Pet Health",
                    vec![],
                    Some("Pet HP report at 50% (classic /pet health line)"),
                ),
                t_regex(
                    "eql-essentials-pet-died",
                    "Your pet died",
                    r"^{C}`s .+ has been slain by",
                    Some("PET DIED"),
                    Some("Pet died"),
                    Some("bottle"),
                    Some("EQL pet ownership line (mage / necro / beastlord / …)"),
                ),
                t(
                    "eql-essentials-pet-died-classic",
                    "Your pet died (classic line)",
                    "Your pet has been slain",
                    Some("PET DIED"),
                    Some("Pet died"),
                    Some("bottle"),
                    Some("Fallback if the client emits Your pet has been slain"),
                ),
            ],
        },
        TriggerGroup {
            id: "eql-essentials-combat".to_string(),
            name: "EQL Essentials / Combat".to_string(),
            enabled: true,
            triggers: vec![
                t_regex(
                    "eql-essentials-interrupted",
                    "Spell interrupted",
                    r"^Your(?: .+)? spell is interrupted\.$",
                    Some("Interrupted"),
                    Some("Interrupted"),
                    None,
                    Some("Only your interrupts — classic Your spell is interrupted. and EQL Your <Spell> spell is interrupted."),
                ),
                t(
                    "eql-essentials-did-not-take",
                    "Spell did not take hold",
                    "Your spell did not take hold.",
                    Some("Didn't take"),
                    Some("Did not take hold"),
                    Some("tink"),
                    None,
                ),
                t_regex(
                    "eql-essentials-resisted",
                    "Target resisted",
                    r"^Your target resisted the .+ spell\.$",
                    Some("Resisted"),
                    Some("Resisted"),
                    Some("tink"),
                    None,
                ),
                off(t(
                    "eql-essentials-must-stand",
                    "Must be standing",
                    "You must be standing to cast a spell.",
                    Some("Stand up"),
                    Some("Stand up"),
                    Some("ping"),
                    Some("Off by default — turn on TTS or switch to sound in the editor"),
                )),
                off(t_regex(
                    "eql-essentials-los",
                    "Can't see target",
                    r"^You can't see your target from here\.$",
                    Some("No LOS"),
                    Some("Can't see target"),
                    Some("ping"),
                    Some("Off by default (melee spam)"),
                )),
                off(t_regex(
                    "eql-essentials-out-of-range",
                    "Target too far",
                    r"^Your target is too far away, get closer!$",
                    Some("Out of range"),
                    Some("Out of range"),
                    Some("ping"),
                    Some("Off by default (melee spam) — TTS if you arm it"),
                )),
                t_regex(
                    "eql-essentials-enrage",
                    "Enrage",
                    r"^([\w -'`]+) has become ENRAGED\.$",
                    Some("ENRAGED"),
                    Some("Enraged"),
                    Some("sosumi"),
                    Some("Raid / named target enrage"),
                ),
                t_regex(
                    "eql-essentials-enrage-over",
                    "Enrage over",
                    r"^([\w -'`]+) is no longer enraged\.$",
                    Some("Rage over"),
                    Some("Rage over"),
                    Some("glass"),
                    None,
                ),
            ],
        },
        TriggerGroup {
            id: "eql-essentials-danger".to_string(),
            name: "EQL Essentials / Danger".to_string(),
            enabled: true,
            triggers: vec![
                t_timer_regex(
                    "eql-essentials-stunned",
                    "Stunned",
                    r"^You are stunned!$",
                    Some("STUNNED"),
                    Some("Stunned"),
                    Some("sosumi"),
                    8,
                    "Stunned",
                    vec![
                        r"^You are unstunned\.$",
                        r"^You are no longer stunned\.$",
                    ],
                    Some("Exact line only — do not match “can't cast … while stunned”"),
                ),
                t(
                    "eql-essentials-feared",
                    "Feared",
                    "You flee in terror.",
                    Some("FEARED"),
                    Some("Feared"),
                    Some("sosumi"),
                    Some("Dragon roar / fear lands"),
                ),
                t_regex(
                    "eql-essentials-slowed",
                    "Slowed",
                    r"^You (slow down|feel drowsy)\.$",
                    Some("SLOWED"),
                    Some("Slowed"),
                    Some("sosumi"),
                    Some("Attack slow on you — cure it"),
                ),
                t_regex(
                    "eql-essentials-slow-broke",
                    "Slow broke",
                    r"^Your speed returns to normal\.$",
                    Some("Slow free"),
                    Some("Slow broke"),
                    Some("glass"),
                    None,
                ),
                t_regex(
                    "eql-essentials-rooted",
                    "Rooted",
                    r"^Your feet (adhere to the ground|become entwined)\.$",
                    Some("ROOTED"),
                    Some("Rooted"),
                    Some("ping"),
                    None,
                ),
                t_regex(
                    "eql-essentials-root-broke",
                    "Root broke",
                    r"^(The roots fall from your feet|Your feet come free)\.$",
                    Some("Root free"),
                    Some("Root broke"),
                    Some("glass"),
                    None,
                ),
                t(
                    "eql-essentials-charmed",
                    "Charmed",
                    "You have been charmed.",
                    Some("CHARMED"),
                    Some("Charmed"),
                    Some("sosumi"),
                    None,
                ),
                t(
                    "eql-essentials-silenced",
                    "Silenced",
                    "You *CANNOT* cast spells, you have been silenced!",
                    Some("SILENCED"),
                    Some("Silenced"),
                    Some("sosumi"),
                    None,
                ),
                off(t_regex(
                    "eql-essentials-dispelled",
                    "Dispelled",
                    r"^You feel (((a bit|very) dispelled)|dispelled|annulled)\.$",
                    Some("Dispelled"),
                    Some("Dispelled"),
                    Some("ping"),
                    None,
                )),
                t(
                    "eql-essentials-drowning",
                    "Drowning",
                    "You are drowning!",
                    Some("DROWNING"),
                    Some("Drowning"),
                    Some("submarine"),
                    None,
                ),
                off(t(
                    "eql-essentials-encumbered",
                    "Encumbered",
                    "You are encumbered!",
                    Some("Encumbered"),
                    Some("Encumbered"),
                    Some("ping"),
                    None,
                )),
            ],
        },
        TriggerGroup {
            id: "eql-essentials-fades".to_string(),
            name: "EQL Essentials / Fades".to_string(),
            enabled: true,
            triggers: vec![
                t(
                    "eql-essentials-invis-fading",
                    "Invis fading",
                    "You feel yourself starting to appear.",
                    Some("Invis fading"),
                    Some("Invis fading"),
                    Some("ping"),
                    None,
                ),
                off(t_regex(
                    "eql-essentials-invis-faded",
                    "Invis faded",
                    r"^(You appear|Your (shadows fade|skin stops tingling))\.$",
                    Some("Invis faded"),
                    Some("Invis faded"),
                    Some("tink"),
                    None,
                )),
                t(
                    "eql-essentials-levitate-fading",
                    "Levitate fading",
                    "You feel as if you are about to fall.",
                    Some("Falling"),
                    Some("Levitate fading"),
                    Some("sosumi"),
                    None,
                ),
            ],
        },
        TriggerGroup {
            id: "eql-essentials-social".to_string(),
            name: "EQL Essentials / Social".to_string(),
            enabled: false,
            triggers: vec![
                t_regex(
                    "eql-essentials-group-invite",
                    "Group invite",
                    r"^([\w]+) invites you to join a group\.$",
                    Some("Group invite"),
                    Some("Invited to group"),
                    Some("glass"),
                    None,
                ),
                t_regex(
                    "eql-essentials-raid-invite",
                    "Raid invite",
                    r"^([\w]+) invites you to join a raid\.$",
                    Some("Raid invite"),
                    Some("Invited to raid"),
                    Some("glass"),
                    None,
                ),
                t_regex(
                    "eql-essentials-linkdead",
                    "Group linkdead",
                    r"^([\w]+) has gone Linkdead\.$",
                    Some("Linkdead"),
                    Some("Linkdead"),
                    Some("ping"),
                    None,
                ),
                t(
                    "eql-essentials-out-of-charges",
                    "Out of charges",
                    "Item is out of charges.",
                    Some("Out of charges"),
                    Some("Out of charges"),
                    Some("tink"),
                    None,
                ),
                t(
                    "eql-essentials-out-of-ammo",
                    "Out of ammo",
                    "You have run out of ammo!",
                    Some("OUT OF AMMO"),
                    Some("Out of ammo"),
                    Some("ping"),
                    None,
                ),
            ],
        },
    ]
}

fn is_legacy_essentials(group: &TriggerGroup) -> bool {
    group.id == "eql-essentials" || group.name == "EQL Essentials"
}

fn is_essentials_group(group: &TriggerGroup) -> bool {
    group.id.starts_with("eql-essentials") || group.name.starts_with("EQL Essentials")
}

fn is_core_essentials(group: &TriggerGroup) -> bool {
    group.id == "eql-essentials-core" || group.name == "EQL Essentials / Core"
}

/// Categories that should stay armed for gameplay (Social is opt-in).
fn is_gameplay_essentials(group: &TriggerGroup) -> bool {
    matches!(
        group.id.as_str(),
        "eql-essentials-core"
            | "eql-essentials-combat"
            | "eql-essentials-danger"
            | "eql-essentials-fades"
    ) || matches!(
        group.name.as_str(),
        "EQL Essentials / Core"
            | "EQL Essentials / Combat"
            | "EQL Essentials / Danger"
            | "EQL Essentials / Fades"
    )
}

/// Insert missing built-in essentials categories / triggers.
/// Existing user edits are kept; Restore defaults rewrites stock packs.
pub fn ensure_essentials(library: &mut TriggerLibrary) -> usize {
    let fresh = essentials_groups();
    let mut changed = 0usize;

    // Drop the old flat "EQL Essentials" group if present (migrated into Core).
    let before_len = library.groups.len();
    library.groups.retain(|g| !is_legacy_essentials(g));
    if library.groups.len() != before_len {
        changed += 1;
    }

    for fresh_group in fresh.iter().rev() {
        let existing_idx = library
            .groups
            .iter()
            .position(|g| g.id == fresh_group.id || g.name == fresh_group.name);

        if let Some(idx) = existing_idx {
            let existing = &mut library.groups[idx];
            existing.id = fresh_group.id.clone();
            existing.name = fresh_group.name.clone();
            if is_gameplay_essentials(fresh_group) {
                existing.enabled = true;
            }

            for trigger in &fresh_group.triggers {
                if existing.triggers.iter().any(|t| t.id == trigger.id) {
                    // Leave user edits alone (name, patterns, TTS, …).
                    // Restore defaults is the path back to stock essentials.
                    continue;
                }
                existing.triggers.push(trigger.clone());
                changed += 1;
            }
        } else {
            changed += fresh_group.triggers.len();
            library.groups.insert(0, fresh_group.clone());
        }
    }

    // Keep essentials categories at the front in category order.
    let mut essentials = Vec::new();
    let mut rest = Vec::new();
    for group in library.groups.drain(..) {
        if is_essentials_group(&group) && !is_legacy_essentials(&group) {
            essentials.push(group);
        } else {
            rest.push(group);
        }
    }
    essentials.sort_by_key(|g| {
        fresh
            .iter()
            .position(|f| f.id == g.id)
            .unwrap_or(usize::MAX)
    });
    for g in &mut essentials {
        if is_gameplay_essentials(g) {
            g.enabled = true;
        }
    }
    library.groups = essentials;
    library.groups.append(&mut rest);

    changed
}

/// One-shot: apply gameplay defaults (arm Core/Combat/Danger/Fades, quiet spammy triggers).
pub fn apply_gameplay_essentials_defaults(library: &mut TriggerLibrary) -> usize {
    let fresh = essentials_groups();
    let mut changed = 0usize;
    let _ = ensure_essentials(library);

    for fresh_group in &fresh {
        let Some(existing) = library
            .groups
            .iter_mut()
            .find(|g| g.id == fresh_group.id || g.name == fresh_group.name)
        else {
            continue;
        };
        if existing.enabled != fresh_group.enabled {
            existing.enabled = fresh_group.enabled;
            changed += 1;
        }
        for fresh_trigger in &fresh_group.triggers {
            if let Some(slot) = existing
                .triggers
                .iter_mut()
                .find(|t| t.id == fresh_trigger.id)
            {
                if slot.enabled != fresh_trigger.enabled {
                    slot.enabled = fresh_trigger.enabled;
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Turn off non-Core essentials categories (opt-in packs).
pub fn demote_optional_essentials(library: &mut TriggerLibrary) -> usize {
    let mut changed = 0usize;
    for group in &mut library.groups {
        if !is_essentials_group(group) || is_legacy_essentials(group) || is_core_essentials(group) {
            continue;
        }
        if group.enabled {
            group.enabled = false;
            changed += 1;
        }
    }
    changed
}

/// Tiny placeholder from early builds — replace with the real starter pack.
pub fn is_placeholder_library(lib: &TriggerLibrary) -> bool {
    if lib.groups.is_empty() {
        return true;
    }
    if lib.groups.len() == 1 {
        let g = &lib.groups[0];
        if g.id == "general" || g.name == "General" {
            return true;
        }
    }
    false
}

/// Refresh EQL-specific ability timer defaults that classic GINA packs get wrong.
pub fn ensure_eql_ability_timers(library: &mut TriggerLibrary) -> usize {
    let mut changed = 0usize;
    for group in &mut library.groups {
        for trigger in &mut group.triggers {
            if trigger.id == "eql-essentials-interrupted" {
                let before = serde_json::to_string(trigger).unwrap_or_default();
                // Old plain "spell is interrupted" matched every NPC/player interrupt in zone.
                if trigger.search == "spell is interrupted" {
                    trigger.search = r"^Your(?: .+)? spell is interrupted\.$".to_string();
                    trigger.use_regex = true;
                    trigger.comments = Some(
                        "Only your interrupts — classic Your spell is interrupted. and EQL Your <Spell> spell is interrupted."
                            .to_string(),
                    );
                }
                trigger.speak = Some("Interrupted".to_string());
                trigger.display_text = Some("Interrupted".to_string());
                trigger.sound = None;
                let after = serde_json::to_string(trigger).unwrap_or_default();
                if before != after {
                    changed += 1;
                }
            }
            if trigger.id == "eql-essentials-fizzle" {
                let before = serde_json::to_string(trigger).unwrap_or_default();
                if trigger.search == "spell fizzles!" {
                    trigger.search = r"^Your(?: .+)? spell fizzles!$".to_string();
                    trigger.use_regex = true;
                    trigger.comments = Some(
                        "Only your fizzles — classic Your spell fizzles! and EQL Your <Spell> spell fizzles!"
                            .to_string(),
                    );
                }
                let after = serde_json::to_string(trigger).unwrap_or_default();
                if before != after {
                    changed += 1;
                }
            }
            let is_loh = trigger.id == "80a4144d8e7f-1"
                || trigger.name == "Lay Hands Cooldown"
                || trigger.timer_name.as_deref() == Some("Lay Hands Cooldown");
            if is_loh {
                let before = serde_json::to_string(trigger).unwrap_or_default();
                trigger.search = "^You healed .+ by Lay on Hands".to_string();
                trigger.use_regex = true;
                trigger.display_text = Some("Lay on Hands".to_string());
                trigger.timer_seconds = Some(900);
                trigger.timer_name = Some("Lay on Hands".to_string());
                trigger.early_end.clear();
                if trigger.speak.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                    trigger.speak = Some("Lay on Hands".to_string());
                }
                trigger.comments = Some(
                    "EQL base is 15m at early ranks (not classic 72m). Overlay also syncs from “You can use the ability … again in …” when you press early."
                        .to_string(),
                );
                let after = serde_json::to_string(trigger).unwrap_or_default();
                if before != after {
                    changed += 1;
                }
            }

            let is_mend = trigger.id == "24fc739023d9-1" || trigger.name == "Mend Cooldown";
            if is_mend {
                let before = serde_json::to_string(trigger).unwrap_or_default();
                trigger.timer_seconds = Some(90);
                trigger.timer_name = Some("Mend".to_string());
                trigger.comments = Some(
                    "EQL base is 1m 30s (not classic 6m). Overlay also syncs from “You can use the ability … again in …” when you press early."
                        .to_string(),
                );
                let after = serde_json::to_string(trigger).unwrap_or_default();
                if before != after {
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Add shaman slow/malo land + resist alerts if the Warnings group is missing them.
pub fn ensure_shaman_warnings(library: &mut TriggerLibrary) -> usize {
    let fresh = shaman_warning_triggers();
    let Some(idx) = library.groups.iter().position(|g| {
        g.id == "class-b61ac75a50"
            || g.name == "Classes / Shaman / Warnings"
            || g.name.contains("Shaman / Warnings")
    }) else {
        return 0;
    };

    let mut changed = 0usize;
    let existing = &mut library.groups[idx];
    for trigger in fresh {
        if existing.triggers.iter().any(|t| t.id == trigger.id) {
            continue;
        }
        // Insert land/resist before matching wore-off so alerts read in cast order.
        let insert_at = existing
            .triggers
            .iter()
            .position(|t| {
                (trigger.id.contains("slow") && t.id == "837cfbac69da-0")
                    || (trigger.id.contains("malo") && t.id == "837cfbac69da-1")
            })
            .unwrap_or(existing.triggers.len());
        existing.triggers.insert(insert_at, trigger);
        changed += 1;
    }
    changed
}

fn shaman_warning_triggers() -> Vec<Trigger> {
    vec![
        t_regex(
            "eql-shm-slow-landed",
            "Slowed",
            r"^([\w -'`]+)(?: yawns|'s motions slow as a plague of insects chews at their skin)\.$",
            Some("${1} Slowed"),
            Some("${1} Slowed"),
            None,
            Some("EQL: yawns is shared with others' Drowsy — engine only fires after your slow cast"),
        ),
        t_regex(
            "eql-shm-slow-resisted",
            "Slow Resisted",
            r"^Your target resisted the (Walking Sleep|.+? Insects) spell\.$",
            Some("Slow resisted"),
            Some("Slow resisted"),
            None,
            None,
        ),
        t_regex(
            "eql-shm-malo-landed",
            "Maloed",
            r"^([\w -'`]+) looks very uncomfortable\.$",
            Some("${1} Maloed"),
            Some("${1} Maloed"),
            None,
            Some("Malo / Malosini / Malise land"),
        ),
        t_regex(
            "eql-shm-malo-resisted",
            "Malo Resisted",
            r"^Your target resisted the Mal(ise|aisement|osi|o|osini) spell\.$",
            Some("Malo resisted"),
            Some("Malo resisted"),
            None,
            None,
        ),
        t_regex(
            "eql-shm-incap-landed",
            "Incapacitate",
            r"^([\w -'`]+) looks frail\.$",
            Some("${1} Incapacitated"),
            Some("${1} Incapacitated"),
            None,
            Some("EQL: looks frail is shared — engine only fires after your Incapacitate cast"),
        ),
        t_regex(
            "eql-shm-incap-resisted",
            "Incapacitate Resisted",
            r"^Your target resisted the Incapacitate spell\.$",
            Some("Incapacitate resisted"),
            Some("Incapacitate resisted"),
            None,
            None,
        ),
    ]
}

/// Align Enchanter mez durations / early-ends with EverQuest Legends wiki values.
pub fn ensure_eql_mez_timers(library: &mut TriggerLibrary) -> usize {
    let mut changed = 0usize;

    // Find Crowd Control group (starter or gina_pack naming).
    let cc_idx = library.groups.iter().position(|g| {
        let n = g.name.to_ascii_lowercase();
        n.contains("enchanter") && n.contains("crowd control")
    });

    for group in &mut library.groups {
        for trigger in &mut group.triggers {
            let before = serde_json::to_string(trigger).unwrap_or_default();
            let name = trigger.name.as_str();

            if name == "Glamour of Kintaz" || trigger.timer_name.as_deref() == Some("Kintaz - ${1}")
            {
                trigger.timer_seconds = Some(30);
                ensure_early_end(
                    trigger,
                    r"^Your Glamour of Kintaz spell has worn off\.$",
                );
                if trigger.comments.is_none() {
                    trigger.comments = Some("EQL: 30s (5 ticks), not classic 54s".into());
                }
            } else if name == "Rapture"
                && trigger.timer_name.as_deref() == Some("Rapture - ${1}")
            {
                trigger.timer_seconds = Some(24);
                ensure_early_end(trigger, r"^Your Rapture spell has worn off\.$");
                if trigger.comments.is_none() {
                    trigger.comments = Some("EQL: 24s (4 ticks), not classic 42s".into());
                }
            } else if name == "Dictate" && trigger.timer_name.as_deref() == Some("Dictate") {
                trigger.timer_seconds = Some(48);
                ensure_early_end(trigger, r"^Your Dictate spell has worn off\.$");
            } else if name == "Entrance" {
                if !trigger
                    .timer_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("${1}")
                {
                    trigger.timer_name = Some("Entrance - ${1}".into());
                }
                ensure_early_end(
                    trigger,
                    r"^Your Entrance spell has worn off of ${1}\.$",
                );
                ensure_early_end(trigger, r"^Your Entrance spell has worn off\.$");
            } else if name == "Enthrall" {
                if !trigger
                    .timer_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("${1}")
                {
                    trigger.timer_name = Some("Enthrall - ${1}".into());
                }
                ensure_early_end(
                    trigger,
                    r"^Your Enthrall spell has worn off of ${1}\.$",
                );
                ensure_early_end(trigger, r"^Your Enthrall spell has worn off\.$");
            } else if name == "Fascination" {
                if !trigger
                    .timer_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("${1}")
                {
                    trigger.timer_name = Some("Fascination - ${1}".into());
                }
                // Land search stays "fascinated" (wiki); EQL AE uses mesmerized + engine remap.
                ensure_early_end(
                    trigger,
                    r"^Your Fascination spell has worn off of ${1}\.$",
                );
                ensure_early_end(trigger, r"^Your Fascination spell has worn off\.$");
                if trigger.comments.is_none() {
                    trigger.comments = Some(
                        "EQL AE: land is often 'mesmerized' — engine remaps after Fascination cast"
                            .into(),
                    );
                }
            } else if name.starts_with("Mesmerize") {
                if !trigger
                    .timer_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("${1}")
                {
                    trigger.timer_name = Some("Mesmerize - ${1}".into());
                }
                ensure_early_end(
                    trigger,
                    r"^Your (Mesmerize|Mesmerization) spell has worn off of ${1}\.$",
                );
                ensure_early_end(
                    trigger,
                    r"^Your (Mesmerize|Mesmerization) spell has worn off\.$",
                );
            } else if name == "Dazzle" {
                if !trigger
                    .timer_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("${1}")
                {
                    trigger.timer_name = Some("Dazzle - ${1}".into());
                }
                ensure_early_end(
                    trigger,
                    r"^Your Dazzle spell has worn off of ${1}\.$",
                );
                ensure_early_end(trigger, r"^Your Dazzle spell has worn off\.$");
            } else if name == "Rapture Cooldown" {
                trigger.timer_seconds = Some(24);
            } else if name == "Dictate Cooldown" {
                trigger.timer_seconds = Some(300);
            }

            let after = serde_json::to_string(trigger).unwrap_or_default();
            if before != after {
                changed += 1;
            }
        }
    }

    // Ensure silent Dazzle cast-track + worn-off clear exists in CC group.
    if let Some(idx) = cc_idx {
        let has_dazzle = library.groups[idx]
            .triggers
            .iter()
            .any(|t| t.id == "eql-dazzle-mez" || t.name == "Dazzle");
        if !has_dazzle {
            library.groups[idx].triggers.push(Trigger {
                id: "eql-dazzle-mez".into(),
                name: "Dazzle".into(),
                enabled: true,
                search: r"^You begin casting Dazzle\.$".into(),
                use_regex: true,
                display_text: Some("".into()),
                timer_seconds: None,
                timer_name: Some("Dazzle - ${1}".into()),
                early_end: vec![
                    r"^Your Dazzle spell has worn off of ${1}\.$".into(),
                    r"^Your Dazzle spell has worn off\.$".into(),
                ],
                sound: Some("none".into()),
                speak: None,
                tts_enabled: false,
                comments: Some(
                    "EQL: 96s via Mesmerize land line after this cast. Clears Dazzle timers on worn-off."
                        .into(),
                ),
            });
            changed += 1;
        }
    }

    changed
}


/// Cleric/Paladin Divine Aura & Barrier: ensure 18s duration timers + EQL land lines.
pub fn ensure_divine_invulnerability(library: &mut TriggerLibrary) -> usize {
    let mut changed = 0usize;
    let aura_duration = Trigger {
        id: "eql-divine-aura-duration".into(),
        name: "Divine Aura".into(),
        enabled: true,
        search: r"^(The gods have rendered you invulnerable|You are surrounded by a divine aura)\.$"
            .into(),
        use_regex: true,
        display_text: None,
        timer_seconds: Some(18),
        timer_name: Some("Divine Aura".into()),
        early_end: vec![
            r"^Your (aura fades|invulnerability fades)\.$".into(),
            r"^You have been slain by (?:[^!]+)\!$".into(),
        ],
        sound: None,
        speak: Some("Divine Aura".into()),
        tts_enabled: true,
        comments: Some("EQL: 18s duration; 900s CD is a separate trigger".into()),
    };
    let barrier_duration = Trigger {
        id: "eql-divine-barrier-duration".into(),
        name: "Divine Barrier".into(),
        enabled: true,
        search: r"^You are surrounded by a divine barrier\.$".into(),
        use_regex: true,
        display_text: None,
        timer_seconds: Some(18),
        timer_name: Some("Divine Barrier".into()),
        early_end: vec![
            r"^The barrier fades\.$".into(),
            r"^You have been slain by (?:[^!]+)\!$".into(),
        ],
        sound: None,
        speak: Some("Divine Barrier".into()),
        tts_enabled: true,
        comments: Some("EQL: 18s duration; 900s CD is a separate trigger".into()),
    };
    let aura_cd_search = r"^(The gods have rendered you invulnerable|You are surrounded by a divine aura|You have finished memorizing Divine Aura)\.$";

    for group in &mut library.groups {
        let n = group.name.to_ascii_lowercase();
        let is_invuln = n.contains("invulnerability")
            && (n.contains("cleric") || n.contains("paladin"));
        if !is_invuln {
            continue;
        }

        for trigger in &mut group.triggers {
            let before = serde_json::to_string(trigger).unwrap_or_default();
            if trigger.name == "Divine Aura Cooldown" {
                trigger.search = aura_cd_search.into();
            }
            if trigger.name == "Divine Barrier" && trigger.timer_seconds == Some(18) {
                ensure_early_end(trigger, r"^The barrier fades\.$");
                ensure_early_end(trigger, r"^You have been slain by (?:[^!]+)\!$");
                trigger.search = r"^You are surrounded by a divine barrier\.$".into();
            }
            let after = serde_json::to_string(trigger).unwrap_or_default();
            if before != after {
                changed += 1;
            }
        }

        let has_aura_dur = group.triggers.iter().any(|t| {
            t.id == "eql-divine-aura-duration"
                || (t.name == "Divine Aura" && t.timer_seconds == Some(18))
        });
        if !has_aura_dur {
            group.triggers.push(aura_duration.clone());
            changed += 1;
        }

        if n.contains("cleric") {
            let has_barrier_dur = group.triggers.iter().any(|t| {
                t.id == "eql-divine-barrier-duration"
                    || t.id == "073b8ff8f8e8-1"
                    || (t.name == "Divine Barrier" && t.timer_seconds == Some(18))
            });
            if !has_barrier_dur {
                group.triggers.push(barrier_duration.clone());
                changed += 1;
            }
        }
    }

    changed
}

fn ensure_early_end(trigger: &mut Trigger, pattern: &str) {
    if !trigger.early_end.iter().any(|e| e == pattern) {
        trigger.early_end.insert(0, pattern.to_string());
    }
}

/// Shared DoT emotes fire for any caster (you, pet, mob, other players).
/// Rewrite Damage Over Time timers to your land-hit line so clocks start only for you.
/// Also tolerate EQL upgrade ranks (`by Plague IV.`).
/// Odium is excluded — it lands on a shared emote and is cast-gated instead.
pub fn ensure_eql_disease_dot_timers(library: &mut TriggerLibrary) -> usize {
    let mut changed = 0usize;
    for group in &mut library.groups {
        if !group
            .name
            .to_ascii_lowercase()
            .contains("damage over time")
        {
            continue;
        }
        for trigger in &mut group.triggers {
            if trigger.timer_seconds.unwrap_or(0) == 0 {
                continue;
            }
            let Some(spell) = crate::engine::spell_basename(&trigger.name) else {
                continue;
            };
            if spell.eq_ignore_ascii_case("Odium") {
                continue;
            }
            let desired = crate::engine::you_hit_by_spell_pattern(&spell);
            if trigger.search == desired {
                continue;
            }
            let is_land = trigger.search.contains("You hit") && trigger.search.contains(" by ");
            let is_shared = !crate::engine::is_self_attributed_search(&trigger.search);
            if !is_land && !is_shared {
                continue;
            }
            let before = serde_json::to_string(trigger).unwrap_or_default();
            trigger.search = desired;
            trigger.use_regex = true;
            if trigger.comments.is_none() {
                trigger.comments = Some(
                    "EQL: match your land hit — shared emotes fire for any caster; ranks OK"
                        .into(),
                );
            }
            let after = serde_json::to_string(trigger).unwrap_or_default();
            if before != after {
                changed += 1;
            }
        }
    }
    changed
}

fn odium_land_pattern() -> &'static str {
    r"^([\w -'`]+) staggers under a dark curse\.$"
}

/// Shaman DoTs that belong in the class pack (Odium / Plague / Envenomed Bolt are EQL shaman spells).
pub fn ensure_shaman_dots(library: &mut TriggerLibrary) -> usize {
    let group_name = "Classes / Shaman / Damage Over Time";
    let Some(idx) = library
        .groups
        .iter()
        .position(|g| g.name == group_name || g.name.eq_ignore_ascii_case(group_name))
    else {
        return 0;
    };

    let early_end = vec![
        r"^(You have slain ${1}|${1} has been slain by (?:[^!]+))\!$".to_string(),
        r"^cleardots? is not online at this time\.$".to_string(),
    ];

    let mut changed = 0usize;

    // Patch existing Odium / Plague so TTS and Odium land matching stay correct.
    for trigger in &mut library.groups[idx].triggers {
        if trigger.id == "eql-shaman-odium" || spell_basename_eq(&trigger.name, "Odium") {
            let before = serde_json::to_string(trigger).unwrap_or_default();
            trigger.search = odium_land_pattern().into();
            trigger.use_regex = true;
            trigger.display_text = Some("${1} Odium".into());
            trigger.speak = Some("${1} Odium".into());
            trigger.tts_enabled = true;
            if trigger.timer_seconds.unwrap_or(0) == 0 {
                trigger.timer_seconds = Some(30);
            }
            if trigger.timer_name.is_none() {
                trigger.timer_name = Some("Odium - ${1}".into());
            }
            trigger.comments = Some(
                "EQL: dark curse emote is shared — engine only fires after your Odium cast"
                    .into(),
            );
            let after = serde_json::to_string(trigger).unwrap_or_default();
            if before != after {
                changed += 1;
            }
            continue;
        }
        if trigger.id == "eql-shaman-plague" || spell_basename_eq(&trigger.name, "Plague") {
            let before = serde_json::to_string(trigger).unwrap_or_default();
            trigger.display_text = Some("${1} Plagued".into());
            trigger.speak = Some("${1} Plagued".into());
            trigger.tts_enabled = true;
            let after = serde_json::to_string(trigger).unwrap_or_default();
            if before != after {
                changed += 1;
            }
        }
    }

    let specs: Vec<(&str, &str, u64, &str, Option<&str>, &str, &str)> = vec![
        (
            "eql-shaman-odium",
            "Odium",
            30,
            "Odium - ${1}",
            Some("${1} Odium"),
            odium_land_pattern(),
            "EQL: dark curse emote is shared — engine only fires after your Odium cast",
        ),
        (
            "eql-shaman-envenomed-bolt",
            "Envenomed Bolt",
            48,
            "Envenomed Bolt - ${1}",
            Some("${1} Envenom Bolted"),
            "",
            "EQL: match your land hit — shared emotes fire for any caster; ranks OK",
        ),
        (
            "eql-shaman-plague",
            "Plague",
            132,
            "Plague - ${1}",
            Some("${1} Plagued"),
            "",
            "EQL: match your land hit — shared emotes fire for any caster; ranks OK",
        ),
    ];

    for (id, name, secs, timer_name, display, search, comments) in specs {
        let exists = library.groups[idx]
            .triggers
            .iter()
            .any(|t| t.name == name || spell_basename_eq(&t.name, name));
        if exists {
            continue;
        }
        let search = if search.is_empty() {
            crate::engine::you_hit_by_spell_pattern(name)
        } else {
            search.to_string()
        };
        library.groups[idx].triggers.push(Trigger {
            id: id.into(),
            name: name.into(),
            enabled: true,
            search,
            use_regex: true,
            display_text: display.map(str::to_string),
            timer_seconds: Some(secs),
            timer_name: Some(timer_name.into()),
            early_end: early_end.clone(),
            sound: None,
            speak: display.map(str::to_string),
            tts_enabled: true,
            comments: Some(comments.into()),
        });
        changed += 1;
    }

    changed
}


/// Ensure Spirit of the Puma self-buff timer exists in Shaman Buffs / Self.
pub fn ensure_shaman_buffs(library: &mut TriggerLibrary) -> usize {
    let Some(idx) = library.groups.iter().position(|g| {
        g.id == "class-68ef1f9bea"
            || g.name == "Classes / Shaman / Buffs / Self"
            || g.name.contains("Shaman / Buffs / Self")
    }) else {
        return 0;
    };

    let trigger = shaman_puma_trigger();
    let existing = &mut library.groups[idx];
    if existing
        .triggers
        .iter()
        .any(|t| t.id == trigger.id || t.name == trigger.name)
    {
        return 0;
    }

    // Place near Spirit of Cheetah when present.
    let insert_at = existing
        .triggers
        .iter()
        .position(|t| t.name == "Spirit of Cheetah")
        .map(|i| i + 1)
        .unwrap_or(existing.triggers.len());
    existing.triggers.insert(insert_at, trigger);
    1
}

fn shaman_puma_trigger() -> Trigger {
    t_timer_regex(
        "eql-shm-spirit-of-the-puma",
        "Spirit of the Puma",
        r"^You begin to snarl as your features become feline\.$",
        None,
        Some("Puma"),
        None,
        85,
        "Spirit of the Puma",
        vec![
            r"^The spirit of the puma departs\.$",
            r"^You have been slain by (?:[^!]+)\!$",
            r"^clearbuffs? is not online at this time\.$",
        ],
        Some("EQL: ~85s self proc buff (not wiki 60s)"),
    )
}

fn spell_basename_eq(trigger_name: &str, want: &str) -> bool {
    crate::engine::spell_basename(trigger_name)
        .map(|b| b.eq_ignore_ascii_case(want))
        .unwrap_or(false)
}

/// Fill missing speak lines so every trigger can announce via TTS by default.
pub fn ensure_default_tts(library: &mut TriggerLibrary) -> usize {
    let pack: TriggerLibrary =
        serde_json::from_str(STARTER_JSON).unwrap_or_else(|_| TriggerLibrary { groups: vec![] });
    let mut by_id = std::collections::HashMap::new();
    for group in &pack.groups {
        for trigger in &group.triggers {
            if let Some(speak) = &trigger.speak {
                if !speak.is_empty() {
                    by_id.insert(trigger.id.clone(), speak.clone());
                }
            }
        }
    }
    // Essentials live in code, not the JSON pack — include those speaks too.
    for group in essentials_groups() {
        for trigger in group.triggers {
            if let Some(speak) = trigger.speak {
                if !speak.is_empty() {
                    by_id.insert(trigger.id, speak);
                }
            }
        }
    }

    let mut changed = 0usize;
    for group in &mut library.groups {
        for trigger in &mut group.triggers {
            let needs_speak = trigger
                .speak
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true);
            if !needs_speak {
                continue;
            }
            if let Some(speak) = by_id.get(&trigger.id) {
                trigger.speak = Some(speak.clone());
                changed += 1;
                continue;
            }
            let from_display = trigger
                .display_text
                .as_deref()
                .map(str::trim)
                .filter(|s| {
                    !s.is_empty() && s.len() <= 64 && !s.contains('<') && !s.contains('{')
                });
            let line = from_display.unwrap_or(trigger.name.as_str()).to_string();
            trigger.speak = Some(line);
            changed += 1;
        }
    }
    changed
}

pub fn starter_stats(lib: &TriggerLibrary) -> (usize, usize) {
    let groups = lib.groups.len();
    let triggers = lib.groups.iter().map(|g| g.triggers.len()).sum();
    (groups, triggers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_pack_loads() {
        let pack = starter_pack();
        assert!(pack.groups.len() > 20);
        assert!(pack.groups[0].name.starts_with("EQL Essentials /"));
        assert!(pack.groups[0].enabled);
        let cats: Vec<_> = pack
            .groups
            .iter()
            .filter(|g| g.name.starts_with("EQL Essentials /"))
            .map(|g| g.name.as_str())
            .collect();
        assert!(cats.contains(&"EQL Essentials / Core"));
        assert!(cats.contains(&"EQL Essentials / Combat"));
        assert!(cats.contains(&"EQL Essentials / Danger"));
        assert!(cats.contains(&"EQL Essentials / Fades"));
        assert!(cats.contains(&"EQL Essentials / Social"));
        let total: usize = pack.groups.iter().map(|g| g.triggers.len()).sum();
        assert!(total > 200);
        assert!(
            pack.groups
                .iter()
                .any(|g| g.name.starts_with("EQL Raids /")),
            "expected EQL Raids zone/boss groups"
        );
        assert!(
            pack.groups
                .iter()
                .any(|g| g.name.starts_with("Classes / Cleric")),
            "expected Classes / <class> groups"
        );
        assert!(pack.groups.iter().any(|g| {
            g.name == "EQL Essentials / Core" && g.enabled
        }));
        assert!(pack.groups.iter().any(|g| {
            g.name == "EQL Essentials / Combat" && g.enabled
        }));
        assert!(pack.groups.iter().any(|g| {
            g.name == "EQL Essentials / Danger" && g.enabled
        }));
        assert!(pack.groups.iter().any(|g| {
            g.name == "EQL Essentials / Fades" && g.enabled
        }));
        assert!(pack.groups.iter().any(|g| {
            g.name == "EQL Essentials / Social" && !g.enabled
        }));
        let range = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-essentials-out-of-range")
            .expect("range");
        assert!(!range.enabled);
        let oom = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-essentials-oom")
            .expect("oom");
        assert!(oom.use_regex);
        assert!(oom.search.starts_with('^'));

        let low_pet = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-essentials-low-pet-health")
            .expect("low pet health");
        assert!(low_pet.enabled);
        assert!(low_pet.search.contains("50 percent"));
        assert_eq!(low_pet.speak.as_deref(), Some("Pet low health"));
        let pet_died = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-essentials-pet-died")
            .expect("pet died");
        assert!(pet_died.enabled);
        assert!(pet_died.search.contains("{C}"));

        let kintaz = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.name == "Glamour of Kintaz")
            .expect("kintaz");
        assert_eq!(kintaz.timer_seconds, Some(30));
        assert!(kintaz
            .early_end
            .iter()
            .any(|e| e.contains("Glamour of Kintaz")));
        let rapture = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.name == "Rapture" && t.timer_name.as_deref() == Some("Rapture - ${1}"))
            .expect("rapture mez");
        assert_eq!(rapture.timer_seconds, Some(24));
        let dazzle = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-dazzle-mez")
            .expect("dazzle");
        assert_eq!(dazzle.timer_name.as_deref(), Some("Dazzle - ${1}"));

        let mend = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "24fc739023d9-1")
            .expect("mend");
        assert_eq!(mend.timer_seconds, Some(90));
        let loh = pack
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "80a4144d8e7f-1")
            .expect("lay on hands");
        assert_eq!(loh.timer_seconds, Some(900));

        let shm = pack
            .groups
            .iter()
            .find(|g| g.name == "Classes / Shaman / Warnings")
            .expect("shaman warnings");
        let ids: Vec<_> = shm.triggers.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"eql-shm-slow-landed"));
        assert!(ids.contains(&"eql-shm-slow-resisted"));
        assert!(ids.contains(&"eql-shm-malo-landed"));
        assert!(ids.contains(&"eql-shm-malo-resisted"));
        assert!(ids.contains(&"eql-shm-incap-landed"));
        assert!(ids.contains(&"eql-shm-incap-resisted"));
    }

    #[test]
    fn ensure_shaman_warnings_fills_missing() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "class-b61ac75a50".into(),
                name: "Classes / Shaman / Warnings".into(),
                enabled: false,
                triggers: vec![
                    t_regex(
                        "837cfbac69da-0",
                        "Slow Wore Off",
                        r"^Your (Walking Sleep|.+? Insects) spell has worn off\.$",
                        Some("Slow off"),
                        Some("Slow off"),
                        None,
                        None,
                    ),
                    t_regex(
                        "837cfbac69da-1",
                        "Malo Wore Off",
                        r"^Your Mal(ise|aisement|osi|o|osini) spell has worn off\.$",
                        Some("Malo off"),
                        Some("Malo off"),
                        None,
                        None,
                    ),
                ],
            }],
        };
        assert_eq!(ensure_shaman_warnings(&mut lib), 6);
        assert_eq!(ensure_shaman_warnings(&mut lib), 0);
        let names: Vec<_> = lib.groups[0]
            .triggers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "Slowed",
                "Slow Resisted",
                "Slow Wore Off",
                "Maloed",
                "Malo Resisted",
                "Malo Wore Off",
                "Incapacitate",
                "Incapacitate Resisted",
            ]
        );
    }

    #[test]
    fn ensure_eql_ability_timers_patches_mend_and_loh() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "cds".into(),
                name: "Classes / Monk / Cooldowns".into(),
                enabled: false,
                triggers: vec![
                    Trigger {
                        id: "24fc739023d9-1".into(),
                        name: "Mend Cooldown".into(),
                        enabled: true,
                        search: "mend".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(361),
                        timer_name: Some("Mend".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                    Trigger {
                        id: "80a4144d8e7f-1".into(),
                        name: "Lay Hands Cooldown".into(),
                        enabled: true,
                        search: "old".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(4320),
                        timer_name: Some("Lay Hands Cooldown".into()),
                        early_end: vec!["^You have been slain".into()],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                ],
            }],
        };
        assert!(ensure_eql_ability_timers(&mut lib) >= 2);
        assert_eq!(lib.groups[0].triggers[0].timer_seconds, Some(90));
        assert_eq!(lib.groups[0].triggers[1].timer_seconds, Some(900));
        assert_eq!(
            lib.groups[0].triggers[1].timer_name.as_deref(),
            Some("Lay on Hands")
        );
        assert_eq!(ensure_eql_ability_timers(&mut lib), 0);
    }

    #[test]
    fn ensure_eql_mez_patches_classic_durations() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "cc".into(),
                name: "Classes / Enchanter / Crowd Control".into(),
                enabled: false,
                triggers: vec![Trigger {
                    id: "k".into(),
                    name: "Glamour of Kintaz".into(),
                    enabled: true,
                    search: "x".into(),
                    use_regex: true,
                    display_text: None,
                    timer_seconds: Some(54),
                    timer_name: Some("Kintaz - ${1}".into()),
                    early_end: vec![],
                    sound: None,
                    speak: None,
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        assert!(ensure_eql_mez_timers(&mut lib) >= 2);
        assert_eq!(lib.groups[0].triggers[0].timer_seconds, Some(30));
        assert!(lib.groups[0].triggers.iter().any(|t| t.id == "eql-dazzle-mez"));
    }

    #[test]
    fn ensure_eql_mez_adds_eql_worn_off_of_target() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "cc".into(),
                name: "Classes / Enchanter / Crowd Control".into(),
                enabled: false,
                triggers: vec![
                    Trigger {
                        id: "e".into(),
                        name: "Entrance".into(),
                        enabled: true,
                        search: "x".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(72),
                        timer_name: Some("Entrance".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                    Trigger {
                        id: "f".into(),
                        name: "Fascination".into(),
                        enabled: true,
                        search: r"^([\w -'`]+) has been fascinated\.$".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(36),
                        timer_name: Some("Fascination".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                ],
            }],
        };
        assert!(ensure_eql_mez_timers(&mut lib) > 0);
        let entrance = lib.groups[0]
            .triggers
            .iter()
            .find(|t| t.name == "Entrance")
            .unwrap();
        assert_eq!(entrance.timer_name.as_deref(), Some("Entrance - ${1}"));
        assert!(entrance
            .early_end
            .iter()
            .any(|e| e.contains("worn off of ${1}")));
        let fasc = lib.groups[0]
            .triggers
            .iter()
            .find(|t| t.name == "Fascination")
            .unwrap();
        assert_eq!(fasc.timer_name.as_deref(), Some("Fascination - ${1}"));
        assert!(fasc
            .early_end
            .iter()
            .any(|e| e.contains("worn off of ${1}")));
    }

    #[test]
    fn ensure_divine_invulnerability_adds_aura_duration() {
        let mut lib = TriggerLibrary {
            groups: vec![
                TriggerGroup {
                    id: "c".into(),
                    name: "Classes / Cleric / Invulnerability".into(),
                    enabled: false,
                    triggers: vec![Trigger {
                        id: "cd".into(),
                        name: "Divine Aura Cooldown".into(),
                        enabled: true,
                        search: r"^(The gods have rendered you invulnerable|You have finished memorizing Divine Aura)\.$".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(900),
                        timer_name: Some("Divine Aura Cooldown".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    }],
                },
                TriggerGroup {
                    id: "p".into(),
                    name: "Classes / Paladin / Invulnerability".into(),
                    enabled: false,
                    triggers: vec![Trigger {
                        id: "pcd".into(),
                        name: "Divine Aura Cooldown".into(),
                        enabled: true,
                        search: "old".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(900),
                        timer_name: Some("Divine Aura Cooldown".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    }],
                },
            ],
        };
        assert!(ensure_divine_invulnerability(&mut lib) >= 2);
        for g in &lib.groups {
            assert!(g.triggers.iter().any(|t| {
                t.name == "Divine Aura" && t.timer_seconds == Some(18)
            }));
            let cd = g
                .triggers
                .iter()
                .find(|t| t.name == "Divine Aura Cooldown")
                .unwrap();
            assert!(cd.search.contains("You are surrounded by a divine aura"));
        }
        assert!(lib.groups[0].triggers.iter().any(|t| {
            t.name == "Divine Barrier" && t.timer_seconds == Some(18)
        }));
    }

    #[test]
    fn apply_gameplay_defaults_arms_fight_helpers() {
        let mut lib = starter_pack();
        for g in &mut lib.groups {
            if !g.name.starts_with("EQL Essentials /") {
                continue;
            }
            g.enabled = false;
            for t in &mut g.triggers {
                t.enabled = true;
            }
        }
        let _changed = apply_gameplay_essentials_defaults(&mut lib);
        assert!(lib
            .groups
            .iter()
            .find(|g| g.name == "EQL Essentials / Combat")
            .unwrap()
            .enabled);
        assert!(!lib
            .groups
            .iter()
            .find(|g| g.name == "EQL Essentials / Social")
            .unwrap()
            .enabled);
        let los = lib
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-essentials-los")
            .unwrap();
        assert!(!los.enabled);
        assert!(los.tts_enabled);
        assert!(los.speak.is_some());
        let range = lib
            .groups
            .iter()
            .flat_map(|g| g.triggers.iter())
            .find(|t| t.id == "eql-essentials-out-of-range")
            .unwrap();
        assert!(!range.enabled);
        assert!(range.tts_enabled);
        assert!(range.speak.is_some());
    }

    #[test]
    fn demote_optional_essentials_turns_off_non_core() {
        let mut lib = starter_pack();
        for g in &mut lib.groups {
            if g.name.starts_with("EQL Essentials /") {
                g.enabled = true;
            }
        }
        assert!(demote_optional_essentials(&mut lib) >= 1);
        assert!(lib
            .groups
            .iter()
            .find(|g| g.name == "EQL Essentials / Core")
            .unwrap()
            .enabled);
        // demote still treats anything non-Core as optional (legacy one-shot).
        assert!(!lib
            .groups
            .iter()
            .find(|g| g.name == "EQL Essentials / Combat")
            .unwrap()
            .enabled);
    }

    #[test]
    fn ensure_eql_disease_dot_timers_rewrites_shared_fever_line() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Classes / Shaman / Damage Over Time".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "dc9fed12dd5a-3".into(),
                    name: "Scourge".into(),
                    enabled: true,
                    search: r"^([\w -'`]+) sweats and shivers, looking feverish\.$".into(),
                    use_regex: true,
                    display_text: None,
                    timer_seconds: Some(126),
                    timer_name: Some("Scourge - ${1}".into()),
                    early_end: vec![],
                    sound: None,
                    speak: None,
                    tts_enabled: true,
                    comments: None,
                }],
            }],
        };
        assert_eq!(ensure_eql_disease_dot_timers(&mut lib), 1);
        assert_eq!(ensure_eql_disease_dot_timers(&mut lib), 0);
        let scourge = &lib.groups[0].triggers[0];
        assert!(scourge.search.contains("by Scourge"));
        assert!(scourge.search.contains(r"(?: [IVX]+)?"));
        assert!(!scourge.search.contains("feverish"));
    }

    #[test]
    fn ensure_eql_disease_dot_timers_rewrites_shared_poison_line() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Classes / Shaman / Damage Over Time".into(),
                enabled: true,
                triggers: vec![
                    Trigger {
                        id: "venom".into(),
                        name: "Venom of the Snake".into(),
                        enabled: true,
                        search: r"^([\w -'`]+) has been poisoned\.$".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(42),
                        timer_name: Some("Venom of the Snake - ${1}".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                    Trigger {
                        id: "bolt".into(),
                        name: "Envenomed Bolt".into(),
                        enabled: true,
                        search: r"^([\w -'`]+) has been poisoned\.$".into(),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(42),
                        timer_name: Some("Envenomed Bolt - ${1}".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: true,
                        comments: None,
                    },
                ],
            }],
        };
        assert_eq!(ensure_eql_disease_dot_timers(&mut lib), 2);
        assert_eq!(ensure_eql_disease_dot_timers(&mut lib), 0);
        assert!(lib.groups[0].triggers[0]
            .search
            .contains("by Venom of the Snake"));
        assert!(lib.groups[0].triggers[1]
            .search
            .contains("by Envenomed Bolt"));
        assert!(!lib.groups[0].triggers[0]
            .search
            .contains("has been poisoned"));
        assert!(lib.groups[0].triggers[0]
            .search
            .contains(r"(?: [IVX]+)?"));
    }

    #[test]
    fn ensure_eql_disease_dot_timers_upgrades_rank_suffix() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Classes / Shaman / Damage Over Time".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "plague".into(),
                    name: "Plague".into(),
                    enabled: true,
                    search: r"^You hit ([\w -'`]+) for \d+ points of disease damage by Plague\.$"
                        .into(),
                    use_regex: true,
                    display_text: None,
                    timer_seconds: Some(132),
                    timer_name: Some("Plague - ${1}".into()),
                    early_end: vec![],
                    sound: None,
                    speak: None,
                    tts_enabled: false,
                    comments: None,
                }],
            }],
        };
        assert_eq!(ensure_eql_disease_dot_timers(&mut lib), 1);
        assert_eq!(ensure_eql_disease_dot_timers(&mut lib), 0);
        assert!(lib.groups[0].triggers[0]
            .search
            .contains(r"(?: [IVX]+)?"));
    }

    #[test]
    fn ensure_shaman_dots_adds_odium_plague_bolt() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Classes / Shaman / Damage Over Time".into(),
                enabled: true,
                triggers: vec![Trigger {
                    id: "bane".into(),
                    name: "Bane of Nife".into(),
                    enabled: true,
                    search: crate::engine::you_hit_by_spell_pattern("Bane of Nife"),
                    use_regex: true,
                    display_text: None,
                    timer_seconds: Some(45),
                    timer_name: Some("Bane - ${1}".into()),
                    early_end: vec![],
                    sound: None,
                    speak: None,
                    tts_enabled: false,
                    comments: None,
                }],
            }],
        };
        assert_eq!(ensure_shaman_dots(&mut lib), 3);
        assert_eq!(ensure_shaman_dots(&mut lib), 0);
        let names: Vec<_> = lib.groups[0]
            .triggers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert!(names.contains(&"Odium"));
        assert!(names.contains(&"Envenomed Bolt"));
        assert!(names.contains(&"Plague"));
    }

    #[test]
    fn starter_scourge_ignores_other_casters_sicken() {
        use crate::engine::TriggerEngine;

        let scourge = Trigger {
            id: "dc9fed12dd5a-3".into(),
            name: "Scourge".into(),
            enabled: true,
            search: crate::engine::you_hit_by_spell_pattern("Scourge"),
            use_regex: true,
            display_text: None,
            timer_seconds: Some(126),
            timer_name: Some("Scourge - ${1}".into()),
            early_end: vec![],
            sound: None,
            speak: None,
            tts_enabled: false,
            comments: None,
        };
        let mut engine = TriggerEngine::new(TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Shaman DoTs".into(),
                enabled: true,
                triggers: vec![scourge],
            }],
        });

        // Other player's Sicken land text used to start Scourge.
        let false_pos = engine.process_action(
            "A zol ghoul knight sweats and shivers, looking feverish.",
        );
        assert!(false_pos.is_empty());
        assert!(engine.snapshot().timers.is_empty());

        let yours = engine.process_action(
            "You hit a zol ghoul knight for 69 points of disease damage by Scourge.",
        );
        assert_eq!(yours.len(), 1);
        assert_eq!(
            yours[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Scourge - a zol ghoul knight")
        );

        let ranked = engine.process_action(
            "You hit a lava guardian for 80 points of disease damage by Scourge IV.",
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(
            ranked[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Scourge - a lava guardian")
        );
    }

    #[test]
    fn starter_venom_ignores_mob_and_other_player_poison() {
        use crate::engine::TriggerEngine;

        let venom = Trigger {
            id: "venom".into(),
            name: "Venom of the Snake".into(),
            enabled: true,
            search: crate::engine::you_hit_by_spell_pattern("Venom of the Snake"),
            use_regex: true,
            display_text: None,
            timer_seconds: Some(42),
            timer_name: Some("Venom of the Snake - ${1}".into()),
            early_end: vec![],
            sound: None,
            speak: None,
            tts_enabled: false,
            comments: None,
        };
        let mut engine = TriggerEngine::new(TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Shaman DoTs".into(),
                enabled: true,
                triggers: vec![venom],
            }],
        });

        // Mob land on a player, or other player's Envenomed Breath.
        assert!(engine
            .process_action("Kabektik has been poisoned.")
            .is_empty());
        assert!(engine
            .process_action(
                "a zol ghoul knight hit Kabektik for 39 points of poison damage by Venom of the Snake.",
            )
            .is_empty());
        assert!(engine
            .process_action(
                "Jasab hit a vis ghoul knight for 21 points of poison damage by Envenomed Breath.",
            )
            .is_empty());
        assert!(engine.snapshot().timers.is_empty());

        let yours = engine.process_action(
            "You hit a vampire bat for 39 points of poison damage by Venom of the Snake.",
        );
        assert_eq!(yours.len(), 1);
        assert_eq!(
            yours[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Venom of the Snake - a vampire bat")
        );
    }

    #[test]
    fn shaman_odium_plague_bolt_match_ranked_lands() {
        use crate::engine::TriggerEngine;

        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Classes / Shaman / Damage Over Time".into(),
                enabled: true,
                triggers: vec![],
            }],
        };
        assert_eq!(ensure_shaman_dots(&mut lib), 3);
        let _ = ensure_eql_disease_dot_timers(&mut lib);
        let mut engine = TriggerEngine::new(lib);

        // Shared land emote — only after your Odium cast.
        assert!(engine
            .process_action("a lava guardian staggers under a dark curse.")
            .is_empty());
        engine.process_action("You begin casting Odium III.");
        let odium = engine
            .process_action("a lava guardian staggers under a dark curse.");
        assert_eq!(odium.len(), 1);
        assert_eq!(
            odium[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Odium - a lava guardian")
        );
        assert_eq!(
            odium[0].speak.as_deref(),
            Some("a lava guardian Odium")
        );

        let bolt = engine.process_action(
            "You hit a lava guardian for 55 points of poison damage by Envenomed Bolt II.",
        );
        assert_eq!(bolt.len(), 1);
        assert_eq!(
            bolt[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Envenomed Bolt - a lava guardian")
        );

        let plague = engine.process_action(
            "You hit a lava guardian for 1,234 points of disease damage by Plague IV.",
        );
        assert_eq!(plague.len(), 1);
        assert_eq!(
            plague[0].started_timer.as_ref().map(|t| t.name.as_str()),
            Some("Plague - a lava guardian")
        );
        assert_eq!(
            plague[0].speak.as_deref(),
            Some("a lava guardian Plagued")
        );
    }

    #[test]
    fn ensure_shaman_dots_enables_plague_tts_and_odium_land() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "dots".into(),
                name: "Classes / Shaman / Damage Over Time".into(),
                enabled: true,
                triggers: vec![
                    Trigger {
                        id: "eql-shaman-odium".into(),
                        name: "Odium".into(),
                        enabled: true,
                        search: crate::engine::you_hit_by_spell_pattern("Odium"),
                        use_regex: true,
                        display_text: Some("${1} Odium".into()),
                        timer_seconds: Some(30),
                        timer_name: Some("Odium - ${1}".into()),
                        early_end: vec![],
                        sound: None,
                        speak: Some("${1} Odium".into()),
                        tts_enabled: true,
                        comments: None,
                    },
                    Trigger {
                        id: "eql-shaman-plague".into(),
                        name: "Plague".into(),
                        enabled: true,
                        search: crate::engine::you_hit_by_spell_pattern("Plague"),
                        use_regex: true,
                        display_text: None,
                        timer_seconds: Some(132),
                        timer_name: Some("Plague - ${1}".into()),
                        early_end: vec![],
                        sound: None,
                        speak: None,
                        tts_enabled: false,
                        comments: None,
                    },
                ],
            }],
        };
        assert_eq!(ensure_shaman_dots(&mut lib), 3);
        assert_eq!(ensure_shaman_dots(&mut lib), 0);

        let odium = lib.groups[0]
            .triggers
            .iter()
            .find(|t| t.name == "Odium")
            .unwrap();
        assert!(odium.search.contains("staggers under a dark curse"));
        assert!(odium.tts_enabled);

        let plague = lib.groups[0]
            .triggers
            .iter()
            .find(|t| t.name == "Plague")
            .unwrap();
        assert_eq!(plague.speak.as_deref(), Some("${1} Plagued"));
        assert!(plague.tts_enabled);
    }

    #[test]
    fn ensure_shaman_buffs_adds_puma_once() {
        let mut lib = TriggerLibrary {
            groups: vec![TriggerGroup {
                id: "class-68ef1f9bea".into(),
                name: "Classes / Shaman / Buffs / Self".into(),
                enabled: false,
                triggers: vec![t_timer_regex(
                    "f0a20f45cc96-4",
                    "Spirit of Cheetah",
                    r"^You feel the spirit of cheetah enter you\.$",
                    None,
                    None,
                    None,
                    48,
                    "Spirit of Cheetah",
                    vec![r"^The spirit of cheetah fades\.$"],
                    None,
                )],
            }],
        };
        assert_eq!(ensure_shaman_buffs(&mut lib), 1);
        assert_eq!(ensure_shaman_buffs(&mut lib), 0);
        let names: Vec<_> = lib.groups[0]
            .triggers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(names, vec!["Spirit of Cheetah", "Spirit of the Puma"]);
        let puma = &lib.groups[0].triggers[1];
        assert_eq!(puma.id, "eql-shm-spirit-of-the-puma");
        assert_eq!(puma.timer_seconds, Some(85));
        assert_eq!(puma.speak.as_deref(), Some("Puma"));
        assert!(puma.tts_enabled);
    }
}
