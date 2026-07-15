import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { EngineState, FiredAlert, TriggerLibrary } from "./App";
import { resolveTimerIcon, resolveToastIcon } from "./overlayIcons";
import { formatCountdown } from "./time";

type OverlayStatus = {
  open: boolean;
  click_through: boolean;
};

type Toast = FiredAlert & { expires: number };

export default function Overlay() {
  const [engine, setEngine] = useState<EngineState | null>(null);
  const [clickThrough, setClickThrough] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [now, setNow] = useState(Date.now());
  const [modeHint, setModeHint] = useState<string | null>(null);
  const [groupByTrigger, setGroupByTrigger] = useState<Map<string, string>>(
    () => new Map(),
  );

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 200);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    invoke<EngineState>("get_engine_state")
      .then(setEngine)
      .catch(() => setEngine(null));

    invoke<OverlayStatus>("get_overlay_status")
      .then((status) => setClickThrough(status.click_through))
      .catch(() => setClickThrough(false));

    invoke<TriggerLibrary>("get_triggers")
      .then((lib) => {
        const map = new Map<string, string>();
        for (const group of lib.groups) {
          for (const trigger of group.triggers) {
            map.set(trigger.id, group.name);
          }
        }
        setGroupByTrigger(map);
      })
      .catch(() => setGroupByTrigger(new Map()));

    const unlistenEngine = listen<EngineState>("alerts-update", (event) => {
      setEngine(event.payload);
      if (event.payload.recent_alerts.length === 0) {
        setToasts([]);
      }
      const latest = event.payload.recent_alerts[0];
      if (!latest) return;
      setToasts((prev) => {
        if (prev.some((t) => t.id === latest.id)) return prev;
        return [{ ...latest, expires: Date.now() + 4500 }, ...prev].slice(0, 6);
      });
    });

    const unlistenStatus = listen<OverlayStatus>("overlay-status", (event) => {
      setClickThrough(event.payload.click_through);
    });

    const alwaysOnTop = window.setInterval(() => {
      void getCurrentWindow()
        .setAlwaysOnTop(true)
        .catch(() => undefined);
    }, 2000);

    return () => {
      unlistenEngine.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
      window.clearInterval(alwaysOnTop);
    };
  }, []);

  useEffect(() => {
    setToasts((prev) => prev.filter((t) => t.expires > now));
  }, [now]);

  useEffect(() => {
    if (!modeHint) return;
    const id = window.setTimeout(() => setModeHint(null), 3200);
    return () => window.clearTimeout(id);
  }, [modeHint]);

  async function enableClickThrough() {
    setModeHint("Click-through on — ⌘⇧U (Ctrl+Shift+U) to edit again");
    try {
      await invoke("set_overlay_click_through", { enabled: true });
    } catch (err) {
      setModeHint(`Click-through failed: ${String(err)}`);
    }
  }

  async function closeOverlay() {
    try {
      await invoke("close_overlay");
    } catch {
      // ignore
    }
  }

  const setupMode = !clickThrough;
  // Hide as soon as ends_ms passes — backend prune only runs on log events / tick.
  const timers = (engine?.timers ?? []).filter((t) => t.ends_ms > now);
  const hasContent = timers.length > 0 || toasts.length > 0 || modeHint;

  return (
    <div className={`overlay ${setupMode ? "setup" : "passthrough"}`}>
      {setupMode ? (
        <div className="chrome">
          <div
            className="drag"
            onMouseDown={(e) => {
              // Only drag from the bar, not from buttons.
              if ((e.target as HTMLElement).closest("button")) return;
              void getCurrentWindow().startDragging();
            }}
          >
            <span className="drag-title">EQL Alerts</span>
            <div className="drag-actions">
              <button
                type="button"
                title="Clicks pass through to the game. Use ⌘⇧U / Ctrl+Shift+U to edit again."
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  void enableClickThrough();
                }}
              >
                Click-through
              </button>
              <button
                type="button"
                className="close-btn"
                title="Hide the overlay. Open it again from the main window."
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  void closeOverlay();
                }}
              >
                Close
              </button>
            </div>
          </div>
          {!hasContent ? (
            <div className="empty-hint">
              Timers and alerts show here. Drag the title to move · Drag the
              corner to resize · Click-through to play through the overlay ·
              Close to hide it · ⌘⇧U / Ctrl+Shift+U restores edit mode · click
              a timer to clear it.
            </div>
          ) : null}
          <div
            className="resize-grip"
            title="Drag to resize"
            onMouseDown={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void getCurrentWindow().startResizeDragging("SouthEast");
            }}
          />
        </div>
      ) : null}

      {modeHint ? <div className="mode-hint">{modeHint}</div> : null}

      <div className="toasts">
        {toasts.map((toast) => {
          const groupName = groupByTrigger.get(toast.trigger_id) ?? null;
          const iconSrc = resolveToastIcon(toast.text, groupName);
          return (
            <div className="toast" key={toast.id}>
              <img className="row-icon" src={iconSrc} alt="" />
              <span className="toast-text">{toast.text}</span>
              {setupMode ? (
                <button
                  type="button"
                  className="dismiss"
                  title="Dismiss"
                  onClick={() =>
                    setToasts((prev) => prev.filter((t) => t.id !== toast.id))
                  }
                >
                  ×
                </button>
              ) : null}
            </div>
          );
        })}
      </div>

      <div className="timers">
        {timers.map((timer) => {
          const left = Math.max(0, Math.ceil((timer.ends_ms - now) / 1000));
          const pct = Math.max(
            0,
            Math.min(
              100,
              ((timer.ends_ms - now) / (timer.duration_secs * 1000)) * 100,
            ),
          );
          const groupName = groupByTrigger.get(timer.trigger_id) ?? null;
          const iconSrc = resolveTimerIcon(timer.name, groupName);

          function clearThisTimer() {
            void invoke<EngineState>("clear_timer", {
              timerId: timer.id,
            })
              .then(setEngine)
              .catch(() => undefined);
          }

          return (
            <div
              className={`timer${setupMode ? " dismissable" : ""}`}
              key={timer.id}
              role={setupMode ? "button" : undefined}
              tabIndex={setupMode ? 0 : undefined}
              title={setupMode ? "Click to clear this timer" : undefined}
              onClick={
                setupMode
                  ? (e) => {
                      if ((e.target as HTMLElement).closest("button")) return;
                      clearThisTimer();
                    }
                  : undefined
              }
              onKeyDown={
                setupMode
                  ? (e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        clearThisTimer();
                      }
                    }
                  : undefined
              }
            >
              <img className="row-icon" src={iconSrc} alt="" />
              <div className="timer-body">
                <div className="timer-top">
                  <div className="name">{timer.name}</div>
                  <div className="left">{formatCountdown(left)}</div>
                  {setupMode ? (
                    <button
                      type="button"
                      className="dismiss"
                      title="Clear this timer"
                      onClick={(e) => {
                        e.stopPropagation();
                        clearThisTimer();
                      }}
                    >
                      ×
                    </button>
                  ) : null}
                </div>
                <div className="bar">
                  <span style={{ width: `${pct}%` }} />
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
