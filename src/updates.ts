import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";

export type PendingUpdate = {
  version: string;
  notes: string;
};

export type UpdateProgress = {
  phase: "checking" | "downloading" | "installing" | "restarting";
  downloaded: number;
  total: number | null;
};

const RELEASES_PAGE = "https://github.com/kpxcoolx/eql-alerts/releases/latest";
const CHECK_TIMEOUT_MS = 12_000;

function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error(`${label} timed out. Try Open latest release instead.`));
    }, ms);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (err) => {
        window.clearTimeout(timer);
        reject(err);
      }
    );
  });
}

export async function checkForAppUpdate(): Promise<PendingUpdate | null> {
  let update: Update | null;
  try {
    update = await withTimeout(
      check({ timeout: CHECK_TIMEOUT_MS }),
      CHECK_TIMEOUT_MS + 1000,
      "Update check"
    );
  } catch (err) {
    const raw = String(err);
    if (
      raw.includes("valid release JSON") ||
      raw.includes("Could not fetch") ||
      raw.includes("404")
    ) {
      throw new Error(
        "Update feed not ready yet (latest.json missing). Use Latest release… to download the installer."
      );
    }
    throw err;
  }
  if (!update) {
    return null;
  }
  return {
    version: update.version,
    notes: update.body ?? "",
  };
}

export function formatUpdateBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function updateProgressLabel(progress: UpdateProgress): string {
  if (progress.phase === "checking") return "Checking for update…";
  if (progress.phase === "installing") return "Installing update…";
  if (progress.phase === "restarting") return "Restarting…";

  if (progress.total != null && progress.total > 0) {
    const pct = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
    return `Downloading update… ${pct}% (${formatUpdateBytes(progress.downloaded)} / ${formatUpdateBytes(progress.total)})`;
  }
  if (progress.downloaded > 0) {
    return `Downloading update… ${formatUpdateBytes(progress.downloaded)}`;
  }
  return "Downloading update…";
}

export function updateProgressPercent(progress: UpdateProgress): number | null {
  if (progress.phase !== "downloading") return null;
  if (progress.total == null || progress.total <= 0) return null;
  return Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
}

export async function installAppUpdate(
  onProgress?: (progress: UpdateProgress) => void
): Promise<void> {
  onProgress?.({ phase: "checking", downloaded: 0, total: null });
  const update: Update | null = await withTimeout(
    check({ timeout: CHECK_TIMEOUT_MS }),
    CHECK_TIMEOUT_MS + 1000,
    "Update check"
  );
  if (!update) {
    throw new Error("No update available");
  }

  let downloaded = 0;
  let total: number | null = null;

  onProgress?.({ phase: "downloading", downloaded: 0, total: null });

  const handleEvent = (event: DownloadEvent) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? null;
      downloaded = 0;
      onProgress?.({ phase: "downloading", downloaded, total });
      return;
    }
    if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      onProgress?.({ phase: "downloading", downloaded, total });
      return;
    }
    if (event.event === "Finished") {
      onProgress?.({ phase: "installing", downloaded, total });
    }
  };

  await update.downloadAndInstall(handleEvent, { timeout: 120_000 });
  onProgress?.({ phase: "restarting", downloaded, total });
  await relaunch();
}

export async function openLatestReleasePage(): Promise<void> {
  await openUrl(RELEASES_PAGE);
}
