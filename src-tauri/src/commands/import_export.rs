use tauri::{AppHandle, Emitter, State};
use crate::storage::{
    models::{
        ExportBatchRequest, ExportFileResult, ExportScope, ExportSelectionPreview,
        ExportSingleNoteRequest, ExportSummary, ImportRequest, ImportSummary,
    },
    Database, ImportExportService, ImportPreview,
};

/// Scans candidate file paths, inspects format/size, checks duplicates, and returns an ImportPreview report.
#[tauri::command]
pub fn scan_import_files(
    state: State<'_, Database>,
    paths: Vec<String>,
) -> Result<ImportPreview, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::scan_candidates(&conn, &paths)
        .map_err(|e| e.user_friendly_message())
}

/// Executes note import for candidate files with atomic transactions, progress emission, and cancellation support.
#[tauri::command]
pub fn execute_import(
    app: AppHandle,
    state: State<'_, Database>,
    request: ImportRequest,
) -> Result<ImportSummary, String> {
    let mut conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::execute_import_with_progress(&mut conn, request, |prog| {
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
#[tauri::command]
pub fn export_single_note(
    state: State<'_, Database>,
    request: ExportSingleNoteRequest,
) -> Result<ExportFileResult, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::export_single_note(&conn, request)
        .map_err(|e| e.user_friendly_message())
}

/// Previews a proposed export selection, calculating counts and lightweight metadata without loading full content.
#[tauri::command]
pub fn preview_export_selection(
    state: State<'_, Database>,
    scope: ExportScope,
) -> Result<ExportSelectionPreview, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::resolve_export_selection(&conn, &scope)
        .map_err(|e| e.user_friendly_message())
}

/// Executes a batch export for multiple notes or notebook selection with progress emission and cancellation support.
#[tauri::command]
pub fn export_notes_batch(
    app: AppHandle,
    state: State<'_, Database>,
    request: ExportBatchRequest,
) -> Result<ExportSummary, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ImportExportService::export_notes_batch_with_progress(&conn, request, |prog| {
        let _ = app.emit("export-progress", prog);
    })
    .map_err(|e| e.user_friendly_message())
}

