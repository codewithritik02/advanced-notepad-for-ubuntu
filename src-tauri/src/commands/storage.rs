use serde::Serialize;
use tauri::State;
use crate::storage::{
    models::{
        CreateNotebookDto, CreateNoteDto, Note, Notebook, SearchResult, Tag, UpdateNoteDto,
        UpdateNotebookDto, UpdateTagDto,
    },
    repositories::{
        NotebookRepository, NoteRepository, SearchRepository, SettingsRepository, TagRepository,
    },
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
    notebook_id: Option<String>,
    unfiled_only: Option<bool>,
) -> Result<Vec<Note>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::list_filtered(
        &conn,
        include_deleted.unwrap_or(false),
        notebook_id.as_deref(),
        unfiled_only.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
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
pub fn restore_note(db: State<'_, Database>, id: String) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::restore(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_note_to_notebook(
    db: State<'_, Database>,
    id: String,
    notebook_id: Option<String>,
) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::move_to_notebook(&conn, &id, notebook_id.as_deref())
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn set_note_favorite(
    db: State<'_, Database>,
    id: String,
    is_favorite: bool,
) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::set_favorite(&conn, &id, is_favorite).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn toggle_note_favorite(
    db: State<'_, Database>,
    id: String,
) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::toggle_favorite(&conn, &id).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn list_favorite_notes(db: State<'_, Database>) -> Result<Vec<Note>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::list_favorites(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notebooks(db: State<'_, Database>) -> Result<Vec<Notebook>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_notebook(db: State<'_, Database>, id: String) -> Result<Option<Notebook>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::get_by_id(&conn, &id).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn create_notebook(
    db: State<'_, Database>,
    dto: CreateNotebookDto,
) -> Result<Notebook, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::create(&conn, dto).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn update_notebook(
    db: State<'_, Database>,
    id: String,
    dto: UpdateNotebookDto,
) -> Result<Notebook, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::update(&conn, &id, dto).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn delete_notebook(db: State<'_, Database>, id: String) -> Result<bool, String> {
    let mut conn = db.conn.lock().map_err(|e| e.to_string())?;
    NotebookRepository::delete(&mut conn, &id).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn list_tags(db: State<'_, Database>) -> Result<Vec<Tag>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_tag(db: State<'_, Database>, id: String) -> Result<Option<Tag>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::get_by_id(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(db: State<'_, Database>, name: String) -> Result<Tag, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::create(&conn, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_tag(
    db: State<'_, Database>,
    id: String,
    dto: UpdateTagDto,
) -> Result<Tag, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::update(&conn, &id, dto).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_tag(db: State<'_, Database>, id: String) -> Result<bool, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::delete(&conn, &id).map_err(|e| e.to_string())
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
pub fn remove_note_tag(
    db: State<'_, Database>,
    note_id: String,
    tag_id: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::remove_tag_from_note(&conn, &note_id, &tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_tags(db: State<'_, Database>, note_id: String) -> Result<Vec<Tag>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::get_tags_for_note(&conn, &note_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_note_tags(
    db: State<'_, Database>,
    note_id: String,
    tag_ids: Vec<String>,
) -> Result<Vec<Tag>, String> {
    let mut conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::set_tags_for_note(&mut conn, &note_id, &tag_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_notes_for_tag(
    db: State<'_, Database>,
    tag_id: String,
) -> Result<Vec<Note>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::list_by_tag(&conn, &tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_tag_note_counts(
    db: State<'_, Database>,
) -> Result<std::collections::HashMap<String, usize>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::get_tag_note_counts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_all_notes_tags(
    db: State<'_, Database>,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::get_all_notes_tag_names(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_tags_for_notes(
    db: State<'_, Database>,
    note_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TagRepository::get_tags_for_notes(&conn, &note_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_metadata(
    db: State<'_, Database>,
    note_id: String,
) -> Result<Option<crate::storage::models::NoteMetadata>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::get_metadata(&conn, &note_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_notebook_path(
    db: State<'_, Database>,
    note_id: String,
) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::get_notebook_path(&conn, &note_id).map_err(|e| e.to_string())
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

#[tauri::command]
pub fn search_notes(
    db: State<'_, Database>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    SearchRepository::search(&conn, &query, limit).map_err(|e| e.user_friendly_message())
}
