use rusqlite::Connection;
use crate::storage::attachment_storage::AttachmentStorage;
use crate::storage::errors::TrashError;
use crate::storage::models::{EmptyTrashResult, Note, PermanentDeleteResult};
use crate::storage::repositories::NoteRepository;

/// High-level domain service that coordinates note lifecycle operations (Phase 8: Trash & Recovery)
pub struct TrashService;

impl TrashService {
    /// Moves an active note into Trash (Phase 8, Task 5: Soft-Delete Service).
    ///
    /// Pipeline & Invariants:
    /// 1. Validates note ID is non-empty.
    /// 2. Verifies note exists in database.
    /// 3. Verifies note is currently active (rejects repeated soft deletion with InvalidState).
    /// 4. Sets is_deleted = true and records ISO RFC3339 deletion timestamp.
    /// 5. Preserves stable note ID, content, creation timestamp, notebooks, tags, and attachments.
    /// 6. Returns typed updated Note.
    pub fn soft_delete(conn: &Connection, note_id: &str) -> Result<Note, TrashError> {
        let trimmed_id = note_id.trim();
        if trimmed_id.is_empty() {
            return Err(TrashError::InvalidRequest("Note ID cannot be empty".to_string()));
        }

        let existing = match NoteRepository::get_by_id(conn, trimmed_id)? {
            Some(n) => n,
            None => {
                return Err(TrashError::NoteNotFound(format!(
                    "Note with id '{trimmed_id}' not found"
                )))
            }
        };

        if existing.is_deleted {
            return Err(TrashError::InvalidState(format!(
                "Note with id '{trimmed_id}' is already in Trash"
            )));
        }

        let updated = NoteRepository::soft_delete(conn, trimmed_id)?;
        Ok(updated)
    }

    /// Restores a trashed note back to active state (Phase 8, Task 6: Restore Service).
    ///
    /// Pipeline & Invariants:
    /// 1. Validates note ID is non-empty.
    /// 2. Confirms note exists.
    /// 3. Confirms note is currently trashed (rejects active notes with InvalidState).
    /// 4. Handles deleted notebook relationships defensively (unfiles if parent notebook was removed).
    /// 5. Clears deletion timestamp and sets is_deleted = false.
    /// 6. Preserves stable note ID, content, creation timestamp, tags, favorites, and attachments.
    /// 7. Returns typed restored Note.
    pub fn restore(conn: &Connection, note_id: &str) -> Result<Note, TrashError> {
        let trimmed_id = note_id.trim();
        if trimmed_id.is_empty() {
            return Err(TrashError::InvalidRequest("Note ID cannot be empty".to_string()));
        }

        let existing = match NoteRepository::get_by_id(conn, trimmed_id)? {
            Some(n) => n,
            None => {
                return Err(TrashError::NoteNotFound(format!(
                    "Note with id '{trimmed_id}' not found"
                )))
            }
        };

        if !existing.is_deleted {
            return Err(TrashError::InvalidState(format!(
                "Note with id '{trimmed_id}' is not in Trash"
            )));
        }

        let restored = NoteRepository::restore(conn, trimmed_id)?;
        Ok(restored)
    }

    /// Permanently removes a note, its cascade associations, and managed attachment files (Phase 8, Task 7: Safe Permanent Deletion).
    ///
    /// Validations and Invariants:
    /// 1. Validates note ID (rejects empty or whitespace IDs with TrashError::InvalidRequest).
    /// 2. Verifies note exists in database (rejects with TrashError::NoteNotFound).
    /// 3. Confirms that note is currently in Trash (is_deleted == true). Rejects active notes with
    ///    TrashError::InvalidState to guarantee normal delete cannot accidentally bypass Trash.
    /// 4. Coordinates attachment-file cleanup via `AttachmentStorage::remove_note_directory`
    ///    using the trusted storage abstraction (prevents path traversal and constructing paths from user strings).
    /// 5. Removes note and related database rows (cascading note_tags, attachments metadata, FTS rows).
    /// 6. Preserves unrelated notes, tags, notebooks, and attachments.
    /// 7. Reports filesystem cleanup failures honestly via typed `PermanentDeleteResult`.
    pub fn permanently_delete_note(
        conn: &Connection,
        storage: &AttachmentStorage,
        note_id: &str,
    ) -> Result<PermanentDeleteResult, TrashError> {
        let trimmed_id = note_id.trim();
        if trimmed_id.is_empty() {
            return Err(TrashError::InvalidRequest("Note ID cannot be empty".to_string()));
        }

        let existing = match NoteRepository::get_by_id(conn, trimmed_id)? {
            Some(n) => n,
            None => {
                return Err(TrashError::NoteNotFound(format!(
                    "Note with id '{trimmed_id}' not found"
                )))
            }
        };

        if !existing.is_deleted {
            return Err(TrashError::InvalidState(format!(
                "Note with id '{trimmed_id}' is active. Only trashed notes can be permanently deleted."
            )));
        }

        // Identify associated attachment metadata and coordinate file cleanup
        let (attachment_cleanup_failed, cleanup_err) = match storage.remove_note_directory(trimmed_id) {
            Ok(()) => (false, None),
            Err(e) => (true, Some(e.user_friendly_message())),
        };

        // Remove note-related records from database (cascades note_tags, attachments, FTS triggers)
        let deleted = NoteRepository::delete(conn, trimmed_id, false)?;
        if !deleted {
            return Err(TrashError::NoteNotFound(format!(
                "Note with id '{trimmed_id}' was not found during final deletion"
            )));
        }

        Ok(PermanentDeleteResult {
            note_id: trimmed_id.to_string(),
            success: true,
            attachment_cleanup_failed,
            error_message: cleanup_err,
        })
    }

    /// Permanently removes all notes currently in Trash (Phase 8, Task 8: Empty-Trash Service).
    ///
    /// Validations & Invariants:
    /// 1. Finds all notes currently confirmed in Trash (`is_deleted = true`).
    /// 2. If Trash is empty, immediately returns a zero-count successful result (safe no-op).
    /// 3. Never deletes active notes.
    /// 4. Processes each trashed note sequentially without holding an unnecessarily long transaction
    ///    over filesystem I/O operations.
    /// 5. For each note, coordinates attachment file cleanup via trusted storage abstraction and cascades database rows.
    /// 6. Truthfully tracks and reports:
    ///    - deleted_count (notes whose DB records were permanently removed)
    ///    - failed_count (notes that failed DB removal or encountered unexpected errors)
    ///    - attachment_cleanup_failures (note IDs where filesystem cleanup had errors)
    ///    - errors (descriptive messages of any encountered errors)
    /// 7. Returns typed EmptyTrashResult.
    pub fn empty_trash(
        conn: &Connection,
        storage: &AttachmentStorage,
    ) -> Result<EmptyTrashResult, TrashError> {
        let trashed_notes = NoteRepository::list_trashed(conn)?;
        if trashed_notes.is_empty() {
            return Ok(EmptyTrashResult {
                deleted_count: 0,
                failed_count: 0,
                attachment_cleanup_failures: Vec::new(),
                errors: Vec::new(),
            });
        }

        let mut deleted_count = 0;
        let mut failed_count = 0;
        let mut attachment_cleanup_failures = Vec::new();
        let mut errors = Vec::new();

        for note in trashed_notes {
            match Self::permanently_delete_note(conn, storage, &note.id) {
                Ok(perm_res) => {
                    deleted_count += 1;
                    if perm_res.attachment_cleanup_failed {
                        attachment_cleanup_failures.push(note.id.clone());
                        if let Some(msg) = perm_res.error_message {
                            errors.push(format!("Note {}: attachment cleanup issue: {}", note.id, msg));
                        }
                    }
                }
                Err(err) => {
                    failed_count += 1;
                    errors.push(format!("Failed to delete note {}: {}", note.id, err.user_friendly_message()));
                }
            }
        }

        Ok(EmptyTrashResult {
            deleted_count,
            failed_count,
            attachment_cleanup_failures,
            errors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        attachment_storage::AttachmentStorage,
        database::init_connection,
        migrations::run_migrations,
        models::{CreateNotebookDto, CreateNoteDto, NoteLifecycle},
        paths::StoragePaths,
        repositories::{AttachmentRepository, NotebookRepository, TagRepository},
        services::AttachmentService,
    };

    #[test]
    fn test_trash_service_soft_delete_lifecycle_and_validation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_trash_service_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Validation: Reject empty or whitespace note ID
        let empty_err = TrashService::soft_delete(&conn, "").unwrap_err();
        assert!(matches!(empty_err, TrashError::InvalidRequest(_)));

        let ws_err = TrashService::soft_delete(&conn, "   ").unwrap_err();
        assert!(matches!(ws_err, TrashError::InvalidRequest(_)));

        // 2. Validation: Reject nonexistent note
        let not_found_err = TrashService::soft_delete(&conn, "non-existent-id").unwrap_err();
        assert!(matches!(not_found_err, TrashError::NoteNotFound(_)));

        // 3. Setup active note with notebook, tag, favorite, and content
        let nb = NotebookRepository::create(&conn, CreateNotebookDto {
            name: "Work Notebook".to_string(),
            parent_id: None,
        }).unwrap();

        let tag = TagRepository::create(&conn, "urgent").unwrap();

        let note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Important Project Specs".to_string()),
            content: Some("Critical architectural notes".to_string()),
            format: Some("md".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

        assert_eq!(note.lifecycle(), NoteLifecycle::Active);
        assert!(!note.is_deleted);
        assert_eq!(note.deleted_at, None);

        // 4. Execute soft delete via TrashService
        let trashed = TrashService::soft_delete(&conn, &note.id).unwrap();

        // 5. Verify all invariants:
        // - Stable ID preserved
        assert_eq!(trashed.id, note.id);
        // - Title and content preserved intact
        assert_eq!(trashed.title, "Important Project Specs");
        assert_eq!(trashed.content, "Critical architectural notes");
        assert_eq!(trashed.format, "md");
        // - Notebook relationship preserved
        assert_eq!(trashed.notebook_id, Some(nb.id.clone()));
        // - Creation timestamp preserved
        assert_eq!(trashed.created_at, note.created_at);
        // - Favorite state preserved
        assert!(trashed.is_favorite);
        // - Lifecycle is Trashed and deletion timestamp recorded
        assert!(trashed.is_deleted);
        assert!(trashed.deleted_at.is_some());
        assert_eq!(trashed.lifecycle(), NoteLifecycle::Trashed);

        // - Tag relationship preserved
        let tags_after = TagRepository::get_tags_for_note(&conn, &note.id).unwrap();
        assert_eq!(tags_after.len(), 1);
        assert_eq!(tags_after[0].id, tag.id);

        // - Active lists exclude the note
        let active_notes = NoteRepository::list_active(&conn).unwrap();
        assert_eq!(active_notes.len(), 0);

        // - Trash list includes the note
        let trashed_notes = NoteRepository::list_trashed(&conn).unwrap();
        assert_eq!(trashed_notes.len(), 1);
        assert_eq!(trashed_notes[0].id, note.id);

        // 6. Repeated deletion: Must return clearly documented InvalidState error
        let repeated_err = TrashService::soft_delete(&conn, &note.id).unwrap_err();
        match repeated_err {
            TrashError::InvalidState(msg) => {
                assert!(msg.contains("already in Trash"));
            }
            other => panic!("Expected InvalidState for already trashed note, got: {other:?}"),
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_trash_service_restore_lifecycle_and_notebook_handling() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_trash_restore_{}", uuid::Uuid::new_v4()));
        let db_path = temp_dir.join("database.sqlite");
        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Validation: Reject empty or whitespace note ID
        let empty_err = TrashService::restore(&conn, "").unwrap_err();
        assert!(matches!(empty_err, TrashError::InvalidRequest(_)));

        // 2. Validation: Reject nonexistent note ID
        let not_found_err = TrashService::restore(&conn, "non-existent-id").unwrap_err();
        assert!(matches!(not_found_err, TrashError::NoteNotFound(_)));

        // 3. Create active note with notebook, tag, favorite
        let nb = NotebookRepository::create(&conn, CreateNotebookDto {
            name: "Research Notebook".to_string(),
            parent_id: None,
        }).unwrap();

        let tag = TagRepository::create(&conn, "active-tag").unwrap();

        let note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Quantum Algorithms".to_string()),
            content: Some("Notes on Grover search and Shor algorithm".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

        // 4. Validation: Cannot restore an already active note
        let already_active_err = TrashService::restore(&conn, &note.id).unwrap_err();
        assert!(matches!(already_active_err, TrashError::InvalidState(_)));

        // 5. Soft-delete the note into Trash
        let _trashed = TrashService::soft_delete(&conn, &note.id).unwrap();
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 0);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 1);

        // 6. Restore the note back to active state
        let restored = TrashService::restore(&conn, &note.id).unwrap();

        // 7. Verify all restore invariants:
        // - Retains exact same note ID
        assert_eq!(restored.id, note.id);
        // - Content, title, format, creation timestamp preserved
        assert_eq!(restored.title, "Quantum Algorithms");
        assert_eq!(restored.content, "Notes on Grover search and Shor algorithm");
        assert_eq!(restored.format, "txt");
        assert_eq!(restored.created_at, note.created_at);
        // - Lifecycle is Active, deleted_at cleared
        assert!(!restored.is_deleted);
        assert_eq!(restored.deleted_at, None);
        assert_eq!(restored.lifecycle(), NoteLifecycle::Active);
        // - Existing notebook relationship preserved since notebook still exists
        assert_eq!(restored.notebook_id, Some(nb.id.clone()));
        // - Tags and favorite preserved
        assert!(restored.is_favorite);
        let tags_after_restore = TagRepository::get_tags_for_note(&conn, &note.id).unwrap();
        assert_eq!(tags_after_restore.len(), 1);
        assert_eq!(tags_after_restore[0].id, tag.id);

        // - Query counts correctly reflect 1 active and 0 trashed
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 1);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 0);

        // 8. Test notebook handling when original notebook was deleted while note was in Trash:
        // Soft delete the note again
        let _trashed_again = TrashService::soft_delete(&conn, &note.id).unwrap();

        // Delete the notebook
        NotebookRepository::delete(&mut conn, &nb.id).unwrap();

        // Restore the note: Should defensively unfile note (notebook_id = None) instead of failing
        let restored_unfiled = TrashService::restore(&conn, &note.id).unwrap();
        assert_eq!(restored_unfiled.id, note.id);
        assert!(!restored_unfiled.is_deleted);
        assert_eq!(restored_unfiled.notebook_id, None, "Note must be safely unfiled if notebook was deleted");

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_trash_service_permanent_delete_lifecycle_and_validation() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_trash_perm_del_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let db_path = paths.database_file.clone();
        let attachments_dir = paths.attachments_dir.clone();
        let storage = AttachmentStorage::new(paths);

        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Validation: Reject empty or whitespace note ID
        let empty_err = TrashService::permanently_delete_note(&conn, &storage, "").unwrap_err();
        assert!(matches!(empty_err, TrashError::InvalidRequest(_)));

        let ws_err = TrashService::permanently_delete_note(&conn, &storage, "   ").unwrap_err();
        assert!(matches!(ws_err, TrashError::InvalidRequest(_)));

        // 2. Validation: Reject nonexistent note
        let not_found_err = TrashService::permanently_delete_note(&conn, &storage, "non-existent-id").unwrap_err();
        assert!(matches!(not_found_err, TrashError::NoteNotFound(_)));

        // 3. Create active note with notebook, tag, favorite, and an attachment
        let nb = NotebookRepository::create(&conn, CreateNotebookDto {
            name: "Permanent Delete Notebook".to_string(),
            parent_id: None,
        }).unwrap();

        let tag = TagRepository::create(&conn, "perm-tag").unwrap();

        let note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Note to be destroyed".to_string()),
            content: Some("Sensitive data to be permanently erased".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            is_pinned: Some(false),
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

        // Add attachment file
        let src_file = temp_dir.join("sample.pdf");
        std::fs::write(&src_file, b"%PDF-1.4 sample attachment content").unwrap();
        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        let note_att_dir = attachments_dir.join(&note.id);
        assert!(note_att_dir.exists(), "Attachment directory must exist on disk");
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(), 1);

        // 4. Validation: Cannot permanently delete an active note (must be in Trash first)
        let active_err = TrashService::permanently_delete_note(&conn, &storage, &note.id).unwrap_err();
        assert!(matches!(active_err, TrashError::InvalidState(_)));

        // 5. Create an unrelated note to verify strict isolation
        let other_note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Survivor Note".to_string()),
            content: Some("Must not be touched".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &other_note.id, &tag.id).unwrap();

        // 6. Soft delete target note into Trash
        TrashService::soft_delete(&conn, &note.id).unwrap();
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 1);
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 1);

        // 7. Execute permanent deletion
        let perm_res = TrashService::permanently_delete_note(&conn, &storage, &note.id).unwrap();
        assert_eq!(perm_res.note_id, note.id);
        assert!(perm_res.success);
        assert!(!perm_res.attachment_cleanup_failed);
        assert_eq!(perm_res.error_message, None);

        // 8. Verify target note is completely removed from DB
        assert_eq!(NoteRepository::get_by_id(&conn, &note.id).unwrap(), None);
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &note.id).unwrap(), 0);
        assert_eq!(AttachmentRepository::get_by_id(&conn, &att.id).unwrap(), None);
        assert!(TagRepository::get_tags_for_note(&conn, &note.id).unwrap().is_empty());

        // 9. Verify physical attachment directory is wiped
        assert!(!note_att_dir.exists(), "Note attachment folder must be wiped from disk");

        // 10. Verify unrelated data is preserved (shared entities and survivor note)
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 1);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 0);
        let survivor = NoteRepository::get_by_id(&conn, &other_note.id).unwrap().unwrap();
        assert_eq!(survivor.title, "Survivor Note");

        // Notebook and tag records remain 100% intact
        let nb_check = NotebookRepository::get_by_id(&conn, &nb.id).unwrap().unwrap();
        assert_eq!(nb_check.name, "Permanent Delete Notebook");
        let tags_check = TagRepository::get_tags_for_note(&conn, &other_note.id).unwrap();
        assert_eq!(tags_check.len(), 1);
        assert_eq!(tags_check[0].id, tag.id);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_trash_service_permanent_delete_reports_attachment_cleanup_failure_honestly() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_trash_perm_fail_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let db_path = paths.database_file.clone();
        let attachments_dir = paths.attachments_dir.clone();
        let storage = AttachmentStorage::new(paths);

        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        let note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Locked Attachment Note".to_string()),
            content: Some("Test partial cleanup reporting".to_string()),
            ..Default::default()
        }).unwrap();

        let src_file = temp_dir.join("locked.txt");
        std::fs::write(&src_file, b"sample file").unwrap();
        AttachmentService::add_attachment(&conn, &storage, &note.id, &src_file).unwrap();

        let note_att_dir = attachments_dir.join(&note.id);
        assert!(note_att_dir.exists());

        TrashService::soft_delete(&conn, &note.id).unwrap();

        // Make note attachment directory read-only so remove_dir_all fails with permission denied
        let _ = std::fs::set_permissions(&note_att_dir, std::fs::Permissions::from_mode(0o555));

        let res = TrashService::permanently_delete_note(&conn, &storage, &note.id).unwrap();
        assert_eq!(res.note_id, note.id);
        assert!(res.success, "Database deletion still succeeds");
        assert!(res.attachment_cleanup_failed, "Must report attachment cleanup failure honestly");
        assert!(res.error_message.is_some(), "Must provide detailed error message");

        // Restore permissions for cleanup
        let _ = std::fs::set_permissions(&note_att_dir, std::fs::Permissions::from_mode(0o755));
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_trash_service_empty_trash_lifecycle_and_invariants() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_empty_trash_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let db_path = paths.database_file.clone();
        let attachments_dir = paths.attachments_dir.clone();
        let storage = AttachmentStorage::new(paths);

        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Empty Trash when already empty: must succeed as safe zero-count no-op
        let empty_res = TrashService::empty_trash(&conn, &storage).unwrap();
        assert_eq!(empty_res.deleted_count, 0);
        assert_eq!(empty_res.failed_count, 0);
        assert!(empty_res.attachment_cleanup_failures.is_empty());
        assert!(empty_res.errors.is_empty());

        // 2. Setup: 2 active survivor notes (one with notebook, tag, and attachment)
        let nb = NotebookRepository::create(&conn, CreateNotebookDto {
            name: "Survivor Notebook".to_string(),
            parent_id: None,
        }).unwrap();
        let tag = TagRepository::create(&conn, "survivor-tag").unwrap();

        let survivor1 = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Active Survivor 1".to_string()),
            content: Some("I will never be deleted by empty trash".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &survivor1.id, &tag.id).unwrap();

        let src_file1 = temp_dir.join("survivor_att.txt");
        std::fs::write(&src_file1, b"survivor attachment").unwrap();
        let survivor_att = AttachmentService::add_attachment(&conn, &storage, &survivor1.id, &src_file1).unwrap();
        let survivor_att_path = attachments_dir.join(&survivor_att.relative_path);
        assert!(survivor_att_path.exists());

        let survivor2 = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Active Survivor 2".to_string()),
            content: Some("I am also active".to_string()),
            ..Default::default()
        }).unwrap();

        // 3. Setup: 3 notes doomed to be in Trash
        let doomed1 = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Doomed Note 1".to_string()),
            content: Some("Content 1".to_string()),
            ..Default::default()
        }).unwrap();
        let src_file2 = temp_dir.join("doomed1.txt");
        std::fs::write(&src_file2, b"doomed attachment 1").unwrap();
        AttachmentService::add_attachment(&conn, &storage, &doomed1.id, &src_file2).unwrap();
        let doomed1_att_dir = attachments_dir.join(&doomed1.id);
        assert!(doomed1_att_dir.exists());

        let doomed2 = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Doomed Note 2".to_string()),
            content: Some("Content 2".to_string()),
            ..Default::default()
        }).unwrap();
        let src_file3 = temp_dir.join("doomed2.txt");
        std::fs::write(&src_file3, b"doomed attachment 2").unwrap();
        AttachmentService::add_attachment(&conn, &storage, &doomed2.id, &src_file3).unwrap();
        let doomed2_att_dir = attachments_dir.join(&doomed2.id);
        assert!(doomed2_att_dir.exists());

        let doomed3 = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Doomed Note 3 without attachments".to_string()),
            content: Some("Content 3".to_string()),
            ..Default::default()
        }).unwrap();

        // Soft-delete the 3 doomed notes
        TrashService::soft_delete(&conn, &doomed1.id).unwrap();
        TrashService::soft_delete(&conn, &doomed2.id).unwrap();
        TrashService::soft_delete(&conn, &doomed3.id).unwrap();

        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 2);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 3);

        // 4. Execute empty_trash
        let res = TrashService::empty_trash(&conn, &storage).unwrap();
        assert_eq!(res.deleted_count, 3, "All 3 trashed notes must be permanently deleted");
        assert_eq!(res.failed_count, 0);
        assert!(res.attachment_cleanup_failures.is_empty());
        assert!(res.errors.is_empty());

        // 5. Verify database state
        assert_eq!(NoteRepository::count_active(&conn).unwrap(), 2);
        assert_eq!(NoteRepository::count_trashed(&conn).unwrap(), 0);

        // Doomed notes are gone
        assert_eq!(NoteRepository::get_by_id(&conn, &doomed1.id).unwrap(), None);
        assert_eq!(NoteRepository::get_by_id(&conn, &doomed2.id).unwrap(), None);
        assert_eq!(NoteRepository::get_by_id(&conn, &doomed3.id).unwrap(), None);

        // Doomed attachment folders are wiped from disk
        assert!(!doomed1_att_dir.exists());
        assert!(!doomed2_att_dir.exists());

        // 6. Verify survivor active notes are completely untouched
        let s1_check = NoteRepository::get_by_id(&conn, &survivor1.id).unwrap().unwrap();
        assert_eq!(s1_check.title, "Active Survivor 1");
        assert!(!s1_check.is_deleted);

        let s2_check = NoteRepository::get_by_id(&conn, &survivor2.id).unwrap().unwrap();
        assert_eq!(s2_check.title, "Active Survivor 2");
        assert!(!s2_check.is_deleted);

        // Survivor attachment remains intact on disk
        assert!(survivor_att_path.exists());
        assert_eq!(AttachmentRepository::count_by_note_id(&conn, &survivor1.id).unwrap(), 1);

        // Notebook and tag remain intact
        assert!(NotebookRepository::get_by_id(&conn, &nb.id).unwrap().is_some());
        assert_eq!(TagRepository::get_tags_for_note(&conn, &survivor1.id).unwrap().len(), 1);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_trash_service_empty_trash_handles_partial_failures() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = std::env::temp_dir().join(format!("pn_test_empty_trash_fail_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let db_path = paths.database_file.clone();
        let attachments_dir = paths.attachments_dir.clone();
        let storage = AttachmentStorage::new(paths);

        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // Create 2 notes with attachments
        let note_normal = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Normal Trashed".to_string()),
            ..Default::default()
        }).unwrap();
        let f1 = temp_dir.join("f1.txt");
        std::fs::write(&f1, b"file 1").unwrap();
        AttachmentService::add_attachment(&conn, &storage, &note_normal.id, &f1).unwrap();

        let note_locked = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Locked Trashed".to_string()),
            ..Default::default()
        }).unwrap();
        let f2 = temp_dir.join("f2.txt");
        std::fs::write(&f2, b"file 2").unwrap();
        AttachmentService::add_attachment(&conn, &storage, &note_locked.id, &f2).unwrap();

        TrashService::soft_delete(&conn, &note_normal.id).unwrap();
        TrashService::soft_delete(&conn, &note_locked.id).unwrap();

        // Lock note_locked's attachment folder
        let locked_dir = attachments_dir.join(&note_locked.id);
        let _ = std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o555));

        let res = TrashService::empty_trash(&conn, &storage).unwrap();
        assert_eq!(res.deleted_count, 2, "Both database records permanently removed");
        assert_eq!(res.failed_count, 0);
        assert_eq!(res.attachment_cleanup_failures.len(), 1);
        assert_eq!(res.attachment_cleanup_failures[0], note_locked.id);
        assert_eq!(res.errors.len(), 1);

        // Restore permissions for cleanup
        let _ = std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755));
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_22_database_integrity_and_lifecycle_invariants() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_task_22_integrity_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let db_path = paths.database_file.clone();
        let storage = AttachmentStorage::new(paths);

        let mut conn = init_connection(&db_path).expect("Init failed");
        run_migrations(&mut conn).expect("Migrations failed");

        // 1. Foreign-key pragma verification
        let foreign_keys_enabled: bool = conn
            .query_row("PRAGMA foreign_keys;", [], |r| r.get(0))
            .expect("PRAGMA foreign_keys query failed");
        assert!(foreign_keys_enabled, "Foreign keys must be actively enforced");

        // 2. Setup notebook and tag
        let nb = NotebookRepository::create(&conn, CreateNotebookDto {
            name: "Integrity Notebook".to_string(),
            parent_id: None,
        }).unwrap();
        let tag = TagRepository::create(&conn, "integrity-tag").unwrap();

        // 3. Test note with unusual / SQL injection-like content and title
        let note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Robert'); DROP TABLE notes;-- 🚀".to_string()),
            content: Some("SELECT * FROM attachments WHERE '1'='1';".to_string()),
            notebook_id: Some(nb.id.clone()),
            is_favorite: Some(true),
            ..Default::default()
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

        // Add attachment
        let sample_file = temp_dir.join("payload.txt");
        std::fs::write(&sample_file, b"injection payload test").unwrap();
        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &sample_file).unwrap();

        // Verify injection strings did not corrupt database schema or tables
        let notes_table_exists: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name='notes';",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(notes_table_exists, "Notes table must remain intact");

        // 4. Soft-delete and restore with unusual identifiers
        let trashed = TrashService::soft_delete(&conn, &note.id).unwrap();
        assert!(trashed.is_deleted);
        assert_eq!(trashed.title, "Robert'); DROP TABLE notes;-- 🚀");

        let restored = TrashService::restore(&conn, &note.id).unwrap();
        assert!(!restored.is_deleted);
        assert_eq!(restored.title, "Robert'); DROP TABLE notes;-- 🚀");

        // 5. Foreign-key cascading verification on permanent deletion
        TrashService::soft_delete(&conn, &note.id).unwrap();
        let perm_del = TrashService::permanently_delete_note(&conn, &storage, &note.id).unwrap();
        assert!(perm_del.success);

        // Verify note record removed
        assert!(NoteRepository::get_by_id(&conn, &note.id).unwrap().is_none());
        // Verify note_tags assignment cascaded
        assert_eq!(TagRepository::get_tags_for_note(&conn, &note.id).unwrap().len(), 0);
        // Verify attachment record cascaded
        assert!(AttachmentRepository::get_by_id(&conn, &att.id).unwrap().is_none());
        // Verify parent notebook and tag records remain completely intact
        assert!(NotebookRepository::get_by_id(&conn, &nb.id).unwrap().is_some());
        assert!(TagRepository::get_by_id(&conn, &tag.id).unwrap().is_some());

        // 6. Migration idempotency: running migrations on existing database preserves data
        let survivor = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Survivor Note".to_string()),
            ..Default::default()
        }).unwrap();
        run_migrations(&mut conn).expect("Re-running migrations must succeed idempotently");
        assert!(NoteRepository::get_by_id(&conn, &survivor.id).unwrap().is_some());

        // 7. Repeated empty-trash calls: multiple consecutive calls must be safe and idempotent
        let empty_1 = TrashService::empty_trash(&conn, &storage).unwrap();
        assert_eq!(empty_1.deleted_count, 0);
        let empty_2 = TrashService::empty_trash(&conn, &storage).unwrap();
        assert_eq!(empty_2.deleted_count, 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_task_24_data_safety_security_and_reliability_verification() {
        let temp_dir = std::env::temp_dir().join(format!("pn_test_t24_verification_{}", uuid::Uuid::new_v4()));
        let paths = StoragePaths::from_root(temp_dir.clone());
        paths.ensure_directories().unwrap();
        let db_path = paths.database_file.clone();
        let attachments_dir = paths.attachments_dir.clone();
        let storage = AttachmentStorage::new(paths.clone());

        // -------------------------------------------------------------
        // 1. RESTART RELIABILITY: RESTORE & PERMANENT DELETE ACROSS RESTARTS
        // -------------------------------------------------------------
        let mut conn = init_connection(&db_path).expect("Initial connection failed");
        run_migrations(&mut conn).expect("Initial migrations failed");

        let nb = NotebookRepository::create(&conn, CreateNotebookDto {
            name: "Restart Notebook".to_string(),
            parent_id: None,
        }).unwrap();
        let tag = TagRepository::create(&conn, "restart-tag").unwrap();

        let note = NoteRepository::create(&conn, CreateNoteDto {
            title: Some("Restart Note".to_string()),
            content: Some("Restart content body".to_string()),
            notebook_id: Some(nb.id.clone()),
            ..Default::default()
        }).unwrap();
        TagRepository::add_tag_to_note(&conn, &note.id, &tag.id).unwrap();

        let att_src = temp_dir.join("restart_file.txt");
        std::fs::write(&att_src, b"bytes across restart").unwrap();
        let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &att_src).unwrap();

        // Soft-delete the note
        TrashService::soft_delete(&conn, &note.id).unwrap();

        // Simulate app shutdown: drop connection
        drop(conn);

        // Simulate app restart: re-initialize connection & rerun migrations
        let mut conn2 = init_connection(&db_path).expect("Restart connection failed");
        run_migrations(&mut conn2).expect("Restart migrations failed");

        // Restore after restart
        let restored = TrashService::restore(&conn2, &note.id).unwrap();
        assert!(!restored.is_deleted);
        assert_eq!(restored.title, "Restart Note");
        assert_eq!(restored.notebook_id, Some(nb.id.clone()));
        assert!(storage.file_exists(&att.relative_path));

        // Soft-delete again and simulate another restart before permanent delete
        TrashService::soft_delete(&conn2, &note.id).unwrap();
        drop(conn2);

        let mut conn3 = init_connection(&db_path).expect("Second restart connection failed");
        run_migrations(&mut conn3).expect("Second restart migrations failed");

        // Permanent deletion after restart
        let perm_res = TrashService::permanently_delete_note(&conn3, &storage, &note.id).unwrap();
        assert!(perm_res.success);
        assert!(!storage.file_exists(&att.relative_path));
        assert!(NoteRepository::get_by_id(&conn3, &note.id).unwrap().is_none());

        // -------------------------------------------------------------
        // 2. MISSING ATTACHMENT RESILIENCE DURING PERMANENT DELETION
        // -------------------------------------------------------------
        let ghost_note = NoteRepository::create(&conn3, CreateNoteDto {
            title: Some("Ghost Attachment Note".to_string()),
            ..Default::default()
        }).unwrap();
        let ghost_src = temp_dir.join("ghost.txt");
        std::fs::write(&ghost_src, b"ghost payload").unwrap();
        let _ = AttachmentService::add_attachment(&conn3, &storage, &ghost_note.id, &ghost_src).unwrap();
        TrashService::soft_delete(&conn3, &ghost_note.id).unwrap();

        // Delete disk directory externally before permanent deletion
        let ghost_dir = attachments_dir.join(&ghost_note.id);
        if ghost_dir.exists() {
            let _ = std::fs::remove_dir_all(&ghost_dir);
        }

        // Permanent deletion must succeed cleanly and purge DB rows without error
        let ghost_del = TrashService::permanently_delete_note(&conn3, &storage, &ghost_note.id).unwrap();
        assert!(ghost_del.success);
        assert!(!ghost_del.attachment_cleanup_failed);
        assert!(NoteRepository::get_by_id(&conn3, &ghost_note.id).unwrap().is_none());

        // -------------------------------------------------------------
        // 3. BACKEND BOUNDARY SECURITY: REJECT ACTIVE NOTE DELETIONS
        // -------------------------------------------------------------
        let active_note = NoteRepository::create(&conn3, CreateNoteDto {
            title: Some("Active Boundary Note".to_string()),
            ..Default::default()
        }).unwrap();

        // Reject permanently deleting active notes
        assert!(TrashService::permanently_delete_note(&conn3, &storage, &active_note.id).is_err());
        // Reject restoring active notes
        assert!(TrashService::restore(&conn3, &active_note.id).is_err());

        // -------------------------------------------------------------
        // 4. LARGE TRASH LIST EMPTYING & ACTIVE SURVIVOR PRESERVATION
        // -------------------------------------------------------------
        for i in 0..25 {
            let n = NoteRepository::create(&conn3, CreateNoteDto {
                title: Some(format!("Bulk Trashed Note {}", i)),
                ..Default::default()
            }).unwrap();
            TrashService::soft_delete(&conn3, &n.id).unwrap();
        }

        let empty_batch = TrashService::empty_trash(&conn3, &storage).unwrap();
        assert_eq!(empty_batch.deleted_count, 25);
        assert_eq!(empty_batch.failed_count, 0);

        // Active note must remain untouched
        let active_survivor = NoteRepository::get_by_id(&conn3, &active_note.id).unwrap().unwrap();
        assert!(!active_survivor.is_deleted);
        assert_eq!(active_survivor.title, "Active Boundary Note");

        drop(conn3);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
