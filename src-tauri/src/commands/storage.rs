use serde::Serialize;
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use crate::storage::{
    attachment_storage::AttachmentStorage,
    errors::{AttachmentError, StorageError},
    models::{
        AttachmentConsistencyReport, AttachmentMetadataDto, BacklinkItem, CreateNotebookDto, CreateNoteDto,
        EmptyTrashResult, Note, Notebook, PermanentDeleteResult, ResolvedLinkTarget, SearchResult, Tag,
        TrashedNoteItem, UpdateNoteDto, UpdateNotebookDto, UpdateTagDto,
    },
    repositories::{
        AttachmentRepository, NotebookRepository, NoteRepository, SettingsRepository,
        TagRepository,
    },
    services::{AttachmentService, InternalLinkService, SearchService, TrashService},
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
    paths: State<'_, StoragePaths>,
    id: String,
    soft: Option<bool>,
) -> Result<bool, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let is_soft = soft.unwrap_or(true);
    if is_soft {
        TrashService::soft_delete(&conn, &id)
            .map(|_| true)
            .map_err(|e| e.user_friendly_message())
    } else {
        let storage = AttachmentStorage::new((*paths).clone());
        let _ = storage.remove_note_directory(&id);
        TrashService::permanently_delete_note(&conn, &storage, &id)
            .map(|res| res.success)
            .map_err(|e| e.user_friendly_message())
    }
}

#[tauri::command]
pub fn soft_delete_note(db: State<'_, Database>, id: String) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TrashService::soft_delete(&conn, &id).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn restore_note(db: State<'_, Database>, id: String) -> Result<Note, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    TrashService::restore(&conn, &id).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn permanently_delete_note(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    id: String,
) -> Result<PermanentDeleteResult, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let storage = AttachmentStorage::new((*paths).clone());
    TrashService::permanently_delete_note(&conn, &storage, &id)
        .map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn empty_trash(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
) -> Result<EmptyTrashResult, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let storage = AttachmentStorage::new((*paths).clone());
    TrashService::empty_trash(&conn, &storage).map_err(|e| e.user_friendly_message())
}

#[tauri::command]
pub fn list_trashed_notes(db: State<'_, Database>) -> Result<Vec<TrashedNoteItem>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    NoteRepository::list_trashed_items(&conn).map_err(|e| e.user_friendly_message())
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
    let conn = db.conn.lock().map_err(|_| "Unable to search notes.".to_string())?;
    SearchService::search(&conn, &query, limit).map_err(|e| match e {
        StorageError::Validation(msg) => msg,
        StorageError::SearchError(_) | StorageError::Database(_) => "Unable to search notes.".to_string(),
        other => other.user_friendly_message(),
    })
}

/// Adds a file as an attachment to a note (Phase 7, Task 8).
/// Validates input, delegates to AttachmentService, and returns a sanitized AttachmentMetadataDto.
#[tauri::command]
pub fn add_attachment(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    note_id: String,
    source_path: String,
) -> Result<AttachmentMetadataDto, String> {
    let trimmed_note_id = note_id.trim();
    if trimmed_note_id.is_empty() {
        return Err("Note ID cannot be empty.".to_string());
    }
    let trimmed_source = source_path.trim();
    if trimmed_source.is_empty() {
        return Err("Source file path cannot be empty.".to_string());
    }

    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    let storage = AttachmentStorage::new((*paths).clone());
    let src = std::path::Path::new(trimmed_source);

    let attachment = AttachmentService::add_attachment(&conn, &storage, trimmed_note_id, src)
        .map_err(|e| e.user_friendly_message())?;

    Ok(AttachmentMetadataDto::from(attachment))
}

/// Lists all attachments for a specific note (Phase 7, Task 8).
/// Returns deterministic, frontend-safe AttachmentMetadataDto items without leaking internal storage paths.
#[tauri::command]
pub fn list_attachments(
    db: State<'_, Database>,
    note_id: String,
) -> Result<Vec<AttachmentMetadataDto>, String> {
    let trimmed_note_id = note_id.trim();
    if trimmed_note_id.is_empty() {
        return Err("Note ID cannot be empty.".to_string());
    }

    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;

    let attachments = AttachmentService::list_attachments(&conn, trimmed_note_id)
        .map_err(|e| e.user_friendly_message())?;

    Ok(attachments.into_iter().map(AttachmentMetadataDto::from).collect())
}

/// Removes an attachment from a note and cleans up its managed file safely (Phase 7, Task 8).
#[tauri::command]
pub fn remove_attachment(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    id: String,
) -> Result<bool, String> {
    let trimmed_id = id.trim();
    if trimmed_id.is_empty() {
        return Err("Attachment ID cannot be empty.".to_string());
    }

    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    let storage = AttachmentStorage::new((*paths).clone());

    AttachmentService::remove_attachment(&conn, &storage, trimmed_id)
        .map_err(|e| e.user_friendly_message())?;

    Ok(true)
}

/// Opens an attachment using the operating system's default viewer via the approved Tauri opener (Phase 7, Task 8).
/// Resolves and validates the safe managed path without allowing arbitrary filesystem opening.
#[tauri::command]
pub fn open_attachment(
    app: tauri::AppHandle,
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
    id: String,
) -> Result<(), String> {
    let trimmed_id = id.trim();
    if trimmed_id.is_empty() {
        return Err("Attachment ID cannot be empty.".to_string());
    }

    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    let storage = AttachmentStorage::new((*paths).clone());

    let safe_path = AttachmentService::get_attachment_for_open(&conn, &storage, trimmed_id)
        .map_err(|e| e.user_friendly_message())?;

    app.opener()
        .open_path(safe_path.to_string_lossy(), None::<&str>)
        .map_err(|e| AttachmentError::OpenFailed(e.to_string()).user_friendly_message())?;

    Ok(())
}

/// Returns the number of attachments associated with a note (Phase 7, Task 19).
#[tauri::command]
pub fn get_attachment_count(
    db: State<'_, Database>,
    note_id: String,
) -> Result<usize, String> {
    let trimmed_note_id = note_id.trim();
    if trimmed_note_id.is_empty() {
        return Ok(0);
    }

    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;

    AttachmentRepository::count_by_note_id(&conn, trimmed_note_id)
        .map_err(|_| "Failed to count attachments for note.".to_string())
}

/// Inspects attachment consistency and detects orphaned or missing files (Phase 7, Task 21).
#[tauri::command]
pub fn check_attachment_consistency(
    db: State<'_, Database>,
    paths: State<'_, StoragePaths>,
) -> Result<AttachmentConsistencyReport, String> {
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;
    let storage = AttachmentStorage::new((*paths).clone());

    AttachmentService::check_consistency(&conn, &storage)
        .map_err(|e| e.user_friendly_message())
}

/// Cleans up abandoned temporary staging files left behind by interrupted operations (Phase 7, Task 21).
#[tauri::command]
pub fn cleanup_abandoned_temp_files(
    paths: State<'_, StoragePaths>,
    older_than_seconds: Option<u64>,
) -> Result<usize, String> {
    let storage = AttachmentStorage::new((*paths).clone());
    let age = older_than_seconds.unwrap_or(60);

    AttachmentService::cleanup_abandoned_temp_files(&storage, age)
        .map_err(|e| e.user_friendly_message())
}

/// Resolves an internal link target by its stable note ID (Phase 9, Task 9).
#[tauri::command]
pub fn resolve_link_target(
    db: State<'_, Database>,
    target_id: String,
) -> Result<ResolvedLinkTarget, String> {
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;

    InternalLinkService::resolve_link_target(&conn, &target_id)
        .map_err(|e| e.user_friendly_message())
}

/// Discovers incoming links / backlinks pointing to the target note ID (Phase 9, Task 14).
#[tauri::command]
pub fn get_backlinks(
    db: State<'_, Database>,
    target_note_id: String,
) -> Result<Vec<BacklinkItem>, String> {
    let conn = db
        .conn
        .lock()
        .map_err(|_| "Database connection is unavailable.".to_string())?;

    InternalLinkService::get_backlinks(&conn, &target_note_id)
        .map_err(|e| e.user_friendly_message())
}

