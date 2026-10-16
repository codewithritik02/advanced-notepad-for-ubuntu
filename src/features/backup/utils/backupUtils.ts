import type {
  BackupProgressStage,
  BackupSchedulerState,
  BackupSettings,
} from "../../../services/storage/types.ts";

export const MAX_CONSECUTIVE_FAILURES = 5;
export const MIN_RETRY_BACKOFF_MINUTES = 15;

/**
 * Formats byte sizes into human-readable strings (e.g. 1.2 MB, 450 KB).
 */
export function formatBackupBytes(bytes: number): string {
  if (bytes <= 0) return "0 Bytes";
  const units = ["Bytes", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const boundedIndex = Math.min(i, units.length - 1);
  const value = bytes / Math.pow(1024, boundedIndex);
  return `${value.toFixed(boundedIndex === 0 ? 0 : 1)} ${units[boundedIndex]}`;
}

/**
 * Maps BackupProgressStage enum into human-friendly description text.
 */
export function getBackupStageDescription(stage: BackupProgressStage): string {
  switch (stage) {
    case "starting":
      return "Initializing backup process...";
    case "snapshottingDatabase":
      return "Creating consistent SQLite database snapshot...";
    case "collectingAttachments":
      return "Capturing and verifying attachment files...";
    case "generatingManifest":
      return "Building authoritative manifest and computing checksums...";
    case "writingArchive":
      return "Compressing and writing backup archive...";
    case "verifyingArchive":
      return "Reading back and verifying archive integrity...";
    case "applyingRetention":
      return "Applying backup retention policies...";
    case "validatingBackup":
      return "Validating backup archive integrity...";
    case "preparingStagingData":
    case "extractingStaging":
      return "Extracting backup to isolated staging directory...";
    case "checkingDatabaseIntegrity":
      return "Verifying database integrity and schema compatibility...";
    case "validatingStagedData":
      return "Validating staged database and applying migrations...";
    case "checkingAttachments":
      return "Verifying staged attachments...";
    case "creatingCheckpoint":
      return "Creating pre-restore recovery checkpoint...";
    case "preparingActivation":
      return "Preparing activation (cancellation locked)...";
    case "activatingRestoredData":
      return "Activating restored database and attachments...";
    case "verifyingRestoredState":
      return "Verifying restored state and reinitializing search index...";
    case "rebuildingSearchIndex":
      return "Rebuilding search indexes...";
    case "completed":
      return "Backup complete and verified!";
    case "failed":
      return "Backup process failed.";
    case "failedWithRecoveryAvailable":
      return "Restore failed. Previous state remains safely active.";
    case "failedAndRecoveryRequired":
      return "Restore failed. Manual recovery action is required.";
    case "cancelled":
      return "Operation was cancelled.";
    default:
      return "Processing backup...";
  }
}

/**
 * Sanitizes a proposed backup filename stem to prevent path traversal
 * and illegal character violations across Linux and Windows filesystems (Phase 11, Task 9).
 */
export function sanitizeBackupFilename(rawName: string): string {
  // 1. Strip illegal path and device characters: / \ : * ? " < > | \0..\x1F
  const filtered = rawName.replace(/[/\\:*?"<>|\x00-\x1F]/g, "");

  // 2. Trim whitespace and leading/trailing dots
  const trimmed = filtered.trim().replace(/^\.+|\.+$/g, "").trim();

  if (!trimmed) {
    return "personal-notepad-backup";
  }

  // 3. Neutralize Windows reserved device names
  const reserved = /^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/i;
  const neutralized = reserved.test(trimmed) ? `_${trimmed}` : trimmed;

  // 4. Bound length to 120 characters
  return Array.from(neutralized).slice(0, 120).join("");
}

/**
 * Generates the standard default backup filename with timestamp.
 * Format: personal-notepad-backup-YYYYMMDD-HHMMSS.pnbackup
 */
export function generateDefaultBackupFilename(
  date = new Date(),
  type: "manual" | "automatic" | "checkpoint" = "manual"
): string {
  const pad = (n: number) => n.toString().padStart(2, "0");
  const yyyy = date.getFullYear();
  const mm = pad(date.getMonth() + 1);
  const dd = pad(date.getDate());
  const hh = pad(date.getHours());
  const min = pad(date.getMinutes());
  const ss = pad(date.getSeconds());
  const prefix =
    type === "manual"
      ? "backup"
      : type === "automatic"
      ? "auto-backup"
      : "checkpoint";
  return `personal-notepad-${prefix}-${yyyy}${mm}${dd}-${hh}${min}${ss}.pnbackup`;
}

/**
 * Pure evaluation function testing whether an automatic backup is due according
 * to configuration, historical execution state, and lifecycle triggers (Phase 11, Task 11).
 */
export function isBackupDue(
  settings: BackupSettings,
  state: BackupSchedulerState,
  now: Date = new Date(),
  isStartup: boolean = false
): boolean {
  // 1. User Preference: disabled
  if (!settings.autoBackupEnabled) {
    return false;
  }

  // 2. Max consecutive failures cap (protect against unbounded retry thrashing)
  if (state.failureCount >= MAX_CONSECUTIVE_FAILURES) {
    return false;
  }

  // 3. Failure backoff check
  if (state.failureCount > 0 && state.lastAttemptAt) {
    const lastAttemptTime = new Date(state.lastAttemptAt).getTime();
    if (!isNaN(lastAttemptTime)) {
      const elapsedMinutes = (now.getTime() - lastAttemptTime) / (1000 * 60);
      const backoff = MIN_RETRY_BACKOFF_MINUTES * state.failureCount;
      if (elapsedMinutes < backoff) {
        return false;
      }
    }
  }

  // 4 & 5. Rapid restart protection & elapsed interval check
  if (state.lastSuccessfulAt) {
    const lastSuccessTime = new Date(state.lastSuccessfulAt).getTime();
    if (!isNaN(lastSuccessTime)) {
      if (now.getTime() < lastSuccessTime) {
        return false; // Clock skew guard
      }
      const elapsedMinutes = (now.getTime() - lastSuccessTime) / (1000 * 60);
      return elapsedMinutes >= settings.intervalMinutes;
    }
  }

  // 6. First run / no previous successful backup
  if (isStartup) {
    return settings.backupOnStartup;
  } else if (state.lastAttemptAt) {
    const lastAttemptTime = new Date(state.lastAttemptAt).getTime();
    if (!isNaN(lastAttemptTime)) {
      const elapsedMinutes = (now.getTime() - lastAttemptTime) / (1000 * 60);
      return elapsedMinutes >= settings.intervalMinutes;
    }
    return true;
  } else {
    return true;
  }
}

export interface BackupCandidateItem {
  filePath: string;
  fileName: string;
  createdAt: string;
  backupType: "manual" | "automatic" | "checkpoint";
}

export interface RetentionPlanResult {
  totalManagedFound: number;
  kept: BackupCandidateItem[];
  toDelete: BackupCandidateItem[];
}

/**
 * Pure evaluation function computing which automatic backups to keep and which to prune
 * according to retention count and creation timestamp (Phase 11, Task 12).
 *
 * Safety Invariants:
 * 1. Only eligible automatic backups are candidates for retention cleanup.
 * 2. Manual backups and checkpoints are NEVER pruned.
 * 3. Backups are ordered descending (newest first).
 * 4. The newest `retentionCount` automatic backups are retained.
 * 5. If `newlyCreatedFilePath` is specified, it is guaranteed never to be pruned.
 */
export function computeRetentionPlan(
  backups: BackupCandidateItem[],
  retentionCount: number,
  newlyCreatedFilePath?: string
): RetentionPlanResult {
  // 1. Filter ONLY eligible automatic backups
  const eligible = backups.filter((b) => b.backupType === "automatic");

  // 2. Sort descending (newest first, oldest last)
  const sorted = [...eligible].sort((a, b) => {
    const timeA = new Date(a.createdAt).getTime();
    const timeB = new Date(b.createdAt).getTime();
    return timeB - timeA;
  });

  const boundedRetention = Math.max(1, retentionCount);
  const kept = sorted.slice(0, boundedRetention);
  const excess = sorted.slice(boundedRetention);

  // Guarantee newly created backup is never deleted
  const toDelete = excess.filter((b) => b.filePath !== newlyCreatedFilePath);

  return {
    totalManagedFound: eligible.length,
    kept,
    toDelete,
  };
}

/**
 * Safely sorts backups descending chronologically (newest first).
 * Invalid or missing timestamps are sorted to the end.
 */
export function sortBackupsDescending<T extends { createdAt: string }>(items: T[]): T[] {
  return [...items].sort((a, b) => {
    const timeA = new Date(a.createdAt).getTime();
    const timeB = new Date(b.createdAt).getTime();
    const validA = !isNaN(timeA);
    const validB = !isNaN(timeB);
    if (!validA && !validB) return 0;
    if (!validA) return 1;
    if (!validB) return -1;
    return timeB - timeA;
  });
}

/**
 * Checks whether a proposed target file path points to a protected system file
 * (e.g. active database, WAL, SHM, attachments directory), preventing catastrophic deletion.
 */
export function isDeletionTargetProtected(
  targetPath: string,
  liveDatabasePath?: string,
  liveAttachmentsDir?: string
): boolean {
  if (!targetPath) return true;
  const normalized = targetPath.replace(/\\/g, "/").toLowerCase();

  // Guard live database & SQLite journal/WAL files
  if (
    normalized.endsWith("/database.sqlite") ||
    normalized.endsWith("/database.sqlite-wal") ||
    normalized.endsWith("/database.sqlite-shm") ||
    normalized.endsWith("/database.sqlite-journal") ||
    normalized === "database.sqlite" ||
    normalized === "database.sqlite-wal" ||
    normalized === "database.sqlite-shm"
  ) {
    return true;
  }

  // Guard pre-restore recovery checkpoints
  if (
    normalized.includes("/checkpoints/") ||
    normalized.startsWith("checkpoints/") ||
    normalized.includes("checkpoint-")
  ) {
    return true;
  }

  // Guard database path if provided
  if (liveDatabasePath) {
    const normDb = liveDatabasePath.replace(/\\/g, "/").toLowerCase();
    if (normalized === normDb || normalized.startsWith(`${normDb}-`)) {
      return true;
    }
  }

  // Guard live attachments directory if provided
  if (liveAttachmentsDir) {
    const normAtt = liveAttachmentsDir.replace(/\\/g, "/").toLowerCase().replace(/\/$/, "");
    if (normalized === normAtt || normalized.startsWith(`${normAtt}/`)) {
      return true;
    }
  }

  // Must only allow deleting archives ending in .pnbackup or .zip
  if (!normalized.endsWith(".pnbackup") && !normalized.endsWith(".zip")) {
    return true; // Not an archive, protected!
  }

  return false;
}


