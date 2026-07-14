import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
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
  return `./icons/classes/${slug}.png`;
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
  return root;
}

/** Keep Essentials first, then Classes, then EQL Raids. */
function rootRank(label: string): number {
  if (label === "EQL Essentials") return 0;
  if (label === "Classes") return 1;
  if (label === "EQL Raids") return 2;
  return 3;
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
    if (node.group) {
      if (groupMatchesFilter(node.group, query, filter)) out.push(node);
      continue;
    }
    const kids = filterTree(node.children, query, filter);
    if (kids.length > 0) out.push({ ...node, children: kids });
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
  const voicePreviewTimer = useRef<number | null>(null);

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
    invoke<TriggerLibrary>("get_triggers")
      .then((lib) => {
        setLibrary(lib);
        setExpanded({});
        const tree = buildTree(lib.groups);
        const first = firstSelectable(tree);
        if (first?.group) {
          setSelection({
            groupId: first.group.id,
            triggerId: first.group.triggers[0]?.id ?? null,
          });
        }
      })
      .catch((err) => setError(String(err)));

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

    const unlisten = listen<EngineState>("alerts-update", (event) => {
      setEngine(event.payload);
    });
    const unlistenTts = bindTtsPlayback();

    return () => {
      unlisten.then((fn) => fn());
      unlistenTts.then((fn) => fn());
    };
  }, []);

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

  const stats = useMemo(() => {
    const groupsOn = library.groups.filter((g) => g.enabled).length;
    const triggers = library.groups.reduce((n, g) => n + g.triggers.length, 0);
    const armed = library.groups.reduce((n, g) => {
      if (!g.enabled) return n;
      return n + g.triggers.filter((t) => t.enabled).length;
    }, 0);
    return { groups: library.groups.length, groupsOn, triggers, armed };
  }, [library]);

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
    const allOn = pack.enabledCount === pack.groupIds.length;
    setGroupsEnabled(pack.groupIds, !allOn);
  }

  function selectTrigger(groupId: string, triggerId: string) {
    setSelection({ groupId, triggerId });
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
        "Replace your trigger library with the built-in EQL starter (Essentials + classes + classic EQL Raids)? Your custom edits will be overwritten."
      )
    ) {
      return;
    }
    try {
      const result = await invoke<{
        library: TriggerLibrary;
        groups: number;
        triggers: number;
      }>("install_starter_pack");
      setLibrary(result.library);
      setExpanded({});
      setNote(
        `Starter installed — ${result.groups} groups / ${result.triggers} triggers. Arm your class chip; open EQL Raids for zone bosses.`
      );
    } catch (err) {
      setError(String(err));
    }
  }

  function patchDraft(partial: Partial<Draft>) {
    setDraft((prev) => (prev ? { ...prev, ...partial } : prev));
    setDraftDirty(true);
  }

  function saveDraft() {
    if (!selected || !draft) return;
    const timerRaw = draft.timer_seconds.trim();
    void persist({
      groups: library.groups.map((group) => {
        if (group.id !== selected.group.id) return group;
        return {
          ...group,
          triggers: group.triggers.map((trigger) => {
            if (trigger.id !== selected.trigger.id) return trigger;
            return {
              ...trigger,
              name: draft.name.trim() || "Untitled",
              search: draft.search,
              use_regex: draft.use_regex,
              display_text: draft.display_text.trim() || null,
              timer_seconds: timerRaw === "" ? null : Number(timerRaw),
              timer_name: draft.timer_name.trim() || null,
              early_end: draft.early_end
                .split("\n")
                .map((s) => s.trim())
                .filter(Boolean),
              sound:
                draft.sound.trim() && draft.sound !== "none"
                  ? draft.sound.trim()
                  : draft.tts_enabled
                    ? null
                    : "ping",
              speak: draft.speak.trim() || draft.display_text.trim() || draft.name,
              tts_enabled: draft.tts_enabled,
              comments: draft.comments.trim() || null,
            };
          }),
        };
      }),
    });
    setDraftDirty(false);
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
  }

  function addGroup() {
    const group: TriggerGroup = {
      id: newId(),
      name: "Custom / New set",
      enabled: true,
      triggers: [emptyTrigger()],
    };
    void persist({ groups: [...library.groups, group] });
    setSelection({ groupId: group.id, triggerId: group.triggers[0].id });
  }

  function renderNode(node: TreeNode, depth: number): ReactNode {
    const isOpen = expanded[node.key] === true;
    const statsNode = nodeStats(node);

    if (node.group) {
      const group = node.group;
      const shown = triggersMatchingQuery(group, query.trim());
      return (
        <div className="leaf" key={node.key}>
          <div
            className={`row ${selection?.groupId === group.id ? "active" : ""}`}
            style={{ paddingLeft: 6 + depth * 6 }}
            onClick={() => {
              setExpanded((e) => ({ ...e, [node.key]: !isOpen }));
              if (selection?.groupId !== group.id) {
                const first = group.triggers[0];
                if (first) selectTrigger(group.id, first.id);
              }
            }}
          >
            <span className="chev">{isOpen ? "▾" : "▸"}</span>
            <input
              type="checkbox"
              checked={group.enabled}
              onClick={(e) => e.stopPropagation()}
              onChange={(e) => setGroupsEnabled([group.id], e.target.checked)}
            />
            <span className="name">{node.label}</span>
            {group.enabled ? <span className="badge">armed</span> : null}
            <span className="meta">{group.triggers.length}</span>
          </div>
          {isOpen
            ? shown.map((trigger) => {
                const active =
                  selection?.groupId === group.id &&
                  selection?.triggerId === trigger.id;
                return (
                  <div
                    key={trigger.id}
                    className={`trigger ${active ? "active" : ""}`}
                    onClick={() => selectTrigger(group.id, trigger.id)}
                  >
                    <input
                      type="checkbox"
                      checked={trigger.enabled}
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => {
                        void persist({
                          groups: library.groups.map((g) => {
                            if (g.id !== group.id) return g;
                            return {
                              ...g,
                              triggers: g.triggers.map((t) =>
                                t.id === trigger.id
                                  ? { ...t, enabled: e.target.checked }
                                  : t
                              ),
                            };
                          }),
                        });
                      }}
                    />
                    <div className="body">
                      <div className="title">{trigger.name}</div>
                      <div className="tags">
                        {trigger.timer_seconds ? (
                          <span className="tag timer">
                            {formatCountdown(trigger.timer_seconds)}
                          </span>
                        ) : null}
                        {trigger.tts_enabled !== false ? (
                          <span className="tag">TTS</span>
                        ) : (
                          <span className="tag">SFX</span>
                        )}
                        {trigger.use_regex ? (
                          <span className="tag regex">regex</span>
                        ) : null}
                        {trigger.display_text ? (
                          <span className="tag">toast</span>
                        ) : null}
                      </div>
                      <span className="pattern">
                        {trigger.search || "(empty pattern)"}
                      </span>
                    </div>
                  </div>
                );
              })
            : null}
        </div>
      );
    }

    return (
      <div className="folder" key={node.key}>
        <div
          className="row"
          style={{ paddingLeft: 6 + depth * 6 }}
          onClick={() => setExpanded((e) => ({ ...e, [node.key]: !isOpen }))}
        >
          <span className="chev">{isOpen ? "▾" : "▸"}</span>
          <input
            type="checkbox"
            checked={statsNode.enabled === statsNode.groups && statsNode.groups > 0}
            ref={(el) => {
              if (!el) return;
              el.indeterminate =
                statsNode.enabled > 0 && statsNode.enabled < statsNode.groups;
            }}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setGroupsEnabled(collectGroupIds(node), e.target.checked)}
          />
          <span className="name">{node.label}</span>
          <span className="meta">
            {statsNode.enabled}/{statsNode.groups}
          </span>
        </div>
        {isOpen ? (
          <div className="kids">
            {node.children.map((child) => renderNode(child, depth + 1))}
          </div>
        ) : null}
      </div>
    );
  }

  const live = !!engine?.monitoring;

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

      <header className="header">
        <div className="logo">
          <strong>EQL Alerts</strong>
          <em>EverQuest Legends</em>
        </div>

        <div className={`conn ${live ? "live" : ""}`} title={engine?.log_path ?? ""}>
          <span className="pulse" />
          <div className="conn-copy">
            <div className="title">
              {live ? "Live" : "Not attached"} · {engine?.character ?? "—"}
            </div>
            <div className="sub">{shortPath(engine?.log_path ?? null)}</div>
          </div>
        </div>

        <div className="audio-panel" title="Callout voice settings">
          <div className="audio-row">
            <label className="audio-field audio-field-voice">
              <span className="audio-label">Voice</span>
              <div className="audio-inline">
                <select
                  className="voice-select"
                  value={settings?.voice_id || settings?.voice_female || "bf_isabella"}
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
                        settings?.voice_volume ?? 0.2,
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
                  className="btn sm audio-preview-btn"
                  type="button"
                  title="Play a sample at the current volume"
                  onClick={() => {
                    const voice_id =
                      settings?.voice_id || settings?.voice_female || "bf_isabella";
                    void previewVoice(
                      voice_id,
                      "Out of mana",
                      settings?.voice_volume ?? 0.2,
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
          </div>

          <div className="audio-row audio-row-secondary">
            <label className="audio-field audio-field-vol">
              <span className="audio-label">Volume</span>
              <div className="audio-vol-line">
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={Math.round((settings?.voice_volume ?? 0.2) * 100)}
                  onChange={(e) => {
                    const voice_volume = Number(e.target.value) / 100;
                    void patchSettings({ voice_volume });
                  }}
                  onPointerUp={(e) => {
                    const voice_volume = Number(e.currentTarget.value) / 100;
                    const voice_id =
                      settings?.voice_id || settings?.voice_female || "bf_isabella";
                    void previewVoice(voice_id, "Out of mana", voice_volume).catch(
                      () => undefined,
                    );
                  }}
                />
                <em className="voice-vol-pct">
                  {Math.round((settings?.voice_volume ?? 0.2) * 100)}%
                </em>
              </div>
            </label>

            <label className="audio-field audio-field-out">
              <span className="audio-label">Output</span>
              <select
                className="output-select"
                value={settings?.audio_output_device ?? ""}
                onChange={(e) => {
                  const audio_output_device = e.target.value;
                  const voice_id =
                    settings?.voice_id || settings?.voice_female || "bf_isabella";
                  void patchSettings({ audio_output_device }).then(() =>
                    previewVoice(
                      voice_id,
                      "Out of mana",
                      settings?.voice_volume ?? 0.2,
                    )
                      .then((msg) => {
                        setNote(msg);
                        setError(null);
                      })
                      .catch((err) => {
                        setError(`Output preview failed: ${String(err)}`);
                      }),
                  );
                }}
                title={settings?.audio_output_device || "System default output"}
              >
                <option value="">System default</option>
                {audioDevices.map((d) => {
                  let label = d.name;
                  if (d.is_default) {
                    label = `${label} · default`;
                  }
                  if (d.channels > 2) {
                    label = `${label} · ${d.channels}ch`;
                  }
                  return (
                    <option key={d.name} value={d.name}>
                      {label}
                    </option>
                  );
                })}
              </select>
            </label>
          </div>
        </div>

        <div className="header-actions">
          <div className="header-action-row">
            <button
              className="btn primary"
              type="button"
              title="Auto-find the newest EverQuest Legends character log and start monitoring"
              onClick={() => void attachLog(false)}
            >
              Find log
            </button>
            <button
              className="btn"
              type="button"
              title="Pick a log file manually (eqlog_*.txt)"
              onClick={() => void attachLog(true)}
            >
              Browse…
            </button>
            {live ? (
              <button
                className="btn ghost"
                type="button"
                title="Stop watching the attached log"
                onClick={() => void stop()}
              >
                Disconnect
              </button>
            ) : (
              <button
                className="btn"
                type="button"
                disabled={!(engine?.log_path || settings?.last_log_path)}
                title="Resume monitoring the last attached log"
                onClick={() => void reconnect()}
              >
                Reconnect
              </button>
            )}
          </div>
          <div className="header-action-row">
            <button
              className="btn mint"
              type="button"
              title="Open the always-on-top timer and toast overlay"
              onClick={() => void invoke("open_overlay")}
            >
              Overlay
            </button>
            <button
              className="btn ghost"
              type="button"
              title="Clear active countdown timers from the overlay"
              onClick={() => void clearTimers()}
            >
              Clear timers
            </button>
            <button
              className="btn ghost"
              type="button"
              title="Show the quick-start guide"
              onClick={() => setShowQuickStart(true)}
            >
              Help
            </button>
            <button
              className="btn"
              type="button"
              title="Import a GINA .gtp trigger package"
              onClick={() => void importGinaPack()}
            >
              Import…
            </button>
            <button
              className="btn ghost"
              type="button"
              title="Replace the library with Essentials + classes + classic EQL Raids"
              onClick={() => void restoreStarter()}
            >
              Reset starter
            </button>
          </div>
        </div>
      </header>

      <section className="class-shelf">
        <div className="class-shelf-top">
          <div className="class-shelf-copy">
            <h2>Your class</h2>
            <p>Tap a class to arm its trigger sets. Leave the rest off to keep noise down.</p>
          </div>
          <div className="class-shelf-stats">
            <div className="stat-block">
              <span>Armed</span>
              <b>{stats.armed}</b>
            </div>
            <div className="stat-block">
              <span>Sets</span>
              <b>
                {stats.groupsOn}/{stats.groups}
              </b>
            </div>
            <div className="stat-block">
              <span>Timers</span>
              <b>{engine?.timers.length ?? 0}</b>
            </div>
            {classPacks.some((p) => p.enabledCount > 0) ? (
              <button
                className="btn sm ghost"
                type="button"
                onClick={() => {
                  const ids = classPacks.flatMap((p) => p.groupIds);
                  setGroupsEnabled(ids, false);
                }}
              >
                Disarm classes
              </button>
            ) : null}
          </div>
        </div>

        {classPacks.length === 0 ? (
          <div className="class-empty">
            {library.groups.length === 0
              ? "No trigger sets yet — install the starter pack below."
              : "No class packs found in this library."}
          </div>
        ) : (
          <div className="class-grid">
            {classPacks.map((pack) => {
              const on =
                pack.enabledCount === pack.groupIds.length && pack.groupIds.length > 0;
              const partial =
                pack.enabledCount > 0 && pack.enabledCount < pack.groupIds.length;
              return (
                <button
                  key={pack.name}
                  type="button"
                  className={`class-tile ${on ? "on" : ""} ${partial ? "partial" : ""}`}
                  title={`${pack.triggerCount} triggers · ${pack.groupIds.length} sets`}
                  onClick={() => toggleClassPack(pack)}
                >
                  <img
                    src={classIconUrl(pack.name)}
                    alt=""
                    onError={(e) => {
                      (e.target as HTMLImageElement).style.visibility = "hidden";
                    }}
                  />
                  <span className="class-name">{pack.name}</span>
                  <span className="class-count">
                    {pack.enabledCount}/{pack.groupIds.length}
                  </span>
                </button>
              );
            })}
          </div>
        )}
      </section>

      {error ? (
        <div className="banner error">
          <span className="grow">{error}</span>
          <button className="btn sm ghost" type="button" onClick={() => setError(null)}>
            Dismiss
          </button>
        </div>
      ) : null}
      {!error && note ? (
        <div className="banner info">
          <span className="grow">{note}</span>
          <button className="btn sm ghost" type="button" onClick={() => setNote(null)}>
            Got it
          </button>
        </div>
      ) : null}

      <div className="workspace">
        <section className="col">
          <div className="col-head">
            <h2>Triggers</h2>
            <span className="grow" />
            <button className="btn sm" type="button" onClick={addGroup}>
              New set
            </button>
          </div>
          <div className="tools">
            <input
              className="search"
              type="search"
              placeholder="Search name or pattern…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <div className="seg">
              {(
                [
                  ["all", "All"],
                  ["armed", "Armed"],
                  ["off", "Off"],
                ] as const
              ).map(([mode, label]) => (
                <button
                  key={mode}
                  type="button"
                  className={filter === mode ? "on" : ""}
                  onClick={() => setFilter(mode)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <div className="scroll">
            {visibleTree.length === 0 ? (
              <div className="empty">
                {library.groups.length === 0 ? (
                  <>
                    No triggers loaded.{" "}
                    <button className="btn sm primary" type="button" onClick={() => void restoreStarter()}>
                      Install starter pack
                    </button>
                  </>
                ) : (
                  "Nothing matches that search."
                )}
              </div>
            ) : (
              visibleTree.map((node) => renderNode(node, 0))
            )}
          </div>
        </section>

        <section className="col">
          <div className="col-head">
            <h2>Inspector</h2>
            <span className="grow" />
            {selected ? (
              <button className="btn sm" type="button" onClick={addSibling}>
                Add trigger
              </button>
            ) : null}
          </div>
          <div className="inspector">
            {!selected || !draft ? (
              <div className="empty">
                <b>Quick start</b>
                <br />
                1. Find log · 2. Arm your class chip · 3. Open Overlay
                <br />
                <br />
                EQL Essentials Core / Combat / Danger / Fades are armed — Social and range/LOS stay opt-in.
              </div>
            ) : (
              <>
                <h3>{draft.name || "Untitled"}</h3>
                <p className="path">{selected.group.name}</p>

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
                </div>

                <div className="field">
                  <span>Name</span>
                  <input
                    type="text"
                    value={draft.name}
                    onChange={(e) => patchDraft({ name: e.target.value })}
                  />
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
                <div className="field">
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={draft.tts_enabled}
                      onChange={(e) => patchDraft({ tts_enabled: e.target.checked })}
                    />
                    Speak with TTS (uncheck to use a chime instead)
                  </label>
                </div>
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
                        onClick={() => {
                          const line = draft.speak.trim() || "Alert test";
                          void testSpeech(line, null)
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
                <div className="field">
                  <span>Notes</span>
                  <input
                    type="text"
                    value={draft.comments}
                    onChange={(e) => patchDraft({ comments: e.target.value })}
                  />
                </div>

                <div className="actions">
                  <button
                    className="btn primary"
                    type="button"
                    disabled={!draftDirty}
                    onClick={saveDraft}
                  >
                    Save
                  </button>
                  <button
                    className="btn ghost"
                    type="button"
                    disabled={!draftDirty}
                    onClick={() => {
                      if (selected) {
                        setDraft(triggerToDraft(selected.trigger));
                        setDraftDirty(false);
                      }
                    }}
                  >
                    Revert
                  </button>
                  <button className="btn danger" type="button" onClick={deleteSelected}>
                    Delete
                  </button>
                </div>
              </>
            )}
          </div>
        </section>

        <section className="col col-activity">
          <div className="col-head">
            <h2>Activity</h2>
            <span className="grow" />
            <button
              className="btn sm"
              type="button"
              onClick={() => void clearAlerts()}
            >
              Clear
            </button>
          </div>
          <div className="scroll">
            <div className="activity-block">
              <h3>Timers</h3>
              {(engine?.timers.length ?? 0) === 0 ? (
                <div className="empty" style={{ margin: "0 6px" }}>
                  No running timers
                </div>
              ) : (
                engine!.timers.map((timer) => {
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
            <div className="activity-block">
              <h3>Fired</h3>
              {(engine?.recent_alerts.length ?? 0) === 0 ? (
                <div className="empty" style={{ margin: "0 6px" }}>
                  Waiting for matches on the live log
                </div>
              ) : (
                engine!.recent_alerts.map((alert, i) => (
                  <div className={`event ${i === 0 ? "fresh" : ""}`} key={alert.id}>
                    <div className="when">{formatTime(alert.at_ms)}</div>
                    <div className="text">{alert.text}</div>
                    <div className="from">
                      {alert.trigger_name} · {alert.kind}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
