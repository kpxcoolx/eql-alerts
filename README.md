# EQL Alerts

Lightweight log triggers for **EverQuest Legends** — a modern, no-frills take on [GINA](https://quarm.guide/gina).

[Latest release](https://github.com/kpxcoolx/eql-alerts/releases/latest) · [Discussions](https://github.com/kpxcoolx/eql-alerts/discussions) · [eqlalerts.com](https://eqlalerts.com)

Tails your `eqlog_*.txt`, matches simple search text (or regex), and fires:

- **Text overlays** (toasts)
- **Countdown timers** (with early-end text)
- **Optional sound** — pick from chime presets (Glass, Ping, Sosumi, …) per trigger
- **Voice callouts** — DBM-style male/female system voices speaking the trigger line


Legacy GINA binaries in `GINA/` are reference only — this app does not wrap them.

## How it differs from GINA

| GINA | EQL Alerts |
|------|------------|
| Full WPF ribbon UI, GimaLink packages, TTS, multi-overlay behaviors | Thin Tauri app: JSON triggers, one overlay |
| Broad EQ ecosystem | EverQuest Legends log paths only |
| `.gtp` / GamTextTriggers import | JSON config (GINA import later) |

Same core idea: **watch the log → match text → notify**.

## Mac + osxEQL (native Apple Silicon)

Play EQ with [osxEQL](https://github.com/kpxcoolx/osxEQL) (Wine), and run EQL Alerts as a native Mac app. Auto-detect looks in the Wine prefix:

```text
~/Library/Application Support/osxEQL/prefix/drive_c/users/Public/Daybreak Game Company/Installed Games/EverQuest Legends/Logs/eqlog_*.txt
```

1. Install and run EQ Legends via osxEQL (`/log on` in-game)
2. Install EQL Alerts from the latest `.dmg` on [Releases](https://github.com/kpxcoolx/eql-alerts/releases/latest) (or build locally — see below)
3. **First open:** the app isn’t signed by Apple, so macOS Gatekeeper blocks it (“damaged” / “can’t be opened”). Clear quarantine once in Terminal:

```bash
xattr -dr com.apple.quarantine "/Applications/EQL Alerts.app"
```

   After that it opens normally every time.
4. Click **Find log** — it should pick up the osxEQL path

### Mac + Parallels (alternate)

Run the app on the **Mac host**; EQ stays in the Windows VM. The log is read through the mounted `C:` under `/Volumes`.

Typical path:

```text
/Volumes/[C] Windows 11.hidden/Users/Public/Daybreak Game Company/Installed Games/EverQuest Legends/Logs/eqlog_*.txt
```

### Overlay

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd+Alt+U` | Overlay editable (drag / setup) |
| `Ctrl/Cmd+Alt+L` | Click-through to the game |

## Trigger model

Triggers live in app config as `triggers.json`:

```json
{
  "groups": [
    {
      "id": "general",
      "name": "General",
      "enabled": true,
      "triggers": [
        {
          "id": "zoning",
          "name": "Zoning",
          "enabled": true,
          "search": "LOADING, PLEASE WAIT...",
          "use_regex": false,
          "display_text": "Zoning…",
          "timer_seconds": null,
          "timer_name": null,
          "early_end": [],
          "sound": null,
          "comments": null
        }
      ]
    }
  ]
}
```

Matching uses the line **after** `[timestamp]`. Display tokens: `{C}` character name, `{S}` full action text.

## Quick start

1. `npm run tauri:dev` (or install from the Mac `.dmg` / Windows setup.exe)
2. **Find log** (osxEQL Wine path, Parallels `/Volumes`, or Windows Legends path)
3. Click your **class chip** to arm that set
4. Open **Overlay**

A rebuilt Legends starter loads on first run:

1. **EQL Essentials** — **Core / Combat / Danger / Fades** always on (spammy triggers inside stay off); **Social** is opt-in
2. **Classes** — each class under `Classes / …` (arm with the class chips)
3. **EQL Raids → Zone → Boss** — classic raid targets currently available (Nagafen, Vox, Fear, Hate, Sky, Hole, Kedge)

Everything except Essentials starts disabled. Use **Reset starter** to replace your library with this pack.

Regenerate from the archived GINA convert with:

```bash
python3 scripts/rebuild_eql_starter.py
```

### EQL vs classic GINA timers

Many self-buffs that were short classic timers are **permanent** on Legends (Yaulp I–III, Divine Might/Purpose, Lich, Elemental Armor, Greater Wolf Form, …). The starter pack and GINA import strip those countdown timers so the overlay does not show a fake expiry. List: `samples/eql_permanent_buffs.json`. Re-apply with:

```bash
python3 scripts/eql_compat.py samples/eql_starter.triggers.json
```

Existing installs auto-strip these timers once on next launch (`eql_compat_permanent_v1`). You can still use **Reset starter** for a clean pack.

### Self-only combat clocks (not other players)

Many classic land emotes are **zone-visible** (`X has been poisoned.`, `X yawns.`, `X has been mesmerized.`). Without filtering, every nearby caster’s spells light your overlay.

EQL Alerts scopes those to **your** casts:

| Kind | How |
|------|-----|
| **Damage Over Time** timers | Match `You hit <target> … by <Spell>` including EQL upgrade ranks (`Plague IV`) via `ensure_eql_disease_dot_timers` |
| **Crowd Control** (mez, etc.) | Land emote only arms if you recently `You begin casting` that spell (Dazzle still upgrades the shared mesmerize line) |
| **Slowed / Maloed** warnings | Same cast gate — ignores party Drowsy / other malo lands |

Restart the app after updates so migrations and engine cast-gating apply. Group buff trackers under `Buffs / Others` stay intentionally shared.

## Import GINA packs

`GINA/gina_pack.gtp` converts into our trigger model (timers, text/TTS→toast, early-enders). Groups import **disabled** so you opt in like GINA. Permanent-buff timers are cleared during import.

In the app: **Import GINA pack…** and choose a `.gtp` / `.json` / `.xml` file. For a clean Legends layout prefer **Reset starter** over importing the raw classic GINA tree.

CLI (archive rebuild path):

```bash
python3 scripts/import_gina_gtp.py GINA/gina_pack.gtp -o samples/gina_pack.triggers.json
python3 scripts/rebuild_eql_starter.py
```

Notes: Rust regex skips a few GINA patterns that use lookaround/backrefs. GINA **Text-to-voice** lines become `speak` and play through native OS TTS (macOS `say` / Windows SAPI) — Web Speech inside Tauri is unreliable. Optional wav/mp3 paths can go in `sound`. Permanent-buff timer stripping still runs after import.

## Installers (Windows + Mac)

GitHub Actions builds a Windows NSIS setup.exe, a Mac Apple Silicon `.dmg`, and a shared auto-update feed.

### Windows

1. Download the latest `*_x64-setup.exe` from [Releases](https://github.com/kpxcoolx/eql-alerts/releases/latest)
2. Run it (current-user install; no admin)
3. Find log → Overlay → Ctrl+Alt+L for click-through

### Mac (Apple Silicon)

1. Download the latest `.dmg` from [Releases](https://github.com/kpxcoolx/eql-alerts/releases/latest)
2. Drag **EQL Alerts** into Applications
3. Clear Gatekeeper quarantine (required once — unsigned build):

```bash
xattr -dr com.apple.quarantine "/Applications/EQL Alerts.app"
```

4. Find log (osxEQL or Parallels) → Overlay → Cmd+Alt+L for click-through

Local Mac package:

```bash
npm install
npm run tauri:build:macos
# → src-tauri/target/release/bundle/dmg/*.dmg
```

### Ship a new build

1. Bump versions in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`
2. Tag and push:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Or: GitHub → Actions → **release** → Run workflow with `v0.1.0`.

CI creates a draft release with:

- `EQL.Alerts_*_x64-setup.exe` (Windows)
- `EQL Alerts_*_aarch64.dmg` (Mac)
- `.sig` + `latest.json` for in-app updates

Then publishes only when Windows + Mac assets exist. In the app: **Updates** / banner **Install update**.

### Secrets (repo Settings → Secrets)

| Secret | Purpose |
|--------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `.tauri-keys/eql-alerts.key` (local only; never commit) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional if the key is passwordless |

## Contributors

Feature ideas and questions: use [Discussions](https://github.com/kpxcoolx/eql-alerts/discussions) (Ideas / Q&A). Bugs: open an [Issue](https://github.com/kpxcoolx/eql-alerts/issues).

## Stack

Same lightweight approach as EQL Meter: **Tauri 2 + Rust log tail (notify + poll) + React UI**.
