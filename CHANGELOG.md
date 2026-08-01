# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.26] - 2026-08-01

### Fixed
- DoT land matches for raid names with commas (e.g. Odium on `Innoruuk, the Prince of Hate`)

## [0.1.25] - 2026-08-01

### Added
- Essentials: Promised Renewal 12s incoming-heal timer when cast on you (all ranks)

## [0.1.24] - 2026-07-31

### Fixed
- Clear overlay timers when a mob dies, including partner kills where EQL capitalizes the name (`A foo` vs `a foo`)

## [0.1.23] - 2026-07-31

### Fixed
- Shaman DoT TTS uses short spell names only (no mob name) so stacked lands stay snappy

## [0.1.22] - 2026-07-31

### Fixed
- Disable Paladin Divine Strength/Glory (Others) timers by default (long overlay clutter from other players)

## [0.1.21] - 2026-07-31

### Fixed
- Disable Voice of the Berserker tracking (self-buff; Rage shared its land line and false-fired the overlay)

## [0.1.20] - 2026-07-31

### Fixed
- Shaman Cannibalization AA timer matches the self-hit log (`Cannibalization I`), not the unused `You activate` line

## [0.1.19] - 2026-07-31

### Fixed
- Modulating Rod overlay timer is self-only (no other players)

## [0.1.18] - 2026-07-31

### Fixed
- Mac: restore bundled `eql-speak` playback helper (0.1.17 omitted it, so TTS synthesized but stayed silent)
- Mac: fall back to `afplay` if `eql-speak` is missing

## [0.1.17] - 2026-07-31

### Added
- Essentials: Modulating Rod 300s cooldown when you modulate

## [0.1.16] - 2026-07-31

### Added
- Shaman Cannibalization AA 3-minute cooldown timer
- Mac releases bundle Kokoro TTS (first launch extracts into Application Support)

### Fixed
- TTS on/off toggle now persists and actually mutes live alert speech

## [0.1.15] - 2026-07-17

### Fixed
- Zoning essential only matches the exact `LOADING, PLEASE WAIT...` line (no chat false fires)

## [0.1.14] - 2026-07-17

### Added
- Global TTS on/off from the main sidebar and Settings
- Shaman: Odium land TTS, Plague TTS, Incapacitate warnings, and Spirit of the Puma buff timer
- Essentials: self Slowed / Slow broke cure reminder
- Enchanter: Fascination AE mez timers; Entrance/Enthrall EQL worn-off-of-target; Divine Aura 18s duration

### Fixed
- Ability CD overlay sync (mash hotkey snaps remaining time; ready callout at 0; Lay Hands name matching)

## [0.1.13] - 2026-07-16

### Changed
- Prior release baseline (see git history for details before this changelog existed).
