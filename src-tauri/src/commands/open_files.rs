use tauri::State;

use crate::open_files::{open_files_as_notes, OpenFilesReport, PendingOpenFiles};
use crate::storage::errors::CryptoError;
use crate::storage::{CryptoSession, Database};

/// Opens the files that the desktop handed to the app ("Open with Personal Notepad", a double
/// click, or a second launch) as notes, and returns what was opened and what failed.
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
    let dek = match session.get_active_dek() {
        Ok(dek) => Some(dek),
        Err(CryptoError::NotEnabled) => None,
        Err(CryptoError::AppLocked) => {
            return Err("Application is locked. Please unlock first.".to_string())
        }
        Err(error) => return Err(error.user_friendly_message()),
    };

    let paths = pending.take_all();
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    Ok(open_files_as_notes(&conn, dek.as_ref(), &paths))
}
