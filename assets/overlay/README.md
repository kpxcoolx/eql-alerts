# Overlay UI baseline assets

Design baseline taken from the eqlalerts.com product shots:

- `eqlalerts.com/img/main-window.png` — main app chrome (warm dark + gold)
- `eqlalerts.com/img/overlay-toasts.png` — click-through overlay (metal bars, left icons, molten amber timer)

That look is **cleaner** than the live mint overlay (`src/Overlay.css`): shared gold metal panels, left icon badge, name + countdown on one row, amber progress under the name.

## Palette

See `tokens.css`. Matches the marketing site amber system (`--amber: #e0a84a`, Cinzel / Source Sans 3).

## Layout target (from overlay shot)

```
┌──────────────────────────────────────────────┐
│ [icon]  Timer name                    00:12  │
│         ████████████░░░░░░  (amber fill)     │
└──────────────────────────────────────────────┘
┌──────────────────────────────────────────────┐
│ [icon]  Toast text                           │
└──────────────────────────────────────────────┘
```

- Stack: toasts and timers share the same metal chrome language
- Icon slot: ~40px left of the text
- Progress: recessed track, orange→gold fill (not mint)
- Type: display serif for labels, mono for countdown (`00:12`)

## Icons (`icons/` masters · `public/icons/overlay/` runtime)

| File | Use |
|---|---|
| `icon-timer-aura.png` | Default countdown / boss aura timers |
| `icon-buff.png` | Generic buff / class-pack timers |
| `icon-zoning.png` | Zoning toast |
| `icon-alert.png` | Generic toast / danger callouts |
| `icon-enrage.png` | Enrage / ENRAGED |
| `icon-fades.png` | Fades / invis fading |
| `icon-death.png` | YOU DIED / Death timer |
| `icon-stun.png` | STUNNED / crowd-control |

### Classes (`icons/classes/` · same bronze/gold frame language)

All 16 EverQuest classes — original symbols (not Daybreak client icons):

| File | Class |
|---|---|
| `warrior.png` | Warrior — sword + shield |
| `cleric.png` | Cleric — holy warhammer |
| `paladin.png` | Paladin — sword + holy shield |
| `ranger.png` | Ranger — longbow |
| `shadowknight.png` | Shadow Knight — dark blade + skull |
| `druid.png` | Druid — scimitar + leaf |
| `monk.png` | Monk — wrapped fists |
| `bard.png` | Bard — lute |
| `rogue.png` | Rogue — crossed daggers |
| `shaman.png` | Shaman — spirit totem |
| `necromancer.png` | Necromancer — skull + scythe |
| `wizard.png` | Wizard — crystal staff |
| `magician.png` | Magician — elemental gem |
| `enchanter.png` | Enchanter — mesmer eye |
| `beastlord.png` | Beastlord — fist + wolf |
| `berserker.png` | Berserker — greataxe |

Runtime copies:

- `public/icons/overlay/*.png` — 128px essentials (app default)
- `public/icons/overlay/classes/*.png` — 128px class badges
- `…/128/` · `…/256/` — sized variants under both roots
- `assets/overlay/icons/` — 512px masters

## Chrome (`chrome/`)

Reference panels only (prefer CSS from `tokens.css` for the live overlay):

- `chrome-timer-panel.png` — metal bar + left square icon well
- `chrome-toast-panel.png` — metal bar + left round emblem well
- `chrome-progress-bar.png` — molten amber fill reference

## Suggested mapping (`map.json`)

Display-text / timer-name → icon. Unmatched timers → `icon-buff` or `icon-timer-aura`; unmatched toasts → `icon-alert`.

## Next wiring (not done yet)

1. Import `tokens.css` (or copy vars) into `Overlay.css`
2. Add left `<img>` slot on toast + timer rows
3. Resolve icon from `map.json` by toast text / timer name / category
4. Move countdown beside the name (marketing layout)
5. Swap progress gradient to amber fill tokens
