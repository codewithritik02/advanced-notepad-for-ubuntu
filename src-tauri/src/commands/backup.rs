use std::path::Path;
use tauri::{AppHandle, Emitter, State};
use crate::storage::{
    models::{
        BackupCreateRequest, BackupCreateResult, BackupHealthStatus, BackupListItem,
        BackupRestoreRequest, BackupRestoreResult, BackupSchedulerState, BackupSettings,
        BackupValidationResult, PostRestoreValidationReport, RecoveryCheckpoint,
        RestorePreviewResult, StagedMigrationResult, StagedRestoreSummary,
    },
    services::{
        BackupHealthService, BackupListService, BackupSchedulerService, BackupService,
        BackupSettingsService, BackupValidationService, RestoreService,
    },
    Database, StoragePaths,
};

/// Creates a consistent, verified manual or automatic backup of Personal Notepad data.
#[tauri::command]
pub fn create_backup(
    app: AppHandle,
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    request: BackupCreateRequest,
) -> Result<BackupCreateResult, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    BackupService::create_backup(
        &conn,
        &paths,
        request,
        Some(|evt| {
            let _ = app.emit("backup-progress", evt);
        }),
    )
    .map_err(|e| e.user_friendly_message())
}

/// Retrieves persistent automatic backup settings (Phase 11, Task 10).
#[tauri::command]
pub fn get_backup_settings(state: State<'_, Database>) -> Result<BackupSettings, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    BackupSettingsService::get_settings(&conn).map_err(|e| e.user_friendly_message())
}

/// Validates and saves updated automatic backup settings (Phase 11, Task 10).
#[tauri::command]
pub fn update_backup_settings(
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    settings: BackupSettings,
) -> Result<BackupSettings, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    BackupSettingsService::save_settings(&conn, &paths, settings)
        .map_err(|e| e.user_friendly_message())
}

/// Evaluates automatic backup policy and executes backup if due (Phase 11, Task 11).
#[tauri::command]
pub fn check_and_run_automatic_backup(
    app: AppHandle,
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    is_startup: bool,
) -> Result<Option<BackupCreateResult>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    match BackupSchedulerService::check_and_run_backup(
        &conn,
        &paths,
        is_startup,
        Some(|evt| {
            let _ = app.emit("backup-progress", evt);
        }),
    ) {
        Ok(Some(result)) => {
            let _ = app.emit("automatic-backup-completed", &result);
            Ok(Some(result))
        }
        Ok(None) => Ok(None),
        Err(err) => {
            let msg = err.user_friendly_message();
            let _ = app.emit("automatic-backup-failed", &msg);
            Err(msg)
        }
    }
}

/// Retrieves the current persistent scheduler state (Phase 11, Task 11).
#[tauri::command]
pub fn get_backup_scheduler_state(
    state: State<'_, Database>,
) -> Result<BackupSchedulerState, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    BackupSchedulerService::get_state(&conn).map_err(|e| e.user_friendly_message())
}

/// Discovers and lists all available backup archives across configured destinations (Phase 11, Task 13).
#[tauri::command]
pub fn list_backups(
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
) -> Result<Vec<BackupListItem>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    BackupListService::list_backups(&conn, &paths).map_err(|e| e.user_friendly_message())
}

/// Safely removes an existing managed backup archive upon explicit user action (Phase 11, Task 13).
#[tauri::command]
pub fn delete_managed_backup(
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    file_path: String,
) -> Result<bool, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    BackupListService::delete_managed_backup(&conn, &paths, &file_path)
        .map_err(|e| e.user_friendly_message())
}

/// Inspects and validates a backup archive before allowing restore (Phase 11, Task 14).
#[tauri::command]
pub fn validate_backup(
    paths: State<'_, StoragePaths>,
    file_path: String,
) -> Result<BackupValidationResult, String> {
    let archive_path = Path::new(file_path.trim());
    BackupValidationService::validate_backup(archive_path, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Generates a comprehensive, non-destructive restore preview for a candidate backup archive (Phase 11, Task 15).
#[tauri::command]
pub fn preview_restore(
    paths: State<'_, StoragePaths>,
    file_path: String,
) -> Result<RestorePreviewResult, String> {
    let archive_path = Path::new(file_path.trim());
    RestoreService::preview_restore(archive_path, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Creates an immediate, verified pre-restore recovery checkpoint of current working data (Phase 11, Task 16).
#[tauri::command]
pub fn create_recovery_checkpoint(
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
) -> Result<RecoveryCheckpoint, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    RestoreService::create_recovery_checkpoint(&conn, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Lists all available pre-restore recovery checkpoints in descending chronological order (Phase 11, Task 16).
#[tauri::command]
pub fn list_recovery_checkpoints(
    paths: State<'_, StoragePaths>,
) -> Result<Vec<RecoveryCheckpoint>, String> {
    RestoreService::list_recovery_checkpoints(&paths)
        .map_err(|e| e.user_friendly_message())
}

/// Retrieves the most recent verified recovery checkpoint, if any (Phase 11, Task 16).
#[tauri::command]
pub fn get_latest_recovery_checkpoint(
    paths: State<'_, StoragePaths>,
) -> Result<Option<RecoveryCheckpoint>, String> {
    RestoreService::get_latest_recovery_checkpoint(&paths)
        .map_err(|e| e.user_friendly_message())
}

/// Safely extracts candidate backup archive into an isolated staging location and verifies contents (Phase 11, Task 17).
#[tauri::command]
pub fn stage_restore_archive(
    paths: State<'_, StoragePaths>,
    file_path: String,
) -> Result<StagedRestoreSummary, String> {
    let archive_path = Path::new(file_path.trim());
    RestoreService::stage_restore_archive(archive_path, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Applies required database schema migrations on a staged database before activation (Phase 11, Task 18).
#[tauri::command]
pub fn migrate_staged_database(
    paths: State<'_, StoragePaths>,
    staging_dir: String,
) -> Result<StagedMigrationResult, String> {
    let staging_path = Path::new(staging_dir.trim());
    RestoreService::migrate_staged_database(staging_path, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Cleans up an isolated restore staging directory after restore completion or cancellation (Phase 11, Task 17).
#[tauri::command]
pub fn cleanup_staged_restore(
    paths: State<'_, StoragePaths>,
    staging_dir: String,
) -> Result<bool, String> {
    let staging_path = Path::new(staging_dir.trim());
    RestoreService::cleanup_staged_restore(staging_path, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Safely replaces active database and attachments with verified backup data, with automated rollback on failure (Phase 11, Task 19).
#[tauri::command]
pub fn execute_restore(
    app: AppHandle,
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    request: BackupRestoreRequest,
) -> Result<BackupRestoreResult, String> {
    let mut conn = state.conn.lock().map_err(|e| e.to_string())?;
    RestoreService::execute_restore(
        &mut *conn,
        &paths,
        request,
        Some(|evt| {
            let _ = app.emit("backup-progress", evt);
        }),
    )
    .map_err(|e| e.user_friendly_message())
}

/// Checks and recovers previous interrupted restore operation if an unexpected shutdown occurred (Phase 11, Task 19).
#[tauri::command]
pub fn check_and_recover_interrupted_restore(
    paths: State<'_, StoragePaths>,
) -> Result<Option<String>, String> {
    RestoreService::recover_if_interrupted(&paths).map_err(|e| e.user_friendly_message())
}

/// Runs comprehensive post-restore validation and ensures application state is fully initialized (Phase 11, Task 20).
#[tauri::command]
pub fn validate_post_restore_state(
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
) -> Result<PostRestoreValidationReport, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    RestoreService::verify_and_reinitialize_activated_state(&conn, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Cancels an in-flight restore operation during its cancellable preparation phase (Phase 11, Task 21).
#[tauri::command]
pub fn cancel_restore_operation(
    operation_id: String,
) -> Result<bool, String> {
    Ok(RestoreService::request_cancellation(&operation_id))
}

/// Retrieves authoritative backup operational health, destination status, and discoverable warnings (Phase 11, Task 22).
#[tauri::command]
pub fn get_backup_health_status(
    state: State<'_, Database>,
    paths: State<'_, StoragePaths>,
) -> Result<BackupHealthStatus, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    BackupHealthService::get_health_status(&conn, &paths).map_err(|e| e.user_friendly_message())
}






