#!/usr/bin/env python3
"""Rebuild samples/eql_starter.triggers.json from the classic GINA pack.

Keeps class-specific triggers (renamed off the old Raid root) and builds
EQL Raids / Zone / Boss groups for classic raids currently available in
EverQuest Legends. Drops Kunark/Velious/guild/common/debuff-macro clutter.
"""

from __future__ import annotations

import copy
import json
import re
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "samples" / "gina_pack.triggers.json"
OUT = ROOT / "samples" / "eql_starter.triggers.json"

CLASSES = [
    "Bard",
    "Beastlord",
    "Cleric",
    "Druid",
    "Enchanter",
    "Magician",
    "Monk",
    "Necromancer",
    "Paladin",
    "Ranger",
    "Rogue",
    "Shadow Knight",
    "Shaman",
    "Warrior",
    "Wizard",
]

# Classic EQL raid targets (patch notes / wiki raid zones as of now).
# Structure: Zone -> Boss -> list of source trigger names to clone (optional).
EQL_RAIDS: dict[str, dict[str, list[str]]] = {
    "Nagafen's Lair": {
        "Lord Nagafen": [
            "Lava Breath (Sontalak/Nagafen/Ragefire/Faydedar/Telkorenar)",
            "Dragon Roar (Cooldown)",
            "Feared",
        ],
    },
    "Permafrost": {
        "Lady Vox": [
            "Frost Breath (Jorlleag/Terror/Gozzrem/Lady Vox/Nanzata the Warder)",
            "Dragon Roar (Cooldown)",
            "Feared",
        ],
    },
    "Plane of Fear": {
        "Terror": [
            "Frost Breath (Jorlleag/Terror/Gozzrem/Lady Vox/Nanzata the Warder)",
        ],
        "Fright": [],
        "Dread": [],
        "a dracoliche": [],
        "Cazic Thule": [
            "Cazic Thule Pop",
        ],
    },
    "Plane of Hate": {
        "Innoruuk": [],
        "Maestro of Rancor": [],
        "Magi P`Tasa": [],
        "High Priest M`kari": [],
        "Master of Spite": [],
        "Lord of Loathing": [],
        "Coercer T`vala": [],
    },
    "Plane of Sky": {
        "Bazzt Zzzt": ["Bazzt Zzzt DT"],
        "The Spiroc Lord": ["The Spiroc Lord DT"],
        "Keeper of Souls": ["Keeper of Souls DT"],
        "Overseer of Air": ["Overseer of Air DT"],
        "Sister of the Spire": ["Sister of the Spire DT"],
        "Eye of Veeshan": ["Eye of Veeshan DT"],
        "the Hand of Veeshan": ["the Hand of Veeshan DT"],
    },
    "The Hole": {
        "Master Yael": ["Master Yael"],
    },
    "Kedge Keep": {
        "Phinigel Autropos": [],
    },
}

# Zone-entry display names used by EQ logs (classic-ish).
ZONE_ENTER = {
    "Nagafen's Lair": "Nagafen's Lair",
    "Permafrost": "Permafrost Keep",
    "Plane of Fear": "Plane of Fear",
    "Plane of Hate": "Plane of Hate",
    "Plane of Sky": "The Plane of Sky",
    "The Hole": "The Hole",
    "Kedge Keep": "Kedge Keep",
}


def new_id(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex[:10]}"


def clone_trigger(src: dict, *, name: str | None = None, comments: str | None = None) -> dict:
    t = copy.deepcopy(src)
    t["id"] = new_id("eql")
    t["enabled"] = True
    if name:
        t["name"] = name
    if comments is not None:
        t["comments"] = comments
    # Drop classic "/who is not online" helper noise if somehow included.
    if "is not online at this time" in (t.get("search") or ""):
        return {}
    return t


def make_trigger(
    *,
    name: str,
    search: str,
    display: str | None = None,
    speak: str | None = None,
    timer: int | None = None,
    timer_name: str | None = None,
    regex: bool = False,
    early_end: list[str] | None = None,
    comments: str | None = None,
    sound: str | None = None,
    tts_enabled: bool = True,
) -> dict:
    return {
        "id": new_id("eql"),
        "name": name,
        "enabled": True,
        "search": search,
        "use_regex": regex,
        "display_text": display,
        "timer_seconds": timer,
        "timer_name": timer_name,
        "early_end": early_end or [],
        "sound": sound,
        "speak": speak,
        "tts_enabled": tts_enabled,
        "comments": comments,
    }


def is_useless(t: dict) -> bool:
    search = t.get("search") or ""
    if "is not online at this time" in search:
        return True
    # Pure wiki-card display with no real search (shouldn't exist, but be safe)
    if not search.strip():
        return True
    return False


def class_groups(src_groups: list[dict]) -> list[dict]:
    out: list[dict] = []
    for g in src_groups:
        name = g.get("name") or ""
        if "01 - Class Specific" not in name:
            continue
        parts = [p.strip() for p in name.split(" / ") if p.strip()]
        try:
            idx = parts.index("01 - Class Specific")
        except ValueError:
            continue
        if idx + 1 >= len(parts):
            continue
        class_name = parts[idx + 1]
        if class_name not in CLASSES:
            continue
        rest = parts[idx + 2 :]
        new_name = " / ".join(["Classes", class_name, *rest])
        triggers = [copy.deepcopy(t) for t in g.get("triggers", []) if not is_useless(t)]
        if not triggers:
            continue
        for t in triggers:
            t["enabled"] = True
        out.append(
            {
                "id": new_id("class"),
                "name": new_name,
                "enabled": False,
                "triggers": triggers,
            }
        )
    # Stable class order, then subpath alpha
    class_rank = {c: i for i, c in enumerate(CLASSES)}

    def sort_key(group: dict):
        parts = group["name"].split(" / ")
        # Classes / Cleric / …
        cls = parts[1] if len(parts) > 1 else parts[0]
        return (class_rank.get(cls, 99), group["name"].lower())

    out.sort(key=sort_key)
    return out


def index_triggers(src_groups: list[dict]) -> dict[str, dict]:
    by_name: dict[str, dict] = {}
    for g in src_groups:
        for t in g.get("triggers", []):
            by_name[t["name"]] = t
    return by_name


def slain_patterns(boss: str) -> list[str]:
    esc = re.escape(boss)
    return [
        rf"^{esc} has been slain by .+!$",
        rf"^You have slain {esc}!$",
    ]


# One zone-entry trigger per zone (on the main boss leaf) so arming every mini
# does not spam "Entered zone" toasts.
ZONE_ENTRY_BOSS = {
    "Nagafen's Lair": "Lord Nagafen",
    "Permafrost": "Lady Vox",
    "Plane of Fear": "Cazic Thule",
    "Plane of Hate": "Innoruuk",
    "Plane of Sky": "Eye of Veeshan",
    "The Hole": "Master Yael",
    "Kedge Keep": "Phinigel Autropos",
}


def raid_groups(by_name: dict[str, dict]) -> list[dict]:
    out: list[dict] = []

    for zone, bosses in EQL_RAIDS.items():
        zone_enter = ZONE_ENTER.get(zone, zone)
        entry_boss = ZONE_ENTRY_BOSS.get(zone)

        for boss, source_names in bosses.items():
            triggers: list[dict] = []
            if boss == entry_boss:
                triggers.append(
                    make_trigger(
                        name="Entered zone",
                        search=f"You have entered {zone_enter}.",
                        display=zone,
                        speak=zone,
                        comments=f"Zone entry · {zone}",
                        sound="ping",
                    )
                )

            for src_name in source_names:
                src = by_name.get(src_name)
                if not src or is_useless(src):
                    continue
                # Rename long shared AoE titles to boss-local names
                rename = None
                if src_name.startswith("Lava Breath"):
                    rename = "Lava Breath"
                elif src_name.startswith("Frost Breath"):
                    rename = "Frost Breath"
                elif src_name.startswith("Dragon Roar"):
                    rename = "Dragon Roar"
                elif src_name == "Feared":
                    rename = "Feared (Dragon Roar landed)"
                cloned = clone_trigger(
                    src,
                    name=rename,
                    comments=f"EQL Raids · {zone} · {boss}",
                )
                if not cloned:
                    continue
                # Point early-end at this boss when the source listed several dragons
                if rename in ("Lava Breath", "Frost Breath", "Dragon Roar"):
                    cloned["early_end"] = slain_patterns(boss)
                triggers.append(cloned)

            # Always add a kill-clear helper so empty bosses still have a real hook
            triggers.append(
                make_trigger(
                    name=f"{boss} slain",
                    search=rf"^({re.escape(boss)} has been slain by .+!|You have slain {re.escape(boss)}!)$",
                    regex=True,
                    display=f"{boss} down",
                    speak=f"{boss} down",
                    sound="glass",
                    comments="Kill confirm — extend with shout/AoE triggers as you learn them",
                )
            )

            out.append(
                {
                    "id": new_id("raid"),
                    "name": f"EQL Raids / {zone} / {boss}",
                    "enabled": False,
                    "triggers": triggers,
                }
            )

    return out


def main() -> None:
    raw = json.loads(SRC.read_text())
    src_groups = raw["groups"]
    by_name = index_triggers(src_groups)

    classes = class_groups(src_groups)
    raids = raid_groups(by_name)
    pack = {"groups": classes + raids}

    OUT.write_text(json.dumps(pack, indent=2) + "\n")

    class_n = sum(1 for g in classes)
    class_t = sum(len(g["triggers"]) for g in classes)
    raid_n = sum(1 for g in raids)
    raid_t = sum(len(g["triggers"]) for g in raids)
    print(f"Wrote {OUT}")
    print(f"  Classes: {class_n} groups / {class_t} triggers")
    print(f"  EQL Raids: {raid_n} groups / {raid_t} triggers")
    print(f"  Total: {class_n + raid_n} groups / {class_t + raid_t} triggers")
    print("  Top-level (sample):")
    tops = []
    for g in pack["groups"]:
        top = g["name"].split(" / ")[0]
        if top not in tops:
            tops.append(top)
    print("   ", ", ".join(tops[:8]), "…" if len(tops) > 8 else "")


if __name__ == "__main__":
    main()
