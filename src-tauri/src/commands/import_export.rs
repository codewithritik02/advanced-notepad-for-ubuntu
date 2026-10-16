use tauri::{AppHandle, Emitter, State};
use crate::storage::{
    crypto::{CryptoSession, SecretBytes},
    errors::CryptoError,
    models::{
        ExportBatchRequest, ExportFileResult, ExportScope, ExportSelectionPreview,
        ExportSingleNoteRequest, ExportSummary, ImportRequest, ImportSummary,
    },
    Database, ImportExportService, ImportPreview,
};

fn get_active_dek(session: &CryptoSession) -> Result<Option<SecretBytes>, String> {
    match session.get_active_dek() {
        Ok(dek) => Ok(Some(dek)),
        Err(CryptoError::NotEnabled) => Ok(None),
        Err(CryptoError::AppLocked) => {
            Err("Application is locked. Please unlock first.".to_string())
        }
        Err(e) => Err(e.user_friendly_message()),
    }
}

/// Scans candidate file paths, inspects format/size, checks duplicates, and returns an ImportPreview report.
/// Rejects when application is locked and encryption is enabled.
#[tauri::command]
pub fn scan_import_files(
    state: State<'_, Database>,
    session: State<'_, CryptoSession>,
    paths: Vec<String>,
) -> Result<ImportPreview, String> {
    let dek = get_active_dek(&session)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::scan_candidates_with_key(&conn, &paths, dek.as_ref())
        .map_err(|e| e.user_friendly_message())
}

/// Executes note import for candidate files with atomic transactions, encryption with active key,
/// progress emission, and cancellation support.
#[tauri::command]
pub fn execute_import(
    app: AppHandle,
    state: State<'_, Database>,
    session: State<'_, CryptoSession>,
    request: ImportRequest,
) -> Result<ImportSummary, String> {
    let _concurrency_guard = crate::storage::services::backup_scheduler::BackupSchedulerService::try_acquire_import_export_lock()
        .map_err(|e| e.user_friendly_message())?;
    let dek = get_active_dek(&session)?;
    let mut conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::execute_import_with_progress_and_key(&mut conn, request, dek.as_ref(), |prog| {
        let _ = app.emit("import-progress", prog);
    })
    .map_err(|e| e.user_friendly_message())
}

/// Cancels an in-flight import or export operation by operation ID.
#[tauri::command]
pub fn cancel_import_export_operation(operation_id: String) -> Result<bool, String> {
    Ok(ImportExportService::cancel_operation(&operation_id))
}

/// Exports an individual note to a user-selected destination file.
/// Decrypts ciphertext with active DEK before writing requested unencrypted file representation.
#[tauri::command]
pub fn export_single_note(
    state: State<'_, Database>,
    session: State<'_, CryptoSession>,
    request: ExportSingleNoteRequest,
) -> Result<ExportFileResult, String> {
    let dek = get_active_dek(&session)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::export_single_note_with_key(&conn, request, dek.as_ref())
        .map_err(|e| e.user_friendly_message())
}

/// Previews a proposed export selection, calculating counts and lightweight metadata without loading full content.
/// Rejects when locked to avoid leaking protected note titles or metadata.
#[tauri::command]
pub fn preview_export_selection(
    state: State<'_, Database>,
    session: State<'_, CryptoSession>,
    scope: ExportScope,
) -> Result<ExportSelectionPreview, String> {
    let dek = get_active_dek(&session)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::resolve_export_selection_with_key(&conn, &scope, dek.as_ref())
        .map_err(|e| e.user_friendly_message())
}

/// Executes a batch export for multiple notes or notebook selection with progress emission and cancellation support.
/// Decrypts notes using active DEK before writing requested export files.
#[tauri::command]
pub fn export_notes_batch(
    app: AppHandle,
    state: State<'_, Database>,
    session: State<'_, CryptoSession>,
    request: ExportBatchRequest,
) -> Result<ExportSummary, String> {
    let _concurrency_guard = crate::storage::services::backup_scheduler::BackupSchedulerService::try_acquire_import_export_lock()
        .map_err(|e| e.user_friendly_message())?;
    let dek = get_active_dek(&session)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::export_notes_batch_with_progress_and_key(&conn, request, dek.as_ref(), |prog| {
        let _ = app.emit("export-progress", prog);
    })
    .map_err(|e| e.user_friendly_message())
}
