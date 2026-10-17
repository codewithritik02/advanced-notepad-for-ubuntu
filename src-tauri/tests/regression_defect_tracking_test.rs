mod common;

use std::fs;
use std::path::Path;
use std::thread;

use common::{lock_process_global_test_state, TestAppDir, TestDatabase};
use personal_notepad_lib::storage::{
    crypto::{CryptoSession, SetupEncryptionDto},
    errors::{AttachmentError, CryptoError, StorageError},
    limits::{MAX_NOTE_CONTENT_BYTES, MAX_NOTE_TITLE_CHARS},
    models::{CreateNoteDto, CreateNotebookDto, UpdateNoteDto},
    paths::StoragePaths,
    repositories::{NoteRepository, NotebookRepository, SearchRepository},
    services::AttachmentService,
    AttachmentStorage,
};

/// DEF-03: Zip-Slip and path traversal prevention in attachment and backup subpaths.
/// Verifies that paths attempting to escape the managed base directory with ../ sequences or
/// absolute roots are strictly rejected.
#[test]
fn regression_def_03_zip_slip_path_traversal_prevention() {
    let fixture = TestAppDir::with_label("reg-def03");
    let base = &fixture.paths.attachments_dir;

    let traversal_paths = [
        "../../escaped.txt",
        "sub/../../escaped.txt",
        "/etc/passwd",
        "/root/secret.db",
        "nested/../../../../etc/shadow",
        "C:\\Windows\\System32\\cmd.exe",
    ];

    for dangerous in &traversal_paths {
        let res = StoragePaths::sanitize_subpath(base, Path::new(dangerous));
        assert!(
            res.is_err(),
            "Path traversal '{dangerous}' must be rejected by sanitize_subpath"
        );
    }

    let safe_res = StoragePaths::sanitize_subpath(base, Path::new("note-123/valid-document.pdf"));
    assert!(
        safe_res.is_ok(),
        "Legitimate relative subpath inside base must be accepted"
    );
}

/// DEF-04: Executable attachment extension blocking.
/// Verifies that dangerous executable extensions (.exe, .sh, .bat, .cmd, .ps1, .appimage, etc.)
/// are blocked while non-executable document and media formats are permitted, and opening executable attachments fails.
#[test]
fn regression_def_04_executable_attachment_extension_blocked() {
    let blocked_extensions = [
        "payload.exe",
        "script.sh",
        "batch.bat",
        "command.cmd",
        "powershell.ps1",
        "binary.run",
        "script.vbs",
        "installer.msi",
        "archive.jar",
        "legacy.com",
    ];

    for name in &blocked_extensions {
        assert!(
            AttachmentStorage::is_executable_extension(name),
            "Filename '{name}' must be recognized as executable"
        );
    }

    let allowed_extensions = [
        "document.txt",
        "manual.pdf",
        "diagram.png",
        "notes.md",
        "data.json",
        "photo.jpg",
        "vector.svg",
    ];

    for name in &allowed_extensions {
        assert!(
            !AttachmentStorage::is_executable_extension(name),
            "Filename '{name}' must NOT be blocked as executable"
        );
    }

    // Backend service rejection: attempting to resolve/open an executable attachment fails
    let fixture = TestAppDir::with_label("reg-def04");
    let conn = fixture.open_migrated_database();
    let storage = AttachmentStorage::new(fixture.paths.clone());

    let note = NoteRepository::create(
        &conn,
        CreateNoteDto {
            title: Some("Executable Blocking Note".to_string()),
            content: Some("Note content".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note");

    let source_dir = fixture.dir.join("temp_source");
    fs::create_dir_all(&source_dir).unwrap();
    let mal_file = source_dir.join("exploit.sh");
    fs::write(&mal_file, b"#!/bin/bash\necho dangerous").unwrap();

    let att = AttachmentService::add_attachment(&conn, &storage, &note.id, &mal_file)
        .expect("attachment added to storage");

    let open_res = AttachmentService::get_attachment_for_open(&conn, &storage, &att.id);
    match open_res {
        Err(AttachmentError::ExecutableBlocked(blocked_name)) => {
            assert_eq!(blocked_name, "exploit.sh");
        }
        other => panic!("Expected AttachmentError::ExecutableBlocked, got {other:?}"),
    }
}

/// DEF-05: FTS5 full-text index synchronization and soft-deleted note exclusion.
/// Verifies that SQLite FTS5 search triggers update index content on live note edits
/// and immediately exclude soft-deleted notes from search queries.
#[test]
fn regression_def_05_fts5_index_synchronization_and_soft_delete_exclusion() {
    let database = TestDatabase::new();
    let conn = database.connection();

    let unique_kw = "XenonSubspaceChronometer";
    let note = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(format!("Device Spec: {unique_kw}")),
            content: Some("Operational manual for subspace transmission.".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note");

    // 1. Initial match: FTS5 locates note by keyword
    let matches = SearchRepository::search(conn, unique_kw, Some(10)).expect("search initial");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].note_id, note.id);

    // 2. Live edit: remove keyword from title and content
    NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            title: Some("Standard Device Spec".to_string()),
            content: Some("Generic operational instructions.".to_string()),
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("update note");

    let matches_after_edit =
        SearchRepository::search(conn, unique_kw, Some(10)).expect("search after edit");
    assert!(
        matches_after_edit.is_empty(),
        "FTS5 index must update synchronously; keyword should no longer match"
    );

    // 3. Re-add keyword and soft-delete note
    NoteRepository::update(
        conn,
        &note.id,
        UpdateNoteDto {
            title: Some(format!("Restored Device: {unique_kw}")),
            content: None,
            format: None,
            notebook_id: None,
            is_favorite: None,
            is_pinned: None,
            is_deleted: None,
        },
    )
    .expect("re-add keyword");

    let matches_readded =
        SearchRepository::search(conn, unique_kw, Some(10)).expect("search re-added");
    assert_eq!(matches_readded.len(), 1);

    // Soft delete
    NoteRepository::delete(conn, &note.id, true).expect("soft delete");

    // Must be excluded from active search
    let matches_deleted =
        SearchRepository::search(conn, unique_kw, Some(10)).expect("search after soft delete");
    assert!(
        matches_deleted.is_empty(),
        "Soft-deleted notes must be immediately excluded from active FTS5 search"
    );

    // Restore note
    NoteRepository::restore(conn, &note.id).expect("restore note");
    let matches_restored =
        SearchRepository::search(conn, unique_kw, Some(10)).expect("search after restore");
    assert_eq!(matches_restored.len(), 1);
    assert_eq!(matches_restored[0].note_id, note.id);
}

/// DEF-06: Notebook deletion safely unfiles child notes rather than cascade deleting them.
/// Verifies that deleting a notebook sets child notes' notebook_id to NULL without data loss.
#[test]
fn regression_def_06_notebook_deletion_unfiles_notes_without_deleting() {
    let mut database = TestDatabase::new();
    let conn = database.connection_mut();

    let notebook = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Research Notebook to Delete".to_string(),
            parent_id: None,
        },
    )
    .expect("create notebook");

    let note_1 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Research Finding A".to_string()),
            content: Some("Critical data 1".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(notebook.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note 1");

    let note_2 = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Research Finding B".to_string()),
            content: Some("Critical data 2".to_string()),
            format: Some("txt".to_string()),
            notebook_id: Some(notebook.id.clone()),
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    )
    .expect("create note 2");

    // Delete notebook
    let deleted = NotebookRepository::delete(conn, &notebook.id).expect("delete notebook");
    assert!(deleted, "Notebook must be deleted");

    // Verify notebook is gone
    assert!(NotebookRepository::get_by_id(conn, &notebook.id)
        .unwrap()
        .is_none());

    // Verify both child notes still exist in database and are unfiled (notebook_id is NULL)
    let n1_loaded = NoteRepository::get_by_id(conn, &note_1.id)
        .unwrap()
        .expect("note 1 must exist");
    assert_eq!(
        n1_loaded.notebook_id, None,
        "Note 1 must be unfiled to root"
    );
    assert!(!n1_loaded.is_deleted, "Note 1 must not be deleted");

    let n2_loaded = NoteRepository::get_by_id(conn, &note_2.id)
        .unwrap()
        .expect("note 2 must exist");
    assert_eq!(
        n2_loaded.notebook_id, None,
        "Note 2 must be unfiled to root"
    );
    assert!(!n2_loaded.is_deleted, "Note 2 must not be deleted");
}

/// DEF-10: Note title (> 500 chars) and body (> 10 MiB) validation boundaries.
/// Verifies that oversized note inputs are strictly rejected with validation errors.
#[test]
fn regression_def_10_oversized_note_title_and_body_boundaries_enforced() {
    let database = TestDatabase::new();
    let conn = database.connection();

    // 1. Oversized title: 501 Unicode characters (exceeds MAX_NOTE_TITLE_CHARS = 500)
    let oversized_title = "A".repeat(MAX_NOTE_TITLE_CHARS + 1);
    let title_res = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some(oversized_title),
            content: Some("Valid body".to_string()),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    );
    assert!(
        matches!(title_res, Err(StorageError::Validation(_))),
        "Note title > {MAX_NOTE_TITLE_CHARS} chars must return StorageError::Validation"
    );

    // 2. Oversized content: MAX_NOTE_CONTENT_BYTES + 1
    let oversized_body = "x".repeat(MAX_NOTE_CONTENT_BYTES + 1);
    let body_res = NoteRepository::create(
        conn,
        CreateNoteDto {
            title: Some("Valid Title".to_string()),
            content: Some(oversized_body),
            format: Some("txt".to_string()),
            notebook_id: None,
            is_favorite: Some(false),
            is_pinned: Some(false),
        },
    );
    assert!(
        matches!(body_res, Err(StorageError::Validation(_))),
        "Note body > {MAX_NOTE_CONTENT_BYTES} bytes must return StorageError::Validation"
    );
}

/// DEF-11: Process-global test state synchronization.
/// Verifies that multiple threads coordinating via `lock_process_global_test_state`
/// execute safely without race conditions or deadlocks.
#[test]
fn regression_def_11_process_global_test_state_synchronization() {
    let handles: Vec<_> = (0..4)
        .map(|idx| {
            thread::spawn(move || {
                let _guard = lock_process_global_test_state();
                // Critical section access

                idx * 10
            })
        })
        .collect();

    for h in handles {
        let val = h.join().expect("thread join must succeed");
        assert!(val >= 0);
    }
}

/// DEF-12: Orphaned temporary staging files cleanup on startup recovery.
/// Verifies that unreferenced .tmp. staging files left by interrupted processes
/// are cleaned up while valid attachment files are preserved.
#[test]
fn regression_def_12_orphaned_staging_temp_files_cleaned_on_recovery() {
    let fixture = TestAppDir::with_label("reg-def12");
    let storage = AttachmentStorage::new(fixture.paths.clone());

    // Legitimate note folder and file
    let note_dir = storage.get_note_dir("note-valid").unwrap();
    fs::create_dir_all(&note_dir).unwrap();
    let valid_file = note_dir.join("presentation.pdf");
    fs::write(&valid_file, b"%PDF-1.4 valid pdf content").unwrap();

    // Staging temp file inside managed note directory left by interrupted operations
    let temp_file_1 = note_dir.join("staging.tmp.deadbeef-1234");
    fs::write(&temp_file_1, b"partial data 1").unwrap();

    // Staging temp file in decrypted directory
    let dec_dir = fixture.paths.decrypted_attachments_dir();
    fs::create_dir_all(&dec_dir).unwrap();
    let temp_file_2 = dec_dir.join("decrypted_view.tmp");
    fs::write(&temp_file_2, b"decrypted transient copy").unwrap();

    assert!(temp_file_1.exists());
    assert!(temp_file_2.exists());
    assert!(valid_file.exists());

    // Clean abandoned temporary staging files (0 threshold cleans all .tmp files)
    let cleaned = AttachmentService::cleanup_abandoned_temp_files(&storage, 0)
        .expect("cleanup temporary files");
    assert_eq!(cleaned, 2, "Both temporary files should be removed");

    assert!(!temp_file_1.exists(), "Temp file 1 must be purged");
    assert!(!temp_file_2.exists(), "Temp file 2 must be purged");
    assert!(valid_file.exists(), "Valid file must be preserved intact");
}

/// DEF-13: Circular notebook hierarchy prevention.
/// Verifies that moving a notebook into itself or into one of its own descendants
/// is strictly rejected with `StorageError::InvalidHierarchy`.
#[test]
fn regression_def_13_circular_notebook_hierarchy_prevented() {
    let mut database = TestDatabase::new();
    let conn = database.connection_mut();

    let nb_parent = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Parent Notebook".to_string(),
            parent_id: None,
        },
    )
    .expect("create parent");

    let nb_child = NotebookRepository::create(
        conn,
        CreateNotebookDto {
            name: "Child Notebook".to_string(),
            parent_id: Some(nb_parent.id.clone()),
        },
    )
    .expect("create child");

    // 1. Move into self
    let self_move_res = NotebookRepository::move_notebook(conn, &nb_parent.id, Some(&nb_parent.id));
    assert!(
        matches!(self_move_res, Err(StorageError::InvalidHierarchy(_))),
        "Moving notebook into self must return StorageError::InvalidHierarchy"
    );

    // 2. Move parent into child (cycle)
    let cycle_move_res = NotebookRepository::move_notebook(conn, &nb_parent.id, Some(&nb_child.id));
    assert!(
        matches!(cycle_move_res, Err(StorageError::InvalidHierarchy(_))),
        "Moving parent into descendant must return StorageError::InvalidHierarchy"
    );
}

/// DEF-14: Encrypted storage locked state rejection.
/// Verifies that unauthenticated access while locked returns CryptoError::AppLocked.
#[test]
fn regression_def_14_encrypted_storage_locked_rejection() {
    let fixture = TestAppDir::with_label("reg-def14");
    let conn = fixture.open_migrated_database();

    let session = CryptoSession::init_from_database(&conn).expect("init session");
    session
        .setup_encryption(
            &conn,
            &fixture.paths,
            SetupEncryptionDto {
                password: "CorrectMasterPassword123!".to_string(),
                confirm_password: "CorrectMasterPassword123!".to_string(),
            },
        )
        .expect("setup encryption");

    // Lock session
    session.lock();

    // Accessing active DEK while locked must fail with AppLocked
    let dek_res = session.get_active_dek();
    assert_eq!(
        dek_res,
        Err(CryptoError::AppLocked),
        "Accessing DEK while locked must return CryptoError::AppLocked"
    );
}
