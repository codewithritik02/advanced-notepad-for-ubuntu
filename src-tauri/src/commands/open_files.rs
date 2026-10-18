use tauri::State;

use crate::open_files::{
    close_external_document as close_document, list_external_documents as list_documents,
    open_files_as_notes, set_external_pinned as set_pinned, ExternalDocument, OpenFilesReport,
    PendingOpenFiles,
};
use crate::storage::errors::CryptoError;
use crate::storage::{CryptoSession, Database};

fn ensure_unlocked(session: &CryptoSession) -> Result<(), String> {
    match session.get_active_dek() {
        Ok(_) | Err(CryptoError::NotEnabled) => Ok(()),
        Err(CryptoError::AppLocked) => {
            Err("Application is locked. Please unlock first.".to_string())
        }
        Err(error) => Err(error.user_friendly_message()),
    }
}

/// Opens the files that the desktop handed to the app ("Open with Personal Notepad", a double
/// click, or a second launch) as external documents, and returns what was opened and what failed.
///
/// While the app is locked nothing is consumed: the paths wait until the user unlocks it.
#[tauri::command]
pub fn process_pending_open_files(
    db: State<'_, Database>,
    session: State<'_, CryptoSession>,
    pending: State<'_, PendingOpenFiles>,
) -> Result<OpenFilesReport, String> {
    if pending.is_empty() {
        return Ok(OpenFilesReport::default());
    }
    ensure_unlocked(&session)?;

    let paths = pending.take_all();
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    Ok(open_files_as_notes(&conn, &paths))
}

/// Files opened from outside ("Open with"), newest first. They are not part of the notes list.
#[tauri::command]
pub fn list_external_documents(
    db: State<'_, Database>,
    session: State<'_, CryptoSession>,
) -> Result<Vec<ExternalDocument>, String> {
    ensure_unlocked(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    list_documents(&conn)
}

/// Removes a document from the External list. The file on disk is left untouched.
#[tauri::command]
pub fn close_external_document(
    db: State<'_, Database>,
    session: State<'_, CryptoSession>,
    id: String,
) -> Result<bool, String> {
    ensure_unlocked(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    close_document(&conn, &id)
}

/// Opens one file by path (used by the Recent menu) as an external document.
#[tauri::command]
pub fn open_external_file(
    db: State<'_, Database>,
    session: State<'_, CryptoSession>,
    path: String,
) -> Result<OpenFilesReport, String> {
    ensure_unlocked(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    Ok(open_files_as_notes(
        &conn,
        &[std::path::PathBuf::from(path)],
    ))
}

/// Pins a document so it stays in the External list next time; unpinned ones are cleared on restart.
#[tauri::command]
pub fn set_external_pinned(
    db: State<'_, Database>,
    session: State<'_, CryptoSession>,
    id: String,
    pinned: bool,
) -> Result<bool, String> {
    ensure_unlocked(&session)?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    set_pinned(&conn, &id, pinned)
}
