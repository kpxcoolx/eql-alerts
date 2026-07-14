#!/usr/bin/env python3
"""Convert a GINA .gtp package (ShareData.xml zip) into EQL Alerts triggers.json."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path


def fix_atomic_groups(s: str) -> str:
    """Replace .NET atomic groups (?>...) with (?:...) for Rust regex."""
    if not s or "(?>" not in s:
        return s
    out: list[str] = []
    i = 0
    while i < len(s):
        if s.startswith("(?>", i):
            depth = 0
            j = i + 3
            start = j
            while j < len(s):
                if s[j] == "\\" and j + 1 < len(s):
                    j += 2
                    continue
                if s[j] == "(":
                    depth += 1
                elif s[j] == ")":
                    if depth == 0:
                        inner = s[start:j]
                        out.append("(?:" + fix_atomic_groups(inner) + ")")
                        i = j + 1
                        break
                    depth -= 1
                j += 1
            else:
                out.append(s[i])
                i += 1
        else:
            out.append(s[i])
            i += 1
    return "".join(out)


def expand_gina_tokens(text: str) -> str:
    """Expand GINA {C}/{S}/{N}/{L} tokens to regex fragments (string already regex-ish)."""
    text = re.sub(r"\{C\}", ".+?", text)
    text = re.sub(r"\{S\d*\}", ".+?", text)
    text = re.sub(r"\{N(?:[><=]+\d+)?\}", r"\\d+", text)
    text = re.sub(r"\{L\}", ".+", text)
    text = re.sub(r"\{COUNTER\}", r"\\d+", text)
    return text


def gina_plain_to_regex(text: str) -> str:
    """Escape plain search text but keep GINA token wildcards."""
    placeholders: dict[str, str] = {}

    def hold(pattern: str, replacement: str) -> None:
        nonlocal text

        def sub(m: re.Match[str]) -> str:
            key = f"__T{len(placeholders)}__"
            placeholders[key] = replacement
            return key

        text = re.sub(pattern, sub, text)

    hold(r"\{C\}", r".+?")
    hold(r"\{S\d*\}", r".+?")
    hold(r"\{N(?:[><=]+\d+)?\}", r"\d+")
    hold(r"\{L\}", r".+")
    hold(r"\{COUNTER\}", r"\d+")
    escaped = re.escape(text)
    for key, value in placeholders.items():
        escaped = escaped.replace(re.escape(key), value)
    return f"^{escaped}$"


def prepare_pattern(text: str, enable_regex: bool) -> tuple[str, bool]:
    text = (text or "").strip()
    if not text:
        return "", False
    has_tokens = bool(re.search(r"\{[CSNL]|\{COUNTER\}", text))
    if enable_regex:
        text = expand_gina_tokens(text)
        text = fix_atomic_groups(text)
        return text, True
    if has_tokens:
        return gina_plain_to_regex(text), True
    return text, False


def convert_trigger(t: ET.Element, path_id: str, idx: int) -> dict | None:
    name = (t.findtext("Name") or f"Trigger {idx}").strip()
    search_raw = (t.findtext("TriggerText") or "").strip()
    enable_regex = t.findtext("EnableRegex") == "True"
    comments = (t.findtext("Comments") or "").strip() or None

    search, use_regex = prepare_pattern(search_raw, enable_regex)
    if not search:
        return None

    display = None
    if t.findtext("UseText") == "True":
        display = (t.findtext("DisplayText") or "").strip() or name

    speak = None
    if t.findtext("UseTextToVoice") == "True":
        speak = (t.findtext("TextToVoiceText") or "").strip() or None

    # TTS-only: keep a toast so the alert is visible without speech hardware.
    if display is None and speak:
        display = speak

    ttype = t.findtext("TimerType") or "NoTimer"
    ms = int(float(t.findtext("TimerMillisecondDuration") or 0))
    timer_seconds = None
    timer_name = None
    if ttype in ("Timer", "RepeatingTimer") and ms > 0:
        timer_seconds = max(1, ms // 1000)
        timer_name = (t.findtext("TimerName") or "").strip() or name

    early_end: list[str] = []
    early_el = t.find("TimerEarlyEnders")
    if early_el is not None:
        for ee in early_el.findall("EarlyEnder"):
            et_raw = (ee.findtext("EarlyEndText") or "").strip()
            if not et_raw:
                continue
            ee_regex = ee.findtext("EnableRegex") == "True"
            et, _ = prepare_pattern(et_raw, ee_regex or use_regex)
            if et:
                early_end.append(et)

    has_action = bool(display) or bool(speak) or timer_seconds is not None
    if not has_action:
        return None

    return {
        "id": f"{path_id}-{idx}",
        "name": name,
        "enabled": True,
        "search": search,
        "use_regex": use_regex,
        "display_text": display,
        "timer_seconds": timer_seconds,
        "timer_name": timer_name,
        "early_end": early_end,
        "sound": None,
        "speak": speak,
        "comments": comments,
    }


def walk_groups(group: ET.Element, path: list[str], out_groups: list[dict]) -> None:
    name = group.findtext("Name") or "Unnamed"
    full = path + [name]
    triggers_el = group.find("Triggers")
    if triggers_el is not None:
        trigs = list(triggers_el.findall("Trigger"))
        if trigs:
            path_label = " / ".join(full)
            path_id = hashlib.md5(path_label.encode()).hexdigest()[:12]
            converted = []
            for i, t in enumerate(trigs):
                c = convert_trigger(t, path_id, i)
                if c:
                    converted.append(c)
            if converted:
                out_groups.append(
                    {
                        "id": path_id,
                        "name": path_label,
                        "enabled": False,
                        "triggers": converted,
                    }
                )
    tg = group.find("TriggerGroups")
    if tg is not None:
        for child in tg.findall("TriggerGroup"):
            walk_groups(child, full, out_groups)


def load_share_xml(gtp_path: Path) -> ET.Element:
    if gtp_path.suffix.lower() == ".xml":
        return ET.parse(gtp_path).getroot()
    with zipfile.ZipFile(gtp_path) as zf:
        # Prefer ShareData.xml; otherwise first xml
        names = zf.namelist()
        target = None
        for n in names:
            if n.endswith("ShareData.xml"):
                target = n
                break
        if target is None:
            for n in names:
                if n.lower().endswith(".xml"):
                    target = n
                    break
        if target is None:
            raise SystemExit(f"No XML found in {gtp_path}")
        with zf.open(target) as f:
            return ET.parse(f).getroot()


def convert_gtp(gtp_path: Path) -> dict:
    root = load_share_xml(gtp_path)
    out_groups: list[dict] = []
    groups = root.find("TriggerGroups")
    if groups is None:
        raise SystemExit("Missing TriggerGroups in ShareData.xml")
    for g in groups.findall("TriggerGroup"):
        walk_groups(g, [], out_groups)
    return {"groups": out_groups}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "gtp",
        nargs="?",
        default=str(
            Path(__file__).resolve().parents[1] / "GINA" / "gina_pack.gtp"
        ),
    )
    parser.add_argument(
        "-o",
        "--output",
        default=str(
            Path(__file__).resolve().parents[1]
            / "samples"
            / "gina_pack.triggers.json"
        ),
    )
    args = parser.parse_args()
    lib = convert_gtp(Path(args.gtp))

    # Classic GINA packs still carry timers for buffs that are permanent on Legends.
    scripts_dir = Path(__file__).resolve().parent
    if str(scripts_dir) not in sys.path:
        sys.path.insert(0, str(scripts_dir))
    from eql_compat import load_permanent_names, strip_permanent_timers

    buffs = scripts_dir.parent / "samples" / "eql_permanent_buffs.json"
    changed = strip_permanent_timers(lib, load_permanent_names(buffs))
    if changed:
        print(f"EQL compat: stripped {len(changed)} permanent-buff timers")

    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(lib, indent=2) + "\n")
    n = sum(len(g["triggers"]) for g in lib["groups"])
    print(f"Wrote {n} triggers in {len(lib['groups'])} groups → {out}")


if __name__ == "__main__":
    main()
