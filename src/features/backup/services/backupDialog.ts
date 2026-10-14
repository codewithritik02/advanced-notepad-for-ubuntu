import { open, save } from "@tauri-apps/plugin-dialog";
export {
  formatBackupBytes,
  getBackupStageDescription,
  sanitizeBackupFilename,
  generateDefaultBackupFilename,
  sortBackupsDescending,
  isDeletionTargetProtected,
} from "../utils/backupUtils";

export interface BackupFilePickerResult {
  cancelled: boolean;
  path: string | null;
}

/**
 * Native save picker for selecting a custom destination .pnbackup archive file.
 */
export async function pickBackupDestinationFile(
  defaultName?: string
): Promise<BackupFilePickerResult> {
  try {
    const defaultPath = defaultName || "personal-notepad-backup.pnbackup";
    const selected = await save({
      title: "Choose Backup Destination",
      defaultPath,
      filters: [
        {
          name: "Personal Notepad Backup (*.pnbackup)",
          extensions: ["pnbackup"],
        },
        {
          name: "Zip Archive (*.zip)",
          extensions: ["zip"],
        },
      ],
    });

    if (!selected) {
      return { cancelled: true, path: null };
    }

    return { cancelled: false, path: selected };
  } catch (err) {
    console.warn("Backup save destination selection cancelled or failed:", err);
    return { cancelled: true, path: null };
  }
}

/**
 * Native directory picker for selecting an automatic backup destination folder.
 */
export async function pickBackupDestinationFolder(): Promise<BackupFilePickerResult> {
  try {
    const selected = await open({
      title: "Select Automatic Backup Folder",
      multiple: false,
      directory: true,
    });

    if (!selected) {
      return { cancelled: true, path: null };
    }

    const path = typeof selected === "string" ? selected : selected[0];
    return { cancelled: false, path: path || null };
  } catch (err) {
    console.warn("Backup folder selection cancelled or failed:", err);
    return { cancelled: true, path: null };
  }
}

/**
 * Native file picker for selecting an existing backup archive from any disk location to inspect or restore.
 */
export async function pickBackupArchiveFile(): Promise<BackupFilePickerResult> {
  try {
    const selected = await open({
      title: "Select Backup Archive",
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Personal Notepad Backup (*.pnbackup)",
          extensions: ["pnbackup"],
        },
        {
          name: "Zip Archive (*.zip)",
          extensions: ["zip"],
        },
      ],
    });

    if (!selected) {
      return { cancelled: true, path: null };
    }

    const path = typeof selected === "string" ? selected : selected[0];
    return { cancelled: false, path: path || null };
  } catch (err) {
    console.warn("Backup archive file selection cancelled or failed:", err);
    return { cancelled: true, path: null };
  }
}

/**
 * Opens the containing folder of a backup archive in the native file manager (Phase 11, Task 13).
 */
export async function openContainingFolder(filePath: string): Promise<boolean> {
  try {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(filePath);
    return true;
  } catch (err) {
    console.warn("Failed to reveal item in folder:", err);
    return false;
  }
}

