import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import QuickStart from "./QuickStart";
import {
  bindTtsPlayback,
  listAlertSounds,
  listAudioOutputDevices,
  playAlertSound,
  previewVoice,
  testSpeech,
  type AlertSoundInfo,
  type AudioOutputDevice,
} from "./speech";
import { formatCountdown } from "./time";
import {
  checkForAppUpdate,
  installAppUpdate,
  openLatestReleasePage,
  updateProgressLabel,
  updateProgressPercent,
  type PendingUpdate,
  type UpdateProgress,
} from "./updates";
import { getVersion } from "@tauri-apps/api/app";

type AppSettings = {
  last_log_path: string | null;
  auto_monitor_on_start: boolean;
  quick_start_dismissed: boolean;
  voice_id: string;
  voice_gender: string;
  voice_female: string;
  voice_male: string;
  voice_volume: number;
  audio_output_device: string;
  default_alert_sound: string;
  main_window: unknown;
  overlay_window: unknown;
};

type KokoroVoice = {
  id: string;
  label: string;
  gender: string;
  locale: string;
};
export type FoundLog = {
  path: string;
  character: string | null;
  modified_secs: number;
  size_bytes: number;
  source: string;
};

export type Trigger = {
  id: string;
  name: string;
  enabled: boolean;
  search: string;
  use_regex: boolean;
  display_text: string | null;
  timer_seconds: number | null;
  timer_name: string | null;
  early_end: string[];
  sound: string | null;
  speak: string | null;
  /** When true (default), alert speaks. Turn off to use a chime instead. */
  tts_enabled: boolean;
  comments: string | null;
};

export type TriggerGroup = {
  id: string;
  name: string;
  enabled: boolean;
  triggers: Trigger[];
};

export type TriggerLibrary = {
  groups: TriggerGroup[];
};

export type FiredAlert = {
  id: string;
  trigger_id: string;
  trigger_name: string;
  kind: string;
  text: string;
  at_ms: number;
};

export type ActiveTimer = {
  id: string;
  trigger_id: string;
  name: string;
  started_ms: number;
  ends_ms: number;
  duration_secs: number;
  captures?: string[];
};

export type EngineState = {
  character: string | null;
  log_path: string | null;
  monitoring: boolean;
  recent_alerts: FiredAlert[];
  timers: ActiveTimer[];
};

type Selection = { groupId: string; triggerId: string } | null;
type FilterMode = "all" | "armed" | "off";

type TreeNode = {
  key: string;
  label: string;
  children: TreeNode[];
  group: TriggerGroup | null;
};

type ClassPack = {
  name: string;
  groupIds: string[];
  triggerCount: number;
  enabledCount: number;
};

const MAX_ARMED_CLASSES = 3;

type Draft = {
  name: string;
  search: string;
  use_regex: boolean;
  display_text: string;
  timer_seconds: string;
  timer_name: string;
  early_end: string;
  sound: string;
  speak: string;
  tts_enabled: boolean;
  comments: string;
};

function newId(): string {
  return crypto.randomUUID();
}

function emptyTrigger(): Trigger {
  return {
    id: newId(),
    name: "New trigger",
    enabled: true,
    search: "",
    use_regex: false,
    display_text: "",
    timer_seconds: null,
    timer_name: "",
    early_end: [],
    sound: "ping",
    speak: "New trigger",
    tts_enabled: true,
    comments: null,
  };
}

function sortClassPacks(packs: ClassPack[]): ClassPack[] {
  return [...packs].sort((a, b) => a.name.localeCompare(b.name));
}

function classIconUrl(name: string): string {
  const slug = name.toLowerCase().replace(/\s+/g, "");
  return `./icons/overlay/classes/${slug}.png`;
}

function essentialsIcon(label: string): string {
  const map: Record<string, string> = {
    Core: "icon-zoning.png",
    Combat: "icon-enrage.png",
    Danger: "icon-death.png",
    Fades: "icon-fades.png",
    Social: "icon-buff.png",
  };
  const file = map[label] ?? "icon-alert.png";
  return `./icons/overlay/${file}`;
}

function classAccent(name: string): string {
  const accents: Record<string, string> = {
    Cleric: "#d4af37",
    Wizard: "#4fd1c5",
    Warrior: "#9b2c2c",
    Paladin: "#d4af37",
    "Shadow Knight": "#8b3a4a",
    Ranger: "#6b9e5a",
    Druid: "#6b9e5a",
    Shaman: "#5a8fbf",
    Enchanter: "#9b7ed8",
    Magician: "#c45a8a",
    Necromancer: "#7a6a9a",
    Monk: "#c9a46a",
    Rogue: "#a08060",
    Bard: "#d4a04a",
    Beastlord: "#6a9a7a",
    Berserker: "#b84a3a",
  };
  return accents[name] ?? "#e0a84a";
}

function groupBlurb(group: TriggerGroup): string | null {
  const note = group.triggers.find((t) => t.comments)?.comments?.trim();
  if (!note) return null;
  return note;
}

function groupArmedStatus(group: TriggerGroup): string | null {
  const n = group.triggers.length;
  if (n === 0) return null;
  // Group master switch must be on for triggers to fire — count effective armed.
  let on = 0;
  if (group.enabled) {
    on = group.triggers.filter((t) => t.enabled).length;
  }
  return `${on}/${n} triggers armed`;
}

function folderArmedStatus(node: TreeNode): string | null {
  const s = nodeStats(node);
  if (s.groups === 0) return null;
  return `${s.enabled}/${s.groups} sets · ${s.triggers} triggers`;
}

/** Armed triggers / total under a section (group must be on to count as armed). */
function nodeTriggerArmedCounts(node: TreeNode): { armed: number; total: number } {
  if (node.group) {
    const total = node.group.triggers.length;
    let armed = 0;
    if (node.group.enabled) {
      armed = node.group.triggers.filter((t) => t.enabled).length;
    }
    return { armed, total };
  }
  let armed = 0;
  let total = 0;
  for (const child of node.children) {
    const s = nodeTriggerArmedCounts(child);
    armed += s.armed;
    total += s.total;
  }
  return { armed, total };
}

function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString();
}

function remainingSecs(timer: ActiveTimer, now: number): number {
  return Math.max(0, Math.ceil((timer.ends_ms - now) / 1000));
}

function shortPath(path: string | null): string {
  if (!path) return "No log attached";
  const parts = path.split(/[/\\]/);
  return parts.slice(-2).join("/");
}

/** Strip EQ timestamp so try-line matches the engine. */
function actionFromLogLine(line: string): string {
  const trimmed = line.trim();
  if (!trimmed.startsWith("[")) return trimmed;
  const close = trimmed.indexOf("]");
  if (close < 0) return trimmed;
  return trimmed.slice(close + 1).trim();
}

function triggerMatchesAction(trigger: Trigger, action: string): boolean {
  if (!trigger.search) return false;
  if (!trigger.use_regex) return action.includes(trigger.search);
  try {
    return new RegExp(trigger.search).test(action);
  } catch {
    return false;
  }
}

function buildTree(groups: TriggerGroup[]): TreeNode[] {
  const root: TreeNode[] = [];

  function ensure(nodes: TreeNode[], label: string, key: string): TreeNode {
    let node = nodes.find((n) => n.label === label);
    if (!node) {
      node = { key, label, children: [], group: null };
      nodes.push(node);
    }
    return node;
  }

  for (const group of groups) {
    const parts = group.name
      .split(" / ")
      .map((p) => p.trim())
      .filter(Boolean);
    if (parts.length === 0) {
      root.push({
        key: group.id,
        label: group.name || "Untitled",
        children: [],
        group,
      });
      continue;
    }

    let nodes = root;
    let pathSoFar = "";
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      pathSoFar = pathSoFar ? `${pathSoFar} / ${part}` : part;
      const node = ensure(nodes, part, pathSoFar);
      if (i === parts.length - 1) {
        node.group = group;
      } else {
        nodes = node.children;
      }
    }
  }

  function sortNodes(nodes: TreeNode[]) {
    nodes.sort((a, b) => {
      const ra = rootRank(a.label);
      const rb = rootRank(b.label);
      if (ra !== rb) return ra - rb;
      return a.label.localeCompare(b.label);
    });
    for (const n of nodes) sortNodes(n.children);
  }
  sortNodes(root);
  if (!root.some((n) => n.label === "Custom")) {
    root.push({
      key: "Custom",
      label: "Custom",
      children: [],
      group: null,
    });
    sortNodes(root);
  }
  return root;
}

/** Essentials → Classes → Raids → Custom → everything else. */
function rootRank(label: string): number {
  if (label === "EQL Essentials") return 0;
  if (label === "Classes") return 1;
  if (label === "EQL Raids") return 2;
  if (label === "Custom") return 3;
  return 4;
}

function isCustomGroup(group: TriggerGroup): boolean {
  return group.name === "Custom" || group.name.startsWith("Custom / ");
}

function nodeStats(node: TreeNode): { groups: number; enabled: number; triggers: number } {
  if (node.group) {
    return {
      groups: 1,
      enabled: node.group.enabled ? 1 : 0,
      triggers: node.group.triggers.length,
    };
  }
  let groups = 0;
  let enabled = 0;
  let triggers = 0;
  for (const child of node.children) {
    const s = nodeStats(child);
    groups += s.groups;
    enabled += s.enabled;
    triggers += s.triggers;
  }
  return { groups, enabled, triggers };
}

function collectGroupIds(node: TreeNode): string[] {
  if (node.group) return [node.group.id];
  return node.children.flatMap(collectGroupIds);
}

/** First leaf group in tree order (for initial selection). */
function firstSelectable(nodes: TreeNode[]): TreeNode | null {
  for (const node of nodes) {
    if (node.group) return node;
    const child = firstSelectable(node.children);
    if (child) return child;
  }
  return null;
}

/** Pull "Classes / Cleric / …" (or legacy paths) into one-click class packs. */
const CLASS_NAMES = new Set([
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
]);

function extractClassPacks(groups: TriggerGroup[]): ClassPack[] {
  const map = new Map<string, ClassPack>();
  for (const group of groups) {
    const parts = group.name.split(" / ").map((p) => p.trim());
    let className: string | null = null;
    const legacyIdx = parts.findIndex((p) => /class specific/i.test(p));
    if (legacyIdx >= 0 && legacyIdx + 1 < parts.length) {
      className = parts[legacyIdx + 1];
    } else if (parts[0] === "Classes" && parts.length > 1 && CLASS_NAMES.has(parts[1])) {
      className = parts[1];
    } else if (parts.length > 0 && CLASS_NAMES.has(parts[0])) {
      className = parts[0];
    }
    if (!className || !CLASS_NAMES.has(className)) continue;
    let pack = map.get(className);
    if (!pack) {
      pack = { name: className, groupIds: [], triggerCount: 0, enabledCount: 0 };
      map.set(className, pack);
    }
    pack.groupIds.push(group.id);
    pack.triggerCount += group.triggers.length;
    if (group.enabled) pack.enabledCount += 1;
  }
  return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
}

function groupMatchesFilter(
  group: TriggerGroup,
  query: string,
  filter: FilterMode
): boolean {
  if (filter === "armed" && !group.enabled) return false;
  if (filter === "off" && group.enabled) return false;
  if (!query) return true;
  const q = query.toLowerCase();
  if (group.name.toLowerCase().includes(q)) return true;
  return group.triggers.some(
    (t) =>
      t.name.toLowerCase().includes(q) ||
      t.search.toLowerCase().includes(q) ||
      (t.display_text ?? "").toLowerCase().includes(q)
  );
}

function filterTree(nodes: TreeNode[], query: string, filter: FilterMode): TreeNode[] {
  const out: TreeNode[] = [];
  for (const node of nodes) {
    if (node.group && node.children.length === 0) {
      if (groupMatchesFilter(node.group, query, filter)) out.push(node);
      continue;
    }
    const kids = filterTree(node.children, query, filter);
    const selfMatch = node.group
      ? groupMatchesFilter(node.group, query, filter)
      : false;
    if (selfMatch || kids.length > 0) {
      out.push({
        ...node,
        children: kids,
        group: selfMatch ? node.group : null,
      });
    } else if (
      node.label === "Custom" &&
      !query &&
      filter === "all" &&
      !node.group &&
      node.children.length === 0
    ) {
      // Keep an empty Custom section so users can add their own triggers.
      out.push(node);
    }
  }
  return out;
}

function triggersMatchingQuery(group: TriggerGroup, query: string): Trigger[] {
  if (!query) return group.triggers;
  const q = query.toLowerCase();
  if (group.name.toLowerCase().includes(q)) return group.triggers;
  return group.triggers.filter(
    (t) =>
      t.name.toLowerCase().includes(q) ||
      t.search.toLowerCase().includes(q) ||
      (t.display_text ?? "").toLowerCase().includes(q)
  );
}

function triggerToDraft(t: Trigger): Draft {
  return {
    name: t.name,
    search: t.search,
    use_regex: t.use_regex,
    display_text: t.display_text ?? "",
    timer_seconds: t.timer_seconds == null ? "" : String(t.timer_seconds),
    timer_name: t.timer_name ?? "",
    early_end: t.early_end.join("\n"),
    sound: t.sound ?? "ping",
    speak: t.speak ?? t.display_text ?? t.name,
    tts_enabled: t.tts_enabled !== false,
    comments: t.comments ?? "",
  };
}

function draftToTrigger(id: string, draft: Draft): Trigger {
  const timerRaw = draft.timer_seconds.trim();
  // TTS mode: no chime. Sound mode: keep the pick (including "none" = visual only).
  let sound: string | null = null;
  if (!draft.tts_enabled) {
    const picked = draft.sound.trim() || "none";
    sound = picked;
  }
  return {
    id,
    name: draft.name.trim() || "Untitled",
    enabled: true,
    search: draft.search,
    use_regex: draft.use_regex,
    display_text: draft.display_text.trim() || null,
    timer_seconds: timerRaw === "" ? null : Number(timerRaw),
    timer_name: draft.timer_name.trim() || null,
    early_end: draft.early_end
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean),
    sound,
    speak: draft.speak.trim() || draft.display_text.trim() || draft.name,
    tts_enabled: draft.tts_enabled,
    comments: draft.comments.trim() || null,
  };
}

export default function App() {
  const [library, setLibrary] = useState<TriggerLibrary>({ groups: [] });
  const [engine, setEngine] = useState<EngineState | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<FilterMode>("all");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [draft, setDraft] = useState<Draft | null>(null);
  const [draftDirty, setDraftDirty] = useState(false);
  const [tryLine, setTryLine] = useState("");
  const [showQuickStart, setShowQuickStart] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [alertSounds, setAlertSounds] = useState<AlertSoundInfo[]>([]);
  const [kokoroVoices, setKokoroVoices] = useState<KokoroVoice[]>([]);
  const [audioDevices, setAudioDevices] = useState<AudioOutputDevice[]>([]);
  const [appVersion, setAppVersion] = useState("");
  const [pendingUpdate, setPendingUpdate] = useState<PendingUpdate | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [triggersLoading, setTriggersLoading] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [showAddClass, setShowAddClass] = useState(false);
  const [inspecting, setInspecting] = useState(false);
  const [overlayOpen, setOverlayOpen] = useState(false);
  const [pendingNameFocus, setPendingNameFocus] = useState(false);
  const voicePreviewTimer = useRef<number | null>(null);
  const mainScrollRef = useRef<HTMLDivElement | null>(null);
  const nameInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    return () => {
      if (voicePreviewTimer.current != null) {
        window.clearTimeout(voicePreviewTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!inspecting) return;
    const el = mainScrollRef.current;
    if (!el) return;
    el.scrollTop = 0;
  }, [inspecting, selection?.groupId, selection?.triggerId]);

  useEffect(() => {
    if (!pendingNameFocus || !draft || !inspecting) return;
    setPendingNameFocus(false);
    window.requestAnimationFrame(() => {
      const el = nameInputRef.current;
      if (!el) return;
      el.focus();
      el.select();
    });
  }, [pendingNameFocus, draft, inspecting, selection?.triggerId]);

  useEffect(() => {
    invoke<TriggerLibrary>("get_triggers")
      .then((lib) => {
        setLibrary(lib);
        const tree = buildTree(lib.groups);
        const nextExpanded: Record<string, boolean> = {};
        for (const node of tree) {
          if (node.label === "EQL Essentials" || node.label === "Custom") {
            nextExpanded[node.key] = true;
          } else if (node.label === "Classes" || node.label === "EQL Raids") {
            nextExpanded[node.key] = false;
          }
        }
        setExpanded(nextExpanded);
        const first = firstSelectable(tree);
        if (first?.group) {
          setSelection({
            groupId: first.group.id,
            triggerId: first.group.triggers[0]?.id ?? null,
          });
        }
      })
      .catch((err) => setError(String(err)))
      .finally(() => setTriggersLoading(false));

    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setShowQuickStart(!s.quick_start_dismissed);
      })
      .catch(() => setShowQuickStart(true));

    void listAlertSounds()
      .then(setAlertSounds)
      .catch(() => setAlertSounds([{ id: "none", label: "None" }]));

    invoke<KokoroVoice[]>("list_kokoro_voices")
      .then(setKokoroVoices)
      .catch(() => setKokoroVoices([]));

    void listAudioOutputDevices()
      .then(setAudioDevices)
      .catch(() => setAudioDevices([]));

    void invoke("kokoro_status").catch(() => undefined);

    invoke<EngineState>("get_engine_state")
      .then(setEngine)
      .catch(() => setEngine(null));

    invoke<{ open: boolean }>("get_overlay_status")
      .then((status) => setOverlayOpen(status.open))
      .catch(() => setOverlayOpen(false));

    const unlisten = listen<EngineState>("alerts-update", (event) => {
      setEngine(event.payload);
    });
    const unlistenOverlay = listen<{ open: boolean }>("overlay-status", (event) => {
      setOverlayOpen(event.payload.open);
    });
    const unlistenTts = bindTtsPlayback();

    return () => {
      unlisten.then((fn) => fn());
      unlistenOverlay.then((fn) => fn());
      unlistenTts.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(""));
    checkForAppUpdate()
      .then((update) => {
        if (update) setPendingUpdate(update);
      })
      .catch(() => undefined);
  }, []);

  async function runUpdateCheck() {
    setUpdateBusy(true);
    setError(null);
    try {
      const update = await checkForAppUpdate();
      if (!update) {
        setPendingUpdate(null);
        setNote(
          appVersion
            ? `You're on the latest version (${appVersion})`
            : "You're on the latest version"
        );
        return;
      }
      setPendingUpdate(update);
      setNote(`Update ${update.version} is available`);
    } catch (err) {
      setPendingUpdate(null);
      setNote("Update check failed — use Latest release…");
      setError(String(err).replace(/^Error:\s*/, ""));
    } finally {
      setUpdateBusy(false);
    }
  }

  async function runInstallUpdate() {
    setUpdateBusy(true);
    setError(null);
    setNote(null);
    setUpdateProgress({ phase: "checking", downloaded: 0, total: null });
    try {
      await installAppUpdate((progress) => {
        setUpdateProgress(progress);
      });
    } catch (err) {
      setUpdateProgress(null);
      setNote("Update install failed — use Latest release…");
      setError(String(err).replace(/^Error:\s*/, ""));
    } finally {
      setUpdateBusy(false);
    }
  }

  const selected = useMemo(() => {
    if (!selection) return null;
    const group = library.groups.find((g) => g.id === selection.groupId);
    if (!group) return null;
    const trigger = group.triggers.find((t) => t.id === selection.triggerId);
    if (!trigger) return null;
    return { group, trigger };
  }, [library, selection]);

  useEffect(() => {
    if (!selected) {
      setDraft(null);
      setDraftDirty(false);
      return;
    }
    setDraft(triggerToDraft(selected.trigger));
    setDraftDirty(false);
  }, [selected?.trigger.id, selected?.group.id]);

  const tree = useMemo(() => buildTree(library.groups), [library.groups]);
  const visibleTree = useMemo(
    () => filterTree(tree, query.trim(), filter),
    [tree, query, filter]
  );
  const classPacks = useMemo(
    () => sortClassPacks(extractClassPacks(library.groups)),
    [library.groups]
  );

  const tryResult = useMemo(() => {
    if (!selected || !tryLine.trim()) return null;
    const action = actionFromLogLine(tryLine);
    const hit = triggerMatchesAction(selected.trigger, action);
    return { action, hit };
  }, [selected, tryLine]);

  async function persist(next: TriggerLibrary) {
    setLibrary(next);
    try {
      const saved = await invoke<TriggerLibrary>("save_triggers", { library: next });
      setLibrary(saved);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  function setGroupsEnabled(ids: string[], enabled: boolean) {
    const idSet = new Set(ids);
    // Optimistic UI — don't wait on a full library rewrite.
    setLibrary((prev) => ({
      groups: prev.groups.map((g) =>
        idSet.has(g.id) ? { ...g, enabled } : g
      ),
    }));
    void invoke("set_groups_enabled", { ids, enabled }).catch((err) => {
      setError(String(err));
      void invoke<TriggerLibrary>("get_triggers").then(setLibrary);
    });
  }

  function toggleClassPack(pack: ClassPack) {
    const allOn =
      pack.enabledCount === pack.groupIds.length && pack.groupIds.length > 0;
    if (allOn) {
      setGroupsEnabled(pack.groupIds, false);
      return;
    }
    const alreadyArmed = pack.enabledCount > 0;
    if (!alreadyArmed && armedPacks.length >= MAX_ARMED_CLASSES) {
      setError(
        `You can arm up to ${MAX_ARMED_CLASSES} classes at a time. Disarm one first.`
      );
      return;
    }
    setGroupsEnabled(pack.groupIds, true);
  }

  function selectTrigger(groupId: string, triggerId: string) {
    setSelection({ groupId, triggerId });
    setInspecting(true);
  }

  async function dismissQuickStart() {
    setShowQuickStart(false);
    const base = settings ?? {
      last_log_path: null,
      auto_monitor_on_start: true,
      quick_start_dismissed: true,
      voice_id: "bf_isabella",
      voice_gender: "female",
      voice_female: "bf_isabella",
      voice_male: "am_michael",
      voice_volume: 0.2,
      audio_output_device: "",
      default_alert_sound: "none",
      main_window: null,
      overlay_window: null,
    };
    const next = { ...base, quick_start_dismissed: true };
    setSettings(next);
    try {
      await invoke("save_app_settings", { settings: next });
    } catch (err) {
      setError(String(err));
    }
  }

  async function patchSettings(partial: Partial<AppSettings>) {
    const base = settings ?? {
      last_log_path: null,
      auto_monitor_on_start: true,
      quick_start_dismissed: true,
      voice_id: "bf_isabella",
      voice_gender: "female",
      voice_female: "bf_isabella",
      voice_male: "am_michael",
      voice_volume: 0.2,
      audio_output_device: "",
      default_alert_sound: "none",
      main_window: null,
      overlay_window: null,
    };
    const next = { ...base, ...partial };
    setSettings(next);
    try {
      await invoke("save_app_settings", { settings: next });
    } catch (err) {
      setError(String(err));
    }
  }

  async function attachLog(choose: boolean) {
    setError(null);
    try {
      let path: string | null = null;
      if (choose) {
        const picked = await open({
          multiple: false,
          filters: [{ name: "EQ logs", extensions: ["txt"] }],
        });
        if (typeof picked === "string") path = picked;
      } else {
        path = (await invoke<FoundLog>("auto_detect_log")).path;
      }
      if (!path) return;
      setEngine(
        await invoke<EngineState>("start_monitoring", {
          path,
          fromStart: false,
        })
      );
      setNote("Attached to EverQuest Legends log.");
    } catch (err) {
      setError(String(err));
    }
  }

  async function stop() {
    setEngine(await invoke<EngineState>("stop_monitoring"));
  }

  async function reconnect() {
    setError(null);
    const path = engine?.log_path || settings?.last_log_path || null;
    if (!path) {
      setError("No previous log to reconnect. Use Find log or Browse… first.");
      return;
    }
    try {
      setEngine(
        await invoke<EngineState>("start_monitoring", {
          path,
          fromStart: false,
        }),
      );
      setNote("Reconnected to log.");
    } catch (err) {
      setError(String(err));
    }
  }

  async function clearTimers() {
    try {
      const next = await invoke<EngineState>("clear_timers");
      setEngine(next);
      setNote(
        next.timers.length === 0
          ? "Cleared active timers."
          : `Timers remaining: ${next.timers.length}`,
      );
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  async function clearAlerts() {
    try {
      const next = await invoke<EngineState>("clear_alerts");
      setEngine(next);
      setNote("Cleared recent alerts.");
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  async function importGinaPack() {
    setError(null);
    setNote(null);
    try {
      const picked = await open({
        multiple: false,
        filters: [
          { name: "Trigger packs", extensions: ["gtp", "json", "xml"] },
        ],
      });
      if (typeof picked !== "string") return;
      const result = await invoke<{
        library: TriggerLibrary;
        groups: number;
        triggers: number;
      }>("import_triggers_path", { path: picked, merge: true });
      setLibrary(result.library);
      setExpanded({});
      setNote(
        `Merged ${result.triggers} triggers. Arm your class with the chips above.`
      );
    } catch (err) {
      setError(String(err));
    }
  }

  async function restoreStarter() {
    setError(null);
    if (
      !window.confirm(
        "Replace built-in triggers with EQL defaults (Essentials + classes + classic EQL Raids)? Your Custom sets are kept; edits to built-in sets are overwritten."
      )
    ) {
      return;
    }
    try {
      const customGroups = library.groups.filter(isCustomGroup);
      const result = await invoke<{
        library: TriggerLibrary;
        groups: number;
        triggers: number;
      }>("install_starter_pack");
      const merged: TriggerLibrary = {
        groups: [...result.library.groups, ...customGroups],
      };
      if (customGroups.length > 0) {
        await persist(merged);
      } else {
        setLibrary(result.library);
      }
      setExpanded({ Custom: true });
      const kept =
        customGroups.length > 0
          ? ` Kept ${customGroups.length} custom set${customGroups.length === 1 ? "" : "s"}.`
          : "";
      setNote(
        `Defaults restored — ${result.groups} groups / ${result.triggers} triggers.${kept} Arm your class chip; open EQL Raids for zone bosses.`
      );
    } catch (err) {
      setError(String(err));
    }
  }

  function patchDraft(partial: Partial<Draft>) {
    setDraft((prev) => (prev ? { ...prev, ...partial } : prev));
    setDraftDirty(true);
  }

  /** Rename trigger; keep Speak in sync when it still matches the old name. */
  function patchTriggerName(name: string) {
    setDraft((prev) => {
      if (!prev) return prev;
      const next: Draft = { ...prev, name };
      if (prev.speak === prev.name) {
        next.speak = name;
      }
      return next;
    });
    setDraftDirty(true);
  }

  function saveDraft() {
    if (!selected || !draft) return;
    const next = draftToTrigger(selected.trigger.id, draft);
    void persist({
      groups: library.groups.map((group) => {
        if (group.id !== selected.group.id) return group;
        return {
          ...group,
          triggers: group.triggers.map((trigger) => {
            if (trigger.id !== selected.trigger.id) return trigger;
            return next;
          }),
        };
      }),
    });
    setDraftDirty(false);
  }

  async function fireInOverlay() {
    if (!selected || !draft) return;
    setError(null);
    try {
      await invoke("open_overlay");
      setOverlayOpen(true);
      const sample = tryLine.trim() ? actionFromLogLine(tryLine) : null;
      const next = await invoke<EngineState>("test_trigger", {
        trigger: draftToTrigger(selected.trigger.id, draft),
        sampleAction: sample,
      });
      setEngine(next);
      setNote(
        sample
          ? "Fired in overlay using your try line."
          : "Fired in overlay. Paste a log line above to expand ${1} / {S}."
      );
    } catch (err) {
      setError(`Overlay test failed: ${String(err)}`);
      setNote(null);
    }
  }

  function deleteSelected() {
    if (!selected) return;
    if (!window.confirm(`Delete “${selected.trigger.name}”?`)) return;
    void persist({
      groups: library.groups
        .map((group) => {
          if (group.id !== selected.group.id) return group;
          return {
            ...group,
            triggers: group.triggers.filter((t) => t.id !== selected.trigger.id),
          };
        })
        .filter((g) => g.triggers.length > 0),
    });
    setSelection(null);
    setInspecting(false);
  }

  function addSibling() {
    if (!selected) return;
    const trigger = emptyTrigger();
    void persist({
      groups: library.groups.map((group) => {
        if (group.id !== selected.group.id) return group;
        return { ...group, triggers: [...group.triggers, trigger] };
      }),
    });
    setSelection({ groupId: selected.group.id, triggerId: trigger.id });
    setInspecting(true);
    setPendingNameFocus(true);
  }

  function addCustomTrigger() {
    const trigger = emptyTrigger();
    const existing = library.groups.find((g) => g.name === "Custom");
    if (existing) {
      void persist({
        groups: library.groups.map((group) => {
          if (group.id !== existing.id) return group;
          return { ...group, triggers: [...group.triggers, trigger] };
        }),
      });
      setSelection({ groupId: existing.id, triggerId: trigger.id });
    } else {
      const group: TriggerGroup = {
        id: newId(),
        name: "Custom",
        enabled: true,
        triggers: [trigger],
      };
      void persist({ groups: [...library.groups, group] });
      setSelection({ groupId: group.id, triggerId: trigger.id });
    }
    setExpanded((prev) => ({ ...prev, Custom: true }));
    setInspecting(true);
    setShowSettings(false);
    setPendingNameFocus(true);
  }

  function deleteCustomGroup(groupId: string) {
    const group = library.groups.find((g) => g.id === groupId);
    if (!group || !isCustomGroup(group) || group.name === "Custom") return;
    if (
      !window.confirm(
        `Delete custom set “${group.name.replace(/^Custom \/ /, "")}” and all its triggers?`
      )
    ) {
      return;
    }
    void persist({
      groups: library.groups.filter((g) => g.id !== groupId),
    });
    if (selection?.groupId === groupId) {
      setSelection(null);
      setInspecting(false);
    }
  }

  function toggleExpand(key: string) {
    setExpanded((e) => ({ ...e, [key]: !e[key] }));
  }

  function focusClassInTriggers(className: string) {
    setInspecting(false);
    setShowSettings(false);
    const classKey = `Classes / ${className}`;
    setExpanded((prev) => ({
      ...prev,
      Classes: true,
      [classKey]: true,
    }));
    window.requestAnimationFrame(() => {
      const el = document.querySelector(`[data-tree-key="${CSS.escape(classKey)}"]`);
      if (el instanceof HTMLElement) {
        el.scrollIntoView({ behavior: "smooth", block: "nearest" });
      }
    });
  }

  function setTriggerEnabled(groupId: string, triggerId: string, enabled: boolean) {
    void persist({
      groups: library.groups.map((g) => {
        if (g.id !== groupId) return g;
        return {
          ...g,
          triggers: g.triggers.map((t) =>
            t.id === triggerId ? { ...t, enabled } : t
          ),
        };
      }),
    });
  }

  function rowIcon(node: TreeNode): string {
    if (node.group) {
      const parts = node.group.name.split(" / ").map((p) => p.trim());
      if (parts[0] === "EQL Essentials" && parts[1]) {
        return essentialsIcon(parts[1]);
      }
      for (const name of CLASS_NAMES) {
        if (parts.includes(name) || node.label === name) {
          return classIconUrl(name);
        }
      }
      return "./icons/overlay/icon-alert.png";
    }
    if (CLASS_NAMES.has(node.label)) return classIconUrl(node.label);
    if (node.label === "EQL Essentials" || node.key.includes("EQL Essentials")) {
      return "./icons/overlay/icon-alert.png";
    }
    return "./icons/overlay/icon-alert.png";
  }

  function rowAccent(node: TreeNode): string | undefined {
    if (CLASS_NAMES.has(node.label)) return classAccent(node.label);
    if (node.group) {
      const parts = node.group.name.split(" / ").map((p) => p.trim());
      for (const name of CLASS_NAMES) {
        if (parts.includes(name)) return classAccent(name);
      }
    }
    return undefined;
  }

  function renderTriggerLeaves(group: TriggerGroup): ReactNode {
    const shown = triggersMatchingQuery(group, query.trim());
    if (shown.length === 0) {
      return <div className="quiet">No triggers match.</div>;
    }
    return shown.map((trigger) => {
      const active =
        selection?.groupId === group.id && selection?.triggerId === trigger.id;
      return (
        <div
          key={trigger.id}
          className={`trigger-leaf ${active ? "active" : ""}`}
          onClick={() => selectTrigger(group.id, trigger.id)}
        >
          <button
            type="button"
            className={`switch ${trigger.enabled ? "on" : ""}`}
            style={
              trigger.enabled
                ? ({ "--switch-accent": "var(--amber)" } as CSSProperties)
                : undefined
            }
            aria-label={trigger.enabled ? "Disable trigger" : "Enable trigger"}
            title={trigger.enabled ? "Disable this trigger" : "Enable this trigger"}
            onClick={(e) => {
              e.stopPropagation();
              setTriggerEnabled(group.id, trigger.id, !trigger.enabled);
            }}
          />
          <div className="body">
            <div className="title-row">
              <div className="title">{trigger.name}</div>
              <div className="tags">
                {trigger.timer_seconds ? (
                  <span className="tag timer">
                    {formatCountdown(trigger.timer_seconds)}
                  </span>
                ) : (
                  <span className="tag timer empty" aria-hidden />
                )}
                {trigger.tts_enabled !== false ? (
                  <span className="tag">TTS</span>
                ) : (
                  <span className="tag">SFX</span>
                )}
              </div>
            </div>
            <span className="pattern">
              {trigger.search || "(empty pattern)"}
            </span>
          </div>
        </div>
      );
    });
  }

  function renderGroupRow(node: TreeNode): ReactNode {
    if (!node.group) return null;
    const group = node.group;
    const searching = query.trim().length > 0;
    const isOpen = searching || expanded[node.key] === true;
    const accent = rowAccent(node);
    const blurb = groupBlurb(group);
    const armed = groupArmedStatus(group);
    const custom = isCustomGroup(group);
    return (
      <div key={node.key}>
        <div
          className={`trigger-row ${selection?.groupId === group.id ? "active" : ""}`}
          style={
            accent
              ? ({ "--row-accent": accent } as CSSProperties)
              : undefined
          }
        >
          <img
            className="trigger-row-icon"
            src={rowIcon(node)}
            alt=""
            onError={(e) => {
              (e.target as HTMLImageElement).src =
                "./icons/overlay/icon-alert.png";
            }}
          />
          <div
            className="trigger-row-copy"
            role="button"
            tabIndex={0}
            onClick={() => {
              toggleExpand(node.key);
              if (selection?.groupId !== group.id) {
                const first = group.triggers[0];
                if (first) {
                  setSelection({ groupId: group.id, triggerId: first.id });
                  setInspecting(false);
                }
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                toggleExpand(node.key);
              }
            }}
          >
            <div className="title-line">
              <span className="title">{node.label}</span>
              {blurb ? <span className="blurb">: {blurb}</span> : null}
            </div>
            {armed ? <div className="desc">{armed}</div> : null}
          </div>
          {custom ? (
            <button
              type="button"
              className="btn ghost sm custom-delete-set"
              title="Delete this custom set"
              onClick={(e) => {
                e.stopPropagation();
                deleteCustomGroup(group.id);
              }}
            >
              Delete
            </button>
          ) : null}
          <button
            type="button"
            className={`switch ${group.enabled ? "on" : ""}`}
            style={
              accent
                ? ({ "--switch-accent": accent } as CSSProperties)
                : undefined
            }
            aria-label={group.enabled ? "Disarm set" : "Arm set"}
            title={group.enabled ? "Disarm this set" : "Arm this set"}
            onClick={(e) => {
              e.stopPropagation();
              setGroupsEnabled([group.id], !group.enabled);
            }}
          />
          <button
            type="button"
            className="trigger-row-nav"
            title={isOpen ? "Collapse" : "Open triggers"}
            onClick={() => {
              if (!isOpen) toggleExpand(node.key);
              const first = group.triggers[0];
              if (first) selectTrigger(group.id, first.id);
            }}
          >
            ›
          </button>
        </div>
        {isOpen ? (
          <div className="trigger-kids">{renderTriggerLeaves(group)}</div>
        ) : null}
      </div>
    );
  }

  function renderFolderRows(node: TreeNode): ReactNode {
    if (node.group) return renderGroupRow(node);
    const searching = query.trim().length > 0;
    const isOpen = searching || expanded[node.key] === true;
    const statsNode = nodeStats(node);
    const allOn = statsNode.enabled === statsNode.groups && statsNode.groups > 0;
    const accent = rowAccent(node);
    const armed = folderArmedStatus(node);
    return (
      <div key={node.key} data-tree-key={node.key}>
        <div
          className="trigger-row"
          style={
            accent
              ? ({ "--row-accent": accent } as CSSProperties)
              : undefined
          }
        >
          <img
            className="trigger-row-icon"
            src={rowIcon(node)}
            alt=""
            onError={(e) => {
              (e.target as HTMLImageElement).style.visibility = "hidden";
            }}
          />
          <div
            className="trigger-row-copy"
            role="button"
            tabIndex={0}
            onClick={() => toggleExpand(node.key)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                toggleExpand(node.key);
              }
            }}
          >
            <div className="title-line">
              <span className="title">{node.label}</span>
            </div>
            {armed ? <div className="desc">{armed}</div> : null}
          </div>
          <button
            type="button"
            className={`switch ${allOn ? "on" : ""}`}
            style={
              accent
                ? ({ "--switch-accent": accent } as CSSProperties)
                : undefined
            }
            aria-label={allOn ? "Disarm folder" : "Arm folder"}
            title={allOn ? "Disarm all sets in this folder" : "Arm all sets in this folder"}
            onClick={(e) => {
              e.stopPropagation();
              setGroupsEnabled(collectGroupIds(node), !allOn);
            }}
          />
          <button
            type="button"
            className="trigger-row-nav"
            title={isOpen ? "Collapse" : "Expand"}
            onClick={() => toggleExpand(node.key)}
          >
            {isOpen ? "▾" : "›"}
          </button>
        </div>
        {isOpen ? (
          <div className="trigger-kids">
            {node.children.map((child) => renderFolderRows(child))}
          </div>
        ) : null}
      </div>
    );
  }

  function renderRootSection(node: TreeNode): ReactNode {
    const searching = query.trim().length > 0;
    const isCustom = node.label === "Custom";
    let isOpen = false;
    if (searching) {
      isOpen = true;
    } else if (node.label === "EQL Essentials" || isCustom) {
      isOpen = expanded[node.key] !== false;
    } else {
      isOpen = expanded[node.key] === true;
    }
    const customHasLeaves = !!(node.group && node.group.triggers.length > 0);
    const customEmpty =
      isCustom && !customHasLeaves && node.children.length === 0;
    const fullNode = tree.find((n) => n.key === node.key) ?? node;
    const counts = nodeTriggerArmedCounts(fullNode);
    return (
      <section className="trigger-group" key={node.key}>
        <button
          type="button"
          className="trigger-group-head"
          title={isOpen ? `Collapse ${node.label}` : `Expand ${node.label}`}
          onClick={() =>
            setExpanded((e) => ({
              ...e,
              [node.key]: e[node.key] === false ? true : false,
            }))
          }
        >
          <span>{node.label}</span>
          <span className="grow" />
          {counts.total > 0 ? (
            <span
              className="trigger-group-count"
              title={`${counts.armed} of ${counts.total} triggers armed`}
            >
              {counts.armed}/{counts.total}
            </span>
          ) : null}
          <span className="chev">{isOpen ? "▴" : "▾"}</span>
        </button>
        {isOpen ? (
          <div className="trigger-group-body">
            {isCustom ? (
              <>
                {customEmpty ? (
                  <div className="custom-empty">
                    <p>
                      Add your own log-line triggers here — patterns, timers, and
                      TTS.
                    </p>
                    <button
                      type="button"
                      className="btn sm primary"
                      title="Create a custom trigger"
                      onClick={addCustomTrigger}
                    >
                      Add custom trigger
                    </button>
                  </div>
                ) : (
                  <>
                    {node.group ? (
                      <div className="custom-leaves">
                        {renderTriggerLeaves(node.group)}
                      </div>
                    ) : null}
                    {node.children.map((child) => renderFolderRows(child))}
                    <div className="custom-add-row">
                      <button
                        type="button"
                        className="btn sm primary"
                        title="Create a custom trigger"
                        onClick={addCustomTrigger}
                      >
                        Add custom trigger
                      </button>
                    </div>
                  </>
                )}
              </>
            ) : node.group ? (
              renderGroupRow(node)
            ) : (
              node.children.map((child) => renderFolderRows(child))
            )}
          </div>
        ) : null}
      </section>
    );
  }

  const live = !!engine?.monitoring;
  const armedPacks = classPacks.filter((p) => p.enabledCount > 0);
  const activeTimers = (engine?.timers ?? []).filter((t) => t.ends_ms > now);
  const installPercent = updateProgress
    ? updateProgressPercent(updateProgress)
    : null;

  return (
    <div className="app">
      {showQuickStart ? (
        <QuickStart
          onSkip={() => void dismissQuickStart()}
          onDone={() => {
            void invoke("open_overlay");
            void dismissQuickStart();
          }}
          onFindLog={() => void attachLog(false)}
        />
      ) : null}

      <header className="topbar">
        <div className="brand">
          <img className="brand-mark" src="./icons/overlay/icon-alert.png" alt="" />
          <span className="brand-title">EQL Alerts</span>
          {appVersion ? (
            <span className="brand-version">v{appVersion}</span>
          ) : null}
        </div>
      </header>

      {pendingUpdate && !updateProgress ? (
        <div className="update-banner">
          <span>
            Update <strong>{pendingUpdate.version}</strong> is available
            {appVersion ? ` (you have ${appVersion})` : ""}.
          </span>
          <button
            type="button"
            className="btn gold"
            disabled={updateBusy}
            title="Download and install the available update"
            onClick={() => void runInstallUpdate()}
          >
            Install update
          </button>
          <button
            type="button"
            className="btn"
            disabled={updateBusy}
            title="Open release notes in your browser"
            onClick={() => void openLatestReleasePage()}
          >
            Release notes
          </button>
        </div>
      ) : null}

      {error ? (
        <div className="banner error">
          <span className="grow">{error}</span>
          <button
            className="btn sm ghost"
            type="button"
            title="Dismiss this error"
            onClick={() => setError(null)}
          >
            Dismiss
          </button>
        </div>
      ) : null}
      {!error && note ? (
        <div className="banner info">
          <span className="grow">{note}</span>
          <button
            className="btn sm ghost"
            type="button"
            title="Dismiss this message"
            onClick={() => setNote(null)}
          >
            Got it
          </button>
        </div>
      ) : null}

      <div className="shell">
        <aside className="sidebar">
          <div className="sidebar-art" aria-hidden />
          <div className="sidebar-inner">
            <div className="sidebar-label">Classes</div>
            <div className="sidebar-list">
              {armedPacks.length === 0 ? (
                <div className="sidebar-empty">
                  No classes armed yet. Use + Add class to arm your pack.
                </div>
              ) : (
                armedPacks.slice(0, MAX_ARMED_CLASSES).map((pack) => {
                  const on =
                    pack.enabledCount === pack.groupIds.length &&
                    pack.groupIds.length > 0;
                  const partial =
                    pack.enabledCount > 0 &&
                    pack.enabledCount < pack.groupIds.length;
                  return (
                    <button
                      key={pack.name}
                      type="button"
                      className={`class-chip ${partial ? "partial" : ""}`}
                      style={
                        {
                          "--chip-accent": classAccent(pack.name),
                        } as CSSProperties
                      }
                      title={`Open ${pack.name} triggers`}
                      onClick={() => focusClassInTriggers(pack.name)}
                    >
                      <img
                        src={classIconUrl(pack.name)}
                        alt=""
                        onError={(e) => {
                          (e.target as HTMLImageElement).style.visibility =
                            "hidden";
                        }}
                      />
                      <span className="class-chip-name">{pack.name}</span>
                      {on || partial ? <span className="class-chip-dot" /> : null}
                    </button>
                  );
                })
              )}
            </div>
            <button
              type="button"
              className="sidebar-add"
              title="Arm another class pack (up to 3)"
              onClick={() => setShowAddClass(true)}
            >
              + Add class
            </button>

            <div className="sidebar-tools">
              <div className="sidebar-label">Tools</div>
              <button
                className={`sidebar-tool-btn ${overlayOpen ? "" : "accent"}`}
                type="button"
                title={
                  overlayOpen
                    ? "Hide the always-on-top timer and toast overlay"
                    : "Open the always-on-top timer and toast overlay"
                }
                onClick={() =>
                  void invoke(overlayOpen ? "close_overlay" : "open_overlay")
                }
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                  <rect x="3" y="5" width="18" height="14" rx="2" />
                  <path d="M8 19v2M16 19v2M3 10h18" />
                </svg>
                {overlayOpen ? "Close overlay" : "Overlay"}
              </button>
              <button
                className={`sidebar-tool-btn ${showSettings ? "accent" : ""}`}
                type="button"
                title="Settings"
                onClick={() => setShowSettings(true)}
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                  <path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
                  <path d="M19.4 13a7.8 7.8 0 0 0 .1-2l2-1.2-2-3.4-2.3.7a7.6 7.6 0 0 0-1.7-1L15 3.5h-6l-.5 2.6a7.6 7.6 0 0 0-1.7 1L4.5 6.4l-2 3.4 2 1.2a7.8 7.8 0 0 0 0 2l-2 1.2 2 3.4 2.3-.7a7.6 7.6 0 0 0 1.7 1l.5 2.6h6l.5-2.6a7.6 7.6 0 0 0 1.7-1l2.3.7 2-3.4-2-1.2Z" />
                </svg>
                Settings
              </button>
            </div>

            <section className="sidebar-activity">
              <div className="sidebar-activity-head">
                <div className="sidebar-label">Activity</div>
                <button
                  className="btn sm"
                  type="button"
                  title="Clear recent fired alerts and running timers"
                  onClick={() => void clearAlerts()}
                >
                  Clear
                </button>
              </div>
              <div className="sidebar-activity-body">
                  <div className="activity-col">
                    <h4>Timers</h4>
                    {activeTimers.length === 0 ? (
                      <div className="quiet">No running timers</div>
                    ) : (
                      activeTimers.map((timer) => {
                        const left = remainingSecs(timer, now);
                        const pct = Math.max(
                          0,
                          Math.min(
                            100,
                            ((timer.ends_ms - now) / (timer.duration_secs * 1000)) * 100
                          )
                        );
                        return (
                          <div className="timer" key={timer.id}>
                            <div className="t-head">
                              <div className="t-name">{timer.name}</div>
                              <button
                                type="button"
                                className="timer-dismiss"
                                title="Clear this timer"
                                onClick={() => {
                                  void invoke<EngineState>("clear_timer", {
                                    timerId: timer.id,
                                  })
                                    .then(setEngine)
                                    .catch((err) => setError(String(err)));
                                }}
                              >
                                ×
                              </button>
                            </div>
                            <div className="bar">
                              <i style={{ width: `${pct}%` }} />
                            </div>
                            <div className="left">{formatCountdown(left)}</div>
                          </div>
                        );
                      })
                    )}
                  </div>
                  <div className="activity-col">
                    <h4>Fired</h4>
                    {(engine?.recent_alerts.length ?? 0) === 0 ? (
                      <div className="quiet">Waiting for matches on the live log</div>
                    ) : (
                      engine!.recent_alerts.map((alert, i) => {
                        const showFrom =
                          alert.trigger_name.trim() !== "" &&
                          alert.trigger_name !== alert.text;
                        return (
                          <div
                            className={`event ${i === 0 ? "fresh" : ""}`}
                            key={alert.id}
                            title={`${alert.trigger_name} · ${alert.kind}`}
                          >
                            <div className="meta">
                              <span className="when">
                                {formatTime(alert.at_ms)}
                              </span>
                              <span className="kind">{alert.kind}</span>
                            </div>
                            <div className="text">{alert.text}</div>
                            {showFrom ? (
                              <div className="from">{alert.trigger_name}</div>
                            ) : null}
                          </div>
                        );
                      })
                    )}
                  </div>
                </div>
            </section>

            <div className={`sidebar-status ${live ? "live" : ""}`} title={engine?.log_path ?? ""}>
              <span className="dot" />
              {live
                ? `Connected to log${engine?.character ? ` · ${engine.character}` : ""}`
                : "Not connected to log"}
            </div>
          </div>
        </aside>

        <main className="main">
          <div className="main-scroll" ref={mainScrollRef}>
            {inspecting && selected && draft ? (
              <div className="inspector">
                <div className="inspector-chrome">
                  <div className="detail-bar">
                    <button
                      type="button"
                      className="btn ghost sm"
                      title="Return to the triggers list"
                      onClick={() => setInspecting(false)}
                    >
                      ← Triggers
                    </button>
                    <input
                      className="detail-title"
                      type="text"
                      value={draft.name}
                      placeholder="Untitled"
                      title="Trigger name"
                      aria-label="Trigger name"
                      onChange={(e) => patchTriggerName(e.target.value)}
                    />
                    <span className="grow" />
                    <button
                      className="btn sm"
                      type="button"
                      title="Add another trigger in this set"
                      onClick={addSibling}
                    >
                      Add trigger
                    </button>
                  </div>
                  <p className="path">{selected.group.name}</p>
                </div>

                <div className="inspector-form">
                  <div className="inspector-col">
                    <div className="field">
                      <span>Name</span>
                      <input
                        ref={nameInputRef}
                        type="text"
                        value={draft.name}
                        placeholder="Untitled"
                        onChange={(e) => patchTriggerName(e.target.value)}
                      />
                    </div>
                    <div className="try">
                      <label>Try a log line</label>
                      <textarea
                        placeholder="Paste a line from eqlog_*.txt to verify this pattern"
                        value={tryLine}
                        onChange={(e) => setTryLine(e.target.value)}
                      />
                      {tryResult ? (
                        <div className={`try-result ${tryResult.hit ? "hit" : "miss"}`}>
                          {tryResult.hit
                            ? "Would fire on that line"
                            : "No match — adjust the pattern"}
                        </div>
                      ) : (
                        <div className="try-result">
                          Strips the [timestamp] automatically, same as the live engine.
                        </div>
                      )}
                      <button
                        className="btn sm"
                        type="button"
                        title="Show this trigger in the overlay (toast, timer, and audio). Uses the try line for captures when filled in."
                        onClick={() => {
                          void fireInOverlay();
                        }}
                      >
                        Fire in overlay
                      </button>
                    </div>

                    <div className="field">
                      <span>Pattern</span>
                      <textarea
                        value={draft.search}
                        onChange={(e) => patchDraft({ search: e.target.value })}
                      />
                    </div>
                    <label className="check">
                      <input
                        type="checkbox"
                        checked={draft.use_regex}
                        onChange={(e) => patchDraft({ use_regex: e.target.checked })}
                      />
                      Regex pattern
                    </label>
                    <div className="field">
                      <span>Notes</span>
                      <input
                        type="text"
                        value={draft.comments}
                        onChange={(e) => patchDraft({ comments: e.target.value })}
                      />
                    </div>
                  </div>

                  <div className="inspector-col">
                    <div className="field">
                      <span>Toast text</span>
                      <input
                        type="text"
                        value={draft.display_text}
                        onChange={(e) => patchDraft({ display_text: e.target.value })}
                        placeholder="{C} and {S} supported"
                      />
                    </div>
                    <div className="grid-2">
                      <div className="field">
                        <span>Timer (sec)</span>
                        <input
                          type="number"
                          min={0}
                          value={draft.timer_seconds}
                          onChange={(e) => patchDraft({ timer_seconds: e.target.value })}
                        />
                      </div>
                      <div className="field">
                        <span>Timer label</span>
                        <input
                          type="text"
                          value={draft.timer_name}
                          onChange={(e) => patchDraft({ timer_name: e.target.value })}
                        />
                      </div>
                    </div>
                    <div className="field">
                      <span>Early end patterns</span>
                      <textarea
                        value={draft.early_end}
                        onChange={(e) => patchDraft({ early_end: e.target.value })}
                        placeholder="One per line"
                      />
                    </div>
                    <label className="check">
                      <input
                        type="checkbox"
                        checked={draft.tts_enabled}
                        onChange={(e) => patchDraft({ tts_enabled: e.target.checked })}
                      />
                      Speak with TTS (uncheck to use a chime instead)
                    </label>
                    {draft.tts_enabled ? (
                      <div className="field">
                        <span>Speak line</span>
                        <div className="row-inline">
                          <input
                            type="text"
                            value={draft.speak}
                            onChange={(e) => patchDraft({ speak: e.target.value })}
                            placeholder="What the voice says — e.g. Out of mana"
                          />
                          <button
                            className="btn"
                            type="button"
                            title="Speak this line with the selected Kokoro voice"
                            onClick={() => {
                              const line = draft.speak.trim() || "Alert test";
                              void testSpeech(line)
                                .then((msg) => {
                                  setNote(msg);
                                  setError(null);
                                })
                                .catch((err) => {
                                  setError(`Audio failed: ${String(err)}`);
                                  setNote(null);
                                });
                            }}
                          >
                            Test
                          </button>
                        </div>
                      </div>
                    ) : (
                      <div className="field">
                        <span>Alert sound</span>
                        <div className="row-inline">
                          <select
                            value={draft.sound || "ping"}
                            onChange={(e) => patchDraft({ sound: e.target.value })}
                          >
                            {alertSounds.map((s) => (
                              <option key={s.id} value={s.id}>
                                {s.label}
                              </option>
                            ))}
                          </select>
                          <button
                            className="btn"
                            type="button"
                            disabled={!draft.sound || draft.sound === "none"}
                            title="Preview the selected alert chime"
                            onClick={() => {
                              void playAlertSound(draft.sound).catch((err) =>
                                setError(`Sound failed: ${String(err)}`)
                              );
                            }}
                          >
                            Preview
                          </button>
                        </div>
                      </div>
                    )}
                  </div>

                  <div className="actions">
                    <button
                      className="btn primary"
                      type="button"
                      disabled={!draftDirty}
                      title="Save changes to this trigger"
                      onClick={saveDraft}
                    >
                      Save
                    </button>
                    <button
                      className="btn ghost"
                      type="button"
                      disabled={!draftDirty}
                      title="Discard unsaved edits"
                      onClick={() => {
                        if (selected) {
                          setDraft(triggerToDraft(selected.trigger));
                          setDraftDirty(false);
                        }
                      }}
                    >
                      Revert
                    </button>
                    <button
                      className="btn danger"
                      type="button"
                      title="Delete this trigger permanently"
                      onClick={deleteSelected}
                    >
                      Delete
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <>
                <div className="main-heading">
                  <h1 className="main-title">Triggers</h1>
                  <p className="main-sub">Configure when alerts are triggered.</p>
                </div>

                {triggersLoading ? (
                  <div className="empty loading-panel" aria-busy="true" aria-live="polite">
                    <p>Loading triggers…</p>
                    <div className="loading-bar" role="progressbar" aria-label="Loading triggers">
                      <i />
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="tools">
                      <input
                        className="search"
                        type="search"
                        title="Search triggers by name or pattern"
                        placeholder="Search name or pattern…"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                      />
                      <div className="filter-seg">
                        {(
                          [
                            ["all", "All"],
                            ["armed", "Armed"],
                            ["off", "Off"],
                          ] as const
                        ).map(([mode, label]) => {
                          let tip = "Show all trigger sets";
                          if (mode === "armed") tip = "Show only armed sets";
                          if (mode === "off") tip = "Show only disarmed sets";
                          return (
                          <button
                            key={mode}
                            type="button"
                            className={`btn sm ${filter === mode ? "gold" : "ghost"}`}
                            title={tip}
                            onClick={() => setFilter(mode)}
                          >
                            {label}
                          </button>
                          );
                        })}
                      </div>
                    </div>

                    {visibleTree.length === 0 ? (
                      <div className="empty">
                        {library.groups.length === 0 ? (
                          <>
                            <p>No triggers loaded. Restore the built-in defaults to get started.</p>
                            <button
                              className="btn sm primary"
                              type="button"
                              title="Install the built-in EQL Essentials, classes, and raids pack"
                              onClick={() => void restoreStarter()}
                            >
                              Restore default triggers
                            </button>
                          </>
                        ) : (
                          "Nothing matches that search."
                        )}
                      </div>
                    ) : (
                      visibleTree.map((node) => renderRootSection(node))
                    )}
                  </>
                )}
              </>
            )}
          </div>
        </main>
      </div>

      {updateProgress ? (
        <div
          className="update-progress-backdrop"
          role="alertdialog"
          aria-busy="true"
          aria-live="polite"
          aria-label="Installing update"
        >
          <div className="update-progress-card">
            <h2>Installing update</h2>
            {pendingUpdate ? (
              <p className="update-progress-version">
                Version {pendingUpdate.version}
                {appVersion ? ` (from ${appVersion})` : ""}
              </p>
            ) : null}
            <p className="update-progress-status">
              {updateProgressLabel(updateProgress)}
            </p>
            {installPercent != null ? (
              <div
                className="update-progress-bar"
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={installPercent}
              >
                <i style={{ width: `${installPercent}%` }} />
              </div>
            ) : (
              <div
                className="update-progress-bar indeterminate"
                role="progressbar"
                aria-label={updateProgressLabel(updateProgress)}
              >
                <i />
              </div>
            )}
            <p className="update-progress-hint">
              Keep the app open. The installer will open when the download
              finishes, then EQL Alerts will restart.
            </p>
          </div>
        </div>
      ) : null}

      {showSettings ? (
        <div
          className="drawer-backdrop"
          onClick={() => setShowSettings(false)}
        >
          <aside
            className="drawer"
            role="dialog"
            aria-label="Settings"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="drawer-head">
              <h2>Settings</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="Close settings"
                title="Close settings"
                onClick={() => setShowSettings(false)}
              >
                ✕
              </button>
            </div>
            <div className="drawer-body">
              <section className="settings-section">
                <h3>Log connection</h3>
                <p className="settings-note">{shortPath(engine?.log_path ?? null)}</p>
                <div className="settings-row">
                  <button
                    className="btn primary"
                    type="button"
                    title="Auto-find the newest EverQuest Legends character log"
                    onClick={() => void attachLog(false)}
                  >
                    Find log
                  </button>
                  <button
                    className="btn"
                    type="button"
                    title="Pick a log file manually"
                    onClick={() => void attachLog(true)}
                  >
                    Browse…
                  </button>
                  {live ? (
                    <button
                      className="btn ghost"
                      type="button"
                      title="Stop watching the log"
                      onClick={() => void stop()}
                    >
                      Disconnect
                    </button>
                  ) : (
                    <button
                      className="btn"
                      type="button"
                      title="Reconnect to the last known log"
                      disabled={!(engine?.log_path || settings?.last_log_path)}
                      onClick={() => void reconnect()}
                    >
                      Reconnect
                    </button>
                  )}
                </div>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={settings?.auto_monitor_on_start ?? true}
                    onChange={(e) =>
                      void patchSettings({ auto_monitor_on_start: e.target.checked })
                    }
                  />
                  Auto-monitor on start
                </label>
              </section>

              <section className="settings-section">
                <h3>Voice & audio</h3>
                <label className="audio-field">
                  <span className="audio-label">Voice</span>
                  <div className="audio-inline">
                    <select
                      className="voice-select"
                      value={
                        settings?.voice_id || settings?.voice_female || "bf_isabella"
                      }
                      onChange={(e) => {
                        const voice_id = e.target.value;
                        const male =
                          voice_id.startsWith("am_") || voice_id.startsWith("bm_");
                        void patchSettings({
                          voice_id,
                          voice_gender: male ? "male" : "female",
                          voice_female: male
                            ? settings?.voice_female ?? "bf_isabella"
                            : voice_id,
                          voice_male: male
                            ? voice_id
                            : settings?.voice_male ?? "am_michael",
                        });
                        if (voicePreviewTimer.current != null) {
                          window.clearTimeout(voicePreviewTimer.current);
                        }
                        voicePreviewTimer.current = window.setTimeout(() => {
                          void previewVoice(
                            voice_id,
                            "Out of mana",
                            settings?.voice_volume ?? 0.2
                          )
                            .then((msg) => {
                              setNote(msg);
                              setError(null);
                            })
                            .catch((err) => {
                              setError(`Voice preview failed: ${String(err)}`);
                            });
                        }, 450);
                      }}
                    >
                      <optgroup label="Female">
                        {(kokoroVoices.length
                          ? kokoroVoices
                          : [
                              {
                                id: "bf_isabella",
                                label: "Isabella (UK)",
                                gender: "female",
                                locale: "en-GB",
                              },
                              {
                                id: "af_bella",
                                label: "Bella",
                                gender: "female",
                                locale: "en-US",
                              },
                            ]
                        )
                          .filter((v) => v.gender === "female")
                          .map((v) => (
                            <option key={v.id} value={v.id}>
                              {v.label}
                              {v.locale !== "en-US" ? ` (${v.locale})` : ""}
                            </option>
                          ))}
                      </optgroup>
                      <optgroup label="Male">
                        {(kokoroVoices.length
                          ? kokoroVoices
                          : [
                              {
                                id: "am_michael",
                                label: "Michael",
                                gender: "male",
                                locale: "en-US",
                              },
                              {
                                id: "am_fenrir",
                                label: "Fenrir",
                                gender: "male",
                                locale: "en-US",
                              },
                            ]
                        )
                          .filter((v) => v.gender === "male")
                          .map((v) => (
                            <option key={v.id} value={v.id}>
                              {v.label}
                              {v.locale !== "en-US" ? ` (${v.locale})` : ""}
                            </option>
                          ))}
                      </optgroup>
                    </select>
                    <button
                      className="btn sm"
                      type="button"
                      title="Preview the selected Kokoro voice"
                      onClick={() => {
                        const voice_id =
                          settings?.voice_id ||
                          settings?.voice_female ||
                          "bf_isabella";
                        void previewVoice(
                          voice_id,
                          "Out of mana",
                          settings?.voice_volume ?? 0.2
                        )
                          .then((msg) => {
                            setNote(msg);
                            setError(null);
                          })
                          .catch((err) => {
                            setError(`Voice preview failed: ${String(err)}`);
                            setNote(null);
                          });
                      }}
                    >
                      Preview
                    </button>
                  </div>
                </label>
                <label className="audio-field">
                  <span className="audio-label">Volume</span>
                  <div className="audio-vol-line">
                    <input
                      type="range"
                      min={0}
                      max={100}
                      title="Voice volume"
                      value={Math.round((settings?.voice_volume ?? 0.2) * 100)}
                      onChange={(e) => {
                        const voice_volume = Number(e.target.value) / 100;
                        void patchSettings({ voice_volume });
                      }}
                      onPointerUp={(e) => {
                        const voice_volume = Number(e.currentTarget.value) / 100;
                        const voice_id =
                          settings?.voice_id ||
                          settings?.voice_female ||
                          "bf_isabella";
                        void previewVoice(
                          voice_id,
                          "Out of mana",
                          voice_volume
                        ).catch(() => undefined);
                      }}
                    />
                    <em className="voice-vol-pct">
                      {Math.round((settings?.voice_volume ?? 0.2) * 100)}%
                    </em>
                  </div>
                </label>
                <label className="audio-field">
                  <span className="audio-label">Output</span>
                  <select
                    className="output-select"
                    value={settings?.audio_output_device ?? ""}
                    onChange={(e) => {
                      const audio_output_device = e.target.value;
                      const voice_id =
                        settings?.voice_id ||
                        settings?.voice_female ||
                        "bf_isabella";
                      void patchSettings({ audio_output_device }).then(() =>
                        previewVoice(
                          voice_id,
                          "Out of mana",
                          settings?.voice_volume ?? 0.2
                        )
                          .then((msg) => {
                            setNote(msg);
                            setError(null);
                          })
                          .catch((err) => {
                            setError(`Output preview failed: ${String(err)}`);
                          })
                      );
                    }}
                  >
                    <option value="">System default</option>
                    {audioDevices.map((d) => {
                      let label = d.name;
                      if (d.is_default) label = `${label} · default`;
                      if (d.channels > 2) label = `${label} · ${d.channels}ch`;
                      return (
                        <option key={d.name} value={d.name}>
                          {label}
                        </option>
                      );
                    })}
                  </select>
                </label>
              </section>

              <section className="settings-section">
                <h3>Overlay & activity</h3>
                <div className="settings-row">
                  <button
                    className={overlayOpen ? "btn ghost" : "btn gold"}
                    type="button"
                    title={
                      overlayOpen
                        ? "Hide the always-on-top overlay"
                        : "Open the always-on-top timer and toast overlay"
                    }
                    onClick={() =>
                      void invoke(overlayOpen ? "close_overlay" : "open_overlay")
                    }
                  >
                    {overlayOpen ? "Close overlay" : "Open overlay"}
                  </button>
                  <button
                    className="btn ghost"
                    type="button"
                    title="Clear all running overlay timers"
                    onClick={() => void clearTimers()}
                  >
                    Clear timers
                  </button>
                  <button
                    className="btn ghost"
                    type="button"
                    title="Clear recent fired alerts from activity"
                    onClick={() => void clearAlerts()}
                  >
                    Clear alerts
                  </button>
                </div>
              </section>

              <section className="settings-section span-2">
                <h3>Trigger library</h3>
                <p className="settings-note">
                  Import packs or restore built-in defaults. Add your own triggers under
                  Custom on the Triggers page.
                </p>
                <div className="settings-row">
                  <button
                    className="btn"
                    type="button"
                    title="Create a custom trigger"
                    onClick={addCustomTrigger}
                  >
                    Add custom trigger
                  </button>
                  <button
                    className="btn"
                    type="button"
                    title="Import a GINA or EQL package into your library"
                    onClick={() => void importGinaPack()}
                  >
                    Import…
                  </button>
                </div>
                <div className="settings-restore">
                  <div className="settings-restore-copy">
                    <strong>Deleted triggers or wiped a set?</strong>
                    <span>
                      Restore the full default pack (EQL Essentials, all classes, and
                      classic EQL Raids). Custom sets are kept; edits to built-in sets
                      are overwritten.
                    </span>
                  </div>
                  <button
                    className="btn gold"
                    type="button"
                    title="Replace your library with the built-in EQL starter pack"
                    onClick={() => void restoreStarter()}
                  >
                    Restore default triggers
                  </button>
                </div>
              </section>

              <section className="settings-section">
                <h3>Updates</h3>
                <p className="settings-note">
                  {appVersion ? `Installed v${appVersion}` : "Version unknown"}
                  {pendingUpdate ? ` · ${pendingUpdate.version} available` : ""}
                </p>
                <div className="settings-row">
                  <button
                    className="btn"
                    type="button"
                    disabled={updateBusy}
                    title="Check GitHub for a newer EQL Alerts build"
                    onClick={() => void runUpdateCheck()}
                  >
                    {updateBusy ? "Checking…" : "Check for updates"}
                  </button>
                  {pendingUpdate ? (
                    <button
                      className="btn gold"
                      type="button"
                      disabled={updateBusy}
                      title={`Install update ${pendingUpdate.version}`}
                      onClick={() => void runInstallUpdate()}
                    >
                      {updateProgress
                        ? updateProgressLabel(updateProgress)
                        : `Install ${pendingUpdate.version}`}
                    </button>
                  ) : null}
                  <button
                    className="btn ghost"
                    type="button"
                    title="Open the latest GitHub release page"
                    onClick={() => void openLatestReleasePage()}
                  >
                    Latest release…
                  </button>
                </div>
              </section>

              <section className="settings-section">
                <h3>Help</h3>
                <div className="settings-row">
                  <button
                    className="btn ghost"
                    type="button"
                    title="Open the in-app quick start walkthrough"
                    onClick={() => {
                      setShowSettings(false);
                      setShowQuickStart(true);
                    }}
                  >
                    Quick start guide
                  </button>
                </div>
              </section>
            </div>
          </aside>
        </div>
      ) : null}

      {showAddClass ? (
        <div className="modal-backdrop" onClick={() => setShowAddClass(false)}>
          <div
            className="modal-card"
            role="dialog"
            aria-label="Add class"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-head">
              <h2>Add class</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="Close"
                title="Close"
                onClick={() => setShowAddClass(false)}
              >
                ✕
              </button>
            </div>
            <div className="modal-body">
              <p className="settings-note" style={{ fontFamily: "inherit" }}>
                Arm up to {MAX_ARMED_CLASSES} class packs for the sidebar. Disarm one
                to free a slot.
              </p>
              {classPacks.length === 0 ? (
                <div className="empty">
                  No class packs in this library. Install the starter pack from
                  Settings.
                </div>
              ) : (
                <div className="add-class-grid">
                  {classPacks.map((pack) => {
                    const on =
                      pack.enabledCount === pack.groupIds.length &&
                      pack.groupIds.length > 0;
                    const partial =
                      pack.enabledCount > 0 &&
                      pack.enabledCount < pack.groupIds.length;
                    const armed = on || partial;
                    const atCap =
                      !armed && armedPacks.length >= MAX_ARMED_CLASSES;
                    return (
                      <button
                        key={pack.name}
                        type="button"
                        className={`add-class-tile ${armed ? "on" : ""} ${atCap ? "locked" : ""}`}
                        style={
                          {
                            "--chip-accent": classAccent(pack.name),
                          } as CSSProperties
                        }
                        disabled={atCap}
                        title={
                          atCap
                            ? `Limit of ${MAX_ARMED_CLASSES} classes — disarm one first`
                            : undefined
                        }
                        onClick={() => toggleClassPack(pack)}
                      >
                        <img
                          src={classIconUrl(pack.name)}
                          alt=""
                          onError={(e) => {
                            (e.target as HTMLImageElement).style.visibility =
                              "hidden";
                          }}
                        />
                        <span>{pack.name}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

