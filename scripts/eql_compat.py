#!/usr/bin/env python3
"""Strip classic GINA timers from buffs that are permanent on EverQuest Legends."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BUFFS = ROOT / "samples" / "eql_permanent_buffs.json"
DEFAULT_PACK = ROOT / "samples" / "eql_starter.triggers.json"
NOTE = "EQL: permanent buff — classic timer removed; alerts off"


def load_permanent_names(path: Path) -> set[str]:
    data = json.loads(path.read_text())
    return {name.strip().lower() for name in data.get("spells", [])}


def trigger_base_name(name: str) -> str:
    """'Yaulp' from 'Yaulp'; 'Call of Bones' from 'Call of Bones (Soul Defiler)'."""
    base = name.split("(", 1)[0].strip()
    return base


def is_permanent_buff(name: str, permanent: set[str]) -> bool:
    return trigger_base_name(name).lower() in permanent


def strip_permanent_timers(library: dict, permanent: set[str]) -> list[str]:
    """Clear timers and silence alerts for permanent buffs. Returns change list."""
    changed: list[str] = []
    for group in library.get("groups", []):
        for trigger in group.get("triggers", []):
            if not is_permanent_buff(trigger.get("name", ""), permanent):
                continue
            secs = trigger.get("timer_seconds")
            already_quiet = (
                not secs
                and not trigger.get("enabled", True)
                and not trigger.get("speak")
                and not trigger.get("tts_enabled", True)
            )
            if already_quiet:
                continue
            label = f"{group.get('name', '?')} :: {trigger.get('name')}"
            if secs:
                label = f"{label} ({secs}s)"
            trigger["timer_seconds"] = None
            trigger["timer_name"] = None
            trigger["display_text"] = None
            trigger["speak"] = None
            trigger["sound"] = None
            trigger["tts_enabled"] = False
            trigger["enabled"] = False
            trigger["comments"] = NOTE
            changed.append(label)
    return changed


def patch_file(pack_path: Path, buffs_path: Path) -> int:
    permanent = load_permanent_names(buffs_path)
    library = json.loads(pack_path.read_text())
    changed = strip_permanent_timers(library, permanent)
    pack_path.write_text(json.dumps(library, indent=2) + "\n")
    return len(changed)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "pack",
        nargs="?",
        default=str(DEFAULT_PACK),
        help="triggers.json to patch",
    )
    parser.add_argument(
        "--buffs",
        default=str(DEFAULT_BUFFS),
        help="eql_permanent_buffs.json",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="List matches without writing",
    )
    args = parser.parse_args()
    pack_path = Path(args.pack)
    permanent = load_permanent_names(Path(args.buffs))
    library = json.loads(pack_path.read_text())
    changed = strip_permanent_timers(library, permanent)
    print(f"Matched {len(changed)} permanent-buff timers:")
    for line in changed:
        print(f"  - {line}")
    if args.dry_run:
        print("Dry run — no write")
        return
    pack_path.write_text(json.dumps(library, indent=2) + "\n")
    print(f"Wrote {pack_path}")


if __name__ == "__main__":
    main()
