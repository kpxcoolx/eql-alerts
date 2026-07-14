import rawMap from "../assets/overlay/map.json";

const ICON_BASE = "./icons/overlay/";

type IconMap = {
  defaultToast: string;
  defaultTimer: string;
  byDisplayText: Record<string, string>;
  byTimerName: Record<string, string>;
  byGroupHint: Record<string, string>;
  byClass: Record<string, string>;
};

const iconMap = rawMap as IconMap;

const CLASS_NAMES = new Set([
  "Bard",
  "Beastlord",
  "Berserker",
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
]);

/** Longer names first so "Shadow Knight" wins over shorter substrings. */
const CLASS_NAMES_SORTED = [...CLASS_NAMES].sort((a, b) => b.length - a.length);

export function overlayIconUrl(file: string): string {
  return `${ICON_BASE}${file}`;
}

export function classFromGroupName(groupName: string): string | null {
  const parts = groupName.split(" / ").map((p) => p.trim());
  const legacyIdx = parts.findIndex((p) => /class specific/i.test(p));
  if (legacyIdx >= 0 && legacyIdx + 1 < parts.length) {
    const name = parts[legacyIdx + 1];
    if (CLASS_NAMES.has(name)) return name;
  }
  if (parts[0] === "Classes" && parts.length > 1 && CLASS_NAMES.has(parts[1])) {
    return parts[1];
  }
  if (parts.length > 0 && CLASS_NAMES.has(parts[0])) {
    return parts[0];
  }
  for (const name of CLASS_NAMES_SORTED) {
    if (groupName.includes(name)) return name;
  }
  return null;
}

function groupHintFromName(groupName: string): string | null {
  for (const hint of Object.keys(iconMap.byGroupHint)) {
    if (groupName.includes(hint)) return hint;
  }
  return null;
}

/** Resolve toast row icon from display text, then group hint / class, else alert. */
export function resolveToastIcon(text: string, groupName?: string | null): string {
  const byText = iconMap.byDisplayText[text];
  if (byText) return overlayIconUrl(byText);

  if (groupName) {
    const hint = groupHintFromName(groupName);
    if (hint) {
      const file = iconMap.byGroupHint[hint];
      if (file) return overlayIconUrl(file);
    }
    const cls = classFromGroupName(groupName);
    if (cls) {
      const file = iconMap.byClass[cls];
      if (file) return overlayIconUrl(file);
    }
  }

  return overlayIconUrl(iconMap.defaultToast);
}

/** Resolve timer row icon: name map, class pack, group hint, else timer aura. */
export function resolveTimerIcon(name: string, groupName?: string | null): string {
  const byName = iconMap.byTimerName[name];
  if (byName) return overlayIconUrl(byName);

  if (groupName) {
    const cls = classFromGroupName(groupName);
    if (cls) {
      const file = iconMap.byClass[cls];
      if (file) return overlayIconUrl(file);
    }
    const hint = groupHintFromName(groupName);
    if (hint) {
      const file = iconMap.byGroupHint[hint];
      if (file) return overlayIconUrl(file);
    }
  }

  return overlayIconUrl(iconMap.defaultTimer);
}
