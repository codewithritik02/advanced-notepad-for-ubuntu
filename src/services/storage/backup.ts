import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  BackupCreateRequest,
  BackupCreateResult,
  BackupHealthStatus,
  BackupListItem,
  BackupProgressEvent,
  BackupRestoreRequest,
  BackupRestoreResult,
  BackupSchedulerState,
  BackupSettings,
  BackupValidationResult,
  PostRestoreValidationReport,
  RecoveryCheckpoint,
  RestorePreviewResult,
  StagedMigrationResult,
  StagedRestoreSummary,
} from "./types";

export const backupStorage = {
  /**
   * Triggers creation of a complete, verified manual or automatic backup.
   * Emits live progress events and ensures the resulting archive is validated before returning.
   */
  async createBackup(
    request: BackupCreateRequest = {},
    onProgress?: (progress: BackupProgressEvent) => void
  ): Promise<BackupCreateResult> {
    let unlisten: UnlistenFn | undefined;
    if (onProgress) {
      unlisten = await listen<BackupProgressEvent>("backup-progress", (event) => {
        onProgress(event.payload);
      });
    }

    try {
      return await invoke<BackupCreateResult>("create_backup", { request });
    } finally {
      if (unlisten) {
        unlisten();
      }
    }
  },

  /**
   * Evaluates automatic backup policy and executes a backup if due (Phase 11, Task 11).
   * Runs non-blockingly without throwing, and emits events for progress or completion.
   */
  async checkAndRunAutomaticBackup(
    isStartup: boolean = false,
    onProgress?: (progress: BackupProgressEvent) => void
  ): Promise<BackupCreateResult | null> {
    let unlisten: UnlistenFn | undefined;
    if (onProgress) {
      unlisten = await listen<BackupProgressEvent>("backup-progress", (event) => {
        onProgress(event.payload);
      });
    }

    try {
      return await invoke<BackupCreateResult | null>(
        "check_and_run_automatic_backup",
        { isStartup }
      );
    } finally {
      if (unlisten) {
        unlisten();
      }
    }
  },

  /**
   * Retrieves persistent scheduler runtime state (Phase 11, Task 11).
   */
  async getSchedulerState(): Promise<BackupSchedulerState> {
    return await invoke<BackupSchedulerState>("get_backup_scheduler_state");
  },

  /**
   * Discovers and lists all available backup archives across configured destinations (Phase 11, Task 13).
   */
  async listBackups(): Promise<BackupListItem[]> {
    return await invoke<BackupListItem[]>("list_backups");
  },

  /**
   * Safely deletes a managed backup archive upon explicit user action (Phase 11, Task 13).
   */
  async deleteManagedBackup(filePath: string): Promise<boolean> {
    return await invoke<boolean>("delete_managed_backup", { filePath });
  },

  /**
   * Subscribes to live backup progress events emitted across the application lifecycle.
   */
  async onBackupProgress(
    callback: (progress: BackupProgressEvent) => void
  ): Promise<UnlistenFn> {
    return await listen<BackupProgressEvent>("backup-progress", (event) => {
      callback(event.payload);
    });
  },

  /**
   * Retrieves persistent automatic backup settings from the SQLite database.
   */
  async getSettings(): Promise<BackupSettings> {
    return await invoke<BackupSettings>("get_backup_settings");
  },

  /**
   * Validates and persists updated automatic backup settings.
   */
  async updateSettings(settings: BackupSettings): Promise<BackupSettings> {
    return await invoke<BackupSettings>("update_backup_settings", { settings });
  },

  /**
   * Deeply inspects and validates a backup archive before restoring (Phase 11, Task 14).
   * Non-destructively inspects archive readability, manifest checksums, database integrity,
   * foreign key constraints, table/column schema, attachment inventory, and schema version compatibility.
   */
  async validateBackup(filePath: string): Promise<BackupValidationResult> {
    return await invoke<BackupValidationResult>("validate_backup", { filePath });
  },

  /**
   * Generates a comprehensive, non-destructive restore preview for a candidate backup archive (Phase 11, Task 15).
   * Revalidates candidate backup, inspects content counts, verifies compatibility, and compiles warnings.
   */
  async previewRestore(filePath: string): Promise<RestorePreviewResult> {
    return await invoke<RestorePreviewResult>("preview_restore", { filePath });
  },

  /**
   * Creates an immediate, verified pre-restore recovery checkpoint of current working data (Phase 11, Task 16).
   * Must be called before destructive changes begin. Fails fast if snapshotting or validation fails.
   */
  async createRecoveryCheckpoint(): Promise<RecoveryCheckpoint> {
    return await invoke<RecoveryCheckpoint>("create_recovery_checkpoint");
  },

  /**
   * Discovers and lists all pre-restore recovery checkpoints in descending chronological order (Phase 11, Task 16).
   */
  async listRecoveryCheckpoints(): Promise<RecoveryCheckpoint[]> {
    return await invoke<RecoveryCheckpoint[]>("list_recovery_checkpoints");
  },

  /**
   * Retrieves the most recently created recovery checkpoint, if any (Phase 11, Task 16).
   */
  async getLatestRecoveryCheckpoint(): Promise<RecoveryCheckpoint | null> {
    return await invoke<RecoveryCheckpoint | null>("get_latest_recovery_checkpoint");
  },

  /**
   * Safely extracts a candidate backup archive into an isolated staging location and verifies contents (Phase 11, Task 17).
   * Enforces decompression quotas, Zip-Slip path sanitization, duplicate rejection, and staged data integrity checks.
   */
  async stageRestoreArchive(filePath: string): Promise<StagedRestoreSummary> {
    return await invoke<StagedRestoreSummary>("stage_restore_archive", { filePath });
  },

  /**
   * Applies schema migrations to a staged database before activation (Phase 11, Task 18).
   * Ensures older supported backups (v1, v2, v3) are upgraded to the current application schema (v4).
   */
  async migrateStagedDatabase(stagingDir: string): Promise<StagedMigrationResult> {
    return await invoke<StagedMigrationResult>("migrate_staged_database", { stagingDir });
  },

  /**
   * Cleans up an isolated restore staging directory after restore completion or cancellation (Phase 11, Task 17).
   */
  async cleanupStagedRestore(stagingDir: string): Promise<boolean> {
    return await invoke<boolean>("cleanup_staged_restore", { stagingDir });
  },

  /**
   * Safely activates restored database and attachments with automated rollback on failure (Phase 11, Task 19).
   * Validates archive, stages extraction, runs migrations if necessary,
   * creates a pre-restore recovery checkpoint, safely swaps database and attachments,
   * reinitializes database and search indexes, and automatically rolls back if any check fails.
   */
  async executeRestore(
    request: BackupRestoreRequest,
    onProgress?: (progress: BackupProgressEvent) => void
  ): Promise<BackupRestoreResult> {
    let unlisten: UnlistenFn | undefined;
    if (onProgress) {
      unlisten = await listen<BackupProgressEvent>("backup-progress", (event) => {
        onProgress(event.payload);
      });
    }

    try {
      return await invoke<BackupRestoreResult>("execute_restore", { request });
    } finally {
      if (unlisten) {
        unlisten();
      }
    }
  },

  /**
   * Checks for and recovers previous interrupted restore operation after an unexpected shutdown (Phase 11, Task 19).
   */
  async checkAndRecoverInterruptedRestore(): Promise<string | null> {
    return await invoke<string | null>("check_and_recover_interrupted_restore");
  },

  /**
   * Requests cancellation of an in-progress restore operation during its safe preparation phase (Phase 11, Task 21).
   */
  async cancelRestore(operationId: string): Promise<boolean> {
    return await invoke<boolean>("cancel_restore_operation", { operationId });
  },

  /**
   * Performs comprehensive post-restore validation ensuring active SQLite integrity,
   * schema version, foreign keys, tables, attachments, and FTS5 search index (Phase 11, Task 20).
   */
  async validatePostRestoreState(): Promise<PostRestoreValidationReport> {
    return await invoke<PostRestoreValidationReport>("validate_post_restore_state");
  },

  /**
   * Retrieves authoritative backup health status, destination accessibility,
   * failure categories, and active operational warnings (Phase 11, Task 22).
   */
  async getHealthStatus(): Promise<BackupHealthStatus> {
    return await invoke<BackupHealthStatus>("get_backup_health_status");
  },
};



