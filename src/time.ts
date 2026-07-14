/** Format a duration in whole seconds for timers (e.g. 900 → 15:00). */
export function formatCountdown(totalSecs: number): string {
  const secs = Math.max(0, Math.floor(totalSecs));
  if (secs < 60) {
    return `${secs}s`;
  }
  const hours = Math.floor(secs / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  const seconds = secs % 60;
  const mm = String(minutes).padStart(2, "0");
  const ss = String(seconds).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${mm}:${ss}`;
  }
  return `${minutes}:${ss}`;
}
