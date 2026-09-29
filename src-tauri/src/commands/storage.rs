use serde::Serialize;
use tauri::State;
use crate::storage::{
    models::{CreateNotebookDto, CreateNoteDto, Note, Notebook, Tag, UpdateNoteDto},
    repositories::{NotebookRepository, NoteRepository, SettingsRepository, TagRepository},
    Database, StoragePaths,
};

#[derive(Debug, Clone, Serialize)]
pub struct StorageInfo {
    pub data_dir: String,
    pub database_file: String,
    pub is_initialized: bool,
}

#[tauri::command]
pub fn get_storage_info(paths: State<'_, StoragePaths>) -> Result<StorageInfo, String> {
    Ok(StorageInfo {
        data_dir: paths.root.to_string_lossy().to_string(),
        database_file: paths.database_file.to_string_lossy().to_string(),
        is_initialized: paths.database_file.exists(),
    })
}

#[tauri::command]
pub fn list_notes(
    db: State<'_, Database>,
    include_deleted: Option<bool>,
) -> Result<Vec<Note>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::list(&conn, include_deleted.unwrap_or(false)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note(db: State<'_, Database>, id: String) -> Result<Option<Note>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::get_by_id(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_note(db: State<'_, Database>, dto: CreateNoteDto) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::create(&conn, dto).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_note(
    db: State<'_, Database>,
    id: String,
    dto: UpdateNoteDto,
) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::update(&conn, &id, dto).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(
    db: State<'_, Database>,
    id: String,
    soft: Option<bool>,
) -> Result<bool, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::delete(&conn, &id, soft.unwrap_or(true)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notebooks(db: State<'_, Database>) -> Result<Vec<Notebook>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_notebook(
    db: State<'_, Database>,
    dto: CreateNotebookDto,
) -> Result<Notebook, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::create(&conn, dto).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tags(db: State<'_, Database>) -> Result<Vec<Tag>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(db: State<'_, Database>, name: String) -> Result<Tag, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::create(&conn, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_note_tag(
    db: State<'_, Database>,
    note_id: String,
    tag_id: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::add_tag_to_note(&conn, &note_id, &tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_tags(db: State<'_, Database>, note_id: String) -> Result<Vec<Tag>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::get_tags_for_note(&conn, &note_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_setting(db: State<'_, Database>, key: String) -> Result<Option<String>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    SettingsRepository::get(&conn, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(
    db: State<'_, Database>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    SettingsRepository::set(&conn, &key, &value).map_err(|e| e.to_string())
}
