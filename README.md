# EQL Alerts

Lightweight log triggers for **EverQuest Legends** — a modern, no-frills take on [GINA](https://quarm.guide/gina).

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

## Mac + Parallels (same flow as EQL Meter)

Run the app on the **Mac host**; EQ stays in the Windows VM. The log is read through the mounted `C:` under `/Volumes`.

1. Start the Windows VM and EQ Legends (`/log on`)
2. Confirm Finder → `/Volumes` shows something like `[C] Windows 11.hidden`
3. From this folder:

```bash
npm install
npm run tauri:dev
```

4. Click **Auto-detect log** (or **Choose log…** and pick `eqlog_*.txt` under the Parallels Logs path)

Typical path:

```text
/Volumes/[C] Windows 11.hidden/Users/Public/Daybreak Game Company/Installed Games/EverQuest Legends/Logs/eqlog_Kenkyo_*.txt
```

There is no Mac installer yet — Mac is for dogfooding via `tauri:dev`, same as the meter.

### Overlay

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd+Shift+U` | Overlay editable (drag / setup) |
| `Ctrl/Cmd+Shift+L` | Click-through to the game |

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

1. `npm run tauri:dev`
2. **Find log** (Parallels / Windows Legends path)
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

## Import GINA packs

`GINA/gina_pack.gtp` converts into our trigger model (timers, text/TTS→toast, early-enders). Groups import **disabled** so you opt in like GINA. Permanent-buff timers are cleared during import.

In the app: **Import GINA pack…** and choose a `.gtp` / `.json` / `.xml` file. For a clean Legends layout prefer **Reset starter** over importing the raw classic GINA tree.

CLI (archive rebuild path):

```bash
python3 scripts/import_gina_gtp.py GINA/gina_pack.gtp -o samples/gina_pack.triggers.json
python3 scripts/rebuild_eql_starter.py
```

Notes: Rust regex skips a few GINA patterns that use lookaround/backrefs. GINA **Text-to-voice** lines become `speak` and play through native OS TTS (macOS `say` / Windows SAPI) — Web Speech inside Tauri is unreliable. Optional wav/mp3 paths can go in `sound`. Permanent-buff timer stripping still runs after import.

## Windows installer (Parallels VM)

Same flow as EQL Meter — GitHub Actions builds the NSIS setup.exe and an auto-update feed.

### Install in the VM

1. Download the latest `*_x64-setup.exe` from [Releases](https://github.com/kpxcoolx/eql-alerts/releases/latest)
2. Run it (current-user install; no admin)
3. Find log → Overlay → Ctrl+Shift+L for click-through

### Ship a new build

1. Bump versions in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`
2. Tag and push:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Or: GitHub → Actions → **windows-build** → Run workflow with `v0.1.0`.

CI creates a draft release with:

- `EQL.Alerts_*_x64-setup.exe`
- `.sig` + `latest.json` for in-app updates

Then publishes only when those assets exist. In the app: **Updates** / banner **Install update**.

### Secrets (repo Settings → Secrets)

| Secret | Purpose |
|--------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `.tauri-keys/eql-alerts.key` (local only; never commit) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional if the key is passwordless |

## Stack

Same lightweight approach as EQL Meter: **Tauri 2 + Rust log tail (notify + poll) + React UI**.
